use super::*;
use crate::identity::{OwnerStatementStatus, VerifiedOwnerStatement};
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::Keys,
};
use std::sync::atomic::Ordering;

// There is one physical radio and one exported nearby candidate in a process.
static RADIO_TEST: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn identities() -> (Vec<tempfile::TempDir>, Vec<DeviceIdentity>) {
    let roots: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
    for root in &roots {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let identities = roots
        .iter()
        .map(|root| DeviceIdentity::load_or_create(root.path()).unwrap())
        .collect();
    (roots, identities)
}

#[test]
fn nearby_credentials_match_on_both_sides_and_differ_for_another_peer() {
    let (_roots, devices) = identities();
    let a = Credentials::derive(&devices[0], devices[1].device_public_key().unwrap()).unwrap();
    let b = Credentials::derive(&devices[1], devices[0].device_public_key().unwrap()).unwrap();
    let c = Credentials::derive(&devices[0], devices[2].device_public_key().unwrap()).unwrap();
    assert_eq!(a.ssid, b.ssid);
    assert_eq!(*a.passphrase, *b.passphrase);
    assert_ne!(a.ssid, c.ssid);
    assert_ne!(*a.passphrase, *c.passphrase);
    assert_eq!(a.ssid.len(), 32);
    assert_eq!(a.passphrase.len(), 60);
    assert_ne!(a.starter, b.starter);
    assert_eq!(
        a.starter,
        devices[0].device_public_key() < devices[1].device_public_key()
    );
}

fn snapshot(state: PeerState) -> PeerSnapshot {
    PeerSnapshot {
        device_public_key: "test-peer".into(),
        membership: VerifiedOwnerStatement {
            event_id: "test-event".into(),
            owner_public_key: "test-owner".into(),
            device_public_key: "test-peer".into(),
            status: OwnerStatementStatus::Owned,
            created_at: 1,
        },
        current_endpoint: None,
        remembered_endpoint: None,
        first_seen: 1,
        last_seen: 1,
        state,
        updated_at: 1,
    }
}

#[test]
fn nearby_trigger_requires_no_default_route_failed_member_and_elapsed_backoff() {
    let mut backoff = Backoff::default();
    let now = Instant::now();
    let peers = [snapshot(PeerState::Failed {
        error: "unreachable".into(),
    })];
    assert_eq!(backoff.peer(false, &peers, now), Some("test-peer"));
    assert_eq!(backoff.peer(true, &peers, now), None);
    assert_eq!(backoff.peer(false, &[], now), None);
    for state in [PeerState::Loading, PeerState::Ready] {
        assert_eq!(backoff.peer(false, &[snapshot(state)], now), None);
    }
    backoff.attempted("test-peer", now);
    assert_eq!(
        backoff.peer(false, &peers, now + Duration::from_secs(29)),
        None
    );
    assert_eq!(
        backoff.peer(false, &peers, now + Duration::from_secs(30)),
        Some("test-peer")
    );
    backoff.attempted("test-peer", now);
    assert_eq!(
        backoff.peer(false, &peers, now + Duration::from_secs(59)),
        None
    );
    assert_eq!(
        backoff.peer(false, &peers, now + Duration::from_secs(60)),
        Some("test-peer")
    );
}

fn owned_resources(lower: bool) -> (Vec<tempfile::TempDir>, FederationResources, String) {
    let (mut roots, mut devices) = identities();
    if (devices[0].device_public_key() < devices[1].device_public_key()) != lower {
        roots.swap(0, 1);
        devices.swap(0, 1);
    }
    let owner = Keys::generate();
    for device in &mut devices[..2] {
        let key = device.device_public_key().unwrap();
        let statement = EventBuilder::new(Kind::Custom(30_078), "")
            .tags([
                Tag::parse(["d", &format!("org.korri.device-owner:{key}")]).unwrap(),
                Tag::parse(["device", key]).unwrap(),
                Tag::parse(["status", "owned"]).unwrap(),
            ])
            .finalize(&owner)
            .unwrap()
            .as_json();
        device.apply_owner_statement(&statement).unwrap();
    }
    let resources = FederationResources::open(roots[0].path()).unwrap();
    resources
        .directory
        .apply_membership(
            &resources.directory.begin_work().unwrap(),
            &[devices[1].owner_statement_json().unwrap()],
        )
        .unwrap();
    (
        roots,
        resources,
        devices[1].device_public_key().unwrap().into(),
    )
}

#[tokio::test]
async fn nearby_starter_scans_first_starts_only_lower_key_and_restores_on_success_and_failure() {
    let _radio = RADIO_TEST.lock().await;
    for activation in [2, 4] {
        let (_roots, resources, peer) = owned_resources(true);
        let credentials = resources.credentials.identity_snapshot().unwrap();
        let derived = Credentials::derive(&credentials, &peer).unwrap();
        let (_server, bus, state) = dbus::dbus_tests::bus().await;
        *state.nearby_ssid.lock().unwrap() = Some(derived.ssid);
        state.nearby_visible.store(false, Ordering::SeqCst);
        state.activation.store(activation, Ordering::SeqCst);
        let mut link = NearbyLink {
            bus,
            resources,
            link: None,
        };
        assert_eq!(nearby_peer_ip(&peer), None);
        assert_eq!(link.start(&peer).await.is_ok(), activation == 2);
        if activation == 2 {
            assert_eq!(nearby_peer_ip(&peer), Some("10.77.77.2".parse().unwrap()));
            assert_eq!(nearby_peer_ip("other-peer"), None);
            // The normal local network controls share the radio operation.
            assert!(radio_operation().try_lock().is_err());
            link.stop().await.unwrap();
        }
        assert_eq!(nearby_peer_ip(&peer), None);
        assert!(state.home.load(Ordering::SeqCst));
        assert_eq!(
            *state.calls.lock().unwrap(),
            [
                "CheckpointCreate",
                "NearbyAP",
                "CheckpointRollback",
                "CheckpointDestroy"
            ]
        );
    }
}

#[tokio::test]
async fn nearby_visible_hidden_network_wins_over_starter_rule_and_higher_key_never_starts() {
    let _radio = RADIO_TEST.lock().await;
    for lower in [true, false] {
        let (_roots, resources, peer) = owned_resources(lower);
        let derived =
            Credentials::derive(&resources.credentials.identity_snapshot().unwrap(), &peer)
                .unwrap();
        let (_server, bus, state) = dbus::dbus_tests::bus().await;
        *state.nearby_ssid.lock().unwrap() = Some(derived.ssid);
        let mut link = NearbyLink {
            bus,
            resources,
            link: None,
        };
        link.start(&peer).await.unwrap();
        assert_eq!(nearby_peer_ip(&peer), Some("10.77.77.1".parse().unwrap()));
        link.stop().await.unwrap();
        assert!(state.calls.lock().unwrap().contains(&"NearbyClient"));
        if !lower {
            state.nearby_visible.store(false, Ordering::SeqCst);
            assert!(
                tokio::time::timeout(Duration::from_millis(50), link.start(&peer))
                    .await
                    .is_err()
            );
            assert_eq!(
                state
                    .calls
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|call| **call == "NearbyAP")
                    .count(),
                0
            );
            assert!(link.link.is_none());
            assert_eq!(nearby_peer_ip(&peer), None);
        }
    }
}

