// SPDX-License-Identifier: GPL-3.0-or-later
//! One runtime, owned by one launch, and its controller: korrid through a
//! socket on Linux (ADR 0060), or a controller in the viewer's own process on
//! Android. No plugin or compositor effects.
//!
//! On Android the stream's controls and status (#434, slice D11) go through
//! the same treaty: the thread that owns the stream attaches them
//! (`Lifecycle::controls`); Snapshot lists them with the readings while the
//! stream is connected, and Apply hands each change to that thread and
//! answers once it has performed it. With none attached, as on Linux,
//! Snapshot has no controls, battery or health and Apply is unavailable.
use crate::controls::{self, Change, Readings, Values};
use anyhow::{Context, Result, anyhow, bail, ensure};
use korri_viewer_treaty::{
    SurfaceGameplayControlValue, ViewerCommand as C, ViewerDisplay, ViewerEvent as E,
    ViewerMessage as M, ViewerReply as R, ViewerRequest, ViewerShape, ViewerSnapshot,
    ViewerStatus as S, read_message, write_message,
};
mod access;
mod termination;

use std::{
    io::BufReader,
    net::Shutdown,
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    time::Duration,
};

const DEADLINE: Duration = Duration::from_secs(30);
/// How long Apply waits for the stream's thread: Artemis waits 5 s for its
/// UI thread (`KorriStreamViewer.withGame`).
const APPLY_DEADLINE: Duration = Duration::from_secs(5);

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
    controls: Option<Attached>,
}

/// The stream's controls, from `Lifecycle::controls` until the runtime
/// stops.
struct Attached {
    prefix: String,
    /// What the stream's thread has performed: it writes them when it
    /// confirms a change, also after Apply stopped waiting.
    values: Arc<Mutex<Values>>,
    readings: Arc<dyn Fn() -> Readings + Send + Sync>,
    changes: mpsc::Sender<ControlChange>,
}

/// One change that Apply accepted, for the thread that owns the stream.
pub struct ControlChange {
    pub change: Change,
    values: Arc<Mutex<Values>>,
    answer: mpsc::Sender<Result<(), String>>,
}

impl ControlChange {
    /// The stream's thread has performed the change, or could not: Apply
    /// answers with it. A change dropped unanswered fails its Apply.
    pub fn done(self, result: Result<(), String>) {
        if result.is_ok() {
            let mut values = self.values.lock().unwrap_or_else(PoisonError::into_inner);
            *values = values.with(self.change);
        }
        let _ = self.answer.send(result);
    }
}

/// The changes that Apply accepted, in order, for the thread that owns the
/// stream. Dropping it makes every later Apply fail at once.
pub struct ControlChanges(mpsc::Receiver<ControlChange>);

impl ControlChanges {
    /// The next change, without waiting.
    pub fn next(&self) -> Option<ControlChange> {
        self.0.try_recv().ok()
    }
}

/// Each runtime's number in this process: a control of an earlier runtime
/// of the same launch does not match a later one's.
static GENERATIONS: AtomicU64 = AtomicU64::new(1);

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
    generation: u64,
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
            generation: GENERATIONS.fetch_add(1, Ordering::Relaxed),
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
            generation: GENERATIONS.fetch_add(1, Ordering::Relaxed),
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

    /// The stream's controls (slice D11), until the runtime stops. While
    /// the stream is connected, Snapshot lists them with `readings`; Apply
    /// sends each change that it accepts to the queue that this returns, and
    /// answers once the stream's thread has performed it. `lease` names the
    /// activity that shows the stream, as Artemis's lease does. The values
    /// start at Artemis's defaults. `readings` runs on the thread that asks
    /// for the snapshot, outside the lifecycle's lock.
    pub fn controls(
        &self,
        lease: u64,
        readings: impl Fn() -> Readings + Send + Sync + 'static,
    ) -> ControlChanges {
        let (changes, queue) = mpsc::channel();
        self.0.state.lock().unwrap().controls = Some(Attached {
            prefix: controls::prefix(&self.0.launch, self.0.generation, lease),
            values: Arc::new(Mutex::new(Values::default())),
            readings: Arc::new(readings),
            changes,
        });
        ControlChanges(queue)
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
        // No Apply goes to a stream that has stopped.
        state.controls = None;
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
            snapshot: snapshot(state),
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
        C::Apply { control, value } => apply(state, &control, value),
        C::Notice { .. } => bail!("Viewer notice unavailable"),
    }
}

