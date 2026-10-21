//! Game acquisition (ADR 0014): provider search, details and acquire jobs.
//! A downloaded cart stays in korrid's cache (ADR 0012 grammar) and becomes a
//! library game through the records ADR 0013 already uses. Claims and jobs
//! live in memory only; a restart forgets both.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tokio::sync::watch;

use super::{
    claims::{DownloadResolution, FailedDownloadReason, ProviderClaim, ProviderClaimDetails},
    download::{DownloadEngine, StagedGame},
    placement,
    turns::{self, ProviderContext, ProviderError},
    types::{
        AcquireArtifactRequest, AcquireJobState, AcquireJobStatus, AcquisitionProvider,
        DetailsRequest, Failure, PluginListResponse, SearchRequest, SearchResponse,
    },
    AcquisitionCoordinator,
};
use crate::{
    config::ConfigSnapshot,
    discovery::{reconcile::AcquiredCart, DiscoveryLifecycleCoordinator},
    plugin::PluginRegistry,
    RpcFailure,
};

/// What a game acquisition needs from the device. Only the local brain has
/// discovery, which owns the library documents and their publication lock.
#[derive(Clone)]
pub struct GameContext {
    pub root: PathBuf,
    pub registry: PluginRegistry,
    pub discovery: DiscoveryLifecycleCoordinator,
}

type ClaimKey = (String, String);

/// The claim fields korrid acts on; the portal can never substitute them.
#[derive(Clone)]
struct HeldClaim {
    url: String,
    title: String,
    system: Option<String>,
}

struct Job {
    status: AcquireJobStatus,
    cancel: watch::Sender<bool>,
    done: watch::Receiver<bool>,
}

#[derive(Clone, Default)]
pub(super) struct GameAcquisition {
    claims: Arc<Mutex<BTreeMap<ClaimKey, HeldClaim>>>,
    jobs: Arc<Mutex<BTreeMap<String, Arc<Mutex<Job>>>>>,
    newest: Arc<Mutex<BTreeMap<ClaimKey, String>>>,
    engine: DownloadEngine,
}

fn failure(code: &str, message: &str) -> RpcFailure {
    RpcFailure {
        code: code.into(),
        message: crate::launcher::redaction::status_text(
            message,
            crate::launcher::redaction::MAX_REASON_CHARS,
        ),
    }
}
fn caller() -> RpcFailure {
    failure("AcquisitionCaller", "The download request is not valid.")
}
fn unknown_provider() -> RpcFailure {
    failure(
        "AcquisitionUnknownProvider",
        "That game source is not available.",
    )
}

fn live(state: &AcquireJobState) -> bool {
    matches!(
        state,
        AcquireJobState::Queued
            | AcquireJobState::Resolving
            | AcquireJobState::Downloading { .. }
            | AcquireJobState::Placing
    )
}

/// The provider link key `<provider>/<claim id>` (as `reconcile_providers`).
fn link_key(provider: &str, claim_id: &str) -> String {
    format!("{provider}/{claim_id}")
}

/// ADR 0014 Q3: the ready directory of one library release in the cache of
/// a provider linked to its game: `cache/<plugin>/<provider>/<sha256 hex>/`.
/// Returns the directories in the game's release order.
pub(crate) fn cart_dirs(
    root: &Path,
    registry: &PluginRegistry,
    snapshot: &ConfigSnapshot,
    game_id: &str,
) -> Vec<(String, PathBuf)> {
    let Some(game) = snapshot.library.get(game_id) else {
        return Vec::new();
    };
    let providers: Vec<_> = snapshot
        .provider_links
        .values()
        .filter(|link| link.playable.0 == game_id)
        .filter_map(|link| {
            registry
                .acquisition_provider(&link.provider.0)
                .map(|(record, owner)| placement::provider_cache(root, owner, &record.id))
        })
        .collect();
    game.releases
        .iter()
        .filter_map(|release| {
            let key = release.identity.as_ref()?.value();
            let hex = key.strip_prefix("sha256:")?;
            providers
                .iter()
                .map(|cache| cache.join(hex))
                .find(|ready| placement::cart_file(ready).is_some())
                .map(|ready| (key.to_owned(), ready))
        })
        .collect()
}

