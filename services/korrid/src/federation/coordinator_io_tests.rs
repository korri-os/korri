use super::coordinator_tests::{inputs, timing};
use super::*;
use crate::relay::{
    CoordinatedRelays, InProcessRelayNetwork, RelayAuthSigner, RelayCoordinator, RelayError,
    RelayEvent, RelayFilter, RelayList, RelayTransport,
};
use coordinator::{Discovery, DiscoveryInputs};
use futures::future::BoxFuture;
use std::{sync::atomic::AtomicUsize, time::Duration};

struct RecordingTransport {
    network: InProcessRelayNetwork,
    block_read: bool,
    block_publish: bool,
    entered: AtomicUsize,
    dropped: AtomicUsize,
    publications: AtomicUsize,
}
struct PendingIo<'a>(&'a AtomicUsize);
impl Drop for PendingIo<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl RelayTransport for RecordingTransport {
    fn publish<'a>(
        &'a self,
        relay: &'a str,
        event: &'a str,
        auth: Arc<dyn RelayAuthSigner>,
    ) -> BoxFuture<'a, Result<(), RelayError>> {
        Box::pin(async move {
            self.publications.fetch_add(1, Ordering::SeqCst);
            if self.block_publish {
                let _pending = PendingIo(&self.dropped);
                self.entered.fetch_add(1, Ordering::SeqCst);
                std::future::pending::<()>().await;
            }
            self.network.publish(relay, event, auth).await
        })
    }
    fn read<'a>(
        &'a self,
        relay: &'a str,
        filter: &'a RelayFilter,
        auth: Arc<dyn RelayAuthSigner>,
    ) -> BoxFuture<'a, Result<Vec<RelayEvent>, RelayError>> {
        Box::pin(async move {
            if self.block_read {
                let _pending = PendingIo(&self.dropped);
                self.entered.fetch_add(1, Ordering::SeqCst);
                std::future::pending::<()>().await;
            }
            self.network.read(relay, filter, auth).await
        })
    }
}
async fn wait_for(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn cancellation_and_wake_drop_blocked_read_or_publication_before_join() {
    for block_read in [false, true] {
        let f = Fixture::new();
        let relays = RelayList::configured(vec!["ws://localhost:7001".into()]).unwrap();
        let network = Arc::new(RecordingTransport {
            network: InProcessRelayNetwork::new(&relays),
            block_read,
            block_publish: !block_read,
            entered: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
            publications: AtomicUsize::new(0),
        });
        let config = inputs(&relays, true);
        let task = Discovery::new(
            f.directory().unwrap(),
            f.credentials.clone(),
            Arc::new(move || Ok(config.clone())),
            network.clone(),
            timing(),
            Arc::new(|| 1000),
        )
        .spawn();
        wait_for(|| network.entered.load(Ordering::SeqCst) == 1).await;
        task.control().wake().unwrap();
        wait_for(|| network.entered.load(Ordering::SeqCst) == 2).await;
        assert_eq!(network.dropped.load(Ordering::SeqCst), 1);
        tokio::time::timeout(Duration::from_secs(1), task.shutdown())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(network.dropped.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn success_polls_but_empty_success_does_not_use_failure_backoff() {
    let f = Fixture::new();
    let relays = RelayList::configured(vec!["ws://localhost:7001".into()]).unwrap();
    let network = Arc::new(RecordingTransport {
        network: InProcessRelayNetwork::new(&relays),
        block_read: false,
        block_publish: false,
        entered: AtomicUsize::new(0),
        dropped: AtomicUsize::new(0),
        publications: AtomicUsize::new(0),
    });
    let config = inputs(&relays, false);
    let task = Discovery::new(
        f.directory().unwrap(),
        f.credentials.clone(),
        Arc::new(move || Ok(config.clone())),
        network.clone(),
        timing(),
        Arc::new(|| 1000),
    )
    .spawn();
    wait_for(|| network.publications.load(Ordering::SeqCst) == 1).await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(network.publications.load(Ordering::SeqCst), 1);
    wait_for(|| network.publications.load(Ordering::SeqCst) >= 2).await;
    network.network.set_available(&relays.as_slice()[0], false);
    task.control().wake().unwrap();
    wait_for(|| network.publications.load(Ordering::SeqCst) >= 5).await;
    task.shutdown().await.unwrap();
}

#[tokio::test]
async fn two_owned_devices_discover_and_persist_each_other_through_actual_relays() {
    let f = Fixture::new();
    fs::set_permissions(f._peer_root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let local = f.directory().unwrap();
    let peer_credentials = PeerCredentials::from_identity(f.peer.clone());
    let peer = FederationDirectory::open(
        f._peer_root.path(),
        peer_credentials.clone(),
        Authorization::load(f._peer_root.path()).unwrap(),
        Arc::new(|| 1000),
    )
    .unwrap();
    let relays = RelayList::configured(vec!["ws://localhost:7001".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let a = inputs(&relays, true);
    let b = inputs(&relays, true);
    let first = Discovery::new(
        local.clone(),
        f.credentials.clone(),
        Arc::new(move || Ok(a.clone())),
        network.clone(),
        timing(),
        Arc::new(|| 1000),
    )
    .spawn();
    let second = Discovery::new(
        peer.clone(),
        peer_credentials,
        Arc::new(move || Ok(b.clone())),
        network,
        timing(),
        Arc::new(|| 1000),
    )
    .spawn();
    wait_for(|| {
        local
            .snapshot()
            .unwrap()
            .first()
            .is_some_and(|p| p.current_endpoint.is_some())
            && peer
                .snapshot()
                .unwrap()
                .first()
                .is_some_and(|p| p.current_endpoint.is_some())
    })
    .await;
    first.shutdown().await.unwrap();
    second.shutdown().await.unwrap();
    drop(local);
    assert!(f.directory().unwrap().snapshot().unwrap()[0]
        .remembered_endpoint
        .is_some());
}

#[tokio::test]
async fn partial_announcements_retry_and_restart_wins_same_second_replacement() {
    let f = Fixture::new();
    let relays = RelayList::configured(vec![
        "ws://localhost:7001".into(),
        "ws://localhost:7002".into(),
    ])
    .unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let recipient =
        CoordinatedRelays::new(relays.clone(), f.peer.clone(), network.clone()).unwrap();
    recipient.publish_owner_statement().await.unwrap();
    network.set_available(&relays.as_slice()[1], false);
    let config = inputs(&relays, true);
    let directory = f.directory().unwrap();
    let task = Discovery::new(
        directory.clone(),
        f.credentials.clone(),
        Arc::new(move || Ok(config.clone())),
        network.clone(),
        timing(),
        Arc::new(|| 1000),
    )
    .spawn();
    wait_for(|| network.stored_event_count(&relays.as_slice()[0]) == 3).await;
    network.set_available(&relays.as_slice()[1], true);
    task.control().wake().unwrap();
    wait_for(|| network.stored_event_count(&relays.as_slice()[1]) == 3).await;
    assert_eq!(
        recipient.receive(1000).await.unwrap().endpoints[0]
            .record
            .generation,
        1
    );
    task.shutdown().await.unwrap();
    drop(directory);
    let config = inputs(&relays, true);
    let task = Discovery::new(
        f.directory().unwrap(),
        f.credentials.clone(),
        Arc::new(move || Ok(config.clone())),
        network,
        timing(),
        Arc::new(|| 1000),
    )
    .spawn();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let received = recipient.receive(1000).await.unwrap();
            if received.endpoints[0].record.generation == 2 {
                assert_eq!(received.endpoints[0].record.issued_at, 1001);
                let event =
                    DeviceIdentity::verify_encrypted_event(&received.endpoints[0].event_json)
                        .unwrap();
                assert_eq!(event.created_at, 1001);
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    task.shutdown().await.unwrap();
}

#[test]
fn android_inputs_reload_actual_config_and_reject_retained_unauthorized_authority() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("device.yaml"),
        "host:\n  title: Tablet\n  relays:\n    - ws://localhost:7001\n",
    )
    .unwrap();
    let config = crate::config::snapshot::ConfigSnapshotCoordinator::new(root.path());
    assert!(config.current_snapshot().host.is_none());
    let inputs = DiscoveryInputs::android(&config).unwrap();
    assert_eq!(inputs.relays.unwrap().as_slice(), &["ws://localhost:7001"]);
    assert!(inputs.advertised_endpoints.is_empty());
    // Query-only Android does not publish metadata, so the endpoint metadata
    // bounds must not add a constraint to existing readable host.title.
    fs::write(
        root.path().join("device.yaml"),
        "host:\n  title: ''\n  relays: [ws://localhost:7001]\n",
    )
    .unwrap();
    DiscoveryInputs::android(&config)
        .unwrap()
        .validate()
        .unwrap();
    fs::remove_file(root.path().join("device.yaml")).unwrap();
    fs::create_dir(root.path().join("device.yaml")).unwrap();
    assert!(DiscoveryInputs::android(&config).is_err());
}
