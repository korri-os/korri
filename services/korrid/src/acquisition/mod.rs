//! Acquisition owns downloads, verification and publication; plugins supply data.
//! Ready is a cache-directory fact. Only active work and failures live in memory.
pub mod claims;
mod download;
mod placement;
mod policy;
mod sleep;
pub mod turns;
mod types;
mod unit;

use crate::{
    plugin::{ArtifactRecord, PluginRegistry},
    RpcFailure,
};
use indexmap::IndexSet;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;
use types::Failure;
pub use types::{
    AcquireArtifactRequest, AcquireCancelRequest, AcquireJobOutcome, AcquireJobState,
    AcquireJobStatus, AcquireSourceChoice, AcquireStatusRequest, AcquisitionCancelRequest, AcquisitionDownloadRequest, AcquisitionOutcome, AcquisitionPluginsPayload, AcquisitionProvider, AcquisitionRemoveRequest, AcquisitionSnapshot, AcquisitionStartRequest, AcquisitionStatusRequest, ArtifactState, ArtifactStatus, DetailsOutcome, DetailsRequest, PluginListOutcome, PluginListResponse, SearchOutcome, SearchRequest, SearchResponse,
};

/// One job runs at a time. `waiting` is the person's order, front first; a
/// launch moves its artifacts to the front and pauses a background job,
/// which later resumes with `Range`. A queued contribution always has a
/// `.part` marker, so a restart can rebuild the queue from the cache alone.
#[derive(Clone, Default)]
pub struct AcquisitionCoordinator {
    queue: Arc<Mutex<Queue>>,
    engine: download::DownloadEngine,
    sleep: sleep::SleepBlock,
}
#[derive(Default)]
struct Queue {
    jobs: BTreeMap<PathBuf, Job>,
    waiting: VecDeque<PathBuf>,
    running: Option<Running>,
    worker: bool,
}
struct Job {
    artifact: ArtifactRecord,
    unpack: Option<PathBuf>,
    root: PathBuf,
    private_root: PathBuf,
    state: ArtifactState,
}
struct Running {
    base: PathBuf,
    stop: watch::Sender<Stop>,
    done: watch::Receiver<bool>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stop {
    Run,
    Pause,
    Cancel,
}

/// The artifacts of one game's default route, release artifacts first.
pub(crate) struct GameArtifacts {
    pub ids: Vec<String>,
    /// Every artifact that one of the game's declared releases names.
    pub release: BTreeSet<String>,
}

impl AcquisitionCoordinator {
    fn lock(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.queue.lock().expect("acquisition queue mutex poisoned")
    }

    pub(crate) fn recover(
        &self,
        root: &Path,
        private_root: &Path,
        registry: &PluginRegistry,
    ) -> Result<(), RpcFailure> {
        let mut interrupted = Vec::new();
        for artifact in registry.artifacts().values() {
            let base = placement::contribution(root, artifact);
            if !placement::is_ready(&base) {
                continue;
            }
            let recovery = (|| {
                // Another live daemon owns this contribution; leave it alone.
                let Some(_guard) = placement::lock(&base)? else {
                    return Ok(false);
                };
                unit::reclaim(artifact)?;
                placement::clean_work(&base)?;
                if placement::is_ready(&placement::ready_path(root, artifact)) {
                    if base.join(".downloads").is_dir() {
                        std::fs::remove_dir_all(base.join(".downloads"))?;
                    }
                    return Ok(false);
                }
                Ok::<bool, Failure>(true)
            })();
            if recovery.map_err(Failure::rpc)? {
                // Downloads with no ready directory always mean interrupted.
                if let Some(since) = placement::interrupted_since(&base) {
                    interrupted.push((since, artifact));
                }
            }
        }
        interrupted.sort_by_key(|(since, _)| *since);
        let ids: Vec<_> = interrupted
            .into_iter()
            .map(|(_, artifact)| artifact.id.clone())
            .collect();
        // One artifact that can no longer be queued must not hold back others.
        for id in ids {
            if let Err(failure) = self.enqueue(root, private_root, registry, &[id], false) {
                eprintln!("korrid acquisition recovery: {}", failure.message);
            }
        }
        Ok(())
    }

