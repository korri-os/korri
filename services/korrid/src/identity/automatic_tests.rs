use super::*;
use crate::{
    local_signer::LocalPersonSigner,
    remote_signer::{PersonSigner, PersonSignerRequest, PersonSignerState},
};
use futures::future::BoxFuture;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn automatic_owner_binding_is_silent_and_stable_across_restarts() {
    let identity_root = tempfile::tempdir().unwrap();
    let signer_root = tempfile::tempdir().unwrap();
    let expected_device = match DeviceIdentity::load_or_create(identity_root.path())
        .unwrap()
        .state()
    {
        IdentityState::Unowned { device_public_key } => device_public_key.clone(),
        state => panic!("unexpected identity state: {state:?}"),
    };
    let signer = LocalPersonSigner::load_or_create(signer_root.path(), &expected_device).unwrap();

    let first = bind_automatic_owner(identity_root.path(), &signer, 100)
        .await
        .unwrap();
    let IdentityState::Owned {
        device_public_key: first_device,
        owner_public_key: first_owner,
        ..
    } = first
    else {
        panic!("first boot did not bind the automatic owner")
    };
    let second = bind_automatic_owner(identity_root.path(), &signer, 200)
        .await
        .unwrap();
    let IdentityState::Owned {
        device_public_key: second_device,
        owner_public_key: second_owner,
        created_at,
        ..
    } = second
    else {
        panic!("restart lost the automatic owner")
    };

    assert_eq!(second_device, first_device);
    assert_eq!(second_owner, first_owner);
    assert_eq!(created_at, 100);
}

struct RefusingSigner {
    requests: Arc<Mutex<usize>>,
}

impl PersonSigner for RefusingSigner {
    fn state(&self) -> PersonSignerState {
        PersonSignerState::Denied {
            message: "configured refusal".into(),
        }
    }

    fn request<'a>(&'a self, _request: PersonSignerRequest) -> BoxFuture<'a, PersonSignerState> {
        Box::pin(async move {
            *self.requests.lock().unwrap() += 1;
            self.state()
        })
    }
}

#[tokio::test]
async fn signer_failure_leaves_the_device_unowned_and_retries_cleanly() {
    let identity_root = tempfile::tempdir().unwrap();
    let requests = Arc::new(Mutex::new(0));
    let signer = RefusingSigner {
        requests: requests.clone(),
    };

    assert!(bind_automatic_owner(identity_root.path(), &signer, 100)
        .await
        .is_err());
    assert!(matches!(
        DeviceIdentity::load_or_create(identity_root.path())
            .unwrap()
            .state(),
        IdentityState::Unowned { .. }
    ));
    assert!(bind_automatic_owner(identity_root.path(), &signer, 101)
        .await
        .is_err());
    assert_eq!(*requests.lock().unwrap(), 2);
}

fn unowned_device(root: &std::path::Path) -> String {
    match DeviceIdentity::load_or_create(root).unwrap().state() {
        IdentityState::Unowned { device_public_key } => device_public_key.clone(),
        state => panic!("unexpected identity state: {state:?}"),
    }
}

#[tokio::test]
async fn a_fresh_start_in_setup_stays_unowned() {
    let identity_root = tempfile::tempdir().unwrap();
    let signer_root = tempfile::tempdir().unwrap();
    let device = unowned_device(identity_root.path());
    let signer = LocalPersonSigner::load_or_create(signer_root.path(), &device).unwrap();
    for at in [100, 200] {
        let state = bind_automatic_owner_after_setup(identity_root.path(), &signer, at)
            .await
            .unwrap();
        assert!(matches!(state, IdentityState::Unowned { .. }), "{state:?}");
    }
}

#[tokio::test]
async fn finishing_setup_binds_the_automatic_owner() {
    let identity_root = tempfile::tempdir().unwrap();
    let signer_root = tempfile::tempdir().unwrap();
    let device = unowned_device(identity_root.path());
    let signer = LocalPersonSigner::load_or_create(signer_root.path(), &device).unwrap();
    let (outcome, bound) =
        crate::onboarding::finish_with_automatic_owner(identity_root.path(), &signer, 100).await;
    assert!(bound);
    assert!(matches!(
        outcome,
        crate::onboarding::OnboardingStatusOutcome::Ok(crate::onboarding::OnboardingStatus {
            needed: false
        })
    ));
    let IdentityState::Owned {
        owner_public_key, ..
    } = DeviceIdentity::load_or_create(identity_root.path())
        .unwrap()
        .state()
        .clone()
    else {
        panic!("setup's end did not bind the automatic owner")
    };
    // A restart after setup keeps today's behaviour: the same owner, no
    // second binding.
    let restarted = bind_automatic_owner_after_setup(identity_root.path(), &signer, 300)
        .await
        .unwrap();
    assert!(matches!(
        restarted,
        IdentityState::Owned { owner_public_key: ref again, created_at: 100, .. }
            if *again == owner_public_key
    ));
}

#[tokio::test]
async fn a_restart_after_setup_binds_an_unowned_device_as_before() {
    let identity_root = tempfile::tempdir().unwrap();
    let signer_root = tempfile::tempdir().unwrap();
    let device = unowned_device(identity_root.path());
    let signer = LocalPersonSigner::load_or_create(signer_root.path(), &device).unwrap();
    assert!(matches!(
        crate::onboarding::finish(identity_root.path()),
        crate::onboarding::OnboardingStatusOutcome::Ok(_)
    ));
    let state = bind_automatic_owner_after_setup(identity_root.path(), &signer, 100)
        .await
        .unwrap();
    assert!(matches!(state, IdentityState::Owned { .. }), "{state:?}");
}

#[tokio::test]
async fn a_device_that_joined_during_setup_is_not_rebound_when_setup_finishes() {
    let identity_root = tempfile::tempdir().unwrap();
    let signer_root = tempfile::tempdir().unwrap();
    let device = unowned_device(identity_root.path());
    // An owned device of another person vouches for this one, as a join does.
    let voucher_root = tempfile::tempdir().unwrap();
    let mut voucher = DeviceIdentity::load_or_create(voucher_root.path()).unwrap();
    let person = Keys::generate();
    let statement = EventBuilder::new(Kind::Custom(OWNER_EVENT_KIND), "")
        .tags(
            owner_tags(
                voucher.device_public_key().unwrap(),
                OwnerStatementStatus::Owned,
            )
            .into_iter()
            .map(|tag| Tag::parse(tag).unwrap()),
        )
        .custom_created_at(Timestamp::from(50))
        .finalize(&person)
        .unwrap()
        .as_json();
    voucher.apply_owner_statement(&statement).unwrap();
    let vouch = voucher.vouch_for(&device, 60).unwrap();
    DeviceIdentity::load_or_create(identity_root.path())
        .unwrap()
        .apply_owner_statement(&vouch.owner_statement)
        .unwrap();

    let signer = LocalPersonSigner::load_or_create(signer_root.path(), &device).unwrap();
    let (outcome, bound) =
        crate::onboarding::finish_with_automatic_owner(identity_root.path(), &signer, 100).await;
    assert!(!bound);
    assert!(matches!(
        outcome,
        crate::onboarding::OnboardingStatusOutcome::Ok(_)
    ));
    for _ in 0..2 {
        let state = bind_automatic_owner_after_setup(identity_root.path(), &signer, 200)
            .await
            .unwrap();
        assert!(
            matches!(
                state,
                IdentityState::Owned { ref owner_public_key, created_at: 60, .. }
                    if *owner_public_key == person.public_key().to_hex()
            ),
            "{state:?}"
        );
    }
}
