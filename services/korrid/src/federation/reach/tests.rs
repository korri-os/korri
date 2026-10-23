use super::*;
#[path = "lease_tests.rs"]
mod leases;
#[path = "order_tests.rs"]
mod order;
#[path = "refresh_tests.rs"]
mod refresh;
#[path = "relay_tests.rs"]
mod relaying;
#[path = "saves_tests.rs"]
mod saves;
#[path = "signalling_tests.rs"]
mod signalling;
use crate::{
    authorization::Authorization,
    federation::{FederationDirectory, PeerState},
    identity::DeviceIdentity,
    peer_rpc::{unix_time, PeerCredentials, PeerRpcServer},
};
use iroh::{
    endpoint::{Builder, PortmapperConfig},
    Endpoint,
};
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::Keys,
    types::Timestamp,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::time::timeout;

struct Devices {
    _a: tempfile::TempDir,
    b: tempfile::TempDir,
    owner: Keys,
    ca: PeerCredentials,
    cb: PeerCredentials,
    da: FederationDirectory,
    _db: FederationDirectory,
}
fn owned(root: &std::path::Path, owner: &Keys) -> DeviceIdentity {
    let mut identity = DeviceIdentity::load_or_create(root).unwrap();
    let key = identity.device_public_key().unwrap();
    let event = EventBuilder::new(Kind::Custom(30_078), "")
        .tags([
            Tag::parse(["d", &format!("org.korri.device-owner:{key}")]).unwrap(),
            Tag::parse(["device", key]).unwrap(),
            Tag::parse(["status", "owned"]).unwrap(),
        ])
        .custom_created_at(Timestamp::from(unix_time()))
        .finalize(owner)
        .unwrap()
        .as_json();
    identity.apply_owner_statement(&event).unwrap();
    identity
}
impl Devices {
    fn new() -> Self {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        use std::os::unix::fs::PermissionsExt;
        for root in [&a, &b] {
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let owner = Keys::generate();
        let (ia, ib) = (owned(a.path(), &owner), owned(b.path(), &owner));
        let (ca, cb) = (
            PeerCredentials::from_identity(ia.clone()),
            PeerCredentials::from_identity(ib.clone()),
        );
        let da = FederationDirectory::open(
            a.path(),
            ca.clone(),
            Authorization::load(a.path()).unwrap(),
            Arc::new(unix_time),
        )
        .unwrap();
        let db = FederationDirectory::open(
            b.path(),
            cb.clone(),
            Authorization::load(b.path()).unwrap(),
            Arc::new(unix_time),
        )
        .unwrap();
        da.apply_membership(
            &da.begin_work().unwrap(),
            &[ib.owner_statement_json().unwrap()],
        )
        .unwrap();
        db.apply_membership(
            &db.begin_work().unwrap(),
            &[ia.owner_statement_json().unwrap()],
        )
        .unwrap();
        Self {
            _a: a,
            b,
            owner,
            ca,
            cb,
            da,
            _db: db,
        }
    }
    fn router(&self) -> axum::Router {
        let config = self.b.path().join("host.toml");
        std::fs::write(&config, "label = \"Home PC\"\n[[games]]\nid = \"game\"\ntitle = \"Game\"\ncommand = [\"game\"]\n").unwrap();
        let runtime = crate::host::HostRuntime::from_paths_with_backend(
            &config,
            None,
            self.b.path().to_owned(),
            Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
        );
        let (app, _) = crate::app_states(runtime);
        PeerRpcServer::with_shared_authority(
            app,
            self.b.path(),
            self.cb.clone(),
            Authorization::load(self.b.path()).unwrap(),
        )
        .unwrap()
        .iroh_router()
    }
}
async fn endpoint() -> Endpoint {
    Builder::empty()
        .crypto_provider(iroh::tls::default_provider())
        .alpns(vec![ALPN.to_vec()])
        .relay_mode(iroh::RelayMode::Custom(iroh::RelayMap::empty()))
        .portmapper_config(PortmapperConfig::Disabled)
        .clear_ip_transports()
        .bind_addr("127.0.0.1:0")
        .unwrap()
        .bind()
        .await
        .unwrap()
}
pub(super) async fn pair() -> (
    Endpoint,
    Endpoint,
    iroh::endpoint::Connection,
    iroh::endpoint::Connection,
) {
    let (a, b) = (endpoint().await, endpoint().await);
    let (outgoing, incoming) = tokio::join!(a.connect(b.addr(), ALPN), async {
        b.accept().await.unwrap().await.unwrap()
    });
    (a, b, outgoing.unwrap(), incoming)
}

#[test]
fn reach_build_has_no_public_relay_or_discovery_defaults() {
    let cargo: toml::Value = toml::from_str(include_str!("../../../Cargo.toml")).unwrap();
    for name in ["iroh", "iroh-relay"] {
        let dependency = &cargo["dependencies"][name];
        assert_eq!(dependency["version"].as_str(), Some("=1.4.1"));
        assert_eq!(dependency["default-features"].as_bool(), Some(false));
    }
    let source = [
        include_str!("controller.rs"),
        include_str!("http.rs"),
        include_str!("relay.rs"),
        include_str!("tunnel.rs"),
    ]
    .concat();
    assert!(source.contains("Builder::empty()"));
    for forbidden in [
        "RelayMode::Default",
        "RelayMap::default()",
        "presets::N0",
        "presets::Minimal",
        "Endpoint::builder(",
        "iroh.link",
        "iroh.network",
        "n0.computer",
        "discovery.iroh",
    ] {
        assert!(
            !source.contains(forbidden),
            "forbidden default: {forbidden}"
        );
    }
}

#[tokio::test]
async fn reach_device_binding_rejects_wrong_device_endpoint_and_expiry() {
    let devices = Devices::new();
    let (a, b, outgoing, _incoming) = pair().await;
    let offer = signal::Offer {
        endpoint_id: b.id().to_string(),
        direct_addresses: controller::endpoint_addresses(&b),
        expires_at: unix_time() + 60,
        request_id: "ab".repeat(32),
    };
    let bindings = Arc::new(RwLock::new(BTreeMap::new()));
    assert!(controller::bind_offer(
        &devices.da,
        &bindings,
        &devices.cb.public_key().unwrap(),
        &offer,
        unix_time()
    )
    .is_ok());
    assert!(controller::bind_offer(
        &devices.da,
        &bindings,
        &devices.ca.public_key().unwrap(),
        &offer,
        unix_time()
    )
    .is_err());
    let mut expired = offer.clone();
    expired.expires_at = unix_time();
    assert!(controller::bind_offer(
        &devices.da,
        &bindings,
        &devices.cb.public_key().unwrap(),
        &expired,
        unix_time()
    )
    .is_err());
    assert!(controller::check_remote(&outgoing, b.id()).is_ok());
    assert!(controller::check_remote(&outgoing, a.id()).is_err());
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn reach_signed_peer_call_and_plaintext_403_over_real_iroh() {
    let devices = Devices::new();
    let (a, b, outgoing, incoming) = pair().await;
    let task = tokio::spawn(tunnel::serve(incoming, devices.router()));
    devices.da.reach.live.write().unwrap().insert(
        devices.cb.public_key().unwrap(),
        LiveRoute {
            connection: outgoing.clone(),
        },
    );
    let client = crate::upstream_native::NativeClient::new_secure_candidates(
        Vec::new(),
        devices.cb.public_key().unwrap(),
        devices.ca.clone(),
    )
    .with_directory(devices.da.clone());
    assert!(timeout(Duration::from_secs(5), client.session_status())
        .await
        .unwrap()
        .is_ok());
    assert_eq!(devices.da.snapshot().unwrap()[0].state, PeerState::Ready);
    let mut denied = http::post(
        &outgoing,
        "/rpc",
        "{\"_tag\":\"system.health\"}".into(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(denied.status(), axum::http::StatusCode::FORBIDDEN);
    while denied.chunk().await.unwrap().is_some() {}
    task.abort();
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn reach_failure_sentence_clears_after_recovery_and_forward_address_wins() {
    let devices = Devices::new();
    let key = devices.cb.public_key().unwrap();
    // Use the actual endpoint event producer, not an unsigned directory row.
    let now = unix_time();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed_origin = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let endpoint = crate::relay::EndpointRecord {
        device_public_key: key.clone(),
        owner_public_key: match devices.cb.identity_snapshot().unwrap().state() {
            crate::identity::IdentityState::Owned {
                owner_public_key, ..
            } => owner_public_key.clone(),
            _ => unreachable!(),
        },
        generation: 1,
        candidates: vec![closed_origin.clone()],
        issued_at: now,
        expires_at: now + 60,
        label: Some("Home PC".into()),
        moonlight_address: Some("192.168.1.2".into()),
    };
    let recipient = devices.ca.public_key().unwrap();
    let event = devices
        .cb
        .identity_snapshot()
        .unwrap()
        .encrypt_tagged_event(
            &recipient,
            crate::relay::ENDPOINT_EVENT_KIND,
            vec![
                vec!["d".into(), format!("org.korri.endpoint:{recipient}")],
                vec!["p".into(), recipient.clone()],
                vec!["expiration".into(), endpoint.expires_at.to_string()],
            ],
            &serde_json::to_string(&endpoint).unwrap(),
            now,
        )
        .unwrap();
    devices
        .da
        .apply_endpoint_event(&devices.da.begin_work().unwrap(), &event.json)
        .unwrap();
    let sentence = devices.da.unreachable_sentence(&key);
    assert_eq!(sentence, "Can't reach Home PC from this network right now");
    devices
        .da
        .set_peer_state(
            &devices.da.begin_peer_work(&key).unwrap(),
            &key,
            PeerState::Failed { error: sentence },
        )
        .unwrap();
    let registry = crate::upstreams::UpstreamRegistry::new_secure(Vec::new(), devices.ca.clone())
        .with_federation(devices.da.clone());
    assert_eq!(
        registry.moonlight_host_candidates().unwrap()[0].address,
        "192.168.1.2"
    );
    let configured = crate::upstreams::UpstreamRegistry::new_secure(
        vec![crate::upstreams::UpstreamHostConfig::native_secure(
            "Configured desk",
            closed_origin,
            key.clone(),
        )
        .with_moonlight_address("192.168.1.2")],
        devices.ca.clone(),
    )
    .with_federation(devices.da.clone());
    assert!(configured.catalog_snapshot().await.is_err());
    assert_eq!(
        devices.da.snapshot().unwrap()[0].state,
        PeerState::Failed {
            error: "Can't reach Configured desk from this network right now".into()
        }
    );
    let (a, b, outgoing, incoming) = pair().await;
    let task = tokio::spawn(tunnel::serve(incoming, devices.router()));
    devices.da.reach.live.write().unwrap().insert(
        key.clone(),
        LiveRoute {
            connection: outgoing,
        },
    );
    assert!(
        devices.da.stream_address(&key).is_none(),
        "a relay-only connection must not own forwards"
    );
    *devices.da.reach.bindings.write().unwrap() = Some(tunnel::Bindings {
        address: "127.0.0.1".parse().unwrap(),
        tcp: [0; 3],
        udp: [0; 5],
    });
    let lease = devices.da.activate_stream(&key).await.unwrap().unwrap();
    let address = lease.address().unwrap();
    assert_eq!(
        registry.moonlight_host_candidates().unwrap()[0].address,
        address
    );
    let client = crate::upstream_native::NativeClient::new_secure_candidates(
        Vec::new(),
        key.clone(),
        devices.ca.clone(),
    )
    .with_directory(devices.da.clone());
    client.session_status().await.unwrap();
    assert_eq!(devices.da.snapshot().unwrap()[0].state, PeerState::Ready);
    devices.da.reach.clear();
    assert_eq!(
        registry.moonlight_host_candidates().unwrap()[0].address,
        "192.168.1.2"
    );
    task.abort();
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn reach_relay_refuses_nonmembers_and_stops_existing_sessions_within_30_seconds() {
    use futures::{future::BoxFuture, StreamExt};
    use policy::{RelayEnvironment, RelayFacts};
    struct Environment(tokio::sync::watch::Receiver<RelayFacts>);
    impl RelayEnvironment for Environment {
        fn facts(&self) -> BoxFuture<'_, std::io::Result<RelayFacts>> {
            let value = *self.0.borrow();
            Box::pin(async move { Ok(value) })
        }
    }
    let devices = Devices::new();
    let endpoint = endpoint().await;
    let member_key = iroh::SecretKey::generate();
    let outsider_key = iroh::SecretKey::generate();
    let bindings = Arc::new(RwLock::new(BTreeMap::from([(
        member_key.public(),
        devices.cb.public_key().unwrap(),
    )])));
    let access = relay::Membership {
        directory: devices.da.clone(),
        bindings,
        local: endpoint.id(),
    };
    let (facts, receiver) = tokio::sync::watch::channel(RelayFacts {
        external_power: true,
        unmetered: true,
    });
    let advertised = Arc::new(RwLock::new(Vec::new()));
    let task = tokio::spawn(relay::supervise(
        Arc::new(Environment(receiver)),
        access,
        vec!["127.0.0.1".parse().unwrap()],
        advertised.clone(),
        0,
    ));
    let address = timeout(Duration::from_secs(3), async {
        loop {
            if let Some(address) = advertised.read().unwrap().first().copied() {
                break address;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let url: iroh::RelayUrl = format!("http://{address}").parse().unwrap();
    let resolver = endpoint.dns_resolver().unwrap().clone();
    assert!(
        iroh_relay::client::ClientBuilder::new(url.clone(), outsider_key, resolver.clone())
            .tls_client_config(
                iroh_relay::tls::CaTlsConfig::default()
                    .client_config(iroh::tls::default_provider())
                    .unwrap()
            )
            .connect()
            .await
            .is_err()
    );
    let mut client = iroh_relay::client::ClientBuilder::new(url, member_key, resolver)
        .tls_client_config(
            iroh_relay::tls::CaTlsConfig::default()
                .client_config(iroh::tls::default_provider())
                .unwrap(),
        )
        .connect()
        .await
        .unwrap();
    let started = tokio::time::Instant::now();
    facts
        .send(RelayFacts {
            external_power: false,
            unmetered: true,
        })
        .unwrap();
    timeout(Duration::from_secs(15), async {
        while let Some(message) = client.next().await {
            if message.is_err() {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(started.elapsed() < Duration::from_secs(30));
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    task.abort();
    endpoint.close().await;
    for (power, unmetered, expected) in [
        (false, false, false),
        (false, true, false),
        (true, false, false),
        (true, true, true),
    ] {
        assert_eq!(
            RelayFacts {
                external_power: power,
                unmetered
            }
            .eligible(),
            expected
        );
    }
    assert_eq!(controller::retry_delay(0), Duration::from_secs(5));
    assert_eq!(controller::retry_delay(32), Duration::from_secs(300));
}
