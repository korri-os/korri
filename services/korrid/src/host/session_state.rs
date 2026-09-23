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

use super::compositor_focus::{
    focus_launch_window, focused_ownership, CompositorControl, FocusOutcome, FocusOwnership,
};
use super::identity::{
    clear_active, clear_crash_temporary_active, consume_active, persist_active, read_active,
    replace_active, ActiveSession,
};
#[cfg(test)]
use super::input_seat::DisabledInputSeats;
use super::input_seat::{InputSeatLease, InputSeatManager};
use super::play_log::{PlayHistoryKey, PlayLogStore};
#[cfg(test)]
use super::systemd_unit::{
    read_unit_pids, LaunchUnitError, LaunchUnitErrorKind, SystemdLaunchUnitBackend,
};
use super::systemd_unit::{LaunchUnitBackend, LaunchUnitState, RUNNER_ID_ENV};

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
    FocusFailed {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HostSessionEffectFailure {
    NoActive,
    StaleIdentity,
    Stopping,
    RecoveryBlocked,
    FocusFailed(String),
    Unavailable(String),
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
    /// The exact unit is running, but its window could not be raised.
    FocusFailed {
        launch_id: String,
        message: String,
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
    FocusFailed {
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
    /// One startup-only exact handoff reconstructed from live compositor facts.
    /// Browser status consumes it into the server's process-local overlay intent.
    recovered_overlay_intent: Arc<Mutex<Option<String>>>,
    /// Required to prove that an exact launch owns focus before Game input can
    /// return. Absence is a product-path focus failure, never implicit success.
    compositor: Option<Arc<dyn CompositorControl>>,
    /// Surfaces that must never be focused as a game, such as the kiosk hub.
    never_focus: Vec<String>,
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
            recovered_overlay_intent: Arc::new(Mutex::new(None)),
            compositor: None,
            never_focus: Vec::new(),
        }
    }

    /// Give this session control the authority needed to prove that a resumed
    /// game returned to the front.
    pub(crate) fn with_compositor(
        mut self,
        compositor: Arc<dyn CompositorControl>,
        never_focus: Vec<String>,
    ) -> Self {
        self.compositor = Some(compositor);
        self.never_focus = never_focus;
        self
    }

    fn current_focus_ownership(&self, launch_id: &str) -> Result<FocusOwnership, String> {
        let compositor = self
            .compositor
            .as_ref()
            .ok_or("compositor focus authority is not configured")?;
        let pids = self
            .backend
            .window_pids(launch_id)
            .map_err(|error| error.message)?;
        let tree = compositor.tree()?;
        let excluded: Vec<&str> = self.never_focus.iter().map(String::as_str).collect();
        focused_ownership(&tree, &pids, &excluded)
    }

    fn recovered_running_state(&self, launch_id: String, game_id: Option<String>) -> ActiveState {
        let ownership = self.current_focus_ownership(&launch_id);
        let mut recovered_intent = self
            .recovered_overlay_intent
            .lock()
            .expect("recovered overlay intent mutex poisoned");
        match ownership {
            Ok(FocusOwnership::Launch) => {
                recovered_intent.take();
                ActiveState::Running { launch_id, game_id }
            }
            // Only an excluded Korri surface proves a restart interrupted an
            // overlay handoff. Reconstruct that exact intent in memory.
            Ok(FocusOwnership::Excluded) => {
                *recovered_intent = Some(launch_id.clone());
                ActiveState::FocusFailed { launch_id, game_id }
            }
            // Ambiguous, missing, unreadable, or unrelated focus still fails
            // closed to Portal but cannot manufacture an overlay intent.
            Ok(FocusOwnership::Other) | Err(_) => {
                recovered_intent.take();
                ActiveState::FocusFailed { launch_id, game_id }
            }
        }
    }

    pub(crate) fn status_with_recovered_overlay_intent(
        &self,
    ) -> (HostSessionStatus, Option<String>) {
        let status = self.status();
        let expected = match &status {
            HostSessionStatus::FocusFailed { launch_id, .. } => Some(launch_id.as_str()),
            _ => None,
        };
        let mut recovered_intent = self
            .recovered_overlay_intent
            .lock()
            .expect("recovered overlay intent mutex poisoned");
        let Some(expected) = expected else {
            recovered_intent.take();
            return (status, None);
        };
        if recovered_intent.as_deref() != Some(expected) {
            return (status, None);
        }
        let recovered = matches!(
            self.current_focus_ownership(expected),
            Ok(FocusOwnership::Excluded)
        )
        .then(|| expected.to_owned());
        recovered_intent.take();
        (status, recovered)
    }

    /// Raise the window of the exact launch that was just resumed. A missing
    /// compositor is distinct from a successful focus and fails closed.
    fn focus_launch(&self, launch_id: &str) -> Option<FocusOutcome> {
        let compositor = self.compositor.as_ref()?;
        let pids = match self.backend.window_pids(launch_id) {
            Ok(pids) => pids,
            Err(error) => return Some(FocusOutcome::Failed(error.message)),
        };
        let excluded: Vec<&str> = self.never_focus.iter().map(String::as_str).collect();
        Some(focus_launch_window(compositor.as_ref(), &pids, &excluded))
    }

    fn focus_failure(outcome: Option<FocusOutcome>) -> Option<String> {
        match outcome {
            None => Some("compositor focus authority is not configured".into()),
            Some(FocusOutcome::Focused(_)) => None,
            Some(FocusOutcome::NothingToFocus) => {
                Some("no unique exact-launch window could be focused".into())
            }
            Some(FocusOutcome::Failed(message)) => Some(message),
        }
    }

    pub(crate) fn play_log(&self) -> &PlayLogStore {
        &self.play_log
    }

    pub(crate) fn active_runner_id(
        &self,
        expected_launch_id: &str,
    ) -> Result<Option<String>, String> {
        let mut state = self.state.lock().expect("host session mutex poisoned");
        self.refresh_recovery(&mut state);
        let active_launch_id = match &*state {
            ActiveState::Running { launch_id, .. }
            | ActiveState::Frozen { launch_id, .. }
            | ActiveState::FocusFailed { launch_id, .. } => launch_id,
            _ => return Ok(None),
        };
        if active_launch_id != expected_launch_id {
            return Ok(None);
        }
        let record = read_active(&self.identity_root)?;
        if !record.is_some_and(|record| record.launch_id() == expected_launch_id) {
            return Ok(None);
        }
        self.backend
            .runner_id(expected_launch_id)
            .map_err(|error| error.message)
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
                ..
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

    fn reset_seats(&self, launch_id: &str) -> Result<(), String> {
        let current = self.seat_lease.lock().expect("input-seat mutex poisoned");
        match current.as_ref() {
            Some((active, lease)) if active == launch_id && lease.alive() => lease.reset(launch_id),
            Some((active, _lease)) if active != launch_id => {
                Err("input-seat lease belongs to a different launch".into())
            }
            Some((_active, _lease)) => Err("input-seat lease is not live".into()),
            None => Err("input-seat lease is missing".into()),
        }
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

    fn record_focus_failure(
        &self,
        state: &mut ActiveState,
        launch_id: String,
        game_id: Option<String>,
    ) -> Result<(), ()> {
        // The game keeps running after a compositor refusal. Revoke only the
        // launch-scoped streamed input seat; never use the unit freezer as an
        // input-ownership mechanism.
        if self.stop_seats(&launch_id).is_err() {
            *state = ActiveState::RecoveryBlocked;
            return Err(());
        }
        *state = ActiveState::FocusFailed { launch_id, game_id };
        Ok(())
    }

    fn seats_are_live_for(&self, launch_id: &str) -> bool {
        self.seat_lease
            .lock()
            .expect("input-seat mutex poisoned")
            .as_ref()
            .is_some_and(|(active, lease)| active == launch_id && lease.alive())
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
        configured_command: Result<&[String], RpcFailure>,
        environment: &BTreeMap<String, String>,
    ) -> Result<SessionPrepared, RpcFailure> {
        self.prepare_inner(
            game_id,
            person_public_key,
            configured_command,
            environment,
            true,
            None,
        )
    }

    /// An explicit route must start fresh. Its runner identity is attached to
    /// the live transient unit for runtime discovery; the durable session
    /// journal keeps its established schema.
    pub fn prepare_fresh_route(
        &self,
        game_id: &str,
        runner_id: &str,
        person_public_key: Option<&str>,
        configured_command: &[String],
        environment: &BTreeMap<String, String>,
    ) -> Result<SessionPrepared, RpcFailure> {
        self.prepare_inner(
            game_id,
            person_public_key,
            Ok(configured_command),
            environment,
            false,
            Some(runner_id),
        )
    }

    fn prepare_inner(
        &self,
        game_id: &str,
        person_public_key: Option<&str>,
        configured_command: Result<&[String], RpcFailure>,
        environment: &BTreeMap<String, String>,
        resume_same_game: bool,
        runner_id: Option<&str>,
    ) -> Result<SessionPrepared, RpcFailure> {
        let mut state = self.state.lock().expect("host session mutex poisoned");
        self.refresh_recovery(&mut state);
        match &*state {
            ActiveState::Running {
                launch_id,
                game_id: Some(active_game_id),
            } if resume_same_game && active_game_id == game_id => {
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
            | ActiveState::FocusFailed { .. }
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
        // Stored route choices apply to new launches, not the running game.
        // Resolve failures only after resume and conflicts under the same lock.
        let configured_command = configured_command?;
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
        let active = ActiveSession::running(
            launch_id.clone(),
            game_id.into(),
            person_public_key.map(str::to_owned),
            self.clock.now_epoch_seconds(),
        );
        persist_active(&self.identity_root, &active).map_err(|message| {
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
        let mut launch_environment = environment.clone();
        if let Some(runner_id) = runner_id {
            launch_environment.insert(RUNNER_ID_ENV.into(), runner_id.into());
        }
        if let Err(error) = self
            .backend
            .launch(&launch_id, configured_command, &launch_environment)
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
            | ActiveState::FocusFailed { launch_id, .. }
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
                    if matches!(&*state, ActiveState::FocusFailed { .. })
                        && observed == LaunchUnitState::Running
                    {
                        // Preserve the compositor failure and keep the streamed
                        // seat revoked. Status observation never freezes or thaws
                        // a focus-failed game.
                    } else if self.ensure_seats(&launch_id).is_err() {
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

    pub(crate) fn invoke_running_effect<F>(
        &self,
        expected_launch_id: &str,
        focus_after: bool,
        wait_for_completion: bool,
        effect: F,
    ) -> Result<(), HostSessionEffectFailure>
    where
        F: FnOnce() -> Result<(), String>,
    {
        let mut state = self.state.lock().expect("host session mutex poisoned");
        self.refresh_recovery(&mut state);
        let (launch_id, game_id, recorded_frozen, portal_owned) = match &*state {
            ActiveState::Running { launch_id, game_id } if launch_id == expected_launch_id => {
                (launch_id.clone(), game_id.clone(), false, false)
            }
            ActiveState::Frozen { launch_id, game_id } if launch_id == expected_launch_id => {
                (launch_id.clone(), game_id.clone(), true, false)
            }
            ActiveState::FocusFailed { launch_id, game_id } if launch_id == expected_launch_id => {
                (launch_id.clone(), game_id.clone(), false, true)
            }
            ActiveState::Running { .. }
            | ActiveState::Frozen { .. }
            | ActiveState::FocusFailed { .. } => {
                return Err(HostSessionEffectFailure::StaleIdentity)
            }
            ActiveState::Stopping { launch_id, .. } if launch_id == expected_launch_id => {
                return Err(HostSessionEffectFailure::Stopping)
            }
            ActiveState::Stopping { .. } => return Err(HostSessionEffectFailure::StaleIdentity),
            ActiveState::Completed { .. } | ActiveState::NoActive => {
                return Err(HostSessionEffectFailure::NoActive)
            }
            ActiveState::RecoveryPending | ActiveState::RecoveryBlocked => {
                return Err(HostSessionEffectFailure::RecoveryBlocked)
            }
        };
        let observed = self
            .backend
            .state(&launch_id)
            .map_err(|_| HostSessionEffectFailure::RecoveryBlocked)?;
        let observed_frozen = matches!(
            observed,
            LaunchUnitState::Frozen | LaunchUnitState::FreezerTransition
        );
        match observed {
            LaunchUnitState::Running
            | LaunchUnitState::Frozen
            | LaunchUnitState::FreezerTransition => {}
            LaunchUnitState::Stopping => {
                *state = ActiveState::Stopping { launch_id, game_id };
                return Err(HostSessionEffectFailure::Stopping);
            }
            LaunchUnitState::Completed => {
                let _ = self.stop_seats(&launch_id);
                let _ = self.complete_active();
                *state = ActiveState::Completed { launch_id };
                return Err(HostSessionEffectFailure::NoActive);
            }
        }
        let restore_frozen = recorded_frozen || observed_frozen;
        if observed_frozen {
            self.backend
                .thaw(&launch_id)
                .map_err(|error| HostSessionEffectFailure::Unavailable(error.message))?;
        }
        *state = ActiveState::Running {
            launch_id: launch_id.clone(),
            game_id: game_id.clone(),
        };
        let seats_were_live = self.seats_are_live_for(&launch_id);
        if let Err(message) = self.ensure_seats(&launch_id) {
            if portal_owned {
                let _ = self.stop_seats(&launch_id);
                *state = ActiveState::FocusFailed { launch_id, game_id };
            } else if restore_frozen && self.backend.freeze(&launch_id).is_ok() {
                *state = ActiveState::Frozen { launch_id, game_id };
            }
            return Err(HostSessionEffectFailure::Unavailable(message));
        }
        if let Err(message) = effect() {
            if !seats_were_live || portal_owned {
                let _ = self.stop_seats(&launch_id);
            }
            if portal_owned {
                *state = ActiveState::FocusFailed { launch_id, game_id };
            } else if restore_frozen {
                if self.backend.freeze(&launch_id).is_err() {
                    *state = ActiveState::RecoveryBlocked;
                    return Err(HostSessionEffectFailure::RecoveryBlocked);
                }
                *state = ActiveState::Frozen { launch_id, game_id };
            }
            return Err(HostSessionEffectFailure::Unavailable(message));
        }
        if wait_for_completion {
            return self.wait_for_effect_completion(
                &mut state,
                launch_id,
                game_id,
                restore_frozen,
                portal_owned,
                seats_were_live,
            );
        }
        if restore_frozen {
            if self.backend.freeze(&launch_id).is_err() {
                *state = ActiveState::RecoveryBlocked;
                return Err(HostSessionEffectFailure::RecoveryBlocked);
            }
            *state = ActiveState::Frozen { launch_id, game_id };
            return Ok(());
        }
        if focus_after {
            if let Some(message) = Self::focus_failure(self.focus_launch(&launch_id)) {
                if self
                    .record_focus_failure(&mut state, launch_id, game_id)
                    .is_err()
                {
                    return Err(HostSessionEffectFailure::RecoveryBlocked);
                }
                return Err(HostSessionEffectFailure::FocusFailed(message));
            }
        }
        Ok(())
    }

    fn wait_for_effect_completion(
        &self,
        state: &mut ActiveState,
        launch_id: String,
        game_id: Option<String>,
        restore_frozen: bool,
        portal_owned: bool,
        seats_were_live: bool,
    ) -> Result<(), HostSessionEffectFailure> {
        const ATTEMPTS: usize = 40;
        const INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

        for attempt in 0..ATTEMPTS {
            match self.backend.state(&launch_id) {
                Ok(LaunchUnitState::Completed) => {
                    if self.stop_seats(&launch_id).is_err() || self.complete_active().is_err() {
                        *state = ActiveState::RecoveryBlocked;
                        return Err(HostSessionEffectFailure::RecoveryBlocked);
                    }
                    *state = ActiveState::Completed { launch_id };
                    return Ok(());
                }
                Ok(LaunchUnitState::Stopping) => {
                    if self.stop_seats(&launch_id).is_err() {
                        *state = ActiveState::RecoveryBlocked;
                        return Err(HostSessionEffectFailure::RecoveryBlocked);
                    }
                    *state = ActiveState::Stopping {
                        launch_id: launch_id.clone(),
                        game_id: game_id.clone(),
                    };
                }
                Ok(
                    LaunchUnitState::Running
                    | LaunchUnitState::Frozen
                    | LaunchUnitState::FreezerTransition,
                ) => {}
                Err(_) => {
                    *state = ActiveState::RecoveryBlocked;
                    return Err(HostSessionEffectFailure::RecoveryBlocked);
                }
            }
            if attempt + 1 < ATTEMPTS {
                std::thread::sleep(INTERVAL);
            }
        }

        if matches!(&*state, ActiveState::Stopping { .. }) {
            return Err(HostSessionEffectFailure::Unavailable(
                "The game did not finish stopping.".into(),
            ));
        }
        if portal_owned {
            if self.stop_seats(&launch_id).is_err() {
                *state = ActiveState::RecoveryBlocked;
                return Err(HostSessionEffectFailure::RecoveryBlocked);
            }
            *state = ActiveState::FocusFailed { launch_id, game_id };
        } else if restore_frozen {
            if self.backend.freeze(&launch_id).is_err()
                || (!seats_were_live && self.stop_seats(&launch_id).is_err())
            {
                *state = ActiveState::RecoveryBlocked;
                return Err(HostSessionEffectFailure::RecoveryBlocked);
            }
            *state = ActiveState::Frozen { launch_id, game_id };
        } else {
            *state = ActiveState::Running { launch_id, game_id };
        }
        Err(HostSessionEffectFailure::Unavailable(
            "RetroArch did not complete the quit request.".into(),
        ))
    }

    /// Moves the exact active launch to the requested freezer state. The
    /// session mutex is held across the helper call so an exact stop cannot
    /// interleave with a freezer change. Ordinary freezes keep input-seat
    /// leases alive. A focus failure leaves the unit running and revokes only
    /// its streamed input-seat lease.
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
            | ActiveState::FocusFailed { launch_id, game_id }
                if launch_id == expected_launch_id =>
            {
                (launch_id.clone(), game_id.clone())
            }
            ActiveState::Running { launch_id, .. }
            | ActiveState::Frozen { launch_id, .. }
            | ActiveState::FocusFailed { launch_id, .. }
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
            ActiveState::Running { .. }
            | ActiveState::Frozen { .. }
            | ActiveState::FocusFailed { .. } => unreachable!(),
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
                game_id: game_id.clone(),
            },
            FreezerTarget::Running => ActiveState::Running {
                launch_id: launch_id.clone(),
                game_id: game_id.clone(),
            },
        };
        // Returning to a game restores the exact launch's streamed input-seat
        // lease before the game can take focus.
        if target == FreezerTarget::Running {
            if let Err(message) = self.ensure_seats(&launch_id) {
                if self
                    .record_focus_failure(&mut state, launch_id.clone(), game_id.clone())
                    .is_err()
                {
                    return HostSessionFreezeChange::RecoveryBlocked;
                }
                return HostSessionFreezeChange::FocusFailed {
                    launch_id,
                    message: format!("input seats could not return to the game: {message}"),
                };
            }
            if let Err(message) = self.reset_seats(&launch_id) {
                if self
                    .record_focus_failure(&mut state, launch_id.clone(), game_id.clone())
                    .is_err()
                {
                    return HostSessionFreezeChange::RecoveryBlocked;
                }
                return HostSessionFreezeChange::FocusFailed {
                    launch_id,
                    message: format!("input seats could not reset for the game: {message}"),
                };
            }
            if let Some(message) = Self::focus_failure(self.focus_launch(&launch_id)) {
                if self
                    .record_focus_failure(&mut state, launch_id.clone(), game_id)
                    .is_err()
                {
                    return HostSessionFreezeChange::RecoveryBlocked;
                }
                return HostSessionFreezeChange::FocusFailed { launch_id, message };
            }
        }
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
                | ActiveState::FocusFailed { launch_id, game_id }
                    if launch_id == expected_launch_id =>
                {
                    let launch_id = launch_id.clone();
                    *state = ActiveState::Stopping {
                        launch_id: launch_id.clone(),
                        game_id: game_id.clone(),
                    };
                    launch_id
                }
                ActiveState::Running { launch_id, .. }
                | ActiveState::Frozen { launch_id, .. }
                | ActiveState::FocusFailed { launch_id, .. } => {
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
        | ActiveState::FocusFailed { game_id, .. }
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
        ActiveState::FocusFailed { launch_id, game_id } => HostSessionStatus::FocusFailed {
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
                    Ok(LaunchUnitState::Running) => self.recovered_running_state(
                        record.launch_id().to_owned(),
                        Some(record.game_id().to_owned()),
                    ),
                    Ok(
                        observed @ (LaunchUnitState::Frozen | LaunchUnitState::FreezerTransition),
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

    use std::collections::BTreeSet;

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
        window_pids: BTreeMap<String, BTreeSet<i32>>,
        window_pids_fail: bool,
        runners: BTreeMap<String, String>,
        return_events: Option<Arc<Mutex<Vec<&'static str>>>>,
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

        fn set_pids(&self, id: &str, pids: &[i32]) {
            self.state
                .lock()
                .unwrap()
                .window_pids
                .insert(id.into(), pids.iter().copied().collect());
        }

        fn release_stop(&self) {
            let mut state = self.state.lock().unwrap();
            state.block_stop = false;
            self.changed.notify_all();
        }
    }

    impl LaunchUnitBackend for DeterministicBackend {
        fn window_pids(&self, launch_id: &str) -> Result<BTreeSet<i32>, LaunchUnitError> {
            let state = self.state.lock().unwrap();
            if state.window_pids_fail {
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    "control group unreadable",
                ));
            }
            Ok(state
                .window_pids
                .get(launch_id)
                .cloned()
                .unwrap_or_default())
        }

        fn launch(
            &self,
            launch_id: &str,
            _command: &[String],
            environment: &BTreeMap<String, String>,
        ) -> Result<(), LaunchUnitError> {
            if self.state.lock().unwrap().launch_rejected {
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    "launch rejected",
                ));
            }
            self.insert(launch_id, LaunchUnitState::Running);
            if let Some(runner_id) = environment.get(RUNNER_ID_ENV) {
                self.state
                    .lock()
                    .unwrap()
                    .runners
                    .insert(launch_id.into(), runner_id.clone());
            }
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
                    if let Some(events) = &state.return_events {
                        events.lock().unwrap().push("thaw");
                    }
                    Ok(())
                }
                _ => Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    "unit is not active",
                )),
            }
        }

        fn runner_id(&self, launch_id: &str) -> Result<Option<String>, LaunchUnitError> {
            Ok(self.state.lock().unwrap().runners.get(launch_id).cloned())
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
        fn reset(&self, _launch_id: &str) -> Result<(), String> {
            Ok(())
        }
        fn stop(self: Box<Self>, _launch_id: &str) -> Result<(), String> {
            self.alive.store(false, Ordering::SeqCst);
            Ok(())
        }
    }

    #[derive(Default)]
    struct ResetSeatState {
        starts: Vec<String>,
        resets: Vec<String>,
        stops: Vec<String>,
        reset_fails: bool,
    }

    #[derive(Clone, Default)]
    struct ResetSeatManager {
        state: Arc<Mutex<ResetSeatState>>,
        return_events: Option<Arc<Mutex<Vec<&'static str>>>>,
    }

    struct ResetSeatLease {
        state: Arc<Mutex<ResetSeatState>>,
        return_events: Option<Arc<Mutex<Vec<&'static str>>>>,
        alive: AtomicBool,
    }

    impl InputSeatManager for ResetSeatManager {
        fn start(&self, launch_id: &str) -> Result<Box<dyn InputSeatLease>, String> {
            self.state.lock().unwrap().starts.push(launch_id.into());
            Ok(Box::new(ResetSeatLease {
                state: self.state.clone(),
                return_events: self.return_events.clone(),
                alive: AtomicBool::new(true),
            }))
        }
    }

    impl InputSeatLease for ResetSeatLease {
        fn alive(&self) -> bool {
            self.alive.load(Ordering::SeqCst)
        }

        fn reset(&self, launch_id: &str) -> Result<(), String> {
            let mut state = self.state.lock().unwrap();
            state.resets.push(launch_id.into());
            if let Some(events) = &self.return_events {
                events.lock().unwrap().push("reset");
            }
            if state.reset_fails {
                Err("seat reset refused".into())
            } else {
                Ok(())
            }
        }

        fn stop(self: Box<Self>, launch_id: &str) -> Result<(), String> {
            self.alive.store(false, Ordering::SeqCst);
            self.state.lock().unwrap().stops.push(launch_id.into());
            Ok(())
        }
    }

    fn prepare(control: &HostSessionControl, game: &str) -> SessionPrepared {
        control
            .prepare(game, None, Ok(&["game".into()]), &BTreeMap::new())
            .unwrap()
    }

    #[test]
    fn restart_without_compositor_authority_reattaches_fail_closed_to_portal() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let first = control(root.path(), backend.clone());
        let prepared = prepare(&first, "one");
        drop(first);

        let recovered = control(root.path(), backend);
        let (status, overlay_intent) = recovered.status_with_recovered_overlay_intent();
        assert_eq!(
            status,
            HostSessionStatus::FocusFailed {
                launch_id: prepared.launch_id,
                game_id: Some("one".into()),
            }
        );
        assert_eq!(overlay_intent, None);
        assert_eq!(
            recovered
                .prepare("two", None, Ok(&["game".into()]), &BTreeMap::new())
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
                    .prepare("two", None, Ok(&["game".into()]), &BTreeMap::new())
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
                .prepare("one", None, Ok(&["game".into()]), &BTreeMap::new())
                .unwrap_err()
                .code,
            "HostRecoveryBlocked"
        );
        assert!(matches!(
            control.status(),
            HostSessionStatus::FocusFailed { .. }
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
                .prepare("two", None, Ok(&["game".into()]), &BTreeMap::new())
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
            .prepare("one", None, Ok(&["game".into()]), &BTreeMap::new())
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
            .prepare("one", None, Ok(&["game".into()]), &BTreeMap::new())
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
            "--property=InaccessiblePaths=/var/lib/korrid /run/korrid /run/korrid-browser /run/korrid-control/control.sock /run/korrid-control /home/korri/.config/sunshine /run/korri-compositor /run/korri-certificate-control /run/user/1001 -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources".into(),
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
    fn systemd_runner_identity_query_reads_live_unit_environment() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemctl");
        let log = root.path().join("calls.log");
        fs::write(
            &helper,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\nprintf 'Environment=KORRI_LIVE_RUNNER_ID=@korri:mgba/mgba\\n'\n",
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

        assert_eq!(
            backend.runner_id(id).unwrap(),
            Some("@korri:mgba/mgba".into())
        );
        assert_eq!(
            fs::read_to_string(log).unwrap(),
            format!(
                "--system --no-ask-password show korri-game-{id}.service --property=Environment\n"
            )
        );
    }

    /// The production reader must name windows from the kernel's own view of
    /// the exact unit, and must not treat a finished unit as a failure.
    #[test]
    fn unit_processes_come_from_that_units_own_control_group() {
        let root = tempfile::tempdir().unwrap();
        let unit = "korri-game-0123456789abcdef0123456789abcdef.service";
        let group = root.path().join(unit);
        fs::create_dir_all(&group).unwrap();
        fs::write(group.join("cgroup.procs"), "9100\n9101\n\n0\nnot-a-pid\n").unwrap();

        assert_eq!(
            read_unit_pids(root.path(), unit).unwrap(),
            BTreeSet::from([9100, 9101])
        );
        // A unit that already finished has no control group. That is an empty
        // process set, never a helper failure.
        assert_eq!(
            read_unit_pids(
                root.path(),
                "korri-game-ffffffffffffffffffffffffffffffff.service"
            )
            .unwrap(),
            BTreeSet::new()
        );
        // An unreadable group must not be mistaken for a game without windows.
        let broken = "korri-game-11111111111111111111111111111111.service";
        fs::create_dir_all(root.path().join(broken).join("cgroup.procs")).unwrap();
        assert!(read_unit_pids(root.path(), broken).is_err());
    }

    /// Records what korrid asked the compositor to do while resuming.
    #[derive(Debug, Default)]
    struct RecordingCompositor {
        tree: Mutex<String>,
        focused: Mutex<Vec<i64>>,
        focus_fails: AtomicBool,
        return_events: Option<Arc<Mutex<Vec<&'static str>>>>,
    }

    impl CompositorControl for RecordingCompositor {
        fn tree(&self) -> Result<String, String> {
            Ok(self.tree.lock().unwrap().clone())
        }

        fn focus(&self, node_id: i64) -> Result<(), String> {
            self.focused.lock().unwrap().push(node_id);
            if let Some(events) = &self.return_events {
                events.lock().unwrap().push("focus");
            }
            if self.focus_fails.load(Ordering::SeqCst) {
                return Err("compositor refused focus".into());
            }
            Ok(())
        }
    }

    const PORTAL_APP_ID: &str = "chromium-browser";

    fn compositor_tree(game_pid: i32) -> String {
        compositor_tree_with_focus(game_pid, 3)
    }

    fn compositor_tree_with_focus(game_pid: i32, focused_id: i64) -> String {
        format!(
            r#"{{"id": 1, "nodes": [
                 {{"id": 2, "pid": 4100, "app_id": "{PORTAL_APP_ID}",
                  "focused": {}, "nodes": [], "floating_nodes": []}},
                 {{"id": 3, "pid": {game_pid}, "app_id": null,
                  "focused": {}, "nodes": [], "floating_nodes": []}}
               ], "floating_nodes": []}}"#,
            focused_id == 2,
            focused_id == 3,
        )
    }

    fn resuming_control(
        root: &Path,
        backend: Arc<DeterministicBackend>,
        compositor: Arc<RecordingCompositor>,
    ) -> HostSessionControl {
        HostSessionControl::new(root, backend)
            .with_compositor(compositor, vec![PORTAL_APP_ID.to_string()])
    }

    #[test]
    fn restart_recovers_portal_ownership_when_the_live_game_is_behind_korri() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let control = resuming_control(root.path(), backend.clone(), compositor.clone());
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree_with_focus(9100, 2);
        drop(control);

        let recovered = resuming_control(root.path(), backend, compositor);
        let (status, overlay_intent) = recovered.status_with_recovered_overlay_intent();
        assert_eq!(
            status,
            HostSessionStatus::FocusFailed {
                launch_id: id.clone(),
                game_id: Some("one".into()),
            }
        );
        assert_eq!(overlay_intent.as_deref(), Some(id.as_str()));
        assert_eq!(
            recovered.status_with_recovered_overlay_intent().1,
            None,
            "startup overlay intent is consumed once into server memory"
        );
    }

    #[test]
    fn restart_with_game_focus_recovers_running_without_overlay_intent() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let control = resuming_control(root.path(), backend.clone(), compositor.clone());
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree_with_focus(9100, 3);
        drop(control);

        let recovered = resuming_control(root.path(), backend, compositor);
        let (status, overlay_intent) = recovered.status_with_recovered_overlay_intent();
        assert!(matches!(status, HostSessionStatus::Running { .. }));
        assert_eq!(overlay_intent, None);
    }

    #[test]
    fn restart_with_ambiguous_launch_windows_never_recovers_overlay_intent() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let control = resuming_control(root.path(), backend.clone(), compositor.clone());
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = r#"{"id":1,"nodes":[
            {"id":2,"pid":4100,"app_id":"chromium-browser","focused":true},
            {"id":3,"pid":9100,"focused":false},
            {"id":4,"pid":9100,"focused":false}
        ]}"#
        .into();
        drop(control);

        let recovered = resuming_control(root.path(), backend, compositor);
        let (status, overlay_intent) = recovered.status_with_recovered_overlay_intent();
        assert!(matches!(status, HostSessionStatus::FocusFailed { .. }));
        assert_eq!(overlay_intent, None);
    }

    #[test]
    fn resuming_raises_the_window_of_that_exact_game() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let control = resuming_control(root.path(), backend.clone(), compositor.clone());
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);

        assert_eq!(
            control.freeze(&id),
            HostSessionFreezeChange::Changed {
                launch_id: id.clone()
            }
        );
        assert!(compositor.focused.lock().unwrap().is_empty());
        assert_eq!(
            control.thaw(&id),
            HostSessionFreezeChange::Changed {
                launch_id: id.clone()
            }
        );
        assert_eq!(*compositor.focused.lock().unwrap(), vec![3]);
    }

    #[test]
    fn exact_return_orders_thaw_then_seat_reset_then_focus_before_success() {
        let root = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let backend = Arc::new(DeterministicBackend::default());
        backend.state.lock().unwrap().return_events = Some(events.clone());
        let manager = Arc::new(ResetSeatManager {
            return_events: Some(events.clone()),
            ..ResetSeatManager::default()
        });
        let compositor = Arc::new(RecordingCompositor {
            return_events: Some(events.clone()),
            ..RecordingCompositor::default()
        });
        let control =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), manager.clone())
                .with_compositor(compositor.clone(), vec![PORTAL_APP_ID.to_string()]);
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        control.freeze(&id);
        events.lock().unwrap().clear();

        assert_eq!(
            control.thaw(&id),
            HostSessionFreezeChange::Changed {
                launch_id: id.clone()
            }
        );
        events.lock().unwrap().push("success");

        assert_eq!(
            *events.lock().unwrap(),
            ["thaw", "reset", "focus", "success"]
        );
        assert_eq!(manager.state.lock().unwrap().resets, [id]);
    }

    #[test]
    fn stale_return_never_resets_the_replacement_launch_lease() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let manager = Arc::new(ResetSeatManager::default());
        let control =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), manager.clone());
        let first = prepare(&control, "one");
        backend.insert(&first.launch_id, LaunchUnitState::Completed);
        assert!(matches!(
            control.status(),
            HostSessionStatus::Completed { .. }
        ));
        let second = prepare(&control, "two");

        assert_eq!(
            control.thaw(&first.launch_id),
            HostSessionFreezeChange::StaleIdentity {
                active_launch_id: Some(second.launch_id.clone())
            }
        );
        let seats = manager.state.lock().unwrap();
        assert!(seats.resets.is_empty());
        assert_eq!(seats.starts.last(), Some(&second.launch_id));
    }

    #[test]
    fn reset_failure_revokes_the_exact_lease_and_reports_focus_failed() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let manager = Arc::new(ResetSeatManager::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let control =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), manager.clone())
                .with_compositor(compositor.clone(), vec![PORTAL_APP_ID.to_string()]);
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        control.freeze(&id);
        manager.state.lock().unwrap().reset_fails = true;

        assert_eq!(
            control.thaw(&id),
            HostSessionFreezeChange::FocusFailed {
                launch_id: id.clone(),
                message: "input seats could not reset for the game: seat reset refused".into(),
            }
        );
        assert!(compositor.focused.lock().unwrap().is_empty());
        assert_eq!(backend.state(&id).unwrap(), LaunchUnitState::Running);
        assert_eq!(
            control.status(),
            HostSessionStatus::FocusFailed {
                launch_id: id.clone(),
                game_id: Some("one".into()),
            }
        );
        let seats = manager.state.lock().unwrap();
        assert_eq!(seats.resets, [id.as_str()]);
        assert_eq!(seats.stops, [id.as_str()]);
    }

    #[test]
    fn each_exact_unchanged_return_resets_before_refocusing() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let manager = Arc::new(ResetSeatManager::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let control =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), manager.clone())
                .with_compositor(compositor.clone(), vec![PORTAL_APP_ID.to_string()]);
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);

        for _ in 0..2 {
            assert_eq!(
                control.thaw(&id),
                HostSessionFreezeChange::Unchanged {
                    launch_id: id.clone()
                }
            );
        }

        assert_eq!(
            manager.state.lock().unwrap().resets,
            [id.as_str(), id.as_str()]
        );
        assert_eq!(*compositor.focused.lock().unwrap(), [3, 3]);
    }

    #[test]
    fn thaw_resets_a_restart_recovered_exact_lease() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        persist_test_record(root.path(), id);
        backend.insert(id, LaunchUnitState::Frozen);
        backend.set_pids(id, &[9100]);
        let manager = Arc::new(ResetSeatManager::default());
        let compositor = Arc::new(RecordingCompositor::default());
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        let control = HostSessionControl::with_input_seats(root.path(), backend, manager.clone())
            .with_compositor(compositor, vec![PORTAL_APP_ID.to_string()]);

        assert!(matches!(control.status(), HostSessionStatus::Frozen { .. }));
        assert_eq!(manager.state.lock().unwrap().starts, [id]);
        assert_eq!(
            control.thaw(id),
            HostSessionFreezeChange::Changed {
                launch_id: id.into()
            }
        );
        assert_eq!(manager.state.lock().unwrap().resets, [id]);
    }

    #[test]
    fn resuming_without_one_exact_game_window_fails_closed_to_portal() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let control = resuming_control(root.path(), backend.clone(), compositor.clone());
        let id = prepare(&control, "one").launch_id;
        // The hub shares the runtime user, so its process can appear here.
        backend.set_pids(&id, &[4100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);

        assert_eq!(
            control.freeze(&id),
            HostSessionFreezeChange::Changed {
                launch_id: id.clone()
            }
        );
        assert_eq!(
            control.thaw(&id),
            HostSessionFreezeChange::FocusFailed {
                launch_id: id.clone(),
                message: "no unique exact-launch window could be focused".into(),
            }
        );
        assert!(compositor.focused.lock().unwrap().is_empty());
        assert_eq!(
            control.status(),
            HostSessionStatus::FocusFailed {
                launch_id: id,
                game_id: Some("one".into()),
            }
        );
    }

    #[test]
    fn resuming_with_ambiguous_exact_game_windows_fails_closed_to_portal() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let control = resuming_control(root.path(), backend.clone(), compositor.clone());
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = r#"{"id":1,"nodes":[
                {"id":3,"pid":9100,"nodes":[],"floating_nodes":[]},
                {"id":4,"pid":9100,"nodes":[],"floating_nodes":[]}
            ],"floating_nodes":[]}"#
            .into();
        control.freeze(&id);

        assert_eq!(
            control.thaw(&id),
            HostSessionFreezeChange::FocusFailed {
                launch_id: id.clone(),
                message: "no unique exact-launch window could be focused".into(),
            }
        );
        assert!(compositor.focused.lock().unwrap().is_empty());
        assert_eq!(
            control.status(),
            HostSessionStatus::FocusFailed {
                launch_id: id,
                game_id: Some("one".into()),
            }
        );
    }

    #[test]
    fn a_refused_focus_is_reported_instead_of_claiming_the_game_returned() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor {
            focus_fails: AtomicBool::new(true),
            ..RecordingCompositor::default()
        });
        let alive = Arc::new(AtomicBool::new(false));
        let manager = Arc::new(TestSeatManager {
            starts: AtomicUsize::new(0),
            alive: alive.clone(),
        });
        let control =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), manager.clone())
                .with_compositor(compositor.clone(), vec![PORTAL_APP_ID.to_string()]);
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        control.freeze(&id);

        assert_eq!(
            control.thaw(&id),
            HostSessionFreezeChange::FocusFailed {
                launch_id: id.clone(),
                message: "compositor refused focus".into()
            }
        );
        // Focus refusal never pauses the game. The streamed seat is revoked,
        // and repeated status reads do not continuously enforce freezer state.
        assert_eq!(backend.state(&id).unwrap(), LaunchUnitState::Running);
        assert_eq!(
            control.status(),
            HostSessionStatus::FocusFailed {
                launch_id: id.clone(),
                game_id: Some("one".into()),
            }
        );
        assert_eq!(
            control.status(),
            HostSessionStatus::FocusFailed {
                launch_id: id.clone(),
                game_id: Some("one".into()),
            }
        );
        assert_eq!(backend.state.lock().unwrap().frozen, vec![id.clone()]);
        assert!(!alive.load(Ordering::SeqCst));
        assert!(!control.seats_are_live_for(&id));

        // A successful exact return reacquires only this launch's route.
        compositor.focus_fails.store(false, Ordering::SeqCst);
        assert_eq!(
            control.thaw(&id),
            HostSessionFreezeChange::Unchanged {
                launch_id: id.clone()
            }
        );
        assert!(alive.load(Ordering::SeqCst));
        assert!(control.seats_are_live_for(&id));
        assert_eq!(manager.starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn an_unreadable_control_group_reports_focus_failure_without_refreezing() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let control = resuming_control(root.path(), backend.clone(), compositor.clone());
        let id = prepare(&control, "one").launch_id;
        control.freeze(&id);
        backend.state.lock().unwrap().window_pids_fail = true;

        assert!(matches!(
            control.thaw(&id),
            HostSessionFreezeChange::FocusFailed { .. }
        ));
        // This control uses DisabledInputSeats, the production local-game
        // shape. A compositor failure must still leave the game running.
        assert_eq!(backend.state(&id).unwrap(), LaunchUnitState::Running);
        assert_eq!(backend.state.lock().unwrap().frozen, vec![id.clone()]);
        assert!(matches!(
            control.status(),
            HostSessionStatus::FocusFailed { launch_id, .. } if launch_id == id
        ));
    }

    #[test]
    fn freezing_and_stopping_never_touch_the_compositor() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let control = resuming_control(root.path(), backend.clone(), compositor.clone());
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);

        control.freeze(&id);
        control.stop(&id);
        assert!(compositor.focused.lock().unwrap().is_empty());
    }

    #[test]
    fn a_host_without_compositor_authority_fails_return_closed_to_portal() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let control = HostSessionControl::new(root.path(), backend.clone());
        let id = prepare(&control, "one").launch_id;
        control.freeze(&id);

        assert_eq!(
            control.thaw(&id),
            HostSessionFreezeChange::FocusFailed {
                launch_id: id.clone(),
                message: "compositor focus authority is not configured".into(),
            }
        );
        assert_eq!(backend.state(&id).unwrap(), LaunchUnitState::Running);
        assert_eq!(
            control.status(),
            HostSessionStatus::FocusFailed {
                launch_id: id,
                game_id: Some("one".into()),
            }
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
        let compositor = Arc::new(RecordingCompositor::default());
        let control =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), manager.clone())
                .with_compositor(compositor.clone(), vec![PORTAL_APP_ID.to_string()]);
        let prepared = prepare(&control, "one");
        let id = prepared.launch_id.clone();
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
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
        let compositor = Arc::new(RecordingCompositor::default());
        let session = resuming_control(root.path(), backend.clone(), compositor.clone());
        let prepared = prepare(&session, "one");
        let id = prepared.launch_id.clone();
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);

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
            fn window_pids(&self, launch_id: &str) -> Result<BTreeSet<i32>, LaunchUnitError> {
                self.0.window_pids(launch_id)
            }
        }

        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(FreezeOnLaunch(DeterministicBackend::default()));
        let alive = Arc::new(AtomicBool::new(false));
        let manager = Arc::new(TestSeatManager {
            starts: AtomicUsize::new(0),
            alive: alive.clone(),
        });
        let compositor = Arc::new(RecordingCompositor::default());
        let session =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), manager.clone())
                .with_compositor(compositor.clone(), vec![PORTAL_APP_ID.to_string()]);

        // An operator froze the slice before prepare observed the unit. The
        // launch is real: it is recorded as frozen, seats are kept, and
        // recovery is not blocked.
        let prepared = session
            .prepare("one", None, Ok(&["game".into()]), &BTreeMap::new())
            .unwrap();
        backend.0.set_pids(&prepared.launch_id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
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
                .prepare("two", None, Ok(&["game".into()]), &BTreeMap::new())
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
            &"--property=InaccessiblePaths=/srv/korri-test/private-recovery /run/korrid /run/korrid-browser /run/korri-test/control/device.sock /run/korri-test/control /home/gameplay/.config/sunshine /run/korri-test/compositor-control /run/korri-certificate-control /run/user/1001 -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources".into()
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
        backend.set_pids(id, &[9100]);
        let alive = Arc::new(AtomicBool::new(true));
        let manager = Arc::new(TestSeatManager {
            starts: AtomicUsize::new(0),
            alive: alive.clone(),
        });
        let compositor = Arc::new(RecordingCompositor::default());
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        let control = HostSessionControl::with_input_seats(root.path(), backend, manager.clone())
            .with_compositor(compositor, vec![PORTAL_APP_ID.to_string()]);
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
        backend.set_pids(id, &[9100]);
        let compositor = Arc::new(RecordingCompositor::default());
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        let control = HostSessionControl::with_input_seats(
            root.path(),
            backend.clone(),
            Arc::new(FailingSeatManager),
        )
        .with_compositor(compositor, vec![PORTAL_APP_ID.to_string()]);

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
            .prepare(
                "wario",
                Some(PERSON),
                Ok(&["game".into()]),
                &BTreeMap::new(),
            )
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
            .prepare(
                "wario",
                Some(PERSON),
                Ok(&["game".into()]),
                &BTreeMap::new(),
            )
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
            .prepare(
                "wario",
                Some(PERSON),
                Ok(&["game".into()]),
                &BTreeMap::new(),
            )
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
                .prepare(
                    "wario",
                    Some(PERSON),
                    Ok(&["game".into()]),
                    &BTreeMap::new()
                )
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
                .prepare(
                    &game_id,
                    Some(PERSON),
                    Ok(&["game".into()]),
                    &BTreeMap::new(),
                )
                .unwrap_err();
            assert_eq!(failure.code, "PlayLogPathUnavailable");
            assert!(backend.state.lock().unwrap().units.is_empty());
            assert!(!root.path().join("host-session").exists());
        }
    }
}
