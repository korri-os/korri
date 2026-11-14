// SPDX-License-Identifier: GPL-3.0-or-later
//! One runtime, owned by one launch, and its controller: korrid through a
//! socket on Linux (ADR 0060), or a controller in the viewer's own process on
//! Android. No plugin or compositor effects.
use anyhow::{Context, Result, bail, ensure};
use korri_viewer_treaty::{
    ViewerCommand as C, ViewerDisplay, ViewerEvent as E, ViewerMessage as M, ViewerReply as R,
    ViewerRequest, ViewerShape, ViewerSnapshot, ViewerStatus as S, read_message, write_message,
};
mod access;
mod termination;

use std::{
    io::BufReader,
    net::Shutdown,
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
    sync::{Arc, Condvar, Mutex, Weak},
    time::Duration,
};

const DEADLINE: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct Start {
    pub address: String,
    pub display: Option<ViewerDisplay>,
}

impl Start {
    /// Same endpoint grammar as korrid's existing Moonlight server_url producer.
    pub fn configure(&self, options: &mut crate::options::Options) -> Result<()> {
        let url = url::Url::parse(&format!("http://{}/serverinfo", self.address))?;
        ensure!(
            url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/serverinfo"
                && url.query().is_none()
                && url.fragment().is_none(),
            "invalid viewer address"
        );
        options.host = match url.host().context("missing viewer host")? {
            url::Host::Ipv6(ip) => ip.to_string(),
            host => host.to_string(),
        };
        let explicit = self
            .address
            .rsplit_once(':')
            .is_some_and(|(_, port)| port.parse::<u16>().is_ok());
        options.port = if explicit {
            url.port_or_known_default().context("missing viewer port")?
        } else {
            47989
        };
        ensure!(options.port > 0, "invalid viewer port");
        Ok(())
    }
}

/// A stop (Stop, a lost controller or a termination signal) ended the launch
/// before its stream ran. It is the requested end, not a failure.
#[derive(Debug)]
pub struct Stopping;
impl std::fmt::Display for Stopping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("viewer is stopping")
    }
}
impl std::error::Error for Stopping {}

