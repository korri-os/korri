//! Caller-specific metadata uses real configuration and the existing RPC handlers.
use super::*;
use authorization::{AuthorizationContext, Principal};

const NAME: &str = config::settings::DEVICE_NAME_SETTING_ID;
const CREDENTIAL: &str = "steamgriddb-credential";
const SECRET: &str = "editability-test-credential-not-a-real-secret";

fn brain_state(root: &std::path::Path, permission: PortalPermission) -> AppState {
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
    state
}

fn assert_ids(snapshot: &Value, expected: &[&str]) {
    assert_eq!(
        snapshot["editableSettingIds"],
        json!(expected),
        "{snapshot}"
    );
    let plugins = snapshot["plugins"].as_array().unwrap();
    assert!(!plugins.is_empty(), "exercise installed plugin rows");
    for plugin in plugins {
        assert!(!snapshot["editableSettingIds"]
            .as_array()
            .unwrap()
            .contains(&plugin["id"]));
    }
    assert!(!snapshot.to_string().contains(SECRET));
}

#[tokio::test]
async fn host_metadata_reports_only_supported_and_authorized_handlers() {
    let fixture = Fixture::new();
    config::settings::set_steamgriddb_credential(&fixture.root.path().join("private"), SECRET)
        .unwrap();
    for (permission, ids) in [
        (PortalPermission::Full, vec![COUNT]),
        (PortalPermission::LocalSessions, vec![COUNT]),
        (PortalPermission::ReadOnly, vec![]),
    ] {
        let (app, _) = fixture.routers(permission);
        let before = snapshot(&app).await;
        assert_eq!(before["steamGridDbCredential"], "Configured");
        assert_ids(&before, &ids);
        let mut write = update(before["revision"].as_str().unwrap(), COUNT, "6");
        write["payload"]["editableSettingIds"] = json!([COUNT, NAME, CREDENTIAL]);
        let (status, result) =
            request(&app, write, Some(&format!("Bearer {TOKEN}")), Some(ORIGIN)).await;
        if permission == PortalPermission::ReadOnly {
            assert_eq!(status, StatusCode::FORBIDDEN);
        } else {
            assert_eq!(status, StatusCode::OK);
            assert_eq!(result["outcome"]["_tag"], "Ok", "{result}");
            assert_ids(&result["outcome"]["payload"], &ids);
        }
    }
    // The field is mandatory, including when the list is empty.
    let (app, _) = fixture.routers(PortalPermission::ReadOnly);
    let mut value = snapshot(&app).await;
    assert!(serde_json::from_value::<SettingsSnapshot>(value.clone()).is_ok());
    value.as_object_mut().unwrap().remove("editableSettingIds");
    assert!(serde_json::from_value::<SettingsSnapshot>(value).is_err());
}

#[tokio::test]
async fn brain_metadata_preserves_name_and_secret_writes_but_never_plugin_or_count_writes() {
    let root = tempfile::tempdir().unwrap();
    let state = brain_state(root.path(), PortalPermission::Full);
    let app = portal_router_from_state(state);
    let before = snapshot(&app).await;
    assert_ids(&before, &[NAME, CREDENTIAL]);
    let revision = before["revision"].as_str().unwrap();
    let renamed = rpc(&app, update(revision, NAME, "metadata-device")).await;
    assert_eq!(renamed["outcome"]["_tag"], "Ok", "{renamed}");
    assert_eq!(
        renamed["outcome"]["payload"]["deviceName"],
        "metadata-device"
    );
    assert_ids(&renamed["outcome"]["payload"], &[NAME, CREDENTIAL]);
    assert_error(
        &rpc(&app, update(revision, NAME, "stale")).await,
        "SettingsConflict",
    );
    let before = snapshot(&app).await;
    let revision = before["revision"].as_str().unwrap();
    let bytes = std::fs::read(root.path().join("device.yaml")).unwrap();
    let document: serde_yaml::Value = serde_yaml::from_slice(&bytes).unwrap();
    assert_eq!(document["host"]["title"].as_str(), Some("metadata-device"));
    for (id, value, code) in [
        (
            before["plugins"][0]["id"].as_str().unwrap(),
            "false",
            "SettingsInvalid",
        ),
        (COUNT, "6", "HostRecoveryBlocked"),
    ] {
        let mut forged = update(revision, id, value);
        forged["payload"]["editableSettingIds"] = json!([id]);
        assert_error(&rpc(&app, forged).await, code);
        assert_eq!(
            std::fs::read(root.path().join("device.yaml")).unwrap(),
            bytes
        );
    }
    for (body, status) in [
        (
            json!({"_tag":"system.settings.steamgriddbCredential.set","payload":{"token":SECRET}}),
            "Configured",
        ),
        (
            json!({"_tag":"system.settings.steamgriddbCredential.clear","payload":{}}),
            "NotConfigured",
        ),
    ] {
        let result = rpc(&app, body).await;
        assert_eq!(result["outcome"]["_tag"], "Ok", "{result}");
        assert_eq!(result["outcome"]["payload"]["status"], status);
        let current = snapshot(&app).await;
        assert_eq!(current["steamGridDbCredential"], status);
        assert_ids(&current, &[NAME, CREDENTIAL]);
        assert!(!result.to_string().contains(SECRET));
        assert_eq!(
            config::settings::read_steamgriddb_credential(&root.path().join("private"))
                .unwrap()
                .as_deref(),
            (status == "Configured").then_some(SECRET)
        );
    }
}