#[tokio::test]
async fn nearby_trigger_uses_real_member_snapshot_and_default_route_fact() {
    let _radio = RADIO_TEST.lock().await;
    let (_roots, resources, peer) = owned_resources(true);
    let derived =
        Credentials::derive(&resources.credentials.identity_snapshot().unwrap(), &peer).unwrap();
    let (_server, bus, state) = dbus::dbus_tests::bus().await;
    *state.nearby_ssid.lock().unwrap() = Some(derived.ssid);
    state.nearby_visible.store(false, Ordering::SeqCst);
    let mut link = NearbyLink {
        bus,
        resources,
        link: None,
    };
    let mut backoff = Backoff::default();
    let mut refresh_at = Instant::now();
    link.tick(&mut backoff, &mut refresh_at, false)
        .await
        .unwrap();
    assert!(state.calls.lock().unwrap().is_empty());
    link.resources
        .directory
        .set_peer_state(
            &link.resources.directory.begin_peer_work(&peer).unwrap(),
            &peer,
            PeerState::Failed {
                error: "unreachable".into(),
            },
        )
        .unwrap();
    // A kernel route outside NetworkManager also blocks the trigger.
    link.tick(&mut backoff, &mut refresh_at, true)
        .await
        .unwrap();
    assert!(state.calls.lock().unwrap().is_empty());
    // A route blocks the trigger, even for a Failed member.
    // Set the D-Bus property through the fixture's shared physical fact.
    state.default_route.store(true, Ordering::SeqCst);
    link.tick(&mut backoff, &mut refresh_at, false)
        .await
        .unwrap();
    assert!(state.calls.lock().unwrap().is_empty());
    state.default_route.store(false, Ordering::SeqCst);
    link.tick(&mut backoff, &mut refresh_at, false)
        .await
        .unwrap();
    assert!(state.calls.lock().unwrap().contains(&"NearbyAP"));
    assert!(nearby_peer_ip(&peer).is_some());
    link.stop().await.unwrap();
    let calls = state.calls.lock().unwrap().len();
    link.tick(&mut backoff, &mut refresh_at, false)
        .await
        .unwrap();
    assert_eq!(
        state.calls.lock().unwrap().len(),
        calls,
        "backoff prevents immediate retry"
    );
}

