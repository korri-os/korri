use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use korrid::{
    catalog_cli,
    config::snapshot::ConfigSnapshotCoordinator,
    discovery::{
        coordinator::DiscoveryRequestError, sources::PairingError, DiscoveryLifecycleCoordinator,
        DiscoveryPhase, FolderSelectionGrantStore,
    },
    LibraryFoldersRescanRequest, LibraryFoldersSnapshot, LibraryFoldersSnapshotOutcome,
    LibraryFoldersSnapshotRequest, LibraryFoldersState, RpcRequest, RpcResponse,
};
use std::{
    ffi::OsString,
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tower::ServiceExt;

// The live RPCs use this same coordinator and one-use receipt store. The
// configuration it publishes still calls the persisted collection `sources`.
fn live_folders(root: &Path, private: &Path) -> DiscoveryLifecycleCoordinator {
    let discovery = DiscoveryLifecycleCoordinator::new_with_registry_source(
        root,
        private,
        Arc::new(Mutex::new(())),
        FolderSelectionGrantStore::default(),
        korrid::plugin_policy::RegistrySource::Selected(
            korrid::plugin_test_fixtures::installed(root).into(),
        ),
    );
    settled(&discovery);
    discovery
}

fn settled(discovery: &DiscoveryLifecycleCoordinator) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = discovery.snapshot();
        if !matches!(
            snapshot.state,
            DiscoveryPhase::Scanning | DiscoveryPhase::Enriching
        ) {
            assert_eq!(snapshot.state, DiscoveryPhase::Idle, "{snapshot:?}");
            return;
        }
        assert!(
            Instant::now() < deadline,
            "scan did not settle: {snapshot:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn add(discovery: &DiscoveryLifecycleCoordinator, selected: &Path, system: &str) {
    let receipt = discovery
        .grants()
        .issue_approved_path(selected)
        .unwrap()
        .token;
    discovery.pair(&receipt, system).unwrap();
    settled(discovery);
}

#[test]
fn add_rescan_and_remove_use_persisted_sources_and_one_library_document() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let selected = tempfile::tempdir().unwrap();
    let game = selected.path().join("Wario Land 4.gba");
    fs::write(&game, b"game").unwrap();
    let discovery = live_folders(root.path(), private.path());
    add(&discovery, selected.path(), "gba");
    let state = ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(state.diagnostic.is_none());
    assert!(state.snapshot.sources.contains_key("gba"));
    assert!(state.snapshot.library.contains_key("wario-land-4"));
    assert!(!root.path().join("catalog/games.yaml").exists());
    assert!(!root.path().join("catalog/releases.yaml").exists());
    let before = fs::read(root.path().join("catalog/library.yaml")).unwrap();
    discovery.rescan();
    settled(&discovery);
    assert_eq!(
        fs::read(root.path().join("catalog/library.yaml")).unwrap(),
        before
    );
    discovery.unpair("gba").unwrap();
    settled(&discovery);
    let state = ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(state.snapshot.library.is_empty());
    assert!(state.snapshot.sources.is_empty());
    assert_eq!(fs::read(&game).unwrap(), b"game");
}

#[test]
fn retired_catalog_syntax_is_rejected_without_state_changes() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    for args in [
        vec!["import", "/tmp"],
        vec!["remove", "gba"],
        vec!["pair", "/tmp", "gba"],
        vec!["unpair", "gba"],
        vec!["rescan"],
    ] {
        let args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
        assert!(catalog_cli::run(
            &args,
            Some(root.path().as_os_str()),
            Some(private.path().as_os_str()),
        )
        .unwrap_err()
        .starts_with("usage:"));
    }
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    assert_eq!(fs::read_dir(private.path()).unwrap().count(), 0);
}

#[test]
fn adding_unknown_system_refuses_without_making_a_persisted_source() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let selected = tempfile::tempdir().unwrap();
    let discovery = live_folders(root.path(), private.path());
    let documents = ["device.yaml", "catalog/library.yaml"];
    let before = documents.map(|path| fs::read(root.path().join(path)).unwrap());
    let receipt = discovery
        .grants()
        .issue_approved_path(selected.path())
        .unwrap()
        .token;
    assert!(matches!(
        discovery.pair(&receipt, "unknown"),
        Err(DiscoveryRequestError::Pairing(PairingError::UnknownSystem(system))) if system == "unknown"
    ));
    assert_eq!(
        documents.map(|path| fs::read(root.path().join(path)).unwrap()),
        before
    );
    let state = ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(state.diagnostic.is_none());
    assert!(state.snapshot.sources.is_empty());
    assert!(state.snapshot.library.is_empty());
}

