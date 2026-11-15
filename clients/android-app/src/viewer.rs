// SPDX-License-Identifier: GPL-3.0-or-later
//! korrid's stream viewer in this process (#434, slice D10): the Rust viewer
//! of `clients/viewer`, run through its in-process lifecycle
//! (`Lifecycle::in_process`, ADR 0060) in the place of korrid's Java adapter
//! (`services/korrid/src/android/viewer.rs` → `KorriStreamViewer` → `Game`).
//!
//! Two halves, one launch at a time:
//!
//! | Half | Thread | Does |
//! | --- | --- | --- |
//! | [`Adapter`] | korrid's workers | korrid's `LinuxStreamViewer`: the client certificate, the server certificate through korrid's broker, the runtime's lifecycle, every treaty request (`Start`, `Status`, `Snapshot`, `Apply`, `Stop`), and korrid's Resume, which closes the gameplay overlay (slice D5b) |
//! | [`Stream`] | the shell's loop (Slint's) | the video and overlay layers, the runtime's thread, the stream's input from the shell's input filter, the stream's controls (slice D11), and the shell's gameplay overlay (slice D5b, the shell's `Overlay` host), on the shell's 16 ms timer |
//!
//! The stream's controls and status (slice D11): the loop thread attaches
//! them to the runtime's lifecycle when the stream starts
//! (`Lifecycle::controls`, with the viewer's battery and health readings).
//! korrid's `snapshot` then lists them, and its `apply` is the treaty's
//! Apply, which the loop thread performs at its next tick: "Crop to fill"
//! moves the video layer (confirmed once Android's main thread has applied
//! the transaction), "Flip A/B and X/Y" changes the controllers' mapping,
//! and "Rumble" goes to the controllers' feedback thread (slice D4b's). The
//! loop thread offers Rumble while a controller that can rumble is in the
//! stream.
//!
//! The adapter hands a launch to the loop thread over a channel, because the
//! layers and the input belong to that thread. The loop thread never waits:
//! it asks Android's main thread for the layers and reads the answer at its
//! next tick, and the treaty's blocking requests run on threads of their own.
//!
//! The certificates, as korrid provisions them for its Linux viewers:
//!
//! - The client key directory is `<no_backup>/moonlight`, Moonlight's own
//!   directory name, as Linux korrid's `/home/korri/.config/moonlight`
//!   (`linux_viewer/production.rs`). The viewer's `Identity::load_or_generate`
//!   makes it with mode 0700 and its key files with mode 0600, under
//!   Moonlight's names (`client.pem`, `key.pem`).
//! - The pinned server certificate is what korrid's broker returns
//!   (`korrid::android_app::provision_moonlight_certificate`). The runtime
//!   reads it from a file of its launch in the app's cache directory, mode
//!   0600, which goes when the launch ends: Linux korrid hands its viewer
//!   the same certificate as a credential file, never in the key directory.
//!   The adapter removes the files that an earlier process left
//!   (`rules::remove_stale_pins`).
//!
//! What the person does, by the gameplay overlay's rules of slice D5
//! (`korri_viewer::play`): Back or Guide opens the gameplay overlay and
//! holds the stream's input until it closes. Since slice D5b the overlay is
//! the shell's (`hosted.rs`): the loop thread lends the overlay layer's
//! Surface to the shell, which draws korrid's gameplay overlay into it, and
//! korrid hears that the person left (`session.leave`). The overlay's keys,
//! sticks and touches go to the shell; Back and Guide too, and the shell's
//! surface package dismisses the overlay through korrid. korrid's Resume
//! then reaches `resume_and_focus`: the layer hides, the shell lets the
//! Surface go, and the stream's input goes on. Leaving the app (Home,
//! another app) and Ctrl+Alt+Shift+Q end the runtime; korrid then sees
//! `Stopped`, and its own recovery starts the stream again while the launch
//! is selected and the app is in front.
mod hosted;
mod rules;