    pub fn status(
        &self,
        root: &Path,
        registry: &PluginRegistry,
        ids: Option<&[String]>,
    ) -> Result<AcquisitionSnapshot, RpcFailure> {
        let artifacts: Vec<_> = match ids {
            Some(ids) => ids
                .iter()
                .map(|id| registry.artifact(id))
                .collect::<Result<_, _>>()
                .map_err(|_| unknown_artifact())?,
            None => registry.artifacts().values().collect(),
        };
        self.ensure_worker();
        let queue = self.lock();
        Ok(AcquisitionSnapshot {
            artifacts: artifacts
                .into_iter()
                .map(|artifact| {
                    let path = placement::ready_path(root, artifact);
                    let state = if placement::is_ready(&path) {
                        ArtifactState::Ready {
                            path: path.display().to_string(),
                        }
                    } else {
                        queue
                            .jobs
                            .get(&placement::contribution(root, artifact))
                            .filter(|job| {
                                job.artifact.version == artifact.version
                                    && !matches!(job.state, ArtifactState::Ready { .. })
                            })
                            .map(|job| job.state.clone())
                            .unwrap_or(ArtifactState::Missing)
                    };
                    ArtifactStatus {
                        id: artifact.id.clone(),
                        title: title(registry, artifact),
                        version: artifact.version.clone(),
                        bytes_total: declared_bytes(artifact),
                        state,
                        release: None,
                    }
                })
                .collect(),
        })
    }

    /// Status of one game's artifacts, marking those its releases name.
    pub(crate) fn game_status(
        &self,
        root: &Path,
        registry: &PluginRegistry,
        game: &GameArtifacts,
    ) -> Result<AcquisitionSnapshot, RpcFailure> {
        let mut snapshot = self.status(root, registry, Some(&game.ids))?;
        for status in &mut snapshot.artifacts {
            if game.release.contains(&status.id) {
                status.release = Some(true);
            }
        }
        Ok(snapshot)
    }

    /// A launch is waiting for these: they go to the front, in this order.
    pub fn start(
        &self,
        root: &Path,
        private_root: &Path,
        registry: &PluginRegistry,
        ids: &[String],
    ) -> Result<AcquisitionSnapshot, RpcFailure> {
        self.enqueue(root, private_root, registry, ids, true)?;
        self.status(root, registry, Some(ids))
    }

    /// The Download action: every artifact of the route, behind earlier ones.
    pub(crate) fn download(
        &self,
        root: &Path,
        private_root: &Path,
        registry: &PluginRegistry,
        game: &GameArtifacts,
    ) -> Result<AcquisitionSnapshot, RpcFailure> {
        self.enqueue(root, private_root, registry, &game.ids, false)?;
        self.game_status(root, registry, game)
    }

