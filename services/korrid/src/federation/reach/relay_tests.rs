use super::*;

#[test]
fn reach_relay_refuses_sessions_after_its_own_device_is_revoked() {
    let devices = Devices::new();
    let local = iroh::SecretKey::generate().public();
    let peer = iroh::SecretKey::generate().public();
    let access = relay::Membership {
        directory: devices.da.clone(),
        bindings: Arc::new(RwLock::new(BTreeMap::from([(
            peer,
            devices.cb.public_key().unwrap(),
        )]))),
        local,
    };
    assert!(access.allows(local));
    assert!(access.allows(peer));
    let key = devices.ca.public_key().unwrap();
    let now = unix_time() + 1;
    let revoked = EventBuilder::new(Kind::Custom(30_078), "")
        .tags([
            Tag::parse(["d", &format!("org.korri.device-owner:{key}")]).unwrap(),
            Tag::parse(["device", &key]).unwrap(),
            Tag::parse(["status", "revoked"]).unwrap(),
        ])
        .custom_created_at(Timestamp::from(now))
        .finalize(&devices.owner)
        .unwrap()
        .as_json();
    let identity = devices.ca.identity_snapshot().unwrap();
    let peer_identity = devices.cb.identity_snapshot().unwrap();
    let attempt = devices
        .da
        .authorization
        .attempt(
            identity.state(),
            peer_identity.device_public_key().unwrap(),
            peer_identity.owner_statement_json().as_deref(),
            None,
            &[revoked],
            now,
        )
        .unwrap();
    devices
        .da
        .authorization
        .commit_revocations(&attempt)
        .unwrap();
    assert!(devices.da.peer_is_revoked(&key).unwrap());
    assert!(!access.allows(local));
    assert!(!access.allows(peer));
}

#[tokio::test]
async fn reach_relay_uses_an_actual_fallback_port_and_unmetered_loss_stops_it() {
    let devices = Devices::new();
    let endpoint = endpoint().await;
    let access = relay::Membership {
        directory: devices.da.clone(),
        bindings: Arc::new(RwLock::new(BTreeMap::new())),
        local: endpoint.id(),
    };
    let held = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let preferred = held.local_addr().unwrap().port();
    let ips = ["127.0.0.1".parse().unwrap()];
    let mut fleet = relay::Fleet::new();
    fleet
        .update(
            policy::RelayFacts {
                external_power: true,
                unmetered: true,
            },
            &ips,
            &access,
            preferred,
        )
        .await;
    assert_eq!(fleet.addresses.len(), 1);
    assert_ne!(fleet.addresses[0].port(), preferred);
    let address = fleet.addresses[0];
    let start = tokio::time::Instant::now();
    fleet
        .update(
            policy::RelayFacts {
                external_power: true,
                unmetered: false,
            },
            &ips,
            &access,
            preferred,
        )
        .await;
    assert!(fleet.addresses.is_empty());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    assert!(start.elapsed() < Duration::from_secs(30));
    endpoint.close().await;
}

#[test]
fn reach_relay_addresses_are_bounded_and_never_loopback_or_tailscale() {
    let mut answer = signal::Answer {
        endpoint_id: iroh::SecretKey::generate().public().to_string(),
        direct_addresses: Vec::new(),
        expires_at: 160,
        request_id: "ab".repeat(32),
        can_relay_now: true,
        relay_addresses: vec!["192.168.1.2:443".parse().unwrap()],
    };
    assert!(answer.validate(100).is_ok());
    for address in [
        "127.0.0.1:443",
        "[::1]:443",
        "100.64.0.1:443",
        "[fd7a:115c:a1e0::1]:443",
        "192.168.1.2:0",
    ] {
        answer.relay_addresses = vec![address.parse().unwrap()];
        assert!(answer.validate(100).is_err());
    }
    answer.relay_addresses = vec!["192.168.1.2:443".parse().unwrap(); 9];
    assert!(answer.validate(100).is_err());
    answer.relay_addresses.truncate(1);
    answer.can_relay_now = false;
    assert!(answer.validate(100).is_err());
    answer.relay_addresses.clear();
    assert!(answer.validate(100).is_ok());
    let entry = |interface: &str, ip: &str, default: bool| {
        crate::federation::interfaces::InterfaceAddress {
            interface: interface.into(),
            address: ip.parse().unwrap(),
            has_default_route: default,
        }
    };
    assert_eq!(
        relay::listener_ips(&[
            entry("eth0", "192.168.1.2", true),
            entry("docker0", "172.17.0.1", false),
            entry("tailscale0", "100.64.0.1", false),
            entry("eth0", "2001:db8::1", true),
            entry("eth0", "fd00::1", true)
        ]),
        [
            "192.168.1.2".parse::<std::net::IpAddr>().unwrap(),
            "2001:db8::1".parse().unwrap()
        ]
    );
}

#[test]
fn reach_relay_admits_a_device_that_a_member_vouched_for() {
    let devices = Devices::new();
    let local = iroh::SecretKey::generate().public();
    let joined_endpoint = iroh::SecretKey::generate().public();
    let joined_root = tempfile::tempdir().unwrap();
    let joined = crate::identity::DeviceIdentity::load_or_create(joined_root.path()).unwrap();
    let joined_key = joined.device_public_key().unwrap().to_owned();
    let access = relay::Membership {
        directory: devices.da.clone(),
        bindings: Arc::new(RwLock::new(BTreeMap::from([(
            joined_endpoint,
            joined_key.clone(),
        )]))),
        local,
    };
    assert!(!access.allows(joined_endpoint));
    let vouch = devices
        .cb
        .identity_snapshot()
        .unwrap()
        .vouch_for(&joined_key, unix_time())
        .unwrap();
    devices
        .da
        .apply_membership(&devices.da.begin_work().unwrap(), &[vouch.owner_statement])
        .unwrap();
    assert!(access.allows(joined_endpoint));
}

#[test]
fn the_local_relay_stops_when_a_voucher_of_its_own_device_is_revoked() {
    let owner = Keys::generate();
    let voucher_root = tempfile::tempdir().unwrap();
    let voucher = owned(voucher_root.path(), &owner);
    let voucher_key = voucher.device_public_key().unwrap().to_owned();
    let root = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut local = crate::identity::DeviceIdentity::load_or_create(root.path()).unwrap();
    let local_key = local.device_public_key().unwrap().to_owned();
    local
        .apply_owner_statement(
            &voucher
                .vouch_for(&local_key, unix_time())
                .unwrap()
                .owner_statement,
        )
        .unwrap();
    let directory = FederationDirectory::open(
        root.path(),
        PeerCredentials::from_identity(local),
        Authorization::load(root.path()).unwrap(),
        Arc::new(unix_time),
    )
    .unwrap();
    let endpoint = iroh::SecretKey::generate().public();
    let access = relay::Membership {
        directory: directory.clone(),
        bindings: Arc::new(RwLock::new(BTreeMap::new())),
        local: endpoint,
    };
    assert!(access.allows(endpoint));
    let revoked = EventBuilder::new(Kind::Custom(30_078), "")
        .tags([
            Tag::parse(["d", &format!("org.korri.device-owner:{voucher_key}")]).unwrap(),
            Tag::parse(["device", &voucher_key]).unwrap(),
            Tag::parse(["status", "revoked"]).unwrap(),
        ])
        .custom_created_at(Timestamp::from(unix_time() + 1))
        .finalize(&owner)
        .unwrap()
        .as_json();
    directory
        .apply_membership(&directory.begin_work().unwrap(), &[revoked])
        .unwrap();
    assert!(!access.allows(endpoint));
}
