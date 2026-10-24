use super::*;
use crate::{
    authorization::{AuthorizationContext, Principal},
    host::control::InMemoryLaunchUnitBackend,
    identity::OwnerStatementStatus,
    local_signer::{serve_connection, LocalPersonSigner},
};
use std::{fs, process::Command};

#[test]
fn local_cli_person_name_dispatch_preserves_signer_authority_and_cas() {
    // The real signer selector reads process configuration. Isolate it rather
    // than change the environment of other Rust tests or inject a signing bypass.
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "stream_play::person_name::device::tests::cli_person_name_dispatch_child",
            "--ignored",
            "--nocapture",
        ])
        .env(
            "KORRID_LOCAL_SIGNER_SOCKET",
            root.path().join("signer.sock"),
        )
        .env("KORRI_PERSON_NAME_DISPATCH_TEST_ROOT", root.path())
        .env_remove("KORRID_RELAYS")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed; 0 failed"),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn read(state: &AppState, context: &AuthorizationContext) -> SettingsSnapshot {
    let response = crate::dispatch(
        state,
        context,
        RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
    )
    .await
    .unwrap();
    match response {
        RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(snapshot)) => snapshot,
        other => panic!("settings read failed: {other:?}"),
    }
}
async fn write(
    state: &AppState,
    context: &AuthorizationContext,
    revision: &str,
    value: &str,
) -> SettingsUpdateOutcome {
    let response = crate::dispatch(
        state,
        context,
        RpcRequest::SettingsUpdate(SettingsUpdateRequest {
            expected_revision: revision.into(),
            setting_id: SETTING_ID.into(),
            value: value.into(),
        }),
    )
    .await
    .unwrap();
    match response {
        RpcResponse::SettingsUpdate(outcome) => outcome,
        other => panic!("wrong update reply: {other:?}"),
    }
}
fn assert_failure(outcome: SettingsUpdateOutcome, code: &str) {
    assert!(
        matches!(&outcome, SettingsUpdateOutcome::Err(error) if error.code == code),
        "{outcome:?}"
    );
}
fn editable(snapshot: &SettingsSnapshot) -> bool {
    snapshot
        .editable_setting_ids
        .iter()
        .any(|id| id == SETTING_ID)
}

