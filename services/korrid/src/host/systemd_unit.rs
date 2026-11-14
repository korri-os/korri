#[path = "viewer_units.rs"]
mod viewer_units;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read},
    os::fd::AsRawFd,
    path::{Component, Path, PathBuf},
    process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Output, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
pub(crate) use viewer_units::viewer_lifecycle_socket;

#[cfg(test)]
use std::sync::Mutex;

const UNIT_PREFIX: &str = "korri-game-";
const UNIT_SUFFIX: &str = ".service";
const DEFAULT_HELPER_TIMEOUT: Duration = Duration::from_secs(10);
const HELPER_POLL_INTERVAL: Duration = Duration::from_millis(5);
const MAX_HELPER_OUTPUT_BYTES: u64 = 64 * 1024;
const DEFAULT_PRIVATE_STATE_ROOT: &str = "/var/lib/korrid";
const DEFAULT_CONTROL_SOCKET: &str = "/run/korrid-control/control.sock";
const DEFAULT_CONTROL_DIRECTORY: &str = "/run/korrid-control";
const DEFAULT_COMPOSITOR_CONTROL_DIRECTORY: &str = "/run/korri-compositor";
/// Control group of a system unit started by systemd-run.
const DEFAULT_UNIT_CGROUP_ROOT: &str = "/sys/fs/cgroup/korri.slice/korri-games.slice";
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

pub(super) const DISPLAY_ENV: &str = "KORRI_LIVE_DISPLAY";
pub(super) const PLACEMENT_ENV: &str = "KORRI_LIVE_PLACEMENT";

pub(crate) const RUNNER_ID_ENV: &str = "KORRI_LIVE_RUNNER_ID";

/// Mount-driving launch facts are resolved once from the game unit's baseline.
/// Process environment changes never grant a hook a different sandbox.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchSandbox {
    netplay_config_source: Option<PathBuf>,
    modifiers: bool,
    plugin: bool,
    pulse_audio: bool,
}

impl LaunchSandbox {
    pub(crate) fn new(command: &[String], baseline: &BTreeMap<String, String>) -> Self {
        Self {
            netplay_config_source: command
                .get(4)
                .and_then(|value| {
                    serde_json::from_str::<crate::launcher::plugin_launch::LaunchComposition>(value)
                        .ok()
                })
                .and_then(|composition| composition.netplay_config_source),
            modifiers: crate::launcher::plugin_launch::has_modifiers(command),
            plugin: command
                .get(1)
                .is_some_and(|value| matches!(value.as_str(), "plugin-launch" | "plugin-prepare")),
            pulse_audio: baseline.get("PULSE_SERVER").map(String::as_str)
                == Some("unix:/run/korri-game-audio/native"),
        }
    }
}

pub trait LaunchUnitBackend: Send + Sync {
    fn apply_sunshine_settings(&self, _args: &[String]) -> Result<(), LaunchUnitError> {
        Err(LaunchUnitError::new(
            LaunchUnitErrorKind::Failed,
            "launch service settings are unavailable",
        ))
    }

    fn launch(
        &self,
        launch_id: &str,
        command: &[String],
        environment: &BTreeMap<String, String>,
        _sandbox: &super::systemd_unit::LaunchSandbox,
    ) -> Result<(), LaunchUnitError>;
    /// `controller`: korrid's UID, for a viewer with a lifecycle socket.
    fn launch_viewer(
        &self,
        _launch: &str,
        _command: &[String],
        _environment: &BTreeMap<String, String>,
        _sandbox: &LaunchSandbox,
        _certificate: &str,
        _controller: Option<u32>,
    ) -> Result<(), LaunchUnitError> {
        Err(LaunchUnitError::new(
            LaunchUnitErrorKind::Failed,
            "Viewer credentials are unavailable",
        ))
    }
    fn bootstrap_viewer(
        &self,
        _launch: &str,
        _command: &[String],
        _environment: &BTreeMap<String, String>,
        _sandbox: &LaunchSandbox,
    ) -> Result<String, LaunchUnitError> {
        Err(LaunchUnitError::new(
            LaunchUnitErrorKind::Failed,
            "Viewer identity bootstrap is unavailable",
        ))
    }
    /// Runs one bounded hook in the game's runtime sandbox, waiting for completion.
    #[allow(clippy::too_many_arguments)]
    fn run_hook(
        &self,
        _launch_id: &str,
        _step: usize,
        _game_command: &[String],
        _run: &str,
        _timeout: Duration,
        _environment: &BTreeMap<String, String>,
        _sandbox: &super::systemd_unit::LaunchSandbox,
    ) -> Result<(), LaunchUnitError> {
        Err(LaunchUnitError::new(
            LaunchUnitErrorKind::Failed,
            "hook execution unavailable",
        ))
    }
    /// A live hook reuses the existing game sandbox and diagnostics mounts.
    /// It must never create or delete the active game's diagnostic files.
    fn run_live_hook(
        &self,
        _launch_id: &str,
        _command: &[String],
        _run: &str,
        _timeout: Duration,
        _environment: &BTreeMap<String, String>,
        _sandbox: &LaunchSandbox,
    ) -> Result<String, LaunchUnitError> {
        Err(LaunchUnitError::new(
            LaunchUnitErrorKind::Failed,
            "live hook execution unavailable",
        ))
    }
    fn exit_status(&self, _launch_id: &str) -> Result<Option<i32>, LaunchUnitError> {
        Ok(None)
    }
    /// Release completion diagnostics only after the host journals the result.
    fn cleanup_completed(&self, _launch_id: &str) -> Result<(), LaunchUnitError> {
        Ok(())
    }

    fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError>;
    /// State and runner status from one native property observation.
    fn state_with_status(
        &self,
        launch_id: &str,
    ) -> Result<(LaunchUnitState, Option<String>), LaunchUnitError> {
        self.state(launch_id).map(|state| (state, None))
    }

    /// Stops the unit from any live state. A backend must thaw a frozen
    /// unit itself before stopping it; callers never issue a thaw before
    /// a stop. Success may acknowledge a queued job, not completion; callers
    /// must observe the exact unit before completing the session.
    fn stop(&self, launch_id: &str) -> Result<(), LaunchUnitError>;
    fn freeze(&self, launch_id: &str) -> Result<(), LaunchUnitError>;
    fn thaw(&self, launch_id: &str) -> Result<(), LaunchUnitError>;
    fn live_launch_ids(&self) -> Result<Vec<String>, LaunchUnitError>;
    /// Runner identity retained by the live transient unit. This is runtime
    /// discovery, not part of the durable active-session journal.
    fn runner_id(&self, _launch_id: &str) -> Result<Option<String>, LaunchUnitError> {
        Ok(None)
    }
    fn placement(
        &self,
        _launch_id: &str,
    ) -> Result<Option<super::display_placement::Placement>, LaunchUnitError> {
        Ok(None)
    }
    /// The display from this exact launch's composition, retained by its unit.
    fn display(
        &self,
        _launch_id: &str,
    ) -> Result<Option<crate::launcher::plugin_launch::PluginDisplay>, LaunchUnitError> {
        Ok(None)
    }
    /// Processes of this launch, used to recognize its compositor windows.
    /// A backend that cannot see processes reports none, which makes korrid
    /// focus nothing rather than guess at another program's window.
    fn control_group(&self, _launch_id: &str) -> Result<String, LaunchUnitError> {
        Err(LaunchUnitError::new(
            LaunchUnitErrorKind::Failed,
            "exact game control group is unavailable",
        ))
    }
    fn window_pids(&self, _launch_id: &str) -> Result<BTreeSet<i32>, LaunchUnitError> {
        Ok(BTreeSet::new())
    }
}

/// Names the shell unit korrid freezes while a focused game owns the
/// screen. Absent, the shell is never frozen.
pub(super) const SHELL_UNIT_ENV: &str = "KORRID_SHELL_UNIT";

/// The shell surface korrid pauses while the player is in a focused live
/// game. A frozen shell can neither paint nor take input, so every thaw
/// request must leave it running.
pub trait ShellUnit: Send + Sync {
    /// Identity does not grant native freezer authority.
    fn freezer_enabled(&self) -> bool {
        true
    }

    /// Processes owned by the configured shell unit, not window decorations.
    fn window_pids(&self) -> Result<BTreeSet<i32>, LaunchUnitError> {
        Err(LaunchUnitError::new(
            LaunchUnitErrorKind::Failed,
            "shell process identity unavailable",
        ))
    }
    fn freeze(&self) -> Result<(), LaunchUnitError>;
    /// Leaves the unit running. A unit that is not frozen, including one
    /// that is inactive or not loaded, needs no thaw and succeeds.
    fn thaw(&self) -> Result<(), LaunchUnitError>;
}

/// Calls native FreezeUnit/ThawUnit through busctl, with the same bounded
/// process runner as game controls. Unlike systemctl this takes no RefUnit
/// authority. systemd holds the reply until the kernel freezer settles.
#[derive(Clone, Debug)]
pub struct SystemdShellUnit {
    helper: SystemdLaunchUnitBackend,
    unit: String,
    freezer_enabled: bool,
    cgroup_root: PathBuf,
}

impl SystemdShellUnit {
    #[cfg(test)]
    pub(super) fn with_cgroup_root(mut self, root: PathBuf) -> Self {
        self.cgroup_root = root;
        self
    }

    pub(super) fn new(
        helper: SystemdLaunchUnitBackend,
        unit: &str,
        freezer_enabled: bool,
    ) -> Result<Self, LaunchUnitError> {
        if !valid_shell_unit(unit) {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "shell unit must be one exact service name that is not a game unit",
            ));
        }
        Ok(Self {
            helper,
            unit: unit.to_owned(),
            freezer_enabled,
            cgroup_root: "/sys/fs/cgroup".into(),
        })
    }

    /// Reads the configured shell unit. `None` keeps the feature inert.
    pub(super) fn from_environment() -> Option<Result<Self, LaunchUnitError>> {
        let unit = std::env::var(SHELL_UNIT_ENV).ok()?;
        let enabled = match std::env::var("KORRID_SHELL_FREEZER_ENABLED").as_deref() {
            Ok("true") => true,
            Ok("false") | Err(std::env::VarError::NotPresent) => false,
            _ => {
                return Some(Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::InvalidConfiguration,
                    "shell freezer gate must be true or false",
                )))
            }
        };
        Some(Self::new(
            SystemdLaunchUnitBackend::default(),
            &unit,
            enabled,
        ))
    }

    /// systemd answers a freezer call only once the freezer settles, so the
    /// call returns with the shell frozen or running.
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

