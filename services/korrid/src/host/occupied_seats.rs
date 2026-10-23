//! The receiver is the source of truth for connected local seat bindings.
use super::HostRuntime;
use std::path::PathBuf;

impl HostRuntime {
    pub(crate) fn occupied_local_seats(&self) -> Result<Vec<PathBuf>, String> {
        let coordinator = self
            .input_coordinator
            .as_ref()
            .ok_or("The local input-seat coordinator is unavailable")?;
        let snapshot = coordinator.snapshot()?;
        if snapshot.failure.is_some() || snapshot.recovery_required {
            return Err("The local input-seat coordinator is not ready".into());
        }
        Ok(snapshot.occupied_local_seats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{control::InMemoryLaunchUnitBackend, input_coordination::test_support};
    use std::sync::Arc;

    #[test]
    fn occupied_seat_paths_cross_the_real_coordinator_channel_without_inventing_inputs() {
        for paths in [vec![PathBuf::from("/dev/input/event17")], vec![]] {
            let expected = paths.clone();
            let (coordinator, receiver) = test_support::coordinator(move |_| {
                let mut reply = test_support::reply();
                reply.occupied_local_seats = paths.clone();
                reply
            });
            let root = tempfile::tempdir().unwrap();
            let config = root.path().join("host.toml");
            std::fs::write(&config, "label = \"local\"\n").unwrap();
            let mut runtime = HostRuntime::from_paths_with_backend(
                &config,
                None,
                root.path().join("private"),
                Arc::new(InMemoryLaunchUnitBackend::default()),
            );
            runtime.input_coordinator = Some(coordinator);
            assert_eq!(runtime.occupied_local_seats().unwrap(), expected);
            drop(runtime);
            receiver.join().unwrap();
        }
    }
}
