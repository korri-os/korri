//! ADR 0092 (#552): devices are install sources for file releases.
//!
//! A holder hashes a release in place with iroh-blobs (`ImportMode::
//! TryReference`) and keeps the BLAKE3 tree in its store at
//! `<storage root>/cache/.blobs`. The store is derived data: korrid rebuilds
//! it by hashing again when it is missing or unreadable. An iroh-blobs named
//! tag, whose name is the release key (ADR 0030), names the release's
//! collection: its file names and their BLAKE3 hashes. A disc set or a cue
//! with its tracks is one collection.
//!
//! Pieces travel over korrid's signed peer HTTP, the same authenticated
//! boundary as the other peer objects. A holder answers a piece with the
//! iroh-blobs BAO encoding of its chunks; the receiver checks every piece
//! against the file's BLAKE3 hash as it arrives (`import_bao_bytes`). One
//! install takes pieces from every reachable holder at once; a holder that
//! fails or sends a bad piece leaves the install, and its pieces go to the
//! others. The finished install is checked against its release key, then
//! published in the ADR 0092 layout and recorded in the library.
//!
//! Access (ADR 0077): the device owner's devices and family members' devices
//! may install, with no prompt on the holder. Guests may not.
//!
//! Netplay installs the release it needs from the holding device here
//! (#554, `netplay.rs`); play-here (#553) installs from every holder.
use super::{game::GameContext, placement, space::Floor, Failure, Place};
use crate::{
    authorization::{AuthorizationContext, Principal},
    RpcFailure,
};
use bao_tree::{ChunkNum, ChunkRanges};
use iroh_blobs::{
    api::{
        blobs::{AddPathOptions, ExportMode, ExportOptions, ImportMode},
        proto::BlobStatus,
    },
    format::collection::Collection,
    store::fs::FsStore,
    BlobFormat, Hash, HashAndFormat,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

/// One piece: 1 MiB, a whole number of iroh-blobs chunk groups (16 KiB).
/// Tests use 64 KiB pieces, the same rule at a size that keeps the debug
/// build's hashing light beside the other tests of the lib suite.
pub(crate) const PIECE_BYTES: u64 = if cfg!(test) { 64 * 1024 } else { 1 << 20 };
/// A holder that sends this many bad or failed pieces leaves the install.
const HOLDER_STRIKES: u32 = 3;
const PIECE_TIMEOUT: Duration = Duration::from_secs(60);

/// `app.installs.manifest`: what the holder holds of one release.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallManifestRequest {
    pub release_key: String,
}

/// One file of a release: its path in the release, its size and its BLAKE3
/// hash (iroh-blobs `Hash`, hex).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallFile {
    pub name: String,
    pub bytes: u64,
    pub blake3: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallManifest {
    pub files: Vec<InstallFile>,
}

impl InstallManifest {
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|file| file.bytes).sum()
    }
    /// The files as a set of contents, whatever the holder named them.
    fn contents(&self) -> Vec<(String, u64)> {
        let mut contents: Vec<_> = self
            .files
            .iter()
            .map(|file| (file.blake3.clone(), file.bytes))
            .collect();
        contents.sort();
        contents
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum InstallManifestOutcome {
    Ok(InstallManifest),
    Err(RpcFailure),
}

/// `app.installs.piece`: the BAO encoding of `[offset, offset + length)` of
/// one file of a release.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallPieceRequest {
    pub release_key: String,
    pub blake3: String,
    pub offset: u64,
    pub length: u64,
}

/// ADR 0077: the owner's devices and family members' devices install from
/// this device with no prompt. Guests and unknown devices do not.
pub(crate) fn may_install(context: &AuthorizationContext) -> bool {
    matches!(
        context,
        AuthorizationContext::Peer(Principal::OwnerDevice { .. } | Principal::Household { .. })
    )
}

fn failure(code: &str, message: &str) -> RpcFailure {
    RpcFailure {
        code: code.into(),
        message: message.into(),
    }
}
fn not_held() -> RpcFailure {
    failure(
        "InstallSourceUnavailable",
        "This device does not hold that release.",
    )
}
fn refused() -> RpcFailure {
    failure(
        "InstallRefused",
        "This device does not serve installs to you.",
    )
}
fn install_failed() -> RpcFailure {
    failure(
        "InstallFailed",
        "The release could not be installed from devices.",
    )
}

/// The chunks of `[offset, offset + length)`.
fn chunk_ranges(offset: u64, length: u64) -> ChunkRanges {
    ChunkRanges::from(ChunkNum::full_chunks(offset)..ChunkNum::chunks(offset + length))
}

/// The store of one storage root, opened once.
#[derive(Clone, Default)]
pub(crate) struct Pieces {
    stores: Arc<tokio::sync::Mutex<BTreeMap<PathBuf, FsStore>>>,
    /// One release is hashed at a time.
    hashing: Arc<tokio::sync::Mutex<()>>,
    /// Installs from devices, by job id. Memory only (see `DeviceJob`).
    jobs: Arc<Mutex<BTreeMap<String, DeviceJob>>>,
    /// ADR 0092 (#556): taking and serving pieces yield to play.
    gate: super::play::Gate,
    #[cfg(test)]
    fault: Arc<Mutex<Option<PublishFault>>>,
}

pub(crate) fn store_path(root: &Path) -> PathBuf {
    placement::install_cache(root).join(".blobs")
}

/// The folder of the receive stores, inside the holder store's folder.
const RECEIVE_FOLDER: &str = "installs";

/// Delete the holder store's own files, and never the receive stores
/// beside them (`installs/`), which hold verified pieces and may be in use.
fn remove_holder_store(path: &Path) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        if entry.file_name() == RECEIVE_FOLDER {
            continue;
        }
        let entry = entry.path();
        let _ = if entry.is_dir() && !entry.is_symlink() {
            std::fs::remove_dir_all(&entry)
        } else {
            std::fs::remove_file(&entry)
        };
    }
}

