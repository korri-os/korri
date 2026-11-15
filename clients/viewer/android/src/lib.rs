// SPDX-License-Identifier: GPL-3.0-or-later
//! The Rust viewer as an Android app, #391 step D: Android's own
//! `NativeActivity` runs one launch of the shared viewer in this process
//! (ADR 0060), with the in-process lifecycle in the place of korrid's socket.
//! The app is that lifecycle's controller: it sends Start once its window
//! exists, and Stop on Back or when the window goes, and writes every event
//! and reply to logcat. The picture is Android's hardware decoder rendering
//! into a video layer that the app makes (slice D2, `layer.rs`). The sound is
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

mod input;
mod intent;
mod layer;
mod logcat;

use android_activity::{
    AndroidApp, InputStatus, MainEvent, PollEvent,
    input::{InputEvent, KeyAction, Keycode},
};
use anyhow::{Context, Result, anyhow};
use korri_viewer::{
    devices::{AndroidDevices, now_ms},
    input::{self as stream_input, CoreSink, Devices, Input, Outcome, Settings, core_sink},
    launch,
    lifecycle::Lifecycle,
    options::Options,
    overlay::{Keys, Overlay, Route},
    picture::{FrameRecord, Picture},
};
use korri_viewer_treaty::{ViewerCommand, ViewerRequest};
use ndk::native_window::NativeWindow;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
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
    let picture = Picture {
        surface: layer.surface(),
        record,
    };
    let ended = stream(app, id, address, options, pin, picture, &mut layer);
    layer.release(app);
    ended
}

/// The placement in the log, and the window's one black fill for the bars.
fn placed(layer: &layer::Layers, window: &NativeWindow) {
    let (placed, stream, size) = (layer.placed, layer.stream, layer.window);
    logcat::info(&format!(
        "picture: video layer {}x{} placed at ({}, {})-({}, {}) in the window {}x{}",
        stream.0, stream.1, placed.left, placed.top, placed.right, placed.bottom, size.0, size.1
    ));
    if let Err(error) = layer::fill_black(window) {
        logcat::error(&format!(
            "picture: the bars keep the window's content: {error:#}"
        ));
    }
}

