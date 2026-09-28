//! Device-count RPC checks use the real settings writer and temporary documents.
use super::*;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use tower::ServiceExt;

const TOKEN: &str = "controller-count-test-token";
const ORIGIN: &str = "http://127.0.0.1:8099";
const COUNT: &str = config::settings::PLAYER_COUNT_SETTING_ID;

struct Fixture {
    root: tempfile::TempDir,
    runtime: host::HostRuntime,
    backend: Arc<host::control::InMemoryLaunchUnitBackend>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        config::test_fixtures::gba(root.path());
        let registry = plugin_test_fixtures::installed(root.path());
        let host_config = root.path().join("host.toml");
        std::fs::write(&host_config, "label = \"count-device\"\n[[games]]\nid = \"neverball\"\ntitle = \"Neverball\"\ncommand = [\"neverball\"]\n").unwrap();
        let backend = Arc::new(host::control::InMemoryLaunchUnitBackend::default());
        let runtime = host::HostRuntime::from_paths_with_backend(
            &host_config,
            Some(root.path().into()),
            root.path().join("private"),
            backend.clone(),
        )
        .with_route_registry(root.path().into(), registry);
        Self {
            root,
            runtime,
            backend,
        }
    }

    fn routers(&self, permission: PortalPermission) -> (Router, Router) {
        secure_host_routers(
            self.runtime.clone(),
            &self.root.path().join("private"),
            Some(PortalAccess::new(TOKEN, ORIGIN, permission)),
        )
    }

    fn bytes(&self) -> Vec<u8> {
        std::fs::read(self.root.path().join("device.yaml")).unwrap()
    }
}

fn update(revision: &str, setting: &str, value: &str) -> Value {
    json!({"_tag":"system.settings.update","payload":{
        "expectedRevision":revision,"settingId":setting,"value":value
    }})
}