impl Pieces {
    pub(crate) fn gate(&self) -> &super::play::Gate {
        &self.gate
    }

    /// The store, opened once. A store that cannot be opened is rebuilt.
    async fn store(&self, root: &Path) -> Result<FsStore, RpcFailure> {
        let mut stores = self.stores.lock().await;
        if let Some(store) = stores.get(root) {
            return Ok(store.clone());
        }
        let path = store_path(root);
        let cache = placement::create_base(root, placement::install_cache(root))
            .map_err(|error| error.rpc())?;
        debug_assert_eq!(path.parent(), Some(cache.as_path()));
        let store = match FsStore::load(&path).await {
            Ok(store) => store,
            Err(error) => {
                eprintln!("korrid install sources: the piece store is rebuilt: {error}");
                remove_holder_store(&path);
                FsStore::load(&path).await.map_err(|_| install_failed())?
            }
        };
        stores.insert(root.to_path_buf(), store.clone());
        Ok(store)
    }

    /// Holder: the release's files, hashed in place once and kept.
    pub(crate) async fn manifest(
        &self,
        context: &GameContext,
        key: &str,
    ) -> Result<InstallManifest, RpcFailure> {
        self.manifest_with(context, key, false).await
    }

    async fn manifest_with(
        &self,
        context: &GameContext,
        key: &str,
        rehash: bool,
    ) -> Result<InstallManifest, RpcFailure> {
        placement::ready_name(key).ok_or_else(not_held)?;
        let files = {
            let (context, key) = (context.clone(), key.to_owned());
            tokio::task::spawn_blocking(move || {
                let snapshot = context.discovery.published().map_err(|_| not_held())?;
                release_files(&context.root, &snapshot, &key)
            })
            .await
            .map_err(|_| not_held())??
        };
        let store = self.store(&context.root).await?;
        if !rehash {
            if let Some(manifest) = kept(&store, key, &files).await {
                return Ok(manifest);
            }
        }
        let _hashing = self.hashing.lock().await;
        if !rehash {
            if let Some(manifest) = kept(&store, key, &files).await {
                return Ok(manifest);
            }
        }
        let mut collection = Collection::default();
        let mut manifest = InstallManifest { files: Vec::new() };
        let mut tags = Vec::new();
        for (name, path) in files {
            let tag = store
                .add_path_with_opts(AddPathOptions {
                    path,
                    format: BlobFormat::Raw,
                    mode: ImportMode::TryReference,
                })
                .temp_tag()
                .await
                .map_err(|_| not_held())?;
            let hash = tag.hash();
            let BlobStatus::Complete { size } = store.status(hash).await.map_err(|_| not_held())?
            else {
                return Err(not_held());
            };
            manifest.files.push(InstallFile {
                name: name.clone(),
                bytes: size,
                blake3: hash.to_hex(),
            });
            collection.push(name, hash);
            tags.push(tag);
        }
        let root = collection.store(&store).await.map_err(|_| not_held())?;
        store
            .tags()
            .set(key, HashAndFormat::hash_seq(root.hash()))
            .await
            .map_err(|_| not_held())?;
        Ok(manifest)
    }

    /// Holder: one piece of one file of a release this device holds. A
    /// file that moved or changed since it was hashed is hashed again once.
    pub(crate) async fn piece(
        &self,
        context: &GameContext,
        request: &InstallPieceRequest,
    ) -> Result<Vec<u8>, RpcFailure> {
        if request.length == 0 || request.length > PIECE_BYTES {
            return Err(not_held());
        }
        let mut rehash = false;
        loop {
            let manifest = self
                .manifest_with(context, &request.release_key, rehash)
                .await?;
            let file = manifest
                .files
                .iter()
                .find(|file| file.blake3 == request.blake3)
                .ok_or_else(not_held)?;
            if request.offset >= file.bytes {
                return Err(not_held());
            }
            let hash: Hash = file.blake3.parse().map_err(|_| not_held())?;
            let store = self.store(&context.root).await?;
            let length = request.length.min(file.bytes - request.offset);
            match store
                .export_bao(hash, chunk_ranges(request.offset, length))
                .bao_to_vec()
                .await
            {
                Ok(encoded) => return Ok(encoded),
                Err(_) if !rehash => rehash = true,
                Err(_) => return Err(not_held()),
            }
        }
    }
}

/// The kept manifest, when the store still holds every file of `files`.
async fn kept(store: &FsStore, key: &str, files: &[(String, PathBuf)]) -> Option<InstallManifest> {
    let tag = store.tags().get(key).await.ok()??;
    let collection = Collection::load(tag.hash, &**store).await.ok()?;
    let names: Vec<_> = collection.iter().map(|(name, _)| name.clone()).collect();
    let expected: Vec<_> = files.iter().map(|(name, _)| name.clone()).collect();
    if names != expected {
        return None;
    }
    let mut manifest = InstallManifest { files: Vec::new() };
    for ((name, hash), (_, path)) in collection.iter().zip(files) {
        let BlobStatus::Complete { size } = store.status(*hash).await.ok()? else {
            return None;
        };
        // A file that changed size is not the file that was hashed.
        if std::fs::metadata(path).ok()?.len() != size {
            return None;
        }
        manifest.files.push(InstallFile {
            name: name.clone(),
            bytes: size,
            blake3: hash.to_hex(),
        });
    }
    Some(manifest)
}

