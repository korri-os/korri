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
//! The holder side runs today. The receiver side (`Pieces::install`,
//! `PeerHolder`) waits for its first caller, play-here (#553).
#![cfg_attr(not(test), allow(dead_code))]
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
pub(crate) const PIECE_BYTES: u64 = 1 << 20;
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
}

pub(crate) fn store_path(root: &Path) -> PathBuf {
    placement::copied_release_cache(root).join(".blobs")
}

impl Pieces {
    /// The store, opened once. A store that cannot be opened is rebuilt.
    async fn store(&self, root: &Path) -> Result<FsStore, RpcFailure> {
        let mut stores = self.stores.lock().await;
        if let Some(store) = stores.get(root) {
            return Ok(store.clone());
        }
        let path = store_path(root);
        let cache = placement::create_base(root, placement::copied_release_cache(root))
            .map_err(|error| error.rpc())?;
        debug_assert_eq!(path.parent(), Some(cache.as_path()));
        let store = match FsStore::load(&path).await {
            Ok(store) => store,
            Err(error) => {
                eprintln!("korrid install sources: the piece store is rebuilt: {error}");
                let _ = std::fs::remove_dir_all(&path);
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
            tokio::task::spawn_blocking(move || release_files(&context, &key))
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
/// library location of the release key that is not missing.
fn release_files(context: &GameContext, key: &str) -> Result<Vec<(String, PathBuf)>, RpcFailure> {
    let ready = placement::copied_release_cache(&context.root)
        .join(placement::ready_name(key).ok_or_else(not_held)?);
    if placement::is_ready(&ready) {
        let mut files = Vec::new();
        walk(&ready, &ready, &mut files)?;
        files.sort();
        if !files.is_empty() {
            return Ok(files);
        }
    }
    let snapshot = context.discovery.published().map_err(|_| not_held())?;
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
                    &context.root,
                    &snapshot,
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

/// What an install from devices reports while it runs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Progress {
    pub done: u64,
    pub total: u64,
}

/// One piece that still has to arrive.
#[derive(Clone, Debug)]
struct Want {
    file: usize,
    offset: u64,
    length: u64,
}

impl Pieces {
    /// Receiver: install the file release `key` from these holders at once,
    /// as one install in the line (ADR 0092). Returns the library game id.
    pub(crate) async fn install<H: Holder + 'static>(
        &self,
        coordinator: &super::AcquisitionCoordinator,
        context: &GameContext,
        key: &str,
        title: &str,
        holders: Vec<Arc<H>>,
        place: Place,
        progress: impl Fn(Progress) + Send + Sync + 'static,
    ) -> Result<String, RpcFailure> {
        let hex = placement::ready_name(key).ok_or_else(install_failed)?;
        let cache = placement::create_base(
            &context.root,
            placement::copied_release_cache(&context.root),
        )
        .map_err(|error| error.rpc())?;
        let _turn = coordinator
            .queue_turn(cache.join(hex), place, None)
            .turn()
            .await;
        // Every holder is asked; the first answer names the release's files,
        // and every holder whose files have the same contents serves.
        let answers = futures::future::join_all(holders.iter().map(|holder| async move {
            tokio::time::timeout(PIECE_TIMEOUT, holder.manifest(key))
                .await
                .ok()
                .and_then(Result::ok)
        }))
        .await;
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
        let store = self.store(&context.root).await?;
        let total = manifest.bytes();
        let floor = Floor::new(&cache, None).map_err(|error| error.rpc())?;
        // Verified pieces of an earlier try are kept in the store.
        let mut wants = VecDeque::new();
        let mut done = 0;
        for (index, (file, hash)) in manifest.files.iter().zip(&hashes).enumerate() {
            let have = match store.observe(*hash).stream().await {
                Ok(mut stream) => {
                    use futures::StreamExt;
                    stream.next().await.map(|bitfield| bitfield.ranges)
                }
                Err(_) => None,
            };
            let mut offset = 0;
            while offset < file.bytes {
                let length = PIECE_BYTES.min(file.bytes - offset);
                if have
                    .as_ref()
                    .is_some_and(|have| chunk_ranges(offset, length).is_subset(have))
                {
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
        }
        if floor.room().map_err(|error| error.rpc())? < total - done {
            return Err(Failure::NoSpace.rpc());
        }
        progress(Progress { done, total });
        let shared = Arc::new(Mutex::new(Shared {
            wants,
            in_flight: 0,
            done,
        }));
        let progress = Arc::new(progress);
        let workers = serving.into_iter().map(|holder| {
            let (shared, store, manifest, hashes) = (
                shared.clone(),
                store.clone(),
                manifest.clone(),
                hashes.clone(),
            );
            let progress = progress.clone();
            let key = key.to_owned();
            async move {
                let mut strikes = 0;
                loop {
                    let want = {
                        let mut shared = shared.lock().expect("install pieces mutex poisoned");
                        match shared.wants.pop_front() {
                            Some(want) => {
                                shared.in_flight += 1;
                                want
                            }
                            None if shared.in_flight == 0 => return,
                            None => {
                                drop(shared);
                                tokio::time::sleep(Duration::from_millis(20)).await;
                                continue;
                            }
                        }
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
                        progress(Progress {
                            done: shared.done,
                            total,
                        });
                    } else {
                        // Another holder takes the piece.
                        shared.wants.push_front(want);
                        strikes += 1;
                        if strikes >= HOLDER_STRIKES {
                            eprintln!(
                                "korrid install sources: {} left the install",
                                holder.label()
                            );
                            return;
                        }
                    }
                }
            }
        });
        futures::future::join_all(workers).await;
        let finished = {
            let shared = shared.lock().expect("install pieces mutex poisoned");
            shared.wants.is_empty() && shared.done == total
        };
        if !finished {
            // Verified pieces stay in the store for the next try.
            return Err(failure(
                "InstallSourceUnavailable",
                "The devices that hold this release stopped answering.",
            ));
        }
        self.publish(context, &store, &cache, key, title, &manifest, &hashes)
            .await
    }

    /// Move the verified files into a work folder, check the release key,
    /// publish and record (as a netplay copy does, ADR 0040).
    #[allow(clippy::too_many_arguments)]
    async fn publish(
        &self,
        context: &GameContext,
        store: &FsStore,
        cache: &Path,
        key: &str,
        title: &str,
        manifest: &InstallManifest,
        hashes: &[Hash],
    ) -> Result<String, RpcFailure> {
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
            let installs = placement::copied_release_cache(&context.root);
            placement::publish_acquired_game(&installs, work, &key).map_err(|error| error.rpc())?;
            context
                .discovery
                .record_copied_release(&title, &system, &identity)
                .map_err(|_| Failure::Storage.rpc())
        })
        .await
        .map_err(|_| install_failed())?
    }
}

struct Shared {
    wants: VecDeque<Want>,
    in_flight: usize,
    done: u64,
}

/// A federation device as an install source, over signed peer HTTP: its
/// advertised HTTP origins first, then its iroh route, as save transfers do.
pub(crate) struct PeerHolder {
    pub route: PeerRoute,
    pub credentials: crate::peer_rpc::PeerCredentials,
    pub http: reqwest::Client,
    pub device: String,
}

/// How a receiver reaches a holder.
pub(crate) enum PeerRoute {
    /// The federation: the device's advertised origins, then its iroh route.
    #[cfg_attr(test, allow(dead_code))] // Tests use `Origin`.
    Federation(crate::federation::FederationDirectory),
    /// One fixed loopback origin, for two korrids in one test process.
    #[cfg(test)]
    Origin(String),
}

impl PeerHolder {
    fn origins(&self) -> Vec<String> {
        let directory = match &self.route {
            PeerRoute::Federation(directory) => directory,
            #[cfg(test)]
            PeerRoute::Origin(origin) => return vec![origin.clone()],
        };
        let mut candidates = Vec::new();
        if let Some(peer) = directory.snapshot().ok().and_then(|peers| {
            peers
                .into_iter()
                .find(|peer| peer.device_public_key == self.device)
        }) {
            if let Some(endpoint) = peer.current_endpoint.or(peer.remembered_endpoint) {
                candidates.extend(
                    endpoint
                        .candidates
                        .iter()
                        .filter_map(|origin| crate::federation::peer_origin(origin).ok()),
                );
            }
        }
        crate::federation::reach::nearby_seam::prepend(&self.device, &mut candidates);
        candidates
    }

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
        let origins = self.origins();
        let mut response = match crate::federation::connect::select_origin(&origins).await {
            Ok(index) => crate::federation::reach::http::Response::Tcp(
                self.http
                    .post(format!("{}{path}", origins[index]))
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(encoded.event_json.clone())
                    .send()
                    .await
                    .map_err(|_| unreachable())?,
            ),
            Err(_) => {
                let directory = match &self.route {
                    PeerRoute::Federation(directory) => directory,
                    #[cfg(test)]
                    PeerRoute::Origin(_) => return Err(unreachable()),
                };
                let connection = directory
                    .reach_connection(&self.device)
                    .await
                    .map_err(|_| unreachable())?;
                crate::federation::reach::http::post(
                    &connection,
                    path,
                    encoded.event_json.clone(),
                    None,
                )
                .await
                .map_err(|_| unreachable())?
            }
        };
        if response.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(refused());
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
    match state.acquisition.pieces().piece(&games, request).await {
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
