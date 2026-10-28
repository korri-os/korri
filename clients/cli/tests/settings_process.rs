use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use korrid::*;
use std::{
    collections::VecDeque,
    io::{self, Write},
    process::{Command, Output, Stdio},
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct Exchange {
    request: RpcRequest,
    response: serde_json::Value,
    denied: bool,
}
#[derive(Clone)]
struct Service {
    exchanges: Arc<Mutex<VecDeque<Exchange>>>,
    calls: Arc<Mutex<Vec<RpcRequest>>>,
}
struct RpcServer {
    root: tempfile::TempDir,
    service: Service,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl RpcServer {
    fn start(exchanges: Vec<Exchange>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let listener =
            std::os::unix::net::UnixListener::bind(root.path().join("cli.sock")).unwrap();
        listener.set_nonblocking(true).unwrap();
        let service = Service {
            exchanges: Arc::new(Mutex::new(exchanges.into())),
            calls: Arc::new(Mutex::new(vec![])),
        };
        let serving = service.clone();
        let (shutdown, stopped) = tokio::sync::oneshot::channel();
        let thread = std::thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let app = Router::new()
                        .route(
                            "/rpc",
                            post(
                                |State(service): State<Service>,
                                 Json(request): Json<RpcRequest>| async move {
                                    service.calls.lock().unwrap().push(request.clone());
                                    let next = service
                                        .exchanges
                                        .lock()
                                        .unwrap()
                                        .pop_front()
                                        .expect("unexpected extra effect");
                                    assert_eq!(
                                        serde_json::to_value(request).unwrap(),
                                        serde_json::to_value(next.request).unwrap()
                                    );
                                    (
                                        if next.denied {
                                            StatusCode::FORBIDDEN
                                        } else {
                                            StatusCode::OK
                                        },
                                        Json(next.response),
                                    )
                                },
                            ),
                        )
                        .with_state(serving);
                    axum::serve(tokio::net::UnixListener::from_std(listener).unwrap(), app)
                        .with_graceful_shutdown(async {
                            let _ = stopped.await;
                        })
                        .await
                        .unwrap();
                });
        });
        Self {
            root,
            service,
            shutdown: Some(shutdown),
            thread: Some(thread),
        }
    }
    fn run(&self, args: &[&str], input: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_korri"))
            .args(args)
            .arg("--socket")
            .arg(self.root.path().join("cli.sock"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let written = child.stdin.take().unwrap().write_all(input.as_bytes());
        let output = child.wait_with_output().unwrap();
        if let Err(error) = written {
            // A usage refusal may exit before consuming piped input. No other
            // write error or child outcome may be hidden by this race handling.
            assert_eq!(error.kind(), io::ErrorKind::BrokenPipe, "{error}");
            assert_eq!(
                output.status.code(),
                Some(2),
                "stdin closed without a usage refusal"
            );
        }
        output
    }
    fn done(&self) {
        assert!(self.service.exchanges.lock().unwrap().is_empty());
    }
}
impl Drop for RpcServer {
    fn drop(&mut self) {
        let _ = self.shutdown.take().unwrap().send(());
        self.thread.take().unwrap().join().unwrap();
    }
}
fn exchange(request: RpcRequest, response: impl serde::Serialize) -> Exchange {
    Exchange {
        request,
        response: serde_json::to_value(response).unwrap(),
        denied: false,
    }
}
fn settings() -> SettingsSnapshot {
    SettingsSnapshot {
        revision: "exact-revision".into(),
        device_name: Some("Device".into()),
        person_name: None,
        player_count: 2,
        editable_setting_ids: vec![config::settings::PLAYER_COUNT_SETTING_ID.into()],
        plugins: vec![],
        steam_grid_db_credential: config::settings::SecretSettingStatus::Configured,
    }
}
fn assert_json(output: &Output, response: &RpcResponse, exit: i32) {
    assert_eq!(
        output.status.code(),
        Some(exit),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::to_value(response).unwrap()
    );
}
fn failure() -> RpcFailure {
    RpcFailure {
        code: "SettingsConflict".into(),
        message: "Settings changed; reload.".into(),
    }
}

#[test]
fn settings_seats_uses_exact_revision_and_never_retries_conflicts() {
    let response = RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Err(failure()));
    let server = RpcServer::start(vec![
        exchange(
            RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
            RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings())),
        ),
        exchange(
            RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                expected_revision: "exact-revision".into(),
                setting_id: config::settings::PLAYER_COUNT_SETTING_ID.into(),
                value: "4".into(),
            }),
            response.clone(),
        ),
    ]);
    assert_json(
        &server.run(&["controllers", "seats", "4", "--json"], ""),
        &response,
        1,
    );
    server.done();
    assert_eq!(server.service.calls.lock().unwrap().len(), 2);
}

#[test]
fn settings_seats_reads_without_inventing_controller_assignment() {
    let response = RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings()));
    let server = RpcServer::start(vec![exchange(
        RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
        response.clone(),
    )]);
    assert_json(
        &server.run(&["controllers", "seats", "--json"], ""),
        &response,
        0,
    );
    server.done();
}

