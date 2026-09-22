use axum::{serve::Listener, Router};
use std::{ffi::OsString, io, net::SocketAddr, os::fd::FromRawFd, path::PathBuf, time::Duration};
use tokio::sync::oneshot;

const MAX_TRANSIENT_ACCEPT_RETRIES: u8 = 4;
const INITIAL_ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(10);
const MAX_ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(80);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExpectedControlPeer {
    uid: u32,
    primary_gid: u32,
}

impl ExpectedControlPeer {
    fn from_environment() -> Result<Self, String> {
        let uid = required_unprivileged_id("KORRID_CONTROL_PEER_UID")?;
        let primary_gid = required_unprivileged_id("KORRID_CONTROL_PEER_GID")?;
        Ok(Self { uid, primary_gid })
    }
}

fn required_unprivileged_id(name: &str) -> Result<u32, String> {
    let value = std::env::var(name).map_err(|_| format!("{name} must be set"))?;
    let id = value
        .parse::<u32>()
        .map_err(|_| format!("{name} must be a numeric ID"))?;
    if id == 0 {
        return Err(format!("{name} must identify an unprivileged account"));
    }
    Ok(id)
}

fn unix_peer_credentials(stream: &tokio::net::UnixStream) -> io::Result<(u32, u32)> {
    stream
        .peer_cred()
        .map(|credentials| (credentials.uid(), credentials.gid()))
}

fn authorize_peer_credentials(
    expected: ExpectedControlPeer,
    credentials: io::Result<(u32, u32)>,
) -> bool {
    matches!(
        credentials,
        Ok((uid, gid)) if uid == expected.uid && gid == expected.primary_gid
    )
}

#[derive(Default)]
struct AcceptErrorBudget {
    transient_failures: u8,
}

impl AcceptErrorBudget {
    fn retry_delay(&mut self, error: &io::Error) -> Option<Duration> {
        if !is_transient_accept_error(error)
            || self.transient_failures >= MAX_TRANSIENT_ACCEPT_RETRIES
        {
            return None;
        }
        let delay = INITIAL_ACCEPT_RETRY_DELAY
            .saturating_mul(1_u32 << self.transient_failures)
            .min(MAX_ACCEPT_RETRY_DELAY);
        self.transient_failures += 1;
        Some(delay)
    }

    fn accepted(&mut self) {
        self.transient_failures = 0;
    }
}

fn is_transient_accept_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock | io::ErrorKind::ConnectionAborted
    ) || matches!(
        error.raw_os_error(),
        Some(
            libc::ENETDOWN
                | libc::EPROTO
                | libc::ENOPROTOOPT
                | libc::EHOSTDOWN
                | libc::ENONET
                | libc::EHOSTUNREACH
                | libc::EOPNOTSUPP
                | libc::ENETUNREACH
                | libc::EMFILE
                | libc::ENFILE
                | libc::ENOBUFS
                | libc::ENOMEM
        )
    )
}

struct AuthorizedUnixListener {
    listener: tokio::net::UnixListener,
    expected: ExpectedControlPeer,
    terminal_error: Option<oneshot::Sender<io::Error>>,
    error_budget: AcceptErrorBudget,
}

impl AuthorizedUnixListener {
    fn new(
        listener: tokio::net::UnixListener,
        expected: ExpectedControlPeer,
    ) -> (Self, oneshot::Receiver<io::Error>) {
        let (terminal_error, failure) = oneshot::channel();
        (
            Self {
                listener,
                expected,
                terminal_error: Some(terminal_error),
                error_budget: AcceptErrorBudget::default(),
            },
            failure,
        )
    }
}

