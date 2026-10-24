use super::*;
use crate::*;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use std::sync::Arc;
use tower::ServiceExt;

#[test]
fn cli_has_browser_authority_without_changing_inputd_or_peers() {
    for request in [
        RpcRequest::IdentityStatus(IdentityStatusRequest {}),
        RpcRequest::NetworkStatus(network::NetworkStatusRequest {}),
        RpcRequest::OnboardingStatus(onboarding::OnboardingStatusRequest {}),
        RpcRequest::PluginRepositories(plugin_admin::PluginRepositoriesRequest {}),
    ] {
        assert_eq!(
            authorization::authorize(&AuthorizationContext::LocalCli, &request),
            authorization::authorize(&AuthorizationContext::LocalBrowser, &request)
        );
    }
    let network = RpcRequest::NetworkStatus(network::NetworkStatusRequest {});
    assert!(authorization::authorize(&AuthorizationContext::LocalUnixControl, &network).is_err());
    assert_eq!(
        AuthorizationContext::LocalCli.person_public_key(Some("owner")),
        Some("owner")
    );
}

fn fixture() -> (tempfile::TempDir, Router, Router, FederationResources) {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("host.toml");
    std::fs::write(&config, "label = \"CLI test\"\n").unwrap();
    let private = root.path().join("private");
    let resources = FederationResources::open(&private).unwrap();
    let runtime = host::HostRuntime::from_paths_with_backend(
        &config,
        Some(root.path().join("library")),
        private.clone(),
        Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
    );
    let (_, control, cli) =
        secure_host_routers_with_federation(runtime, &private, resources.clone(), None, None);
    (root, cli, control, resources)
}

