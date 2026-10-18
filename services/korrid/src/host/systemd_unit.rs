use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read},
    os::fd::AsRawFd,
    path::{Component, Path, PathBuf},
    process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(test)]
use std::sync::{Arc, Mutex};

const UNIT_PREFIX: &str = "korri-game-";
const UNIT_SUFFIX: &str = ".service";
const DEFAULT_HELPER_TIMEOUT: Duration = Duration::from_secs(10);
const HELPER_POLL_INTERVAL: Duration = Duration::from_millis(5);
const MAX_HELPER_OUTPUT_BYTES: u64 = 64 * 1024;
const DEFAULT_PRIVATE_STATE_ROOT: &str = "/var/lib/korrid";
const DEFAULT_CONTROL_SOCKET: &str = "/run/korrid-control/control.sock";
const DEFAULT_CONTROL_DIRECTORY: &str = "/run/korrid-control";
const DEFAULT_BROWSER_RUNTIME_DIRECTORY: &str = "/run/korrid-browser";
const DEFAULT_COMPOSITOR_CONTROL_DIRECTORY: &str = "/run/korri-compositor";
/// Control group of a system unit started by systemd-run.
const DEFAULT_UNIT_CGROUP_ROOT: &str = "/sys/fs/cgroup/system.slice";
const DEFAULT_CERTIFICATE_CONTROL_DIRECTORY: &str = "/run/korri-certificate-control";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchUnitState {
    Running,
    /// The unit is active and its control group is settled frozen.
    Frozen,
    /// The unit is active and its control group is between running and
    /// frozen (`freezing` or `thawing`). The settled state is unknown, so a
    /// freezer request must issue its verb instead of short-circuiting.
    FreezerTransition,
    Stopping,
    Completed,
}

impl LaunchUnitState {
    /// The unit is active but its processes are not (or may not be)
    /// scheduled. A stop must thaw such a unit first; systemd refuses
    /// `stop` on a frozen unit.
    pub(super) fn needs_thaw_before_stop(self) -> bool {
        matches!(self, Self::Frozen | Self::FreezerTransition)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchUnitErrorKind {
    InvalidConfiguration,
    Spawn,
    Timeout,
    Failed,
    /// The game unit ended unsuccessfully, not a failed observation helper.
    UnitFailed,
    Protocol,
    OutputLimit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchUnitError {
    pub kind: LaunchUnitErrorKind,
    pub message: String,
}

impl LaunchUnitError {
    pub(super) fn new(kind: LaunchUnitErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

pub(super) const RUNNER_ID_ENV: &str = "KORRI_LIVE_RUNNER_ID";

pub trait LaunchUnitBackend: Send + Sync {
    fn launch(
        &self,
        launch_id: &str,
        command: &[String],
        environment: &BTreeMap<String, String>,
    ) -> Result<(), LaunchUnitError>;
    fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError>;
    /// Stops the unit from any live state. A backend must thaw a frozen
    /// unit itself before stopping it; callers never issue a thaw before
    /// a stop.
    fn stop(&self, launch_id: &str) -> Result<(), LaunchUnitError>;
    fn freeze(&self, launch_id: &str) -> Result<(), LaunchUnitError>;
    fn thaw(&self, launch_id: &str) -> Result<(), LaunchUnitError>;
    fn live_launch_ids(&self) -> Result<Vec<String>, LaunchUnitError>;
    /// Runner identity retained by the live transient unit. This is runtime
    /// discovery, not part of the durable active-session journal.
    fn runner_id(&self, _launch_id: &str) -> Result<Option<String>, LaunchUnitError> {
        Ok(None)
    }
    /// Processes of this launch, used to recognize its compositor windows.
    /// A backend that cannot see processes reports none, which makes korrid
    /// focus nothing rather than guess at another program's window.
    fn window_pids(&self, _launch_id: &str) -> Result<BTreeSet<i32>, LaunchUnitError> {
        Ok(BTreeSet::new())
    }
}

/// Names the portal unit korrid freezes while a focused game owns the
/// screen. Absent, the portal is never frozen.
pub(super) const PORTAL_UNIT_ENV: &str = "KORRID_PORTAL_UNIT";

/// The portal surface korrid pauses while the player is in a focused live
/// game. A frozen portal can neither paint nor take input, so every thaw
/// request must leave it running.
pub trait PortalUnit: Send + Sync {
    fn freeze(&self) -> Result<(), LaunchUnitError>;
    /// Leaves the unit running. A unit that is not frozen, including one
    /// that is inactive or not loaded, needs no thaw and succeeds.
    fn thaw(&self) -> Result<(), LaunchUnitError>;
}

/// Calls native FreezeUnit/ThawUnit through busctl, with the same bounded
/// process runner as game controls. Unlike systemctl this takes no RefUnit
/// authority. systemd holds the reply until the kernel freezer settles.
#[derive(Clone, Debug)]
pub struct SystemdPortalUnit {
    helper: SystemdLaunchUnitBackend,
    unit: String,
}

impl SystemdPortalUnit {
    pub(super) fn new(
        helper: SystemdLaunchUnitBackend,
        unit: &str,
    ) -> Result<Self, LaunchUnitError> {
        if !valid_portal_unit(unit) {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "portal unit must be one exact service name that is not a game unit",
            ));
        }
        Ok(Self {
            helper,
            unit: unit.to_owned(),
        })
    }

    /// Reads the configured portal unit. `None` keeps the feature inert.
    pub(super) fn from_environment() -> Option<Result<Self, LaunchUnitError>> {
        let unit = std::env::var(PORTAL_UNIT_ENV).ok()?;
        Some(Self::new(SystemdLaunchUnitBackend::default(), &unit))
    }

    /// systemd answers a freezer call only once the freezer settles, so the
    /// call returns with the portal frozen or running.
    fn verb_arguments(&self, method: &str) -> Vec<String> {
        vec![
            "--system".into(),
            "--allow-interactive-authorization=no".into(),
            "call".into(),
            "org.freedesktop.systemd1".into(),
            "/org/freedesktop/systemd1".into(),
            "org.freedesktop.systemd1.Manager".into(),
            method.into(),
            "s".into(),
            self.unit.clone(),
        ]
    }

    fn call(&self, method: &str) -> Result<(), LaunchUnitError> {
        // Both executables belong to the configured native systemd artifact.
        self.helper
            .require_success(
                &self.helper.systemctl.with_file_name("busctl"),
                &self.verb_arguments(method),
            )
            .map(|_| ())
    }
}

impl PortalUnit for SystemdPortalUnit {
    fn freeze(&self) -> Result<(), LaunchUnitError> {
        self.call("FreezeUnit")
    }

    /// systemd refuses `thaw` on an inactive unit. Read the freezer state
    /// first so a stopped or absent portal is not a thaw failure that korrid
    /// would retry on every status read.
    fn thaw(&self) -> Result<(), LaunchUnitError> {
        let state = self.helper.observe_state(&[
            "--system".into(),
            "--no-ask-password".into(),
            "show".into(),
            self.unit.clone(),
            "--property=LoadState".into(),
            "--property=ActiveState".into(),
            "--property=FreezerState".into(),
        ])?;
        if !state.needs_thaw_before_stop() {
            return Ok(());
        }
        self.call("ThawUnit")
    }
}

fn valid_portal_unit(unit: &str) -> bool {
    let Some(stem) = unit.strip_suffix(UNIT_SUFFIX) else {
        return false;
    };
    unit.len() <= 255
        && !unit.starts_with(UNIT_PREFIX)
        && stem
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && stem
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:@".contains(&byte))
}

/// Records portal freezer requests the way the systemd portal unit applies
/// them, so session tests can prove when korrid freezes and thaws it.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct RecordingPortalUnit {
    frozen: Mutex<bool>,
    requests: Mutex<Vec<&'static str>>,
    events: Option<Arc<Mutex<Vec<&'static str>>>>,
    refused_thaws: Mutex<usize>,
    refuse_freeze: Mutex<bool>,
    pub(crate) thawed: tokio::sync::Notify,
}

#[cfg(test)]
impl RecordingPortalUnit {
    /// A portal an earlier korrid run froze and never thawed.
    pub(crate) fn left_frozen() -> Self {
        Self {
            frozen: Mutex::new(true),
            ..Self::default()
        }
    }

    /// Adds `portal-freeze` and `portal-thaw` to a log shared with other
    /// recorders so tests can prove ordering across effects.
    pub(crate) fn with_events(events: Arc<Mutex<Vec<&'static str>>>) -> Self {
        Self {
            events: Some(events),
            ..Self::default()
        }
    }

    pub(crate) fn frozen(&self) -> bool {
        *self.frozen.lock().unwrap()
    }

    pub(crate) fn requests(&self) -> Vec<&'static str> {
        self.requests.lock().unwrap().clone()
    }

    pub(crate) fn refuse_next_thaws(&self, count: usize) {
        *self.refused_thaws.lock().unwrap() = count;
    }

    pub(crate) fn refuse_freeze(&self) {
        *self.refuse_freeze.lock().unwrap() = true;
    }

    fn record(&self, request: &'static str, event: &'static str) {
        self.requests.lock().unwrap().push(request);
        if let Some(events) = &self.events {
            events.lock().unwrap().push(event);
        }
    }
}

#[cfg(test)]
impl PortalUnit for RecordingPortalUnit {
    fn freeze(&self) -> Result<(), LaunchUnitError> {
        self.record("freeze", "portal-freeze");
        if *self.refuse_freeze.lock().unwrap() {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "portal freeze refused",
            ));
        }
        *self.frozen.lock().unwrap() = true;
        Ok(())
    }

    fn thaw(&self) -> Result<(), LaunchUnitError> {
        self.record("thaw", "portal-thaw");
        let mut refused = self.refused_thaws.lock().unwrap();
        if *refused > 0 {
            *refused -= 1;
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "portal thaw refused",
            ));
        }
        *self.frozen.lock().unwrap() = false;
        self.thawed.notify_one();
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct ProtectedPaths {
    private_state_root: PathBuf,
    control_socket: PathBuf,
    control_directory: PathBuf,
    stream_private_state_root: PathBuf,
    compositor_control_directory: PathBuf,
    certificate_control_directory: PathBuf,
}

