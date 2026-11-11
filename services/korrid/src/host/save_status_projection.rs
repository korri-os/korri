//! One in-memory save-status result per observed input generation.
//! Filesystem notifications invalidate status, never launch authority.

use crate::file_changes::{FileChanges, InputPath, InputState};
use crate::{saves::types::SaveStatus, RpcFailure};
use std::sync::Mutex;
use tokio::sync::watch;

#[cfg(test)]
#[path = "save_status_projection_tests.rs"]
mod tests;

pub(super) struct SaveStatusProjection {
    inputs: FileChanges,
    cached: Mutex<Option<(u64, String, SaveStatus)>>,
}

fn unavailable(message: impl ToString) -> RpcFailure {
    RpcFailure {
        code: "SavesUnavailable".into(),
        message: message.to_string(),
    }
}

impl SaveStatusProjection {
    pub(super) fn new(paths: Vec<InputPath>) -> Result<Self, RpcFailure> {
        Ok(Self {
            inputs: FileChanges::watch(paths).map_err(unavailable)?,
            cached: Mutex::new(None),
        })
    }

    pub(super) fn subscribe(&self) -> watch::Receiver<InputState> {
        self.inputs.subscribe()
    }

    pub(super) async fn ready(&self) -> Result<(), RpcFailure> {
        let mut changes = self.subscribe();
        changes
            .wait_for(|state| !matches!(state, InputState::Unavailable { revision: 0, .. }))
            .await
            .map_err(unavailable)?;
        match self.inputs.current() {
            InputState::Ready(_) => Ok(()),
            InputState::Unavailable { .. } => Err(unavailable("Save inputs cannot be watched.")),
        }
    }

    pub(super) fn read(
        &self,
        person: &str,
        compute: impl FnOnce() -> Result<SaveStatus, RpcFailure>,
    ) -> Result<SaveStatus, RpcFailure> {
        // Coalesce concurrent initial reads without duplicate plugin work.
        let mut cached = self.cached.lock().expect("save projection poisoned");
        let InputState::Ready(revision) = self.inputs.current() else {
            *cached = None;
            return Err(unavailable("Save inputs cannot be watched."));
        };
        if let Some((cached_revision, cached_person, status)) = &*cached {
            if *cached_revision == revision && cached_person == person {
                return Ok(status.clone());
            }
        }
        *cached = None;
        let status = compute()?;
        // Reconciliation can itself write a join record. Never label an
        // earlier computation with a revision received during that work.
        if self.inputs.current() == InputState::Ready(revision) {
            *cached = Some((revision, person.to_owned(), status.clone()));
        }
        Ok(status)
    }
}
