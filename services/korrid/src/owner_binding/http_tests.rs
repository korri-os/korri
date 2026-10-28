use crate::*;
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::Keys,
    types::Timestamp,
};

#[test]
fn authenticated_setup_uses_the_current_native_effect_and_signature_validation() {
    let _guard = crate::tests::embedded_server_guard();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        struct Stop;
        impl Drop for Stop {
            fn drop(&mut self) {
                let _ = stop_local_server();
            }
        }
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let bridge = owner_binding::OwnerBindingCoordinator::default();
        let platform = EmbeddedPlatform {
            relays: None,
            owner_binding: Some(bridge.clone()),
            ..EmbeddedPlatform::default()
        };
        let registry = plugin_policy::RegistrySource::Selected(Arc::new(
            plugin_test_fixtures::installed(readable.path()),
        ));
        let port = start_local_server_for_platform(
            "https://portal.example",
            readable.path().to_str().unwrap(),
            private.path().to_str().unwrap(),
            registry,
            platform,
        )
        .unwrap();
        let _stop = Stop;
        let generation = update_local_owner_signer(
            0,
            r#"{"personSigner":{"_tag":"Unavailable","message":"Choose a signer"}}"#,
        )
        .unwrap();
        let capability = local_server_capability().unwrap();
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/rpc");
        let request =
            serde_json::json!({"_tag":"system.identity.owner-binding.start","payload":{}});
        assert_eq!(
            client
                .post(&url)
                .json(&request)
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::UNAUTHORIZED
        );
        assert!(take_local_owner_binding_request(generation)
            .unwrap()
            .is_none());
        let result: serde_json::Value = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&request)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            result["outcome"]["payload"]["personSigner"]["_tag"],
            "Pending"
        );
        assert!(result["outcome"]["payload"]
            .get("signerRequirement")
            .is_none());
        let folders = serde_json::json!({"_tag":"app.library.folders.rescan","payload":{}});
        let before: serde_json::Value = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&folders)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        // Local setup is deliberately allowed before ownership; do not add
        // a new permission gate merely to make initial binding mandatory.
        assert_eq!(before["outcome"]["_tag"], "Ok");
        let template = take_local_owner_binding_request(generation)
            .unwrap()
            .unwrap();
        assert!(take_local_owner_binding_request(generation)
            .unwrap()
            .is_none());
        let template_json: serde_json::Value = serde_json::from_str(&template).unwrap();
        let state: serde_json::Value =
            serde_json::from_str(&local_identity_status_json().unwrap()).unwrap();
        let device = state["devicePublicKey"].as_str().unwrap();
        let owner = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(30078), "")
            .tags([
                Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
                Tag::parse(["device", device]).unwrap(),
                Tag::parse(["status", "owned"]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(
                template_json["created_at"].as_u64().unwrap(),
            ))
            .finalize(&owner)
            .unwrap();
        assert!(apply_local_owner_binding(&template, &"01".repeat(32), &event.as_json()).is_err());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&local_identity_status_json().unwrap())
                .unwrap()["_tag"],
            "Unowned"
        );
        apply_local_owner_binding(&template, &owner.public_key().to_hex(), &event.as_json())
            .unwrap();
        let status: serde_json::Value = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({"_tag":"system.identity.status","payload":{}}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(status["outcome"]["payload"]["identity"]["_tag"], "Owned");
        assert_eq!(status["outcome"]["payload"]["canSwitchIdentity"], false);
        assert_eq!(status["outcome"]["payload"]["localBackupAvailable"], false);
        let after: serde_json::Value = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&folders)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(after["outcome"]["_tag"], "Ok");
        assert_eq!(local_server_port(), Some(port));
        assert_eq!(
            local_server_capability().as_deref(),
            Some(capability.as_str())
        );
        assert!(bridge.request(private.path()).is_err());
        let identity = identity::DeviceIdentity::load_or_create(private.path()).unwrap();
        assert!(matches!(
            identity.state(),
            identity::IdentityState::Owned { .. }
        ));
    });
}

#[test]
fn owner_binding_is_never_a_peer_permission() {
    let request = RpcRequest::OwnerBindingStart(OwnerBindingRequest {});
    assert_eq!(
        authorization::policy_for(&request),
        authorization::PeerPolicy::LocalPortalOnly
    );
    assert!(serde_json::from_value::<RpcRequest>(serde_json::json!({"_tag":"system.identity.owner-binding.start","payload":{"signedEvent":"untrusted"}})).is_err());
}
