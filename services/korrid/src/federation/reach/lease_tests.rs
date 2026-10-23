use super::*;

#[tokio::test]
async fn reach_selection_switches_exact_peer_and_drop_releases_the_ports() {
    let (a, b, ab, _ba) = pair().await;
    let (c, d, cd, _dc) = pair().await;
    let routes = Routes::default();
    *routes.bindings.write().unwrap() = Some(tunnel::Bindings {
        address: "127.0.0.1".parse().unwrap(),
        tcp: [0; 3],
        udp: [0; 5],
    });
    routes
        .live
        .write()
        .unwrap()
        .insert("A".into(), LiveRoute { connection: ab });
    routes
        .live
        .write()
        .unwrap()
        .insert("B".into(), LiveRoute { connection: cd });
    assert!(routes.stream_address("A").is_none());
    assert!(routes.stream_address("B").is_none());
    let first = routes.activate("A").await.unwrap().unwrap();
    assert!(routes.stream_address("A").is_some());
    assert!(routes.stream_address("B").is_none());
    let bindings = {
        let forwards = first.0.forwards.lock().unwrap();
        let forwards = forwards.as_ref().unwrap();
        tunnel::Bindings {
            address: "127.0.0.1".parse().unwrap(),
            tcp: forwards
                .tcp
                .iter()
                .map(|a| a.port())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            udp: forwards
                .udp
                .iter()
                .map(|a| a.port())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        }
    };
    // B must reuse the exact same ports. Keeping A's lease clone alive must
    // not keep its listeners alive after the selected peer changes.
    *routes.bindings.write().unwrap() = Some(bindings);
    let second = routes.activate("B").await.unwrap().unwrap();
    assert!(first.address().is_none());
    assert!(routes.stream_address("A").is_none());
    assert!(routes.stream_address("B").is_some());
    drop(second);
    assert!(routes.stream_address("B").is_none());
    let third = routes.activate("A").await.unwrap().unwrap();
    third.release();
    assert!(routes.stream_address("A").is_none());
    // A peer without an iroh route still retires the previous selection.
    assert!(routes.activate("LAN-only").await.unwrap().is_none());
    a.close().await;
    b.close().await;
    c.close().await;
    d.close().await;
}

#[tokio::test]
async fn reach_stream_probe_keeps_native_then_uses_iroh_and_preserves_native_failure() {
    use crate::linux_viewer::test_backend::{LinuxViewerAction, TestLinuxStreamViewer};
    let Ok(ipv6) = tokio::net::TcpListener::bind("[::1]:0").await else {
        eprintln!("skip: IPv6 loopback is unavailable");
        return;
    };
    drop(ipv6);
    let devices = Devices::new();
    *devices.da.reach.bindings.write().unwrap() = Some(tunnel::Bindings {
        address: "::1".parse().unwrap(),
        tcp: [0; 3],
        udp: [0; 5],
    });
    let (a, b, outgoing, _incoming) = pair().await;
    let key = devices.cb.public_key().unwrap();
    devices.da.reach.live.write().unwrap().insert(
        key.clone(),
        LiveRoute {
            connection: outgoing,
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let router = devices.router();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let sunshine = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let sunshine_address = sunshine.local_addr().unwrap().to_string();
    let viewer = Arc::new(TestLinuxStreamViewer::default());
    let registry = crate::upstreams::UpstreamRegistry::new_secure(
        vec![
            crate::upstreams::UpstreamHostConfig::native_secure("B", origin, key.clone())
                .with_moonlight_address(sunshine_address.clone()),
        ],
        devices.ca.clone(),
    )
    .with_federation(devices.da.clone())
    .with_linux_viewer(viewer.clone());
    let prepared = registry.prepare_stream("game", Some("B")).await.unwrap();
    assert!(devices.da.stream_address(&key).is_none());
    assert_eq!(
        viewer
            .calls()
            .iter()
            .find(|c| c.action == LinuxViewerAction::Start)
            .unwrap()
            .moonlight_address
            .as_deref(),
        Some(sunshine_address.as_str())
    );
    registry
        .session_stop(Some(&prepared.session.launch_id), false)
        .await
        .unwrap();
    // RPC stays reachable. Only Sunshine's control address now fails.
    drop(sunshine);
    let prepared = registry.prepare_stream("game", Some("B")).await.unwrap();
    let forwarded = devices.da.stream_address(&key).unwrap();
    assert!(forwarded.starts_with("[::1]:"));
    assert_eq!(tunnel::Bindings::default().tcp[1], 47989);
    registry
        .session_stop(Some(&prepared.session.launch_id), false)
        .await
        .unwrap();
    // Without iroh, let the existing viewer failure and compensation stand.
    devices.da.reach.clear();
    viewer.fail_next(
        LinuxViewerAction::Start,
        "controlled unreachable stream address",
    );
    assert!(
        matches!(registry.prepare_stream("game",Some("B")).await,Err(crate::upstreams::UpstreamError::Tagged {code,..}) if code == "ViewerStartFailed")
    );
    assert!(devices.da.stream_address(&key).is_none());
    server.abort();
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn reach_selected_session_end_and_viewer_error_release_the_lease() {
    use crate::linux_viewer::test_backend::{LinuxViewerAction, TestLinuxStreamViewer};
    let devices = Devices::new();
    let (a, b, outgoing, incoming) = pair().await;
    let task = tokio::spawn(tunnel::serve(incoming, devices.router()));
    let key = devices.cb.public_key().unwrap();
    devices.da.reach.live.write().unwrap().insert(
        key.clone(),
        LiveRoute {
            connection: outgoing,
        },
    );
    *devices.da.reach.bindings.write().unwrap() = Some(tunnel::Bindings {
        address: "127.0.0.1".parse().unwrap(),
        tcp: [0; 3],
        udp: [0; 5],
    });
    let viewer = Arc::new(TestLinuxStreamViewer::default());
    let registry = crate::upstreams::UpstreamRegistry::new_secure(
        vec![crate::upstreams::UpstreamHostConfig::native_secure(
            "B",
            "http://127.0.0.1:9".into(),
            key.clone(),
        )
        .with_moonlight_address("192.168.1.2")],
        devices.ca.clone(),
    )
    .with_federation(devices.da.clone())
    .with_linux_viewer(viewer.clone());
    assert!(devices.da.stream_address(&key).is_none());
    let prepared = registry.prepare_stream("game", Some("B")).await.unwrap();
    let address = devices.da.stream_address(&key).unwrap();
    assert_eq!(
        viewer
            .calls()
            .iter()
            .find(|c| c.action == LinuxViewerAction::Start)
            .unwrap()
            .moonlight_address
            .as_deref(),
        Some(address.as_str())
    );
    registry
        .session_stop(Some(&prepared.session.launch_id), false)
        .await
        .unwrap();
    assert!(devices.da.stream_address(&key).is_none());
    viewer.fail_next(LinuxViewerAction::Start, "controlled viewer failure");
    assert!(registry.prepare_stream("game", Some("B")).await.is_err());
    assert!(devices.da.stream_address(&key).is_none());
    task.abort();
    a.close().await;
    b.close().await;
}
