//! Game acquisition (ADR 0016): provider search, details and acquire jobs.
//! A downloaded game stays in korrid's cache (ADR 0012 grammar) and becomes a
//! library game through the records ADR 0013 already uses. Claims and jobs
//! live in memory only; a restart forgets both.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tokio::sync::watch;

use super::{
    archive,
    claims::{DownloadResolution, FailedDownloadReason, ProviderClaim, ProviderClaimDetails},
    download::{DownloadEngine, GameRequest, Validator, STAGING_PREFIX},
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
    discovery::{
        identify::{self, ContentIdentity, HashCache, SourceInput},
        reconcile::AcquiredGame,
        scanner::TraversalBudget,
        DiscoveryLifecycleCoordinator,
    },
    plugin::PluginRegistry,
    RpcFailure,
};

/// What a game acquisition needs from the device. Only the local brain has
/// discovery, which owns the library documents and their publication lock.
#[derive(Clone)]
pub struct GameContext {
    pub root: PathBuf,
    /// The private state root, which the reader's sandbox unit cannot see.
    pub private_root: PathBuf,
    pub registry: PluginRegistry,
    pub discovery: DiscoveryLifecycleCoordinator,
}

type ClaimKey = (String, String);

/// The claim fields korrid acts on; the shell can never substitute them.
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
    /// Test-only: while set, the reader waits before it starts, so a test
    /// can observe and cancel `Unpacking`.
    #[cfg(test)]
    hold_unpack: Option<Arc<std::sync::atomic::AtomicBool>>,
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
        "That game provider is not available.",
    )
}

fn live(state: &AcquireJobState) -> bool {
    matches!(
        state,
        AcquireJobState::Queued
            | AcquireJobState::Resolving
            | AcquireJobState::Downloading { .. }
            | AcquireJobState::Unpacking
            | AcquireJobState::Placing
    )
}

/// The provider link key `<provider>/<claim id>` (as `reconcile_providers`).
fn link_key(provider: &str, claim_id: &str) -> String {
    format!("{provider}/{claim_id}")
}

/// The library game that discovery found installed for a provider item, in
/// the published snapshot: the game whose release has the provider identity
/// `<provider>/<claim id>`, while that key has a provider location.
/// `reconcile_providers` writes exactly these two records for a Steam
/// appmanifest, and no provider link; it withdraws the location when the
/// manifest goes, so a game that is only in the library's history is not
/// found.
pub(crate) fn discovered_provider_game(
    context: &GameContext,
    provider: &str,
    claim_id: &str,
) -> Option<String> {
    let snapshot = context.discovery.published().ok()?;
    let key = link_key(provider, claim_id);
    snapshot
        .locations
        .get(&key)?
        .iter()
        .any(|location| matches!(location, crate::config::Location::ProviderRef { .. }))
        .then_some(())?;
    snapshot
        .library
        .iter()
        .find(|(_, game)| {
            game.releases.iter().any(|release| {
                matches!(
                    &release.identity,
                    Some(crate::config::library::LibraryIdentity::Provider { value }) if *value == key
                )
            })
        })
        .map(|(id, _)| id.clone())
}

/// ADR 0092: the ready directory of one library release, whatever source
/// installed it: `cache/<hex of the release key>/` (ADR 0040's layout, folder
/// name from ADR 0030), with a file of the release system for the route.
/// Returns the directories in release order.
pub(crate) fn acquired_game_dirs(
    root: &Path,
    registry: &PluginRegistry,
    snapshot: &ConfigSnapshot,
    game_id: &str,
) -> Vec<(String, PathBuf)> {
    let Some(game) = snapshot.library.get(game_id) else {
        return Vec::new();
    };
    game.releases
        .iter()
        .filter_map(|release| {
            let key = release.identity.as_ref()?.value();
            let hex = placement::ready_name(key)?;
            let rules = registry.content_rules(&release.system.as_ref()?.0)?;
            let ready = placement::copied_release_cache(root).join(hex);
            placement::ready_game_file(&ready, rules).map(|_| (key.to_owned(), ready))
        })
        .collect()
}