#[tokio::test]
async fn restricted_brain_metadata_and_forged_ids_cannot_grant_write_permission() {
    for permission in [PortalPermission::ReadOnly, PortalPermission::LocalSessions] {
        let root = tempfile::tempdir().unwrap();
        let state = brain_state(root.path(), permission);
        config::settings::set_steamgriddb_credential(&root.path().join("private"), SECRET).unwrap();
        let app = portal_router_from_state(state);
        // Even a forged snapshot request cannot select the returned permissions.
        let forged_snapshot = rpc(
            &app,
            json!({
                "_tag":"system.settings.snapshot",
                "payload":{"editableSettingIds":[NAME,CREDENTIAL,COUNT]}
            }),
        )
        .await;
        let before = &forged_snapshot["outcome"]["payload"];
        assert_ids(before, &[]);
        assert_eq!(before["steamGridDbCredential"], "Configured");
        let bytes = std::fs::read(root.path().join("device.yaml")).unwrap();
        let revision = before["revision"].as_str().unwrap();
        for mut body in [
            update(revision, NAME, "forged"),
            update(
                revision,
                before["plugins"][0]["id"].as_str().unwrap(),
                "false",
            ),
            json!({"_tag":"system.settings.steamgriddbCredential.set","payload":{"token":"replacement"}}),
            json!({"_tag":"system.settings.steamgriddbCredential.clear","payload":{}}),
        ] {
            body["payload"]["editableSettingIds"] = json!([NAME, CREDENTIAL]);
            assert_eq!(
                request(&app, body, Some(&format!("Bearer {TOKEN}")), Some(ORIGIN))
                    .await
                    .0,
                StatusCode::FORBIDDEN
            );
        }
        let count = request(
            &app,
            update(revision, COUNT, "6"),
            Some(&format!("Bearer {TOKEN}")),
            Some(ORIGIN),
        )
        .await;
        if permission == PortalPermission::LocalSessions {
            assert_eq!(count.0, StatusCode::OK);
            assert_error(&count.1, "HostRecoveryBlocked");
        } else {
            assert_eq!(count.0, StatusCode::FORBIDDEN);
        }
        assert_eq!(
            std::fs::read(root.path().join("device.yaml")).unwrap(),
            bytes
        );
        assert_eq!(snapshot(&app).await["steamGridDbCredential"], "Configured");
        assert_eq!(
            config::settings::read_steamgriddb_credential(&root.path().join("private"))
                .unwrap()
                .as_deref(),
            Some(SECRET)
        );
    }
}

#[tokio::test]
async fn non_browser_brain_metadata_uses_dispatch_authority_not_portal_permission() {
    let root = tempfile::tempdir().unwrap();
    let mut state = brain_state(root.path(), PortalPermission::ReadOnly);
    let owner = AuthorizationContext::Peer(Principal::OwnerDevice {
        device_public_key: "peer".into(),
        owner_statement: "verified upstream".into(),
        owner_public_key: "owner".into(),
    });
    for context in [
        owner,
        AuthorizationContext::LocalUnixControl,
        AuthorizationContext::TrustReconciliation,
    ] {
        let response = dispatch(
            &state,
            &context,
            RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
        )
        .await
        .unwrap();
        let response = serde_json::to_value(response).unwrap();
        let before = &response["outcome"]["payload"];
        assert_ids(before, &[NAME, CREDENTIAL]);
        let response = dispatch(
            &state,
            &context,
            serde_json::from_value(update(
                before["revision"].as_str().unwrap(),
                NAME,
                "authorized-caller",
            ))
            .unwrap(),
        )
        .await
        .unwrap();
        let response = serde_json::to_value(response).unwrap();
        assert_eq!(response["outcome"]["_tag"], "Ok", "{response}");
        assert_ids(&response["outcome"]["payload"], &[NAME, CREDENTIAL]);
    }
    // Full browser permission never turns an untrusted peer into an owner.
    state.portal_access = Some(PortalAccess::new(TOKEN, ORIGIN, PortalPermission::Full));
    let pass = authorization::PersonPass {
        event_id: "pass".into(),
        event_json: "verified upstream".into(),
        expires_at: u64::MAX,
        scopes: vec![
            authorization::Scope::CatalogRead,
            authorization::Scope::StreamLaunch,
        ],
        person_public_key: "person".into(),
    };
    for principal in [
        Principal::Unknown {
            device_public_key: "peer".into(),
        },
        Principal::Guest {
            device_public_key: "peer".into(),
            owner_statement: "verified upstream".into(),
            person_pass: pass.clone(),
        },
        Principal::Household {
            device_public_key: "peer".into(),
            owner_statement: "verified upstream".into(),
            person_pass: pass,
        },
    ] {
        let context = AuthorizationContext::Peer(principal);
        assert!(editable_setting_ids(&state, &context).is_empty());
        for body in [
            json!({"_tag":"system.settings.snapshot","payload":{"editableSettingIds":[NAME,CREDENTIAL]}}),
            update("forged-revision", NAME, "forged"),
            json!({"_tag":"system.settings.steamgriddbCredential.clear","payload":{}}),
        ] {
            assert!(
                dispatch(&state, &context, serde_json::from_value(body).unwrap())
                    .await
                    .is_err()
            );
        }
    }
}
