//! One gate at native commit, shared with peer selection. Route preparation
//! remains cancellable and does not block an unrelated replacement launch.
use crate::{upstreams::UpstreamRegistry, AppState, RpcFailure};
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedMutexGuard};

pub(crate) struct LocalLaunchAdmission {
    gate: Arc<Mutex<()>>,
    peers: Option<UpstreamRegistry>,
}

impl LocalLaunchAdmission {
    pub(crate) fn for_device(state: &AppState) -> Self {
        Self {
            gate: state.session_selection.clone(),
            peers: state.remote.clone(),
        }
    }

    /// Only the blocking native worker calls this, after route preparation.
    /// The guard lasts through native preparation, never through the reply.
    pub(crate) fn enter(self) -> Result<OwnedMutexGuard<()>, RpcFailure> {
        let guard = self.gate.blocking_lock_owned();
        if self.peers.is_some_and(|peers| peers.has_selected_session()) {
            return Err(RpcFailure {
                code: "ActiveSessionConflict".into(),
                message: "This device already has a selected session.".into(),
            });
        }
        Ok(guard)
    }
}
