use super::*;
use crate::{
    upstream_native::NativeClient,
    upstreams::{UpstreamHostConfig, UpstreamRegistry},
};

#[test]
fn strict_origins_reject_all_non_origin_forms() {
    for input in [
        "ws://peer",
        "file:///tmp/peer",
        "http://user:secret@peer",
        "http://@peer",
        "http://peer/path",
        "http://peer/?q=x",
        "http://peer/#fragment",
        "http://peer:0",
        "http://peer:65536",
        "http://peer:",
        "http://peer/../",
        " http://peer",
        "http://peer\n",
        "http:\\peer",
    ] {
        assert!(super::super::peer_origin(input).is_err(), "{input:?}");
        assert!(
            crate::relay::ConfiguredNativePeerDirectory::new(vec![
                crate::relay::ConfiguredNativePeer {
                    device_public_key: "ab".repeat(32),
                    endpoint: input.into()
                }
            ])
            .is_err(),
            "configured {input:?}"
        );
    }
    assert!(super::super::peer_origin(&format!("http://{}", "a".repeat(2048))).is_err());
    for (input, expected) in [
        ("HTTP://EXAMPLE.COM:80/", "http://example.com"),
        ("https://peer:443", "https://peer"),
        ("http://[::1]:43117/", "http://[::1]:43117"),
    ] {
        assert_eq!(super::super::peer_origin(input).unwrap(), expected);
    }
}

#[test]
fn strict_origins_apply_to_received_and_loaded_signed_evidence() {
    for candidate in [
        "ws://peer",
        "http://peer/path",
        "http://peer:0",
        "http://user:secret@peer",
        "http://peer/?x",
        "http://peer/#x",
    ] {
        let f = Fixture::new();
        let d = f.directory().unwrap();
        f.roster(&d);
        let mut endpoint = f.endpoint(1, 1000);
        endpoint.candidates = vec![candidate.into()];
        let event = f.event(&endpoint);
        assert!(
            d.apply_endpoint_event(&d.begin_work().unwrap(), &event)
                .is_err(),
            "{candidate}"
        );
        drop(d);
        let path = f.root.path().join("federation/peers.json");
        let mut memory: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        memory["peers"][f.key()]["endpointEvent"] = event.into();
        fs::write(path, serde_json::to_vec(&memory).unwrap()).unwrap();
        assert!(f.directory().is_err(), "load {candidate}");
    }
}

async fn serve(app: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    url
}

async fn host(f: &Fixture) -> String {
    let config = f._peer_root.path().join("host.toml");
    fs::write(
        &config,
        "label = \"remote\"\n[[games]]\nid = \"game\"\ntitle = \"Game\"\ncommand = [\"game\"]\n",
    )
    .unwrap();
    let runtime = crate::host::HostRuntime::from_paths_with_backend(
        &config,
        None,
        f._peer_root.path().into(),
        Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
    );
    serve(crate::secure_host_routers(runtime, f._peer_root.path(), None).0).await
}

async fn unavailable() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

fn announce(f: &Fixture, d: &FederationDirectory, urls: Vec<String>, moonlight: Option<String>) {
    let mut endpoint = f.endpoint(1, 1000);
    endpoint.candidates = urls;
    endpoint.moonlight_address = moonlight;
    d.apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&endpoint))
        .unwrap();
}

#[tokio::test]
async fn roster_catalog_source_session_and_optional_moonlight_survive_rebuild_and_restart() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let url = host(&f).await;
    announce(&f, &d, vec![unavailable().await, url.clone()], None);
    let registry =
        UpstreamRegistry::new_secure(vec![], f.credentials.clone()).with_federation(d.clone());
    assert!(registry.moonlight_host_candidates().is_err());
    let catalog = registry.catalog_snapshot().await.unwrap();
    assert_eq!(catalog.games.len(), 1);
    assert_eq!(catalog.games[0].source.label, "Living room");
    assert_eq!(catalog.games[0].source.device_public_key, Some(f.key()));
    assert!(registry.source_status(&f.key()).await.is_ok());
    let prepared = registry.prepare_stream("game", None).await.unwrap();
    assert!(registry
        .session_freeze(Some(&prepared.launch_id))
        .await
        .is_ok());
    assert!(registry
        .session_thaw(Some(&prepared.launch_id))
        .await
        .is_ok());
    assert!(registry
        .session_stop(Some(&prepared.launch_id), false)
        .await
        .is_ok());
    assert_eq!(d.snapshot().unwrap()[0].state, PeerState::Ready);
    drop(registry);
    drop(d);
    f.clock.store(2000, Ordering::SeqCst);
    let d = f.directory().unwrap();
    assert!(d.snapshot().unwrap()[0].current_endpoint.is_none());
    let registry = UpstreamRegistry::new_secure(vec![], f.credentials.clone()).with_federation(d);
    assert_eq!(registry.catalog_snapshot().await.unwrap().games.len(), 1);
    assert_eq!(
        reqwest::Client::new()
            .post(format!("{url}/rpc"))
            .json(&crate::RpcRequest::SessionStatus(
                crate::SessionStatusRequest {}
            ))
            .send()
            .await
            .unwrap()
            .status(),
        426
    );
    assert_eq!(
        reqwest::Client::new()
            .post(format!("{url}/peer-rpc"))
            .json(&crate::RpcRequest::SessionStatus(
                crate::SessionStatusRequest {}
            ))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
}

