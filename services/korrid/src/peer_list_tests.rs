use super::*;
use crate::federation::{FederationDirectory, PeerState};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicU64, Ordering},
};

const NOW: u64 = 1_700_000_000;
const CAPABILITY: &str = "peer-list-capability";

struct PeerListFixture {
    root: tempfile::TempDir,
    readable: tempfile::TempDir,
    owner: Keys,
    resources: FederationResources,
    clock: Arc<AtomicU64>,
    peers: Vec<peer_rpc::PeerCredentials>,
    _peer_roots: Vec<tempfile::TempDir>,
}

impl PeerListFixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let owner = Keys::parse(&"41".repeat(32)).unwrap();
        let credentials = peer_rpc::test_owned_credentials(
            root.path(),
            &"42".repeat(32),
            &owner.secret_key().to_secret_hex(),
        );
        let authorization = authorization::Authorization::load(root.path()).unwrap();
        let clock = Arc::new(AtomicU64::new(NOW));
        let source = clock.clone();
        let directory = FederationDirectory::open(
            root.path(),
            credentials.clone(),
            authorization.clone(),
            Arc::new(move || source.load(Ordering::SeqCst)),
        )
        .unwrap();
        let peer_roots: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
        let peers = peer_roots
            .iter()
            .enumerate()
            .map(|(index, root)| {
                peer_rpc::test_owned_credentials(
                    root.path(),
                    &format!("{:02x}", index + 67).repeat(32),
                    &owner.secret_key().to_secret_hex(),
                )
            })
            .collect();
        Self {
            root,
            readable: tempfile::tempdir().unwrap(),
            owner,
            resources: FederationResources {
                credentials,
                authorization,
                directory,
            },
            clock,
            peers,
            _peer_roots: peer_roots,
        }
    }

    fn roster(&self) {
        let events: Vec<_> = self
            .peers
            .iter()
            .rev()
            .map(|peer| {
                peer.identity_snapshot()
                    .unwrap()
                    .owner_statement_json()
                    .unwrap()
            })
            .collect();
        self.resources
            .directory
            .apply_membership(&self.resources.directory.begin_work().unwrap(), &events)
            .unwrap();
    }

    fn router(&self) -> Router {
        router_with_capability_and_federation(
            CAPABILITY,
            "https://portal.example",
            self.readable.path(),
            self.root.path(),
            Some(self.resources.clone()),
            None,
        )
    }

    fn endpoint(&self, index: usize, label: &str) {
        let recipient = self.resources.credentials.public_key().unwrap();
        let record = relay::EndpointRecord {
            device_public_key: self.peers[index].public_key().unwrap(),
            owner_public_key: self.owner.public_key().to_hex(),
            generation: 1,
            candidates: vec!["http://127.0.0.1:9".into()],
            issued_at: NOW,
            expires_at: NOW + 600,
            label: Some(label.into()),
            moonlight_address: None,
        };
        let event = self.peers[index]
            .identity_snapshot()
            .unwrap()
            .encrypt_tagged_event(
                &recipient,
                relay::ENDPOINT_EVENT_KIND,
                vec![
                    vec!["d".into(), format!("org.korri.endpoint:{recipient}")],
                    vec!["p".into(), recipient.clone()],
                    vec!["expiration".into(), record.expires_at.to_string()],
                ],
                &serde_json::to_string(&record).unwrap(),
                NOW,
            )
            .unwrap();
        self.resources
            .directory
            .apply_endpoint_event(&self.resources.directory.begin_work().unwrap(), &event.json)
            .unwrap();
    }
}

fn peer_list_request() -> RpcRequest {
    serde_json::from_value(serde_json::json!({"_tag":"app.peer.list", "payload":{}})).unwrap()
}