fn stream(
    app: &AndroidApp,
    id: String,
    address: String,
    options: Options,
    pin: PathBuf,
    picture: Picture,
    layer: &mut layer::Layers,
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
    let runtime = {
        let lifecycle = lifecycle.clone();
        thread::Builder::new()
            .name("viewer".into())
            .spawn(move || {
                launch::run_in_app(options, Some(&lifecycle), Some(pin), picture, attached)
            })?
    };
    let mut controller = Controller {
        lifecycle,
        launch: id,
        requests: Vec::new(),
    };
    let input = Input::new(settings, attached, layer.placed);
    let driven = drive(
        app,
        &mut controller,
        address,
        &runtime,
        layer,
        input,
        &mut devices,
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
/// meanwhile, and the layers follow the window's size.
fn drive(
    app: &AndroidApp,
    controller: &mut Controller,
    address: String,
    runtime: &JoinHandle<Result<()>>,
    layer: &mut layer::Layers,
    input: Input,
    devices: &mut AndroidDevices,
) -> Result<()> {
    controller.send(ViewerCommand::Start {
        address,
        display: None,
    })?;
    input::unbuffered(app);
    let mut input = Guarded(Some(input));
    // The core's input behind its gate: closed before the core stops, also
    // when the host or the network ends the stream while this thread sends.
    let mut sink = core_sink();
    let mut gameplay = Gameplay::default();
    let mut stopping = false;
    let mut checked = now_ms();
    while !runtime.is_finished() {
        // The input's timers (a touch's click, the emulated mouse, the
        // controllers' heartbeat) wake the loop, or it waits for 100 ms.
        let wait = input
            .next_deadline()
            .map_or(100, |at| at.saturating_sub(now_ms()).clamp(0, 100));
        let mut stop = false;
        app.poll_events(Some(Duration::from_millis(wait as u64)), |event| {
            // Leaving the app ends the stream, as Artemis's Game activity
            // does in onStop: its window and the layer's parent go.
            if let PollEvent::Main(MainEvent::Destroy | MainEvent::TerminateWindow { .. }) = event {
                stop = true;
            }
        });
        stop |= events(app, &mut input, devices, &mut sink, &mut gameplay, layer);
        input.run("tick", |input| input.tick(now_ms(), &mut sink));
        // Android tells no native code that a device went: ask twice a
        // second, as Artemis's listener would have told it.
        if now_ms() - checked >= 500 {
            checked = now_ms();
            input.run("devices", |input| input.check_devices(devices, &mut sink));
        }
        if stop && !stopping {
            stopping = true;
            if gameplay.overlay == Overlay::Open {
                logcat::info("overlay: open when the stream stopped");
            }
            // Nothing more goes to a stream that stops.
            if let Some(input) = input.0.take() {
                logcat::info(&format!("input: ended: {}", input.report()));
            }
            controller.send(ViewerCommand::Stop)?;
        }
        if !stop
            && let Some(window) = app.native_window()
            && layer.follow(app, &window)
        {
            placed(layer, &window);
            input.run("place", |input| input.place(layer.placed));
        }
    }
    if let Some(input) = input.0.take() {
        logcat::info(&format!("input: ended: {}", input.report()));
    }
    Ok(())
}

/// The stream's input, which a panic in it ends: the stream goes on
/// without it. No panic reaches android-activity's input queue.
struct Guarded(Option<Input>);

impl Guarded {
    fn run<T>(&mut self, what: &str, work: impl FnOnce(&mut Input) -> T) -> Option<T> {
        let input = self.0.as_mut()?;
        match catch_unwind(AssertUnwindSafe(|| work(input))) {
            Ok(value) => Some(value),
            Err(_) => {
                logcat::error(&format!(
                    "input: {what} panicked; the stream goes on without input"
                ));
                self.0 = None;
                None
            }
        }
    }

    fn next_deadline(&self) -> Option<i64> {
        self.0.as_ref().and_then(Input::next_deadline)
    }
}

/// The gameplay overlay while the stream runs: Artemis's
/// `showSurfaceOverlay` and `hideSurfaceOverlay`. Opening holds the
/// stream's input, then shows the layer; closing hides the layer, then the
/// input goes to the stream again.
#[derive(Default)]
struct Gameplay {
    overlay: Overlay,
    keys: Keys,
    /// What the overlay took while it was open, for the log.
    taken_keys: u32,
    taken_motions: u32,
    opened: Option<Instant>,
}

impl Gameplay {
    fn open(
        &mut self,
        why: &str,
        app: &AndroidApp,
        layer: &layer::Layers,
        input: &mut Guarded,
        sink: &mut CoreSink,
    ) {
        if self.overlay == Overlay::Open {
            return;
        }
        if !layer.has_overlay() {
            logcat::info(&format!("overlay: {why}, but there is no overlay layer"));
            return;
        }
        self.overlay = Overlay::Open;
        self.opened = Some(Instant::now());
        input.run("hold", |input| input.hold(sink));
        layer.show_overlay(app, true);
        logcat::info(&format!(
            "overlay: open on {why}; the stream's input is held"
        ));
    }

    fn close(
        &mut self,
        why: &str,
        app: &AndroidApp,
        layer: &layer::Layers,
        input: &mut Guarded,
        sink: &mut CoreSink,
    ) {
        if self.overlay == Overlay::Closed {
            return;
        }
        layer.show_overlay(app, false);
        input.run("release", |input| input.release(sink));
        self.overlay = Overlay::Closed;
        let open = self
            .opened
            .take()
            .map_or(0.0, |at| at.elapsed().as_secs_f64());
        logcat::info(&format!(
            "overlay: closed on {why} after {open:.1} s; it took {} keys and {} motion events; the stream's input goes on",
            std::mem::take(&mut self.taken_keys),
            std::mem::take(&mut self.taken_motions)
        ));
    }
}

/// The key that opened or closed the overlay, for the log.
fn toggle_name(key_code: Keycode) -> &'static str {
    match key_code {
        Keycode::ButtonMode => "Guide",
        Keycode::Back => "Back",
        _ => "a key",
    }
}

/// Every pending key and motion event, to the stream's input or to the
/// gameplay overlay. True: the user leaves (Ctrl+Alt+Shift+Q).
fn events(
    app: &AndroidApp,
    input: &mut Guarded,
    devices: &mut AndroidDevices,
    sink: &mut CoreSink,
    gameplay: &mut Gameplay,
    layer: &layer::Layers,
) -> bool {
    let mut leave = false;
    let Ok(mut events) = app.input_events_iter() else {
        return false;
    };
    while events.next(|event| match event {
        InputEvent::KeyEvent(key) => {
            let event = input::key(key);
            match gameplay.keys.route(&event, gameplay.overlay) {
                Route::Android => InputStatus::Unhandled,
                Route::Taken => {
                    if gameplay.overlay == Overlay::Open {
                        gameplay.taken_keys += 1;
                    }
                    InputStatus::Handled
                }
                Route::Open => {
                    gameplay.open(toggle_name(key.key_code()), app, layer, input, sink);
                    InputStatus::Handled
                }
                Route::Close => {
                    gameplay.close(toggle_name(key.key_code()), app, layer, input, sink);
                    InputStatus::Handled
                }
                Route::Stream => {
                    let outcome = input
                        .run("key", |input| input.key(&event, now_ms(), devices, sink))
                        .unwrap_or(Outcome::Unhandled);
                    match outcome {
                        Outcome::Handled => InputStatus::Handled,
                        Outcome::Leave => {
                            leave = true;
                            InputStatus::Handled
                        }
                        Outcome::Overlay => {
                            gameplay.open("Start pressed twice and held", app, layer, input, sink);
                            InputStatus::Handled
                        }
                        Outcome::Unhandled => InputStatus::Unhandled,
                    }
                }
            }
        }
        // The overlay takes every motion event while it is open.
        InputEvent::MotionEvent(_) if gameplay.overlay == Overlay::Open => {
            gameplay.taken_motions += 1;
            InputStatus::Handled
        }
        InputEvent::MotionEvent(motion) => {
            let event = input::motion(motion);
            let handled = input
                .run("motion", |input| {
                    input.motion(&event, now_ms(), devices, sink)
                })
                .unwrap_or(false);
            if handled {
                InputStatus::Handled
            } else {
                InputStatus::Unhandled
            }
        }
        _ => InputStatus::Unhandled,
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
