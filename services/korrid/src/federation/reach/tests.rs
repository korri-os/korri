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
    pub(super) _a: tempfile::TempDir,
    pub(super) b: tempfile::TempDir,
    pub(super) ca: PeerCredentials,
    pub(super) cb: PeerCredentials,
    pub(super) da: FederationDirectory,
    pub(super) _db: FederationDirectory,
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
    pub(super) fn router_on(root: &std::path::Path, credentials: PeerCredentials) -> axum::Router {
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
            responses: super::tunnel::responses(),
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
        super::tunnel::responses(),
        None,
    ));
    devices.da.reach.live.write().unwrap().insert(
        devices.cb.public_key().unwrap(),
        PeerConnection {
            connection: outgoing.clone(),
            responses: super::tunnel::responses(),
        },
    );
    devices.da.reach.endpoint.set(a.clone()).unwrap();
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
    // The exact launch's initial snapshot and terminal change travel on a
    // second stream of this same iroh connection, with no status polling.
    let prepared = client.prepare_stream("game", None).await.unwrap();
    let mut events = client
        .session_events(&outgoing, &prepared.session.launch_id)
        .await
        .unwrap();
    let initial = timeout(Duration::from_secs(5), events.next())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(initial, crate::upstream::UpstreamSessionStatus::SessionStatus { active: Some(active) } if active.launch_id == prepared.session.launch_id)
    );
    client
        .session_stop(&prepared.session.launch_id, false)
        .await
        .unwrap();
    let ended = timeout(Duration::from_secs(5), events.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        ended,
        crate::upstream::UpstreamSessionStatus::SessionStatus { active: None }
    ));
    let mut stale = client
        .session_events(&outgoing, "ffffffffffffffffffffffffffffffff")
        .await
        .unwrap();
    assert!(matches!(
        timeout(Duration::from_secs(5), stale.next())
            .await
            .unwrap()
            .unwrap(),
        crate::upstream::UpstreamSessionStatus::SessionStatus { active: None }
    ));
    let denied = http::post(
        &outgoing,
        "/rpc",
        "{\"_tag\":\"system.health\"}".into(),
        None,
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
async fn closed_connection_is_evicted_and_reported_once_without_background_redial() {
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
    tokio::task::yield_now().await;
    assert_eq!(directory.snapshot().unwrap()[0].updated_at, updated);
    assert!(directory.reach.live.read().unwrap().is_empty());
    a.close().await;
    b.close().await;
}

/// Test composition uses iroh's real in-memory address lookup, without public services.
pub(crate) async fn listen(
    directory: FederationDirectory,
    router: axum::Router,
) -> (Endpoint, tokio::task::JoinHandle<()>) {
    let (endpoint, task, _) = listen_counted(directory, router).await;
    (endpoint, task)
}
async fn listen_counted(
    directory: FederationDirectory,
    router: axum::Router,
) -> (
    Endpoint,
    tokio::task::JoinHandle<()>,
    tokio::sync::mpsc::UnboundedReceiver<usize>,
) {
    let (accepted, connections) = tokio::sync::mpsc::unbounded_channel();
    let endpoint = endpoint().await;
    directory.reach.router.set(router).unwrap();
    directory.reach.endpoint.set(endpoint.clone()).unwrap();
    let server = endpoint.clone();
    let task = tokio::spawn(async move {
        while let Some(incoming) = server.accept().await {
            let directory = directory.clone();
            let accepted = accepted.clone();
            tokio::spawn(async move {
                if let Ok(connection) = incoming.await {
                    let _ = accepted.send(connection.stable_id());
                    endpoint::admit(directory, None, connection);
                }
            });
        }
    });
    (endpoint, task, connections)
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
    let response = http::post(&outgoing, "/peer-rpc", tampered.to_string(), None, None)
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

struct RpcPeer {
    devices: Devices,
    local: Endpoint,
    remote: Endpoint,
    tasks: [tokio::task::JoinHandle<()>; 2],
    accepted: tokio::sync::mpsc::UnboundedReceiver<usize>,
    client: crate::upstream_native::NativeClient,
    device: String,
}
impl Drop for RpcPeer {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
impl RpcPeer {
    async fn new() -> Self {
        let devices = Devices::new();
        let (local, local_task) = listen(
            devices.da.clone(),
            Devices::router_on(devices._a.path(), devices.ca.clone()),
        )
        .await;
        let (remote, remote_task, accepted) =
            listen_counted(devices._db.clone(), devices.router()).await;
        learn(&devices.da, &remote);
        let device = devices.cb.public_key().unwrap();
        remember_endpoint(&devices, remote.id());
        let client = crate::upstream_native::NativeClient::new_secure_candidates(
            Vec::new(),
            device.clone(),
            devices.ca.clone(),
        )
        .with_directory(devices.da.clone());
        Self {
            devices,
            local,
            remote,
            tasks: [local_task, remote_task],
            accepted,
            client,
            device,
        }
    }
    async fn accepted(&mut self) {
        timeout(Duration::from_secs(5), self.accepted.recv())
            .await
            .unwrap()
            .unwrap();
    }
    async fn close(&self) {
        self.local.close().await;
        self.remote.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn later_peer_rpc_reconnects_after_a_closed_connection_is_evicted() {
    let mut peer = RpcPeer::new().await;
    peer.client.session_status().await.unwrap();
    peer.accepted().await;
    let old = peer
        .devices
        .da
        .reach_connection(&peer.device)
        .await
        .unwrap();
    peer.devices
        .da
        .reach
        .bindings
        .write()
        .unwrap()
        .replace(tunnel::Bindings {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            tcp: [0; 3],
            udp: [0; 5],
        });
    let old_stream = peer
        .devices
        .da
        .activate_stream(&peer.device)
        .await
        .unwrap()
        .unwrap();
    old.close(0u32.into(), b"test demand after close");
    timeout(Duration::from_secs(5), async {
        while peer
            .devices
            .da
            .reach
            .live
            .read()
            .unwrap()
            .contains_key(&peer.device)
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("closed connection must be evicted without another caller");
    assert!(
        timeout(Duration::from_millis(100), peer.accepted.recv())
            .await
            .is_err(),
        "closure alone must not cause a background dial"
    );
    assert!(peer.client.session_status_on(&old).await.is_err());
    assert!(
        peer.devices.da.reach.live.read().unwrap().is_empty(),
        "a session poll must not dial after closure"
    );
    assert!(matches!(
        peer.accepted.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    peer.client.session_status().await.unwrap();
    peer.accepted().await;
    let new = peer
        .devices
        .da
        .reach_connection(&peer.device)
        .await
        .unwrap();
    assert_ne!(old.stable_id(), new.stable_id());
    assert!(old_stream.connection_gone());
    assert!(
        old_stream.address().is_none(),
        "a new RPC must not recover the old stream"
    );
    let new_stream = peer
        .devices
        .da
        .activate_stream(&peer.device)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_stream.0.connection.stable_id(), new.stable_id());
    assert!(new_stream.address().is_some());
    assert!(matches!(
        peer.accepted.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    assert_eq!(
        peer.devices.da.snapshot().unwrap()[0].state,
        PeerState::Ready
    );
    peer.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_peer_rpcs_after_close_share_one_new_dial() {
    let mut peer = RpcPeer::new().await;
    peer.client.session_status().await.unwrap();
    peer.accepted().await;
    let old = peer
        .devices
        .da
        .reach_connection(&peer.device)
        .await
        .unwrap();
    old.close(0u32.into(), b"test concurrent demand after close");
    let barrier = Arc::new(tokio::sync::Barrier::new(9));
    let mut callers = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let (client, directory, device, barrier) = (
            peer.client.clone(),
            peer.devices.da.clone(),
            peer.device.clone(),
            barrier.clone(),
        );
        callers.spawn(async move {
            barrier.wait().await;
            client.session_status().await.unwrap();
            directory
                .reach_connection(&device)
                .await
                .unwrap()
                .stable_id()
        });
    }
    barrier.wait().await;
    let mut connections = std::collections::BTreeSet::new();
    while let Some(result) = callers.join_next().await {
        connections.insert(result.unwrap());
    }
    assert_eq!(connections.len(), 1);
    assert!(!connections.contains(&old.stable_id()));
    peer.accepted().await;
    assert!(
        matches!(
            peer.accepted.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ),
        "concurrent callers must cause only one new accepted connection"
    );
    assert_eq!(peer.devices.da.reach.live.read().unwrap().len(), 1);
    assert_eq!(
        peer.devices.da.snapshot().unwrap()[0].state,
        PeerState::Ready
    );
    peer.close().await;
}

#[tokio::test]
async fn old_close_observer_does_not_evict_or_fail_a_new_connection() {
    let devices = Devices::new();
    let (a, b, old, _old_incoming) = pair().await;
    let (c, d, new, _new_incoming) = pair().await;
    let directory = devices.da.clone();
    let device = devices.cb.public_key().unwrap();
    directory.reach.router.set(axum::Router::new()).unwrap();
    let old_observer =
        endpoint::admit(directory.clone(), Some(device.clone()), old.clone()).unwrap();
    // On the current-thread runtime, neither observer runs before replacement.
    old.close(0u32.into(), b"test late close observer");
    endpoint::admit(directory.clone(), Some(device.clone()), new.clone());
    directory
        .set_peer_state(
            &directory.begin_peer_work(&device).unwrap(),
            &device,
            PeerState::Ready,
        )
        .unwrap();
    timeout(Duration::from_secs(5), old_observer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        directory
            .reach_connection(&device)
            .await
            .unwrap()
            .stable_id(),
        new.stable_id()
    );
    assert_eq!(directory.snapshot().unwrap()[0].state, PeerState::Ready);
    a.close().await;
    b.close().await;
    c.close().await;
    d.close().await;
}

#[tokio::test]
async fn a_replayed_signed_rpc_from_another_endpoint_does_not_bind() {
    let devices = Devices::new();
    let (attacker, receiver, outgoing, incoming) = pair().await;
    let rightful = endpoint().await;
    devices.da.reach.endpoint.set(rightful.clone()).unwrap();
    devices._db.reach.router.set(devices.router()).unwrap();
    devices._db.reach.endpoint.set(receiver.clone()).unwrap();
    endpoint::admit(devices._db.clone(), None, incoming);
    let stolen = devices
        .ca
        .encode_request(
            &devices.cb.public_key().unwrap(),
            crate::RpcRequest::SessionStatus(crate::SessionStatusRequest {}),
            unix_time(),
        )
        .unwrap();
    let proof = devices
        .da
        .own_endpoint_event(&devices.cb.public_key().unwrap())
        .unwrap();
    // Neither a captured RPC by itself nor its device's captured endpoint record
    // proves ownership of the attacker's different transport key.
    for event in [None, Some(proof.as_str())] {
        let response = http::post(
            &outgoing,
            "/peer-rpc",
            stolen.event_json.clone(),
            None,
            event,
        )
        .await
        .unwrap();
        assert!(
            !response.status().is_success(),
            "proof must name the authenticated QUIC EndpointId"
        );
        assert!(!devices
            ._db
            .has_peer_connection(&devices.ca.public_key().unwrap()));
        let (mut send, mut recv) = outgoing.open_bi().await.unwrap();
        send.write_all(&[2]).await.unwrap();
        use tokio::io::AsyncReadExt;
        assert!(
            recv.read_u8().await.is_err(),
            "replayed RPC must not open netplay"
        );
    }
    rightful
        .address_lookup()
        .unwrap()
        .add(iroh::address_lookup::memory::MemoryLookup::from_endpoint_info([receiver.addr()]));
    let (legitimate, accepted) = tokio::join!(rightful.connect(receiver.id(), ALPN), async {
        receiver.accept().await.unwrap().await.unwrap()
    });
    let legitimate = legitimate.unwrap();
    endpoint::admit(devices._db.clone(), None, accepted);
    // Failed channel proof must not consume the legitimate request's nonce.
    let response = http::post(
        &legitimate,
        "/peer-rpc",
        stolen.event_json,
        None,
        Some(&proof),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());
    assert_eq!(
        devices
            ._db
            .reach_connection(&devices.ca.public_key().unwrap())
            .await
            .unwrap()
            .remote_id(),
        rightful.id()
    );
    let (mut send, mut recv) = legitimate.open_bi().await.unwrap();
    send.write_all(&[2]).await.unwrap();
    use tokio::io::AsyncReadExt;
    assert!(
        recv.read_u8().await.is_ok(),
        "matching proof must enable netplay"
    );
    attacker.close().await;
    rightful.close().await;
    receiver.close().await;
}

#[tokio::test]
async fn an_idle_unbound_connection_closes_after_its_proof_deadline() {
    let devices = Devices::new();
    let (receiver, task) = listen(devices._db.clone(), devices.router()).await;
    let caller = endpoint().await;
    caller
        .address_lookup()
        .unwrap()
        .add(iroh::address_lookup::memory::MemoryLookup::from_endpoint_info([receiver.addr()]));
    let connection = caller.connect(receiver.id(), ALPN).await.unwrap();
    timeout(Duration::from_secs(11), connection.closed())
        .await
        .expect("unbound connection must have a proof deadline");
    task.abort();
    caller.close().await;
    receiver.close().await;
}

/// Compose real signed discovery with an isolated iroh address lookup.
pub(crate) async fn remember_peer(directory: &FederationDirectory, peer: &FederationDirectory) {
    async fn ready(directory: &FederationDirectory) -> Endpoint {
        timeout(Duration::from_secs(5), async {
            loop {
                if let Some(endpoint) = directory.reach.endpoint.get() {
                    break endpoint.clone();
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap()
    }
    ready(directory).await;
    let endpoint = ready(peer).await;
    learn(directory, &endpoint);
    let identity = peer.credentials.identity_snapshot().unwrap();
    directory
        .apply_membership(
            &directory.begin_work().unwrap(),
            &[identity.owner_statement_json().unwrap()],
        )
        .unwrap();
    let event = peer
        .own_endpoint_event(&directory.credentials.public_key().unwrap())
        .unwrap();
    directory
        .apply_endpoint_event(&directory.begin_work().unwrap(), &event)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sixty_five_idle_unbound_connections_do_not_block_a_member() {
    let devices = Devices::new();
    let (member, member_task) = listen(
        devices.da.clone(),
        Devices::router_on(devices._a.path(), devices.ca.clone()),
    )
    .await;
    // Exercise the production accept loop, not only the admission helper.
    let server = endpoint::start(devices.b.path(), devices._db.clone(), devices.router()).unwrap();
    remember_peer(&devices.da, &devices._db).await;
    remember_peer(&devices._db, &devices.da).await;
    let receiver = devices._db.reach.endpoint.get().unwrap().clone();
    let mut idle = Vec::new();
    for _ in 0..65 {
        let caller = endpoint().await;
        caller
            .address_lookup()
            .unwrap()
            .add(iroh::address_lookup::memory::MemoryLookup::from_endpoint_info([receiver.addr()]));
        let connection = caller.connect(receiver.id(), ALPN).await;
        idle.push((caller, connection));
    }
    assert_eq!(devices._db.reach.admission.unbound.available_permits(), 0);
    let client = crate::upstream_native::NativeClient::new_secure_candidates(
        Vec::new(),
        devices.cb.public_key().unwrap(),
        devices.ca.clone(),
    )
    .with_directory(devices.da.clone());
    timeout(Duration::from_secs(5), client.session_status())
        .await
        .expect("idle strangers must not occupy member slots")
        .unwrap();
    assert!(devices
        ._db
        .has_peer_connection(&devices.ca.public_key().unwrap()));
    for (caller, _) in idle {
        caller.close().await;
    }
    member_task.abort();
    member.close().await;
    drop(server);
}

/// Device a's directory remembers device b's signed endpoint record naming `id`.
fn remember_endpoint(devices: &Devices, id: iroh::EndpointId) {
    let device = devices.cb.public_key().unwrap();
    let recipient = devices.ca.public_key().unwrap();
    let record = crate::relay::EndpointRecord {
        device_public_key: device,
        owner_public_key: devices.da.snapshot().unwrap()[0]
            .membership
            .owner_public_key
            .clone(),
        generation: 1,
        issued_at: unix_time(),
        endpoint_id: id.to_string(),
    };
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
            ],
            &serde_json::to_string(&record).unwrap(),
            record.issued_at,
        )
        .unwrap();
    devices
        .da
        .apply_endpoint_event(&devices.da.begin_work().unwrap(), &event.json)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_peer_call_does_not_wait_past_its_deadline_for_a_dial_with_no_answer() {
    let devices = Devices::new();
    let (local, local_task) = listen(
        devices.da.clone(),
        Devices::router_on(devices._a.path(), devices.ca.clone()),
    )
    .await;
    // The peer's endpoint is known, but nothing answers at its address: an
    // offline device whose record is still remembered.
    let gone = endpoint().await;
    let gone_id = gone.id();
    gone.close().await;
    let silent = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    local.address_lookup().unwrap().add(
        iroh::address_lookup::memory::MemoryLookup::from_endpoint_info([iroh::EndpointAddr::new(
            gone_id,
        )
        .with_ip_addr(silent.local_addr().unwrap())]),
    );
    remember_endpoint(&devices, gone_id);
    let device = devices.cb.public_key().unwrap();
    let client = crate::upstream_native::NativeClient::new_secure_candidates(
        Vec::new(),
        device.clone(),
        devices.ca.clone(),
    )
    .with_directory(devices.da.clone())
    .with_timeout(Duration::from_millis(500));
    let started = std::time::Instant::now();
    let result = timeout(Duration::from_secs(20), client.session_status())
        .await
        .expect("the call must end on its own deadline");
    assert!(
        matches!(result, Err(crate::upstreams::UpstreamError::Unreachable(_))),
        "{result:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the dial held the call for {:?}",
        started.elapsed()
    );
    assert!(matches!(
        devices.da.snapshot().unwrap()[0].state,
        PeerState::Failed { .. }
    ));
    local_task.abort();
    local.close().await;
    drop(silent);
}
