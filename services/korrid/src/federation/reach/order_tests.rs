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
async fn reach_direct_failure_waits_five_seconds_before_the_member_relay() {
    let devices = Devices::new();
    let (a, b) = (endpoint().await, endpoint().await);
    let bindings = Arc::new(RwLock::new(BTreeMap::from([
        (a.id(), devices.ca.public_key().unwrap()),
        (b.id(), devices.cb.public_key().unwrap()),
    ])));
    let access = relay::Membership {
        directory: devices._db.clone(),
        bindings,
        local: b.id(),
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
        controller::connect_iroh(&a, b.id(), Vec::new(), vec![address]),
        async {
            let dial = controller::connect_iroh(&b, a.id(), Vec::new(), vec![address]);
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
    outgoing.close(0u32.into(), b"test complete");
    incoming.close(0u32.into(), b"test complete");
    server.shutdown().await.unwrap();
    a.close().await;
    b.close().await;
}
