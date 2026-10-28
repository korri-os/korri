use super::*;
use crate::relay::{PublishState, RelayCoordinator};
use crate::{
    federation::reach::tests::Devices, relay::InProcessRelayNetwork, upstreams::UpstreamError,
};
use std::os::unix::fs::PermissionsExt;

#[derive(Clone, Copy)]
enum NativePath {
    Healthy,
    HealthyAfterNetworkReset,
    RecoveryBlocked,
    NoCandidate,
    UnsignedResponse,
}

async fn publishes_offer(path: NativePath) -> bool {
    let devices = Devices::new();
    if matches!(path, NativePath::RecoveryBlocked | NativePath::NoCandidate) {
        // Only a temporary journal is malformed. Exercise the actual host guard
        // without repairing or bypassing recovery, ownership or permissions.
        let recovery = devices.b.path().join("host-session");
        std::fs::create_dir(&recovery).unwrap();
        std::fs::set_permissions(&recovery, std::fs::Permissions::from_mode(0o700)).unwrap();
        let record = recovery.join("active.json");
        std::fs::write(&record, "invalid recovery record").unwrap();
        std::fs::set_permissions(record, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let router = if matches!(path, NativePath::UnsignedResponse) {
        axum::Router::new().route(
            "/peer-rpc",
            axum::routing::post(|| async { "unsigned response" }),
        )
    } else {
        devices.router()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let serving = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let peer = devices.cb.public_key().unwrap();
    let native = crate::upstream_native::NativeClient::new_secure(
        origin.clone(),
        peer.clone(),
        devices.ca.clone(),
    );
    let check_native = async || {
        let result = native.session_status().await;
        match path {
            NativePath::RecoveryBlocked | NativePath::NoCandidate => assert!(matches!(result,
                Err(UpstreamError::Tagged { code, .. }) if code == "HostRecoveryBlocked")),
            NativePath::Healthy | NativePath::HealthyAfterNetworkReset => {
                assert!(result.is_ok(), "{result:?}")
            }
            NativePath::UnsignedResponse => {
                assert!(matches!(result, Err(UpstreamError::Wire(_))), "{result:?}")
            }
        }
    };
    check_native().await;

    let relays = RelayList::configured(vec!["wss://signal.invalid".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let signal = CoordinatedRelays::new(
        relays.clone(),
        devices.ca.identity_snapshot().unwrap(),
        network.clone(),
    )
    .unwrap();
    let receiver =
        CoordinatedRelays::new(relays, devices.cb.identity_snapshot().unwrap(), network).unwrap();
    let key = iroh::SecretKey::generate();
    let endpoint = Builder::empty()
        .crypto_provider(iroh::tls::default_provider())
        .secret_key(key.clone())
        .alpns(vec![ALPN.to_vec()])
        .relay_mode(RelayMode::Custom(iroh::RelayMap::empty()))
        .portmapper_config(iroh::endpoint::PortmapperConfig::Disabled)
        .clear_ip_transports()
        .bind_addr("127.0.0.1:0")
        .unwrap()
        .bind()
        .await
        .unwrap();
    let mut controller = Controller {
        environment: Environment::linux(),
        directory: devices.da.clone(),
        credentials: devices.ca.clone(),
        config: None,
        key,
        peer_router: axum::Router::new(),
        relay_port: 0,
        endpoint: Some(endpoint.clone()),
        tasks: JoinSet::new(),
        relay_task: None,
        bindings: Arc::new(RwLock::new(BTreeMap::new())),
        dialing: Arc::new(Mutex::new(BTreeSet::new())),
        relay_addresses: Arc::new(RwLock::new(Vec::new())),
        pending: BTreeMap::new(),
        processed: BTreeMap::new(),
        retry: BTreeMap::new(),
        relaying: Arc::new(RwLock::new(BTreeMap::new())),
    };
    if matches!(path, NativePath::HealthyAfterNetworkReset) {
        controller.reset().await;
    }
    let mut snapshot = devices.da.snapshot().unwrap().remove(0);
    // Use the existing endpoint producer's shape. No public relay or STUN
    // service is contacted, and every key and server belongs to this test.
    snapshot.current_endpoint = Some(crate::relay::EndpointRecord {
        device_public_key: peer,
        owner_public_key: snapshot.membership.owner_public_key.clone(),
        generation: 1,
        candidates: if matches!(path, NativePath::NoCandidate) {
            Vec::new()
        } else {
            vec![origin]
        },
        issued_at: unix_time(),
        expires_at: unix_time() + 60,
        label: None,
        moonlight_address: None,
    });
    controller
        .offer_peer(snapshot, &signal, &StunList(Vec::new()), unix_time())
        .await;
    let received = receiver.receive(unix_time()).await.unwrap();
    let offered = received.commands.iter().any(|command| {
        matches!(command,
            CoordinationCommand::ReachOffer { sender_device_public_key, .. }
                if sender_device_public_key == &devices.ca.public_key().unwrap()
        )
    });
    if matches!(
        path,
        NativePath::RecoveryBlocked | NativePath::UnsignedResponse
    ) {
        assert_eq!(
            devices.da.snapshot().unwrap()[0].state,
            PeerState::Failed {
                error: crate::upstream_native::PEER_OPERATION_FAILED_MESSAGE.into(),
            },
            "discovery must retain the sanitized operation failure, not report Ready"
        );
    }
    // The same operation still has the same result after reach discovery.
    check_native().await;
    serving.abort();
    endpoint.close().await;
    offered
}

#[tokio::test]
async fn reach_native_recovery_refusal_does_not_suppress_relay_discovery() {
    assert!(
        publishes_offer(NativePath::RecoveryBlocked).await,
        "authenticated host recovery refusal suppressed independent relay discovery"
    );
}

#[tokio::test]
async fn reach_healthy_native_peer_refreshes_relay_discovery() {
    assert!(publishes_offer(NativePath::Healthy).await);
}

#[tokio::test]
async fn reach_native_peer_restarts_discovery_after_network_reset() {
    assert!(
        publishes_offer(NativePath::HealthyAfterNetworkReset).await,
        "native HTTP must not suppress rebuilding the iroh route after a network reset"
    );
}

#[tokio::test]
async fn reach_absent_native_candidate_allows_relay_discovery() {
    assert!(publishes_offer(NativePath::NoCandidate).await);
}

#[tokio::test]
async fn reach_unsigned_native_response_still_stops_relay_discovery() {
    assert!(!publishes_offer(NativePath::UnsignedResponse).await);
}

async fn relay_only_endpoint_with_key(key: iroh::SecretKey) -> Endpoint {
    Builder::empty()
        .crypto_provider(iroh::tls::default_provider())
        .secret_key(key)
        .alpns(vec![ALPN.to_vec()])
        .relay_mode(RelayMode::Custom(iroh::RelayMap::empty()))
        .portmapper_config(iroh::endpoint::PortmapperConfig::Disabled)
        .clear_ip_transports()
        .bind()
        .await
        .unwrap()
}

#[derive(Clone, Copy)]
enum RelayMetadata {
    Current,
    ExpiresDuringDirect,
    Nonmember,
}

async fn signed_call_after_relay_metadata(
    already_dialing: bool,
    metadata: RelayMetadata,
) -> Result<(), UpstreamError> {
    let devices = Devices::new();
    let key = iroh::SecretKey::generate();
    let endpoint = relay_only_endpoint_with_key(key.clone()).await;
    let peer_endpoint = relay_only_endpoint_with_key(iroh::SecretKey::generate()).await;
    let peer = devices.cb.public_key().unwrap();
    let access = relay::Membership {
        directory: devices.da.clone(),
        bindings: Arc::new(RwLock::new(BTreeMap::from([(
            peer_endpoint.id(),
            peer.clone(),
        )]))),
        local: endpoint.id(),
    };
    let mut relay_config =
        iroh_relay::server::RelayConfig::new("127.0.0.1:0".parse::<SocketAddr>().unwrap());
    relay_config.access = Arc::new(access);
    let mut config = iroh_relay::server::ServerConfig::default();
    config.relay = Some(relay_config);
    let server = iroh_relay::server::Server::spawn(config).await.unwrap();
    let address = server.http_addr().unwrap();
    let router = devices.router();
    let serving_endpoint = peer_endpoint.clone();
    let local_id = endpoint.id();
    let serving = tokio::spawn(async move {
        let dial = connect_iroh(&serving_endpoint, local_id, Vec::new(), || vec![address]);
        tokio::pin!(dial);
        let incoming = tokio::select! {
            connection = async { serving_endpoint.accept().await.unwrap().await.unwrap() } => connection,
            result = &mut dial => result.unwrap(),
        };
        tunnel::serve(incoming, router).await;
    });
    let mut controller = Controller {
        environment: Environment::linux(),
        directory: devices.da.clone(),
        credentials: devices.ca.clone(),
        config: None,
        key,
        peer_router: axum::Router::new(),
        relay_port: 0,
        endpoint: Some(endpoint.clone()),
        tasks: JoinSet::new(),
        relay_task: None,
        bindings: Arc::new(RwLock::new(BTreeMap::new())),
        dialing: Arc::new(Mutex::new(BTreeSet::new())),
        relay_addresses: Arc::new(RwLock::new(Vec::new())),
        pending: BTreeMap::new(),
        processed: BTreeMap::new(),
        retry: BTreeMap::new(),
        relaying: Arc::new(RwLock::new(BTreeMap::new())),
    };
    if already_dialing {
        // Receiving an offer starts this production dial without relay metadata.
        controller.dial(
            peer.clone(),
            peer_endpoint.id().to_string(),
            Vec::new(),
            Vec::new(),
        );
        assert!(controller.dialing.lock().unwrap().contains(&peer));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // Model the existing answer handler's current metadata and follow-up dial.
    // Both fixtures differ ONLY in whether an offer already started a dial.
    controller.relaying.write().unwrap().insert(
        if matches!(metadata, RelayMetadata::Nonmember) {
            "ff".repeat(32)
        } else {
            peer.clone()
        },
        (
            unix_time()
                + if matches!(metadata, RelayMetadata::ExpiresDuringDirect) {
                    2
                } else {
                    SIGNAL_LIFETIME
                },
            vec![address],
        ),
    );
    controller.dial(
        peer.clone(),
        peer_endpoint.id().to_string(),
        Vec::new(),
        Vec::new(),
    );
    timeout(Duration::from_secs(15), async {
        while let Some(result) = controller.tasks.join_next().await {
            result.unwrap();
        }
    })
    .await
    .expect("bounded production dial did not finish");
    let result = crate::upstream_native::NativeClient::new_secure_candidates(
        Vec::new(),
        peer,
        devices.ca.clone(),
    )
    .with_directory(devices.da.clone())
    .with_timeout(Duration::from_secs(1))
    .session_status()
    .await
    .map(|_| ());
    serving.abort();
    server.shutdown().await.unwrap();
    endpoint.close().await;
    peer_endpoint.close().await;
    result
}

#[tokio::test]
async fn reach_relay_metadata_before_dial_permits_a_signed_call() {
    let result = signed_call_after_relay_metadata(false, RelayMetadata::Current).await;
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn reach_relay_metadata_during_dial_permits_a_signed_call() {
    let result = signed_call_after_relay_metadata(true, RelayMetadata::Current).await;
    assert!(
        result.is_ok(),
        "current relay metadata was ignored by the in-flight dial: {result:?}"
    );
}

#[tokio::test]
async fn reach_relay_answer_expiring_during_direct_is_not_used_at_fallback() {
    let result = signed_call_after_relay_metadata(false, RelayMetadata::ExpiresDuringDirect).await;
    assert!(
        matches!(result, Err(UpstreamError::Unreachable(_))),
        "{result:?}"
    );
}

#[tokio::test]
async fn reach_relay_answer_without_current_membership_is_not_used_at_fallback() {
    let result = signed_call_after_relay_metadata(false, RelayMetadata::Nonmember).await;
    assert!(
        matches!(result, Err(UpstreamError::Unreachable(_))),
        "{result:?}"
    );
}

#[tokio::test]
async fn reach_direct_connection_does_not_read_relay_answers() {
    let endpoint = || async {
        Builder::empty()
            .crypto_provider(iroh::tls::default_provider())
            .alpns(vec![ALPN.to_vec()])
            .relay_mode(RelayMode::Custom(iroh::RelayMap::empty()))
            .portmapper_config(iroh::endpoint::PortmapperConfig::Disabled)
            .clear_ip_transports()
            .bind_addr("127.0.0.1:0")
            .unwrap()
            .bind()
            .await
            .unwrap()
    };
    let (a, b) = (endpoint().await, endpoint().await);
    let (outgoing, incoming) = tokio::join!(
        connect_iroh(&a, b.id(), endpoint_addresses(&b), || {
            panic!("relay lookup ran before direct methods succeeded")
        }),
        async { b.accept().await.unwrap().await.unwrap() }
    );
    let outgoing = outgoing.unwrap();
    assert_eq!(outgoing.remote_id(), b.id());
    outgoing.close(0u32.into(), b"test complete");
    incoming.close(0u32.into(), b"test complete");
    a.close().await;
    b.close().await;
}

// Both branches are real transports: actual signed/encrypted in-process events,
// and a local WebSocket server which accepts TCP but never completes its upgrade.
struct ReachReceiveTransports {
    live: Arc<InProcessRelayNetwork>,
    websocket: WebSocketRelayTransport,
    unavailable: String,
    live_completed: std::sync::atomic::AtomicU64,
    unavailable_completed: std::sync::atomic::AtomicU64,
}
impl crate::relay::RelayTransport for ReachReceiveTransports {
    fn publish<'a>(
        &'a self,
        relay: &'a str,
        event: &'a str,
        auth: Arc<dyn crate::relay::RelayAuthSigner>,
    ) -> futures::future::BoxFuture<'a, Result<(), crate::relay::RelayError>> {
        if relay == self.unavailable {
            crate::relay::RelayTransport::publish(&self.websocket, relay, event, auth)
        } else {
            crate::relay::RelayTransport::publish(self.live.as_ref(), relay, event, auth)
        }
    }
    fn read<'a>(
        &'a self,
        relay: &'a str,
        filter: &'a crate::relay::RelayFilter,
        auth: Arc<dyn crate::relay::RelayAuthSigner>,
    ) -> futures::future::BoxFuture<
        'a,
        Result<Vec<crate::relay::RelayEvent>, crate::relay::RelayError>,
    > {
        use std::sync::atomic::Ordering;
        Box::pin(async move {
            if relay == self.unavailable {
                let result =
                    crate::relay::RelayTransport::read(&self.websocket, relay, filter, auth).await;
                self.unavailable_completed
                    .store(unix_time(), Ordering::SeqCst);
                result
            } else {
                let result =
                    crate::relay::RelayTransport::read(self.live.as_ref(), relay, filter, auth)
                        .await;
                self.live_completed.store(unix_time(), Ordering::SeqCst);
                result
            }
        })
    }
}

#[tokio::test]
async fn reach_admits_matching_live_batch_before_slow_relay_expires_pending_offer() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let devices = Devices::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let unavailable = format!("ws://{address}");
    let accepting = tokio::spawn(async move {
        let mut connections = Vec::new();
        loop {
            connections.push(listener.accept().await.unwrap().0);
        }
    });
    let relays =
        RelayList::configured(vec![unavailable.clone(), "wss://live.invalid".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let transport = Arc::new(ReachReceiveTransports {
        live: network.clone(),
        websocket: WebSocketRelayTransport::new(),
        unavailable,
        live_completed: AtomicU64::new(0),
        unavailable_completed: AtomicU64::new(0),
    });
    let signal = CoordinatedRelays::new(
        relays,
        devices.ca.identity_snapshot().unwrap(),
        transport.clone(),
    )
    .unwrap();
    let sender = CoordinatedRelays::new(
        RelayList::configured(vec!["wss://live.invalid".into()]).unwrap(),
        devices.cb.identity_snapshot().unwrap(),
        network,
    )
    .unwrap();
    let peer = devices.cb.public_key().unwrap();
    let key = iroh::SecretKey::generate();
    let endpoint = relay_only_endpoint_with_key(key.clone()).await;
    let remote = iroh::SecretKey::generate().public();
    let request_id = "ab".repeat(32);
    let now = unix_time();
    let answer = Answer {
        endpoint_id: remote.to_string(),
        direct_addresses: Vec::new(),
        expires_at: now + SIGNAL_LIFETIME,
        request_id: request_id.clone(),
        can_relay_now: true,
        relay_addresses: vec!["192.0.2.1:443".parse().unwrap()],
    };
    assert!(sender
        .queue_command(
            &devices.ca.public_key().unwrap(),
            CoordinationCommand::ReachAnswer {
                sender_device_public_key: peer.clone(),
                answer: answer.clone(),
            },
            now
        )
        .await
        .unwrap()
        .accepted());
    // The pending offer is near the end of its unchanged 60-second lifetime.
    // Do not pause Tokio: the controller uses the real Unix clock at admission.
    let pending_expires = unix_time() + 2;
    let token = devices.da.begin_peer_work(&peer).unwrap();
    let mut controller = Controller {
        environment: Environment::linux(),
        directory: devices.da.clone(),
        credentials: devices.ca.clone(),
        config: None,
        key,
        peer_router: axum::Router::new(),
        relay_port: 0,
        endpoint: Some(endpoint.clone()),
        tasks: JoinSet::new(),
        relay_task: None,
        bindings: Arc::new(RwLock::new(BTreeMap::new())),
        // An existing active dial is a valid receive state. Keep this test at
        // admission; never attempt the documentation-only relay address.
        dialing: Arc::new(Mutex::new(BTreeSet::from([peer.clone()]))),
        relay_addresses: Arc::new(RwLock::new(Vec::new())),
        pending: BTreeMap::from([(
            peer.clone(),
            Pending {
                request_id,
                expires_at: pending_expires,
                token,
            },
        )]),
        processed: BTreeMap::new(),
        retry: BTreeMap::new(),
        relaying: Arc::new(RwLock::new(BTreeMap::new())),
    };
    controller
        .receive_reach(&signal, &StunList(Vec::new()), unix_time())
        .await;
    let admitted = controller.relaying.read().unwrap().get(&peer).cloned();
    controller.tasks.abort_all();
    while controller.tasks.join_next().await.is_some() {}
    accepting.abort();
    endpoint.close().await;
    assert!(
        transport.live_completed.load(Ordering::SeqCst) < pending_expires,
        "live batch did not finish before original pending expiry"
    );
    assert!(
        transport.unavailable_completed.load(Ordering::SeqCst) >= pending_expires,
        "unavailable branch did not cross the original pending expiry"
    );
    assert!(
        answer.validate(unix_time()).is_ok(),
        "answer itself expired"
    );
    assert_eq!(
        admitted,
        Some((answer.expires_at, answer.relay_addresses)),
        "timely signed matching answer was withheld until original pending offer expired"
    );
    assert!(!controller.pending.contains_key(&peer));
    assert_eq!(
        controller.bindings.read().unwrap().get(&remote),
        Some(&peer)
    );
}

struct PublicationFixture {
    devices: Devices,
    controller: Controller,
    signal: CoordinatedRelays<ReachReceiveTransports>,
    receiver: CoordinatedRelays<InProcessRelayNetwork>,
    network: Arc<InProcessRelayNetwork>,
    sockets: tokio::sync::mpsc::UnboundedReceiver<tokio::net::TcpStream>,
    accepting: JoinHandle<()>,
}
impl Drop for PublicationFixture {
    fn drop(&mut self) {
        self.accepting.abort();
    }
}
async fn publication_fixture() -> PublicationFixture {
    use std::sync::atomic::AtomicU64;
    let devices = Devices::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let unavailable = format!("ws://{}", listener.local_addr().unwrap());
    let (sent, sockets) = tokio::sync::mpsc::unbounded_channel();
    let accepting = tokio::spawn(async move {
        loop {
            let socket = listener.accept().await.unwrap().0;
            if sent.send(socket).is_err() {
                break;
            }
        }
    });
    let relays =
        RelayList::configured(vec!["wss://live.invalid".into(), unavailable.clone()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let transport = Arc::new(ReachReceiveTransports {
        live: network.clone(),
        websocket: WebSocketRelayTransport::new(),
        unavailable,
        live_completed: AtomicU64::new(0),
        unavailable_completed: AtomicU64::new(0),
    });
    let signal =
        CoordinatedRelays::new(relays, devices.ca.identity_snapshot().unwrap(), transport).unwrap();
    let receiver = CoordinatedRelays::new(
        RelayList::configured(vec!["wss://live.invalid".into()]).unwrap(),
        devices.cb.identity_snapshot().unwrap(),
        network.clone(),
    )
    .unwrap();
    let key = iroh::SecretKey::generate();
    let endpoint = relay_only_endpoint_with_key(key.clone()).await;
    let controller = Controller {
        environment: Environment::linux(),
        directory: devices.da.clone(),
        credentials: devices.ca.clone(),
        config: None,
        key,
        peer_router: axum::Router::new(),
        relay_port: 0,
        endpoint: Some(endpoint),
        tasks: JoinSet::new(),
        relay_task: None,
        bindings: Arc::new(RwLock::new(BTreeMap::new())),
        dialing: Arc::new(Mutex::new(BTreeSet::new())),
        relay_addresses: Arc::new(RwLock::new(Vec::new())),
        pending: BTreeMap::new(),
        processed: BTreeMap::new(),
        retry: BTreeMap::new(),
        relaying: Arc::new(RwLock::new(BTreeMap::new())),
    };
    PublicationFixture {
        devices,
        controller,
        signal,
        receiver,
        network,
        sockets,
        accepting,
    }
}

#[tokio::test]
async fn reach_offer_advances_on_first_ack_and_keeps_the_bounded_fanout_alive() {
    use tokio::io::AsyncReadExt;
    let mut fixture = publication_fixture().await;
    let peer = fixture.devices.cb.public_key().unwrap();
    let snapshot = fixture.devices.da.snapshot().unwrap().remove(0);
    let offered = timeout(
        Duration::from_secs(2),
        fixture.controller.offer_peer(
            snapshot,
            &fixture.signal,
            &StunList(Vec::new()),
            unix_time(),
        ),
    )
    .await;
    if offered.is_err() {
        fixture.controller.reset().await;
        panic!("a working relay ack was withheld behind the unavailable relay's 8-second timeout");
    }
    let received = fixture.receiver.receive(unix_time()).await.unwrap();
    assert!(received
        .commands
        .iter()
        .any(|command| matches!(command, CoordinationCommand::ReachOffer { .. })));
    assert!(fixture.controller.pending.contains_key(&peer));
    let mut socket = timeout(Duration::from_secs(1), fixture.sockets.recv())
        .await
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    assert!(
        timeout(Duration::from_millis(100), socket.read_to_end(&mut bytes))
            .await
            .is_err(),
        "the other relay attempt was cancelled on first ack instead of continuing fanout"
    );
    timeout(Duration::from_secs(9), socket.read_to_end(&mut bytes))
        .await
        .expect("background publication exceeded the existing bound")
        .unwrap();
    fixture.controller.reset().await;
}

#[tokio::test]
async fn reach_reset_aborts_background_publication_and_closes_its_socket() {
    use tokio::io::AsyncReadExt;
    let mut fixture = publication_fixture().await;
    let snapshot = fixture.devices.da.snapshot().unwrap().remove(0);
    timeout(
        Duration::from_secs(2),
        fixture.controller.offer_peer(
            snapshot,
            &fixture.signal,
            &StunList(Vec::new()),
            unix_time(),
        ),
    )
    .await
    .expect("working relay did not release offer");
    let mut socket = timeout(Duration::from_secs(1), fixture.sockets.recv())
        .await
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    assert!(
        timeout(Duration::from_millis(50), socket.read_to_end(&mut bytes))
            .await
            .is_err()
    );
    fixture.controller.reset().await;
    timeout(Duration::from_secs(1), socket.read_to_end(&mut bytes))
        .await
        .expect("reset leaked a publication socket")
        .unwrap();
}

#[tokio::test]
async fn reach_drop_aborts_background_publication_and_closes_its_socket() {
    use tokio::io::AsyncReadExt;
    let mut fixture = publication_fixture().await;
    let snapshot = fixture.devices.da.snapshot().unwrap().remove(0);
    timeout(
        Duration::from_secs(2),
        fixture.controller.offer_peer(
            snapshot,
            &fixture.signal,
            &StunList(Vec::new()),
            unix_time(),
        ),
    )
    .await
    .expect("working relay did not release offer");
    let mut socket = timeout(Duration::from_secs(1), fixture.sockets.recv())
        .await
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    assert!(
        timeout(Duration::from_millis(50), socket.read_to_end(&mut bytes))
            .await
            .is_err()
    );
    drop(fixture);
    timeout(Duration::from_secs(1), socket.read_to_end(&mut bytes))
        .await
        .expect("controller drop leaked a publication socket")
        .unwrap();
}

#[tokio::test]
async fn reach_all_failed_relays_report_failure_within_the_existing_bound() {
    let mut fixture = publication_fixture().await;
    fixture.network.set_available("wss://live.invalid", false);
    let peer = fixture.devices.cb.public_key().unwrap();
    let now = unix_time();
    let command = CoordinationCommand::ReachOffer {
        sender_device_public_key: fixture.devices.ca.public_key().unwrap(),
        offer: Offer {
            endpoint_id: fixture.controller.key.public().to_string(),
            direct_addresses: Vec::new(),
            expires_at: now + SIGNAL_LIFETIME,
            request_id: "ef".repeat(32),
        },
    };
    let accepted = timeout(
        Duration::from_secs(9),
        fixture
            .controller
            .publish_reach(&fixture.signal, &peer, command, now, "offer_published"),
    )
    .await
    .expect("all-relay failure exceeded its existing deadline");
    assert!(
        !accepted,
        "all failed relays were reported as a successful offer"
    );
    fixture.controller.reset().await;
}

#[tokio::test]
async fn non_reach_publication_still_waits_for_complete_per_relay_results() {
    let fixture = publication_fixture().await;
    let started = Instant::now();
    let now = unix_time();
    let result = timeout(
        Duration::from_secs(9),
        fixture.signal.queue_command(
            &fixture.devices.cb.public_key().unwrap(),
            CoordinationCommand::RunIdleKorrid {
                sender_device_public_key: fixture.devices.ca.public_key().unwrap(),
                requested_at: now,
                expires_at: now + 60,
            },
            now,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        started.elapsed() >= Duration::from_secs(7),
        "non-reach publication returned first-ack results"
    );
    assert!(
        matches!(result, PublishState::Partial { accepted_relays, failed_relays }
        if accepted_relays == ["wss://live.invalid"] && failed_relays.len() == 1)
    );
}

#[tokio::test]
async fn reach_sends_its_own_offer_before_an_unavailable_read_tail() {
    let mut fixture = publication_fixture().await;
    let reader = CoordinatedRelays::new(
        RelayList::configured(vec!["wss://live.invalid".into()]).unwrap(),
        fixture.devices.cb.identity_snapshot().unwrap(),
        fixture.network.clone(),
    )
    .unwrap();
    let exchange = tokio::spawn(async move {
        fixture
            .controller
            .exchange_reach(&fixture.signal, &StunList(Vec::new()), unix_time())
            .await;
    });
    let offered = timeout(Duration::from_secs(2), async {
        loop {
            if reader
                .receive(unix_time())
                .await
                .unwrap()
                .commands
                .iter()
                .any(|command| matches!(command, CoordinationCommand::ReachOffer { .. }))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    let tail_still_active = !exchange.is_finished();
    exchange.abort();
    let _ = exchange.await;
    assert!(
        offered.is_ok(),
        "an unavailable read tail delayed our own signed offer"
    );
    assert!(
        tail_still_active,
        "fixture did not keep a real unavailable read in flight"
    );
}

async fn publication_loopback_endpoint(key: iroh::SecretKey) -> Endpoint {
    Builder::empty()
        .crypto_provider(iroh::tls::default_provider())
        .secret_key(key)
        .alpns(vec![ALPN.to_vec()])
        .relay_mode(RelayMode::Custom(iroh::RelayMap::empty()))
        .portmapper_config(iroh::endpoint::PortmapperConfig::Disabled)
        .clear_ip_transports()
        .bind_addr("127.0.0.1:0")
        .unwrap()
        .bind()
        .await
        .unwrap()
}

#[tokio::test]
async fn reach_answer_and_direct_dial_advance_before_the_failed_publication_tail() {
    let mut fixture = publication_fixture().await;
    let endpoint = publication_loopback_endpoint(fixture.controller.key.clone()).await;
    fixture.controller.endpoint.take().unwrap().close().await;
    fixture.controller.endpoint = Some(endpoint.clone());
    let peer_endpoint = publication_loopback_endpoint(iroh::SecretKey::generate()).await;
    let observing = CoordinatedRelays::new(
        RelayList::configured(vec!["wss://live.invalid".into()]).unwrap(),
        fixture.devices.cb.identity_snapshot().unwrap(),
        fixture.network.clone(),
    )
    .unwrap();
    let request_id = "cd".repeat(32);
    let now = unix_time();
    fixture
        .receiver
        .queue_command(
            &fixture.devices.ca.public_key().unwrap(),
            CoordinationCommand::ReachOffer {
                sender_device_public_key: fixture.devices.cb.public_key().unwrap(),
                offer: Offer {
                    endpoint_id: peer_endpoint.id().to_string(),
                    direct_addresses: endpoint_addresses(&peer_endpoint),
                    expires_at: now + SIGNAL_LIFETIME,
                    request_id: request_id.clone(),
                },
            },
            now,
        )
        .await
        .unwrap();
    let (sent, connected) = tokio::sync::oneshot::channel();
    let serving_endpoint = peer_endpoint.clone();
    let router = fixture.devices.router();
    let serving = tokio::spawn(async move {
        let connection = serving_endpoint.accept().await.unwrap().await.unwrap();
        let _ = sent.send(());
        tunnel::serve(connection, router).await;
    });
    let receiving = tokio::spawn(async move {
        fixture
            .controller
            .receive_reach(&fixture.signal, &StunList(Vec::new()), unix_time())
            .await;
    });
    let result = timeout(Duration::from_secs(2), connected).await;
    let replies = observing.receive(unix_time()).await.unwrap();
    receiving.abort();
    let _ = receiving.await;
    serving.abort();
    let _ = serving.await;
    endpoint.close().await;
    peer_endpoint.close().await;
    assert!(
        matches!(result, Ok(Ok(()))),
        "the answer's slow relay prevented the direct dial"
    );
    assert!(
        replies.commands.iter().any(
            |command| matches!(command, CoordinationCommand::ReachAnswer { answer, .. }
        if answer.request_id == request_id)
        ),
        "the peer never received a valid signed answer"
    );
}

/// Signed in-process relay, real native peer and a controller without an
/// endpoint, as `reset` leaves it. `bindable` models the first seconds after
/// a rejoin, when a new interface address exists but cannot yet be bound.
struct RebindFixture {
    devices: Devices,
    controller: Controller,
    signal: CoordinatedRelays<InProcessRelayNetwork>,
    receiver: CoordinatedRelays<InProcessRelayNetwork>,
    snapshot: crate::federation::PeerSnapshot,
    bindable: Arc<std::sync::atomic::AtomicBool>,
    serving: JoinHandle<()>,
}
impl Drop for RebindFixture {
    fn drop(&mut self) {
        self.serving.abort();
    }
}
async fn rebind_fixture() -> RebindFixture {
    use std::sync::atomic::{AtomicBool, Ordering};
    let devices = Devices::new();
    let router = devices.router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let serving = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let relays = RelayList::configured(vec!["wss://signal.invalid".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let signal = CoordinatedRelays::new(
        relays.clone(),
        devices.ca.identity_snapshot().unwrap(),
        network.clone(),
    )
    .unwrap();
    let receiver =
        CoordinatedRelays::new(relays, devices.cb.identity_snapshot().unwrap(), network).unwrap();
    let bindable = Arc::new(AtomicBool::new(false));
    let reader = bindable.clone();
    // TEST-NET-1 is never assigned to this host, so bind fails with the same
    // EADDRNOTAVAIL as an IPv6 address during duplicate address detection.
    // Bind one stable host address only. This host's other addresses, such
    // as rotating IPv6 temporary addresses, can be briefly unbindable too.
    let interfaces: InterfaceReader = Arc::new(move || {
        let mut addresses = interfaces::interface_addresses()?
            .into_iter()
            .filter(|a| a.has_default_route && a.address.is_ipv4() && a.interface != "tailscale0")
            .take(1)
            .collect::<Vec<_>>();
        if !reader.load(Ordering::SeqCst) {
            addresses.push(interfaces::InterfaceAddress {
                interface: "wlan0".into(),
                address: "192.0.2.1".parse().unwrap(),
                has_default_route: false,
            });
        }
        Ok(addresses)
    });
    let controller = Controller {
        environment: Environment {
            interfaces,
            ..Environment::linux()
        },
        directory: devices.da.clone(),
        credentials: devices.ca.clone(),
        config: None,
        key: iroh::SecretKey::generate(),
        peer_router: axum::Router::new(),
        relay_port: 0,
        endpoint: None,
        tasks: JoinSet::new(),
        relay_task: None,
        bindings: Arc::new(RwLock::new(BTreeMap::new())),
        dialing: Arc::new(Mutex::new(BTreeSet::new())),
        relay_addresses: Arc::new(RwLock::new(Vec::new())),
        pending: BTreeMap::new(),
        processed: BTreeMap::new(),
        retry: BTreeMap::new(),
        relaying: Arc::new(RwLock::new(BTreeMap::new())),
    };
    let mut snapshot = devices.da.snapshot().unwrap().remove(0);
    snapshot.current_endpoint = Some(crate::relay::EndpointRecord {
        device_public_key: devices.cb.public_key().unwrap(),
        owner_public_key: snapshot.membership.owner_public_key.clone(),
        generation: 1,
        candidates: vec![origin],
        issued_at: unix_time(),
        expires_at: unix_time() + 60,
        label: None,
        moonlight_address: None,
    });
    RebindFixture {
        devices,
        controller,
        signal,
        receiver,
        snapshot,
        bindable,
        serving,
    }
}
impl RebindFixture {
    async fn offers(&self) -> usize {
        let sender = self.devices.ca.public_key().unwrap();
        self.receiver
            .receive(unix_time())
            .await
            .unwrap()
            .commands
            .iter()
            .filter(|command| {
                matches!(command,
                CoordinationCommand::ReachOffer { sender_device_public_key, .. }
                    if *sender_device_public_key == sender)
            })
            .count()
    }
    async fn offer(&mut self) {
        self.controller
            .offer_peer(
                self.snapshot.clone(),
                &self.signal,
                &StunList(Vec::new()),
                unix_time(),
            )
            .await;
    }
}

#[tokio::test]
async fn reach_unbindable_endpoint_after_reset_does_not_defer_our_offer_to_a_native_peer() {
    let mut fixture = rebind_fixture().await;
    fixture.offer().await;
    assert_eq!(fixture.offers().await, 0, "fixture endpoint was bindable");
    assert_eq!(
        fixture.devices.da.snapshot().unwrap()[0].state,
        PeerState::Ready,
        "fixture did not take the native-ready path"
    );
    // The address becomes bindable. Native HTTP stays healthy, but it cannot
    // carry netplay: our iroh offer is still owed, on the normal first retry.
    fixture
        .bindable
        .store(true, std::sync::atomic::Ordering::SeqCst);
    tokio::time::sleep(retry_delay(0) + Duration::from_millis(200)).await;
    fixture.offer().await;
    let offers = fixture.offers().await;
    fixture.controller.reset().await;
    assert_eq!(
        offers, 1,
        "a native-ready refresh deferred the iroh offer that the endpoint could not send"
    );
}

#[tokio::test]
async fn reach_network_reset_does_not_answer_a_replayed_offer_again() {
    let mut fixture = rebind_fixture().await;
    fixture
        .bindable
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let peer_endpoint = publication_loopback_endpoint(iroh::SecretKey::generate()).await;
    let request_id = "e1".repeat(32);
    let now = unix_time();
    fixture
        .receiver
        .queue_command(
            &fixture.devices.ca.public_key().unwrap(),
            CoordinationCommand::ReachOffer {
                sender_device_public_key: fixture.devices.cb.public_key().unwrap(),
                offer: Offer {
                    endpoint_id: peer_endpoint.id().to_string(),
                    direct_addresses: endpoint_addresses(&peer_endpoint),
                    expires_at: now + SIGNAL_LIFETIME,
                    request_id: request_id.clone(),
                },
            },
            now,
        )
        .await
        .unwrap();
    let answers = async |fixture: &RebindFixture| {
        fixture
            .receiver
            .receive(unix_time())
            .await
            .unwrap()
            .commands
            .iter()
            .filter(|command| {
                matches!(command,
                CoordinationCommand::ReachAnswer { answer, .. } if answer.request_id == request_id)
            })
            .count()
    };
    fixture
        .controller
        .receive_reach(&fixture.signal, &StunList(Vec::new()), unix_time())
        .await;
    assert_eq!(answers(&fixture).await, 1, "fixture offer was not answered");
    // Relay reads have no cursor: every pass rereads each unexpired offer.
    // The sender consumed our answer, so after our own network reset the
    // replay must not cost a second answer or a dial bound to time out.
    fixture.controller.reset().await;
    fixture
        .controller
        .receive_reach(&fixture.signal, &StunList(Vec::new()), unix_time())
        .await;
    let replayed = answers(&fixture).await;
    fixture.controller.reset().await;
    peer_endpoint.close().await;
    assert_eq!(
        replayed, 1,
        "a network reset answered an already answered offer again"
    );
}

/// The controller holds a live route, dialed through production `dial`, to
/// the peer's `old` endpoint. The peer side keeps that connection open but no
/// longer serves it, like a peer whose CONNECTION_CLOSE was lost with its
/// Wi-Fi: until iroh's idle timeout the route still looks live here.
struct LiveRouteFixture {
    fixture: RebindFixture,
    peer_key: iroh::SecretKey,
    old: Endpoint,
    _old_connection: Connection,
    endpoint: Endpoint,
}
async fn live_route_fixture() -> LiveRouteFixture {
    let mut fixture = rebind_fixture().await;
    let endpoint = publication_loopback_endpoint(fixture.controller.key.clone()).await;
    fixture.controller.endpoint = Some(endpoint.clone());
    let peer = fixture.devices.cb.public_key().unwrap();
    let peer_key = iroh::SecretKey::generate();
    let old = publication_loopback_endpoint(peer_key.clone()).await;
    let accepting = old.clone();
    let router = fixture.devices.router();
    let (sent, accepted) = tokio::sync::oneshot::channel();
    let serving = tokio::spawn(async move {
        let connection = accepting.accept().await.unwrap().await.unwrap();
        let _ = sent.send(connection.clone());
        tunnel::serve(connection, router).await;
    });
    fixture.controller.dial(
        peer.clone(),
        old.id().to_string(),
        endpoint_addresses(&old),
        Vec::new(),
    );
    let old_connection = timeout(Duration::from_secs(5), accepted)
        .await
        .unwrap()
        .unwrap();
    timeout(Duration::from_secs(10), async {
        while let Some(result) = fixture.controller.tasks.join_next().await {
            result.unwrap();
        }
    })
    .await
    .expect("production dial did not finish");
    assert!(
        fixture.devices.da.has_iroh_route(&peer),
        "fixture has no live route"
    );
    serving.abort();
    let _ = serving.await;
    LiveRouteFixture {
        fixture,
        peer_key,
        old,
        _old_connection: old_connection,
        endpoint,
    }
}
impl LiveRouteFixture {
    async fn offer_from(&mut self, endpoint: &Endpoint, request_id: &str) {
        let now = unix_time();
        self.fixture
            .receiver
            .queue_command(
                &self.fixture.devices.ca.public_key().unwrap(),
                CoordinationCommand::ReachOffer {
                    sender_device_public_key: self.fixture.devices.cb.public_key().unwrap(),
                    offer: Offer {
                        endpoint_id: endpoint.id().to_string(),
                        direct_addresses: endpoint_addresses(endpoint),
                        expires_at: now + SIGNAL_LIFETIME,
                        request_id: request_id.into(),
                    },
                },
                now,
            )
            .await
            .unwrap();
        self.fixture
            .controller
            .receive_reach(&self.fixture.signal, &StunList(Vec::new()), unix_time())
            .await;
    }
    async fn close(mut self) {
        self.fixture.controller.reset().await;
        self.endpoint.close().await;
        self.old.close().await;
    }
}

#[tokio::test]
async fn reach_offer_from_a_rebuilt_peer_endpoint_dials_despite_an_old_live_route() {
    let mut fixture = live_route_fixture().await;
    // A network reset rebuilds the peer's endpoint: same key, new port. Its
    // firewall admits our packets only after we send to the new address, so
    // its own dial to us cannot succeed unless we dial it too.
    let rebuilt = publication_loopback_endpoint(fixture.peer_key.clone()).await;
    let accepting = rebuilt.clone();
    let accepted = tokio::spawn(async move { accepting.accept().await.is_some() });
    fixture.offer_from(&rebuilt, &"a2".repeat(32)).await;
    let dialed = timeout(Duration::from_secs(5), accepted).await;
    rebuilt.close().await;
    fixture.close().await;
    assert!(
        matches!(dialed, Ok(Ok(true))),
        "a route to the peer's old endpoint suppressed the dial to its rebuilt endpoint"
    );
}

#[tokio::test]
async fn reach_repeated_offer_from_the_same_peer_endpoint_keeps_the_live_route() {
    let mut fixture = live_route_fixture().await;
    let peer = fixture.fixture.devices.cb.public_key().unwrap();
    let route = fixture.fixture.devices.da.reach.connection(&peer).unwrap();
    // Live peers re-offer to refresh relay eligibility. That must not replace
    // a route that an active netplay or stream forward may be using.
    let accepting = fixture.old.clone();
    let accepted = tokio::spawn(async move { accepting.accept().await.is_some() });
    let old = fixture.old.clone();
    fixture.offer_from(&old, &"a3".repeat(32)).await;
    let redialed = timeout(Duration::from_secs(1), accepted).await;
    let current = fixture.fixture.devices.da.reach.connection(&peer);
    fixture.close().await;
    assert!(
        redialed.is_err(),
        "a repeated offer redialed the same endpoint"
    );
    assert_eq!(
        current.map(|connection| connection.stable_id()),
        Some(route.stable_id()),
        "a repeated offer replaced a live route"
    );
}

async fn host_endpoint(key: iroh::SecretKey, ip: IpAddr) -> Endpoint {
    Builder::empty()
        .crypto_provider(iroh::tls::default_provider())
        .secret_key(key)
        .alpns(vec![ALPN.to_vec()])
        .relay_mode(RelayMode::Custom(iroh::RelayMap::empty()))
        .portmapper_config(iroh::endpoint::PortmapperConfig::Disabled)
        .clear_ip_transports()
        .bind_addr(SocketAddr::new(ip, 0))
        .unwrap()
        .bind()
        .await
        .unwrap()
}

/// A controller with its production accept loop, and a peer endpoint bound
/// to the peer device by a signed offer. The peer serves every connection.
struct IncomingFixture {
    fixture: RebindFixture,
    endpoint: Endpoint,
    local: Vec<SocketAddr>,
    peer: String,
    remote: Endpoint,
    accepted: Arc<std::sync::atomic::AtomicUsize>,
    serving: JoinHandle<()>,
}
async fn incoming_fixture() -> IncomingFixture {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut fixture = rebind_fixture().await;
    fixture.bindable.store(true, Ordering::SeqCst);
    let endpoint = fixture
        .controller
        .ensure_endpoint(&StunList(Vec::new()))
        .await
        .unwrap();
    let local = endpoint_addresses(&endpoint);
    let peer = fixture.devices.cb.public_key().unwrap();
    let remote = host_endpoint(iroh::SecretKey::generate(), local[0].ip()).await;
    bind_offer(
        &fixture.devices.da,
        &fixture.controller.bindings,
        &peer,
        &Offer {
            endpoint_id: remote.id().to_string(),
            direct_addresses: endpoint_addresses(&remote),
            expires_at: unix_time() + SIGNAL_LIFETIME,
            request_id: "b4".repeat(32),
        },
        unix_time(),
    )
    .unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let (accepting, counter, router) = (remote.clone(), accepted.clone(), fixture.devices.router());
    let serving = tokio::spawn(async move {
        let mut sessions = JoinSet::new();
        while let Some(incoming) = accepting.accept().await {
            if let Ok(connection) = incoming.await {
                counter.fetch_add(1, Ordering::SeqCst);
                sessions.spawn(tunnel::serve(connection, router.clone()));
            }
        }
    });
    fixture
        .devices
        .da
        .reach
        .active
        .store(true, Ordering::Release);
    IncomingFixture {
        fixture,
        endpoint,
        local,
        peer,
        remote,
        accepted,
        serving,
    }
}
impl IncomingFixture {
    /// The peer read our answer and dialed us. Its firewall dropped our dial.
    async fn peer_dials(&self) -> Connection {
        connect_iroh(
            &self.remote,
            self.endpoint.id(),
            self.local.clone(),
            Vec::new,
        )
        .await
        .unwrap()
    }
    async fn close(mut self) {
        self.serving.abort();
        self.fixture.controller.reset().await;
        self.remote.close().await;
    }
}

#[tokio::test]
async fn reach_admitted_incoming_connection_gives_a_waiting_invite_a_route() {
    let fixture = incoming_fixture().await;
    let directory = fixture.fixture.devices.da.clone();
    let waited = fixture.peer.clone();
    let waiting = tokio::spawn(async move {
        directory
            .reach_connection(&waited)
            .await
            .map(|connection| connection.remote_id())
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let incoming = fixture.peer_dials().await;
    let route = timeout(Duration::from_secs(5), waiting).await;
    let remote = fixture.remote.id();
    incoming.close(0u32.into(), b"test complete");
    fixture.close().await;
    assert!(
        matches!(route, Ok(Ok(Ok(id))) if id == remote),
        "a waiting invite never saw the admitted incoming connection: {route:?}"
    );
}

#[tokio::test]
async fn reach_incoming_connection_with_a_live_route_does_not_dial_back() {
    use std::sync::atomic::Ordering;
    let fixture = incoming_fixture().await;
    let first = fixture.peer_dials().await;
    let route = timeout(
        Duration::from_secs(5),
        fixture.fixture.devices.da.reach_connection(&fixture.peer),
    )
    .await
    .unwrap()
    .unwrap();
    let _ = timeout(Duration::from_secs(2), async {
        while fixture.accepted.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    let dialed = fixture.accepted.load(Ordering::SeqCst);
    // The peer dials again, for example after its next exchange. Our route
    // is live, so another dial back would only churn connections.
    let second = fixture.peer_dials().await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let current = fixture.fixture.devices.da.reach.connection(&fixture.peer);
    let redialed = fixture.accepted.load(Ordering::SeqCst);
    first.close(0u32.into(), b"test complete");
    second.close(0u32.into(), b"test complete");
    fixture.close().await;
    assert_eq!(dialed, 1, "fixture did not dial back once");
    assert_eq!(
        redialed, 1,
        "an incoming connection dialed back over a live route"
    );
    assert_eq!(
        current.map(|connection| connection.stable_id()),
        Some(route.stable_id())
    );
}