use crate::logcat;
use futures::{channel::oneshot, future::BoxFuture};
use hosted::{Closing, Hosted, Step};
use jni::{objects::GlobalRef, objects::JObject, JNIEnv, JavaVM};
use korri_shell::backend::android::Overlay;
use korri_viewer::{
    controls::{Control, Offer},
    devices::AndroidDevices,
    events,
    feedback::RumbleSwitch,
    feedback_jni,
    input::{self as stream_input, Settings},
    launch,
    layer::{Layers, Pending},
    lifecycle::{ControlChange, ControlChanges, Lifecycle},
    options::Options,
    overlay::{is_system_key, Presenter},
    picture::{FrameRecord, Picture},
    play::{Play, Routed},
    session::Identity,
    status,
};
use korri_viewer_treaty::{ViewerCommand, ViewerReply, ViewerRequest};
use korrid::{
    launcher::plugin_launch::PluginDisplay,
    linux_viewer::{LinuxStreamViewer, LinuxViewerError, LinuxViewerStatus, ViewerSnapshot},
};
use slint::android::{
    android_activity::{input::InputEvent, MainEvent},
    AndroidApp,
};
use std::{
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex, OnceLock, PoisonError,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// How long korrid's Resume waits for the overlay to close: Artemis's wait
/// for its activity (`KorriStreamViewer.CONTROL_TIMEOUT_MS`).
const RESUME_DEADLINE: Duration = Duration::from_secs(15);

/// What the adapter asks of the loop thread of the activity in front.
enum Order {
    Show(Box<Show>),
    Resume(Resume),
}

/// korrid's Resume of a launch: the gameplay overlay closes, then it hears
/// so; a stream that is not in front answers why not.
struct Resume {
    launch: String,
    closed: oneshot::Sender<Result<(), String>>,
}

/// One launch for the loop thread to show.
struct Show {
    launch: String,
    address: String,
    options: Options,
    pin: Pin,
    lifecycle: Lifecycle,
    /// Start's reply: korrid's `start` waits for it.
    started: oneshot::Sender<Result<(), String>>,
}

/// The launch that korrid started last, with its runtime's lifecycle.
#[derive(Clone)]
struct Current {
    launch: String,
    lifecycle: Lifecycle,
}

/// What the adapter and the loop thread share, for the life of the process:
/// korrid starts once per process and keeps the adapter it was given, while
/// each activity of the process runs its own loop (`Stream`).
struct Shared {
    keydir: PathBuf,
    /// Where each launch's pinned server certificate lives while it plays.
    pins: PathBuf,
    /// The loop thread of the activity in front, by its number.
    orders: rules::Slot<Order>,
    /// The activity has a window, so a launch can be shown.
    window: AtomicBool,
    current: Mutex<Option<Current>>,
    /// One start or stop at a time.
    operation: tokio::sync::Mutex<()>,
    changed: Arc<tokio::sync::Notify>,
    /// The status that korrid last read, so that the log names each change.
    reported: Mutex<Option<(String, LinuxViewerStatus)>>,
    /// The controls of the snapshot that korrid last read, as the treaty's
    /// JSON, so that the log names each change but not each reading.
    reported_controls: Mutex<Option<(String, String)>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    /// The lifecycle of `launch`, when it is the launch that korrid started.
    fn lifecycle(&self, launch: &str) -> Option<Lifecycle> {
        lock(&self.current)
            .as_ref()
            .filter(|current| current.launch == launch)
            .map(|current| current.lifecycle.clone())
    }

    fn report(&self, launch: &str, status: LinuxViewerStatus) {
        let mut reported = lock(&self.reported);
        if reported.as_ref() != Some(&(launch.to_owned(), status)) {
            logcat::info(&format!(
                "korri-app: viewer status for korrid: {launch} {status:?}"
            ));
            *reported = Some((launch.to_owned(), status));
        }
    }

    /// The treaty's snapshot that korrid read, in the log when its controls
    /// change. korrid's gameplay overlay asks every second; the readings
    /// change each time.
    fn report_snapshot(&self, launch: &str, snapshot: &korri_viewer_treaty::ViewerSnapshot) {
        let controls = json(&snapshot.controls);
        let mut reported = lock(&self.reported_controls);
        if reported.as_ref() != Some(&(launch.to_owned(), controls.clone())) {
            logcat::info(&format!(
                "korri-app: viewer snapshot for korrid: {launch} {}",
                json(snapshot)
            ));
            *reported = Some((launch.to_owned(), controls));
        }
    }
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
static ADAPTER: OnceLock<Arc<Adapter>> = OnceLock::new();

/// korrid's viewer in this process. The first call fixes the key directory
/// and the pins' directory, and removes the pinned certificates that an
/// earlier process left there; every call returns the same adapter, the one
/// that korrid holds (korrid starts once per process).
pub fn adapter(keydir: PathBuf, pins: PathBuf) -> Arc<Adapter> {
    ADAPTER
        .get_or_init(|| {
            let shared = SHARED.get_or_init(|| {
                match rules::remove_stale_pins(&pins) {
                    Ok(0) => {}
                    Ok(removed) => logcat::info(&format!(
                        "korri-app: viewer removed {removed} pinned certificates of an earlier process"
                    )),
                    Err(cause) => logcat::error(&format!(
                        "korri-app: viewer could not remove the pinned certificates of an earlier process: {cause}"
                    )),
                }
                Arc::new(Shared {
                    keydir,
                    pins,
                    orders: Mutex::new(None),
                    window: AtomicBool::new(false),
                    current: Mutex::new(None),
                    operation: tokio::sync::Mutex::new(()),
                    changed: Arc::new(tokio::sync::Notify::new()),
                    reported: Mutex::new(None),
                    reported_controls: Mutex::new(None),
                })
            });
            Arc::new(Adapter(shared.clone()))
        })
        .clone()
}

/// The adapter that korrid holds, once `adapter` has made it: the emulator
/// proof's trigger calls it (`proof.rs`, fixture APK only).
#[cfg(feature = "proof")]
pub fn korrids_adapter() -> Option<Arc<Adapter>> {
    ADAPTER.get().cloned()
}

pub struct Adapter(Arc<Shared>);

fn error(message: impl ToString) -> LinuxViewerError {
    LinuxViewerError::new(message.to_string())
}

/// korrid's exact launch identity: 32 lowercase hexadecimal digits.
fn exact(launch: &str) -> Result<(), LinuxViewerError> {
    if launch.len() == 32
        && launch
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err(error("invalid exact viewer launch identity"))
    }
}

/// The treaty's JSON of a request, reply or event, for the log.
fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// A command's name in the treaty (`kind`), for the log.
fn kind(command: &ViewerCommand) -> String {
    serde_json::to_value(command)
        .ok()
        .and_then(|value| value["kind"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// One treaty request on a blocking thread of korrid's runtime: Start and
/// Stop wait for the runtime.
async fn request(
    lifecycle: Lifecycle,
    request: ViewerRequest,
) -> Result<ViewerReply, LinuxViewerError> {
    let name = kind(&request.command);
    logcat::info(&format!("korri-app: viewer request {}", json(&request)));
    let reply = rules::blocking(move || lifecycle.request(request))
        .await
        .ok_or_else(|| error("viewer worker failed"))?;
    logcat::info(&format!("korri-app: viewer reply {name} {}", json(&reply)));
    Ok(reply)
}

/// The client certificate of this device's key directory, made on its first
/// use. Only the public certificate leaves this function.
fn client_certificate(keydir: &Path) -> Result<String, LinuxViewerError> {
    Identity::load_or_generate(keydir)
        .and_then(|identity| Ok(String::from_utf8(identity.cert.to_pem()?)?))
        .map_err(|cause| error(format!("the viewer's client key is unusable: {cause:#}")))
}

impl LinuxStreamViewer for Adapter {
    fn change_signal(&self) -> Option<Arc<tokio::sync::Notify>> {
        Some(self.0.changed.clone())
    }

    fn start<'a>(
        &'a self,
        launch: &'a str,
        address: &'a str,
        display: Option<PluginDisplay>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            let _operation = self.0.operation.lock().await;
            exact(launch)?;
            if let Some(current) = lock(&self.0.current).as_ref() {
                if let ViewerReply::Status(status) = current.lifecycle.request(ViewerRequest {
                    launch_id: current.launch.clone(),
                    command: ViewerCommand::Status,
                }) {
                    if LinuxViewerStatus::from(status) != LinuxViewerStatus::Stopped {
                        return Err(error("another launch plays in this app"));
                    }
                } else {
                    return Err(error("another launch is starting in this app"));
                }
            }
            if !self.0.window.load(Ordering::Acquire) {
                return Err(error(
                    "the app is not in front: it has no window to show the stream in",
                ));
            }
            let keydir = self.0.keydir.clone();
            let client = {
                let keydir = keydir.clone();
                tokio::task::spawn_blocking(move || client_certificate(&keydir))
                    .await
                    .map_err(|_| error("viewer worker failed"))??
            };
            let server =
                korrid::android_app::provision_moonlight_certificate(address, &client).await?;
            logcat::info(&format!(
                "korri-app: viewer certificate provisioned through korrid's broker for {address}"
            ));
            let pin = Pin::new(&self.0.pins, launch, &server)
                .map_err(|cause| error(format!("the server certificate has no place: {cause}")))?;
            let options = Options::for_lifecycle(
                keydir,
                display.map(|display| (display.width, display.height)),
            )
            .map_err(|cause| error(format!("{cause:#}")))?;
            let changed = self.0.changed.clone();
            let lifecycle = Lifecycle::in_process(launch, move |event| {
                logcat::info(&format!("korri-app: viewer event {}", json(event)));
                changed.notify_one();
            })
            .map_err(|cause| error(format!("{cause:#}")))?;
            *lock(&self.0.current) = Some(Current {
                launch: launch.to_owned(),
                lifecycle: lifecycle.clone(),
            });
            let (started, answer) = oneshot::channel();
            let show = Show {
                launch: launch.to_owned(),
                address: address.to_owned(),
                options,
                pin,
                lifecycle: lifecycle.clone(),
                started,
            };
            let sent = lock(&self.0.orders)
                .as_ref()
                .map(|(_, orders)| orders.send(Order::Show(Box::new(show))).is_ok())
                .unwrap_or(false);
            if !sent {
                lifecycle.failed("the app has no activity to show the stream in".into());
                return Err(error("the app has no activity to show the stream in"));
            }
            match answer.await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(message)) => Err(error(message)),
                Err(_) => Err(error("the stream ended before it started")),
            }
        })
    }

    fn status<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<LinuxViewerStatus, LinuxViewerError>> {
        Box::pin(async move {
            exact(launch)?;
            let status = match self.0.lifecycle(launch) {
                None => LinuxViewerStatus::Stopped,
                Some(lifecycle) => match lifecycle.request(ViewerRequest {
                    launch_id: launch.to_owned(),
                    command: ViewerCommand::Status,
                }) {
                    ViewerReply::Status(status) => status.into(),
                    ViewerReply::Error { message } => return Err(error(message)),
                    _ => return Err(error("unexpected viewer status reply")),
                },
            };
            self.0.report(launch, status);
            Ok(status)
        })
    }

    fn snapshot<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<Option<ViewerSnapshot>, LinuxViewerError>> {
        Box::pin(async move {
            exact(launch)?;
            let Some(lifecycle) = self.0.lifecycle(launch) else {
                return Ok(Some(ViewerSnapshot {
                    connected: Some(false),
                    ..Default::default()
                }));
            };
            // The readings go into Android (the battery through JNI) and take
            // the picture's lock: not on korrid's async worker.
            let request = ViewerRequest {
                launch_id: launch.to_owned(),
                command: ViewerCommand::Snapshot,
            };
            let reply = rules::blocking(move || lifecycle.request(request))
                .await
                .ok_or_else(|| error("viewer worker failed"))?;
            match reply {
                ViewerReply::Snapshot { snapshot } => Ok(snapshot.map(|snapshot| {
                    self.0.report_snapshot(launch, &snapshot);
                    let mut snapshot = ViewerSnapshot::from(snapshot);
                    // The viewer samples its health on this process's
                    // CLOCK_MONOTONIC, as Artemis on SystemClock.uptimeMillis.
                    snapshot.date_sample(
                        korrid::linux_viewer::monotonic_ms(),
                        std::time::Instant::now(),
                    );
                    snapshot
                })),
                ViewerReply::Error { message } => Err(error(message)),
                _ => Err(error("unexpected viewer snapshot reply")),
            }
        })
    }

    /// The treaty's Apply: it returns once the stream's loop thread has
    /// performed the change, or with why it did not (`KorriViewerControls.apply`).
    fn apply<'a>(
        &'a self,
        launch: &'a str,
        control: &'a str,
        value: Option<korri_viewer_treaty::SurfaceGameplayControlValue>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            exact(launch)?;
            let Some(lifecycle) = self.0.lifecycle(launch) else {
                return Err(error("viewer is not running"));
            };
            match request(
                lifecycle,
                ViewerRequest {
                    launch_id: launch.to_owned(),
                    command: ViewerCommand::Apply {
                        control: control.to_owned(),
                        value,
                    },
                },
            )
            .await?
            {
                ViewerReply::Ok => Ok(()),
                ViewerReply::Error { message } => Err(error(message)),
                _ => Err(error("unexpected viewer apply reply")),
            }
        })
    }

    /// The treaty's Background is a confirmed loss of focus: korrid's "Open
    /// Korri", for which Artemis ends its activity and keeps the launch to
    /// resume. This app does not take the stream out of the front yet, so
    /// there is no effect to confirm.
    fn background<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            exact(launch)?;
            Err(error("the Android viewer cannot leave the front yet"))
        })
    }

    /// korrid's Resume of a stream that runs (`KorriStreamViewer.resume`):
    /// the gameplay overlay closes, and the answer comes once the shell has
    /// let the overlay layer's Surface go and the stream's input goes on. A
    /// stream with no overlay open answers at once.
    fn resume_and_focus<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            exact(launch)?;
            if self.0.lifecycle(launch).is_none() {
                return Err(error("viewer is not running"));
            }
            let (closed, answer) = oneshot::channel();
            let resume = Resume {
                launch: launch.to_owned(),
                closed,
            };
            let sent = lock(&self.0.orders)
                .as_ref()
                .map(|(_, orders)| orders.send(Order::Resume(resume)).is_ok())
                .unwrap_or(false);
            if !sent {
                return Err(error("the app has no activity that shows the stream"));
            }
            let answered = match tokio::time::timeout(RESUME_DEADLINE, answer).await {
                Ok(Ok(answered)) => answered,
                Ok(Err(_)) => Err("the activity ended before the overlay closed".to_owned()),
                Err(_) => Err("the gameplay overlay did not close within 15 s".to_owned()),
            };
            match &answered {
                Ok(()) => logcat::info(&format!(
                    "korri-app: viewer resume for korrid: {launch}: the stream is in front, its input goes on"
                )),
                Err(why) => logcat::error(&format!(
                    "korri-app: viewer resume for korrid: {launch} failed: {why}"
                )),
            }
            answered.map_err(error)
        })
    }

    fn stop<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            let _operation = self.0.operation.lock().await;
            exact(launch)?;
            let Some(lifecycle) = self.0.lifecycle(launch) else {
                return Ok(());
            };
            match request(
                lifecycle,
                ViewerRequest {
                    launch_id: launch.to_owned(),
                    command: ViewerCommand::Stop,
                },
            )
            .await?
            {
                ViewerReply::Ok => Ok(()),
                ViewerReply::Error { message } => Err(error(message)),
                _ => Err(error("unexpected viewer stop reply")),
            }
        })
    }
}

