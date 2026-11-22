use super::*;
use crate::{
    authorization::Authorization,
    federation::reach::{
        endpoint,
        tests::{learn, Devices},
    },
    peer_rpc::{unix_time, PeerCredentials},
};
use std::{net::SocketAddr, time::Duration};
use tokio::time::timeout;

fn builder(address: SocketAddr) -> iroh::endpoint::Builder {
    iroh::endpoint::Builder::empty()
        .crypto_provider(iroh::tls::default_provider())
        .relay_mode(iroh::RelayMode::Disabled)
        .portmapper_config(iroh::endpoint::PortmapperConfig::Disabled)
        .clear_ip_transports()
        .bind_addr(address)
        .unwrap()
}
async fn ready(directory: &FederationDirectory) -> Endpoint {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Some(endpoint) = directory.reach.endpoint.get() {
                return endpoint.clone();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}
fn client(
    directory: &FederationDirectory,
    credentials: PeerCredentials,
    peer: &PeerCredentials,
) -> crate::upstream_native::NativeClient {
    crate::upstream_native::NativeClient::new_secure_candidates(
        Vec::new(),
        peer.public_key().unwrap(),
        credentials,
    )
    .with_directory(directory.clone())
}
async fn reopened(root: &Path, credentials: PeerCredentials) -> FederationDirectory {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(directory) = FederationDirectory::open(
                root,
                credentials.clone(),
                Authorization::load(root).unwrap(),
                Arc::new(unix_time),
            ) {
                return directory;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("closed connection observers must release the old directory")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restarted_endpoints_dial_by_key_using_only_observed_disk_addresses() {
    let Devices {
        _a: a_root,
        b: b_root,
        ca,
        cb,
        da,
        _db: db,
    } = Devices::new();
    fs::write(b_root.path().join(FILE), b"{broken").unwrap();
    fs::set_permissions(b_root.path().join(FILE), fs::Permissions::from_mode(0o600)).unwrap();
    let a_life = endpoint::start_with_builder(
        a_root.path(),
        da.clone(),
        Devices::router_on(a_root.path(), ca.clone()),
        builder("127.0.0.1:0".parse().unwrap()),
    )
    .unwrap();
    let b_life = endpoint::start_with_builder(
        b_root.path(),
        db.clone(),
        Devices::router_on(b_root.path(), cb.clone()),
        builder("127.0.0.1:0".parse().unwrap()),
    )
    .unwrap();
    let a = ready(&da).await;
    let b = ready(&db).await;
    // Seed discovery only for the first boot. Cache writes must come from the
    // authenticated live connection, not from this fixture's lookup seed.
    learn(&da, &b);
    let binding = db.own_endpoint_event(&ca.public_key().unwrap()).unwrap();
    da.apply_endpoint_event(&da.begin_work().unwrap(), &binding)
        .unwrap();
    timeout(
        Duration::from_secs(5),
        client(&da, ca.clone(), &cb).session_status(),
    )
    .await
    .unwrap()
    .unwrap();
    timeout(Duration::from_secs(5), async {
        loop {
            let a_cached = read(&a_root.path().join(FILE)).unwrap_or_default();
            let b_cached = read(&b_root.path().join(FILE)).unwrap_or_default();
            if a_cached
                .iter()
                .any(|entry| entry.id == b.id() && !entry.is_empty())
                && b_cached
                    .iter()
                    .any(|entry| entry.id == a.id() && !entry.is_empty())
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("both sides must persist iroh's observed addresses");
    let (a_id, b_id) = (a.id(), b.id());
    let (a_bind, b_bind) = (
        *a.addr().ip_addrs().next().unwrap(),
        *b.addr().ip_addrs().next().unwrap(),
    );
    a.close().await;
    b.close().await;
    drop((a_life, b_life, a, b, da, db));
    let da = reopened(a_root.path(), ca.clone()).await;
    let db = reopened(b_root.path(), cb.clone()).await;
    // Both endpoints and their in-memory providers are new. No n0 services,
    // mDNS, external lookup seed or explicit address is supplied on this boot.
    let a_life = endpoint::start_with_builder(
        a_root.path(),
        da.clone(),
        Devices::router_on(a_root.path(), ca.clone()),
        builder(a_bind),
    )
    .unwrap();
    let b_life = endpoint::start_with_builder(
        b_root.path(),
        db.clone(),
        Devices::router_on(b_root.path(), cb.clone()),
        builder(b_bind),
    )
    .unwrap();
    let a = ready(&da).await;
    let b = ready(&db).await;
    assert_eq!((a.id(), b.id()), (a_id, b_id));
    timeout(
        Duration::from_secs(5),
        client(&da, ca.clone(), &cb).session_status(),
    )
    .await
    .unwrap()
    .expect("cached addresses must support a fresh authenticated key-only dial");
    a.close().await;
    b.close().await;
    // Test the reverse direction as a new connection, not the accepted one.
    drop((a_life, b_life, a, b, da, db));
    let da = reopened(a_root.path(), ca.clone()).await;
    let db = reopened(b_root.path(), cb.clone()).await;
    let a_life = endpoint::start_with_builder(
        a_root.path(),
        da.clone(),
        Devices::router_on(a_root.path(), ca.clone()),
        builder(a_bind),
    )
    .unwrap();
    let b_life = endpoint::start_with_builder(
        b_root.path(),
        db.clone(),
        Devices::router_on(b_root.path(), cb.clone()),
        builder(b_bind),
    )
    .unwrap();
    let a = ready(&da).await;
    let b = ready(&db).await;
    timeout(
        Duration::from_secs(5),
        client(&db, cb, &ca).session_status(),
    )
    .await
    .unwrap()
    .expect("the accepted side must also retain usable addresses across restart");
    a.close().await;
    b.close().await;
    drop((a_life, b_life));
}

#[tokio::test]
async fn corrupt_cache_is_ignored_and_rebuilt_without_affecting_authority() {
    let devices = Devices::new();
    let path = devices._a.path().join(FILE);
    fs::write(&path, b"{broken").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let authority = fs::read(devices._a.path().join("federation/peers.json")).unwrap();
    let lookup = devices.da.start_address_cache();
    assert!(lookup
        .get_endpoint_info(iroh::SecretKey::generate().public())
        .is_none());
    assert_eq!(read(&path).unwrap(), Vec::<EndpointAddr>::new());
    assert_eq!(
        fs::read(devices._a.path().join("federation/peers.json")).unwrap(),
        authority
    );
    let life = endpoint::start_with_builder(
        devices._a.path(),
        devices.da.clone(),
        Devices::router_on(devices._a.path(), devices.ca.clone()),
        builder("127.0.0.1:0".parse().unwrap()),
    )
    .unwrap();
    let endpoint = ready(&devices.da).await;
    assert!(
        !endpoint.is_closed(),
        "bad hints must not prevent endpoint startup"
    );
    endpoint.close().await;
    drop(life);
}

#[test]
fn cache_bounds_and_roster_removal_apply_to_disk_and_lookup() {
    let devices = Devices::new();
    let id = iroh::SecretKey::generate().public();
    let other = iroh::SecretKey::generate().public();
    let allowed = BTreeSet::from([id]);
    let cache = Cache::load(devices._a.path(), &allowed);
    let addr = EndpointAddr::from_parts(
        id,
        (1..=16).map(|port| iroh::TransportAddr::Ip(([127, 0, 0, 1], port).into())),
    );
    let foreign = EndpointAddr::new(other).with_ip_addr("127.0.0.1:1234".parse().unwrap());
    cache.update(&allowed, Some(addr.clone())).unwrap();
    cache.update(&allowed, Some(foreign)).unwrap();
    assert_eq!(read(&cache.path).unwrap(), vec![addr]);
    assert!(cache.lookup.get_endpoint_info(other).is_none());
    cache.update(&BTreeSet::new(), None).unwrap();
    assert!(read(&cache.path).unwrap().is_empty());
    assert!(cache.lookup.get_endpoint_info(id).is_none());
    let too_many = EndpointAddr::from_parts(
        id,
        (1..=17).map(|port| iroh::TransportAddr::Ip(([127, 0, 0, 1], port).into())),
    );
    write(&cache.path, &[&too_many]).unwrap();
    let reloaded = Cache::load(devices._a.path(), &allowed);
    assert!(reloaded.lookup.get_endpoint_info(id).is_none());
}
