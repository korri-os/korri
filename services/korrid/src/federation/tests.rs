use super::*;
#[path = "additional_tests.rs"]
mod additional;
#[path = "coordinator_effect_tests.rs"]
mod coordinator_effect_tests;
#[path = "coordinator_io_tests.rs"]
mod coordinator_io_tests;
#[path = "coordinator_tests.rs"]
mod coordinator_tests;
#[path = "routing_tests.rs"]
mod routing;
use crate::relay::ENDPOINT_EVENT_KIND;
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::Keys,
    types::Timestamp,
};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Barrier;

fn binding(owner: &Keys, device: &str, status: &str, at: u64) -> String {
    EventBuilder::new(Kind::Custom(30_078), "")
        .tags([
            Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
            Tag::parse(["device", device]).unwrap(),
            Tag::parse(["status", status]).unwrap(),
        ])
        .custom_created_at(Timestamp::from(at))
        .finalize(owner)
        .unwrap()
        .as_json()
}

struct Fixture {
    root: tempfile::TempDir,
    _peer_root: tempfile::TempDir,
    owner: Keys,
    credentials: PeerCredentials,
    peer: DeviceIdentity,
    clock: Arc<AtomicU64>,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let peer_root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let owner = Keys::generate();
        let mut local = DeviceIdentity::load_or_create(root.path()).unwrap();
        local
            .apply_owner_statement(&binding(
                &owner,
                local.device_public_key().unwrap(),
                "owned",
                10,
            ))
            .unwrap();
        let mut peer = DeviceIdentity::load_or_create(peer_root.path()).unwrap();
        peer.apply_owner_statement(&binding(
            &owner,
            peer.device_public_key().unwrap(),
            "owned",
            10,
        ))
        .unwrap();
        Self {
            root,
            _peer_root: peer_root,
            owner,
            credentials: PeerCredentials::from_identity(local),
            peer,
            clock: Arc::new(AtomicU64::new(1000)),
        }
    }
    fn directory(&self) -> Result<FederationDirectory, FederationError> {
        let clock = self.clock.clone();
        FederationDirectory::open(
            self.root.path(),
            self.credentials.clone(),
            Authorization::load(self.root.path()).unwrap(),
            Arc::new(move || clock.load(Ordering::SeqCst)),
        )
    }
    fn key(&self) -> String {
        self.peer.device_public_key().unwrap().into()
    }
    fn roster(&self, directory: &FederationDirectory) {
        directory
            .apply_membership(
                &directory.begin_work().unwrap(),
                &[self.peer.owner_statement_json().unwrap()],
            )
            .unwrap();
    }
    fn endpoint(&self, generation: u64, at: u64) -> EndpointRecord {
        EndpointRecord {
            device_public_key: self.key(),
            owner_public_key: self.owner.public_key().to_hex(),
            generation,
            candidates: vec!["http://192.168.1.4:43117".into()],
            issued_at: at,
            expires_at: at + 600,
            label: Some("Living room".into()),
            moonlight_address: Some("192.168.1.4:47989".into()),
        }
    }
    fn event(&self, endpoint: &EndpointRecord) -> String {
        let recipient = self.credentials.public_key().unwrap();
        self.peer
            .encrypt_tagged_event(
                &recipient,
                ENDPOINT_EVENT_KIND,
                vec![
                    vec!["d".into(), format!("org.korri.endpoint:{recipient}")],
                    vec!["p".into(), recipient.clone()],
                    vec!["expiration".into(), endpoint.expires_at.to_string()],
                ],
                &serde_json::to_string(endpoint).unwrap(),
                endpoint.issued_at,
            )
            .unwrap()
            .json
    }
}

#[test]
fn memory_missing_is_only_initialization_and_private() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    assert!(d.snapshot().unwrap().is_empty());
    for (path, mode) in [
        (f.root.path().join("federation"), 0o700),
        (f.root.path().join("federation/peers.json"), 0o600),
    ] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            mode
        );
    }
    fs::remove_file(f.root.path().join("federation/peers.json")).unwrap();
    assert!(d.reserve_publication(&d.begin_work().unwrap()).is_err());
    assert!(f.directory().is_err());
}