#[test]
fn settings_passwords_only_use_explicit_private_stdin_and_network_rpc() {
    use korrid::network::*;
    for hidden in [false, true] {
        let request = if hidden {
            RpcRequest::WifiConnectHidden(WifiConnectHiddenRequest {
                ssid: "Hidden".into(),
                password: Some("private-password".into()),
            })
        } else {
            RpcRequest::WifiConnect(WifiConnectRequest {
                network_id: "exact-network-id".into(),
                password: Some("private-password".into()),
            })
        };
        let status = NetworkStatusOutcome::Ok(NetworkSnapshot {
            connection: NetworkConnection::Offline,
            wifi: Wifi::NoAdapter,
        });
        let response = if hidden {
            RpcResponse::WifiConnectHidden(status)
        } else {
            RpcResponse::WifiConnect(status)
        };
        let server = RpcServer::start(vec![exchange(request, response.clone())]);
        let mut args = vec![
            "wifi",
            "connect",
            if hidden { "Hidden" } else { "exact-network-id" },
            "--password-stdin",
            "--json",
        ];
        if hidden {
            args.push("--hidden");
        }
        let output = server.run(&args, "private-password\n");
        assert_json(&output, &response, 0);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("private-password"));
        server.done();
    }
}

#[test]
fn settings_authorization_refusal_never_tries_another_effect_path() {
    use korrid::network::*;
    let server = RpcServer::start(vec![Exchange {
        request: RpcRequest::WifiScan(WifiScanRequest {}),
        response: serde_json::to_value(RpcResponse::WifiScan(NetworkStatusOutcome::Err(failure())))
            .unwrap(),
        denied: true,
    }]);
    let output = server.run(&["wifi", "scan", "--json"], "");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error.code, "AuthorizationDenied");
    assert_eq!(server.service.calls.lock().unwrap().len(), 1);
    server.done();
}

const MISSING_INPUT: &str = "Missing input. Scripts must provide all inputs explicitly.";
const MISSING_PRIVATE_INPUT: &str =
    "Could not read private input. Use the explicit private stdin option or a terminal.";
const EARLY_REJECTIONS: &[(&[&str], &str)] = &[
    (&["wifi", "connect", "--json"], MISSING_INPUT),
    (
        &["wifi", "connect", "id", "--ask-password", "--json"],
        MISSING_PRIVATE_INPUT,
    ),
    (&["metadata", "key", "set", "--json"], MISSING_PRIVATE_INPUT),
    (
        &["saves", "restore", "game", "snapshot", "--json"],
        MISSING_INPUT,
    ),
    (
        &[
            "plugins",
            "repositories",
            "remove",
            "https://example.com/catalog",
            "--json",
        ],
        MISSING_INPUT,
    ),
    (
        &["identity", "switch", "local", "--yes", "--json"],
        MISSING_INPUT,
    ),
];

fn assert_early_rejection(server: &RpcServer, args: &[&str], input: &str, message: &str) {
    let output = server.run(args, input);
    assert_eq!(
        output.status.code(),
        Some(2),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let mut expected = serde_json::to_vec(&RpcFailure {
        code: "Usage".into(),
        message: message.into(),
    })
    .unwrap();
    expected.push(b'\n');
    assert_eq!(output.stderr, expected, "{args:?}");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("not-consented"));
    assert!(server.service.calls.lock().unwrap().is_empty());
}

#[test]
fn settings_missing_inputs_and_confirmations_do_not_prompt_when_piped() {
    let server = RpcServer::start(vec![]);
    for (args, message) in EARLY_REJECTIONS {
        assert_early_rejection(&server, args, "not-consented\n", message);
    }
}

#[test]
fn settings_repeated_early_rejection_handles_unconsumed_pipe_input() {
    let server = RpcServer::start(vec![]);
    // Exceed pipe capacity so fast rejection can close stdin during write_all.
    // This reproduces the release-package race without sleeps or scheduling assumptions.
    let input = "not-consented\n".repeat(256 * 1024);
    for _ in 0..32 {
        for (args, message) in EARLY_REJECTIONS {
            assert_early_rejection(&server, args, &input, message);
        }
    }
}

#[test]
fn settings_saves_keep_exact_producer_ids_and_confirm_both_writes() {
    use korrid::saves::types::*;
    for restore in [true, false] {
        let pick = SavePickRequest {
            game_id: "exact-game-id".into(),
            snapshot_id: "exact-snapshot-id".into(),
        };
        let result = SaveHistoryOutcome::Err(RpcFailure {
            code: "SaveInUse".into(),
            message: "Game is in use.".into(),
        });
        let (request, response, action) = if restore {
            (
                RpcRequest::SavesRestore(pick),
                RpcResponse::SavesRestore(result),
                "restore",
            )
        } else {
            (
                RpcRequest::SavesChoose(pick),
                RpcResponse::SavesChoose(result),
                "choose",
            )
        };
        let server = RpcServer::start(vec![exchange(request, response.clone())]);
        assert_json(
            &server.run(
                &[
                    "saves",
                    action,
                    "exact-game-id",
                    "exact-snapshot-id",
                    "--yes",
                    "--json",
                ],
                "",
            ),
            &response,
            1,
        );
        server.done();
    }
}

#[test]
fn settings_metadata_never_prints_the_credential() {
    let response = RpcResponse::SteamGridDbCredentialSet(SensitiveSettingOutcome::Ok(
        SensitiveSettingResult {
            status: config::settings::SecretSettingStatus::Configured,
        },
    ));
    let server = RpcServer::start(vec![exchange(
        RpcRequest::SteamGridDbCredentialSet(SteamGridDbCredentialSetRequest {
            token: "private-token".into(),
        }),
        response.clone(),
    )]);
    assert_json(
        &server.run(
            &["metadata", "key", "set", "--key-stdin", "--json"],
            "private-token\n",
        ),
        &response,
        0,
    );
    server.done();
}

