//! Immutable starter authority, read from the exact launch journal.
use super::*;

impl HostSessionControl {
    /// Call context for a prospective launch. Never changes an existing journal.
    pub(crate) fn for_starting_device(mut self, device: Option<String>) -> Self {
        self.starting_device = device;
        self
    }

    pub(crate) fn require_session_starter(
        &self,
        device: &str,
        expected_launch_id: Option<&str>,
    ) -> Result<(), RpcFailure> {
        // Exact effects recheck the launch under their transition lock. Starter
        // authority is immutable for that launch, so it cannot race a replacement.
        let _state = self.lock_state();
        let record = read_active(&self.identity_root).map_err(|_| recovery_blocked_failure())?;
        match record {
            Some(ActiveSession::Running {
                launch_id,
                starter_device_public_key,
                ..
            }) if expected_launch_id.is_none_or(|expected| expected == launch_id) => {
                crate::authorization::require_session_starter(
                    device,
                    starter_device_public_key.as_deref(),
                )
            }
            _ => Err(failure(
                "NoActiveSession",
                "No matching live session is held here.",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::control::InMemoryLaunchUnitBackend;

    #[test]
    fn session_starter_survives_recovery_and_local_sessions_refuse_every_peer() {
        for starter in [None, Some("starter")] {
            let root = tempfile::tempdir().unwrap();
            let backend = Arc::new(InMemoryLaunchUnitBackend::default());
            let control = HostSessionControl::new(root.path(), backend.clone())
                .for_starting_device(starter.map(str::to_owned));
            let session = control
                .prepare("game", None, Ok(&["game".into()]), &BTreeMap::new())
                .unwrap();
            drop(control);
            let recovered = HostSessionControl::new(root.path(), backend);
            for device in ["starter", "other"] {
                let result = recovered.require_session_starter(device, Some(&session.launch_id));
                if starter == Some(device) {
                    result.unwrap();
                } else {
                    assert_eq!(result.unwrap_err().code, "NotSessionStarter");
                }
            }
        }
    }
}