#[test]
fn memory_rejects_corrupt_oversized_modes_links_and_special_files() {
    for kind in [
        "corrupt",
        "oversized",
        "mode",
        "link",
        "directory",
        "socket",
        "hardlink",
        "dir-mode",
        "dir-link",
    ] {
        let f = Fixture::new();
        f.directory().unwrap();
        let path = f.root.path().join("federation/peers.json");
        match kind {
            "corrupt" => fs::write(&path, b"{").unwrap(),
            "oversized" => fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len((MAX_MEMORY_BYTES + 1) as u64)
                .unwrap(),
            "mode" => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
            "link" => {
                fs::remove_file(&path).unwrap();
                symlink("absent", &path).unwrap();
            }
            "directory" => {
                fs::remove_file(&path).unwrap();
                fs::create_dir(&path).unwrap();
            }
            "socket" => {
                fs::remove_file(&path).unwrap();
                let _socket = std::os::unix::net::UnixListener::bind(&path).unwrap();
            }
            "hardlink" => fs::hard_link(&path, f.root.path().join("copy")).unwrap(),
            "dir-mode" => {
                fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o755))
                    .unwrap()
            }
            "dir-link" => {
                fs::rename(path.parent().unwrap(), f.root.path().join("moved")).unwrap();
                symlink("moved", path.parent().unwrap()).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(f.directory().is_err(), "{kind}");
    }
}

#[test]
fn membership_endpoint_expiry_restart_and_timestamp_high_water() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let endpoint = f.endpoint(7, 1000);
    d.apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&endpoint))
        .unwrap();
    f.clock.store(1700, Ordering::SeqCst);
    let p = d.snapshot().unwrap().remove(0);
    assert!(p.current_endpoint.is_none());
    assert_eq!(p.remembered_endpoint, Some(endpoint.clone()));
    assert_eq!((p.first_seen, p.last_seen), (1000, 1000));
    drop(d);
    let d = f.directory().unwrap();
    let stale = f.endpoint(6, 1700);
    assert!(!d
        .apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&stale))
        .unwrap());
    assert_eq!(d.snapshot().unwrap()[0].remembered_endpoint, Some(endpoint));
    let fresh = f.endpoint(8, 1700);
    assert!(d
        .apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&fresh))
        .unwrap());
    f.clock.store(1600, Ordering::SeqCst);
    f.roster(&d);
    let p = d.snapshot().unwrap().remove(0);
    assert_eq!((p.first_seen, p.last_seen), (1000, 1700));
    // The earlier snapshots cached the old endpoint; a new signed event
    // must replace it, not be shadowed by the cache.
    assert_eq!(p.remembered_endpoint, Some(fresh));
}

#[test]
fn revocation_survives_deletion_restart_and_stale_in_flight_work() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let old = d.begin_work().unwrap();
    let revoked = binding(&f.owner, &f.key(), "revoked", 11);
    d.apply_membership(&old, &[revoked]).unwrap();
    assert!(d.snapshot().unwrap().is_empty());
    assert!(d
        .apply_endpoint_event(&old, &f.event(&f.endpoint(1, 1000)))
        .is_err());
    assert!(d.set_peer_state(&old, &f.key(), PeerState::Ready).is_err());
    drop(d);
    let d = f.directory().unwrap();
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[binding(&f.owner, &f.key(), "owned", 999)],
    )
    .unwrap();
    assert!(d.snapshot().unwrap().is_empty());
    assert_eq!(
        fs::read_dir(f.root.path().join("identity/authorization-revocations"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn membership_reverifies_sources_and_strictly_binds_private_identity() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    let other = Keys::generate();
    assert!(d
        .apply_membership(
            &d.begin_work().unwrap(),
            &[binding(&other, &f.key(), "owned", 20)]
        )
        .is_err());
    assert!(d
        .apply_membership(
            &d.begin_work().unwrap(),
            &[f.credentials
                .identity_snapshot()
                .unwrap()
                .owner_statement_json()
                .unwrap()]
        )
        .is_err());
    assert!(d
        .apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&f.endpoint(1, 1000)))
        .is_err());
    f.roster(&d);
    let mut forged = f.endpoint(1, 1000);
    forged.owner_public_key = other.public_key().to_hex();
    assert!(d
        .apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&forged))
        .is_err());
    drop(d);
    let path = f.root.path().join("federation/peers.json");
    let original = fs::read(&path).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&original).unwrap();
    json["peers"][f.key()]["ownerStatement"] =
        serde_json::Value::String(binding(&other, &f.key(), "owned", 20));
    fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(f.directory().is_err());
    fs::write(&path, original).unwrap();
    DeviceIdentity::reset(f.root.path()).unwrap();
    assert!(f.credentials.reload_identity(f.root.path()).is_err());
    let credentials = PeerCredentials::load(f.root.path()).unwrap();
    assert!(FederationDirectory::open(
        f.root.path(),
        credentials,
        Authorization::load(f.root.path()).unwrap(),
        Arc::new(|| 1000)
    )
    .is_err());
}