    fn enqueue(
        &self,
        root: &Path,
        private_root: &Path,
        registry: &PluginRegistry,
        ids: &[String],
        front: bool,
    ) -> Result<(), RpcFailure> {
        // Resolve every id before starting any effects for this request.
        let artifacts = ids
            .iter()
            .map(|id| {
                let artifact = registry
                    .artifact(id)
                    .map_err(|_| unknown_artifact())?
                    .clone();
                let unpack = artifact
                    .unpack
                    .as_ref()
                    .map(|key| registry.installed_file(id, key))
                    .transpose()
                    .map_err(|_| Failure::Unpack.rpc())?;
                Ok((artifact, unpack))
            })
            .collect::<Result<Vec<_>, RpcFailure>>()?;
        {
            let mut queue = self.lock();
            let mut first = Vec::new();
            for (artifact, unpack) in artifacts {
                let base = placement::contribution(root, &artifact);
                if placement::is_ready(&placement::ready_path(root, &artifact)) {
                    continue;
                }
                let running = queue.running.as_ref().is_some_and(|run| run.base == base);
                if running || first.contains(&base) {
                    if running && front {
                        first.push(base);
                    }
                    continue;
                }
                if let Some(position) = queue.waiting.iter().position(|item| *item == base) {
                    if front {
                        queue.waiting.remove(position);
                        first.push(base);
                    }
                    continue;
                }
                let state = match placement::mark_queued(root, &artifact) {
                    Ok(()) => ArtifactState::Queued,
                    Err(error) => ArtifactState::Failed {
                        reason: error.rpc(),
                    },
                };
                let queued = matches!(state, ArtifactState::Queued);
                queue.jobs.insert(
                    base.clone(),
                    Job {
                        artifact,
                        unpack,
                        root: root.to_path_buf(),
                        private_root: private_root.to_path_buf(),
                        state,
                    },
                );
                if !queued {
                    continue;
                }
                if front {
                    first.push(base);
                } else {
                    queue.waiting.push_back(base);
                }
            }
            if front && !first.is_empty() {
                // A background download pauses for the launch and resumes
                // next, before anything else that was waiting.
                let mut paused = None;
                if let Some(running) = &queue.running {
                    if !first.contains(&running.base)
                        && queue.jobs.get(&running.base).is_some_and(|job| {
                            matches!(job.state, ArtifactState::Downloading { .. })
                        })
                    {
                        let _ = running.stop.send(Stop::Pause);
                        paused = Some(running.base.clone());
                    }
                }
                let running = queue.running.as_ref().map(|run| run.base.clone());
                let mut order: VecDeque<_> = first
                    .into_iter()
                    .filter(|base| Some(base) != running.as_ref())
                    .collect();
                order.extend(paused);
                order.extend(queue.waiting.drain(..));
                queue.waiting = order;
            }
        }
        self.ensure_worker();
        Ok(())
    }

    fn ensure_worker(&self) {
        let mut queue = self.lock();
        if queue.worker || queue.waiting.is_empty() {
            return;
        }
        // Startup may run outside a runtime; the next request starts it then.
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        queue.worker = true;
        handle.spawn(self.clone().work());
    }

    async fn work(self) {
        let mut sleep_guard = None;
        loop {
            let (base, artifact, unpack, root, private_root, stop, done) = {
                let mut queue = self.lock();
                let Some(base) = queue.waiting.pop_front() else {
                    queue.worker = false;
                    break;
                };
                let Some(job) = queue.jobs.get_mut(&base) else {
                    continue;
                };
                job.state = ArtifactState::Downloading {
                    file: 1,
                    file_count: job.artifact.files.len() as u32,
                    bytes_done: 0,
                    bytes_total: declared_bytes(&job.artifact),
                    bytes_per_second: None,
                };
                let next = (
                    base.clone(),
                    job.artifact.clone(),
                    job.unpack.clone(),
                    job.root.clone(),
                    job.private_root.clone(),
                );
                let (stop, stop_rx) = watch::channel(Stop::Run);
                let (done, done_rx) = watch::channel(false);
                queue.running = Some(Running {
                    base,
                    stop,
                    done: done_rx,
                });
                (next.0, next.1, next.2, next.3, next.4, stop_rx, done)
            };
            if sleep_guard.is_none() {
                sleep_guard = Some(self.sleep.block().await);
            }
            let result = acquire(
                &root,
                &private_root,
                &artifact,
                unpack,
                self.engine.clone(),
                &self.queue,
                &base,
                stop.clone(),
            )
            .await;
            let stopped = *stop.borrow();
            {
                let mut queue = self.lock();
                queue.running = None;
                let state = match result {
                    Ok(()) => ArtifactState::Ready {
                        path: placement::ready_path(&root, &artifact)
                            .display()
                            .to_string(),
                    },
                    Err(Failure::Cancelled) if stopped == Stop::Pause => ArtifactState::Queued,
                    Err(Failure::Cancelled) => ArtifactState::Missing,
                    Err(error) => ArtifactState::Failed {
                        reason: error.rpc(),
                    },
                };
                if let Some(job) = queue.jobs.get_mut(&base) {
                    job.state = state;
                }
            }
            if stopped == Stop::Cancel {
                if let Err(error) = discard_partials(&base).await {
                    eprintln!(
                        "korrid acquisition: partial files stayed after cancel: {}",
                        error.message
                    );
                }
            }
            let _ = done.send(true);
        }
        drop(sleep_guard);
    }