/// The connection's observations and, while the stream is connected, its
/// controls and readings (`KorriViewerControls.snapshot`). None before
/// Start has a status.
fn snapshot(state: MutexGuard<'_, State>) -> Option<ViewerSnapshot> {
    state.status?;
    let mut snapshot = state.snapshot.clone();
    let Some(attached) = state
        .controls
        .as_ref()
        .filter(|_| snapshot.connected == Some(true))
    else {
        return Some(snapshot);
    };
    let values = *attached
        .values
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    snapshot.controls = controls::list(&attached.prefix, values);
    let readings = attached.readings.clone();
    // The readings call into Android: never under the lifecycle's lock.
    drop(state);
    let readings = readings();
    snapshot.battery_percent = readings.battery_percent;
    snapshot.sampled_at_ms = readings.sampled_at_ms;
    snapshot.stream_health = readings.stream_health;
    Some(snapshot)
}

/// One change for the stream's thread (`KorriViewerControls.apply`): it is
/// checked here, performed there, and confirmed within `APPLY_DEADLINE`. The
/// lifecycle's lock is not held while the stream's thread works.
fn apply(
    state: MutexGuard<'_, State>,
    id: &str,
    value: Option<SurfaceGameplayControlValue>,
) -> Result<R> {
    let Some(attached) = &state.controls else {
        bail!("Local viewer control unavailable");
    };
    ensure!(
        matches!(state.status, Some(S::Running | S::Backgrounded)),
        "viewer is not running"
    );
    let change = controls::parse(&attached.prefix, id, value.as_ref())?;
    let (answer, answered) = mpsc::channel();
    attached
        .changes
        .send(ControlChange {
            change,
            values: attached.values.clone(),
            answer,
        })
        .map_err(|_| anyhow!("the stream's controls are gone"))?;
    drop(state);
    match answered.recv_timeout(APPLY_DEADLINE) {
        Ok(Ok(())) => Ok(R::Ok),
        Ok(Err(message)) => bail!("{message}"),
        Err(RecvTimeoutError::Timeout) => {
            bail!("the stream did not confirm the control within 5 s")
        }
        Err(RecvTimeoutError::Disconnected) => {
            bail!("the stream ended before the control took effect")
        }
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

    /// An in-process lifecycle of launch "a", as Android's controller holds it.
    fn in_app() -> Lifecycle {
        Lifecycle::in_process("a", |_| {}).unwrap()
    }

    fn ask(lifecycle: &Lifecycle, command: C) -> R {
        lifecycle.request(ViewerRequest {
            launch_id: "a".into(),
            command,
        })
    }

    fn observed(lifecycle: &Lifecycle) -> ViewerSnapshot {
        match ask(lifecycle, C::Snapshot) {
            R::Snapshot {
                snapshot: Some(snapshot),
            } => snapshot,
            other => panic!("no snapshot: {other:?}"),
        }
    }

    fn connect(lifecycle: &Lifecycle) {
        lifecycle
            .connected(ViewerDisplay {
                width: 1280,
                height: 720,
            })
            .unwrap();
    }

    fn readings() -> Readings {
        Readings {
            battery_percent: Some(53.0),
            sampled_at_ms: Some(1234.0),
            stream_health: Some(korri_viewer_treaty::SurfaceStreamHealth {
                rtt_ms: Some(8.0),
                delivered_bitrate_kbps: Some(789.0),
                frames_dropped: Some(0.0),
            }),
        }
    }

    /// A control's id and toggle value, from a snapshot.
    fn toggles(snapshot: &ViewerSnapshot) -> Vec<(String, bool)> {
        snapshot
            .controls
            .iter()
            .map(|control| match &control.interaction {
                korri_viewer_treaty::SurfaceGameplayControlInteraction::Toggle {
                    value, ..
                } => (control.id.clone(), *value),
                other => panic!("not a toggle: {other:?}"),
            })
            .collect()
    }

    fn fill(snapshot: &ViewerSnapshot) -> String {
        toggles(snapshot)
            .into_iter()
            .map(|(id, _)| id)
            .find(|id| id.ends_with(":set-fill-mode"))
            .expect("the fill control")
    }

    fn turn_on(id: &str) -> C {
        C::Apply {
            control: id.into(),
            value: Some(SurfaceGameplayControlValue::Toggle { value: true }),
        }
    }

    // Linux's viewer, and Android's before its stream attaches them: the
    // replies of the trait's defaults.
    #[test]
    fn without_controls_a_snapshot_has_none_and_apply_is_unavailable() {
        let lifecycle = in_app();
        connect(&lifecycle);
        let snapshot = observed(&lifecycle);
        assert!(snapshot.controls.is_empty());
        assert!(snapshot.battery_percent.is_none() && snapshot.stream_health.is_none());
        assert!(matches!(
            ask(&lifecycle, turn_on("viewer:a:1:1:set-fill-mode")),
            R::Error { message } if message == "Local viewer control unavailable"
        ));
    }

    #[test]
    fn the_controls_and_readings_are_listed_only_while_the_stream_is_connected() {
        let lifecycle = in_app();
        let _changes = lifecycle.controls(7, readings);
        assert!(matches!(
            ask(&lifecycle, C::Snapshot),
            R::Snapshot { snapshot: None }
        ));
        connect(&lifecycle);
        let snapshot = observed(&lifecycle);
        let listed = toggles(&snapshot);
        assert_eq!(listed.len(), 2);
        let prefix = listed[0].0.strip_suffix("set-fill-mode").unwrap();
        assert!(
            prefix.starts_with("viewer:a:") && prefix.ends_with(":7:"),
            "{prefix}"
        );
        assert_eq!(
            listed,
            [
                (format!("{prefix}set-fill-mode"), false),
                (format!("{prefix}set-face-button-flip"), false)
            ]
        );
        assert_eq!(snapshot.battery_percent, Some(53.0));
        assert_eq!(snapshot.sampled_at_ms, Some(1234.0));
        assert_eq!(snapshot.stream_health, readings().stream_health);
        assert_eq!(snapshot.connected, Some(true));

        lifecycle.disconnected();
        let snapshot = observed(&lifecycle);
        assert_eq!(snapshot.connected, Some(false));
        assert!(snapshot.controls.is_empty() && snapshot.battery_percent.is_none());
    }

    #[test]
    fn apply_answers_once_the_streams_thread_performed_it() {
        let lifecycle = in_app();
        let changes = lifecycle.controls(1, Readings::default);
        connect(&lifecycle);
        let id = fill(&observed(&lifecycle));
        let stream = std::thread::spawn(move || {
            loop {
                if let Some(change) = changes.next() {
                    let asked = change.change;
                    change.done(Ok(()));
                    return asked;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        assert!(matches!(ask(&lifecycle, turn_on(&id)), R::Ok));
        assert_eq!(
            stream.join().unwrap(),
            Change {
                control: controls::Control::Fill,
                value: true
            }
        );
        assert_eq!(toggles(&observed(&lifecycle))[0], (id, true));
    }

    #[test]
    fn a_refused_stale_or_late_change_changes_nothing() {
        let lifecycle = in_app();
        let changes = lifecycle.controls(1, Readings::default);
        // Before the stream runs.
        assert!(matches!(
            ask(&lifecycle, turn_on("viewer:a:1:1:set-fill-mode")),
            R::Error { message } if message == "viewer is not running"
        ));
        connect(&lifecycle);
        let id = fill(&observed(&lifecycle));

        // The stream's thread cannot perform it.
        let stream = std::thread::spawn(move || {
            loop {
                if let Some(change) = changes.next() {
                    change.done(Err("the video layer did not move".into()));
                    return changes;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        assert!(matches!(
            ask(&lifecycle, turn_on(&id)),
            R::Error { message } if message == "the video layer did not move"
        ));
        let changes = stream.join().unwrap();

        // Another runtime's control, or a value of another kind: nothing
        // reaches the stream.
        let stale = id.replacen("viewer:a:", "viewer:a:0", 1);
        assert!(matches!(ask(&lifecycle, turn_on(&stale)), R::Error { .. }));
        let range = C::Apply {
            control: id.clone(),
            value: Some(SurfaceGameplayControlValue::Range { value: 1.0 }),
        };
        assert!(matches!(ask(&lifecycle, range), R::Error { .. }));
        assert!(changes.next().is_none());

        // The stream's queue is gone: Apply fails at once.
        drop(changes);
        assert!(matches!(ask(&lifecycle, turn_on(&id)), R::Error { .. }));
        assert_eq!(toggles(&observed(&lifecycle))[0], (id.clone(), false));

        // After the runtime stops, no Apply reaches a stream.
        lifecycle.stopped();
        assert!(matches!(
            ask(&lifecycle, turn_on(&id)),
            R::Error { message } if message == "Local viewer control unavailable"
        ));
    }

    #[test]
    fn each_runtime_of_a_launch_has_its_own_controls() {
        let first = in_app();
        let second = in_app();
        let _a = first.controls(1, Readings::default);
        let _b = second.controls(1, Readings::default);
        connect(&first);
        connect(&second);
        let id = fill(&observed(&first));
        assert_ne!(id, fill(&observed(&second)));
        assert!(matches!(ask(&second, turn_on(&id)), R::Error { .. }));
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