#[derive(Default)]
struct State {
    start: Option<Start>,
    started: bool,
    screen: Option<ViewerDisplay>,
    status: Option<S>,
    snapshot: ViewerSnapshot,
    stop: bool,
    finished: bool,
    failure: Option<String>,
    writer: Option<UnixStream>,
}
/// Where requests come from and where events go.
enum Controller {
    /// korrid in its own process and UID, through the socket that `bind` made.
    Socket { paths: access::SocketPath, uid: u32 },
    /// A controller in this process. It calls `Lifecycle::request`, and each
    /// event goes to its function, under the runtime's lock.
    InProcess(Box<dyn Fn(&E) + Send + Sync>),
}
struct Inner {
    launch: String,
    controller: Controller,
    state: Mutex<State>,
    changed: Condvar,
}
impl Inner {
    /// The socket and its directory go; a controller in this process has neither.
    fn remove_socket(&self) {
        if let Controller::Socket { paths, .. } = &self.controller {
            paths.remove();
        }
    }
    fn emit(&self, state: &mut State, event: E) {
        match &self.controller {
            Controller::InProcess(events) => events(&event),
            Controller::Socket { .. } => {
                if let Some(writer) = &mut state.writer
                    && write_message(writer, &M::Event(event)).is_err()
                {
                    state.stop = true;
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct Lifecycle(Arc<Inner>);
impl Lifecycle {
    pub fn bind(path: &Path, launch: &str, controller_uid: u32) -> Result<Self> {
        ensure!(!launch.is_empty(), "empty launch identity");
        let (listener, paths) = access::SocketPath::bind(path, controller_uid)?;
        let inner = Arc::new(Inner {
            launch: launch.into(),
            controller: Controller::Socket {
                paths,
                uid: controller_uid,
            },
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        });
        listener.set_nonblocking(true)?;
        let weak = Arc::downgrade(&inner);
        std::thread::Builder::new()
            .name("viewer-lifecycle".into())
            .spawn(move || serve(listener, weak))?;
        Ok(Self(inner))
    }

    /// The viewer inside its controller's process, as on Android (ADR 0060):
    /// the same runtime and replies, with no socket, UID check or signal
    /// handling. Each event goes to `events`, which must not call back into
    /// this lifecycle.
    pub fn in_process(launch: &str, events: impl Fn(&E) + Send + Sync + 'static) -> Result<Self> {
        ensure!(!launch.is_empty(), "empty launch identity");
        Ok(Self(Arc::new(Inner {
            launch: launch.into(),
            controller: Controller::InProcess(Box::new(events)),
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        })))
    }

    /// One request from a controller in this process, answered as the socket
    /// answers it. It blocks as the socket's reply does: Start until the
    /// stream runs or fails, Stop until the runtime has stopped.
    pub fn request(&self, request: ViewerRequest) -> R {
        if !matches!(self.0.controller, Controller::InProcess(_)) {
            return R::Error {
                message: "this viewer is controlled through its socket".into(),
            };
        }
        reply(&self.0, request)
    }

    /// The CLI supplies native settings and credentials; Start supplies only
    /// LinuxStreamViewer.start's address and optional dimensions. `None`: a
    /// stop came first (Stop, a lost controller or a termination signal).
    pub fn wait_start(&self) -> Result<Option<Start>> {
        let state = self.0.state.lock().unwrap();
        let (mut state, _) = self
            .0
            .changed
            .wait_timeout_while(state, DEADLINE, |s| s.start.is_none() && !s.stop)
            .unwrap();
        if state.stop {
            return Ok(None);
        }
        state
            .start
            .take()
            .map(Some)
            .context("viewer Start was not received before deadline")
    }

    /// Called only from the native connectionStarted callback, like patch 0023.
    pub fn connected(&self, display: ViewerDisplay) -> Result<()> {
        ensure!(
            display.width > 0 && display.height > 0,
            "invalid connected dimensions"
        );
        let mut now: libc::timespec = unsafe { std::mem::zeroed() };
        ensure!(
            unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) } == 0,
            "read connection clock"
        );
        let mut state = self.0.state.lock().unwrap();
        state.snapshot = ViewerSnapshot {
            connected: Some(true),
            shape: state.screen.clone().map(|screen| ViewerShape {
                // Same PID + CLOCK_MONOTONIC identity producer as patch 0023.
                connection: format!("{}:{}:{}", std::process::id(), now.tv_sec, now.tv_nsec),
                screen,
                encoded: display,
                ready: false,
            }),
            ..Default::default()
        };
        state.status = Some(S::Running);
        self.0.emit(&mut state, E::Status(S::Running));
        let snapshot = state.snapshot.clone();
        self.0.emit(&mut state, E::Snapshot(snapshot));
        self.0.changed.notify_all();
        Ok(())
    }

    pub fn disconnected(&self) {
        let mut state = self.0.state.lock().unwrap();
        state.snapshot = ViewerSnapshot {
            connected: Some(false),
            ..Default::default()
        };
        let snapshot = state.snapshot.clone();
        self.0.emit(&mut state, E::Snapshot(snapshot));
        self.0.changed.notify_all();
    }
    pub fn stop_requested(&self) -> bool {
        self.0.state.lock().unwrap().stop
    }
    /// Between the connection steps after Start: `Stopping` once a stop is
    /// requested, so that no later step runs.
    pub fn ensure_running(&self) -> Result<()> {
        if self.stop_requested() {
            Err(Stopping.into())
        } else {
            Ok(())
        }
    }
    /// A termination signal stops the launch as Stop does. The socket and its
    /// directory go at once, by descriptor, while the runtime may still be
    /// busy with Start; Stopped follows its teardown as usual.
    fn terminate(&self) {
        let mut state = self.0.state.lock().unwrap();
        // Under the lock, as in finish: one removal at a time.
        self.0.remove_socket();
        state.stop = true;
        self.0.changed.notify_all();
    }

    /// Publish Stopped only after the network core and picture threads stop.
    pub fn stopped(&self) {
        self.finish(None);
    }
    pub fn failed(&self, message: String) {
        self.finish(Some(message));
    }
    fn finish(&self, failure: Option<String>) {
        self.disconnected();
        let mut state = self.0.state.lock().unwrap();
        state.failure = failure;
        state.status = Some(S::Stopped);
        state.finished = true;
        self.0.emit(&mut state, E::Status(S::Stopped));
        // The reader may be idle or hold an incomplete JSON line. Wake it
        // without closing the write half needed for an in-flight terminal reply.
        if let Some(writer) = &state.writer {
            let _ = writer.shutdown(Shutdown::Read);
        }
        self.0.remove_socket();
        self.0.changed.notify_all();
    }
    /// Let the socket thread deliver the terminal reply before process exit.
    pub fn drain(&self) {
        let state = self.0.state.lock().unwrap();
        let _ = self
            .0
            .changed
            .wait_timeout_while(state, Duration::from_secs(2), |s| s.writer.is_some());
    }
}
fn serve(listener: UnixListener, weak: Weak<Inner>) {
    loop {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        if inner.state.lock().unwrap().finished {
            return;
        }
        let Controller::Socket { uid, .. } = inner.controller else {
            return;
        };
        match listener.accept() {
            Ok((socket, _)) => {
                if let Err(error) = access::check_peer(&socket, uid) {
                    eprintln!("korri-viewer: refused lifecycle peer {error}");
                    continue;
                }
                let _ = connection(&inner, socket);
                let mut state = inner.state.lock().unwrap();
                state.writer = None;
                // A lost controlling korrid must not orphan a running stream.
                state.stop = true;
                inner.changed.notify_all();
                return;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(_) => return,
        }
        drop(inner);
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn connection(inner: &Inner, socket: UnixStream) -> Result<()> {
    socket.set_write_timeout(Some(Duration::from_secs(2)))?;
    {
        let mut state = inner.state.lock().unwrap();
        ensure!(!state.finished, "viewer already stopped");
        state.writer = Some(socket.try_clone()?);
    }
    let mut reader = BufReader::new(socket);
    while let Some(request) = read_message::<ViewerRequest>(&mut reader)? {
        let reply = reply(inner, request);
        let mut state = inner.state.lock().unwrap();
        write_message(
            state.writer.as_mut().context("viewer socket closed")?,
            &M::Reply(reply),
        )?;
        if state.finished {
            break;
        }
    }
    Ok(())
}
/// Every request, from either controller, for this launch only.
fn reply(inner: &Inner, request: ViewerRequest) -> R {
    if request.launch_id != inner.launch {
        return R::Error {
            message: "viewer launch identity does not match".into(),
        };
    }
    handle(inner, request.command).unwrap_or_else(|e| R::Error {
        message: e.to_string(),
    })
}
fn handle(inner: &Inner, command: C) -> Result<R> {
    let mut state = inner.state.lock().unwrap();
    match command {
        C::Start { address, display } => {
            ensure!(!state.stop && !state.finished, Stopping);
            ensure!(!state.started, "viewer launch already started");
            ensure!(
                !address.is_empty() && !address.contains('\0'),
                "invalid viewer address"
            );
            if let Some(d) = &display {
                ensure!(
                    d.width > 0 && d.height > 0 && d.width <= 16384 && d.height <= 16384,
                    "invalid viewer dimensions"
                );
            }
            state.screen = display.clone();
            state.started = true;
            state.start = Some(Start { address, display });
            inner.changed.notify_all();
            let (next, timed) = inner
                .changed
                .wait_timeout_while(state, DEADLINE, |s| s.status.is_none())
                .unwrap();
            state = next;
            if timed.timed_out() {
                state.stop = true;
                bail!("viewer connection deadline exceeded");
            }
            if let Some(message) = &state.failure {
                bail!("{message}");
            }
            ensure!(
                state.status == Some(S::Running),
                "viewer disconnected before Start completed"
            );
            Ok(R::Ok)
        }
        C::Status => Ok(R::Status(state.status.context("viewer is not connected")?)),
        C::Snapshot => Ok(R::Snapshot {
            snapshot: state.status.map(|_| state.snapshot.clone()),
        }),
        C::Background | C::ResumeAndFocus => {
            ensure!(
                matches!(state.status, Some(S::Running | S::Backgrounded)),
                "viewer is not running"
            );
            state.status = Some(if matches!(command, C::Background) {
                S::Backgrounded
            } else {
                S::Running
            });
            let status = state.status.unwrap();
            inner.emit(&mut state, E::Status(status));
            Ok(R::Ok)
        }
        C::Stop => {
            state.stop = true;
            inner.changed.notify_all();
            let (state, timed) = inner
                .changed
                .wait_timeout_while(state, DEADLINE, |s| !s.finished)
                .unwrap();
            ensure!(
                !timed.timed_out() && state.finished,
                "viewer stop was not confirmed"
            );
            Ok(R::Ok)
        }
        C::Resize { .. } => bail!("Live resolution is unavailable"),
        C::Apply { .. } => bail!("Local viewer control unavailable"),
        C::Notice { .. } => bail!("Viewer notice unavailable"),
    }
}

/// Opt-in process arguments. Legacy Moonlight-compatible CLI remains unchanged.
pub fn from_args(args: &mut Vec<String>) -> Result<Option<Lifecycle>> {
    if args.first().map(String::as_str) != Some("--lifecycle-socket") {
        return Ok(None);
    }
    ensure!(
        args.len() >= 7 && args[2] == "--launch-id" && args[4] == "--controller-uid",
        "expected --lifecycle-socket PATH --launch-id ID --controller-uid UID stream ..."
    );
    let controller_uid = args[5].parse::<u32>().context("invalid --controller-uid")?;
    // Before the socket exists and before any thread starts. From here a
    // termination signal cannot end the process past the socket's removal.
    let signals = termination::Held::block()?;
    let lifecycle = Lifecycle::bind(Path::new(&args[1]), &args[3], controller_uid)?;
    let weak = Arc::downgrade(&lifecycle.0);
    signals.receive(move || {
        if let Some(inner) = weak.upgrade() {
            Lifecycle(inner).terminate();
        }
    })?;
    args.drain(..6);
    Ok(Some(lifecycle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn request(socket: &mut BufReader<UnixStream>, command: C) -> R {
        let launch_id = "a".into();
        write_message(socket.get_mut(), &ViewerRequest { launch_id, command }).unwrap();
        loop {
            if let M::Reply(reply) = read_message(socket).unwrap().unwrap() {
                return reply;
            }
        }
    }

    // #468 review: a Start that arrived after a stop request was admitted,
    // then waited out the connection deadline, or, once the runtime had
    // finished, was told that the launch had already started.
    #[test]
    fn start_after_a_stop_request_is_refused_at_once_as_stopping() {
        let root = std::fs::metadata("/").unwrap().uid();
        if root != 0 && root != unsafe { libc::geteuid() } {
            return; // The Nix build sandbox: the viewer refuses every location.
        }
        let dir = match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(runtime) => tempfile::tempdir_in(runtime).unwrap(),
            None => tempfile::tempdir().unwrap(),
        };
        let path = dir.path().join("control/viewer.sock");
        let lifecycle = Lifecycle::bind(&path, "a", unsafe { libc::geteuid() }).unwrap();
        let socket = UnixStream::connect(&path).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut socket = BufReader::new(socket);
        assert!(matches!(request(&mut socket, C::Status), R::Error { .. }));

        lifecycle.terminate();
        let asked = std::time::Instant::now();
        let reply = request(
            &mut socket,
            C::Start {
                address: "127.0.0.1".into(),
                display: None,
            },
        );

        assert!(
            matches!(&reply, R::Error { message } if message == "viewer is stopping"),
            "{reply:?}"
        );
        assert!(asked.elapsed() < Duration::from_secs(1));
        lifecycle.stopped();
        lifecycle.drain();
    }
}
