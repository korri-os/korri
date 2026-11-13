// SPDX-License-Identifier: GPL-3.0-or-later
//! A socket and one runtime, owned by one launch. No plugin or compositor effects.
use anyhow::{Context, Result, bail, ensure};
use korri_viewer_treaty::{
    ViewerCommand as C, ViewerDisplay, ViewerEvent as E, ViewerMessage as M, ViewerReply as R,
    ViewerRequest, ViewerShape, ViewerSnapshot, ViewerStatus as S, read_message, write_message,
};
use std::{
    io::BufReader,
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
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
struct Inner {
    launch: String,
    path: PathBuf,
    inode: u64,
    state: Mutex<State>,
    changed: Condvar,
}
impl Drop for Inner {
    fn drop(&mut self) {
        // Never remove a path another process replaced.
        if std::fs::symlink_metadata(&self.path).is_ok_and(|m| m.ino() == self.inode) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[derive(Clone)]
pub struct Lifecycle(Arc<Inner>);
impl Lifecycle {
    pub fn bind(path: &Path, launch: &str) -> Result<Self> {
        ensure!(!launch.is_empty(), "empty launch identity");
        // bind fails on any existing path, including a stale socket. Its owner
        // must remove it; guessing that another launch is stale is unsafe.
        let listener = UnixListener::bind(path).context("bind viewer lifecycle socket")?;
        let inode = std::fs::symlink_metadata(path)?.ino();
        let inner = Arc::new(Inner {
            launch: launch.into(),
            path: path.into(),
            inode,
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        });
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let weak = Arc::downgrade(&inner);
        std::thread::Builder::new()
            .name("viewer-lifecycle".into())
            .spawn(move || serve(listener, weak))?;
        Ok(Self(inner))
    }

    /// The CLI supplies native settings and credentials; Start supplies only
    /// LinuxStreamViewer.start's address and optional dimensions.
    pub fn wait_start(&self) -> Result<Start> {
        let state = self.0.state.lock().unwrap();
        let (mut state, _) = self
            .0
            .changed
            .wait_timeout_while(state, DEADLINE, |s| s.start.is_none() && !s.stop)
            .unwrap();
        state
            .start
            .take()
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
        emit(&mut state, E::Status(S::Running));
        let snapshot = state.snapshot.clone();
        emit(&mut state, E::Snapshot(snapshot));
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
        emit(&mut state, E::Snapshot(snapshot));
        self.0.changed.notify_all();
    }
    pub fn stop_requested(&self) -> bool {
        self.0.state.lock().unwrap().stop
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
        emit(&mut state, E::Status(S::Stopped));
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
fn emit(state: &mut State, event: E) {
    if let Some(writer) = &mut state.writer
        && write_message(writer, &M::Event(event)).is_err()
    {
        state.stop = true;
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
        match listener.accept() {
            Ok((socket, _)) => {
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
    inner.state.lock().unwrap().writer = Some(socket.try_clone()?);
    let mut reader = BufReader::new(socket);
    while let Some(request) = read_message::<ViewerRequest>(&mut reader)? {
        let reply = if request.launch_id != inner.launch {
            R::Error {
                message: "viewer launch identity does not match".into(),
            }
        } else {
            handle(inner, request.command).unwrap_or_else(|e| R::Error {
                message: e.to_string(),
            })
        };
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
fn handle(inner: &Inner, command: C) -> Result<R> {
    let mut state = inner.state.lock().unwrap();
    match command {
        C::Start { address, display } => {
            ensure!(
                !state.started && !state.finished,
                "viewer launch already started"
            );
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
            emit(&mut state, E::Status(status));
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
        args.len() >= 5 && args[2] == "--launch-id",
        "expected --lifecycle-socket PATH --launch-id ID stream ..."
    );
    let lifecycle = Lifecycle::bind(Path::new(&args[1]), &args[3])?;
    args.drain(..4);
    Ok(Some(lifecycle))
}