/// The authorized server certificate as the runtime reads it
/// (`KORRI_MOONLIGHT_SERVER_CERTIFICATE`): a new file of this launch, mode
/// 0600, in the app's cache directory. It goes when the launch ends.
struct Pin(PathBuf);

impl Pin {
    fn new(directory: &Path, launch: &str, pem: &str) -> io::Result<Self> {
        // A restart of the same launch (korrid's recovery) gets a new name.
        static STARTS: AtomicU64 = AtomicU64::new(1);
        let start = STARTS.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(rules::pin_name(launch, start));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let pin = Self(path);
        file.write_all(pem.as_bytes())?;
        file.sync_all()?;
        Ok(pin)
    }

    fn path(&self) -> PathBuf {
        self.0.clone()
    }
}

impl Drop for Pin {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A stream's place on the loop thread.
enum Phase {
    Idle,
    /// Android's main thread makes the layers.
    Making(Box<Making>),
    Playing(Box<Playing>),
}

struct Making {
    show: Show,
    pending: Pending,
}

struct Playing {
    launch: String,
    lifecycle: Lifecycle,
    layers: Layers,
    play: Play,
    runtime: JoinHandle<anyhow::Result<()>>,
    /// A stop was asked for: nothing more goes to the stream.
    stopping: bool,
    /// The decoded picture's newest size, from the decoder's output thread.
    decoded: Decoded,
    /// The changes that korrid's Apply asks for (slice D11).
    changes: ControlChanges,
    /// The control "Rumble": the controllers' feedback thread takes it.
    rumble: RumbleSwitch,
    /// "Crop to fill" waits for Android's main thread to move the layer.
    placing: Option<(ControlChange, mpsc::Receiver<anyhow::Result<()>>)>,
    /// The shell's gameplay overlay over this stream (slice D5b).
    hosted: Hosted,
    /// Where each opening's leave thread answers, with the opening's
    /// number.
    answers: mpsc::Sender<(u64, bool)>,
    /// korrid's answers to the openings' leaves.
    left: mpsc::Receiver<(u64, bool)>,
    /// korrid's resumes that wait for the overlay to close.
    resumes: Vec<oneshot::Sender<Result<(), String>>>,
    /// The runtime reads it when it connects; it goes with the launch.
    _pin: Pin,
}

/// The decoded picture's newest size: the decoder's output thread stores
/// it (`Picture.resized`) and the next tick of the shell's 16 ms timer
/// takes it, so the decoder never waits for the loop thread.
#[derive(Clone, Default)]
struct Decoded(Arc<Mutex<Option<(i32, i32)>>>);

impl Decoded {
    fn hearer(&self) -> Arc<dyn Fn((i32, i32)) + Send + Sync> {
        let size = self.0.clone();
        Arc::new(move |new| *lock(&size) = Some(new))
    }

