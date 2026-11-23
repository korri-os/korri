//! The engine host as a launch service: its channel, its unit, its commands
//! and its events (ADR 0083, contract "Process model").
//!
//! korrid starts the host as its own system unit, as the game user, outside
//! the game unit: before the game unit for a session, and alone for a job
//! (sign-in, list, install, uninstall). The spike's rules hold:
//! `KillSignal=SIGKILL` (FINDINGS section 10, `cold-1` against `cold-2`), the
//! host ends with `_exit` (the host's own code), the host translates the game
//! unit's PIDs through the cgroup it reads from the session lock, and a free
//! session lock means the session is gone, so the exit sync runs even when
//! the game unit is killed. The path unit and the request file of the spike
//! are gone: korrid starts the unit.
//!
//! The channel holds the two locks, the command FIFO, the host's event file
//! and the save archives it writes. It sits next to the per-launch
//! diagnostics directories, `/run/korrid/launches/<launch id>.steam-engine`
//! for a session and `/run/korrid/launches/steam-engine-job-<id>` for a job.
//! The host unit sees it at `/run/korri-steam-host-<name>/channel`, the game
//! unit at `/run/korri-game-<launch id>/steam-engine` (`KORRI_STEAM_ENGINE_CHANNEL`).
use super::{folder::EngineFolder, protocol};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command as Process,
    time::{Duration, Instant},
};

pub const COMMANDS: &str = "commands";
/// The folder the host writes save archives into (`saves/<appid>-...tar`).
pub const SAVES: &str = "saves";
pub const SESSION_LOCK: &str = "session.lock";
pub const HOST_LOCK: &str = "host.lock";
pub const EVENTS: &str = "events.jsonl";
/// The variable that tells the shim where the channel is in the game unit.
pub const CHANNEL_ENV: &str = "KORRI_STEAM_ENGINE_CHANNEL";

/// What one host run is for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Mode {
    /// Beside one game unit; the session lock decides when the game is gone.
    Session { launch_id: String },
    /// One job outside a session; korrid stops the host when the job ends.
    Job { id: String },
}

impl Mode {
    /// The unit and runtime directory suffix.
    pub fn name(&self) -> String {
        match self {
            Self::Session { launch_id } => launch_id.clone(),
            Self::Job { id } => format!("job-{id}"),
        }
    }

    pub fn unit(&self) -> String {
        format!("korri-steam-host-{}.service", self.name())
    }

    /// The channel directory under the launches root.
    pub fn channel(&self, launches_root: &Path) -> PathBuf {
        match self {
            Self::Session { launch_id } => launches_root.join(format!("{launch_id}.steam-engine")),
            Self::Job { id } => launches_root.join(format!("steam-engine-job-{id}")),
        }
    }

    /// The channel as the host unit sees it.
    pub fn host_channel(&self) -> PathBuf {
        PathBuf::from(format!("/run/korri-steam-host-{}/channel", self.name()))
    }
}