#[tokio::test]
#[ignore = "subprocess helper; isolates the actual signer environment"]
async fn cli_person_name_dispatch_child() {
    let socket = PathBuf::from(std::env::var_os("KORRID_LOCAL_SIGNER_SOCKET").unwrap());
    let root = PathBuf::from(
        std::env::var_os("KORRI_PERSON_NAME_DISPATCH_TEST_ROOT")
            .expect("run this helper through its subprocess parent test"),
    );
    assert_eq!(socket, root.join("signer.sock"));
    let private = root.join("device-private");
    let signer_root = root.join("signer-private");
    let mut identity = DeviceIdentity::load_or_create(&private).unwrap();
    let device_key = identity.device_public_key().unwrap().to_owned();
    let local = LocalPersonSigner::load_or_create(&signer_root, &device_key).unwrap();
    let binding = identity
        .owner_statement_template(OwnerStatementStatus::Owned, 1)
        .unwrap();
    let PersonSignerState::Approved {
        owner_public_key,
        signed_event_json,
        ..
    } = local
        .request(PersonSignerRequest {
            unsigned_event_template: binding.clone(),
        })
        .await
    else {
        panic!("actual local signer refused owner binding")
    };
    identity
        .apply_signed_owner_binding(&binding, &owner_public_key, &signed_event_json)
        .unwrap();
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let expected = Arc::new(Mutex::new(device_key));
    let public = root.join("person.pub");
    let server = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            serve_connection(
                stream,
                signer_root.clone(),
                expected.clone(),
                public.clone(),
            )
            .await
            .unwrap();
        }
    });

    let config = root.join("host.toml");
    fs::write(&config, "label = \"person-name-test\"\n").unwrap();
    let library = root.join("library");
    fs::create_dir(&library).unwrap();
    let registry = crate::plugin_test_fixtures::installed(&library);
    let runtime = host::HostRuntime::from_paths_with_backend(
        &config,
        None,
        private.clone(),
        Arc::new(InMemoryLaunchUnitBackend::default()),
    )
    .with_route_registry(library, registry);
    let (mut state, _) = crate::app_states(runtime);
    assert!(
        state.portal_access.is_none(),
        "the person socket needs no browser access object"
    );
    let cli = AuthorizationContext::LocalCli;
    let initial = read(&state, &cli).await;
    assert!(
        editable(&initial),
        "ready signer must advertise person-name to LocalCli: {initial:?}"
    );
    assert!(initial.person_name.is_none());
    let SettingsUpdateOutcome::Ok(updated) = write(&state, &cli, &initial.revision, "Simon").await
    else {
        panic!("LocalCli name write failed")
    };
    assert_eq!(updated.person_name.as_deref(), Some("Simon"));
    assert!(editable(&updated));
    let signed_record = private.join("identity/person-name.event.json");
    let signed_bytes = fs::read(&signed_record).unwrap();
    let event = DeviceIdentity::load_or_create(&private)
        .unwrap()
        .person_name_event()
        .unwrap();
    assert_eq!(
        verified_name(&event, &owner_public_key).as_deref(),
        Some("Simon")
    );
    assert_failure(
        write(&state, &cli, "unobserved-stale-revision", "Not signed").await,
        "SettingsConflict",
    );
    assert_eq!(fs::read(&signed_record).unwrap(), signed_bytes);
    assert_failure(
        write(&state, &cli, &updated.revision, &"x".repeat(65)).await,
        "SettingsInvalid",
    );
    assert_eq!(fs::read(&signed_record).unwrap(), signed_bytes);

    let peer = AuthorizationContext::Peer(Principal::OwnerDevice {
        device_public_key: "33".repeat(32),
        owner_public_key: owner_public_key.clone(),
        owner_statement: "verified".into(),
    });
    for denied in [AuthorizationContext::LocalUnixControl, peer.clone()] {
        assert_failure(
            write(&state, &denied, &updated.revision, "Denied").await,
            "OperationUnsupported",
        );
    }
    assert!(!editable(&read(&state, &peer).await));
    assert_failure(
        write(
            &state,
            &AuthorizationContext::LocalBrowser,
            &updated.revision,
            "Denied",
        )
        .await,
        "OperationUnsupported",
    );
    state.portal_access = Some(PortalAccess::new(
        "person-name-cap",
        "https://portal.example",
        PortalPermission::ReadOnly,
    ));
    assert!(!editable(
        &read(&state, &AuthorizationContext::LocalBrowser).await
    ));
    assert_failure(
        write(
            &state,
            &AuthorizationContext::LocalBrowser,
            &updated.revision,
            "Denied",
        )
        .await,
        "OperationUnsupported",
    );
    assert!(
        editable(&read(&state, &cli).await),
        "browser restrictions must not demote the CLI"
    );
    assert_eq!(fs::read(&signed_record).unwrap(), signed_bytes);
    // LocalSessions already permits person-name, unlike other owner settings.
    for permission in [PortalPermission::LocalSessions, PortalPermission::Full] {
        state.portal_access = Some(PortalAccess::new(
            "person-name-cap",
            "https://portal.example",
            permission,
        ));
        let browser = read(&state, &AuthorizationContext::LocalBrowser).await;
        assert_eq!(browser.person_name, read(&state, &cli).await.person_name);
        assert!(editable(&browser));
        let SettingsUpdateOutcome::Ok(browser_update) = write(
            &state,
            &AuthorizationContext::LocalBrowser,
            &browser.revision,
            "Browser name",
        )
        .await
        else {
            panic!("existing {permission:?} browser name write regressed")
        };
        assert_eq!(browser_update.person_name.as_deref(), Some("Browser name"));
        assert_eq!(
            read(&state, &cli).await.person_name,
            browser_update.person_name
        );
    }

    server.abort();
    let _ = server.await;
    let before_unavailable = fs::read(&signed_record).unwrap();
    let unavailable = read(&state, &cli).await;
    assert_eq!(unavailable.person_name.as_deref(), Some("Browser name"));
    assert!(
        !editable(&unavailable),
        "unavailable signer must remove the edit affordance"
    );
    assert_failure(
        write(&state, &cli, &unavailable.revision, "No signer").await,
        "PersonSignerUnavailable",
    );
    assert_eq!(fs::read(&signed_record).unwrap(), before_unavailable);
}
