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
