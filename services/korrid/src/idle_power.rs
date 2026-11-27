//! Dedicated-device inactivity policy. All time is monotonic and process-local.
//! The surface warns; only korrid's OS adapter may perform the action.
use korri_surface_treaty::{SurfaceAction, SurfaceStatus};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(crate) const CANCEL_ACTION: &str = "idle-power:cancel";
const IDLE: Duration = Duration::from_secs(15 * 60);
const WARNING: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Signals {
    pub on_battery: bool,
    pub known: bool,
    pub game: bool,
    pub stream: bool,
    pub install: bool,
    pub save_sync: bool,
}
impl Signals {
    fn eligible(self) -> bool {
        self.on_battery
            && self.known
            && !self.game
            && !self.stream
            && !self.install
            && !self.save_sync
    }
}

#[derive(Clone, Copy)]
enum Phase {
    Counting,
    Warning {
        token: u64,
        delivered: Option<Instant>,
    },
    Acted,
}
struct State {
    last_activity: Instant,
    phase: Phase,
    generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Wait,
    Warn,
    Act,
}

pub(crate) struct IdlePower {
    state: Mutex<State>,
    changes: tokio::sync::watch::Sender<u64>,
}
impl Default for IdlePower {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}
impl IdlePower {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            state: Mutex::new(State {
                last_activity: now,
                phase: Phase::Counting,
                generation: 0,
            }),
            changes: tokio::sync::watch::channel(0).0,
        }
    }
    pub(crate) fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changes.subscribe()
    }
    fn changed(&self) {
        self.changes
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }
    pub(crate) fn activity(&self) {
        self.activity_at(Instant::now());
    }
    /// True when this activity moved the timer forward.
    fn activity_at(&self, now: Instant) -> bool {
        let mut state = self.state.lock().expect("idle power mutex poisoned");
        if now <= state.last_activity {
            return false;
        }
        state.last_activity = now;
        let changed = matches!(state.phase, Phase::Warning { .. });
        state.phase = Phase::Counting;
        drop(state);
        if changed {
            self.changed();
        }
        true
    }
    pub(crate) fn check(&self, now: Instant, eligible: bool) -> Decision {
        if !eligible {
            self.activity_at(now);
            return Decision::Wait;
        }
        let mut state = self.state.lock().expect("idle power mutex poisoned");
        let elapsed = now.saturating_duration_since(state.last_activity);
        let decision = match state.phase {
            Phase::Counting if elapsed >= IDLE - WARNING => {
                state.generation = state.generation.wrapping_add(1);
                state.phase = Phase::Warning {
                    token: state.generation,
                    delivered: None,
                };
                Decision::Warn
            }
            Phase::Warning {
                delivered: Some(at),
                ..
            } if elapsed >= IDLE && now.saturating_duration_since(at) >= WARNING => {
                state.phase = Phase::Acted;
                Decision::Act
            }
            _ => Decision::Wait,
        };
        drop(state);
        if decision != Decision::Wait {
            self.changed();
        }
        decision
    }
    fn action_pending(&self) -> bool {
        matches!(
            self.state.lock().expect("idle power mutex poisoned").phase,
            Phase::Acted
        )
    }

    pub(crate) fn warning_token(&self) -> Option<u64> {
        match self.state.lock().expect("idle power mutex poisoned").phase {
            Phase::Warning { token, .. } => Some(token),
            _ => None,
        }
    }
    pub(crate) fn delivered(&self, token: u64, now: Instant) {
        let mut state = self.state.lock().expect("idle power mutex poisoned");
        if let Phase::Warning {
            token: current,
            delivered,
        } = &mut state.phase
        {
            if *current == token && delivered.is_none() {
                *delivered = Some(now);
            }
        }
    }
    pub(crate) fn status(&self) -> Option<SurfaceStatus> {
        self.warning_token()?;
        Some(SurfaceStatus::Confirm {
            kicker: "Power off soon".into(),
            detail: Some("This device is idle and on battery. It will power off in one minute. Press a button or touch the screen to keep it on.".into()),
            actions: vec![SurfaceAction::new(CANCEL_ACTION, "Keep on", true)],
            decline_action_id: Some(CANCEL_ACTION.into()), game_id: None,
        })
    }
}

