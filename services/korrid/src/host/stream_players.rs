//! Registration and projection share the holder's exact-session transition.
use super::*;

impl HostSessionControl {
    pub(crate) fn describe_stream_players(
        &self,
        active: &mut crate::ActiveSession,
    ) -> Result<(), RpcFailure> {
        if let Some(ActiveSession::Running {
            launch_id,
            person_public_key,
            stream_players,
            ..
        }) = read_active(&self.identity_root).map_err(|_| recovery_blocked_failure())?
        {
            if active.launch_id == launch_id {
                active.person_public_key = person_public_key;
                active.stream_players = stream_players;
            }
        }
        Ok(())
    }

    pub(crate) fn stream_play(
        &self,
        expected: &str,
        player: crate::stream_play::StreamPlayer,
    ) -> Result<crate::ActiveSession, RpcFailure> {
        self.observe_status(|status, ownership, handoff| {
            let outcome = crate::host_session_status_outcome(Ok(status));
            let crate::SessionStatusOutcome::Ok(status) = outcome else {
                return Err(failure(
                    "NoActiveSession",
                    "This device has no live session.",
                ));
            };
            let mut active = status
                .active
                .ok_or_else(|| failure("NoActiveSession", "This device has no live session."))?;
            if active.phase.as_deref() == Some("stopping") {
                return Err(failure("NoActiveSession", "This session is ending."));
            }
            if active.launch_id != expected {
                return Err(failure(
                    "LaunchMismatch",
                    "That launch is no longer live on this device.",
                ));
            }
            let mut record = read_active(&self.identity_root)
                .map_err(|_| recovery_blocked_failure())?
                .ok_or_else(|| failure("NoActiveSession", "This device has no live session."))?;
            if let ActiveSession::Running { stream_players, .. } = &mut record {
                if !stream_players
                    .iter()
                    .any(|known| known.device_public_key == player.device_public_key)
                {
                    stream_players.push(player);
                    replace_active(&self.identity_root, &record)
                        .map_err(|_| recovery_blocked_failure())?;
                }
            }
            active.focus_ownership = ownership;
            active.initial_handoff = handoff;
            self.describe_stream_players(&mut active)?;
            Ok(active)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{host::control::InMemoryLaunchUnitBackend, stream_play::StreamPlayer};

    #[test]
    fn stream_play_requires_the_live_launch_and_keeps_one_entry_per_device_until_end() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(InMemoryLaunchUnitBackend::default());
        let control = HostSessionControl::new(root.path(), backend.clone());
        let player = || StreamPlayer {
            device_public_key: "player".into(),
            label: "Tablet".into(),
        };
        assert_eq!(
            control.stream_play("absent", player()).unwrap_err().code,
            "NoActiveSession"
        );
        let person = "33".repeat(32);
        let session = control
            .prepare(
                "game",
                Some(&person),
                Ok(&["game".into()]),
                &BTreeMap::new(),
            )
            .unwrap();
        assert_eq!(
            control.stream_play("wrong", player()).unwrap_err().code,
            "LaunchMismatch"
        );
        let active = control.stream_play(&session.launch_id, player()).unwrap();
        assert_eq!(active.stream_players, vec![player()]);
        assert_eq!(active.person_public_key.as_deref(), Some(person.as_str()));
        assert_eq!(
            control
                .stream_play(&session.launch_id, player())
                .unwrap()
                .stream_players
                .len(),
            1
        );
        let recovered = HostSessionControl::new(root.path(), backend);
        assert_eq!(
            recovered
                .stream_play(&session.launch_id, player())
                .unwrap()
                .stream_players
                .len(),
            1
        );
        recovered.stop(&session.launch_id);
        assert_eq!(
            recovered
                .stream_play(&session.launch_id, player())
                .unwrap_err()
                .code,
            "NoActiveSession"
        );
    }
}
