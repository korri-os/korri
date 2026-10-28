use super::*;
use crate::relay::{
    CoordinatedRelays, CoordinationCommand, InProcessRelayNetwork, RelayCoordinator, RelayList,
};
use std::sync::atomic::{AtomicUsize, Ordering};

async fn failed_publication(devices: &Devices, endpoint: &Endpoint, peer: &str) {
    let relays = RelayList::configured(vec!["wss://signal.invalid".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    network.set_available(&relays.as_slice()[0], false);
    let signal =
        CoordinatedRelays::new(relays, devices.ca.identity_snapshot().unwrap(), network).unwrap();
    let now = unix_time();
    let result = signal
        .queue_command(
            peer,
            CoordinationCommand::ReachOffer {
                sender_device_public_key: devices.ca.public_key().unwrap(),
                offer: signal::Offer {
                    endpoint_id: endpoint.id().to_string(),
                    direct_addresses: controller::endpoint_addresses(endpoint),
                    expires_at: now + 60,
                    request_id: "ab".repeat(32),
                },
            },
            now,
        )
        .await
        .unwrap();
    assert!(!result.accepted());
    let token = devices.da.begin_peer_work(peer).unwrap();
    controller::handle_signalling_failure(&devices.da, peer, &token).await;
}

#[tokio::test]
async fn reach_failed_signalling_keeps_a_live_signed_route_ready() {
    let devices = Devices::new();
    let (a, b, outgoing, incoming) = pair().await;
    let key = devices.cb.public_key().unwrap();
    devices.da.reach.live.write().unwrap().insert(
        key.clone(),
        LiveRoute {
            connection: outgoing,
            signalled: Vec::new(),
        },
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let router = devices.router().layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::SeqCst);
                next.run(request).await
            }
        },
    ));
    let task = tokio::spawn(tunnel::serve(incoming, router));
    devices
        .da
        .set_peer_state(
            &devices.da.begin_peer_work(&key).unwrap(),
            &key,
            PeerState::Ready,
        )
        .unwrap();
    failed_publication(&devices, &a, &key).await;
    assert_eq!(
        devices
            .da
            .snapshot()
            .unwrap()
            .into_iter()
            .find(|p| p.device_public_key == key)
            .unwrap()
            .state,
        PeerState::Ready
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    task.abort();
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn reach_failed_signalling_with_a_dead_route_sets_the_sentence() {
    let devices = Devices::new();
    let (a, b, outgoing, _incoming) = pair().await;
    let key = devices.cb.public_key().unwrap();
    devices.da.reach.live.write().unwrap().insert(
        key.clone(),
        LiveRoute {
            connection: outgoing.clone(),
            signalled: Vec::new(),
        },
    );
    devices
        .da
        .set_peer_state(
            &devices.da.begin_peer_work(&key).unwrap(),
            &key,
            PeerState::Ready,
        )
        .unwrap();
    outgoing.close(0u32.into(), b"controlled lost route");
    outgoing.closed().await;
    failed_publication(&devices, &a, &key).await;
    assert_eq!(
        devices
            .da
            .snapshot()
            .unwrap()
            .into_iter()
            .find(|p| p.device_public_key == key)
            .unwrap()
            .state,
        PeerState::Failed {
            error: devices.da.unreachable_sentence(&key)
        }
    );
    a.close().await;
    b.close().await;
}