impl Listener for AuthorizedUnixListener {
    type Io = tokio::net::UnixStream;
    type Addr = tokio::net::unix::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.listener.accept().await {
                Ok((stream, address)) => {
                    self.error_budget.accepted();
                    if authorize_peer_credentials(self.expected, unix_peer_credentials(&stream)) {
                        return (stream, address);
                    }
                }
                Err(error) => match self.error_budget.retry_delay(&error) {
                    Some(delay) => tokio::time::sleep(delay).await,
                    None => {
                        if let Some(terminal_error) = self.terminal_error.take() {
                            let _ = terminal_error.send(error);
                        }
                        std::future::pending::<()>().await;
                    }
                },
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

async fn serve_local_control(
    listener: AuthorizedUnixListener,
    failure: oneshot::Receiver<io::Error>,
    router: Router,
) -> io::Result<()> {
    tokio::select! {
        result = axum::serve(listener, router) => result,
        failure = failure => Err(failure.unwrap_or_else(|_| io::Error::other(
            "local control listener failure channel closed",
        ))),
    }
}

async fn first_server_exit<L, R, T>(lan: L, local: R) -> (&'static str, T)
where
    L: std::future::Future<Output = T>,
    R: std::future::Future<Output = T>,
{
    tokio::pin!(lan);
    tokio::pin!(local);
    tokio::select! {
        result = &mut lan => ("LAN", result),
        result = &mut local => ("local control", result),
    }
}

async fn serve_host_surfaces(
    lan_listener: tokio::net::TcpListener,
    lan_router: Router,
    local_listener: AuthorizedUnixListener,
    local_failure: oneshot::Receiver<io::Error>,
    local_router: Router,
) -> (&'static str, io::Result<()>) {
    let lan_server = async move { axum::serve(lan_listener, lan_router).await };
    let local_server = serve_local_control(local_listener, local_failure, local_router);
    first_server_exit(lan_server, local_server).await
}

async fn serve_with_discovery<S, Q, D>(
    serving: S,
    shutdown: Q,
    control: korrid::federation::coordinator::DiscoveryControl,
    discovery: D,
) -> Option<(&'static str, io::Result<()>)>
where
    S: std::future::Future<Output = (&'static str, io::Result<()>)>,
    Q: std::future::Future<Output = ()>,
    D: std::future::Future<Output = ()> + Send + 'static,
{
    let discovery = tokio::spawn(discovery);
    let result = tokio::select! {
        _ = shutdown => None,
        result = serving => Some(result),
    };
    control.cancel();
    discovery.await.expect("join relay discovery");
    result
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Brain,
    Host,
}

impl Mode {
    fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("brain") {
            "brain" => Ok(Self::Brain),
            "host" => Ok(Self::Host),
            other => Err(format!("KORRID_MODE must be brain or host, got {other:?}")),
        }
    }

    fn default_address(self) -> &'static str {
        match self {
            Self::Brain | Self::Host => "127.0.0.1:43117",
        }
    }
}

fn resolve_address(
    mode: Mode,
    configured: Option<&str>,
) -> Result<SocketAddr, std::net::AddrParseError> {
    configured.unwrap_or_else(|| mode.default_address()).parse()
}

fn resolve_host_config_path(
    explicit: Option<OsString>,
    config_home: Option<OsString>,
    home: Option<OsString>,
) -> PathBuf {
    explicit
        .map(PathBuf::from)
        .or_else(|| {
            config_home
                .map(PathBuf::from)
                .map(|root| root.join("korrid/host.toml"))
        })
        .or_else(|| {
            home.map(PathBuf::from)
                .map(|root| root.join(".config/korrid/host.toml"))
        })
        .unwrap_or_else(|| PathBuf::from("host.toml"))
}

fn host_config_path() -> PathBuf {
    resolve_host_config_path(
        std::env::var_os("KORRID_HOST_CONFIG"),
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

fn resolve_host_storage_root(explicit: Option<OsString>, home: Option<OsString>) -> PathBuf {
    explicit
        .map(PathBuf::from)
        .or_else(|| {
            home.map(PathBuf::from)
                .map(|root| root.join(".local/share/korri"))
        })
        .unwrap_or_else(|| PathBuf::from("korri"))
}

fn host_storage_root() -> PathBuf {
    resolve_host_storage_root(
        std::env::var_os("KORRID_STORAGE_ROOT"),
        std::env::var_os("HOME"),
    )
}

fn resolve_private_state_root(
    explicit: Option<OsString>,
    state_home: Option<OsString>,
    home: Option<OsString>,
) -> PathBuf {
    explicit
        .map(PathBuf::from)
        .or_else(|| state_home.map(PathBuf::from).map(|root| root.join("korri")))
        .or_else(|| {
            home.map(PathBuf::from)
                .map(|root| root.join(".local/state/korri"))
        })
        .unwrap_or_else(|| PathBuf::from("korri-state"))
}

fn private_state_root() -> PathBuf {
    resolve_private_state_root(
        std::env::var_os("KORRID_PRIVATE_STATE_ROOT"),
        std::env::var_os("XDG_STATE_HOME"),
        std::env::var_os("HOME"),
    )
}

fn owner_uses_local_signer(
    state: &korrid::identity::IdentityState,
    signer_public_key: &str,
) -> bool {
    matches!(
        state,
        korrid::identity::IdentityState::Owned { owner_public_key, .. }
            if owner_public_key == signer_public_key
    )
}

fn brain_router(
    resources: korrid::federation::coordinator::FederationResources,
    wake: Option<korrid::federation::coordinator::DiscoveryControl>,
) -> Router {
    let capability = std::env::var("KORRID_RPC_CAPABILITY")
        .expect("KORRID_RPC_CAPABILITY must be set for the brain server");
    let allowed_origin = std::env::var("KORRID_PORTAL_ORIGIN")
        .unwrap_or_else(|_| "https://appassets.androidplatform.net".into());
    korrid::router_with_capability_and_federation(
        &capability,
        &allowed_origin,
        std::env::var_os("KORRI_LOCAL_STORAGE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("korri")),
        private_state_root(),
        Some(resources),
        wake,
    )
}

fn credential_is_private(file: &std::fs::File) -> io::Result<bool> {
    use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
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

// Linux startup reads the same capability credential that its browser shell
// receives. Authentication and permissions remain in the shared RPC handler.
fn host_portal_access(
    origin: Option<&str>,
    credentials: Option<&std::path::Path>,
) -> Result<Option<korrid::portal_access::PortalAccess>, String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let Some(origin) = origin else {
        return Ok(None);
    };
    let directory = credentials.ok_or("portal access requires systemd credentials")?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(directory.join("KORRID_RPC_CAPABILITY"))
        .map_err(|_| "cannot open the portal credential")?;
    if !credential_is_private(&file).map_err(|_| "cannot inspect the portal credential")? {
        return Err("the portal credential must be private to root and this service".into());
    }
    let mut capability = String::new();
    file.take(4097)
        .read_to_string(&mut capability)
        .map_err(|_| "cannot read the portal credential")?;
    if capability.ends_with('\n') {
        capability.pop();
    }
    if capability.is_empty()
        || capability.len() > 4096
        || !capability.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return Err("invalid portal credential".into());
    }
    Ok(Some(korrid::portal_access::PortalAccess::new(
        &capability,
        origin,
        korrid::portal_access::PortalPermission::LocalSessions,
    )))
}

fn validate_socket_activation(
    listen_pid: Option<&str>,
    listen_fds: Option<&str>,
    current_pid: u32,
) -> Result<bool, String> {
    let Some(count) = listen_fds else {
        return Ok(false);
    };
    let count: u32 = count
        .parse()
        .map_err(|_| "LISTEN_FDS must be a number".to_owned())?;
    let pid: u32 = listen_pid
        .ok_or_else(|| "LISTEN_PID is required with LISTEN_FDS".to_owned())?
        .parse()
        .map_err(|_| "LISTEN_PID must be a number".to_owned())?;
    if pid != current_pid || count != 1 {
        return Err("korrid requires exactly one inherited listener for its current PID".into());
    }
    Ok(true)
}

fn inherited_control_listener() -> Result<Option<std::os::unix::net::UnixListener>, String> {
    let listen_pid = std::env::var("LISTEN_PID").ok();
    let listen_fds = std::env::var("LISTEN_FDS").ok();
    if !validate_socket_activation(
        listen_pid.as_deref(),
        listen_fds.as_deref(),
        std::process::id(),
    )? {
        return Ok(None);
    }
    // systemd's socket-activation treaty assigns the first inherited descriptor
    // to fd 3. Ownership transfers to this listener exactly once.
    let listener = unsafe { std::os::unix::net::UnixListener::from_raw_fd(3) };
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("could not configure inherited listener: {error}"))?;
    Ok(Some(listener))
}

#[tokio::main]
async fn main() {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|argument| argument == "plugin-launch")
    {
        let result = match arguments.as_slice() {
            [_, source, input] => input
                .to_str()
                .ok_or_else(|| "launch input must be UTF-8".to_owned())
                .and_then(|input| {
                    korrid::launcher::plugin_launch::execute(std::path::Path::new(source), input)
                }),
            _ => Err("usage: korrid plugin-launch SOURCE INPUT_JSON".into()),
        };
        if let Err(error) = result {
            eprintln!("korrid plugin-launch: {error}");
            std::process::exit(1);
        }
        return;
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "catalog")
    {
        match korrid::catalog_cli::run_from_environment(&arguments[1..]) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("korrid catalog: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "identity")
    {
        match korrid::identity_cli::run_from_environment(&arguments[1..], &private_state_root()) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("korrid identity: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let private_state_root = private_state_root();
    let signer_socket = std::env::var_os("KORRID_LOCAL_SIGNER_SOCKET")
        .map(PathBuf::from)
        .expect("KORRID_LOCAL_SIGNER_SOCKET must be set");
    korrid::identity_switch::recover_pending_identity_switch(
        &private_state_root,
        signer_socket.clone(),
    )
    .await
    .unwrap_or_else(|error| panic!("recover identity switch: {error}"));
    let signer = korrid::local_signer::UnixPersonSigner::new(signer_socket);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let identity_state = korrid::identity::bind_automatic_owner(&private_state_root, &signer, now)
        .await
        .unwrap_or_else(|error| panic!("bind automatic identity: {error}"));
    let signer_public_key_path = std::env::var_os("KORRID_LOCAL_SIGNER_PUBLIC_KEY_FILE")
        .map(PathBuf::from)
        .expect("KORRID_LOCAL_SIGNER_PUBLIC_KEY_FILE must be set");
    let signer_public_key =
        korrid::local_signer::read_published_public_key_if_present(&signer_public_key_path)
            .unwrap_or_else(|error| panic!("read local signer public key: {error}"));
    let local_only_owner = signer_public_key
        .as_deref()
        .is_some_and(|public_key| owner_uses_local_signer(&identity_state, public_key));

    use korrid::federation::coordinator::{
        Discovery, DiscoveryInputs, DiscoveryTiming, FederationResources,
    };
    use std::sync::Arc;
    let resources =
        FederationResources::open(&private_state_root).expect("open federation authority");
    let mode_value = std::env::var("KORRID_MODE").ok();
    let mode = Mode::parse(mode_value.as_deref()).unwrap_or_else(|error| panic!("{error}"));
    let config_root = match mode {
        Mode::Host => host_storage_root(),
        Mode::Brain => std::env::var_os("KORRI_LOCAL_STORAGE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("korri")),
    };
    let discovery = if local_only_owner {
        None
    } else {
        let relays = korrid::relay::RelayList::from_linux_environment(
            std::env::var("KORRID_RELAYS").ok().as_deref(),
        )
        .unwrap_or_else(|error| panic!("invalid relay configuration: {error}"));
        let advertised_endpoints = std::env::var("KORRID_ADVERTISED_ENDPOINTS")
            .ok()
            .map(|json| {
                serde_json::from_str::<Vec<String>>(&json)
                    .expect("KORRID_ADVERTISED_ENDPOINTS must be a JSON array")
            })
            .unwrap_or_default();
        let initial_inputs = DiscoveryInputs {
            relays,
            advertised_endpoints,
            label: std::env::var("HOSTNAME").ok(),
            moonlight_address: std::env::var("KORRID_MOONLIGHT_ADDRESS").ok(),
        };
        initial_inputs
            .validate()
            .expect("valid advertised endpoints and metadata");
        let config = korrid::config::snapshot::ConfigSnapshotCoordinator::new(config_root);
        Some(
            Discovery::new(
                resources.directory.clone(),
                resources.credentials.clone(),
                Arc::new(move || DiscoveryInputs::linux(&config, &initial_inputs)),
                Arc::new(korrid::relay::WebSocketRelayTransport::new()),
                DiscoveryTiming::default(),
                Arc::new(|| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                }),
            )
            .start(),
        )
    };
    let wake = discovery.as_ref().map(|(control, _)| control.clone());
    let (lan_router, local_control_router) = match mode {
        Mode::Brain => (brain_router(resources, wake), None),
        Mode::Host => {
            let origin = std::env::var("KORRID_PORTAL_ORIGIN").ok();
            let credentials = std::env::var_os("CREDENTIALS_DIRECTORY").map(PathBuf::from);
            let portal = host_portal_access(origin.as_deref(), credentials.as_deref())
                .unwrap_or_else(|error| panic!("invalid portal access: {error}"));
            let (lan, local) = korrid::host_routers_with_federation(
                host_config_path(),
                Some(host_storage_root()),
                private_state_root.clone(),
                resources,
                portal,
            );
            (lan, Some(local))
        }
    };
    let configured_address = std::env::var("KORRID_ADDRESS").ok();
    let address =
        resolve_address(mode, configured_address.as_deref()).expect("valid KORRID_ADDRESS");
    let lan_listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("bind korrid server");

    // Install both signal handlers before spawning discovery. Either serving
    // failure and either shutdown signal cancel and join the same task.
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("install SIGTERM handler");
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .expect("install SIGINT handler");
    let local = local_control_router.and_then(|router| {
        inherited_control_listener()
            .unwrap_or_else(|error| panic!("{error}"))
            .map(|listener| {
                let listener = tokio::net::UnixListener::from_std(listener)
                    .expect("adopt inherited local control listener");
                let expected = ExpectedControlPeer::from_environment()
                    .unwrap_or_else(|error| panic!("invalid local control peer identity: {error}"));
                let (listener, failure) = AuthorizedUnixListener::new(listener, expected);
                (router, listener, failure)
            })
    });
    let serving = async move {
        if let Some((router, listener, failure)) = local {
            serve_host_surfaces(lan_listener, lan_router, listener, failure, router).await
        } else {
            ("LAN", axum::serve(lan_listener, lan_router).await)
        }
    };
    let shutdown = async move {
        tokio::select! {
            _ = terminate.recv() => {},
            _ = interrupt.recv() => {},
        }
    };
    let result = if let Some((wake, discovery)) = discovery {
        serve_with_discovery(serving, shutdown, wake, discovery).await
    } else {
        tokio::select! {
            _ = shutdown => None,
            result = serving => Some(result),
        }
    };
    if let Some((name, result)) = result {
        result.unwrap_or_else(|error| panic!("serve {name} korrid: {error}"));
        panic!("{name} korrid server exited unexpectedly");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::{
            fd::AsRawFd,
            unix::fs::{MetadataExt, OpenOptionsExt},
        },
    };

    fn credential_test_file() -> fs::File {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
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
        assert!(!credential_is_private(&fs::File::open(std::env::temp_dir()).unwrap()).unwrap());
    }

    #[tokio::test]
    async fn either_server_failure_cancels_and_joins_the_directory_owner() {
        use korrid::federation::coordinator::{
            Discovery, DiscoveryInputs, DiscoveryTiming, FederationResources,
        };
        use std::{os::unix::fs::PermissionsExt, sync::Arc};
        for failure in ["LAN", "local control"] {
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let resources = FederationResources::open(root.path()).unwrap();
            let relays =
                korrid::relay::RelayList::configured(vec!["ws://localhost:7001".into()]).unwrap();
            let transport = Arc::new(korrid::relay::InProcessRelayNetwork::new(&relays));
            let (control, discovery) = Discovery::new(
                resources.directory.clone(),
                resources.credentials.clone(),
                Arc::new(move || {
                    Ok(DiscoveryInputs {
                        relays: Some(relays.clone()),
                        advertised_endpoints: vec![],
                        label: None,
                        moonlight_address: None,
                    })
                }),
                transport,
                DiscoveryTiming::default(),
                Arc::new(|| 1000),
            )
            .start();
            drop(resources);
            let server = |name| async move {
                if name != failure {
                    std::future::pending::<()>().await;
                }
                tokio::task::yield_now().await;
                Err(io::Error::other("configured serving failure"))
            };
            let result = serve_with_discovery(
                first_server_exit(server("LAN"), server("local control")),
                std::future::pending(),
                control,
                discovery,
            )
            .await
            .unwrap();
            assert_eq!(result.0, failure);
            assert!(result.1.is_err());
            // A detached directory owner would keep the private-root writer lease.
            FederationResources::open(root.path()).unwrap();
        }
    }

    #[test]
    fn only_the_runtime_local_signer_owner_is_local_only() {
        let state = korrid::identity::IdentityState::Owned {
            device_public_key: "device".into(),
            owner_public_key: "local".into(),
            event_id: "event".into(),
            created_at: 1,
        };
        assert!(owner_uses_local_signer(&state, "local"));
        assert!(!owner_uses_local_signer(&state, "another-owner"));
        assert!(!owner_uses_local_signer(
            &korrid::identity::IdentityState::Unowned {
                device_public_key: "device".into(),
            },
            "local",
        ));
    }

    #[test]
    fn mode_defaults_to_brain_and_rejects_unknown_values() {
        assert_eq!(Mode::parse(None), Ok(Mode::Brain));
        assert_eq!(Mode::parse(Some("host")), Ok(Mode::Host));
        assert!(Mode::parse(Some("other")).unwrap_err().contains("other"));
    }

    #[test]
    fn host_address_defaults_to_loopback_unless_explicitly_configured() {
        assert_eq!(
            resolve_address(Mode::Host, None).unwrap(),
            "127.0.0.1:43117".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(
            resolve_address(Mode::Host, Some("0.0.0.0:43117")).unwrap(),
            "0.0.0.0:43117".parse::<SocketAddr>().unwrap()
        );
    }

    #[test]
    fn host_config_path_uses_explicit_then_xdg_then_home() {
        assert_eq!(
            resolve_host_config_path(
                Some("/explicit.toml".into()),
                Some("/xdg".into()),
                Some("/home/test".into()),
            ),
            PathBuf::from("/explicit.toml"),
        );
        assert_eq!(
            resolve_host_config_path(None, Some("/xdg".into()), Some("/home/test".into())),
            PathBuf::from("/xdg/korrid/host.toml"),
        );
        assert_eq!(
            resolve_host_config_path(None, None, Some("/home/test".into())),
            PathBuf::from("/home/test/.config/korrid/host.toml"),
        );
    }

    #[test]
    fn host_storage_uses_explicit_then_home() {
        assert_eq!(
            resolve_host_storage_root(Some("/games".into()), Some("/home/test".into())),
            PathBuf::from("/games"),
        );
        assert_eq!(
            resolve_host_storage_root(None, Some("/home/test".into())),
            PathBuf::from("/home/test/.local/share/korri"),
        );
    }

    #[test]
    fn socket_activation_accepts_only_one_listener_for_the_current_process() {
        assert_eq!(validate_socket_activation(None, None, 42), Ok(false));
        assert_eq!(
            validate_socket_activation(Some("42"), Some("1"), 42),
            Ok(true)
        );
        assert!(validate_socket_activation(Some("41"), Some("1"), 42).is_err());
        assert!(validate_socket_activation(Some("42"), Some("2"), 42).is_err());
        assert!(validate_socket_activation(None, Some("1"), 42).is_err());
    }

    #[test]
    fn host_portal_requires_private_credentials_when_an_origin_is_configured() {
        assert!(host_portal_access(None, None).unwrap().is_none());
        assert!(host_portal_access(Some("http://127.0.0.1:8099"), None).is_err());
        let directory = tempfile::tempdir().unwrap();
        assert!(host_portal_access(Some("http://127.0.0.1:8099"), Some(directory.path())).is_err());
        let path = directory.path().join("KORRID_RPC_CAPABILITY");
        std::fs::write(&path, "private-token\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(host_portal_access(Some("http://127.0.0.1:8099"), Some(directory.path())).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            host_portal_access(Some("http://127.0.0.1:8099"), Some(directory.path()))
                .unwrap()
                .is_some()
        );
        std::fs::write(&path, "\n").unwrap();
        assert!(host_portal_access(Some("http://127.0.0.1:8099"), Some(directory.path())).is_err());
    }

    #[tokio::test]
    async fn host_portal_credentials_grant_local_sessions_without_other_mutations() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use std::os::unix::fs::PermissionsExt;
        use tower::ServiceExt;

        let root = tempfile::tempdir().unwrap();
        let credential = root.path().join("KORRID_RPC_CAPABILITY");
        std::fs::write(&credential, "private-token\n").unwrap();
        std::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o600)).unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"rg353m\"\n").unwrap();
        let origin = "http://127.0.0.1:8099";
        let (app, _) = korrid::host_routers_with_storage_and_private(
            &config,
            None::<PathBuf>,
            root.path().join("private"),
            host_portal_access(Some(origin), Some(root.path())).unwrap(),
        );
        for (body, expected) in [
            (
                serde_json::json!({"_tag":"system.health","payload":{}}),
                StatusCode::OK,
            ),
            (
                serde_json::json!({"_tag":"app.session.prepare","payload":{"gameId":"missing"}}),
                StatusCode::OK,
            ),
            (
                serde_json::json!({"_tag":"app.session.stop","payload":{}}),
                StatusCode::OK,
            ),
            (
                serde_json::json!({"_tag":"app.session.prepare","payload":{"gameId":"missing","host":"rg353m"}}),
                StatusCode::FORBIDDEN,
            ),
            (
                serde_json::json!({"_tag":"app.discovery.rescan","payload":{}}),
                StatusCode::FORBIDDEN,
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/rpc")
                        .header("Content-Type", "application/json")
                        .header("Authorization", "Bearer private-token")
                        .header("Origin", origin)
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{body}");
        }
    }

    #[test]
    fn host_portal_reads_acl_credentials_and_rejects_symlinks_and_fifos() {
        use std::os::unix::{ffi::OsStrExt, fs::symlink};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("KORRID_RPC_CAPABILITY");
        let file = fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        use std::io::Write;
        (&file).write_all(b"private-token").unwrap();
        set_credential_acl(&file, &credential_acl(unsafe { libc::geteuid() }));
        assert!(
            host_portal_access(Some("http://127.0.0.1:8099"), Some(directory.path()))
                .unwrap()
                .is_some()
        );
        let target = directory.path().join("target");
        fs::rename(&path, &target).unwrap();
        symlink(&target, &path).unwrap();
        assert!(host_portal_access(Some("http://127.0.0.1:8099"), Some(directory.path())).is_err());
        fs::remove_file(&path).unwrap();
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        assert!(host_portal_access(Some("http://127.0.0.1:8099"), Some(directory.path())).is_err());
    }

    #[test]
    fn local_control_peer_requires_exact_uid_and_primary_gid_and_fails_closed() {
        let expected = ExpectedControlPeer {
            uid: 1001,
            primary_gid: 1002,
        };
        assert!(authorize_peer_credentials(expected, Ok((1001, 1002))));
        assert!(!authorize_peer_credentials(expected, Ok((1003, 1002))));
        assert!(!authorize_peer_credentials(expected, Ok((1001, 1004))));
        assert!(!authorize_peer_credentials(
            expected,
            Err(io::Error::other("SO_PEERCRED unavailable")),
        ));
    }

    #[test]
    fn transient_accept_errors_use_bounded_backoff_then_surface_failure() {
        let mut budget = AcceptErrorBudget::default();
        let transient = || io::Error::from(io::ErrorKind::ConnectionAborted);

        assert_eq!(
            (0..MAX_TRANSIENT_ACCEPT_RETRIES)
                .map(|_| budget.retry_delay(&transient()))
                .collect::<Vec<_>>(),
            vec![
                Some(Duration::from_millis(10)),
                Some(Duration::from_millis(20)),
                Some(Duration::from_millis(40)),
                Some(Duration::from_millis(80)),
            ]
        );
        assert_eq!(budget.retry_delay(&transient()), None);
    }

    #[test]
    fn successful_accept_resets_error_budget_and_permanent_errors_surface_immediately() {
        let mut budget = AcceptErrorBudget::default();
        let transient = io::Error::from(io::ErrorKind::Interrupted);
        assert_eq!(
            budget.retry_delay(&transient),
            Some(INITIAL_ACCEPT_RETRY_DELAY)
        );
        budget.accepted();
        assert_eq!(
            budget.retry_delay(&transient),
            Some(INITIAL_ACCEPT_RETRY_DELAY)
        );
        assert_eq!(
            budget.retry_delay(&io::Error::from(io::ErrorKind::InvalidInput)),
            None
        );
    }

    #[tokio::test]
    async fn unix_peer_credentials_are_read_from_the_connected_socket() {
        let (left, _right) = std::os::unix::net::UnixStream::pair().unwrap();
        left.set_nonblocking(true).unwrap();
        let stream = tokio::net::UnixStream::from_std(left).unwrap();

        assert_eq!(
            unix_peer_credentials(&stream).unwrap(),
            (unsafe { libc::geteuid() }, unsafe { libc::getegid() })
        );
    }

    #[tokio::test]
    async fn tcp_and_authorized_unix_listeners_keep_their_assigned_surfaces() {
        use axum::routing::get;
        use std::io::{Read, Write};

        let root = tempfile::tempdir().unwrap();
        let socket_path = root.path().join("control.sock");
        let unix_listener = tokio::net::UnixListener::bind(&socket_path).unwrap();
        let expected = ExpectedControlPeer {
            uid: unsafe { libc::geteuid() },
            primary_gid: unsafe { libc::getegid() },
        };
        let (authorized_listener, failure) = AuthorizedUnixListener::new(unix_listener, expected);
        let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let tcp_address = tcp_listener.local_addr().unwrap();
        let server = tokio::spawn(serve_host_surfaces(
            tcp_listener,
            Router::new().route("/surface", get(|| async { "LAN" })),
            authorized_listener,
            failure,
            Router::new().route("/surface", get(|| async { "LocalControl" })),
        ));

        let lan = reqwest::get(format!("http://{tcp_address}/surface"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let local = tokio::task::spawn_blocking(move || {
            let mut stream = std::os::unix::net::UnixStream::connect(socket_path).unwrap();
            stream
                .write_all(b"GET /surface HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response.split("\r\n\r\n").nth(1).unwrap().to_owned()
        })
        .await
        .unwrap();

        assert_eq!(lan, "LAN");
        assert_eq!(local, "LocalControl");
        server.abort();
    }

    #[tokio::test]
    async fn local_listener_exit_is_process_visible_to_the_supervisor() {
        let (name, result) = first_server_exit(
            std::future::pending::<Result<(), &'static str>>(),
            std::future::ready(Err("local failed")),
        )
        .await;

        assert_eq!(name, "local control");
        assert_eq!(result, Err("local failed"));
    }

    #[test]
    fn private_state_root_uses_explicit_then_xdg_state_then_home() {
        assert_eq!(
            resolve_private_state_root(
                Some("/private".into()),
                Some("/state".into()),
                Some("/home/test".into()),
            ),
            PathBuf::from("/private"),
        );
        assert_eq!(
            resolve_private_state_root(None, Some("/state".into()), Some("/home/test".into())),
            PathBuf::from("/state/korri"),
        );
        assert_eq!(
            resolve_private_state_root(None, None, Some("/home/test".into())),
            PathBuf::from("/home/test/.local/state/korri"),
        );
    }
}