    fn take(&self) -> Option<(i32, i32)> {
        lock(&self.0).take()
    }
}

/// The stream of one activity, on the shell's loop thread. The activity's
/// host (`host.rs`) hands it the shell's input, main events and ticks.
pub struct Stream {
    app: AndroidApp,
    /// The app's own reference to the activity: android-activity's can go
    /// before a post to the main thread runs.
    activity: GlobalRef,
    shared: Arc<Shared>,
    id: u64,
    orders: mpsc::Receiver<Order>,
    phase: Phase,
}

impl Stream {
    /// The stream of this activity. Launches that korrid starts from now on
    /// come here, until this activity ends.
    pub fn new(app: &AndroidApp, env: &mut JNIEnv, activity: &JObject) -> anyhow::Result<Self> {
        static IDS: AtomicU64 = AtomicU64::new(1);
        let shared = SHARED
            .get()
            .ok_or_else(|| anyhow::anyhow!("korrid's viewer is not made yet"))?
            .clone();
        let id = IDS.fetch_add(1, Ordering::Relaxed);
        let (sender, orders) = mpsc::channel();
        *lock(&shared.orders) = Some((id, sender));
        // The window comes with this activity's InitWindow.
        shared.window.store(false, Ordering::Release);
        Ok(Self {
            app: app.clone(),
            activity: env.new_global_ref(activity)?,
            shared,
            id,
            orders,
            phase: Phase::Idle,
        })
    }

    /// Once per tick of the shell's timer: a new launch, korrid's resumes,
    /// the layers being made, and the stream's timers, window size and end.
    pub fn tick(&mut self) {
        while let Ok(order) = self.orders.try_recv() {
            match order {
                Order::Show(show) => self.begin(*show),
                Order::Resume(resume) => self.resume(resume),
            }
        }
        self.phase = match std::mem::replace(&mut self.phase, Phase::Idle) {
            Phase::Idle => Phase::Idle,
            Phase::Making(making) => self.make(*making),
            Phase::Playing(playing) => self.play(playing),
        };
    }