fn publisher() -> korri::plugin_wire::PluginPublisher {
    serde_json::from_value(serde_json::json!({"namespace":"@test", "state":"Bound", "fingerprint":"test AbCd EfGh IjKl MnOp"})).unwrap()
}
fn inspection() -> korri::plugin_wire::PluginInspection {
    korri::plugin_wire::PluginInspection {
        id: "plugin".into(),
        title: Some("Plugin".into()),
        approval: "exact-digest".into(),
        warning: "Permission warning".into(),
        ports: plugin_admin::PluginPorts {
            allowed_tcp_ports: vec![80],
            allowed_udp_ports: vec![90],
        },
        native_units: vec!["plugin.service".into()],
        publisher: publisher(),
        brings: vec![korri::plugin_wire::PluginDependency {
            id: "dependency".into(),
            title: None,
            warning: "Dependency warning".into(),
            already_approved: false,
            publisher: publisher(),
        }],
    }
}
fn inspect_request() -> RpcRequest {
    RpcRequest::PluginInspect(plugin_admin::PluginInspectRequest {
        url: "https://example.com/catalog".into(),
        plugin_id: "plugin".into(),
        release_version: "1".into(),
    })
}
#[test]
fn settings_plugin_review_shows_every_permission_before_approval() {
    let response = korri::plugin_wire::RpcResponse::PluginInspect(korri::plugin_wire::Outcome::Ok(
        inspection(),
    ));
    let server = RpcServer::start(vec![exchange(inspect_request(), response)]);
    let output = server.run(
        &[
            "plugins",
            "review",
            "https://example.com/catalog",
            "plugin",
            "1",
        ],
        "",
    );
    assert!(output.status.success());
    let human = String::from_utf8(output.stdout).unwrap();
    for value in [
        "exact-digest",
        "Permission warning",
        "plugin.service",
        "80",
        "90",
        "dependency",
        "Dependency warning",
    ] {
        assert!(human.contains(value), "{value}: {human}");
    }
    server.done();
}

#[test]
fn settings_plugin_yes_never_fabricates_review_and_stale_digest_never_installs() {
    let server = RpcServer::start(vec![]);
    let output = server.run(
        &[
            "plugins",
            "install",
            "https://example.com/catalog",
            "plugin",
            "1",
            "--yes",
            "--json",
        ],
        "",
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(server.service.calls.lock().unwrap().is_empty());
    let server = RpcServer::start(vec![exchange(
        inspect_request(),
        korri::plugin_wire::RpcResponse::PluginInspect(korri::plugin_wire::Outcome::Ok(
            inspection(),
        )),
    )]);
    let output = server.run(
        &[
            "plugins",
            "install",
            "https://example.com/catalog",
            "plugin",
            "1",
            "--approval",
            "stale-digest",
            "--yes",
            "--json",
        ],
        "",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error.code, "PluginNeedsReview");
    server.done();
}

#[test]
fn settings_plugin_install_passes_exact_reviewed_digest_and_keeps_host_refusals() {
    let response = RpcResponse::PluginInstall(PluginChangeOutcome::Err(RpcFailure {
        code: "PluginHostRefused".into(),
        message: "Publisher signature refused.".into(),
    }));
    let server = RpcServer::start(vec![
        exchange(
            inspect_request(),
            korri::plugin_wire::RpcResponse::PluginInspect(korri::plugin_wire::Outcome::Ok(
                inspection(),
            )),
        ),
        exchange(
            RpcRequest::PluginInstall(plugin_admin::PluginInstallRequest {
                url: "https://example.com/catalog".into(),
                plugin_id: "plugin".into(),
                release_version: "1".into(),
                approval: "exact-digest".into(),
            }),
            response.clone(),
        ),
    ]);
    assert_json(
        &server.run(
            &[
                "plugins",
                "install",
                "https://example.com/catalog",
                "plugin",
                "1",
                "--approval",
                "exact-digest",
                "--json",
            ],
            "",
        ),
        &response,
        1,
    );
    server.done();
}

#[test]
fn settings_identity_switch_preserves_coordinator_disposition_preconditions() {
    let request = IdentityLocalSwitchRequest {
        encrypted_secret: "private-ciphertext".into(),
        password: "private-password".into(),
        data_disposition: IdentityDataDisposition::Delete,
        trust_loss_confirmed: false,
    };
    let input = serde_json::to_string(&request).unwrap();
    let response = RpcResponse::IdentityLocalSwitch(IdentitySwitchOutcome::Err(RpcFailure {
        code: "IdentitySwitchFailed".into(),
        message: "Confirm trust loss first.".into(),
    }));
    let server = RpcServer::start(vec![exchange(
        RpcRequest::IdentityLocalSwitch(request),
        response.clone(),
    )]);
    let output = server.run(
        &[
            "identity",
            "switch",
            "local",
            "--private-stdin",
            "--yes",
            "--json",
        ],
        &input,
    );
    assert_json(&output, &response, 1);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private-"));
    server.done();
}

#[test]
fn settings_identity_backup_delivers_only_requested_ciphertext() {
    let response = RpcResponse::IdentityBackupExport(IdentityBackupOutcome::Ok(IdentityBackup {
        encrypted_secret: "ncryptsec-encrypted-output".into(),
    }));
    let server = RpcServer::start(vec![exchange(
        RpcRequest::IdentityBackupExport(IdentityBackupExportRequest {
            password: "private-password".into(),
        }),
        response.clone(),
    )]);
    assert_json(
        &server.run(
            &["identity", "backup", "--password-stdin", "--json"],
            "private-password\n",
        ),
        &response,
        0,
    );
    server.done();
}

#[test]
fn settings_identity_reset_is_blocked_without_exclusive_writer_proof_and_never_touches_state() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let server = RpcServer::start(vec![]);
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("identity");
    std::fs::create_dir(&state).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = state.join("existing-private-state");
    std::fs::write(&path, "preserve this state").unwrap();
    let before = std::fs::metadata(&state).unwrap();
    for args in [
        vec!["identity", "reset", "--json"],
        vec!["identity", "reset", "--yes", "--json"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_korri"))
            .args(args)
            .env("KORRID_PRIVATE_STATE_ROOT", root.path())
            .env("HOME", root.path())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let error: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error.code, "IdentityRecoveryUnavailable");
    }
    let after = std::fs::metadata(&state).unwrap();
    assert_eq!(
        (before.uid(), before.gid(), before.mode()),
        (after.uid(), after.gid(), after.mode())
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "preserve this state"
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    assert!(server.service.calls.lock().unwrap().is_empty());
}

#[test]
fn settings_hidden_terminal_input_disables_echo_restores_it_and_keeps_secrets_out_of_argv() {
    let response = RpcResponse::SteamGridDbCredentialSet(SensitiveSettingOutcome::Ok(
        SensitiveSettingResult {
            status: config::settings::SecretSettingStatus::Configured,
        },
    ));
    let server = RpcServer::start(vec![exchange(
        RpcRequest::SteamGridDbCredentialSet(SteamGridDbCredentialSetRequest {
            token: "private-token".into(),
        }),
        response,
    )]);
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/settings-terminal.py"
        ))
        .arg(env!("CARGO_BIN_EXE_korri"))
        .arg(server.root.path().join("cli.sock"))
        .arg("hidden")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    server.done();
}