/// Reads one unit's control group. A missing group means the unit already
/// finished or has not started, which is an empty process set rather than a
/// failure. Only an unreadable group is reported as an error.
pub(super) fn read_unit_pids(root: &Path, unit: &str) -> Result<BTreeSet<i32>, LaunchUnitError> {
    let procs = root.join(unit).join("cgroup.procs");
    let text = match std::fs::read_to_string(&procs) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                format!("could not read {}: {error}", procs.display()),
            ))
        }
    };
    Ok(text
        .lines()
        .filter_map(|line| line.trim().parse::<i32>().ok())
        .filter(|pid| *pid > 0)
        .collect())
}

#[derive(Clone, Debug)]
pub struct SystemdLaunchUnitBackend {
    pub(super) systemd_run: PathBuf,
    pub(super) systemctl: PathBuf,
    runtime_uid: u32,
    runtime_gid: u32,
    private_state_root: PathBuf,
    control_socket: PathBuf,
    control_directory: PathBuf,
    stream_private_state_root: PathBuf,
    compositor_control_directory: PathBuf,
    certificate_control_directory: PathBuf,
    compositor_wayland_socket: Option<PathBuf>,
    diagnostics_root: PathBuf,
    helper_timeout: Duration,
}

impl Default for SystemdLaunchUnitBackend {
    fn default() -> Self {
        Self::for_current_process()
    }
}

#[cfg(test)]
#[derive(Clone)]
struct StopBehavior {
    completes: bool,
    fails: bool,
    probe: Option<Arc<dyn Fn() + Send + Sync>>,
}

#[cfg(test)]
impl Default for StopBehavior {
    fn default() -> Self {
        Self {
            completes: true,
            fails: false,
            probe: None,
        }
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct InMemoryLaunchUnitBackend {
    units: Mutex<BTreeMap<String, LaunchUnitState>>,
    runners: Mutex<BTreeMap<String, String>>,
    window_pids: Mutex<BTreeMap<String, BTreeSet<i32>>>,
    thawed: Mutex<Vec<String>>,
    launch_probe: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    stop_behavior: Mutex<StopBehavior>,
    state_unavailable: std::sync::atomic::AtomicBool,
    launch_state: Mutex<Option<LaunchUnitState>>,
}

#[cfg(test)]
impl InMemoryLaunchUnitBackend {
    pub(crate) fn set_launch_probe(&self, probe: Arc<dyn Fn() + Send + Sync>) {
        *self.launch_probe.lock().unwrap() = Some(probe);
    }

    pub(crate) fn configure_stop(
        &self,
        completes: bool,
        fails: bool,
        probe: Option<Arc<dyn Fn() + Send + Sync>>,
    ) {
        *self.stop_behavior.lock().unwrap() = StopBehavior {
            completes,
            fails,
            probe,
        };
    }

    pub(crate) fn set_state_unavailable(&self, unavailable: bool) {
        self.state_unavailable
            .store(unavailable, std::sync::atomic::Ordering::SeqCst);
    }

    pub(crate) fn set_launch_completed(&self, completed: bool) {
        *self.launch_state.lock().unwrap() = completed.then_some(LaunchUnitState::Completed);
    }

    pub(crate) fn set_launch_stopping(&self) {
        *self.launch_state.lock().unwrap() = Some(LaunchUnitState::Stopping);
    }

    pub(crate) fn launch_count(&self) -> usize {
        self.units.lock().unwrap().len()
    }

    pub(crate) fn set_window_pids(&self, launch_id: &str, pids: BTreeSet<i32>) {
        self.window_pids
            .lock()
            .unwrap()
            .insert(launch_id.into(), pids);
    }

    pub(crate) fn complete_live(&self) {
        let mut units = self.units.lock().unwrap();
        let launch_id = units
            .iter()
            .find(|(_, state)| **state != LaunchUnitState::Completed)
            .map(|(launch_id, _)| launch_id.clone())
            .expect("one live test launch");
        units.insert(launch_id, LaunchUnitState::Completed);
    }

    pub(crate) fn thaw_count(&self) -> usize {
        self.thawed.lock().unwrap().len()
    }
}

#[cfg(test)]
impl LaunchUnitBackend for InMemoryLaunchUnitBackend {
    fn window_pids(&self, launch_id: &str) -> Result<BTreeSet<i32>, LaunchUnitError> {
        Ok(self
            .window_pids
            .lock()
            .unwrap()
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
        if let Some(probe) = self.launch_probe.lock().unwrap().clone() {
            probe();
        }
        self.units.lock().unwrap().insert(
            launch_id.into(),
            self.launch_state
                .lock()
                .unwrap()
                .unwrap_or(LaunchUnitState::Running),
        );
        if let Some(runner_id) = environment.get(RUNNER_ID_ENV) {
            self.runners
                .lock()
                .unwrap()
                .insert(launch_id.into(), runner_id.clone());
        }
        Ok(())
    }

    fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError> {
        if self
            .state_unavailable
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "native unit observation unavailable",
            ));
        }
        Ok(self
            .units
            .lock()
            .unwrap()
            .get(launch_id)
            .copied()
            .unwrap_or(LaunchUnitState::Completed))
    }