/// The files of one file release on this device, as `(name in the release,
/// absolute path)`: an installed release (ADR 0092 layout), else the first
/// library location of the release key that is not missing. Netplay's
/// holder (`host/netplay.rs`) reads the size of the release it serves here.
pub(crate) fn release_files(
    root: &Path,
    snapshot: &crate::config::ConfigSnapshot,
    key: &str,
) -> Result<Vec<(String, PathBuf)>, RpcFailure> {
    let ready =
        placement::install_cache(root).join(placement::ready_name(key).ok_or_else(not_held)?);
    if placement::is_ready(&ready) {
        let mut files = Vec::new();
        walk(&ready, &ready, &mut files)?;
        files.sort();
        if !files.is_empty() {
            return Ok(files);
        }
    }
    for location in snapshot.locations.get(key).into_iter().flatten() {
        let (storage, parts, discovery) = match location {
            crate::config::Location::File {
                storage,
                path,
                discovery,
            } => {
                let name = Path::new(&path.0)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (storage, vec![(name, path.0.clone())], discovery)
            }
            crate::config::Location::FileSet {
                storage,
                root,
                files,
                discovery,
            } => (
                storage,
                files
                    .0
                    .iter()
                    .map(|part| {
                        let path = root
                            .as_ref()
                            .map(|root| format!("{}/{}", root.0, part.path.0))
                            .unwrap_or_else(|| part.path.0.clone());
                        (part.path.0.clone(), path)
                    })
                    .collect(),
                discovery,
            ),
            _ => continue,
        };
        if discovery
            .as_ref()
            .is_some_and(|d| d.missing_since.is_some())
        {
            continue;
        }
        let resolved: Option<Vec<_>> = parts
            .into_iter()
            .map(|(name, path)| {
                crate::config::storage::resolve_file_target(
                    root,
                    snapshot,
                    &crate::config::resolver::ResolvedFileTarget {
                        storage_id: storage.0.clone(),
                        path,
                    },
                )
                .ok()
                .map(|file| (name, file.path))
            })
            .collect();
        if let Some(files) = resolved.filter(|files| !files.is_empty()) {
            return Ok(files);
        }
    }
    Err(not_held())
}

fn walk(base: &Path, folder: &Path, files: &mut Vec<(String, PathBuf)>) -> Result<(), RpcFailure> {
    for entry in std::fs::read_dir(folder).map_err(|_| not_held())? {
        let entry = entry.map_err(|_| not_held())?;
        let kind = entry.file_type().map_err(|_| not_held())?;
        if kind.is_dir() {
            walk(base, &entry.path(), files)?;
        } else if kind.is_file() {
            let name = entry
                .path()
                .strip_prefix(base)
                .map_err(|_| not_held())?
                .to_string_lossy()
                .into_owned();
            files.push((name, entry.path()));
        }
    }
    Ok(())
}

/// A name in a holder's manifest becomes a path in the work folder: each
/// segment must be a safe file name (legacy `SafeFileName`), never `.` or
/// `..`, never hidden, and with no control character.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('/').all(|segment| {
            super::claims::safe_file_name(segment)
                && !segment.starts_with('.')
                && !segment.chars().any(char::is_control)
        })
}

/// Where a receiver asks a holder. Production is a federation peer over
/// signed peer HTTP (`PeerHolder`); the trait keeps the scheduler testable.
pub(crate) trait Holder: Send + Sync {
    fn label(&self) -> String;
    fn manifest(
        &self,
        key: &str,
    ) -> impl std::future::Future<Output = Result<InstallManifest, RpcFailure>> + Send;
    fn piece(
        &self,
        request: InstallPieceRequest,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, RpcFailure>> + Send;
}

/// What a surface can read of one install from devices (ADR 0092, #552).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag")]
pub(crate) enum DeviceInstallState {
    /// In the line, behind another install.
    Waiting,
    Installing {
        #[serde(rename = "bytesDone")]
        bytes_done: u64,
        #[serde(rename = "bytesTotal")]
        bytes_total: u64,
    },
    Imported {
        #[serde(rename = "gameId")]
        game_id: String,
    },
    Failed {
        reason: RpcFailure,
    },
    Cancelled,
}

impl DeviceInstallState {
    fn live(&self) -> bool {
        matches!(self, Self::Waiting | Self::Installing { .. })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeviceInstallStatus {
    pub job_id: String,
    pub release_key: String,
    pub state: DeviceInstallState,
    /// Set while the install yields to play (#556); none at full pace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yielding: Option<super::InstallYield>,
}

/// One install from devices. It lives in memory only: a restart forgets
/// it, because the schema of a kept devices-install record is not decided.
/// Its verified pieces are not lost: they stay in its receive store.
struct DeviceJob {
    status: tokio::sync::watch::Sender<DeviceInstallStatus>,
    cancel: tokio::sync::watch::Sender<bool>,
    /// Its place in the line (`AcquisitionCoordinator::queue_turn`).
    place: PathBuf,
    /// Its receive store folder (`receive_path`).
    receive: PathBuf,
    /// When it ended. An ended job is pruned `ENDED_JOB_KEEP` later.
    ended: Option<std::time::Instant>,
}

/// How long the status of an ended install from devices stays readable.
const ENDED_JOB_KEEP: Duration = Duration::from_secs(10 * 60);

/// A test fault in publication, after the files left the receive store.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublishFault {
    /// Publication fails as a failed identification does.
    Fail,
    /// korrid stops: the work folder goes (startup deletes `.tmp-*`) and
    /// nothing else runs.
    Crash,
}

/// One piece that still has to arrive.
#[derive(Clone, Debug)]
struct Want {
    file: usize,
    offset: u64,
    length: u64,
}

/// Where the verified pieces of one install from devices wait, beside the
/// holder store: `<storage root>/cache/.blobs/installs/<hex of the release
/// key>/<one store per try>`. Derived data, like the holder store; it goes when the install
/// ends, and a store whose bytes cannot be read goes and starts again.
pub(crate) fn receive_path(root: &Path, hex: &str) -> PathBuf {
    store_path(root).join(RECEIVE_FOLDER).join(hex)
}

fn cancelled_failure() -> RpcFailure {
    failure("InstallCancelled", "The install was cancelled.")
}

/// Resolves when the person or the line cancels the install. A channel
/// that closed is not a cancel: it never resolves then.
async fn cancel_asked(
    cancelled: &mut tokio::sync::watch::Receiver<bool>,
    stop: &mut tokio::sync::watch::Receiver<super::Stop>,
) {
    let person = async { cancelled.wait_for(|cancelled| *cancelled).await.is_ok() };
    let line = async {
        stop.wait_for(|stop| *stop == super::Stop::Cancel)
            .await
            .is_ok()
    };
    tokio::select! {
        true = person => (),
        true = line => (),
        else => std::future::pending().await,
    }
}

/// Forget imported and cancelled installs `ENDED_JOB_KEEP` after they
/// ended. A failed install stays until it is cancelled or a new install of
/// its release starts (`start`), so the person can see it and throw it away.
fn prune(jobs: &mut BTreeMap<String, DeviceJob>, now: std::time::Instant) {
    jobs.retain(|_, job| {
        matches!(job.status.borrow().state, DeviceInstallState::Failed { .. })
            || job
                .ended
                .is_none_or(|ended| now.saturating_duration_since(ended) < ENDED_JOB_KEEP)
    });
}

async fn drop_store(store: FsStore, path: &Path) {
    let _ = store.shutdown().await;
    let release = path.parent().map(Path::to_path_buf);
    match std::fs::remove_dir_all(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            eprintln!("korrid install sources: a receive store stayed: {error}")
        }
        _ => (),
    }
    // The release's folder goes with its last store.
    if let Some(release) = release {
        let _ = std::fs::remove_dir(release);
    }
}

impl Pieces {
    /// Receiver: install the file release `key` from these holders at once,
    /// as one install in the line (ADR 0092), and wait for its end. Returns
    /// the library game id.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn install<H: Holder + 'static>(
        &self,
        coordinator: &super::AcquisitionCoordinator,
        context: &GameContext,
        key: &str,
        title: &str,
        holders: Vec<Arc<H>>,
        place: Place,
    ) -> Result<String, RpcFailure> {
        let job = self.start(coordinator, context, key, title, holders, place)?;
        self.wait(&job).await
    }

