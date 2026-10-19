//! Acquisition owns downloads, verification and publication; plugins supply data.
//! Ready is a cache-directory fact. Only active work and failures live in memory.
mod download;
mod placement;
mod policy;
mod types;
mod unit;

use crate::{
    plugin::{ArtifactRecord, PluginRegistry},
    RpcFailure,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;
use types::Failure;
pub use types::{
    AcquisitionCancelRequest, AcquisitionOutcome, AcquisitionSnapshot, AcquisitionStartRequest,
    AcquisitionStatusRequest, ArtifactState, ArtifactStatus,
};

#[derive(Clone, Default)]
pub struct AcquisitionCoordinator {
    jobs: Arc<Mutex<BTreeMap<PathBuf, Arc<Mutex<Job>>>>>,
    engine: download::DownloadEngine,
}
struct Job {
    version: String,
    state: ArtifactState,
    cancel: watch::Sender<bool>,
    done: watch::Receiver<bool>,
}

impl AcquisitionCoordinator {
    pub(crate) fn recover(&self, root: &Path, registry: &PluginRegistry) -> Result<(), RpcFailure> {
        for artifact in registry.artifacts().values() {
            let base = placement::contribution(root, artifact);
            if !placement::is_ready(&base) {
                continue;
            }
            let recovery = (|| {
                // Another live daemon owns this contribution; leave it alone.
                let Some(_guard) = placement::lock(&base)? else {
                    return Ok(());
                };
                unit::reclaim(artifact)?;
                placement::clean_work(&base)?;
                if placement::is_ready(&placement::ready_path(root, artifact))
                    && base.join(".downloads").is_dir()
                {
                    std::fs::remove_dir_all(base.join(".downloads"))?;
                }
                Ok::<(), Failure>(())
            })();
            recovery.map_err(Failure::rpc)?;
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
        let jobs = self.jobs.lock().expect("acquisition jobs mutex poisoned");
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
                        jobs.get(&placement::contribution(root, artifact))
                            .map(|job| {
                                let job = job.lock().expect("acquisition job mutex poisoned");
                                if job.version == artifact.version
                                    && !matches!(job.state, ArtifactState::Ready { .. })
                                {
                                    job.state.clone()
                                } else {
                                    ArtifactState::Missing
                                }
                            })
                            .unwrap_or(ArtifactState::Missing)
                    };
                    ArtifactStatus {
                        id: artifact.id.clone(),
                        title: title(registry, artifact),
                        version: artifact.version.clone(),
                        bytes_total: declared_bytes(artifact),
                        state,
                    }
                })
                .collect(),
        })
    }

    pub fn start(
        &self,
        root: &Path,
        private_root: &Path,
        registry: &PluginRegistry,
        ids: &[String],
    ) -> Result<AcquisitionSnapshot, RpcFailure> {
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
            let mut jobs = self.jobs.lock().expect("acquisition jobs mutex poisoned");
            for (artifact, unpack) in artifacts {
                let base = placement::contribution(root, &artifact);
                if placement::is_ready(&placement::ready_path(root, &artifact)) {
                    continue;
                }
                if jobs.get(&base).is_some_and(|job| {
                    !*job
                        .lock()
                        .expect("acquisition job mutex poisoned")
                        .done
                        .borrow()
                }) {
                    continue;
                }
                let (cancel, cancel_rx) = watch::channel(false);
                let (done_tx, done) = watch::channel(false);
                let job = Arc::new(Mutex::new(Job {
                    version: artifact.version.clone(),
                    state: ArtifactState::Downloading {
                        file: 1,
                        file_count: artifact.files.len() as u32,
                        bytes_done: 0,
                        bytes_total: declared_bytes(&artifact),
                        bytes_per_second: None,
                    },
                    cancel,
                    done,
                }));
                jobs.insert(base, job.clone());
                let root = root.to_path_buf();
                let private_root = private_root.to_path_buf();
                let engine = self.engine.clone();
                tokio::spawn(async move {
                    let result = acquire(
                        &root,
                        &private_root,
                        &artifact,
                        unpack,
                        engine,
                        &job,
                        cancel_rx,
                    )
                    .await;
                    let state = match result {
                        Ok(()) => ArtifactState::Ready {
                            path: placement::ready_path(&root, &artifact)
                                .display()
                                .to_string(),
                        },
                        Err(Failure::Cancelled) => ArtifactState::Missing,
                        Err(error) => ArtifactState::Failed {
                            reason: error.rpc(),
                        },
                    };
                    job.lock().expect("acquisition job mutex poisoned").state = state;
                    let _ = done_tx.send(true);
                });
            }
        }
        self.status(root, registry, Some(ids))
    }

    pub async fn cancel(
        &self,
        root: &Path,
        registry: &PluginRegistry,
        id: &str,
    ) -> Result<AcquisitionSnapshot, RpcFailure> {
        let artifact = registry.artifact(id).map_err(|_| unknown_artifact())?;
        let job = self
            .jobs
            .lock()
            .expect("acquisition jobs mutex poisoned")
            .get(&placement::contribution(root, artifact))
            .cloned();
        if let Some(job) = job {
            let done = {
                let job = job.lock().expect("acquisition job mutex poisoned");
                if matches!(job.state, ArtifactState::Downloading { .. }) {
                    let _ = job.cancel.send(true);
                    Some(job.done.clone())
                } else {
                    None
                }
            };
            if let Some(mut done) = done {
                while !*done.borrow() {
                    if done.changed().await.is_err() {
                        break;
                    }
                }
            }
        }
        self.status(root, registry, None)
    }
}