#[tokio::test]
async fn roster_only_moonlight_metadata_and_certificate_operations_use_the_same_peer() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let config = f._peer_root.path().join("host.toml");
    fs::write(&config, "label = \"remote\"\n").unwrap();
    let adapter = crate::tests::RecordingMoonlightCertificates::matching("sunshine-host");
    let runtime = crate::host::HostRuntime::from_paths_with_backends(
        &config,
        None,
        f._peer_root.path().into(),
        Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
        adapter.clone(),
    );
    let url = serve(crate::secure_host_routers(runtime, f._peer_root.path(), None).0).await;
    let mut endpoint = f.endpoint(1, 1000);
    endpoint.candidates = vec![url];
    endpoint.label = None;
    endpoint.moonlight_address = Some("living-room:47989".into());
    d.apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&endpoint))
        .unwrap();
    let registry =
        UpstreamRegistry::new_secure(vec![], f.credentials.clone()).with_federation(d.clone());
    let candidates = registry.moonlight_host_candidates().unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].label, f.key());
    assert_eq!(candidates[0].address, "living-room:47989");
    assert!(
        registry
            .moonlight_certificate_attest("sunshine-host")
            .await
            .unwrap()
            .matched
    );
    let certificate = "-----BEGIN CERTIFICATE-----\nclient\n-----END CERTIFICATE-----\n";
    assert!(registry
        .moonlight_certificate_provision("sunshine-host", certificate)
        .await
        .is_ok());
    assert!(registry
        .moonlight_certificate_revoke("sunshine-host", certificate)
        .await
        .is_ok());
    assert_eq!(
        adapter.calls(),
        vec![
            "attest:sunshine-host",
            "attest:sunshine-host",
            "provision:sunshine-host",
            "attest:sunshine-host",
            "revoke:sunshine-host"
        ]
    );
    assert_eq!(d.snapshot().unwrap()[0].state, PeerState::Ready);
}

#[tokio::test]
async fn static_overlay_deduplicates_fails_over_and_cannot_override_revocation() {
    let f = Fixture::new();
    let authorization = Authorization::load(f.root.path()).unwrap();
    let clock = f.clock.clone();
    let d = FederationDirectory::open(
        f.root.path(),
        f.credentials.clone(),
        authorization.clone(),
        Arc::new(move || clock.load(Ordering::SeqCst)),
    )
    .unwrap();
    f.roster(&d);
    announce(&f, &d, vec![host(&f).await], Some("relay:47989".into()));
    let registry = UpstreamRegistry::new_secure(
        vec![
            UpstreamHostConfig::native_secure("Static", unavailable().await, f.key())
                .with_moonlight_address("static:47989"),
        ],
        f.credentials.clone(),
    )
    .with_federation(d.clone());
    let catalog = registry.catalog_snapshot().await.unwrap();
    assert_eq!(catalog.games.len(), 1);
    assert_eq!(catalog.games[0].source.label, "Static");
    assert_eq!(
        registry.moonlight_host_candidates().unwrap()[0].address,
        "static:47989"
    );
    let stale = d.begin_peer_work(&f.key()).unwrap();
    let revoked = binding(&f.owner, &f.key(), "revoked", 1001);
    d.apply_membership(&d.begin_work().unwrap(), &[revoked])
        .unwrap();
    assert!(registry.catalog_snapshot().await.unwrap().games.is_empty());
    assert!(registry.source_status(&f.key()).await.is_err());
    assert!(registry.moonlight_host_candidates().is_err());
    assert!(d
        .set_peer_state(&stale, &f.key(), PeerState::Ready)
        .is_err());
}

