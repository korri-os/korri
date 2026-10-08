use super::launch_reservation::LaunchReservation;
use crate::{InitialHandoff, RpcFailure, SessionPrepared};
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
    read_unit_pids, LaunchUnitError, LaunchUnitErrorKind, RecordingPortalUnit,
    SystemdLaunchUnitBackend,
};
use super::systemd_unit::{LaunchUnitBackend, LaunchUnitState, PortalUnit, RUNNER_ID_ENV};

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
    /// A native control refused the change. A failed portal thaw during
    /// Leave rolls back the game freeze before returning this failure.
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

/// What korrid last applied to the portal unit's freezer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PortalFreezerState {
    /// Not proven since startup, or the last thaw failed.
    Unknown,
    /// Frozen, or a freeze was attempted. A refused freeze costs only power,
    /// so it is not retried on every status read.
    Frozen,
    Thawed,
}

#[derive(Clone)]
struct PortalFreezer {
    unit: Arc<dyn PortalUnit>,
    applied: Arc<Mutex<PortalFreezerState>>,
    last_error: Arc<Mutex<Option<String>>>,
}

/// The session state under its mutex. Releasing it reconciles the portal
/// freezer with the state left behind, so no transition or early return can
/// leave the portal frozen outside a focused live game.
struct SessionStateGuard<'a> {
    control: &'a HostSessionControl,
    state: MutexGuard<'a, ActiveState>,
    reconcile: bool,
}

impl Deref for SessionStateGuard<'_> {
    type Target = ActiveState;

    fn deref(&self) -> &ActiveState {
        &self.state
    }
}

impl DerefMut for SessionStateGuard<'_> {
    fn deref_mut(&mut self) -> &mut ActiveState {
        &mut self.state
    }
}