    /// Start an install from devices, or join the one of the same release
    /// that is already in the line: both callers get its job id.
    pub(crate) fn start<H: Holder + 'static>(
        &self,
        coordinator: &super::AcquisitionCoordinator,
        context: &GameContext,
        key: &str,
        title: &str,
        holders: Vec<Arc<H>>,
        place: Place,
    ) -> Result<String, RpcFailure> {
        let hex = placement::ready_name(key).ok_or_else(install_failed)?;
        let cache = placement::create_base(&context.root, placement::install_cache(&context.root))
            .map_err(|error| error.rpc())?;
        let mut jobs = self.jobs.lock().expect("device installs mutex poisoned");
        prune(&mut jobs, std::time::Instant::now());
        if let Some((id, job)) = jobs.iter().find(|(_, job)| {
            let status = job.status.borrow();
            status.release_key == key && status.state.live()
        }) {
            // A caller that joins at the front moves the job there.
            if place == Place::Front {
                coordinator.to_front(&job.place);
            }
            return Ok(id.clone());
        }
        // A new install of the release replaces its failed ones.
        jobs.retain(|_, job| {
            let status = job.status.borrow();
            status.release_key != key || !matches!(status.state, DeviceInstallState::Failed { .. })
        });
        let id = format!("{:032x}", rand::random::<u128>());
        // ADR 0092: the line shows it under its job id; a restart forgets it.
        let wait = coordinator.queue_turn(
            cache.join(hex),
            place,
            super::Listed::Shown(super::Member::Job(id.clone())),
        );
        let (status, _) = tokio::sync::watch::channel(DeviceInstallStatus {
            job_id: id.clone(),
            release_key: key.to_owned(),
            state: DeviceInstallState::Waiting,
            yielding: None,
        });
        let (cancel, cancelled) = tokio::sync::watch::channel(false);
        jobs.insert(
            id.clone(),
            DeviceJob {
                status: status.clone(),
                cancel,
                place: wait.key.clone(),
                receive: receive_path(&context.root, hex),
                ended: None,
            },
        );
        drop(jobs);
        let (pieces, context) = (self.clone(), context.clone());
        let (key, title, job) = (key.to_owned(), title.to_owned(), id.clone());
        tokio::spawn(async move {
            let set = |state| status.send_modify(|status| status.state = state);
            let yielding = |pace| status.send_modify(|status| status.yielding = pace);
            let result = pieces
                .run(
                    &context, &cache, &key, &title, holders, wait, cancelled, &set, &yielding,
                )
                .await;
            let mut jobs = pieces.jobs.lock().expect("device installs mutex poisoned");
            yielding(None);
            set(match result {
                Ok(game_id) => DeviceInstallState::Imported { game_id },
                Err(reason) if reason.code == "InstallCancelled" => DeviceInstallState::Cancelled,
                Err(reason) => DeviceInstallState::Failed { reason },
            });
            if let Some(job) = jobs.get_mut(&job) {
                job.ended = Some(std::time::Instant::now());
            }
        });
        Ok(id)
    }

    /// The status of one install from devices.
    pub(crate) fn status(&self, job: &str) -> Option<DeviceInstallStatus> {
        let jobs = self.jobs.lock().expect("device installs mutex poisoned");
        jobs.get(job).map(|job| job.status.borrow().clone())
    }

    /// The install of `key` that waits or runs, if any.
    pub(crate) fn live_job(&self, key: &str) -> Option<String> {
        let jobs = self.jobs.lock().expect("device installs mutex poisoned");
        jobs.iter()
            .find(|(_, job)| {
                let status = job.status.borrow();
                status.release_key == key && status.state.live()
            })
            .map(|(id, _)| id.clone())
    }

    /// Every change of one install's status, from now on.
    pub(crate) fn watch(
        &self,
        job: &str,
    ) -> Option<tokio::sync::watch::Receiver<DeviceInstallStatus>> {
        let jobs = self.jobs.lock().expect("device installs mutex poisoned");
        jobs.get(job).map(|job| job.status.subscribe())
    }

    /// Wait for the end of one install from devices.
    pub(crate) async fn wait(&self, job: &str) -> Result<String, RpcFailure> {
        let mut status = self
            .jobs
            .lock()
            .expect("device installs mutex poisoned")
            .get(job)
            .map(|job| job.status.subscribe())
            .ok_or_else(install_failed)?;
        loop {
            let state = status.borrow_and_update().state.clone();
            match state {
                DeviceInstallState::Imported { game_id } => return Ok(game_id),
                DeviceInstallState::Failed { reason } => return Err(reason),
                DeviceInstallState::Cancelled => return Err(cancelled_failure()),
                _ => (),
            }
            if status.changed().await.is_err() {
                return Err(install_failed());
            }
        }
    }

    /// Cancel one install from devices, waiting, running or failed, and
    /// wait for it to end. Its verified pieces go. A failed install's pieces
    /// stay while another install of the same release runs, which uses them.
    pub(crate) async fn cancel(&self, job: &str) -> Option<DeviceInstallStatus> {
        {
            let jobs = self.jobs.lock().expect("device installs mutex poisoned");
            if let Some(found) = jobs.get(job) {
                let _ = found.cancel.send(true);
                let (failed, key) = {
                    let status = found.status.borrow();
                    let failed = matches!(status.state, DeviceInstallState::Failed { .. });
                    (failed, status.release_key.clone())
                };
                let in_use = jobs.values().any(|other| {
                    let status = other.status.borrow();
                    status.release_key == key && status.state.live()
                });
                // The jobs lock keeps a new install of the release from
                // opening the folder while it goes.
                if failed && !in_use {
                    match std::fs::remove_dir_all(&found.receive) {
                        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                            eprintln!("korrid install sources: a receive store stayed: {error}")
                        }
                        _ => (),
                    }
                    found
                        .status
                        .send_modify(|status| status.state = DeviceInstallState::Cancelled);
                }
            }
        }
        let _ = self.wait(job).await;
        self.status(job)
    }

    /// Forget the imported and cancelled installs that ended
    /// `ENDED_JOB_KEEP` before `now`.
    #[cfg(test)]
    pub(super) fn prune(&self, now: std::time::Instant) {
        prune(
            &mut self.jobs.lock().expect("device installs mutex poisoned"),
            now,
        );
    }

    #[allow(clippy::too_many_arguments)]
    async fn run<H: Holder + 'static>(
        &self,
        context: &GameContext,
        cache: &Path,
        key: &str,
        title: &str,
        holders: Vec<Arc<H>>,
        mut wait: super::TurnWait,
        mut cancelled: tokio::sync::watch::Receiver<bool>,
        set: &(dyn Fn(DeviceInstallState) + Send + Sync),
        yielding: &(dyn Fn(Option<super::InstallYield>) + Send + Sync),
    ) -> Result<String, RpcFailure> {
        let hex = placement::ready_name(key).ok_or_else(install_failed)?;
        let mut turn = tokio::select! {
            biased;
            true = async { cancelled.wait_for(|cancelled| *cancelled).await.is_ok() } => {
                wait.end();
                return Err(cancelled_failure());
            }
            turn = wait.turn() => turn,
        };
        let result = self
            .transfer(
                context,
                cache,
                hex,
                key,
                title,
                holders,
                &mut cancelled,
                &mut turn,
                set,
                yielding,
            )
            .await;
        wait.end();
        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn transfer<H: Holder + 'static>(
        &self,
        context: &GameContext,
        cache: &Path,
        hex: &str,
        key: &str,
        title: &str,
        holders: Vec<Arc<H>>,
        cancelled: &mut tokio::sync::watch::Receiver<bool>,
        turn: &mut super::Turn,
        set: &(dyn Fn(DeviceInstallState) + Send + Sync),
        yielding: &(dyn Fn(Option<super::InstallYield>) + Send + Sync),
    ) -> Result<String, RpcFailure> {
        // Every holder is asked; the first answer names the release's files,
        // and every holder whose files have the same contents serves.
        let asked = futures::future::join_all(holders.iter().map(|holder| async move {
            tokio::time::timeout(PIECE_TIMEOUT, holder.manifest(key))
                .await
                .ok()
                .and_then(Result::ok)
        }));
        let answers = tokio::select! {
            biased;
            () = cancel_asked(cancelled, &mut turn.stop) => return Err(cancelled_failure()),
            answers = asked => answers,
        };
        let manifest = answers.iter().flatten().next().cloned().ok_or_else(|| {
            failure(
                "InstallSourceUnavailable",
                "No device that holds this release answered.",
            )
        })?;
        if manifest.files.is_empty() || !manifest.files.iter().all(|file| safe_name(&file.name)) {
            return Err(install_failed());
        }
        let serving: Vec<Arc<H>> = holders
            .into_iter()
            .zip(&answers)
            .filter(|(_, answer)| {
                answer
                    .as_ref()
                    .is_some_and(|answer| answer.contents() == manifest.contents())
            })
            .map(|(holder, _)| holder)
            .collect();
        let hashes: Vec<Hash> = manifest
            .files
            .iter()
            .map(|file| file.blake3.parse().map_err(|_| install_failed()))
            .collect::<Result<_, _>>()?;
        let receive = receive_path(&context.root, hex);
        let mut fresh = false;
        let (store, store_dir, mut wants, done) = loop {
            // One store folder per try: a store that was shut down can still
            // finish deletes under its own paths, so a new store never
            // reuses them. The newest kept folder resumes; after a drop, a
            // fresh folder starts, so a folder that will not go is never
            // picked twice.
            let newest = if fresh {
                None
            } else {
                std::fs::read_dir(&receive).ok()
            };
            let store_dir = newest
                .and_then(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .filter(|entry| entry.path().is_dir())
                        .max_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok())
                        .map(|entry| entry.path())
                })
                .unwrap_or_else(|| receive.join(format!("{:016x}", rand::random::<u64>())));
            std::fs::create_dir_all(&store_dir).map_err(|_| install_failed())?;
            let store = FsStore::load(&store_dir)
                .await
                .map_err(|_| install_failed())?;
            match kept_pieces(&store, &manifest, &hashes).await {
                Some((wants, done)) => break (store, store_dir, wants, done),
                // N1: the store says it holds bytes it cannot read (a
                // restart during publication moved them away). Start again.
                None if fresh => {
                    drop_store(store, &store_dir).await;
                    return Err(install_failed());
                }
                None => {
                    eprintln!("korrid install sources: a receive store held unreadable pieces; it starts again");
                    drop_store(store, &store_dir).await;
                    fresh = true;
                }
            }
        };
        let total = manifest.bytes();
        let floor = Floor::new(cache, None).map_err(|error| error.rpc())?;
        if floor.room().map_err(|error| error.rpc())? < total - done {
            return Err(Failure::NoSpace.rpc());
        }
        set(DeviceInstallState::Installing {
            bytes_done: done,
            bytes_total: total,
        });
        let shared = Arc::new(Mutex::new(Shared {
            wants: std::mem::take(&mut wants),
            in_flight: 0,
            done,
            progress: false,
            serving: serving.len(),
            busy: 0,
            pace: None,
        }));
        let workers = serving.into_iter().map(|holder| {
            let (shared, store, manifest, hashes) = (
                shared.clone(),
                store.clone(),
                manifest.clone(),
                hashes.clone(),
            );
            let key = key.to_owned();
            let gate = self.gate.clone();
            async move {
                let mut strikes = 0;
                let leave = |shared: &mut Shared| shared.serving -= 1;
                loop {
                    // #556: installs yield to play on this device.
                    gate.take(PIECE_BYTES, true, &mut |pace| {
                        shared.lock().expect("install pieces mutex poisoned").pace = pace;
                    })
                    .await;
                    let next = {
                        let mut shared = shared.lock().expect("install pieces mutex poisoned");
                        match shared.wants.pop_front() {
                            Some(want) => {
                                shared.in_flight += 1;
                                Some(want)
                            }
                            None if shared.in_flight == 0 => {
                                leave(&mut shared);
                                return;
                            }
                            None => None,
                        }
                    };
                    let Some(want) = next else {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        continue;
                    };
                    let file = &manifest.files[want.file];
                    let fetched = tokio::time::timeout(
                        PIECE_TIMEOUT,
                        holder.piece(InstallPieceRequest {
                            release_key: key.clone(),
                            blake3: file.blake3.clone(),
                            offset: want.offset,
                            length: want.length,
                        }),
                    )
                    .await;
                    if matches!(&fetched, Ok(Err(failure)) if failure.code == super::play::busy().code)
                    {
                        // A busy source (it plays, or its battery is low) is
                        // no failure: another source takes the piece, and
                        // this one is asked again later.
                        {
                            let mut shared =
                                shared.lock().expect("install pieces mutex poisoned");
                            shared.in_flight -= 1;
                            shared.wants.push_front(want);
                            shared.busy += 1;
                        }
                        tokio::time::sleep(super::play::BUSY_REST).await;
                        shared.lock().expect("install pieces mutex poisoned").busy -= 1;
                        continue;
                    }
                    let checked = match fetched {
                        Ok(Ok(encoded)) => {
                            // The BAO stream starts with the file's size.
                            let size_matches = encoded
                                .get(..8)
                                .is_some_and(|size| size == file.bytes.to_le_bytes());
                            size_matches
                                && store
                                    .import_bao_bytes(
                                        hashes[want.file],
                                        chunk_ranges(want.offset, want.length),
                                        encoded,
                                    )
                                    .await
                                    .is_ok()
                        }
                        _ => false,
                    };
                    let mut shared = shared.lock().expect("install pieces mutex poisoned");
                    shared.in_flight -= 1;
                    if checked {
                        shared.done += want.length;
                        shared.progress = true;
                    } else {
                        // Another holder takes the piece.
                        shared.wants.push_front(want);
                        strikes += 1;
                        if strikes >= HOLDER_STRIKES {
                            eprintln!(
                                "korrid install sources: {} left the install",
                                holder.label()
                            );
                            leave(&mut shared);
                            return;
                        }
                    }
                }
            }
        });
        let report = {
            let shared = shared.clone();
            async move {
                let mut reported = None;
                loop {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let (done, pace) = {
                        let mut shared = shared.lock().expect("install pieces mutex poisoned");
                        (
                            std::mem::take(&mut shared.progress).then_some(shared.done),
                            shared.yielding(),
                        )
                    };
                    if pace != reported {
                        reported = pace;
                        yielding(pace);
                    }
                    if let Some(done) = done {
                        set(DeviceInstallState::Installing {
                            bytes_done: done,
                            bytes_total: total,
                        });
                    }
                }
            }
        };
        // Cancel, from the person or from the line, drops every worker and
        // so every piece fetch in flight.
        let stopped = cancel_asked(cancelled, &mut turn.stop);
        let stopped = tokio::select! {
            _ = futures::future::join_all(workers) => false,
            () = stopped => true,
            () = report => unreachable!("the report loop never ends"),
        };
        if stopped {
            drop_store(store, &store_dir).await;
            return Err(cancelled_failure());
        }
        let (finished, done) = {
            let shared = shared.lock().expect("install pieces mutex poisoned");
            (shared.wants.is_empty() && shared.done == total, shared.done)
        };
        set(DeviceInstallState::Installing {
            bytes_done: done,
            bytes_total: total,
        });
        if !finished {
            // Verified pieces stay in the receive store for the next try.
            let _ = store.shutdown().await;
            return Err(failure(
                "InstallSourceUnavailable",
                "The devices that hold this release stopped answering.",
            ));
        }
        // Cancel is heard while the files move; once they are checked and
        // published, the install ends as it is.
        // A game that runs here keeps the disk and the processor: the files
        // move and are checked after it (#556).
        let published = tokio::select! {
            biased;
            () = cancel_asked(cancelled, &mut turn.stop) => Err(cancelled_failure()),
            work = async {
                self.gate.take(0, true, &mut |pace| yielding(pace)).await;
                yielding(None);
                self.export(&store, cache, &manifest, &hashes).await
            } => work,
        };
        let published = match published {
            Ok(work) => self.record(context, work, key, title).await,
            Err(failure) => Err(failure),
        };
        #[cfg(test)]
        if matches!(published, Err(ref failure) if failure.code == "TestCrash") {
            // A crash leaves the store as it is.
            let _ = store.shutdown().await;
            return published;
        }
        // N1: whatever the end, the store's bytes have left it (they moved
        // into the work folder); a store that names them must not stay.
        drop_store(store, &store_dir).await;
        published
    }

    /// Move the verified files into a work folder.
    async fn export(
        &self,
        store: &FsStore,
        cache: &Path,
        manifest: &InstallManifest,
        hashes: &[Hash],
    ) -> Result<placement::WorkFolder, RpcFailure> {
        let work = placement::WorkFolder::create(cache).map_err(|error| error.rpc())?;
        for (file, hash) in manifest.files.iter().zip(hashes) {
            let target = work.path.join(&file.name);
            if let Some(parent) = target.parent() {
                placement::create_base(&work.path, parent.to_path_buf())
                    .map_err(|error| error.rpc())?;
            }
            // The store owns the verified bytes; they move, they are not
            // copied. iroh-blobs copies only across file systems.
            store
                .export_with_opts(ExportOptions {
                    hash: *hash,
                    mode: ExportMode::TryReference,
                    target: target.clone(),
                })
                .finish()
                .await
                .map_err(|_| install_failed())?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644))
                .map_err(|_| install_failed())?;
        }
        Ok(work)
    }

    /// Check the release key of the work folder, publish and record it.
    async fn record(
        &self,
        context: &GameContext,
        work: placement::WorkFolder,
        key: &str,
        title: &str,
    ) -> Result<String, RpcFailure> {
        #[cfg(test)]
        if let Some(fault) = *self.fault.lock().expect("fault mutex poisoned") {
            drop(work);
            return Err(match fault {
                PublishFault::Fail => install_failed(),
                PublishFault::Crash => failure("TestCrash", "korrid stopped."),
            });
        }
        let (context, key, title) = (context.clone(), key.to_owned(), title.to_owned());
        tokio::task::spawn_blocking(move || {
            let (system, identity) = context
                .registry
                .systems()
                .values()
                .find_map(|system| {
                    let rules = context.registry.content_rules(&system.id)?;
                    let identity = super::game::identify_work(&work.path, rules).ok()?;
                    (identity.value() == key).then_some((system.id.clone(), identity))
                })
                .ok_or_else(|| {
                    failure(
                        "InstallFailed",
                        "The installed files are not the release that was asked for.",
                    )
                })?;
            let installs = placement::install_cache(&context.root);
            placement::publish_acquired_game(&installs, work, &key).map_err(|error| error.rpc())?;
            context
                .discovery
                .record_copied_release(&title, &system, &identity)
                .map_err(|_| Failure::Storage.rpc())
        })
        .await
        .map_err(|_| install_failed())?
    }

    #[cfg(test)]
    pub(crate) fn fault(&self, fault: Option<PublishFault>) {
        *self.fault.lock().expect("fault mutex poisoned") = fault;
    }
}