#[tokio::test]
async fn relay_receive_exposes_original_signed_evidence_for_directory_ingestion() {
    use crate::relay::{CoordinatedRelays, InProcessRelayNetwork, RelayCoordinator, RelayList};
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let relays = RelayList::configured(vec!["ws://127.0.0.1:17001".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&relays));
    let publisher =
        CoordinatedRelays::new(relays.clone(), f.peer.clone(), network.clone()).unwrap();
    let reader =
        CoordinatedRelays::new(relays, f.credentials.identity_snapshot().unwrap(), network)
            .unwrap();
    publisher
        .publish_endpoint(
            &f.credentials.public_key().unwrap(),
            f.endpoint(1, 1000),
            1000,
        )
        .await
        .unwrap();
    let snapshot = reader.receive(1000).await.unwrap();
    assert_eq!(snapshot.endpoints.len(), 1);
    let evidence = &snapshot.endpoints[0];
    assert_eq!(evidence.record, f.endpoint(1, 1000));
    assert!(d
        .apply_endpoint_event(&d.begin_work().unwrap(), &evidence.event_json)
        .unwrap());
    assert_eq!(
        d.snapshot().unwrap()[0].remembered_endpoint.as_ref(),
        Some(&evidence.record)
    );
}

#[tokio::test]
async fn static_first_success_and_typed_application_failure_never_contact_later_candidates() {
    let f = Fixture::new();
    let good = host(&f).await;
    let later = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = later.clone();
    let fallback = serve(axum::Router::new().route(
        "/peer-rpc",
        axum::routing::post(move || {
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                "must not be called"
            }
        }),
    ))
    .await;
    let d = f.directory().unwrap();
    f.roster(&d);
    announce(&f, &d, vec![fallback.clone()], None);
    let registry = UpstreamRegistry::new_secure(
        vec![
            UpstreamHostConfig::native_secure("Static", good.clone(), f.key())
                .with_moonlight_address("static"),
        ],
        f.credentials.clone(),
    )
    .with_federation(d.clone());
    assert_eq!(registry.catalog_snapshot().await.unwrap().games.len(), 1);
    assert_eq!(later.load(Ordering::SeqCst), 0);
    // The first candidate returns a signed typed SourceDeviceMismatch. It is
    // terminal, rather than a reason to try another address of the same peer.
    let client = NativeClient::new_secure(good, f.key(), f.credentials.clone())
        .with_candidates(vec![host(&f).await, fallback])
        .unwrap()
        .with_directory(d.clone());
    assert!(
        matches!(client.source_status(&"ab".repeat(32)).await, Err(crate::upstreams::UpstreamError::Tagged { code, .. }) if code == "SourceDeviceMismatch")
    );
    assert_eq!(later.load(Ordering::SeqCst), 0);
    assert!(matches!(
        d.snapshot().unwrap()[0].state,
        PeerState::Failed { .. }
    ));
}

#[tokio::test]
async fn deferred_static_file_preserves_selected_route_while_using_live_directory() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let url = host(&f).await;
    announce(&f, &d, vec![url.clone()], None);
    let path = f.root.path().join("upstreams.json");
    let registry =
        UpstreamRegistry::from_env_or_file(&path, f.credentials.clone()).with_federation(d.clone());
    let prepared = registry.prepare_stream("game", None).await.unwrap();
    fs::write(&path, serde_json::json!([{"label":"Static", "kind":"native", "baseUrl":url, "devicePublicKey": f.key(), "moonlightAddress":"static"}]).to_string()).unwrap();
    // Resolving OnceLock must retain the previously selected exact launch even
    // though its mutable display label changed with the new static overlay.
    assert!(registry
        .session_freeze(Some(&prepared.launch_id))
        .await
        .is_ok());
    let crate::upstream::UpstreamSessionStatus::SessionStatus {
        active: Some(active),
    } = registry.session_status().await.unwrap()
    else {
        panic!("selected route lost")
    };
    assert_eq!(active.host.as_deref(), Some("Static"));
    assert_eq!(active.launch_id, prepared.launch_id);
    assert!(registry
        .session_stop(Some(&prepared.launch_id), false)
        .await
        .is_ok());
}