async fn request(
    app: &Router,
    request: Value,
    bearer: Option<&str>,
    origin: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/rpc")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(bearer) = bearer {
        builder = builder.header(header::AUTHORIZATION, bearer);
    }
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(request.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn rpc(app: &Router, request_body: Value) -> Value {
    let (status, value) = request(
        app,
        request_body,
        Some(&format!("Bearer {TOKEN}")),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}

async fn snapshot(app: &Router) -> Value {
    let value = rpc(app, json!({"_tag":"system.settings.snapshot","payload":{}})).await;
    assert_eq!(value["outcome"]["_tag"], "Ok", "{value}");
    value["outcome"]["payload"].clone()
}

fn assert_error(value: &Value, code: &str) {
    assert_eq!(value["outcome"]["_tag"], "Err", "{value}");
    assert_eq!(value["outcome"]["payload"]["code"], code, "{value}");
}

#[tokio::test]
async fn host_count_default_read_update_and_cas_use_real_device_config() {
    let fixture = Fixture::new();
    let (app, _) = fixture.routers(PortalPermission::LocalSessions);
    let before = snapshot(&app).await;
    assert_eq!(before["playerCount"], 4);
    assert_eq!(before["editableSettingIds"], json!([COUNT]));
    assert!(before["plugins"].as_array().is_some());
    assert_eq!(before["steamGridDbCredential"], "NotConfigured");
    let revision = before["revision"].as_str().unwrap();
    let updated = rpc(&app, update(revision, COUNT, "6")).await;
    assert_eq!(updated["outcome"]["_tag"], "Ok", "{updated}");
    assert_eq!(updated["outcome"]["payload"]["playerCount"], 6);
    assert_eq!(
        updated["outcome"]["payload"]["editableSettingIds"],
        json!([COUNT])
    );
    assert_eq!(snapshot(&app).await, updated["outcome"]["payload"]);
    let bytes = fixture.bytes();
    assert!(String::from_utf8_lossy(&bytes).contains("playerCount: 6"));
    let stale = rpc(&app, update(revision, COUNT, "8")).await;
    assert_error(&stale, "SettingsConflict");
    assert_eq!(fixture.bytes(), bytes);

    // An external edit also invalidates the returned revision.
    let revision = updated["outcome"]["payload"]["revision"].as_str().unwrap();
    let mut external = bytes.clone();
    external.extend_from_slice(b"\n# external edit\n");
    std::fs::write(fixture.root.path().join("device.yaml"), &external).unwrap();
    assert_error(
        &rpc(&app, update(revision, COUNT, "7")).await,
        "SettingsConflict",
    );
    assert_eq!(fixture.bytes(), external);
    for count in [1, 255] {
        let current = snapshot(&app).await;
        let updated = rpc(
            &app,
            update(
                current["revision"].as_str().unwrap(),
                COUNT,
                &count.to_string(),
            ),
        )
        .await;
        assert_eq!(updated["outcome"]["_tag"], "Ok", "{updated}");
        assert_eq!(updated["outcome"]["payload"]["playerCount"], count);
    }
}

#[tokio::test]
async fn host_count_invalid_values_and_unrelated_settings_leave_bytes_unchanged() {
    let fixture = Fixture::new();
    let (app, _) = fixture.routers(PortalPermission::Full);
    let before = snapshot(&app).await;
    let revision = before["revision"].as_str().unwrap();
    let bytes = fixture.bytes();
    for value in ["", "0", "256", "-1", "1.5", "true", "four", " 4"] {
        assert_error(
            &rpc(&app, update(revision, COUNT, value)).await,
            "SettingsInvalid",
        );
        assert_eq!(fixture.bytes(), bytes);
    }
    for (setting, value) in [
        ("device-name", "changed"),
        ("@korri:mgba", "false"),
        ("host.preferences.playerCount.extra", "8"),
    ] {
        assert_error(
            &rpc(&app, update(revision, setting, value)).await,
            "OperationUnsupported",
        );
        assert_eq!(fixture.bytes(), bytes);
    }
    for request in [
        json!({"_tag":"system.settings.steamgriddbCredential.set","payload":{"token":"not-a-real-secret"}}),
        json!({"_tag":"system.settings.steamgriddbCredential.clear","payload":{}}),
    ] {
        assert_error(&rpc(&app, request).await, "OperationUnsupported");
    }
}

#[tokio::test]
async fn host_count_rejects_running_frozen_and_focus_failed_without_mutation() {
    let fixture = Fixture::new();
    let (app, _) = fixture.routers(PortalPermission::LocalSessions);
    let before = snapshot(&app).await;
    let revision = before["revision"].as_str().unwrap();
    let bytes = fixture.bytes();
    let prepared = rpc(
        &app,
        json!({"_tag":"app.session.prepare","payload":{"gameId":"neverball"}}),
    )
    .await;
    assert_eq!(prepared["outcome"]["_tag"], "Ok", "{prepared}");
    let launch = prepared["outcome"]["payload"]["launchId"].as_str().unwrap();
    let identity = fixture.root.path().join("private/host-session/active.json");
    let journal = std::fs::read(&identity).unwrap();
    for phase in ["running", "frozen", "focus-failed"] {
        if phase == "frozen" {
            let frozen = rpc(
                &app,
                json!({"_tag":"app.session.freeze","payload":{"expectedLaunchId":launch}}),
            )
            .await;
            assert_eq!(frozen["outcome"]["_tag"], "Ok", "{frozen}");
        }
        if phase == "focus-failed" {
            let thawed = rpc(
                &app,
                json!({"_tag":"app.session.thaw","payload":{"expectedLaunchId":launch}}),
            )
            .await;
            assert_eq!(thawed["outcome"]["_tag"], "Err", "{thawed}");
        }
        let status = rpc(&app, json!({"_tag":"app.session.status","payload":{}})).await;
        assert_eq!(
            status["outcome"]["payload"]["active"]["phase"], phase,
            "{status}"
        );
        let thaw_count = fixture.backend.thaw_count();
        // Availability is not authority or a promise that this write is idle.
        assert_eq!(snapshot(&app).await["editableSettingIds"], json!([COUNT]));
        let mut forged = update(revision, COUNT, "6");
        forged["payload"]["editableSettingIds"] = json!([COUNT]);
        assert_error(&rpc(&app, forged).await, "ActiveSessionConflict");
        assert_eq!(fixture.bytes(), bytes);
        assert_eq!(std::fs::read(&identity).unwrap(), journal);
        assert_eq!(fixture.backend.thaw_count(), thaw_count);
    }
    let stopped = rpc(
        &app,
        json!({"_tag":"app.session.stop","payload":{"expectedLaunchId":launch}}),
    )
    .await;
    assert_eq!(stopped["outcome"]["_tag"], "Ok", "{stopped}");
    let updated = rpc(&app, update(revision, COUNT, "6")).await;
    assert_eq!(updated["outcome"]["_tag"], "Ok", "{updated}");
}

#[tokio::test]
async fn host_count_uncertain_recovery_preserves_config_and_journal() {
    let fixture = Fixture::new();
    let (app, _) = fixture.routers(PortalPermission::LocalSessions);
    let before = snapshot(&app).await;
    let bytes = fixture.bytes();
    let directory = fixture.root.path().join("private/host-session");
    std::fs::create_dir_all(&directory).unwrap();
    let journal = directory.join("active.json");
    std::fs::write(&journal, b"unreadable recovery evidence").unwrap();
    assert_error(
        &rpc(
            &app,
            update(before["revision"].as_str().unwrap(), COUNT, "6"),
        )
        .await,
        "HostRecoveryBlocked",
    );
    assert_eq!(fixture.bytes(), bytes);
    assert_eq!(
        std::fs::read(&journal).unwrap(),
        b"unreadable recovery evidence"
    );
}

#[tokio::test]
async fn host_count_capability_is_exact_and_keeps_bearer_origin_checks() {
    let fixture = Fixture::new();
    let (app, _) = fixture.routers(PortalPermission::LocalSessions);
    let before = snapshot(&app).await;
    let revision = before["revision"].as_str().unwrap();
    let bytes = fixture.bytes();
    for setting in [
        "device-name",
        "@korri:mgba",
        "host.preferences.playerCount.extra",
        "host.preferences.playercount",
    ] {
        let (status, _) = request(
            &app,
            update(revision, setting, "6"),
            Some(&format!("Bearer {TOKEN}")),
            Some(ORIGIN),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    // Native callers may still omit Origin, but never the capability.
    let bearer = format!("Bearer {TOKEN}");
    for (token, origin, expected) in [
        (None, Some(ORIGIN), StatusCode::UNAUTHORIZED),
        (Some("Bearer wrong"), Some(ORIGIN), StatusCode::UNAUTHORIZED),
        (
            Some(bearer.as_str()),
            Some("https://foreign.example"),
            StatusCode::FORBIDDEN,
        ),
    ] {
        assert_eq!(
            request(&app, update(revision, COUNT, "6"), token, origin)
                .await
                .0,
            expected
        );
    }
    for duplicate in [header::AUTHORIZATION, header::ORIGIN] {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/rpc")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, &bearer)
            .header(header::ORIGIN, ORIGIN);
        let (value, expected) = if duplicate == header::AUTHORIZATION {
            (bearer.as_str(), StatusCode::UNAUTHORIZED)
        } else {
            (ORIGIN, StatusCode::FORBIDDEN)
        };
        builder = builder.header(duplicate, value);
        let response = app
            .clone()
            .oneshot(
                builder
                    .body(Body::from(update(revision, COUNT, "6").to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    let (read_only, _) = fixture.routers(PortalPermission::ReadOnly);
    assert_eq!(snapshot(&read_only).await["playerCount"], 4);
    assert_eq!(
        request(
            &read_only,
            update(revision, COUNT, "6"),
            Some(&bearer),
            Some(ORIGIN)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(fixture.bytes(), bytes);
    let (status, updated) = request(&app, update(revision, COUNT, "6"), Some(&bearer), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["outcome"]["_tag"], "Ok", "{updated}");
}

#[tokio::test]
async fn brain_count_write_fails_closed_without_changing_existing_settings_behavior() {
    let root = tempfile::tempdir().unwrap();
    config::test_fixtures::gba(root.path());
    let app = test_router_with_capability_and_local_root(TOKEN, ORIGIN, root.path());
    let before = snapshot(&app).await;
    assert_eq!(before["playerCount"], 4);
    let bytes = std::fs::read(root.path().join("device.yaml")).unwrap();
    let revision = before["revision"].as_str().unwrap();
    assert_error(
        &rpc(&app, update(revision, COUNT, "6")).await,
        "HostRecoveryBlocked",
    );
    assert_eq!(
        std::fs::read(root.path().join("device.yaml")).unwrap(),
        bytes
    );
    let renamed = rpc(&app, update(revision, "device-name", "renamed")).await;
    assert_eq!(renamed["outcome"]["_tag"], "Ok", "{renamed}");
    assert_eq!(renamed["outcome"]["payload"]["deviceName"], "renamed");
    assert_eq!(renamed["outcome"]["payload"]["playerCount"], 4);
}

#[tokio::test]
async fn host_count_does_not_expand_peer_lan_or_private_control_authority() {
    let fixture = Fixture::new();
    let (app, private) = fixture.routers(PortalPermission::Full);
    let before = snapshot(&app).await;
    let revision = before["revision"].as_str().unwrap();
    let bytes = fixture.bytes();
    let (lan, _) = app_states(fixture.runtime.clone());
    for body in [
        update(revision, COUNT, "6"),
        json!({"_tag":"system.settings.snapshot","payload":{}}),
    ] {
        let (status, denied) = request(&private, body.clone(), None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_error(&denied, "OperationUnsupported");
        let plain_lan = plain_host_routers(fixture.runtime.clone()).0;
        assert_error(
            &request(&plain_lan, body.clone(), None, None).await.1,
            "OperationUnsupported",
        );
        for context in [
            authorization::AuthorizationContext::Peer(authorization::Principal::OwnerDevice {
                device_public_key: "peer".into(),
                owner_statement: "verified upstream".into(),
                owner_public_key: "owner".into(),
            }),
            authorization::AuthorizationContext::TrustReconciliation,
        ] {
            let response = dispatch(
                &lan,
                &context,
                serde_json::from_value(body.clone()).unwrap(),
            )
            .await
            .unwrap();
            assert_error(
                &serde_json::to_value(response).unwrap(),
                "OperationUnsupported",
            );
            assert!(editable_setting_ids(&lan, &context).is_empty());
        }
    }
    assert_eq!(fixture.bytes(), bytes);
}

#[cfg(test)]
#[path = "settings_editability_tests.rs"]
mod settings_editability_tests;