    /// The shell's input filter asks first (`Host::input`). While a stream
    /// is in front it takes every key and motion event: to its input, to its
    /// overlay, or, for Android's own keys, back to Android.
    pub fn input(&mut self, event: &InputEvent<'_>) -> Option<bool> {
        let Phase::Playing(playing) = &mut self.phase else {
            return None;
        };
        if playing.stopping {
            // Nothing more goes to a stream that stops; Android's own keys
            // go on to Android.
            return Some(match event {
                InputEvent::KeyEvent(key) => {
                    rules::keeps_key(is_system_key(key.key_code().into()), true, false)
                }
                _ => true,
            });
        }
        let mut android = false;
        let routed = match event {
            InputEvent::KeyEvent(key) => {
                android = is_system_key(key.key_code().into());
                playing.play.key(&self.app, &playing.layers, key)
            }
            // Every motion stays the stream's: a touch in the bars must not
            // reach the shell drawn under the video.
            InputEvent::MotionEvent(motion) => {
                playing.play.motion(motion);
                Routed::Handled
            }
            _ => Routed::Unhandled,
        };
        match routed {
            Routed::Handled => Some(rules::keeps_key(android, false, true)),
            // The stream's input is held: the shell takes the overlay
            // layer's Surface at its next tick.
            Routed::Opened => {
                playing.hosted.open();
                Some(true)
            }
            // Android's own keys (volume, power) go on: Slint leaves them to
            // Android. Any other key ends here (`rules::keeps_key`).
            Routed::Unhandled => Some(rules::keeps_key(android, false, false)),
            Routed::Leave => {
                self.leave("Ctrl+Alt+Shift+Q");
                Some(true)
            }
        }
    }

    /// android-activity's main events. Leaving the app ends the stream, as
    /// Artemis's Game activity ends it in `onStop`: the window, and with it
    /// the layers' parent, goes.
    pub fn main_event(&mut self, event: &MainEvent<'_>) {
        match event {
            MainEvent::InitWindow { .. } => self.shared.window.store(true, Ordering::Release),
            MainEvent::TerminateWindow { .. } => {
                self.shared.window.store(false, Ordering::Release);
                self.leave("the app's window went");
            }
            MainEvent::Destroy => {
                self.shared.window.store(false, Ordering::Release);
                self.leave("the activity ended");
            }
            _ => {}
        }
    }

    fn window_size(&self) -> Option<(i32, i32)> {
        self.app
            .native_window()
            .map(|window| (window.width(), window.height()))
            .filter(|(width, height)| *width > 0 && *height > 0)
    }

    fn begin(&mut self, show: Show) {
        if !matches!(self.phase, Phase::Idle) {
            return refuse(show, "another launch is in front");
        }
        let Some(window) = self
            .window_size()
            .filter(|_| self.shared.window.load(Ordering::Acquire))
        else {
            return refuse(show, "the app has no window to show the stream in");
        };
        match Layers::request(
            &self.app,
            window,
            (show.options.width, show.options.height),
            Presenter::Shell,
        ) {
            Ok(pending) => self.phase = Phase::Making(Box::new(Making { show, pending })),
            Err(cause) => refuse(
                show,
                &format!("the video layer was not asked for: {cause:#}"),
            ),
        }
    }

    /// korrid's Resume (`resume_and_focus`): the gameplay overlay closes,
    /// and korrid hears so once the stream's input goes on.
    fn resume(&mut self, resume: Resume) {
        let Resume { launch, closed } = resume;
        let Phase::Playing(playing) = &mut self.phase else {
            return refuse_resumes(vec![closed], "the stream is not in front");
        };
        if playing.launch != launch || playing.stopping {
            return refuse_resumes(vec![closed], "the stream is not in front");
        }
        if !playing.hosted.is_open() {
            let _ = closed.send(Ok(()));
            return;
        }
        playing.resumes.push(closed);
        let steps = playing.hosted.close(Closing::Resumed);
        perform(&self.app, playing, steps);
    }

    fn make(&mut self, mut making: Making) -> Phase {
        if making.show.lifecycle.stop_requested() {
            logcat::info("korri-app: viewer stopped while its layers were made; nothing started");
            making.show.lifecycle.stopped();
            let _ = making
                .show
                .started
                .send(Err("the viewer stopped before the stream started".into()));
            return Phase::Idle;
        }
        if !self.shared.window.load(Ordering::Acquire) {
            refuse(
                making.show,
                "the app's window went while the layers were made",
            );
            return Phase::Idle;
        }
        match making.pending.poll() {
            None => Phase::Making(Box::new(making)),
            Some(Err(cause)) => {
                refuse(making.show, &format!("no video layer: {cause:#}"));
                Phase::Idle
            }
            Some(Ok(layers)) => self.start(making.show, layers),
        }
    }