#[tokio::test]
async fn duplicate_effective_labels_cannot_select_a_peer_by_first_match() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut second = DeviceIdentity::load_or_create(root.path()).unwrap();
    second
        .apply_owner_statement(&binding(
            &f.owner,
            second.device_public_key().unwrap(),
            "owned",
            10,
        ))
        .unwrap();
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[second.owner_statement_json().unwrap()],
    )
    .unwrap();
    let label = second.device_public_key().unwrap();
    let registry = UpstreamRegistry::new_secure(
        vec![
            UpstreamHostConfig::native_secure(label, unavailable().await, f.key())
                .with_moonlight_address("static"),
        ],
        f.credentials.clone(),
    )
    .with_federation(d);
    assert!(
        matches!(registry.prepare_stream("game", Some(label)).await, Err(crate::upstreams::UpstreamError::Failure(message)) if message.contains("ambiguous"))
    );
}

#[tokio::test]
async fn secure_static_origins_fail_closed_even_with_directory() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    for url in [
        "http://peer/path",
        "http://peer:0",
        "https://user:secret@peer",
        "ws://peer",
        "http://peer?x",
        "http://peer#x",
    ] {
        let registry = UpstreamRegistry::new_secure(
            vec![
                UpstreamHostConfig::native_secure("Static", url.into(), f.key())
                    .with_moonlight_address("peer"),
            ],
            f.credentials.clone(),
        )
        .with_federation(d.clone());
        assert!(registry.catalog_snapshot().await.is_err(), "{url}");
        assert!(registry.moonlight_host_candidates().is_err(), "{url}");
        assert!(registry.session_status().await.is_err(), "{url}");
    }
}

#[tokio::test]
async fn candidate_read_failover_but_mutations_do_not_repeat_ambiguous_delivery() {
    let f = Fixture::new();
    let good = host(&f).await;
    let received = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = received.clone();
    // A real HTTP server consumes the encrypted request but cannot return a
    // complete response. This is not evidence that the effect did not happen.
    let lost = serve(axum::Router::new().route(
        "/peer-rpc",
        axum::routing::post(move |body: String| {
            let counter = counter.clone();
            async move {
                assert!(!body.contains("gameId"));
                counter.fetch_add(1, Ordering::SeqCst);
                axum::http::Response::builder()
                    .header("content-length", "100")
                    .body(axum::body::Body::empty())
                    .unwrap()
            }
        }),
    ))
    .await;
    let client = NativeClient::new_secure(lost.clone(), f.key(), f.credentials.clone())
        .with_candidates(vec![lost, good.clone()])
        .unwrap();
    assert!(client.prepare_stream("game").await.is_err());
    assert_eq!(received.load(Ordering::SeqCst), 1);
    let status = NativeClient::new_secure(good, f.key(), f.credentials.clone())
        .session_status()
        .await
        .unwrap();
    assert!(matches!(
        status,
        crate::upstream::UpstreamSessionStatus::SessionStatus { active: None }
    ));
    assert!(client.catalog_snapshot().await.is_ok());
    let pre_send = NativeClient::new_secure_candidates(
        vec![unavailable().await, host(&f).await],
        f.key(),
        f.credentials.clone(),
    );
    assert!(pre_send.prepare_stream("game").await.is_ok());
}

#[tokio::test]
async fn wrong_key_reads_fail_over_with_fresh_signed_requests_but_mutations_stop() {
    let f = Fixture::new();
    let good = host(&f).await;
    let other_root = tempfile::tempdir().unwrap();
    let other = DeviceIdentity::load_or_create(other_root.path()).unwrap();
    let wrong_response = other
        .encrypt_event(
            &f.credentials.public_key().unwrap(),
            crate::peer_rpc::PEER_RESPONSE_KIND,
            "{}",
            crate::peer_rpc::unix_time(),
        )
        .unwrap()
        .json;
    let requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let received = requests.clone();
    let wrong = serve(axum::Router::new().route(
        "/peer-rpc",
        axum::routing::post(move |body: String| {
            let received = received.clone();
            let response = wrong_response.clone();
            async move {
                received.lock().unwrap().push(body);
                response
            }
        }),
    ))
    .await;
    let received = requests.clone();
    let target = good.clone();
    let forwarding = serve(axum::Router::new().route(
        "/peer-rpc",
        axum::routing::post(move |body: String| {
            let received = received.clone();
            let target = target.clone();
            async move {
                received.lock().unwrap().push(body.clone());
                reqwest::Client::new()
                    .post(format!("{target}/peer-rpc"))
                    .body(body)
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
            }
        }),
    ))
    .await;
    let client = NativeClient::new_secure(wrong.clone(), f.key(), f.credentials.clone())
        .with_candidates(vec![wrong, forwarding])
        .unwrap();
    assert_eq!(client.catalog_snapshot().await.unwrap().games.len(), 1);
    {
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        let first: serde_json::Value =
            serde_json::from_str(&f.peer.decrypt_event(&requests[0]).unwrap()).unwrap();
        let second: serde_json::Value =
            serde_json::from_str(&f.peer.decrypt_event(&requests[1]).unwrap()).unwrap();
        assert_ne!(first["nonce"], second["nonce"]);
        assert_ne!(first["requestId"], second["requestId"]);
    }
    for operation in 0..7 {
        let before = requests.lock().unwrap().len();
        let result = match operation {
            0 => client.prepare_stream("game").await.map(|_| ()),
            1 => client.session_stop("launch", false).await.map(|_| ()),
            2 => client.session_freeze("launch").await.map(|_| ()),
            3 => client.session_thaw("launch").await.map(|_| ()),
            4 => client
                .moonlight_certificate_attest("host")
                .await
                .map(|_| ()),
            5 => client
                .moonlight_certificate_provision("host", "secret certificate")
                .await
                .map(|_| ()),
            _ => client
                .moonlight_certificate_revoke("host", "secret certificate")
                .await
                .map(|_| ()),
        };
        assert!(result.is_err());
        assert_eq!(requests.lock().unwrap().len(), before + 1);
    }
}