/// ADR 0014 Q3: the cart file a route passes as `contentPath`.
pub(crate) fn cart_content(
    root: &Path,
    registry: &PluginRegistry,
    snapshot: &ConfigSnapshot,
    game_id: &str,
    key: &str,
) -> Option<PathBuf> {
    cart_dirs(root, registry, snapshot, game_id)
        .into_iter()
        .find(|(release, _)| release == key)
        .and_then(|(_, ready)| placement::cart_file(&ready))
}

impl GameAcquisition {
    #[cfg(test)]
    pub(super) fn with_engine(engine: DownloadEngine) -> Self {
        Self {
            engine,
            ..Self::default()
        }
    }

    pub(super) fn providers(&self, registry: &PluginRegistry) -> PluginListResponse {
        PluginListResponse {
            providers: registry
                .acquisition_providers()
                .map(|(record, owner)| AcquisitionProvider {
                    id: record.id.clone(),
                    title: record.title.clone().unwrap_or_default(),
                    plugin_id: owner.to_owned(),
                })
                .collect(),
        }
    }

    fn provider(
        registry: &PluginRegistry,
        provider_id: &str,
    ) -> Result<(ProviderContext, String), RpcFailure> {
        let (_, owner) = registry
            .acquisition_provider(provider_id)
            .ok_or_else(unknown_provider)?;
        let source = registry
            .plugin_source(owner)
            .map_err(|_| unknown_provider())?;
        Ok((
            ProviderContext {
                provider_id: provider_id.to_owned(),
                source: Arc::new(source),
                hosts: Arc::new(registry.plugin_hosts(owner).to_vec()),
            },
            owner.to_owned(),
        ))
    }

    fn hold(&self, provider_id: &str, id: &str, url: &str, title: &str, system: Option<&str>) {
        self.claims.lock().expect("claims mutex poisoned").insert(
            (provider_id.to_owned(), id.to_owned()),
            HeldClaim {
                url: url.to_owned(),
                title: title.to_owned(),
                system: system.map(str::to_owned),
            },
        );
    }

    pub(super) async fn search(
        &self,
        registry: &PluginRegistry,
        request: &SearchRequest,
    ) -> Result<SearchResponse, RpcFailure> {
        let [provider_id] = request.provider_ids.as_slice() else {
            return Err(caller());
        };
        if request.query.chars().count() > 8192 {
            return Err(caller());
        }
        let (context, _) = Self::provider(registry, provider_id)?;
        let claims: Vec<ProviderClaim> = turns::search(&self.engine, &context, &request.query)
            .await
            .map_err(|error| provider_failure(provider_id, "claims.search", &error))?;
        for claim in &claims {
            self.hold(
                &claim.provider_id,
                &claim.id,
                &claim.url,
                &claim.title,
                claim.artifact.as_ref().and_then(|a| a.system.as_deref()),
            );
        }
        Ok(SearchResponse { claims })
    }

    pub(super) async fn details(
        &self,
        registry: &PluginRegistry,
        request: &DetailsRequest,
    ) -> Result<ProviderClaimDetails, RpcFailure> {
        let count = request.id.chars().count();
        if count == 0 || count > 2048 {
            return Err(caller());
        }
        let (context, _) = Self::provider(registry, &request.provider_id)?;
        let details = turns::details(&self.engine, &context, &request.id)
            .await
            .map_err(|error| provider_failure(&request.provider_id, "claims.details", &error))?;
        self.hold(
            &details.provider_id,
            &details.id,
            &details.url,
            &details.title,
            details.artifact.as_ref().and_then(|a| a.system.as_deref()),
        );
        Ok(details)
    }

    pub(super) fn status(&self, job_id: &str) -> Result<AcquireJobStatus, RpcFailure> {
        self.jobs
            .lock()
            .expect("jobs mutex poisoned")
            .get(job_id)
            .map(|job| job.lock().expect("job mutex poisoned").status.clone())
            .ok_or_else(|| {
                failure(
                    "AcquisitionUnknownJob",
                    "This download is not running. Korri may have restarted.",
                )
            })
    }

