use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn reach_network_watcher_detects_addresses_and_interfaces_despite_failed_reads() {
    for change in [
        vec![("wlan0".into(), "192.168.1.3".parse().unwrap(), true)],
        vec![("eth0".into(), "192.168.1.2".parse().unwrap(), true)],
    ] {
        let original = vec![("wlan0".into(), "192.168.1.2".parse().unwrap(), true)];
        let reads = std::sync::Mutex::new(std::collections::VecDeque::from([
            None,
            Some(original.clone()),
            Some(change),
        ]));
        let start = tokio::time::Instant::now();
        tokio::time::timeout(
            Duration::from_secs(1),
            controller::network_changed_with(
                Some(original),
                || reads.lock().unwrap().pop_front().flatten(),
                Duration::from_millis(10),
            ),
        )
        .await
        .unwrap();
        assert!(start.elapsed() >= Duration::from_millis(30));
    }
}

#[tokio::test]
async fn reach_candidate_race_precedes_iroh_and_injected_failure_uses_it() {
    let devices = Devices::new();
    let (a, b, outgoing, incoming) = pair().await;
    let quic_calls = Arc::new(AtomicUsize::new(0));
    let observed = quic_calls.clone();
    let router = devices.router().layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::SeqCst);
                next.run(request).await
            }
        },
    ));
    let quic = tokio::spawn(tunnel::serve(incoming, router));
    let key = devices.cb.public_key().unwrap();
    devices.da.reach.live.write().unwrap().insert(
        key.clone(),
        LiveRoute {
            connection: outgoing,
            signalled: Vec::new(),
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let router = devices.router();
    let http = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let direct = crate::upstream_native::NativeClient::new_secure_candidates(
        vec![origin.clone()],
        key.clone(),
        devices.ca.clone(),
    )
    .with_directory(devices.da.clone());
    direct.session_status().await.unwrap();
    assert_eq!(quic_calls.load(Ordering::SeqCst), 0);
    http.abort();
    let _ = http.await;
    // A refused candidate is a real injected transport failure. No product
    // request reaches it, and the same signed call now goes through QUIC.
    let unreachable = crate::upstream_native::NativeClient::new_secure_candidates(
        vec![origin],
        key,
        devices.ca.clone(),
    )
    .with_directory(devices.da.clone());
    unreachable.session_status().await.unwrap();
    assert_eq!(quic_calls.load(Ordering::SeqCst), 1);
    quic.abort();
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn catalog_stalled_tcp_response_falls_back_to_live_iroh_within_budget() {
    let devices = Devices::new();
    let (a, b, outgoing, incoming) = pair().await;
    let serving = tokio::spawn(tunnel::serve(incoming, devices.router()));
    let key = devices.cb.public_key().unwrap();
    devices.da.reach.live.write().unwrap().insert(
        key.clone(),
        LiveRoute {
            connection: outgoing,
            signalled: Vec::new(),
        },
    );
    let tcp_calls = Arc::new(AtomicUsize::new(0));
    let calls = tcp_calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let router = axum::Router::new().route(
        "/peer-rpc",
        axum::routing::post(move || {
            let calls = calls.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                std::future::pending::<&'static str>().await
            }
        }),
    );
    let stalled = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = crate::upstream_native::NativeClient::new_secure_candidates(
        vec![origin],
        key,
        devices.ca.clone(),
    )
    .with_directory(devices.da.clone());
    let started = tokio::time::Instant::now();
    let result = timeout(
        crate::upstream_native::NATIVE_RPC_TIMEOUT + Duration::from_secs(1),
        client.catalog_snapshot(),
    )
    .await;
    let elapsed = started.elapsed();
    stalled.abort();
    serving.abort();
    a.close().await;
    b.close().await;
    let catalog = result
        .expect("catalog exceeded its aggregate budget")
        .expect("stalled TCP hid a working iroh route");
    assert_eq!(catalog.games[0].id, "game");
    assert_eq!(tcp_calls.load(Ordering::SeqCst), 1);
    assert!(elapsed < crate::upstream_native::NATIVE_RPC_TIMEOUT + Duration::from_secs(1));
}

#[tokio::test]
async fn command_waits_for_cold_route_beyond_catalog_budget_while_catalog_stays_bounded() {
    let devices = Devices::new();
    let (a, b, outgoing, incoming) = pair().await;
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
    let serving = tokio::spawn(tunnel::serve(incoming, router));
    let key = devices.cb.public_key().unwrap();
    let client = crate::upstream_native::NativeClient::new_secure_candidates(
        Vec::new(),
        key.clone(),
        devices.ca.clone(),
    )
    .with_directory(devices.da.clone());
    devices.da.reach.active.store(true, Ordering::Release);
    let routes = devices.da.reach.clone();
    // Real signed commands must survive the controller's five-second direct
    // phase before relay discovery can publish a usable connection.
    let installing = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(6)).await;
        routes.live.write().unwrap().insert(
            key,
            LiveRoute {
                connection: outgoing,
                signalled: Vec::new(),
            },
        );
        routes.changed.notify_waiters();
    });
    let started = tokio::time::Instant::now();
    let (catalog, command) = tokio::join!(
        async {
            let result = timeout(Duration::from_secs(6), client.catalog_snapshot()).await;
            (result, started.elapsed())
        },
        timeout(Duration::from_secs(10), client.prepare_stream("game")),
    );
    installing.await.unwrap();
    serving.abort();
    a.close().await;
    b.close().await;
    assert!(catalog.1 < Duration::from_secs(6));
    assert!(matches!(
        catalog.0.expect("catalog waited for command discovery"),
        Err(crate::upstreams::UpstreamError::Unreachable(_))
    ));
    let prepared = command
        .expect("cold-route command exceeded discovery budget")
        .expect("command expired before its route became available");
    assert_eq!(prepared.session.game_id, "game");
    assert!(started.elapsed() >= Duration::from_secs(6));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "only the command may reach the newly available route"
    );
}

