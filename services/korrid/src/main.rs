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

fn brain_router() -> Router {
    let capability = std::env::var("KORRID_RPC_CAPABILITY")
        .expect("KORRID_RPC_CAPABILITY must be set for the brain server");
    let allowed_origin = std::env::var("KORRID_PORTAL_ORIGIN")
        .unwrap_or_else(|_| "https://appassets.androidplatform.net".into());
    korrid::router_with_capability_and_roots(
        &capability,
        &allowed_origin,
        std::env::var_os("KORRI_LOCAL_STORAGE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("korri")),
        private_state_root(),
    )
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
    korrid::relay::RelayList::from_linux_environment(
        std::env::var("KORRID_RELAYS").ok().as_deref(),
    )
    .unwrap_or_else(|error| panic!("invalid relay configuration: {error}"));
    let mode_value = std::env::var("KORRID_MODE").ok();
    let mode = Mode::parse(mode_value.as_deref()).unwrap_or_else(|error| panic!("{error}"));
    let (lan_router, local_control_router) = match mode {
        Mode::Brain => (brain_router(), None),
        Mode::Host => {
            let (lan, local) = korrid::host_routers_with_storage_and_private(
                host_config_path(),
                Some(host_storage_root()),
                private_state_root(),
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

    if let Some(local_router) = local_control_router {
        if let Some(listener) =
            inherited_control_listener().unwrap_or_else(|error| panic!("{error}"))
        {
            let listener = tokio::net::UnixListener::from_std(listener)
                .expect("adopt inherited local control listener");
            let expected = ExpectedControlPeer::from_environment()
                .unwrap_or_else(|error| panic!("invalid local control peer identity: {error}"));
            let (local_listener, local_failure) = AuthorizedUnixListener::new(listener, expected);
            let (name, result) = serve_host_surfaces(
                lan_listener,
                lan_router,
                local_listener,
                local_failure,
                local_router,
            )
            .await;
            result.unwrap_or_else(|error| panic!("serve {name} korrid: {error}"));
            panic!("{name} korrid server exited unexpectedly");
        }
    }
    axum::serve(lan_listener, lan_router)
        .await
        .expect("serve korrid");
}

#[cfg(test)]
mod tests {
    use super::*;

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
