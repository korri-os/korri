//! The launch units of korrid's federation fixture (feature `fixture`, #434
//! D10): a host that keeps its game units in memory. A unit is Running from
//! its launch until its stop, and nothing runs. The fixture's holder then
//! launches its game as a real host does, with no systemd unit, so that a
//! test device can stream from it. Never part of a shipped korrid.
use super::systemd_unit::{
    LaunchSandbox, LaunchUnitBackend, LaunchUnitError, LaunchUnitErrorKind, LaunchUnitState,
    DISPLAY_ENV, RUNNER_ID_ENV,
};
use std::{
    collections::BTreeMap,
    sync::{Mutex, PoisonError},
};

#[derive(Default)]
pub(crate) struct FixtureLaunchUnits {
    units: Mutex<BTreeMap<String, Unit>>,
}

struct Unit {
    state: LaunchUnitState,
    display: Option<crate::launcher::plugin_launch::PluginDisplay>,
    runner: Option<String>,
}

fn inactive() -> LaunchUnitError {
    LaunchUnitError::new(LaunchUnitErrorKind::Failed, "unit is not active")
}

impl FixtureLaunchUnits {
    fn with_unit<T>(&self, launch_id: &str, work: impl FnOnce(Option<&mut Unit>) -> T) -> T {
        let mut units = self.units.lock().unwrap_or_else(PoisonError::into_inner);
        work(units.get_mut(launch_id))
    }
}

impl LaunchUnitBackend for FixtureLaunchUnits {
    fn launch(
        &self,
        launch_id: &str,
        _command: &[String],
        environment: &BTreeMap<String, String>,
        _sandbox: &LaunchSandbox,
    ) -> Result<(), LaunchUnitError> {
        let display = environment
            .get(DISPLAY_ENV)
            .map(|encoded| serde_json::from_str(encoded))
            .transpose()
            .map_err(|error| {
                LaunchUnitError::new(LaunchUnitErrorKind::Protocol, error.to_string())
            })?;
        let mut units = self.units.lock().unwrap_or_else(PoisonError::into_inner);
        if units
            .get(launch_id)
            .is_some_and(|unit| unit.state != LaunchUnitState::Completed)
        {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "unit already runs",
            ));
        }
        units.insert(
            launch_id.into(),
            Unit {
                state: LaunchUnitState::Running,
                display,
                runner: environment.get(RUNNER_ID_ENV).cloned(),
            },
        );
        Ok(())
    }

    fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError> {
        Ok(self.with_unit(launch_id, |unit| {
            unit.map_or(LaunchUnitState::Completed, |unit| unit.state)
        }))
    }

    fn stop(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        self.with_unit(launch_id, |unit| {
            if let Some(unit) = unit {
                unit.state = LaunchUnitState::Completed;
            }
        });
        Ok(())
    }

    fn freeze(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        self.with_unit(launch_id, |unit| match unit {
            Some(unit) if unit.state != LaunchUnitState::Completed => {
                unit.state = LaunchUnitState::Frozen;
                Ok(())
            }
            _ => Err(inactive()),
        })
    }

    fn thaw(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        self.with_unit(launch_id, |unit| match unit {
            Some(unit) if unit.state != LaunchUnitState::Completed => {
                unit.state = LaunchUnitState::Running;
                Ok(())
            }
            _ => Err(inactive()),
        })
    }

    fn live_launch_ids(&self) -> Result<Vec<String>, LaunchUnitError> {
        Ok(self
            .units
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(_, unit)| unit.state != LaunchUnitState::Completed)
            .map(|(id, _)| id.clone())
            .collect())
    }

    fn runner_id(&self, launch_id: &str) -> Result<Option<String>, LaunchUnitError> {
        Ok(self.with_unit(launch_id, |unit| unit.and_then(|unit| unit.runner.clone())))
    }

    fn display(
        &self,
        launch_id: &str,
    ) -> Result<Option<crate::launcher::plugin_launch::PluginDisplay>, LaunchUnitError> {
        Ok(self.with_unit(launch_id, |unit| unit.and_then(|unit| unit.display.clone())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAUNCH: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn a_unit_runs_from_its_launch_until_its_stop_and_runs_nothing() {
        let units = FixtureLaunchUnits::default();
        let command = vec!["/bin/false".to_owned()];
        let environment = BTreeMap::new();
        let sandbox = LaunchSandbox::new(&command, &environment);
        assert_eq!(units.state(LAUNCH).unwrap(), LaunchUnitState::Completed);
        units
            .launch(LAUNCH, &command, &environment, &sandbox)
            .unwrap();
        assert_eq!(units.state(LAUNCH).unwrap(), LaunchUnitState::Running);
        assert_eq!(units.live_launch_ids().unwrap(), vec![LAUNCH.to_owned()]);
        assert!(units
            .launch(LAUNCH, &command, &environment, &sandbox)
            .is_err());
        units.freeze(LAUNCH).unwrap();
        assert_eq!(units.state(LAUNCH).unwrap(), LaunchUnitState::Frozen);
        units.thaw(LAUNCH).unwrap();
        units.stop(LAUNCH).unwrap();
        assert_eq!(units.state(LAUNCH).unwrap(), LaunchUnitState::Completed);
        assert!(units.live_launch_ids().unwrap().is_empty());
        assert!(units.freeze(LAUNCH).is_err());
    }
}
