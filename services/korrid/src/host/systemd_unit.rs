use std::{
    collections::BTreeMap,
    io::{self, Read},
    os::fd::AsRawFd,
    path::{Component, Path, PathBuf},
    process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

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
const DEFAULT_CERTIFICATE_CONTROL_DIRECTORY: &str = "/run/korri-certificate-control";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchUnitState {
    Running,
    Stopping,
    Completed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchUnitErrorKind {
    InvalidConfiguration,
    Spawn,
    Timeout,
    Failed,
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

pub trait LaunchUnitBackend: Send + Sync {
    fn launch(
        &self,
        launch_id: &str,
        command: &[String],
        environment: &BTreeMap<String, String>,
    ) -> Result<(), LaunchUnitError>;
    fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError>;
    fn stop(&self, launch_id: &str) -> Result<(), LaunchUnitError>;
    fn live_launch_ids(&self) -> Result<Vec<String>, LaunchUnitError>;
}

#[derive(Clone, Debug)]
struct ProtectedPaths {
    private_state_root: PathBuf,
    control_socket: PathBuf,
    control_directory: PathBuf,
    sunshine_private_state_root: PathBuf,
    compositor_control_directory: PathBuf,
    certificate_control_directory: PathBuf,
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
    sunshine_private_state_root: PathBuf,
    compositor_control_directory: PathBuf,
    certificate_control_directory: PathBuf,
    helper_timeout: Duration,
}

impl Default for SystemdLaunchUnitBackend {
    fn default() -> Self {
        Self::for_current_process()
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct InMemoryLaunchUnitBackend {
    units: Mutex<BTreeMap<String, LaunchUnitState>>,
}

#[cfg(test)]
impl LaunchUnitBackend for InMemoryLaunchUnitBackend {
    fn launch(
        &self,
        launch_id: &str,
        _command: &[String],
        _environment: &BTreeMap<String, String>,
    ) -> Result<(), LaunchUnitError> {
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
        sunshine_private_state_root: PathBuf,
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
                sunshine_private_state_root,
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
                sunshine_private_state_root: PathBuf::from("/home/korri/.config/sunshine"),
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
            sunshine_private_state_root,
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
            ("Sunshine private state root", &sunshine_private_state_root),
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
            sunshine_private_state_root,
            compositor_control_directory,
            certificate_control_directory,
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
        let sunshine_private_state_root = std::env::var_os("KORRID_SUNSHINE_PRIVATE_STATE_ROOT")
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
                sunshine_private_state_root: sunshine_private_state_root.clone(),
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
            sunshine_private_state_root,
            compositor_control_directory,
            certificate_control_directory,
            helper_timeout: DEFAULT_HELPER_TIMEOUT,
        })
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
                sunshine_private_state_root: self.sunshine_private_state_root.clone(),
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
        let mut arguments = vec![
            "--system".into(),
            "--no-ask-password".into(),
            "--quiet".into(),
            "--collect".into(),
            "--service-type=exec".into(),
            format!("--unit={unit}"),
            format!("--uid={}", self.runtime_uid),
            format!("--gid={}", self.runtime_gid),
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
            "--property=ProtectProc=invisible".into(),
            "--property=ProcSubset=pid".into(),
            format!(
                "--property=InaccessiblePaths={} /run/korrid {} {} {} {} {} /run/user/{} -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources",
                self.private_state_root.display(),
                self.control_socket.display(),
                self.control_directory.display(),
                self.sunshine_private_state_root.display(),
                self.compositor_control_directory.display(),
                self.certificate_control_directory.display(),
                self.runtime_uid
            ),
            "--property=RestrictSUIDSGID=yes".into(),
        ];
        arguments.extend(
            environment
                .iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "WAYLAND_DISPLAY" | "SWAYSOCK" | "XDG_RUNTIME_DIR"
                    )
                })
                .map(|(key, value)| format!("--setenv={key}={value}")),
        );
        arguments.push("--".into());
        arguments.extend(configured_command.iter().cloned());
        Ok(arguments)
    }

    pub(super) fn stop_arguments(launch_id: &str) -> Result<Vec<String>, LaunchUnitError> {
        Ok(vec![
            "--system".into(),
            "--no-ask-password".into(),
            "stop".into(),
            Self::unit_name(launch_id)?,
        ])
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
    fn launch(
        &self,
        launch_id: &str,
        configured_command: &[String],
        environment: &BTreeMap<String, String>,
    ) -> Result<(), LaunchUnitError> {
        let arguments = self.launch_arguments(launch_id, configured_command, environment)?;
        self.require_success(&self.systemd_run, &arguments)
            .map(|_| ())
    }

    fn state(&self, launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError> {
        let unit = Self::unit_name(launch_id)?;
        let output = self.run(
            &self.systemctl,
            &[
                "--system".into(),
                "--no-ask-password".into(),
                "show".into(),
                unit,
                "--property=LoadState".into(),
                "--property=ActiveState".into(),
            ],
        )?;
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
        match values.get("ActiveState").copied() {
            Some("activating" | "active" | "reloading") => Ok(LaunchUnitState::Running),
            Some("deactivating") => Ok(LaunchUnitState::Stopping),
            Some("inactive" | "failed" | "dead") => Ok(LaunchUnitState::Completed),
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

    fn stop(&self, launch_id: &str) -> Result<(), LaunchUnitError> {
        let arguments = Self::stop_arguments(launch_id)?;
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
