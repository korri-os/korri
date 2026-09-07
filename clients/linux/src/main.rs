use serde_json::{json, Value};
use std::{
    collections::HashMap,
    env, fs,
    io::{self, Read, Write},
    net::SocketAddr,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        linux::net::SocketAddrExt,
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
            net::{SocketAddr as UnixSocketAddr, UnixDatagram, UnixStream},
            process::CommandExt,
        },
    },
    process::{Child, Command, ExitCode, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const MESSAGE_LIMIT: usize = 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const CONSUMED_BINDING: &str = "__korriRpcConsumed";
static STOP: AtomicBool = AtomicBool::new(false);
type Result<T> = std::result::Result<T, &'static str>;

extern "C" fn stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

struct Configuration {
    capability: String,
    port: u16,
    origin: String,
    url: String,
}

impl Configuration {
    fn from_environment() -> Result<Self> {
        let directory =
            env::var_os("CREDENTIALS_DIRECTORY").ok_or("systemd credentials are required")?;
        let path = std::path::PathBuf::from(directory).join("KORRID_RPC_CAPABILITY");
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|_| "cannot open the RPC credential")?;
        if !credential_is_private(&file).map_err(|_| "cannot inspect the RPC credential")? {
            return Err("the RPC credential must be a private regular file accessible only to root and this service");
        }
        let mut capability = String::new();
        file.take(4097)
            .read_to_string(&mut capability)
            .map_err(|_| "cannot read the RPC credential")?;
        if capability.ends_with('\n') {
            capability.pop();
        }
        if capability.is_empty()
            || capability.len() > 4096
            || !capability.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
        {
            return Err("the RPC credential is invalid");
        }
        let address: SocketAddr = env::var("KORRID_ADDRESS")
            .map_err(|_| "KORRID_ADDRESS is required")?
            .parse()
            .map_err(|_| "KORRID_ADDRESS must be a socket address")?;
        if address.port() == 0 {
            return Err("korrid must have a nonzero port");
        }
        let origin =
            env::var("KORRID_PORTAL_ORIGIN").map_err(|_| "KORRID_PORTAL_ORIGIN is required")?;
        let url =
            env::var("KORRI_WEB_SURFACE_URL").map_err(|_| "KORRI_WEB_SURFACE_URL is required")?;
        validate_url(&origin, &url)?;
        Ok(Self {
            capability,
            port: address.port(),
            origin,
            url,
        })
    }

    fn binding_script(&self) -> String {
        // Names and methods come from contracts/bridge/korri-rpc-bridge.ts.
        // Each new document evaluates this check before the credential exists
        // in its world. Do not install a binding into child frames or popups.
        format!(
            r#"(() => {{
if (window.top !== window || location.origin !== {origin}) return;
const capability = {capability};
const consumed = window.{consumed_binding};
delete window.{consumed_binding};
let used = 0;
const markUsed = bit => {{
  const previous = used;
  used |= bit;
  if (used === 3 && previous !== 3) consumed('');
}};
const binding = Object.freeze({{
  korridPort: () => {{ markUsed(1); return {port}; }},
  korridCapability: () => {{ markUsed(2); return capability; }}
}});
Object.defineProperty(window, 'KorriRpc', {{value: binding, enumerable: false, writable: false, configurable: false}});
}})();"#,
            origin = json!(self.origin),
            capability = json!(self.capability),
            port = self.port,
            consumed_binding = CONSUMED_BINDING
        )
    }
}

fn credential_is_private(file: &fs::File) -> io::Result<bool> {
    let metadata = file.metadata()?;
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_file() || (metadata.uid() != 0 && metadata.uid() != uid) {
        return Ok(false);
    }
    // Linux POSIX ACL xattrs use a version header and eight-byte entries.
    // Bound the read to systemd's observed five-entry credential ACL. Extra
    // entries, unsupported shapes and xattr errors fail closed.
    let mut acl = [0_u8; 44];
    let size = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            c"system.posix_acl_access".as_ptr(),
            acl.as_mut_ptr().cast(),
            acl.len(),
        )
    };
    if size < 0 {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ENODATA | libc::EOPNOTSUPP) => {
                Ok(metadata.uid() == uid && metadata.mode() & 0o077 == 0)
            }
            Some(libc::ERANGE) => Ok(false),
            _ => Err(error),
        };
    }
    Ok(metadata.mode() & 0o777 == 0o440 && credential_acl_is_private(&acl[..size as usize], uid))
}