/// Scoped accounting for sync, including futures that fail or are cancelled.
#[derive(Clone)]
pub(crate) struct Work(Arc<Mutex<(usize, Instant)>>);
impl Default for Work {
    fn default() -> Self {
        Self(Arc::new(Mutex::new((0, Instant::now()))))
    }
}
pub(crate) struct WorkGuard(Work);
impl Work {
    pub(crate) fn begin(&self) -> WorkGuard {
        let mut state = self.0.lock().expect("work mutex poisoned");
        state.0 += 1;
        state.1 = Instant::now();
        WorkGuard(self.clone())
    }
    pub(crate) fn busy(&self) -> bool {
        self.0.lock().expect("work mutex poisoned").0 != 0
    }
    pub(crate) fn last_activity(&self) -> Instant {
        self.0.lock().expect("work mutex poisoned").1
    }
}
impl Drop for WorkGuard {
    fn drop(&mut self) {
        let mut state = self.0 .0.lock().expect("work mutex poisoned");
        state.0 -= 1;
        state.1 = Instant::now();
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn start(state: crate::AppState) {
    // Only Nix's immutable dedicated-device declaration enables the OS effect.
    // Android never enters this composition. Tests never read a machine switch.
    if cfg!(test) || std::env::var("KORRID_DEDICATED_TO_PLAY").as_deref() != Ok("1") {
        return;
    }
    tokio::spawn(async move {
        let mut power = crate::battery::power();
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = interval.tick() => {},
                changed = power.changed() => if changed.is_err() { return; },
            }
            let signals = tokio::time::timeout(Duration::from_secs(2), signals(&state)).await;
            let eligible = signals.is_ok_and(|signals| signals.eligible())
                && power.borrow().is_some_and(|power| power.discharging);
            let decision = state.idle_power.check(Instant::now(), eligible);
            if decision == Decision::Warn {
                // The ordinary five-minute display blanking already ran.
                // A warning must be visible, not merely acknowledged in the dark.
                let woke = match &state.mode {
                    crate::ServerMode::Host(host) => host.wake_displays().await,
                    _ => Err("device has no local display control".into()),
                };
                if let Err(error) = woke {
                    eprintln!("korrid idle warning display wake failed: {error}");
                    state.idle_power.activity();
                }
            }
            if decision == Decision::Act {
                // The effect remains separate from policy: a later verified
                // device suspend changes this adapter, not the warning/timer.
                if let Err(error) = power_off(&state, &power).await {
                    eprintln!("korrid idle power-off refused: {error}");
                    state.idle_power.activity();
                }
            }
        }
    });
}

#[cfg(target_os = "linux")]
async fn signals(state: &crate::AppState) -> Signals {
    use crate::host::control::HostSessionStatus as S;
    let (known, game) = match &state.mode {
        crate::ServerMode::Host(host) => {
            if let Some(at) = host.last_input_activity() {
                if state.idle_power.activity_at(at) {
                    // Off the battery the timer restarts every second anyway.
                    korri_input_trace::record("idle_input_activity", format_args!(""));
                }
            }
            match host.session_status().await {
                Ok(S::NoActive | S::Completed { .. } | S::Failed { .. }) => {
                    (true, host.has_pending_launch())
                }
                Ok(
                    S::Running { .. }
                    | S::Frozen { .. }
                    | S::FocusFailed { .. }
                    | S::Stopping { .. },
                ) => (true, true),
                _ => (false, false),
            }
        }
        _ => (false, false),
    };
    if let Some(saves) = &state.saves {
        state
            .idle_power
            .activity_at(saves.sync_work.last_activity());
    }
    Signals {
        on_battery: true,
        known,
        game,
        stream: state
            .remote
            .as_ref()
            .is_some_and(|remote| remote.selected_launch_id().is_some())
            || state.netplay_receiver.playing(),
        install: state.acquisition.busy() || crate::steam_engine::store().busy(),
        save_sync: state
            .saves
            .as_ref()
            .is_some_and(|saves| saves.sync_work.busy())
            || crate::steam_engine::live_sessions().load(std::sync::atomic::Ordering::SeqCst) > 0,
    }
}

#[cfg(target_os = "linux")]
async fn power_off(
    state: &crate::AppState,
    power: &tokio::sync::watch::Receiver<Option<crate::battery::Power>>,
) -> Result<(), String> {
    let bus = tokio::time::timeout(Duration::from_secs(2), zbus::Connection::system())
        .await
        .map_err(|_| "system bus timed out".to_owned())?
        .map_err(|error| error.to_string())?;
    // Bus setup may yield. Re-read all inhibitors and the latest input before
    // dispatching: input or work that started during setup cancels the action.
    let eligible = tokio::time::timeout(Duration::from_secs(2), signals(state))
        .await
        .is_ok_and(|signals| signals.eligible());
    if !eligible
        || !power.borrow().is_some_and(|power| power.discharging)
        || !state.idle_power.action_pending()
    {
        state.idle_power.activity();
        return Ok(());
    }
    power_off_on(&bus).await.map_err(|error| error.to_string())
}
#[cfg(target_os = "linux")]
async fn power_off_on(bus: &zbus::Connection) -> zbus::Result<()> {
    let manager = zbus::Proxy::new(
        bus,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await?;
    tokio::time::timeout(Duration::from_secs(5), manager.call("PowerOff", &(false,)))
        .await
        .map_err(|_| zbus::Error::Failure("power-off timed out".into()))?
}

#[cfg(test)]
#[path = "idle_power_tests.rs"]
mod tests;
