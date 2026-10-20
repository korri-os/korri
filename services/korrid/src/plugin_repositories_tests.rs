//! The plugin RPCs forward to a stand-in admin socket and return its answer.
use super::*;
use authorization::{AuthorizationContext, Principal};
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use plugin_admin::*;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;

const TOKEN: &str = "plugin-repositories-test-token";
const ORIGIN: &str = "http://127.0.0.1:8099";

fn state(root: &Path, permission: PortalPermission, socket: Option<PathBuf>) -> AppState {
    config::test_fixtures::gba(root);
    let registry = plugin_test_fixtures::installed(root);
    let (mut state, _) = brain_app_state(
        TOKEN,
        ORIGIN,
        root,
        root.join("private"),
        plugin_policy::RegistrySource::Selected(Arc::new(registry)),
        config::snapshot::ConfigSnapshotCoordinator::new(root),
        discovery::FolderSelectionGrantStore::default(),
        None,
        None,
        None,
    );
    state.portal_access = Some(PortalAccess::new(TOKEN, ORIGIN, permission));
    state.plugin_admin = socket.map(plugin_repositories::PluginAdminSocket::new);
    state
}

/// Answers each connection from `answer` and records the requests it read.
fn stand_in_host(
    path: &Path,
    answer: fn(PluginAdminRequest) -> PluginAdminResponse,
) -> Arc<Mutex<Vec<Value>>> {
    let listener = tokio::net::UnixListener::bind(path).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).await.unwrap();
            record
                .lock()
                .unwrap()
                .push(serde_json::from_slice::<Value>(&bytes).unwrap());
            let request = serde_json::from_slice(&bytes).unwrap();
            let response = serde_json::to_vec(&answer(request)).unwrap();
            stream.write_all(&response).await.unwrap();
        }
    });
    seen
}

async fn call(app: &Router, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rpc")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .header(header::ORIGIN, ORIGIN)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn answer(request: PluginAdminRequest) -> PluginAdminResponse {
    match request {
        PluginAdminRequest::Repositories(_) => {
            PluginAdminResponse::Repositories(PluginRepositories {
                official: None,
                user_added: vec!["https://plugins.example/catalog.json".into()],
                repository_installs: vec![PluginRepositoryInstall {
                    id: "@example:clock".into(),
                    source_url: "https://removed.example/catalog.json".into(),
                    release_version: "v1".into(),
                    enabled: true,
                }],
            })
        }
        PluginAdminRequest::RepositoryAdd(_) => {
            PluginAdminResponse::RepositoryAdded(PluginRepositoryAddResult::AlreadyPresent)
        }
        PluginAdminRequest::Catalog(_) => PluginAdminResponse::Catalog(PluginCatalog {
            entries: vec![PluginCatalogEntry {
                plugin_id: "@example:clock".into(),
                title: Some("Clock".into()),
                description: None,
                release_version: "v1".into(),
                installed: Some(PluginInstalledState::Disabled),
                publisher_trusted: true,
            }],
        }),
        PluginAdminRequest::SetEnabled(request) if request.enabled => {
            PluginAdminResponse::Refused(PluginAdminFailure {
                code: CODE_NEEDS_REVIEW.into(),
                message: crate_host_stale_text().into(),
            })
        }
        PluginAdminRequest::SetEnabled(_) => PluginAdminResponse::Applied(PluginChangeApplied {}),
        // The wrong kind for any other request.
        _ => PluginAdminResponse::Applied(PluginChangeApplied {}),
    }
}

fn crate_host_stale_text() -> &'static str {
    "installed package or host policy no longer matches its approval"
}