fn credential_acl_is_private(acl: &[u8], uid: u32) -> bool {
    // systemd 258 keeps ownership with root and grants the consumer read access
    // with a named-user ACL. Mode 0440's group bits are its mask, not a grant.
    if acl.len() != 44 || acl[..4] != 2_u32.to_le_bytes() {
        return false;
    }
    let expected = [
        (1_u16, 4_u16, u32::MAX), // owner
        (2, 4, uid),              // consuming user
        (4, 0, u32::MAX),         // owning group: no access
        (16, 4, u32::MAX),        // mask: read only
        (32, 0, u32::MAX),        // other: no access
    ];
    acl[4..]
        .chunks_exact(8)
        .zip(expected)
        .all(|(entry, (tag, permission, id))| {
            entry[..2] == tag.to_le_bytes()
                && entry[2..4] == permission.to_le_bytes()
                && entry[4..] == id.to_le_bytes()
        })
}

fn validate_url(origin: &str, url: &str) -> Result<()> {
    let address: SocketAddr = origin
        .strip_prefix("http://")
        .ok_or("the portal origin must use local HTTP")?
        .parse()
        .map_err(|_| "the portal origin must identify the local HTTP server")?;
    if address.ip() != std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        || address.port() == 0
        || origin != format!("http://{address}")
    {
        return Err("the portal origin must be a canonical 127.0.0.1 HTTP origin");
    }
    if !url.starts_with(&format!("{origin}/"))
        || url.contains('\\')
        || url.bytes().any(|byte| byte <= 0x20 || byte == 0x7f)
    {
        return Err("the portal URL must stay on the configured origin");
    }
    Ok(())
}

fn chromium_arguments() -> Result<(std::ffi::OsString, Vec<std::ffi::OsString>)> {
    let mut args = env::args_os().skip(1);
    let executable = args
        .next()
        .ok_or("usage: korri-portal-shell CHROMIUM [--flag ...]")?;
    let flags: Vec<_> = args.collect();
    for flag in &flags {
        let text = flag.to_str().ok_or("Chromium flags must be UTF-8")?;
        if !text.starts_with("--")
            || text.starts_with("--remote-debugging")
            || text.starts_with("--restore-last-session")
            || text.starts_with("--app")
            || text.starts_with("--no-sandbox")
            || text.starts_with("--disable-setuid-sandbox")
            || text.starts_with("--disable-web-security")
            || text.starts_with("--single-process")
            || text.starts_with("--enable-crash")
            || text.starts_with("--crash-dumps-dir")
            || text.starts_with("--enable-logging")
            || text.starts_with("--log-file")
        {
            return Err("Chromium flags cannot override navigation, debugging, or sandboxing");
        }
    }
    Ok((executable, flags))
}