async fn list_local(router: Router, capability: Option<&str>) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/rpc")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(capability) = capability {
        request = request.header(header::AUTHORIZATION, format!("Bearer {capability}"));
    }
    let response = router
        .oneshot(
            request
                .body(Body::from(
                    serde_json::to_vec(&peer_list_request()).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn peer_list_empty_and_local_capability_policy() {
    let f = PeerListFixture::new();
    for capability in [None, Some("wrong")] {
        assert_eq!(
            list_local(f.router(), capability).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let (status, value) = list_local(f.router(), Some(CAPABILITY)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        value,
        serde_json::json!({"_tag":"app.peer.list","outcome":{"_tag":"Ok","payload":{"peers":[]}}})
    );
}

#[tokio::test]
async fn peer_list_projects_one_sorted_snapshot_without_network_or_static_liveness() {
    let f = PeerListFixture::new();
    f.roster();
    f.endpoint(0, "Authenticated label");
    f.endpoint(1, "Overridden label");
    fs::write(f.readable.path().join("upstreams.json"), serde_json::json!([
        {"kind":"native","label":"Static label","baseUrl":"http://127.0.0.1:9","devicePublicKey":f.peers[1].public_key().unwrap(),"moonlightAddress":"peer:47989"},
        {"kind":"native","label":"Static only","baseUrl":"http://127.0.0.1:9","devicePublicKey":"ff".repeat(32),"moonlightAddress":"other:47989"}
    ]).to_string()).unwrap();
    let d = &f.resources.directory;
    f.clock.store(NOW + 2, Ordering::SeqCst);
    let key = f.peers[1].public_key().unwrap();
    d.set_peer_state(&d.begin_peer_work(&key).unwrap(), &key, PeerState::Ready)
        .unwrap();
    let key = f.peers[2].public_key().unwrap();
    d.set_peer_state(
        &d.begin_peer_work(&key).unwrap(),
        &key,
        PeerState::Failed {
            error: "Authenticated peer operation failed".into(),
        },
    )
    .unwrap();
    let app = f.router();
    let before = d.snapshot().unwrap();
    let (status, value) = list_local(app.clone(), Some(CAPABILITY)).await;
    assert_eq!(status, StatusCode::OK);
    let mut expected = vec![
        serde_json::json!({"devicePublicKey":f.peers[0].public_key().unwrap(),"label":"Authenticated label","state":"loading","updatedAt":NOW}),
        serde_json::json!({"devicePublicKey":f.peers[1].public_key().unwrap(),"label":"Static label","state":"ready","updatedAt":NOW+2}),
        serde_json::json!({"devicePublicKey":f.peers[2].public_key().unwrap(),"label":f.peers[2].public_key().unwrap(),"state":"failed","updatedAt":NOW+2,"lastError":"Authenticated peer operation failed"}),
    ];
    expected.sort_by(|a, b| {
        a["devicePublicKey"]
            .as_str()
            .cmp(&b["devicePublicKey"].as_str())
    });
    assert_eq!(
        value["outcome"]["payload"]["peers"],
        serde_json::json!(expected)
    );
    assert_eq!(list_local(app.clone(), Some(CAPABILITY)).await.1, value);
    for (before, after) in before.iter().zip(d.snapshot().unwrap()) {
        assert_eq!(before.state, after.state);
        assert_eq!(before.updated_at, after.updated_at);
    }
    // Endpoint expiry retains authenticated metadata; it does not prove failure.
    f.clock.store(NOW + 601, Ordering::SeqCst);
    assert_eq!(list_local(app.clone(), Some(CAPABILITY)).await.1, value);
    let key = f.peers[1].public_key().unwrap();
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[owner_revocation(&f.owner, &key, NOW + 601)],
    )
    .unwrap();
    let (_, value) = list_local(app, Some(CAPABILITY)).await;
    assert_eq!(
        value["outcome"]["payload"]["peers"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(!value.to_string().contains(&key));
}

#[tokio::test]
async fn peer_list_encrypted_owner_only_and_local_unix_control() {
    let f = PeerListFixture::new();
    f.roster();
    let config = f.readable.path().join("host.toml");
    fs::write(&config, "label = \"Host\"\n").unwrap();
    let runtime = host::HostRuntime::from_paths_with_backend(
        &config,
        None,
        f.root.path().to_owned(),
        Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
    );
    let (peer, local) =
        secure_host_routers_with_federation(runtime, f.root.path(), f.resources.clone(), None);
    let (status, expected) = list_local(local, None).await;
    assert_eq!(status, StatusCode::OK);
    let other_root = tempfile::tempdir().unwrap();
    let other =
        peer_rpc::test_owned_credentials(other_root.path(), &"51".repeat(32), &"52".repeat(32));
    let key = other.public_key().unwrap();
    let now = peer_rpc::unix_time();
    let mut callers = vec![
        (f.peers[0].clone(), StatusCode::OK),
        (other.clone(), StatusCode::FORBIDDEN),
    ];
    for tier in ["guest", "household"] {
        let pass = EventBuilder::new(Kind::Custom(authorization::PERSON_PASS_EVENT_KIND), "")
            .tags([
                Tag::parse(["d", &format!("org.korri.person-pass:{}", "11".repeat(32))]).unwrap(),
                Tag::parse(["device", &key]).unwrap(),
                Tag::parse(["tier", tier]).unwrap(),
                Tag::parse(["expires", &(now + 60).to_string()]).unwrap(),
                Tag::parse(["scope", authorization::STREAM_LAUNCH_SCOPE]).unwrap(),
                Tag::parse(["scope", authorization::CATALOG_READ_SCOPE]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(now))
            .finalize(&f.owner)
            .unwrap();
        callers.push((
            other.with_person_pass(Some(pass.as_json())),
            StatusCode::FORBIDDEN,
        ));
    }
    let host_key = f.resources.credentials.public_key().unwrap();
    // A valid scoped pass must work for catalog before PeerList rejects its policy.
    for (caller, _) in callers.iter().skip(2) {
        let encoded = caller
            .encode_request(
                &host_key,
                RpcRequest::CatalogSnapshot(CatalogSnapshotRequest {}),
                now,
            )
            .unwrap();
        let response = peer
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/peer-rpc")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(encoded.event_json))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    // Exercise the same encrypted router over an actual HTTP listener.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = peer.clone();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    for (caller, status) in callers {
        let encoded = caller
            .encode_request(&host_key, peer_list_request(), now)
            .unwrap();
        let response = reqwest::Client::new()
            .post(format!("http://{address}/peer-rpc"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(encoded.event_json.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        if status == StatusCode::OK {
            let bytes = response.bytes().await.unwrap();
            let decoded = caller
                .decode_response(
                    &host_key,
                    &encoded,
                    std::str::from_utf8(&bytes).unwrap(),
                    now,
                )
                .unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
        }
    }
    stop.send(()).unwrap();
    server.await.unwrap();
    let response = peer
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/peer-rpc")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&peer_list_request()).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn peer_list_uses_sanitized_native_failure_and_clears_error_on_recovery() {
    let f = PeerListFixture::new();
    f.roster();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let router = Router::new().route(
            "/peer-rpc",
            post(|| async { "untrusted private path /private/token and peer-controlled error" }),
        );
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    let key = f.peers[0].public_key().unwrap();
    let client = upstream_native::NativeClient::new_secure(
        format!("http://{address}"),
        key.clone(),
        f.resources.credentials.clone(),
    )
    .with_directory(f.resources.directory.clone());
    assert!(client.catalog_snapshot().await.is_err());
    stop.send(()).unwrap();
    server.await.unwrap();
    let app = f.router();
    let (_, response) = list_local(app.clone(), Some(CAPABILITY)).await;
    let entry = response["outcome"]["payload"]["peers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["devicePublicKey"] == key)
        .unwrap();
    assert_eq!(entry["state"], "failed");
    assert_eq!(entry["lastError"], "Authenticated peer operation failed");
    assert!(!response.to_string().contains("/private/token"));
    assert!(!response.to_string().contains(&address.to_string()));
    let d = &f.resources.directory;
    d.set_peer_state(&d.begin_peer_work(&key).unwrap(), &key, PeerState::Ready)
        .unwrap();
    let (_, response) = list_local(app, Some(CAPABILITY)).await;
    let entry = response["outcome"]["payload"]["peers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["devicePublicKey"] == key)
        .unwrap();
    assert_eq!(entry["state"], "ready");
    assert!(entry.get("lastError").is_none());
}

#[tokio::test]
async fn peer_list_rejects_timestamps_that_javascript_cannot_represent_exactly() {
    let f = PeerListFixture::new();
    f.roster();
    let key = f.peers[0].public_key().unwrap();
    let d = &f.resources.directory;
    f.clock.store(9_007_199_254_740_992, Ordering::SeqCst);
    d.set_peer_state(&d.begin_peer_work(&key).unwrap(), &key, PeerState::Ready)
        .unwrap();
    let (_, response) = list_local(f.router(), Some(CAPABILITY)).await;
    assert_eq!(
        response["outcome"]["payload"]["code"],
        "PeerListUnavailable"
    );
}

#[tokio::test]
async fn peer_list_directory_failure_is_sanitized() {
    let f = PeerListFixture::new();
    f.roster();
    let app = f.router();
    fs::set_permissions(
        f.root.path().join("federation/peers.json"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let (_, value) = list_local(app, Some(CAPABILITY)).await;
    assert_eq!(
        value["outcome"],
        serde_json::json!({"_tag":"Err","payload":{"code":"PeerListUnavailable","message":"peer directory unavailable"}})
    );
}