    /// The layers exist: the runtime starts, and Start goes to it.
    fn start(&mut self, show: Show, layers: Layers) -> Phase {
        let placed = layers.placed;
        log_placement(&layers);
        let mut devices = match AndroidDevices::new() {
            Ok(devices) => devices,
            Err(cause) => {
                layers.release(&self.app);
                refuse(
                    show,
                    &format!("the input devices are unreadable: {cause:#}"),
                );
                return Phase::Idle;
            }
        };
        // The controllers attached now are the launch's controller mask, as
        // in Artemis.
        let attached = stream_input::attached(&mut devices);
        logcat::info(&format!(
            "korri-app: viewer input: controllers attached at the start: mask {attached:#x}"
        ));
        // The controllers' rumble, light and battery reports (slice D4b), on
        // their own thread for the launch: the input tells it which
        // controller is which, and the runtime stops it after the input's
        // gate and before the core (`stream.rs`).
        let feedback = match feedback_jni::start() {
            Ok(feedback) => feedback,
            Err(cause) => {
                layers.release(&self.app);
                refuse(
                    show,
                    &format!("the controllers' feedback did not start: {cause:#}"),
                );
                return Phase::Idle;
            }
        };
        let seats = feedback.seats();
        // The stream's control "Rumble" (slice D11) reaches the same thread.
        let rumble = feedback.rumble_switch();
        let Show {
            launch,
            address,
            options,
            pin,
            lifecycle,
            started,
        } = show;
        let settings = Settings {
            mouse_emulation: options.mouse_emulation,
        };
        let landscape = options.width >= options.height;
        let decoded = Decoded::default();
        let picture = Picture {
            surface: layers.surface(),
            record: FrameRecord::default(),
            resized: decoded.hearer(),
        };
        let pin_path = pin.path();
        let runtime = {
            let lifecycle = lifecycle.clone();
            thread::Builder::new().name("viewer".into()).spawn(move || {
                launch::run_in_app(
                    options,
                    Some(&lifecycle),
                    Some(pin_path),
                    picture,
                    attached,
                    feedback,
                )
            })
        };
        let runtime = match runtime {
            Ok(runtime) => runtime,
            Err(cause) => {
                layers.release(&self.app);
                lifecycle.failed(format!("the viewer's thread did not start: {cause}"));
                let _ = started.send(Err("the viewer's thread did not start".into()));
                return Phase::Idle;
            }
        };
        // Start waits for the stream to run; korrid's start waits for its
        // reply. A Start that cannot be sent leaves the runtime to its own
        // deadline, and korrid's start ends with the dropped reply.
        let request = ViewerRequest {
            launch_id: launch.clone(),
            command: ViewerCommand::Start {
                address,
                display: None,
            },
        };
        logcat::info(&format!("korri-app: viewer request {}", json(&request)));
        let starter = lifecycle.clone();
        if let Err(cause) = thread::Builder::new()
            .name("viewer-start".into())
            .spawn(move || {
                let reply = starter.request(request);
                logcat::info(&format!("korri-app: viewer reply Start {}", json(&reply)));
                let _ = started.send(match reply {
                    ViewerReply::Ok => Ok(()),
                    ViewerReply::Error { message } => Err(message),
                    other => Err(format!("unexpected Start reply {}", json(&other))),
                });
            })
        {
            logcat::error(&format!("korri-app: viewer Start was not sent: {cause}"));
        }
        events::unbuffered(&self.app);
        self.orient(Some(landscape));
        // The stream's controls and status, for korrid's snapshot and
        // Apply; this activity is their lease.
        let changes = lifecycle.controls(self.id, status::readings);
        let (answers, left) = mpsc::channel();
        Phase::Playing(Box::new(Playing {
            launch,
            lifecycle,
            play: Play::new(
                settings,
                attached,
                placed,
                devices,
                Box::new(seats),
                Presenter::Shell,
            ),
            layers,
            runtime,
            stopping: false,
            decoded,
            changes,
            rumble,
            placing: None,
            hosted: Hosted::default(),
            answers,
            left,
            resumes: Vec::new(),
            _pin: pin,
        }))
    }

    fn play(&mut self, mut playing: Box<Playing>) -> Phase {
        while let Ok((opening, ok)) = playing.left.try_recv() {
            logcat::info(&format!(
                "korri-app: overlay: korrid's session.leave for {}: {}",
                playing.launch,
                if ok { "ok" } else { "refused" }
            ));
            let steps = playing.hosted.left(opening, ok);
            perform(&self.app, &mut playing, steps);
        }
        if playing.runtime.is_finished() {
            // The shell lets the overlay layer's Surface go before the
            // layers go: at its next tick.
            if playing.hosted.lends() {
                let steps = playing.hosted.close(Closing::StreamEnded);
                perform(&self.app, &mut playing, steps);
                return Phase::Playing(playing);
            }
            self.ended(playing);
            return Phase::Idle;
        }
        if !playing.stopping && playing.lifecycle.stop_requested() {
            // korrid asked for Stop: nothing more goes to the stream.
            playing.stopping = true;
            playing.play.end();
            let steps = playing.hosted.close(Closing::StreamStops);
            perform(&self.app, &mut playing, steps);
        }
        if playing.stopping {
            return Phase::Playing(playing);
        }
        playing.play.tick();
        let window = self
            .app
            .native_window()
            .is_some_and(|window| playing.layers.follow(&self.app, &window));
        // The sender changed the stream's size (#476): the decoder gives the
        // new one with its first frame. The video's place and the input's
        // map into it follow; the overlay and a held input stay as they are.
        let stream = playing
            .decoded
            .take()
            .is_some_and(|size| playing.layers.restream(&self.app, size));
        if window || stream {
            log_placement(&playing.layers);
            playing.play.place(playing.layers.placed);
        }
        self.controls(&mut playing);
        Phase::Playing(playing)
    }

    /// korrid's changes to the stream's controls, in order: each is
    /// confirmed once it has taken effect. A fill waits for its layer.
    fn controls(&self, playing: &mut Playing) {
        // Rumble is offered while a controller that can rumble is in the
        // stream, as Artemis offers it.
        playing.changes.offer(Offer {
            rumble: playing.play.has_rumble(),
        });
        if let Some((change, answer)) = playing.placing.take() {
            let result = match answer.try_recv() {
                Err(mpsc::TryRecvError::Empty) => {
                    playing.placing = Some((change, answer));
                    return;
                }
                Ok(Ok(())) => Ok(()),
                Ok(Err(cause)) => Err(format!("the video layer did not move: {cause:#}")),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err("the video layer did not move".to_owned())
                }
            };
            // The fill counts only now; one that the main thread did not
            // apply is undone.
            playing.layers.fill_answered(&self.app, result.is_ok());
            // Touches map into the picture's place once it is there.
            playing.play.place(playing.layers.placed);
            log_placement(&playing.layers);
            done(change, result);
        }
        while let Some(change) = playing.changes.next() {
            let value = change.change.value;
            match change.change.control {
                Control::Fill => match playing.layers.set_fill(&self.app, value) {
                    Some(answer) => {
                        playing.placing = Some((change, answer));
                        return;
                    }
                    // The window has the stream's shape: nothing moves.
                    None => done(change, Ok(())),
                },
                Control::FlipFaceButtons => {
                    let result = if playing.play.set_flip_face_buttons(value) {
                        Ok(())
                    } else {
                        Err("the stream's input has ended".to_owned())
                    };
                    done(change, result);
                }
                // In effect at once for the host's next rumble; the
                // feedback thread stops the vibrations that run.
                Control::Rumble => {
                    playing.rumble.set(value);
                    done(change, Ok(()));
                }
            }
        }
    }