#[tokio::test]
async fn nearby_pending_activation_can_be_cancelled_and_stopped_without_a_candidate() {
    let _radio = RADIO_TEST.lock().await;
    let (_roots, resources, peer) = owned_resources(true);
    let derived =
        Credentials::derive(&resources.credentials.identity_snapshot().unwrap(), &peer).unwrap();
    let (_server, bus, state) = dbus::dbus_tests::bus().await;
    *state.nearby_ssid.lock().unwrap() = Some(derived.ssid);
    state.nearby_visible.store(false, Ordering::SeqCst);
    state.activation.store(1, Ordering::SeqCst);
    let mut link = NearbyLink {
        bus,
        resources,
        link: None,
    };
    assert!(
        tokio::time::timeout(Duration::from_millis(100), link.start(&peer))
            .await
            .is_err()
    );
    assert!(link.link.is_some());
    assert_eq!(nearby_peer_ip(&peer), None);
    link.stop().await.unwrap();
    assert!(state.home.load(Ordering::SeqCst));
    assert_eq!(nearby_peer_ip(&peer), None);
}

#[tokio::test]
async fn nearby_candidate_expires_without_refresh_and_drop_clears_it() {
    let _radio = RADIO_TEST.lock().await;
    let (_roots, resources, peer) = owned_resources(true);
    let (_server, bus, _state) = dbus::dbus_tests::bus().await;
    let link = NearbyLink {
        bus,
        resources,
        link: None,
    };
    *ACTIVE_PEER.lock().unwrap() = Some(PeerAddress {
        key: peer.clone(),
        ip: "10.77.77.2".parse().unwrap(),
        valid_until: Instant::now() - Duration::from_secs(1),
    });
    assert_eq!(nearby_peer_ip(&peer), None);
    ACTIVE_PEER.lock().unwrap().as_mut().unwrap().valid_until = Instant::now() + REFRESH_INTERVAL;
    assert!(nearby_peer_ip(&peer).is_some());
    drop(link);
    assert_eq!(nearby_peer_ip(&peer), None);
}

#[tokio::test]
async fn nearby_unreachable_peer_is_bounded_and_ready_peer_keeps_the_link() {
    let _radio = RADIO_TEST.lock().await;
    let (_roots, resources, peer) = owned_resources(true);
    let derived =
        Credentials::derive(&resources.credentials.identity_snapshot().unwrap(), &peer).unwrap();
    let (_server, bus, state) = dbus::dbus_tests::bus().await;
    *state.nearby_ssid.lock().unwrap() = Some(derived.ssid);
    state.nearby_visible.store(false, Ordering::SeqCst);
    let mut link = NearbyLink {
        bus,
        resources,
        link: None,
    };
    let mut backoff = Backoff::default();
    let mut refresh_at = Instant::now() + REFRESH_INTERVAL;
    link.start(&peer).await.unwrap();
    link.link.as_mut().unwrap().contact_deadline = Some(Instant::now() - Duration::from_secs(1));
    link.resources
        .directory
        .set_peer_state(
            &link.resources.directory.begin_peer_work(&peer).unwrap(),
            &peer,
            PeerState::Ready,
        )
        .unwrap();
    link.tick(&mut backoff, &mut refresh_at, false)
        .await
        .unwrap();
    assert!(link.link.as_ref().unwrap().contact_deadline.is_none());
    assert!(!state.home.load(Ordering::SeqCst));
    link.resources
        .directory
        .set_peer_state(
            &link.resources.directory.begin_peer_work(&peer).unwrap(),
            &peer,
            PeerState::Failed {
                error: "unreachable".into(),
            },
        )
        .unwrap();
    link.tick(&mut backoff, &mut refresh_at, false)
        .await
        .unwrap();
    assert!(link.link.as_ref().unwrap().contact_deadline.is_some());
    link.link.as_mut().unwrap().contact_deadline = Some(Instant::now() - Duration::from_secs(1));
    link.tick(&mut backoff, &mut refresh_at, false)
        .await
        .unwrap();
    assert!(link.link.is_none());
    assert!(state.home.load(Ordering::SeqCst));
    assert_eq!(nearby_peer_ip(&peer), None);
}

#[tokio::test]
async fn nearby_refuses_a_device_outside_the_member_snapshot() {
    let _radio = RADIO_TEST.lock().await;
    let (_roots, resources, _peer) = owned_resources(true);
    let (_server, bus, state) = dbus::dbus_tests::bus().await;
    let mut link = NearbyLink {
        bus,
        resources,
        link: None,
    };
    assert!(link.start(&"00".repeat(32)).await.is_err());
    assert!(state.calls.lock().unwrap().is_empty());
    assert!(state.home.load(Ordering::SeqCst));
}