/// The pieces that a receive store already holds and can read, and the
/// pieces still wanted. `None` when the store reports bytes that cannot be
/// read.
async fn kept_pieces(
    store: &FsStore,
    manifest: &InstallManifest,
    hashes: &[Hash],
) -> Option<(VecDeque<Want>, u64)> {
    use futures::StreamExt;
    let mut wants = VecDeque::new();
    let mut done = 0;
    for (index, (file, hash)) in manifest.files.iter().zip(hashes).enumerate() {
        let have = match store.observe(*hash).stream().await {
            Ok(mut stream) => stream.next().await.map(|bitfield| bitfield.ranges),
            Err(_) => return None,
        };
        let mut offset = 0;
        let mut read_checked = false;
        while offset < file.bytes {
            let length = PIECE_BYTES.min(file.bytes - offset);
            let ranges = chunk_ranges(offset, length);
            if have.as_ref().is_some_and(|have| ranges.is_subset(have)) {
                // A store that knows a file must be able to read what it
                // holds: after a stop during publication it reports bytes
                // whose file is gone. The first kept piece is read.
                if !read_checked {
                    store.export_bao(*hash, ranges).bao_to_vec().await.ok()?;
                    read_checked = true;
                }
                done += length;
            } else {
                wants.push_back(Want {
                    file: index,
                    offset,
                    length,
                });
            }
            offset += length;
        }
        // A file the store calls complete whose pieces it cannot show.
        if !read_checked && matches!(store.status(*hash).await.ok()?, BlobStatus::Complete { .. }) {
            return None;
        }
    }
    Some((wants, done))
}

