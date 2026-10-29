use super::*;
use crate::host::{control::InMemoryLaunchUnitBackend, LaunchUnitBackend, LaunchUnitState};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    task::JoinHandle,
};

struct Fixture {
    state: AppState,
    holder: host::HostRuntime,
    backend: Arc<InMemoryLaunchUnitBackend>,
    launch: String,
    held: String,
    address: std::net::SocketAddr,
    monitor: JoinHandle<()>,
    http: JoinHandle<()>,
    status_delay_ms: Arc<AtomicU64>,
    status_fails: Arc<AtomicBool>,
    _roots: [tempfile::TempDir; 2],
    _route: federation::reach::tests::NetplayRoute,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.monitor.abort();
        self.http.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        fn device(
            root: &std::path::Path,
            backend: Arc<InMemoryLaunchUnitBackend>,
        ) -> host::HostRuntime {
            use sha2::{Digest, Sha256};
            let key = format!("sha256:{}", hex::encode(Sha256::digest(b"game bytes")));
            std::fs::create_dir_all(root.join("roms")).unwrap();
            std::fs::create_dir_all(root.join("catalog")).unwrap();
            std::fs::write(root.join("roms/Game.sfc"), b"game bytes").unwrap();
            std::fs::write(
                root.join("device.yaml"),
                format!("host:\n  title: Device\nlocations:\n  '{key}':\n    - storage: roms\n      path: Game.sfc\n"),
            )
            .unwrap();
            std::fs::write(root.join("catalog/library.yaml"), format!("library:\n  game:\n    title: Game\n    releases:\n      - id: snes\n        system: snes\n        identity: {{kind: hash, value: '{key}'}}\n")).unwrap();
            let package = root.join("plugin");
            std::fs::create_dir(&package).unwrap();
            std::fs::write(package.join("plugin.ts"), r#"
                export const name='core';
                export const systems={snes:{id:'snes',title:'Super Nintendo',content:{extensions:['sfc']}}};
                export const runners={core:{id:'@test:core/core',program:'program',systems:['snes'],netplay:{library:'Snes9x 2010',version:'1.52.4fe690dd'}}};
                export const handlers={'launch.prepare':input=>({command:input.program,args:[]})};
            "#).unwrap();
            std::fs::write(
                package.join("manifest.json"),
                r#"{"entry":"plugin.ts","sources":["plugin.ts"]}"#,
            )
            .unwrap();
            let registry = crate::plugin::PluginRegistry::from_installed(vec![
                crate::plugin_installation::EnabledPackage {
                    id: "@test:core".into(),
                    package,
                    entry: "plugin.ts".into(),
                    sources: vec!["plugin.ts".into()],
                    requires: vec![],
                    hosts: vec![],
                    files: std::collections::BTreeMap::from([(
                        "program".into(),
                        std::env::current_exe().unwrap(),
                    )]),
                },
            ])
            .unwrap();
            let config = root.join("host.toml");
            std::fs::write(&config, "label = \"Device\"\ngames = []\n").unwrap();
            let host = host::HostRuntime::from_paths_with_backend(
                &config,
                Some(root.into()),
                root.join("private"),
                backend,
            )
            .with_route_registry(root.into(), registry);
            host.claim_save_owner();
            host
        }
        let roots = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
        let backend = Arc::new(InMemoryLaunchUnitBackend::default());
        let holder = device(
            roots[0].path(),
            Arc::new(InMemoryLaunchUnitBackend::default()),
        );
        let receiver = device(roots[1].path(), backend.clone());
        let held = holder
            .prepare("game", holder.owner_public_key().as_deref())
            .await
            .unwrap()
            .session
            .launch_id;
        let launch = receiver
            .prepare("game", receiver.owner_public_key().as_deref())
            .await
            .unwrap()
            .session
            .launch_id;
        let facts = holder.netplay_local(&held).unwrap().facts;
        let (mut state, _) = app_states(receiver);
        let context = state.game_context().unwrap();
        assert_eq!(
            release_game(&context, &facts.release_key).as_deref(),
            Some("game")
        );
        assert_eq!(core(&context, &facts).as_deref(), Some("@test:core/core"));
        let mut route = federation::reach::tests::netplay_route().await;
        state.federation = Some(route.directory.clone());
        let forward = route.forward.take().unwrap();
        let address = forward.address;
        let prepared = SessionNetplayPrepared {
            seat: 2,
            port: 55435,
            password: "testpassword".into(),
            release_key: facts.release_key.clone(),
            release_bytes: facts.release_bytes,
            runner: facts.runner.clone(),
        };
        let (holder_state, _) = app_states(holder.clone());
        let reply = prepared.clone();
        let router = axum::Router::new().route(
            "/rpc",
            axum::routing::post(move |axum::Json(request): axum::Json<RpcRequest>| {
                let state = holder_state.clone();
                let reply = reply.clone();
                async move {
                    // The holder's prepared seat and credential stay the same on rejoin.
                    let response = if matches!(request, RpcRequest::SessionNetplay(_)) {
                        RpcResponse::SessionNetplay(SessionNetplayOutcome::Ok(reply))
                    } else {
                        crate::dispatch(
                            &state,
                            &authorization::AuthorizationContext::LocalBrowser,
                            request,
                        )
                        .await
                        .unwrap()
                    };
                    axum::Json(response)
                }
            }),
        );
        let status_delay_ms = Arc::new(AtomicU64::new(0));
        let status_fails = Arc::new(AtomicBool::new(false));
        let delay = status_delay_ms.clone();
        let fails = status_fails.clone();
        let router = router.layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let delay = delay.clone();
                let fails = fails.clone();
                async move {
                    tokio::time::sleep(Duration::from_millis(delay.load(Ordering::SeqCst))).await;
                    if fails.load(Ordering::SeqCst) {
                        return axum::response::IntoResponse::into_response(
                            axum::http::StatusCode::SERVICE_UNAVAILABLE,
                        );
                    }
                    next.run(request).await
                }
            },
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let http = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        state.remote = Some(upstreams::UpstreamRegistry::new(vec![
            upstreams::UpstreamHostConfig::native_secure(
                "Holder",
                url.clone(),
                route.device.clone(),
            ),
        ]));
        let peer = upstream_native::NativeClient::new(url);
        let invite = invites::Invite {
            invite_id: "invite".into(), device_public_key: route.device.clone(),
            device_label: "Holder".into(), person_name: "Owner".into(), netplay: Some(InviteNetplay::Ready),
            session: serde_json::from_value(serde_json::json!({"launchId":held,"gameId":"game","title":"Game","phase":"running","netplay":facts})).unwrap(),
        };
        let status = NetplayStatus {
            launch_id: launch.clone(),
            device_label: "Holder".into(),
            game_title: Some("Game".into()),
            phase: NetplayPhase::Connecting,
        };
        let active = serde_json::from_value(
            serde_json::json!({"launchId":launch,"gameId":"game","phase":"running"}),
        )
        .unwrap();
        let (attempt, cancelled) = state.netplay_receiver.begin(status).unwrap();
        state.netplay_receiver.activate(&attempt, active).unwrap();
        let monitor = tokio::spawn(monitor(
            state.clone(),
            invite,
            peer,
            MonitoredCopy {
                attempt_id: attempt,
                launch_id: launch.clone(),
                forward,
            },
            prepared,
            cancelled,
        ));
        invites::device::spawn_watch(state.clone());
        Self {
            state,
            holder,
            backend,
            launch,
            held,
            address,
            monitor,
            http,
            status_delay_ms,
            status_fails,
            _roots: roots,
            _route: route,
        }
    }
    async fn status(&self) -> SessionStatus {
        let response = crate::dispatch(
            &self.state,
            &authorization::AuthorizationContext::LocalBrowser,
            RpcRequest::SessionStatus(SessionStatusRequest {}),
        )
        .await
        .unwrap();
        let RpcResponse::SessionStatus(SessionStatusOutcome::Ok(status)) = response else {
            panic!("status failed");
        };
        status
    }
    async fn connected_client(&self) -> tokio::net::TcpStream {
        let mut client = tokio::net::TcpStream::connect(self.address).await.unwrap();
        client.write_all(b"netplay").await.unwrap();
        let mut bytes = [0; 7];
        tokio::time::timeout(Duration::from_secs(3), client.read_exact(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&bytes, b"netplay");
        tokio::time::timeout(Duration::from_secs(3), async {
            while !matches!(
                self.status().await.netplay.unwrap().phase,
                NetplayPhase::Playing
            ) {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        client
    }
    async fn wait_until_left(&self, launches: usize) {
        tokio::time::timeout(Duration::from_secs(4), async {
            while self.state.netplay_receiver.busy() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("deliberate quit must release the receiver instead of reconnecting");
        assert!(invites::device::idle(&self.state).await);
        assert_eq!(
            self.backend.launch_count(),
            launches,
            "a quit must never relaunch the client"
        );
        assert!(matches!(
            self.holder.session_status().await.unwrap(),
            host::control::HostSessionStatus::Running { .. }
        ));
        let response = crate::dispatch(
            &self.state,
            &authorization::AuthorizationContext::LocalBrowser,
            RpcRequest::SessionStatus(SessionStatusRequest {}),
        )
        .await
        .unwrap();
        assert!(
            matches!(&response, RpcResponse::SessionStatus(SessionStatusOutcome::Ok(status)) if status.active.is_none())
                || matches!(&response, RpcResponse::SessionStatus(SessionStatusOutcome::Err(error)) if error.code == "SessionCompleted"),
            "the receiver must no longer expose an active copy: {response:?}"
        );
        tokio::time::timeout(Duration::from_secs(2), async {
            while !self.monitor.is_finished() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("a deliberate Leave must retire its reconnect monitor");
    }
    async fn take_part_again_from_invite(&self) {
        let invite = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let InvitesListOutcome::Ok(list) = invites::device::list(&self.state).await else {
                    panic!("list failed")
                };
                if let Some(invite) = list.invites.into_iter().next() {
                    break invite;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("the holder's invite must return after Leave");
        assert_eq!(invite.session.launch_id, self.held);
        assert_eq!(invite.netplay, Some(InviteNetplay::Ready));
        let response = crate::dispatch(
            &self.state,
            &authorization::AuthorizationContext::LocalBrowser,
            RpcRequest::InvitesNetplay(InviteRequest {
                invite_id: invite.invite_id,
            }),
        )
        .await
        .unwrap();
        assert!(
            matches!(
                response,
                RpcResponse::InvitesNetplay(SessionStreamPlayOutcome::Ok(_))
            ),
            "explicit Play here must start another copy: {response:?}"
        );
        assert_eq!(self.backend.launch_count(), 2);
        let status = self.status().await;
        let response = crate::dispatch(
            &self.state,
            &authorization::AuthorizationContext::LocalBrowser,
            RpcRequest::SessionLeave(SessionLeaveRequest {
                expected_launch_id: Some(status.active.unwrap().launch_id),
            }),
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            RpcResponse::SessionLeave(SessionTransitionOutcome::Ok(_))
        ));
    }
    async fn holder_ends_and_copy_follows(&self) {
        let (_, holder) = app_states(self.holder.clone());
        stop_via_inputd(holder, self._roots[0].path()).await;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if self.backend.state(&self.launch).unwrap() == LaunchUnitState::Completed
                    && self.status().await.stream_play_ended.is_some()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("the original monitor must still end its copy with the holder");
        let status = self.status().await;
        assert!(status.active.is_none());
        assert!(status.netplay.is_none());
        assert!(status.stream_play_ended.is_some());
    }
}

#[tokio::test]
async fn netplay_second_start_refusal_keeps_the_first_copy_monitored() {
    let fixture = Fixture::new().await;
    let _client = fixture.connected_client().await;
    let result = start(
        &fixture.state,
        &InviteRequest {
            invite_id: "second-invite".into(),
        },
    )
    .await;
    assert!(
        matches!(result, SessionStreamPlayOutcome::Err(RpcFailure { code, .. }) if code == "ActiveSessionConflict")
    );
    fixture.holder_ends_and_copy_follows().await;
}

#[tokio::test]
async fn netplay_failed_leave_keeps_the_copy_monitored() {
    let fixture = Fixture::new().await;
    let _client = fixture.connected_client().await;
    fixture.backend.configure_stop(false, true, None);
    let response = crate::dispatch(
        &fixture.state,
        &authorization::AuthorizationContext::LocalBrowser,
        RpcRequest::SessionLeave(SessionLeaveRequest {
            expected_launch_id: Some(fixture.launch.clone()),
        }),
    )
    .await
    .unwrap();
    assert!(matches!(
        response,
        RpcResponse::SessionLeave(SessionTransitionOutcome::Err(_))
    ));
    fixture.backend.configure_stop(true, false, None);
    fixture.holder_ends_and_copy_follows().await;
}

#[tokio::test]
async fn netplay_delayed_first_connection_plays_without_freezing_the_client() {
    let fixture = Fixture::new().await;
    // RetroArch cannot open its TCP connection while its unit is frozen.
    // Delay that connection past two monitor ticks, then use the real forward.
    tokio::time::sleep(Duration::from_millis(1150)).await;
    assert_eq!(
        fixture.backend.freeze_count(),
        0,
        "initial connection must not freeze RetroArch"
    );
    assert!(matches!(
        fixture.status().await.netplay.unwrap().phase,
        NetplayPhase::Connecting
    ));
    let _client = fixture.connected_client().await;
    assert_eq!(
        fixture.backend.freeze_count(),
        0,
        "the client was never frozen before Playing"
    );
    fixture.holder_ends_and_copy_follows().await;
}

#[tokio::test]
async fn netplay_slow_holder_status_does_not_freeze_a_connected_client() {
    let fixture = Fixture::new().await;
    let mut client = fixture.connected_client().await;
    fixture.status_delay_ms.store(1750, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        fixture.backend.freeze_count(),
        0,
        "a slow status poll must not freeze RetroArch"
    );
    assert_eq!(fixture.backend.launch_count(), 1);
    assert!(matches!(
        fixture.status().await.netplay.unwrap().phase,
        NetplayPhase::Playing
    ));
    client.write_all(b"still connected").await.unwrap();
    let mut bytes = [0; 15];
    tokio::time::timeout(Duration::from_secs(2), client.read_exact(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&bytes, b"still connected");
    fixture.status_delay_ms.store(0, Ordering::SeqCst);
    fixture.holder_ends_and_copy_follows().await;
}

#[tokio::test]
async fn netplay_failed_holder_status_does_not_freeze_a_connected_client() {
    let fixture = Fixture::new().await;
    let _client = fixture.connected_client().await;
    fixture.status_fails.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        fixture.backend.freeze_count(),
        0,
        "a failed poll is not a lost bridge"
    );
    assert_eq!(fixture.backend.launch_count(), 1);
    assert!(matches!(
        fixture.status().await.netplay.unwrap().phase,
        NetplayPhase::Playing
    ));
    fixture.status_fails.store(false, Ordering::SeqCst);
    fixture.holder_ends_and_copy_follows().await;
}

#[tokio::test]
async fn netplay_leave_uses_the_observed_identity_after_a_reconnect_relaunch() {
    let fixture = Fixture::new().await;
    let client = fixture.connected_client().await;
    let observed = fixture.status().await.active.unwrap().launch_id;
    drop(client);
    tokio::time::timeout(Duration::from_secs(4), async {
        while fixture.backend.launch_count() < 2 {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("a lost bridge must relaunch the native client");
    let status = fixture.status().await;
    assert!(matches!(
        status.netplay.unwrap().phase,
        NetplayPhase::Reconnecting
    ));
    let replacement = if let ServerMode::Host(host) = &fixture.state.mode {
        let SessionStatusOutcome::Ok(native) =
            host_session_status_outcome(host.session_status().await)
        else {
            panic!("native status failed");
        };
        native.active.unwrap().launch_id
    } else {
        unreachable!()
    };
    assert_ne!(replacement, fixture.launch);
    let refused = crate::dispatch(
        &fixture.state,
        &authorization::AuthorizationContext::LocalBrowser,
        RpcRequest::SessionLeave(SessionLeaveRequest {
            expected_launch_id: Some("another-run".into()),
        }),
    )
    .await
    .unwrap();
    assert!(
        matches!(refused, RpcResponse::SessionLeave(SessionTransitionOutcome::Err(RpcFailure { code, .. })) if code == "StaleLaunchIdentity")
    );
    let response = crate::dispatch(
        &fixture.state,
        &authorization::AuthorizationContext::LocalBrowser,
        RpcRequest::SessionLeave(SessionLeaveRequest {
            expected_launch_id: Some(observed.clone()),
        }),
    )
    .await
    .unwrap();
    assert!(
        matches!(
            response,
            RpcResponse::SessionLeave(SessionTransitionOutcome::Ok(_))
        ),
        "Leave with the observed identity must stop the replacement: {response:?}"
    );
    assert_eq!(status.active.unwrap().launch_id, observed);
    assert!(!fixture.state.netplay_receiver.busy());
    assert_eq!(
        fixture.backend.state(&replacement).unwrap(),
        LaunchUnitState::Completed
    );
    assert_eq!(
        fixture.backend.freeze_count(),
        0,
        "reconnecting must not freeze the client"
    );
    assert!(matches!(
        fixture.holder.session_status().await.unwrap(),
        host::control::HostSessionStatus::Running { .. }
    ));
}

#[tokio::test]
async fn netplay_timed_out_holder_status_keeps_the_last_connected_observation() {
    let fixture = Fixture::new().await;
    let _client = fixture.connected_client().await;
    fixture.status_delay_ms.store(4500, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(8)).await;
    assert_eq!(fixture.backend.freeze_count(), 0);
    assert_eq!(fixture.backend.launch_count(), 1);
    assert!(matches!(
        fixture.status().await.netplay.unwrap().phase,
        NetplayPhase::Playing
    ));
    fixture.status_delay_ms.store(0, Ordering::SeqCst);
    fixture.holder_ends_and_copy_follows().await;
}

#[tokio::test]
async fn netplay_holder_left_waits_through_failed_polls_then_resumes() {
    let fixture = Fixture::new().await;
    let _client = fixture.connected_client().await;
    fixture.holder.session_freeze(&fixture.held).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !matches!(
            fixture.status().await.netplay.unwrap().phase,
            NetplayPhase::HolderLeft
        ) {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let thaws = fixture.backend.thaw_count();
    fixture.status_fails.store(true, Ordering::SeqCst);
    // Waiting for the holder is not part of the ten-second reconnect budget.
    tokio::time::sleep(Duration::from_secs(11)).await;
    assert!(matches!(
        fixture.status().await.netplay.unwrap().phase,
        NetplayPhase::HolderLeft
    ));
    assert_eq!(fixture.backend.thaw_count(), thaws);
    assert_eq!(fixture.backend.launch_count(), 1);
    fixture.status_fails.store(false, Ordering::SeqCst);
    fixture.holder.session_thaw(&fixture.held).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !matches!(
            fixture.status().await.netplay.unwrap().phase,
            NetplayPhase::Playing
        ) {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert!(fixture.backend.thaw_count() > thaws);
    fixture.holder_ends_and_copy_follows().await;
}

#[tokio::test]
async fn netplay_lost_bridge_expires_even_when_holder_status_is_slow() {
    let fixture = Fixture::new().await;
    let client = fixture.connected_client().await;
    fixture.status_delay_ms.store(4500, Ordering::SeqCst);
    let lost_at = tokio::time::Instant::now();
    drop(client);
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if let Some(ended) = fixture.status().await.stream_play_ended {
                assert!(matches!(
                    ended.cause,
                    stream_play::StreamPlayEndCause::DeviceUnreachable
                ));
                assert_eq!(ended.launch_id, fixture.launch);
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("transport reconnect must not wait for the slow status RPC");
    assert!(lost_at.elapsed() >= RECONNECT_WINDOW);
    assert_eq!(fixture.backend.launch_count(), 2);
    assert_eq!(fixture.backend.freeze_count(), 0);
    assert!(matches!(
        fixture.holder.session_status().await.unwrap(),
        host::control::HostSessionStatus::Running { .. }
    ));
}

#[tokio::test]
async fn netplay_lost_iroh_route_expires_even_while_waiting_for_the_holder() {
    let fixture = Fixture::new().await;
    let _client = fixture.connected_client().await;
    fixture.holder.session_freeze(&fixture.held).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !matches!(
            fixture.status().await.netplay.unwrap().phase,
            NetplayPhase::HolderLeft
        ) {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let connection = fixture
        .state
        .federation
        .as_ref()
        .unwrap()
        .reach_connection(&fixture._route.device)
        .await
        .unwrap();
    fixture.status_fails.store(true, Ordering::SeqCst);
    let lost_at = tokio::time::Instant::now();
    connection.close(0u32.into(), b"test route loss");
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if let Some(ended) = fixture.status().await.stream_play_ended {
                assert!(matches!(
                    ended.cause,
                    stream_play::StreamPlayEndCause::DeviceUnreachable
                ));
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("losing the iroh route must still have a reconnect deadline");
    assert!(lost_at.elapsed() >= RECONNECT_WINDOW);
    assert_eq!(fixture.backend.launch_count(), 1);
}

#[tokio::test]
async fn netplay_kill_chord_exact_end_leaves_only_the_receiver_and_can_rejoin() {
    let fixture = Fixture::new().await;
    let client = fixture.connected_client().await;
    stop_via_inputd(fixture.state.clone(), fixture._roots[1].path()).await;
    drop(client);
    fixture.wait_until_left(1).await;
    fixture.take_part_again_from_invite().await;
}

async fn stop_via_inputd(state: AppState, root: &std::path::Path) {
    // Use inputd's real status -> exact End sequence over the private socket.
    let path = root.join("control.sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let mut local = state;
    local.rpc_surface = RpcSurface::LocalControl;
    let router = axum::Router::new()
        .route("/rpc", axum::routing::post(crate::rpc))
        .with_state(local);
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let inputd = korri_inputd::korrid_client::KorridClient::new(path);
    let result = inputd.stop_active_exact().await;
    server.abort();
    assert!(
        matches!(
            result,
            Ok(korri_inputd::korrid_client::ExactStopOutcome::Completed)
        ),
        "the exact stop must complete: {result:?}"
    );
}

#[tokio::test]
async fn netplay_holder_kill_chord_ends_the_session_and_the_receiving_copy() {
    let fixture = Fixture::new().await;
    let _client = fixture.connected_client().await;
    fixture.holder_ends_and_copy_follows().await;
}

#[tokio::test]
async fn netplay_receiver_kill_chord_after_reconnect_stops_the_replacement() {
    let fixture = Fixture::new().await;
    let client = fixture.connected_client().await;
    drop(client);
    tokio::time::timeout(Duration::from_secs(4), async {
        while fixture.backend.launch_count() < 2 {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("a broken link must still reconnect");
    stop_via_inputd(fixture.state.clone(), fixture._roots[1].path()).await;
    fixture.wait_until_left(2).await;
}

#[tokio::test]
async fn netplay_receiver_exact_end_refuses_missing_or_stale_launch_identity() {
    let fixture = Fixture::new().await;
    let _client = fixture.connected_client().await;
    for expected in [None, Some("another-run".into())] {
        let response = crate::dispatch(
            &fixture.state,
            &authorization::AuthorizationContext::LocalUnixControl,
            RpcRequest::SessionEnd(SessionEndRequest {
                expected_launch_id: expected,
                force: None,
            }),
        )
        .await
        .unwrap();
        assert!(
            matches!(response, RpcResponse::SessionEnd(SessionEndOutcome::Err(error))
            if error.code == "StaleLaunchIdentity")
        );
    }
    assert!(fixture.state.netplay_receiver.busy());
    assert_eq!(fixture.backend.launch_count(), 1);
    fixture.holder_ends_and_copy_follows().await;
}