#[test]
fn generation_and_nip_timestamp_are_serialized_persisted_and_clock_bounded() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    let handles: Vec<_> = (0..20)
        .map(|_| {
            let d = d.clone();
            std::thread::spawn(move || d.reserve_publication(&d.begin_work().unwrap()).unwrap())
        })
        .collect();
    let mut reservations: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    reservations.sort_by_key(|r| r.generation);
    for (i, r) in reservations.iter().enumerate() {
        assert_eq!(
            (r.generation, r.created_at),
            (i as u64 + 1, 1000 + i as u64)
        );
    }
    drop(d);
    f.clock.store(900, Ordering::SeqCst);
    let d = f.directory().unwrap();
    let next = d.reserve_publication(&d.begin_work().unwrap()).unwrap();
    assert_eq!((next.generation, next.created_at), (21, 1020));
    f.clock.store(1, Ordering::SeqCst);
    assert!(d.reserve_publication(&d.begin_work().unwrap()).is_err());
    f.clock.store(2000, Ordering::SeqCst);
    assert_eq!(
        d.reserve_publication(&d.begin_work().unwrap())
            .unwrap()
            .generation,
        22
    );
}

#[test]
fn identity_switch_reset_replaces_populated_memory_and_allows_only_explicit_rebinding() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    d.apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&f.endpoint(1, 1000)))
        .unwrap();
    d.reserve_publication(&d.begin_work().unwrap()).unwrap();

    let new_device = Keys::generate().public_key().to_hex();
    let new_owner = Keys::generate().public_key().to_hex();
    d.reset_after_identity_switch(&new_device, &new_owner)
        .unwrap();
    let path = f.root.path().join("federation/peers.json");
    let document: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(document["localDevicePublicKey"], new_device);
    assert_eq!(document["ownerPublicKey"], new_owner);
    assert!(document["publication"].is_null());
    assert_eq!(document["peers"], serde_json::json!({}));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    d.reset_after_identity_switch(&new_device, &new_owner)
        .unwrap();
    let retried = fs::read(&path).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&retried).unwrap(),
        document
    );

    let replacement_device = Keys::generate().public_key().to_hex();
    let replacement_owner = Keys::generate().public_key().to_hex();
    d.reset_after_identity_switch(&replacement_device, &replacement_owner)
        .unwrap();
    let rebound: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(rebound["localDevicePublicKey"], replacement_device);
    assert_eq!(rebound["ownerPublicKey"], replacement_owner);
    assert!(rebound["publication"].is_null());
    assert_eq!(rebound["peers"], serde_json::json!({}));
    assert!(
        d.begin_work().is_err(),
        "ordinary work keeps the old identity strict"
    );
}