impl Drop for SessionStateGuard<'_> {
    fn drop(&mut self) {
        // Confirmed absence/completion retires only the exact live fact. A
        // recovery/observation failure is not proof that its handoff ended.
        self.control.retire_initial_handoff(&self.state);
        // A panic may leave a half-made transition. Thaw rather than trust it.
        if std::thread::panicking() {
            *self.state = ActiveState::RecoveryBlocked;
            if let Some(portal) = &self.control.portal {
                *portal
                    .applied
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = PortalFreezerState::Unknown;
            }
            let _ = self.control.reconcile_portal(&self.state);
        } else if self.reconcile {
            if let Err(message) = self.control.reconcile_portal(&self.state) {
                eprintln!("korrid: input/freezer transition failed: {message}");
                if let ActiveState::Running { launch_id, game_id } = &*self.state {
                    let (launch_id, game_id) = (launch_id.clone(), game_id.clone());
                    let _ = self
                        .control
                        .record_focus_failure(&mut self.state, launch_id, game_id);
                }
                let _ = self.control.thaw_portal();
            }
        }
    }
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
    /// One process-local focus observation for the accepted exact launch.
    /// The journal deliberately contains no historical screen ownership.
    initial_handoff: Arc<Mutex<Option<(String, InitialHandoff)>>>,
    /// Required to prove that an exact launch owns focus before Game input can
    /// return. Absence is a product-path focus failure, never implicit success.
    compositor: Option<Arc<dyn CompositorControl>>,
    /// Surfaces that must never be focused as a game, such as the kiosk hub.
    never_focus: Vec<String>,
    /// Frozen only while a running exact launch has observed compositor focus.
    /// Absent, the portal is never touched.
    portal: Option<PortalFreezer>,
    native_input: Option<(
        Arc<crate::portal_input::PortalInputSource>,
        tokio::runtime::Handle,
    )>,
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
            initial_handoff: Arc::new(Mutex::new(None)),
            compositor: None,
            never_focus: Vec::new(),
            portal: None,
            native_input: None,
        }
    }

    /// Freeze this portal unit while a focused live game owns the screen.
    /// Its freezer state is unknown until the first reconcile, so a portal
    /// left frozen by an earlier korrid is thawed unless a game still holds
    /// proven focus.
    pub(crate) fn with_portal(mut self, unit: Arc<dyn PortalUnit>) -> Self {
        self.portal = Some(PortalFreezer {
            unit,
            applied: Arc::new(Mutex::new(PortalFreezerState::Unknown)),
            last_error: Arc::new(Mutex::new(None)),
        });
        self
    }

    pub(crate) fn with_native_input(
        mut self,
        source: Arc<crate::portal_input::PortalInputSource>,
    ) -> Self {
        self.native_input = Some((source, tokio::runtime::Handle::current()));
        self
    }

    pub(crate) fn initialize_input(&self, count: u8) -> Result<(), String> {
        let mut state = self.lock_state();
        state.reconcile = false;
        self.refresh_recovery(&mut state);
        let session = match &*state {
            ActiveState::Running { launch_id, .. }
            | ActiveState::Frozen { launch_id, .. }
            | ActiveState::FocusFailed { launch_id, .. }
            | ActiveState::Stopping { launch_id, .. } => Some(launch_id.as_str()),
            ActiveState::NoActive | ActiveState::Completed { .. } => None,
            _ => return Err("host session authority is unresolved".into()),
        };
        self.input_seats.initialize(count, session)
    }

    pub(crate) fn apply_input_count(&self, count: u8) -> Result<(), String> {
        self.input_seats.apply_count(count)
    }
    pub(crate) fn fence_input(&self) {
        self.input_seats.fence();
    }
    fn retire_native(&self) -> Result<(), String> {
        if let Some((source, runtime)) = &self.native_input {
            if let Err(error) = runtime.block_on(source.suspend(std::time::Duration::from_secs(2)))
            {
                self.input_seats.route(None)?;
                source.resume();
                return Err(format!(
                    "native input retirement was not acknowledged: {error}"
                ));
            }
        }
        Ok(())
    }
    fn resume_native(&self) {
        if let Some((source, _)) = &self.native_input {
            source.resume();
        }
    }

    fn lock_state(&self) -> SessionStateGuard<'_> {
        let state = self.state.lock().unwrap_or_else(|error| {
            let mut state = error.into_inner();
            *state = ActiveState::RecoveryBlocked;
            self.state.clear_poison();
            state
        });
        SessionStateGuard {
            control: self,
            state,
            reconcile: true,
        }
    }

    /// Called only under the session transition mutex. Log a failure once,
    /// then only a different failure or recovery, not once per watcher tick.
    fn portal_result(
        &self,
        result: Result<(), super::systemd_unit::LaunchUnitError>,
    ) -> Result<(), String> {
        let portal = self.portal.as_ref().expect("configured portal");
        let mut last = portal
            .last_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match result {
            Ok(()) => {
                if last.take().is_some() {
                    eprintln!("korrid: portal freezer recovered");
                }
                Ok(())
            }
            Err(error) => {
                if last.as_ref() != Some(&error.message) {
                    eprintln!("korrid: portal freezer failed: {}", error.message);
                }
                *last = Some(error.message.clone());
                Err(error.message)
            }
        }
    }

    /// A Leave acknowledgement must include this result. Drop is only a
    /// safety net for other transitions, not the Leave transaction itself.
    fn thaw_portal(&self) -> Result<(), String> {
        // Native resume must never overlap the old gameplay route, including
        // focus loss and guard-driven recovery, not only explicit Leave.
        if let Err(message) = self.input_seats.route(None) {
            if self.input_seats.is_fenced() {
                self.emergency_thaw_portal()?;
            }
            // Even a successful emergency unit thaw does not acknowledge an
            // input handoff or resume native delivery.
            return Err(message);
        }
        self.thaw_portal_unit()?;
        self.resume_native();
        Ok(())
    }

    /// Terminal coordinator loss retires controller delivery permanently for
    /// this runtime. Thaw only the UI unit so keyboard/mouse recovery remains
    /// possible. Keep the journal/reservations and launch fence untouched.
    fn emergency_thaw_portal(&self) -> Result<(), String> {
        self.input_seats.fence();
        if let Some((source, _)) = &self.native_input {
            source.reset();
        }
        self.thaw_portal_unit()
    }

    fn thaw_portal_unit(&self) -> Result<(), String> {
        let Some(portal) = &self.portal else {
            return Ok(());
        };
        let mut applied = portal
            .applied
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if *applied == PortalFreezerState::Thawed {
            return Ok(());
        }
        *applied = PortalFreezerState::Unknown;
        self.portal_result(portal.unit.thaw())?;
        *applied = PortalFreezerState::Thawed;
        Ok(())
    }

    /// Process liveness is not focus. Observe the existing exact-launch
    /// compositor contract before freezing, including initial launch and
    /// every watcher tick. Missing/delayed windows leave the portal running.
    fn reconcile_portal(&self, state: &ActiveState) -> Result<(), String> {
        if self.input_seats.is_fenced() {
            self.emergency_thaw_portal()?;
            return Err("input coordination is fenced; restart korrid after resolving the exact active session".into());
        }
        let freeze = matches!(state, ActiveState::Running { launch_id, .. }
            if matches!(self.current_focus_ownership(launch_id), Ok(FocusOwnership::Launch)));
        let route = match state {
            ActiveState::Running { launch_id, .. } if freeze => Some(launch_id.as_str()),
            _ => None,
        };
        let Some(portal) = &self.portal else {
            // Focus and controller ownership do not depend on an optional
            // cgroup freezer. The same exact-launch proof selects gameplay.
            return self.input_seats.route(route);
        };
        let unknown = *portal
            .applied
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            == PortalFreezerState::Unknown;
        if !freeze {
            return self.thaw_portal();
        }
        if unknown {
            self.thaw_portal()?;
        }
        self.input_seats.route(route)?;
        if freeze {
            let mut applied = portal
                .applied
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if *applied != PortalFreezerState::Frozen {
                // A refused freeze costs only power. Do not retry until focus
                // leaves and returns. Every attempted freeze still needs thaw.
                self.retire_native()?;
                if self.portal_result(portal.unit.freeze()).is_err() && self.native_input.is_some()
                {
                    // Keep the existing best-effort freezer policy. A refused
                    // helper can have partially frozen the unit: prove thaw
                    // before resuming the retired browser stream.
                    *applied = PortalFreezerState::Unknown;
                    drop(applied);
                    self.thaw_portal()?;
                    self.input_seats.route(route)?;
                    // Suppress repeated freeze attempts until focus leaves.
                    *portal
                        .applied
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = PortalFreezerState::Frozen;
                    return Ok(());
                }
                *applied = PortalFreezerState::Frozen;
            }
        }
        Ok(())
    }

    /// Test observation of a pending release. The production watcher always
    /// observes focus too, even while a delayed game has no mapped window.
    #[cfg(test)]
    fn portal_watch_needed(&self) -> bool {
        self.portal.as_ref().is_some_and(|portal| {
            *portal
                .applied
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                != PortalFreezerState::Thawed
        })
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
        let ownership = focused_ownership(&tree, &pids, &excluded)?;
        if ownership == FocusOwnership::Launch {
            self.record_initial_handoff(launch_id);
        }
        Ok(ownership)
    }

    fn record_initial_handoff(&self, launch_id: &str) {
        let mut handoff = self
            .initial_handoff
            .lock()
            .expect("initial handoff mutex poisoned");
        if let Some((exact, phase)) = handoff.as_mut() {
            if exact == launch_id {
                *phase = InitialHandoff::Observed;
            }
        }
    }

    fn retire_initial_handoff(&self, state: &ActiveState) {
        let mut handoff = self
            .initial_handoff
            .lock()
            .expect("initial handoff mutex poisoned");
        match state {
            ActiveState::NoActive => {
                handoff.take();
            }
            ActiveState::Completed { launch_id }
                if handoff
                    .as_ref()
                    .is_some_and(|(exact, _)| exact == launch_id) =>
            {
                handoff.take();
            }
            _ => {}
        }
    }

    fn initial_handoff_for(&self, state: &ActiveState) -> Option<InitialHandoff> {
        let exact = match state {
            ActiveState::Running { launch_id, .. }
            | ActiveState::Frozen { launch_id, .. }
            | ActiveState::FocusFailed { launch_id, .. }
            | ActiveState::Stopping { launch_id, .. } => launch_id,
            _ => return None,
        };
        let mut handoff = self
            .initial_handoff
            .lock()
            .expect("initial handoff mutex poisoned");
        match handoff.as_ref() {
            Some((launch_id, phase)) if launch_id == exact => Some(*phase),
            _ => {
                handoff.take();
                Some(InitialHandoff::Recovered)
            }
        }
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
    ) -> (HostSessionStatus, Option<FocusOwnership>, Option<String>) {
        self.observe_status_with_recovered_overlay_intent(|status, ownership, _, recovered| {
            (status, ownership, recovered)
        })
    }

    pub(crate) fn observe_status_with_recovered_overlay_intent<T>(
        &self,
        observe: impl FnOnce(
            HostSessionStatus,
            Option<FocusOwnership>,
            Option<InitialHandoff>,
            Option<String>,
        ) -> T,
    ) -> T {
        self.observe_status(|status, ownership, handoff| {
            self.observe_recovered_status(status, ownership, handoff, observe)
        })
    }

    pub(crate) fn try_observe_status_with_recovered_overlay_intent<T>(
        &self,
        wait: Duration,
        observe: impl FnOnce(
            HostSessionStatus,
            Option<FocusOwnership>,
            Option<InitialHandoff>,
            Option<String>,
        ) -> T,
    ) -> Option<T> {
        let started = Instant::now();
        let state = loop {
            match self.state.try_lock() {
                Ok(state) => break state,
                Err(std::sync::TryLockError::WouldBlock) => {
                    let remaining = wait.saturating_sub(started.elapsed());
                    if remaining.is_zero() {
                        return None;
                    }
                    std::thread::sleep(Duration::from_millis(5).min(remaining));
                }
                Err(std::sync::TryLockError::Poisoned(error)) => {
                    let mut state = error.into_inner();
                    *state = ActiveState::RecoveryBlocked;
                    self.state.clear_poison();
                    break state;
                }
            }
        };
        let guard = SessionStateGuard {
            control: self,
            state,
            reconcile: true,
        };
        Some(
            self.observe_status_locked(guard, |status, ownership, handoff| {
                self.observe_recovered_status(status, ownership, handoff, observe)
            }),
        )
    }

    fn observe_recovered_status<T>(
        &self,
        status: HostSessionStatus,
        ownership: Option<FocusOwnership>,
        handoff: Option<InitialHandoff>,
        observe: impl FnOnce(
            HostSessionStatus,
            Option<FocusOwnership>,
            Option<InitialHandoff>,
            Option<String>,
        ) -> T,
    ) -> T {
        if matches!(status, HostSessionStatus::RecoveryBlocked) {
            // Unknown native state cannot consume a recovered Home fact.
            return observe(status, ownership, handoff, None);
        }
        let expected = match &status {
            HostSessionStatus::FocusFailed { launch_id, .. } => Some(launch_id.as_str()),
            _ => None,
        };
        let mut recovered_intent = self
            .recovered_overlay_intent
            .lock()
            .expect("recovered overlay intent mutex poisoned");
        let recovered = match expected {
            None => {
                recovered_intent.take();
                None
            }
            Some(expected) if recovered_intent.as_deref() == Some(expected) => {
                recovered_intent.take();
                matches!(ownership, Some(FocusOwnership::Excluded)).then(|| expected.to_owned())
            }
            Some(_) => None,
        };
        drop(recovered_intent);
        observe(status, ownership, handoff, recovered)
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
        let outcome = focus_launch_window(compositor.as_ref(), &pids, &excluded);
        // Focused is returned only after the compositor tree proves exact
        // ownership, never from the focus command's acknowledgement alone.
        if matches!(outcome, FocusOutcome::Focused(_)) {
            self.record_initial_handoff(launch_id);
        }
        Some(outcome)
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
        let mut state = self.lock_state();
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
        self.input_seats.end_session(record.launch_id())?;
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
        let mut current = self.seat_lease.lock().expect("input-seat mutex poisoned");
        if current
            .as_ref()
            .is_some_and(|(active, _)| active != launch_id)
        {
            return Err("input-seat lease belongs to a different launch".into());
        }
        self.input_seats.route(None)?;
        match current.take() {
            Some((_, lease)) => lease.stop(launch_id),
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
        }
    }

    /// Serialize device-only settings with prepare, freeze, and stop. Prove
    /// absence from both the live units and the durable journal, not a cached
    /// status read. This check never stops, thaws, or recovers a game merely to
    /// permit a configuration write. Unresolved recovery must finish elsewhere.
    pub(crate) fn with_idle_session<T>(
        &self,
        mutation: impl FnOnce() -> Result<T, RpcFailure>,
    ) -> Result<T, RpcFailure> {
        let mut state = self.lock_state();
        // Even a rejected settings request must not change portal/game state.
        state.reconcile = false;
        match &*state {
            ActiveState::Running { .. }
            | ActiveState::Frozen { .. }
            | ActiveState::FocusFailed { .. }
            | ActiveState::Stopping { .. } => {
                return Err(failure(
                    "ActiveSessionConflict",
                    "player count cannot change while a host session is active",
                ));
            }
            ActiveState::RecoveryBlocked => return Err(recovery_blocked_failure()),
            ActiveState::RecoveryPending
            | ActiveState::Completed { .. }
            | ActiveState::NoActive => {}
        }
        let live = self
            .backend
            .live_launch_ids()
            .map_err(|_| recovery_blocked_failure())?;
        let recorded = read_active(&self.identity_root).map_err(|_| recovery_blocked_failure())?;
        if !live.is_empty() || recorded.is_some() {
            return Err(recovery_blocked_failure());
        }
        // Keep the transition guard alive across the complete CAS write.
        mutation()
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
            None,
        )
    }

    pub(super) fn prepare_reserved(
        &self,
        reservation: &LaunchReservation,
        runner_id: Option<&str>,
        configured_command: &[String],
        environment: &BTreeMap<String, String>,
    ) -> Result<SessionPrepared, RpcFailure> {
        self.prepare_inner(
            &reservation.identity.game_id,
            reservation.person.as_deref(),
            Ok(configured_command),
            environment,
            false,
            runner_id,
            Some(reservation),
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
        reservation: Option<&LaunchReservation>,
    ) -> Result<SessionPrepared, RpcFailure> {
        let mut state = self.lock_state();
        self.input_seats
            .ready()
            .map_err(|message| failure("InputSeatUnavailable", message))?;
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
                self.reconcile_portal(&state)
                    .map_err(|message| failure("InputTransitionFailed", message))?;
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

        // Cancellation and committing effects have one atomic winner. Route
        // preparation ran outside this lock; a cancelled token cannot persist,
        // acquire input, or spawn. Cancellation after commit reports Pending
        // and exact-stops when this transition releases the lock.
        let launch_id = match reservation {
            Some(reservation) => {
                reservation.commit()?;
                reservation.identity.launch_id.clone()
            }
            None => crate::generate_launch_id(),
        };
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
        let seat_lease = match self
            .input_seats
            .begin_session(&launch_id)
            .and_then(|()| self.input_seats.start(&launch_id))
        {
            Ok(lease) => lease,
            Err(message) => {
                // The game never ran; discard the record without logging.
                *state = if self.discard_active().is_ok() {
                    ActiveState::NoActive
                } else {
                    ActiveState::RecoveryBlocked
                };
                return Err(failure("InputSeatUnavailable", message));
            }
        };
        let mut launch_environment = environment.clone();
        if let Some(runner_id) = runner_id {
            launch_environment.insert(RUNNER_ID_ENV.into(), runner_id.into());
        }
        // Input-seat acquisition can block after committing. If cancellation
        // arrived there, discard the startup journal and do not spawn at all.
        if reservation.is_some_and(LaunchReservation::is_cancelled) {
            let _ = seat_lease.stop(&launch_id);
            *state = if self.discard_active().is_ok() {
                ActiveState::NoActive
            } else {
                ActiveState::RecoveryBlocked
            };
            return Err(super::launch_reservation::cancelled());
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
                *self
                    .initial_handoff
                    .lock()
                    .expect("initial handoff mutex poisoned") =
                    Some((launch_id.clone(), InitialHandoff::Waiting));
                *state = active_from_observed(observed, launch_id.clone(), Some(game_id.into()));
                if let Err(message) = self.reconcile_portal(&state) {
                    let _ = self.record_focus_failure(
                        &mut state,
                        launch_id.clone(),
                        Some(game_id.into()),
                    );
                    return Err(failure("InputTransitionFailed", message));
                }
                Ok(SessionPrepared {
                    game_id: game_id.into(),
                    launch_id,
                })
            }
            Ok(LaunchUnitState::Stopping) => {
                *self
                    .initial_handoff
                    .lock()
                    .expect("initial handoff mutex poisoned") =
                    Some((launch_id.clone(), InitialHandoff::Waiting));
                let _ = seat_lease.stop(&launch_id);
                *state = ActiveState::Stopping {
                    launch_id,
                    game_id: Some(game_id.into()),
                };
                Err(failure(
                    "HostLaunchFailed",
                    "host game is stopping before prepare completed",
                ))
            }
            Ok(LaunchUnitState::Completed) => {
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
        self.status_snapshot().0
    }

    fn status_snapshot(&self) -> (HostSessionStatus, Option<FocusOwnership>) {
        self.observe_status(|status, ownership, _| (status, ownership))
    }

    fn observe_status<T>(
        &self,
        observe: impl FnOnce(HostSessionStatus, Option<FocusOwnership>, Option<InitialHandoff>) -> T,
    ) -> T {
        self.observe_status_locked(self.lock_state(), observe)
    }

    fn observe_status_locked<T>(
        &self,
        mut state: SessionStateGuard<'_>,
        observe: impl FnOnce(HostSessionStatus, Option<FocusOwnership>, Option<InitialHandoff>) -> T,
    ) -> T {
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
        state.reconcile = false;
        if let Err(message) = self.reconcile_portal(&state) {
            if let ActiveState::Running { launch_id, game_id } = &*state {
                let (launch_id, game_id) = (launch_id.clone(), game_id.clone());
                eprintln!("korrid: input transition failed: {message}");
                let _ = self.record_focus_failure(&mut state, launch_id, game_id);
            }
        }
        let ownership = match &*state {
            ActiveState::Running { launch_id, .. }
            | ActiveState::Frozen { launch_id, .. }
            | ActiveState::FocusFailed { launch_id, .. }
            | ActiveState::Stopping { launch_id, .. } => {
                self.current_focus_ownership(launch_id).ok()
            }
            _ => None,
        };
        // Correlate server metadata before releasing the exact-session lock;
        // a delayed RPC continuation must not reconcile an older snapshot.
        observe(
            status_from_state(&state),
            ownership,
            self.initial_handoff_for(&state),
        )
    }

    /// Removes the active record for a launch that never reached the
    /// player. Nothing is logged.
    fn discard_active(&self) -> Result<(), String> {
        if let Some(record) = read_active(&self.identity_root)? {
            self.input_seats.end_session(record.launch_id())?;
        }
        consume_active(&self.identity_root).map(|_| ())
    }

    pub fn freeze(&self, expected_launch_id: &str) -> HostSessionFreezeChange {
        self.observe_freeze(expected_launch_id, |outcome| outcome)
    }

    pub(crate) fn observe_freeze<T>(
        &self,
        expected_launch_id: &str,
        observe: impl FnOnce(HostSessionFreezeChange) -> T,
    ) -> T {
        self.set_freezer_observed(expected_launch_id, FreezerTarget::Frozen, observe)
    }

    pub fn thaw(&self, expected_launch_id: &str) -> HostSessionFreezeChange {
        self.observe_thaw(expected_launch_id, |outcome| outcome)
    }

    pub(crate) fn observe_thaw<T>(
        &self,
        expected_launch_id: &str,
        observe: impl FnOnce(HostSessionFreezeChange) -> T,
    ) -> T {
        self.set_freezer_observed(expected_launch_id, FreezerTarget::Running, observe)
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
        let mut state = self.lock_state();
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
        self.input_seats
            .ready()
            .map_err(HostSessionEffectFailure::Unavailable)?;
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
    fn set_freezer_observed<T>(
        &self,
        expected_launch_id: &str,
        target: FreezerTarget,
        observe: impl FnOnce(HostSessionFreezeChange) -> T,
    ) -> T {
        let mut state = self.lock_state();
        let outcome = (|| {
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
            if target == FreezerTarget::Running {
                if let Err(message) = self.input_seats.ready() {
                    return HostSessionFreezeChange::HelperFailed { launch_id, message };
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
            if target == FreezerTarget::Frozen {
                if let Err(message) = self
                    .input_seats
                    .route(None)
                    .and_then(|()| self.thaw_portal())
                {
                    // Inputd keeps Game input when Leave fails. Undo the game
                    // freeze before reporting failure, including an idempotent
                    // Leave of an already frozen launch. Never leave a frozen
                    // game behind with Game input.
                    state.reconcile = false;
                    if self.backend.thaw(&launch_id).is_err() {
                        self.stop_game_after_seat_failure(&mut state, &launch_id);
                        return HostSessionFreezeChange::RecoveryBlocked;
                    }
                    *state = ActiveState::Running {
                        launch_id: launch_id.clone(),
                        game_id: game_id.clone(),
                    };
                    if self.ensure_seats(&launch_id).is_err()
                        || self.input_seats.route(Some(&launch_id)).is_err()
                        || self.reset_seats(&launch_id).is_err()
                        || Self::focus_failure(self.focus_launch(&launch_id)).is_some()
                    {
                        if self
                            .record_focus_failure(&mut state, launch_id.clone(), game_id)
                            .is_err()
                        {
                            return HostSessionFreezeChange::RecoveryBlocked;
                        }
                    }
                    return HostSessionFreezeChange::HelperFailed { launch_id, message };
                }
            }
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
            if let Err(message) = self.reconcile_portal(&state) {
                let _ = self.record_focus_failure(&mut state, launch_id.clone(), game_id);
                return HostSessionFreezeChange::HelperFailed { launch_id, message };
            }
            if already {
                HostSessionFreezeChange::Unchanged { launch_id }
            } else {
                HostSessionFreezeChange::Changed { launch_id }
            }
        })();
        // Publish Home/Return (including terminal cleanup) in the same
        // transition as the native effect, never from its later RPC reply.
        observe(outcome)
    }

    pub fn stop(&self, expected_launch_id: &str) -> HostSessionStop {
        self.observe_stop(expected_launch_id, |outcome| outcome)
    }

    pub(crate) fn observe_stop<T>(
        &self,
        expected_launch_id: &str,
        observe: impl FnOnce(HostSessionStop) -> T,
    ) -> T {
        let (launch_id, seat_stop_failed) = {
            let mut state = self.lock_state();
            self.refresh_recovery(&mut state);
            let initial = match &*state {
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
                    // Route and release A's lease while the transition lock
                    // still prevents any replacement from claiming input.
                    let seat_stop_failed = self.stop_seats(&launch_id).is_err();
                    Ok((launch_id, seat_stop_failed))
                }
                ActiveState::Running { launch_id, .. }
                | ActiveState::Frozen { launch_id, .. }
                | ActiveState::FocusFailed { launch_id, .. } => {
                    Err(HostSessionStop::StaleIdentity {
                        active_launch_id: Some(launch_id.clone()),
                    })
                }
                ActiveState::Stopping { launch_id, .. } if launch_id == expected_launch_id => {
                    Err(HostSessionStop::AlreadyStopping {
                        launch_id: launch_id.clone(),
                    })
                }
                ActiveState::Stopping { launch_id, .. } => Err(HostSessionStop::StaleIdentity {
                    active_launch_id: Some(launch_id.clone()),
                }),
                ActiveState::Completed { launch_id } if launch_id == expected_launch_id => {
                    Err(HostSessionStop::Completed {
                        launch_id: launch_id.clone(),
                    })
                }
                ActiveState::Completed { .. } | ActiveState::NoActive => {
                    Err(HostSessionStop::NoActive)
                }
                ActiveState::RecoveryPending | ActiveState::RecoveryBlocked => {
                    Err(HostSessionStop::RecoveryBlocked)
                }
            };
            match initial {
                Ok(pending) => pending,
                // Absence is an observation about a live unit, not cancellation
                // of a reserved startup that may later use the same exact ID.
                Err(outcome) => return observe(outcome),
            }
        };

        // The session mutex is released here on purpose: the backend stop
        // (and any thaw the backend performs first) runs outside the
        // mutex, as it did before freezer support. `set_freezer` observes
        // `Stopping` and refuses, so no freezer change interleaves with
        // the stop. Both halves of the stop path consistently run the
        // helper without the mutex.
        let stopped = self.backend.stop(&launch_id);
        let mut state = self.lock_state();
        let outcome = (|| {
            // Even guard-drop input/freezer reconciliation belongs to the launch
            // being stopped, not to a replacement that won the transition lock.
            state.reconcile = false;
            // Status can collect A and prepare B while A's helper is running.
            // A late success or error must not mutate B's input, journal or phase.
            match &*state {
                ActiveState::Stopping {
                    launch_id: active, ..
                } if active == &launch_id => {}
                ActiveState::Completed { launch_id: active } if active == &launch_id => {
                    return HostSessionStop::Completed { launch_id };
                }
                ActiveState::Running {
                    launch_id: active, ..
                }
                | ActiveState::Frozen {
                    launch_id: active, ..
                }
                | ActiveState::FocusFailed {
                    launch_id: active, ..
                }
                | ActiveState::Stopping {
                    launch_id: active, ..
                }
                | ActiveState::Completed { launch_id: active } => {
                    return HostSessionStop::StaleIdentity {
                        active_launch_id: Some(active.clone()),
                    };
                }
                ActiveState::NoActive => return HostSessionStop::NoActive,
                ActiveState::RecoveryPending | ActiveState::RecoveryBlocked => {
                    return HostSessionStop::RecoveryBlocked;
                }
            }
            match read_active(&self.identity_root) {
                Ok(Some(record)) if record.launch_id() != launch_id => {
                    return HostSessionStop::StaleIdentity {
                        active_launch_id: Some(record.launch_id().into()),
                    };
                }
                Err(_) => {
                    state.reconcile = true;
                    *state = ActiveState::RecoveryBlocked;
                    return HostSessionStop::RecoveryBlocked;
                }
                Ok(_) => {}
            }
            state.reconcile = true;
            if stopped.is_err() {
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
        })();
        // The helper still ran outside the lock. Publish only after its
        // original identity/journal checks, while transition authority is held.
        observe(outcome)
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

    #[test]
    fn idle_mutation_rejects_live_units_stopping_and_uncertain_recovery() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let control = HostSessionControl::new(root.path(), backend.clone());
        assert_eq!(control.status(), HostSessionStatus::NoActive);
        let mutation = || panic!("a refused idle mutation must never execute");

        // A cached idle status cannot conceal an untracked unit.
        let launch = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        backend.insert(launch, LaunchUnitState::Running);
        assert_eq!(
            control.with_idle_session::<()>(mutation).unwrap_err().code,
            "HostRecoveryBlocked"
        );
        backend.insert(launch, LaunchUnitState::Completed);
        backend.state.lock().unwrap().enumeration_unavailable = true;
        assert_eq!(
            control.with_idle_session::<()>(mutation).unwrap_err().code,
            "HostRecoveryBlocked"
        );
        backend.state.lock().unwrap().enumeration_unavailable = false;
        let prepared = control
            .prepare("game", None, Ok(&["game".into()]), &BTreeMap::new())
            .unwrap();
        backend.insert(&prepared.launch_id, LaunchUnitState::Stopping);
        assert!(matches!(
            control.status(),
            HostSessionStatus::Stopping { .. }
        ));
        assert_eq!(
            control.with_idle_session::<()>(mutation).unwrap_err().code,
            "ActiveSessionConflict"
        );
        assert!(backend.state.lock().unwrap().stopped.is_empty());
        assert!(backend.state.lock().unwrap().thawed.is_empty());
    }

    #[test]
    fn idle_cas_mutation_and_prepare_hold_the_same_transition_mutex() {
        use crate::config::settings::{self, SettingChange};
        use std::sync::mpsc;
        let root = tempfile::tempdir().unwrap();
        crate::config::test_fixtures::gba(root.path());
        let source = crate::plugin_policy::RegistrySource::Selected(Arc::new(
            crate::plugin_test_fixtures::installed(root.path()),
        ));
        let revision = settings::read_with_registry_source(root.path(), &source)
            .unwrap()
            .revision;
        let private = root.path().join("private");
        fs::create_dir(&private).unwrap();
        let backend = Arc::new(super::super::systemd_unit::InMemoryLaunchUnitBackend::default());
        let control = HostSessionControl::new(&private, backend.clone());
        let (entered, inside) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let (preparing, started) = mpsc::channel();
        let (prepared, completed) = mpsc::channel();
        thread::scope(|threads| {
            let mutation_control = &control;
            let root = root.path();
            let private = &private;
            let source = &source;
            threads.spawn(move || {
                mutation_control
                    .with_idle_session(|| {
                        assert!(matches!(
                            mutation_control.state.try_lock(),
                            Err(std::sync::TryLockError::WouldBlock)
                        ));
                        entered.send(()).unwrap();
                        released.recv_timeout(Duration::from_secs(10)).unwrap();
                        settings::update_with_registry_source(
                            root,
                            private,
                            &Mutex::new(()),
                            &revision,
                            SettingChange::PlayerCount(std::num::NonZeroU8::new(6).unwrap()),
                            source,
                        )
                        .map_err(crate::settings_failure)
                    })
                    .unwrap();
            });
            inside.recv_timeout(Duration::from_secs(10)).unwrap();
            let prepare_control = &control;
            threads.spawn(move || {
                preparing.send(()).unwrap();
                let result =
                    prepare_control.prepare("game", None, Ok(&["game".into()]), &BTreeMap::new());
                prepared.send(result).unwrap();
            });
            started.recv_timeout(Duration::from_secs(10)).unwrap();
            assert!(completed.recv_timeout(Duration::from_millis(100)).is_err());
            assert!(backend.live_launch_ids().unwrap().is_empty());
            release.send(()).unwrap();
            let result = completed.recv_timeout(Duration::from_secs(10)).unwrap();
            assert!(result.is_ok(), "{result:?}");
        });
        assert_eq!(
            settings::read_with_registry_source(root.path(), &source)
                .unwrap()
                .player_count,
            6
        );
        assert_eq!(backend.live_launch_ids().unwrap().len(), 1);
        let bytes = fs::read(root.path().join("device.yaml")).unwrap();
        assert_eq!(
            control
                .with_idle_session(|| {
                    fs::write(root.path().join("device.yaml"), b"must not execute").unwrap();
                    Ok(())
                })
                .unwrap_err()
                .code,
            "ActiveSessionConflict"
        );
        assert_eq!(fs::read(root.path().join("device.yaml")).unwrap(), bytes);
    }

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
            if let Some(events) = &state.return_events {
                events.lock().unwrap().push("stop");
            }
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
            if let Some(events) = &state.return_events {
                events.lock().unwrap().push("freeze");
            }
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

    fn late_stop_helper_preserves_replacement_seats_and_play_facts(fails: bool) {
        use super::super::input_seat::RecordingInputPool;
        use super::super::systemd_unit::InMemoryLaunchUnitBackend;
        use std::sync::mpsc;

        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(InMemoryLaunchUnitBackend::default());
        let pool = Arc::new(RecordingInputPool::default());
        let control =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), pool.clone());
        let a = control
            .prepare("a", Some(PERSON), Ok(&["game".into()]), &BTreeMap::new())
            .unwrap();
        let (entered, inside) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let released = Mutex::new(released);
        backend.configure_stop(
            true,
            fails,
            Some(Arc::new(move || {
                entered.send(()).unwrap();
                released
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap();
            })),
        );
        thread::scope(|threads| {
            let stopping = threads.spawn(|| control.stop(&a.launch_id));
            inside.recv_timeout(Duration::from_secs(10)).unwrap();
            assert_eq!(
                control.status(),
                HostSessionStatus::Completed {
                    launch_id: a.launch_id.clone()
                }
            );
            let b = control
                .prepare("b", Some(PERSON), Ok(&["game".into()]), &BTreeMap::new())
                .unwrap();
            let journal = fs::read(control.identity_root.join(ACTIVE_FILE)).unwrap();
            let calls = pool.calls.lock().unwrap().clone();
            let a_stats = stats(&control, "a");
            let b_stats = stats(&control, "b");
            release.send(()).unwrap();
            let result = stopping.join().unwrap();
            assert_eq!(
                result,
                HostSessionStop::StaleIdentity {
                    active_launch_id: Some(b.launch_id.clone())
                }
            );
            assert_eq!(
                fs::read(control.identity_root.join(ACTIVE_FILE)).unwrap(),
                journal
            );
            assert_eq!(
                pool.session.lock().unwrap().as_deref(),
                Some(b.launch_id.as_str())
            );
            assert_eq!(
                *pool.calls.lock().unwrap(),
                calls,
                "late helper must perform no input effects on B"
            );
            assert_eq!(
                control.status(),
                HostSessionStatus::Running {
                    launch_id: b.launch_id.clone(),
                    game_id: Some("b".into())
                }
            );
            assert_eq!(
                backend.state(&b.launch_id).unwrap(),
                LaunchUnitState::Running
            );
            assert_eq!(stats(&control, "a"), a_stats);
            assert_eq!(stats(&control, "b"), b_stats);
            assert_eq!(a_stats.play_count, 1);
            assert_eq!(b_stats.play_count, 0);
        });
    }

    #[test]
    fn late_successful_stop_helper_preserves_replacement_seats_and_play_facts() {
        late_stop_helper_preserves_replacement_seats_and_play_facts(false);
    }

    #[test]
    fn late_failed_stop_helper_preserves_replacement_seats_and_play_facts() {
        late_stop_helper_preserves_replacement_seats_and_play_facts(true);
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
        fn initialize(&self, _: u8, _: Option<&str>) -> Result<(), String> {
            Ok(())
        }
        fn apply_count(&self, _: u8) -> Result<(), String> {
            Ok(())
        }
        fn begin_session(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn end_session(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn route(&self, _: Option<&str>) -> Result<(), String> {
            Ok(())
        }
        fn ready(&self) -> Result<(), String> {
            Ok(())
        }
        fn is_fenced(&self) -> bool {
            false
        }
        fn fence(&self) {}
        fn start(&self, _launch_id: &str) -> Result<Box<dyn InputSeatLease>, String> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            self.alive.store(true, Ordering::SeqCst);
            Ok(Box::new(TestSeatLease {
                alive: self.alive.clone(),
            }))
        }
    }
    impl InputSeatManager for FailingSeatManager {
        fn initialize(&self, _: u8, _: Option<&str>) -> Result<(), String> {
            Ok(())
        }
        fn apply_count(&self, _: u8) -> Result<(), String> {
            Ok(())
        }
        fn begin_session(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn end_session(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn route(&self, _: Option<&str>) -> Result<(), String> {
            Ok(())
        }
        fn ready(&self) -> Result<(), String> {
            Ok(())
        }
        fn is_fenced(&self) -> bool {
            false
        }
        fn fence(&self) {}
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
        fn initialize(&self, _: u8, _: Option<&str>) -> Result<(), String> {
            Ok(())
        }
        fn apply_count(&self, _: u8) -> Result<(), String> {
            Ok(())
        }
        fn begin_session(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn end_session(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn route(&self, _: Option<&str>) -> Result<(), String> {
            Ok(())
        }
        fn ready(&self) -> Result<(), String> {
            Ok(())
        }
        fn is_fenced(&self) -> bool {
            false
        }
        fn fence(&self) {}
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
        let (status, _, overlay_intent) = recovered.status_with_recovered_overlay_intent();
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
            "--property=InaccessiblePaths=/var/lib/korrid /run/korrid /run/korrid-browser /run/korrid-control/control.sock /run/korrid-control -/home/korri/.config/sunshine /run/korri-compositor -/run/korri-certificate-control /run/user/1001 -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources".into(),
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
        let (status, ownership, overlay_intent) = recovered.status_with_recovered_overlay_intent();
        assert_eq!(ownership, Some(FocusOwnership::Excluded));
        assert_eq!(
            status,
            HostSessionStatus::FocusFailed {
                launch_id: id.clone(),
                game_id: Some("one".into()),
            }
        );
        assert_eq!(overlay_intent.as_deref(), Some(id.as_str()));
        assert_eq!(
            recovered.status_with_recovered_overlay_intent().2,
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
        let (status, ownership, overlay_intent) = recovered.status_with_recovered_overlay_intent();
        assert_eq!(ownership, Some(FocusOwnership::Launch));
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
        let (status, ownership, overlay_intent) = recovered.status_with_recovered_overlay_intent();
        assert_eq!(ownership, None);
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

    fn portal_control(
        root: &Path,
        backend: Arc<DeterministicBackend>,
        compositor: Arc<RecordingCompositor>,
        portal: Arc<RecordingPortalUnit>,
    ) -> HostSessionControl {
        resuming_control(root, backend, compositor).with_portal(portal)
    }

    #[test]
    fn portal_stays_active_until_the_exact_delayed_game_window_has_focus() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let portal = Arc::new(RecordingPortalUnit::default());
        let control = portal_control(
            root.path(),
            backend.clone(),
            compositor.clone(),
            portal.clone(),
        );
        let id = prepare(&control, "one").launch_id;
        assert!(!portal.frozen(), "process liveness is not focus");
        backend.set_pids(&id, &[9100]);
        // No mapped window, then an unrelated focused process, then a mapped
        // game behind the portal: none proves that this launch owns focus.
        for tree in [
            "{\"nodes\":[]}".to_owned(),
            compositor_tree(9200),
            compositor_tree_with_focus(9100, 2),
        ] {
            *compositor.tree.lock().unwrap() = tree;
            control.status();
            assert!(!portal.frozen());
        }
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        for _ in 0..3 {
            control.status();
        }
        assert!(portal.frozen());
        assert_eq!(portal.requests(), ["thaw", "freeze"]);
        assert!(matches!(
            control.freeze(&id),
            HostSessionFreezeChange::Changed { .. }
        ));
        assert!(!portal.frozen(), "Leave thaws before acknowledging success");
        assert!(matches!(
            control.freeze(&id),
            HostSessionFreezeChange::Unchanged { .. }
        ));
        assert_eq!(portal.requests(), ["thaw", "freeze", "thaw"]);
    }

    #[test]
    fn return_freezes_the_portal_only_after_the_game_is_focused() {
        let root = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let backend = Arc::new(DeterministicBackend::default());
        backend.state.lock().unwrap().return_events = Some(events.clone());
        let compositor = Arc::new(RecordingCompositor {
            return_events: Some(events.clone()),
            ..RecordingCompositor::default()
        });
        let portal = Arc::new(RecordingPortalUnit::with_events(events.clone()));
        let control = portal_control(root.path(), backend.clone(), compositor.clone(), portal);
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
        assert_eq!(*events.lock().unwrap(), ["thaw", "focus", "portal-freeze"]);
    }

    #[test]
    fn a_refused_return_leaves_the_portal_running() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let portal = Arc::new(RecordingPortalUnit::default());
        let control = portal_control(
            root.path(),
            backend.clone(),
            compositor.clone(),
            portal.clone(),
        );
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        control.status();
        control.freeze(&id);
        compositor.focus_fails.store(true, Ordering::SeqCst);

        assert!(matches!(
            control.thaw(&id),
            HostSessionFreezeChange::FocusFailed { .. }
        ));
        assert!(!portal.frozen());
        assert!(matches!(
            control.status(),
            HostSessionStatus::FocusFailed { .. }
        ));
        assert_eq!(portal.requests(), ["thaw", "freeze", "thaw"]);
    }

    #[test]
    fn exact_stop_thaws_the_portal_before_the_game_stops() {
        let root = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let backend = Arc::new(DeterministicBackend::default());
        backend.state.lock().unwrap().return_events = Some(events.clone());
        let portal = Arc::new(RecordingPortalUnit::with_events(events.clone()));
        let compositor = Arc::new(RecordingCompositor::default());
        let control = portal_control(
            root.path(),
            backend.clone(),
            compositor.clone(),
            portal.clone(),
        );
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        control.status();
        events.lock().unwrap().clear();

        assert_eq!(
            control.stop(&id),
            HostSessionStop::Completed {
                launch_id: id.clone()
            }
        );
        assert_eq!(*events.lock().unwrap(), ["portal-thaw", "stop"]);
        assert!(!portal.frozen());
    }

    #[test]
    fn a_game_that_ends_on_its_own_thaws_the_portal() {
        for ending in [LaunchUnitState::Completed, LaunchUnitState::Stopping] {
            let root = tempfile::tempdir().unwrap();
            let backend = Arc::new(DeterministicBackend::default());
            let portal = Arc::new(RecordingPortalUnit::default());
            let compositor = Arc::new(RecordingCompositor::default());
            let control = portal_control(
                root.path(),
                backend.clone(),
                compositor.clone(),
                portal.clone(),
            );
            let id = prepare(&control, "one").launch_id;
            backend.set_pids(&id, &[9100]);
            *compositor.tree.lock().unwrap() = compositor_tree(9100);
            control.status();
            assert!(portal.frozen());
            backend.insert(&id, ending);

            control.status();
            assert!(!portal.frozen(), "{ending:?}");
            assert!(!control.portal_watch_needed(), "{ending:?}");
        }
    }

    #[test]
    fn a_failed_launch_never_freezes_the_portal() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        backend.state.lock().unwrap().launch_rejected = true;
        let portal = Arc::new(RecordingPortalUnit::default());
        let control = portal_control(
            root.path(),
            backend,
            Arc::new(RecordingCompositor::default()),
            portal.clone(),
        );

        assert_eq!(
            control
                .prepare("one", None, Ok(&["game".into()]), &BTreeMap::new())
                .unwrap_err()
                .code,
            "HostLaunchFailed"
        );
        assert!(!portal.frozen());
        assert!(!portal.requests().contains(&"freeze"));
    }

    #[test]
    fn overlay_controls_on_a_left_game_keep_the_portal_running() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let portal = Arc::new(RecordingPortalUnit::default());
        let control = portal_control(
            root.path(),
            backend.clone(),
            Arc::new(RecordingCompositor::default()),
            portal.clone(),
        );
        let id = prepare(&control, "one").launch_id;
        control.freeze(&id);

        // The effect briefly thaws the game under one lock, then refreezes it.
        control
            .invoke_running_effect(&id, false, false, || Ok(()))
            .unwrap();
        assert_eq!(backend.state(&id).unwrap(), LaunchUnitState::Frozen);
        assert!(!portal.frozen());
        assert_eq!(portal.requests(), ["thaw"]);
    }

    #[test]
    fn restart_thaws_a_portal_left_frozen_unless_the_game_holds_focus() {
        // No live game: the leftover freeze is released on first status.
        let root = tempfile::tempdir().unwrap();
        let portal = Arc::new(RecordingPortalUnit::left_frozen());
        let control = portal_control(
            root.path(),
            Arc::new(DeterministicBackend::default()),
            Arc::new(RecordingCompositor::default()),
            portal.clone(),
        );
        assert!(control.portal_watch_needed(), "startup state is unproven");
        assert_eq!(control.status(), HostSessionStatus::NoActive);
        assert!(!portal.frozen());
        assert!(!control.portal_watch_needed());

        // A live game behind the portal: recovery hands focus to Portal.
        for (focused, frozen) in [(2, false), (3, true)] {
            let root = tempfile::tempdir().unwrap();
            let backend = Arc::new(DeterministicBackend::default());
            let compositor = Arc::new(RecordingCompositor::default());
            let first = resuming_control(root.path(), backend.clone(), compositor.clone());
            let id = prepare(&first, "one").launch_id;
            backend.set_pids(&id, &[9100]);
            *compositor.tree.lock().unwrap() = compositor_tree_with_focus(9100, focused);
            drop(first);

            let portal = Arc::new(RecordingPortalUnit::left_frozen());
            let recovered = portal_control(root.path(), backend, compositor, portal.clone());
            recovered.status();
            assert_eq!(portal.frozen(), frozen, "focused node {focused}");
        }
    }

    #[test]
    fn lost_receiver_reservations_block_frozen_resume_and_new_launch_without_recreation() {
        use super::super::{input_coordination::test_support, input_seat::UnixInputSeatManager};
        let root = tempfile::tempdir().unwrap();
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        persist_test_record(root.path(), id);
        let backend = Arc::new(DeterministicBackend::default());
        backend.insert(id, LaunchUnitState::Frozen);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let (coordinator, receiver) = test_support::coordinator(move |request| {
            seen.lock().unwrap().push(request);
            let mut reply = test_support::reply();
            reply.recovery_required = true;
            reply
        });
        let manager = Arc::new(UnixInputSeatManager::new(
            "must-not-open".into(),
            Ok(coordinator.clone()),
        ));
        let control =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), manager.clone());
        assert!(control
            .initialize_input(6)
            .unwrap_err()
            .contains("lost live session reservations"));
        assert!(manager.ready().is_err());
        assert!(matches!(
            control.thaw(id),
            HostSessionFreezeChange::HelperFailed { .. }
        ));
        assert_eq!(backend.state(id).unwrap(), LaunchUnitState::Frozen);
        assert!(backend.state.lock().unwrap().thawed.is_empty());
        assert!(control
            .prepare("new", None, Ok(&["game".into()]), &BTreeMap::new())
            .is_err());
        assert!(requests.lock().unwrap().iter().all(|request| matches!(
            request,
            korri_input_contract::SeatRequest::Hello | korri_input_contract::SeatRequest::Poll
        )));
        drop(control);
        drop(manager);
        drop(coordinator);
        receiver.join().unwrap();
        // Resolution needs actual game completion, not a replacement input
        // session. A subsequent idle startup can apply the configured count.
        backend.insert(id, LaunchUnitState::Completed);
        let (coordinator, receiver) = test_support::coordinator(|request| {
            let mut reply = test_support::reply();
            if let korri_input_contract::SeatRequest::ApplyCount { count } = request {
                reply.count = count;
            }
            reply
        });
        let manager = Arc::new(UnixInputSeatManager::new(
            "unused".into(),
            Ok(coordinator.clone()),
        ));
        let recovered = HostSessionControl::with_input_seats(root.path(), backend, manager.clone());
        recovered.initialize_input(6).unwrap();
        assert!(manager.ready().is_ok());
        drop(recovered);
        drop(manager);
        drop(coordinator);
        receiver.join().unwrap();
    }

    #[tokio::test]
    async fn terminal_control_loss_thaws_only_portal_and_keeps_native_and_launches_fenced() {
        use super::super::{input_coordination::test_support, input_seat::UnixInputSeatManager};
        use futures::{SinkExt, StreamExt};
        use korri_input_contract::SeatRequest;
        use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

        // Real seqpacket EOF and native WS; systemd/compositor are recording
        // backends. This proves the host transition, not a kernel freezer.
        for (completed, refuse_first_thaw) in [(false, false), (true, false), (true, true)] {
            let access = crate::portal_access::PortalAccess::new(
                "test-capability",
                "http://portal.local",
                crate::portal_access::PortalPermission::LocalSessions,
            );
            let (app, source) = crate::portal_input::router(access, Default::default());
            let source = Arc::new(source);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut request = format!("ws://{}/", listener.local_addr().unwrap())
                .into_client_request()
                .unwrap();
            request
                .headers_mut()
                .insert("Origin", "http://portal.local".parse().unwrap());
            let reconnect = request.clone();
            let server = tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });
            let (mut client, _) = tokio_tungstenite::connect_async(request).await.unwrap();
            client
                .send(Message::Text("Bearer test-capability".into()))
                .await
                .unwrap();
            client
                .send(Message::Text(r#"{"classes":["gamepad"]}"#.into()))
                .await
                .unwrap();
            let initial: serde_json::Value =
                serde_json::from_str(client.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(initial["kind"], "initialization-complete");

            let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            let lost = Arc::new(AtomicBool::new(false));
            let loss = lost.clone();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let received = requests.clone();
            let (coordinator, receiver) =
                test_support::coordinator_until_disconnect(Some(source.clone()), move |request| {
                    if loss.load(Ordering::SeqCst) {
                        return None;
                    }
                    received.lock().unwrap().push(request);
                    let mut reply = test_support::reply();
                    reply.session = Some(id.into());
                    Some(reply)
                });
            let root = tempfile::tempdir().unwrap();
            persist_test_record(root.path(), id);
            let backend = Arc::new(DeterministicBackend::default());
            backend.insert(id, LaunchUnitState::Running);
            backend.set_pids(id, &[9100]);
            let compositor = Arc::new(RecordingCompositor::default());
            *compositor.tree.lock().unwrap() = compositor_tree(9100);
            let portal = Arc::new(RecordingPortalUnit::default());
            let manager = Arc::new(UnixInputSeatManager::new(
                "unused-mirror-socket".into(),
                Ok(coordinator.clone()),
            ));
            let control =
                HostSessionControl::with_input_seats(root.path(), backend.clone(), manager.clone())
                    .with_compositor(compositor.clone(), vec![PORTAL_APP_ID.into()])
                    .with_portal(portal.clone())
                    .with_native_input(source.clone());
            control.initialize_input(4).unwrap();
            let observe = control.clone();
            let frozen = tokio::task::spawn_blocking(move || observe.status());
            let retire: serde_json::Value =
                serde_json::from_str(client.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(retire["kind"], "suspend");
            client.send(Message::Text(serde_json::json!({"kind":"suspended","generation":retire["generation"],"requestId":retire["requestId"]}).to_string().into())).await.unwrap();
            assert!(matches!(
                frozen.await.unwrap(),
                HostSessionStatus::Running { .. }
            ));
            assert!(portal.frozen());

            lost.store(true, Ordering::SeqCst);
            let transport = coordinator.clone();
            assert!(
                tokio::task::spawn_blocking(move || transport.request(SeatRequest::Poll))
                    .await
                    .unwrap()
                    .is_err()
            );
            receiver.join().unwrap();
            assert!(manager.is_fenced());
            if completed {
                backend.insert(id, LaunchUnitState::Completed);
            } else {
                *compositor.tree.lock().unwrap() = compositor_tree_with_focus(9100, 2);
            }
            if refuse_first_thaw {
                portal.refuse_next_thaws(1);
            }
            let observe = control.clone();
            assert_eq!(
                tokio::task::spawn_blocking(move || observe.status())
                    .await
                    .unwrap(),
                HostSessionStatus::RecoveryBlocked
            );
            if refuse_first_thaw {
                assert!(portal.frozen(), "a refused helper is not claimed as thawed");
                let retry = control.clone();
                assert_eq!(
                    tokio::task::spawn_blocking(move || retry.status())
                        .await
                        .unwrap(),
                    HostSessionStatus::RecoveryBlocked
                );
            }
            assert!(
                !portal.frozen(),
                "keyboard/mouse recovery must survive terminal input loss"
            );
            assert_eq!(
                backend.state(id).unwrap(),
                if completed {
                    LaunchUnitState::Completed
                } else {
                    LaunchUnitState::Running
                }
            );
            assert!(
                backend.state.lock().unwrap().thawed.is_empty(),
                "emergency portal thaw must not resume the game"
            );
            assert!(manager.ready().is_err());
            assert!(manager.is_fenced());
            assert!(
                root.path().join("host-session").join(ACTIVE_FILE).exists(),
                "uncertain reservation completion keeps the exact recovery record"
            );
            let attempt = control.clone();
            assert!(tokio::task::spawn_blocking(move || attempt.prepare(
                "new",
                None,
                Ok(&["game".into()]),
                &BTreeMap::new()
            ))
            .await
            .unwrap()
            .is_err());
            assert_eq!(
                requests
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|request| matches!(request, SeatRequest::BeginSession { .. }))
                    .count(),
                1
            );
            assert!(!requests.lock().unwrap().iter().any(|request| matches!(
                request,
                SeatRequest::ApplyCount { .. } | SeatRequest::EndSession { .. }
            )));
            // The original attachment is reset, not resumed. New attachment
            // also remains blocked by the original native suspension.
            tokio::time::timeout(Duration::from_secs(2), async {
                while let Some(Ok(message)) = client.next().await {
                    assert!(
                        !message.is_text(),
                        "terminal recovery must not send native resume/input"
                    );
                    if message.is_close() {
                        break;
                    }
                }
            })
            .await
            .unwrap();
            assert_eq!(
                source.suspend(Duration::from_millis(1)).await,
                Err(crate::portal_input::SuspendError::AlreadySuspended)
            );
            let error = tokio_tungstenite::connect_async(reconnect)
                .await
                .unwrap_err();
            assert!(
                matches!(error, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == axum::http::StatusCode::SERVICE_UNAVAILABLE)
            );
            drop(client);
            drop(control);
            drop(manager);
            drop(coordinator);
            drop(source);
            server.abort();
            let _ = server.await;
        }
    }

    #[test]
    fn external_focus_loss_requires_neutral_route_ack_before_thaw() {
        use super::super::input_seat::RecordingInputPool;
        for refuse in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let backend = Arc::new(DeterministicBackend::default());
            let pool = Arc::new(RecordingInputPool::default());
            let compositor = Arc::new(RecordingCompositor::default());
            let portal = Arc::new(RecordingPortalUnit::default());
            let control =
                HostSessionControl::with_input_seats(root.path(), backend.clone(), pool.clone())
                    .with_compositor(compositor.clone(), vec![PORTAL_APP_ID.into()])
                    .with_portal(portal.clone());
            let id = prepare(&control, "one").launch_id;
            backend.set_pids(&id, &[9100]);
            *compositor.tree.lock().unwrap() = compositor_tree(9100);
            control.status();
            assert!(portal.frozen());
            let (entered, enter) = std::sync::mpsc::sync_channel(1);
            let (release, released) = std::sync::mpsc::sync_channel(1);
            let mut first = true;
            let observed = portal.clone();
            *pool.on_route.lock().unwrap() = Some(Box::new(move |route| {
                assert!(route.is_none());
                assert!(
                    observed.frozen(),
                    "unit thaw must follow receiver route acknowledgement"
                );
                if first {
                    first = false;
                    entered.send(()).unwrap();
                    released.recv_timeout(Duration::from_secs(2)).unwrap();
                }
                if refuse {
                    Err("route refused".into())
                } else {
                    Ok(())
                }
            }));
            *compositor.tree.lock().unwrap() = compositor_tree_with_focus(9100, 2);
            let worker = thread::spawn(move || control.status());
            enter.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(portal.frozen());
            release.send(()).unwrap();
            let result = worker.join().unwrap();
            assert!(
                !pool.is_fenced(),
                "this is an ordinary route refusal, not terminal transport loss"
            );
            assert_eq!(portal.frozen(), refuse);
            if refuse {
                assert_eq!(result, HostSessionStatus::RecoveryBlocked);
            }
            assert_eq!(backend.state(&id).unwrap(), LaunchUnitState::Running);
        }
    }

    #[test]
    fn focus_routes_shared_pool_without_a_portal_freezer() {
        use super::super::input_seat::RecordingInputPool;
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let pool = Arc::new(RecordingInputPool::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let control =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), pool.clone())
                .with_compositor(compositor.clone(), vec![PORTAL_APP_ID.into()]);
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        control.status();
        assert_eq!(
            pool.calls.lock().unwrap().last().unwrap(),
            &format!("route:{id}")
        );
        *compositor.tree.lock().unwrap() = compositor_tree_with_focus(9100, 2);
        control.status();
        assert_eq!(pool.calls.lock().unwrap().last().unwrap(), "route:portal");
        assert_eq!(backend.state(&id).unwrap(), LaunchUnitState::Running);
        assert_eq!(pool.session.lock().unwrap().as_deref(), Some(id.as_str()));
        backend.insert(&id, LaunchUnitState::Completed);
        control.status();
        assert!(pool.session.lock().unwrap().is_none());
    }

    #[test]
    fn shared_pool_reservations_survive_pause_and_focus_failure_until_exact_completion() {
        use super::super::input_seat::RecordingInputPool;
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let pool = Arc::new(RecordingInputPool::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let control =
            HostSessionControl::with_input_seats(root.path(), backend.clone(), pool.clone())
                .with_compositor(compositor.clone(), vec![PORTAL_APP_ID.into()]);
        control.initialize_input(6).unwrap();
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        assert_eq!(pool.session.lock().unwrap().as_deref(), Some(id.as_str()));
        assert!(matches!(
            control.freeze(&id),
            HostSessionFreezeChange::Changed { .. }
        ));
        assert_eq!(pool.session.lock().unwrap().as_deref(), Some(id.as_str()));
        compositor.focus_fails.store(true, Ordering::SeqCst);
        assert!(matches!(
            control.thaw(&id),
            HostSessionFreezeChange::FocusFailed { .. }
        ));
        assert_eq!(pool.session.lock().unwrap().as_deref(), Some(id.as_str()));
        assert!(control.with_idle_session(|| Ok(())).is_err());
        assert!(matches!(
            control.stop(&id),
            HostSessionStop::Completed { .. }
        ));
        assert!(pool.session.lock().unwrap().is_none());
        let calls = pool.calls.lock().unwrap();
        assert_eq!(calls.first().unwrap(), "count:6");
        assert_eq!(
            calls
                .iter()
                .filter(|call| call.starts_with("begin:"))
                .count(),
            1
        );
        assert_eq!(
            calls.iter().filter(|call| call.starts_with("end:")).count(),
            1
        );
    }

    #[tokio::test]
    async fn native_retirement_ack_precedes_actual_portal_freeze_and_thaw_resumes() {
        for acknowledge in [true, false] {
            use futures::{SinkExt, StreamExt};
            use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
            let access = crate::portal_access::PortalAccess::new(
                "test-capability",
                "http://portal.local",
                crate::portal_access::PortalPermission::LocalSessions,
            );
            let (app, source) = crate::portal_input::router(access, Default::default());
            let source = Arc::new(source);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut request = format!("ws://{}/", listener.local_addr().unwrap())
                .into_client_request()
                .unwrap();
            request
                .headers_mut()
                .insert("Origin", "http://portal.local".parse().unwrap());
            let server = tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });
            let (mut client, _) = tokio_tungstenite::connect_async(request).await.unwrap();
            client
                .send(Message::Text("Bearer test-capability".into()))
                .await
                .unwrap();
            client
                .send(Message::Text(r#"{"classes":["gamepad"]}"#.into()))
                .await
                .unwrap();
            let initial: serde_json::Value =
                serde_json::from_str(client.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(initial["kind"], "initialization-complete");
            let root = tempfile::tempdir().unwrap();
            let backend = Arc::new(DeterministicBackend::default());
            let compositor = Arc::new(RecordingCompositor::default());
            let portal = Arc::new(RecordingPortalUnit::default());
            let control = portal_control(
                root.path(),
                backend.clone(),
                compositor.clone(),
                portal.clone(),
            )
            .with_native_input(source);
            let launch = control.clone();
            let id = tokio::task::spawn_blocking(move || prepare(&launch, "one").launch_id)
                .await
                .unwrap();
            backend.set_pids(&id, &[9100]);
            *compositor.tree.lock().unwrap() = compositor_tree(9100);
            let observe = control.clone();
            let frozen = tokio::task::spawn_blocking(move || observe.status());
            let retire: serde_json::Value =
                serde_json::from_str(client.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(retire["kind"], "suspend");
            assert!(
                !portal.frozen(),
                "the unit cannot freeze before browser retirement acknowledgement"
            );
            if !acknowledge {
                assert!(matches!(
                    frozen.await.unwrap(),
                    HostSessionStatus::FocusFailed { .. }
                ));
                assert!(
                    !portal.frozen(),
                    "missing acknowledgement must fail rather than bypass retirement"
                );
                assert_eq!(backend.state(&id).unwrap(), LaunchUnitState::Running);
                drop(client);
                drop(control);
                server.abort();
                let _ = server.await;
                continue;
            }
            client.send(Message::Text(serde_json::json!({"kind":"suspended","generation":retire["generation"],"requestId":retire["requestId"]}).to_string().into())).await.unwrap();
            assert!(matches!(
                frozen.await.unwrap(),
                HostSessionStatus::Running { .. }
            ));
            assert!(portal.frozen());
            let leave = control.clone();
            assert!(matches!(
                tokio::task::spawn_blocking(move || leave.freeze(&id))
                    .await
                    .unwrap(),
                HostSessionFreezeChange::Changed { .. }
            ));
            let resumed: serde_json::Value =
                serde_json::from_str(client.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(resumed["kind"], "resume");
            assert!(!portal.frozen());
            drop(client);
            drop(control);
            server.abort();
            let _ = server.await;
        }
    }

    #[tokio::test]
    async fn refused_best_effort_freeze_thaws_before_native_resume_without_retrying() {
        let access = crate::portal_access::PortalAccess::new(
            "test-capability",
            "http://portal.local",
            crate::portal_access::PortalPermission::LocalSessions,
        );
        let (_app, source) = crate::portal_input::router(access, Default::default());
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let portal = Arc::new(RecordingPortalUnit::default());
        let control = portal_control(
            root.path(),
            backend.clone(),
            compositor.clone(),
            portal.clone(),
        )
        .with_native_input(Arc::new(source));
        let first = control.clone();
        let id = tokio::task::spawn_blocking(move || prepare(&first, "one").launch_id)
            .await
            .unwrap();
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        portal.refuse_freeze();
        tokio::task::spawn_blocking(move || {
            assert!(matches!(
                control.status(),
                HostSessionStatus::Running { .. }
            ));
            control.status();
        })
        .await
        .unwrap();
        assert!(!portal.frozen());
        assert_eq!(portal.requests(), ["thaw", "freeze", "thaw"]);
    }

    #[test]
    fn a_refused_portal_thaw_is_retried_and_a_refused_freeze_is_not() {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(DeterministicBackend::default());
        let compositor = Arc::new(RecordingCompositor::default());
        let portal = Arc::new(RecordingPortalUnit::default());
        let control = portal_control(
            root.path(),
            backend.clone(),
            compositor.clone(),
            portal.clone(),
        );
        let id = prepare(&control, "one").launch_id;
        backend.set_pids(&id, &[9100]);
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        control.status();
        portal.refuse_next_thaws(1);

        assert!(matches!(
            control.freeze(&id),
            HostSessionFreezeChange::HelperFailed { .. }
        ));
        assert!(portal.frozen(), "the refused thaw left it frozen");
        assert_eq!(
            backend.state(&id).unwrap(),
            LaunchUnitState::Running,
            "failed Leave rolls back game freeze before returning to Game input"
        );
        assert!(control.portal_watch_needed());
        // The failed Leave kept Game input. A subsequent focus loss still
        // retries the portal thaw, independently of any browser request.
        *compositor.tree.lock().unwrap() = compositor_tree_with_focus(9100, 2);
        control.status();
        assert!(!portal.frozen(), "the next observation retried the thaw");
        control.status();
        assert_eq!(portal.requests(), ["thaw", "freeze", "thaw", "thaw"]);
        control.freeze(&id);

        portal.refuse_freeze();
        *compositor.tree.lock().unwrap() = compositor_tree(9100);
        assert_eq!(
            control.thaw(&id),
            HostSessionFreezeChange::Changed {
                launch_id: id.clone()
            }
        );
        control.status();
        control.status();
        assert!(!portal.frozen());
        assert_eq!(
            portal.requests(),
            ["thaw", "freeze", "thaw", "thaw", "freeze"]
        );
    }

    #[test]
    fn a_panicking_transition_thaws_and_allows_the_watcher_to_retry() {
        let root = tempfile::tempdir().unwrap();
        let portal = Arc::new(RecordingPortalUnit::default());
        let control = portal_control(
            root.path(),
            Arc::new(DeterministicBackend::default()),
            Arc::new(RecordingCompositor::default()),
            portal.clone(),
        );
        control.status();
        portal.refuse_next_thaws(1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = control.lock_state();
            // A panic after the native effect but before the applied-state
            // update must not trust the earlier Thawed cache.
            portal.freeze().unwrap();
            panic!("interrupted transition");
        }));
        assert!(result.is_err());
        assert!(portal.frozen());
        assert_eq!(control.status(), HostSessionStatus::NoActive);
        assert!(!portal.frozen());
    }

    #[test]
    fn failed_leave_rolls_back_before_acknowledgement_even_if_game_was_already_frozen() {
        for already in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let events = Arc::new(Mutex::new(Vec::new()));
            let backend = Arc::new(DeterministicBackend::default());
            backend.state.lock().unwrap().return_events = Some(events.clone());
            let compositor = Arc::new(RecordingCompositor {
                return_events: Some(events.clone()),
                ..RecordingCompositor::default()
            });
            let portal = Arc::new(RecordingPortalUnit::with_events(events.clone()));
            let control = portal_control(
                root.path(),
                backend.clone(),
                compositor.clone(),
                portal.clone(),
            );
            let id = prepare(&control, "one").launch_id;
            backend.set_pids(&id, &[9100]);
            *compositor.tree.lock().unwrap() = compositor_tree(9100);
            control.status();
            if already {
                backend.insert(&id, LaunchUnitState::Frozen);
            }
            portal.refuse_next_thaws(1);
            events.lock().unwrap().clear();
            assert!(matches!(
                control.freeze(&id),
                HostSessionFreezeChange::HelperFailed { .. }
            ));
            events.lock().unwrap().push("failed-ack");
            let expected = if already {
                vec!["portal-thaw", "thaw", "focus", "failed-ack"]
            } else {
                vec!["freeze", "portal-thaw", "thaw", "focus", "failed-ack"]
            };
            assert_eq!(*events.lock().unwrap(), expected);
            assert_eq!(backend.state(&id).unwrap(), LaunchUnitState::Running);
            assert!(portal.frozen());
        }
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
        assert!(manager.state.lock().unwrap().starts.is_empty());
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
        assert_eq!(manager.starts.load(Ordering::SeqCst), 0);
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
    fn failed_seat_return_from_recovered_frozen_unit_keeps_it_live() {
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

        // Status does not open a lease. Return reports the failure without
        // stopping a recovered game whose seats cannot be acquired.
        assert!(matches!(control.status(), HostSessionStatus::Frozen { .. }));
        assert!(matches!(
            control.thaw(id),
            HostSessionFreezeChange::FocusFailed { .. }
        ));
        assert_eq!(backend.state.lock().unwrap().thawed, [id]);
        assert!(backend.state.lock().unwrap().stopped.is_empty());
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
            &"--property=InaccessiblePaths=/srv/korri-test/private-recovery /run/korrid /run/korrid-browser /run/korri-test/control/device.sock /run/korri-test/control -/home/gameplay/.config/sunshine /run/korri-test/compositor-control -/run/korri-certificate-control /run/user/1001 -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources".into()
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
        // The backend kills the helper when this timeout ends, so the helper
        // must start a shell and write its pid file inside it. A loaded machine
        // missed a 30 ms window about one run in five.
        let helper_timeout = Duration::from_millis(500);
        let backend = SystemdLaunchUnitBackend::with_timeout(
            helper.clone(),
            helper,
            1000,
            1000,
            helper_timeout,
        )
        .unwrap();

        let started = Instant::now();
        let error = backend
            .run(&backend.systemctl, &["ignored".into()])
            .unwrap_err();

        assert_eq!(error.kind, LaunchUnitErrorKind::Timeout);
        let elapsed = started.elapsed();
        assert!(
            elapsed < helper_timeout + Duration::from_secs(5),
            "backend took {elapsed:?} to kill a helper with a {helper_timeout:?} timeout"
        );
        let pid = fs::read_to_string(&pid_file).unwrap_or_else(|error| {
            panic!(
                "the helper did not write {} before the backend killed it at the \
                 {helper_timeout:?} helper timeout ({error}); the helper side timed \
                 out, so the machine is too loaded for this margin",
                pid_file.display()
            )
        });
        let pid: i32 = pid.parse().unwrap();
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }

    #[test]
    fn status_never_opens_or_replaces_input_seats_but_return_does() {
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
        assert_eq!(manager.starts.load(Ordering::SeqCst), 0);
        assert!(matches!(
            control.thaw(id),
            HostSessionFreezeChange::Unchanged { .. }
        ));
        assert_eq!(manager.starts.load(Ordering::SeqCst), 1);
        alive.store(false, Ordering::SeqCst);
        assert!(matches!(
            control.status(),
            HostSessionStatus::Running { .. }
        ));
        assert_eq!(manager.starts.load(Ordering::SeqCst), 1);
        assert!(matches!(
            control.thaw(id),
            HostSessionFreezeChange::Unchanged { .. }
        ));
        assert_eq!(manager.starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn failed_seat_return_keeps_the_known_game_without_losing_stop_identity() {
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

        assert!(matches!(
            control.status(),
            HostSessionStatus::Running { .. }
        ));
        assert!(backend.state.lock().unwrap().stopped.is_empty());
        assert!(matches!(
            control.thaw(id),
            HostSessionFreezeChange::FocusFailed { .. }
        ));
        assert!(matches!(
            control.stop(id),
            HostSessionStop::Completed { .. }
        ));
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