#[test]
fn settings_private_terminal_cancellation_restores_echo_without_rpc_or_secret_exposure() {
    let server = RpcServer::start(vec![]);
    for mode in ["sigint", "sigterm", "switch-sigterm"] {
        let output = Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/settings-terminal.py"
            ))
            .arg(env!("CARGO_BIN_EXE_korri"))
            .arg(server.root.path().join("cli.sock"))
            .arg(mode)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(server.service.calls.lock().unwrap().is_empty());
}

#[test]
fn settings_private_prompts_require_both_actual_terminals_even_with_explicit_stdin() {
    let server = RpcServer::start(vec![]);
    for mode in ["stdin-pipe", "stdout-pipe"] {
        let output = Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/settings-terminal.py"
            ))
            .arg(env!("CARGO_BIN_EXE_korri"))
            .arg(server.root.path().join("cli.sock"))
            .arg(mode)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(server.service.calls.lock().unwrap().is_empty());
}

#[test]
fn settings_seats_uses_the_producers_validation_before_contacting_korrid() {
    let server = RpcServer::start(vec![]);
    for count in ["0", "256", "not-a-count"] {
        let output = server.run(&["controllers", "seats", count, "--json"], "");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    assert!(server.service.calls.lock().unwrap().is_empty());
}

#[test]
fn settings_repository_and_enable_actions_keep_existing_typed_requests_and_responses() {
    use korrid::plugin_admin::*;
    let repositories = PluginRepositories {
        official: Some("https://example.com/official".into()),
        user_added: vec![],
        repository_installs: vec![],
        other_installs: vec![],
    };
    let cases = vec![
        (
            vec!["plugins", "list", "--json"],
            RpcRequest::PluginRepositories(PluginRepositoriesRequest {}),
            RpcResponse::PluginRepositories(PluginRepositoriesOutcome::Ok(repositories.clone())),
        ),
        (
            vec!["plugins", "repositories", "list", "--json"],
            RpcRequest::PluginRepositories(PluginRepositoriesRequest {}),
            RpcResponse::PluginRepositories(PluginRepositoriesOutcome::Ok(repositories)),
        ),
        (
            vec!["plugins", "list", "https://example.com/catalog", "--json"],
            RpcRequest::PluginCatalog(PluginRepositoryRequest {
                url: "https://example.com/catalog".into(),
            }),
            RpcResponse::PluginCatalog(PluginCatalogOutcome::Ok(PluginCatalog { entries: vec![] })),
        ),
        (
            vec![
                "plugins",
                "repositories",
                "add",
                "https://example.com/catalog",
                "--json",
            ],
            RpcRequest::PluginRepositoryAdd(PluginRepositoryRequest {
                url: "https://example.com/catalog".into(),
            }),
            RpcResponse::PluginRepositoryAdd(PluginRepositoryAddOutcome::Ok(
                PluginRepositoryAddResult::Added,
            )),
        ),
        (
            vec![
                "plugins",
                "repositories",
                "remove",
                "https://example.com/catalog",
                "--yes",
                "--json",
            ],
            RpcRequest::PluginRepositoryRemove(PluginRepositoryRequest {
                url: "https://example.com/catalog".into(),
            }),
            RpcResponse::PluginRepositoryRemove(PluginRepositoryRemoveOutcome::Ok(
                PluginRepositoryRemoveResult::Removed,
            )),
        ),
        (
            vec!["plugins", "on", "exact-plugin", "--json"],
            RpcRequest::PluginSetEnabled(PluginSetEnabledRequest {
                id: "exact-plugin".into(),
                enabled: true,
            }),
            RpcResponse::PluginSetEnabled(PluginChangeOutcome::Err(RpcFailure {
                code: CODE_NEEDS_REVIEW.into(),
                message: "Approval changed; review again.".into(),
            })),
        ),
        (
            vec!["plugins", "off", "exact-plugin", "--json"],
            RpcRequest::PluginSetEnabled(PluginSetEnabledRequest {
                id: "exact-plugin".into(),
                enabled: false,
            }),
            RpcResponse::PluginSetEnabled(PluginChangeOutcome::Ok(PluginChangeApplied {})),
        ),
    ];
    for (args, request, response) in cases {
        let exit = i32::from(korri::output::failure(&response).is_some());
        let server = RpcServer::start(vec![exchange(request, response.clone())]);
        assert_json(&server.run(&args, ""), &response, exit);
        server.done();
    }
}

#[test]
fn settings_saves_reads_use_only_existing_status_and_history() {
    use korrid::saves::types::*;
    for (args, request, response) in [
        (
            vec!["saves", "status", "--json"],
            RpcRequest::SavesStatus(SaveStatusRequest {}),
            RpcResponse::SavesStatus(SaveStatusOutcome::Ok(SaveStatus { games: vec![] })),
        ),
        (
            vec!["saves", "history", "game", "--json"],
            RpcRequest::SavesHistory(SaveGameRequest {
                game_id: "game".into(),
            }),
            RpcResponse::SavesHistory(SaveHistoryOutcome::Ok(SaveHistory {
                records: vec![],
                current: None,
            })),
        ),
    ] {
        let server = RpcServer::start(vec![exchange(request, response.clone())]);
        assert_json(&server.run(&args, ""), &response, 0);
        server.done();
    }
}

fn identity_status() -> IdentityStatus {
    IdentityStatus {
        can_switch_identity: true,
        identity: identity::IdentityState::Owned {
            device_public_key: "device-key".into(),
            owner_public_key: "person-key".into(),
            event_id: "event".into(),
            created_at: 1,
        },
        local_backup_available: true,
        retired_public_keys: vec!["retired-person-key".into()],
    }
}
#[test]
fn settings_identity_show_nip46_and_retired_operations_preserve_live_coordinator_contracts() {
    let server = RpcServer::start(vec![exchange(
        RpcRequest::IdentityStatus(IdentityStatusRequest {}),
        RpcResponse::IdentityStatus(IdentityStatusOutcome::Ok(identity_status())),
    )]);
    let output = server.run(&["identity", "show"], "");
    assert!(output.status.success());
    let human = String::from_utf8(output.stdout).unwrap();
    for value in [
        "device-key",
        "person-key",
        "retired-person-key",
        "available",
    ] {
        assert!(human.contains(value));
    }
    server.done();

    let request = IdentityNip46SwitchRequest {
        bunker_uri: "bunker://private-connection".into(),
        data_disposition: IdentityDataDisposition::Transfer,
        trust_loss_confirmed: true,
    };
    let input = serde_json::to_string(&request).unwrap();
    let response =
        RpcResponse::IdentityNip46Switch(IdentitySwitchOutcome::Ok(IdentitySwitchCompleted {
            owner_public_key: "replacement-person".into(),
        }));
    let server = RpcServer::start(vec![exchange(
        RpcRequest::IdentityNip46Switch(request),
        response.clone(),
    )]);
    let output = server.run(
        &[
            "identity",
            "switch",
            "nip46",
            "--private-stdin",
            "--yes",
            "--json",
        ],
        &input,
    );
    assert_json(&output, &response, 0);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private-connection"));
    server.done();

    let response = RpcResponse::IdentityRetiredExport(IdentityBackupOutcome::Ok(IdentityBackup {
        encrypted_secret: "ncryptsec-retired".into(),
    }));
    let server = RpcServer::start(vec![exchange(
        RpcRequest::IdentityRetiredExport(IdentityRetiredExportRequest {
            public_key: "retired-person-key".into(),
            password: "private-password".into(),
        }),
        response.clone(),
    )]);
    assert_json(
        &server.run(
            &[
                "identity",
                "backup",
                "--retired",
                "retired-person-key",
                "--password-stdin",
                "--json",
            ],
            "private-password\n",
        ),
        &response,
        0,
    );
    server.done();

    let response =
        RpcResponse::IdentityRetiredDelete(IdentityRetiredDeleteOutcome::Ok(identity_status()));
    let server = RpcServer::start(vec![exchange(
        RpcRequest::IdentityRetiredDelete(IdentityRetiredDeleteRequest {
            public_key: "retired-person-key".into(),
            backup_confirmed: true,
        }),
        response.clone(),
    )]);
    assert_json(
        &server.run(
            &[
                "identity",
                "retired",
                "delete",
                "retired-person-key",
                "--backup-confirmed",
                "--yes",
                "--json",
            ],
            "",
        ),
        &response,
        0,
    );
    server.done();
}

#[test]
fn settings_identity_delete_requires_backup_and_destruction_acknowledgements_separately() {
    let server = RpcServer::start(vec![]);
    for args in [
        vec!["identity", "retired", "delete", "key", "--yes", "--json"],
        vec![
            "identity",
            "retired",
            "delete",
            "key",
            "--backup-confirmed",
            "--json",
        ],
    ] {
        let output = server.run(&args, "");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    assert!(server.service.calls.lock().unwrap().is_empty());
}

#[test]
fn settings_invalid_private_input_and_rejected_secret_arguments_are_never_echoed() {
    let server = RpcServer::start(vec![]);
    for args in [
        vec![
            "identity",
            "switch",
            "local",
            "--private-stdin",
            "--yes",
            "--json",
        ],
        vec![
            "identity",
            "switch",
            "nip46",
            "--private-stdin",
            "--yes",
            "--json",
        ],
    ] {
        let output = server.run(&args, "{malformed-private-password}");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("malformed-private-password"));
    }
    for args in [
        vec![
            "wifi",
            "connect",
            "id",
            "--password",
            "private-password",
            "--json",
        ],
        vec!["metadata", "key", "set", "private-token", "--json"],
        vec![
            "identity",
            "backup",
            "--password",
            "private-password",
            "--json",
        ],
    ] {
        let output = server.run(&args, "");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-"));
    }
    assert!(server.service.calls.lock().unwrap().is_empty());
}

#[test]
fn settings_setup_uses_each_just_returned_revision_and_preserves_aggregate_responses() {
    use korrid::{network::*, onboarding::*};
    let status = RpcResponse::OnboardingStatus(OnboardingStatusOutcome::Ok(OnboardingStatus {
        needed: true,
    }));
    let first = RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings()));
    let mut seats = settings();
    seats.player_count = 3;
    seats.revision = "after-seats".into();
    let seats = RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Ok(seats));
    let mut named = settings();
    named.device_name = Some("New name".into());
    named.player_count = 3;
    named.revision = "after-name".into();
    let named = RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Ok(named));
    let network = RpcResponse::WifiConnectHidden(NetworkStatusOutcome::Ok(NetworkSnapshot {
        connection: NetworkConnection::Offline,
        wifi: Wifi::NoAdapter,
    }));
    let server = RpcServer::start(vec![
        exchange(
            RpcRequest::OnboardingStatus(OnboardingStatusRequest {}),
            status.clone(),
        ),
        exchange(
            RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
            first.clone(),
        ),
        exchange(
            RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                expected_revision: "exact-revision".into(),
                setting_id: config::settings::PLAYER_COUNT_SETTING_ID.into(),
                value: "3".into(),
            }),
            seats.clone(),
        ),
        exchange(
            RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                expected_revision: "after-seats".into(),
                setting_id: config::settings::DEVICE_NAME_SETTING_ID.into(),
                value: "New name".into(),
            }),
            named.clone(),
        ),
        exchange(
            RpcRequest::WifiConnectHidden(WifiConnectHiddenRequest {
                ssid: "Hidden".into(),
                password: Some("private-password".into()),
            }),
            network.clone(),
        ),
    ]);
    let output = server.run(
        &[
            "setup",
            "--seats",
            "3",
            "--name",
            "New name",
            "--network",
            "Hidden",
            "--hidden",
            "--password-stdin",
            "--json",
        ],
        "private-password\n",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        actual,
        serde_json::to_value(vec![status, first, seats, named, network]).unwrap()
    );
    server.done();
}