#[test]
fn identity_switch_reset_serializes_against_concurrent_old_memory_mutation() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let token = d.begin_work().unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mutation = {
        let directory = d.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            let _ = directory.reserve_publication(&token);
        })
    };
    let new_device = Keys::generate().public_key().to_hex();
    let new_owner = Keys::generate().public_key().to_hex();
    let reset = {
        let directory = d.clone();
        let barrier = barrier.clone();
        let new_device = new_device.clone();
        let new_owner = new_owner.clone();
        std::thread::spawn(move || {
            barrier.wait();
            directory
                .reset_after_identity_switch(&new_device, &new_owner)
                .unwrap();
        })
    };
    barrier.wait();
    mutation.join().unwrap();
    reset.join().unwrap();

    let document: serde_json::Value =
        serde_json::from_slice(&fs::read(f.root.path().join("federation/peers.json")).unwrap())
            .unwrap();
    assert_eq!(document["localDevicePublicKey"], new_device);
    assert_eq!(document["ownerPublicKey"], new_owner);
    assert!(document["publication"].is_null());
    assert_eq!(document["peers"], serde_json::json!({}));
}

#[test]
fn identity_switch_reset_rejects_invalid_keys_and_unsafe_storage_without_writing() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    d.reserve_publication(&d.begin_work().unwrap()).unwrap();
    let path = f.root.path().join("federation/peers.json");
    let before = fs::read(&path).unwrap();
    let valid = Keys::generate().public_key().to_hex();
    let uppercase = "A".repeat(64);
    for (device, owner) in [
        ("not-a-key", valid.as_str()),
        (valid.as_str(), uppercase.as_str()),
        (valid.as_str(), valid.as_str()),
    ] {
        assert!(d.reset_after_identity_switch(device, owner).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let replacement = Keys::generate().public_key().to_hex();
    assert!(d.reset_after_identity_switch(&valid, &replacement).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);

    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let directory = path.parent().unwrap();
    let moved = f.root.path().join("federation-moved");
    fs::rename(directory, &moved).unwrap();
    symlink("federation-moved", directory).unwrap();
    assert!(d.reset_after_identity_switch(&valid, &replacement).is_err());
    assert_eq!(fs::read(moved.join("peers.json")).unwrap(), before);
}

#[test]
fn peer_states_and_configuration_epoch_reject_stale_completions() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    assert_eq!(d.snapshot().unwrap()[0].state, PeerState::Loading);
    for state in [
        PeerState::Ready,
        PeerState::Failed {
            error: "unreachable".into(),
        },
        PeerState::Ready,
    ] {
        d.set_peer_state(
            &d.begin_peer_work(&f.key()).unwrap(),
            &f.key(),
            state.clone(),
        )
        .unwrap();
        assert_eq!(d.snapshot().unwrap()[0].state, state);
    }
    let old = d.begin_work().unwrap();
    d.invalidate_work().unwrap();
    assert!(d.set_peer_state(&old, &f.key(), PeerState::Ready).is_err());
    assert!(d
        .apply_membership(&old, &[f.peer.owner_statement_json().unwrap()])
        .is_err());
}

#[test]
fn equal_generation_uses_issued_at_and_equal_records_do_not_replace() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let first = f.endpoint(2, 1000);
    assert!(d
        .apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&first))
        .unwrap());
    let mut conflict = first.clone();
    conflict.label = Some("conflict".into());
    assert!(!d
        .apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&conflict))
        .unwrap());
    conflict.issued_at += 1;
    assert!(d
        .apply_endpoint_event(&d.begin_work().unwrap(), &f.event(&conflict))
        .unwrap());
}

#[test]
fn publication_overflow_and_disk_rollback_fail_without_success() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    let path = f.root.path().join("federation/peers.json");
    let before = fs::read(&path).unwrap();
    d.reserve_publication(&d.begin_work().unwrap()).unwrap();
    fs::write(&path, &before).unwrap();
    assert!(d.reserve_publication(&d.begin_work().unwrap()).is_err());
    drop(d);
    let mut json: serde_json::Value = serde_json::from_slice(&before).unwrap();
    json["publication"] = serde_json::json!({"generation": u64::MAX, "createdAt": 1000});
    fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    let d = f.directory().unwrap();
    assert!(d.reserve_publication(&d.begin_work().unwrap()).is_err());
}