    /// The runtime has ended: its layers go, after the decoder, and after
    /// the shell has let the overlay layer's Surface go.
    fn ended(&mut self, playing: Box<Playing>) {
        let Playing {
            launch,
            lifecycle,
            layers,
            mut play,
            runtime,
            resumes,
            ..
        } = *playing;
        refuse_resumes(resumes, "the stream ended");
        play.end();
        let joined = runtime.join();
        match &joined {
            Ok(Ok(())) => logcat::info(&format!("korri-app: viewer {launch} ended")),
            Ok(Err(cause)) => {
                logcat::error(&format!("korri-app: viewer {launch} failed: {cause:#}"))
            }
            Err(_) => logcat::error(&format!("korri-app: viewer {launch}: its thread panicked")),
        }
        if let Some(why) = rules::unreported(&joined) {
            lifecycle.failed(why.to_owned());
        }
        layers.release(&self.app);
        self.orient(None);
    }

    /// The person leaves the stream: its runtime stops. Stop waits for the
    /// runtime, so it runs on a thread of its own.
    fn leave(&mut self, why: &str) {
        match std::mem::replace(&mut self.phase, Phase::Idle) {
            Phase::Idle => {}
            Phase::Making(making) => {
                refuse(making.show, &format!("{why} before the stream started"))
            }
            Phase::Playing(mut playing) => {
                if !playing.stopping {
                    logcat::info(&format!("korri-app: viewer stops: {why}"));
                    playing.stopping = true;
                    playing.play.end();
                    let steps = playing.hosted.close(Closing::StreamStops);
                    perform(&self.app, &mut playing, steps);
                    stop_on_a_thread(&playing.launch, playing.lifecycle.clone());
                }
                self.phase = Phase::Playing(playing);
            }
        }
    }

    /// Artemis's Korri mode turns its activity to the stream's shape while it
    /// plays (`Game.setPreferredOrientationForActivity`); this activity
    /// turns back to its manifest's choice after. Posted to Android's main
    /// thread without waiting.
    fn orient(&self, landscape: Option<bool>) {
        // ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED, SENSOR_LANDSCAPE,
        // SENSOR_PORTRAIT.
        let orientation = match landscape {
            None => -1,
            Some(true) => 6,
            Some(false) => 7,
        };
        let vm = self.app.vm_as_ptr() as usize;
        let activity = self.activity.clone();
        self.app.run_on_java_main_thread(Box::new(move || {
            // SAFETY: the process's one Java VM, which outlives the app.
            let Ok(vm) = (unsafe { JavaVM::from_raw(vm as *mut jni::sys::JavaVM) }) else {
                return;
            };
            let Ok(mut env) = vm.attach_current_thread() else {
                return;
            };
            if env
                .call_method(
                    activity.as_obj(),
                    "setRequestedOrientation",
                    "(I)V",
                    &[orientation.into()],
                )
                .is_err()
            {
                let _ = env.exception_clear();
                logcat::error("korri-app: viewer orientation was not set");
            }
        }));
    }
}

impl Drop for Stream {
    /// The activity ends. Each launch still queued for it is refused. A
    /// runtime that still runs was asked to stop when the window went; its
    /// layers go once it has ended, on a thread that waits for it, so the
    /// layers outlive the decoder.
    fn drop(&mut self) {
        rules::close(
            &self.shared.orders,
            self.id,
            &self.orders,
            |order| match order {
                Order::Show(show) => refuse(*show, "the activity ended"),
                Order::Resume(resume) => refuse_resumes(vec![resume.closed], "the activity ended"),
            },
        );
        match std::mem::replace(&mut self.phase, Phase::Idle) {
            Phase::Idle => {}
            Phase::Making(making) => refuse(making.show, "the activity ended"),
            Phase::Playing(playing) => {
                let Playing {
                    launch,
                    lifecycle,
                    layers,
                    mut play,
                    runtime,
                    stopping,
                    decoded: _,
                    changes: _,
                    rumble: _,
                    placing: _,
                    hosted: _,
                    answers: _,
                    left: _,
                    resumes,
                    _pin,
                } = *playing;
                refuse_resumes(resumes, "the activity ended");
                play.end();
                if !stopping {
                    stop_on_a_thread(&launch, lifecycle.clone());
                }
                let app = self.app.clone();
                let _ = thread::Builder::new()
                    .name("viewer-release".into())
                    .spawn(move || {
                        let joined = runtime.join();
                        if let Some(why) = rules::unreported(&joined) {
                            logcat::error(&format!("korri-app: viewer {launch}: {why}"));
                            lifecycle.failed(why.to_owned());
                        }
                        drop(_pin);
                        layers.release(&app);
                    });
            }
        }
    }
}

/// The shell's `Overlay` host (slice D5b): the shell lends the overlay
/// layer's Surface while korrid's gameplay overlay shows, and takes its
/// input. Each call runs on the loop thread, inside the shell's 16 ms timer,
/// after `Host::tick`; none waits for Android's main thread.
impl Overlay for Stream {
    fn closing(&mut self, _env: &mut JNIEnv) -> jni::errors::Result<bool> {
        Ok(matches!(&self.phase, Phase::Playing(playing) if playing.hosted.closing()))
    }

    fn released(&mut self, _env: &mut JNIEnv, failed: bool) -> jni::errors::Result<()> {
        if let Phase::Playing(playing) = &mut self.phase {
            if playing.hosted.lends() {
                logcat::info(&format!(
                    "korri-app: overlay: the shell let the layer's Surface go{}",
                    if failed {
                        ": it could not draw into it"
                    } else {
                        ""
                    }
                ));
            }
            let steps = playing.hosted.released(failed);
            perform(&self.app, playing, steps);
        }
        Ok(())
    }