async fn public_snapshot(app: Router, request: RpcRequest) -> (LibraryFoldersSnapshot, String) {
    let response = app
        .oneshot(
            Request::post("/rpc")
                .header("authorization", "Bearer catalog-contract")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    let snapshot = match serde_json::from_str::<RpcResponse>(&body).unwrap() {
        RpcResponse::LibraryFoldersSnapshot(LibraryFoldersSnapshotOutcome::Ok(snapshot))
        | RpcResponse::LibraryFoldersRescan(LibraryFoldersSnapshotOutcome::Ok(snapshot)) => {
            snapshot
        }
        other => panic!("unexpected public response: {other:?}"),
    };
    (snapshot, body)
}

#[tokio::test]
async fn malformed_library_reports_public_scan_failure_without_echoing_private_content() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("catalog")).unwrap();
    fs::write(root.path().join("device.yaml"), "{}\n").unwrap();
    let broken = "private-secret-token: [broken\n";
    let library = root.path().join("catalog/library.yaml");
    fs::write(&library, broken).unwrap();
    let app = korrid::router_with_capability_and_roots(
        "catalog-contract",
        "https://portal.example",
        root.path(),
        private.path().join("state"),
    );
    let (mut snapshot, body) = public_snapshot(
        app.clone(),
        RpcRequest::LibraryFoldersRescan(LibraryFoldersRescanRequest {}),
    )
    .await;
    assert!(!body.contains("private-secret-token"), "{body}");
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(
        snapshot.state,
        LibraryFoldersState::Scanning {} | LibraryFoldersState::Enriching {}
    ) {
        assert!(
            Instant::now() < deadline,
            "scan did not settle: {snapshot:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
        let (next, body) = public_snapshot(
            app.clone(),
            RpcRequest::LibraryFoldersSnapshot(LibraryFoldersSnapshotRequest {}),
        )
        .await;
        // Inspect the actual serialized public response, not only fields kept
        // after decoding it, so an unexpected diagnostic cannot hide a leak.
        assert!(!body.contains("private-secret-token"), "{body}");
        snapshot = next;
    }
    assert_eq!(
        snapshot.state,
        LibraryFoldersState::Problem {},
        "{snapshot:?}"
    );
    assert!(
        snapshot
            .diagnostics
            .iter()
            .any(|d| d.code == "LocalConfigReloadFailed"),
        "{snapshot:?}"
    );
    assert_eq!(fs::read_to_string(&library).unwrap(), broken);
}

#[test]
fn only_paired_games_enter_catalog_not_build_trees_or_unpaired_folders() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    for directory in [
        "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/build",
        "unpaired",
        "games/gba",
    ] {
        fs::create_dir_all(root.path().join(directory)).unwrap();
        for (file, bytes) in [
            ("compiler.o", b"object".as_slice()),
            ("README.md", b"readme".as_slice()),
            ("Game.gba", b"real game".as_slice()),
        ] {
            fs::write(root.path().join(directory).join(file), bytes).unwrap();
        }
    }
    let discovery = live_folders(root.path(), private.path());
    discovery.rescan();
    settled(&discovery);
    assert!(ConfigSnapshotCoordinator::new(root.path())
        .reload()
        .snapshot
        .library
        .is_empty());
    add(&discovery, &root.path().join("games/gba"), "gba");
    let state = ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(state.diagnostic.is_none());
    assert_eq!(state.snapshot.library.len(), 1);
    assert_eq!(
        state.snapshot.library["game"].title.as_deref(),
        Some("Game")
    );
    let locations: Vec<_> = state.snapshot.locations.values().flatten().collect();
    assert_eq!(locations.len(), 1);
    assert!(
        matches!(locations[0], korrid::config::Location::File { storage, path, .. }
        if storage.0 == "gba" && path.0 == "Game.gba")
    );
}