// Reserve the pipe ends above fd 4 before pre_exec. A dup2 cannot overwrite
// the other end even when this process starts with vacant standard fds.
fn reserve_fd(stream: &UnixStream) -> Result<OwnedFd> {
    let fd = unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 5) };
    if fd < 0 {
        return Err("cannot reserve the Chromium pipe");
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

struct Browser(Child);
impl Drop for Browser {
    fn drop(&mut self) {
        // The isolated process group includes renderer/utility processes. It
        // is established by pre_exec before the executable can create them.
        unsafe {
            libc::kill(-(self.0.id() as libc::pid_t), libc::SIGKILL);
        }
        let _ = self.0.wait();
    }
}

// CDP supplies the execution-context origin and frame, not the page. A binding
// callback from an iframe or another origin cannot authorize systemd readiness.
struct PortalReadiness {
    session: String,
    frame: String,
    origin: String,
    contexts: HashMap<u64, bool>,
    consumed: Option<u64>,
}

impl PortalReadiness {
    fn observe(&mut self, message: &Value) {
        if message["sessionId"].as_str() != Some(self.session.as_str()) {
            return;
        }
        let params = &message["params"];
        match message["method"].as_str() {
            Some("Runtime.executionContextCreated") => {
                let context = &params["context"];
                if let Some(id) = context["id"].as_u64() {
                    self.contexts.insert(
                        id,
                        context["origin"] == self.origin
                            && context["auxData"]["frameId"] == self.frame
                            && context["auxData"]["isDefault"] == true,
                    );
                }
            }
            Some("Runtime.executionContextDestroyed") => {
                if let Some(id) = params["executionContextId"].as_u64() {
                    self.contexts.remove(&id);
                    if self.consumed == Some(id) {
                        self.consumed = None;
                    }
                }
            }
            Some("Runtime.executionContextsCleared") => {
                self.contexts.clear();
                self.consumed = None;
            }
            Some("Runtime.bindingCalled")
                if params["name"] == CONSUMED_BINDING && params["payload"] == "" =>
            {
                if let Some(id) = params["executionContextId"].as_u64() {
                    if self.contexts.get(&id) == Some(&true) {
                        self.consumed = Some(id);
                    }
                }
            }
            _ => {}
        }
    }
}

fn notify_ready_at(socket: Option<&std::ffi::OsStr>) -> Result<()> {
    let Some(socket) = socket else {
        return Ok(());
    };
    let bytes = socket.as_bytes();
    let address = if let Some(name) = bytes.strip_prefix(b"@") {
        if name.is_empty() {
            return Err("invalid systemd notification address");
        }
        UnixSocketAddr::from_abstract_name(name)
    } else if bytes.starts_with(b"/") {
        UnixSocketAddr::from_pathname(socket)
    } else {
        return Err("invalid systemd notification address");
    }
    .map_err(|_| "invalid systemd notification address")?;
    let socket = UnixDatagram::unbound().map_err(|_| "cannot open systemd notification socket")?;
    socket
        .set_write_timeout(Some(COMMAND_TIMEOUT))
        .map_err(|_| "cannot limit systemd notification")?;
    if socket
        .send_to_addr(b"READY=1", &address)
        .map_err(|_| "cannot notify systemd readiness")?
        != 7
    {
        return Err("incomplete systemd readiness notification");
    }
    Ok(())
}

struct Protocol {
    input: UnixStream,
    output: UnixStream,
    pending: Vec<u8>,
    sequence: u64,
    readiness: Option<PortalReadiness>,
}

impl Protocol {
    fn receive(&mut self, deadline: Instant) -> Result<Option<Value>> {
        loop {
            if STOP.load(Ordering::Relaxed) {
                return Err("shell stopped");
            }
            if let Some(end) = self.pending.iter().position(|byte| *byte == 0) {
                let message: Value = serde_json::from_slice(&self.pending[..end])
                    .map_err(|_| "invalid Chromium response")?;
                self.pending.drain(..=end);
                if let Some(readiness) = &mut self.readiness {
                    readiness.observe(&message);
                }
                return Ok(Some(message));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            self.input
                .set_read_timeout(Some(remaining.min(POLL_INTERVAL)))
                .map_err(|_| "cannot limit Chromium reads")?;
            let mut buffer = [0; 8192];
            match self.input.read(&mut buffer) {
                Ok(0) => return Err("Chromium closed its private pipe"),
                Ok(count) => {
                    if self.pending.len() + count > MESSAGE_LIMIT {
                        return Err("Chromium response exceeded its limit");
                    }
                    self.pending.extend_from_slice(&buffer[..count]);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut
                            | io::ErrorKind::WouldBlock
                            | io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return Err("cannot read the Chromium pipe"),
            }
        }
    }

    fn command(&mut self, method: &str, params: Value, session: Option<&str>) -> Result<Value> {
        self.sequence += 1;
        let mut message = json!({"id": self.sequence, "method": method, "params": params});
        if let Some(session) = session {
            message["sessionId"] = json!(session);
        }
        let mut bytes =
            serde_json::to_vec(&message).map_err(|_| "cannot prepare the Chromium command")?;
        if bytes.len() >= MESSAGE_LIMIT {
            return Err("Chromium command exceeded its limit");
        }
        bytes.push(0);
        self.output
            .set_write_timeout(Some(COMMAND_TIMEOUT))
            .map_err(|_| "cannot limit Chromium writes")?;
        self.output
            .write_all(&bytes)
            .map_err(|_| "cannot write the Chromium pipe")?;
        let deadline = Instant::now() + COMMAND_TIMEOUT;
        loop {
            let response = self
                .receive(deadline)?
                .ok_or("Chromium command timed out")?;
            if response.get("id").and_then(Value::as_u64) != Some(self.sequence) {
                continue;
            }
            if response.get("error").is_some() {
                return Err("Chromium rejected a bootstrap command");
            }
            return response
                .get("result")
                .cloned()
                .ok_or("Chromium omitted its command result");
        }
    }
}

fn run() -> Result<()> {
    let config = Configuration::from_environment()?;
    let (executable, flags) = chromium_arguments()?;
    let (input, child_output) =
        UnixStream::pair().map_err(|_| "cannot create the Chromium output pipe")?;
    let (output, child_input) =
        UnixStream::pair().map_err(|_| "cannot create the Chromium input pipe")?;
    let child_input_fd = reserve_fd(&child_input)?;
    let child_output_fd = reserve_fd(&child_output)?;
    let mut command = Command::new(executable);
    command
        .args(flags)
        .args([
            "--remote-debugging-pipe",
            "--disable-breakpad",
            "--disable-crash-reporter",
            "about:blank",
        ])
        .env_remove("NOTIFY_SOCKET")
        .env_remove("CREDENTIALS_DIRECTORY")
        .env_remove("KORRID_RPC_CAPABILITY")
        .env_remove("CHROMIUM_FLAGS")
        .env_remove("CHROMIUM_USER_FLAGS")
        .env_remove("CHROME_LOG_FILE")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(move || {
            if libc::setpgid(0, 0) != 0
                || libc::dup2(child_input_fd.as_raw_fd(), 3) < 0
                || libc::dup2(child_output_fd.as_raw_fd(), 4) < 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut browser = Browser(command.spawn().map_err(|_| "cannot start Chromium")?);
    // Command retains the closure and its OwnedFds. Close every local copy of
    // the child ends so EOF reliably detects a failed or stopped browser.
    drop(command);
    drop(child_input);
    drop(child_output);
    let mut protocol = Protocol {
        input,
        output,
        pending: Vec::new(),
        sequence: 0,
        readiness: None,
    };
    let targets = protocol.command("Target.getTargets", json!({}), None)?;
    let existing = targets["targetInfos"]
        .as_array()
        .and_then(|targets| {
            targets
                .iter()
                .find(|target| target["type"] == "page" && target["url"] == "about:blank")
        })
        .and_then(|target| target["targetId"].as_str());
    let target = if let Some(target) = existing {
        target.to_owned()
    } else {
        protocol.command("Target.createTarget", json!({"url": "about:blank"}), None)?["targetId"]
            .as_str()
            .ok_or("Chromium did not create the initial page")?
            .to_owned()
    };
    let attached = protocol.command(
        "Target.attachToTarget",
        json!({"targetId": target, "flatten": true}),
        None,
    )?;
    let session = attached["sessionId"]
        .as_str()
        .ok_or("Chromium did not attach the initial page")?;
    protocol.command("Page.enable", json!({}), Some(session))?;
    let tree = protocol.command("Page.getFrameTree", json!({}), Some(session))?;
    let frame = tree["frameTree"]["frame"]["id"]
        .as_str()
        .ok_or("Chromium omitted the portal frame")?;
    protocol.readiness = Some(PortalReadiness {
        session: session.to_owned(),
        frame: frame.to_owned(),
        origin: config.origin.clone(),
        contexts: HashMap::new(),
        consumed: None,
    });
    protocol.command("Runtime.enable", json!({}), Some(session))?;
    protocol.command(
        "Runtime.addBinding",
        json!({"name": CONSUMED_BINDING}),
        Some(session),
    )?;
    protocol.command(
        "Page.addScriptToEvaluateOnNewDocument",
        json!({"source": config.binding_script()}),
        Some(session),
    )?;
    let navigation =
        protocol.command("Page.navigate", json!({"url": config.url}), Some(session))?;
    if navigation.get("errorText").is_some() {
        return Err("Chromium could not open the portal");
    }
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    while protocol
        .readiness
        .as_ref()
        .and_then(|ready| ready.consumed)
        .is_none()
    {
        let message = protocol
            .receive(deadline)?
            .ok_or("the portal did not consume its RPC binding")?;
        if message["method"] == "Inspector.targetCrashed" {
            return Err("the portal page crashed");
        }
    }
    notify_ready_at(env::var_os("NOTIFY_SOCKET").as_deref())?;
    loop {
        if STOP.load(Ordering::Relaxed) {
            return Ok(());
        }
        match browser.0.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err("Chromium exited unsuccessfully")
                }
            }
            Ok(None) => {}
            Err(_) => return Err("cannot monitor Chromium"),
        }
        match protocol.receive(Instant::now() + POLL_INTERVAL) {
            Ok(Some(message)) if message["method"] == "Inspector.targetCrashed" => {
                return Err("the portal page crashed")
            }
            Ok(_) => {}
            Err(_) if STOP.load(Ordering::Relaxed) => return Ok(()),
            Err(error) => {
                return match browser.0.try_wait() {
                    Ok(Some(status)) if status.success() => Ok(()),
                    _ => Err(error),
                };
            }
        }
    }
}

fn main() -> ExitCode {
    // Credentials must not enter a core dump. Chromium inherits this limit.
    // The signal handler only stores a lock-free flag; main unwinds the guard.
    unsafe {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::setrlimit(libc::RLIMIT_CORE, &limit) != 0 {
            eprintln!("korri-portal-shell: cannot disable core dumps");
            return ExitCode::FAILURE;
        }
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = stop as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        if libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut()) != 0
            || libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut()) != 0
        {
            eprintln!("korri-portal-shell: cannot install shutdown handlers");
            return ExitCode::FAILURE;
        }
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) if STOP.load(Ordering::Relaxed) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("korri-portal-shell: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential_test_file() -> fs::File {
        use std::sync::atomic::AtomicUsize;
        static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
        let path = env::temp_dir().join(format!(
            "korri-credential-test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        fs::remove_file(path).unwrap();
        file
    }

    fn credential_acl(uid: u32) -> Vec<u8> {
        let mut bytes = 2_u32.to_le_bytes().to_vec();
        for (tag, permission, id) in [
            (1_u16, 4_u16, u32::MAX),
            (2, 4, uid),
            (4, 0, u32::MAX),
            (16, 4, u32::MAX),
            (32, 0, u32::MAX),
        ] {
            bytes.extend(tag.to_le_bytes());
            bytes.extend(permission.to_le_bytes());
            bytes.extend(id.to_le_bytes());
        }
        bytes
    }

    fn set_credential_acl(file: &fs::File, acl: &[u8]) {
        let result = unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                c"system.posix_acl_access".as_ptr(),
                acl.as_ptr().cast(),
                acl.len(),
                0,
            )
        };
        assert_eq!(result, 0, "{}", io::Error::last_os_error());
    }

    #[test]
    fn credential_acl_policy_accepts_only_the_observed_private_shape() {
        let acl = credential_acl(1001);
        assert!(credential_acl_is_private(&acl, 1001));
        assert!(!credential_acl_is_private(&acl, 1002));
        for index in 0..acl.len() {
            let mut changed = acl.clone();
            changed[index] ^= 1;
            assert!(!credential_acl_is_private(&changed, 1001), "byte {index}");
        }
        let mut extra = acl.clone();
        extra.extend_from_slice(&acl[12..20]);
        assert!(!credential_acl_is_private(&extra, 1001));
        assert!(!credential_acl_is_private(&acl[..43], 1001));
    }

    #[test]
    fn credential_accepts_systemd_acl() {
        let file = credential_test_file();
        assert!(credential_is_private(&file).unwrap());
        set_credential_acl(&file, &credential_acl(unsafe { libc::geteuid() }));
        assert_eq!(file.metadata().unwrap().mode() & 0o777, 0o440);
        assert!(credential_is_private(&file).unwrap());
    }

    #[test]
    fn credential_rejects_other_users_groups_and_world_access() {
        let uid = unsafe { libc::geteuid() };
        let file = credential_test_file();
        let allowed = credential_acl(uid);
        let mut other_user = allowed.clone();
        other_user[16..20].copy_from_slice(&uid.wrapping_add(1).to_le_bytes());
        let mut owning_group = allowed.clone();
        owning_group[22..24].copy_from_slice(&4_u16.to_le_bytes());
        let mut named_group = allowed.clone();
        named_group[12..20].copy_from_slice(&allowed[20..28]);
        named_group[20..28].copy_from_slice(&allowed[12..20]);
        named_group[20..22].copy_from_slice(&8_u16.to_le_bytes());
        let mut extra_user = allowed.clone();
        extra_user.splice(20..20, other_user[12..20].iter().copied());
        let mut world = allowed;
        world[38..40].copy_from_slice(&4_u16.to_le_bytes());
        for acl in [other_user, owning_group, named_group, world, extra_user] {
            set_credential_acl(&file, &acl);
            assert!(!credential_is_private(&file).unwrap());
        }
    }

    #[test]
    fn credential_rejects_malformed_or_extra_acl_entries() {
        let acl = credential_acl(1001);
        assert!(credential_acl_is_private(&acl, 1001));
        assert!(credential_acl_is_private(&credential_acl(0), 0));
        for size in 0..acl.len() {
            assert!(!credential_acl_is_private(&acl[..size], 1001));
        }
        for index in [0, 4, 6, 8, 12, 14, 20, 22, 28, 30, 36, 38] {
            let mut malformed = acl.clone();
            malformed[index] ^= 1;
            assert!(!credential_acl_is_private(&malformed, 1001));
        }
        let mut extra = acl;
        extra.extend([0_u8; 8]);
        assert!(!credential_acl_is_private(&extra, 1001));
    }

    #[test]
    fn credential_preserves_private_modes_and_rejects_nonregular_files() {
        use std::os::unix::fs::PermissionsExt;
        let file = credential_test_file();
        for mode in [0o400, 0o600] {
            file.set_permissions(fs::Permissions::from_mode(mode))
                .unwrap();
            assert!(credential_is_private(&file).unwrap());
        }
        for mode in [0o440, 0o640, 0o644] {
            file.set_permissions(fs::Permissions::from_mode(mode))
                .unwrap();
            assert!(!credential_is_private(&file).unwrap());
        }
        assert!(!credential_is_private(&fs::File::open(env::temp_dir()).unwrap()).unwrap());
    }

    #[test]
    fn readiness_notification_uses_real_unix_datagrams() {
        let name = format!("korri-notify-test-{}", std::process::id());
        let address = UnixSocketAddr::from_abstract_name(name.as_bytes()).unwrap();
        let receiver = UnixDatagram::bind_addr(&address).unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let environment_value = std::ffi::OsString::from(format!("@{name}"));
        notify_ready_at(Some(&environment_value)).unwrap();
        let mut message = [0; 32];
        let count = receiver.recv(&mut message).unwrap();
        assert_eq!(&message[..count], b"READY=1");
        assert!(notify_ready_at(None).is_ok());
        assert!(notify_ready_at(Some(std::ffi::OsStr::new("relative.sock"))).is_err());
        assert!(notify_ready_at(Some(std::ffi::OsStr::new("@"))).is_err());
    }

    #[test]
    fn portal_url_cannot_change_origin() {
        for url in [
            "http://127.0.0.1:8099.evil/",
            "http://127.0.0.1:8099@evil/",
            "http://127.0.0.1:8099/\\evil",
            "http://127.0.0.1:8099/\n",
        ] {
            assert!(validate_url("http://127.0.0.1:8099", url).is_err());
        }
        assert!(validate_url(
            "http://127.0.0.1:8099",
            "http://127.0.0.1:8099/?surface=pico"
        )
        .is_ok());
        assert!(validate_url("http://0.0.0.0:8099", "http://0.0.0.0:8099/").is_err());
    }
    #[test]
    fn private_protocol_is_bounded_and_detects_eof() {
        let (input, mut sender) = UnixStream::pair().unwrap();
        let (output, _receiver) = UnixStream::pair().unwrap();
        let mut protocol = Protocol {
            input,
            output,
            pending: Vec::new(),
            sequence: 0,
            readiness: None,
        };
        sender.write_all(b"{\"id\":1,\"result\":{}}\0").unwrap();
        assert_eq!(
            protocol
                .receive(Instant::now() + COMMAND_TIMEOUT)
                .unwrap()
                .unwrap()["id"],
            1
        );
        assert!(protocol.receive(Instant::now()).unwrap().is_none());
        protocol.pending = vec![b' '; MESSAGE_LIMIT];
        sender.write_all(b"x").unwrap();
        assert_eq!(
            protocol
                .receive(Instant::now() + COMMAND_TIMEOUT)
                .unwrap_err(),
            "Chromium response exceeded its limit"
        );
        protocol.pending.clear();
        drop(sender);
        assert_eq!(
            protocol
                .receive(Instant::now() + COMMAND_TIMEOUT)
                .unwrap_err(),
            "Chromium closed its private pipe"
        );
    }
}