    /// Stop one download, delete its partial files and leave the queue.
    /// Unpacking cannot be cancelled.
    pub async fn cancel(
        &self,
        root: &Path,
        registry: &PluginRegistry,
        id: &str,
    ) -> Result<AcquisitionSnapshot, RpcFailure> {
        let artifact = registry.artifact(id).map_err(|_| unknown_artifact())?;
        let base = placement::contribution(root, artifact);
        enum Cancel {
            /// Unpacking cannot be cancelled.
            Unpacking,
            /// The worker stops the job and deletes its partial files.
            Running(watch::Receiver<bool>),
            /// Nothing runs: delete the partial files here.
            Idle,
        }
        let cancel = {
            let mut queue = self.lock();
            let unpacking = queue
                .jobs
                .get(&base)
                .is_some_and(|job| matches!(job.state, ArtifactState::Unpacking));
            if unpacking {
                Cancel::Unpacking
            } else if let Some(running) = queue.running.as_ref().filter(|run| run.base == base) {
                let _ = running.stop.send(Stop::Cancel);
                let done = running.done.clone();
                // A job a launch just paused is also back in the queue.
                queue.waiting.retain(|item| *item != base);
                Cancel::Running(done)
            } else {
                queue.waiting.retain(|item| *item != base);
                if let Some(job) = queue.jobs.get_mut(&base) {
                    if matches!(job.state, ArtifactState::Queued) {
                        job.state = ArtifactState::Missing;
                    }
                }
                Cancel::Idle
            }
        };
        match cancel {
            Cancel::Unpacking => (),
            Cancel::Running(mut done) => {
                while !*done.borrow() {
                    if done.changed().await.is_err() {
                        break;
                    }
                }
            }
            Cancel::Idle => discard_partials(&base).await?,
        }
        self.status(root, registry, None)
    }