#[tokio::test]
async fn candidates_share_one_timeout_and_all_failures_mark_failed_after_completion() {
    let f = Fixture::new();
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut urls = Vec::new();
    for _ in 0..3 {
        let attempts = attempts.clone();
        urls.push(
            serve(axum::Router::new().route(
                "/peer-rpc",
                axum::routing::post(move || {
                    let attempts = attempts.clone();
                    async move {
                        attempts.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        "unavailable"
                    }
                }),
            ))
            .await,
        );
    }
    let d = f.directory().unwrap();
    f.roster(&d);
    announce(&f, &d, urls.clone(), None);
    let client = NativeClient::new_secure(urls[0].clone(), f.key(), f.credentials.clone())
        .with_candidates(urls)
        .unwrap()
        .with_directory(d.clone())
        .with_timeout(std::time::Duration::from_millis(240));
    let start = std::time::Instant::now();
    assert!(client.catalog_snapshot().await.is_err());
    assert!(start.elapsed() < std::time::Duration::from_millis(600));
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert!(matches!(
        d.snapshot().unwrap()[0].state,
        PeerState::Failed { .. }
    ));
}

#[tokio::test]
async fn static_peer_without_roster_membership_obeys_shared_revocation() {
    let f = Fixture::new();
    let authorization = Authorization::load(f.root.path()).unwrap();
    let d = FederationDirectory::open(
        f.root.path(),
        f.credentials.clone(),
        authorization.clone(),
        Arc::new(|| 1000),
    )
    .unwrap();
    let registry = UpstreamRegistry::new_secure(
        vec![
            UpstreamHostConfig::native_secure("Static", host(&f).await, f.key())
                .with_moonlight_address("static"),
        ],
        f.credentials.clone(),
    )
    .with_federation(d.clone());
    assert!(d.snapshot().unwrap().is_empty());
    assert_eq!(registry.catalog_snapshot().await.unwrap().games.len(), 1);
    let local = f.credentials.identity_snapshot().unwrap();
    let attempt = authorization
        .attempt(
            local.state(),
            local.device_public_key().unwrap(),
            local.owner_statement_json().as_deref(),
            None,
            &[binding(&f.owner, &f.key(), "revoked", 1001)],
            1001,
        )
        .unwrap();
    authorization.commit_revocations(&attempt).unwrap();
    assert!(registry.catalog_snapshot().await.unwrap().games.is_empty());
    assert!(registry.moonlight_host_candidates().is_err());
}

#[tokio::test]
async fn revoked_peer_cannot_complete_an_in_flight_catalog_success() {
    let f = Fixture::new();
    let good = host(&f).await;
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let notify = started.clone();
    let wait = release.clone();
    let gateway = serve(axum::Router::new().route(
        "/peer-rpc",
        axum::routing::post(move |body: String| {
            let notify = notify.clone();
            let wait = wait.clone();
            let good = good.clone();
            async move {
                notify.notify_one();
                wait.notified().await;
                reqwest::Client::new()
                    .post(format!("{good}/peer-rpc"))
                    .body(body)
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
            }
        }),
    ))
    .await;
    let d = f.directory().unwrap();
    f.roster(&d);
    announce(&f, &d, vec![gateway], None);
    let registry =
        UpstreamRegistry::new_secure(vec![], f.credentials.clone()).with_federation(d.clone());
    let task = tokio::spawn(async move { registry.catalog_snapshot().await });
    started.notified().await;
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[binding(&f.owner, &f.key(), "revoked", 1001)],
    )
    .unwrap();
    release.notify_one();
    assert!(task.await.unwrap().is_err());
    assert!(d.snapshot().unwrap().is_empty());
}

