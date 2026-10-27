use super::*;
use crate::host::{control::InMemoryLaunchUnitBackend, LaunchUnitBackend, LaunchUnitState};
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
            let config = root.join("host.toml");
            std::fs::write(&config, "label = \"Device\"\ngames = [{ id = \"game\", title = \"Game\", command = [\"game\"] }]\n").unwrap();
            let host = host::HostRuntime::from_paths_with_backend(
                &config,
                None,
                root.join("private"),
                backend,
            );
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
            .prepare("game", None)
            .await
            .unwrap()
            .session
            .launch_id;
        let launch = receiver
            .prepare("game", None)
            .await
            .unwrap()
            .session
            .launch_id;
        let (mut state, _) = app_states(receiver);
        let mut route = federation::reach::tests::netplay_route().await;
        state.federation = Some(route.directory.clone());
        let forward = route.forward.take().unwrap();
        let address = forward.address;
        let (router, _) = plain_host_routers_for_tests(holder.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let http = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let peer = upstream_native::NativeClient::new(url);
        let facts = super::tests::facts();
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
        let prepared = SessionNetplayPrepared {
            seat: 2,
            port: 55435,
            password: "test-password".into(),
            release_key: facts.release_key,
            release_bytes: facts.release_bytes,
            runner: facts.runner,
        };
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
        Self {
            state,
            holder,
            backend,
            launch,
            held,
            address,
            monitor,
            http,
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
    async fn holder_ends_and_copy_follows(&self) {
        assert!(self.holder.session_stop(&self.held).await.is_ok());
        tokio::time::timeout(Duration::from_secs(3), async {
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