    pub(super) async fn cancel(&self, job_id: &str) -> Result<AcquireJobStatus, RpcFailure> {
        self.status(job_id)?;
        let job = self.jobs.lock().expect("jobs mutex poisoned")[job_id].clone();
        let done = {
            let job = job.lock().expect("job mutex poisoned");
            matches!(
                job.status.state,
                AcquireJobState::Queued
                    | AcquireJobState::Resolving
                    | AcquireJobState::Downloading { .. }
            )
            .then(|| {
                let _ = job.cancel.send(true);
                job.done.clone()
            })
        };
        if let Some(mut done) = done {
            while !*done.borrow() {
                if done.changed().await.is_err() {
                    break;
                }
            }
        }
        self.status(job_id)
    }

    /// True while a job for a claim linked to `game_id` is still working:
    /// Remove download waits for Cancel first (ADR 0013 fifth round).
    pub(super) fn busy_for_game(&self, snapshot: &ConfigSnapshot, game_id: &str) -> bool {
        let newest = self.newest.lock().expect("newest mutex poisoned").clone();
        newest.into_iter().any(|((provider, claim), job)| {
            snapshot
                .provider_links
                .get(&link_key(&provider, &claim))
                .is_some_and(|link| link.playable.0 == game_id)
                && self.status(&job).is_ok_and(|status| live(&status.state))
        })
    }

    pub(super) fn acquire(
        &self,
        coordinator: &AcquisitionCoordinator,
        context: &GameContext,
        request: &AcquireArtifactRequest,
    ) -> Result<AcquireJobStatus, RpcFailure> {
        if request
            .file_name
            .as_deref()
            .is_some_and(|name| !super::claims::safe_file_name(name))
        {
            return Err(caller());
        }
        let (provider, owner) = Self::provider(&context.registry, &request.provider_id)?;
        let key = (request.provider_id.clone(), request.id.clone());
        // Legacy activeByClaim: a live job for this claim answers again.
        if let Some(existing) = self
            .newest
            .lock()
            .expect("newest mutex poisoned")
            .get(&key)
            .and_then(|id| self.status(id).ok())
        {
            if live(&existing.state) {
                return Ok(existing);
            }
        }
        // Already in the library, also after a restart and with no claim in
        // memory: the provider link names the game and its cart is still in
        // the cache. No network is needed to answer Play.
        if let Some(game_id) = self.in_library(context, &request.provider_id, &request.id) {
            let (cancel, _) = watch::channel(false);
            let (done_tx, done) = watch::channel(true);
            drop(done_tx);
            let status = AcquireJobStatus {
                job_id: job_id(),
                provider_id: request.provider_id.clone(),
                id: request.id.clone(),
                file_name: request.file_name.clone(),
                system: None,
                message: None,
                state: AcquireJobState::Imported { game_id },
            };
            self.insert(&key, status.clone(), cancel, done);
            return Ok(status);
        }
        let claim = self
            .claims
            .lock()
            .expect("claims mutex poisoned")
            .get(&key)
            .cloned()
            .ok_or_else(|| {
                failure(
                    "AcquisitionUnknownClaim",
                    "Search for this game again before downloading it.",
                )
            })?;
        let system = claim
            .system
            .clone()
            .filter(|system| context.registry.content_rules(system).is_some())
            .ok_or_else(|| {
                failure(
                    "AcquisitionSystemUnknown",
                    "Korri cannot play this kind of game on this device.",
                )
            })?;
        let status = AcquireJobStatus {
            job_id: job_id(),
            provider_id: request.provider_id.clone(),
            id: request.id.clone(),
            file_name: request.file_name.clone(),
            system: Some(system.clone()),
            message: None,
            state: AcquireJobState::Queued,
        };
        let (cancel, cancelled) = watch::channel(false);
        let (done_tx, done) = watch::channel(false);
        let job = self.insert(&key, status.clone(), cancel, done);
        let pipeline = Pipeline {
            context: context.clone(),
            turn: Some(
                coordinator.queue_turn(
                    placement::provider_cache(&context.root, &owner, &request.provider_id)
                        .join(format!(".job-{}", status.job_id)),
                ),
            ),
            provider,
            owner,
            claim_id: request.id.clone(),
            claim,
            system,
            file_name: request.file_name.clone(),
            engine: self.engine.clone(),
            job: job.clone(),
        };
        tokio::spawn(async move {
            let state = match pipeline.run(cancelled).await {
                Ok(state) | Err(state) => state,
            };
            {
                let mut job = job.lock().expect("job mutex poisoned");
                job.status.message = match &state {
                    AcquireJobState::Failed { reason } => Some(reason.message.clone()),
                    _ => None,
                };
                job.status.state = state;
            }
            let _ = done_tx.send(true);
        });
        Ok(status)
    }