/// A launch or job id is 32 lowercase hex characters, like a launch id.
/// At korrid start: end the Steam engine hosts a previous korrid left, and
/// remove their channels. Each host has a channel under the launches root
/// (korrid makes it before the unit and removes it after). A session host
/// whose game unit still runs (`game_running`) is kept: its game's exit
/// sync still needs it. Returns the hosts it ended.
pub fn clean_leftovers(
    units: &dyn HostUnits,
    launches_root: &Path,
    game_running: impl Fn(&str) -> bool,
) -> Vec<Mode> {
    let Ok(entries) = fs::read_dir(launches_root) else {
        return Vec::new();
    };
    let mut ended = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let mode = if let Some(id) = name.strip_prefix("steam-engine-job-") {
            Mode::Job { id: id.into() }
        } else if let Some(launch_id) = name.strip_suffix(".steam-engine") {
            Mode::Session {
                launch_id: launch_id.into(),
            }
        } else {
            continue;
        };
        let id = match &mode {
            Mode::Job { id } => id,
            Mode::Session { launch_id } => launch_id,
        };
        if !valid_id(id) {
            continue;
        }
        if let Mode::Session { launch_id } = &mode {
            if game_running(launch_id) {
                eprintln!(
                    "korrid steam engine: kept {}: its game still runs",
                    mode.unit()
                );
                continue;
            }
        }
        let channel = Channel::open(&entry.path());
        if units.active(&mode).unwrap_or(false) {
            // Its own end first (the engine writes its stores), then the unit.
            if channel
                .send_within(protocol::Command::Shutdown, Duration::from_secs(2))
                .is_ok()
            {
                let deadline = Instant::now() + Duration::from_secs(10);
                while units.active(&mode).unwrap_or(false) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
        if let Err(error) = units.stop(&mode) {
            eprintln!("korrid steam engine: cannot stop {}: {error}", mode.unit());
            continue;
        }
        if let Err(error) = channel.remove() {
            eprintln!("korrid steam engine: cannot remove {}: {error}", name);
        }
        eprintln!("korrid steam engine: ended leftover {}", mode.unit());
        ended.push(mode);
    }
    ended
}

pub fn valid_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Everything korrid needs to start one host.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostPlan {
    /// The `@korri:steam` package's `korri-steam-host` file.
    pub host_program: PathBuf,
    /// The pinned engine artifact's `steamclient.so` for this platform
    /// (`ENGINE_FILE`).
    pub engine: PathBuf,
    /// korrid's Proton artifact, linked as the engine's compat tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proton: Option<PathBuf>,
    /// The person's account root; the engine folder is under it.
    pub account_root: PathBuf,
    /// The device's Korri title, the name Steam shows for this device: the
    /// QR sign-in's device and the machine of its cloud operations
    /// (korri#595). `None` when the device has no usable title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
}

/// The variable the host reads the device's name from.
pub const DEVICE_NAME_ENV: &str = "KORRI_STEAM_DEVICE_NAME";

/// A title Steam can show as the device's name: one line of at most 64
/// bytes. `%` is left out, as systemd may read it as a specifier.
pub fn device_name(title: &str) -> Option<String> {
    let title = title.trim();
    (!title.is_empty() && title.len() <= 64 && !title.chars().any(|c| c.is_control() || c == '%'))
        .then(|| title.to_owned())
}

/// The channel: made by korrid, writable by the game user.
pub struct Channel {
    pub dir: PathBuf,
}

/// The default ACL of `saves/`: the archive's owner (the host) and the
/// runtime user rwx, korrid (`korrid_uid`) rwx, nobody else. The mask is the
/// group bits of the mode the host creates with, so korrid reads a 0644 or
/// 0640 archive, and nothing more.
fn saves_default_acl(runtime_uid: u32, korrid_uid: u32) -> Vec<u8> {
    let mut users = [runtime_uid, korrid_uid];
    users.sort_unstable();
    let mut entries = vec![(1_u16, 7_u16, u32::MAX)];
    for uid in users {
        if entries.iter().all(|(_, _, id)| *id != uid) {
            entries.push((2, 7, uid));
        }
    }
    entries.extend([(4, 0, u32::MAX), (16, 7, u32::MAX), (32, 0, u32::MAX)]);
    let mut bytes = 2_u32.to_le_bytes().to_vec();
    for (tag, permission, id) in entries {
        bytes.extend(tag.to_le_bytes());
        bytes.extend(permission.to_le_bytes());
        bytes.extend(id.to_le_bytes());
    }
    bytes
}

fn set_acl(file: &File, name: &std::ffi::CStr, acl: &[u8]) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let result = unsafe {
        libc::fsetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            acl.as_ptr().cast(),
            acl.len(),
            0,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

impl Channel {
    /// Make the channel for `runtime_uid`. The directory, the locks and the
    /// FIFO are korrid's, with a named-user ACL for the game user only, as
    /// `launch_diagnostics` does for game stderr. The event file is korrid's,
    /// readable by the game user: systemd opens it for the host's stdout and
    /// the shim reads it. The host writes save archives into `saves/`.
    pub fn create(dir: &Path, runtime_uid: u32) -> io::Result<Self> {
        if let Some(parent) = dir.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        let grant = runtime_uid != unsafe { libc::geteuid() };
        if grant {
            crate::host::launch_diagnostics::grant_user(&File::open(dir)?, 7, runtime_uid, 7)?;
        }
        for lock in [SESSION_LOCK, HOST_LOCK] {
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(dir.join(lock))?;
            if grant {
                crate::host::launch_diagnostics::grant_user(&file, 6, runtime_uid, 6)?;
            }
        }
        let fifo = std::ffi::CString::new(dir.join(COMMANDS).as_os_str().as_encoded_bytes())
            .map_err(io::Error::other)?;
        if unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if grant {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
                .open(dir.join(COMMANDS))?;
            crate::host::launch_diagnostics::grant_user(&file, 6, runtime_uid, 6)?;
        }
        let events = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join(EVENTS))?;
        if grant {
            // The shim in the game unit reads the host's answers.
            crate::host::launch_diagnostics::grant_user(&events, 6, runtime_uid, 4)?;
        }
        // Save archives: korrid's folder, so korrid can always remove what
        // the host wrote there. The host gets rwx; the default ACL gives
        // korrid read on every archive the host makes, whatever its umask.
        let saves = dir.join(SAVES);
        fs::create_dir(&saves)?;
        fs::set_permissions(&saves, fs::Permissions::from_mode(0o700))?;
        if grant {
            let folder = File::open(&saves)?;
            crate::host::launch_diagnostics::grant_user(&folder, 7, runtime_uid, 7)?;
            set_acl(
                &folder,
                c"system.posix_acl_default",
                &saves_default_acl(runtime_uid, unsafe { libc::geteuid() }),
            )?;
        }
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    pub fn open(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    /// Write one command. `WouldBlock` until the host has the FIFO open for
    /// reading; the caller retries within its own bound.
    pub fn send(&self, command: protocol::Command) -> io::Result<()> {
        let mut fifo = match OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(self.dir.join(COMMANDS))
        {
            Ok(fifo) => fifo,
            // No reader yet (ENXIO).
            Err(error) if error.raw_os_error() == Some(libc::ENXIO) => {
                return Err(io::ErrorKind::WouldBlock.into())
            }
            Err(error) => return Err(error),
        };
        // A line is far below PIPE_BUF, so the write is atomic.
        fifo.write_all(command.line().as_bytes())
    }

    /// Send, retrying while the host has not opened the FIFO yet.
    pub fn send_within(&self, command: protocol::Command, bound: Duration) -> io::Result<()> {
        let deadline = Instant::now() + bound;
        loop {
            match self.send(command) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "the Steam engine host did not take the command",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                other => return other,
            }
        }
    }

    pub fn remove(self) -> io::Result<()> {
        match fs::remove_dir_all(&self.dir) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

/// Reads the host's events as they are appended.
pub struct EventReader {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
}

/// One host stdout line is a bounded JSON event; a longer line is dropped.
const MAX_LINE: usize = 64 * 1024;

impl EventReader {
    pub fn new(channel: &Path) -> Self {
        Self {
            path: channel.join(EVENTS),
            offset: 0,
            partial: Vec::new(),
        }
    }

    /// Every complete line written since the last call, parsed.
    pub fn read(&mut self) -> io::Result<Vec<protocol::Event>> {
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        file.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = Vec::new();
        file.take(1 << 20).read_to_end(&mut bytes)?;
        self.offset += bytes.len() as u64;
        self.partial.extend_from_slice(&bytes);
        let mut events = Vec::new();
        while let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=end).collect();
            if let Some(event) = std::str::from_utf8(&line).ok().and_then(protocol::parse) {
                events.push(event);
            }
        }
        if self.partial.len() > MAX_LINE {
            self.partial.clear();
        }
        Ok(events)
    }
}

/// The game user's ids, for the host unit.
#[derive(Clone, Debug)]
pub struct Identity {
    pub uid: u32,
    pub gid: u32,
}

/// The host command inside the unit. The plugin's `korri-steam-host` is a
/// start wrapper: it makes the person's engine folder (links, the engine
/// file, the compat tool and the ACLs korrid's discovery needs), locks it
/// (one engine per folder) and runs the host in the Steam FHS.
pub fn command(plan: &HostPlan, mode: &Mode) -> Vec<String> {
    let channel = mode.host_channel();
    let mut command = vec![
        plan.host_program.display().to_string(),
        "--commands".into(),
        channel.join(COMMANDS).display().to_string(),
        "--host-lock".into(),
        channel.join(HOST_LOCK).display().to_string(),
    ];
    if matches!(mode, Mode::Session { .. }) {
        command.extend([
            "--session-lock".into(),
            channel.join(SESSION_LOCK).display().to_string(),
        ]);
    }
    command.push(plan.engine.display().to_string());
    command
}

/// The host's environment: HOME in the person's engine folder, and korrid's
/// Proton, without which Windows games fail to install ("Invalid platform").
pub fn environment(plan: &HostPlan) -> Vec<(String, String)> {
    let mut environment = vec![(
        "HOME".to_owned(),
        EngineFolder::for_account(&plan.account_root)
            .home()
            .display()
            .to_string(),
    )];
    if let Some(proton) = &plan.proton {
        environment.push(("KORRI_PROTON_CACHYOS".into(), proton.display().to_string()));
    }
    if let Some(name) = plan.device_name.as_deref().and_then(device_name) {
        environment.push((DEVICE_NAME_ENV.into(), name));
    }
    environment
}

/// systemd-run's arguments for the host unit, then the command.
pub fn unit_arguments(
    plan: &HostPlan,
    mode: &Mode,
    identity: &Identity,
    launches_root: &Path,
    hidden: &[PathBuf],
) -> Result<Vec<String>, String> {
    let channel = mode.channel(launches_root);
    for path in [
        &channel,
        &plan.account_root,
        &plan.host_program,
        &plan.engine,
    ]
    .into_iter()
    .chain(plan.proton.iter())
    {
        let text = path.to_str().ok_or("Steam engine path is not UTF-8")?;
        if !path.is_absolute() || text.contains(['%', ' ', '\n', ':']) {
            return Err("unsafe Steam engine path".into());
        }
    }
    let name = mode.name();
    let mut arguments = vec![
        "--system".into(),
        "--no-ask-password".into(),
        "--quiet".into(),
        "--collect".into(),
        "--service-type=exec".into(),
        format!("--unit={}", mode.unit()),
        "--slice=korri-games.slice".into(),
        format!("--uid={}", identity.uid),
        format!("--gid={}", identity.gid),
        // FINDINGS section 10: Wine children of the engine ignore SIGTERM and
        // held the unit `deactivating` for up to 90 s. korrid stops the host
        // only after the exit sync, so a hard kill loses nothing.
        "--property=KillSignal=SIGKILL".into(),
        "--property=KillMode=control-group".into(),
        "--property=TimeoutStopSec=10s".into(),
        "--property=UMask=0077".into(),
        "--property=NoNewPrivileges=yes".into(),
        "--property=CapabilityBoundingSet=".into(),
        "--property=AmbientCapabilities=".into(),
        "--property=PrivateTmp=yes".into(),
        "--property=ProtectKernelModules=yes".into(),
        "--property=ProtectControlGroups=yes".into(),
        "--property=RestrictSUIDSGID=yes".into(),
        format!("--property=RuntimeDirectory=korri-steam-host-{name}"),
        "--property=RuntimeDirectoryMode=0700".into(),
        format!(
            "--property=BindPaths={}:{}",
            channel.display(),
            mode.host_channel().display()
        ),
        // systemd opens stdout before the mount namespace: the host source.
        format!(
            "--property=StandardOutput=append:{}",
            channel.join(EVENTS).display()
        ),
        "--property=StandardError=journal".into(),
    ];
    if !hidden.is_empty() {
        arguments.push(format!(
            "--property=InaccessiblePaths={}",
            hidden
                .iter()
                .map(|path| format!("-{}", path.display()))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    for (name, value) in environment(plan) {
        arguments.push(format!("--setenv={name}={value}"));
    }
    arguments.push("--".into());
    arguments.extend(command(plan, mode));
    Ok(arguments)
}

/// Starts and stops host units. Production uses systemd; tests run the same
/// command as a child process with the same channel.
pub trait HostUnits: Send + Sync {
    fn start(&self, plan: &HostPlan, mode: &Mode) -> io::Result<()>;
    /// Stop now: the unit's KillSignal is SIGKILL.
    fn stop(&self, mode: &Mode) -> io::Result<()>;
    fn active(&self, mode: &Mode) -> io::Result<bool>;
}

pub struct SystemdHostUnits {
    pub systemd_run: PathBuf,
    pub systemctl: PathBuf,
    pub identity: Identity,
    pub launches_root: PathBuf,
    pub hidden: Vec<PathBuf>,
}

impl HostUnits for SystemdHostUnits {
    fn start(&self, plan: &HostPlan, mode: &Mode) -> io::Result<()> {
        let arguments = unit_arguments(
            plan,
            mode,
            &self.identity,
            &self.launches_root,
            &self.hidden,
        )
        .map_err(io::Error::other)?;
        let output = Process::new(&self.systemd_run)
            .args(arguments)
            .output()
            .map_err(|error| {
                io::Error::other(format!(
                    "cannot run {}: {error}",
                    self.systemd_run.display()
                ))
            })?;
        if output.status.success() {
            Ok(())
        } else {
            // systemd-run's own words (a polkit refusal, a bad property) say
            // why; they name units and paths, never a token.
            let said = String::from_utf8_lossy(&output.stderr);
            Err(io::Error::other(format!(
                "systemd-run failed with {}: {}",
                output.status,
                said.trim().chars().take(400).collect::<String>()
            )))
        }
    }

    fn stop(&self, mode: &Mode) -> io::Result<()> {
        let output = Process::new(&self.systemctl)
            .args(["--system", "--no-ask-password", "stop"])
            .arg(mode.unit())
            .output()?;
        if output.status.success() || !self.active(mode)? {
            Ok(())
        } else {
            Err(io::Error::other("the Steam engine host did not stop"))
        }
    }

    fn active(&self, mode: &Mode) -> io::Result<bool> {
        let output = Process::new(&self.systemctl)
            .args(["--system", "--no-ask-password", "show"])
            .arg(mode.unit())
            .args(["--property=LoadState", "--property=ActiveState"])
            .output()?;
        let text = String::from_utf8_lossy(&output.stdout);
        if text.lines().any(|line| line == "LoadState=not-found") {
            return Ok(false);
        }
        Ok(text.lines().any(|line| {
            matches!(
                line,
                "ActiveState=active" | "ActiveState=activating" | "ActiveState=deactivating"
            )
        }))
    }
}

#[cfg(test)]
pub mod process_units {
    //! The same host command as a child of the test, stdout appended to the
    //! channel's event file; stop is SIGKILL, as the unit's KillSignal.
    use super::*;
    use std::{collections::BTreeMap, process::Child, sync::Mutex};

    pub struct ProcessHostUnits {
        pub launches_root: PathBuf,
        /// Replaces `korrid steam-engine-host ... --`: the test host runs
        /// directly, with the channel at its real path.
        pub children: Mutex<BTreeMap<String, Child>>,
    }

    impl ProcessHostUnits {
        pub fn new(launches_root: &Path) -> Self {
            Self {
                launches_root: launches_root.to_path_buf(),
                children: Mutex::new(BTreeMap::new()),
            }
        }
    }

    impl HostUnits for ProcessHostUnits {
        fn start(&self, plan: &HostPlan, mode: &Mode) -> io::Result<()> {
            let channel = mode.channel(&self.launches_root);
            let events = OpenOptions::new().append(true).open(channel.join(EVENTS))?;
            let mut command = Process::new(&plan.host_program);
            command
                .arg("--commands")
                .arg(channel.join(COMMANDS))
                .arg("--host-lock")
                .arg(channel.join(HOST_LOCK));
            if matches!(mode, Mode::Session { .. }) {
                command
                    .arg("--session-lock")
                    .arg(channel.join(SESSION_LOCK));
            }
            let child = command
                .arg(&plan.engine)
                .envs(environment(plan))
                .stdin(std::process::Stdio::null())
                .stdout(events)
                .spawn()?;
            self.children.lock().unwrap().insert(mode.unit(), child);
            Ok(())
        }

        fn stop(&self, mode: &Mode) -> io::Result<()> {
            if let Some(mut child) = self.children.lock().unwrap().remove(&mode.unit()) {
                let _ = child.kill();
                child.wait()?;
            }
            Ok(())
        }

        fn active(&self, mode: &Mode) -> io::Result<bool> {
            let mut children = self.children.lock().unwrap();
            Ok(match children.get_mut(&mode.unit()) {
                Some(child) => child.try_wait()?.is_none(),
                None => false,
            })
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn plan() -> HostPlan {
        HostPlan {
            host_program: "/nix/store/x-steam/bin/korri-steam-host".into(),
            engine:
                "/var/lib/korri/cache/steam/engine-package/1792618285/linuxarm64/steamclient.so"
                    .into(),
            proton: Some("/var/lib/korri/cache/proton/cachyos/11.0".into()),
            account_root: "/var/lib/korri/accounts/p1".into(),
            device_name: None,
        }
    }

    fn identity() -> Identity {
        Identity {
            uid: 1000,
            gid: 100,
        }
    }

    #[test]
    fn a_session_host_unit_runs_as_the_game_user_outside_the_game_unit_with_sigkill() {
        let id = "0123456789abcdef0123456789abcdef";
        let mode = Mode::Session {
            launch_id: id.into(),
        };
        let arguments = unit_arguments(
            &plan(),
            &mode,
            &identity(),
            Path::new("/run/korrid/launches"),
            &["/var/lib/korrid".into(), "/run/korrid".into()],
        )
        .unwrap();
        for expected in [
            format!("--unit=korri-steam-host-{id}.service"),
            "--uid=1000".into(),
            "--gid=100".into(),
            "--property=KillSignal=SIGKILL".into(),
            format!("--property=BindPaths=/run/korrid/launches/{id}.steam-engine:/run/korri-steam-host-{id}/channel"),
            format!("--property=StandardOutput=append:/run/korrid/launches/{id}.steam-engine/events.jsonl"),
            "--property=InaccessiblePaths=-/var/lib/korrid -/run/korrid".into(),
        ] {
            assert!(arguments.contains(&expected), "{expected} not in {arguments:?}");
        }
        // Not in the game unit's slice member list: its own unit name.
        assert!(!arguments.iter().any(|a| a.contains("korri-game-")));
        assert!(arguments
            .contains(&"--setenv=HOME=/var/lib/korri/accounts/p1/steam-engine/home".to_string()));
        assert!(arguments.contains(
            &"--setenv=KORRI_PROTON_CACHYOS=/var/lib/korri/cache/proton/cachyos/11.0".to_string()
        ));
        let command = &arguments[arguments.iter().position(|a| a == "--").unwrap() + 1..];
        assert_eq!(
            command,
            [
                "/nix/store/x-steam/bin/korri-steam-host",
                "--commands",
                &format!("/run/korri-steam-host-{id}/channel/commands"),
                "--host-lock",
                &format!("/run/korri-steam-host-{id}/channel/host.lock"),
                "--session-lock",
                &format!("/run/korri-steam-host-{id}/channel/session.lock"),
                "/var/lib/korri/cache/steam/engine-package/1792618285/linuxarm64/steamclient.so",
            ]
        );
    }

    /// korri#595: the host names the device to Steam with its Korri title.
    #[test]
    fn the_host_gets_the_device_title_as_its_steam_device_name() {
        let mode = Mode::Job {
            id: "fedcba9876543210fedcba9876543210".into(),
        };
        let arguments = |name: Option<&str>| {
            let mut plan = plan();
            plan.device_name = name.map(Into::into);
            unit_arguments(
                &plan,
                &mode,
                &identity(),
                Path::new("/run/korrid/launches"),
                &[],
            )
            .unwrap()
        };
        assert!(arguments(Some("OnePlus 6 Korri"))
            .contains(&"--setenv=KORRI_STEAM_DEVICE_NAME=OnePlus 6 Korri".to_string()));
        for none in [None, Some(""), Some("two\nlines"), Some("100%")] {
            assert!(
                !arguments(none)
                    .iter()
                    .any(|a| a.starts_with("--setenv=KORRI_STEAM_DEVICE_NAME")),
                "{none:?}"
            );
        }
        assert_eq!(device_name(" Living room ").as_deref(), Some("Living room"));
        assert_eq!(device_name(&"x".repeat(65)), None);
    }

    #[test]
    fn a_job_host_has_no_session_lock_and_its_own_channel() {
        let mode = Mode::Job {
            id: "fedcba9876543210fedcba9876543210".into(),
        };
        assert_eq!(
            mode.channel(Path::new("/run/korrid/launches")),
            Path::new("/run/korrid/launches/steam-engine-job-fedcba9876543210fedcba9876543210")
        );
        let command = command(&plan(), &mode);
        assert!(!command.iter().any(|a| a == "--session-lock"));
        let mut unsafe_plan = plan();
        unsafe_plan.account_root = "/var/lib/korri/accounts/a b".into();
        assert!(unit_arguments(
            &unsafe_plan,
            &mode,
            &identity(),
            Path::new("/run/korrid/launches"),
            &[]
        )
        .is_err());
        assert!(valid_id("fedcba9876543210fedcba9876543210"));
        // korrid's polkit rule (nixos-module.nix) grants exactly these names.
        let granted =
            regex::Regex::new(r"^korri-steam-host-(job-)?[0-9a-f]{32}\.service$").unwrap();
        assert!(granted.is_match(&mode.unit()), "{}", mode.unit());
        assert!(granted.is_match(
            &Mode::Session {
                launch_id: "0123456789abcdef0123456789abcdef".into()
            }
            .unit()
        ));
        let rule = include_str!("../../nixos-module.nix");
        assert!(rule.contains(r"/^korri-steam-host-(job-)?[0-9a-f]{32}\.service$/"));
        assert!(!valid_id("../x") && !valid_id("FEDCBA9876543210FEDCBA9876543210"));
    }

    /// A real host process: a shell script that reads the FIFO and answers
    /// each line with an event, as the spike host does.
    pub(crate) fn script_host(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("korri-steam-host");
        fs::write(
            &path,
            format!(
                "#!/bin/sh\nwhile [ \"$1\" != --commands ]; do shift; done\nfifo=\"$2\"\n{body}\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn the_host_starts_takes_commands_reports_events_and_stops() {
        let temp = tempfile::tempdir().unwrap();
        let launches = temp.path().join("launches");
        let id = "0123456789abcdef0123456789abcdef";
        let mode = Mode::Session {
            launch_id: id.into(),
        };
        let channel =
            Channel::create(&mode.channel(&launches), unsafe { libc::geteuid() }).unwrap();
        for file in [COMMANDS, SESSION_LOCK, HOST_LOCK, EVENTS] {
            assert!(channel.dir.join(file).exists(), "{file}");
        }
        let host = script_host(
            temp.path(),
            r#"echo '{"event":"phase","phase":"dlopen"}'
exec 3<>"$fifo"
while read -r line <&3; do
  case "$line" in
    "exited "*) echo "{\"event\":\"sync_finished\",\"appid\":${line#exited },\"kind\":\"exit\",\"result\":\"synchronized\"}" ;;
    shutdown) echo '{"event":"phase","phase":"stopped"}'; exit 0 ;;
  esac
done"#,
        );
        let units = process_units::ProcessHostUnits::new(&launches);
        let mut plan = plan();
        plan.host_program = host;
        units.start(&plan, &mode).unwrap();
        assert!(units.active(&mode).unwrap());
        let mut reader = EventReader::new(&channel.dir);
        channel
            .send_within(protocol::Command::Exited(584400), Duration::from_secs(10))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = Vec::new();
        while !seen
            .iter()
            .any(|e| matches!(e, protocol::Event::SyncFinished { .. }))
        {
            assert!(Instant::now() < deadline, "no exit sync: {seen:?}");
            seen.extend(reader.read().unwrap());
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            seen,
            [
                protocol::Event::Phase {
                    phase: "dlopen".into()
                },
                protocol::Event::SyncFinished {
                    appid: 584400,
                    kind: "exit".into(),
                    result: "synchronized".into()
                }
            ]
        );
        units.stop(&mode).unwrap();
        assert!(!units.active(&mode).unwrap());
        // A command with no host reading is refused within its bound.
        assert_eq!(
            channel
                .send_within(protocol::Command::Shutdown, Duration::from_millis(100))
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        channel.remove().unwrap();
        assert!(!mode.channel(&launches).exists());
    }

    /// Review C6: at start korrid ends the hosts and channels a previous
    /// korrid left, but keeps a session host whose game still runs.
    #[test]
    fn leftover_hosts_and_channels_are_cleaned_but_a_running_games_host_stays() {
        let temp = tempfile::tempdir().unwrap();
        let launches = temp.path().join("launches");
        let units = process_units::ProcessHostUnits::new(&launches);
        let me = unsafe { libc::geteuid() };
        let job = Mode::Job {
            id: "0123456789abcdef0123456789abcdef".into(),
        };
        let gone = Mode::Session {
            launch_id: "11111111111111111111111111111111".into(),
        };
        let playing = Mode::Session {
            launch_id: "22222222222222222222222222222222".into(),
        };
        let host = script_host(
            temp.path(),
            r#"exec 3<>"$fifo"
while read -r line <&3; do case "$line" in shutdown) exit 0 ;; esac; done"#,
        );
        let plan = HostPlan {
            host_program: host,
            engine: temp.path().join("steamclient.so"),
            proton: None,
            account_root: temp.path().into(),
            device_name: None,
        };
        for mode in [&job, &gone, &playing] {
            Channel::create(&mode.channel(&launches), me).unwrap();
            units.start(&plan, mode).unwrap();
        }
        // Not a host's channel: left alone.
        fs::create_dir_all(launches.join("0123456789abcdef0123456789abcdef")).unwrap();
        let ended = clean_leftovers(&units, &launches, |launch| {
            launch == "22222222222222222222222222222222"
        });
        assert_eq!(ended.len(), 2);
        assert!(ended.contains(&job) && ended.contains(&gone));
        for mode in [&job, &gone] {
            assert!(!units.active(mode).unwrap());
            assert!(!mode.channel(&launches).exists());
        }
        assert!(units.active(&playing).unwrap());
        assert!(playing.channel(&launches).exists());
        assert!(launches.join("0123456789abcdef0123456789abcdef").exists());
        units.stop(&playing).unwrap();
    }

    fn xattr(path: &Path, name: &std::ffi::CStr) -> Vec<u8> {
        let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let mut buffer = vec![0_u8; 256];
        let size = unsafe {
            libc::getxattr(
                path.as_ptr(),
                name.as_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        assert!(size >= 0, "{}", io::Error::last_os_error());
        buffer.truncate(size as usize);
        buffer
    }

    /// Review B1: korrid makes `saves/` itself, so it owns what it removes;
    /// the host (the runtime user) gets rwx, and the default ACL gives
    /// korrid read on the host's archives. Only a device run with two real
    /// uids proves the kernel side; this checks the ACLs korrid sets.
    #[test]
    fn the_saves_folder_is_korrids_with_acls_for_the_host_and_korrid() {
        let temp = tempfile::tempdir().unwrap();
        let me = unsafe { libc::geteuid() };
        let runtime = me + 4242;
        let channel = Channel::create(&temp.path().join("c"), runtime).unwrap();
        let saves = channel.dir.join(SAVES);
        let metadata = fs::metadata(&saves).unwrap();
        assert!(metadata.is_dir());
        assert_eq!(std::os::unix::fs::MetadataExt::uid(&metadata), me);
        assert_eq!(
            xattr(&saves, c"system.posix_acl_access"),
            crate::host::launch_diagnostics::private_acl(7, runtime, 7)
        );
        let default = xattr(&saves, c"system.posix_acl_default");
        assert_eq!(default, saves_default_acl(runtime, me));
        // user_obj, the two named users (sorted), group_obj, mask, other.
        let entries: Vec<(u16, u16, u32)> = default[4..]
            .chunks(8)
            .map(|e| {
                (
                    u16::from_le_bytes([e[0], e[1]]),
                    u16::from_le_bytes([e[2], e[3]]),
                    u32::from_le_bytes([e[4], e[5], e[6], e[7]]),
                )
            })
            .collect();
        assert_eq!(
            entries,
            [
                (1, 7, u32::MAX),
                (2, 7, me),
                (2, 7, runtime),
                (4, 0, u32::MAX),
                (16, 7, u32::MAX),
                (32, 0, u32::MAX)
            ]
        );
        // An archive made there carries the named entries; korrid removes it.
        fs::write(saves.join("70-exit-2.tar"), b"tar").unwrap();
        channel.remove().unwrap();
        assert!(!temp.path().join("c").exists());
    }

    #[test]
    fn the_event_reader_keeps_a_partial_line_for_the_next_read() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(EVENTS);
        fs::write(&path, b"{\"event\":\"apps_done\"}\n{\"event\":\"signed_").unwrap();
        let mut reader = EventReader::new(temp.path());
        assert_eq!(reader.read().unwrap(), [protocol::Event::AppsDone {}]);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"out\"}\nnot json\n").unwrap();
        assert_eq!(
            reader.read().unwrap(),
            [protocol::Event::SignedOut { reason: None }]
        );
        assert!(reader.read().unwrap().is_empty());
    }
}