#[tokio::test]
async fn plugin_requests_reach_the_host_socket_and_return_its_answers() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("admin.sock");
    let seen = stand_in_host(&socket, answer);
    let app = portal_router_from_state(state(root.path(), PortalPermission::Full, Some(socket)));

    let (status, value) = call(
        &app,
        json!({"_tag":"system.plugins.repositories","payload":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        value["outcome"],
        json!({"_tag":"Ok","payload":{
            "official":null,
            "userAdded":["https://plugins.example/catalog.json"],
            "repositoryInstalls":[{
                "id":"@example:clock",
                "sourceUrl":"https://removed.example/catalog.json",
                "releaseVersion":"v1",
                "enabled":true
            }]
        }})
    );

    let (_, value) = call(
        &app,
        json!({"_tag":"system.plugins.repository.add","payload":{"url":"https://plugins.example/catalog.json"}}),
    )
    .await;
    assert_eq!(
        value["outcome"],
        json!({"_tag":"Ok","payload":"AlreadyPresent"})
    );

    let (_, value) = call(
        &app,
        json!({"_tag":"system.plugins.catalog","payload":{"url":"https://plugins.example/catalog.json"}}),
    )
    .await;
    assert_eq!(
        value["outcome"]["payload"]["entries"][0],
        json!({
            "pluginId":"@example:clock",
            "title":"Clock",
            "releaseVersion":"v1",
            "installed":"Disabled",
            "publisherTrusted":true
        })
    );

    let (_, value) = call(
        &app,
        json!({"_tag":"system.plugins.setEnabled","payload":{"id":"@example:clock","enabled":true}}),
    )
    .await;
    assert_eq!(value["outcome"]["_tag"], "Err");
    assert_eq!(value["outcome"]["payload"]["code"], "PluginNeedsReview");
    assert_eq!(
        value["outcome"]["payload"]["message"],
        crate_host_stale_text()
    );

    let (_, value) = call(
        &app,
        json!({"_tag":"system.plugins.setEnabled","payload":{"id":"@example:clock","enabled":false}}),
    )
    .await;
    assert_eq!(value["outcome"], json!({"_tag":"Ok","payload":{}}));

    // A response of the wrong kind is never passed off as an answer.
    let (_, value) = call(
        &app,
        json!({"_tag":"system.plugins.repository.remove","payload":{"url":"https://plugins.example/catalog.json"}}),
    )
    .await;
    assert_eq!(value["outcome"]["payload"]["code"], CODE_REFUSED);

    // korrid forwards the typed request; it adds nothing of its own.
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen[0], json!({"_tag":"Repositories","payload":{}}));
    assert_eq!(
        seen[3],
        json!({"_tag":"SetEnabled","payload":{"id":"@example:clock","enabled":true}})
    );
}

#[tokio::test]
async fn plugin_requests_fail_plainly_without_a_reachable_host() {
    let root = tempfile::tempdir().unwrap();
    for socket in [None, Some(root.path().join("missing.sock"))] {
        let app = portal_router_from_state(state(root.path(), PortalPermission::Full, socket));
        let (_, value) = call(
            &app,
            json!({"_tag":"system.plugins.repositories","payload":{}}),
        )
        .await;
        assert_eq!(value["outcome"]["_tag"], "Err", "{value}");
        assert_eq!(value["outcome"]["payload"]["code"], CODE_UNAVAILABLE);
    }
}

#[tokio::test]
async fn only_a_full_local_portal_may_change_plugins() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("admin.sock");
    stand_in_host(&socket, answer);
    let app =
        portal_router_from_state(state(root.path(), PortalPermission::ReadOnly, Some(socket)));
    let (status, _) = call(
        &app,
        json!({"_tag":"system.plugins.repositories","payload":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for body in [
        json!({"_tag":"system.plugins.repository.add","payload":{"url":"https://a.example/"}}),
        json!({"_tag":"system.plugins.catalog","payload":{"url":"https://a.example/"}}),
        json!({"_tag":"system.plugins.setEnabled","payload":{"id":"@a:b","enabled":true}}),
    ] {
        let (status, _) = call(&app, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    // No peer device, not even the owner's own, reaches root plugin authority.
    let request = RpcRequest::PluginSetEnabled(PluginSetEnabledRequest {
        id: "@a:b".into(),
        enabled: true,
    });
    for context in [
        AuthorizationContext::LocalUnixControl,
        AuthorizationContext::TrustReconciliation,
        AuthorizationContext::Peer(Principal::OwnerDevice {
            device_public_key: "a".repeat(64),
            owner_statement: String::new(),
            owner_public_key: "b".repeat(64),
        }),
    ] {
        assert!(authorization::authorize(&context, &request).is_err());
    }
    assert!(authorization::authorize(&AuthorizationContext::LocalBrowser, &request).is_ok());
}