    /// Mirrors systemd: a frozen unit refuses `stop`, so the backend thaws
    /// first and then stops.
    fn stop(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        let mut units = self.units.lock().unwrap();
        if units
            .get(launch_id)
            .is_some_and(|state| state.needs_thaw_before_stop())
        {
            units.insert(launch_id.into(), LaunchUnitState::Running);
        }
        let behavior = self.stop_behavior.lock().unwrap().clone();
        if behavior.completes {
            units.insert(launch_id.into(), LaunchUnitState::Completed);
        }
        drop(units);
        if let Some(probe) = behavior.probe {
            probe();
        }
        if behavior.fails {
            Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "stop helper failed",
            ))
        } else {
            Ok(())
        }
    }

    fn freeze(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        let mut units = self.units.lock().unwrap();
        match units.get(launch_id).copied() {
            Some(
                LaunchUnitState::Running
                | LaunchUnitState::Frozen
                | LaunchUnitState::FreezerTransition,
            ) => {
                units.insert(launch_id.into(), LaunchUnitState::Frozen);
                Ok(())
            }
            _ => Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "unit is not active",
            )),
        }
    }

    fn thaw(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        self.thawed.lock().unwrap().push(launch_id.into());
        let mut units = self.units.lock().unwrap();
        match units.get(launch_id).copied() {
            Some(
                LaunchUnitState::Running
                | LaunchUnitState::Frozen
                | LaunchUnitState::FreezerTransition,
            ) => {
                units.insert(launch_id.into(), LaunchUnitState::Running);
                Ok(())
            }
            _ => Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "unit is not active",
            )),
        }
    }

    fn runner_id(&self, launch_id: &str) -> Result<Option<String>, LaunchUnitError> {
        Ok(self.runners.lock().unwrap().get(launch_id).cloned())
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

impl SystemdLaunchUnitBackend {
    #[cfg(test)]
    pub fn new(
        systemd_run: PathBuf,
        systemctl: PathBuf,
        runtime_uid: u32,
        runtime_gid: u32,
    ) -> Result<Self, LaunchUnitError> {
        Self::with_protected_paths(
            systemd_run,
            systemctl,
            runtime_uid,
            runtime_gid,
            PathBuf::from(DEFAULT_PRIVATE_STATE_ROOT),
            PathBuf::from(DEFAULT_CONTROL_SOCKET),
            PathBuf::from(DEFAULT_CONTROL_DIRECTORY),
            PathBuf::from("/home/korri/.config/sunshine"),
            PathBuf::from(DEFAULT_COMPOSITOR_CONTROL_DIRECTORY),
        )
    }

    #[cfg(test)]
    pub fn with_protected_paths(
        systemd_run: PathBuf,
        systemctl: PathBuf,
        runtime_uid: u32,
        runtime_gid: u32,
        private_state_root: PathBuf,
        control_socket: PathBuf,
        control_directory: PathBuf,
        stream_private_state_root: PathBuf,
        compositor_control_directory: PathBuf,
    ) -> Result<Self, LaunchUnitError> {
        Self::with_timeout_and_paths(
            systemd_run,
            systemctl,
            runtime_uid,
            runtime_gid,
            ProtectedPaths {
                private_state_root,
                control_socket,
                control_directory,
                stream_private_state_root,
                compositor_control_directory,
                certificate_control_directory: PathBuf::from(DEFAULT_CERTIFICATE_CONTROL_DIRECTORY),
            },
            DEFAULT_HELPER_TIMEOUT,
        )
    }

    #[cfg(test)]
    pub(super) fn with_timeout(
        systemd_run: PathBuf,
        systemctl: PathBuf,
        runtime_uid: u32,
        runtime_gid: u32,
        helper_timeout: Duration,
    ) -> Result<Self, LaunchUnitError> {
        Self::with_timeout_and_paths(
            systemd_run,
            systemctl,
            runtime_uid,
            runtime_gid,
            ProtectedPaths {
                private_state_root: PathBuf::from(DEFAULT_PRIVATE_STATE_ROOT),
                control_socket: PathBuf::from(DEFAULT_CONTROL_SOCKET),
                control_directory: PathBuf::from(DEFAULT_CONTROL_DIRECTORY),
                stream_private_state_root: PathBuf::from("/home/korri/.config/sunshine"),
                compositor_control_directory: PathBuf::from(DEFAULT_COMPOSITOR_CONTROL_DIRECTORY),
                certificate_control_directory: PathBuf::from(DEFAULT_CERTIFICATE_CONTROL_DIRECTORY),
            },
            helper_timeout,
        )
    }

    fn with_timeout_and_paths(
        systemd_run: PathBuf,
        systemctl: PathBuf,
        runtime_uid: u32,
        runtime_gid: u32,
        paths: ProtectedPaths,
        helper_timeout: Duration,
    ) -> Result<Self, LaunchUnitError> {
        let ProtectedPaths {
            private_state_root,
            control_socket,
            control_directory,
            stream_private_state_root,
            compositor_control_directory,
            certificate_control_directory,
        } = paths;
        if !systemd_run.is_absolute() || !systemctl.is_absolute() {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "systemd helper paths must be absolute",
            ));
        }
        if runtime_uid == 0 || runtime_gid == 0 {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "runtime UID and GID must both be unprivileged",
            ));
        }
        if helper_timeout.is_zero() {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "systemd helper timeout must be positive",
            ));
        }
        for (name, path) in [
            ("private state root", &private_state_root),
            ("control socket", &control_socket),
            ("control directory", &control_directory),
            ("stream-host private state root", &stream_private_state_root),
            (
                "compositor control directory",
                &compositor_control_directory,
            ),
            (
                "certificate control directory",
                &certificate_control_directory,
            ),
        ] {
            if !valid_protected_path(path) {
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::InvalidConfiguration,
                    format!("{name} must be a normalized absolute path without whitespace"),
                ));
            }
        }
        if control_socket.parent() != Some(control_directory.as_path()) {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "control socket must be directly inside the configured control directory",
            ));
        }
        Ok(Self {
            systemd_run,
            systemctl,
            runtime_uid,
            runtime_gid,
            private_state_root,
            control_socket,
            control_directory,
            stream_private_state_root,
            compositor_control_directory,
            certificate_control_directory,
            compositor_wayland_socket: None,
            diagnostics_root: PathBuf::from("/run/korrid/launches"),
            helper_timeout,
        })
    }

    fn for_current_process() -> Self {
        let systemd_run = std::env::var_os("KORRID_SYSTEMD_RUN")
            .map(PathBuf::from)
            .unwrap_or_default();
        let systemctl = std::env::var_os("KORRID_SYSTEMCTL")
            .map(PathBuf::from)
            .unwrap_or_default();
        let runtime_uid = configured_runtime_id("KORRID_RUNTIME_UID")
            .unwrap_or_else(|| unsafe { libc::geteuid() });
        let runtime_gid = configured_runtime_id("KORRID_RUNTIME_GID")
            .unwrap_or_else(|| unsafe { libc::getegid() });
        let private_state_root =
            configured_path("KORRID_PRIVATE_STATE_ROOT", DEFAULT_PRIVATE_STATE_ROOT);
        let control_socket = configured_path("KORRID_CONTROL_SOCKET", DEFAULT_CONTROL_SOCKET);
        let control_directory =
            configured_path("KORRID_CONTROL_DIRECTORY", DEFAULT_CONTROL_DIRECTORY);
        let stream_private_state_root = std::env::var_os("KORRID_STREAM_PRIVATE_STATE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_default();
        let compositor_control_directory = configured_path(
            "KORRID_COMPOSITOR_CONTROL_DIRECTORY",
            DEFAULT_COMPOSITOR_CONTROL_DIRECTORY,
        );
        let certificate_control_directory = configured_path(
            "KORRID_CERTIFICATE_CONTROL_DIRECTORY",
            DEFAULT_CERTIFICATE_CONTROL_DIRECTORY,
        );
        Self::with_timeout_and_paths(
            systemd_run.clone(),
            systemctl.clone(),
            runtime_uid,
            runtime_gid,
            ProtectedPaths {
                private_state_root: private_state_root.clone(),
                control_socket: control_socket.clone(),
                control_directory: control_directory.clone(),
                stream_private_state_root: stream_private_state_root.clone(),
                compositor_control_directory: compositor_control_directory.clone(),
                certificate_control_directory: certificate_control_directory.clone(),
            },
            DEFAULT_HELPER_TIMEOUT,
        )
        .unwrap_or(Self {
            systemd_run,
            systemctl,
            runtime_uid,
            runtime_gid,
            private_state_root,
            control_socket,
            control_directory,
            stream_private_state_root,
            compositor_control_directory,
            certificate_control_directory,
            compositor_wayland_socket: None,
            diagnostics_root: PathBuf::from("/run/korrid/launches"),
            helper_timeout: DEFAULT_HELPER_TIMEOUT,
        })
        .with_compositor_wayland_socket(
            std::env::var_os("KORRID_COMPOSITOR_WAYLAND_SOCKET").map(PathBuf::from),
        )
    }

    pub(super) fn with_compositor_wayland_socket(mut self, socket: Option<PathBuf>) -> Self {
        self.compositor_wayland_socket = socket;
        self
    }

    fn validate(&self) -> Result<(), LaunchUnitError> {
        Self::with_timeout_and_paths(
            self.systemd_run.clone(),
            self.systemctl.clone(),
            self.runtime_uid,
            self.runtime_gid,
            ProtectedPaths {
                private_state_root: self.private_state_root.clone(),
                control_socket: self.control_socket.clone(),
                control_directory: self.control_directory.clone(),
                stream_private_state_root: self.stream_private_state_root.clone(),
                compositor_control_directory: self.compositor_control_directory.clone(),
                certificate_control_directory: self.certificate_control_directory.clone(),
            },
            self.helper_timeout,
        )
        .map(|_| ())
    }

    pub(super) fn unit_name(launch_id: &str) -> Result<String, LaunchUnitError> {
        validate_launch_id(launch_id)?;
        Ok(format!("{UNIT_PREFIX}{launch_id}{UNIT_SUFFIX}"))
    }

    fn wait_until_exit(
        child: &mut std::process::Child,
        deadline: Instant,
    ) -> Result<Option<ExitStatus>, std::io::Error> {
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(Some(status));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(
                HELPER_POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    fn set_nonblocking(fd: &impl AsRawFd, name: &str) -> Result<(), LaunchUnitError> {
        let descriptor = fd.as_raw_fd();
        let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
        if flags == -1
            || unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
        {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Spawn,
                format!(
                    "could not configure {name} as bounded pipe: {}",
                    io::Error::last_os_error()
                ),
            ));
        }
        Ok(())
    }

    fn drain_helper_output(
        reader: &mut impl Read,
        bytes: &mut Vec<u8>,
        name: &str,
    ) -> Result<bool, LaunchUnitError> {
        let mut chunk = [0_u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => return Ok(true),
                Ok(count) => {
                    if bytes.len() + count > MAX_HELPER_OUTPUT_BYTES as usize {
                        return Err(LaunchUnitError::new(
                            LaunchUnitErrorKind::OutputLimit,
                            format!("{name} exceeded {MAX_HELPER_OUTPUT_BYTES} bytes"),
                        ));
                    }
                    bytes.extend_from_slice(&chunk[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    return Err(LaunchUnitError::new(
                        LaunchUnitErrorKind::Protocol,
                        format!("could not read {name}: {error}"),
                    ));
                }
            }
        }
    }

    fn terminate_and_reap(child: &mut Child, deadline: Instant) -> Result<(), LaunchUnitError> {
        let kill_error = child.kill().err();
        match Self::wait_until_exit(child, deadline) {
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                "systemd helper could not be reaped after termination",
            )),
            Err(error) => Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                format!("systemd helper reap failed after termination: {error}"),
            )),
        }
        .map_err(|mut error| {
            if let Some(kill_error) = kill_error {
                error
                    .message
                    .push_str(&format!("; kill reported {kill_error}"));
            }
            error
        })
    }

    fn bounded_output_error(
        child: &mut Child,
        deadline: Instant,
        error: LaunchUnitError,
    ) -> LaunchUnitError {
        match Self::terminate_and_reap(child, deadline) {
            Ok(()) => error,
            Err(reap_error) => LaunchUnitError::new(
                error.kind,
                format!("{}; {}", error.message, reap_error.message),
            ),
        }
    }

    pub(super) fn run(
        &self,
        program: &Path,
        arguments: &[String],
    ) -> Result<Output, LaunchUnitError> {
        self.validate()?;
        let display = program.display();
        let mut child = Command::new(program)
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                LaunchUnitError::new(
                    LaunchUnitErrorKind::Spawn,
                    format!("could not execute {display}: {error}"),
                )
            })?;
        let mut stdout: ChildStdout = child.stdout.take().expect("piped helper stdout");
        let mut stderr: ChildStderr = child.stderr.take().expect("piped helper stderr");
        if let Err(error) = Self::set_nonblocking(&stdout, "systemd helper stdout")
            .and_then(|_| Self::set_nonblocking(&stderr, "systemd helper stderr"))
        {
            return Err(Self::bounded_output_error(
                &mut child,
                Instant::now() + self.helper_timeout,
                error,
            ));
        }
        let mut stdout_bytes = Vec::new();
        let mut stderr_bytes = Vec::new();
        let deadline = Instant::now() + self.helper_timeout;
        let status = loop {
            for result in [
                Self::drain_helper_output(&mut stdout, &mut stdout_bytes, "systemd helper stdout"),
                Self::drain_helper_output(&mut stderr, &mut stderr_bytes, "systemd helper stderr"),
            ] {
                if let Err(error) = result {
                    return Err(Self::bounded_output_error(
                        &mut child,
                        Instant::now() + self.helper_timeout,
                        error,
                    ));
                }
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    Self::drain_helper_output(
                        &mut stdout,
                        &mut stdout_bytes,
                        "systemd helper stdout",
                    )
                    .and_then(|_| {
                        Self::drain_helper_output(
                            &mut stderr,
                            &mut stderr_bytes,
                            "systemd helper stderr",
                        )
                    })?;
                    break status;
                }
                Ok(None) if Instant::now() < deadline => thread::sleep(
                    HELPER_POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
                ),
                Ok(None) => {
                    let error = LaunchUnitError::new(
                        LaunchUnitErrorKind::Timeout,
                        format!("systemd helper {display} timed out"),
                    );
                    return Err(Self::bounded_output_error(
                        &mut child,
                        Instant::now() + self.helper_timeout,
                        error,
                    ));
                }
                Err(error) => {
                    let error = LaunchUnitError::new(
                        LaunchUnitErrorKind::Protocol,
                        format!("could not wait for systemd helper {display}: {error}"),
                    );
                    return Err(Self::bounded_output_error(
                        &mut child,
                        Instant::now() + self.helper_timeout,
                        error,
                    ));
                }
            }
        };
        Ok(Output {
            status,
            stdout: stdout_bytes,
            stderr: stderr_bytes,
        })
    }

    pub(super) fn launch_arguments(
        &self,
        launch_id: &str,
        configured_command: &[String],
        environment: &BTreeMap<String, String>,
    ) -> Result<Vec<String>, LaunchUnitError> {
        self.validate()?;
        let unit = Self::unit_name(launch_id)?;
        if configured_command.is_empty() {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "configured command is empty",
            ));
        }
        let modifiers = crate::launcher::plugin_launch::has_modifiers(configured_command);
        // An unwrapped game draws through the compositor's Xwayland display.
        // A modifier such as nested Gamescope runs its own Xwayland and must
        // create a fresh socket in the unit's private /tmp/.X11-unix. A
        // read-only bind of X0 there turns that directory read-only and
        // Gamescope finds no free display (it then crashes at start).
        let mut read_only_binds = if modifiers {
            String::new()
        } else {
            String::from("/tmp/.X11-unix/X0")
        };
        if environment.get("PULSE_SERVER").map(String::as_str)
            == Some("unix:/run/korri-game-audio/native")
        {
            // Expose only Pulse outside the hidden runtime, never its control sockets.
            read_only_binds.push_str(&format!(
                " /run/user/{}/pulse:/run/korri-game-audio",
                self.runtime_uid
            ));
        }
        let modifier_runtime = if modifiers {
            let socket = self
                .compositor_wayland_socket
                .as_ref()
                .filter(|path| valid_protected_path(path))
                .ok_or_else(|| {
                    LaunchUnitError::new(
                        LaunchUnitErrorKind::InvalidConfiguration,
                        "launch modifiers require the published compositor Wayland socket",
                    )
                })?;
            let runtime = format!("korri-game-{launch_id}");
            // A Wayland client of Sway can reach protocols unavailable to X11
            // clients (for example screencopy and virtual-keyboard, unless Sway
            // restricts them). Limit this access to resolved modifier launches.
            // Bind only korri-publish-wayland-socket's single published socket;
            // all control sockets and the rest of /run/user remain inaccessible.
            read_only_binds.push_str(&format!(
                " {}:/run/{runtime}/wayland-parent",
                socket.display()
            ));
            Some(runtime)
        } else {
            None
        };
        let mut arguments = vec![
            "--system".into(),
            "--no-ask-password".into(),
            "--quiet".into(),
            // Default CollectMode=inactive retains failed-unit exit metadata.
            // The controller stops and resets the exact failed unit after reading it.
            "--service-type=exec".into(),
            format!("--unit={unit}"),
            format!("--uid={}", self.runtime_uid),
            format!("--gid={}", self.runtime_gid),
            "--property=KillMode=control-group".into(),
            // STATUS= retains a plugin-launch refusal in the existing unit failure path.
            "--property=NotifyAccess=main".into(),
            "--property=NoNewPrivileges=yes".into(),
            "--property=CapabilityBoundingSet=".into(),
            "--property=AmbientCapabilities=".into(),
            "--property=PrivateTmp=yes".into(),
            "--property=PrivatePIDs=yes".into(),
            format!("--property=BindReadOnlyPaths={}", read_only_binds.trim_start()),
            // Kernel tunables stay unwritable to the unprivileged runtime user
            // without this. Steam's FHS and pressure-vessel run bubblewrap,
            // which reads /proc/sys/kernel/overflowuid and mounts a new proc in
            // its user namespace; both need a fully visible proc (no /proc/sys
            // overmount, no pid-only subset). Other processes stay invisible.
            "--property=ProtectKernelModules=yes".into(),
            "--property=ProtectControlGroups=yes".into(),
            "--property=ProtectProc=invisible".into(),
            // The stream-host paths exist only when a stream-host plugin is
            // installed. systemd refuses to start the unit for a missing
            // path unless it has the "-" prefix.
            format!(
                "--property=InaccessiblePaths={} /run/korrid {} {} {} -{} {} -{} /run/user/{} -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources",
                self.private_state_root.display(),
                DEFAULT_BROWSER_RUNTIME_DIRECTORY,
                self.control_socket.display(),
                self.control_directory.display(),
                self.stream_private_state_root.display(),
                self.compositor_control_directory.display(),
                self.certificate_control_directory.display(),
                self.runtime_uid
            ),
            "--property=RestrictSUIDSGID=yes".into(),
        ];
        if let Some(runtime) = modifier_runtime {
            arguments.extend([
                format!("--property=RuntimeDirectory={runtime}"),
                "--property=RuntimeDirectoryMode=0700".into(),
                format!(
                    "--property=BindPaths={}:/run/{runtime}",
                    self.diagnostics_root.join(launch_id).display()
                ),
                // systemd opens stdio before applying mount namespaces
                // (exec-invoke.c setup_output precedes apply_mount_namespace).
                // Use the host source, not the still-unmounted runtime bind.
                format!(
                    "--property=StandardError=append:{}",
                    self.diagnostics_root
                        .join(launch_id)
                        .join(super::launch_diagnostics::STDERR)
                        .display()
                ),
                format!(
                    "--setenv={}=/run/{runtime}",
                    super::launch_diagnostics::DIRECTORY_ENV
                ),
                format!(
                    "--setenv={}={}",
                    super::launch_diagnostics::READER_ENV,
                    unsafe { libc::geteuid() }
                ),
                format!("--setenv=XDG_RUNTIME_DIR=/run/{runtime}"),
                "--setenv=WAYLAND_DISPLAY=wayland-parent".into(),
            ]);
        }
        arguments.extend(
            environment
                .iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "WAYLAND_DISPLAY"
                            | "SWAYSOCK"
                            | "XDG_RUNTIME_DIR"
                            | super::launch_diagnostics::DIRECTORY_ENV
                            | super::launch_diagnostics::READER_ENV
                    ) && !(modifiers && key.as_str() == "DISPLAY")
                })
                .map(|(key, value)| format!("--setenv={key}={value}")),
        );
        arguments.push("--".into());
        arguments.extend(configured_command.iter().cloned());
        Ok(arguments)
    }

    pub(super) fn stop_arguments(launch_id: &str) -> Result<Vec<String>, LaunchUnitError> {
        Self::unit_verb_arguments("stop", launch_id)
    }

    pub(super) fn reset_failed_arguments(launch_id: &str) -> Result<Vec<String>, LaunchUnitError> {
        Self::unit_verb_arguments("reset-failed", launch_id)
    }

    pub(super) fn freeze_arguments(launch_id: &str) -> Result<Vec<String>, LaunchUnitError> {
        Self::unit_verb_arguments("freeze", launch_id)
    }

    pub(super) fn thaw_arguments(launch_id: &str) -> Result<Vec<String>, LaunchUnitError> {
        Self::unit_verb_arguments("thaw", launch_id)
    }

    fn unit_verb_arguments(verb: &str, launch_id: &str) -> Result<Vec<String>, LaunchUnitError> {
        Ok(vec![
            "--system".into(),
            "--no-ask-password".into(),
            verb.into(),
            Self::unit_name(launch_id)?,
        ])
    }

    pub(super) fn state_arguments(launch_id: &str) -> Result<Vec<String>, LaunchUnitError> {
        Ok(vec![
            "--system".into(),
            "--no-ask-password".into(),
            "show".into(),
            Self::unit_name(launch_id)?,
            "--property=LoadState".into(),
            "--property=ActiveState".into(),
            "--property=FreezerState".into(),
            "--property=ExecMainCode".into(),
            "--property=ExecMainStatus".into(),
            "--property=StatusText".into(),
            "--property=Environment".into(),
        ])
    }

    pub(super) fn runner_id_arguments(launch_id: &str) -> Result<Vec<String>, LaunchUnitError> {
        Ok(vec![
            "--system".into(),
            "--no-ask-password".into(),
            "show".into(),
            Self::unit_name(launch_id)?,
            "--property=Environment".into(),
        ])
    }

    pub(super) fn parse_runner_id(environment: &str) -> Result<Option<String>, LaunchUnitError> {
        let malformed = || {
            LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                "live unit has malformed runner identity metadata",
            )
        };
        let serialized = environment
            .strip_prefix("Environment=")
            .ok_or_else(malformed)?;
        let assignments = Self::parse_systemd_environment(serialized)?;
        let mut names = BTreeSet::new();
        let mut runner_id = None;
        for assignment in assignments {
            let (name, value) = assignment.split_once('=').ok_or_else(malformed)?;
            if !Self::valid_environment_name(name) || !names.insert(name.to_owned()) {
                return Err(malformed());
            }
            if name != RUNNER_ID_ENV {
                continue;
            }
            if runner_id.is_some() || value.is_empty() {
                return Err(malformed());
            }
            let parsed = serde_json::from_value::<crate::config::ReleaseKey>(value.into())
                .map_err(|_| malformed())?;
            runner_id = Some(parsed.0);
        }
        Ok(runner_id)
    }

    fn valid_environment_name(name: &str) -> bool {
        let mut characters = name.chars();
        characters
            .next()
            .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
            && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    }

    fn parse_systemd_environment(serialized: &str) -> Result<Vec<String>, LaunchUnitError> {
        let malformed = || {
            LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                "systemctl returned malformed Environment serialization",
            )
        };
        if serialized.contains(['\n', '\r']) {
            return Err(malformed());
        }
        let chars = serialized.chars().collect::<Vec<_>>();
        let mut offset = 0;
        let mut words = Vec::new();
        while offset < chars.len() {
            while chars
                .get(offset)
                .is_some_and(|character| character.is_ascii_whitespace())
            {
                offset += 1;
            }
            if offset == chars.len() {
                break;
            }
            let quoted = chars[offset] == '"';
            if quoted {
                offset += 1;
            }
            let mut word = String::new();
            loop {
                let Some(character) = chars.get(offset).copied() else {
                    if quoted {
                        return Err(malformed());
                    }
                    break;
                };
                if quoted && character == '"' {
                    offset += 1;
                    if chars
                        .get(offset)
                        .is_some_and(|next| !next.is_ascii_whitespace())
                    {
                        return Err(malformed());
                    }
                    break;
                }
                if !quoted && character.is_ascii_whitespace() {
                    break;
                }
                if !quoted && character == '"' {
                    return Err(malformed());
                }
                if character == '\\' {
                    offset += 1;
                    word.push(Self::decode_systemd_escape(&chars, &mut offset)?);
                } else {
                    if character == '\0' {
                        return Err(malformed());
                    }
                    word.push(character);
                    offset += 1;
                }
            }
            if word.is_empty() {
                return Err(malformed());
            }
            words.push(word);
        }
        Ok(words)
    }

    fn decode_systemd_escape(chars: &[char], offset: &mut usize) -> Result<char, LaunchUnitError> {
        let malformed = || {
            LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                "systemctl returned malformed Environment escape",
            )
        };
        let escaped = chars.get(*offset).copied().ok_or_else(malformed)?;
        *offset += 1;
        let decoded = match escaped {
            '\\' | '"' | '\'' => escaped,
            'a' => '\u{0007}',
            'b' => '\u{0008}',
            'f' => '\u{000c}',
            'n' => '\n',
            'r' => '\r',
            's' => ' ',
            't' => '\t',
            'v' => '\u{000b}',
            'x' => Self::decode_systemd_digits(chars, offset, 2, 16)?,
            'u' => Self::decode_systemd_digits(chars, offset, 4, 16)?,
            'U' => Self::decode_systemd_digits(chars, offset, 8, 16)?,
            '0'..='7' => {
                *offset -= 1;
                Self::decode_systemd_digits(chars, offset, 3, 8)?
            }
            _ => return Err(malformed()),
        };
        if decoded == '\0' {
            return Err(malformed());
        }
        Ok(decoded)
    }

    fn decode_systemd_digits(
        chars: &[char],
        offset: &mut usize,
        count: usize,
        radix: u32,
    ) -> Result<char, LaunchUnitError> {
        let malformed = || {
            LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                "systemctl returned malformed Environment escape",
            )
        };
        let end = offset.checked_add(count).ok_or_else(malformed)?;
        let digits = chars.get(*offset..end).ok_or_else(malformed)?;
        let mut value = 0_u32;
        for digit in digits {
            value = value
                .checked_mul(radix)
                .and_then(|current| {
                    digit
                        .to_digit(radix)
                        .and_then(|digit| current.checked_add(digit))
                })
                .ok_or_else(malformed)?;
        }
        *offset = end;
        char::from_u32(value).ok_or_else(malformed)
    }

    /// Parses `systemctl show` output. `FreezerState` is consulted only for
    /// active units. Zao's systemd 259 reports `running`, `freezing`,
    /// `frozen`, and `thawing`. Only the two settled values map to
    /// `Running` and `Frozen`; the transitional values are reported
    /// separately so a freezer request never assumes a settled state.
    pub(super) fn parse_unit_state(
        values: &BTreeMap<&str, &str>,
    ) -> Result<LaunchUnitState, LaunchUnitError> {
        match values.get("ActiveState").copied() {
            Some("activating" | "active" | "reloading") => {
                match values.get("FreezerState").copied() {
                    None | Some("") | Some("running") => Ok(LaunchUnitState::Running),
                    Some("frozen") => Ok(LaunchUnitState::Frozen),
                    Some("freezing" | "thawing") => Ok(LaunchUnitState::FreezerTransition),
                    Some(other) => Err(LaunchUnitError::new(
                        LaunchUnitErrorKind::Protocol,
                        format!("unit has unknown FreezerState {other:?}"),
                    )),
                }
            }
            Some("deactivating") => Ok(LaunchUnitState::Stopping),
            Some("failed") => {
                let property = |name| {
                    values
                        .get(name)
                        .and_then(|value| value.parse::<i32>().ok())
                        .filter(|value| *value >= 0)
                        .ok_or_else(|| {
                            LaunchUnitError::new(
                                LaunchUnitErrorKind::Protocol,
                                format!("failed unit has no valid {name}"),
                            )
                        })
                };
                let code = property("ExecMainCode")?;
                let status = property("ExecMainStatus")?;
                let message = match code {
                    libc::CLD_EXITED => format!("game unit failed with exit status {status}"),
                    libc::CLD_KILLED | libc::CLD_DUMPED => {
                        format!("game unit failed with signal {status}")
                    }
                    _ => format!("game unit failed: ExecMainCode={code}, ExecMainStatus={status}"),
                };
                let message = match values
                    .get("StatusText")
                    .filter(|text| text.starts_with("plugin-launch: "))
                {
                    Some(reason) => format!("{message}: {reason}"),
                    None => message,
                };
                Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::UnitFailed,
                    message,
                ))
            }
            Some("inactive" | "dead") => Ok(LaunchUnitState::Completed),
            Some(other) => Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                format!("unit has unknown ActiveState {other:?}"),
            )),
            None => Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                "systemctl returned no ActiveState",
            )),
        }
    }

    /// Runs one `systemctl show` for ActiveState and FreezerState. A unit
    /// systemd does not know is reported as completed.
    fn observe_state(&self, arguments: &[String]) -> Result<LaunchUnitState, LaunchUnitError> {
        let output = self.run(&self.systemctl, arguments)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let values: BTreeMap<_, _> = stdout
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        if values.get("LoadState").copied() == Some("not-found") {
            return Ok(LaunchUnitState::Completed);
        }
        if !output.status.success() {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                format!(
                    "systemctl show failed with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ));
        }
        Self::parse_unit_state(&values)
    }

    // This is the diagnostic part of an already-observed UnitFailed result.
    // Keep it separate from the systemctl subprocess so its lock-held work can
    // be regression-tested independently of process scheduling and IPC.
    // This remains synchronous under session control's transition guard:
    // moving the observation outside it requires revalidating the exact live
    // launch and cleanup after relocking. Instead the reader bounds metadata,
    // words and matched text; the debug timing tests cover that critical work.
    fn failure_with_stderr(&self, launch_id: &str, mut error: LaunchUnitError) -> LaunchUnitError {
        match super::launch_diagnostics::failure_reason(
            &self.diagnostics_root.join(launch_id),
            self.runtime_uid,
        ) {
            Ok(Some(reason)) => error.message.push_str(&format!(": {reason}")),
            Ok(None) => {}
            Err(cause) => eprintln!("korrid game stderr diagnostics for {launch_id}: {cause}"),
        }
        error
    }

    fn require_success(
        &self,
        program: &Path,
        arguments: &[String],
    ) -> Result<Output, LaunchUnitError> {
        let output = self.run(program, arguments)?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                format!(
                    "{} failed with {}: {}",
                    program.display(),
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ))
        }
    }
}

