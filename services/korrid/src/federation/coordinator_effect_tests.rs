use super::coordinator_tests::{inputs, timing};
use super::*;
use crate::relay::{CoordinatedRelays, InProcessRelayNetwork, RelayCoordinator, RelayList};
use coordinator::Discovery;
use std::time::Duration;

#[tokio::test]
async fn renewal_address_change_and_revocation_preserve_signed_order_and_memory() {
    let f = Fixture::new();
    let directory = f.directory().unwrap();
    let relays = RelayList::configured(vec!["ws://localhost:7001".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let recipient =
        CoordinatedRelays::new(relays.clone(), f.peer.clone(), network.clone()).unwrap();
    recipient.publish_owner_statement().await.unwrap();
    let current = Arc::new(Mutex::new(inputs(&relays, true)));
    let config = current.clone();
    let clock = f.clock.clone();
    let task = Discovery::new(
        directory.clone(),
        f.credentials.clone(),
        Arc::new(move || Ok(config.lock().unwrap().clone())),
        network.clone(),
        timing(),
        Arc::new(move || clock.load(Ordering::SeqCst)),
    )
    .spawn();
    async fn generation(
        recipient: &CoordinatedRelays<InProcessRelayNetwork>,
        now: u64,
        generation: u64,
    ) -> crate::relay::EndpointRecord {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(evidence) = recipient.receive(now).await.unwrap().endpoints.first() {
                    if evidence.record.generation == generation {
                        return evidence.record.clone();
                    }
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap()
    }
    assert_eq!(
        generation(&recipient, 1000, 1).await.expires_at,
        1000 + 86400
    );
    f.clock.store(1000 + 21600, Ordering::SeqCst);
    task.control().wake().unwrap();
    let renewed = generation(&recipient, 22600, 2).await;
    assert_eq!(renewed.issued_at, 22600);
    assert_eq!(renewed.expires_at, 22600 + 86400);
    {
        let mut config = current.lock().unwrap();
        config.advertised_endpoints = vec!["https://moved.example:43117".into()];
        config.label = Some("Moved".into());
        config.moonlight_address = Some("moved:47989".into());
    }
    task.control().wake().unwrap();
    let moved = generation(&recipient, 22600, 3).await;
    assert_eq!(moved.issued_at, 22601);
    assert_eq!(moved.candidates, ["https://moved.example:43117"]);
    assert_eq!(moved.label.as_deref(), Some("Moved"));
    assert_eq!(moved.moonlight_address.as_deref(), Some("moved:47989"));
    let revoked = binding(&f.owner, &f.key(), "revoked", 2000);
    network.inject(&relays.as_slice()[0], &revoked).unwrap();
    task.control().wake().unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !directory.snapshot().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    task.shutdown().await.unwrap();
    drop(directory);
    assert!(f.directory().unwrap().snapshot().unwrap().is_empty());
}

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
        Arc::new(move || coordinator::DiscoveryInputs::android(&config)),
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
