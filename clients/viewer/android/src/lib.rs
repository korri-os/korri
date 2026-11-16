// SPDX-License-Identifier: GPL-3.0-or-later
//! The Rust viewer as an Android app, #391 step D: Android's own
//! `NativeActivity` runs one launch of the shared viewer in this process
//! (ADR 0060), with the in-process lifecycle in the place of korrid's socket.
//! The app is that lifecycle's controller: it sends Start once its window
//! exists, and Stop on Back or when the window goes, and writes every event
//! and reply to logcat. The picture is Android's hardware decoder rendering
//! into a video layer that the app makes (slice D2, `layer.rs`); the layer
//! follows the window's size and the decoded picture's size, which the
//! sender can change in the middle of the stream (#476). The sound is
//! Opus into AAudio inside the shared viewer (slice D3); the app adds
//! nothing for it. The input (slice D4) is the shared viewer's mapping of
//! Artemis's: this thread hands it each key and motion event (`input.rs`),
//! and it sends through the core. Ctrl+Alt+Shift+Q, and leaving the app,
//! end the stream.
//!
//! The gameplay overlay (slice D5) is a second layer above the video, as
//! Artemis's Korri mode opens it: Guide or Back, from any device, or Start
//! pressed twice quickly and held. While it is open the stream's input is
//! held (`Input::hold`), and the overlay takes every key and motion event
//! but Android's volume and power keys. Guide or Back closes it. The shell's
//! overlay cannot run in this app (`clients/viewer/README.md`), so the layer
//! shows a stand-in.
//!
//! The controllers' rumble, light and battery reports (slice D4b) run on a
//! feedback thread of the shared viewer; the input tells it which
//! controller is which.
//!
//! Until korrid drives the viewer in this process, the launch comes from the
//! starting intent's extras, with the names of their sources:
//!
//! | Extra | Type | Source |
//! | --- | --- | --- |
//! | `launch_id` | string | `ViewerRequest.launch_id` |
//! | `address` | string | `ViewerCommand::Start.address` |
//! | `args` | string array | the command's arguments, `stream [Moonlight arguments] HOST` |
//! | `KORRI_MOONLIGHT_SERVER_CERTIFICATE` | string | the pinned server certificate's path |
//! | `CADENCE_FRAME_TIMES`, `CADENCE_DUMP` | string | optional: the frame record's new files, the measure's ask (`clients/viewer/README.md`) |
#![cfg(target_os = "android")]

mod intent;
mod logcat;

use android_activity::{
    AndroidApp, InputStatus, MainEvent, PollEvent,
    input::{InputEvent, KeyAction, Keycode},
};
use anyhow::{Context, Result, anyhow};
use korri_viewer::{
    devices::{AndroidDevices, now_ms},
    events, feedback_jni,
    input::{self as stream_input, Settings},
    launch, layer,
    lifecycle::Lifecycle,
    options::Options,
    overlay::Presenter,
    picture::{FrameRecord, Picture},
    play::{Play, Routed},
};
use korri_viewer_treaty::{ViewerCommand, ViewerRequest};
use ndk::native_window::NativeWindow;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
    thread::{self, JoinHandle},
    time::Duration,
};

struct Launch {
    id: String,
    address: String,
    options: Options,
    pin: PathBuf,
    record: FrameRecord,
}

impl Launch {
    fn from_intent(app: &AndroidApp) -> Result<Self> {
        let extras = intent::Extras::of(app)?;
        let required = |name: &str| -> Result<String> {
            extras
                .string(name)?
                .with_context(|| format!("no intent extra {name}"))
        };
        let args = extras.strings("args")?.context("no intent extra args")?;
        Ok(Self {
            id: required("launch_id")?,
            address: required("address")?,
            options: Options::parse(args)?,
            pin: required("KORRI_MOONLIGHT_SERVER_CERTIFICATE")?.into(),
            record: FrameRecord {
                times: extras.string("CADENCE_FRAME_TIMES")?.map(Into::into),
                recording: extras.string("CADENCE_DUMP")?.map(Into::into),
            },
        })
    }
}

/// The controller's requests, each on its own thread: Start blocks until the
/// stream runs, and Stop may come while it waits.
struct Controller {
    lifecycle: Lifecycle,
    launch: String,
    requests: Vec<JoinHandle<()>>,
}

impl Controller {
    fn send(&mut self, command: ViewerCommand) -> Result<()> {
        let lifecycle = self.lifecycle.clone();
        let request = ViewerRequest {
            launch_id: self.launch.clone(),
            command,
        };
        let name = serde_json::to_value(&request.command)?["kind"]
            .as_str()
            .unwrap_or("request")
            .to_owned();
        logcat::info(&format!("request {}", serde_json::to_string(&request)?));
        self.requests.push(
            thread::Builder::new()
                .name(format!("viewer-{name}"))
                .spawn(move || {
                    let reply = lifecycle.request(request);
                    let reply = serde_json::to_string(&reply).unwrap_or_default();
                    logcat::info(&format!("reply {name} {reply}"));
                })?,
        );
        Ok(())
    }

