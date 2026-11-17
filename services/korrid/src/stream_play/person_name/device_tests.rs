use super::*;
use crate::{
    authorization::AuthorizationContext, host::control::InMemoryLaunchUnitBackend,
    identity::OwnerStatementStatus, local_signer::LocalPersonSigner, recovery_words::RecoveryWords,
};
use std::{fs, process::Command};

#[test]
fn an_automatic_identity_shows_its_name_but_cannot_sign_a_new_one() {
    // Publication reads KORRID_RELAYS from the process. Isolate it rather
    // than change the environment of other Rust tests.
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "stream_play::person_name::device::tests::cli_person_name_dispatch_child",
            "--ignored",
            "--nocapture",
        ])
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
#[ignore = "subprocess helper; isolates the relay environment"]
async fn cli_person_name_dispatch_child() {
    let root = PathBuf::from(
        std::env::var_os("KORRI_PERSON_NAME_DISPATCH_TEST_ROOT")
            .expect("run this helper through its subprocess parent test"),
    );
    let private = root.join("device-private");
    let mut identity = DeviceIdentity::load_or_create(&private).unwrap();
    let device_key = identity.device_public_key().unwrap().to_owned();
    // The person key exists only while setup signs with it (ADR 0082). It
    // signed the owner statement and a name; then it is gone.
    let signed_name = {
        let local = LocalPersonSigner::from_words(&RecoveryWords::generate(), &device_key).unwrap();
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
            panic!("the words' key refused the owner binding")
        };
        identity
            .apply_signed_owner_binding(&binding, &owner_public_key, &signed_event_json)
            .unwrap();
        let PersonSignerState::Approved {
            signed_event_json, ..
        } = local
            .request(PersonSignerRequest {
                unsigned_event_template: template("Simon", 2).unwrap(),
            })
            .await
        else {
            panic!("the words' key refused the name")
        };
        identity.store_person_name(&signed_event_json).unwrap();
        signed_event_json
    };

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
    let cli = AuthorizationContext::LocalCli;
    let signed_record = private.join("identity/person-name.event.json");
    let initial = read(&state, &cli).await;
    assert_eq!(initial.person_name.as_deref(), Some("Simon"));
    assert!(
        !editable(&initial),
        "no device holds the person key, so no one can edit the name: {initial:?}"
    );
    assert_failure(
        write(&state, &cli, &initial.revision, "Changed").await,
        "PersonSignerUnavailable",
    );
    state.surface_host_access = Some(SurfaceHostAccess::new(
        "person-name-cap",
        "https://shell.example",
        SurfaceHostPermission::Full,
    ));
    let surface_host = read(&state, &AuthorizationContext::LocalSurfaceHost).await;
    assert_eq!(surface_host.person_name.as_deref(), Some("Simon"));
    assert!(!editable(&surface_host));
    assert_failure(
        write(
            &state,
            &AuthorizationContext::LocalSurfaceHost,
            &surface_host.revision,
            "Changed",
        )
        .await,
        "PersonSignerUnavailable",
    );
    assert_eq!(fs::read_to_string(&signed_record).unwrap(), signed_name);
    // This child owns its environment. Exercise the real idle publication path
    // over a simulated minute, not a separately retained test transport.
    use futures::{SinkExt, StreamExt};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio_tungstenite::{accept_async, tungstenite::Message};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("ws://{}", listener.local_addr().unwrap());
    std::env::set_var("KORRID_RELAYS", serde_json::json!([relay]).to_string());
    let connections = Arc::new(AtomicUsize::new(0));
    let count = connections.clone();
    let relaying = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            count.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let mut socket = accept_async(stream).await.unwrap();
                while let Some(Ok(Message::Text(text))) = socket.next().await {
                    let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
                    assert_eq!(frame[0], "EVENT");
                    socket
                        .send(Message::Text(
                            serde_json::json!(["OK", frame[1]["id"], true, ""])
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap();
                }
            });
        }
    });
    for tick in 0..3 {
        publish_local(&state).await;
        if tick < 2 {
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(30)).await;
            tokio::time::resume();
        }
    }
    relaying.abort();
    assert_eq!(
        connections.load(Ordering::SeqCst),
        1,
        "idle name publication recreated its relay transport"
    );
}