struct Shared {
    wants: VecDeque<Want>,
    in_flight: usize,
    done: u64,
    /// A piece arrived since the last report.
    progress: bool,
    /// Sources still in the install, and those that rest after "busy".
    serving: usize,
    busy: usize,
    /// How installs on this device yield to play now.
    pace: Option<super::InstallYield>,
}

impl Shared {
    fn yielding(&self) -> Option<super::InstallYield> {
        use super::{InstallYield, YieldPace, YieldReason};
        self.pace.or_else(|| {
            (self.serving > 0 && self.busy == self.serving).then_some(InstallYield {
                pace: YieldPace::Paused,
                reason: YieldReason::SourcesBusy,
            })
        })
    }
}

/// A federation device as an install source, over signed peer HTTP on its
/// iroh route, with this device's endpoint proof, as save transfers do.
pub(crate) struct PeerHolder {
    pub route: PeerRoute,
    pub credentials: crate::peer_rpc::PeerCredentials,
    pub device: String,
}

/// How a receiver reaches a holder.
pub(crate) enum PeerRoute {
    /// The federation's iroh route to the device.
    Federation(crate::federation::FederationDirectory),
    /// One fixed loopback origin, for korrids in one test process.
    #[cfg(test)]
    Origin(String),
}

impl PeerHolder {
    async fn post(
        &self,
        path: &str,
        request: crate::RpcRequest,
    ) -> Result<(crate::peer_rpc::EncodedPeerRequest, Vec<u8>), RpcFailure> {
        let unreachable = || {
            failure(
                "InstallSourceUnavailable",
                "A device that holds this release is not reachable.",
            )
        };
        let encoded = self
            .credentials
            .encode_request(&self.device, request, crate::peer_rpc::unix_time())
            .map_err(|_| unreachable())?;
        let mut response = match &self.route {
            PeerRoute::Federation(directory) => {
                let connection = directory
                    .reach_connection(&self.device)
                    .await
                    .map_err(|_| unreachable())?;
                let proof = directory
                    .own_endpoint_event(&self.device)
                    .map_err(|_| unreachable())?;
                crate::federation::reach::http::post(
                    &connection,
                    path,
                    encoded.event_json.clone(),
                    None,
                    Some(&proof),
                )
                .await
                .map_err(|_| unreachable())?
            }
            #[cfg(test)]
            PeerRoute::Origin(origin) => crate::federation::reach::http::Response::Tcp(
                reqwest::Client::new()
                    .post(format!("{origin}{path}"))
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(encoded.event_json.clone())
                    .send()
                    .await
                    .map_err(|_| unreachable())?,
            ),
        };
        if response.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(refused());
        }
        if response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE {
            return Err(super::play::busy());
        }
        if !response.status().is_success() {
            return Err(unreachable());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unreachable())? {
            // A piece is one BAO encoding of at most `PIECE_BYTES`.
            if body.len() + chunk.len()
                > 2 * PIECE_BYTES as usize + crate::peer_rpc::MAX_PEER_EVENT_BYTES
            {
                return Err(unreachable());
            }
            body.extend_from_slice(&chunk);
        }
        Ok((encoded, body))
    }
}

