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
    let advertising = DiscoveryInputs::linux(
        &config,
        &inputs(&relays, true),
        vec!["http://device:43117".into()],
    )
    .unwrap();
    advertising.validate().unwrap();
    assert_eq!(advertising.label.as_deref(), Some("Device"));
    let mut initial = inputs(&relays, false);
    initial.moonlight_address = Some("".into());
    let task = Discovery::new(
        directory.clone(),
        f.credentials.clone(),
        Arc::new(move || DiscoveryInputs::linux(&config, &initial, vec![])),
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

#[tokio::test]
async fn interface_change_wakes_discovery_and_republishes_without_waiting_for_poll() {
    let f = Fixture::new();
    let relays = RelayList::configured(vec!["ws://localhost:7001".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let peer = CoordinatedRelays::new(relays.clone(), f.peer.clone(), network.clone()).unwrap();
    peer.publish_owner_statement().await.unwrap();
    let addresses = Arc::new(std::sync::Mutex::new(vec![
        "http://10.0.0.1:39217".to_owned()
    ]));
    let live = addresses.clone();
    let config = inputs(&relays, true);
    let mut t = timing();
    t.poll = Duration::from_secs(3600);
    let task = Discovery::new(
        f.directory().unwrap(),
        f.credentials.clone(),
        Arc::new(move || {
            let mut fresh = config.clone();
            fresh.advertised_endpoints = live.lock().unwrap().clone();
            Ok(fresh)
        }),
        network.clone(),
        t,
        Arc::new(|| 1000),
    )
    .spawn();
    let live = addresses.clone();
    let monitor = tokio::spawn(super::super::interfaces::watch_endpoints(
        task.control(),
        move || Ok(live.lock().unwrap().clone()),
        Duration::from_millis(5),
    ));
    async fn generation(
        peer: &CoordinatedRelays<InProcessRelayNetwork>,
        wanted: u64,
    ) -> crate::relay::EndpointRecord {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(e) = peer
                    .receive(1000)
                    .await
                    .unwrap()
                    .endpoints
                    .into_iter()
                    .find(|e| e.record.generation == wanted)
                {
                    break e.record;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap()
    }
    assert_eq!(
        generation(&peer, 1).await.candidates,
        ["http://10.0.0.1:39217"]
    );
    // Let the monitor take its initial snapshot before changing the addresses.
    tokio::time::sleep(Duration::from_millis(15)).await;
    *addresses.lock().unwrap() = vec![
        "http://100.64.0.2:39217".into(),
        "http://192.168.2.3:39217".into(),
    ];
    assert_eq!(
        generation(&peer, 2).await.candidates,
        *addresses.lock().unwrap()
    );
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert_eq!(
        peer.receive(1000).await.unwrap().endpoints[0]
            .record
            .generation,
        2
    );
    monitor.abort();
    task.shutdown().await.unwrap();
}