impl LaunchUnitBackend for SystemdLaunchUnitBackend {
    /// Reads the unit's own control group. Kernel process membership is the
    /// only honest answer to "which windows belong to this launch"; window
    /// titles and classes are set by the game itself.
    fn window_pids(&self, launch_id: &str) -> Result<BTreeSet<i32>, LaunchUnitError> {
        let unit = Self::unit_name(launch_id)?;
        let root = configured_path("KORRID_UNIT_CGROUP_ROOT", DEFAULT_UNIT_CGROUP_ROOT);
        read_unit_pids(&root, &unit)
    }

    fn launch(
        &self,
        launch_id: &str,
        configured_command: &[String],
        environment: &BTreeMap<String, String>,
    ) -> Result<(), LaunchUnitError> {
        let arguments = self.launch_arguments(launch_id, configured_command, environment)?;
        if crate::launcher::plugin_launch::has_modifiers(configured_command) {
            super::launch_diagnostics::create(
                &self.diagnostics_root.join(launch_id),
                self.runtime_uid,
            )
            .map_err(|error| {
                LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    format!("cannot prepare game stderr: {error}"),
                )
            })?;
        }
        self.require_success(&self.systemd_run, &arguments)
            .map(|_| ())
    }

    fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError> {
        let arguments = Self::state_arguments(launch_id)?;
        let directory = self.diagnostics_root.join(launch_id);
        let output = self.run(&self.systemctl, &arguments)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let values: BTreeMap<_, _> = stdout
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        // Only the host-injected exact-unit environment marker grants access
        // to these files. It survives daemon restarts without a second cache.
        let marker = format!(
            "{}=/run/korri-game-{launch_id}",
            super::launch_diagnostics::DIRECTORY_ENV
        );
        let captures_stderr = values
            .get("Environment")
            .and_then(|value| Self::parse_systemd_environment(value).ok())
            .is_some_and(|values| values.contains(&marker));
        let diagnostics_available = if captures_stderr {
            // spawn_portal_watch already observes the live session every second,
            // including when the portal cannot poll. No new maintenance thread.
            match super::launch_diagnostics::maintain(&directory) {
                Ok(()) => true,
                Err(error) => {
                    eprintln!("korrid game stderr diagnostics for {launch_id}: {error}");
                    false
                }
            }
        } else {
            false
        };
        let state = if values.get("LoadState").copied() == Some("not-found") {
            Ok(LaunchUnitState::Completed)
        } else if !output.status.success() {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                format!(
                    "systemctl show failed with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ));
        } else {
            Self::parse_unit_state(&values)
        };
        match state {
            Err(error)
                if error.kind == LaunchUnitErrorKind::UnitFailed
                    && diagnostics_available
                    && values
                        .get("StatusText")
                        .is_none_or(|value| value.trim().is_empty()) =>
            {
                Err(self.failure_with_stderr(launch_id, error))
            }
            Ok(LaunchUnitState::Completed) => {
                super::launch_diagnostics::cleanup(&directory).map_err(|error| {
                    LaunchUnitError::new(
                        LaunchUnitErrorKind::Failed,
                        format!("cannot clean game stderr: {error}"),
                    )
                })?;
                Ok(LaunchUnitState::Completed)
            }
            other => other,
        }
    }

    fn runner_id(&self, launch_id: &str) -> Result<Option<String>, LaunchUnitError> {
        let arguments = Self::runner_id_arguments(launch_id)?;
        let output = self.require_success(&self.systemctl, &arguments)?;
        let stdout = std::str::from_utf8(&output.stdout).map_err(|error| {
            LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                format!("systemctl returned non-UTF-8 environment metadata: {error}"),
            )
        })?;
        Self::parse_runner_id(stdout.trim())
    }

    /// Stops the unit. systemd 259 refuses `stop` on a frozen unit
    /// ("Cannot perform operation on frozen unit"), so a unit observed
    /// frozen or in a freezer transition is thawed first. A thaw failure is
    /// a stop failure; the unit is left untouched. Callers that already
    /// know the unit completed tolerate the `Failed` result by re-reading
    /// state, as before.
    fn stop(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        let (needs_thaw, failed) = match self.state(launch_id) {
            Ok(state) => (state.needs_thaw_before_stop(), false),
            Err(error) if error.kind == LaunchUnitErrorKind::UnitFailed => (false, true),
            Err(error) => return Err(error),
        };
        if needs_thaw {
            self.thaw(launch_id).map_err(|error| {
                LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    format!("thaw before stop failed: {}", error.message),
                )
            })?;
        }
        let arguments = Self::stop_arguments(launch_id)?;
        self.require_success(&self.systemctl, &arguments)?;
        // Stop does not reset a failed service. Release its retained exit
        // metadata only after the caller captured the failure.
        if failed {
            self.require_success(&self.systemctl, &Self::reset_failed_arguments(launch_id)?)?;
        }
        super::launch_diagnostics::cleanup(&self.diagnostics_root.join(launch_id)).map_err(
            |error| {
                LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    format!("cannot clean game stderr: {error}"),
                )
            },
        )
    }

    fn freeze(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        let arguments = Self::freeze_arguments(launch_id)?;
        self.require_success(&self.systemctl, &arguments)
            .map(|_| ())
    }

    fn thaw(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        let arguments = Self::thaw_arguments(launch_id)?;
        self.require_success(&self.systemctl, &arguments)
            .map(|_| ())
    }

    fn live_launch_ids(&self) -> Result<Vec<String>, LaunchUnitError> {
        let output = self.require_success(
            &self.systemctl,
            &[
                "--system".into(),
                "--no-ask-password".into(),
                "list-units".into(),
                format!("{UNIT_PREFIX}*{UNIT_SUFFIX}"),
                "--state=activating,active,reloading,deactivating".into(),
                "--plain".into(),
                "--no-legend".into(),
                "--no-pager".into(),
            ],
        )?;
        let mut ids = Vec::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let Some(unit) = line.split_whitespace().next() else {
                continue;
            };
            let Some(id) = unit
                .strip_prefix(UNIT_PREFIX)
                .and_then(|value| value.strip_suffix(UNIT_SUFFIX))
            else {
                continue;
            };
            validate_launch_id(id)?;
            ids.push(id.to_owned());
        }
        ids.sort();
        ids.dedup();
        Ok(ids)
    }
}