#[test]
fn optional_endpoint_metadata_is_bounded_and_empty_endpoints_are_invalid() {
    let f = Fixture::new();
    let mut endpoint = f.endpoint(1, 1000);
    endpoint.label = Some("x".repeat(257));
    assert!(endpoint.validate(1000).is_err());
    endpoint.label = None;
    endpoint.moonlight_address = Some("bad\naddress".into());
    assert!(endpoint.validate(1000).is_err());
    endpoint.moonlight_address = None;
    endpoint.candidates.clear();
    assert!(endpoint.validate(1000).is_err());
}

#[path = "stream_play_label_tests.rs"]
mod stream_play_labels;

#[test]
fn a_device_that_a_peer_vouched_for_joins_the_roster_until_the_voucher_is_revoked() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    let joined_root = tempfile::tempdir().unwrap();
    let joined = DeviceIdentity::load_or_create(joined_root.path()).unwrap();
    let joined_key = joined.device_public_key().unwrap().to_owned();
    let vouch = f.peer.vouch_for(&joined_key, 20).unwrap();
    d.apply_membership(
        &d.begin_work().unwrap(),
        std::slice::from_ref(&vouch.owner_statement),
    )
    .unwrap();
    let peers = d.snapshot().unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].device_public_key, joined_key);
    assert_eq!(
        peers[0].membership.owner_public_key,
        f.owner.public_key().to_hex()
    );
    assert_eq!(peers[0].membership.vouchers, vec![f.key()]);
    // The remembered chain is checked again when the directory opens.
    drop(d);
    let d = f.directory().unwrap();
    assert_eq!(d.snapshot().unwrap().len(), 1);
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[binding(&f.owner, &f.key(), "revoked", 30)],
    )
    .unwrap();
    assert!(d.snapshot().unwrap().is_empty());
}

#[test]
fn a_vouch_chain_to_another_person_key_is_not_a_roster_member() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    let other = Keys::generate();
    let voucher_root = tempfile::tempdir().unwrap();
    let mut voucher = DeviceIdentity::load_or_create(voucher_root.path()).unwrap();
    voucher
        .apply_owner_statement(&binding(
            &other,
            voucher.device_public_key().unwrap(),
            "owned",
            10,
        ))
        .unwrap();
    let vouch = voucher
        .vouch_for(&Keys::generate().public_key().to_hex(), 20)
        .unwrap();
    assert!(matches!(
        d.apply_membership(&d.begin_work().unwrap(), &[vouch.owner_statement]),
        Err(FederationError::Identity)
    ));
    assert!(d.snapshot().unwrap().is_empty());
}

fn owned_device(owner: &Keys, at: u64) -> (tempfile::TempDir, DeviceIdentity) {
    let root = tempfile::tempdir().unwrap();
    let mut identity = DeviceIdentity::load_or_create(root.path()).unwrap();
    let key = identity.device_public_key().unwrap().to_owned();
    identity
        .apply_owner_statement(&binding(owner, &key, "owned", at))
        .unwrap();
    (root, identity)
}