    /// Remove download (ADR 0013): only the game's own release artifacts,
    /// never a runner's, and never one another game in the library names.
    pub(crate) async fn remove(
        &self,
        root: &Path,
        registry: &PluginRegistry,
        game_id: &str,
        game: &GameArtifacts,
    ) -> Result<AcquisitionSnapshot, RpcFailure> {
        let (snapshot, _) = crate::config::settings::runner_choice_snapshot(root)
            .map_err(crate::game_routes::settings_failure)?;
        let runner_artifacts: BTreeSet<&str> = registry
            .runners()
            .values()
            .flat_map(|runner| runner.artifacts.iter().map(String::as_str))
            .collect();
        let shared: BTreeSet<String> = snapshot
            .library
            .keys()
            .filter(|id| *id != game_id)
            .flat_map(|id| release_artifacts(&snapshot, registry, id))
            .collect();
        {
            let queue = self.lock();
            let busy = game.ids.iter().chain(&game.release).any(|id| {
                // Ready is the directory, even while the worker finishes up.
                registry.artifact(id).is_ok_and(|artifact| {
                    let base = placement::contribution(root, artifact);
                    !placement::is_ready(&placement::ready_path(root, artifact))
                        && (queue.running.as_ref().is_some_and(|run| run.base == base)
                            || queue.waiting.contains(&base))
                })
            });
            if busy {
                return Err(busy_failure());
            }
        }
        for id in &game.release {
            if runner_artifacts.contains(id.as_str()) || shared.contains(id) {
                continue;
            }
            let Ok(artifact) = registry.artifact(id) else {
                continue;
            };
            let ready = placement::ready_path(root, artifact);
            if !placement::is_ready(&ready) {
                continue;
            }
            let base = placement::contribution(root, artifact);
            let Some(_guard) = lock_briefly(&base).await? else {
                return Err(busy_failure());
            };
            placement::discard_work(&ready).map_err(Failure::rpc)?;
        }
        self.game_status(root, registry, game)
    }
}

/// Take `.lock`, waiting up to two seconds. A writer that just stopped,
/// publication right after its rename, or a process that one of them forked
/// can hold it for a moment. `None` means another live holder keeps it.
async fn lock_briefly(base: &Path) -> Result<Option<std::fs::File>, RpcFailure> {
    for _ in 0..100 {
        if let Some(guard) = placement::lock(base).map_err(Failure::rpc)? {
            return Ok(Some(guard));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Ok(None)
}

/// Cancel deletes downloaded files; a live writer elsewhere keeps its own.
async fn discard_partials(base: &Path) -> Result<(), RpcFailure> {
    if !placement::is_ready(base) {
        return Ok(());
    }
    let Some(_guard) = lock_briefly(base).await? else {
        return Ok(());
    };
    placement::remove_downloads(base).map_err(Failure::rpc)
}

#[allow(clippy::too_many_arguments)]
async fn acquire(
    root: &Path,
    private_root: &Path,
    artifact: &ArtifactRecord,
    unpack: Option<PathBuf>,
    engine: download::DownloadEngine,
    queue: &Arc<Mutex<Queue>>,
    job: &Path,
    mut stopped: watch::Receiver<Stop>,
) -> Result<(), Failure> {
    let set_state = |state: ArtifactState| {
        if let Some(job) = queue
            .lock()
            .expect("acquisition queue mutex poisoned")
            .jobs
            .get_mut(job)
        {
            job.state = state;
        }
    };
    let base = placement::prepare(root, artifact)?;
    let guard;
    loop {
        if *stopped.borrow() != Stop::Run {
            return Err(Failure::Cancelled);
        }
        if let Some(lock) = placement::lock(&base)? {
            guard = lock;
            break;
        }
        tokio::select! {
            _ = stopped.changed() => return Err(Failure::Cancelled),
            _ = tokio::time::sleep(Duration::from_millis(100)) => (),
        }
    }
    let stale_artifact = artifact.clone();
    tokio::task::spawn_blocking(move || unit::reclaim(&stale_artifact))
        .await
        .map_err(|_| Failure::Unpack)??;
    placement::clean_work(&base)?;
    if placement::is_ready(&placement::ready_path(root, artifact)) {
        return Ok(());
    }
    let downloads = placement::downloads(&base)?;
    let mut files = Vec::new();
    let mut completed = 0;
    let mut sizes: Vec<_> = artifact.files.iter().map(|file| file.bytes).collect();
    for (index, file) in artifact.files.iter().enumerate() {
        let path = tokio::select! {
            biased;
            _ = stopped.changed() => return Err(Failure::Cancelled),
            path = engine.fetch(file, &downloads, |progress| {
                if sizes[index].is_none() { sizes[index] = progress.total; }
                set_state(ArtifactState::Downloading {
                    file: index as u32 + 1, file_count: artifact.files.len() as u32,
                    bytes_done: completed + progress.done, bytes_total: total_bytes(&sizes), bytes_per_second: progress.rate,
                });
            }) => path?,
        };
        let bytes = std::fs::metadata(&path)?.len();
        sizes[index].get_or_insert(bytes);
        completed += bytes;
        files.push(path);
    }
    {
        // Pause and cancel decide under the same lock, so a stop that saw
        // Downloading is never lost once unpacking starts.
        let mut queue = queue.lock().expect("acquisition queue mutex poisoned");
        if *stopped.borrow() != Stop::Run {
            return Err(Failure::Cancelled);
        }
        if let Some(job) = queue.jobs.get_mut(job) {
            job.state = ArtifactState::Unpacking;
        }
    }
    let private_root = private_root.to_path_buf();
    let artifact = artifact.clone();
    tokio::task::spawn_blocking(move || {
        let _guard = guard; // Keep cross-process exclusion until publication completes.
        placement::publish(&base, &private_root, &artifact, unpack.as_deref(), &files)
    })
    .await
    .map_err(|_| Failure::Unpack)?
}

fn total_bytes(sizes: &[Option<u64>]) -> Option<u64> {
    sizes
        .iter()
        .try_fold(0u64, |sum, size| sum.checked_add((*size)?))
}
fn declared_bytes(artifact: &ArtifactRecord) -> Option<u64> {
    total_bytes(
        &artifact
            .files
            .iter()
            .map(|file| file.bytes)
            .collect::<Vec<_>>(),
    )
}
fn title(registry: &PluginRegistry, artifact: &ArtifactRecord) -> String {
    artifact.title.clone().unwrap_or_else(|| {
        let (owner, key) = artifact.id.split_once('/').expect("validated artifact id");
        format!(
            "{} {key}",
            registry.plugin_title(owner).expect("loaded artifact owner")
        )
    })
}
fn failure_title(title: &str) -> String {
    // A plugin title is presentation data, not permission to echo a private URL
    // or path in an error. Normal artifact titles remain useful in launch copy.
    if title.contains(['/', '\\', ':', '@'])
        || title
            .split_whitespace()
            .any(|word| word.len() >= 40 && word.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        "Required downloads".into()
    } else {
        crate::launcher::redaction::status_text(title, 80)
    }
}
fn unknown_artifact() -> RpcFailure {
    RpcFailure {
        code: "AcquisitionUnknownArtifact".into(),
        message: "The requested download is not available.".into(),
    }
}

fn busy_failure() -> RpcFailure {
    RpcFailure {
        code: "AcquisitionBusy".into(),
        message: "This game is still downloading. Cancel the download first.".into(),
    }
}

/// A route's artifacts: the release's first, then the runner's, once each.
pub(crate) fn route_artifact_ids(
    registry: &PluginRegistry,
    route: &crate::config::resolver::ResolvedRoute,
) -> Result<Vec<String>, RpcFailure> {
    let mut ids: IndexSet<String> = route
        .artifact_target
        .iter()
        .flat_map(|target| target.release_artifacts.iter().cloned())
        .collect();
    ids.extend(
        registry
            .artifacts_for_runner(&route.runner_id)
            .map_err(|_| unknown_artifact())?
            .into_iter()
            .map(|artifact| artifact.id.clone()),
    );
    Ok(ids.into_iter().collect())
}

/// Used by both the launch gate and launch input: one directory-based truth.
/// A route refuses until its release's and its runner's artifacts are ready.
pub(crate) fn require_ready(
    root: &Path,
    registry: &PluginRegistry,
    route: &crate::config::resolver::ResolvedRoute,
) -> Result<(), RpcFailure> {
    let artifacts = route_artifact_ids(registry, route)?
        .iter()
        .map(|id| registry.artifact(id).map_err(|_| unknown_artifact()))
        .collect::<Result<Vec<_>, _>>()?;
    let missing: Vec<_> = artifacts
        .iter()
        .filter(|artifact| !placement::is_ready(&placement::ready_path(root, artifact)))
        .collect();
    if !missing.is_empty() {
        return Err(RpcFailure {
            code: "ArtifactsNotReady".into(),
            message: crate::launcher::redaction::status_text(
                &format!(
                    "{} are not downloaded yet.",
                    missing
                        .iter()
                        .map(|artifact| failure_title(&title(registry, artifact)))
                        .collect::<Vec<_>>()
                        .join(" and ")
                ),
                crate::launcher::redaction::MAX_REASON_CHARS,
            ),
        });
    }
    Ok(())
}

/// `contentPath` for an artifact route: its ready directory joined with the
/// declared path. Nothing else is derived.
///
/// Readiness is not checked here. Catalog and runner listings preflight every
/// route through `launch_route` before anything is downloaded, and a game that
/// still needs its download must stay listed and launchable through the
/// download gate (ADR 0013). `require_ready` refuses the session start until
/// every release and runner artifact is ready, so no launch reaches this path
/// before its directory exists.
pub(crate) fn artifact_content_path(
    root: &Path,
    registry: &PluginRegistry,
    target: &crate::config::resolver::ResolvedArtifactTarget,
) -> Result<PathBuf, RpcFailure> {
    let artifact = registry
        .artifact(&target.artifact_id)
        .map_err(|_| unknown_artifact())?;
    Ok(placement::ready_path(root, artifact).join(&target.path))
}

pub(crate) fn launch_artifacts(
    root: &Path,
    registry: &PluginRegistry,
    runner_id: &str,
) -> Result<BTreeMap<String, String>, RpcFailure> {
    let artifacts = registry
        .artifacts_for_runner(runner_id)
        .map_err(|_| unknown_artifact())?;
    let owner = runner_id.split_once('/').expect("validated runner id").0;
    Ok(artifacts
        .into_iter()
        .filter(|artifact| placement::is_ready(&placement::ready_path(root, artifact)))
        .map(|artifact| {
            let (plugin, key) = artifact.id.split_once('/').expect("validated artifact id");
            (
                if plugin == owner {
                    key.to_owned()
                } else {
                    artifact.id.clone()
                },
                placement::ready_path(root, artifact).display().to_string(),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests;

/// The artifacts that this game's declared releases name, in library order.
fn release_artifacts(
    snapshot: &crate::config::ConfigSnapshot,
    registry: &PluginRegistry,
    game_id: &str,
) -> IndexSet<String> {
    snapshot
        .library
        .get(game_id)
        .into_iter()
        .flat_map(|game| &game.releases)
        .filter_map(|release| release.identity.as_ref())
        .flat_map(|identity| registry.declared_hash(identity.value()))
        .flat_map(|declared| declared.release.artifacts.iter().cloned())
        .collect()
}

/// Resolve the game's default route exactly as `app.session.start` does.
pub(crate) fn game_artifacts(
    root: &Path,
    registry: &PluginRegistry,
    game_id: &str,
    runner_id: Option<&str>,
) -> Result<GameArtifacts, RpcFailure> {
    let (snapshot, _) = crate::config::settings::runner_choice_snapshot(root)
        .map_err(crate::game_routes::settings_failure)?;
    let route =
        crate::config::resolver::resolve_linux_route(root, &snapshot, registry, game_id, runner_id)
            .map_err(|error| crate::route_diagnostic_failure(&error))?;
    Ok(GameArtifacts {
        ids: route_artifact_ids(registry, &route)?,
        release: release_artifacts(&snapshot, registry, game_id)
            .into_iter()
            .collect(),
    })
}

/// Remove download does not need a route: a disabled runner must not keep
/// a person from freeing space. Without a route, only release artifacts show.
pub(crate) fn game_artifacts_for_removal(
    root: &Path,
    registry: &PluginRegistry,
    game_id: &str,
) -> Result<GameArtifacts, RpcFailure> {
    let (snapshot, _) = crate::config::settings::runner_choice_snapshot(root)
        .map_err(crate::game_routes::settings_failure)?;
    if !snapshot.library.contains_key(game_id) {
        return Err(crate::game_routes::unavailable(format!(
            "game {game_id} is unavailable"
        )));
    }
    let release = release_artifacts(&snapshot, registry, game_id);
    let mut ids: IndexSet<String> =
        crate::config::resolver::resolve_linux_route(root, &snapshot, registry, game_id, None)
            .ok()
            .and_then(|route| route_artifact_ids(registry, &route).ok())
            .unwrap_or_default()
            .into_iter()
            .collect();
    ids.extend(release.iter().cloned());
    Ok(GameArtifacts {
        ids: ids.into_iter().collect(),
        release: release.into_iter().collect(),
    })
}

#[cfg(test)]
mod game_tests;
#[cfg(test)]
mod queue_tests;
#[cfg(test)]
mod rpc_tests;