#[test]
fn settings_setup_conflict_does_not_retry_or_finish_or_join_network() {
    use korrid::onboarding::*;
    let response = RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Err(failure()));
    let server = RpcServer::start(vec![
        exchange(
            RpcRequest::OnboardingStatus(OnboardingStatusRequest {}),
            RpcResponse::OnboardingStatus(OnboardingStatusOutcome::Ok(OnboardingStatus {
                needed: true,
            })),
        ),
        exchange(
            RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
            RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings())),
        ),
        exchange(
            RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                expected_revision: "exact-revision".into(),
                setting_id: config::settings::PLAYER_COUNT_SETTING_ID.into(),
                value: "3".into(),
            }),
            response.clone(),
        ),
    ]);
    let output = server.run(
        &[
            "setup",
            "--seats",
            "3",
            "--name",
            "Not written",
            "--network",
            "Not joined",
            "--json",
        ],
        "",
    );
    assert_eq!(output.status.code(), Some(1));
    let responses: Vec<RpcResponse> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(responses.len(), 3);
    assert_eq!(
        serde_json::to_value(&responses[2]).unwrap(),
        serde_json::to_value(response).unwrap()
    );
    server.done();
}

#[test]
fn settings_setup_rerun_needs_confirmation_and_finishing_keeps_the_existing_identity() {
    use korrid::onboarding::*;
    let server = RpcServer::start(vec![exchange(
        RpcRequest::OnboardingStatus(OnboardingStatusRequest {}),
        RpcResponse::OnboardingStatus(OnboardingStatusOutcome::Ok(OnboardingStatus {
            needed: false,
        })),
    )]);
    let output = server.run(&["setup", "--json"], "");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    server.done();
    for (action, request, needed) in [
        (
            "restart",
            RpcRequest::OnboardingRestart(OnboardingRestartRequest {}),
            true,
        ),
        (
            "finish",
            RpcRequest::OnboardingFinish(OnboardingFinishRequest {}),
            false,
        ),
    ] {
        let outcome = OnboardingStatusOutcome::Ok(OnboardingStatus { needed });
        let response = if needed {
            RpcResponse::OnboardingRestart(outcome)
        } else {
            RpcResponse::OnboardingFinish(outcome)
        };
        let server = RpcServer::start(vec![exchange(request, response.clone())]);
        assert_json(
            &server.run(&["setup", action, "--yes", "--json"], ""),
            &response,
            0,
        );
        server.done();
    }
}