async fn second_peer(f: &Fixture) -> (tempfile::TempDir, String, String) {
    let root = tempfile::tempdir().unwrap();
    let mut identity = DeviceIdentity::load_or_create(root.path()).unwrap();
    let key = identity.device_public_key().unwrap().to_owned();
    identity
        .apply_owner_statement(&binding(&f.owner, &key, "owned", 10))
        .unwrap();
    let config = root.path().join("host.toml");
    fs::write(
        &config,
        "label = \"B\"\n[[games]]\nid = \"game\"\ntitle = \"Game B\"\ncommand = [\"game\"]\n",
    )
    .unwrap();
    let runtime = crate::host::HostRuntime::from_paths_with_backends(
        &config,
        None,
        root.path().into(),
        Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
        crate::tests::RecordingMoonlightCertificates::matching("different-host"),
    );
    let url = serve(crate::secure_host_routers(runtime, root.path(), None).0).await;
    (root, key, url)
}

#[derive(Clone, Copy, Debug)]
enum BufferedOperation {
    Catalog,
    Recovery,
    Certificate,
}

async fn revoked_buffered_result_is_unavailable(operation: BufferedOperation) {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let config = f._peer_root.path().join("host.toml");
    fs::write(
        &config,
        "label = \"A\"\n[[games]]\nid = \"game\"\ntitle = \"Game A\"\ncommand = [\"game\"]\n",
    )
    .unwrap();
    let runtime = crate::host::HostRuntime::from_paths_with_backends(
        &config,
        None,
        f._peer_root.path().into(),
        Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
        crate::tests::RecordingMoonlightCertificates::matching("sunshine-host"),
    );
    let a_url = serve(crate::secure_host_routers(runtime, f._peer_root.path(), None).0).await;
    if matches!(operation, BufferedOperation::Recovery) {
        NativeClient::new_secure(a_url.clone(), f.key(), f.credentials.clone())
            .prepare_stream("game")
            .await
            .unwrap();
    }
    announce(&f, &d, vec![a_url], None);
    let (_b_root, b_key, b_url) = second_peer(&f).await;
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let notify = started.clone();
    let wait = release.clone();
    let gateway = serve(axum::Router::new().route(
        "/peer-rpc",
        axum::routing::post(move |body: String| {
            let notify = notify.clone();
            let wait = wait.clone();
            let b_url = b_url.clone();
            async move {
                notify.notify_one();
                wait.notified().await;
                reqwest::Client::new()
                    .post(format!("{b_url}/peer-rpc"))
                    .body(body)
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
            }
        }),
    ))
    .await;
    let registry = UpstreamRegistry::new_secure(
        vec![UpstreamHostConfig::native_secure("B", gateway, b_key).with_moonlight_address("B")],
        f.credentials.clone(),
    )
    .with_federation(d.clone());
    let key = f.key();
    let task = tokio::spawn(async move {
        match operation {
            BufferedOperation::Catalog => {
                let catalog = registry.catalog_snapshot().await.unwrap();
                !catalog
                    .games
                    .iter()
                    .any(|game| game.source.device_public_key.as_deref() == Some(key.as_str()))
                    && catalog.games.len() == 1
            }
            BufferedOperation::Recovery => matches!(
                registry.session_status().await,
                Err(crate::upstreams::UpstreamError::NativeSessionRecoveryIncomplete)
            ),
            BufferedOperation::Certificate => registry
                .moonlight_certificate_attest("sunshine-host")
                .await
                .is_err(),
        }
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), started.notified())
        .await
        .unwrap();
    // The current-thread runtime cannot observe Ready until A's call has
    // returned to join_all. B is held at the gateway, so A is buffered.
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while d.snapshot().unwrap()[0].state != PeerState::Ready {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!task.is_finished());
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[binding(&f.owner, &f.key(), "revoked", 1001)],
    )
    .unwrap();
    release.notify_one();
    assert!(
        task.await.unwrap(),
        "buffered {operation:?} escaped revocation"
    );
}

#[tokio::test]
async fn revoked_buffered_catalog_is_not_published_after_slow_peer_completes() {
    revoked_buffered_result_is_unavailable(BufferedOperation::Catalog).await;
}