impl Holder for PeerHolder {
    fn label(&self) -> String {
        self.device.chars().take(8).collect()
    }

    async fn manifest(&self, key: &str) -> Result<InstallManifest, RpcFailure> {
        let (encoded, body) = self
            .post(
                "/peer-rpc",
                crate::RpcRequest::InstallManifest(InstallManifestRequest {
                    release_key: key.into(),
                }),
            )
            .await?;
        let response = self
            .credentials
            .decode_response(
                &self.device,
                &encoded,
                std::str::from_utf8(&body).map_err(|_| install_failed())?,
                crate::peer_rpc::unix_time(),
            )
            .map_err(|_| install_failed())?;
        match response {
            crate::RpcResponse::InstallManifest(InstallManifestOutcome::Ok(manifest)) => {
                Ok(manifest)
            }
            crate::RpcResponse::InstallManifest(InstallManifestOutcome::Err(failure)) => {
                Err(failure)
            }
            _ => Err(install_failed()),
        }
    }

    async fn piece(&self, request: InstallPieceRequest) -> Result<Vec<u8>, RpcFailure> {
        self.post(
            "/peer-installs-piece",
            crate::RpcRequest::InstallPiece(request),
        )
        .await
        .map(|(_, body)| body)
    }
}

/// `app.installs.manifest` on `/peer-rpc`. The peer policy already refused a
/// guest (`PeerPolicy::OwnerOrHouseholdDevice`).
pub(crate) async fn manifest_rpc(
    state: &crate::AppState,
    request: &InstallManifestRequest,
) -> InstallManifestOutcome {
    let context = match state.game_context() {
        Ok(context) => context,
        Err(_) => return InstallManifestOutcome::Err(not_held()),
    };
    match state
        .acquisition
        .pieces()
        .manifest(&context, &request.release_key)
        .await
    {
        Ok(manifest) => InstallManifestOutcome::Ok(manifest),
        Err(failure) => InstallManifestOutcome::Err(failure),
    }
}