#[test]
fn a_revoked_voucher_leaves_the_roster_of_the_device_it_vouched_for() {
    // V and W are owned by the person key. X, this device, joined through V.
    let owner = Keys::generate();
    let (_v_root, voucher) = owned_device(&owner, 10);
    let (_w_root, other) = owned_device(&owner, 10);
    let (v, w) = (
        voucher.device_public_key().unwrap().to_owned(),
        other.device_public_key().unwrap().to_owned(),
    );
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let mut local = DeviceIdentity::load_or_create(root.path()).unwrap();
    let x = local.device_public_key().unwrap().to_owned();
    local
        .apply_owner_statement(&voucher.vouch_for(&x, 20).unwrap().owner_statement)
        .unwrap();
    let authorization = Authorization::load(root.path()).unwrap();
    let d = FederationDirectory::open(
        root.path(),
        PeerCredentials::from_identity(local.clone()),
        authorization.clone(),
        Arc::new(|| 1000),
    )
    .unwrap();
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[
            voucher.owner_statement_json().unwrap(),
            other.owner_statement_json().unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(d.snapshot().unwrap().len(), 2);
    assert!(!d.peer_is_revoked(&v).unwrap());
    assert!(!d.peer_is_revoked(&x).unwrap());

    // One relay batch: V's revocation, and a newer statement for W.
    let w_newer = binding(&owner, &w, "owned", 30);
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[binding(&owner, &v, "revoked", 31), w_newer.clone()],
    )
    .unwrap();
    let peers = d.snapshot().unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].device_public_key, w);
    assert_eq!(
        peers[0].membership.event_id,
        DeviceIdentity::verify_event(&w_newer).unwrap().id
    );
    // X no longer authorizes V.
    assert!(d.peer_is_revoked(&v).unwrap());
    let attempt = authorization
        .attempt(
            local.state(),
            &v,
            voucher.owner_statement_json().as_deref(),
            None,
            &[],
            1000,
        )
        .unwrap();
    assert!(matches!(
        attempt.context(),
        AuthorizationContext::Peer(Principal::Unknown { .. })
    ));
    // X's own chain passes through V, so its own relay stops serving.
    assert!(d.peer_is_revoked(&x).unwrap());
    // Later batches still apply.
    let (_y_root, added) = owned_device(&owner, 40);
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[added.owner_statement_json().unwrap()],
    )
    .unwrap();
    assert_eq!(d.snapshot().unwrap().len(), 2);
}

#[test]
fn a_remembered_chain_with_a_revoked_voucher_makes_its_device_revoked() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    // The peer vouched for Z; then the person key revoked the peer.
    let z_root = tempfile::tempdir().unwrap();
    let z = DeviceIdentity::load_or_create(z_root.path())
        .unwrap()
        .device_public_key()
        .unwrap()
        .to_owned();
    let vouch = f.peer.vouch_for(&z, 20).unwrap();
    d.apply_membership(
        &d.begin_work().unwrap(),
        std::slice::from_ref(&vouch.owner_statement),
    )
    .unwrap();
    assert!(!d.peer_is_revoked(&z).unwrap());
    // The revocation is known, but the batch does not prune Z yet: commit it
    // through the shared authority alone.
    let attempt = d
        .authorization
        .attempt(
            f.credentials.identity_snapshot().unwrap().state(),
            &z,
            None,
            None,
            &[binding(&f.owner, &f.key(), "revoked", 30)],
            1000,
        )
        .unwrap();
    d.authorization.commit_revocations(&attempt).unwrap();
    assert!(d.peer_is_revoked(&z).unwrap());
}

#[test]
fn a_vouch_chain_never_replaces_a_person_signed_statement() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let (_voucher_root, voucher) = owned_device(&f.owner, 10);
    let chain = voucher.vouch_for(&f.key(), 50).unwrap().owner_statement;
    d.apply_membership(&d.begin_work().unwrap(), std::slice::from_ref(&chain))
        .unwrap();
    let membership = &d.snapshot().unwrap()[0].membership;
    assert!(membership.vouchers.is_empty());
    assert_eq!(
        membership.event_id,
        DeviceIdentity::verify_event(&f.peer.owner_statement_json().unwrap())
            .unwrap()
            .id
    );

    // A device known through a chain takes the person key's statement, even
    // an older one.
    let z_root = tempfile::tempdir().unwrap();
    let z = DeviceIdentity::load_or_create(z_root.path())
        .unwrap()
        .device_public_key()
        .unwrap()
        .to_owned();
    let vouched = voucher.vouch_for(&z, 60).unwrap().owner_statement;
    d.apply_membership(&d.begin_work().unwrap(), std::slice::from_ref(&vouched))
        .unwrap();
    let person_signed = binding(&f.owner, &z, "owned", 5);
    d.apply_membership(
        &d.begin_work().unwrap(),
        std::slice::from_ref(&person_signed),
    )
    .unwrap();
    let peers = d.snapshot().unwrap();
    let z_peer = peers
        .iter()
        .find(|peer| peer.device_public_key == z)
        .unwrap();
    assert!(z_peer.membership.vouchers.is_empty());
    assert_eq!(z_peer.membership.created_at, 5);
}