#[test]
fn settings_metadata_show_and_clear_only_show_configuration_status() {
    let response = RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings()));
    let server = RpcServer::start(vec![exchange(
        RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
        response.clone(),
    )]);
    assert_json(
        &server.run(&["metadata", "key", "--json"], ""),
        &response,
        0,
    );
    server.done();
    let response = RpcResponse::SteamGridDbCredentialClear(SensitiveSettingOutcome::Ok(
        SensitiveSettingResult {
            status: config::settings::SecretSettingStatus::NotConfigured,
        },
    ));
    let server = RpcServer::start(vec![exchange(
        RpcRequest::SteamGridDbCredentialClear(SteamGridDbCredentialClearRequest {}),
        response.clone(),
    )]);
    assert_json(
        &server.run(&["metadata", "key", "clear", "--yes", "--json"], ""),
        &response,
        0,
    );
    server.done();
}

#[test]
fn settings_status_and_scan_keep_historical_failed_attempts_as_successful_observations() {
    use korrid::network::*;
    for scan in [false, true] {
        let status = NetworkStatusOutcome::Ok(NetworkSnapshot {
            connection: NetworkConnection::Offline,
            wifi: Wifi::Available {
                scanning: false,
                networks: vec![],
                attempt: WifiAttempt::Failed {
                    ssid: "Network".into(),
                    message: "Could not connect.".into(),
                    password_rejected: true,
                },
            },
        });
        let (request, response, action) = if scan {
            (
                RpcRequest::WifiScan(WifiScanRequest {}),
                RpcResponse::WifiScan(status),
                "scan",
            )
        } else {
            (
                RpcRequest::NetworkStatus(NetworkStatusRequest {}),
                RpcResponse::NetworkStatus(status),
                "status",
            )
        };
        let server = RpcServer::start(vec![exchange(request, response.clone())]);
        assert_json(&server.run(&["wifi", action, "--json"], ""), &response, 0);
        server.done();
    }
}

