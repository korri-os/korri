use axum::{extract::State, routing::post, Json, Router};
use korrid::{identity::IdentityState, network::*, *};
use std::{
    process::{Command, Output},
    sync::{Arc, Mutex},
};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_korri"))
}

#[test]
fn help_version_and_completion_work_without_a_daemon() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec!["--help"],
        vec!["--version"],
        vec!["version"],
        vec!["completion", "bash"],
        vec!["completion", "zsh"],
        vec!["completion", "fish"],
        vec!["completion", "elvish"],
        vec!["completion", "powershell"],
    ] {
        let output = binary()
            .args(&args)
            .env("KORRI_CLI_SOCKET", root.path().join("missing.sock"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    let output = binary()
        .args(["completion", "bash", "--json"])
        .env("KORRI_CLI_SOCKET", root.path().join("missing.sock"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let script: String = serde_json::from_slice(&output.stdout).unwrap();
    assert!(script.contains("korri"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn unreachable_and_usage_have_distinct_exits_and_json_stderr() {
    let root = tempfile::tempdir().unwrap();
    let output = binary()
        .args(["status", "--json", "--socket"])
        .arg(root.path().join("missing.sock"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    let failure: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(failure.code, "KorridUnreachable");
    let output = binary().args(["not-a-command", "--json"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let failure: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(failure.code, "Usage");
    let output = binary()
        .args(["devices", "rename", "--yes"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

type RecordedCalls = Arc<Mutex<Vec<(Option<String>, RpcRequest)>>>;

#[derive(Clone)]
struct RpcState {
    calls: RecordedCalls,
    attention_failure: bool,
    ambiguous_device: bool,
}

fn settings(revision: &str, name: &str) -> SettingsSnapshot {
    SettingsSnapshot {
        revision: revision.into(),
        device_name: Some(name.into()),
        person_name: None,
        player_count: 1,
        editable_setting_ids: vec![config::settings::DEVICE_NAME_SETTING_ID.into()],
        plugins: vec![],
        steam_grid_db_credential: config::settings::SecretSettingStatus::NotConfigured,
    }
}

async fn respond(state: RpcState, device: Option<String>, request: RpcRequest) -> RpcResponse {
    state.calls.lock().unwrap().push((device, request.clone()));
    match request {
        RpcRequest::Health(_) => RpcResponse::Health(HealthOutcome::Ok(Health {
            version: "tested-daemon".into(),
        })),
        RpcRequest::SettingsSnapshot(_) => RpcResponse::SettingsSnapshot(
            SettingsSnapshotOutcome::Ok(settings("before", "Current")),
        ),
        RpcRequest::SettingsUpdate(request) => {
            assert_eq!(request.expected_revision, "before");
            assert_eq!(request.setting_id, config::settings::DEVICE_NAME_SETTING_ID);
            RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Ok(settings(
                "after",
                &request.value,
            )))
        }
        RpcRequest::IdentityStatus(_) => {
            RpcResponse::IdentityStatus(IdentityStatusOutcome::Ok(IdentityStatus {
                can_switch_identity: true,
                identity: IdentityState::Owned {
                    device_public_key: "11".repeat(32),
                    owner_public_key: "22".repeat(32),
                    event_id: "33".repeat(32),
                    created_at: 1,
                },
                local_backup_available: false,
                retired_public_keys: vec![],
            }))
        }
        RpcRequest::SessionStatus(_) => {
            RpcResponse::SessionStatus(SessionStatusOutcome::Ok(SessionStatus {
                netplay: None,
                stream_play_launch_id: None,
                stream_play_ended: None,
                active: None,
                overlay: None,
                pending_launches: None,
                observation_failure: None,
            }))
        }
        RpcRequest::PeerList(_) => RpcResponse::PeerList(PeerListOutcome::Ok(PeerList {
            peers: vec![PeerListEntry {
                device_public_key: "44".repeat(32),
                label: if state.ambiguous_device {
                    "Current"
                } else {
                    "Other"
                }
                .into(),
                state: PeerListState::Ready,
                updated_at: 1u32.into(),
                last_error: None,
            }],
        })),
        RpcRequest::AttentionList(_) if state.attention_failure => {
            RpcResponse::AttentionList(AttentionOutcome::Err(RpcFailure {
                code: "AttentionUnavailable".into(),
                message: "Attention could not be read.".into(),
            }))
        }
        RpcRequest::AttentionList(_) => {
            RpcResponse::AttentionList(AttentionOutcome::Ok(AttentionList { items: vec![] }))
        }
        RpcRequest::NetworkStatus(_) => {
            RpcResponse::NetworkStatus(NetworkStatusOutcome::Ok(NetworkSnapshot {
                connection: NetworkConnection::Wifi {
                    ssid: "Test network".into(),
                    strength_percent: Some(80),
                },
                wifi: Wifi::NoAdapter,
            }))
        }
        _ => panic!("unexpected request"),
    }
}

struct RpcServer {
    root: tempfile::TempDir,
    state: RpcState,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl RpcServer {
    fn start(attention_failure: bool) -> Self {
        Self::configured(attention_failure, false)
    }
    fn configured(attention_failure: bool, ambiguous_device: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("cli.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let state = RpcState {
            calls: Arc::new(Mutex::new(vec![])),
            attention_failure,
            ambiguous_device,
        };
        let serving = state.clone();
        let (shutdown, stopped) = tokio::sync::oneshot::channel();
        let thread = std::thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(async move {
                let app = Router::new()
                    .route("/rpc", post(|State(state): State<RpcState>, Json(request): Json<RpcRequest>| async { Json(respond(state, None, request).await) }))
                    .route("/forward", post(|State(state): State<RpcState>, Json(request): Json<korrid::cli::ForwardRequest>| async { Json(respond(state, Some(request.device_public_key), request.request).await) }))
                    .with_state(serving);
                axum::serve(tokio::net::UnixListener::from_std(listener).unwrap(), app)
                    .with_graceful_shutdown(async { let _ = stopped.await; }).await.unwrap();
            });
        });
        Self {
            root,
            state,
            shutdown: Some(shutdown),
            thread: Some(thread),
        }
    }
    fn run(&self, args: &[&str]) -> Output {
        binary()
            .args(args)
            .arg("--socket")
            .arg(self.root.path().join("cli.sock"))
            .output()
            .unwrap()
    }
}
impl Drop for RpcServer {
    fn drop(&mut self) {
        let _ = self.shutdown.take().unwrap().send(());
        self.thread.take().unwrap().join().unwrap();
    }
}

#[test]
fn status_keeps_all_seven_wire_responses_and_partial_failure() {
    let server = RpcServer::start(true);
    let output = server.run(&["status", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let responses: Vec<RpcResponse> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(responses.len(), 7);
    assert!(
        matches!(&responses[5], RpcResponse::AttentionList(AttentionOutcome::Err(error)) if error.code == "AttentionUnavailable")
    );
    assert_eq!(server.state.calls.lock().unwrap().len(), 7);
    let output = server.run(&["status"]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("tested-daemon"));
    assert!(text.contains("Attention: unknown"));
    assert!(text.contains("reachability unknown"));
    assert!(text.contains("Address: unknown"));
    assert!(text.contains("Test network"));
}

#[test]
fn devices_list_marks_current_and_json_is_the_three_existing_responses() {
    let server = RpcServer::start(false);
    let output = server.run(&["devices", "list", "--json"]);
    assert!(output.status.success());
    let responses: Vec<RpcResponse> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(responses.len(), 3);
    let output = server.run(&["devices", "list"]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("* Current"));
    assert!(text.contains("(current)"));
    assert!(text.contains("Other"));
}

#[test]
fn rename_uses_selected_device_revision_and_prints_only_the_final_response() {
    let server = RpcServer::start(false);
    let output = server.run(&["devices", "rename", "oThEr", "New name", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: RpcResponse = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        matches!(response, RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Ok(settings)) if settings.device_name.as_deref() == Some("New name"))
    );
    let calls = server.state.calls.lock().unwrap();
    assert_eq!(calls.len(), 5);
    assert_eq!(calls[3].0, Some("44".repeat(32)));
    assert_eq!(calls[4].0, Some("44".repeat(32)));
}

#[test]
fn device_picker_requires_explicit_selection_and_both_terminals() {
    for (answer, mode, json, selected) in [
        ("1", "terminal", false, Some(None)),
        ("2", "terminal", false, Some(Some("44".repeat(32)))),
        ("/4444\n1", "terminal", false, Some(Some("44".repeat(32)))),
        ("q", "terminal", false, None),
        ("", "terminal", true, None),
        ("", "stdin-pipe", false, None),
        ("", "stdout-pipe", false, None),
    ] {
        let server = RpcServer::configured(false, true);
        let mut command = Command::new("python3");
        command
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/play-library-terminal.py"
            ))
            .arg(answer)
            .arg(env!("CARGO_BIN_EXE_korri"))
            .args([
                "--device",
                "CURRENT",
                "devices",
                "rename",
                "Chosen name",
                "--socket",
            ])
            .arg(server.root.path().join("cli.sock"))
            .env("KORRI_PICKER_LABEL", "device")
            .env("KORRI_PICKER_MODE", mode);
        if json {
            command.arg("--json");
        }
        let prompts = !json && mode == "terminal";
        if !prompts {
            command.env("KORRI_PICKER_NO_PROMPT", "1");
        }
        let output = command.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(if selected.is_some() { 0 } else { 1 }),
            "{answer}/{mode}/{json}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let calls = server.state.calls.lock().unwrap();
        assert_eq!(calls.len(), if selected.is_some() { 5 } else { 3 });
        if let Some(selected) = selected {
            assert_eq!(calls[3].0, selected);
            assert_eq!(calls[4].0, selected);
            assert!(
                matches!(&calls[4].1, RpcRequest::SettingsUpdate(r) if r.value == "Chosen name")
            );
        } else if !prompts {
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(text.contains("ambiguous"));
            assert!(text.contains(&"11".repeat(32)));
            assert!(text.contains(&"44".repeat(32)));
            assert!(!text.contains("Choose device"));
            if json {
                assert!(text.contains("DeviceAmbiguous"));
            }
        }
    }
}

#[test]
fn device_picker_piped_ambiguity_lists_matches_without_effects() {
    let server = RpcServer::configured(false, true);
    let output = server.run(&["--device", "Current", "devices", "rename", "New", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let failure: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(failure.code, "DeviceAmbiguous");
    assert!(failure.message.contains(&"11".repeat(32)));
    assert!(failure.message.contains(&"44".repeat(32)));
    assert_eq!(server.state.calls.lock().unwrap().len(), 3);
}

#[test]
fn internal_plugin_prepare_never_appears_in_person_help() {
    let output = binary().arg("--help").output().unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("plugin-prepare"));
    let output = binary().arg("plugin-prepare").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}
