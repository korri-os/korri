#[path = "fixtures/readable.rs"]
mod readable;
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use korrid::config::settings::{
    clear_steamgriddb_credential, read_sensitive, set_steamgriddb_credential, SecretSettingStatus,
};
use korrid::config::snapshot::{DEVICE_FILE_NAME, GAMES_FILE_NAME};
use serde_json::Value;
use tower::ServiceExt;

fn readable_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(DEVICE_FILE_NAME), "host:\n  title: usu\n").unwrap();
    readable::write(root.path().join(GAMES_FILE_NAME), "{}\n").unwrap();
    root
}

async fn rpc_body(app: axum::Router, body: Value) -> Value {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rpc")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer test-cap")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

#[test]
fn steamgriddb_secret_repository_is_write_only_and_crash_safe() {
    let private = tempfile::tempdir().unwrap();
    assert_eq!(
        read_sensitive(private.path())
            .unwrap()
            .steam_grid_db_credential,
        SecretSettingStatus::NotConfigured
    );

    set_steamgriddb_credential(private.path(), "sgdb-secret-token").unwrap();
    assert_eq!(
        read_sensitive(private.path())
            .unwrap()
            .steam_grid_db_credential,
        SecretSettingStatus::Configured
    );

    let all_private_bytes = std::fs::read_dir(private.path())
        .unwrap()
        .map(|entry| std::fs::read(entry.unwrap().path()).unwrap())
        .flatten()
        .collect::<Vec<u8>>();
    assert!(String::from_utf8_lossy(&all_private_bytes).contains("sgdb-secret-token"));

    clear_steamgriddb_credential(private.path()).unwrap();
    assert_eq!(
        read_sensitive(private.path())
            .unwrap()
            .steam_grid_db_credential,
        SecretSettingStatus::NotConfigured
    );
    let remaining = std::fs::read_dir(private.path()).unwrap().count();
    assert_eq!(remaining, 0, "clear removes persisted secret bytes");
}

#[test]
fn sensitive_repository_reports_unavailable_private_root_without_token_leak() {
    let private_parent = tempfile::tempdir().unwrap();
    let not_a_directory = private_parent.path().join("state-file");
    std::fs::write(&not_a_directory, "not a dir").unwrap();

    let error = set_steamgriddb_credential(&not_a_directory, "sgdb-secret-token")
        .unwrap_err()
        .to_string();

    assert!(!error.contains("sgdb-secret-token"));
    assert!(!error.contains("Bearer"));
}

#[tokio::test]
async fn sensitive_rpc_actions_never_return_or_write_the_token_to_readable_config() {
    let readable = readable_root();
    let private = tempfile::tempdir().unwrap();
    let app = korrid::router_with_capability_and_roots(
        "test-cap",
        "https://appassets.androidplatform.net",
        readable.path(),
        private.path(),
    );

    let set = rpc_body(
        app.clone(),
        serde_json::json!({
            "_tag": "system.settings.steamgriddbCredential.set",
            "payload": { "token": "sgdb-secret-token" }
        }),
    )
    .await;
    assert_eq!(set["outcome"]["_tag"], "Ok");
    assert_eq!(set["outcome"]["payload"]["status"], "Configured");
    assert!(!set.to_string().contains("sgdb-secret-token"));

    let snapshot = rpc_body(
        app.clone(),
        serde_json::json!({"_tag":"system.settings.snapshot","payload":{}}),
    )
    .await;
    assert_eq!(
        snapshot["outcome"]["payload"]["steamGridDbCredential"],
        "Configured"
    );
    assert!(!snapshot.to_string().contains("sgdb-secret-token"));
    assert!(
        !std::fs::read_to_string(readable.path().join(DEVICE_FILE_NAME))
            .unwrap()
            .contains("sgdb-secret-token")
    );
    assert!(
        !std::fs::read_to_string(readable.path().join(GAMES_FILE_NAME))
            .unwrap()
            .contains("sgdb-secret-token")
    );

    let clear = rpc_body(
        app,
        serde_json::json!({
            "_tag": "system.settings.steamgriddbCredential.clear",
            "payload": {}
        }),
    )
    .await;
    assert_eq!(clear["outcome"]["_tag"], "Ok");
    assert_eq!(clear["outcome"]["payload"]["status"], "NotConfigured");
}

#[tokio::test]
async fn sensitive_rpc_does_not_participate_in_revision_conflicts() {
    let readable = readable_root();
    let private = tempfile::tempdir().unwrap();
    let app = korrid::router_with_capability_and_roots(
        "test-cap",
        "https://appassets.androidplatform.net",
        readable.path(),
        private.path(),
    );

    let before = rpc_body(
        app.clone(),
        serde_json::json!({"_tag":"system.settings.snapshot","payload":{}}),
    )
    .await;
    let revision = before["outcome"]["payload"]["revision"].as_str().unwrap();
    std::fs::write(
        readable.path().join(DEVICE_FILE_NAME),
        "host:\n  title: outside\n",
    )
    .unwrap();

    let sensitive = rpc_body(
        app.clone(),
        serde_json::json!({
            "_tag": "system.settings.steamgriddbCredential.set",
            "payload": { "token": "sgdb-secret-token" }
        }),
    )
    .await;
    assert_eq!(sensitive["outcome"]["_tag"], "Ok");

    let stale_update = rpc_body(
        app,
        serde_json::json!({
            "_tag":"system.settings.update",
            "payload": {"expectedRevision": revision, "settingId":"device-name", "value":"new"}
        }),
    )
    .await;
    assert_eq!(stale_update["outcome"]["_tag"], "Err");
    assert_eq!(
        stale_update["outcome"]["payload"]["code"],
        "SettingsConflict"
    );
}
