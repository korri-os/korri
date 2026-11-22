use super::*;
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
use std::{sync::Arc, time::Duration};
use tokio::time::timeout;

pub(super) struct Devices {
    _a: tempfile::TempDir,
    pub(super) b: tempfile::TempDir,
    pub(super) ca: PeerCredentials,
    pub(super) cb: PeerCredentials,
    pub(super) da: FederationDirectory,
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
    pub(super) fn new() -> Self {
        Self::with_receiver_roster(true)
    }
    fn with_receiver_roster(known: bool) -> Self {
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
        if known {
            db.apply_membership(
                &db.begin_work().unwrap(),
                &[ia.owner_statement_json().unwrap()],
            )
            .unwrap();
        }
        Self {
            _a: a,
            b,
            ca,
            cb,
            da,
            _db: db,
        }
    }
    pub(super) fn router(&self) -> axum::Router {
        Self::router_on(self.b.path(), self.cb.clone())
    }
    fn router_on(root: &std::path::Path, credentials: PeerCredentials) -> axum::Router {
        let config = root.join("host.toml");
        std::fs::write(&config, "label = \"Home PC\"\n[[games]]\nid = \"game\"\ntitle = \"Game\"\ncommand = [\"game\"]\n").unwrap();
        let runtime = crate::host::HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.to_owned(),
            Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
        );
        let (app, _) = crate::app_states(runtime);
        PeerRpcServer::with_shared_authority(
            app,
            root,
            credentials,
            Authorization::load(root).unwrap(),
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
    a.address_lookup()
        .unwrap()
        .add(iroh::address_lookup::memory::MemoryLookup::from_endpoint_info([b.addr()]));
    let (outgoing, incoming) = tokio::join!(a.connect(b.id(), ALPN), async {
        b.accept().await.unwrap().await.unwrap()
    });
    (a, b, outgoing.unwrap(), incoming)
}

/// A same-owner directory with a real loopback iroh netplay route.
pub(crate) struct NetplayRoute {
    pub(crate) directory: FederationDirectory,
    pub(crate) device: String,
    pub(crate) forward: Option<NetplayForward>,
    echo: tokio::task::JoinHandle<()>,
    _endpoints: [Endpoint; 2],
    _devices: Devices,
}
impl Drop for NetplayRoute {
    fn drop(&mut self) {
        self.echo.abort();
    }
}
pub(crate) async fn netplay_route() -> NetplayRoute {
    let devices = Devices::new();
    let (a, b, outgoing, incoming) = pair().await;
    let device = devices.cb.public_key().unwrap();
    devices.da.reach.live.write().unwrap().insert(
        device.clone(),
        PeerConnection {
            connection: outgoing.clone(),
            responses: tokio::sync::broadcast::channel(64).0,
        },
    );
    let forward = devices.da.netplay_forward(&device).await.unwrap();
    NetplayRoute {
        directory: devices.da.clone(),
        device,
        forward: Some(forward),
        echo: tunnel::netplay_echo(incoming).await,
        _endpoints: [a, b],
        _devices: devices,
    }
}

#[tokio::test]
async fn key_only_connection_carries_signed_peer_rpc_and_rejects_local_authority() {
    let devices = Devices::new();
    let (a, b, outgoing, incoming) = pair().await;
    let task = tokio::spawn(tunnel::serve(
        incoming,
        devices.router(),
        tokio::sync::broadcast::channel(64).0,
        None,
    ));
    devices.da.reach.live.write().unwrap().insert(
        devices.cb.public_key().unwrap(),
        PeerConnection {
            connection: outgoing.clone(),
            responses: tokio::sync::broadcast::channel(64).0,
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
    let denied = http::post(
        &outgoing,
        "/rpc",
        "{\"_tag\":\"system.health\"}".into(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(denied.status(), axum::http::StatusCode::FORBIDDEN);
    task.abort();
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn closed_connection_is_terminal_and_reported_once_without_redial() {
    let devices = Devices::new();
    let (a, b, outgoing, incoming) = pair().await;
    let directory = devices.da.clone();
    let device = devices.cb.public_key().unwrap();
    directory.reach.router.set(axum::Router::new()).unwrap();
    endpoint::admit(directory.clone(), Some(device.clone()), outgoing.clone());
    incoming.close(0u32.into(), b"test connection gone");
    timeout(Duration::from_secs(5), async {
        while !matches!(
            directory.snapshot().unwrap()[0].state,
            PeerState::Failed { .. }
        ) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let updated = directory.snapshot().unwrap()[0].updated_at;
    for _ in 0..4 {
        assert!(directory.reach_connection(&device).await.is_err());
    }
    assert_eq!(directory.snapshot().unwrap()[0].updated_at, updated);
    assert_eq!(directory.reach.live.read().unwrap().len(), 1);
    a.close().await;
    b.close().await;
}

/// Test composition uses iroh's real in-memory address lookup, without public services.
pub(crate) async fn listen(
    directory: FederationDirectory,
    router: axum::Router,
) -> (Endpoint, tokio::task::JoinHandle<()>) {
    let endpoint = endpoint().await;
    directory.reach.router.set(router).unwrap();
    directory.reach.endpoint.set(endpoint.clone()).unwrap();
    let server = endpoint.clone();
    let task = tokio::spawn(async move {
        while let Some(incoming) = server.accept().await {
            let directory = directory.clone();
            tokio::spawn(async move {
                if let Ok(connection) = incoming.await {
                    endpoint::admit(directory, None, connection);
                }
            });
        }
    });
    (endpoint, task)
}
pub(crate) fn learn(directory: &FederationDirectory, peer: &Endpoint) {
    directory
        .reach
        .endpoint
        .get()
        .unwrap()
        .address_lookup()
        .unwrap()
        .add(iroh::address_lookup::memory::MemoryLookup::from_endpoint_info([peer.addr()]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_signed_http_contact_binds_an_unknown_caller_before_stream_channels_open() {
    use tokio::io::AsyncReadExt;
    let devices = Devices::with_receiver_roster(false);
    let (a, b, outgoing, incoming) = pair().await;
    assert!(devices._db.snapshot().unwrap().is_empty());
    devices._db.reach.router.set(devices.router()).unwrap();
    devices._db.reach.endpoint.set(b.clone()).unwrap();
    endpoint::admit(devices._db.clone(), None, incoming);
    async fn stream(connection: &Connection) -> io::Result<u8> {
        let (mut send, mut recv) = connection.open_bi().await.map_err(io::Error::other)?;
        send.write_all(&[2]).await.map_err(io::Error::other)?;
        recv.read_u8().await
    }
    assert!(
        stream(&outgoing).await.is_err(),
        "unbound stream must be refused"
    );
    let signed = devices
        .ca
        .encode_request(
            &devices.cb.public_key().unwrap(),
            crate::RpcRequest::SessionStatus(crate::SessionStatusRequest {}),
            unix_time(),
        )
        .unwrap();
    let mut tampered: serde_json::Value = serde_json::from_str(&signed.event_json).unwrap();
    tampered["sig"] = "00".repeat(64).into();
    let response = http::post(&outgoing, "/peer-rpc", tampered.to_string(), None)
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    assert!(devices._db.snapshot().unwrap().is_empty());
    assert!(
        stream(&outgoing).await.is_err(),
        "invalid signature must not bind"
    );
    devices
        .da
        .reach
        .router
        .set(Devices::router_on(devices._a.path(), devices.ca.clone()))
        .unwrap();
    devices.da.reach.endpoint.set(a.clone()).unwrap();
    endpoint::admit(
        devices.da.clone(),
        Some(devices.cb.public_key().unwrap()),
        outgoing.clone(),
    );
    let client = crate::upstream_native::NativeClient::new_secure_candidates(
        Vec::new(),
        devices.cb.public_key().unwrap(),
        devices.ca.clone(),
    )
    .with_directory(devices.da.clone());
    let (first, concurrent) = tokio::join!(client.session_status(), client.session_status());
    first.unwrap();
    concurrent.unwrap();
    assert_eq!(
        devices._db.snapshot().unwrap()[0].device_public_key,
        devices.ca.public_key().unwrap()
    );
    assert_eq!(
        devices
            ._db
            .reach_connection(&devices.ca.public_key().unwrap())
            .await
            .unwrap()
            .stable_id(),
        devices
            ._db
            .reach
            .connection(&devices.ca.public_key().unwrap())
            .unwrap()
            .stable_id()
    );
    let reverse = crate::upstream_native::NativeClient::new_secure_candidates(
        Vec::new(),
        devices.ca.public_key().unwrap(),
        devices.cb.clone(),
    )
    .with_directory(devices._db.clone());
    reverse.session_status().await.unwrap();
    assert_eq!(devices.da.reach.live.read().unwrap().len(), 1);
    assert_eq!(devices._db.reach.live.read().unwrap().len(), 1);
    assert!(
        stream(&outgoing).await.is_ok(),
        "bound netplay channel must return its target acknowledgement"
    );
    a.close().await;
    b.close().await;
}