#[tokio::test]
async fn reach_direct_failure_waits_five_seconds_before_the_member_relay() {
    let devices = Devices::new();
    async fn relay_only_endpoint() -> Endpoint {
        Builder::empty()
            .crypto_provider(iroh::tls::default_provider())
            .alpns(vec![ALPN.to_vec()])
            .relay_mode(iroh::RelayMode::Custom(iroh::RelayMap::empty()))
            .portmapper_config(PortmapperConfig::Disabled)
            .clear_ip_transports()
            .bind()
            .await
            .unwrap()
    }
    let (a, b) = (relay_only_endpoint().await, relay_only_endpoint().await);
    assert!(controller::endpoint_addresses(&a).is_empty());
    assert!(controller::endpoint_addresses(&b).is_empty());
    // A third owned federation device carries the traffic, not either peer.
    let relay_root = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(relay_root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let credentials = PeerCredentials::from_identity(owned(relay_root.path(), &devices.owner));
    let directory = FederationDirectory::open(
        relay_root.path(),
        credentials,
        Authorization::load(relay_root.path()).unwrap(),
        Arc::new(unix_time),
    )
    .unwrap();
    directory
        .apply_membership(
            &directory.begin_work().unwrap(),
            &[
                devices
                    .ca
                    .identity_snapshot()
                    .unwrap()
                    .owner_statement_json()
                    .unwrap(),
                devices
                    .cb
                    .identity_snapshot()
                    .unwrap()
                    .owner_statement_json()
                    .unwrap(),
            ],
        )
        .unwrap();
    let bindings = Arc::new(RwLock::new(BTreeMap::from([
        (a.id(), devices.ca.public_key().unwrap()),
        (b.id(), devices.cb.public_key().unwrap()),
    ])));
    let access = relay::Membership {
        directory,
        bindings,
        local: iroh::SecretKey::generate().public(),
    };
    let mut config = iroh_relay::server::ServerConfig::default();
    let mut relay_config = iroh_relay::server::RelayConfig::new(
        "127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap(),
    );
    relay_config.access = Arc::new(access);
    config.relay = Some(relay_config);
    let server = iroh_relay::server::Server::spawn(config).await.unwrap();
    let address = server.http_addr().unwrap();
    // This test supplies no direct candidate. Both sides dial only after their
    // device-signed signal, as the production controller does.
    let start = tokio::time::Instant::now();
    let (outgoing, incoming) = tokio::join!(
        controller::connect_iroh(&a, b.id(), Vec::new(), || vec![address]),
        async {
            let dial = controller::connect_iroh(&b, a.id(), Vec::new(), || vec![address]);
            tokio::pin!(dial);
            tokio::select! {
                connection = async {b.accept().await.unwrap().await.unwrap()} => connection,
                result = &mut dial => result.unwrap(),
            }
        }
    );
    let outgoing = outgoing.unwrap();
    assert!(start.elapsed() >= Duration::from_secs(5));
    assert!(start.elapsed() < Duration::from_secs(15));
    assert_eq!(outgoing.remote_id(), b.id());
    let serving = tokio::spawn(tunnel::serve(incoming.clone(), devices.router()));
    let key = devices.cb.public_key().unwrap();
    devices.da.reach.live.write().unwrap().insert(
        key.clone(),
        LiveRoute {
            connection: outgoing.clone(),
            signalled: Vec::new(),
        },
    );
    let client = crate::upstream_native::NativeClient::new_secure_candidates(
        Vec::new(),
        key,
        devices.ca.clone(),
    )
    .with_directory(devices.da.clone())
    .with_timeout(Duration::from_secs(1));
    assert!(!outgoing.paths().is_empty());
    assert!(outgoing.paths().iter().all(|path| path.is_relay()));
    timeout(Duration::from_secs(5), client.session_status())
        .await
        .expect("signed peer call timed out over the member relay")
        .expect("signed peer call failed over the member relay");
    assert!(outgoing.paths().iter().all(|path| path.is_relay()));
    // Remove only the relay. The SAME signed call must fail; both endpoints
    // and the authenticated peer router remain alive, with no IP transport.
    server.shutdown().await.unwrap();
    let result = timeout(Duration::from_secs(5), client.session_status())
        .await
        .unwrap();
    assert!(
        matches!(result, Err(crate::upstreams::UpstreamError::Unreachable(_))),
        "{result:?}"
    );
    serving.abort();
    outgoing.close(0u32.into(), b"test complete");
    incoming.close(0u32.into(), b"test complete");
    a.close().await;
    b.close().await;
}