/// ADR 0016 Q3: the acquired game file a route passes as `contentPath`.
pub(crate) fn acquired_game_content(
    root: &Path,
    registry: &PluginRegistry,
    snapshot: &ConfigSnapshot,
    game_id: &str,
    key: &str,
) -> Option<PathBuf> {
    let system = &snapshot
        .library
        .get(game_id)?
        .releases
        .iter()
        .find(|release| release.identity.as_ref().is_some_and(|i| i.value() == key))?
        .system
        .as_ref()?
        .0;
    let rules = registry.content_rules(system)?;
    acquired_game_dirs(root, registry, snapshot, game_id)
        .into_iter()
        .find(|(release, _)| release == key)
        .and_then(|(_, ready)| placement::ready_game_file(&ready, rules))
}

impl GameAcquisition {
    #[cfg(test)]
    pub(super) fn with_engine(engine: DownloadEngine) -> Self {
        Self {
            engine,
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub(super) fn holding_unpack(mut self, hold: Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.hold_unpack = Some(hold);
        self
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
                // No acquisition provider uses a secret (ADR 0034 has one
                // real case, the artwork operation).
                credential: None,
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
            // ADR 0030: a store job can be cancelled while it unpacks;
            // korrid has published nothing yet.
            matches!(
                job.status.state,
                AcquireJobState::Queued
                    | AcquireJobState::Resolving
                    | AcquireJobState::Downloading { .. }
                    | AcquireJobState::Unpacking
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

    pub(super) async fn acquire(
        &self,
        coordinator: &AcquisitionCoordinator,
        context: &GameContext,
        request: &AcquireArtifactRequest,
        place: super::Place,
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
        // memory: the provider link names the game and its download is still in
        // the cache. No network is needed to answer Play.
        // published() takes discovery's publication lock, which a scan holds
        // for its whole run: read it off the async workers.
        let in_library = {
            let (context, provider, id) = (
                context.clone(),
                request.provider_id.clone(),
                request.id.clone(),
            );
            tokio::task::spawn_blocking(move || in_library(&context, &provider, &id))
                .await
                .map_err(|_| {
                    failure(
                        "AcquisitionUnavailable",
                        "Downloads are unavailable on this device.",
                    )
                })?
        };
        if let Some(game_id) = in_library {
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
        // ADR 0092: a Get of a claim whose install was interrupted resumes
        // it, with its job id and its staged bytes.
        let staging = placement::provider_cache(&context.root, &owner, &request.provider_id)
            .join(".downloads");
        let kept = InstallRecord::find(&staging, &request.provider_id, &request.id);
        let record = InstallRecord {
            job_id: kept
                .as_ref()
                .map_or_else(job_id, |kept| kept.job_id.clone()),
            provider_id: request.provider_id.clone(),
            id: request.id.clone(),
            file_name: request.file_name.clone(),
            url: claim.url.clone(),
            title: claim.title.clone(),
            system,
            etag: kept.as_ref().and_then(|kept| kept.etag.clone()),
            last_modified: kept.and_then(|kept| kept.last_modified),
        };
        if let Err(error) = placement::game_staging(&context.root, &owner, &request.provider_id)
            .and_then(|staging| record.save(&staging))
        {
            // The install still runs; only a restart would forget it.
            eprintln!(
                "korrid acquisition: the install record was not saved: {}",
                error.rpc().message
            );
        }
        Ok(self.start(coordinator, context, provider, owner, record, place))
    }

    /// Queue one provider install and run its pipeline in the background.
    fn start(
        &self,
        coordinator: &AcquisitionCoordinator,
        context: &GameContext,
        provider: ProviderContext,
        owner: String,
        record: InstallRecord,
        place: super::Place,
    ) -> AcquireJobStatus {
        let key = (record.provider_id.clone(), record.id.clone());
        let claim = HeldClaim {
            url: record.url.clone(),
            title: record.title.clone(),
            system: Some(record.system.clone()),
        };
        self.claims
            .lock()
            .expect("claims mutex poisoned")
            .insert(key.clone(), claim.clone());
        let status = AcquireJobStatus {
            job_id: record.job_id.clone(),
            provider_id: record.provider_id.clone(),
            id: record.id.clone(),
            file_name: record.file_name.clone(),
            system: Some(record.system.clone()),
            message: None,
            state: AcquireJobState::Queued,
        };
        let (cancel, cancelled) = watch::channel(false);
        let (done_tx, done) = watch::channel(false);
        let job = self.insert(&key, status.clone(), cancel, done);
        let cache = placement::provider_cache(&context.root, &owner, &record.provider_id);
        let pipeline = Pipeline {
            context: context.clone(),
            turn: Some(coordinator.queue_turn(
                cache.join(format!(".job-{}", record.job_id)),
                place,
                Some((
                    &context.private_root,
                    super::Member::Job(record.job_id.clone()),
                )),
            )),
            provider,
            owner,
            claim_id: record.id.clone(),
            claim,
            system: record.system.clone(),
            file_name: record.file_name.clone(),
            engine: self.engine.clone(),
            job: job.clone(),
            staging: cache.join(".downloads"),
            record,
            keep: false,
            #[cfg(test)]
            hold_unpack: self.hold_unpack.clone(),
        };
        tokio::spawn(async move {
            let (state, record) = pipeline.run(cancelled).await;
            if let Some((staging, record)) = record {
                record.discard(&staging);
            }
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
        status
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

    /// Startup: remove every staging file that no kept install owns, and
    /// every interrupted publication. `resume` then queues the kept ones.
    ///
    /// Returns the folder name of every ready folder in the install cache
    /// that no library release names: korrid stopped between publishing it
    /// and writing the record. Nothing deletes it; the next Get of that game
    /// records it.
    pub(super) fn recover(
        &self,
        root: &Path,
        registry: &PluginRegistry,
    ) -> Result<Vec<String>, Failure> {
        for (record, owner) in registry.acquisition_providers() {
            let cache = placement::provider_cache(root, owner, &record.id);
            if !cache.is_dir() {
                continue;
            }
            // ADR 0030: stop a reader unit that outlived korrid before its
            // archive and work folder go.
            super::unit::reclaim_provider(&super::unit::provider_name(owner, &record.id))?;
            // ADR 0092: a kept install keeps its record and its bytes.
            let staging = cache.join(".downloads");
            let keep: Vec<String> = InstallRecord::all(&staging)
                .into_iter()
                .filter(|install| install.provider_id == record.id)
                .flat_map(|install| {
                    [
                        format!("{STAGING_PREFIX}{}", install.job_id),
                        format!("{STAGING_PREFIX}{}{RECORD_SUFFIX}", install.job_id),
                    ]
                })
                .collect();
            placement::remove_game_staging(&staging, &keep)?;
            placement::clean_work(&cache)?;
        }
        let cache = placement::copied_release_cache(root);
        // An install that korrid stopped while it published (ADR 0092).
        if cache.is_dir() {
            placement::clean_work(&cache)?;
        }
        let Some((library, _)) = crate::config::settings::runner_choice_snapshot(root).ok() else {
            return Ok(Vec::new());
        };
        let entries = match std::fs::read_dir(&cache) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut unrecorded = Vec::new();
        for entry in entries {
            let name = entry?.file_name().to_string_lossy().into_owned();
            let Some(key) = placement::ready_key(&name) else {
                continue;
            };
            let named = library.library.values().any(|game| {
                game.releases
                    .iter()
                    .any(|release| release.identity.as_ref().is_some_and(|i| i.value() == key))
            });
            if !named {
                unrecorded.push(name);
            }
        }
        unrecorded.sort();
        Ok(unrecorded)
    }

    /// ADR 0092: queue again every install that a restart interrupted, with
    /// its job id, its claim and its staged bytes. A record whose provider
    /// or system this device no longer has goes with its bytes.
    pub(super) fn resume(&self, coordinator: &AcquisitionCoordinator, context: &GameContext) {
        if tokio::runtime::Handle::try_current().is_err() {
            eprintln!(
                "korrid acquisition recovery: no runtime; kept installs wait for the next start"
            );
            return;
        }
        for (record, owner) in context.registry.acquisition_providers() {
            let staging =
                placement::provider_cache(&context.root, owner, &record.id).join(".downloads");
            for install in InstallRecord::all(&staging) {
                if install.provider_id != record.id {
                    continue;
                }
                let usable = context.registry.content_rules(&install.system).is_some();
                match Self::provider(&context.registry, &install.provider_id) {
                    Ok((provider, owner)) if usable => {
                        self.start(
                            coordinator,
                            context,
                            provider,
                            owner,
                            install,
                            super::Place::Back,
                        );
                    }
                    _ => install.discard(&staging),
                }
            }
        }
    }
}

/// The game a provider link names, when its download is still in the cache.
fn in_library(context: &GameContext, provider: &str, claim_id: &str) -> Option<String> {
    let snapshot = context.discovery.published().ok()?;
    let game_id = snapshot
        .provider_links
        .get(&link_key(provider, claim_id))?
        .playable
        .0
        .clone();
    (!acquired_game_dirs(&context.root, &context.registry, &snapshot, &game_id).is_empty())
        .then_some(game_id)
}

fn job_id() -> String {
    format!("{:032x}", rand::random::<u128>())
}

/// A provider install kept across a restart (ADR 0092, #551), beside its
/// staged bytes in the provider cache's private `.downloads` folder:
/// `.acquire-<jobId>` holds the bytes and `.acquire-<jobId>.json` this
/// record. Every field is an existing one under its existing name: `jobId`
/// from `AcquireJobStatus`; `providerId`, `id` and `fileName` from
/// `AcquireArtifactRequest`; `url`, `title` and `system` from the
/// `ProviderClaim` that korrid holds (`HeldClaim`); `etag` or `lastModified`
/// is the HTTP validator of the staged bytes, sent back as `If-Range`
/// (RFC 9110 section 13.1.5).
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InstallRecord {
    job_id: String,
    provider_id: String,
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file_name: Option<String>,
    url: String,
    title: String,
    system: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_modified: Option<String>,
}

const RECORD_SUFFIX: &str = ".json";

impl InstallRecord {
    fn validator(&self) -> Option<Validator> {
        self.etag
            .clone()
            .map(Validator::ETag)
            .or_else(|| self.last_modified.clone().map(Validator::LastModified))
    }

    fn keep_validator(&mut self, validator: Option<&Validator>) {
        (self.etag, self.last_modified) = match validator {
            Some(Validator::ETag(tag)) => (Some(tag.clone()), None),
            Some(Validator::LastModified(date)) => (None, Some(date.clone())),
            None => (None, None),
        };
    }

    fn path(staging: &Path, job_id: &str) -> PathBuf {
        staging.join(format!("{STAGING_PREFIX}{job_id}{RECORD_SUFFIX}"))
    }

    /// Written whole and renamed into place, so a crash leaves the old
    /// record or the new one.
    fn save(&self, staging: &Path) -> Result<(), Failure> {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let path = Self::path(staging, &self.job_id);
        let temporary = staging.join(format!("{STAGING_PREFIX}{}.tmp", self.job_id));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec(self).map_err(|_| Failure::Storage)?)?;
        file.sync_all()?;
        std::fs::rename(&temporary, &path)?;
        Ok(())
    }

    /// The install ends: its record and its staged bytes go.
    fn discard(&self, staging: &Path) {
        for path in [
            Self::path(staging, &self.job_id),
            staging.join(format!("{STAGING_PREFIX}{}", self.job_id)),
        ] {
            match std::fs::remove_file(&path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    eprintln!("korrid acquisition: an install file stayed after its end: {error}")
                }
                _ => (),
            }
        }
    }

    /// Every readable record in one staging folder, by job id. A record
    /// whose name and job id differ is not one.
    fn all(staging: &Path) -> Vec<Self> {
        let Ok(entries) = std::fs::read_dir(staging) else {
            return Vec::new();
        };
        let mut records: Vec<(std::time::SystemTime, Self)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                let since = metadata.created().or_else(|_| metadata.modified()).ok()?;
                let name = entry.file_name().into_string().ok()?;
                let job = name
                    .strip_prefix(STAGING_PREFIX)?
                    .strip_suffix(RECORD_SUFFIX)?;
                let record: Self =
                    serde_json::from_slice(&std::fs::read(entry.path()).ok()?).ok()?;
                (record.job_id == job
                    && job.len() == 32
                    && job.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .then_some((since, record))
            })
            .collect();
        // Oldest first: the order a restart keeps for installs that the
        // order file does not name (ADR 0013's rule for artifacts).
        records.sort_by_key(|(since, _)| *since);
        records.into_iter().map(|(_, record)| record).collect()
    }

    fn find(staging: &Path, provider_id: &str, id: &str) -> Option<Self> {
        Self::all(staging)
            .into_iter()
            .find(|record| record.provider_id == provider_id && record.id == id)
    }
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
    /// The provider cache's private `.downloads` folder.
    staging: PathBuf,
    record: InstallRecord,
    /// The record and the staged bytes stay after the end: the install
    /// resumes later.
    keep: bool,
    #[cfg(test)]
    hold_unpack: Option<Arc<std::sync::atomic::AtomicBool>>,
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

    /// The provider's `FinalDownload` for this turn, its outer name and the
    /// claim system's rules. A store job resolves only when its turn starts,
    /// so a signed link is fresh (ADR 0030), also after a pause or restart.
    async fn resolve(
        &self,
        cancelled: &mut watch::Receiver<bool>,
    ) -> Result<(GameRequest, String, crate::plugin::ContentRules), AcquireJobState> {
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
                    "This game cannot be downloaded directly from its provider.",
                )))
            }
            DownloadResolution::Failed(failed_download) => {
                return Err(failed(failure(
                    "DownloadResolutionFailed",
                    match failed_download.reason {
                        FailedDownloadReason::NotFound => {
                            "The provider no longer offers this game."
                        }
                        _ => "The provider could not prepare this download. Please retry later.",
                    },
                )))
            }
        };
        let name = outer_name(&download).map_err(download_failed)?;
        let rules = self
            .context
            .registry
            .content_rules(&self.system)
            .cloned()
            .ok_or_else(|| download_failed(Failure::NotSystemFile))?;
        // ADR 0030: before the download the name must end in a system or an
        // archive extension, or have no extension at all (decision U2:
        // romhustler's real extension is only in Content-Disposition, which
        // korrid never reads). After the download the bytes decide.
        if !system_name(&name, &rules)
            && !archive::archive_name(&name)
            && Path::new(&name).extension().is_some()
        {
            return Err(download_failed(Failure::NotSystemFile));
        }
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
        debug_assert_eq!(staging, self.staging);
        let request = GameRequest {
            url: download.url.clone(),
            form: download.form(),
        };
        Ok((request, name, rules))
    }

