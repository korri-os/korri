//! ADR 0092 (#552): two holder korrids and one receiver korrid in one
//! process. The holders serve over their real signed peer HTTP router.
use super::{
    game::GameContext,
    pieces::{
        may_install, InstallManifestRequest, InstallPieceRequest, PeerHolder, PeerRoute,
        PIECE_BYTES,
    },
    placement, sleep, AcquisitionCoordinator, Place,
};
use crate::{
    authorization::{self, AuthorizationContext, PersonPass, Principal, Scope},
    peer_rpc::{self, PeerCredentials, PeerRpcServer},
    plugin::PluginRegistry,
    plugin_installation::EnabledPackage,
    RpcRequest,
};
use axum::{
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

const OWNER: u8 = 0x51;

fn registry(root: &Path) -> PluginRegistry {
    let package = root.join("package-core");
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("plugin.ts"),
        r#"
export const name = "core";
export const systems = {
  gba: { title: "Game Boy Advance", content: { extensions: ["gba"] } },
  psx: { title: "PlayStation", content: { extensions: ["cue", "bin"], layout: ["{title}.{ext}"] } },
};
"#,
    )
    .unwrap();
    fs::write(
        package.join("manifest.json"),
        serde_json::json!({"entry":"plugin.ts","sources":["plugin.ts"],"files":{}}).to_string(),
    )
    .unwrap();
    PluginRegistry::from_installed(vec![EnabledPackage {
        id: "@test:core".into(),
        package,
        files: BTreeMap::new(),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
        requires: vec![],
        hosts: vec![],
        credential_host: None,
        credential_token_url: None,
    }])
    .unwrap()
}