    /// Stop on this thread, waiting for the runtime's reply.
    fn stop_here(&self) {
        let reply = self.lifecycle.request(ViewerRequest {
            launch_id: self.launch.clone(),
            command: ViewerCommand::Stop,
        });
        let reply = serde_json::to_string(&reply).unwrap_or_default();
        logcat::info(&format!("reply Stop {reply}"));
    }

    fn join(self) {
        for request in self.requests {
            let _ = request.join();
        }
    }
}

/// Before the stream's input exists: Back leaves, as it ends a stream.
fn back_pressed(app: &AndroidApp) -> bool {
    let mut pressed = false;
    if let Ok(mut input) = app.input_events_iter() {
        while input.next(|event| match event {
            InputEvent::KeyEvent(key) if key.key_code() == Keycode::Back => {
                pressed |= key.action() == KeyAction::Up;
                InputStatus::Handled
            }
            _ => InputStatus::Unhandled,
        }) {}
    }
    pressed
}

/// The app's own window, once Android gives it; none when the user leaves
/// first. The window is not touched inside the event callback: making the
/// video layer waits for Android's main thread, which waits there.
fn wait_for_window(app: &AndroidApp) -> Option<NativeWindow> {
    loop {
        let mut left = false;
        let mut ready = false;
        app.poll_events(Some(Duration::from_millis(100)), |event| match event {
            PollEvent::Main(MainEvent::InitWindow { .. }) => ready = true,
            PollEvent::Main(MainEvent::Destroy) => left = true,
            _ => {}
        });
        left |= back_pressed(app);
        if left {
            return None;
        }
        if ready && let Some(window) = app.native_window() {
            return Some(window);
        }
    }
}

fn run(app: &AndroidApp) -> Result<()> {
    let Launch {
        id,
        address,
        options,
        pin,
        record,
    } = Launch::from_intent(app)?;
    let Some(window) = wait_for_window(app) else {
        logcat::info("left before the window was shown; nothing started");
        return Ok(());
    };
    let Some(mut layer) = layer::Layers::make(app, &window, (options.width, options.height))?
    else {
        logcat::info("left while the layers were made; nothing started");
        return Ok(());
    };
    placed(&layer, &window);
    // The app's window is Android's; after it goes, it must not be used.
    drop(window);
    let decoded = Decoded::new(app);
    let picture = Picture {
        surface: layer.surface(),
        record,
        resized: decoded.hearer(),
    };
    let ended = stream(
        app, id, address, options, pin, picture, &mut layer, &decoded,
    );
    layer.release(app);
    ended
}

/// The decoded picture's newest size, from the decoder's output thread to
/// this one, which it wakes.
struct Decoded {
    size: Arc<Mutex<Option<(i32, i32)>>>,
    waker: android_activity::AndroidAppWaker,
}

impl Decoded {
    fn new(app: &AndroidApp) -> Self {
        Self {
            size: Arc::default(),
            waker: app.create_waker(),
        }
    }

    /// What the decoder calls with each new size: it stores and wakes, and
    /// never blocks for long.
    fn hearer(&self) -> Arc<dyn Fn((i32, i32)) + Send + Sync> {
        let (size, waker) = (self.size.clone(), self.waker.clone());
        Arc::new(move |new| {
            *size.lock().unwrap_or_else(PoisonError::into_inner) = Some(new);
            waker.wake();
        })
    }