// Shared output handling belongs to play-library. This regression is expected
// to pass once the parent's Areas combination includes that shared fix.
#[test]
fn settings_connect_failed_attempt_exits_nonzero_without_changing_the_response() {
    use korrid::network::*;
    for hidden in [false, true] {
        let status = NetworkStatusOutcome::Ok(NetworkSnapshot {
            connection: NetworkConnection::Offline,
            wifi: Wifi::Available {
                scanning: false,
                networks: vec![],
                attempt: WifiAttempt::Failed {
                    ssid: "Network".into(),
                    message: "Could not connect.".into(),
                    password_rejected: true,
                },
            },
        });
        let (request, response) = if hidden {
            (
                RpcRequest::WifiConnectHidden(WifiConnectHiddenRequest {
                    ssid: "Network".into(),
                    password: None,
                }),
                RpcResponse::WifiConnectHidden(status),
            )
        } else {
            (
                RpcRequest::WifiConnect(WifiConnectRequest {
                    network_id: "Network".into(),
                    password: None,
                }),
                RpcResponse::WifiConnect(status),
            )
        };
        let server = RpcServer::start(vec![exchange(request, response.clone())]);
        let mut args = vec!["wifi", "connect", "Network", "--json"];
        if hidden {
            args.push("--hidden");
        }
        assert_json(&server.run(&args, ""), &response, 1);
        server.done();
    }
}

fn person_name_settings(name: Option<&str>, editable: bool) -> SettingsSnapshot {
    let mut snapshot = settings();
    snapshot.person_name = name.map(str::to_owned);
    if editable {
        snapshot.editable_setting_ids.push("person-name".into());
    }
    snapshot
}

#[test]
fn settings_identity_name_help_needs_no_rpc() {
    let server = RpcServer::start(vec![]);
    for args in [
        vec!["identity", "--help"],
        vec!["identity", "name", "--help"],
    ] {
        let output = server.run(&args, "");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let help = String::from_utf8_lossy(&output.stdout);
        assert!(help.contains("name"));
        if args.get(1) == Some(&"name") {
            assert!(help.contains("korri identity name [OPTIONS] [NAME]"));
            assert!(help.contains("At most 64 characters"));
            assert!(help.contains("without a fallback name"));
        }
    }
    assert!(server.service.calls.lock().unwrap().is_empty());
}

#[test]
fn settings_identity_name_show_preserves_actual_optional_and_empty_names() {
    for name in [Some("Avery\u{1b}[31m\n\t"), Some(""), None] {
        let response = RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(
            person_name_settings(name, false),
        ));
        let server = RpcServer::start(vec![
            exchange(
                RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
                response.clone(),
            ),
            exchange(
                RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
                response.clone(),
            ),
        ]);
        assert_json(
            &server.run(&["identity", "name", "--json"], ""),
            &response,
            0,
        );
        let output = server.run(&["identity", "name"], "");
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let expected = match name {
            Some("") => "Person name: \n",
            Some(_) => "Person name: Avery�[31m��\n",
            None => "No person name was returned.\n",
        };
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        assert_eq!(server.service.calls.lock().unwrap().len(), 2);
        server.done();
    }
}

#[test]
fn settings_identity_name_edit_uses_exact_snapshot_cas_and_only_final_response() {
    for name in ["  Avery  ".to_owned(), String::new(), "é".repeat(64)] {
        let mut returned = person_name_settings(Some("Producer returned\u{1b}[0m\nname"), true);
        returned.revision = "after-person-name".into();
        let response = RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Ok(returned));
        let server = RpcServer::start(vec![
            exchange(
                RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
                RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(person_name_settings(
                    Some("Before"),
                    true,
                ))),
            ),
            exchange(
                RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                    expected_revision: "exact-revision".into(),
                    setting_id: "person-name".into(),
                    value: name.clone(),
                }),
                response.clone(),
            ),
        ]);
        assert_json(
            &server.run(&["identity", "name", &name, "--json"], ""),
            &response,
            0,
        );
        assert_eq!(server.service.calls.lock().unwrap().len(), 2);
        server.done();
    }
}

#[test]
fn settings_identity_name_edit_renders_returned_name_not_requested_or_fallback_value() {
    for returned_name in [Some("Actual\u{1b}[1m\n\tname"), Some(""), None] {
        let server = RpcServer::start(vec![
            exchange(
                RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
                RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(person_name_settings(
                    Some("Before"),
                    true,
                ))),
            ),
            exchange(
                RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                    expected_revision: "exact-revision".into(),
                    setting_id: "person-name".into(),
                    value: String::new(),
                }),
                RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Ok(person_name_settings(
                    returned_name,
                    true,
                ))),
            ),
        ]);
        let output = server.run(&["identity", "name", ""], "");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            match returned_name {
                Some("") => "Person name: \n",
                Some(_) => "Person name: Actual�[1m��name\n",
                None => "No person name was returned.\n",
            }
        );
        server.done();
    }
}