impl ShellUnit for SystemdShellUnit {
    fn freezer_enabled(&self) -> bool {
        self.freezer_enabled
    }
    fn window_pids(&self) -> Result<BTreeSet<i32>, LaunchUnitError> {
        // The service names its cgroup; the kernel supplies current members.
        // A cached MainPID can outlive a restart and must not authorize a PID
        // that the kernel has since assigned to an unrelated process.
        let snapshot = self.helper.unit_properties(&self.unit, &["ControlGroup"])?;
        if snapshot.values.get("LoadState").map(String::as_str) == Some("not-found") {
            return Ok(BTreeSet::new());
        }
        let malformed = || {
            LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                "invalid shell unit process properties",
            )
        };
        let properties: BTreeMap<&str, &str> = snapshot
            .values
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let group = *properties.get("ControlGroup").ok_or_else(malformed)?;
        let pids = if group.is_empty() {
            BTreeSet::new()
        } else {
            let relative = std::path::Path::new(group)
                .strip_prefix("/")
                .map_err(|_| malformed())?;
            if relative.as_os_str().is_empty()
                || relative
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err(malformed());
            }
            read_unit_pids(&self.cgroup_root, relative.to_str().ok_or_else(malformed)?)?
        };
        Ok(pids)
    }

    fn freeze(&self) -> Result<(), LaunchUnitError> {
        if !self.freezer_enabled {
            return Ok(());
        }
        self.call("FreezeUnit")
    }

    /// systemd refuses `thaw` on an inactive unit. Read the freezer state
    /// first so a stopped or absent shell is not a thaw failure that korrid
    /// would retry on every status read.
    fn thaw(&self) -> Result<(), LaunchUnitError> {
        if !self.freezer_enabled {
            return Ok(());
        }
        self.helper.invalidate_observations()?;
        let snapshot = self
            .helper
            .unit_properties(&self.unit, &["LoadState", "ActiveState", "FreezerState"])?;
        if snapshot.values.get("LoadState").map(String::as_str) == Some("not-found") {
            return Ok(());
        }
        let values = snapshot
            .values
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let state = SystemdLaunchUnitBackend::parse_unit_state(&values)?;
        if !state.needs_thaw_before_stop() {
            return Ok(());
        }
        self.call("ThawUnit")
    }
}

fn valid_shell_unit(unit: &str) -> bool {
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

/// Records shell freezer requests the way the systemd shell unit applies
/// them, so session tests can prove when korrid freezes and thaws it.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct RecordingShellUnit {
    identity_reads: std::sync::atomic::AtomicUsize,
    identity_only: bool,
    frozen: Mutex<bool>,
    requests: Mutex<Vec<&'static str>>,
    events: Option<Arc<Mutex<Vec<&'static str>>>>,
    refused_thaws: Mutex<usize>,
    refuse_freeze: Mutex<bool>,
    pub(crate) thawed: tokio::sync::Notify,
}

#[cfg(test)]
impl RecordingShellUnit {
    pub(crate) fn identity_reads(&self) -> usize {
        self.identity_reads
            .load(std::sync::atomic::Ordering::SeqCst)
    }
    pub(crate) fn identity_only() -> Self {
        Self {
            identity_only: true,
            ..Self::default()
        }
    }
    /// A shell an earlier korrid run froze and never thawed.
    pub(crate) fn left_frozen() -> Self {
        Self {
            frozen: Mutex::new(true),
            ..Self::default()
        }
    }

    /// Adds `shell-freeze` and `shell-thaw` to a log shared with other
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
impl ShellUnit for RecordingShellUnit {
    fn freezer_enabled(&self) -> bool {
        !self.identity_only
    }
    fn window_pids(&self) -> Result<BTreeSet<i32>, LaunchUnitError> {
        self.identity_reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok([4100].into_iter().collect())
    }

