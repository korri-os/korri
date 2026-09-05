use super::{
    config::{HostConfig, HostGame},
    input_seat::{DisabledInputSeats, InputSeatManager, UnixInputSeatManager},
    session_state::HostSessionControl,
    systemd_unit::{LaunchUnitBackend, SystemdLaunchUnitBackend},
};
use crate::{RpcFailure, SessionPrepared};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub struct HostLauncher {
    games: Arc<HashMap<String, HostGame>>,
    environment: Arc<std::collections::BTreeMap<String, String>>,
    control: HostSessionControl,
}

impl HostLauncher {
    pub fn new(config: &HostConfig, private_state_root: &Path) -> Self {
        let input_seats: Arc<dyn InputSeatManager> =
            std::env::var_os("KORRID_INPUT_SEAT_CONTROL_SOCKET")
                .map(PathBuf::from)
                .map(|path| Arc::new(UnixInputSeatManager::new(path)) as Arc<dyn InputSeatManager>)
                .unwrap_or_else(|| Arc::new(DisabledInputSeats));
        Self::with_backends(
            config,
            private_state_root,
            Arc::new(SystemdLaunchUnitBackend::default()),
            input_seats,
        )
    }

    #[cfg(test)]
    pub(crate) fn with_backend(
        config: &HostConfig,
        private_state_root: &Path,
        backend: Arc<dyn LaunchUnitBackend>,
    ) -> Self {
        Self::with_backends(
            config,
            private_state_root,
            backend,
            Arc::new(DisabledInputSeats),
        )
    }

    pub(crate) fn with_backends(
        config: &HostConfig,
        private_state_root: &Path,
        backend: Arc<dyn LaunchUnitBackend>,
        input_seats: Arc<dyn InputSeatManager>,
    ) -> Self {
        Self {
            games: Arc::new(
                config
                    .games
                    .iter()
                    .cloned()
                    .map(|game| (game.id.clone(), game))
                    .collect(),
            ),
            environment: Arc::new(config.environment.clone()),
            control: HostSessionControl::with_input_seats(private_state_root, backend, input_seats),
        }
    }

    pub fn prepare(
        &self,
        game_id: &str,
        person_public_key: Option<&str>,
    ) -> Result<SessionPrepared, RpcFailure> {
        let game = self.games.get(game_id).ok_or_else(|| RpcFailure {
            code: "HostGameNotFound".into(),
            message: format!("host game {game_id:?} is not configured"),
        })?;
        self.prepare_command(game_id, person_public_key, &game.command)
    }

    pub fn prepare_command(
        &self,
        game_id: &str,
        person_public_key: Option<&str>,
        configured_command: &[String],
    ) -> Result<SessionPrepared, RpcFailure> {
        self.control.prepare(
            game_id,
            person_public_key,
            configured_command,
            &self.environment,
        )
    }