    fn take(&self) -> Option<(i32, i32)> {
        self.size
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

/// The placement in the log.
fn log_placement(layer: &layer::Layers) {
    let (placed, stream, size) = (layer.placed, layer.stream, layer.window);
    logcat::info(&format!(
        "picture: video layer {}x{} placed at ({}, {})-({}, {}) in the window {}x{}",
        stream.0, stream.1, placed.left, placed.top, placed.right, placed.bottom, size.0, size.1
    ));
}

/// The placement in the log, and the window's one black fill for the bars.
fn placed(layer: &layer::Layers, window: &NativeWindow) {
    log_placement(layer);
    if let Err(error) = layer::fill_black(window) {
        logcat::error(&format!(
            "picture: the bars keep the window's content: {error:#}"
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn stream(
    app: &AndroidApp,
    id: String,
    address: String,
    options: Options,
    pin: PathBuf,
    picture: Picture,
    layer: &mut layer::Layers,
    decoded: &Decoded,
) -> Result<()> {
    let lifecycle = Lifecycle::in_process(&id, |event| {
        let event = serde_json::to_string(event).unwrap_or_default();
        logcat::info(&format!("event {event}"));
    })?;
    // The controllers attached now are the launch's controller mask, as in
    // Artemis.
    let mut devices = AndroidDevices::new()?;
    let attached = stream_input::attached(&mut devices);
    logcat::info(&format!(
        "input: controllers attached at the start: mask {attached:#x}"
    ));
    let settings = Settings {
        mouse_emulation: options.mouse_emulation,
    };
    // The controllers' rumble, light and battery reports, on their own
    // thread for the whole run: the input tells it which controller is
    // which from the first press, and the run stops it before the core.
    let feedback = feedback_jni::start()?;
    let seats = feedback.seats();
    let runtime = {
        let lifecycle = lifecycle.clone();
        thread::Builder::new()
            .name("viewer".into())
            .spawn(move || {
                launch::run_in_app(
                    options,
                    Some(&lifecycle),
                    Some(pin),
                    picture,
                    attached,
                    feedback,
                )
            })?
    };
    let mut controller = Controller {
        lifecycle,
        launch: id,
        requests: Vec::new(),
    };
    let play = Play::new(
        settings,
        attached,
        layer.placed,
        devices,
        Box::new(seats),
        Presenter::StandIn,
    );
    let driven = drive(
        app,
        &mut controller,
        address,
        &runtime,
        layer,
        decoded,
        play,
    );
    if driven.is_err() {
        // A request that could not go leaves the runtime streaming, or
        // waiting for Start: stop it from this thread, which needs no other.
        controller.stop_here();
    }
    // Always joined: the caller releases the video layer only after this,
    // so the layer outlives the decoder.
    let ended = runtime
        .join()
        .map_err(|_| anyhow!("the viewer thread panicked"))
        .and_then(|ended| ended);
    controller.join();
    driven.and(ended)
}

/// Start, then Stop when the user leaves, until the runtime ends. Each key
/// and motion event goes to the stream's input or to the gameplay overlay
/// meanwhile (`Play`), and the layers follow the window's size; the video
/// layer follows the decoded picture's size too.
fn drive(
    app: &AndroidApp,
    controller: &mut Controller,
    address: String,
    runtime: &JoinHandle<Result<()>>,
    layer: &mut layer::Layers,
    decoded: &Decoded,
    mut play: Play,
) -> Result<()> {
    controller.send(ViewerCommand::Start {
        address,
        display: None,
    })?;
    events::unbuffered(app);
    let mut stopping = false;
    while !runtime.is_finished() {
        // The input's timers (a touch's click, the emulated mouse, the
        // controllers' heartbeat) wake the loop, or it waits for 100 ms.
        let wait = play
            .next_deadline()
            .map_or(100, |at| at.saturating_sub(now_ms()).clamp(0, 100));
        let (mut stop, mut cutout_may_move) = (false, false);
        app.poll_events(Some(Duration::from_millis(wait as u64)), |event| {
            match event {
                // Leaving the app ends the stream, as Artemis's Game activity
                // does in onStop: its window and the layer's parent go.
                PollEvent::Main(MainEvent::Destroy | MainEvent::TerminateWindow { .. }) => {
                    stop = true
                }
                PollEvent::Main(main) if layer::may_move_the_cutout(&main) => {
                    cutout_may_move = true
                }
                _ => {}
            }
        });
        stop |= events(app, &mut play, layer);
        play.tick();
        if stop && !stopping {
            stopping = true;
            // Nothing more goes to a stream that stops.
            play.end();
            controller.send(ViewerCommand::Stop)?;
        }
        if !stop && let Some(window) = app.native_window() {
            if layer.follow(app, &window) {
                placed(layer, &window);
                play.place(layer.placed);
            } else if cutout_may_move {
                // A turn by 180° keeps the size and moves the cutout (#494).
                layer.follow_cutout(app);
            }
        }
        // The sender changed the stream's size: the decoder gives the new
        // one when its first frame comes out. Only the video's place and the
        // input's map into it change; the overlay, open or not, and a held
        // input stay as they are.
        if let Some(size) = decoded.take()
            && !stop
            && layer.restream(app, size)
        {
            log_placement(layer);
            play.place(layer.placed);
        }
    }
    play.end();
    Ok(())
}

/// Every pending key and motion event, to the stream's input or to the
/// gameplay overlay. True: the user leaves (Ctrl+Alt+Shift+Q).
fn events(app: &AndroidApp, play: &mut Play, layer: &layer::Layers) -> bool {
    let mut leave = false;
    let Ok(mut events) = app.input_events_iter() else {
        return false;
    };
    while events.next(|event| {
        let routed = match event {
            InputEvent::KeyEvent(key) => play.key(app, layer, key),
            InputEvent::MotionEvent(motion) => play.motion(layer, motion),
            _ => Routed::Unhandled,
        };
        match routed {
            // The stand-in shows itself when the overlay opens.
            Routed::Handled | Routed::Opened => InputStatus::Handled,
            Routed::Leave => {
                leave = true;
                InputStatus::Handled
            }
            Routed::Unhandled => InputStatus::Unhandled,
        }
    }) {}
    leave
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    logcat::panics();
    match run(&app) {
        Ok(()) => logcat::info("viewer ended"),
        Err(error) => logcat::error(&format!("viewer failed: {error:#}")),
    }
}
