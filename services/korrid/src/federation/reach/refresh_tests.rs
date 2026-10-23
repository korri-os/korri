use super::*;
use crate::{
    linux_viewer::{
        test_backend::{LinuxViewerAction, TestLinuxStreamViewer},
        LinuxViewerStatus,
    },
    relay::{
        CoordinatedRelays, CoordinationCommand, InProcessRelayNetwork, RelayCoordinator, RelayList,
    },
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[tokio::test]
async fn reach_stopped_viewer_without_native_address_keeps_a_live_forward() {
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
        address: "::1".parse().unwrap(),
        tcp: [0; 3],
        udp: [0; 5],
    });
    let viewer = Arc::new(TestLinuxStreamViewer::default());
    let registry = crate::upstreams::UpstreamRegistry::new_secure(Vec::new(), devices.ca.clone())
        .with_federation(devices.da.clone())
        .with_linux_viewer(viewer.clone());
    let prepared = registry.prepare_stream("game", Some(&key)).await.unwrap();
    assert!(devices.da.stream_address(&key).is_some());
    viewer.seed(&prepared.session.launch_id, LinuxViewerStatus::Stopped);
    registry.session_status().await.unwrap();
    let live = devices
        .da
        .stream_address(&key)
        .expect("recovery must keep a live lease");
    assert!(live.starts_with("[::1]:"));
    assert!(tokio::net::TcpStream::connect(&live).await.is_ok());
    let starts = viewer
        .calls()
        .into_iter()
        .filter(|call| call.action == LinuxViewerAction::Start)
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 2);
    assert_eq!(
        starts.last().unwrap().moonlight_address.as_deref(),
        Some(live.as_str())
    );
    registry
        .session_stop(Some(&prepared.session.launch_id), false)
        .await
        .unwrap();
    task.abort();
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn reach_registered_forward_address_is_not_native_evidence() {
    let devices = Devices::new();
    let (a, b, outgoing, _incoming) = pair().await;
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
    let first = devices.da.activate_stream(&key).await.unwrap().unwrap();
    let address = first.address().unwrap();
    let recovered = prepare_peer_viewer(Some(&devices.da), &key, Some(&address))
        .await
        .unwrap();
    assert!(
        recovered.reach.is_some(),
        "a registered listener cannot prove native reachability"
    );
    assert!(devices.da.stream_address(&key).is_some());
    recovered.reach.as_ref().unwrap().release();
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn reach_successful_background_refresh_clears_a_stalled_request_on_the_same_connection() {
    let devices = Devices::new();
    let (a, b, outgoing, incoming) = pair().await;
    let stable_id = outgoing.stable_id();
    let key = devices.cb.public_key().unwrap();
    devices.da.reach.live.write().unwrap().insert(
        key.clone(),
        LiveRoute {
            connection: outgoing.clone(),
        },
    );
    let stalled = Arc::new(AtomicBool::new(true));
    let requests = Arc::new(AtomicUsize::new(0));
    let (state, count) = (stalled.clone(), requests.clone());
    let router = devices.router().layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let (state, count) = (state.clone(), count.clone());
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                if state.load(Ordering::SeqCst) {
                    std::future::pending::<()>().await;
                }
                next.run(request).await
            }
        },
    ));
    let task = tokio::spawn(tunnel::serve(incoming, router));
    let client = crate::upstream_native::NativeClient::new_secure_candidates(
        Vec::new(),
        key.clone(),
        devices.ca.clone(),
    )
    .with_directory(devices.da.clone())
    .with_timeout(Duration::from_secs(1));
    let result = client.session_status().await;
    assert!(
        matches!(result, Err(crate::upstreams::UpstreamError::Unreachable(_))),
        "{result:?}"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    let state = || {
        devices
            .da
            .snapshot()
            .unwrap()
            .into_iter()
            .find(|peer| peer.device_public_key == key)
            .unwrap()
            .state
    };
    assert_eq!(
        state(),
        PeerState::Failed {
            error: devices.da.unreachable_sentence(&key)
        }
    );
    assert!(outgoing.close_reason().is_none());
    stalled.store(false, Ordering::SeqCst);
    let relays = RelayList::configured(vec!["wss://signal.invalid".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let sender = CoordinatedRelays::new(
        relays.clone(),
        devices.cb.identity_snapshot().unwrap(),
        network.clone(),
    )
    .unwrap();
    let receiver =
        CoordinatedRelays::new(relays, devices.ca.identity_snapshot().unwrap(), network).unwrap();
    let now = unix_time();
    assert!(sender
        .queue_command(
            &devices.ca.public_key().unwrap(),
            CoordinationCommand::ReachAnswer {
                sender_device_public_key: key.clone(),
                answer: signal::Answer {
                    endpoint_id: b.id().to_string(),
                    direct_addresses: controller::endpoint_addresses(&b),
                    expires_at: now + 60,
                    request_id: "cd".repeat(32),
                    can_relay_now: false,
                    relay_addresses: Vec::new()
                }
            },
            now
        )
        .await
        .unwrap()
        .accepted());
    let received = receiver.receive(now).await.unwrap();
    assert!(received.commands.iter().any(|command| matches!(
        command,
        CoordinationCommand::ReachAnswer {sender_device_public_key, ..}
            if sender_device_public_key == &key
    )));
    let mut tasks = tokio::task::JoinSet::new();
    assert!(controller::refresh_open_route(
        &mut tasks,
        &devices.da,
        &key
    ));
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    assert_eq!(state(), PeerState::Ready);
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    assert_eq!(
        devices.da.reach.connection(&key).unwrap().stable_id(),
        stable_id
    );
    task.abort();
    a.close().await;
    b.close().await;
}