    /// The overlay layer's Surface, once an opening; korrid then hears that
    /// the person left, so that its overlay has a Surface to show in.
    fn take_surface<'local>(
        &mut self,
        env: &mut JNIEnv<'local>,
    ) -> jni::errors::Result<JObject<'local>> {
        let Phase::Playing(playing) = &mut self.phase else {
            return Ok(JObject::null());
        };
        let Some(surface) = playing
            .layers
            .overlay_surface()
            .filter(|_| playing.hosted.offered())
        else {
            return Ok(JObject::null());
        };
        let surface = env.new_local_ref(surface.as_obj())?;
        let Some(leave) = playing.hosted.take() else {
            return Ok(JObject::null());
        };
        logcat::info(
            "korri-app: overlay: the shell took the layer's Surface; korrid hears that the person left",
        );
        perform(&self.app, playing, [leave]);
        Ok(surface)
    }

    /// The window's density, as Slint's own window reads it.
    fn density(&mut self, _env: &mut JNIEnv) -> jni::errors::Result<f32> {
        Ok(self
            .app
            .config()
            .density()
            .map(|dpi| dpi as f32 / 160.0)
            .unwrap_or(1.0))
    }

    fn resized(&mut self, _env: &mut JNIEnv) -> jni::errors::Result<bool> {
        Ok(matches!(&self.phase, Phase::Playing(playing)
            if playing.hosted.lends() && playing.layers.overlay_resized()))
    }

    fn take_keys(&mut self, _env: &mut JNIEnv) -> jni::errors::Result<Vec<i32>> {
        Ok(self
            .lent()
            .map(|play| play.forwarded().take_keys())
            .unwrap_or_default())
    }

    fn take_axes(&mut self, _env: &mut JNIEnv) -> jni::errors::Result<Vec<i32>> {
        Ok(self
            .lent()
            .map(|play| play.forwarded().take_axes())
            .unwrap_or_default())
    }

    fn take_pointers(&mut self, _env: &mut JNIEnv) -> jni::errors::Result<Vec<f32>> {
        Ok(self
            .lent()
            .map(|play| play.forwarded().take_pointers())
            .unwrap_or_default())
    }

    fn drawn(&mut self) {
        if let Phase::Playing(playing) = &mut self.phase {
            if let Some(show) = playing.hosted.drawn() {
                perform(&self.app, playing, [show]);
            }
        }
    }
}

impl Stream {
    /// The stream's input while the shell holds the overlay's Surface and
    /// is not giving it back: what the overlay took goes to the shell.
    /// Before, and while it closes, nothing does: what waits is dropped.
    fn lent(&mut self) -> Option<&mut Play> {
        let Phase::Playing(playing) = &mut self.phase else {
            return None;
        };
        if playing.hosted.lends() && !playing.hosted.closing() {
            Some(&mut playing.play)
        } else {
            playing.play.forwarded().clear();
            None
        }
    }
}

/// The steps of the shell's overlay (`hosted.rs`), in their order.
fn perform(app: &AndroidApp, playing: &mut Playing, steps: impl IntoIterator<Item = Step>) {
    for step in steps {
        match step {
            Step::Show => {
                logcat::info("korri-app: overlay: the shell drew it; the layer shows");
                playing.layers.show_overlay(app, true);
            }
            Step::Hide => playing.layers.show_overlay(app, false),
            Step::Leave(opening) => {
                leave_on_a_thread(&playing.launch, opening, playing.answers.clone())
            }
            Step::Closed(closing) => {
                // The shell has no Surface any more: the input goes on. A
                // resume hears Ok only when it closed the overlay.
                playing.play.close(closing.why());
                let answer = closing.answer();
                for closed in playing.resumes.drain(..) {
                    let _ = closed.send(answer.clone());
                }
            }
        }
    }
}

/// korrid's `session.leave` for an opening of the overlay, on a thread of
/// its own: it is an RPC to korrid, which the loop thread must not wait for.
fn leave_on_a_thread(launch: &str, opening: u64, answer: mpsc::Sender<(u64, bool)>) {
    let launch = launch.to_owned();
    if let Err(cause) = thread::Builder::new()
        .name("viewer-leave".into())
        .spawn(move || {
            let left = korrid::android_app::leave_viewer(&launch);
            let _ = answer.send((opening, left));
        })
    {
        logcat::error(&format!(
            "korri-app: overlay: korrid's session.leave was not sent: {cause}"
        ));
    }
}

/// korrid's resumes that the stream can no longer answer.
fn refuse_resumes(resumes: Vec<oneshot::Sender<Result<(), String>>>, why: &str) {
    for closed in resumes {
        let _ = closed.send(Err(why.to_owned()));
    }
}

/// Where the video layer is, in the log: at the start, after a turn or a
/// new stream size, and after "Crop to fill".
fn log_placement(layers: &Layers) {
    let (placed, stream, size) = (layers.placed, layers.stream, layers.window);
    logcat::info(&format!(
        "korri-app: viewer video layer {}x{} placed at ({}, {})-({}, {}) in the window {}x{}{}",
        stream.0,
        stream.1,
        placed.left,
        placed.top,
        placed.right,
        placed.bottom,
        size.0,
        size.1,
        if layers.fill() {
            ", cropped to fill"
        } else {
            ""
        }
    ));
}

/// A control change has taken effect, or could not: korrid's Apply answers.
fn done(change: ControlChange, result: Result<(), String>) {
    let (control, value) = (change.change.control, change.change.value);
    match &result {
        Ok(()) => logcat::info(&format!(
            "korri-app: viewer control {} is {value}",
            control.wire()
        )),
        Err(why) => logcat::error(&format!(
            "korri-app: viewer control {} not set to {value}: {why}",
            control.wire()
        )),
    }
    change.done(result);
}

/// A launch that the loop thread will not show: its Start fails and its
/// runtime's lifecycle reports Stopped.
fn refuse(show: Show, why: &str) {
    logcat::error(&format!(
        "korri-app: viewer {} not shown: {why}",
        show.launch
    ));
    show.lifecycle.failed(why.to_owned());
    let _ = show.started.send(Err(why.to_owned()));
}

fn stop_on_a_thread(launch: &str, lifecycle: Lifecycle) {
    let request = ViewerRequest {
        launch_id: launch.to_owned(),
        command: ViewerCommand::Stop,
    };
    logcat::info(&format!("korri-app: viewer request {}", json(&request)));
    if let Err(cause) = thread::Builder::new()
        .name("viewer-stop".into())
        .spawn(move || {
            let reply = lifecycle.request(request);
            logcat::info(&format!("korri-app: viewer reply Stop {}", json(&reply)));
        })
    {
        logcat::error(&format!("korri-app: viewer Stop was not sent: {cause}"));
    }
}