    fn in_library(&self, context: &GameContext, provider: &str, claim_id: &str) -> Option<String> {
        let snapshot = context.discovery.published().ok()?;
        let game_id = snapshot
            .provider_links
            .get(&link_key(provider, claim_id))?
            .playable
            .0
            .clone();
        (!cart_dirs(&context.root, &context.registry, &snapshot, &game_id).is_empty())
            .then_some(game_id)
    }

    fn insert(
        &self,
        key: &ClaimKey,
        status: AcquireJobStatus,
        cancel: watch::Sender<bool>,
        done: watch::Receiver<bool>,
    ) -> Arc<Mutex<Job>> {
        let id = status.job_id.clone();
        let job = Arc::new(Mutex::new(Job {
            status,
            cancel,
            done,
        }));
        self.jobs
            .lock()
            .expect("jobs mutex poisoned")
            .insert(id.clone(), job.clone());
        self.newest
            .lock()
            .expect("newest mutex poisoned")
            .insert(key.clone(), id);
        job
    }

    /// Startup: a cart download never resumes (no expected hash, no saved
    /// URL); remove every staging file and interrupted publication.
    pub(super) fn recover(&self, root: &Path, registry: &PluginRegistry) -> Result<(), Failure> {
        for (record, owner) in registry.acquisition_providers() {
            let cache = placement::provider_cache(root, owner, &record.id);
            placement::remove_game_staging(&cache.join(".downloads"))?;
            if cache.is_dir() {
                placement::clean_work(&cache)?;
            }
        }
        Ok(())
    }
}

fn job_id() -> String {
    format!("{:032x}", rand::random::<u128>())
}

fn provider_failure(provider_id: &str, operation: &str, error: &ProviderError) -> RpcFailure {
    // Diagnostics may hold URLs or plugin text: debug level only.
    log::debug!("acquisition provider {provider_id} {operation}: {error:?}");
    error.rpc()
}

struct Pipeline {
    context: GameContext,
    turn: Option<super::TurnWait>,
    provider: ProviderContext,
    owner: String,
    claim_id: String,
    claim: HeldClaim,
    system: String,
    file_name: Option<String>,
    engine: DownloadEngine,
    job: Arc<Mutex<Job>>,
}

fn failed(failure: RpcFailure) -> AcquireJobState {
    AcquireJobState::Failed { reason: failure }
}
fn download_failed(error: Failure) -> AcquireJobState {
    failed(error.rpc())
}

impl Pipeline {
    fn set(&self, state: AcquireJobState) {
        self.job.lock().expect("job mutex poisoned").status.state = state;
    }

