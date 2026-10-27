//! Native signer effects for an unowned device. No person private key crosses
//! this boundary. The public snapshot fields and signer kinds come from the
//! retained KorriOwnerBindingController.snapshotJson / PersonSigner.State.
//! Unlike the removed bridge, the portal requests work through authenticated RPC;
//! the native shell consumes an effect and returns its public signer state.

use crate::{
    identity::{DeviceIdentity, IdentityState, OwnerStatementStatus},
    peer_rpc::unix_time,
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
use typeshare::typeshare;

#[cfg(test)]
mod http_tests;

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum OwnerSignerKind {
    Unavailable,
    Pending,
    Approved,
    Denied,
    InvalidResponse,
    Defect,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OwnerSignerState {
    #[serde(rename = "_tag")]
    pub kind: OwnerSignerKind,
    pub message: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerBindingSnapshot {
    pub identity: IdentityState,
    pub person_signer: OwnerSignerState,
    pub device_fingerprint: String,
    pub requested_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer_requirement: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeSnapshot {
    person_signer: OwnerSignerState,
    signer_requirement: Option<String>,
}

#[derive(Clone, Default)]
pub struct OwnerBindingCoordinator(Arc<Mutex<NativeState>>);

#[derive(Default)]
struct NativeState {
    generation: u64,
    attached: bool,
    signer: Option<NativeSnapshot>,
    request: Option<String>,
}

impl OwnerBindingCoordinator {
    /// A zero generation attaches a new activity. Updates from an older activity
    /// cannot overwrite the replacement's facts or detach its signer.
    pub fn update(&self, generation: u64, snapshot: &str) -> Result<u64, String> {
        if snapshot.len() > 64 * 1024 {
            return Err("The signer response is too large".into());
        }
        let snapshot: NativeSnapshot =
            serde_json::from_str(snapshot).map_err(|_| "The signer returned invalid status")?;
        if snapshot.person_signer.message.len() > 2048 {
            return Err("The signer status is too large".into());
        }
        let mut state = self.0.lock().expect("owner signer lock");
        if generation == 0 {
            state.generation = NEXT_GENERATION
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1).filter(|next| *next <= i64::MAX as u64)
                })
                .map_err(|_| "Signer generation exhausted")?;
            state.attached = true;
        } else if !state.attached || generation != state.generation {
            return Ok(generation);
        }
        state.signer = Some(snapshot);
        Ok(state.generation)
    }

    pub fn detach(&self, generation: u64) {
        let mut state = self.0.lock().expect("owner signer lock");
        if state.generation == generation {
            state.attached = false;
            state.signer = None;
        }
    }

    pub fn request(&self, root: &Path) -> Result<(), String> {
        let identity =
            DeviceIdentity::load_or_create(root).map_err(|_| "Device identity is unavailable")?;
        if !matches!(identity.state(), IdentityState::Unowned { .. }) {
            return Err("Only an unowned device can set up its owner".into());
        }
        let mut state = self.0.lock().expect("owner signer lock");
        if !state.attached {
            return Err("Open Korri to choose a signer".into());
        }
        let signer = state.signer.as_ref().ok_or("The signer is not ready")?;
        if let Some(requirement) = &signer.signer_requirement {
            return Err(requirement.clone());
        }
        if state.request.is_some() || signer.person_signer.kind == OwnerSignerKind::Pending {
            return Ok(());
        }
        let template = identity
            .owner_statement_template(OwnerStatementStatus::Owned, unix_time())
            .map_err(|_| "Korri could not create the owner request")?;
        state.request = Some(template);
        state
            .signer
            .as_mut()
            .expect("signer checked above")
            .person_signer = OwnerSignerState {
            kind: OwnerSignerKind::Pending,
            message: "Approve the owner binding in your signer".into(),
        };
        Ok(())
    }

    pub fn take_request(&self, generation: u64) -> Option<String> {
        let mut state = self.0.lock().expect("owner signer lock");
        if !state.attached || state.generation != generation {
            return None;
        }
        state.request.take()
    }

    pub fn snapshot(&self, identity: IdentityState) -> Result<OwnerBindingSnapshot, String> {
        let device_fingerprint = match &identity {
            IdentityState::Owned {
                device_public_key, ..
            }
            | IdentityState::Unowned { device_public_key }
            | IdentityState::Revoked {
                device_public_key, ..
            } => device_public_key.clone(),
            IdentityState::Invalid { .. } => return Err("Device identity is unavailable".into()),
        };
        let state = self.0.lock().expect("owner signer lock");
        let (person_signer, signer_requirement) = match &state.signer {
            Some(signer) if state.attached => (
                signer.person_signer.clone(),
                signer.signer_requirement.clone(),
            ),
            _ => {
                let message = "Open Korri to choose a signer".to_owned();
                (
                    OwnerSignerState {
                        kind: OwnerSignerKind::Unavailable,
                        message: message.clone(),
                    },
                    Some(message),
                )
            }
        };
        Ok(OwnerBindingSnapshot {
            identity,
            person_signer,
            device_fingerprint,
            requested_action: "Bind this device to your person identity".into(),
            signer_requirement,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn available() -> &'static str {
        r#"{"personSigner":{"_tag":"Unavailable","message":"Choose Set up owner"}}"#
    }
    #[test]
    fn only_the_current_activity_consumes_one_request_and_late_detach_is_harmless() {
        let root = tempfile::tempdir().unwrap();
        let bridge = OwnerBindingCoordinator::default();
        assert!(bridge.request(root.path()).is_err());
        let old = bridge.update(0, available()).unwrap();
        bridge.request(root.path()).unwrap();
        bridge.request(root.path()).unwrap();
        let current = bridge.update(0, available()).unwrap();
        bridge.detach(old);
        assert!(bridge.take_request(old).is_none());
        let template = bridge.take_request(current).unwrap();
        let value: serde_json::Value = serde_json::from_str(&template).unwrap();
        assert_eq!(value["kind"], 30078);
        assert!(bridge.take_request(current).is_none());
    }
    #[test]
    fn pending_survives_recreation_and_refusal_allows_an_explicit_retry() {
        let root = tempfile::tempdir().unwrap();
        let bridge = OwnerBindingCoordinator::default();
        let pending = r#"{"personSigner":{"_tag":"Pending","message":"Waiting"}}"#;
        let generation = bridge.update(0, pending).unwrap();
        bridge.request(root.path()).unwrap();
        assert!(bridge.take_request(generation).is_none());
        bridge
            .update(
                generation,
                r#"{"personSigner":{"_tag":"Denied","message":"Declined"}}"#,
            )
            .unwrap();
        bridge.request(root.path()).unwrap();
        assert!(bridge.take_request(generation).is_some());
    }
    #[test]
    fn unavailable_signer_and_invalid_native_updates_do_not_create_effects() {
        let root = tempfile::tempdir().unwrap();
        let bridge = OwnerBindingCoordinator::default();
        let generation = bridge.update(0, r#"{"personSigner":{"_tag":"Unavailable","message":"Install a signer"},"signerRequirement":"Install a signer"}"#).unwrap();
        assert_eq!(bridge.request(root.path()).unwrap_err(), "Install a signer");
        assert!(bridge.take_request(generation).is_none());
        assert!(bridge.update(generation, "{}").is_err());
        assert!(bridge.update(generation, &"x".repeat(65537)).is_err());
        assert!(bridge
            .update(
                generation,
                r#"{"personSigner":{"_tag":"Other","message":"bad"}}"#
            )
            .is_err());
    }
    #[test]
    fn native_status_never_supplies_identity_authority() {
        let bridge = OwnerBindingCoordinator::default();
        bridge.update(0, r#"{"identity":{"_tag":"Owned"},"personSigner":{"_tag":"Approved","message":"Signed"}}"#).unwrap();
        let snapshot = bridge
            .snapshot(IdentityState::Unowned {
                device_public_key: "device".into(),
            })
            .unwrap();
        assert!(matches!(snapshot.identity, IdentityState::Unowned { .. }));
        assert_eq!(snapshot.device_fingerprint, "device");
        assert!(bridge
            .snapshot(IdentityState::Invalid {
                reason: "private diagnostic".into()
            })
            .is_err());
    }
}
