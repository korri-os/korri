use super::coordinator_tests::{inputs, timing};
use super::*;
use crate::relay::{CoordinatedRelays, InProcessRelayNetwork, RelayCoordinator, RelayList};
use coordinator::Discovery;
use std::time::Duration;

#[tokio::test]
async fn relay_config_reload_wake_uses_new_authorized_source_without_waiting_for_poll() {
    let f = Fixture::new();
    let readable = tempfile::tempdir().unwrap();
    fs::write(
        readable.path().join("device.yaml"),
        "host:\n  relays: [ws://localhost:7001]\n",
    )
    .unwrap();
    let config = crate::config::snapshot::ConfigSnapshotCoordinator::new(readable.path());
    let relays = RelayList::configured(vec![
        "ws://localhost:7001".into(),
        "ws://localhost:7002".into(),
    ])
    .unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let task = Discovery::new(
        f.directory().unwrap(),
        f.credentials.clone(),
        Arc::new(move || coordinator::DiscoveryInputs::query_only(&config)),
        network.clone(),
        coordinator::DiscoveryTiming::default(),
        Arc::new(|| 1000),
    )
    .spawn();
    tokio::time::timeout(Duration::from_secs(3), async {
        while network.stored_event_count(&relays.as_slice()[0]) == 0 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    fs::write(
        readable.path().join("device.yaml"),
        "host:\n  relays: [ws://localhost:7002]\n",
    )
    .unwrap();
    task.control().wake().unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while network.stored_event_count(&relays.as_slice()[1]) == 0 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    task.shutdown().await.unwrap();
}

#[tokio::test]
async fn endpoint_binding_is_published_once_and_only_changes_with_the_iroh_key() {
    let f = Fixture::new();
    let directory = f.directory().unwrap();
    let relays = RelayList::configured(vec!["ws://localhost:7001".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let recipient =
        CoordinatedRelays::new(relays.clone(), f.peer.clone(), network.clone()).unwrap();
    recipient.publish_owner_statement().await.unwrap();
    let config = inputs(&relays, true);
    let clock = f.clock.clone();
    let task = Discovery::new(
        directory,
        f.credentials.clone(),
        Arc::new(move || Ok(config.clone())),
        network,
        timing(),
        Arc::new(move || clock.load(Ordering::SeqCst)),
    )
    .spawn();
    async fn binding(
        recipient: &CoordinatedRelays<InProcessRelayNetwork>,
        generation: u64,
        now: u64,
    ) -> crate::relay::EndpointRecord {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(e) = recipient.receive(now).await.unwrap().endpoints.first() {
                    if e.record.generation == generation {
                        break e.record.clone();
                    }
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap()
    }
    let first = binding(&recipient, 1, 1000).await;
    f.clock.store(200_000, Ordering::SeqCst);
    task.control().wake().unwrap();
    tokio::time::sleep(Duration::from_millis(180)).await;
    assert_eq!(binding(&recipient, 1, 200_000).await, first);
    let key = iroh::SecretKey::generate();
    std::fs::write(
        f.root.path().join("identity/iroh.key"),
        format!("{}\n", hex::encode(key.to_bytes())),
    )
    .unwrap();
    task.control().wake().unwrap();
    let changed = binding(&recipient, 2, 200_000).await;
    assert_eq!(changed.endpoint_id, key.public().to_string());
    assert_ne!(changed.endpoint_id, first.endpoint_id);
    task.shutdown().await.unwrap();
}