    async fn run(
        mut self,
        mut cancelled: watch::Receiver<bool>,
    ) -> Result<AcquireJobState, AcquireJobState> {
        // ADR 0013 queue: one download at a time, in the order asked. The
        // job reports Queued while it waits; cancel leaves the queue.
        let wait = self.turn.take().expect("one turn per job");
        let _turn = tokio::select! {
            biased;
            _ = cancelled.changed() => return Err(AcquireJobState::Cancelled),
            turn = wait.turn() => turn,
        };
        {
            let mut job = self.job.lock().expect("job mutex poisoned");
            if *cancelled.borrow() {
                return Err(AcquireJobState::Cancelled);
            }
            job.status.state = AcquireJobState::Resolving;
        }
        let resolution = tokio::select! {
            biased;
            _ = cancelled.changed() => return Err(AcquireJobState::Cancelled),
            resolution = turns::resolve_download(
                &self.engine, &self.provider, &self.claim.url, self.file_name.as_deref(),
            ) => resolution.map_err(|error| failed(provider_failure(&self.provider.provider_id, "artifact.resolve-download", &error)))?,
        };
        let download = match resolution {
            DownloadResolution::Final(download) => download,
            DownloadResolution::NonFinal(_) => {
                return Err(failed(failure(
                    "DownloadNotFinal",
                    "This game cannot be downloaded directly from its source.",
                )))
            }
            DownloadResolution::Failed(failed_download) => {
                return Err(failed(failure(
                    "DownloadResolutionFailed",
                    match failed_download.reason {
                        FailedDownloadReason::NotFound => "The source no longer offers this game.",
                        _ => "The source could not prepare this download. Please retry later.",
                    },
                )))
            }
        };
        // ADR 0012: the file is named by the URL's last segment.
        let name = super::policy::basename(&download.url).map_err(download_failed)?;
        let rules = self
            .context
            .registry
            .content_rules(&self.system)
            .cloned()
            .ok_or_else(|| download_failed(Failure::NotSystemFile))?;
        if !rules.extensions.iter().any(|extension| {
            name.to_ascii_lowercase()
                .ends_with(&format!(".{}", extension.to_ascii_lowercase()))
        }) {
            return Err(download_failed(Failure::NotSystemFile));
        }
        self.job
            .lock()
            .expect("job mutex poisoned")
            .status
            .file_name = Some(name.clone());

        let cache =
            placement::provider_cache(&self.context.root, &self.owner, &self.provider.provider_id);
        let staging = {
            let (root, owner, provider) = (
                self.context.root.clone(),
                self.owner.clone(),
                self.provider.provider_id.clone(),
            );
            tokio::task::spawn_blocking(move || placement::game_staging(&root, &owner, &provider))
                .await
                .map_err(|_| download_failed(Failure::Storage))?
                .map_err(download_failed)?
        };
        self.set(AcquireJobState::Downloading {
            bytes_done: 0,
            bytes_total: None,
            bytes_per_second: None,
        });
        let flag = cancelled.clone();
        let staged: StagedGame = tokio::select! {
            biased;
            _ = cancelled.changed() => return Err(AcquireJobState::Cancelled),
            staged = self.engine.fetch_game(&download.url, &self.provider.hosts, &staging, |progress| {
                let mut job = self.job.lock().expect("job mutex poisoned");
                if !*flag.borrow() {
                    job.status.state = AcquireJobState::Downloading {
                        bytes_done: progress.done,
                        bytes_total: progress.total,
                        bytes_per_second: progress.rate,
                    };
                }
            }) => staged.map_err(download_failed)?,
        };
        {
            // Cancellation wins until placement starts; placing cannot be
            // cancelled. Cancel checks the state under this same lock.
            let mut job = self.job.lock().expect("job mutex poisoned");
            if *cancelled.borrow() {
                return Err(AcquireJobState::Cancelled);
            }
            job.status.state = AcquireJobState::Placing;
        }
        let context = self.context.clone();
        let (provider, claim_id, title, system) = (
            self.provider.provider_id.clone(),
            self.claim_id.clone(),
            self.claim.title.clone(),
            self.system.clone(),
        );
        tokio::task::spawn_blocking(move || {
            publish_and_record(
                &context,
                &cache,
                &rules.header,
                &staged,
                &name,
                &provider,
                &claim_id,
                &title,
                &system,
            )
        })
        .await
        .map_err(|_| download_failed(Failure::Storage))?
    }
}

/// Check the header rule, publish the ready directory, write the records.
#[allow(clippy::too_many_arguments)]
fn publish_and_record(
    context: &GameContext,
    cache: &Path,
    header: &[crate::plugin::HeaderRule],
    staged: &StagedGame,
    name: &str,
    provider: &str,
    claim_id: &str,
    title: &str,
    system: &str,
) -> Result<AcquireJobState, AcquireJobState> {
    // Content rules: a header that would consume the whole file is not a game.
    let prefix = {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(&staged.path)
            .and_then(|file| file.take(64 * 1024).read_to_end(&mut bytes))
            .map_err(|error| download_failed(error.into()))?;
        bytes
    };
    if crate::discovery::identify::header_size(&prefix, staged.bytes, header) >= staged.bytes {
        return Err(download_failed(Failure::NotSystemFile));
    }
    placement::publish_cart(cache, &staged.path, name, &staged.sha256).map_err(download_failed)?;
    let game_id = context
        .discovery
        .record_acquired(&AcquiredCart {
            provider,
            claim_id,
            title,
            system,
            sha256_hex: &staged.sha256,
        })
        .map_err(|error| {
            log::debug!("acquisition library record: {error}");
            download_failed(Failure::Storage)
        })?;
    Ok(AcquireJobState::Imported { game_id })
}