fn configured_runtime_id(name: &str) -> Option<u32> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|id| *id != 0)
}

fn configured_path(name: &str, default: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

fn valid_protected_path(path: &Path) -> bool {
    let Some(value) = path.to_str() else {
        return false;
    };
    path.is_absolute()
        && path != Path::new("/")
        && !value.contains("//")
        && !value.contains("/./")
        && !value.ends_with("/.")
        && !value.contains("/../")
        && !value.ends_with("/..")
        && !value.chars().any(char::is_whitespace)
        && path
            .components()
            .all(|component| !matches!(component, Component::CurDir | Component::ParentDir))
}

pub(super) fn validate_launch_id(value: &str) -> Result<(), LaunchUnitError> {
    if value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(LaunchUnitError::new(
            LaunchUnitErrorKind::InvalidConfiguration,
            "invalid host launch identity",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAUNCH_ID: &str = "0123456789abcdef0123456789abcdef";

    fn backend(runtime_uid: u32) -> SystemdLaunchUnitBackend {
        SystemdLaunchUnitBackend::new(
            "/nix/store/systemd/bin/systemd-run".into(),
            "/nix/store/systemd/bin/systemctl".into(),
            runtime_uid,
            2002,
        )
        .unwrap()
    }

    const PORTAL: &str = "korri-chromium-kiosk.service";

    #[test]
    fn portal_native_calls_need_no_unit_reference_or_extra_authority() {
        let portal = SystemdPortalUnit::new(backend(1000), PORTAL).unwrap();
        for method in ["FreezeUnit", "ThawUnit"] {
            assert_eq!(
                portal.verb_arguments(method),
                [
                    "--system",
                    "--allow-interactive-authorization=no",
                    "call",
                    "org.freedesktop.systemd1",
                    "/org/freedesktop/systemd1",
                    "org.freedesktop.systemd1.Manager",
                    method,
                    "s",
                    PORTAL,
                ]
            );
        }
        // Real D-Bus authorization and settled replies are exercised by
        // clients/portal/nix/kiosk-freezer-vm-test.nix, not shell emulation.
    }

    #[test]
    fn portal_unit_must_be_one_exact_non_game_service() {
        for unit in [
            "",
            ".service",
            "korri-chromium-kiosk",
            "korri-chromium-kiosk.socket",
            "-korri-chromium-kiosk.service",
            "../korri-chromium-kiosk.service",
            "korri chromium.service",
            "korri-*.service",
            "korri-game-0123456789abcdef0123456789abcdef.service",
        ] {
            assert_eq!(
                SystemdPortalUnit::new(backend(1000), unit)
                    .unwrap_err()
                    .kind,
                LaunchUnitErrorKind::InvalidConfiguration,
                "accepted {unit:?}"
            );
        }
        for unit in [PORTAL, "cage@tty1.service"] {
            assert!(
                SystemdPortalUnit::new(backend(1000), unit).is_ok(),
                "{unit}"
            );
        }
    }

    #[test]
    fn runner_identity_decodes_strict_systemd_environment_serialization() {
        for serialized in [
            "Environment=DISPLAY=:0 KORRI_LIVE_RUNNER_ID=@korri:mgba/mgba HOME=/home/korri",
            "Environment=\"DISPLAY=hello world\" \"KORRI_LIVE_RUNNER_ID=@korri:mgba/mgba\" OTHER=a\\\\b",
            "Environment=KORRI_LIVE_RUNNER_ID=@korri:mgba\\x2fmgba",
            "Environment=KORRI_LIVE_RUNNER_ID_SUFFIX=@korri:other/value OTHER=KORRI_LIVE_RUNNER_ID=@korri:also/valid KORRI_LIVE_RUNNER_ID=@korri:mgba/mgba",
        ] {
            assert_eq!(
                SystemdLaunchUnitBackend::parse_runner_id(serialized).unwrap(),
                Some("@korri:mgba/mgba".into())
            );
        }
        assert_eq!(
            SystemdLaunchUnitBackend::parse_runner_id("Environment=DISPLAY=:0").unwrap(),
            None
        );
    }

    #[test]
    fn runner_identity_rejects_ambiguous_or_malformed_systemd_metadata() {
        for serialized in [
            "Environment=KORRI_LIVE_RUNNER_ID=@korri:mgba/mgba KORRI_LIVE_RUNNER_ID=@korri:mgba/other",
            "Environment=\"KORRI_LIVE_RUNNER_ID=@korri:mgba/mgba",
            "Environment=BROKEN KORRI_LIVE_RUNNER_ID=@korri:mgba/mgba",
            "Environment=9BROKEN=value KORRI_LIVE_RUNNER_ID=@korri:mgba/mgba",
            "Environment=DISPLAY=:0 DISPLAY=:1 KORRI_LIVE_RUNNER_ID=@korri:mgba/mgba",
            "Environment=KORRI_LIVE_RUNNER_ID=",
            "Environment=KORRI_LIVE_RUNNER_ID=mgba",
            "Environment=KORRI_LIVE_RUNNER_ID=@korri:mgba",
            "Environment=KORRI_LIVE_RUNNER_ID=@korri:mgba/mgba\\q",
            "KORRI_LIVE_RUNNER_ID=@korri:mgba/mgba",
        ] {
            assert!(
                SystemdLaunchUnitBackend::parse_runner_id(serialized).is_err(),
                "accepted {serialized:?}"
            );
        }
    }

    #[test]
    fn pulse_opt_in_adds_only_the_runtime_audio_bind_and_environment() {
        for uid in [2001, 3001] {
            let backend = backend(uid);
            let command = ["/games/retroarch".into()];
            let baseline = backend
                .launch_arguments(LAUNCH_ID, &command, &BTreeMap::new())
                .unwrap();
            let launch = backend
                .launch_arguments(
                    LAUNCH_ID,
                    &command,
                    &BTreeMap::from([(
                        "PULSE_SERVER".into(),
                        "unix:/run/korri-game-audio/native".into(),
                    )]),
                )
                .unwrap();
            let mut expected = baseline;
            let bind = expected
                .iter_mut()
                .find(|arg| arg.starts_with("--property=BindReadOnlyPaths="))
                .unwrap();
            *bind = format!(
                "--property=BindReadOnlyPaths=/tmp/.X11-unix/X0 /run/user/{uid}/pulse:/run/korri-game-audio"
            );
            let separator = expected.iter().position(|arg| arg == "--").unwrap();
            expected.insert(
                separator,
                "--setenv=PULSE_SERVER=unix:/run/korri-game-audio/native".into(),
            );
            assert_eq!(launch, expected);
            assert!(launch.contains(&format!(
                "--property=InaccessiblePaths=/var/lib/korrid /run/korrid /run/korrid-browser /run/korrid-control/control.sock /run/korrid-control -/home/korri/.config/sunshine /run/korri-compositor -/run/korri-certificate-control /run/user/{uid} -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources"
            )));
        }
    }

    #[test]
    fn absent_or_other_pulse_servers_do_not_add_a_bind() {
        let backend = backend(2001);
        let command = ["/games/retroarch".into()];
        let baseline = backend
            .launch_arguments(LAUNCH_ID, &command, &BTreeMap::new())
            .unwrap();
        assert_eq!(
            baseline
                .iter()
                .filter(|arg| arg.starts_with("--property=BindReadOnlyPaths="))
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["--property=BindReadOnlyPaths=/tmp/.X11-unix/X0"]
        );
        for value in [
            "",
            "tcp:localhost:4713",
            "unix:/run/user/2001/pulse/native",
            "/run/korri-game-audio/native",
            "unix:/run/korri-game-audio/native/",
            "unix:/run/korri-game-audio/native ",
            "unix:/run/korri-game-audio/native unix:/run/user/2001/bus",
        ] {
            let launch = backend
                .launch_arguments(
                    LAUNCH_ID,
                    &command,
                    &BTreeMap::from([("PULSE_SERVER".into(), value.into())]),
                )
                .unwrap();
            let mut expected = baseline.clone();
            let separator = expected.iter().position(|arg| arg == "--").unwrap();
            expected.insert(separator, format!("--setenv=PULSE_SERVER={value}"));
            assert_eq!(launch, expected);
        }
    }

    #[test]
    fn configured_runtime_uid_cannot_inject_a_mount_path() {
        // A test-specific variable avoids changing the process's runtime identity.
        const NAME: &str = "KORRID_SYSTEMD_UNIT_TEST_RUNTIME_UID";
        for value in [
            "../control",
            "2001/pulse:/run/escape",
            "2001 /run/escape",
            "-1",
            "4294967296",
            "0",
        ] {
            std::env::set_var(NAME, value);
            let parsed = configured_runtime_id(NAME);
            std::env::remove_var(NAME);
            assert_eq!(parsed, None, "accepted {value:?}");
        }
        let error = SystemdLaunchUnitBackend::new(
            "/nix/store/systemd/bin/systemd-run".into(),
            "/nix/store/systemd/bin/systemctl".into(),
            0,
            2002,
        )
        .unwrap_err();
        assert_eq!(error.kind, LaunchUnitErrorKind::InvalidConfiguration);
    }
}

#[cfg(test)]
mod startup_diagnostic_tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn failed_unit_uses_only_its_own_bounded_redacted_stderr_and_cleans_normal_completion() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemctl");
        let reply = root.path().join("reply");
        fs::write(
            &reply,
            "LoadState=loaded\nActiveState=failed\nExecMainCode=1\nExecMainStatus=1\nStatusText=\nEnvironment=KORRI_LAUNCH_DIAGNOSTICS_DIRECTORY=/run/korri-game-0123456789abcdef0123456789abcdef\n",
        )
        .unwrap();
        fs::write(&helper, format!("#!/bin/sh\ncat '{}'\n", reply.display())).unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let mut backend = SystemdLaunchUnitBackend::new(
            helper.clone(),
            helper,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
        )
        .unwrap();
        backend.diagnostics_root = root.path().join("launches");
        let id = "0123456789abcdef0123456789abcdef";
        let directory = backend.diagnostics_root.join(id);
        super::super::launch_diagnostics::create(&directory, unsafe { libc::geteuid() } + 1)
            .unwrap();
        super::super::launch_diagnostics::write_words(
            &directory,
            unsafe { libc::geteuid() } + 1,
            &["--no-such-reviewed-flag".into(), "private-env-value".into()],
        )
        .unwrap();
        fs::write(
            directory.join("stderr"),
            "old message\ncontext\nstartup refused --no-such-reviewed-flag private-env-value\n",
        )
        .unwrap();
        let error = backend.state(id).unwrap_err();
        assert_eq!(error.kind, LaunchUnitErrorKind::UnitFailed);
        assert!(
            error
                .message
                .contains("startup refused [redacted] [redacted]"),
            "{}",
            error.message
        );
        assert!(!error.message.contains("private-env-value"));
        assert!(!error.message.contains("--no-such-reviewed-flag"));
        fs::write(&reply, "LoadState=loaded\nActiveState=failed\nExecMainCode=1\nExecMainStatus=1\nStatusText=plugin-launch: specific compose refusal\n").unwrap();
        assert!(!backend
            .state(id)
            .unwrap_err()
            .message
            .contains("startup refused"));
        fs::write(&reply, "LoadState=loaded\nActiveState=inactive\n").unwrap();
        assert_eq!(backend.state(id).unwrap(), LaunchUnitState::Completed);
        assert!(!directory.exists());
    }
}

#[cfg(test)]
mod diagnostic_file_hardening_tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
    };

    #[test]
    fn diagnostic_file_tampering_keeps_only_the_generic_unit_exit_reason() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemctl");
        let id = "0123456789abcdef0123456789abcdef";
        fs::write(&helper, format!("#!/bin/sh\nprintf 'LoadState=loaded\\nActiveState=failed\\nExecMainCode=1\\nExecMainStatus=1\\nStatusText=\\nEnvironment=KORRI_LAUNCH_DIAGNOSTICS_DIRECTORY=/run/korri-game-{id}\\n'\n")).unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let uid = unsafe { libc::geteuid() };
        let mut backend =
            SystemdLaunchUnitBackend::new(helper.clone(), helper, uid, unsafe { libc::getegid() })
                .unwrap();
        backend.diagnostics_root = root.path().join("launches");
        let directory = backend.diagnostics_root.join(id);
        let outside = root.path().join("must-not-read");
        fs::write(&outside, "DO_NOT_READ_PRIVATE_CONTENT").unwrap();
        for target in ["stderr", "redaction-words.json", "wrong-words-owner"] {
            super::super::launch_diagnostics::create(&directory, uid + 1).unwrap();
            super::super::launch_diagnostics::write_words(&directory, uid + 1, &[]).unwrap();
            fs::write(directory.join("stderr"), "valid native failure cause").unwrap();
            backend.runtime_uid = uid;
            assert!(backend
                .state(id)
                .unwrap_err()
                .message
                .contains("valid native failure cause"));
            if target == "wrong-words-owner" {
                // No chown privilege is needed: make the trusted expected uid
                // differ from the actual file owner, as a replaced file would.
                backend.runtime_uid = uid + 1;
            } else {
                fs::remove_file(directory.join(target)).unwrap();
                symlink(&outside, directory.join(target)).unwrap();
            }
            let error = backend.state(id).unwrap_err();
            assert_eq!(error.kind, LaunchUnitErrorKind::UnitFailed);
            assert_eq!(
                error.message, "game unit failed with exit status 1",
                "{target}"
            );
            assert!(!error.message.contains("DO_NOT_READ"));
            super::super::launch_diagnostics::cleanup(&directory).unwrap();
        }
    }
}

