use super::*;
use nostr::key::Keys;
use std::sync::atomic::Ordering;
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
};

#[test]
fn missing_security_fields_are_corruption_not_counter_reset() {
    for field in [
        "publication",
        "ownerPublicKey",
        "localDevicePublicKey",
        "peers",
    ] {
        let f = Fixture::new();
        let d = f.directory().unwrap();
        d.reserve_publication(&d.begin_work().unwrap()).unwrap();
        drop(d);
        let path = f.root.path().join("federation/peers.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value.as_object_mut().unwrap().remove(field);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(f.directory().is_err(), "missing {field}");
    }
}

#[test]
fn root_modes_ancestor_links_and_wrong_credentials_are_rejected() {
    let f = Fixture::new();
    fs::set_permissions(f.root.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(f.directory().is_err());
    fs::set_permissions(f.root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let link_parent = tempfile::tempdir().unwrap();
    let link = link_parent.path().join("linked");
    symlink(f.root.path(), &link).unwrap();
    assert!(FederationDirectory::open(
        &link,
        f.credentials.clone(),
        Authorization::load(f.root.path()).unwrap(),
        Arc::new(|| 1000)
    )
    .is_err());
    let other = Fixture::new();
    assert!(FederationDirectory::open(
        f.root.path(),
        other.credentials.clone(),
        Authorization::load(f.root.path()).unwrap(),
        Arc::new(|| 1000)
    )
    .is_err());
}

#[test]
fn newer_peer_attempt_wins_without_invalidating_other_peers() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let second_root = tempfile::tempdir().unwrap();
    let second = DeviceIdentity::load_or_create(second_root.path()).unwrap();
    let second_key = second.device_public_key().unwrap();
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[binding(&f.owner, second_key, "owned", 10)],
    )
    .unwrap();
    let old = d.begin_peer_work(&f.key()).unwrap();
    let unrelated = d.begin_peer_work(second_key).unwrap();
    let new = d.begin_peer_work(&f.key()).unwrap();
    assert!(d.set_peer_state(&old, &f.key(), PeerState::Ready).is_err());
    d.set_peer_state(
        &new,
        &f.key(),
        PeerState::Failed {
            error: "offline".into(),
        },
    )
    .unwrap();
    d.set_peer_state(&unrelated, second_key, PeerState::Ready)
        .unwrap();
    assert_eq!(
        d.snapshot()
            .unwrap()
            .iter()
            .find(|p| p.device_public_key == f.key())
            .unwrap()
            .state,
        PeerState::Failed {
            error: "offline".into()
        }
    );
}

#[test]
fn unsafe_memory_write_fails_then_terminal_revocation_can_commit() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    f.roster(&d);
    let mut events = vec![binding(&f.owner, &f.key(), "revoked", 1)];
    // A bad path must fail before accepting any revocation.
    let path = f.root.path().join("federation/peers.json");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(d
        .apply_membership(&d.begin_work().unwrap(), &events)
        .is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    d.apply_membership(&d.begin_work().unwrap(), &events)
        .unwrap();
    events[0] = binding(&f.owner, &f.key(), "owned", 2000);
    d.apply_membership(&d.begin_work().unwrap(), &events)
        .unwrap();
    assert!(d.snapshot().unwrap().is_empty());
    drop(d);
    assert!(f.directory().unwrap().snapshot().unwrap().is_empty());
}

#[test]
fn concurrent_membership_updates_retry_stale_work_without_losing_peers() {
    let f = Fixture::new();
    let d = f.directory().unwrap();
    assert!(f.directory().is_err());
    let events: Vec<_> = (0..8)
        .map(|_| {
            binding(
                &f.owner,
                &Keys::generate().public_key().to_hex(),
                "owned",
                10,
            )
        })
        .collect();
    let workers: Vec<_> = events
        .into_iter()
        .map(|event| {
            let d = d.clone();
            std::thread::spawn(move || loop {
                match d.apply_membership(&d.begin_work().unwrap(), std::slice::from_ref(&event)) {
                    Ok(()) => break,
                    Err(FederationError::Stale) => continue,
                    Err(error) => panic!("{error}"),
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(d.snapshot().unwrap().len(), 8);
    drop(d);
    assert_eq!(f.directory().unwrap().snapshot().unwrap().len(), 8);
}

#[test]
fn document_size_failure_keeps_committed_revocation_terminal() {
    let f = Fixture::new();
    let authorization = Authorization::load(f.root.path()).unwrap();
    let d = FederationDirectory::open(
        f.root.path(),
        f.credentials.clone(),
        authorization.clone(),
        Arc::new(|| 1000),
    )
    .unwrap();
    f.roster(&d);
    let old = d.begin_work().unwrap();
    let old_peer = d.begin_peer_work(&f.key()).unwrap();
    let original = fs::read(f.root.path().join("federation/peers.json")).unwrap();
    let mut events = vec![binding(&f.owner, &f.key(), "revoked", 11)];
    for _ in 0..129 {
        let mut event = binding(
            &f.owner,
            &Keys::generate().public_key().to_hex(),
            "owned",
            10,
        );
        // Valid bounded signed sources, including insignificant JSON whitespace,
        // can still exceed the aggregate file bound. No private-state injection.
        event.push_str(&" ".repeat(64 * 1024 - event.len()));
        events.push(event);
    }
    assert!(matches!(
        d.apply_membership(&d.begin_work().unwrap(), &events),
        Err(FederationError::Bounds)
    ));
    assert_eq!(
        fs::read(f.root.path().join("federation/peers.json")).unwrap(),
        original
    );
    assert!(d.snapshot().unwrap().is_empty());
    let local = f.credentials.identity_snapshot().unwrap();
    let attempt = authorization
        .attempt(
            local.state(),
            &f.key(),
            f.peer.owner_statement_json().as_deref(),
            None,
            &[],
            1000,
        )
        .unwrap();
    assert!(matches!(
        attempt.context(),
        AuthorizationContext::Peer(Principal::Unknown { .. })
    ));
    assert!(d
        .apply_endpoint_event(&old, &f.event(&f.endpoint(1, 1000)))
        .is_err());
    assert!(d
        .set_peer_state(&old_peer, &f.key(), PeerState::Ready)
        .is_err());
    d.apply_membership(&old, &[binding(&f.owner, &f.key(), "owned", 2000)])
        .unwrap();
    assert!(d.snapshot().unwrap().is_empty());
    drop(d);
    let d = f.directory().unwrap();
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[binding(&f.owner, &f.key(), "owned", 2000)],
    )
    .unwrap();
    assert!(d.snapshot().unwrap().is_empty());
}

#[test]
fn relay_revocation_updates_the_existing_authorization_clones() {
    let f = Fixture::new();
    let authorization = Authorization::load(f.root.path()).unwrap();
    let d = FederationDirectory::open(
        f.root.path(),
        f.credentials.clone(),
        authorization.clone(),
        Arc::new(|| 1000),
    )
    .unwrap();
    f.roster(&d);
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[binding(&f.owner, &f.key(), "revoked", 11)],
    )
    .unwrap();
    let local = f.credentials.identity_snapshot().unwrap();
    let attempt = authorization
        .attempt(
            local.state(),
            &f.key(),
            f.peer.owner_statement_json().as_deref(),
            None,
            &[],
            1000,
        )
        .unwrap();
    assert!(matches!(
        attempt.context(),
        AuthorizationContext::Peer(Principal::Unknown { .. })
    ));
}

#[test]
fn revocation_write_failure_denies_shared_authority_and_retries_before_restart() {
    let f = Fixture::new();
    let authorization = Authorization::load(f.root.path()).unwrap();
    let d = FederationDirectory::open(
        f.root.path(),
        f.credentials.clone(),
        authorization.clone(),
        Arc::new(|| 1000),
    )
    .unwrap();
    f.roster(&d);
    let second_key = Keys::generate().public_key().to_hex();
    let second_owned = binding(&f.owner, &second_key, "owned", 10);
    d.apply_membership(
        &d.begin_work().unwrap(),
        std::slice::from_ref(&second_owned),
    )
    .unwrap();
    let old = d.begin_work().unwrap();
    let old_peer = d.begin_peer_work(&f.key()).unwrap();
    let path = f.root.path().join("federation/peers.json");
    let original = fs::read(&path).unwrap();
    let events = [
        binding(&f.owner, &f.key(), "revoked", 11),
        binding(&f.owner, &second_key, "revoked", 11),
    ];
    let files: Vec<_> = events
        .iter()
        .map(|event| {
            f.root.path().join(format!(
                "identity/authorization-revocations/{}.json",
                DeviceIdentity::verify_event(event).unwrap().id
            ))
        })
        .collect();
    // A real rename failure, independent of test-user privileges. Directory
    // preflight still succeeds; only the signed event's destination is blocked.
    fs::create_dir(&files[0]).unwrap();
    assert!(matches!(
        d.apply_membership(&old, &events),
        Err(FederationError::Storage)
    ));
    let local = f.credentials.identity_snapshot().unwrap();
    for (key, owned) in [
        (f.key(), f.peer.owner_statement_json().unwrap()),
        (second_key.clone(), second_owned),
    ] {
        let attempt = authorization
            .attempt(local.state(), &key, Some(&owned), None, &[], 1000)
            .unwrap();
        assert!(
            matches!(
                attempt.context(),
                AuthorizationContext::Peer(Principal::Unknown { .. })
            ),
            "every verified revocation must deny through an existing clone even when the first write fails"
        );
    }
    assert!(d.snapshot().unwrap().is_empty());
    assert!(d.begin_peer_work(&f.key()).is_err());
    assert!(d
        .apply_endpoint_event(&old, &f.event(&f.endpoint(1, 1000)))
        .is_err());
    assert!(d
        .set_peer_state(&old_peer, &f.key(), PeerState::Ready)
        .is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!files[1].exists());
    // Deduplication must not turn an unsynced retry into success.
    assert!(matches!(
        d.apply_membership(&d.begin_work().unwrap(), &events),
        Err(FederationError::Storage)
    ));
    fs::remove_dir(&files[0]).unwrap();
    d.apply_membership(&d.begin_work().unwrap(), &events)
        .unwrap();
    for (file, event) in files.iter().zip(&events) {
        assert_eq!(fs::read_to_string(file).unwrap(), *event);
        assert_eq!(
            fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    drop(d);
    drop(authorization);
    let authorization = Authorization::load(f.root.path()).unwrap();
    let reowned = binding(&f.owner, &f.key(), "owned", 2000);
    let attempt = authorization
        .attempt(local.state(), &f.key(), Some(&reowned), None, &[], 2000)
        .unwrap();
    assert!(matches!(
        attempt.context(),
        AuthorizationContext::Peer(Principal::Unknown { .. })
    ));
    let d = f.directory().unwrap();
    d.apply_membership(
        &d.begin_work().unwrap(),
        &[reowned, binding(&f.owner, &second_key, "owned", 2000)],
    )
    .unwrap();
    assert!(d.snapshot().unwrap().is_empty());
}

#[test]
fn publication_timestamp_overflow_is_not_wrapped_or_reset() {
    let f = Fixture::new();
    f.clock.store(u64::MAX, Ordering::SeqCst);
    let d = f.directory().unwrap();
    assert_eq!(
        d.reserve_publication(&d.begin_work().unwrap())
            .unwrap()
            .created_at,
        u64::MAX
    );
    drop(d);
    let d = f.directory().unwrap();
    assert!(matches!(
        d.reserve_publication(&d.begin_work().unwrap()),
        Err(FederationError::Publication)
    ));
}

#[test]
fn owner_reload_invalidates_in_flight_unowned_work_without_new_signing_cache() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let credentials = PeerCredentials::load(root.path()).unwrap();
    let directory = FederationDirectory::open(
        root.path(),
        credentials.clone(),
        Authorization::load(root.path()).unwrap(),
        Arc::new(|| 1000),
    )
    .unwrap();
    let old = directory.begin_work().unwrap();
    let owner = Keys::generate();
    let mut identity = DeviceIdentity::load_or_create(root.path()).unwrap();
    identity
        .apply_owner_statement(&binding(
            &owner,
            identity.device_public_key().unwrap(),
            "owned",
            10,
        ))
        .unwrap();
    credentials.reload_identity(root.path()).unwrap();
    assert!(directory.reserve_publication(&old).is_err());
    assert_eq!(
        directory
            .reserve_publication(&directory.begin_work().unwrap())
            .unwrap()
            .generation,
        1
    );
    drop(directory);
    assert!(FederationDirectory::open(
        root.path(),
        credentials,
        Authorization::load(root.path()).unwrap(),
        Arc::new(|| 1000)
    )
    .is_ok());
}