#[tokio::test]
async fn person_router_serves_health_but_has_no_private_input_websocket() {
    let (_root, cli, _, _) = fixture();
    let response = cli
        .clone()
        .oneshot(
            Request::post("/rpc")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"_tag":"system.health","payload":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response: RpcResponse =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert!(matches!(
        response,
        RpcResponse::Health(HealthOutcome::Ok(_))
    ));
    for path in [
        "/",
        "/input",
        "/input/private",
        "/private-input",
        "/native-input",
    ] {
        let response = cli
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn forwarding_rejects_unsigned_and_local_only_targets_with_existing_failure() {
    let (_root, cli, control, resources) = fixture();
    assert!(matches!(
        resources.directory.identity_state().unwrap(),
        identity::IdentityState::Unowned { .. }
    ));
    let request = cli::ForwardRequest {
        device_public_key: "11".repeat(32),
        request: RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
    };
    let response = cli
        .clone()
        .oneshot(
            Request::post("/forward")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response: RpcResponse =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert!(
        matches!(response, RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Err(RpcFailure { code, .. })) if code == "OwnerDeviceRequired")
    );
    let request = cli::ForwardRequest {
        request: RpcRequest::IdentityStatus(IdentityStatusRequest {}),
        ..request
    };
    let body = serde_json::to_vec(&request).unwrap();
    let response = cli
        .oneshot(
            Request::post("/forward")
                .header("content-type", "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = control
        .oneshot(
            Request::post("/forward")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn identity_status_reads_owned_and_unowned_live_authority_without_creating_state() {
    use std::sync::Mutex;
    for owned in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let private = root.path().join("private");
        if owned {
            peer_rpc::test_owned_credentials(&private, &"61".repeat(32), &"62".repeat(32));
        }
        let resources = FederationResources::open(&private).unwrap();
        let expected = resources.directory.identity_state().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"Status\"\n").unwrap();
        let runtime = host::HostRuntime::from_paths_with_backend(
            &config,
            None,
            private.clone(),
            Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
        );
        let (mut state, _) = app_states(runtime);
        let socket = root.path().join("signer.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let signer_root = root.path().join("signer");
        let public = root.path().join("person.pub");
        let device = Arc::new(Mutex::new(resources.credentials.public_key().unwrap()));
        let signer = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            local_signer::serve_connection(stream, signer_root, device, public)
                .await
                .unwrap();
        });
        state.identity_switch = Some(Arc::new(identity_switch::IdentitySwitchCoordinator::new(
            private.clone(),
            socket,
            resources.clone(),
        )));
        state.federation = Some(resources.directory.clone());
        let router = cli::router(state, resources);
        // Status must not reopen or recreate this directory. The live signing
        // authority already owns the authoritative identity snapshot.
        std::fs::rename(
            private.join("identity"),
            private.join("identity-held-by-test"),
        )
        .unwrap();
        let response = router
            .oneshot(
                Request::post("/rpc")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&RpcRequest::IdentityStatus(IdentityStatusRequest {}))
                            .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let response: RpcResponse =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        let RpcResponse::IdentityStatus(IdentityStatusOutcome::Ok(status)) = response else {
            panic!("{response:?}");
        };
        assert_eq!(status.identity, expected);
        assert!(!private.join("identity").exists());
        signer.await.unwrap();
    }
}

#[tokio::test]
async fn person_socket_speaks_http_to_the_real_router() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (root, cli, _, _) = fixture();
    let path = root.path().join("person.sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, cli).await.unwrap();
    });
    let body = r#"{"_tag":"system.health","payload":{}}"#;
    let mut stream = tokio::net::UnixStream::connect(&path).await.unwrap();
    stream.write_all(format!("POST /rpc HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let http = String::from_utf8(bytes).unwrap();
    assert!(http.starts_with("HTTP/1.1 200 OK"));
    let response: RpcResponse =
        serde_json::from_str(http.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert!(matches!(
        response,
        RpcResponse::Health(HealthOutcome::Ok(_))
    ));
    server.abort();
}

fn folder_fixture(
    ttl: std::time::Duration,
) -> (
    tempfile::TempDir,
    Router,
    Router,
    Router,
    discovery::FolderSelectionGrantStore,
) {
    let root = tempfile::tempdir().unwrap();
    let readable = root.path().join("library");
    std::fs::create_dir_all(&readable).unwrap();
    let private = root.path().join("private");
    let resources = FederationResources::open(&private).unwrap();
    let grants = discovery::FolderSelectionGrantStore::new(ttl);
    let registry = plugin_policy::RegistrySource::Selected(Arc::new(
        crate::plugin_test_fixtures::installed(&readable),
    ));
    let (state, _) = brain_app_state(
        "browser-token",
        "https://portal.example",
        &readable,
        &private,
        registry,
        config::snapshot::ConfigSnapshotCoordinator::new(&readable),
        grants.clone(),
        None,
        Some(resources.clone()),
        None,
    );
    let browser = portal_router_from_state(state.clone());
    let peer = peer_rpc::PeerRpcServer::with_shared_authority(
        state.clone(),
        &private,
        resources.credentials.clone(),
        resources.authorization.clone(),
    )
    .unwrap()
    .router(None);
    (root, super::router(state, resources), browser, peer, grants)
}
async fn post_json(
    app: Router,
    path: &str,
    value: &impl serde::Serialize,
) -> axum::response::Response {
    app.oneshot(
        Request::post(path)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(value).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap()
}
async fn pair_receipt(app: Router, receipt: &str) -> RpcResponse {
    let response = post_json(
        app,
        "/rpc",
        &RpcRequest::LibraryFolderAdd(LibraryFolderAddRequest {
            receipt: receipt.into(),
            system: "gba".into(),
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap()
}
async fn folder_snapshot_when_idle(app: Router) -> LibraryFoldersSnapshot {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let response = post_json(
            app.clone(),
            "/rpc",
            &RpcRequest::LibraryFoldersSnapshot(LibraryFoldersSnapshotRequest {}),
        )
        .await;
        let response: RpcResponse =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        let RpcResponse::LibraryFoldersSnapshot(LibraryFoldersSnapshotOutcome::Ok(snapshot)) =
            response
        else {
            panic!("discovery snapshot unavailable")
        };
        if matches!(snapshot.state, LibraryFoldersState::Idle {}) {
            return snapshot;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "discovery did not settle: {snapshot:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
#[tokio::test]
async fn cli_folder_selection_issues_real_shared_one_use_receipt_and_pairs() {
    let (root, cli, browser, peer, _) = folder_fixture(std::time::Duration::from_secs(300));
    let selected = root.path().join("selected");
    std::fs::create_dir(&selected).unwrap();
    std::fs::write(selected.join("game.gba"), b"game content").unwrap();
    let response = post_json(cli.clone(), "/folder-selection", &selected).await;
    assert_eq!(response.status(), StatusCode::OK);
    let token: String =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert!(!token.contains("selected"));
    assert!(matches!(
        pair_receipt(cli.clone(), &token).await,
        RpcResponse::LibraryFolderAdd(LibraryFoldersSnapshotOutcome::Ok(_))
    ));
    assert!(
        matches!(pair_receipt(cli.clone(), &token).await, RpcResponse::LibraryFolderAdd(LibraryFoldersSnapshotOutcome::Err(RpcFailure {code,..})) if code=="FolderSelectionReceiptUnknown")
    );
    let paired = folder_snapshot_when_idle(cli.clone()).await;
    assert_eq!(paired.folders.len(), 1);
    // The wire rename must not migrate the existing device.yaml schema or IDs.
    // discovery::reconcile remains its producer (storage + sources).
    let device: serde_yaml::Value =
        serde_yaml::from_slice(&std::fs::read(root.path().join("library/device.yaml")).unwrap())
            .unwrap();
    let id = paired.folders[0].id.as_str();
    assert_eq!(device["sources"][id]["system"].as_str(), Some("gba"));
    assert_eq!(device["storage"][id]["root"].as_str(), selected.to_str());
    assert!(device.get("folders").is_none());
    let response = post_json(
        cli.clone(),
        "/rpc",
        &RpcRequest::LibraryFolderRemove(LibraryFolderRemoveRequest {
            folder: paired.folders[0].id.clone(),
        }),
    )
    .await;
    let response: RpcResponse =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert!(matches!(
        response,
        RpcResponse::LibraryFolderRemove(LibraryFoldersSnapshotOutcome::Ok(_))
    ));
    assert!(folder_snapshot_when_idle(cli.clone())
        .await
        .folders
        .is_empty());
    assert_eq!(
        std::fs::read(selected.join("game.gba")).unwrap(),
        b"game content"
    );
    for bad in [selected.join("missing"), selected.join("game.gba")] {
        let response = post_json(cli.clone(), "/folder-selection", &bad).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let failure: RpcFailure =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert_eq!(failure.code, "FolderUnavailable");
    }
    for router in [browser, peer] {
        let response = post_json(router, "/folder-selection", &selected).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    let (_, _, control, _) = fixture();
    let response = post_json(control, "/folder-selection", &selected).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
#[tokio::test]
async fn cli_folder_selection_preserves_receipt_ttl_and_rejects_unknown() {
    let (root, cli, _, _, _) = folder_fixture(std::time::Duration::from_millis(1));
    let response = post_json(cli.clone(), "/folder-selection", &root.path()).await;
    let token: String =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    assert!(
        matches!(pair_receipt(cli.clone(),&token).await,RpcResponse::LibraryFolderAdd(LibraryFoldersSnapshotOutcome::Err(RpcFailure{code,..})) if code=="FolderSelectionReceiptExpired")
    );
    assert!(
        matches!(pair_receipt(cli,"invented").await,RpcResponse::LibraryFolderAdd(LibraryFoldersSnapshotOutcome::Err(RpcFailure{code,..})) if code=="FolderSelectionReceiptUnknown")
    );
}