#[cfg(test)]
mod diagnostic_cost_tests {
    use super::*;
    use std::fs;

    const ID: &str = "0123456789abcdef0123456789abcdef";
    const GENERIC: &str = "game unit failed with exit status 1";

    fn fixture(words: &[String], stderr: &str) -> (tempfile::TempDir, SystemdLaunchUnitBackend) {
        let root = tempfile::tempdir().unwrap();
        let uid = unsafe { libc::geteuid() };
        let mut backend = SystemdLaunchUnitBackend::new(
            PathBuf::from("/unused/systemd-run"),
            PathBuf::from("/unused/systemctl"),
            uid,
            unsafe { libc::getegid() },
        )
        .unwrap();
        backend.diagnostics_root = root.path().to_path_buf();
        let directory = root.path().join(ID);
        fs::create_dir(&directory).unwrap();
        // Simulate the game rewriting its own file, bypassing the producer.
        fs::write(
            directory.join("redaction-words.json"),
            serde_json::to_vec(words).unwrap(),
        )
        .unwrap();
        fs::write(directory.join("stderr"), stderr).unwrap();
        (root, backend)
    }

    fn timed_failure(backend: &SystemdLaunchUnitBackend) -> (LaunchUnitError, Duration) {
        let started = Instant::now();
        // Time the complete local diagnostic work executed under the session
        // guard, not the unrelated systemctl IPC. Both are real file reads.
        super::super::launch_diagnostics::maintain(&backend.diagnostics_root.join(ID)).unwrap();
        let result = backend.failure_with_stderr(
            ID,
            LaunchUnitError::new(LaunchUnitErrorKind::UnitFailed, GENERIC),
        );
        (result, started.elapsed())
    }

