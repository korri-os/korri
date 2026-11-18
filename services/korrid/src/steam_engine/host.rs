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
    /// The pinned engine artifact's `linuxarm64/steamclient.so`.
    pub engine: PathBuf,
    /// korrid's Proton artifact, linked as the engine's compat tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proton: Option<PathBuf>,
    /// The person's account root; the engine folder is under it.
    pub account_root: PathBuf,
}

/// The channel: made by korrid, writable by the game user.
pub struct Channel {
    pub dir: PathBuf,
}

impl Channel {
    /// Make the channel for `runtime_uid`. The directory, the locks and the
    /// FIFO are korrid's, with a named-user ACL for the game user only, as
    /// `launch_diagnostics` does for game stderr. The event file stays
    /// korrid's alone: systemd opens it for the host's stdout.
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
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join(EVENTS))?;
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

/// The game user's ids and korrid's own program, for the host unit.
#[derive(Clone, Debug)]
pub struct Identity {
    pub uid: u32,
    pub gid: u32,
    /// korrid's binary: `korrid steam-engine-host` prepares the folder as the
    /// game user and then runs the host.
    pub korrid: PathBuf,
}

/// The host command inside the unit: korrid prepares the person's folder,
/// then becomes the host with HOME set to the folder's `home`.
pub fn command(plan: &HostPlan, mode: &Mode, identity: &Identity) -> Vec<String> {
    let folder = EngineFolder::for_account(&plan.account_root);
    let channel = mode.host_channel();
    let mut command = vec![
        identity.korrid.display().to_string(),
        "steam-engine-host".into(),
        "--folder".into(),
        folder.root().display().to_string(),
        "--engine".into(),
        plan.engine.display().to_string(),
    ];
    if let Some(proton) = &plan.proton {
        command.extend(["--proton".into(), proton.display().to_string()]);
    }
    command.extend([
        "--".into(),
        plan.host_program.display().to_string(),
        "--commands".into(),
        channel.join(COMMANDS).display().to_string(),
        "--host-lock".into(),
        channel.join(HOST_LOCK).display().to_string(),
    ]);
    if matches!(mode, Mode::Session { .. }) {
        command.extend([
            "--session-lock".into(),
            channel.join(SESSION_LOCK).display().to_string(),
        ]);
    }
    command.push(folder.engine().display().to_string());
    command
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
    arguments.push("--".into());
    arguments.extend(command(plan, mode, identity));
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
        let output = Process::new(&self.systemd_run).args(arguments).output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "systemd-run for the Steam engine host failed with {}",
                output.status
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

/// Runs `korrid steam-engine-host` inside the unit: make the folder, then
/// become the host. Arguments: `--folder DIR --engine FILE [--proton DIR] --
/// HOST ARGS...`.
pub fn run_inside(arguments: &[std::ffi::OsString]) -> Result<std::convert::Infallible, String> {
    use std::os::unix::process::CommandExt;
    let mut folder = None;
    let mut engine = None;
    let mut proton = None;
    let mut rest = arguments.iter();
    loop {
        let Some(argument) = rest.next() else {
            return Err("usage: korrid steam-engine-host --folder DIR --engine FILE [--proton DIR] -- HOST ARGS".into());
        };
        let mut value = || rest.next().map(PathBuf::from).ok_or("missing value");
        match argument.to_str() {
            Some("--folder") => folder = Some(value()?),
            Some("--engine") => engine = Some(value()?),
            Some("--proton") => proton = Some(value()?),
            Some("--") => break,
            _ => return Err("unknown steam-engine-host argument".into()),
        }
    }
    let (Some(folder), Some(engine)) = (folder, engine) else {
        return Err("steam-engine-host needs --folder and --engine".into());
    };
    let host: Vec<_> = rest.collect();
    let Some((program, host_arguments)) = host.split_first() else {
        return Err("steam-engine-host needs the host command".into());
    };
    let account = folder.parent().ok_or("engine folder has no account root")?;
    let layout = EngineFolder::for_account(account);
    if layout.root() != folder {
        return Err("engine folder is not the account root's Steam engine folder".into());
    }
    layout
        .prepare(&engine, proton.as_deref())
        .map_err(|error| format!("cannot prepare the Steam engine folder: {error}"))?;
    let error = Process::new(program)
        .args(host_arguments)
        .env("HOME", layout.home())
        .current_dir(layout.home())
        .exec();
    Err(format!("cannot start the Steam engine host: {error}"))
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
mod tests {
    use super::*;

    fn plan() -> HostPlan {
        HostPlan {
            host_program: "/nix/store/x-steam/bin/korri-steam-host".into(),
            engine:
                "/var/lib/korri/cache/steam/engine-package/1792618285/linuxarm64/steamclient.so"
                    .into(),
            proton: Some("/var/lib/korri/cache/proton/cachyos/11.0".into()),
            account_root: "/var/lib/korri/accounts/p1".into(),
        }
    }

    fn identity() -> Identity {
        Identity {
            uid: 1000,
            gid: 100,
            korrid: "/nix/store/k/bin/korrid".into(),
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
        let command = &arguments[arguments.iter().position(|a| a == "--").unwrap() + 1..];
        assert_eq!(
            command,
            [
                "/nix/store/k/bin/korrid",
                "steam-engine-host",
                "--folder",
                "/var/lib/korri/accounts/p1/steam-engine",
                "--engine",
                "/var/lib/korri/cache/steam/engine-package/1792618285/linuxarm64/steamclient.so",
                "--proton",
                "/var/lib/korri/cache/proton/cachyos/11.0",
                "--",
                "/nix/store/x-steam/bin/korri-steam-host",
                "--commands",
                &format!("/run/korri-steam-host-{id}/channel/commands"),
                "--host-lock",
                &format!("/run/korri-steam-host-{id}/channel/host.lock"),
                "--session-lock",
                &format!("/run/korri-steam-host-{id}/channel/session.lock"),
                "/var/lib/korri/accounts/p1/steam-engine/steam/linuxarm64/steamclient.so",
            ]
        );
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
        let command = command(&plan(), &mode, &identity());
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
while read -r line < "$fifo"; do
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

    #[test]
    fn the_event_reader_keeps_a_partial_line_for_the_next_read() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(EVENTS);
        fs::write(&path, b"{\"event\":\"apps_done\"}\n{\"event\":\"signed_").unwrap();
        let mut reader = EventReader::new(temp.path());
        assert_eq!(reader.read().unwrap(), [protocol::Event::AppsDone {}]);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"out\"}\nnot json\n").unwrap();
        assert_eq!(reader.read().unwrap(), [protocol::Event::SignedOut {}]);
        assert!(reader.read().unwrap().is_empty());
    }
}