#[tokio::test]
async fn revoked_buffered_session_is_not_recovered_after_slow_peer_completes() {
    revoked_buffered_result_is_unavailable(BufferedOperation::Recovery).await;
}

#[tokio::test]
async fn revoked_buffered_certificate_is_not_matched_after_slow_peer_completes() {
    revoked_buffered_result_is_unavailable(BufferedOperation::Certificate).await;
}

#[tokio::test]
async fn revoked_selection_allows_new_prepare_without_stopping_the_old_execution() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let a_url = host(&f).await;
    announce(&f, &d, vec![a_url.clone()], None);
    let (_b_root, b_key, b_url) = second_peer(&f).await;
    let registry = UpstreamRegistry::new_secure(
        vec![UpstreamHostConfig::native_secure("B", b_url, b_key).with_moonlight_address("B")],
        f.credentials.clone(),
    )
    .with_federation(d.clone());
    let a = registry
        .prepare_stream("game", Some("Living room"))
        .await
        .unwrap();
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[binding(&f.owner, &f.key(), "revoked", 1001)],
    )
    .unwrap();
    let b = registry.prepare_stream("game", Some("B")).await.unwrap();
    assert_ne!(a.launch_id, b.launch_id);
    assert!(matches!(
        registry.session_stop(Some(&a.launch_id), false).await,
        Err(crate::upstreams::UpstreamError::StaleLaunchIdentity)
    ));
    let crate::upstream::UpstreamSessionStatus::SessionStatus {
        active: Some(active),
    } = registry.session_status().await.unwrap()
    else {
        panic!("B selection lost")
    };
    assert_eq!(active.launch_id, b.launch_id);
    // A did not receive the local revocation. Retiring local control is not
    // evidence of a remote stop: its real runtime still has the old launch.
    let crate::upstream::UpstreamSessionStatus::SessionStatus {
        active: Some(active),
    } = NativeClient::new_secure(a_url, f.key(), f.credentials.clone())
        .session_status()
        .await
        .unwrap()
    else {
        panic!("retirement must not stop A")
    };
    assert_eq!(active.launch_id, a.launch_id);
    registry
        .session_stop(Some(&b.launch_id), false)
        .await
        .unwrap();
}

#[tokio::test]
async fn revoked_selected_controls_report_unavailable_not_a_remote_stop() {
    for operation in ["status", "stop", "freeze", "thaw"] {
        let f = Fixture::new();
        let d = f.directory().unwrap();
        f.roster(&d);
        let url = host(&f).await;
        announce(&f, &d, vec![url.clone()], None);
        let registry =
            UpstreamRegistry::new_secure(vec![], f.credentials.clone()).with_federation(d.clone());
        let prepared = registry.prepare_stream("game", None).await.unwrap();
        d.apply_membership(
            &d.begin_work().unwrap(),
            &[binding(&f.owner, &f.key(), "revoked", 1001)],
        )
        .unwrap();
        let result = match operation {
            "status" => registry.session_status().await.map(|_| ()),
            "stop" => registry
                .session_stop(Some(&prepared.launch_id), false)
                .await
                .map(|_| ()),
            "freeze" => registry
                .session_freeze(Some(&prepared.launch_id))
                .await
                .map(|_| ()),
            "thaw" => registry
                .session_thaw(Some(&prepared.launch_id))
                .await
                .map(|_| ()),
            _ => unreachable!(),
        };
        assert!(
            matches!(
                result,
                Err(crate::upstreams::UpstreamError::SourcePeerNotFound)
            ),
            "{operation}: {result:?}"
        );
        // A second read has no selected route to query. The first call retired
        // control, without issuing a successful stop or changing A's runtime.
        assert!(matches!(
            registry.session_status().await.unwrap(),
            crate::upstream::UpstreamSessionStatus::SessionStatus { active: None }
        ));
        let crate::upstream::UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = NativeClient::new_secure(url, f.key(), f.credentials.clone())
            .session_status()
            .await
            .unwrap()
        else {
            panic!("{operation} must not stop A")
        };
        assert_eq!(active.launch_id, prepared.launch_id);
    }
}