    #[test]
    fn startup_diagnostics_rejects_200000_words_with_generic_reason_under_50ms() {
        let (_root, backend) = fixture(&vec!["a".into(); 200_000], &"b".repeat(65_536));
        let (result, elapsed) = timed_failure(&backend);
        assert!(
            elapsed < Duration::from_millis(50),
            "diagnostics took {elapsed:?}"
        );
        assert_eq!(result.kind, LaunchUnitErrorKind::UnitFailed);
        assert_eq!(result.message, GENERIC);
        eprintln!("200,000-word rejection: {elapsed:?}");
    }

    #[test]
    fn startup_diagnostics_at_both_word_caps_finishes_under_50ms() {
        let words: Vec<String> = (0..256)
            .map(|n| format!("{}{:03}", "a".repeat(61), n))
            .collect();
        assert_eq!(words.iter().map(String::len).sum::<usize>(), 16_384);
        for stored_bytes in [300, 65_536] {
            let text = "a".repeat(stored_bytes);
            let (_root, backend) = fixture(&words, &text);
            let (result, elapsed) = timed_failure(&backend);
            assert!(
                elapsed < Duration::from_millis(50),
                "diagnostics took {elapsed:?}"
            );
            assert_eq!(result.message, format!("{GENERIC}: {}", "a".repeat(300)));
            eprintln!("256-word / 16 KiB / {stored_bytes}-byte stored stderr: {elapsed:?}");
        }
    }
}