async fn acquire(
    root: &Path,
    private_root: &Path,
    artifact: &ArtifactRecord,
    unpack: Option<PathBuf>,
    engine: download::DownloadEngine,
    job: &Arc<Mutex<Job>>,
    mut cancelled: watch::Receiver<bool>,
) -> Result<(), Failure> {
    let base = placement::prepare(root, artifact)?;
    let guard;
    loop {
        if *cancelled.borrow() {
            return Err(Failure::Cancelled);
        }
        if let Some(lock) = placement::lock(&base)? {
            guard = lock;
            break;
        }
        tokio::select! {
            _ = cancelled.changed() => return Err(Failure::Cancelled),
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
            _ = cancelled.changed() => return Err(Failure::Cancelled),
            path = engine.fetch(file, &downloads, |progress| {
                if let Some(total) = progress.total { sizes[index] = Some(total); }
                job.lock().expect("acquisition job mutex poisoned").state = ArtifactState::Downloading {
                    file: index as u32 + 1, file_count: artifact.files.len() as u32,
                    bytes_done: completed + progress.done, bytes_total: total_bytes(&sizes), bytes_per_second: progress.rate,
                };
            }) => path?,
        };
        completed += std::fs::metadata(&path)?.len();
        files.push(path);
    }
    {
        let mut job = job.lock().expect("acquisition job mutex poisoned");
        if *cancelled.borrow() {
            return Err(Failure::Cancelled);
        }
        job.state = ArtifactState::Unpacking;
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

/// Used by both the launch gate and launch input: one directory-based truth.
pub(crate) fn require_ready(
    root: &Path,
    registry: &PluginRegistry,
    runner_id: &str,
) -> Result<(), RpcFailure> {
    let artifacts = registry
        .artifacts_for_runner(runner_id)
        .map_err(|_| unknown_artifact())?;
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

pub(crate) fn game_artifact_ids(
    root: &Path,
    registry: &PluginRegistry,
    game_id: &str,
) -> Result<Vec<String>, RpcFailure> {
    let (snapshot, _) = crate::config::settings::runner_choice_snapshot(root)
        .map_err(crate::game_routes::settings_failure)?;
    let route =
        crate::config::resolver::resolve_linux_route(root, &snapshot, registry, game_id, None)
            .map_err(|error| crate::route_diagnostic_failure(&error))?;
    Ok(registry
        .artifacts_for_runner(&route.runner_id)
        .map_err(|_| unknown_artifact())?
        .iter()
        .map(|artifact| artifact.id.clone())
        .collect())
}

#[cfg(test)]
mod rpc_tests;