#[test]
fn settings_identity_name_noneditable_snapshot_never_attempts_a_write() {
    let server = RpcServer::start(vec![exchange(
        RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
        RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(person_name_settings(
            Some("Before"),
            false,
        ))),
    )]);
    let output = server.run(&["identity", "name", "After", "--json"], "");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error.code, "PersonSignerUnavailable");
    assert_eq!(error.message, "This device cannot sign for the owner.");
    assert_eq!(server.service.calls.lock().unwrap().len(), 1);
    server.done();
}

#[test]
fn settings_identity_name_preserves_conflict_and_signer_failures_without_retry() {
    for failure in [
        failure(),
        RpcFailure {
            code: "PersonSignerUnavailable".into(),
            message: "This device cannot sign for the owner.".into(),
        },
    ] {
        let response = RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Err(failure));
        let server = RpcServer::start(vec![
            exchange(
                RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
                RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(person_name_settings(
                    Some("Before"),
                    true,
                ))),
            ),
            exchange(
                RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                    expected_revision: "exact-revision".into(),
                    setting_id: "person-name".into(),
                    value: "After".into(),
                }),
                response.clone(),
            ),
        ]);
        assert_json(
            &server.run(&["identity", "name", "After", "--json"], ""),
            &response,
            1,
        );
        assert_eq!(server.service.calls.lock().unwrap().len(), 2);
        server.done();
    }
}

#[test]
fn settings_identity_name_snapshot_failure_is_preserved_without_writing() {
    let response = RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Err(failure()));
    let server = RpcServer::start(vec![exchange(
        RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
        response.clone(),
    )]);
    assert_json(
        &server.run(&["identity", "name", "After", "--json"], ""),
        &response,
        1,
    );
    assert_eq!(server.service.calls.lock().unwrap().len(), 1);
    server.done();
}

#[test]
fn settings_identity_name_invalid_arguments_never_contact_korrid() {
    let server = RpcServer::start(vec![]);
    for invalid in ["a".repeat(65), "é".repeat(65)] {
        let output = server.run(&["identity", "name", &invalid, "--json"], "");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error.code, "Usage");
    }
    let output = server.run(&["identity", "name", "One", "Two", "--json"], "");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let error: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error.code, "Usage");
    assert!(server.service.calls.lock().unwrap().is_empty());
}

#[test]
fn settings_identity_name_rejects_peer_scope_before_any_name_operation() {
    let server = RpcServer::start(vec![
        exchange(
            RpcRequest::IdentityStatus(IdentityStatusRequest {}),
            RpcResponse::IdentityStatus(IdentityStatusOutcome::Ok(identity_status())),
        ),
        exchange(
            RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
            RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings())),
        ),
        exchange(
            RpcRequest::PeerList(PeerListRequest {}),
            RpcResponse::PeerList(PeerListOutcome::Ok(PeerList {
                peers: vec![PeerListEntry {
                    device_public_key: "peer-key".into(),
                    label: "Peer".into(),
                    state: PeerListState::Ready,
                    updated_at: 1u32.into(),
                    last_error: None,
                }],
            })),
        ),
    ]);
    let output = server.run(
        &["identity", "name", "After", "--device", "Peer", "--json"],
        "",
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let error: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error.code, "Usage");
    assert_eq!(
        error.message,
        "Identity stays local. Use SSH or local access on the selected device."
    );
    assert_eq!(server.service.calls.lock().unwrap().len(), 3);
    server.done();
}

#[test]
fn settings_wifi_failure_causes_are_sanitized_without_echoing_passwords() {
    use korrid::network::*;
    for hidden in [false, true] {
        for (message, sanitized, password_rejected) in [
            (
                "The network did not accept the password. Try again.",
                "The network did not accept the password. Try again.",
                true,
            ),
            (
                "The network did not answer in time. Try again.",
                "The network did not answer in time. Try again.",
                false,
            ),
            (
                "Other cause\u{1b}[31m\n\r\t\u{7f}end",
                "Other cause�[31m����end",
                false,
            ),
        ] {
            let status = NetworkStatusOutcome::Ok(NetworkSnapshot {
                connection: NetworkConnection::Offline,
                wifi: Wifi::Available {
                    scanning: false,
                    networks: vec![],
                    attempt: WifiAttempt::Failed {
                        ssid: "Home\u{1b}[0m".into(),
                        message: message.into(),
                        password_rejected,
                    },
                },
            });
            let (request, response) = if hidden {
                (
                    RpcRequest::WifiConnectHidden(WifiConnectHiddenRequest {
                        ssid: "Home".into(),
                        password: Some("entered-private-password".into()),
                    }),
                    RpcResponse::WifiConnectHidden(status),
                )
            } else {
                (
                    RpcRequest::WifiConnect(WifiConnectRequest {
                        network_id: "network-id".into(),
                        password: Some("entered-private-password".into()),
                    }),
                    RpcResponse::WifiConnect(status),
                )
            };
            let server = RpcServer::start(vec![
                exchange(request.clone(), response.clone()),
                exchange(request, response.clone()),
            ]);
            let mut args = vec![
                "wifi",
                "connect",
                if hidden { "Home" } else { "network-id" },
                "--password-stdin",
            ];
            if hidden {
                args.push("--hidden");
            }
            let output = server.run(&args, "entered-private-password\n");
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stderr.is_empty());
            let human = String::from_utf8(output.stdout).unwrap();
            assert!(human.contains("Could not join Home�[0m"), "{human}");
            assert!(human.contains(sanitized), "{human}");
            assert!(human.contains("Next:"), "{human}");
            assert!(!human.contains('\u{1b}'));
            assert!(!human.contains("entered-private-password"));
            args.push("--json");
            assert_json(
                &server.run(&args, "entered-private-password\n"),
                &response,
                1,
            );
            server.done();
        }
    }
}