    fn freeze(&self) -> Result<(), LaunchUnitError> {
        if self.identity_only {
            return Ok(());
        }
        self.record("freeze", "shell-freeze");
        if *self.refuse_freeze.lock().unwrap() {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "shell freeze refused",
            ));
        }
        *self.frozen.lock().unwrap() = true;
        Ok(())
    }

    fn thaw(&self) -> Result<(), LaunchUnitError> {
        if self.identity_only {
            return Ok(());
        }
        self.record("thaw", "shell-thaw");
        let mut refused = self.refused_thaws.lock().unwrap();
        if *refused > 0 {
            *refused -= 1;
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "shell thaw refused",
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
    cli_socket: PathBuf,
    cli_directory: PathBuf,
    stream_private_state_root: Option<PathBuf>,
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
enum UnitObservation {
    Native(
        Arc<std::sync::OnceLock<Result<Arc<super::systemd_observation::NativeUnitReader>, String>>>,
    ),
    #[cfg(test)]
    Commands,
}

#[derive(Clone, Debug)]
pub struct SystemdLaunchUnitBackend {
    observation: UnitObservation,
    stream_viewer: bool,
    pub(super) systemd_run: PathBuf,
    pub(super) systemctl: PathBuf,
    runtime_uid: u32,
    runtime_gid: u32,
    private_state_root: PathBuf,
    control_socket: PathBuf,
    control_directory: PathBuf,
    cli_socket: PathBuf,
    cli_directory: PathBuf,
    stream_private_state_root: Option<PathBuf>,
    compositor_control_directory: PathBuf,
    certificate_control_directory: PathBuf,
    compositor_wayland_socket: Option<PathBuf>,
    diagnostics_root: PathBuf,
    helper_timeout: Duration,
    /// setfacl, for a lifecycle viewer's runtime directory (KORRID_SETFACL).
    acl_editor: Option<PathBuf>,
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
type LaunchServiceProbe = Arc<dyn Fn() -> Result<(), LaunchUnitError> + Send + Sync>;

#[cfg(test)]
#[derive(Default)]
pub(crate) struct InMemoryLaunchUnitBackend {
    units: Mutex<BTreeMap<String, LaunchUnitState>>,
    status_text: Mutex<BTreeMap<String, String>>,
    failed_units: Mutex<BTreeMap<String, i32>>,
    runners: Mutex<BTreeMap<String, String>>,
    displays: Mutex<BTreeMap<String, crate::launcher::plugin_launch::PluginDisplay>>,
    placements: Mutex<BTreeMap<String, super::display_placement::Placement>>,
    window_pids: Mutex<BTreeMap<String, BTreeSet<i32>>>,
    thawed: Mutex<Vec<String>>,
    frozen: Mutex<Vec<String>>,
    launch_probe: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    hook_probe: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    service_probe: Mutex<Option<LaunchServiceProbe>>,
    stop_behavior: Mutex<StopBehavior>,
    state_unavailable: std::sync::atomic::AtomicBool,
    launch_state: Mutex<Option<LaunchUnitState>>,
}

#[cfg(test)]
impl InMemoryLaunchUnitBackend {
    pub(crate) fn set_status_text(&self, launch_id: &str, text: &str) {
        self.status_text
            .lock()
            .unwrap()
            .insert(launch_id.into(), text.into());
    }

    pub(crate) fn set_service_probe(&self, probe: LaunchServiceProbe) {
        *self.service_probe.lock().unwrap() = Some(probe);
    }

    pub(crate) fn set_launch_probe(&self, probe: Arc<dyn Fn() + Send + Sync>) {
        *self.launch_probe.lock().unwrap() = Some(probe);
    }

    pub(crate) fn set_hook_probe(&self, probe: Arc<dyn Fn() + Send + Sync>) {
        *self.hook_probe.lock().unwrap() = Some(probe);
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

    pub(crate) fn fail_live(&self, exit_status: i32) {
        let units = self.units.lock().unwrap();
        let launch_id = units
            .iter()
            .find(|(_, state)| **state != LaunchUnitState::Completed)
            .map(|(launch_id, _)| launch_id.clone())
            .expect("one live test launch");
        self.failed_units
            .lock()
            .unwrap()
            .insert(launch_id, exit_status);
    }

    pub(crate) fn freeze_count(&self) -> usize {
        self.frozen.lock().unwrap().len()
    }

    pub(crate) fn thaw_count(&self) -> usize {
        self.thawed.lock().unwrap().len()
    }
}

#[cfg(test)]
impl LaunchUnitBackend for InMemoryLaunchUnitBackend {
    fn apply_sunshine_settings(&self, _args: &[String]) -> Result<(), LaunchUnitError> {
        self.service_probe
            .lock()
            .unwrap()
            .as_ref()
            .expect("configured launch service")()
    }

    fn run_hook(
        &self,
        _: &str,
        _: usize,
        _: &[String],
        _: &str,
        _: Duration,
        _: &BTreeMap<String, String>,
        _: &LaunchSandbox,
    ) -> Result<(), LaunchUnitError> {
        if let Some(probe) = self.hook_probe.lock().unwrap().clone() {
            probe();
            Ok(())
        } else {
            Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "hook execution unavailable",
            ))
        }
    }

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
        _sandbox: &super::systemd_unit::LaunchSandbox,
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
        if let Some(encoded) = environment.get(DISPLAY_ENV) {
            let display = serde_json::from_str(encoded)
                .map_err(|e| LaunchUnitError::new(LaunchUnitErrorKind::Protocol, e.to_string()))?;
            self.displays
                .lock()
                .unwrap()
                .insert(launch_id.into(), display);
        }
        if let Some(encoded) = environment.get(PLACEMENT_ENV) {
            let placement = serde_json::from_str(encoded)
                .map_err(|e| LaunchUnitError::new(LaunchUnitErrorKind::Protocol, e.to_string()))?;
            self.placements
                .lock()
                .unwrap()
                .insert(launch_id.into(), placement);
        }
        if let Some(runner_id) = environment.get(RUNNER_ID_ENV) {
            self.runners
                .lock()
                .unwrap()
                .insert(launch_id.into(), runner_id.clone());
        }
        Ok(())
    }

    fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError> {
        if let Some(status) = self.failed_units.lock().unwrap().get(launch_id) {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::UnitFailed,
                format!("game unit failed with exit status {status}"),
            ));
        }
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
            self.failed_units.lock().unwrap().remove(launch_id);
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
        self.frozen.lock().unwrap().push(launch_id.into());
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

    fn placement(
        &self,
        launch_id: &str,
    ) -> Result<Option<super::display_placement::Placement>, LaunchUnitError> {
        Ok(self.placements.lock().unwrap().get(launch_id).cloned())
    }

    fn display(
        &self,
        launch_id: &str,
    ) -> Result<Option<crate::launcher::plugin_launch::PluginDisplay>, LaunchUnitError> {
        Ok(self.displays.lock().unwrap().get(launch_id).cloned())
    }

    fn state_with_status(
        &self,
        launch_id: &str,
    ) -> Result<(LaunchUnitState, Option<String>), LaunchUnitError> {
        self.state(launch_id).map(|state| {
            (
                state,
                self.status_text.lock().unwrap().get(launch_id).cloned(),
            )
        })
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
    pub(super) fn with_native_observation(
        mut self,
        reader: Arc<super::systemd_observation::NativeUnitReader>,
    ) -> Self {
        let source = std::sync::OnceLock::new();
        source.set(Ok(reader)).unwrap();
        self.observation = UnitObservation::Native(Arc::new(source));
        self
    }

    fn invalidate_observations(&self) -> Result<(), LaunchUnitError> {
        match &self.observation {
            UnitObservation::Native(source) => {
                if let Some(Ok(reader)) = source.get() {
                    reader.invalidate(self.helper_timeout).map_err(|error| {
                        LaunchUnitError::new(LaunchUnitErrorKind::Failed, error)
                    })?;
                }
            }
            #[cfg(test)]
            UnitObservation::Commands => (),
        }
        Ok(())
    }

    fn unit_properties(
        &self,
        unit: &str,
        _properties: &[&str],
    ) -> Result<super::systemd_observation::UnitProperties, LaunchUnitError> {
        use super::systemd_observation::{NativeUnitReader, UnitProperties};
        let failed = |message| LaunchUnitError::new(LaunchUnitErrorKind::Failed, message);
        match &self.observation {
            UnitObservation::Native(reader) => {
                let reader = reader
                    .get_or_init(|| {
                        NativeUnitReader::new()
                            .map(Arc::new)
                            .map_err(|error| error.to_string())
                    })
                    .as_ref()
                    .map_err(|error| failed(error.clone()))?;
                Ok(reader
                    .read(unit, self.helper_timeout)
                    .map_err(failed)?
                    .unwrap_or_else(|| UnitProperties {
                        values: BTreeMap::from([("LoadState".into(), "not-found".into())]),
                        environment: Vec::new(),
                    }))
            }
            #[cfg(test)]
            UnitObservation::Commands => {
                // Existing process fixtures supply controlled native facts.
                // Production never selects this source or falls back to it.
                let mut arguments = vec![
                    "--system".into(),
                    "--no-ask-password".into(),
                    "show".into(),
                    unit.into(),
                ];
                arguments.extend(_properties.iter().map(|name| format!("--property={name}")));
                let output = self.run(&self.systemctl, &arguments)?;
                let text = std::str::from_utf8(&output.stdout)
                    .map_err(|error| failed(error.to_string()))?;
                let values: BTreeMap<String, String> = text
                    .lines()
                    .filter_map(|line| line.split_once('='))
                    .map(|(key, value)| (key.into(), value.into()))
                    .collect();
                if values.get("LoadState").map(String::as_str) != Some("not-found")
                    && !output.status.success()
                {
                    return Err(failed(format!(
                        "systemctl show failed with {}: {}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr).trim()
                    )));
                }
                let environment = values
                    .get("Environment")
                    .and_then(|value| Self::parse_systemd_environment(value).ok())
                    .unwrap_or_default();
                Ok(UnitProperties {
                    values,
                    environment,
                })
            }
        }
    }

    /// Only the native viewer may read Moonlight's client private key.
    pub(crate) fn for_stream_viewer(mut self) -> Self {
        self.stream_viewer = true;
        self
    }

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
            Some(PathBuf::from("/home/korri/.config/sunshine")),
            PathBuf::from(DEFAULT_COMPOSITOR_CONTROL_DIRECTORY),
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn with_protected_paths(
        systemd_run: PathBuf,
        systemctl: PathBuf,
        runtime_uid: u32,
        runtime_gid: u32,
        private_state_root: PathBuf,
        control_socket: PathBuf,
        control_directory: PathBuf,
        stream_private_state_root: Option<PathBuf>,
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
                cli_socket: crate::cli::DEFAULT_SOCKET.into(),
                cli_directory: PathBuf::from(crate::cli::DEFAULT_SOCKET)
                    .parent()
                    .unwrap()
                    .to_owned(),
                stream_private_state_root,
                compositor_control_directory,
                certificate_control_directory: PathBuf::from(DEFAULT_CERTIFICATE_CONTROL_DIRECTORY),
            },
            DEFAULT_HELPER_TIMEOUT,
        )
        .map(|mut backend| {
            backend.observation = UnitObservation::Commands;
            backend
        })
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
                cli_socket: crate::cli::DEFAULT_SOCKET.into(),
                cli_directory: PathBuf::from(crate::cli::DEFAULT_SOCKET)
                    .parent()
                    .unwrap()
                    .to_owned(),
                stream_private_state_root: Some(PathBuf::from("/home/korri/.config/sunshine")),
                compositor_control_directory: PathBuf::from(DEFAULT_COMPOSITOR_CONTROL_DIRECTORY),
                certificate_control_directory: PathBuf::from(DEFAULT_CERTIFICATE_CONTROL_DIRECTORY),
            },
            helper_timeout,
        )
        .map(|mut backend| {
            backend.observation = UnitObservation::Commands;
            backend
        })
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
            cli_socket,
            cli_directory,
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
            ("CLI socket", &cli_socket),
            ("CLI directory", &cli_directory),
            (
                "compositor control directory",
                &compositor_control_directory,
            ),
            (
                "certificate control directory",
                &certificate_control_directory,
            ),
        ]
        .into_iter()
        .chain(
            stream_private_state_root
                .as_ref()
                .map(|path| ("stream-host private state root", path)),
        ) {
            if !valid_protected_path(path) {
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::InvalidConfiguration,
                    format!("{name} must be a normalized absolute path without whitespace"),
                ));
            }
        }
        if control_socket.parent() != Some(control_directory.as_path())
            || cli_socket.parent() != Some(cli_directory.as_path())
        {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "control socket must be directly inside the configured control directory",
            ));
        }
        Ok(Self {
            observation: UnitObservation::Native(Arc::new(std::sync::OnceLock::new())),
            stream_viewer: false,
            systemd_run,
            systemctl,
            runtime_uid,
            runtime_gid,
            private_state_root,
            control_socket,
            control_directory,
            cli_socket,
            cli_directory,
            stream_private_state_root,
            compositor_control_directory,
            certificate_control_directory,
            compositor_wayland_socket: None,
            diagnostics_root: PathBuf::from("/run/korrid/launches"),
            helper_timeout,
            acl_editor: None,
        })
    }

    fn for_current_process() -> Self {
        let systemd_run = std::env::var_os("KORRID_SYSTEMD_RUN")
            .map(PathBuf::from)
            .unwrap_or_default();
        let systemctl = std::env::var_os("KORRID_SYSTEMCTL")
            .map(PathBuf::from)
            .unwrap_or_default();
        let (runtime_uid, runtime_gid) = configured_runtime_identity();
        let private_state_root =
            configured_path("KORRID_PRIVATE_STATE_ROOT", DEFAULT_PRIVATE_STATE_ROOT);
        let control_socket = configured_path("KORRID_CONTROL_SOCKET", DEFAULT_CONTROL_SOCKET);
        let control_directory =
            configured_path("KORRID_CONTROL_DIRECTORY", DEFAULT_CONTROL_DIRECTORY);
        let cli_socket = configured_path("KORRID_CLI_SOCKET", crate::cli::DEFAULT_SOCKET);
        let cli_directory = cli_socket.parent().unwrap_or(Path::new("/")).to_owned();
        let stream_private_state_root =
            optional_path(std::env::var_os("KORRID_STREAM_PRIVATE_STATE_ROOT"));
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
                cli_socket: cli_socket.clone(),
                cli_directory: cli_directory.clone(),
                stream_private_state_root: stream_private_state_root.clone(),
                compositor_control_directory: compositor_control_directory.clone(),
                certificate_control_directory: certificate_control_directory.clone(),
            },
            DEFAULT_HELPER_TIMEOUT,
        )
        .unwrap_or(Self {
            observation: UnitObservation::Native(Arc::new(std::sync::OnceLock::new())),
            stream_viewer: false,
            systemd_run,
            systemctl,
            runtime_uid,
            runtime_gid,
            private_state_root,
            control_socket,
            control_directory,
            cli_socket,
            cli_directory,
            stream_private_state_root,
            compositor_control_directory,
            certificate_control_directory,
            compositor_wayland_socket: None,
            diagnostics_root: PathBuf::from("/run/korrid/launches"),
            helper_timeout: DEFAULT_HELPER_TIMEOUT,
            acl_editor: None,
        })
        .with_compositor_wayland_socket(
            std::env::var_os("KORRID_COMPOSITOR_WAYLAND_SOCKET").map(PathBuf::from),
        )
        .with_acl_editor(std::env::var_os("KORRID_SETFACL").map(PathBuf::from))
    }

    pub(super) fn with_acl_editor(mut self, setfacl: Option<PathBuf>) -> Self {
        self.acl_editor = setfacl;
        self
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
                cli_socket: self.cli_socket.clone(),
                cli_directory: self.cli_directory.clone(),
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

    #[cfg(test)]
    pub(super) fn launch_arguments(
        &self,
        launch_id: &str,
        command: &[String],
        environment: &BTreeMap<String, String>,
    ) -> Result<Vec<String>, LaunchUnitError> {
        self.launch_arguments_in_sandbox(
            launch_id,
            command,
            environment,
            &LaunchSandbox::new(command, environment),
        )
    }

    pub(super) fn launch_arguments_in_sandbox(
        &self,
        launch_id: &str,
        configured_command: &[String],
        environment: &BTreeMap<String, String>,
        sandbox: &LaunchSandbox,
    ) -> Result<Vec<String>, LaunchUnitError> {
        self.validate()?;
        let unit = Self::unit_name(launch_id)?;
        if configured_command.is_empty() {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "configured command is empty",
            ));
        }
        let mut arguments = vec![
            "--system".into(),
            "--no-ask-password".into(),
            "--quiet".into(),
            // Retain failed-game metadata until the controller reads and resets it.
            "--service-type=exec".into(),
            format!("--unit={unit}"),
            "--slice=korri-games.slice".into(),
            "--property=CPUWeight=100".into(),
            // Steam's native shutdown allows 15s plus 5s before cleaning Wine
            // leftovers. Keep margin for that cleanup and save-on-exit; this
            // bounds only game termination, not after-hooks or save capture.
            "--property=TimeoutStopSec=30s".into(),
        ];
        arguments.extend(self.sandbox_properties(launch_id, sandbox)?);
        arguments.extend(Self::process_environment(environment, sandbox.modifiers));
        arguments.push("--".into());
        arguments.extend(configured_command.iter().cloned());
        Ok(arguments)
    }

    // Both game and hook units consume this builder. Mount and identity policy
    // must never diverge when a sandbox boundary changes.
    fn sandbox_properties(
        &self,
        launch_id: &str,
        sandbox: &LaunchSandbox,
    ) -> Result<Vec<String>, LaunchUnitError> {
        let modifiers = sandbox.modifiers;
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
        if sandbox.pulse_audio {
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
                " {}:/run/{runtime}/diagnostics/wayland-parent",
                socket.display()
            ));
            Some(runtime)
        } else {
            None
        };
        let mut arguments = vec![
            format!("--uid={}", self.runtime_uid),
            format!("--gid={}", self.runtime_gid),
            "--property=KillMode=control-group".into(),
            // Session children report runner steps; STATUS= also retains plugin-launch refusals.
            "--property=NotifyAccess=all".into(),
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
                "--property=InaccessiblePaths={} /run/korrid {} {} {} {}{} {} -{} /run/user/{} -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources{}",
                self.private_state_root.display(),
                self.control_socket.display(),
                self.control_directory.display(),
                self.cli_socket.display(),
                self.cli_directory.display(),
                self.stream_private_state_root.as_ref().map(|path| format!(" -{}", path.display())).unwrap_or_default(),
                self.compositor_control_directory.display(),
                self.certificate_control_directory.display(),
                self.runtime_uid,
                if self.stream_viewer { "" } else { " -/home/korri/.config/moonlight" }
            ),
            "--property=RestrictSUIDSGID=yes".into(),
        ];
        if let Some(source) = &sandbox.netplay_config_source {
            arguments.push(format!(
                "--property=LoadCredential={}:{}",
                crate::netplay::password::CREDENTIAL,
                source.display()
            ));
        }
        if sandbox.plugin {
            let runtime = format!("korri-game-{launch_id}");
            arguments.extend([
                format!("--property=RuntimeDirectory={runtime}"),
                "--property=RuntimeDirectoryMode=0700".into(),
                format!(
                    "--property=BindPaths={}:/run/{runtime}/exit-status",
                    self.diagnostics_root
                        .join(launch_id)
                        .join(super::launch_diagnostics::EXIT_STATUS)
                        .display()
                ),
                format!(
                    "--setenv={}=/run/{runtime}/exit-status",
                    super::launch_diagnostics::EXIT_STATUS_ENV
                ),
                format!(
                    "--setenv={}={}",
                    super::launch_diagnostics::READER_ENV,
                    unsafe { libc::geteuid() }
                ),
            ]);
        }
        if let Some(runtime) = modifier_runtime {
            arguments.retain(|argument| !argument.starts_with("--property=BindPaths="));
            arguments.extend([
                // Keep XDG runtime entries in systemd's runtime-user-owned
                // directory. systemd removes it on stop, including after SIGKILL.
                // Binding diagnostics over the whole runtime lets private game
                // subdirectories defeat korrid's unprivileged cleanup.
                format!(
                    "--property=BindPaths={}:/run/{runtime}/exit-status {}:/run/{runtime}/diagnostics",
                    self.diagnostics_root.join(launch_id).join(super::launch_diagnostics::EXIT_STATUS).display(),
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
                    "--setenv={}=/run/{runtime}/diagnostics",
                    super::launch_diagnostics::DIRECTORY_ENV
                ),
                format!("--setenv=XDG_RUNTIME_DIR=/run/{runtime}"),
                "--setenv=WAYLAND_DISPLAY=diagnostics/wayland-parent".into(),
            ]);
        }
        Ok(arguments)
    }

    fn process_environment(environment: &BTreeMap<String, String>, modifiers: bool) -> Vec<String> {
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
                        | super::launch_diagnostics::EXIT_STATUS_ENV
                ) && !(modifiers && key.as_str() == "DISPLAY")
            })
            .map(|(key, value)| format!("--setenv={key}={value}"))
            .collect()
    }

    fn collect_previous_hook_attempt(
        &self,
        launch_id: &str,
        step: usize,
    ) -> Result<(), LaunchUnitError> {
        let unit = format!("korri-hook-{launch_id}-{step}.service");
        let deadline = Instant::now() + self.helper_timeout;
        let mut stopped = false;
        loop {
            let output = self.run(
                &self.systemctl,
                &[
                    "--system".into(),
                    "--no-ask-password".into(),
                    "show".into(),
                    unit.clone(),
                    "--property=LoadState".into(),
                    "--property=ActiveState".into(),
                ],
            )?;
            let text = String::from_utf8_lossy(&output.stdout);
            let values: BTreeMap<_, _> = text
                .lines()
                .filter_map(|line| line.split_once('='))
                .collect();
            if values.get("LoadState") == Some(&"not-found") {
                return Ok(());
            }
            if !output.status.success() || !values.contains_key("LoadState") {
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Protocol,
                    "cannot observe previous hook attempt",
                ));
            }
            if !stopped {
                // A daemon restart may leave this exact --wait client/unit
                // running. End it before replaying the unfinished step.
                self.require_success(
                    &self.systemctl,
                    &[
                        "--system".into(),
                        "--no-ask-password".into(),
                        "stop".into(),
                        unit.clone(),
                    ],
                )?;
                stopped = true;
            }
            if Instant::now() >= deadline {
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Timeout,
                    "previous hook attempt was not collected",
                ));
            }
            thread::sleep(HELPER_POLL_INTERVAL);
        }
    }

    #[cfg(test)]
    pub(super) fn hook_arguments(
        &self,
        launch_id: &str,
        step: usize,
        game_command: &[String],
        run: &str,
        timeout: Duration,
        environment: &BTreeMap<String, String>,
    ) -> Result<Vec<String>, LaunchUnitError> {
        self.hook_arguments_in_sandbox(
            launch_id,
            step,
            game_command,
            run,
            timeout,
            environment,
            &LaunchSandbox::new(game_command, environment),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn hook_arguments_in_sandbox(
        &self,
        launch_id: &str,
        step: usize,
        game_command: &[String],
        run: &str,
        timeout: Duration,
        environment: &BTreeMap<String, String>,
        sandbox: &LaunchSandbox,
    ) -> Result<Vec<String>, LaunchUnitError> {
        self.validate()?;
        validate_launch_id(launch_id)?;
        if step > 999 || timeout.is_zero() || run.is_empty() || run.contains('\0') {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "invalid hook step",
            ));
        }
        let mut arguments = vec![
            "--system".into(),
            "--no-ask-password".into(),
            "--quiet".into(),
            "--wait".into(),
            "--collect".into(),
            "--service-type=exec".into(),
            "--slice=korri-games.slice".into(),
            "--property=CPUWeight=100".into(),
            format!("--unit=korri-hook-{launch_id}-{step}.service"),
            format!("--property=RuntimeMaxSec={}us", timeout.as_micros()),
            "--property=TimeoutStopSec=5s".into(),
        ];
        arguments.extend(self.sandbox_properties(launch_id, sandbox)?);
        // The game's prepared plan sets these values inside plugin-launch,
        // after the transport defaults. The shell hook has no such wrapper.
        arguments.extend(
            environment
                .iter()
                .map(|(key, value)| format!("--setenv={key}={value}")),
        );
        let mut unset: BTreeSet<String> = super::hooks::VARIABLES
            .iter()
            .filter(|name| !environment.contains_key(**name))
            .map(|name| (*name).to_owned())
            .collect();
        if let Some((_, prepared)) = crate::launcher::plugin_launch::declared_saves(game_command)
            .map_err(|message| {
                LaunchUnitError::new(LaunchUnitErrorKind::InvalidConfiguration, message)
            })?
        {
            unset.extend(
                prepared
                    .plan
                    .env_unset
                    .into_iter()
                    .filter(|name| !environment.contains_key(name)),
            );
        }
        if !unset.is_empty() {
            arguments.push(format!(
                "--property=UnsetEnvironment={}",
                unset.into_iter().collect::<Vec<_>>().join(" ")
            ));
        }
        arguments.extend(["--".into(), "/bin/sh".into(), "-c".into(), run.into()]);
        Ok(arguments)
    }

    /// A live hook acts on the running game, so it must see the game's
    /// processes. `PrivatePIDs=yes` on a new unit makes a fresh PID namespace,
    /// not the game's: the hook then finds no game process and every reshape
    /// is refused (aka, 2026-11-11). Identity, mounts, capabilities and the
    /// rest of the game's sandbox stay. Before/after hooks keep the namespace.
    pub(super) fn live_hook_arguments(
        &self,
        launch_id: &str,
        game_command: &[String],
        run: &str,
        timeout: Duration,
        environment: &BTreeMap<String, String>,
        sandbox: &LaunchSandbox,
    ) -> Result<Vec<String>, LaunchUnitError> {
        let mut arguments = self.hook_arguments_in_sandbox(
            launch_id,
            0,
            game_command,
            run,
            timeout,
            environment,
            sandbox,
        )?;
        arguments.retain(|argument| argument != "--property=PrivatePIDs=yes");
        // The hook reports its observed size on stdout.
        arguments.insert(1, "--pipe".into());
        Ok(arguments)
    }

    pub(super) fn stop_arguments(launch_id: &str) -> Result<Vec<String>, LaunchUnitError> {
        let mut arguments = Self::unit_verb_arguments("stop", launch_id)?;
        // Acknowledge the job inside inputd's control budget. The session
        // observer owns completion, including a later timeout/SIGKILL.
        arguments.push("--no-block".into());
        Ok(arguments)
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

    // This is the diagnostic part of an already-observed UnitFailed result.
    // Keep it separate from native observation so its lock-held work can
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
    fn apply_sunshine_settings(&self, args: &[String]) -> Result<(), LaunchUnitError> {
        self.apply_sunshine_at(
            Path::new("/run/korri-launch-services/korri-sunshine.args"),
            args,
        )
    }

    fn launch_viewer(
        &self,
        launch: &str,
        command: &[String],
        environment: &BTreeMap<String, String>,
        sandbox: &LaunchSandbox,
        certificate: &str,
        controller: Option<u32>,
    ) -> Result<(), LaunchUnitError> {
        let args = self.viewer_arguments(
            launch,
            command,
            environment,
            sandbox,
            Some(certificate),
            controller,
        )?;
        self.require_success(&self.systemd_run, &args).map(|_| ())
    }
    fn bootstrap_viewer(
        &self,
        launch: &str,
        command: &[String],
        environment: &BTreeMap<String, String>,
        sandbox: &LaunchSandbox,
    ) -> Result<String, LaunchUnitError> {
        self.run_viewer_bootstrap(launch, command, environment, sandbox)
    }
    fn run_live_hook(
        &self,
        launch_id: &str,
        command: &[String],
        run: &str,
        timeout: Duration,
        environment: &BTreeMap<String, String>,
        sandbox: &LaunchSandbox,
    ) -> Result<String, LaunchUnitError> {
        if !matches!(self.state(launch_id)?, LaunchUnitState::Running) {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "game is not running",
            ));
        }
        let args =
            self.live_hook_arguments(launch_id, command, run, timeout, environment, sandbox)?;
        self.collect_previous_hook_attempt(launch_id, 0)?;
        let mut helper = self.clone();
        helper.helper_timeout = timeout.saturating_add(Duration::from_secs(15));
        helper
            .require_success(&self.systemd_run, &args)
            .and_then(|output| {
                String::from_utf8(output.stdout).map_err(|_| {
                    LaunchUnitError::new(
                        LaunchUnitErrorKind::Protocol,
                        "live hook returned non-UTF-8 output",
                    )
                })
            })
    }

    fn control_group(&self, launch_id: &str) -> Result<String, LaunchUnitError> {
        let unit = Self::unit_name(launch_id)?;
        let output = self.require_success(
            &self.systemctl,
            &[
                "show".into(),
                unit,
                "--property=ControlGroup".into(),
                "--value".into(),
            ],
        )?;
        let group = String::from_utf8(output.stdout).map_err(|_| {
            LaunchUnitError::new(LaunchUnitErrorKind::Protocol, "non-UTF-8 control group")
        })?;
        let group = group.trim();
        if !group.starts_with('/')
            || group == "/"
            || group.contains("..")
            || group.chars().any(char::is_whitespace)
        {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                "invalid game control group",
            ));
        }
        Ok(group.into())
    }
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
        sandbox: &super::systemd_unit::LaunchSandbox,
    ) -> Result<(), LaunchUnitError> {
        let arguments =
            self.launch_arguments_in_sandbox(launch_id, configured_command, environment, sandbox)?;
        // Before-hook channels were removed when those units finished. The
        // game gets a fresh inode, never an exit value authored by a hook.
        if sandbox.plugin {
            super::launch_diagnostics::create_exit_channel(
                &self.diagnostics_root.join(launch_id),
                self.runtime_uid,
            )
            .map_err(|error| {
                LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    format!("cannot prepare exit channel: {error}"),
                )
            })?;
        }
        if sandbox.modifiers {
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

    fn run_hook(
        &self,
        launch_id: &str,
        step: usize,
        game_command: &[String],
        run: &str,
        timeout: Duration,
        environment: &BTreeMap<String, String>,
        sandbox: &super::systemd_unit::LaunchSandbox,
    ) -> Result<(), LaunchUnitError> {
        let arguments = self.hook_arguments_in_sandbox(
            launch_id,
            step,
            game_command,
            run,
            timeout,
            environment,
            sandbox,
        )?;
        self.collect_previous_hook_attempt(launch_id, step)?;
        if sandbox.plugin {
            super::launch_diagnostics::create_exit_channel(
                &self.diagnostics_root.join(launch_id),
                self.runtime_uid,
            )
            .map_err(|error| {
                LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    format!("cannot prepare hook exit channel: {error}"),
                )
            })?;
        }
        if sandbox.modifiers {
            super::launch_diagnostics::create(
                &self.diagnostics_root.join(launch_id),
                self.runtime_uid,
            )
            .map_err(|error| {
                LaunchUnitError::new(LaunchUnitErrorKind::Failed, error.to_string())
            })?;
        }
        // The client must outlive RuntimeMaxSec and systemd's bounded stop job.
        let mut helper = self.clone();
        helper.helper_timeout = timeout.saturating_add(Duration::from_secs(15));
        let result = helper
            .run(&self.systemd_run, &arguments)
            .and_then(|output| {
                if output.status.success() {
                    Ok(())
                } else {
                    let reason = match output.status.code() {
                        Some(code) => format!("exited with status {code}"),
                        None => format!("terminated ({})", output.status),
                    };
                    Err(LaunchUnitError::new(LaunchUnitErrorKind::Failed, reason))
                }
            });
        if result.is_err() {
            // A lost client is not proof the service stopped. Exact-stop before
            // returning, so an abort cannot overlap the next launch.
            self.require_success(
                &self.systemctl,
                &[
                    "--system".into(),
                    "--no-ask-password".into(),
                    "stop".into(),
                    format!("korri-hook-{launch_id}-{step}.service"),
                ],
            )
            .or_else(|error| {
                // --collect may already have removed a failed hook.
                let output = self.run(
                    &self.systemctl,
                    &[
                        "--system".into(),
                        "show".into(),
                        format!("korri-hook-{launch_id}-{step}.service"),
                        "--property=LoadState".into(),
                        "--property=ActiveState".into(),
                    ],
                )?;
                let text = String::from_utf8_lossy(&output.stdout);
                if text.lines().any(|line| {
                    matches!(
                        line,
                        "LoadState=not-found" | "ActiveState=inactive" | "ActiveState=failed"
                    )
                }) {
                    Ok(output)
                } else {
                    Err(error)
                }
            })?;
        }
        // Hooks may write their own diagnostic file. Before a game starts it
        // is discarded; after a game ends its real status is already journaled.
        if sandbox.plugin {
            super::launch_diagnostics::cleanup(&self.diagnostics_root.join(launch_id)).map_err(
                |error| {
                    LaunchUnitError::new(
                        LaunchUnitErrorKind::Failed,
                        format!("cannot clean hook diagnostics: {error}"),
                    )
                },
            )?;
        }
        result
    }

    fn cleanup_completed(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        validate_launch_id(launch_id)?;
        super::launch_diagnostics::cleanup(&self.diagnostics_root.join(launch_id)).map_err(
            |error| {
                LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    format!("cannot clean game diagnostics: {error}"),
                )
            },
        )
    }

    fn exit_status(&self, launch_id: &str) -> Result<Option<i32>, LaunchUnitError> {
        validate_launch_id(launch_id)?;
        if let Ok(Some(status)) = super::launch_diagnostics::read_exit_status(
            &self.diagnostics_root.join(launch_id),
            unsafe { libc::geteuid() },
        ) {
            return Ok(Some(status));
        }
        let output = self.require_success(&self.systemctl, &Self::state_arguments(launch_id)?)?;
        let text = String::from_utf8_lossy(&output.stdout);
        let values: BTreeMap<_, _> = text
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        Ok(
            if values
                .get("ExecMainCode")
                .and_then(|v| v.parse::<i32>().ok())
                == Some(libc::CLD_EXITED)
            {
                values
                    .get("ExecMainStatus")
                    .and_then(|v| v.parse::<i32>().ok())
            } else {
                None
            },
        )
    }

    fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError> {
        self.state_with_status(launch_id).map(|(state, _)| state)
    }

    fn state_with_status(
        &self,
        launch_id: &str,
    ) -> Result<(LaunchUnitState, Option<String>), LaunchUnitError> {
        let snapshot = self.unit_properties(
            &Self::unit_name(launch_id)?,
            &[
                "LoadState",
                "ActiveState",
                "FreezerState",
                "ExecMainCode",
                "ExecMainStatus",
                "StatusText",
                "Environment",
            ],
        )?;
        let directory = self.diagnostics_root.join(launch_id);
        let values: BTreeMap<&str, &str> = snapshot
            .values
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        // Only the host-injected exact-unit environment marker grants access
        // to these files. It survives daemon restarts without a second cache.
        let marker = format!(
            "{}=/run/korri-game-{launch_id}/diagnostics",
            super::launch_diagnostics::DIRECTORY_ENV
        );
        let captures_stderr = snapshot.environment.contains(&marker);
        let diagnostics_available = if captures_stderr {
            // spawn_shell_watch already observes the live session every second,
            // including when the shell cannot poll. No new maintenance thread.
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
        } else {
            Self::parse_unit_state(&values)
        };
        let observed = match state {
            Err(error)
                if error.kind == LaunchUnitErrorKind::UnitFailed
                    && diagnostics_available
                    && values.get("StatusText").is_none_or(|value| {
                        // Progress is not a refusal and must not hide stderr.
                        value.trim().is_empty() || value.starts_with("korri-step:")
                    }) =>
            {
                Err(self.failure_with_stderr(launch_id, error))
            }
            Ok(LaunchUnitState::Completed) => {
                if super::launch_diagnostics::read_exit_status(&directory, unsafe {
                    libc::geteuid()
                })
                .ok()
                .flatten()
                .is_some()
                {
                    // Preserve the bounded result until after the host's durable
                    // completion record contains it, including across a restart.
                    return Ok((LaunchUnitState::Completed, None));
                }
                super::launch_diagnostics::cleanup(&directory).map_err(|error| {
                    LaunchUnitError::new(
                        LaunchUnitErrorKind::Failed,
                        format!("cannot clean game stderr: {error}"),
                    )
                })?;
                Ok(LaunchUnitState::Completed)
            }
            other => other,
        }?;
        Ok((
            observed,
            values.get("StatusText").map(|text| (*text).to_owned()),
        ))
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

    fn placement(
        &self,
        launch_id: &str,
    ) -> Result<Option<super::display_placement::Placement>, LaunchUnitError> {
        let output =
            self.require_success(&self.systemctl, &Self::runner_id_arguments(launch_id)?)?;
        let value = std::str::from_utf8(&output.stdout)
            .map_err(|e| LaunchUnitError::new(LaunchUnitErrorKind::Protocol, e.to_string()))?;
        let serialized = value.trim().strip_prefix("Environment=").ok_or_else(|| {
            LaunchUnitError::new(LaunchUnitErrorKind::Protocol, "missing Environment")
        })?;
        let mut placement = None;
        for assignment in Self::parse_systemd_environment(serialized)? {
            if let Some(value) = assignment.strip_prefix(&format!("{PLACEMENT_ENV}=")) {
                if placement.is_some() {
                    return Err(LaunchUnitError::new(
                        LaunchUnitErrorKind::Protocol,
                        "duplicate placement",
                    ));
                }
                placement = Some(serde_json::from_str(value).map_err(|e| {
                    LaunchUnitError::new(LaunchUnitErrorKind::Protocol, e.to_string())
                })?);
            }
        }
        Ok(placement)
    }

    fn display(
        &self,
        launch_id: &str,
    ) -> Result<Option<crate::launcher::plugin_launch::PluginDisplay>, LaunchUnitError> {
        let output =
            self.require_success(&self.systemctl, &Self::runner_id_arguments(launch_id)?)?;
        let value = std::str::from_utf8(&output.stdout)
            .map_err(|e| LaunchUnitError::new(LaunchUnitErrorKind::Protocol, e.to_string()))?;
        let serialized = value.trim().strip_prefix("Environment=").ok_or_else(|| {
            LaunchUnitError::new(LaunchUnitErrorKind::Protocol, "missing Environment")
        })?;
        let mut display = None;
        for assignment in Self::parse_systemd_environment(serialized)? {
            if let Some(value) = assignment.strip_prefix(&format!("{DISPLAY_ENV}=")) {
                if display.is_some() {
                    return Err(LaunchUnitError::new(
                        LaunchUnitErrorKind::Protocol,
                        "duplicate display",
                    ));
                }
                display = Some(serde_json::from_str(value).map_err(|e| {
                    LaunchUnitError::new(LaunchUnitErrorKind::Protocol, e.to_string())
                })?);
            }
        }
        Ok(display)
    }

    /// Stops the unit. systemd 259 refuses `stop` on a frozen unit
    /// ("Cannot perform operation on frozen unit"), so a unit observed
    /// frozen or in a freezer transition is thawed first. A thaw failure is
    /// a stop failure; the unit is left untouched. Callers that already
    /// know the unit completed tolerate the `Failed` result by re-reading
    /// state, as before.
    fn stop(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        self.invalidate_observations()?;
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
        self.invalidate_observations()?;
        // Stop does not reset a failed service. Release its retained exit
        // metadata only after the caller captured the failure.
        if failed {
            self.require_success(&self.systemctl, &Self::reset_failed_arguments(launch_id)?)?;
        } else {
            // A queued stop can still be running. Keep its diagnostics and
            // exit-status inode until the completion journal consumes them.
            return Ok(());
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
        self.require_success(&self.systemctl, &arguments)?;
        self.invalidate_observations()
    }

    fn thaw(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        let arguments = Self::thaw_arguments(launch_id)?;
        self.require_success(&self.systemctl, &arguments)?;
        self.invalidate_observations()
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

pub(crate) fn configured_runtime_identity() -> (u32, u32) {
    (
        configured_runtime_id("KORRID_RUNTIME_UID").unwrap_or_else(|| unsafe { libc::geteuid() }),
        configured_runtime_id("KORRID_RUNTIME_GID").unwrap_or_else(|| unsafe { libc::getegid() }),
    )
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

fn optional_path(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
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

    #[test]
    fn a_cached_main_pid_without_kernel_membership_is_not_shell_identity() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("properties");
        std::fs::write(
            &helper,
            "#!/bin/sh\nprintf 'ControlGroup=\\nMainPID=1234\\n'\n",
        )
        .unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let backend = SystemdLaunchUnitBackend::new(helper.clone(), helper, 1000, 2002).unwrap();
        let shell = SystemdShellUnit::new(backend, "korri-shell.service", false).unwrap();
        assert!(shell.window_pids().unwrap().is_empty());
    }

    #[test]
    fn games_bound_stop_without_shortening_hooks() {
        let backend = backend(1000);
        let command = vec!["/bin/game".into()];
        let environment = BTreeMap::new();
        let game = backend
            .launch_arguments(LAUNCH_ID, &command, &environment)
            .unwrap();
        assert!(game.contains(&"--property=TimeoutStopSec=30s".into()));
        let hook = backend
            .hook_arguments(
                LAUNCH_ID,
                0,
                &command,
                "true",
                Duration::from_secs(60),
                &environment,
            )
            .unwrap();
        assert!(hook.contains(&"--property=RuntimeMaxSec=60000000us".into()));
        assert!(hook.contains(&"--property=TimeoutStopSec=5s".into()));
        assert!(!hook.contains(&"--property=TimeoutStopSec=30s".into()));
    }

    #[test]
    fn stop_acknowledges_the_job_without_waiting_or_removing_live_diagnostics() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemctl");
        // A blocking stop must fail this test, not hold up the test runner.
        std::fs::write(
            &helper,
            concat!(
                "#!/bin/sh\ncase \"$3\" in\n",
                "show) printf 'LoadState=loaded\\nActiveState=deactivating\\n';;\n",
                "stop) case \"$*\" in *--no-block*) exit 0;; *) exit 42;; esac;;\n",
                "esac\n",
            ),
        )
        .unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut backend =
            SystemdLaunchUnitBackend::new(helper.clone(), helper, 1000, 1000).unwrap();
        backend.diagnostics_root = root.path().join("launches");
        let directory = backend.diagnostics_root.join(LAUNCH_ID);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("stderr"), "still live").unwrap();
        backend.stop(LAUNCH_ID).unwrap();
        assert_eq!(backend.state(LAUNCH_ID).unwrap(), LaunchUnitState::Stopping);
        assert!(directory.join("stderr").exists());
        std::fs::write(
            &backend.systemctl,
            concat!(
                "#!/bin/sh\ncase \"$3\" in\n",
                "show) printf 'LoadState=loaded\\nActiveState=active\\n';;\n",
                "stop) echo 'stop refused' >&2; exit 1;;\nesac\n",
            ),
        )
        .unwrap();
        let error = backend.stop(LAUNCH_ID).unwrap_err();
        assert_eq!(error.kind, LaunchUnitErrorKind::Failed);
        assert!(error.message.contains("stop refused"));
        assert!(directory.join("stderr").exists());
    }

    #[test]
    fn backend_without_a_stream_root_validates() {
        let mut backend = backend(1000);
        let mut expected = backend
            .launch_arguments(LAUNCH_ID, &["/bin/game".into()], &BTreeMap::new())
            .unwrap();
        let hidden = expected
            .iter_mut()
            .find(|argument| argument.starts_with("--property=InaccessiblePaths="))
            .unwrap();
        *hidden = hidden.replace(" -/home/korri/.config/sunshine", "");
        for configured in [None, Some("".into())] {
            backend.stream_private_state_root = optional_path(configured);
            assert_eq!(backend.stream_private_state_root, None);
            backend.validate().unwrap();
            assert_eq!(
                backend
                    .launch_arguments(LAUNCH_ID, &["/bin/game".into()], &BTreeMap::new())
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn invalid_configured_stream_roots_are_still_rejected() {
        let mut backend = backend(1000);
        for configured in [
            "relative/sunshine",
            "/",
            "/private/../sunshine",
            "/private/./sunshine",
            "/private//sunshine",
            "/private/space path",
            "/private/line\nbreak",
        ] {
            backend.stream_private_state_root = optional_path(Some(configured.into()));
            assert_eq!(backend.stream_private_state_root, Some(configured.into()));
            for error in [
                backend.validate().unwrap_err(),
                backend.live_launch_ids().unwrap_err(),
                backend
                    .launch_arguments(LAUNCH_ID, &["/bin/game".into()], &BTreeMap::new())
                    .unwrap_err(),
            ] {
                assert_eq!(error.kind, LaunchUnitErrorKind::InvalidConfiguration);
                assert_eq!(error.message, "stream-host private state root must be a normalized absolute path without whitespace");
            }
        }
    }

    const SHELL: &str = "korri-shell.service";

    #[test]
    fn launch_settings_units_use_the_games_slice_with_an_enabled_cpu_controller() {
        let args = backend(1000)
            .launch_arguments(LAUNCH_ID, &["/bin/game".into()], &BTreeMap::new())
            .unwrap();
        assert!(args.contains(&"--slice=korri-games.slice".into()));
        assert!(args.contains(&"--property=CPUWeight=100".into()));
        assert_eq!(
            DEFAULT_UNIT_CGROUP_ROOT,
            "/sys/fs/cgroup/korri.slice/korri-games.slice"
        );
    }

    #[test]
    fn identity_only_shell_never_calls_freezer_helpers() {
        // This backend's helper paths are not executables. Either freezer
        // call would fail if the disabled gate performed an effect.
        let shell = SystemdShellUnit::new(backend(1000), SHELL, false).unwrap();
        assert!(!shell.freezer_enabled());
        shell.freeze().unwrap();
        shell.thaw().unwrap();
    }

    #[test]
    fn shell_native_calls_need_no_unit_reference_or_extra_authority() {
        let shell = SystemdShellUnit::new(backend(1000), SHELL, true).unwrap();
        for method in ["FreezeUnit", "ThawUnit"] {
            assert_eq!(
                shell.verb_arguments(method),
                [
                    "--system",
                    "--allow-interactive-authorization=no",
                    "call",
                    "org.freedesktop.systemd1",
                    "/org/freedesktop/systemd1",
                    "org.freedesktop.systemd1.Manager",
                    method,
                    "s",
                    SHELL,
                ]
            );
        }
        // Real D-Bus authorization and settled replies are exercised by
        // clients/shell/nix/freezer-vm-test.nix, not helper emulation.
    }

    #[test]
    fn shell_unit_must_be_one_exact_non_game_service() {
        for unit in [
            "",
            ".service",
            "korri-shell",
            "korri-shell.socket",
            "-korri-shell.service",
            "../korri-shell.service",
            "korri shell.service",
            "korri-*.service",
            "korri-game-0123456789abcdef0123456789abcdef.service",
        ] {
            assert_eq!(
                SystemdShellUnit::new(backend(1000), unit, true)
                    .unwrap_err()
                    .kind,
                LaunchUnitErrorKind::InvalidConfiguration,
                "accepted {unit:?}"
            );
        }
        for unit in [SHELL, "cage@tty1.service"] {
            assert!(
                SystemdShellUnit::new(backend(1000), unit, true).is_ok(),
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
    fn game_and_modifier_sandboxes_hide_the_cli_socket_and_its_parent() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("person");
        std::fs::create_dir(&parent).unwrap();
        let socket = parent.join("cli.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let mut backend = backend(unsafe { libc::geteuid() });
        backend.cli_socket = socket.clone();
        backend.cli_directory = parent.clone();
        backend.compositor_wayland_socket = Some(root.path().join("wayland"));
        for modifiers in [false, true] {
            let properties = backend
                .sandbox_properties(
                    LAUNCH_ID,
                    &LaunchSandbox {
                        netplay_config_source: None,
                        modifiers,
                        plugin: false,
                        pulse_audio: false,
                    },
                )
                .unwrap();
            let hidden = properties
                .iter()
                .find_map(|property| property.strip_prefix("--property=InaccessiblePaths="))
                .unwrap();
            assert!(hidden
                .split_whitespace()
                .any(|path| path == socket.to_str().unwrap()));
            assert!(hidden
                .split_whitespace()
                .any(|path| path == parent.to_str().unwrap()));
        }
        let helper = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cli-sandbox.py");
        let result = std::process::Command::new(helper)
            .arg(&socket)
            .arg(&parent)
            .output()
            .unwrap();
        if result.status.code() == Some(77) {
            eprintln!("SKIP real namespace negative check: user/mount namespaces or systemd inaccessible inodes unavailable");
        } else {
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }

    #[test]
    fn games_hide_the_moonlight_keydir_and_only_viewer_units_can_read_it() {
        let backend = backend(2001);
        let command = ["/games/program".into()];
        let game = backend
            .launch_arguments(LAUNCH_ID, &command, &BTreeMap::new())
            .unwrap();
        let viewer = backend
            .for_stream_viewer()
            .launch_arguments(LAUNCH_ID, &command, &BTreeMap::new())
            .unwrap();
        let game_paths = game
            .iter()
            .find(|arg| arg.starts_with("--property=InaccessiblePaths="))
            .unwrap();
        let viewer_paths = viewer
            .iter()
            .find(|arg| arg.starts_with("--property=InaccessiblePaths="))
            .unwrap();
        assert!(game_paths.ends_with(" -/home/korri/.config/moonlight"));
        assert_eq!(
            game_paths
                .strip_suffix(" -/home/korri/.config/moonlight")
                .unwrap(),
            viewer_paths
        );
        let mut permitted = game;
        *permitted
            .iter_mut()
            .find(|arg| arg.starts_with("--property=InaccessiblePaths="))
            .unwrap() = viewer_paths.clone();
        assert_eq!(permitted, viewer);
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
                "--property=InaccessiblePaths=/var/lib/korrid /run/korrid /run/korrid-control/control.sock /run/korrid-control /run/korrid-cli/cli.sock /run/korrid-cli -/home/korri/.config/sunshine /run/korri-compositor -/run/korri-certificate-control /run/user/{uid} -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources -/home/korri/.config/moonlight"
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
    fn modifier_completion_and_stop_do_not_clean_the_games_private_runtime_entries() {
        use crate::launcher::plugin_launch::{LaunchComposition, ResolvedLaunchModifier};
        use std::os::unix::fs::{symlink, MetadataExt};

        let id = "0123456789abcdef0123456789abcdef";
        let uid = unsafe { libc::geteuid() };
        let command = vec![
            "/bin/korrid".into(),
            "plugin-launch".into(),
            "/pkg/plugin.ts".into(),
            "{}".into(),
            serde_json::to_string(&LaunchComposition {
                modifiers: vec![ResolvedLaunchModifier {
                    display_change: None,
                    modifier_id: "@test:wrapper/main".into(),
                    manifest: "/pkg/manifest.json".into(),
                    entry: "plugin.ts".into(),
                    settings: Default::default(),
                    config: Default::default(),
                }],
                display: None,
                ..Default::default()
            })
            .unwrap(),
        ];
        // A collected unit and a retained inactive unit both mean Completed.
        // Leave runtime entries in place: korrid must not depend on their removal.
        // SIGKILL retains a failed unit until stop/reset-failed cleans it up.
        for reply in [
            "LoadState=not-found\n",
            "LoadState=loaded\nActiveState=inactive\n",
            "LoadState=loaded\nActiveState=failed\nExecMainCode=2\nExecMainStatus=9\n",
        ] {
            let root = tempfile::tempdir().unwrap();
            let helper = root.path().join("systemctl");
            fs::write(&helper, format!("#!/bin/sh\nprintf '{reply}'\n")).unwrap();
            fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
            let mut backend = SystemdLaunchUnitBackend::new(helper.clone(), helper, uid, unsafe {
                libc::getegid()
            })
            .unwrap()
            .with_compositor_wayland_socket(Some("/run/published-wayland".into()));
            backend.diagnostics_root = root.path().join("launches");
            backend
                .launch(
                    id,
                    &command,
                    &BTreeMap::new(),
                    &LaunchSandbox::new(&command, &BTreeMap::new()),
                )
                .unwrap();
            let arguments = backend
                .launch_arguments(id, &command, &BTreeMap::new())
                .unwrap();
            let runtime = arguments
                .iter()
                .find_map(|arg| arg.strip_prefix("--setenv=XDG_RUNTIME_DIR="))
                .unwrap();
            let bind = arguments
                .iter()
                .find_map(|arg| arg.strip_prefix("--property=BindPaths="))
                .unwrap();
            let diagnostics_bind = bind
                .split_whitespace()
                .find(|entry| entry.ends_with("/diagnostics"))
                .unwrap();
            let (source, destination) = diagnostics_bind.split_once(':').unwrap();
            // Model only the directory bind with a symlink, without mount privileges.
            // Derive both paths from the real unit arguments so the old whole-runtime
            // bind puts dconf in the diagnostic source and fails this regression.
            let runtime = root.path().join(runtime.strip_prefix('/').unwrap());
            let destination = root.path().join(destination.strip_prefix('/').unwrap());
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            symlink(source, &destination).unwrap();
            let private = runtime.join("dconf");
            fs::create_dir(&private).unwrap();
            fs::set_permissions(&private, fs::Permissions::from_mode(0o700)).unwrap();
            fs::write(private.join("user"), "private settings").unwrap();
            assert_eq!(fs::metadata(&private).unwrap().uid(), backend.runtime_uid);
            assert_eq!(fs::metadata(&private).unwrap().mode() & 0o777, 0o700);
            // The unprivileged harness uses its own uid for both processes.
            // An unreadable sibling models korrid's lack of traversal rights
            // to another uid's 0700 directory, without requiring chown privileges.
            let unreadable = runtime.join("private-unreadable");
            fs::create_dir(&unreadable).unwrap();
            fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
            let result = if reply.contains("ActiveState=failed") {
                assert_eq!(
                    backend.state(id).unwrap_err().kind,
                    LaunchUnitErrorKind::UnitFailed
                );
                backend.stop(id).map(|()| LaunchUnitState::Completed)
            } else {
                backend.state(id)
            };
            fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o700)).unwrap();
            assert_eq!(result.unwrap(), LaunchUnitState::Completed);
            assert!(!backend.diagnostics_root.join(id).exists());
            assert!(
                private.exists(),
                "korrid traversed the game runtime directory"
            );
        }
    }

    #[test]
    fn composed_display_recovers_from_exact_live_unit_environment_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemctl");
        let display = crate::launcher::plugin_launch::PluginDisplay {
            width: 640,
            height: 480,
        };
        let assignment = format!("{DISPLAY_ENV}={}", serde_json::to_string(&display).unwrap());
        let environment = format!(
            "Environment={}\n",
            serde_json::to_string(&assignment).unwrap()
        );
        fs::write(root.path().join("reply"), environment).unwrap();
        fs::write(
            &helper,
            format!("#!/bin/sh\ncat '{}'\n", root.path().join("reply").display()),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let backend = SystemdLaunchUnitBackend::new(
            helper.clone(),
            helper,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
        )
        .unwrap();
        assert_eq!(
            backend.display("0123456789abcdef0123456789abcdef").unwrap(),
            Some(display)
        );
    }

    #[test]
    fn placement_recovers_from_exact_live_unit_environment_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemctl");
        let plan = super::super::display_placement::Placement {
            layout: Some("Top/Bottom".into()),
            global: true,
            width: 640,
            height: 960,
            outputs: vec!["DSI-2".into(), "DSI-1".into()],
        };
        let assignment = format!("{PLACEMENT_ENV}={}", serde_json::to_string(&plan).unwrap());
        let environment = format!(
            "Environment={}\n",
            serde_json::to_string(&assignment).unwrap()
        );
        fs::write(root.path().join("reply"), environment).unwrap();
        fs::write(
            &helper,
            format!("#!/bin/sh\ncat '{}'\n", root.path().join("reply").display()),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let backend = SystemdLaunchUnitBackend::new(
            helper.clone(),
            helper,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
        )
        .unwrap();
        assert_eq!(
            backend
                .placement("0123456789abcdef0123456789abcdef")
                .unwrap(),
            Some(plan)
        );
    }

    #[test]
    fn runner_steps_read_the_live_units_status_text_and_allow_child_notifications() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemctl");
        fs::write(&helper, "#!/bin/sh\nprintf 'LoadState=loaded\\nActiveState=active\\nFreezerState=running\\nStatusText=korri-step: game\\n'\n").unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let backend = SystemdLaunchUnitBackend::new(
            helper.clone(),
            helper,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
        )
        .unwrap();
        let (state, status) = backend
            .state_with_status("0123456789abcdef0123456789abcdef")
            .unwrap();
        assert_eq!(state, LaunchUnitState::Running);
        assert_eq!(status.as_deref(), Some("korri-step: game"));
        let args = backend
            .launch_arguments(
                "0123456789abcdef0123456789abcdef",
                &["game".into()],
                &BTreeMap::new(),
            )
            .unwrap();
        assert!(args.iter().any(|arg| arg == "--property=NotifyAccess=all"));
    }

    #[test]
    fn failed_unit_uses_only_its_own_bounded_redacted_stderr_and_cleans_normal_completion() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemctl");
        let reply = root.path().join("reply");
        fs::write(
            &reply,
            "LoadState=loaded\nActiveState=failed\nExecMainCode=1\nExecMainStatus=1\nStatusText=\nEnvironment=KORRI_LAUNCH_DIAGNOSTICS_DIRECTORY=/run/korri-game-0123456789abcdef0123456789abcdef/diagnostics\n",
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
        for status in ["", "korri-step: game"] {
            fs::write(&reply, format!("LoadState=loaded\nActiveState=failed\nExecMainCode=1\nExecMainStatus=1\nStatusText={status}\nEnvironment=KORRI_LAUNCH_DIAGNOSTICS_DIRECTORY=/run/korri-game-{id}/diagnostics\n")).unwrap();
            let error = backend.state(id).unwrap_err();
            assert_eq!(error.kind, LaunchUnitErrorKind::UnitFailed);
            assert!(
                error
                    .message
                    .contains("startup refused [redacted] [redacted]"),
                "status {status:?}: {}",
                error.message
            );
            assert!(!error.message.contains("private-env-value"));
            assert!(!error.message.contains("--no-such-reviewed-flag"));
        }
        fs::write(&reply, format!("LoadState=loaded\nActiveState=failed\nExecMainCode=1\nExecMainStatus=1\nStatusText=plugin-launch: specific compose refusal\nEnvironment=KORRI_LAUNCH_DIAGNOSTICS_DIRECTORY=/run/korri-game-{id}/diagnostics\n")).unwrap();
        let error = backend.state(id).unwrap_err();
        assert!(error
            .message
            .contains("plugin-launch: specific compose refusal"));
        assert!(!error.message.contains("startup refused"));
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
        fs::write(&helper, format!("#!/bin/sh\nprintf 'LoadState=loaded\\nActiveState=failed\\nExecMainCode=1\\nExecMainStatus=1\\nStatusText=\\nEnvironment=KORRI_LAUNCH_DIAGNOSTICS_DIRECTORY=/run/korri-game-{id}/diagnostics\\n'\n")).unwrap();
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

#[cfg(test)]
mod hook_unit_tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    const ID: &str = "0123456789abcdef0123456789abcdef";

    fn backend() -> SystemdLaunchUnitBackend {
        SystemdLaunchUnitBackend::new(
            "/bin/systemd-run".into(),
            "/bin/systemctl".into(),
            1000,
            1000,
        )
        .unwrap()
    }

    #[test]
    fn hook_and_game_share_every_sandbox_property_for_plain_and_modifier_launches() {
        use crate::launcher::plugin_launch::{LaunchComposition, ResolvedLaunchModifier};
        let modifier = vec![
            "korrid".into(),
            "plugin-prepare".into(),
            "/plugin.ts".into(),
            "{}".into(),
            serde_json::to_string(&LaunchComposition {
                modifiers: vec![ResolvedLaunchModifier {
                    display_change: None,
                    modifier_id: "@test:modifier/main".into(),
                    manifest: "/pkg/manifest.json".into(),
                    entry: "plugin.ts".into(),
                    settings: Default::default(),
                    config: Default::default(),
                }],
                display: None,
                ..Default::default()
            })
            .unwrap(),
        ];
        let backend =
            backend().with_compositor_wayland_socket(Some("/run/published-wayland".into()));
        for command in [vec!["game".into()], modifier] {
            let environment = BTreeMap::from([
                (
                    "PULSE_SERVER".into(),
                    "unix:/run/korri-game-audio/native".into(),
                ),
                ("DISPLAY".into(), ":0".into()),
            ]);
            let game = backend
                .launch_arguments(ID, &command, &environment)
                .unwrap();
            let hook = backend
                .hook_arguments(
                    ID,
                    0,
                    &command,
                    "true",
                    Duration::from_secs(30),
                    &environment,
                )
                .unwrap();
            let sandbox = |args: &[String]| {
                args.iter()
                    .filter(|arg| {
                        (arg.starts_with("--property=")
                            || arg.starts_with("--uid=")
                            || arg.starts_with("--gid="))
                            && !arg.starts_with("--property=RuntimeMaxSec=")
                            && !arg.starts_with("--property=TimeoutStopSec=")
                            && !arg.starts_with("--property=UnsetEnvironment=")
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            };
            assert_eq!(sandbox(&game), sandbox(&hook));
            assert!(hook.contains(&"--uid=1000".into()));
            assert!(hook.contains(&"--gid=1000".into()));
            assert!(hook.contains(&format!("--unit=korri-hook-{ID}-0.service")));
            assert!(hook.contains(&"--wait".into()));
            assert!(hook.contains(&"--collect".into()));
            assert!(hook.contains(&"--property=RuntimeMaxSec=30000000us".into()));
            assert_eq!(&hook[hook.len() - 4..], ["--", "/bin/sh", "-c", "true"]);
        }
    }

    #[test]
    fn hook_names_and_timeouts_stay_within_the_polkit_authority() {
        let backend = backend();
        for (id, index, timeout) in [(ID, 1000, 1), ("bad", 0, 1), (ID, 0, 0)] {
            assert!(backend
                .hook_arguments(
                    id,
                    index,
                    &["game".into()],
                    "true",
                    Duration::from_secs(timeout),
                    &BTreeMap::new()
                )
                .is_err());
        }
        let arguments = backend
            .hook_arguments(
                ID,
                999,
                &["game".into()],
                "true",
                Duration::from_secs(2),
                &BTreeMap::from([("KORRI_GAME_ID".into(), "game".into())]),
            )
            .unwrap();
        let unset = arguments
            .iter()
            .find(|arg| arg.starts_with("--property=UnsetEnvironment="))
            .unwrap();
        assert!(unset.contains("KORRI_PROFILE_ID"));
        assert!(unset.contains("KORRI_EXIT_STATUS"));
        assert!(!unset.contains("KORRI_GAME_ID"));
        assert!(arguments.contains(&"--property=RuntimeMaxSec=2000000us".into()));
    }

    #[test]
    fn backend_waits_for_real_shell_completion_and_propagates_failure_without_a_short_helper_deadline(
    ) {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemd-run");
        let ctl = root.path().join("systemctl");
        fs::write(
            &helper,
            "#!/bin/sh\nsleep 1.05\nwhile [ \"$1\" != -- ]; do shift; done\nshift\nexec \"$@\"\n",
        )
        .unwrap();
        fs::write(&ctl, "#!/bin/sh\nprintf 'LoadState=not-found\\n'\n").unwrap();
        for path in [&helper, &ctl] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut backend = SystemdLaunchUnitBackend::new(helper, ctl, 1000, 1000).unwrap();
        backend.helper_timeout = Duration::from_secs(1);
        let output = root.path().join("ran");
        backend
            .run_hook(
                ID,
                0,
                &["game".into()],
                &format!("printf complete > '{}'", output.display()),
                Duration::from_secs(1),
                &BTreeMap::new(),
                &LaunchSandbox::default(),
            )
            .unwrap();
        assert_eq!(fs::read_to_string(output).unwrap(), "complete");
        let error = backend
            .run_hook(
                ID,
                1,
                &["game".into()],
                "exit 7",
                Duration::from_secs(1),
                &BTreeMap::new(),
                &LaunchSandbox::default(),
            )
            .unwrap_err();
        assert_eq!(error.kind, LaunchUnitErrorKind::Failed);
        assert_eq!(error.message, "exited with status 7");
    }
}

#[cfg(test)]
mod captured_exit_tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
    };
    const ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn collected_game_keeps_a_hardened_exit_result_until_host_journals_it_even_after_daemon_restart(
    ) {
        let root = tempfile::tempdir().unwrap();
        let ctl = root.path().join("systemctl");
        fs::write(&ctl, "#!/bin/sh\nprintf 'LoadState=not-found\\n'\n").unwrap();
        fs::set_permissions(&ctl, fs::Permissions::from_mode(0o700)).unwrap();
        let uid = unsafe { libc::geteuid() };
        let mut backend =
            SystemdLaunchUnitBackend::new(ctl.clone(), ctl, uid, unsafe { libc::getegid() })
                .unwrap();
        backend.diagnostics_root = root.path().join("launches");
        let directory = backend.diagnostics_root.join(ID);
        fs::create_dir_all(&directory).unwrap();
        super::super::launch_diagnostics::write_status(&directory, uid, 0).unwrap();
        assert_eq!(backend.state(ID).unwrap(), LaunchUnitState::Completed);
        assert!(
            directory.exists(),
            "state must retain status until it is journaled"
        );
        let restarted = backend.clone();
        assert_eq!(restarted.exit_status(ID).unwrap(), Some(0));
        assert!(
            directory.exists(),
            "reading is not a durable completion acknowledgement"
        );
        restarted.cleanup_completed(ID).unwrap();
        assert!(!directory.exists());
        assert_eq!(
            restarted.exit_status(ID).unwrap(),
            None,
            "an unavailable status must remain absent"
        );
        fs::create_dir_all(&directory).unwrap();
        let outside = root.path().join("private");
        fs::write(&outside, "7").unwrap();
        symlink(&outside, directory.join("exit-status")).unwrap();
        assert_eq!(
            restarted.exit_status(ID).unwrap(),
            None,
            "do not follow a runtime-created link"
        );
    }
}

#[cfg(test)]
mod hook_replay_tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn unfinished_hook_attempt_is_stopped_and_collected_before_its_step_replays() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("running");
        let events = root.path().join("events");
        fs::write(&marker, "old hook").unwrap();
        let ctl = root.path().join("systemctl");
        fs::write(&ctl, format!("#!/bin/sh\ncase \" $* \" in\n*' stop '*) rm '{}'; echo stopped >> '{}' ;;\n*) if [ -f '{}' ]; then printf 'LoadState=loaded\\nActiveState=active\\n'; else printf 'LoadState=not-found\\n'; fi ;;\nesac\n", marker.display(), events.display(), marker.display())).unwrap();
        let run = root.path().join("systemd-run");
        fs::write(
            &run,
            format!("#!/bin/sh\necho replayed >> '{}'\n", events.display()),
        )
        .unwrap();
        for path in [&ctl, &run] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let backend = SystemdLaunchUnitBackend::new(run, ctl, 1000, 1000).unwrap();
        backend
            .run_hook(
                "0123456789abcdef0123456789abcdef",
                0,
                &["game".into()],
                "true",
                Duration::from_secs(30),
                &BTreeMap::new(),
                &LaunchSandbox::default(),
            )
            .unwrap();
        assert_eq!(fs::read_to_string(events).unwrap(), "stopped\nreplayed\n");
    }
}

#[cfg(test)]
mod plain_exit_channel_tests {
    use super::*;
    use crate::host::{
        hooks::{Context, LaunchHooks},
        session_state::{HostSessionControl, HostSessionStatus},
    };
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn plugin_exit_writer_child() {
        if std::env::var_os("KORRID_TEST_PLUGIN_EXIT_WRITER").is_some() {
            let file = std::env::var_os(crate::host::launch_diagnostics::EXIT_STATUS_ENV).unwrap();
            assert!(
                fs::read(&file).unwrap().is_empty(),
                "every unit must receive a fresh exit channel"
            );
            let status = std::env::var("KORRID_TEST_PLUGIN_EXIT_STATUS")
                .unwrap()
                .parse()
                .unwrap();
            crate::host::launch_diagnostics::write_exit_status(status).unwrap();
            let mut log = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(std::env::var_os("KORRID_TEST_PLUGIN_STATUS_LOG").unwrap())
                .unwrap();
            use std::io::Write;
            writeln!(log, "{status}").unwrap();
        }
    }

    #[test]
    fn plain_plugin_exit_survives_collection_and_hook_exit_file_writes() {
        let root = tempfile::tempdir().unwrap();
        let uid = unsafe { libc::geteuid() };
        let gid = unsafe { libc::getegid() };
        let person = "11".repeat(32);
        let account = crate::saves::account_root(root.path(), &person).unwrap();
        let command = vec![
            "/bin/korrid".into(), "plugin-launch".into(), "/pkg/plugin.ts".into(),
            serde_json::json!({"runnerId":"@test:runner/main", "program":"/bin/true", "accountRoot":account,
                "saveRoot":account.join("saves/game"), "cacheRoot":root.path().join("cache"), "files":{}, "artifacts":{}}).to_string(),
            serde_json::to_string(&crate::launcher::plugin_launch::LaunchComposition::default()).unwrap(),
            serde_json::json!({"plan":{"command":"/bin/true", "args":[], "env":{"PATH":"/game/runtime/bin"}, "saveLocations":[]}, "runnerArgv":[], "releaseId":"release"}).to_string(),
        ];
        let baseline = BTreeMap::from([("PATH".into(), "/baseline/bin".into())]);
        let sandbox = LaunchSandbox::new(&command, &baseline);
        assert!(!sandbox.modifiers);
        let marker = root.path().join("active");
        let observed = root.path().join("hook-arguments");
        let writes = root.path().join("status-writes");
        let ctl = root.path().join("systemctl");
        fs::write(&ctl,format!(r#"#!/bin/sh
case " $* " in
*" list-units "*) if [ -f '{}' ]; then printf 'korri-game-%s.service loaded active running\n' "$(cat '{}')"; fi ;;
*"korri-hook-"*) printf 'LoadState=not-found\n' ;;
*) if [ -f '{}' ]; then printf 'LoadState=loaded\nActiveState=active\n'; else printf 'LoadState=not-found\n'; fi ;;
esac
"#,marker.display(),marker.display(),marker.display())).unwrap();
        let run = root.path().join("systemd-run");
        // Simulate only the namespace's file bind. The real plugin-launch exit
        // writer runs as a child process against that exact host-created inode.
        fs::write(&run,format!(r#"#!/bin/sh
set -eu
hook=0
for arg in "$@"; do
  case "$arg" in
    --unit=korri-hook-*) hook=1 ;;
    --unit=korri-game-*) id=${{arg#--unit=korri-game-}}; id=${{id%.service}} ;;
    --property=BindPaths=*) source=${{arg#--property=BindPaths=}}; source=${{source%%:*}} ;;
  esac
done
if [ "$hook" = 1 ]; then
    printf '%s\n' "$@" > '{observed}'
    status=3
else
    printf '%s' "$id" > '{marker}'
    status=0
fi
KORRID_TEST_PLUGIN_EXIT_WRITER=1 KORRID_TEST_PLUGIN_EXIT_STATUS="$status" KORRID_TEST_PLUGIN_STATUS_LOG='{writes}' KORRI_LAUNCH_EXIT_STATUS_FILE="$source" KORRI_LAUNCH_DIAGNOSTICS_READER_UID={uid} '{exe}' --exact host::systemd_unit::plain_exit_channel_tests::plugin_exit_writer_child
"#,observed=observed.display(),marker=marker.display(),writes=writes.display(),uid=uid,exe=std::env::current_exe().unwrap().display())).unwrap();
        for path in [&ctl, &run] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut backend = SystemdLaunchUnitBackend::new(run, ctl, uid, gid).unwrap();
        backend.diagnostics_root = root.path().join("launches");
        let arguments = backend
            .launch_arguments_in_sandbox(
                "0123456789abcdef0123456789abcdef",
                &command,
                &baseline,
                &sandbox,
            )
            .unwrap();
        let binds: Vec<_> = arguments
            .iter()
            .filter_map(|arg| arg.strip_prefix("--property=BindPaths="))
            .collect();
        assert_eq!(binds.len(), 1);
        assert_eq!(
            binds[0].split_whitespace().count(),
            1,
            "plain launches bind only the exit-status file"
        );
        assert!(binds[0].ends_with(":/run/korri-game-0123456789abcdef0123456789abcdef/exit-status"));
        assert!(!arguments.iter().any(|arg| arg.contains("wayland-parent")
            || arg.starts_with("--setenv=WAYLAND_DISPLAY=")
            || arg.starts_with("--property=StandardError=")
            || arg.starts_with(&format!(
                "--setenv={}=",
                super::super::launch_diagnostics::DIRECTORY_ENV
            ))));
        fs::create_dir(root.path().join("private")).unwrap();
        let control =
            HostSessionControl::new(&root.path().join("private"), std::sync::Arc::new(backend));
        let hooks = LaunchHooks {
            resolved: crate::launch_settings::ResolvedHooks {
                before: serde_json::from_value(serde_json::json!([{"run":"forge-before"}]))
                    .unwrap(),
                after: serde_json::from_value(
                    serde_json::json!([{"run":"capture-after"},{"run":"forge-after"}]),
                )
                .unwrap(),
            },
            context: Context::default(),
            environment_changes: crate::launcher::linux_plugin::resolved_plan_environment_changes(
                &command,
            )
            .unwrap(),
        };
        let launched = control
            .prepare_with_limits(
                "game",
                Some(&person),
                Ok(&command),
                &baseline,
                None,
                vec![],
                Some(&hooks),
                None,
            )
            .unwrap();
        fs::remove_file(marker).unwrap(); // systemd collected the cleanly exited unit
        assert_eq!(
            control.status(),
            HostSessionStatus::Completed {
                launch_id: launched.session.launch_id
            }
        );
        let args = fs::read_to_string(observed).unwrap();
        assert_eq!(
            fs::read_to_string(writes).unwrap(),
            "3\n0\n3\n3\n",
            "hooks can write their own channel, never the game's status"
        );
        let game_plan = crate::launcher::plugin_launch::prepared_plan(&command).unwrap();
        assert!(
            args.lines()
                .any(|arg| arg == format!("--setenv=PATH={}", game_plan.env["PATH"])),
            "the hook and final game must receive the same resolved PATH"
        );
        assert!(
            args.lines()
                .any(|arg| arg == "--setenv=KORRI_SESSION_RESULT=exited"),
            "{args}"
        );
        assert!(
            args.lines()
                .any(|arg| arg == "--setenv=KORRI_EXIT_STATUS=0"),
            "{args}"
        );
    }
}

#[cfg(test)]
mod named_hook_failure_tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn refusal_names_the_authored_step_or_index_and_reports_exit_status_without_helper_path() {
        let root = tempfile::tempdir().unwrap();
        let run = root.path().join("systemd-run");
        let ctl = root.path().join("systemctl");
        fs::write(&run, "#!/bin/sh\nexit 3\n").unwrap();
        fs::write(&ctl, "#!/bin/sh\nprintf 'LoadState=not-found\\n'\n").unwrap();
        for path in [&run, &ctl] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let backend = SystemdLaunchUnitBackend::new(run, ctl, 1000, 1000).unwrap();
        for name in [Some("must-fail"), None] {
            let mut step = serde_json::json!({"run":"exit 3"});
            if let Some(name) = name {
                step["name"] = name.into();
            }
            let steps =
                serde_json::from_value::<Vec<crate::config::HookBeforeStep>>(serde_json::json!([
                    step
                ]))
                .unwrap();
            let failure = crate::host::hooks::before(
                &backend,
                "0123456789abcdef0123456789abcdef",
                &["game".into()],
                &BTreeMap::new(),
                &steps,
                &mut vec![],
                &LaunchSandbox::default(),
            )
            .unwrap_err();
            assert_eq!(failure.code, "HookFailed");
            assert_eq!(
                failure.message,
                format!(
                    "HookFailed {{ phase: before, step: {}, reason: exited with status 3 }}",
                    name.unwrap_or("0")
                )
            );
        }
    }
}