/// `app.installs.piece` on `/peer-installs-piece`: the BAO bytes, or 403 for
/// a caller that may not install from this device.
pub(crate) async fn piece_response(
    state: &crate::AppState,
    context: &AuthorizationContext,
    request: &InstallPieceRequest,
) -> axum::response::Response {
    use axum::{http::StatusCode, response::IntoResponse};
    if !may_install(context) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(games) = state.game_context() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // #556: a device that plays serves slowly; one on low battery serves
    // nothing, and the receiver asks another source.
    let pieces = state.acquisition.pieces();
    let device = match context {
        AuthorizationContext::Peer(
            Principal::OwnerDevice {
                device_public_key, ..
            }
            | Principal::Household {
                device_public_key, ..
            },
        ) => Some(device_public_key.as_str()),
        _ => None,
    };
    let requester = device.map(|device| (device, request.release_key.as_str()));
    if !pieces
        .gate
        .serve(request.length.min(PIECE_BYTES), requester)
        .await
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(axum::http::header::RETRY_AFTER, "5")],
        )
            .into_response();
    }
    match pieces.piece(&games, request).await {
        Ok(encoded) => (
            [
                (axum::http::header::CONTENT_TYPE, "application/octet-stream"),
                (axum::http::header::CACHE_CONTROL, "no-store"),
            ],
            encoded,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