    /// Run the install. Returns its end state, and its record and staging
    /// folder when they must go: only a network failure or a stall keeps
    /// them, so the install resumes on the next Get or after a restart.
    async fn run(
        mut self,
        cancelled: watch::Receiver<bool>,
    ) -> (AcquireJobState, Option<(PathBuf, InstallRecord)>) {
        let mut wait = self.turn.take().expect("one turn per job");
        let state = match self.steps(&mut wait, cancelled).await {
            Ok(state) | Err(state) => state,
        };
        // The install ends here and leaves the person's order. A korrid
        // that stops before this keeps it there.
        wait.end();
        let discard = (!self.keep).then(|| (self.staging.clone(), self.record.clone()));
        (state, discard)
    }

    async fn steps(
        &mut self,
        wait: &mut super::TurnWait,
        mut cancelled: watch::Receiver<bool>,
    ) -> Result<AcquireJobState, AcquireJobState> {
        // ADR 0013 queue: one download at a time, in the order asked. The
        // job reports Queued while it waits; cancel leaves the queue. ADR
        // 0092: a launch can pause the download; it keeps its bytes, waits
        // again and resolves a fresh link when its turn comes back.
        let (_turn, downloaded, name, rules) = loop {
            let mut turn = tokio::select! {
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
            let (request, name, rules) = self.resolve(&mut cancelled).await?;
            let staged = self
                .staging
                .join(format!("{STAGING_PREFIX}{}", self.record.job_id));
            let done = std::fs::metadata(&staged).map_or(0, |metadata| metadata.len());
            self.set(AcquireJobState::Downloading {
                bytes_done: done,
                bytes_total: None,
                bytes_per_second: None,
            });
            turn.allow_pause();
            let flag = cancelled.clone();
            let validator = self.record.validator();
            let (job, staging) = (self.job.clone(), self.staging.clone());
            let record = Mutex::new(self.record.clone());
            let fetched = tokio::select! {
                biased;
                _ = cancelled.changed() => return Err(AcquireJobState::Cancelled),
                () = turn.paused() => None,
                downloaded = self.engine.fetch_game_into(
                    &request, &self.provider.hosts, &staged, validator.as_ref(),
                    |validator| {
                        let mut record = record.lock().expect("record mutex poisoned");
                        record.keep_validator(validator);
                        if let Err(error) = record.save(&staging) {
                            eprintln!("korrid acquisition: the install record was not saved: {}", error.rpc().message);
                        }
                    },
                    |progress| {
                        let mut job = job.lock().expect("job mutex poisoned");
                        if !*flag.borrow() {
                            job.status.state = AcquireJobState::Downloading {
                                bytes_done: progress.done,
                                bytes_total: progress.total,
                                bytes_per_second: progress.rate,
                            };
                        }
                    },
                ) => Some(downloaded),
            };
            // A pause keeps the validator of the bytes for the next turn.
            self.record = record.into_inner().expect("record mutex poisoned");
            match fetched {
                // Paused for a launch: the bytes stay, the turn goes back.
                None => {
                    self.set(AcquireJobState::Queued);
                    continue;
                }
                Some(downloaded) => {
                    let downloaded = downloaded.map_err(|error| {
                        self.keep = matches!(error, Failure::Network | Failure::Stalled);
                        download_failed(error)
                    })?;
                    // A pause that comes now lets this install finish first.
                    turn.forbid_pause();
                    break (turn, downloaded, name, rules);
                }
            }
        };
        let cache =
            placement::provider_cache(&self.context.root, &self.owner, &self.provider.provider_id);
        let format = archive::format(&downloaded.lead);
        // A native content format can itself use an archive container. Keep
        // its bytes only with positive plugin-owned byte evidence and a
        // non-transport system filename, after the normal archive validation.
        let native_container = format.is_some()
            && !archive::archive_name(&name)
            && system_name(&name, &rules)
            && identify::evidence_level(&name, &downloaded.lead, &rules, &[], &|_| Vec::new())
                == identify::EvidenceLevel::Bytes;
        if format.is_none() && !system_name(&name, &rules) {
            // A name that says archive, or has no extension, with bytes in
            // no archive format: discovery could never identify it.
            return Err(download_failed(Failure::NotSystemFile));
        }
        {
            // Cancellation wins until placement starts; placing cannot be
            // cancelled. Cancel checks the state under this same lock.
            let mut job = self.job.lock().expect("job mutex poisoned");
            if *cancelled.borrow() {
                return Err(AcquireJobState::Cancelled);
            }
            job.status.state = match format {
                Some(_) => AcquireJobState::Unpacking,
                None => AcquireJobState::Placing,
            };
        }
        // ADR 0030 Change 1: the content goes into a work folder, unpacked by
        // korrid's reader in its sandbox unit or moved in under its name.
        // Dropping the work folder or the download deletes it.
        let unit = super::unit::provider_name(&self.owner, &self.provider.provider_id);
        let launch = Arc::new(super::unit::Launch::default());
        let mut task = {
            let (cache, context, unit) = (cache.clone(), self.context.clone(), unit.clone());
            let (name, launch) = (name.clone(), launch.clone());
            #[cfg(test)]
            let hold = self.hold_unpack.clone();
            tokio::task::spawn_blocking(move || {
                let mut work = placement::WorkFolder::create(&cache)?;
                match format {
                    Some(_) => {
                        #[cfg(test)]
                        while hold.as_ref().is_some_and(|hold| {
                            hold.load(std::sync::atomic::Ordering::SeqCst) && !launch.is_cancelled()
                        }) {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        super::unit::unpack_archive(
                            &cache,
                            &context.private_root,
                            &unit,
                            &work.path,
                            &downloaded.path,
                            &launch,
                        )?
                    }
                    None => work.take_file(&downloaded.path, &name)?,
                }
                if native_container {
                    // Validation applies to native containers too. Discard the
                    // validated extracted view, then retain the original bytes
                    // for the native runner and local-copy content identity.
                    drop(work);
                    work = placement::WorkFolder::create(&cache)?;
                    work.take_file(&downloaded.path, &name)?;
                }
                // A transport archive goes before publication. A native one
                // has moved into the work folder without changing its bytes.
                drop(downloaded);
                Ok::<_, Failure>(work)
            })
        };
        let unpacked = if format.is_some() {
            tokio::select! {
                biased;
                _ = cancelled.changed() => {
                    // Prevent the launch, or stop the launched unit and
                    // wait for it; then the task deletes the work folder and
                    // the archive.
                    let stopper = launch.clone();
                    let _ = tokio::task::spawn_blocking(move || super::unit::cancel_reader(&unit, &stopper)).await;
                    let _ = task.await;
                    return Err(AcquireJobState::Cancelled);
                }
                unpacked = &mut task => unpacked,
            }
        } else {
            task.await
        };
        let work = unpacked
            .map_err(|_| download_failed(Failure::Storage))?
            .map_err(download_failed)?;
        {
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
                &context, &rules, work, &provider, &claim_id, &title, &system,
            )
        })
        .await
        .map_err(|_| download_failed(Failure::Storage))?
    }
}

