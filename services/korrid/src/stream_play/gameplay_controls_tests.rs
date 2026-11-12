//! The real local dispatcher, selected native peer, overlay and plugin executor.
use super::*;
use crate::{
    host::{control::InMemoryLaunchUnitBackend, CompositorControl, HostRuntime},
    linux_viewer::test_backend::TestLinuxStreamViewer,
    surface::{korrid::Korrid, overlay::OverlayController},
};
use korri_surface_treaty::{SurfacePresentation, SurfaceStatus};
use std::{net::UdpSocket, sync::Mutex, thread, time::Duration};

#[derive(Debug)]
struct GameFocus;
impl CompositorControl for GameFocus {
    fn tree(&self) -> Result<String, String> {
        Ok(r#"{"nodes":[{"id":2,"pid":9100,"focused":true}]}"#.into())
    }
    fn focus(&self, _: i64) -> Result<(), String> {
        Ok(())
    }
}

#[tokio::test]
async fn streamed_plugin_controls_reach_the_overlay_and_invoke_the_holder_plugin() {
    let root = tempfile::tempdir().unwrap();
    crate::config::test_fixtures::gba(root.path());
    std::fs::create_dir(root.path().join("roms")).unwrap();
    std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let plugins = crate::plugin_test_fixtures::installed(root.path());
    let config = root.path().join("host.toml");
    std::fs::write(&config, "label = \"holder\"\ngames = []\n").unwrap();
    let units = Arc::new(InMemoryLaunchUnitBackend::default());
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let port = socket.local_addr().unwrap().port();
    let holder = HostRuntime::from_paths_with_backend(
        &config,
        Some(root.path().into()),
        root.path().join("private"),
        units.clone(),
    )
    .with_route_registry(root.path().into(), plugins)
    .with_retroarch_control_port(port)
    .with_compositor(Arc::new(GameFocus));
    holder.claim_save_owner();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = crate::plain_host_routers_for_tests(holder.clone()).0;
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let peers = upstreams::UpstreamRegistry::new(vec![upstreams::UpstreamHostConfig::native(
        "holder",
        format!("http://{address}"),
    )])
    .with_linux_viewer(Arc::new(TestLinuxStreamViewer::default()));
    let launch = peers
        .prepare_stream(crate::config::test_fixtures::GBA_ID, Some("holder"))
        .await
        .unwrap()
        .session
        .launch_id;
    units.set_window_pids(&launch, [9100].into());
    // The desktop OverlayController expects a frozen session. The Android
    // controller uses these same controls while Leave keeps the streamed game live.
    holder.session_freeze(&launch).await.unwrap();
    let expected_controls = holder.session_controls(&launch).await.unwrap();

    let local_root = tempfile::tempdir().unwrap();
    let (brain, _) = crate::brain_app_state(
        "cap",
        "http://shell.local",
        local_root.path(),
        local_root.path().join("private"),
        crate::plugin_policy::RegistrySource::Installed,
        crate::config::snapshot::ConfigSnapshotCoordinator::new(local_root.path()),
        crate::discovery::FolderSelectionGrantStore::default(),
        Some(peers.clone()),
        None,
        None,
    );
    let local_config = local_root.path().join("host.toml");
    std::fs::write(&local_config, "label = \"local\"\ngames = []\n").unwrap();
    let local = HostRuntime::from_paths_with_backend(
        &local_config,
        None,
        local_root.path().join("host-private"),
        Arc::new(InMemoryLaunchUnitBackend::default()),
    );
    let (mut host, _) = crate::app_states(local);
    host.remote = Some(peers);

    let commands = Arc::new(Mutex::new(Vec::new()));
    let received = commands.clone();
    let control_server = thread::spawn(move || {
        // Invoke once from each local mode, using the real UDP executor and
        // RetroArch's existing GET_STATUS/MENU_TOGGLE/readback protocol.
        for _ in 0..2 {
            for (expected, reply) in [
                (
                    "GET_STATUS",
                    Some("GET_STATUS PLAYING gba,wl4.gba,crc32=1a2b3c4d\n"),
                ),
                ("MENU_TOGGLE", None),
                (
                    "GET_STATUS",
                    Some("GET_STATUS PAUSED gba,wl4.gba,crc32=1a2b3c4d\n"),
                ),
            ] {
                let mut buffer = [0; 256];
                let (length, peer) = socket.recv_from(&mut buffer).unwrap();
                let command = std::str::from_utf8(&buffer[..length]).unwrap().trim();
                assert_eq!(command, expected);
                received.lock().unwrap().push(command.to_owned());
                if let Some(reply) = reply {
                    socket.send_to(reply.as_bytes(), peer).unwrap();
                }
            }
        }
    });
    for state in [brain, host] {
        let send = Arc::new(move |request| {
            let state = state.clone();
            Box::pin(async move {
                crate::dispatch(&state, &AuthorizationContext::LocalSurfaceHost, request)
                    .await
                    .map_err(|_| crate::surface::korrid::Refusal::Denied)
            }) as crate::surface::korrid::Reply
        });
        let overlay = OverlayController::new(
            launch.clone(),
            None,
            Korrid::new(send.clone()),
            Arc::new(|| {}),
            Box::new(|_| {}),
        );
        overlay.refresh().await;
        let model = overlay.model();
        assert_eq!(model.status, SurfaceStatus::Browsing, "{model:?}");
        let SurfacePresentation::GameplayOverlay(presentation) = model.presentation else {
            panic!("expected gameplay overlay")
        };
        assert_eq!(presentation.title, expected_controls.title);
        assert_eq!(presentation.groups.len(), 1);
        assert_eq!(presentation.groups[0].id, "@korri:mgba");
        assert_eq!(
            presentation.groups[0]
                .controls
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["@korri:mgba/open-menu", "@korri:mgba/quit"]
        );
        assert!(!presentation.groups[0]
            .controls
            .iter()
            .any(|c| c.label == "Pause"));
        // A failed plugin invocation must keep its typed peer refusal, rather
        // than turning UnknownControl or InvalidValue into a wire failure.
        for (control_id, value, reason) in [
            (
                "@korri:mgba/absent",
                None,
                SessionControlFailureReason::UnknownControl,
            ),
            (
                "@korri:mgba/open-menu",
                Some(SessionControlValue::Toggle(true)),
                SessionControlFailureReason::InvalidValue,
            ),
        ] {
            let response = send(RpcRequest::SessionControlInvoke(
                SessionControlInvokeRequest {
                    launch_id: launch.clone(),
                    control_id: control_id.into(),
                    value,
                },
            ))
            .await
            .unwrap();
            assert!(
                matches!(response, RpcResponse::SessionControlInvoke(SessionControlInvokeOutcome::Err(f)) if f.reason == reason)
            );
        }
        // This is the controller called by SurfaceRequest::InvokeGameplayControl.
        overlay.invoke("@korri:mgba/open-menu".into(), None).await;
        assert!(!matches!(
            overlay.model().status,
            SurfaceStatus::Problem { .. }
        ));
        holder.session_freeze(&launch).await.unwrap();
        overlay.destroy();
    }
    control_server.join().unwrap();
    assert_eq!(
        commands.lock().unwrap().as_slice(),
        [
            "GET_STATUS",
            "MENU_TOGGLE",
            "GET_STATUS",
            "GET_STATUS",
            "MENU_TOGGLE",
            "GET_STATUS"
        ]
    );
    server.abort();
}