    pub fn control(&self) -> &HostSessionControl {
        &self.control
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{
        config::{HostConfig, HostGame},
        systemd_unit::{LaunchUnitError, LaunchUnitState},
    };
    use std::{collections::BTreeMap, sync::Mutex};

    #[derive(Default)]
    struct RecordingBackend {
        launches: Mutex<Vec<(String, Vec<String>)>>,
        units: Mutex<BTreeMap<String, LaunchUnitState>>,
    }

    impl LaunchUnitBackend for RecordingBackend {
        fn launch(
            &self,
            launch_id: &str,
            command: &[String],
            _environment: &BTreeMap<String, String>,
        ) -> Result<(), LaunchUnitError> {
            self.launches
                .lock()
                .unwrap()
                .push((launch_id.into(), command.into()));
            self.units
                .lock()
                .unwrap()
                .insert(launch_id.into(), LaunchUnitState::Running);
            Ok(())
        }

        fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError> {
            Ok(self
                .units
                .lock()
                .unwrap()
                .get(launch_id)
                .copied()
                .unwrap_or(LaunchUnitState::Completed))
        }

        fn stop(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
            self.units
                .lock()
                .unwrap()
                .insert(launch_id.into(), LaunchUnitState::Completed);
            Ok(())
        }

        fn freeze(&self, _launch_id: &str) -> Result<(), LaunchUnitError> {
            unreachable!()
        }

        fn thaw(&self, _launch_id: &str) -> Result<(), LaunchUnitError> {
            unreachable!()
        }

        fn live_launch_ids(&self) -> Result<Vec<String>, LaunchUnitError> {
            Ok(self
                .units
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, state)| **state != LaunchUnitState::Completed)
                .map(|(id, _)| id.clone())
                .collect())
        }
    }

    fn launcher(game: HostGame) -> (tempfile::TempDir, Arc<RecordingBackend>, HostLauncher) {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(RecordingBackend::default());
        let launcher = HostLauncher::with_backend(
            &HostConfig {
                label: "zao".into(),
                games: vec![game],
                environment: BTreeMap::new(),
            },
            root.path(),
            backend.clone(),
        );
        (root, backend, launcher)
    }

    #[test]
    fn prepare_launches_the_configured_command_in_an_owned_unit() {
        let (_root, backend, launcher) = launcher(HostGame {
            id: "neverball".into(),
            title: "Neverball".into(),
            identity: None,
            command: vec!["neverball".into(), "--windowed".into()],
        });

        let prepared = launcher.prepare("neverball", None).unwrap();

        assert_eq!(prepared.game_id, "neverball");
        let launches = backend.launches.lock().unwrap();
        assert_eq!(launches[0].0, prepared.launch_id);
        assert_eq!(launches[0].1, ["neverball", "--windowed"]);
    }

    #[test]
    fn prepare_rejects_an_unknown_game() {
        let (_root, _backend, launcher) = launcher(HostGame {
            id: "neverball".into(),
            title: "Neverball".into(),
            identity: None,
            command: vec!["neverball".into()],
        });

        assert_eq!(
            launcher.prepare("missing", None).unwrap_err().code,
            "HostGameNotFound"
        );
    }

    #[test]
    fn prepare_is_idempotent_only_for_the_same_globally_active_game() {
        let (_root, backend, launcher) = launcher(HostGame {
            id: "slow".into(),
            title: "Slow".into(),
            identity: None,
            command: vec!["slow".into()],
        });

        let first = launcher.prepare("slow", None).unwrap();
        let repeated = launcher.prepare("slow", None).unwrap();

        assert_eq!(repeated.launch_id, first.launch_id);
        assert_eq!(backend.launches.lock().unwrap().len(), 1);
    }

    struct RecordingSeatManager {
        events: Arc<Mutex<Vec<&'static str>>>,
        fail_start: bool,
    }

    struct RecordingSeatLease {
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl InputSeatManager for RecordingSeatManager {
        fn start(
            &self,
            _launch_id: &str,
        ) -> Result<Box<dyn super::super::input_seat::InputSeatLease>, String> {
            self.events.lock().unwrap().push("seats-start");
            if self.fail_start {
                Err("seat start failed".into())
            } else {
                Ok(Box::new(RecordingSeatLease {
                    events: self.events.clone(),
                }))
            }
        }
    }

    impl super::super::input_seat::InputSeatLease for RecordingSeatLease {
        fn alive(&self) -> bool {
            true
        }
        fn stop(self: Box<Self>, _launch_id: &str) -> Result<(), String> {
            self.events.lock().unwrap().push("seats-stop");
            Ok(())
        }
    }

    struct OrderedBackend {
        events: Arc<Mutex<Vec<&'static str>>>,
        fail_launch: bool,
        units: Mutex<BTreeMap<String, LaunchUnitState>>,
    }

    impl LaunchUnitBackend for OrderedBackend {
        fn launch(
            &self,
            launch_id: &str,
            _command: &[String],
            _environment: &BTreeMap<String, String>,
        ) -> Result<(), LaunchUnitError> {
            self.events.lock().unwrap().push("game-launch");
            if self.fail_launch {
                return Err(LaunchUnitError::new(
                    crate::host::systemd_unit::LaunchUnitErrorKind::Failed,
                    "launch failed",
                ));
            }
            self.units
                .lock()
                .unwrap()
                .insert(launch_id.into(), LaunchUnitState::Running);
            Ok(())
        }
        fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError> {
            Ok(self
                .units
                .lock()
                .unwrap()
                .get(launch_id)
                .copied()
                .unwrap_or(LaunchUnitState::Completed))
        }
        fn stop(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
            self.events.lock().unwrap().push("game-stop");
            self.units
                .lock()
                .unwrap()
                .insert(launch_id.into(), LaunchUnitState::Completed);
            Ok(())
        }
        fn freeze(&self, _launch_id: &str) -> Result<(), LaunchUnitError> {
            unreachable!()
        }
        fn thaw(&self, _launch_id: &str) -> Result<(), LaunchUnitError> {
            unreachable!()
        }
        fn live_launch_ids(&self) -> Result<Vec<String>, LaunchUnitError> {
            Ok(self
                .units
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, state)| **state != LaunchUnitState::Completed)
                .map(|(id, _)| id.clone())
                .collect())
        }
    }

    fn ordered_launcher(
        fail_start: bool,
        fail_launch: bool,
    ) -> (
        tempfile::TempDir,
        Arc<Mutex<Vec<&'static str>>>,
        HostLauncher,
    ) {
        let root = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let backend = Arc::new(OrderedBackend {
            events: events.clone(),
            fail_launch,
            units: Mutex::new(BTreeMap::new()),
        });
        let manager = Arc::new(RecordingSeatManager {
            events: events.clone(),
            fail_start,
        });
        let launcher = HostLauncher::with_backends(
            &HostConfig {
                label: "zao".into(),
                games: vec![HostGame {
                    id: "game".into(),
                    title: "Game".into(),
                    identity: None,
                    command: vec!["game".into()],
                }],
                environment: BTreeMap::new(),
            },
            root.path(),
            backend,
            manager,
        );
        (root, events, launcher)
    }

    #[test]
    fn input_seats_are_ready_before_game_launch_and_stop_first() {
        let (_root, events, launcher) = ordered_launcher(false, false);
        let prepared = launcher.prepare("game", None).unwrap();
        assert_eq!(*events.lock().unwrap(), ["seats-start", "game-launch"]);
        assert!(matches!(
            launcher.control().stop(&prepared.launch_id),
            crate::host::session_state::HostSessionStop::Completed { .. }
        ));
        assert_eq!(
            *events.lock().unwrap(),
            ["seats-start", "game-launch", "seats-stop", "game-stop"]
        );
    }

    #[test]
    fn seat_start_failure_prevents_game_launch() {
        let (_root, events, launcher) = ordered_launcher(true, false);
        assert_eq!(
            launcher.prepare("game", None).unwrap_err().code,
            "InputSeatUnavailable"
        );
        assert_eq!(*events.lock().unwrap(), ["seats-start"]);
    }

    #[test]
    fn game_launch_failure_stops_the_created_seats() {
        let (_root, events, launcher) = ordered_launcher(false, true);
        assert_eq!(
            launcher.prepare("game", None).unwrap_err().code,
            "HostLaunchFailed"
        );
        assert_eq!(
            *events.lock().unwrap(),
            ["seats-start", "game-launch", "seats-stop"]
        );
    }
}
