//! Game acquisition (ADR 0013): provider search, details and acquire jobs.
//! Claims and jobs live in memory only; a restart forgets both.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::sync::watch;

use super::{
    claims::{DownloadResolution, FailedDownloadReason, ProviderClaim, ProviderClaimDetails},
    download::{DownloadEngine, StagedGame},
    placement,
    turns::{self, ProviderContext, ProviderError},
    types::{
        AcquireArtifactRequest, AcquireJobState, AcquireJobStatus, AcquireSourceChoice,
        AcquisitionProvider, DetailsRequest, Failure, PluginListResponse, SearchRequest,
        SearchResponse,
    },
};
use crate::{
    discovery::{identify::SourceView, DiscoveryLifecycleCoordinator, DiscoverySourceStatus},
    plugin::PluginRegistry,
    RpcFailure,
};

/// Legacy copy for a download discovery did not recognize.
const STAGED_MESSAGE: &str = "Downloaded, but the Library did not recognize it as a playable game.";
/// How long a finished placement waits for the library rescan.
const RESCAN_WAIT: Duration = Duration::from_secs(120);

/// What a game acquisition needs from the device. Only the local brain has
/// discovery, so only it can acquire games.
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
        AcquireJobState::Resolving | AcquireJobState::Downloading { .. } | AcquireJobState::Placing
    )
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

    fn hold(&self, provider_id: &str, id: &str, url: &str, system: Option<&str>) {
        self.claims.lock().expect("claims mutex poisoned").insert(
            (provider_id.to_owned(), id.to_owned()),
            HeldClaim {
                url: url.to_owned(),
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
                AcquireJobState::Resolving | AcquireJobState::Downloading { .. }
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

    pub(super) fn acquire(
        &self,
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
        // Legacy activeByClaim, plus Imported: a game already placed needs no
        // second download in this process.
        if let Some(existing) = self
            .newest
            .lock()
            .expect("newest mutex poisoned")
            .get(&key)
            .and_then(|id| self.status(id).ok())
        {
            if live(&existing.state) || matches!(existing.state, AcquireJobState::Imported { .. }) {
                return Ok(existing);
            }
        }
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
        let eligible = eligible_sources(context, &system).map_err(|_| unavailable())?;
        if eligible.is_empty() {
            let title = context
                .registry
                .content_rules(&system)
                .and_then(|rules| rules.title.clone())
                .unwrap_or_else(|| system.clone());
            return Err(failure(
                "AcquisitionNoSource",
                &format!("No folder on this device holds {title} games. Add one first."),
            ));
        }
        let source = match &request.source_id {
            Some(id) => Some(
                eligible
                    .iter()
                    .find(|(view, _)| &view.source_id == id)
                    .cloned()
                    .ok_or_else(caller)?,
            ),
            None if eligible.len() == 1 => eligible.first().cloned(),
            None => None,
        };
        let mut status = AcquireJobStatus {
            job_id: job_id(),
            provider_id: request.provider_id.clone(),
            id: request.id.clone(),
            file_name: request.file_name.clone(),
            system: Some(system.clone()),
            message: None,
            state: AcquireJobState::Resolving,
        };
        let (cancel, cancelled) = watch::channel(false);
        let (done_tx, done) = watch::channel(false);
        let Some((source, _)) = source else {
            // Several eligible sources: nothing starts. The follow-up acquire
            // with sourceId is a new job.
            status.state = AcquireJobState::ChooseSource {
                sources: eligible
                    .into_iter()
                    .map(|(view, label)| AcquireSourceChoice {
                        id: view.source_id,
                        label,
                    })
                    .collect(),
            };
            let _ = done_tx.send(true);
            self.insert(&key, status.clone(), cancel, done, false);
            return Ok(status);
        };
        let job = self.insert(&key, status.clone(), cancel, done, true);
        let pipeline = Pipeline {
            context: context.clone(),
            provider,
            owner,
            claim,
            system,
            source,
            file_name: request.file_name.clone(),
            engine: self.engine.clone(),
            job: job.clone(),
        };
        tokio::spawn(async move {
            let outcome = pipeline.run(cancelled).await;
            {
                let mut job = job.lock().expect("job mutex poisoned");
                match outcome {
                    Ok(state) | Err(state) => {
                        job.status.message = match &state {
                            AcquireJobState::Staged { message } => Some(message.clone()),
                            AcquireJobState::Failed { reason } => Some(reason.message.clone()),
                            _ => None,
                        };
                        job.status.state = state;
                    }
                }
            }
            let _ = done_tx.send(true);
        });
        Ok(status)
    }

    fn insert(
        &self,
        key: &ClaimKey,
        status: AcquireJobStatus,
        cancel: watch::Sender<bool>,
        done: watch::Receiver<bool>,
        newest: bool,
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
        if newest {
            self.newest
                .lock()
                .expect("newest mutex poisoned")
                .insert(key.clone(), id);
        }
        job
    }

    /// Startup: game downloads never resume; remove every staging file.
    pub(super) fn recover(&self, root: &Path, registry: &PluginRegistry) -> Result<(), Failure> {
        for (record, owner) in registry.acquisition_providers() {
            placement::remove_game_staging(
                &placement::provider_cache(root, owner, &record.id).join(".downloads"),
            )?;
        }
        Ok(())
    }
}

fn unavailable() -> RpcFailure {
    failure(
        "AcquisitionUnavailable",
        "Downloads are unavailable on this device.",
    )
}

fn job_id() -> String {
    format!("{:032x}", rand::random::<u128>())
}

fn provider_failure(provider_id: &str, operation: &str, error: &ProviderError) -> RpcFailure {
    // Diagnostics may hold URLs or plugin text: debug level only.
    log::debug!("acquisition provider {provider_id} {operation}: {error:?}");
    error.rpc()
}

/// Paired file sources whose system, or system group, holds `system`, with
/// their discovery label. korrid alone decides eligibility.
fn eligible_sources(
    context: &GameContext,
    system: &str,
) -> Result<Vec<(SourceView, String)>, crate::discovery::DiscoveryError> {
    let snapshot = context.discovery.snapshot();
    let views = context.discovery.source_views()?;
    Ok(snapshot
        .sources
        .into_iter()
        .filter(|source| source.status == DiscoverySourceStatus::Ready)
        .filter(|source| {
            source.system == system
                || context
                    .registry
                    .group(&source.system)
                    .is_some_and(|group| group.systems.iter().any(|member| member == system))
        })
        .filter_map(|source| {
            views
                .iter()
                .find(|view| view.source_id == source.id && view.scans_files)
                .map(|view| (view.clone(), source.label))
        })
        .collect())
}

struct Pipeline {
    context: GameContext,
    provider: ProviderContext,
    owner: String,
    claim: HeldClaim,
    system: String,
    source: SourceView,
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
        self,
        mut cancelled: watch::Receiver<bool>,
    ) -> Result<AcquireJobState, AcquireJobState> {
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
        // Legacy order: resolution filename, then the request's, then the URL's.
        let name = sanitize_file_name(
            download
                .filename
                .clone()
                .or_else(|| self.file_name.clone())
                .or_else(|| super::policy::basename(&download.url).ok())
                .as_deref()
                .unwrap_or_default(),
        );
        let rules = self
            .context
            .registry
            .content_rules(&self.system)
            .cloned()
            .ok_or_else(|| download_failed(Failure::NotSystemFile))?;
        let extension = rules
            .extensions
            .iter()
            .filter(|extension| {
                name.to_ascii_lowercase()
                    .ends_with(&format!(".{}", extension.to_ascii_lowercase()))
            })
            .max_by_key(|extension| extension.len())
            .cloned()
            .ok_or_else(|| download_failed(Failure::NotSystemFile))?;
        self.job
            .lock()
            .expect("job mutex poisoned")
            .status
            .file_name = Some(name.clone());

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
        let directory = layout_directory(rules.layouts.first().map(String::as_str));
        let context = self.context.clone();
        let source = self.source.clone();
        let header = rules.header.clone();
        let system = self.system.clone();
        tokio::task::spawn_blocking(move || {
            place_and_import(
                &context,
                &source,
                &system,
                &header,
                &staged,
                &directory,
                &name,
                extension.len(),
            )
        })
        .await
        .map_err(|_| download_failed(Failure::Storage))?
    }
}

/// Validate, place, seed, rescan and look up the release (blocking).
#[allow(clippy::too_many_arguments)]
fn place_and_import(
    context: &GameContext,
    source: &SourceView,
    system: &str,
    header: &[crate::plugin::HeaderRule],
    staged: &StagedGame,
    directory: &str,
    name: &str,
    extension_len: usize,
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
    // The source may have been unpaired while the game downloaded.
    if !eligible_sources(context, system)
        .map_err(|_| failed(unavailable()))?
        .iter()
        .any(|(view, _)| view.source_id == source.source_id)
    {
        return Err(failed(failure(
            "AcquisitionNoSource",
            "The folder chosen for this game is no longer available.",
        )));
    }
    // Byte identity: the same content already in this source is the same
    // release, so it is not copied again under another name.
    let key = format!("sha256:{}", staged.sha256);
    if let Ok(published) = context.discovery.published() {
        let present = published.locations.get(&key).is_some_and(|locations| {
            locations.iter().any(|location| {
                matches!(location, crate::config::Location::File { storage, .. }
                    if storage.0 == source.storage_id)
            })
        });
        if let Some(game_id) = present.then(|| game_for(&published, &key)).flatten() {
            return Ok(AcquireJobState::Imported { game_id });
        }
    }
    let placed = placement::place_game(
        &staged.path,
        &staged.sha256,
        &source.root,
        directory,
        name,
        extension_len,
    )
    .map_err(download_failed)?;
    if let Err(error) =
        context
            .discovery
            .seed_hash(&placed.path, header, &staged.sha256, &staged.sha1)
    {
        log::debug!("acquisition hash seed: {error}");
    }
    context.discovery.rescan();
    context.discovery.wait_until_idle(RESCAN_WAIT);
    let published = context
        .discovery
        .published()
        .map_err(|_| failed(unavailable()))?;
    let release = published.locations.iter().find_map(|(key, locations)| {
        locations
            .iter()
            .any(|location| {
                matches!(location, crate::config::Location::File { storage, path, .. }
                    if storage.0 == source.storage_id && path.0 == placed.relative)
            })
            .then_some(key)
    });
    let game = release.and_then(|key| game_for(&published, key));
    Ok(match game {
        Some(game_id) => AcquireJobState::Imported { game_id },
        None => AcquireJobState::Staged {
            message: STAGED_MESSAGE.into(),
        },
    })
}

/// The library game that holds the release with this identity.
fn game_for(published: &crate::config::ConfigSnapshot, key: &str) -> Option<String> {
    published.library.iter().find_map(|(id, game)| {
        game.releases
            .iter()
            .any(|release| release.identity.as_ref().is_some_and(|i| i.value() == key))
            .then(|| id.clone())
    })
}

/// The literal leading directories of a layout: `**/{title}.{ext}` is the
/// source root; `Games/**/{title}.{ext}` is `Games`.
fn layout_directory(layout: Option<&str>) -> String {
    let Some(layout) = layout else {
        return String::new();
    };
    let segments: Vec<_> = layout.split('/').collect();
    segments[..segments.len().saturating_sub(1)]
        .iter()
        .take_while(|segment| !segment.contains(['{', '*']))
        .copied()
        .collect::<Vec<_>>()
        .join("/")
}

/// Legacy `sanitizeArtifactFileName`.
fn sanitize_file_name(raw: &str) -> String {
    let collapsed: String = {
        let mut out = String::new();
        let mut in_separator = false;
        for character in raw.trim().chars() {
            if matches!(character, '/' | '\\') {
                if !in_separator {
                    out.push('_');
                }
                in_separator = true;
            } else {
                out.push(character);
                in_separator = false;
            }
        }
        out
    };
    let mut cleaned = String::new();
    let mut in_run = false;
    for character in collapsed.chars() {
        if character.is_ascii_alphanumeric() || "_.() []-".contains(character) {
            cleaned.push(character);
            in_run = false;
        } else {
            if !in_run {
                cleaned.push('_');
            }
            in_run = true;
        }
    }
    let cleaned = cleaned.trim_start_matches('.');
    if cleaned.is_empty() {
        "download.bin".into()
    } else {
        cleaned.chars().take(200).collect()
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn layout_directories_and_legacy_file_name_sanitizing() {
        assert_eq!(layout_directory(Some("**/{title}.{ext}")), "");
        assert_eq!(layout_directory(Some("{title}.{ext}")), "");
        assert_eq!(
            layout_directory(Some("Games/GB/**/{title}.{ext}")),
            "Games/GB"
        );
        assert_eq!(layout_directory(Some("{region}/{title}.{ext}")), "");
        assert_eq!(layout_directory(None), "");
        assert_eq!(sanitize_file_name("  2048.gb "), "2048.gb");
        assert_eq!(sanitize_file_name("a/../b.gb"), "a_.._b.gb");
        assert_eq!(sanitize_file_name("..hidden.gb"), "hidden.gb");
        assert_eq!(
            sanitize_file_name("Céleste & co.p8.png"),
            "C_leste _ co.p8.png"
        );
        assert_eq!(sanitize_file_name("..."), "download.bin");
    }
}
