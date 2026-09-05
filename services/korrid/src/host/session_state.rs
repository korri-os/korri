use crate::{RpcFailure, SessionPrepared};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use super::identity::{ACTIVE_FILE, TEMP_ACTIVE_PREFIX};
#[cfg(test)]
use std::fs;

use super::identity::{
    clear_active, clear_crash_temporary_active, consume_active, persist_active, read_active,
    replace_active, ActiveSession,
};
#[cfg(test)]
use super::input_seat::DisabledInputSeats;
use super::input_seat::{InputSeatLease, InputSeatManager};
use super::play_log::{PlayHistoryKey, PlayLogStore};
use super::systemd_unit::{LaunchUnitBackend, LaunchUnitState};
#[cfg(test)]
use super::systemd_unit::{LaunchUnitError, LaunchUnitErrorKind, SystemdLaunchUnitBackend};

/// Wall-clock seam. Production reads the system clock; tests supply
/// deterministic instants so recorded durations are exact.
pub(crate) trait WallClock: Send + Sync {
    fn now_epoch_seconds(&self) -> u64;
}

pub(crate) struct SystemWallClock;

impl WallClock for SystemWallClock {
    fn now_epoch_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostSessionStatus {
    Running {
        launch_id: String,
        game_id: Option<String>,
    },
    Frozen {
        launch_id: String,
        game_id: Option<String>,
    },
    Stopping {
        launch_id: String,
        game_id: Option<String>,
    },
    Completed {
        launch_id: String,
    },
    NoActive,
    RecoveryBlocked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostSessionStop {
    Completed { launch_id: String },
    NoActive,
    StaleIdentity { active_launch_id: Option<String> },
    AlreadyStopping { launch_id: String },
    RecoveryBlocked,
}

/// Outcome of an exact-identity freeze or thaw.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostSessionFreezeChange {
    /// The unit changed to the requested freezer state.
    Changed {
        launch_id: String,
    },
    /// The unit was already in the requested freezer state.
    Unchanged {
        launch_id: String,
    },
    NoActive,
    StaleIdentity {
        active_launch_id: Option<String>,
    },
    /// The unit is stopping; freezer changes are refused.
    Stopping {
        launch_id: String,
    },
    /// The systemd helper refused the change. The unit is untouched and
    /// the session stays in its last known state.
    HelperFailed {
        launch_id: String,
        message: String,
    },
    RecoveryBlocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FreezerTarget {
    Frozen,
    Running,
}

#[derive(Clone, Debug)]
enum ActiveState {
    Running {
        launch_id: String,
        game_id: Option<String>,
    },
    Frozen {
        launch_id: String,
        game_id: Option<String>,
    },
    Stopping {
        launch_id: String,
        game_id: Option<String>,
    },
    Completed {
        launch_id: String,
    },
    NoActive,
    RecoveryPending,
    RecoveryBlocked,
}

#[derive(Clone)]
pub struct HostSessionControl {
    backend: Arc<dyn LaunchUnitBackend>,
    input_seats: Arc<dyn InputSeatManager>,
    identity_root: PathBuf,
    play_log: PlayLogStore,
    clock: Arc<dyn WallClock>,
    state: Arc<Mutex<ActiveState>>,
    seat_lease: Arc<Mutex<Option<(String, Box<dyn InputSeatLease>)>>>,
}

impl HostSessionControl {
    #[cfg(test)]
    pub fn new(private_state_root: &Path, backend: Arc<dyn LaunchUnitBackend>) -> Self {
        Self::with_input_seats(private_state_root, backend, Arc::new(DisabledInputSeats))
    }

    pub fn with_input_seats(
        private_state_root: &Path,
        backend: Arc<dyn LaunchUnitBackend>,
        input_seats: Arc<dyn InputSeatManager>,
    ) -> Self {
        Self::with_clock(
            private_state_root,
            backend,
            input_seats,
            Arc::new(SystemWallClock),
        )
    }

    pub(crate) fn with_clock(
        private_state_root: &Path,
        backend: Arc<dyn LaunchUnitBackend>,
        input_seats: Arc<dyn InputSeatManager>,
        clock: Arc<dyn WallClock>,
    ) -> Self {
        let identity_root = private_state_root.join("host-session");
        Self {
            backend,
            input_seats,
            identity_root,
            play_log: PlayLogStore::new(private_state_root),
            clock,
            state: Arc::new(Mutex::new(ActiveState::RecoveryPending)),
            seat_lease: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn play_log(&self) -> &PlayLogStore {
        &self.play_log
    }

    /// Moves a proven completion through a durable two-phase journal.
    /// `completionPending` is written before the log. Recording is
    /// idempotent for its exact entry, so restart at every boundary is safe.
    fn complete_active(&self) -> Result<(), String> {
        let Some(record) = read_active(&self.identity_root)? else {
            return Ok(());
        };
        let (key, entry) = match record {
            ActiveSession::Running {
                person_public_key: None,
                ..
            } => {
                clear_active(&self.identity_root)?;
                return Ok(());
            }
            ActiveSession::Running {
                launch_id,
                game_id,
                person_public_key: Some(person),
                started_at,
            } => {
                let key = PlayHistoryKey {
                    user_id: person.clone(),
                    game_id: game_id.clone(),
                };
                let now = self.clock.now_epoch_seconds();
                let duration_seconds = now.saturating_sub(started_at) as f64;
                let entry = self
                    .play_log
                    .unique_completion_entry(&key, now.saturating_mul(1_000), duration_seconds)
                    .map_err(|error| error.to_string())?;
                replace_active(
                    &self.identity_root,
                    &ActiveSession::CompletionPending {
                        launch_id,
                        game_id,
                        person_public_key: person,
                        started_at,
                        entry: entry.clone(),
                    },
                )?;
                (key, entry)
            }
            ActiveSession::CompletionPending {
                game_id,
                person_public_key,
                entry,
                ..
            } => (
                PlayHistoryKey {
                    user_id: person_public_key,
                    game_id,
                },
                entry,
            ),
        };
        self.play_log
            .record(&key, entry)
            .map_err(|error| error.to_string())?;
        clear_active(&self.identity_root)
    }

    fn ensure_seats(&self, launch_id: &str) -> Result<(), String> {
        let mut current = self.seat_lease.lock().expect("input-seat mutex poisoned");
        if let Some((active, lease)) = current.as_ref() {
            if active != launch_id {
                return Err("input-seat lease belongs to a different launch".into());
            }
            if lease.alive() {
                return Ok(());
            }
            current.take();
        }
        let lease = self.input_seats.start(launch_id)?;
        *current = Some((launch_id.to_owned(), lease));
        Ok(())
    }

    fn stop_seats(&self, launch_id: &str) -> Result<(), String> {
        let lease = self
            .seat_lease
            .lock()
            .expect("input-seat mutex poisoned")
            .take();
        match lease {
            Some((active, lease)) if active == launch_id => lease.stop(launch_id),
            Some((_active, _lease)) => Err("input-seat lease belongs to a different launch".into()),
            None => Ok(()),
        }
    }

    fn stop_game_after_seat_failure(&self, state: &mut ActiveState, launch_id: &str) {
        let _ = self.stop_seats(launch_id);
        if self.backend.stop(launch_id).is_err()
            && !matches!(
                self.backend.state(launch_id),
                Ok(LaunchUnitState::Completed)
            )
        {
            *state = ActiveState::RecoveryBlocked;
            return;
        }
        match self.backend.state(launch_id) {
            Ok(LaunchUnitState::Completed) if self.complete_active().is_ok() => {
                *state = ActiveState::Completed {
                    launch_id: launch_id.to_owned(),
                };
            }
            Ok(
                LaunchUnitState::Running
                | LaunchUnitState::Frozen
                | LaunchUnitState::FreezerTransition
                | LaunchUnitState::Stopping,
            ) => {
                let game_id = tracked_game_id(state);
                *state = ActiveState::Stopping {
                    launch_id: launch_id.to_owned(),
                    game_id,
                };
            }
            _ => *state = ActiveState::RecoveryBlocked,
        }
    }

    fn refresh_recovery(&self, state: &mut ActiveState) {
        if matches!(
            state,
            ActiveState::RecoveryPending | ActiveState::RecoveryBlocked
        ) {
            *state = self.recover();
            if let ActiveState::Running { launch_id, .. } | ActiveState::Frozen { launch_id, .. } =
                &*state
            {
                let launch_id = launch_id.clone();
                if self.ensure_seats(&launch_id).is_err() {
                    self.stop_game_after_seat_failure(state, &launch_id);
                }
            }
        }
    }

    pub fn prepare(
        &self,
        game_id: &str,
        person_public_key: Option<&str>,
        configured_command: &[String],
        environment: &BTreeMap<String, String>,
    ) -> Result<SessionPrepared, RpcFailure> {
        let mut state = self.state.lock().expect("host session mutex poisoned");
        self.refresh_recovery(&mut state);
        match &*state {
            ActiveState::Running {
                launch_id,
                game_id: Some(active_game_id),
            } if active_game_id == game_id => {
                if self.ensure_seats(launch_id).is_err() {
                    let launch_id = launch_id.clone();
                    self.stop_game_after_seat_failure(&mut state, &launch_id);
                    return Err(failure(
                        "InputSeatUnavailable",
                        "input seats failed and the active game was stopped",
                    ));
                }
                return Ok(SessionPrepared {
                    game_id: game_id.into(),
                    launch_id: launch_id.clone(),
                });
            }
            ActiveState::Running { .. }
            | ActiveState::Frozen { .. }
            | ActiveState::Stopping { .. } => {
                return Err(failure(
                    "ActiveSessionConflict",
                    "one host game is already running or stopping",
                ));
            }
            ActiveState::RecoveryPending | ActiveState::RecoveryBlocked => {
                return Err(recovery_blocked_failure());
            }
            ActiveState::Completed { .. } | ActiveState::NoActive => {}
        }
        if let Some(person) = person_public_key {
            self.play_log
                .validate_key(&PlayHistoryKey {
                    user_id: person.to_owned(),
                    game_id: game_id.to_owned(),
                })
                .map_err(|_| {
                    failure(
                        "PlayLogPathUnavailable",
                        "the person or game identity cannot fit the legacy play-log path",
                    )
                })?;
        }
        if configured_command.is_empty() {
            return Err(failure(
                "HostLaunchFailed",
                format!("host game {game_id:?} has an empty command"),
            ));
        }

        let launch_id = crate::generate_launch_id();
        persist_active(
            &self.identity_root,
            &ActiveSession::running(
                launch_id.clone(),
                game_id.into(),
                person_public_key.map(str::to_owned),
                self.clock.now_epoch_seconds(),
            ),
        )
        .map_err(|message| {
            *state = ActiveState::RecoveryBlocked;
            failure("HostRecoveryBlocked", message)
        })?;
        let seat_lease = match self.input_seats.start(&launch_id) {
            Ok(lease) => lease,
            Err(message) => {
                // The game never ran; discard the record without logging.
                let _ = self.discard_active();
                *state = ActiveState::NoActive;
                return Err(failure("InputSeatUnavailable", message));
            }
        };
        if let Err(error) = self
            .backend
            .launch(&launch_id, configured_command, environment)
        {
            let _ = seat_lease.stop(&launch_id);
            match self.backend.live_launch_ids() {
                Ok(live) if !live.iter().any(|id| id == &launch_id) => {
                    if let Err(message) = self.discard_active() {
                        *state = ActiveState::RecoveryBlocked;
                        return Err(failure("HostRecoveryBlocked", message));
                    }
                    *state = ActiveState::NoActive;
                    return Err(failure("HostLaunchFailed", error.message));
                }
                _ => {
                    *state = ActiveState::RecoveryBlocked;
                    return Err(recovery_blocked_failure());
                }
            }
        }
        match self.backend.state(&launch_id) {
            Ok(
                observed @ (LaunchUnitState::Running
                | LaunchUnitState::Frozen
                | LaunchUnitState::FreezerTransition),
            ) => {
                // A unit that is already frozen right after launch was
                // frozen outside korrid (for example an operator froze the
                // slice). The launch is real, so record it and report the
                // observed freezer state; status, freeze, and thaw handle
                // it from here. Recovery is never blocked on a transient
                // freezer value.
                *self.seat_lease.lock().expect("input-seat mutex poisoned") =
                    Some((launch_id.clone(), seat_lease));
                *state = active_from_observed(observed, launch_id.clone(), Some(game_id.into()));
                Ok(SessionPrepared {
                    game_id: game_id.into(),
                    launch_id,
                })
            }
            Ok(LaunchUnitState::Stopping | LaunchUnitState::Completed) => {
                // The unit died before prepare returned. No play is
                // recorded for a launch the player never received.
                let _ = seat_lease.stop(&launch_id);
                if self.discard_active().is_err() {
                    *state = ActiveState::RecoveryBlocked;
                    return Err(recovery_blocked_failure());
                }
                *state = ActiveState::NoActive;
                Err(failure(
                    "HostLaunchFailed",
                    format!("host game {game_id:?} exited before prepare completed"),
                ))
            }
            Err(_) => {
                let _ = seat_lease.stop(&launch_id);
                *state = ActiveState::RecoveryBlocked;
                Err(recovery_blocked_failure())
            }
        }
    }

    pub fn status(&self) -> HostSessionStatus {
        let mut state = self.state.lock().expect("host session mutex poisoned");
        self.refresh_recovery(&mut state);
        let tracked = match &*state {
            ActiveState::Running { launch_id, .. }
            | ActiveState::Frozen { launch_id, .. }
            | ActiveState::Stopping { launch_id, .. } => Some(launch_id.clone()),
            ActiveState::Completed { .. }
            | ActiveState::NoActive
            | ActiveState::RecoveryPending
            | ActiveState::RecoveryBlocked => None,
        };
        if let Some(launch_id) = tracked {
            match self.backend.state(&launch_id) {
                Ok(
                    observed @ (LaunchUnitState::Running
                    | LaunchUnitState::Frozen
                    | LaunchUnitState::FreezerTransition),
                ) => {
                    if self.ensure_seats(&launch_id).is_err() {
                        self.stop_game_after_seat_failure(&mut state, &launch_id);
                    } else if !matches!(&*state, ActiveState::Stopping { .. }) {
                        // An in-flight exact stop owns the Stopping state;
                        // only reconcile the freezer state of a live launch.
                        let game_id = tracked_game_id(&state);
                        *state = active_from_observed(observed, launch_id.clone(), game_id);
                    }
                }
                Ok(LaunchUnitState::Stopping) => {
                    let _ = self.stop_seats(&launch_id);
                    let game_id = tracked_game_id(&state);
                    *state = ActiveState::Stopping {
                        launch_id: launch_id.clone(),
                        game_id,
                    };
                }
                Ok(LaunchUnitState::Completed) => {
                    if self.stop_seats(&launch_id).is_ok() && self.complete_active().is_ok() {
                        *state = ActiveState::Completed {
                            launch_id: launch_id.clone(),
                        };
                    } else {
                        *state = ActiveState::RecoveryBlocked;
                    }
                }
                Err(_) => *state = ActiveState::RecoveryBlocked,
            }
        }
        status_from_state(&state)
    }

    /// Removes the active record for a launch that never reached the
    /// player. Nothing is logged.
    fn discard_active(&self) -> Result<(), String> {
        consume_active(&self.identity_root).map(|_| ())
    }

    pub fn freeze(&self, expected_launch_id: &str) -> HostSessionFreezeChange {
        self.set_freezer(expected_launch_id, FreezerTarget::Frozen)
    }

    pub fn thaw(&self, expected_launch_id: &str) -> HostSessionFreezeChange {
        self.set_freezer(expected_launch_id, FreezerTarget::Running)
    }

    /// Moves the exact active launch to the requested freezer state. The
    /// session mutex is held across the helper call so an exact stop cannot
    /// interleave with a freezer change. Input-seat leases are kept alive
    /// while frozen so a thaw resumes the same seats.
    fn set_freezer(
        &self,
        expected_launch_id: &str,
        target: FreezerTarget,
    ) -> HostSessionFreezeChange {
        let mut state = self.state.lock().expect("host session mutex poisoned");
        self.refresh_recovery(&mut state);
        let (launch_id, game_id) = match &*state {
            ActiveState::Running { launch_id, game_id }
            | ActiveState::Frozen { launch_id, game_id }
                if launch_id == expected_launch_id =>
            {
                (launch_id.clone(), game_id.clone())
            }
            ActiveState::Running { launch_id, .. }
            | ActiveState::Frozen { launch_id, .. }
            | ActiveState::Stopping { launch_id, .. }
                if launch_id != expected_launch_id =>
            {
                return HostSessionFreezeChange::StaleIdentity {
                    active_launch_id: Some(launch_id.clone()),
                };
            }
            ActiveState::Stopping { launch_id, .. } => {
                return HostSessionFreezeChange::Stopping {
                    launch_id: launch_id.clone(),
                };
            }
            ActiveState::Completed { .. } | ActiveState::NoActive => {
                return HostSessionFreezeChange::NoActive;
            }
            ActiveState::RecoveryPending | ActiveState::RecoveryBlocked => {
                return HostSessionFreezeChange::RecoveryBlocked;
            }
            ActiveState::Running { .. } | ActiveState::Frozen { .. } => unreachable!(),
        };

        // Query the unit so a change made outside korrid is observed first.
        let observed = match self.backend.state(&launch_id) {
            Ok(observed) => observed,
            Err(_) => {
                *state = ActiveState::RecoveryBlocked;
                return HostSessionFreezeChange::RecoveryBlocked;
            }
        };
        match observed {
            LaunchUnitState::Running
            | LaunchUnitState::Frozen
            | LaunchUnitState::FreezerTransition => {}
            LaunchUnitState::Stopping => {
                let _ = self.stop_seats(&launch_id);
                *state = ActiveState::Stopping {
                    launch_id: launch_id.clone(),
                    game_id,
                };
                return HostSessionFreezeChange::Stopping { launch_id };
            }
            LaunchUnitState::Completed => {
                if self.stop_seats(&launch_id).is_ok() && self.complete_active().is_ok() {
                    *state = ActiveState::Completed {
                        launch_id: launch_id.clone(),
                    };
                    return HostSessionFreezeChange::NoActive;
                }
                *state = ActiveState::RecoveryBlocked;
                return HostSessionFreezeChange::RecoveryBlocked;
            }
        }
        // Only a settled state short-circuits. A unit observed `freezing`
        // or `thawing` always receives the verb; systemd's freeze and thaw
        // are idempotent, so the extra call is harmless and the response
        // reflects the requested settled state.
        let already = matches!(
            (observed, target),
            (LaunchUnitState::Frozen, FreezerTarget::Frozen)
                | (LaunchUnitState::Running, FreezerTarget::Running)
        );
        if !already {
            let result = match target {
                FreezerTarget::Frozen => self.backend.freeze(&launch_id),
                FreezerTarget::Running => self.backend.thaw(&launch_id),
            };
            if let Err(error) = result {
                // Re-read so a failed helper call on a unit that completed
                // underneath us is reported as no-active. Any other failure
                // leaves the unit and the session state untouched.
                return match self.backend.state(&launch_id) {
                    Ok(LaunchUnitState::Completed)
                        if self.stop_seats(&launch_id).is_ok()
                            && self.complete_active().is_ok() =>
                    {
                        *state = ActiveState::Completed {
                            launch_id: launch_id.clone(),
                        };
                        HostSessionFreezeChange::NoActive
                    }
                    Ok(LaunchUnitState::Completed) => {
                        *state = ActiveState::RecoveryBlocked;
                        HostSessionFreezeChange::RecoveryBlocked
                    }
                    Ok(
                        LaunchUnitState::Running
                        | LaunchUnitState::Frozen
                        | LaunchUnitState::FreezerTransition
                        | LaunchUnitState::Stopping,
                    ) => HostSessionFreezeChange::HelperFailed {
                        launch_id,
                        message: error.message,
                    },
                    Err(_) => {
                        *state = ActiveState::RecoveryBlocked;
                        HostSessionFreezeChange::RecoveryBlocked
                    }
                };
            }
        }
        *state = match target {
            FreezerTarget::Frozen => ActiveState::Frozen {
                launch_id: launch_id.clone(),
                game_id,
            },
            FreezerTarget::Running => ActiveState::Running {
                launch_id: launch_id.clone(),
                game_id,
            },
        };
        if already {
            HostSessionFreezeChange::Unchanged { launch_id }
        } else {
            HostSessionFreezeChange::Changed { launch_id }
        }
    }

    pub fn stop(&self, expected_launch_id: &str) -> HostSessionStop {
        let launch_id = {
            let mut state = self.state.lock().expect("host session mutex poisoned");
            self.refresh_recovery(&mut state);
            match &*state {
                ActiveState::Running { launch_id, game_id }
                | ActiveState::Frozen { launch_id, game_id }
                    if launch_id == expected_launch_id =>
                {
                    let launch_id = launch_id.clone();
                    *state = ActiveState::Stopping {
                        launch_id: launch_id.clone(),
                        game_id: game_id.clone(),
                    };
                    launch_id
                }
                ActiveState::Running { launch_id, .. } | ActiveState::Frozen { launch_id, .. } => {
                    return HostSessionStop::StaleIdentity {
                        active_launch_id: Some(launch_id.clone()),
                    };
                }
                ActiveState::Stopping { launch_id, .. } if launch_id == expected_launch_id => {
                    return HostSessionStop::AlreadyStopping {
                        launch_id: launch_id.clone(),
                    };
                }
                ActiveState::Stopping { launch_id, .. } => {
                    return HostSessionStop::StaleIdentity {
                        active_launch_id: Some(launch_id.clone()),
                    };
                }
                ActiveState::Completed { launch_id } if launch_id == expected_launch_id => {
                    return HostSessionStop::Completed {
                        launch_id: launch_id.clone(),
                    };
                }
                ActiveState::Completed { .. } | ActiveState::NoActive => {
                    return HostSessionStop::NoActive;
                }
                ActiveState::RecoveryPending | ActiveState::RecoveryBlocked => {
                    return HostSessionStop::RecoveryBlocked;
                }
            }
        };

        // The session mutex is released here on purpose: the backend stop
        // (and any thaw the backend performs first) runs outside the
        // mutex, as it did before freezer support. `set_freezer` observes
        // `Stopping` and refuses, so no freezer change interleaves with
        // the stop. Both halves of the stop path consistently run the
        // helper without the mutex.
        let seat_stop_failed = self.stop_seats(&launch_id).is_err();
        if self.backend.stop(&launch_id).is_err() {
            let mut state = self.state.lock().expect("host session mutex poisoned");
            if matches!(
                self.backend.state(&launch_id),
                Ok(LaunchUnitState::Completed)
            ) && !seat_stop_failed
            {
                return self.complete_stop(&mut state, launch_id);
            }
            *state = ActiveState::RecoveryBlocked;
            return HostSessionStop::RecoveryBlocked;
        }

        let mut state = self.state.lock().expect("host session mutex poisoned");
        if seat_stop_failed {
            *state = ActiveState::RecoveryBlocked;
            return HostSessionStop::RecoveryBlocked;
        }
        match self.backend.state(&launch_id) {
            Ok(LaunchUnitState::Completed) => self.complete_stop(&mut state, launch_id),
            Ok(
                LaunchUnitState::Running
                | LaunchUnitState::Frozen
                | LaunchUnitState::FreezerTransition
                | LaunchUnitState::Stopping,
            ) => HostSessionStop::AlreadyStopping { launch_id },
            Err(_) => {
                *state = ActiveState::RecoveryBlocked;
                HostSessionStop::RecoveryBlocked
            }
        }
    }
}

/// Maps a live observed unit state to the tracked session state. A unit in
/// a freezer transition is tracked as `Frozen`: its processes may not be
/// scheduled, and the next freeze or thaw request re-reads the unit before
/// acting.
fn active_from_observed(
    observed: LaunchUnitState,
    launch_id: String,
    game_id: Option<String>,
) -> ActiveState {
    match observed {
        LaunchUnitState::Frozen | LaunchUnitState::FreezerTransition => {
            ActiveState::Frozen { launch_id, game_id }
        }
        _ => ActiveState::Running { launch_id, game_id },
    }
}

fn tracked_game_id(state: &ActiveState) -> Option<String> {
    match state {
        ActiveState::Running { game_id, .. }
        | ActiveState::Frozen { game_id, .. }
        | ActiveState::Stopping { game_id, .. } => game_id.clone(),
        _ => None,
    }
}

fn status_from_state(state: &ActiveState) -> HostSessionStatus {
    match state {
        ActiveState::Running { launch_id, game_id } => HostSessionStatus::Running {
            launch_id: launch_id.clone(),
            game_id: game_id.clone(),
        },
        ActiveState::Frozen { launch_id, game_id } => HostSessionStatus::Frozen {
            launch_id: launch_id.clone(),
            game_id: game_id.clone(),
        },
        ActiveState::Stopping { launch_id, game_id } => HostSessionStatus::Stopping {
            launch_id: launch_id.clone(),
            game_id: game_id.clone(),
        },
        ActiveState::Completed { launch_id } => HostSessionStatus::Completed {
            launch_id: launch_id.clone(),
        },
        ActiveState::NoActive => HostSessionStatus::NoActive,
        ActiveState::RecoveryPending | ActiveState::RecoveryBlocked => {
            HostSessionStatus::RecoveryBlocked
        }
    }
}

impl HostSessionControl {
    fn complete_stop(&self, state: &mut ActiveState, launch_id: String) -> HostSessionStop {
        if self.complete_active().is_err() {
            *state = ActiveState::RecoveryBlocked;
            HostSessionStop::RecoveryBlocked
        } else {
            *state = ActiveState::Completed {
                launch_id: launch_id.clone(),
            };
            HostSessionStop::Completed { launch_id }
        }
    }

    /// Rebuilds the session state from the persisted record and systemd.
    /// A record whose unit already completed is logged once here, with the
    /// observation time as the play's end.
    fn recover(&self) -> ActiveState {
        let backend = self.backend.as_ref();
        let identity_root = &self.identity_root;
        let live = match backend.live_launch_ids() {
            Ok(live) => live,
            Err(_) => return ActiveState::RecoveryPending,
        };
        let mut persisted = read_active(identity_root);
        if persisted.is_err()
            && live.is_empty()
            && clear_crash_temporary_active(identity_root).unwrap_or(false)
        {
            persisted = read_active(identity_root);
        }
        match persisted {
            Ok(None) if live.is_empty() => ActiveState::NoActive,
            Ok(Some(ActiveSession::CompletionPending { .. })) if live.is_empty() => {
                if self.complete_active().is_ok() {
                    ActiveState::NoActive
                } else {
                    ActiveState::RecoveryBlocked
                }
            }
            Ok(Some(ActiveSession::CompletionPending { .. })) => ActiveState::RecoveryBlocked,
            Ok(Some(record @ ActiveSession::Running { .. }))
                if live.len() == 1
                    && live.first().map(String::as_str) == Some(record.launch_id()) =>
            {
                match backend.state(record.launch_id()) {
                    Ok(
                        observed @ (LaunchUnitState::Running
                        | LaunchUnitState::Frozen
                        | LaunchUnitState::FreezerTransition),
                    ) => active_from_observed(
                        observed,
                        record.launch_id().to_owned(),
                        Some(record.game_id().to_owned()),
                    ),
                    Ok(LaunchUnitState::Stopping) => ActiveState::Stopping {
                        launch_id: record.launch_id().to_owned(),
                        game_id: Some(record.game_id().to_owned()),
                    },
                    Ok(LaunchUnitState::Completed) if self.complete_active().is_ok() => {
                        ActiveState::NoActive
                    }
                    Ok(LaunchUnitState::Completed) => ActiveState::RecoveryBlocked,
                    Err(_) => ActiveState::RecoveryPending,
                }
            }
            Ok(Some(record @ ActiveSession::Running { .. })) if live.is_empty() => {
                match backend.state(record.launch_id()) {
                    Ok(LaunchUnitState::Completed) if self.complete_active().is_ok() => {
                        ActiveState::NoActive
                    }
                    Ok(LaunchUnitState::Completed) => ActiveState::RecoveryBlocked,
                    Ok(
                        LaunchUnitState::Running
                        | LaunchUnitState::Frozen
                        | LaunchUnitState::FreezerTransition
                        | LaunchUnitState::Stopping,
                    ) => ActiveState::RecoveryBlocked,
                    Err(_) => ActiveState::RecoveryPending,
                }
            }
            _ => ActiveState::RecoveryBlocked,
        }
    }
}

fn failure(code: &str, message: impl Into<String>) -> RpcFailure {
    RpcFailure {
        code: code.into(),
        message: message.into(),
    }
}

fn recovery_blocked_failure() -> RpcFailure {
    failure(
        "HostRecoveryBlocked",
        "host recovery identity is missing, tampered, or ambiguous; preserve all game units and require administrator resolution",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::play_log::PlayEntry;
    use std::{
        os::unix::fs::PermissionsExt,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Condvar,
        },
        thread,
        time::{Duration, Instant},
    };

    #[derive(Default)]
    struct BackendState {
        units: BTreeMap<String, LaunchUnitState>,
        stopped: Vec<String>,
        frozen: Vec<String>,
        thawed: Vec<String>,
        block_stop: bool,
        launch_error_after_start: bool,
        launch_rejected: bool,
        enumeration_unavailable: bool,
        stop_fails_when_collected: bool,
        freezer_fails: bool,
    }

    #[derive(Default)]
    struct DeterministicBackend {
        state: Mutex<BackendState>,
        changed: Condvar,
    }

    impl DeterministicBackend {
        fn insert(&self, id: &str, state: LaunchUnitState) {
            self.state.lock().unwrap().units.insert(id.into(), state);
        }

        fn release_stop(&self) {
            let mut state = self.state.lock().unwrap();
            state.block_stop = false;
            self.changed.notify_all();
        }
    }

    impl LaunchUnitBackend for DeterministicBackend {
        fn launch(
            &self,
            launch_id: &str,
            _command: &[String],
            _environment: &BTreeMap<String, String>,
        ) -> Result<(), LaunchUnitError> {
            if self.state.lock().unwrap().launch_rejected {
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    "launch rejected",
                ));
            }
            self.insert(launch_id, LaunchUnitState::Running);
            if self.state.lock().unwrap().launch_error_after_start {
                Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    "uncertain launch result",
                ))
            } else {
                Ok(())
            }
        }

        fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .units
                .get(launch_id)
                .copied()
                .unwrap_or(LaunchUnitState::Completed))
        }

        /// Mirrors systemd 259: `stop` on a frozen unit is refused, so the
        /// backend thaws first (recorded in `thawed`) and then stops. A
        /// refused thaw (`freezer_fails`) is a stop failure that leaves the
        /// unit frozen, exactly as the real helper would.
        fn stop(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
            let mut state = self.state.lock().unwrap();
            if state
                .units
                .get(launch_id)
                .is_some_and(|unit| unit.needs_thaw_before_stop())
            {
                state.thawed.push(launch_id.into());
                if state.freezer_fails {
                    return Err(LaunchUnitError::new(
                        LaunchUnitErrorKind::Failed,
                        "thaw before stop failed: thaw refused",
                    ));
                }
                state
                    .units
                    .insert(launch_id.into(), LaunchUnitState::Running);
            }
            state.stopped.push(launch_id.into());
            while state.block_stop {
                state = self.changed.wait(state).unwrap();
            }
            if state.stop_fails_when_collected
                && state.units.get(launch_id) == Some(&LaunchUnitState::Completed)
            {
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    "unit was collected",
                ));
            }
            if state
                .units
                .get(launch_id)
                .is_some_and(|unit| unit.needs_thaw_before_stop())
            {
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    "Cannot perform operation on frozen unit",
                ));
            }
            state
                .units
                .insert(launch_id.into(), LaunchUnitState::Completed);
            Ok(())
        }

        fn freeze(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
            let mut state = self.state.lock().unwrap();
            state.frozen.push(launch_id.into());
            if state.freezer_fails {
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    "freeze refused",
                ));
            }
            match state.units.get(launch_id).copied() {
                Some(
                    LaunchUnitState::Running
                    | LaunchUnitState::Frozen
                    | LaunchUnitState::FreezerTransition,
                ) => {
                    state
                        .units
                        .insert(launch_id.into(), LaunchUnitState::Frozen);
                    Ok(())
                }
                _ => Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    "unit is not active",
                )),
            }
        }

        fn thaw(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
            let mut state = self.state.lock().unwrap();
            state.thawed.push(launch_id.into());
            if state.freezer_fails {
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    "thaw refused",
                ));
            }
            match state.units.get(launch_id).copied() {
                Some(
                    LaunchUnitState::Running
                    | LaunchUnitState::Frozen
                    | LaunchUnitState::FreezerTransition,
                ) => {
                    state
                        .units
                        .insert(launch_id.into(), LaunchUnitState::Running);
                    Ok(())
                }
                _ => Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    "unit is not active",
                )),
            }
        }

        fn live_launch_ids(&self) -> Result<Vec<String>, LaunchUnitError> {
            let state = self.state.lock().unwrap();
            if state.enumeration_unavailable {
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    "systemd enumeration unavailable",
                ));
            }
            Ok(state
                .units
                .iter()
                .filter(|(_, state)| **state != LaunchUnitState::Completed)
                .map(|(id, _)| id.clone())
                .collect())
        }
    }

    const PERSON: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";

    /// Deterministic wall clock. Each call returns the current instant;
    /// tests advance it explicitly.
    struct TestClock(Mutex<u64>);

    impl TestClock {
        fn at(seconds: u64) -> Arc<Self> {
            Arc::new(Self(Mutex::new(seconds)))
        }

        fn advance(&self, seconds: u64) {
            *self.0.lock().unwrap() += seconds;
        }
    }

    impl WallClock for TestClock {
        fn now_epoch_seconds(&self) -> u64 {
            *self.0.lock().unwrap()
        }
    }

    fn control(root: &Path, backend: Arc<DeterministicBackend>) -> HostSessionControl {
        HostSessionControl::new(root, backend)
    }

    fn control_with_clock(
        root: &Path,
        backend: Arc<DeterministicBackend>,
        clock: Arc<TestClock>,
    ) -> HostSessionControl {
        HostSessionControl::with_clock(root, backend, Arc::new(DisabledInputSeats), clock)
    }

    fn persist_test_record(root: &Path, launch_id: &str) {
        persist_active(
            &root.join("host-session"),
            &ActiveSession::running(
                launch_id.into(),
                "recovered".into(),
                Some(PERSON.into()),
                1_700_000_000,
            ),
        )
        .unwrap();
    }

    fn stats(control: &HostSessionControl, game: &str) -> crate::PlayStats {
        control
            .play_log()
            .stats(&PlayHistoryKey {
                user_id: PERSON.into(),
                game_id: game.into(),
            })
            .unwrap()
    }

    struct TestSeatManager {
        starts: AtomicUsize,
        alive: Arc<AtomicBool>,
    }
    struct TestSeatLease {
        alive: Arc<AtomicBool>,
    }
    struct FailingSeatManager;
    impl InputSeatManager for TestSeatManager {
        fn start(&self, _launch_id: &str) -> Result<Box<dyn InputSeatLease>, String> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            self.alive.store(true, Ordering::SeqCst);
            Ok(Box::new(TestSeatLease {
                alive: self.alive.clone(),
            }))
        }
    }
    impl InputSeatManager for FailingSeatManager {
        fn start(&self, _launch_id: &str) -> Result<Box<dyn InputSeatLease>, String> {
            Err("seat receiver unavailable".into())
        }
    }
    impl InputSeatLease for TestSeatLease {
        fn alive(&self) -> bool {
            self.alive.load(Ordering::SeqCst)
        }
        fn stop(self: Box<Self>, _launch_id: &str) -> Result<(), String> {
            self.alive.store(false, Ordering::SeqCst);
            Ok(())
        }
    }

    fn prepare(control: &HostSessionControl, game: &str) -> SessionPrepared {
        control
            .prepare(game, None, &["game".into()], &BTreeMap::new())
            .unwrap()
    }

    #[test]
    fn restart_reattaches_only_one_exact_persisted_and_live_unit() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let first = control(root.path(), backend.clone());
        let prepared = prepare(&first, "one");
        drop(first);

        let recovered = control(root.path(), backend);
        assert_eq!(
            recovered.status(),
            HostSessionStatus::Running {
                launch_id: prepared.launch_id,
                game_id: Some("one".into()),
            }
        );
        assert_eq!(
            recovered
                .prepare("two", None, &["game".into()], &BTreeMap::new())
                .unwrap_err()
                .code,
            "ActiveSessionConflict"
        );
    }

    #[test]
    fn missing_tampered_and_ambiguous_recovery_preserve_units_and_block_mutation() {
        let cases = ["missing", "tampered", "multiple"];
        for case in cases {
            let root = tempfile::tempdir().unwrap();
            let backend = Arc::new(DeterministicBackend::default());
            let a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            backend.insert(a, LaunchUnitState::Running);
            if case != "missing" {
                let state = root.path().join("host-session");
                fs::create_dir(&state).unwrap();
                fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
                fs::write(
                    state.join(ACTIVE_FILE),
                    if case == "tampered" {
                        "bad".to_owned()
                    } else {
                        serde_json::to_string(&ActiveSession::running(
                            a.into(),
                            "one".into(),
                            None,
                            1,
                        ))
                        .unwrap()
                    },
                )
                .unwrap();
                fs::set_permissions(state.join(ACTIVE_FILE), fs::Permissions::from_mode(0o600))
                    .unwrap();
                if case == "multiple" {
                    fs::write(state.join("other"), a).unwrap();
                }
            }
            let control = control(root.path(), backend.clone());
            assert_eq!(control.status(), HostSessionStatus::RecoveryBlocked);
            assert_eq!(
                control
                    .prepare("two", None, &["game".into()], &BTreeMap::new())
                    .unwrap_err()
                    .code,
                "HostRecoveryBlocked"
            );
            assert_eq!(control.stop(a), HostSessionStop::RecoveryBlocked);
            assert!(backend.state.lock().unwrap().stopped.is_empty());
            assert_eq!(backend.state(a).unwrap(), LaunchUnitState::Running);
        }
    }

    #[test]
    fn uncertain_launch_result_preserves_and_recovers_the_exact_live_unit() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        backend.state.lock().unwrap().launch_error_after_start = true;
        let control = control(root.path(), backend.clone());

        assert_eq!(
            control
                .prepare("one", None, &["game".into()], &BTreeMap::new())
                .unwrap_err()
                .code,
            "HostRecoveryBlocked"
        );
        assert!(matches!(
            control.status(),
            HostSessionStatus::Running { .. }
        ));
        assert!(backend.state.lock().unwrap().stopped.is_empty());
        assert_eq!(
            backend
                .state
                .lock()
                .unwrap()
                .units
                .values()
                .copied()
                .collect::<Vec<_>>(),
            [LaunchUnitState::Running]
        );
    }

    #[test]
    fn stale_stop_never_targets_a_replacement_launch() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let control = control(root.path(), backend.clone());
        let first = prepare(&control, "one");
        backend.insert(&first.launch_id, LaunchUnitState::Completed);
        assert!(matches!(
            control.status(),
            HostSessionStatus::Completed { .. }
        ));
        let second = prepare(&control, "two");

        assert_eq!(
            control.stop(&first.launch_id),
            HostSessionStop::StaleIdentity {
                active_launch_id: Some(second.launch_id.clone())
            }
        );
        assert!(backend.state.lock().unwrap().stopped.is_empty());
        assert_eq!(
            backend.state(&second.launch_id).unwrap(),
            LaunchUnitState::Running
        );
    }

    #[test]
    fn prepare_and_repeated_stop_are_safe_while_exact_stop_is_in_flight() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let control = control(root.path(), backend.clone());
        let prepared = prepare(&control, "one");
        backend.state.lock().unwrap().block_stop = true;
        let stopping = control.clone();
        let expected = prepared.launch_id.clone();
        let stop_thread = thread::spawn(move || stopping.stop(&expected));
        for _ in 0..50 {
            if matches!(control.status(), HostSessionStatus::Stopping { .. }) {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }

        assert_eq!(
            control
                .prepare("two", None, &["game".into()], &BTreeMap::new())
                .unwrap_err()
                .code,
            "ActiveSessionConflict"
        );
        assert_eq!(
            control.stop(&prepared.launch_id),
            HostSessionStop::AlreadyStopping {
                launch_id: prepared.launch_id.clone()
            }
        );
        backend.release_stop();
        assert_eq!(
            stop_thread.join().unwrap(),
            HostSessionStop::Completed {
                launch_id: prepared.launch_id.clone()
            }
        );
        assert_eq!(
            control.stop(&prepared.launch_id),
            HostSessionStop::Completed {
                launch_id: prepared.launch_id
            }
        );
    }

    #[test]
    fn completed_persisted_unit_recovers_inactive_and_clears_only_identity() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        persist_test_record(root.path(), id);
        backend.insert(id, LaunchUnitState::Completed);

        let recovered = control(root.path(), backend.clone());

        assert_eq!(recovered.status(), HostSessionStatus::NoActive);
        assert!(!root.path().join("host-session").join(ACTIVE_FILE).exists());
        assert!(backend.state.lock().unwrap().stopped.is_empty());
    }

    #[test]
    fn exact_stop_collected_completion_race_returns_completed() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let control = control(root.path(), backend.clone());
        let prepared = prepare(&control, "one");
        backend.insert(&prepared.launch_id, LaunchUnitState::Completed);
        backend.state.lock().unwrap().stop_fails_when_collected = true;

        assert_eq!(
            control.stop(&prepared.launch_id),
            HostSessionStop::Completed {
                launch_id: prepared.launch_id
            }
        );
    }

    #[test]
    fn enumeration_unavailability_retries_without_becoming_ambiguity() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        backend.state.lock().unwrap().enumeration_unavailable = true;
        let runtime_control = control(root.path(), backend.clone());

        assert_eq!(runtime_control.status(), HostSessionStatus::RecoveryBlocked);
        backend.state.lock().unwrap().enumeration_unavailable = false;
        assert_eq!(runtime_control.status(), HostSessionStatus::NoActive);
        assert!(runtime_control
            .prepare("one", None, &["game".into()], &BTreeMap::new())
            .is_ok());

        let prepare_root = tempfile::tempdir().unwrap();
        let prepare_backend = Arc::new(DeterministicBackend::default());
        prepare_backend
            .state
            .lock()
            .unwrap()
            .enumeration_unavailable = true;
        let prepare_control = control(prepare_root.path(), prepare_backend.clone());
        assert_eq!(prepare_control.status(), HostSessionStatus::RecoveryBlocked);
        prepare_backend
            .state
            .lock()
            .unwrap()
            .enumeration_unavailable = false;
        assert!(prepare_control
            .prepare("one", None, &["game".into()], &BTreeMap::new())
            .is_ok());
    }

    #[test]
    fn crash_temporary_identity_is_cleared_only_when_no_game_is_live() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let state_root = root.path().join("host-session");
        fs::create_dir(&state_root).unwrap();
        fs::set_permissions(&state_root, fs::Permissions::from_mode(0o700)).unwrap();
        let temporary = state_root.join(format!("{TEMP_ACTIVE_PREFIX}partial"));
        fs::write(&temporary, "partial").unwrap();
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600)).unwrap();

        let recovered = control(root.path(), backend.clone());
        assert_eq!(recovered.status(), HostSessionStatus::NoActive);
        assert!(!temporary.exists());

        let live_root = tempfile::tempdir().unwrap();
        let live_state = live_root.path().join("host-session");
        fs::create_dir(&live_state).unwrap();
        fs::set_permissions(&live_state, fs::Permissions::from_mode(0o700)).unwrap();
        let live_temporary = live_state.join(format!("{TEMP_ACTIVE_PREFIX}partial"));
        fs::write(&live_temporary, "partial").unwrap();
        fs::set_permissions(&live_temporary, fs::Permissions::from_mode(0o600)).unwrap();
        let id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        backend.insert(id, LaunchUnitState::Running);

        let blocked = control(live_root.path(), backend);
        assert_eq!(blocked.status(), HostSessionStatus::RecoveryBlocked);
        assert!(live_temporary.exists());
    }

    fn systemd_backend(uid: u32, gid: u32) -> SystemdLaunchUnitBackend {
        SystemdLaunchUnitBackend::new(
            PathBuf::from("/nix/store/systemd/bin/systemd-run"),
            PathBuf::from("/nix/store/systemd/bin/systemctl"),
            uid,
            gid,
        )
        .unwrap()
    }

    #[test]
    fn systemd_operations_use_exact_immutable_helpers_and_hardened_game_credentials() {
        let id = "0123456789abcdef0123456789abcdef";
        let unit = "korri-game-0123456789abcdef0123456789abcdef.service";
        let backend = systemd_backend(1001, 1002);
        assert_eq!(SystemdLaunchUnitBackend::unit_name(id).unwrap(), unit);
        assert_eq!(
            backend.systemd_run,
            PathBuf::from("/nix/store/systemd/bin/systemd-run")
        );
        assert_eq!(
            backend.systemctl,
            PathBuf::from("/nix/store/systemd/bin/systemctl")
        );
        let launch = backend
            .launch_arguments(
                id,
                &["/games/retroarch".into(), "rom.gba".into()],
                &BTreeMap::from([
                    ("SAVE_ROOT".into(), "/saves".into()),
                    ("WAYLAND_DISPLAY".into(), "korri-wayland".into()),
                    (
                        "SWAYSOCK".into(),
                        "/run/korri-compositor/sway-ipc.sock".into(),
                    ),
                    ("XDG_RUNTIME_DIR".into(), "/run/user/1001".into()),
                ]),
            )
            .unwrap();
        for expected in [
            format!("--unit={unit}"),
            "--uid=1001".into(),
            "--gid=1002".into(),
            "--property=KillMode=control-group".into(),
            "--property=NoNewPrivileges=yes".into(),
            "--property=CapabilityBoundingSet=".into(),
            "--property=AmbientCapabilities=".into(),
            "--property=PrivateTmp=yes".into(),
            "--property=PrivatePIDs=yes".into(),
            "--property=BindReadOnlyPaths=/tmp/.X11-unix/X0".into(),
            "--property=ProtectKernelTunables=yes".into(),
            "--property=ProtectKernelModules=yes".into(),
            "--property=ProtectControlGroups=yes".into(),
            "--property=InaccessiblePaths=/var/lib/korrid /run/korrid /run/korrid-control/control.sock /run/korrid-control /home/korri/.config/sunshine /run/korri-compositor /run/korri-certificate-control /run/user/1001 -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources".into(),
            "--property=RestrictSUIDSGID=yes".into(),
        ] {
            assert!(
                launch.contains(&expected),
                "missing {expected:?} from {launch:?}"
            );
        }
        for withheld in ["WAYLAND_DISPLAY", "SWAYSOCK", "XDG_RUNTIME_DIR"] {
            assert!(
                !launch
                    .iter()
                    .any(|argument| argument.starts_with(&format!("--setenv={withheld}="))),
                "game launch exposed {withheld}: {launch:?}"
            );
        }
        assert_eq!(
            launch[launch.len() - 3..],
            ["--", "/games/retroarch", "rom.gba"]
        );
        assert_eq!(
            SystemdLaunchUnitBackend::stop_arguments(id).unwrap(),
            ["--system", "--no-ask-password", "stop", unit]
        );
        assert!(SystemdLaunchUnitBackend::stop_arguments("game-name").is_err());
        assert_eq!(
            SystemdLaunchUnitBackend::freeze_arguments(id).unwrap(),
            ["--system", "--no-ask-password", "freeze", unit]
        );
        assert_eq!(
            SystemdLaunchUnitBackend::thaw_arguments(id).unwrap(),
            ["--system", "--no-ask-password", "thaw", unit]
        );
        assert!(SystemdLaunchUnitBackend::freeze_arguments("game-name").is_err());
        assert!(SystemdLaunchUnitBackend::thaw_arguments("../escape").is_err());
        assert_eq!(
            SystemdLaunchUnitBackend::state_arguments(id).unwrap(),
            [
                "--system",
                "--no-ask-password",
                "show",
                unit,
                "--property=LoadState",
                "--property=ActiveState",
                "--property=FreezerState",
            ]
        );
    }

    #[test]
    fn systemd_state_parsing_maps_freezer_states_and_rejects_unknown_values() {
        fn parse(pairs: &[(&str, &str)]) -> Result<LaunchUnitState, LaunchUnitError> {
            SystemdLaunchUnitBackend::parse_unit_state(&pairs.iter().copied().collect())
        }
        assert_eq!(
            parse(&[("ActiveState", "active"), ("FreezerState", "running")]).unwrap(),
            LaunchUnitState::Running
        );
        assert_eq!(
            parse(&[("ActiveState", "active"), ("FreezerState", "frozen")]).unwrap(),
            LaunchUnitState::Frozen
        );
        assert_eq!(
            parse(&[("ActiveState", "active"), ("FreezerState", "freezing")]).unwrap(),
            LaunchUnitState::FreezerTransition
        );
        assert_eq!(
            parse(&[("ActiveState", "active"), ("FreezerState", "thawing")]).unwrap(),
            LaunchUnitState::FreezerTransition
        );
        assert!(LaunchUnitState::Frozen.needs_thaw_before_stop());
        assert!(LaunchUnitState::FreezerTransition.needs_thaw_before_stop());
        assert!(!LaunchUnitState::Running.needs_thaw_before_stop());
        assert!(!LaunchUnitState::Stopping.needs_thaw_before_stop());
        assert!(!LaunchUnitState::Completed.needs_thaw_before_stop());
        assert_eq!(
            parse(&[("ActiveState", "activating")]).unwrap(),
            LaunchUnitState::Running
        );
        assert_eq!(
            parse(&[("ActiveState", "deactivating"), ("FreezerState", "frozen")]).unwrap(),
            LaunchUnitState::Stopping
        );
        assert_eq!(
            parse(&[("ActiveState", "inactive"), ("FreezerState", "running")]).unwrap(),
            LaunchUnitState::Completed
        );
        let unknown_freezer =
            parse(&[("ActiveState", "active"), ("FreezerState", "melting")]).unwrap_err();
        assert_eq!(unknown_freezer.kind, LaunchUnitErrorKind::Protocol);
        assert!(unknown_freezer.message.contains("FreezerState"));
        assert_eq!(
            parse(&[("ActiveState", "weird")]).unwrap_err().kind,
            LaunchUnitErrorKind::Protocol
        );
        assert_eq!(
            parse(&[("FreezerState", "frozen")]).unwrap_err().kind,
            LaunchUnitErrorKind::Protocol
        );
    }

    #[test]
    fn systemd_freeze_and_thaw_invoke_the_exact_helper_and_surface_failures() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemctl");
        let log = root.path().join("calls.log");
        fs::write(
            &helper,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\ncase \"$3\" in freeze) exit 0;; thaw) echo refused >&2; exit 3;; esac\n",
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let backend = SystemdLaunchUnitBackend::with_timeout(
            helper.clone(),
            helper,
            1000,
            1000,
            Duration::from_secs(2),
        )
        .unwrap();
        let id = "0123456789abcdef0123456789abcdef";

        backend.freeze(id).unwrap();
        let error = backend.thaw(id).unwrap_err();
        assert_eq!(error.kind, LaunchUnitErrorKind::Failed);
        assert!(error.message.contains("refused"));
        assert!(backend.freeze("bad").is_err());
        assert!(backend.thaw("bad").is_err());
        assert_eq!(
            fs::read_to_string(log).unwrap(),
            format!(
                "--system --no-ask-password freeze korri-game-{id}.service\n--system --no-ask-password thaw korri-game-{id}.service\n"
            )
        );
    }

    #[test]
    fn systemd_stop_thaws_a_frozen_unit_first_and_surfaces_a_refused_thaw() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemctl");
        let log = root.path().join("calls.log");
        let freezer = root.path().join("freezer");
        let thaw_refused = root.path().join("thaw-refused");
        // `show` reports the freezer state from a file; `thaw` flips it to
        // running unless refusal is requested; `stop` mirrors systemd 259
        // and refuses a frozen unit.
        fs::write(
            &helper,
            format!(
                concat!(
                    "#!/bin/sh\n",
                    "printf '%s\\n' \"$*\" >> {log}\n",
                    "case \"$3\" in\n",
                    "  show) printf 'LoadState=loaded\\nActiveState=active\\nFreezerState=%s\\n' \"$(cat {freezer})\";;\n",
                    "  thaw) if [ -e {refused} ]; then echo refused >&2; exit 3; fi; printf running > {freezer};;\n",
                    "  stop) if [ \"$(cat {freezer})\" != running ]; then echo 'Cannot perform operation on frozen unit' >&2; exit 1; fi;;\n",
                    "esac\n"
                ),
                log = log.display(),
                freezer = freezer.display(),
                refused = thaw_refused.display(),
            ),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let backend = SystemdLaunchUnitBackend::with_timeout(
            helper.clone(),
            helper,
            1000,
            1000,
            Duration::from_secs(2),
        )
        .unwrap();
        let id = "0123456789abcdef0123456789abcdef";
        let unit = format!("korri-game-{id}.service");
        let show = format!(
            "--system --no-ask-password show {unit} --property=LoadState --property=ActiveState --property=FreezerState\n"
        );

        // Running: no thaw is issued.
        fs::write(&freezer, "running").unwrap();
        backend.stop(id).unwrap();
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            format!("{show}--system --no-ask-password stop {unit}\n")
        );

        // Frozen: thaw, then stop, in that order.
        fs::remove_file(&log).unwrap();
        fs::write(&freezer, "frozen").unwrap();
        backend.stop(id).unwrap();
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            format!(
                "{show}--system --no-ask-password thaw {unit}\n--system --no-ask-password stop {unit}\n"
            )
        );
        assert_eq!(fs::read_to_string(&freezer).unwrap(), "running");

        // Transitional: the thaw is issued as well.
        fs::remove_file(&log).unwrap();
        fs::write(&freezer, "thawing").unwrap();
        backend.stop(id).unwrap();
        assert!(fs::read_to_string(&log)
            .unwrap()
            .contains(&format!("thaw {unit}\n")));

        // A refused thaw is a stop failure; no stop is attempted and the
        // unit stays frozen.
        fs::remove_file(&log).unwrap();
        fs::write(&freezer, "frozen").unwrap();
        fs::write(&thaw_refused, "").unwrap();
        let error = backend.stop(id).unwrap_err();
        assert_eq!(error.kind, LaunchUnitErrorKind::Failed);
        assert!(error.message.contains("thaw before stop failed"));
        assert!(error.message.contains("refused"));
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            format!("{show}--system --no-ask-password thaw {unit}\n")
        );
        assert_eq!(fs::read_to_string(&freezer).unwrap(), "frozen");
    }

    #[test]
    fn systemd_state_query_reads_the_freezer_property() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemctl");
        let log = root.path().join("calls.log");
        fs::write(
            &helper,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\nprintf 'LoadState=loaded\\nActiveState=active\\nFreezerState=frozen\\n'\n",
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let backend = SystemdLaunchUnitBackend::with_timeout(
            helper.clone(),
            helper,
            1000,
            1000,
            Duration::from_secs(2),
        )
        .unwrap();
        let id = "0123456789abcdef0123456789abcdef";
        assert_eq!(backend.state(id).unwrap(), LaunchUnitState::Frozen);
        assert_eq!(
            fs::read_to_string(log).unwrap(),
            format!(
                "--system --no-ask-password show korri-game-{id}.service --property=LoadState --property=ActiveState --property=FreezerState\n"
            )
        );
    }

    #[test]
    fn freeze_and_thaw_are_exact_idempotent_and_keep_input_seats() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let alive = Arc::new(AtomicBool::new(false));
        let manager = Arc::new(TestSeatManager {
            starts: AtomicUsize::new(0),
            alive: alive.clone(),
        });
        let control =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), manager.clone());
        let prepared = prepare(&control, "one");
        let id = prepared.launch_id.clone();
        assert_eq!(manager.starts.load(Ordering::SeqCst), 1);

        assert_eq!(
            control.thaw(&id),
            HostSessionFreezeChange::Unchanged {
                launch_id: id.clone()
            }
        );
        assert!(backend.state.lock().unwrap().thawed.is_empty());
        assert_eq!(
            control.freeze(&id),
            HostSessionFreezeChange::Changed {
                launch_id: id.clone()
            }
        );
        assert_eq!(backend.state.lock().unwrap().frozen, [id.as_str()]);
        assert_eq!(backend.state(&id).unwrap(), LaunchUnitState::Frozen);
        assert_eq!(
            control.status(),
            HostSessionStatus::Frozen {
                launch_id: id.clone(),
                game_id: Some("one".into()),
            }
        );
        assert!(
            alive.load(Ordering::SeqCst),
            "seats must stay alive while frozen"
        );
        assert_eq!(manager.starts.load(Ordering::SeqCst), 1);

        assert_eq!(
            control.freeze(&id),
            HostSessionFreezeChange::Unchanged {
                launch_id: id.clone()
            }
        );
        assert_eq!(backend.state.lock().unwrap().frozen.len(), 1);

        assert_eq!(
            control.thaw(&id),
            HostSessionFreezeChange::Changed {
                launch_id: id.clone()
            }
        );
        assert_eq!(backend.state.lock().unwrap().thawed, [id.as_str()]);
        assert_eq!(
            control.status(),
            HostSessionStatus::Running {
                launch_id: id.clone(),
                game_id: Some("one".into()),
            }
        );
        assert_eq!(
            control.thaw(&id),
            HostSessionFreezeChange::Unchanged { launch_id: id }
        );
        assert_eq!(manager.starts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn freeze_and_thaw_reject_stale_absent_and_blocked_sessions() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let session = control(root.path(), backend.clone());
        let stale = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        assert_eq!(session.freeze(stale), HostSessionFreezeChange::NoActive);
        assert_eq!(session.thaw(stale), HostSessionFreezeChange::NoActive);

        let prepared = prepare(&session, "one");
        assert_eq!(
            session.freeze(stale),
            HostSessionFreezeChange::StaleIdentity {
                active_launch_id: Some(prepared.launch_id.clone())
            }
        );
        assert_eq!(
            session.thaw(stale),
            HostSessionFreezeChange::StaleIdentity {
                active_launch_id: Some(prepared.launch_id.clone())
            }
        );
        assert!(backend.state.lock().unwrap().frozen.is_empty());
        assert!(backend.state.lock().unwrap().thawed.is_empty());

        backend.insert(&prepared.launch_id, LaunchUnitState::Completed);
        assert_eq!(
            session.freeze(&prepared.launch_id),
            HostSessionFreezeChange::NoActive
        );
        assert!(matches!(
            session.status(),
            HostSessionStatus::Completed { .. }
        ));

        let blocked_root = tempfile::tempdir().unwrap();
        let blocked_backend = Arc::new(DeterministicBackend::default());
        blocked_backend.insert(stale, LaunchUnitState::Running);
        blocked_backend.insert("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", LaunchUnitState::Running);
        let blocked = control(blocked_root.path(), blocked_backend.clone());
        assert_eq!(
            blocked.freeze(stale),
            HostSessionFreezeChange::RecoveryBlocked
        );
        assert_eq!(
            blocked.thaw(stale),
            HostSessionFreezeChange::RecoveryBlocked
        );
        assert!(blocked_backend.state.lock().unwrap().frozen.is_empty());
    }

    #[test]
    fn freezer_helper_failure_is_typed_and_leaves_the_unit_running() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let session = control(root.path(), backend.clone());
        let prepared = prepare(&session, "one");
        backend.state.lock().unwrap().freezer_fails = true;

        assert_eq!(
            session.freeze(&prepared.launch_id),
            HostSessionFreezeChange::HelperFailed {
                launch_id: prepared.launch_id.clone(),
                message: "freeze refused".into(),
            }
        );
        assert_eq!(
            session.status(),
            HostSessionStatus::Running {
                launch_id: prepared.launch_id.clone(),
                game_id: Some("one".into()),
            }
        );
        assert!(backend.state.lock().unwrap().stopped.is_empty());
        assert_eq!(
            backend.state(&prepared.launch_id).unwrap(),
            LaunchUnitState::Running
        );

        backend.state.lock().unwrap().freezer_fails = false;
        assert_eq!(
            session.freeze(&prepared.launch_id),
            HostSessionFreezeChange::Changed {
                launch_id: prepared.launch_id.clone()
            }
        );
        assert!(backend.state.lock().unwrap().thawed.is_empty());
        assert_eq!(
            session.stop(&prepared.launch_id),
            HostSessionStop::Completed {
                launch_id: prepared.launch_id.clone()
            }
        );
        // systemd refuses stop on a frozen unit; the backend thaws first.
        assert_eq!(
            backend.state.lock().unwrap().thawed,
            [prepared.launch_id.as_str()]
        );
        assert_eq!(
            backend.state.lock().unwrap().stopped,
            [prepared.launch_id.as_str()]
        );
    }

    #[test]
    fn stop_of_a_frozen_unit_fails_typed_when_the_thaw_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let session = control(root.path(), backend.clone());
        let prepared = prepare(&session, "one");
        assert_eq!(
            session.freeze(&prepared.launch_id),
            HostSessionFreezeChange::Changed {
                launch_id: prepared.launch_id.clone()
            }
        );
        backend.state.lock().unwrap().freezer_fails = true;

        // The thaw before stop is refused, so the stop is a failure and the
        // unit stays frozen. No `systemctl stop` is attempted.
        assert_eq!(
            session.stop(&prepared.launch_id),
            HostSessionStop::RecoveryBlocked
        );
        assert_eq!(
            backend.state.lock().unwrap().thawed,
            [prepared.launch_id.as_str()]
        );
        assert!(backend.state.lock().unwrap().stopped.is_empty());
        assert_eq!(
            backend.state(&prepared.launch_id).unwrap(),
            LaunchUnitState::Frozen
        );
    }

    #[test]
    fn freeze_and_thaw_are_refused_while_an_exact_stop_is_in_flight() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let session = control(root.path(), backend.clone());
        let prepared = prepare(&session, "one");
        let id = prepared.launch_id.clone();
        backend.state.lock().unwrap().block_stop = true;

        let stopper = {
            let session = session.clone();
            let id = id.clone();
            thread::spawn(move || session.stop(&id))
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while backend.state.lock().unwrap().stopped.is_empty() {
            assert!(Instant::now() < deadline, "stop never reached the backend");
            thread::sleep(Duration::from_millis(5));
        }

        // Tracked state is `Stopping`; both verbs are refused without
        // touching the freezer.
        assert_eq!(
            session.freeze(&id),
            HostSessionFreezeChange::Stopping {
                launch_id: id.clone()
            }
        );
        assert_eq!(
            session.thaw(&id),
            HostSessionFreezeChange::Stopping {
                launch_id: id.clone()
            }
        );
        assert!(backend.state.lock().unwrap().frozen.is_empty());
        assert!(backend.state.lock().unwrap().thawed.is_empty());

        backend.release_stop();
        assert_eq!(
            stopper.join().unwrap(),
            HostSessionStop::Completed {
                launch_id: id.clone()
            }
        );
        assert_eq!(session.freeze(&id), HostSessionFreezeChange::NoActive);
        assert!(backend.state.lock().unwrap().frozen.is_empty());
    }

    #[test]
    fn freeze_and_thaw_are_refused_when_the_unit_is_observed_deactivating() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let session = control(root.path(), backend.clone());
        let prepared = prepare(&session, "one");
        let id = prepared.launch_id.clone();

        // Tracked state is `Running`, but the unit is deactivating outside
        // korrid. The observed-unit arm refuses and moves to `Stopping`.
        backend.insert(&id, LaunchUnitState::Stopping);
        assert_eq!(
            session.freeze(&id),
            HostSessionFreezeChange::Stopping {
                launch_id: id.clone()
            }
        );
        assert_eq!(
            session.status(),
            HostSessionStatus::Stopping {
                launch_id: id.clone(),
                game_id: Some("one".into()),
            }
        );
        assert_eq!(
            session.thaw(&id),
            HostSessionFreezeChange::Stopping {
                launch_id: id.clone()
            }
        );
        assert!(backend.state.lock().unwrap().frozen.is_empty());
        assert!(backend.state.lock().unwrap().thawed.is_empty());
    }

    #[test]
    fn transitional_freezer_state_always_issues_the_verb() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let session = control(root.path(), backend.clone());
        let prepared = prepare(&session, "one");
        let id = prepared.launch_id.clone();

        // systemd reports `thawing` at the moment of a freeze request. The
        // settled state is unknown, so korrid must not report Unchanged.
        backend.insert(&id, LaunchUnitState::FreezerTransition);
        assert_eq!(
            session.freeze(&id),
            HostSessionFreezeChange::Changed {
                launch_id: id.clone()
            }
        );
        assert_eq!(backend.state.lock().unwrap().frozen, [id.as_str()]);
        assert_eq!(backend.state(&id).unwrap(), LaunchUnitState::Frozen);

        // And `freezing` at the moment of a thaw request.
        backend.insert(&id, LaunchUnitState::FreezerTransition);
        assert_eq!(
            session.thaw(&id),
            HostSessionFreezeChange::Changed {
                launch_id: id.clone()
            }
        );
        assert_eq!(backend.state.lock().unwrap().thawed, [id.as_str()]);
        assert_eq!(backend.state(&id).unwrap(), LaunchUnitState::Running);

        // Status reports a transitional unit as frozen and does not touch
        // the freezer.
        backend.insert(&id, LaunchUnitState::FreezerTransition);
        assert_eq!(
            session.status(),
            HostSessionStatus::Frozen {
                launch_id: id.clone(),
                game_id: Some("one".into()),
            }
        );
        assert_eq!(backend.state.lock().unwrap().frozen.len(), 1);
        assert_eq!(backend.state.lock().unwrap().thawed.len(), 1);

        // A stop of a transitional unit thaws first, like a frozen one.
        assert_eq!(
            session.stop(&id),
            HostSessionStop::Completed {
                launch_id: id.clone()
            }
        );
        assert_eq!(
            backend.state.lock().unwrap().thawed,
            [id.as_str(), id.as_str()]
        );
    }

    #[test]
    fn prepare_records_a_launch_that_is_observed_frozen_immediately() {
        struct FreezeOnLaunch(DeterministicBackend);
        impl LaunchUnitBackend for FreezeOnLaunch {
            fn launch(
                &self,
                launch_id: &str,
                command: &[String],
                environment: &BTreeMap<String, String>,
            ) -> Result<(), LaunchUnitError> {
                self.0.launch(launch_id, command, environment)?;
                self.0.insert(launch_id, LaunchUnitState::Frozen);
                Ok(())
            }
            fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError> {
                self.0.state(launch_id)
            }
            fn stop(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
                self.0.stop(launch_id)
            }
            fn freeze(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
                self.0.freeze(launch_id)
            }
            fn thaw(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
                self.0.thaw(launch_id)
            }
            fn live_launch_ids(&self) -> Result<Vec<String>, LaunchUnitError> {
                self.0.live_launch_ids()
            }
        }

        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(FreezeOnLaunch(DeterministicBackend::default()));
        let alive = Arc::new(AtomicBool::new(false));
        let manager = Arc::new(TestSeatManager {
            starts: AtomicUsize::new(0),
            alive: alive.clone(),
        });
        let session =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), manager.clone());

        // An operator froze the slice before prepare observed the unit. The
        // launch is real: it is recorded as frozen, seats are kept, and
        // recovery is not blocked.
        let prepared = session
            .prepare("one", None, &["game".into()], &BTreeMap::new())
            .unwrap();
        assert_eq!(
            session.status(),
            HostSessionStatus::Frozen {
                launch_id: prepared.launch_id.clone(),
                game_id: Some("one".into()),
            }
        );
        assert!(alive.load(Ordering::SeqCst));
        assert_eq!(manager.starts.load(Ordering::SeqCst), 1);
        assert!(backend.0.state.lock().unwrap().stopped.is_empty());

        // A thaw resumes it and a stop completes normally.
        assert_eq!(
            session.thaw(&prepared.launch_id),
            HostSessionFreezeChange::Changed {
                launch_id: prepared.launch_id.clone()
            }
        );
        assert_eq!(
            session.stop(&prepared.launch_id),
            HostSessionStop::Completed {
                launch_id: prepared.launch_id.clone()
            }
        );
        assert!(!alive.load(Ordering::SeqCst));
    }

    #[test]
    fn frozen_unit_recovers_frozen_and_stops_from_frozen() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        persist_test_record(root.path(), id);
        backend.insert(id, LaunchUnitState::Frozen);
        let manager = Arc::new(TestSeatManager {
            starts: AtomicUsize::new(0),
            alive: Arc::new(AtomicBool::new(true)),
        });
        let control =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), manager.clone());

        assert_eq!(
            control.status(),
            HostSessionStatus::Frozen {
                launch_id: id.into(),
                game_id: Some("recovered".into()),
            }
        );
        assert_eq!(manager.starts.load(Ordering::SeqCst), 1);
        assert_eq!(
            control
                .prepare("two", None, &["game".into()], &BTreeMap::new())
                .unwrap_err()
                .code,
            "ActiveSessionConflict"
        );
        assert_eq!(
            control.stop(id),
            HostSessionStop::Completed {
                launch_id: id.into()
            }
        );
        // systemd refuses stop on a frozen unit; the backend thaws first,
        // then stops. The session itself never issues a thaw.
        assert_eq!(backend.state.lock().unwrap().thawed, [id]);
        assert_eq!(backend.state.lock().unwrap().stopped, [id]);
        assert_eq!(
            control.status(),
            HostSessionStatus::Completed {
                launch_id: id.into()
            }
        );
    }

    #[test]
    fn seat_failure_recovery_stops_a_frozen_unit_by_thawing_first() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        persist_test_record(root.path(), id);
        backend.insert(id, LaunchUnitState::Frozen);
        let control = HostSessionControl::with_input_seats(
            root.path(),
            backend.clone(),
            Arc::new(FailingSeatManager),
        );

        // Recovery of a frozen unit whose seats cannot be re-acquired stops
        // the game through the backend, which thaws before stopping.
        assert_eq!(
            control.status(),
            HostSessionStatus::Completed {
                launch_id: id.into()
            }
        );
        assert_eq!(backend.state.lock().unwrap().thawed, [id]);
        assert_eq!(backend.state.lock().unwrap().stopped, [id]);
    }

    #[test]
    fn externally_frozen_unit_is_observed_by_status() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let control = control(root.path(), backend.clone());
        let prepared = prepare(&control, "one");
        backend.insert(&prepared.launch_id, LaunchUnitState::Frozen);
        assert_eq!(
            control.status(),
            HostSessionStatus::Frozen {
                launch_id: prepared.launch_id.clone(),
                game_id: Some("one".into()),
            }
        );
        assert_eq!(
            control.freeze(&prepared.launch_id),
            HostSessionFreezeChange::Unchanged {
                launch_id: prepared.launch_id.clone()
            }
        );
        assert!(backend.state.lock().unwrap().frozen.is_empty());
    }

    #[test]
    fn systemd_game_units_hide_configured_recovery_and_control_paths_for_fresh_and_resumed_launches(
    ) {
        let backend = SystemdLaunchUnitBackend::with_protected_paths(
            PathBuf::from("/nix/store/systemd/bin/systemd-run"),
            PathBuf::from("/nix/store/systemd/bin/systemctl"),
            1001,
            1002,
            PathBuf::from("/srv/korri-test/private-recovery"),
            PathBuf::from("/run/korri-test/control/device.sock"),
            PathBuf::from("/run/korri-test/control"),
            PathBuf::from("/home/gameplay/.config/sunshine"),
            PathBuf::from("/run/korri-test/compositor-control"),
        )
        .unwrap();
        let launch = backend
            .launch_arguments(
                "0123456789abcdef0123456789abcdef",
                &["/games/retroarch".into()],
                &BTreeMap::new(),
            )
            .unwrap();
        assert!(launch.contains(
            &"--property=InaccessiblePaths=/srv/korri-test/private-recovery /run/korrid /run/korri-test/control/device.sock /run/korri-test/control /home/gameplay/.config/sunshine /run/korri-test/compositor-control /run/korri-certificate-control /run/user/1001 -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources".into()
        ));
    }

    #[test]
    fn systemd_backend_rejects_root_credentials_and_relative_helpers() {
        assert!(matches!(
            SystemdLaunchUnitBackend::new(
                PathBuf::from("systemd-run"),
                PathBuf::from("/bin/systemctl"),
                1000,
                1000,
            ),
            Err(LaunchUnitError {
                kind: LaunchUnitErrorKind::InvalidConfiguration,
                ..
            })
        ));
        assert!(SystemdLaunchUnitBackend::new(
            PathBuf::from("/bin/systemd-run"),
            PathBuf::from("/bin/systemctl"),
            0,
            1000,
        )
        .is_err());
        assert!(SystemdLaunchUnitBackend::new(
            PathBuf::from("/bin/systemd-run"),
            PathBuf::from("/bin/systemctl"),
            1000,
            0,
        )
        .is_err());
        assert!(SystemdLaunchUnitBackend::with_protected_paths(
            PathBuf::from("/bin/systemd-run"),
            PathBuf::from("/bin/systemctl"),
            1000,
            1000,
            PathBuf::from("relative/recovery"),
            PathBuf::from("/run/control.sock"),
            PathBuf::from("/run"),
            PathBuf::from("/home/gameplay/.config/sunshine"),
            PathBuf::from("/run/korri-compositor"),
        )
        .is_err());
        assert!(SystemdLaunchUnitBackend::with_protected_paths(
            PathBuf::from("/bin/systemd-run"),
            PathBuf::from("/bin/systemctl"),
            1000,
            1000,
            PathBuf::from("/private/recovery"),
            PathBuf::from("/run/other/control.sock"),
            PathBuf::from("/run/control"),
            PathBuf::from("/home/gameplay/.config/sunshine"),
            PathBuf::from("/run/korri-compositor"),
        )
        .is_err());
        assert!(SystemdLaunchUnitBackend::with_protected_paths(
            PathBuf::from("/bin/systemd-run"),
            PathBuf::from("/bin/systemctl"),
            1000,
            1000,
            PathBuf::from("/private/../recovery"),
            PathBuf::from("/run/control/device.sock"),
            PathBuf::from("/run/control"),
            PathBuf::from("/home/gameplay/.config/sunshine"),
            PathBuf::from("/run/korri-compositor"),
        )
        .is_err());
        assert!(SystemdLaunchUnitBackend::with_protected_paths(
            PathBuf::from("/bin/systemd-run"),
            PathBuf::from("/bin/systemctl"),
            1000,
            1000,
            PathBuf::from("/private/recovery"),
            PathBuf::from("/run/control/device.sock"),
            PathBuf::from("/run/control"),
            PathBuf::from("/home/gameplay/.config/sunshine"),
            PathBuf::from("relative/compositor-control"),
        )
        .is_err());
    }

    #[test]
    fn noisy_systemd_helper_is_killed_reaped_and_returns_tagged_output_limit() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemd-helper");
        let pid_file = root.path().join("helper.pid");
        fs::write(
            &helper,
            format!(
                "#!/bin/sh\nprintf '%s' \"$$\" > {}\nexec yes noisy\n",
                pid_file.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let backend = SystemdLaunchUnitBackend::with_timeout(
            helper.clone(),
            helper,
            1000,
            1000,
            Duration::from_secs(2),
        )
        .unwrap();

        let started = Instant::now();
        let error = backend
            .run(&backend.systemctl, &["ignored".into()])
            .unwrap_err();

        assert_eq!(error.kind, LaunchUnitErrorKind::OutputLimit);
        assert!(error.message.contains("65536 bytes"));
        assert!(started.elapsed() < Duration::from_secs(2));
        let pid: i32 = fs::read_to_string(pid_file).unwrap().parse().unwrap();
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }

    #[test]
    fn hanging_systemd_helper_is_killed_reaped_and_returns_tagged_timeout() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemd-helper");
        let pid_file = root.path().join("helper.pid");
        fs::write(
            &helper,
            format!(
                "#!/bin/sh\nprintf '%s' \"$$\" > {}\nexec sleep 30\n",
                pid_file.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let backend = SystemdLaunchUnitBackend::with_timeout(
            helper.clone(),
            helper,
            1000,
            1000,
            Duration::from_millis(30),
        )
        .unwrap();

        let started = Instant::now();
        let error = backend
            .run(&backend.systemctl, &["ignored".into()])
            .unwrap_err();

        assert_eq!(error.kind, LaunchUnitErrorKind::Timeout);
        assert!(started.elapsed() < Duration::from_secs(2));
        let pid: i32 = fs::read_to_string(pid_file).unwrap().parse().unwrap();
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }

    #[test]
    fn recovery_reacquires_input_seats_and_replaces_a_dead_lease() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        persist_test_record(root.path(), id);
        backend.insert(id, LaunchUnitState::Running);
        let alive = Arc::new(AtomicBool::new(true));
        let manager = Arc::new(TestSeatManager {
            starts: AtomicUsize::new(0),
            alive: alive.clone(),
        });
        let control = HostSessionControl::with_input_seats(root.path(), backend, manager.clone());
        assert_eq!(manager.starts.load(Ordering::SeqCst), 0);
        assert_eq!(
            control.status(),
            HostSessionStatus::Running {
                launch_id: id.into(),
                game_id: Some("recovered".into()),
            }
        );
        alive.store(false, Ordering::SeqCst);
        assert_eq!(
            control.status(),
            HostSessionStatus::Running {
                launch_id: id.into(),
                game_id: Some("recovered".into()),
            }
        );
        assert_eq!(manager.starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn failed_seat_recovery_stops_the_known_game_without_losing_stop_identity() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        persist_test_record(root.path(), id);
        backend.insert(id, LaunchUnitState::Running);
        let control = HostSessionControl::with_input_seats(
            root.path(),
            backend.clone(),
            Arc::new(FailingSeatManager),
        );

        assert_eq!(
            control.status(),
            HostSessionStatus::Completed {
                launch_id: id.into()
            }
        );
        assert_eq!(backend.state.lock().unwrap().stopped, [id]);
        assert_eq!(
            control.stop(id),
            HostSessionStop::Completed {
                launch_id: id.into()
            }
        );
    }

    #[test]
    fn stopping_recovery_does_not_recreate_input_seats() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        persist_test_record(root.path(), id);
        backend.insert(id, LaunchUnitState::Stopping);
        let manager = Arc::new(TestSeatManager {
            starts: AtomicUsize::new(0),
            alive: Arc::new(AtomicBool::new(true)),
        });
        let control = HostSessionControl::with_input_seats(root.path(), backend, manager.clone());

        assert_eq!(
            control.status(),
            HostSessionStatus::Stopping {
                launch_id: id.into(),
                game_id: Some("recovered".into()),
            }
        );
        assert_eq!(manager.starts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn ambiguous_recovery_does_not_create_input_seats() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        backend.insert("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", LaunchUnitState::Running);
        backend.insert("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", LaunchUnitState::Running);
        let manager = Arc::new(TestSeatManager {
            starts: AtomicUsize::new(0),
            alive: Arc::new(AtomicBool::new(true)),
        });
        let control = HostSessionControl::with_input_seats(root.path(), backend, manager.clone());
        assert_eq!(manager.starts.load(Ordering::SeqCst), 0);
        assert_eq!(control.status(), HostSessionStatus::RecoveryBlocked);
        assert_eq!(manager.starts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn natural_completion_records_one_play_on_first_proof_only() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let clock = TestClock::at(1_700_000_000);
        let control = control_with_clock(root.path(), backend.clone(), clock.clone());
        let prepared = control
            .prepare("wario", Some(PERSON), &["game".into()], &BTreeMap::new())
            .unwrap();
        clock.advance(42);
        backend.insert(&prepared.launch_id, LaunchUnitState::Completed);

        assert!(matches!(
            control.status(),
            HostSessionStatus::Completed { .. }
        ));
        assert!(matches!(
            control.status(),
            HostSessionStatus::Completed { .. }
        ));
        assert_eq!(
            stats(&control, "wario"),
            crate::PlayStats {
                last_played: Some("2023-11-14T22:14:02.000Z".into()),
                play_count: 1,
                total_playtime_seconds: 42.0,
            }
        );
    }

    #[test]
    fn exact_stop_records_once_and_frozen_time_counts() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let clock = TestClock::at(1_700_000_000);
        let control = control_with_clock(root.path(), backend, clock.clone());
        let prepared = control
            .prepare("wario", Some(PERSON), &["game".into()], &BTreeMap::new())
            .unwrap();
        clock.advance(5);
        assert!(matches!(
            control.freeze(&prepared.launch_id),
            HostSessionFreezeChange::Changed { .. }
        ));
        clock.advance(55);
        assert!(matches!(
            control.stop(&prepared.launch_id),
            HostSessionStop::Completed { .. }
        ));
        assert!(matches!(
            control.stop(&prepared.launch_id),
            HostSessionStop::Completed { .. }
        ));
        assert_eq!(
            stats(&control, "wario"),
            crate::PlayStats {
                last_played: Some("2023-11-14T22:14:20.000Z".into()),
                play_count: 1,
                total_playtime_seconds: 60.0,
            }
        );
    }

    #[test]
    fn restart_completion_uses_first_observation_time_once() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        persist_test_record(root.path(), id);
        backend.insert(id, LaunchUnitState::Completed);
        let clock = TestClock::at(1_700_000_120);
        let recovered = control_with_clock(root.path(), backend, clock.clone());

        assert_eq!(recovered.status(), HostSessionStatus::NoActive);
        clock.advance(60);
        assert_eq!(recovered.status(), HostSessionStatus::NoActive);
        assert_eq!(
            stats(&recovered, "recovered"),
            crate::PlayStats {
                last_played: Some("2023-11-14T22:15:20.000Z".into()),
                play_count: 1,
                total_playtime_seconds: 120.0,
            }
        );
    }

    #[test]
    fn play_log_failure_restores_active_metadata_for_one_retry() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let clock = TestClock::at(1_700_000_000);
        let control = control_with_clock(root.path(), backend.clone(), clock.clone());
        let prepared = control
            .prepare("wario", Some(PERSON), &["game".into()], &BTreeMap::new())
            .unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), root.path().join("play-log")).unwrap();
        clock.advance(10);
        backend.insert(&prepared.launch_id, LaunchUnitState::Completed);

        assert_eq!(control.status(), HostSessionStatus::RecoveryBlocked);
        assert!(root.path().join("host-session").join(ACTIVE_FILE).exists());
        fs::remove_file(root.path().join("play-log")).unwrap();
        assert_eq!(control.status(), HostSessionStatus::NoActive);
        assert_eq!(stats(&control, "wario").play_count, 1);
    }

    #[test]
    fn failed_launch_discards_active_metadata_without_a_play() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        backend.state.lock().unwrap().launch_rejected = true;
        let clock = TestClock::at(1_700_000_000);
        let control = control_with_clock(root.path(), backend, clock);

        assert_eq!(
            control
                .prepare("wario", Some(PERSON), &["game".into()], &BTreeMap::new())
                .unwrap_err()
                .code,
            "HostLaunchFailed"
        );
        assert_eq!(
            stats(&control, "wario"),
            crate::PlayStats {
                last_played: None,
                play_count: 0,
                total_playtime_seconds: 0.0,
            }
        );
        assert!(!root.path().join("host-session").join(ACTIVE_FILE).exists());
    }

    #[test]
    fn unsupported_existing_play_log_keeps_completion_pending() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("host-session");
        let backend = Arc::new(DeterministicBackend::default());
        let pending = ActiveSession::CompletionPending {
            launch_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            game_id: "wario".into(),
            person_public_key: PERSON.into(),
            started_at: 1_700_000_000,
            entry: PlayEntry {
                occurred_at: "2023-11-14T22:14:02.000Z".into(),
                duration_seconds: 42.0,
                release_id: None,
            },
        };
        persist_active(&state, &pending).unwrap();
        let path = root.path().join("play-log").join(PERSON).join("wario.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for directory in [
            root.path().join("play-log"),
            path.parent().unwrap().to_path_buf(),
        ] {
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::write(
            &path,
            format!(
                r#"{{"userId":"{PERSON}","gameId":"wario","entries":[{{"occurredAt":"2026-07-07","durationSeconds":5}}]}}"#
            ),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let before = fs::read(&path).unwrap();

        let recovered = control_with_clock(root.path(), backend, TestClock::at(1_700_000_100));
        assert_eq!(recovered.status(), HostSessionStatus::RecoveryBlocked);
        assert_eq!(read_active(&state).unwrap(), Some(pending));
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn completion_journal_replays_each_crash_boundary_exactly_once() {
        let pending_entry = PlayEntry {
            occurred_at: "2023-11-14T22:14:02.000Z".into(),
            duration_seconds: 42.0,
            release_id: None,
        };
        for append_before_restart in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let state = root.path().join("host-session");
            let backend = Arc::new(DeterministicBackend::default());
            let launch_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            let key = PlayHistoryKey {
                user_id: PERSON.into(),
                game_id: "wario".into(),
            };
            persist_active(
                &state,
                &ActiveSession::CompletionPending {
                    launch_id: launch_id.into(),
                    game_id: "wario".into(),
                    person_public_key: PERSON.into(),
                    started_at: 1_700_000_000,
                    entry: pending_entry.clone(),
                },
            )
            .unwrap();
            if append_before_restart {
                PlayLogStore::new(root.path())
                    .record(&key, pending_entry.clone())
                    .unwrap();
            }
            let recovered = control_with_clock(root.path(), backend, TestClock::at(1_700_000_100));
            assert_eq!(recovered.status(), HostSessionStatus::NoActive);
            assert_eq!(
                recovered.play_log().load(&key).unwrap().entries,
                std::slice::from_ref(&pending_entry)
            );
            assert_eq!(read_active(&state).unwrap(), None);
            assert_eq!(recovered.status(), HostSessionStatus::NoActive);
            assert_eq!(recovered.play_log().load(&key).unwrap().entries.len(), 1);
        }
    }

    #[test]
    fn play_log_path_overflow_fails_before_seats_or_unit_start() {
        for game_id in ["a".repeat(251), "é".repeat(42)] {
            let root = tempfile::tempdir().unwrap();
            let backend = Arc::new(DeterministicBackend::default());
            let control =
                control_with_clock(root.path(), backend.clone(), TestClock::at(1_700_000_000));
            let failure = control
                .prepare(&game_id, Some(PERSON), &["game".into()], &BTreeMap::new())
                .unwrap_err();
            assert_eq!(failure.code, "PlayLogPathUnavailable");
            assert!(backend.state.lock().unwrap().units.is_empty());
            assert!(!root.path().join("host-session").exists());
        }
    }
}