#[tokio::test]
async fn selected_peer_outage_does_not_retire_control_or_allow_a_second_prepare() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let url = host(&f).await;
    announce(&f, &d, vec![url.clone()], None);
    let (_b_root, b_key, b_url) = second_peer(&f).await;
    let registry = UpstreamRegistry::new_secure(
        vec![UpstreamHostConfig::native_secure("B", b_url, b_key).with_moonlight_address("B")],
        f.credentials.clone(),
    )
    .with_federation(d.clone());
    let prepared = registry
        .prepare_stream("game", Some("Living room"))
        .await
        .unwrap();
    let mut endpoint = f.endpoint(2, 1001);
    endpoint.candidates = vec![unavailable().await];
    d.apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&endpoint))
        .unwrap();
    assert!(matches!(
        registry.session_status().await,
        Err(crate::upstreams::UpstreamError::Unreachable(_))
    ));
    assert!(matches!(
        registry.prepare_stream("game", Some("B")).await,
        Err(crate::upstreams::UpstreamError::Unreachable(_))
    ));
    // Exact selection remains in force despite the failed status and prepare.
    assert!(matches!(
        registry.session_stop(Some("wrong-launch"), false).await,
        Err(crate::upstreams::UpstreamError::StaleLaunchIdentity)
    ));
    endpoint.generation = 3;
    endpoint.candidates = vec![url];
    d.apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&endpoint))
        .unwrap();
    let crate::upstream::UpstreamSessionStatus::SessionStatus {
        active: Some(active),
    } = registry.session_status().await.unwrap()
    else {
        panic!("outage lost selection")
    };
    assert_eq!(active.launch_id, prepared.launch_id);
    registry
        .session_stop(Some(&prepared.launch_id), false)
        .await
        .unwrap();
}

#[tokio::test]
async fn secure_constructor_rejects_raw_double_slash_before_origin_normalization() {
    let f = Fixture::new();
    let url = host(&f).await;
    let client = NativeClient::new_secure(format!("{url}//"), f.key(), f.credentials.clone());
    assert!(matches!(
        client.catalog_snapshot().await,
        Err(crate::upstreams::UpstreamError::Wire(message)) if message == "invalid peer origin"
    ));
    assert!(
        NativeClient::new_secure(format!("{url}/"), f.key(), f.credentials.clone())
            .catalog_snapshot()
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn registry_timeout_records_the_completed_failure() {
    let f = Fixture::new();
    let slow = serve(axum::Router::new().route(
        "/peer-rpc",
        axum::routing::post(|| async {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            "unavailable"
        }),
    ))
    .await;
    let d = f.directory().unwrap();
    f.roster(&d);
    announce(&f, &d, vec![slow], None);
    let registry =
        UpstreamRegistry::new_secure(vec![], f.credentials.clone()).with_federation(d.clone());
    assert!(registry.catalog_snapshot().await.is_err());
    assert!(matches!(
        d.snapshot().unwrap()[0].state,
        PeerState::Failed { .. }
    ));
}

#[tokio::test]
async fn newer_failure_dominates_a_late_success_from_the_registry() {
    let f = Fixture::new();
    let good = host(&f).await;
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let notify = started.clone();
    let wait = release.clone();
    let gateway = serve(axum::Router::new().route(
        "/peer-rpc",
        axum::routing::post(move |body: String| {
            let count = count.clone();
            let notify = notify.clone();
            let wait = wait.clone();
            let good = good.clone();
            async move {
                if count.fetch_add(1, Ordering::SeqCst) != 0 {
                    return (
                        axum::http::StatusCode::SERVICE_UNAVAILABLE,
                        "remote secret\ncredentials".to_owned(),
                    );
                }
                notify.notify_one();
                wait.notified().await;
                (
                    axum::http::StatusCode::OK,
                    reqwest::Client::new()
                        .post(format!("{good}/peer-rpc"))
                        .body(body)
                        .send()
                        .await
                        .unwrap()
                        .text()
                        .await
                        .unwrap(),
                )
            }
        }),
    ))
    .await;
    let d = f.directory().unwrap();
    f.roster(&d);
    announce(&f, &d, vec![gateway], None);
    let registry =
        UpstreamRegistry::new_secure(vec![], f.credentials.clone()).with_federation(d.clone());
    let old = registry.clone();
    let task = tokio::spawn(async move { old.catalog_snapshot().await });
    started.notified().await;
    assert_eq!(d.snapshot().unwrap()[0].state, PeerState::Loading);
    assert!(registry.catalog_snapshot().await.is_err());
    let failed = d.snapshot().unwrap()[0].state.clone();
    let PeerState::Failed { error } = &failed else {
        panic!("expected completed failure")
    };
    assert!(!error.contains("remote secret"));
    assert!(!error.contains("credentials"));
    release.notify_one();
    assert!(task.await.unwrap().is_ok());
    assert_eq!(d.snapshot().unwrap()[0].state, failed);
}