/// ADR 0030 Change 3, coordinator decision Q3a (N1): the download's name is
/// `FinalDownload.filename`, then the URL's last segment, under the stricter
/// of legacy `SafeFileName` and the URL name rule. It never comes from the
/// server.
fn outer_name(download: &super::claims::FinalDownload) -> Result<String, Failure> {
    let name = match &download.filename {
        Some(name) => name.clone(),
        None => super::policy::basename(&download.url)?,
    };
    if !super::claims::safe_file_name(&name) || name.chars().any(char::is_control) {
        return Err(Failure::UnsafePath);
    }
    Ok(name)
}

/// True when a name ends in one of the system's extensions.
fn system_name(name: &str, rules: &crate::plugin::ContentRules) -> bool {
    let lower = name.to_ascii_lowercase();
    rules
        .extensions
        .iter()
        .any(|extension| lower.ends_with(&format!(".{}", extension.to_ascii_lowercase())))
}

/// ADR 0030 Change 1b: identify the work folder with discovery's rules. The
/// claim system's layouts come first and the default `**/{title}.{ext}`
/// after them, because a system's layouts describe a person's folder, not
/// an archive. The folder must give exactly one unit and no attention
/// finding: one ROM, a cue with its tracks, an `.m3u` set or one CHD.
pub(super) fn identify_work(
    work: &Path,
    rules: &crate::plugin::ContentRules,
) -> Result<ContentIdentity, Failure> {
    let rules = placement::work_rules(rules);
    let scan = identify::identify_with_rules(
        &SourceInput {
            source_id: "acquisition",
            storage_id: "cache",
            root: work,
            system: &rules.system,
            excluded_roots: &[],
        },
        &[&rules],
        false,
        &|_, _, _| None,
        &|_| Vec::new(),
        &mut HashCache::default(),
        &TraversalBudget::default(),
    );
    let [unit] = scan.units.as_slice() else {
        return Err(Failure::NotSystemFile);
    };
    if !scan.attention.is_empty() || !scan.diagnostics.is_empty() {
        return Err(Failure::NotSystemFile);
    }
    // Content rules: a header that would consume the whole file is not a game.
    if matches!(unit.identity, ContentIdentity::Hash { .. }) {
        use std::io::Read;
        let path = work.join(&unit.path);
        let size = std::fs::metadata(&path)?.len();
        let mut prefix = Vec::new();
        std::fs::File::open(&path)?
            .take(64 * 1024)
            .read_to_end(&mut prefix)?;
        if identify::header_size(&prefix, size, &rules.header) >= size {
            return Err(Failure::NotSystemFile);
        }
    }
    Ok(unit.identity.clone())
}

/// Identify the content, publish the ready directory, write the records.
#[allow(clippy::too_many_arguments)]
fn publish_and_record(
    context: &GameContext,
    rules: &crate::plugin::ContentRules,
    work: placement::WorkFolder,
    provider: &str,
    claim_id: &str,
    title: &str,
    system: &str,
) -> Result<AcquireJobState, AcquireJobState> {
    let identity = identify_work(&work.path, rules).map_err(download_failed)?;
    // ADR 0092: an install lands in the release-key layout; the provider
    // cache only held the job's work folder.
    let installs = placement::create_base(
        &context.root,
        placement::copied_release_cache(&context.root),
    )
    .map_err(download_failed)?;
    placement::publish_acquired_game(&installs, work, identity.value()).map_err(download_failed)?;
    let game_id = context
        .discovery
        .record_acquired(&AcquiredGame {
            provider,
            claim_id,
            title,
            system,
            identity: &identity,
        })
        .map_err(|error| {
            log::debug!("acquisition library record: {error}");
            download_failed(Failure::Storage)
        })?;
    Ok(AcquireJobState::Imported { game_id })
}
