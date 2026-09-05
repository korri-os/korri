use super::*;
use crate::relay::{CoordinatedRelays, InProcessRelayNetwork, RelayCoordinator, RelayList};
use coordinator::{Discovery, DiscoveryInputs, DiscoveryTiming};
use std::time::Duration;

pub(super) fn inputs(relays: &RelayList, publish: bool) -> DiscoveryInputs {
    DiscoveryInputs {
        relays: Some(relays.clone()),
        advertised_endpoints: if publish {
            vec!["http://device:43117".into()]
        } else {
            vec![]
        },
        label: Some("Device".into()),
        moonlight_address: None,
    }
}
pub(super) fn timing() -> DiscoveryTiming {
    DiscoveryTiming {
        poll: Duration::from_millis(80),
        failure_initial: Duration::from_millis(10),
        failure_max: Duration::from_millis(40),
        lifetime: 86400,
        renew: 21600,
    }
}

#[tokio::test]
async fn endpoint_transport_outage_is_not_empty_success() {
    let f = Fixture::new();
    let relays = RelayList::configured(vec!["ws://localhost:7001".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let relay = CoordinatedRelays::new(
        relays.clone(),
        f.credentials.identity_snapshot().unwrap(),
        network.clone(),
    )
    .unwrap();
    assert!(!relay.receive(1000).await.unwrap().all_relay_reads_failed);
    network.set_available(&relays.as_slice()[0], false);
    assert!(relay.receive(1000).await.unwrap().all_relay_reads_failed);
}

#[tokio::test]
async fn immediate_query_only_owner_publication_and_join() {
    let f = Fixture::new();
    let directory = f.directory().unwrap();
    let relays = RelayList::configured(vec!["ws://localhost:7001".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let config = inputs(&relays, false);
    let task = Discovery::new(
        directory.clone(),
        f.credentials.clone(),
        Arc::new(move || Ok(config.clone())),
        network.clone(),
        timing(),
        Arc::new(|| 1000),
    )
    .spawn();
    tokio::time::timeout(Duration::from_secs(2), async {
        while network.stored_event_count(&relays.as_slice()[0]) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    task.shutdown().await.unwrap();
    assert_eq!(network.stored_event_count(&relays.as_slice()[0]), 1);
    let document: serde_json::Value = serde_json::from_slice(
        &std::fs::read(f.root.path().join("federation/peers.json")).unwrap(),
    )
    .unwrap();
    assert!(document["publication"].is_null());
}

#[tokio::test]
async fn new_owned_recipient_gets_current_generation_without_waiting_for_renewal() {
    let f = Fixture::new();
    let directory = f.directory().unwrap();
    let relays = RelayList::configured(vec![
        "ws://localhost:7001".into(),
        "ws://localhost:7002".into(),
    ])
    .unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let config = inputs(&relays, true);
    let task = Discovery::new(
        directory.clone(),
        f.credentials.clone(),
        Arc::new(move || Ok(config.clone())),
        network.clone(),
        timing(),
        Arc::new(|| 1000),
    )
    .spawn();
    tokio::time::sleep(Duration::from_millis(35)).await;
    let peer = CoordinatedRelays::new(relays.clone(), f.peer.clone(), network.clone()).unwrap();
    peer.publish_owner_statement().await.unwrap();
    task.control().wake().unwrap();
    let first = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(e) = peer.receive(1000).await.unwrap().endpoints.first() {
                break e.record.clone();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(first.generation, 1);
    assert_eq!(first.issued_at, 1000);
    assert_eq!(directory.snapshot().unwrap().len(), 1);
    task.shutdown().await.unwrap();
}

#[test]
fn query_only_ignores_endpoint_metadata_but_advertisements_validate_it() {
    for value in [
        "".to_owned(),
        " ".into(),
        "bad\nlabel".into(),
        "x".repeat(257),
    ] {
        for label in [true, false] {
            let mut config = DiscoveryInputs {
                relays: None,
                advertised_endpoints: vec![],
                label: label.then(|| value.clone()),
                moonlight_address: (!label).then(|| value.clone()),
            };
            assert!(config.validate().is_ok(), "query-only: {config:?}");
            config.advertised_endpoints = vec!["http://device:43117".into()];
            assert!(matches!(config.validate(), Err(FederationError::Bounds)));
        }
    }
}

#[tokio::test]
async fn linux_query_only_empty_readable_title_still_discovers_peers() {
    let f = Fixture::new();
    let readable = tempfile::tempdir().unwrap();
    fs::write(readable.path().join("device.yaml"), "host:\n  title: ''\n").unwrap();
    let config = crate::config::snapshot::ConfigSnapshotCoordinator::new(readable.path());
    let state = config.reload();
    assert_eq!(
        state.authorization,
        crate::config::snapshot::SnapshotAuthorization::Authorized
    );
    assert_eq!(
        state.snapshot.host.as_ref().unwrap().title.as_deref(),
        Some("")
    );
    let relays = RelayList::configured(vec!["ws://localhost:7001".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let peer = CoordinatedRelays::new(relays.clone(), f.peer.clone(), network.clone()).unwrap();
    peer.publish_owner_statement().await.unwrap();
    network
        .inject(&relays.as_slice()[0], &f.event(&f.endpoint(1, 1000)))
        .unwrap();
    let directory = f.directory().unwrap();
    let advertising = DiscoveryInputs::linux(&config, &inputs(&relays, true)).unwrap();
    assert!(matches!(
        advertising.validate(),
        Err(FederationError::Bounds)
    ));
    let mut initial = inputs(&relays, false);
    initial.moonlight_address = Some("".into());
    let task = Discovery::new(
        directory.clone(),
        f.credentials.clone(),
        Arc::new(move || DiscoveryInputs::linux(&config, &initial)),
        network.clone(),
        timing(),
        Arc::new(|| 1000),
    )
    .spawn();
    let discovered = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let peers = directory.snapshot().unwrap();
            if peers
                .first()
                .is_some_and(|peer| peer.current_endpoint.is_some())
            {
                break peers;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await;
    task.shutdown().await.unwrap();
    let peers = discovered.expect("Linux query-only discovery must not be blocked");
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].device_public_key, f.key());
    assert_eq!(
        peers[0].current_endpoint.as_ref().unwrap().candidates,
        f.endpoint(1, 1000).candidates
    );
    assert_eq!(network.stored_event_count(&relays.as_slice()[0]), 3);
    let document: serde_json::Value =
        serde_json::from_slice(&fs::read(f.root.path().join("federation/peers.json")).unwrap())
            .unwrap();
    assert!(document["publication"].is_null());
}

#[test]
fn production_cadence_and_bounded_failure_backoff() {
    let t = DiscoveryTiming::default();
    assert_eq!(t.poll, Duration::from_secs(60));
    assert_eq!(t.retry_delay(0), Duration::from_secs(5));
    assert_eq!(t.retry_delay(1), Duration::from_secs(10));
    assert_eq!(t.retry_delay(99), Duration::from_secs(300));
    assert_eq!((t.lifetime, t.renew), (86400, 21600));
}
