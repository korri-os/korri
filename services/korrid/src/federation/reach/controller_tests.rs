use super::*;
use crate::{
    federation::reach::tests::Devices, relay::InProcessRelayNetwork, upstreams::UpstreamError,
};
use std::os::unix::fs::PermissionsExt;

#[derive(Clone, Copy)]
enum NativePath {
    Healthy,
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
            NativePath::Healthy => assert!(result.is_ok(), "{result:?}"),
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
