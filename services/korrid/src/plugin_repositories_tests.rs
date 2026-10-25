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
                other_installs: vec![PluginOtherInstall {
                    id: "@korri:mgba".into(),
                    title: Some("mGBA".into()),
                    enabled: false,
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
            "userAdded":["https://plugins.example/catalog.json"],
            "repositoryInstalls":[{
                "id":"@example:clock",
                "sourceUrl":"https://removed.example/catalog.json",
                "releaseVersion":"v1",
                "enabled":true
            }],
            "otherInstalls":[{"id":"@korri:mgba","title":"mGBA","enabled":false}]
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
async fn read_only_portal_may_list_plugins_but_cannot_administer_them() {
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
}

#[test]
fn plugin_admin_authority_requires_a_person_surface_or_verified_owner_peer() {
    use nostr::{
        event::{EventBuilder, FinalizeEvent, Kind, Tag},
        key::Keys,
        types::Timestamp,
    };

    let local_root = tempfile::tempdir().unwrap();
    let peer_root = tempfile::tempdir().unwrap();
    let foreign_root = tempfile::tempdir().unwrap();
    let owner_secret = "31".repeat(32);
    let owner = Keys::parse(&owner_secret).unwrap();
    let local = peer_rpc::test_owned_identity(local_root.path(), &"33".repeat(32), &owner_secret);
    let peer = peer_rpc::test_owned_identity(peer_root.path(), &"34".repeat(32), &owner_secret);
    let foreign =
        peer_rpc::test_owned_identity(foreign_root.path(), &"35".repeat(32), &"32".repeat(32));
    let authority = authorization::Authorization::load(local_root.path()).unwrap();
    let now = peer_rpc::unix_time();
    let peer_key = peer.device_public_key().unwrap();
    let foreign_key = foreign.device_public_key().unwrap();
    let statement = peer.owner_statement_json().unwrap();
    let foreign_statement = foreign.owner_statement_json().unwrap();
    let verified_owner = authority
        .attempt(local.state(), peer_key, Some(&statement), None, &[], now)
        .unwrap();
    assert!(matches!(
        verified_owner.context(),
        AuthorizationContext::Peer(Principal::OwnerDevice { .. })
    ));
    let other_owner = authority
        .attempt(
            local.state(),
            foreign_key,
            Some(&foreign_statement),
            None,
            &[],
            now,
        )
        .unwrap();
    assert!(matches!(
        other_owner.context(),
        AuthorizationContext::Peer(Principal::Unknown { .. })
    ));
    let unknown = authority
        .attempt(local.state(), peer_key, None, None, &[], now)
        .unwrap();

    // These signed pass fixtures use authorization.rs's existing person-pass
    // producer and grant both available scopes. Neither scope grants admin.
    let pass = |tier| {
        EventBuilder::new(Kind::Custom(authorization::PERSON_PASS_EVENT_KIND), "")
            .tags([
                Tag::parse(["d", &format!("org.korri.person-pass:{}", "11".repeat(32))]).unwrap(),
                Tag::parse(["device", foreign_key]).unwrap(),
                Tag::parse(["tier", tier]).unwrap(),
                Tag::parse(["expires", &(now + 60).to_string()]).unwrap(),
                Tag::parse(["scope", authorization::CATALOG_READ_SCOPE]).unwrap(),
                Tag::parse(["scope", authorization::STREAM_LAUNCH_SCOPE]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(now))
            .finalize(&owner)
            .unwrap()
            .as_json()
    };
    let guest = authority
        .attempt(
            local.state(),
            foreign_key,
            Some(&foreign_statement),
            Some(&pass("guest")),
            &[],
            now,
        )
        .unwrap();
    let household = authority
        .attempt(
            local.state(),
            foreign_key,
            Some(&foreign_statement),
            Some(&pass("household")),
            &[],
            now,
        )
        .unwrap();
    assert!(matches!(
        guest.context(),
        AuthorizationContext::Peer(Principal::Guest { .. })
    ));
    assert!(matches!(
        household.context(),
        AuthorizationContext::Peer(Principal::Household { .. })
    ));
    let revoked = EventBuilder::new(Kind::Custom(identity::OWNER_EVENT_KIND), "")
        .tags([
            Tag::parse(["d", &format!("org.korri.device-owner:{peer_key}")]).unwrap(),
            Tag::parse(["device", peer_key]).unwrap(),
            Tag::parse(["status", "revoked"]).unwrap(),
        ])
        .custom_created_at(Timestamp::from(now))
        .finalize(&owner)
        .unwrap()
        .as_json();
    let revoked_owner = authority
        .attempt(
            local.state(),
            peer_key,
            Some(&statement),
            None,
            &[revoked],
            now,
        )
        .unwrap();
    assert!(matches!(
        revoked_owner.context(),
        AuthorizationContext::Peer(Principal::Unknown { .. })
    ));

    // ADR 0031 widens exactly the seven existing admin operations to verified
    // same-owner peers. It grants nothing to inputd, reconciliation or passes.
    for body in [
        json!({"_tag":"system.plugins.repositories","payload":{}}),
        json!({"_tag":"system.plugins.repository.add","payload":{"url":"https://a.example/"}}),
        json!({"_tag":"system.plugins.repository.remove","payload":{"url":"https://a.example/"}}),
        json!({"_tag":"system.plugins.catalog","payload":{"url":"https://a.example/"}}),
        json!({"_tag":"system.plugins.inspect","payload":{"url":"https://a.example/","pluginId":"@a:b","releaseVersion":"v1"}}),
        json!({"_tag":"system.plugins.install","payload":{"url":"https://a.example/","pluginId":"@a:b","releaseVersion":"v1","approval":"reviewed"}}),
        json!({"_tag":"system.plugins.setEnabled","payload":{"id":"@a:b","enabled":true}}),
    ] {
        let request: RpcRequest = serde_json::from_value(body).unwrap();
        assert_eq!(
            authorization::policy_for(&request),
            authorization::PeerPolicy::LocalPortalOrOwnerDevice
        );
        assert!(plugin_repositories::is_admin_request(&request));
        for allowed in [
            &AuthorizationContext::LocalBrowser,
            &AuthorizationContext::LocalCli,
            verified_owner.context(),
        ] {
            assert!(
                authorization::authorize(allowed, &request).is_ok(),
                "{allowed:?}: {request:?}"
            );
        }
        for denied in [
            &AuthorizationContext::LocalUnixControl,
            &AuthorizationContext::TrustReconciliation,
            guest.context(),
            household.context(),
            other_owner.context(),
            unknown.context(),
            revoked_owner.context(),
        ] {
            assert!(
                authorization::authorize(denied, &request).is_err(),
                "{denied:?}: {request:?}"
            );
        }
    }
}
