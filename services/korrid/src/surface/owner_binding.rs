//! Port of `clients/portal/src/surface/owner-binding.ts`: the person-signer
//! owner binding, in the treaty's confirmation shape.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use korri_surface_treaty::{SurfaceAction, SurfaceStatus};

use super::{korrid::Korrid, Notify};
use crate::{
    identity::IdentityState,
    owner_binding::{OwnerBindingSnapshot, OwnerSignerKind},
    OwnerBindingOutcome, PeerListOutcome,
};

pub const OWNER_BINDING_ACTION: &str = "owner-binding";
pub const OWNER_BINDING_CONTINUE: &str = "owner-binding-continue";

#[derive(Clone, Debug)]
pub enum OwnerBindingState {
    Loading,
    Unsupported,
    Ready(OwnerBindingSnapshot),
    Submitting(OwnerBindingSnapshot),
    Problem(String),
}

impl OwnerBindingState {
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported)
    }
    fn snapshot(&self) -> Option<&OwnerBindingSnapshot> {
        match self {
            Self::Ready(snapshot) | Self::Submitting(snapshot) => Some(snapshot),
            _ => None,
        }
    }
}

/// Uses the existing confirmation treaty. A surface never sees a signer URI,
/// a private key, or a second backend.
pub fn owner_binding_status(
    state: &OwnerBindingState,
    setup_needed: bool,
) -> Option<SurfaceStatus> {
    let confirm = |kicker: &str, detail: String, action: SurfaceAction| SurfaceStatus::Confirm {
        kicker: kicker.into(),
        detail: Some(detail),
        actions: vec![action],
        decline_action_id: None,
        game_id: None,
    };
    let snapshot = match state {
        OwnerBindingState::Loading | OwnerBindingState::Unsupported => return None,
        OwnerBindingState::Problem(message) => {
            return Some(confirm(
                "Could not read identity",
                message.clone(),
                SurfaceAction::new(OWNER_BINDING_ACTION, "Check identity", true),
            ))
        }
        OwnerBindingState::Ready(snapshot) | OwnerBindingState::Submitting(snapshot) => snapshot,
    };
    match &snapshot.identity {
        IdentityState::Owned { .. } => {
            return setup_needed.then(|| {
                confirm(
                    "Identity connected",
                    "Your signer keeps your person key. Korri can now find your other devices."
                        .into(),
                    SurfaceAction::new(OWNER_BINDING_CONTINUE, "Open library", true),
                )
            })
        }
        IdentityState::Unowned { .. } => {}
        _ => return None,
    }
    if matches!(state, OwnerBindingState::Submitting(_))
        || snapshot.person_signer.kind == OwnerSignerKind::Pending
    {
        return Some(SurfaceStatus::Busy {
            kicker: "Approve in your signer".into(),
            detail: Some("Choose the same person key as your other Korri devices and approve this device's owner binding.".into()),
            progress: None,
            actions: None,
            decline_action_id: None,
            game_id: None,
        });
    }
    let problem = matches!(
        snapshot.person_signer.kind,
        OwnerSignerKind::Denied | OwnerSignerKind::InvalidResponse | OwnerSignerKind::Defect
    )
    .then(|| snapshot.person_signer.message.clone());
    let detail: Vec<String> = [
        problem,
        snapshot.signer_requirement.clone(),
        Some("Use the same person key as your other Korri devices. The private key stays in your signer.".into()),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect();
    Some(confirm(
        "Connect your identity",
        detail.join(" "),
        SurfaceAction::new(
            OWNER_BINDING_ACTION,
            if snapshot.signer_requirement.is_some() {
                "Check for signer"
            } else {
                "Set up owner"
            },
            true,
        ),
    ))
}

pub type Hook = Arc<dyn Fn() + Send + Sync>;

struct Inner {
    state: OwnerBindingState,
    generation: u64,
    read: u64,
    submitting: bool,
    peer_signature: Option<String>,
}

/// Observes the native effect boundary and the verified peer list. Only a
/// membership/state change reloads the library.
pub struct OwnerBinding {
    korrid: Korrid,
    inner: Mutex<Inner>,
    notify: Notify,
    changed: Hook,
    finish: Hook,
}

impl OwnerBinding {
    pub fn new(korrid: Korrid, notify: Notify, changed: Hook, finish: Hook) -> Arc<Self> {
        Arc::new(Self {
            korrid,
            inner: Mutex::new(Inner {
                state: OwnerBindingState::Loading,
                generation: 0,
                read: 0,
                submitting: false,
                peer_signature: None,
            }),
            notify,
            changed,
            finish,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("owner binding poisoned")
    }

    pub fn snapshot(&self) -> OwnerBindingState {
        self.lock().state.clone()
    }

    fn publish(&self, next: OwnerBindingState) {
        self.lock().state = next;
        (self.notify)();
    }

    pub async fn refresh(&self) {
        let (epoch, request) = {
            let mut inner = self.lock();
            if inner.submitting {
                return;
            }
            inner.read += 1;
            (inner.generation, inner.read)
        };
        let current = |this: &Self| {
            let inner = this.lock();
            inner.generation == epoch && inner.read == request
        };
        let outcome = self.korrid.owner_binding_snapshot().await;
        if !current(self) {
            return;
        }
        let snapshot = match outcome {
            OwnerBindingOutcome::Err(failure) => {
                self.publish(if failure.code == "OperationUnsupported" {
                    OwnerBindingState::Unsupported
                } else {
                    OwnerBindingState::Problem(failure.message)
                });
                return;
            }
            OwnerBindingOutcome::Ok(snapshot) => snapshot,
        };
        let owned = matches!(snapshot.identity, IdentityState::Owned { .. });
        self.publish(OwnerBindingState::Ready(snapshot));
        if !owned {
            return;
        }
        let PeerListOutcome::Ok(peers) = self.korrid.peer_list().await else {
            return;
        };
        if !current(self) {
            return;
        }
        let mut rows: Vec<_> = peers
            .peers
            .iter()
            .map(|peer| serde_json::json!([peer.device_public_key, peer.label, peer.state]))
            .collect();
        rows.sort_by(|left, right| left[0].as_str().cmp(&right[0].as_str()));
        let signature = serde_json::to_string(&rows).expect("signature serializes");
        let changed = {
            let mut inner = self.lock();
            if inner.peer_signature.as_ref() != Some(&signature) {
                inner.peer_signature = Some(signature);
                true
            } else {
                false
            }
        };
        if changed {
            (self.changed)();
        }
    }

    /// Read now, then every 1.5 s until the device reports no support.
    pub fn start(self: &Arc<Self>) {
        let epoch = {
            let mut inner = self.lock();
            inner.submitting = false;
            inner.generation += 1;
            inner.generation
        };
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                this.refresh().await;
                {
                    let inner = this.lock();
                    if inner.generation != epoch || inner.state.is_unsupported() {
                        return;
                    }
                }
                tokio::time::sleep(Duration::from_millis(1500)).await;
            }
        });
    }

    pub fn act(self: &Arc<Self>, id: &str) -> bool {
        if id == OWNER_BINDING_CONTINUE {
            let owned = matches!(
                self.lock()
                    .state
                    .snapshot()
                    .map(|snapshot| &snapshot.identity),
                Some(IdentityState::Owned { .. })
            );
            if owned {
                (self.finish)();
            }
            return true;
        }
        if id != OWNER_BINDING_ACTION {
            return false;
        }
        let (snapshot, submitting) = {
            let inner = self.lock();
            (inner.state.snapshot().cloned(), inner.submitting)
        };
        if submitting {
            return true;
        }
        let observe = |this: &Arc<Self>| {
            let this = Arc::clone(this);
            tokio::spawn(async move { this.refresh().await });
        };
        let Some(snapshot) = snapshot else {
            observe(self);
            return true;
        };
        if snapshot.person_signer.kind == OwnerSignerKind::Pending
            || snapshot.signer_requirement.is_some()
        {
            observe(self);
            return true;
        }
        if !matches!(snapshot.identity, IdentityState::Unowned { .. }) {
            return true;
        }
        let epoch = {
            let mut inner = self.lock();
            inner.submitting = true;
            inner.read += 1;
            inner.generation
        };
        self.publish(OwnerBindingState::Submitting(snapshot));
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = this.korrid.start_owner_binding().await;
            if this.lock().generation != epoch {
                return;
            }
            this.lock().submitting = false;
            this.publish(match outcome {
                OwnerBindingOutcome::Ok(snapshot) => OwnerBindingState::Ready(snapshot),
                OwnerBindingOutcome::Err(failure) => OwnerBindingState::Problem(failure.message),
            });
        });
        true
    }
}
