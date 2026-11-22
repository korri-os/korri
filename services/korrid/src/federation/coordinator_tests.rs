use super::*;
use crate::relay::{CoordinatedRelays, InProcessRelayNetwork, RelayCoordinator, RelayList};
use coordinator::{Discovery, DiscoveryInputs, DiscoveryTiming};
use std::time::Duration;

pub(super) fn inputs(relays: &RelayList, _publish: bool) -> DiscoveryInputs {
    DiscoveryInputs {
        relays: Some(relays.clone()),
    }
}
pub(super) fn timing() -> DiscoveryTiming {
    DiscoveryTiming {
        poll: Duration::from_millis(80),
        failure_initial: Duration::from_millis(10),
        failure_max: Duration::from_millis(40),
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
    assert_eq!(document["publication"]["generation"], 1);
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
fn production_cadence_and_bounded_failure_backoff() {
    let t = DiscoveryTiming::default();
    assert_eq!(t.poll, Duration::from_secs(60));
    assert_eq!(t.retry_delay(0), Duration::from_secs(5));
    assert_eq!(t.retry_delay(1), Duration::from_secs(10));
    assert_eq!(t.retry_delay(99), Duration::from_secs(300));
}
