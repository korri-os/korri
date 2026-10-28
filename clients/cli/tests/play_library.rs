use axum::{extract::State, routing::post, Json, Router};
use korrid::*;
use std::{
    collections::VecDeque,
    process::{Command, Output},
    sync::{Arc, Mutex},
};
type Calls = Arc<Mutex<Vec<RpcRequest>>>;
#[derive(Clone)]
struct Service {
    calls: Calls,
    responses: Arc<Mutex<VecDeque<serde_json::Value>>>,
    grants: discovery::FolderSelectionGrantStore,
}
struct Server {
    root: tempfile::TempDir,
    calls: Calls,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn start(responses: Vec<RpcResponse>) -> Self {
        Self::start_in(tempfile::tempdir().unwrap(), responses)
    }
    fn start_in(root: tempfile::TempDir, responses: Vec<RpcResponse>) -> Self {
        Self::start_wire(
            root,
            responses
                .into_iter()
                .map(|response| serde_json::to_value(response).unwrap())
                .collect(),
        )
    }
    fn start_wire(root: tempfile::TempDir, responses: Vec<serde_json::Value>) -> Self {
        let listener =
            std::os::unix::net::UnixListener::bind(root.path().join("cli.sock")).unwrap();
        listener.set_nonblocking(true).unwrap();
        let calls = Arc::new(Mutex::new(vec![]));
        let state = Service {
            calls: calls.clone(),
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
            grants: discovery::FolderSelectionGrantStore::default(),
        };
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let thread = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let app = Router::new()
                        .route(
                            "/rpc",
                            post(
                                |State(state): State<Service>, Json(request): Json<RpcRequest>| async move {
                                    if let RpcRequest::LibraryFolderAdd(request) = &request { state.grants.consume(&request.receipt).expect("real receipt from same store"); }
                                    state.calls.lock().unwrap().push(request);
                                    Json(state.responses.lock().unwrap().pop_front().expect("configured response"))
                                },
                            ),
                        )
                        .route("/forward", post(|State(state): State<Service>, Json(request): Json<korrid::cli::ForwardRequest>| async move {
                            assert_eq!(request.device_public_key, "22".repeat(32));
                            state.calls.lock().unwrap().push(request.request.clone());
                            if matches!(request.request, RpcRequest::SavesCheck(_)) {
                                return Err(axum::http::StatusCode::FORBIDDEN);
                            }
                            Ok(Json(state.responses.lock().unwrap().pop_front().expect("configured forwarded response")))
                        }))
                        .route("/folder-selection", post(|State(state): State<Service>, Json(path): Json<std::path::PathBuf>| async move { Json(state.grants.issue_approved_path(path).unwrap().token) }))
                        .with_state(state);
                    axum::serve(tokio::net::UnixListener::from_std(listener).unwrap(), app)
                        .with_graceful_shutdown(async {
                            let _ = stopped.await;
                        })
                        .await
                        .unwrap();
                })
        });
        Self {
            root,
            calls,
            stop: Some(stop),
            thread: Some(thread),
        }
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_korri"))
            .args(args)
            .arg("--socket")
            .arg(self.root.path().join("cli.sock"))
            .output()
            .unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop.take().unwrap().send(());
        self.thread.take().unwrap().join().unwrap();
    }
}
fn active(failure: Option<RpcFailure>) -> RpcResponse {
    RpcResponse::SessionStatus(SessionStatusOutcome::Ok(SessionStatus {
        netplay: None,
        stream_play_launch_id: None,
        stream_play_ended: None,
        active: Some(ActiveSession {
            person_public_key: None,
            stream_players: vec![],
            launch_id: "observed-launch".into(),
            host: None,
            game_id: Some("game".into()),
            title: None,
            phase: None,
            initial_handoff: None,
            focus_ownership: None,
            netplay: None,
        }),
        overlay: None,
        pending_launches: None,
        observation_failure: failure,
    }))
}
#[test]
fn play_library_session_end_uses_observed_id_and_keeps_pending_outcome() {
    let server = Server::start(vec![
        active(None),
        RpcResponse::SessionEnd(SessionEndOutcome::Ok(SessionEndResult {
            phase: SessionEndPhase::Pending,
        })),
    ]);
    let output = server.run(&["end", "--yes", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(matches!(
        serde_json::from_slice::<RpcResponse>(&output.stdout).unwrap(),
        RpcResponse::SessionEnd(SessionEndOutcome::Ok(SessionEndResult {
            phase: SessionEndPhase::Pending
        }))
    ));
    assert!(
        matches!(&server.calls.lock().unwrap()[1],RpcRequest::SessionEnd(r) if r.expected_launch_id.as_deref()==Some("observed-launch") && r.force!=Some(true))
    );
}
#[test]
fn play_library_observation_failure_never_controls_session() {
    let server = Server::start(vec![active(Some(RpcFailure {
        code: "ObservationFailed".into(),
        message: "Unknown native state".into(),
    }))]);
    let output = server.run(&["leave", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(server.calls.lock().unwrap().len(), 1);
    assert!(matches!(
        serde_json::from_slice::<RpcResponse>(&output.stdout).unwrap(),
        RpcResponse::SessionStatus(_)
    ));
}
#[test]
fn play_library_destructive_nonterminal_requires_yes() {
    for args in [
        vec!["end"],
        vec!["library", "folders", "remove", "folder"],
        vec!["library", "attention", "ignore", "item"],
    ] {
        let is_end = args[0] == "end";
        let server = Server::start(if is_end { vec![active(None)] } else { vec![] });
        let output = server.run(&args);
        assert_eq!(output.status.code(), Some(2));
        let calls = server.calls.lock().unwrap();
        assert_eq!(calls.len(), usize::from(is_end));
        assert!(!calls.iter().any(|r| matches!(
            r,
            RpcRequest::SessionEnd(_)
                | RpcRequest::LibraryFolderRemove(_)
                | RpcRequest::AttentionIgnore(_)
        )));
    }
}
#[test]
fn play_library_folder_remove_only_unregisters_and_preserves_files() {
    let response = RpcResponse::LibraryFolderRemove(LibraryFoldersSnapshotOutcome::Ok(
        LibraryFoldersSnapshot {
            generation: "g".into(),
            state: LibraryFoldersState::Idle {},
            folders: vec![],
            attention: 0,
            diagnostics: vec![],
        },
    ));
    let server = Server::start(vec![response.clone()]);
    let game = server.root.path().join("game.bin");
    std::fs::write(&game, b"game bytes").unwrap();
    let output = server.run(&["library", "folders", "remove", "folder", "--yes", "--json"]);
    assert!(output.status.success());
    assert_eq!(std::fs::read(game).unwrap(), b"game bytes");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::to_value(response).unwrap()
    );
    assert!(
        matches!(&server.calls.lock().unwrap()[0],RpcRequest::LibraryFolderRemove(r) if r.folder=="folder")
    );
}
#[test]
fn play_library_ambiguous_titles_piped_list_exact_choices() {
    let server = Server::start(vec![RpcResponse::LocalGamesList(
        LocalGamesListOutcome::Ok(LocalGames {
            games: ["a", "b"]
                .map(|id| launcher::LocalGame {
                    id: id.into(),
                    title: "Same".into(),
                    system: "sys".into(),
                    identity: None,
                    cover_asset_id: None,
                    play_stats: None,
                })
                .to_vec(),
            failures: None,
        }),
    )]);
    let output = server.run(&["play", "sAmE", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let error: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
    assert!(error.message.contains("Same (a)"));
    assert!(error.message.contains("Same (b)"));
}

fn games() -> RpcResponse {
    RpcResponse::LocalGamesList(LocalGamesListOutcome::Ok(LocalGames {
        games: vec![launcher::LocalGame {
            id: "game".into(),
            title: "A Game".into(),
            system: "sys".into(),
            identity: None,
            cover_asset_id: None,
            play_stats: None,
        }],
        failures: None,
    }))
}
fn test_snapshot() -> launch_settings::LaunchSnapshot {
    use launch_settings::*;
    LaunchSnapshot {
        places: vec![
            LaunchPlaceSnapshot {
                place: LaunchPlace {
                    layer: LaunchLayer::Device,
                    scope: LaunchRunnerScope::Any,
                },
                title: "Device".into(),
                body: serde_json::json!({}),
                editable_kinds: vec![
                    "cpu".into(),
                    "gpu".into(),
                    "runner".into(),
                    "modifiers".into(),
                ],
            },
            LaunchPlaceSnapshot {
                place: LaunchPlace {
                    layer: LaunchLayer::Device,
                    scope: LaunchRunnerScope::Runner("@test/runner".into()),
                },
                title: "Runner".into(),
                body: serde_json::json!({}),
                editable_kinds: vec![
                    "settings".into(),
                    "cpu".into(),
                    "gpu".into(),
                    "modifiers".into(),
                ],
            },
        ],
        resolved: LaunchResolvedSnapshot {
            values: Default::default(),
        },
        runners: vec![LaunchRunnerSnapshot {
            id: "@test/runner".into(),
            family: Some("@test/family".into()),
            title: "Runner".into(),
            selected: true,
            // Settings are nested (ADR 0037): the section `@test/mod` makes the
            // runner path `@test/mod.option` collide with the modifier's option.
            schema: serde_json::json!({"properties":{"speed":{"type":"integer","minimum":1,"maximum":10},"@test/mod":{"type":"object","properties":{"option":{"type":"boolean"}}}}}),
            revision: "runner-rev".into(),
            prepare: LaunchRunnerFacts::Unavailable("not prepared".into()),
            choosable: true,
        }],
        modifiers: vec![LaunchModifierSnapshot {
            id: "@test/mod".into(),
            title: "Modifier".into(),
            schema: Some(serde_json::json!({"properties":{"option":{"type":"boolean"}}})),
            revision: Some("mod-rev".into()),
        }],
        services: vec![],
        runner_resolved: vec![],
        profiles: vec![],
        selected_profile: None,
        device_facts: None,
        notices: vec![],
        route_layers: LaunchSettingsRouteLayers::Present,
        revisions: korrid::config::settings::RunnerChoiceRevisions {
            device: "device-revision".into(),
            games: "games-revision".into(),
        },
    }
}
fn snapshot_response(s: launch_settings::LaunchSnapshot) -> RpcResponse {
    RpcResponse::LaunchSettingsSnapshot(launch_settings::LaunchSettingsSnapshotOutcome::Ok(
        Box::new(s),
    ))
}
#[test]
fn play_library_settings_cas_conflict_is_final_and_never_retried() {
    let failure = RpcResponse::LaunchSettingsSet(launch_settings::LaunchSettingsWriteOutcome::Err(
        RpcFailure {
            code: "SettingsConflict".into(),
            message: "Changed since snapshot".into(),
        },
    ));
    let server = Server::start(vec![snapshot_response(test_snapshot()), failure.clone()]);
    let output = server.run(&["launch-settings", "set", "cpu.max=80", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::to_value(failure).unwrap()
    );
    let calls = server.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(
        matches!(&calls[1],RpcRequest::LaunchSettingsSet(r) if r.expected_revision=="device-revision" && matches!(r.path,launch_settings::LaunchPath::CpuMax) && r.value==Some(serde_json::json!(80)))
    );
}
#[test]
fn play_library_settings_collision_and_config_only_are_refused() {
    for key in [
        "@test/mod.option=true",
        "config.foo=bar",
        "hooks.before=evil",
        "env.PATH=evil",
        "args.append=evil",
        "cwd=/tmp",
    ] {
        let server = Server::start(vec![snapshot_response(test_snapshot())]);
        let output = server.run(&[
            "launch-settings",
            "set",
            key,
            "--runner",
            "@test/runner",
            "--json",
        ]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{key}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert_eq!(server.calls.lock().unwrap().len(), 1);
    }
}
#[test]
fn play_library_stale_launch_is_not_retargeted() {
    let server = Server::start(vec![
        active(None),
        RpcResponse::SessionLeave(SessionTransitionOutcome::Err(RpcFailure {
            code: "StaleSession".into(),
            message: "A different launch is live".into(),
        })),
    ]);
    let output = server.run(&["leave", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let calls = server.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(
        matches!(&calls[1],RpcRequest::SessionLeave(r) if r.expected_launch_id.as_deref()==Some("observed-launch"))
    );
}
#[test]
fn play_library_one_launch_overrides_use_runner_schema_and_never_write_settings() {
    use game_routes::*;
    use launch_settings::*;
    use saves::types::*;
    let routes = RpcResponse::GameRoutes(GameRoutesOutcome::Ok(GameRoutes {
        game_id: "game".into(),
        routes: vec![GameRoute {
            runner_id: "@test/runner".into(),
            family_id: None,
            system_id: "sys".into(),
            runner_build: "build".into(),
            program: "program".into(),
            warnings: vec![],
        }],
        selection: GameRouteSelection::Selected("@test/runner".into()),
        game_runner: None,
        system_runners: Default::default(),
        revisions: test_snapshot().revisions,
    }));
    let accepted =
        RpcResponse::SelectedGameLaunch(SelectedGameLaunchOutcome::Ok(SelectedGameLaunch {
            session: SessionPrepared {
                game_id: "game".into(),
                launch_id: "new-launch".into(),
            },
            warnings: vec![],
        }));
    let server = Server::start(vec![
        games(),
        routes,
        snapshot_response(test_snapshot()),
        RpcResponse::SavesCheck(SaveCheckOutcome::Ok(SaveCheck {
            warnings: vec![],
            status: SaveMark::Synced,
            in_use: vec![],
        })),
        accepted.clone(),
    ]);
    let output = server.run(&[
        "play",
        "a gAmE",
        "--set",
        "speed=2",
        "--set",
        "cpu.max=70",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!([
            RpcResponse::SavesCheck(SaveCheckOutcome::Ok(SaveCheck {
                warnings: vec![],
                status: SaveMark::Synced,
                in_use: vec![]
            })),
            accepted
        ])
    );
    let calls = server.calls.lock().unwrap();
    assert_eq!(calls.len(), 5);
    assert!(
        matches!(&calls[4],RpcRequest::SelectedGameLaunch(r) if r.runner_id=="@test/runner" && r.launch.as_ref().unwrap().cpu.as_ref().unwrap().max==Some(70) && matches!(r.launch.as_ref().unwrap().settings.as_ref().unwrap().scope,LaunchRunnerScope::Runner(_)))
    );
    assert!(!calls
        .iter()
        .any(|r| matches!(r, RpcRequest::LaunchSettingsSet(_))));
}
#[test]
fn play_library_failed_jobs_artifacts_and_partial_games_keep_ok_json_but_exit_one() {
    use acquisition::*;
    let error = RpcFailure {
        code: "DownloadFailed".into(),
        message: "Download failed".into(),
    };
    let responses = vec![
        (
            vec!["downloads", "status", "--job", "job"],
            RpcResponse::AcquisitionAcquireStatus(AcquireJobOutcome::Ok(AcquireJobStatus {
                job_id: "job".into(),
                provider_id: "provider".into(),
                id: "claim".into(),
                file_name: None,
                system: None,
                message: None,
                state: AcquireJobState::Failed {
                    reason: error.clone(),
                },
            })),
        ),
        (
            vec!["downloads", "status"],
            RpcResponse::AcquisitionStatus(AcquisitionOutcome::Ok(AcquisitionSnapshot {
                artifacts: vec![ArtifactStatus {
                    id: "artifact".into(),
                    title: "Artifact".into(),
                    version: "1".into(),
                    bytes_total: None,
                    release: None,
                    state: ArtifactState::Failed {
                        reason: error.clone(),
                    },
                }],
            })),
        ),
        (
            vec!["library", "games", "list"],
            RpcResponse::LocalGamesList(LocalGamesListOutcome::Ok(LocalGames {
                games: vec![],
                failures: Some(vec![error]),
            })),
        ),
    ];
    for (mut args, response) in responses {
        args.push("--json");
        let server = Server::start(vec![response.clone()]);
        let output = server.run(&args);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stderr.is_empty());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
            serde_json::to_value(response).unwrap()
        );
    }
}
#[test]
fn play_library_provider_job_status_and_artifact_cancel_ids_are_distinct() {
    use acquisition::*;
    let job = RpcResponse::AcquisitionAcquireStatus(AcquireJobOutcome::Ok(AcquireJobStatus {
        job_id: "job".into(),
        provider_id: "provider".into(),
        id: "claim".into(),
        file_name: None,
        system: None,
        message: None,
        state: AcquireJobState::Unpacking,
    }));
    let server = Server::start(vec![job]);
    let output = server.run(&["downloads", "status", "--job", "job", "--json"]);
    assert!(output.status.success());
    assert!(
        matches!(&server.calls.lock().unwrap()[0],RpcRequest::AcquisitionAcquireStatus(r) if r.job_id=="job")
    );
    let server = Server::start(vec![RpcResponse::AcquisitionCancel(
        AcquisitionOutcome::Ok(AcquisitionSnapshot { artifacts: vec![] }),
    )]);
    let output = server.run(&["downloads", "cancel", "--artifact", "artifact", "--json"]);
    assert!(output.status.success());
    assert!(
        matches!(&server.calls.lock().unwrap()[0],RpcRequest::AcquisitionCancel(r) if r.artifact_id=="artifact")
    );
}

#[test]
fn play_library_folder_add_uses_local_receipt_but_prints_only_final_pair_response() {
    let response =
        RpcResponse::LibraryFolderAdd(LibraryFoldersSnapshotOutcome::Ok(LibraryFoldersSnapshot {
            generation: "generation".into(),
            state: LibraryFoldersState::Scanning {},
            folders: vec![],
            attention: 0,
            diagnostics: vec![],
        }));
    let server = Server::start(vec![response.clone()]);
    let folder = server.root.path().to_str().unwrap();
    let output = server.run(&["library", "folders", "add", folder, "gba", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = server.calls.lock().unwrap();
    let RpcRequest::LibraryFolderAdd(pair) = &calls[0] else {
        panic!("pair expected")
    };
    assert_eq!(pair.system, "gba");
    assert!(!String::from_utf8_lossy(&output.stdout).contains(&pair.receipt));
    assert!(output.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::to_value(response).unwrap()
    );
}
#[test]
fn play_library_system_layer_without_game_uses_observed_place_and_never_falls_back() {
    use launch_settings::*;
    for system in ["sys", "wrong"] {
        let mut snapshot = test_snapshot();
        snapshot.places.push(LaunchPlaceSnapshot {
            place: LaunchPlace {
                layer: LaunchLayer::System("sys".into()),
                scope: LaunchRunnerScope::Any,
            },
            title: "System".into(),
            body: serde_json::json!({"cpu":{"max":85}}),
            editable_kinds: vec!["cpu".into()],
        });
        let response = RpcResponse::LaunchSettingsSet(LaunchSettingsWriteOutcome::Ok(
            snapshot.revisions.clone(),
        ));
        let server = Server::start(vec![snapshot_response(snapshot), response]);
        let output = server.run(&[
            "launch-settings",
            "set",
            "cpu.max=70",
            "--system",
            system,
            "--json",
        ]);
        let calls = server.calls.lock().unwrap();
        assert!(matches!(&calls[0], RpcRequest::LaunchSettingsSnapshot(r) if r.game_id.is_none()));
        if system == "sys" {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(calls.len(), 2);
            assert!(
                matches!(&calls[1],RpcRequest::LaunchSettingsSet(r) if r.expected_revision=="device-revision" && r.place.layer==LaunchLayer::System("sys".into()))
            );
        } else {
            assert_eq!(output.status.code(), Some(2));
            assert_eq!(calls.len(), 1);
        }
    }
}
#[test]
fn play_library_system_show_without_game_labels_device_resolution() {
    use launch_settings::*;
    let mut snapshot = test_snapshot();
    snapshot.places.push(LaunchPlaceSnapshot {
        place: LaunchPlace {
            layer: LaunchLayer::System("sys".into()),
            scope: LaunchRunnerScope::Any,
        },
        title: "System".into(),
        body: serde_json::json!({"cpu":{"max":85}}),
        editable_kinds: vec!["cpu".into()],
    });
    let response = snapshot_response(snapshot);
    for json in [false, true] {
        let server = Server::start(vec![response.clone()]);
        let mut args = vec!["launch-settings", "show", "--system", "sys"];
        if json {
            args.push("--json");
        }
        let output = server.run(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(server.calls.lock().unwrap().len(), 1);
        if json {
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
                serde_json::to_value(&response).unwrap()
            );
        } else {
            let human = String::from_utf8(output.stdout).unwrap();
            assert!(human.contains("Device context"), "{human}");
            assert!(human.contains("85"), "{human}");
        }
    }
}

#[test]
fn play_library_provider_acquire_keeps_claim_identity_and_gets_details_first() {
    use acquisition::claims::*;
    use acquisition::*;
    let provider = "@test/provider";
    let id = "opaque-provider-claim";
    let job = AcquireJobStatus {
        job_id: "job-only".into(),
        provider_id: provider.into(),
        id: id.into(),
        file_name: None,
        system: Some("gba".into()),
        message: None,
        state: AcquireJobState::Queued,
    };
    let server = Server::start(vec![
        RpcResponse::AcquisitionProviders(PluginListOutcome::Ok(PluginListResponse {
            providers: vec![AcquisitionProvider {
                id: provider.into(),
                title: "Provider".into(),
                plugin_id: "@test/plugin".into(),
            }],
        })),
        RpcResponse::AcquisitionSearch(SearchOutcome::Ok(SearchResponse {
            claims: vec![ProviderClaim {
                tag: ProviderClaimTag::ProviderClaim,
                provider_id: provider.into(),
                id: id.into(),
                title: "Game".into(),
                url: "https://example.org/game".into(),
                platform: None,
                thumbnail_url: None,
                artifact: None,
                fetched_at: None,
            }],
        })),
        RpcResponse::AcquisitionDetails(DetailsOutcome::Ok(ProviderClaimDetails {
            tag: ProviderClaimDetailsTag::ProviderClaimDetails,
            provider_id: provider.into(),
            id: id.into(),
            title: "Game".into(),
            url: "https://example.org/game".into(),
            description: None,
            download_page_url: None,
            artifact: None,
            fetched_at: None,
        })),
        RpcResponse::AcquisitionAcquire(AcquireJobOutcome::Ok(job)),
    ]);
    let output = server.run(&["get", "Game", "--provider", provider, "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let calls = server.calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert!(matches!(&calls[1],RpcRequest::AcquisitionSearch(r) if r.provider_ids==[provider]));
    assert!(
        matches!(&calls[2],RpcRequest::AcquisitionDetails(r) if r.provider_id==provider && r.id==id)
    );
    assert!(
        matches!(&calls[3],RpcRequest::AcquisitionAcquire(r) if r.provider_id==provider && r.id==id)
    );
    assert!(
        matches!(serde_json::from_slice::<RpcResponse>(&output.stdout).unwrap(),RpcResponse::AcquisitionAcquire(AcquireJobOutcome::Ok(j)) if j.job_id=="job-only")
    );
}
#[test]
fn play_library_show_keeps_origins_and_terminal_safe_human_text() {
    use launch_settings::*;
    let mut snapshot = test_snapshot();
    snapshot.resolved.values.insert(
        "cpu.max".into(),
        LaunchSourcedValue {
            path: LaunchValuePath::Editable(LaunchPath::CpuMax),
            value: serde_json::json!(75),
            source: Some(LaunchSource::DeviceDefault),
            contributions: None,
            adjusted: false,
        },
    );
    let server = Server::start(vec![snapshot_response(snapshot)]);
    let output = server.run(&["launch-settings", "show"]);
    assert!(output.status.success());
    let human = String::from_utf8(output.stdout).unwrap();
    assert!(human.contains("cpu.max = 75"));
    assert!(human.contains("DeviceDefault"));
    assert!(!human.contains('\u{1b}'));
}

fn route_response() -> RpcResponse {
    use game_routes::*;
    RpcResponse::GameRoutes(GameRoutesOutcome::Ok(GameRoutes {
        game_id: "game".into(),
        routes: vec![GameRoute {
            runner_id: "@test/runner".into(),
            family_id: None,
            system_id: "sys".into(),
            runner_build: "build".into(),
            program: "program".into(),
            warnings: vec![],
        }],
        selection: GameRouteSelection::Selected("@test/runner".into()),
        game_runner: None,
        system_runners: Default::default(),
        revisions: test_snapshot().revisions,
    }))
}
#[test]
fn play_library_save_checks_never_hide_partial_failure_or_block_on_in_use() {
    use game_routes::*;
    use saves::types::*;
    let accepted =
        RpcResponse::SelectedGameLaunch(SelectedGameLaunchOutcome::Ok(SelectedGameLaunch {
            session: SessionPrepared {
                game_id: "game".into(),
                launch_id: "accepted".into(),
            },
            warnings: vec![],
        }));
    let failed = RpcResponse::SelectedGameLaunch(SelectedGameLaunchOutcome::Err(RpcFailure {
        code: "LaunchFailed".into(),
        message: "Launch refused".into(),
    }));
    let failed_check = RpcResponse::SavesCheck(SaveCheckOutcome::Err(RpcFailure {
        code: "SaveCheckUnavailable".into(),
        message: "Peer did not answer".into(),
    }));
    let warning_check = RpcResponse::SavesCheck(SaveCheckOutcome::Ok(SaveCheck {
        status: SaveMark::NotChecked,
        in_use: vec![SaveInUseWarning {
            device: "other-device".into(),
            since: 1,
        }],
        warnings: vec!["Different release".into()],
    }));
    for (check, launch, exit) in [
        (failed_check.clone(), accepted.clone(), 1),
        (warning_check.clone(), accepted.clone(), 0),
        (warning_check, failed, 1),
    ] {
        let server = Server::start(vec![
            games(),
            route_response(),
            snapshot_response(test_snapshot()),
            check.clone(),
            launch.clone(),
        ]);
        let output = server.run(&["play", "game", "--json"]);
        assert_eq!(
            output.status.code(),
            Some(exit),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
            serde_json::json!([check, launch])
        );
        assert_eq!(server.calls.lock().unwrap().len(), 5);
    }
    let server = Server::start(vec![
        games(),
        route_response(),
        snapshot_response(test_snapshot()),
        failed_check,
        accepted,
    ]);
    let output = server.run(&["play", "game"]);
    assert_eq!(output.status.code(), Some(1));
    let human = String::from_utf8(output.stdout).unwrap();
    assert!(human.contains("Launch accepted"));
    assert!(human.contains("Save check: unknown"));
    assert!(!human.contains("ready to play"));
}
#[test]
fn play_library_release_and_profile_changes_use_games_revision() {
    use launch_settings::*;
    let mut snapshot = test_snapshot();
    snapshot.places.push(LaunchPlaceSnapshot {
        place: LaunchPlace {
            layer: LaunchLayer::Release {
                game_id: "game".into(),
                release_id: "release".into(),
            },
            scope: LaunchRunnerScope::Any,
        },
        title: "Release".into(),
        body: serde_json::json!({}),
        editable_kinds: vec!["cpu".into()],
    });
    let server = Server::start(vec![
        games(),
        snapshot_response(snapshot.clone()),
        RpcResponse::LaunchSettingsSet(LaunchSettingsWriteOutcome::Ok(snapshot.revisions.clone())),
    ]);
    let output = server.run(&[
        "launch-settings",
        "set",
        "cpu.min=25",
        "--game",
        "game",
        "--release",
        "release",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        matches!(&server.calls.lock().unwrap()[2],RpcRequest::LaunchSettingsSet(r) if r.expected_revision=="games-revision" && matches!(&r.place.layer,LaunchLayer::Release{release_id,..} if release_id=="release"))
    );
    let server = Server::start(vec![
        games(),
        snapshot_response(snapshot.clone()),
        RpcResponse::LaunchProfileSave(LaunchSettingsWriteOutcome::Ok(snapshot.revisions.clone())),
    ]);
    let output = server.run(&[
        "launch-settings",
        "profiles",
        "new",
        "Quiet",
        "--game",
        "game",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        matches!(&server.calls.lock().unwrap()[2],RpcRequest::LaunchProfileSave(r) if r.expected_revision=="games-revision" && r.title=="Quiet" && r.profile_id.is_none())
    );
}

#[test]
fn play_library_terminal_picker_requires_an_exact_game_id() {
    let server = Server::start(vec![
        RpcResponse::LocalGamesList(LocalGamesListOutcome::Ok(LocalGames {
            games: ["a", "b"]
                .map(|id| launcher::LocalGame {
                    id: id.into(),
                    title: "Same".into(),
                    system: "sys".into(),
                    identity: None,
                    cover_asset_id: None,
                    play_stats: None,
                })
                .to_vec(),
            failures: None,
        })),
        RpcResponse::GameRoutes(game_routes::GameRoutesOutcome::Err(RpcFailure {
            code: "RouteUnavailable".into(),
            message: "No launch route".into(),
        })),
    ]);
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/play-library-terminal.py"
        ))
        .arg("b")
        .arg(env!("CARGO_BIN_EXE_korri"))
        .arg("--socket")
        .arg(server.root.path().join("cli.sock"))
        .args(["play", "Same"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = server.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(matches!(&calls[1],RpcRequest::GameRoutes(r) if r.game_id=="b"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Choose game"));
}

#[test]
fn play_library_terminal_picker_numbers_filters_and_cancel_never_guess() {
    for (answer, selected, ids) in [
        ("2", Some("b"), ["a", "b"]),
        ("/B\n1", Some("b"), ["a", "b"]),
        ("99\n2", Some("b"), ["a", "b"]),
        ("/no-match\n/\n2", Some("b"), ["a", "b"]),
        ("q", None, ["a", "b"]),
        ("2", Some("2"), ["2", "b"]),
        ("q", Some("q"), ["q", "b"]),
    ] {
        let mut responses = vec![RpcResponse::LocalGamesList(LocalGamesListOutcome::Ok(
            LocalGames {
                games: ids
                    .map(|id| launcher::LocalGame {
                        id: id.into(),
                        title: "Same".into(),
                        system: "sys".into(),
                        identity: None,
                        cover_asset_id: None,
                        play_stats: None,
                    })
                    .to_vec(),
                failures: None,
            },
        ))];
        if selected.is_some() {
            responses.push(RpcResponse::GameRoutes(
                game_routes::GameRoutesOutcome::Err(RpcFailure {
                    code: "RouteUnavailable".into(),
                    message: "No launch route".into(),
                }),
            ));
        }
        let server = Server::start(responses);
        let output = Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/play-library-terminal.py"
            ))
            .arg(answer)
            .arg(env!("CARGO_BIN_EXE_korri"))
            .arg("--socket")
            .arg(server.root.path().join("cli.sock"))
            .args(["play", "Same"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{answer}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let calls = server.calls.lock().unwrap();
        if let Some(id) = selected {
            assert_eq!(calls.len(), 2);
            assert!(matches!(&calls[1], RpcRequest::GameRoutes(r) if r.game_id == id));
        } else {
            assert_eq!(calls.len(), 1);
            assert!(String::from_utf8_lossy(&output.stdout).contains("Selection was cancelled"));
        }
    }
}

#[test]
fn play_library_inherit_clears_only_the_authored_value_with_exact_revision() {
    use launch_settings::*;
    for args in [
        vec!["launch-settings", "set", "cpu.max", "--inherit", "--json"],
        vec!["launch-settings", "runner", "--inherit", "--json"],
    ] {
        let snapshot = test_snapshot();
        let response = RpcResponse::LaunchSettingsSet(LaunchSettingsWriteOutcome::Ok(
            snapshot.revisions.clone(),
        ));
        let server = Server::start(vec![snapshot_response(snapshot), response.clone()]);
        let output = server.run(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
            serde_json::to_value(response).unwrap()
        );
        assert!(
            matches!(&server.calls.lock().unwrap()[1],RpcRequest::LaunchSettingsSet(r) if r.value.is_none() && r.expected_revision=="device-revision" && r.place.layer==LaunchLayer::Device && r.place.scope==LaunchRunnerScope::Any)
        );
    }
    let mut snapshot = test_snapshot();
    snapshot.resolved.values.insert(
        "cpu.max".into(),
        LaunchSourcedValue {
            path: LaunchValuePath::Editable(LaunchPath::CpuMax),
            value: serde_json::json!(93),
            source: Some(LaunchSource::DeviceDefault),
            contributions: None,
            adjusted: false,
        },
    );
    let response =
        RpcResponse::LaunchSettingsSet(LaunchSettingsWriteOutcome::Ok(snapshot.revisions.clone()));
    // The suggested follow-up must remain correct when an opaque socket path
    // happens to contain the inherited value's digits.
    let root = tempfile::Builder::new()
        .prefix("cli-93-")
        .tempdir()
        .unwrap();
    let server = Server::start_in(root, vec![snapshot_response(snapshot), response]);
    let output = server.run(&["launch-settings", "set", "cpu.max", "--inherit"]);
    assert!(output.status.success());
    let human = String::from_utf8(output.stdout).unwrap();
    assert!(human.contains("Cleared the cpu.max override"));
    assert!(human.contains("korri launch-settings show"));
    assert!(human.contains("Device"));
    assert!(human.contains("Any"));
    let acknowledgement = human.lines().next().unwrap();
    assert!(!acknowledgement.contains("93"));
    assert!(!acknowledgement.contains("DeviceDefault"));
    let place = LaunchPlace {
        layer: LaunchLayer::Device,
        scope: LaunchRunnerScope::Any,
    };
    let socket = server.root.path().join("cli.sock");
    assert_eq!(
        human,
        format!(
            "Cleared the cpu.max override at {}.\nNext: korri launch-settings show --socket '{}' to inspect current effective values and origins.\n",
            serde_json::to_string(&place).unwrap(),
            socket.display().to_string().replace('\'', "'\\''"),
        )
    );
    let calls = server.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(matches!(&calls[1], RpcRequest::LaunchSettingsSet(r)
        if r.value.is_none() && !r.clear && r.expected_revision == "device-revision"
            && r.place == place && matches!(r.path, LaunchPath::CpuMax)));
}
#[test]
fn play_library_inherit_conflicts_and_unavailable_places_never_mutate() {
    for args in [
        vec![
            "launch-settings",
            "set",
            "cpu.max=20",
            "--inherit",
            "--json",
        ],
        vec![
            "launch-settings",
            "runner",
            "@test/runner",
            "--inherit",
            "--json",
        ],
    ] {
        let server = Server::start(vec![]);
        let output = server.run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(server.calls.lock().unwrap().is_empty());
    }
    let server = Server::start(vec![games(), snapshot_response(test_snapshot())]);
    let output = server.run(&[
        "launch-settings",
        "set",
        "cpu.max",
        "--inherit",
        "--system",
        "wrong",
        "--game",
        "game",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(server.calls.lock().unwrap().len(), 2);
}
#[test]
fn play_library_literal_strings_do_not_mean_inherit() {
    use launch_settings::*;
    for input in ["inherit", "null", ""] {
        let mut snapshot = test_snapshot();
        snapshot.runners[0].schema["properties"]["text"] = serde_json::json!({"type":"string"});
        let response = RpcResponse::LaunchSettingsSet(LaunchSettingsWriteOutcome::Ok(
            snapshot.revisions.clone(),
        ));
        let server = Server::start(vec![snapshot_response(snapshot), response]);
        let output = server.run(&[
            "launch-settings",
            "set",
            &format!("text={input}"),
            "--runner",
            "@test/runner",
            "--json",
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            matches!(&server.calls.lock().unwrap()[1],RpcRequest::LaunchSettingsSet(r) if r.value==Some(serde_json::json!(input)) && matches!(&r.path,LaunchPath::Setting{key} if key=="text"))
        );
    }
}

#[test]
fn play_library_profiles_select_and_delete_exact_ids_without_magic_values() {
    use launch_settings::*;
    for action in ["use", "delete"] {
        let mut snapshot = test_snapshot();
        snapshot.profiles.push(LaunchProfileSnapshot {
            id: "none".into(),
            title: "A real profile".into(),
            body: serde_json::json!({}),
        });
        let revision = snapshot.revisions.clone();
        let response = if action == "use" {
            RpcResponse::LaunchProfileSelect(LaunchSettingsWriteOutcome::Ok(revision))
        } else {
            RpcResponse::LaunchProfileRemove(LaunchSettingsWriteOutcome::Ok(revision))
        };
        let server = Server::start(vec![games(), snapshot_response(snapshot), response]);
        let output = server.run(&[
            "launch-settings",
            "profiles",
            action,
            "none",
            "--game",
            "game",
            "--yes",
            "--json",
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let calls = server.calls.lock().unwrap();
        assert!(match &calls[2] {
            RpcRequest::LaunchProfileSelect(r) =>
                r.profile_id.as_deref() == Some("none") && r.expected_revision == "games-revision",
            RpcRequest::LaunchProfileRemove(r) =>
                r.profile_id == "none" && r.expected_revision == "games-revision",
            _ => false,
        });
    }
    let snapshot = test_snapshot();
    let response = RpcResponse::LaunchProfileSelect(LaunchSettingsWriteOutcome::Ok(
        snapshot.revisions.clone(),
    ));
    let server = Server::start(vec![games(), snapshot_response(snapshot), response]);
    let output = server.run(&[
        "launch-settings",
        "profiles",
        "use",
        "--none",
        "--game",
        "game",
        "--json",
    ]);
    assert!(output.status.success());
    assert!(
        matches!(&server.calls.lock().unwrap()[2],RpcRequest::LaunchProfileSelect(r) if r.profile_id.is_none())
    );
}
#[test]
fn play_library_resume_names_exact_observed_launch() {
    let server = Server::start(vec![
        active(None),
        RpcResponse::SessionResume(SessionTransitionOutcome::Ok(SessionTransitionResult {
            launch_id: "observed-launch".into(),
            state: SessionFreezerState::Running,
            changed: false,
        })),
    ]);
    let output = server.run(&["resume", "--json"]);
    assert!(output.status.success());
    assert!(
        matches!(&server.calls.lock().unwrap()[1],RpcRequest::SessionResume(r) if r.expected_launch_id.as_deref()==Some("observed-launch"))
    );
}

#[test]
fn play_library_remote_folder_selection_requires_ssh_and_never_issues_receipt() {
    let server = Server::start(vec![
        RpcResponse::IdentityStatus(IdentityStatusOutcome::Ok(IdentityStatus {
            can_switch_identity: true,
            identity: identity::IdentityState::Unowned {
                device_public_key: "11".repeat(32),
            },
            local_backup_available: false,
            retired_public_keys: vec![],
        })),
        RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(SettingsSnapshot {
            revision: "revision".into(),
            device_name: Some("Here".into()),
            person_name: None,
            player_count: 1,
            editable_setting_ids: vec![],
            plugins: vec![],
            steam_grid_db_credential: config::settings::SecretSettingStatus::NotConfigured,
        })),
        RpcResponse::PeerList(PeerListOutcome::Ok(PeerList {
            peers: vec![PeerListEntry {
                device_public_key: "22".repeat(32),
                label: "There".into(),
                state: PeerListState::Ready,
                updated_at: 1u32.into(),
                last_error: None,
            }],
        })),
    ]);
    let output = server.run(&[
        "--device",
        "There",
        "library",
        "folders",
        "add",
        "/a-path-only-valid-on-the-peer",
        "gba",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    let failure: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
    assert!(failure.message.contains("SSH or local access"));
    assert!(output.stdout.is_empty());
    assert_eq!(server.calls.lock().unwrap().len(), 3);
}

#[test]
fn play_library_profile_rename_uses_exact_id_trimmed_title_and_observed_revision() {
    use launch_settings::*;
    for conflict in [false, true] {
        let mut snapshot = test_snapshot();
        snapshot.profiles.push(LaunchProfileSnapshot {
            id: "none".into(),
            title: "Old title".into(),
            body: serde_json::json!({"cpu":{"max":40}}),
        });
        snapshot.selected_profile = Some("none".into());
        let response = RpcResponse::LaunchProfileSave(if conflict {
            LaunchSettingsWriteOutcome::Err(RpcFailure {
                code: "SettingsConflict".into(),
                message: "Changed since snapshot".into(),
            })
        } else {
            LaunchSettingsWriteOutcome::Ok(snapshot.revisions.clone())
        });
        let server = Server::start(vec![games(), snapshot_response(snapshot), response.clone()]);
        let output = server.run(&[
            "launch-settings",
            "profiles",
            "rename",
            "none",
            "  Quiet evening  ",
            "--game",
            "game",
            "--json",
        ]);
        assert_eq!(
            output.status.code(),
            Some(if conflict { 1 } else { 0 }),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
            serde_json::to_value(&response).unwrap()
        );
        let calls = server.calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert!(
            matches!(&calls[2], RpcRequest::LaunchProfileSave(r) if r.game_id == "game" && r.profile_id.as_deref() == Some("none") && r.title == "Quiet evening" && r.expected_revision == "games-revision")
        );
    }
}

#[test]
fn play_library_profile_rename_refuses_blank_title_and_unknown_profile_without_write() {
    for (id, title, expected_reads) in [
        ("existing", " \t ", 0),
        ("existing", "This title is longer than forty characters!", 0),
        ("missing", "Title", 2),
    ] {
        let server = Server::start(if expected_reads == 0 {
            vec![]
        } else {
            vec![games(), snapshot_response(test_snapshot())]
        });
        let output = server.run(&[
            "launch-settings",
            "profiles",
            "rename",
            id,
            title,
            "--game",
            "game",
            "--json",
        ]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert_eq!(server.calls.lock().unwrap().len(), expected_reads);
    }
}

#[test]
fn play_library_remote_refused_save_preflight_stays_visible_and_does_not_lock_launch() {
    use game_routes::*;
    for json in [false, true] {
        let mut responses = target_responses();
        responses.extend([
            games(),
            route_response(),
            snapshot_response(test_snapshot()),
            RpcResponse::SelectedGameLaunch(SelectedGameLaunchOutcome::Ok(SelectedGameLaunch {
                session: SessionPrepared {
                    game_id: "game".into(),
                    launch_id: "remote-launch".into(),
                },
                warnings: vec![],
            })),
        ]);
        let server = Server::start(responses);
        let mut args = vec!["--device", "There", "play", "game"];
        if json {
            args.push("--json");
        }
        let output = server.run(&args);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        if json {
            let responses: Vec<RpcResponse> = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(responses.len(), 2);
            assert!(matches!(
                &responses[0],
                RpcResponse::SavesCheck(saves::types::SaveCheckOutcome::Err(_))
            ));
            assert!(
                matches!(&responses[1], RpcResponse::SelectedGameLaunch(SelectedGameLaunchOutcome::Ok(result)) if result.session.launch_id == "remote-launch")
            );
        } else {
            let human = String::from_utf8(output.stdout).unwrap();
            assert!(
                human.contains(&format!(
                    "Launch accepted on device {}: remote-launch",
                    "22".repeat(32)
                )),
                "{human}"
            );
            assert!(
                human.contains("save check could not run on device"),
                "{human}"
            );
            assert!(human.contains("did not confirm the save state"), "{human}");
        }
        let calls = server.calls.lock().unwrap();
        assert_eq!(calls.len(), 8);
        assert!(matches!(&calls[6], RpcRequest::SavesCheck(_)));
        assert!(matches!(&calls[7], RpcRequest::SelectedGameLaunch(_)));
    }
}

fn target_responses() -> Vec<RpcResponse> {
    vec![
        RpcResponse::IdentityStatus(IdentityStatusOutcome::Ok(IdentityStatus {
            can_switch_identity: true,
            identity: identity::IdentityState::Unowned {
                device_public_key: "11".repeat(32),
            },
            local_backup_available: false,
            retired_public_keys: vec![],
        })),
        RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(SettingsSnapshot {
            revision: "revision".into(),
            device_name: Some("Here".into()),
            person_name: None,
            player_count: 1,
            editable_setting_ids: vec![],
            plugins: vec![],
            steam_grid_db_credential: config::settings::SecretSettingStatus::NotConfigured,
        })),
        RpcResponse::PeerList(PeerListOutcome::Ok(PeerList {
            peers: vec![PeerListEntry {
                device_public_key: "22".repeat(32),
                label: "There".into(),
                state: PeerListState::Ready,
                updated_at: 1u32.into(),
                last_error: None,
            }],
        })),
    ]
}

#[test]
fn play_library_remote_plugin_install_requires_selected_device_review_not_yes() {
    use korrid::plugin_admin::*;
    for (approval, installed) in [
        (None, false),
        (Some("stale"), false),
        (Some("reviewed"), true),
    ] {
        let mut responses = target_responses()
            .into_iter()
            .map(|response| serde_json::to_value(response).unwrap())
            .collect::<Vec<_>>();
        if approval.is_some() {
            responses.push(serde_json::to_value(RpcResponse::PluginInspect(PluginInspectOutcome::Ok(
                PluginInspection {
                    id: "@test:clock".into(),
                    title: None,
                    approval: "reviewed".into(),
                    warning: "native authority".into(),
                    ports: PluginPorts {
                        allowed_tcp_ports: vec![],
                        allowed_udp_ports: vec![],
                    },
                    native_units: vec![],
                    brings: vec![],
                    publisher: serde_json::from_value(serde_json::json!({"namespace":"@test","state":"Bound","fingerprint":"test AbCd EfGh IjKl MnOp"})).unwrap(),
                },
            ))).unwrap());
        }
        if installed {
            responses.push(
                serde_json::to_value(RpcResponse::PluginInstall(PluginChangeOutcome::Ok(
                    PluginChangeApplied {},
                )))
                .unwrap(),
            );
        }
        let server = Server::start_wire(tempfile::tempdir().unwrap(), responses);
        let mut args = vec![
            "--device",
            "There",
            "plugins",
            "install",
            "https://catalog.example/",
            "@test:clock",
            "v1",
            "--yes",
            "--json",
        ];
        if let Some(approval) = approval {
            args.extend(["--approval", approval]);
        }
        let output = server.run(&args);
        assert_eq!(
            output.status.code(),
            Some(if installed {
                0
            } else if approval.is_none() {
                2
            } else {
                1
            }),
            "{output:?}"
        );
        let calls = server.calls.lock().unwrap();
        assert_eq!(
            calls.len(),
            3 + usize::from(approval.is_some()) + usize::from(installed)
        );
        if installed {
            assert!(
                matches!(calls.last().unwrap(), korrid::RpcRequest::PluginInstall(request) if request.approval == "reviewed")
            );
        } else {
            assert!(!calls
                .iter()
                .any(|request| matches!(request, korrid::RpcRequest::PluginInstall(_))));
        }
    }
}

#[test]
fn parity_downloads_start_uses_exact_artifact_ids_and_native_result() {
    let response = RpcResponse::AcquisitionStart(acquisition::AcquisitionOutcome::Ok(
        acquisition::AcquisitionSnapshot { artifacts: vec![] },
    ));
    let server = Server::start(vec![response.clone()]);
    let output = server.run(&[
        "downloads",
        "start",
        "--artifact",
        "runner/program",
        "--artifact",
        "game/rom",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::to_value(response).unwrap()
    );
    let calls = server.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(
        matches!(&calls[0], RpcRequest::AcquisitionStart(r) if r.artifact_ids == ["runner/program", "game/rom"])
    );
}

#[test]
fn parity_downloads_runner_status_and_invalid_inputs_have_no_start_effects() {
    let server = Server::start(vec![
        games(),
        RpcResponse::AcquisitionStatus(acquisition::AcquisitionOutcome::Ok(
            acquisition::AcquisitionSnapshot { artifacts: vec![] },
        )),
    ]);
    let output = server.run(&[
        "downloads",
        "status",
        "--game",
        "game",
        "--runner",
        "runner/id",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = server.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(
        matches!(&calls[1], RpcRequest::AcquisitionStatus(r) if r.game_id.as_deref() == Some("game") && r.runner_id.as_deref() == Some("runner/id"))
    );
    drop(calls);
    for args in [
        vec!["downloads", "start"],
        vec!["downloads", "start", "--artifact", ""],
        vec!["downloads", "status", "--runner", "runner/id"],
    ] {
        let output = server.run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(server.calls.lock().unwrap().len(), 2);
    }
}

#[test]
fn play_library_remote_ambiguous_device_never_launches_or_installs() {
    for command in [vec!["play", "game"], vec!["plugins", "on", "@test:clock"]] {
        let mut responses = target_responses();
        if let RpcResponse::PeerList(PeerListOutcome::Ok(list)) = &mut responses[2] {
            list.peers.push(PeerListEntry {
                device_public_key: "33".repeat(32),
                ..list.peers[0].clone()
            });
        }
        let server = Server::start(responses);
        let mut args = vec!["--device", "There", "--json"];
        args.extend(command);
        let output = server.run(&args);
        assert_eq!(output.status.code(), Some(1));
        let error: RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error.code, "DeviceAmbiguous");
        assert_eq!(server.calls.lock().unwrap().len(), 3);
        assert!(output.stdout.is_empty());
    }
}