/// A holder's piece route: counts served pieces, and refuses everything
/// once the device is gone.
#[derive(Clone, Default)]
struct Behaviour {
    pieces: Arc<AtomicUsize>,
    gone: Arc<AtomicBool>,
    /// The device goes away once it has served this many pieces.
    lose_after: Arc<AtomicUsize>,
    /// Each piece waits this long before it is served.
    delay_ms: Arc<AtomicUsize>,
}
async fn behave(
    axum::extract::State(behaviour): axum::extract::State<Behaviour>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if behaviour.gone.load(Ordering::SeqCst) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let piece = request.uri().path() == "/peer-installs-piece";
    if piece {
        let delay = behaviour.delay_ms.load(Ordering::SeqCst) as u64;
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
    let response = next.run(request).await;
    if piece && response.status().is_success() {
        let served = behaviour.pieces.fetch_add(1, Ordering::SeqCst) + 1;
        let limit = behaviour.lose_after.load(Ordering::SeqCst);
        if limit > 0 && served >= limit {
            behaviour.gone.store(true, Ordering::SeqCst);
        }
    }
    response
}

/// One in-process korrid: its library, its acquisition and its identity.
struct Device {
    _dirs: Vec<tempfile::TempDir>,
    context: GameContext,
    coordinator: AcquisitionCoordinator,
    state: crate::AppState,
    credentials: PeerCredentials,
    base: Option<String>,
    behaviour: Behaviour,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl Drop for Device {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

impl Device {
    async fn new(device: u8, serve: bool) -> Self {
        let dirs: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
        let root = dirs[0].path().to_path_buf();
        let private = dirs[1].path().to_path_buf();
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700)).unwrap();
        let registry = registry(dirs[2].path());
        let identity = peer_rpc::test_owned_identity(
            &private,
            &format!("{device:064x}"),
            &format!("{OWNER:064x}"),
        );
        let credentials = PeerCredentials::from_identity(identity);
        let config = root.join("host.toml");
        fs::write(&config, "label = \"device\"\ngames = []\n").unwrap();
        let runtime = crate::host::HostRuntime::from_paths_with_backend(
            &config,
            Some(root.clone()),
            private.clone(),
            Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
        )
        .with_route_registry(root.clone(), registry.clone())
        .with_library_discovery();
        let discovery = runtime.library_discovery().unwrap().clone();
        discovery.wait_until_idle(Duration::from_secs(10));
        let coordinator = AcquisitionCoordinator {
            sleep: sleep::SleepBlock::InMemory(Arc::default()),
            ..Default::default()
        };
        let (state, _) = crate::app_states_with(runtime, coordinator.clone());
        let behaviour = Behaviour::default();
        let (base, task) = if serve {
            let authorization = authorization::Authorization::load(&private).unwrap();
            let server = PeerRpcServer::with_shared_authority(
                state.clone(),
                &private,
                credentials.clone(),
                authorization,
            )
            .unwrap()
            .router(None)
            .layer(middleware::from_fn_with_state(behaviour.clone(), behave));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let task = tokio::spawn(async move { axum::serve(listener, server).await.unwrap() });
            (Some(base), Some(task))
        } else {
            (None, None)
        };
        Self {
            _dirs: dirs,
            context: GameContext {
                root,
                private_root: private,
                registry,
                discovery,
            },
            coordinator,
            state,
            credentials,
            base,
            behaviour,
            task,
        }
    }

    fn key(&self) -> String {
        self.credentials.public_key().unwrap()
    }

    /// An installed release in the ADR 0092 layout.
    fn hold(&self, key: &str, files: &[(&str, &[u8])]) {
        let ready = placement::copied_release_cache(&self.context.root)
            .join(placement::ready_name(key).unwrap());
        fs::create_dir_all(&ready).unwrap();
        for (name, bytes) in files {
            fs::write(ready.join(name), bytes).unwrap();
        }
    }

    /// How `receiver` reaches this holder.
    fn holder_for(&self, receiver: &Device) -> Arc<PeerHolder> {
        Arc::new(PeerHolder {
            route: PeerRoute::Origin(self.base.clone().unwrap()),
            credentials: receiver.credentials.clone(),
            device: self.key(),
        })
    }

    /// Lose this device: it answers nothing more.
    fn lose(&self) {
        self.behaviour.gone.store(true, Ordering::SeqCst);
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

fn release(bytes: usize) -> (Vec<u8>, String) {
    let rom: Vec<u8> = (0..bytes)
        .map(|index| (index.wrapping_mul(31) % 251) as u8)
        .collect();
    let key = format!("sha256:{}", hex::encode(Sha256::digest(&rom)));
    (rom, key)
}

/// Two holders serve one install at once. The first is lost in the middle
/// of it; the other serves the rest. The finished install is checked
/// against its release key, published and recorded, and the holders were
/// asked with no prompt because they share the receiver's owner.
#[tokio::test(flavor = "multi_thread")]
async fn an_install_takes_pieces_from_two_holders_and_survives_losing_one() {
    let (rom, key) = release(12 * PIECE_BYTES as usize + 4321);
    let first = Device::new(1, true).await;
    let second = Device::new(2, true).await;
    let receiver = Device::new(3, false).await;
    first.hold(&key, &[("Game.gba", &rom)]);
    second.hold(&key, &[("game (copy).gba", &rom)]);

    let holders = vec![first.holder_for(&receiver), second.holder_for(&receiver)];
    // Lose the first holder once it has served two pieces.
    first.behaviour.lose_after.store(2, Ordering::SeqCst);
    let pieces = receiver.coordinator.pieces();
    let job = pieces
        .start(
            &receiver.coordinator,
            &receiver.context,
            &key,
            "Game",
            holders,
            Place::Front,
        )
        .unwrap();
    let game = pieces.wait(&job).await.unwrap();
    assert!(
        first.behaviour.gone.load(Ordering::SeqCst),
        "the first holder was lost"
    );
    let (from_first, from_second) = (
        first.behaviour.pieces.load(Ordering::SeqCst),
        second.behaviour.pieces.load(Ordering::SeqCst),
    );
    assert!(from_first >= 2, "{from_first}");
    assert!(from_second >= 1, "{from_second}");
    assert!(
        from_first + from_second >= 13,
        "every piece came from a holder: {from_first} + {from_second}"
    );
    match pieces.status(&job).unwrap().state {
        super::pieces::DeviceInstallState::Imported { game_id } => assert_eq!(game_id, game),
        other => panic!("{other:?}"),
    }

    // The install is the release: the first answer named it.
    let ready = placement::copied_release_cache(&receiver.context.root)
        .join(placement::ready_name(&key).unwrap());
    assert_eq!(fs::read(ready.join("Game.gba")).unwrap(), rom);
    let snapshot = receiver.context.discovery.published().unwrap();
    assert_eq!(
        snapshot.library[&game].releases[0]
            .identity
            .as_ref()
            .unwrap()
            .value(),
        key
    );
    first.lose();
}

/// A disc set is one collection: a cue and its tracks install together.
#[tokio::test(flavor = "multi_thread")]
async fn a_cue_with_its_tracks_installs_as_one_collection() {
    let first = Device::new(4, true).await;
    let receiver = Device::new(5, false).await;
    let track1: Vec<u8> = (0..(PIECE_BYTES as usize + 77))
        .map(|i| (i % 7) as u8)
        .collect();
    let track2: Vec<u8> = (0..4096).map(|i| (i % 13) as u8).collect();
    let cue = b"FILE \"Game (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\nFILE \"Game (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n";
    // The release key is discovery's, computed over the folder.
    let probe = tempfile::tempdir().unwrap();
    for (name, bytes) in [
        ("Game.cue", &cue[..]),
        ("Game (Track 1).bin", &track1),
        ("Game (Track 2).bin", &track2),
    ] {
        fs::write(probe.path().join(name), bytes).unwrap();
    }
    let rules = receiver
        .context
        .registry
        .content_rules("psx")
        .unwrap()
        .clone();
    let key = super::game::identify_work(probe.path(), &rules)
        .unwrap()
        .value()
        .to_owned();
    first.hold(
        &key,
        &[
            ("Game.cue", &cue[..]),
            ("Game (Track 1).bin", &track1),
            ("Game (Track 2).bin", &track2),
        ],
    );
    receiver
        .coordinator
        .pieces()
        .install(
            &receiver.coordinator,
            &receiver.context,
            &key,
            "Disc game",
            vec![first.holder_for(&receiver)],
            Place::Back,
        )
        .await
        .unwrap();
    let ready = placement::copied_release_cache(&receiver.context.root)
        .join(placement::ready_name(&key).unwrap());
    assert_eq!(fs::read(ready.join("Game (Track 1).bin")).unwrap(), track1);
    assert_eq!(fs::read(ready.join("Game (Track 2).bin")).unwrap(), track2);
    assert_eq!(fs::read(ready.join("Game.cue")).unwrap(), cue);
}

/// A holder that sends bytes that are not the release: every piece fails
/// its check, the holder leaves, and nothing is installed.
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_whose_bytes_are_not_the_release_installs_nothing() {
    let (rom, key) = release(2 * PIECE_BYTES as usize);
    let holder = Device::new(6, true).await;
    let receiver = Device::new(7, false).await;
    let mut other = rom.clone();
    other[10] ^= 0xff;
    // The holder's folder says it is the release, but its bytes differ.
    holder.hold(&key, &[("Game.gba", &other)]);
    let failure = receiver
        .coordinator
        .pieces()
        .install(
            &receiver.coordinator,
            &receiver.context,
            &key,
            "Game",
            vec![holder.holder_for(&receiver)],
            Place::Back,
        )
        .await
        .unwrap_err();
    assert_eq!(failure.code, "InstallFailed");
    let ready = placement::copied_release_cache(&receiver.context.root)
        .join(placement::ready_name(&key).unwrap());
    assert!(!ready.exists());
    assert!(receiver
        .context
        .discovery
        .published()
        .unwrap()
        .library
        .is_empty());
}

fn pass() -> PersonPass {
    PersonPass {
        event_id: "pass".into(),
        event_json: "verified upstream".into(),
        expires_at: u64::MAX,
        scopes: vec![Scope::CatalogRead, Scope::StreamLaunch],
        person_public_key: "person".into(),
    }
}

fn peer(principal: Principal) -> AuthorizationContext {
    AuthorizationContext::Peer(principal)
}

/// ADR 0077: the owner's devices and family members' devices install from
/// a holder with no prompt; a guest is refused, at the peer policy and at
/// the piece route, even with a pass that may stream.
#[tokio::test(flavor = "multi_thread")]
async fn a_guest_is_refused_and_family_and_owner_devices_may_install() {
    let owner = peer(Principal::OwnerDevice {
        device_public_key: "peer".into(),
        owner_statement: "verified upstream".into(),
        owner_public_key: "owner".into(),
    });
    let family = peer(Principal::Household {
        device_public_key: "peer".into(),
        owner_statement: "verified upstream".into(),
        person_pass: pass(),
    });
    let guest = peer(Principal::Guest {
        device_public_key: "peer".into(),
        owner_statement: "verified upstream".into(),
        person_pass: pass(),
    });
    let unknown = peer(Principal::Unknown {
        device_public_key: "peer".into(),
    });
    let manifest = RpcRequest::InstallManifest(InstallManifestRequest {
        release_key: format!("sha256:{}", "a".repeat(64)),
    });
    let piece = RpcRequest::InstallPiece(InstallPieceRequest {
        release_key: format!("sha256:{}", "a".repeat(64)),
        blake3: "b".repeat(64),
        offset: 0,
        length: 1,
    });
    for request in [&manifest, &piece] {
        // N7: local contexts that are not a person never ask for installs.
        assert!(
            authorization::authorize(&AuthorizationContext::LocalUnixControl, request).is_err()
        );
        assert!(
            authorization::authorize(&AuthorizationContext::TrustReconciliation, request).is_err()
        );
        assert!(authorization::authorize(&owner, request).is_ok());
        assert!(authorization::authorize(&family, request).is_ok());
        assert!(authorization::authorize(&guest, request).is_err());
        assert!(authorization::authorize(&unknown, request).is_err());
    }
    assert!(may_install(&owner) && may_install(&family));
    assert!(!may_install(&guest) && !may_install(&unknown));

    // The real piece route of a holder that holds the release.
    let (rom, key) = release(4096);
    let holder = Device::new(8, true).await;
    holder.hold(&key, &[("Game.gba", &rom)]);
    let manifest = holder
        .coordinator
        .pieces()
        .manifest(&holder.context, &key)
        .await
        .unwrap();
    let request = InstallPieceRequest {
        release_key: key.clone(),
        blake3: manifest.files[0].blake3.clone(),
        offset: 0,
        length: 4096,
    };
    for (context, allowed) in [
        (&guest, false),
        (&unknown, false),
        (&family, true),
        (&owner, true),
    ] {
        let response = super::pieces::piece_response(&holder.state, context, &request).await;
        assert_eq!(
            response.status().is_success(),
            allowed,
            "{context:?}: {}",
            response.status()
        );
    }
}

/// The holder keeps the BLAKE3 tree: a second manifest does not hash again,
/// and a file that changed is hashed again.
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_hashes_once_and_again_after_the_file_changes() {
    let (rom, key) = release(3 * PIECE_BYTES as usize);
    let holder = Device::new(9, false).await;
    holder.hold(&key, &[("Game.gba", &rom)]);
    let pieces = holder.coordinator.pieces();
    let first = pieces.manifest(&holder.context, &key).await.unwrap();
    assert_eq!(first.files[0].bytes, rom.len() as u64);
    assert_eq!(first.files[0].blake3, iroh_blobs::Hash::new(&rom).to_hex());
    assert!(super::pieces::store_path(&holder.context.root)
        .join("blobs.db")
        .is_file());
    assert_eq!(pieces.manifest(&holder.context, &key).await.unwrap(), first);
    // In place: the store holds no copy of the release's bytes.
    let copied: u64 = walk_size(&super::pieces::store_path(&holder.context.root));
    assert!(copied < rom.len() as u64 / 2, "{copied}");
    let ready = placement::copied_release_cache(&holder.context.root)
        .join(placement::ready_name(&key).unwrap());
    let mut grown = rom.clone();
    grown.extend_from_slice(b"more");
    fs::write(ready.join("Game.gba"), &grown).unwrap();
    let again = pieces.manifest(&holder.context, &key).await.unwrap();
    assert_eq!(again.files[0].bytes, grown.len() as u64);
}

fn walk_size(path: &PathBuf) -> u64 {
    let mut total = 0;
    for entry in fs::read_dir(path).unwrap().flatten() {
        let metadata = entry.metadata().unwrap();
        total += if metadata.is_dir() {
            walk_size(&entry.path())
        } else {
            metadata.len()
        };
    }
    total
}

/// ADR 0092 measurement: the size of the BLAKE3 tree (outboard) that a
/// holder keeps for one file, hashed in place with the real store.
/// `KORRI_OUTBOARD_FILE=<path> cargo test --lib outboard_size -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs KORRI_OUTBOARD_FILE, a large file"]
async fn outboard_size_of_a_release_hashed_in_place() {
    let path = PathBuf::from(std::env::var("KORRI_OUTBOARD_FILE").expect("KORRI_OUTBOARD_FILE"));
    let path = path.canonicalize().unwrap();
    let store_dir = tempfile::tempdir().unwrap();
    let store = iroh_blobs::store::fs::FsStore::load(store_dir.path())
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let tag = store
        .add_path_with_opts(iroh_blobs::api::blobs::AddPathOptions {
            path: path.clone(),
            format: iroh_blobs::BlobFormat::Raw,
            mode: iroh_blobs::api::blobs::ImportMode::TryReference,
        })
        .temp_tag()
        .await
        .unwrap();
    let elapsed = started.elapsed();
    store.sync_db().await.unwrap();
    let size = fs::metadata(&path).unwrap().len();
    let mut outboards = Vec::new();
    let mut data = 0;
    walk_files(store_dir.path(), &mut |file: &Path, bytes| {
        if file.extension().is_some_and(|ext| ext == "obao4") {
            outboards.push(bytes);
        } else if file.extension().is_some_and(|ext| ext == "data") {
            data += bytes;
        }
    });
    let groups = size.div_ceil(16 * 1024);
    println!(
        "file {} bytes; blake3 {}; outboard {:?} bytes (formula (groups - 1) * 64 = {}); copied data {} bytes; whole store {} bytes; hashed in {:.1} s",
        size,
        tag.hash().to_hex(),
        outboards,
        (groups.saturating_sub(1)) * 64,
        data,
        walk_size(&store_dir.path().to_path_buf()),
        elapsed.as_secs_f64(),
    );
    assert_eq!(outboards, [(groups - 1) * 64]);
    assert_eq!(data, 0, "hashed in place, not copied");
}

fn walk_files(path: &Path, seen: &mut impl FnMut(&Path, u64)) {
    for entry in fs::read_dir(path).unwrap().flatten() {
        let metadata = entry.metadata().unwrap();
        if metadata.is_dir() {
            walk_files(&entry.path(), seen);
        } else {
            seen(&entry.path(), metadata.len());
        }
    }
}

fn ready(device: &Device, key: &str) -> PathBuf {
    placement::copied_release_cache(&device.context.root).join(placement::ready_name(key).unwrap())
}

async fn installing_past(pieces: &super::pieces::Pieces, job: &str, bytes: u64) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(super::pieces::DeviceInstallState::Installing { bytes_done, .. }) =
                pieces.status(job).map(|status| status.state)
            {
                if bytes_done >= bytes {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

/// M2: an install from devices is in the line under its job id, the order
/// file never names it, and cancel during a piece fetch stops it: no ready
/// folder, no receive store, no place in the line.
#[tokio::test(flavor = "multi_thread")]
async fn an_install_from_devices_shows_in_the_line_and_cancel_stops_a_piece_fetch() {
    let (rom, key) = release(8 * PIECE_BYTES as usize);
    let holder = Device::new(10, true).await;
    let receiver = Device::new(11, false).await;
    holder.hold(&key, &[("Game.gba", &rom)]);
    holder.behaviour.delay_ms.store(150, Ordering::SeqCst);
    let pieces = receiver.coordinator.pieces();
    let job = pieces
        .start(
            &receiver.coordinator,
            &receiver.context,
            &key,
            "Game",
            vec![holder.holder_for(&receiver)],
            Place::Back,
        )
        .unwrap();
    installing_past(pieces, &job, PIECE_BYTES).await;
    assert_eq!(
        receiver.coordinator.line(),
        [super::Member::Job(job.clone())]
    );
    let order = super::install::order_path(&receiver.context.private_root);
    if let Ok(bytes) = fs::read(&order) {
        let saved: Vec<super::Member> = serde_json::from_slice(&bytes).unwrap();
        assert!(saved.is_empty(), "a restart forgets it: {saved:?}");
    }
    // The next piece fetch is in flight (each takes 150 ms); cancel now.
    let served = holder.behaviour.pieces.load(Ordering::SeqCst);
    let status = pieces.cancel(&job).await.unwrap();
    assert!(
        matches!(status.state, super::pieces::DeviceInstallState::Cancelled),
        "{status:?}"
    );
    assert!(
        holder.behaviour.pieces.load(Ordering::SeqCst) < 8,
        "{served}"
    );
    assert!(!ready(&receiver, &key).exists());
    assert!(!super::pieces::receive_path(
        &receiver.context.root,
        placement::ready_name(&key).unwrap()
    )
    .exists());
    assert!(receiver.coordinator.line().is_empty());
    assert!(receiver
        .context
        .discovery
        .published()
        .unwrap()
        .library
        .is_empty());
}

/// N2: a second install of a release that is already in the line joins the
/// first: one job, one transfer, the same result for both callers.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_install_of_the_same_release_joins_the_first() {
    let (rom, key) = release(3 * PIECE_BYTES as usize);
    let holder = Device::new(12, true).await;
    let receiver = Device::new(13, false).await;
    holder.hold(&key, &[("Game.gba", &rom)]);
    holder.behaviour.delay_ms.store(20, Ordering::SeqCst);
    let pieces = receiver.coordinator.pieces();
    let start = || {
        pieces.start(
            &receiver.coordinator,
            &receiver.context,
            &key,
            "Game",
            vec![holder.holder_for(&receiver)],
            Place::Front,
        )
    };
    let (first, second) = (start().unwrap(), start().unwrap());
    assert_eq!(first, second);
    let (a, b) = tokio::join!(pieces.wait(&first), pieces.wait(&second));
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(holder.behaviour.pieces.load(Ordering::SeqCst), 3);

    // Below the jobs, the line never lets two installs share one place:
    // the same path waits under its own key (the netplay copy and a
    // devices install of one release).
    let path = receiver.context.root.join("cache").join("x".repeat(64));
    let one = receiver
        .coordinator
        .queue_turn(path.clone(), Place::Back, super::Listed::Hidden);
    let two = receiver
        .coordinator
        .queue_turn(path, Place::Back, super::Listed::Hidden);
    assert_ne!(one.key, two.key);
}

/// N1: publication fails after the files left the receive store. The store
/// goes, so the next try installs the release instead of failing forever.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_publication_does_not_leave_the_release_stuck() {
    let (rom, key) = release(2 * PIECE_BYTES as usize + 99);
    let holder = Device::new(14, true).await;
    let receiver = Device::new(15, false).await;
    holder.hold(&key, &[("Game.gba", &rom)]);
    let pieces = receiver.coordinator.pieces();
    let install = || {
        pieces.install(
            &receiver.coordinator,
            &receiver.context,
            &key,
            "Game",
            vec![holder.holder_for(&receiver)],
            Place::Back,
        )
    };
    pieces.fault(Some(super::pieces::PublishFault::Fail));
    assert_eq!(install().await.unwrap_err().code, "InstallFailed");
    let receive =
        super::pieces::receive_path(&receiver.context.root, placement::ready_name(&key).unwrap());
    assert!(!receive.exists(), "the store that named moved bytes went");
    pieces.fault(None);
    install().await.unwrap();
    assert_eq!(
        fs::read(ready(&receiver, &key).join("Game.gba")).unwrap(),
        rom
    );
}

/// N1: korrid stops during publication. The work folder goes, but the
/// receive store still reports the file complete. The next try finds that
/// the data cannot be read, starts again and installs the release.
#[tokio::test(flavor = "multi_thread")]
async fn a_stop_during_publication_does_not_leave_the_release_stuck() {
    let (rom, key) = release(2 * PIECE_BYTES as usize + 99);
    let holder = Device::new(16, true).await;
    let receiver = Device::new(17, false).await;
    holder.hold(&key, &[("Game.gba", &rom)]);
    let pieces = receiver.coordinator.pieces();
    let install = || {
        pieces.install(
            &receiver.coordinator,
            &receiver.context,
            &key,
            "Game",
            vec![holder.holder_for(&receiver)],
            Place::Back,
        )
    };
    pieces.fault(Some(super::pieces::PublishFault::Crash));
    assert_eq!(install().await.unwrap_err().code, "TestCrash");
    assert!(super::pieces::receive_path(
        &receiver.context.root,
        placement::ready_name(&key).unwrap()
    )
    .exists());
    let served = holder.behaviour.pieces.load(Ordering::SeqCst);
    pieces.fault(None);
    install().await.unwrap();
    assert_eq!(
        fs::read(ready(&receiver, &key).join("Game.gba")).unwrap(),
        rom
    );
    assert_eq!(
        holder.behaviour.pieces.load(Ordering::SeqCst),
        2 * served,
        "every piece came again"
    );
}
