// SPDX-License-Identifier: GPL-3.0-or-later
//! The Rust viewer as an Android app, #391 step D: Android's own
//! `NativeActivity` runs one launch of the shared viewer in this process
//! (ADR 0060), with the in-process lifecycle in the place of korrid's socket.
//! The app is that lifecycle's controller: it sends Start once its window
//! exists, and Stop on Back or when the window goes, and writes every event
//! and reply to logcat. The picture is Android's hardware decoder rendering
//! into a video layer that the app makes (slice D2, `layer.rs`); there is no
//! sound or input yet (D3, D4).
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
mod layer;
mod logcat;

use android_activity::{
    AndroidApp, InputStatus, MainEvent, PollEvent,
    input::{InputEvent, KeyAction, Keycode},
};
use anyhow::{Context, Result, anyhow};
use korri_viewer::{
    launch,
    lifecycle::Lifecycle,
    options::Options,
    picture::{FrameRecord, Picture},
};
use korri_viewer_treaty::{ViewerCommand, ViewerRequest};
use ndk::native_window::NativeWindow;
use std::{
    path::PathBuf,
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

/// Back ends the stream, as leaving a game does; the activity then finishes.
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
    let Some(layer) = layer::VideoLayer::make(app, &window, (options.width, options.height))?
    else {
        logcat::info("left while the video layer was made; nothing started");
        return Ok(());
    };
    let placed = layer.placed;
    logcat::info(&format!(
        "picture: video layer {}x{} placed at ({}, {})-({}, {}) in the window {}x{}",
        options.width,
        options.height,
        placed.left,
        placed.top,
        placed.right,
        placed.bottom,
        window.width(),
        window.height()
    ));
    if let Err(error) = layer::fill_black(&window) {
        logcat::error(&format!(
            "picture: the bars keep the window's content: {error:#}"
        ));
    }
    // The app's window is Android's; after it goes, it must not be used.
    drop(window);
    let ended = stream(
        app,
        id,
        address,
        options,
        pin,
        Picture {
            surface: layer.surface(),
            record,
        },
    );
    layer.release(app);
    ended
}

fn stream(
    app: &AndroidApp,
    id: String,
    address: String,
    options: Options,
    pin: PathBuf,
    picture: Picture,
) -> Result<()> {
    let lifecycle = Lifecycle::in_process(&id, |event| {
        let event = serde_json::to_string(event).unwrap_or_default();
        logcat::info(&format!("event {event}"));
    })?;
    let runtime = {
        let lifecycle = lifecycle.clone();
        thread::Builder::new()
            .name("viewer".into())
            .spawn(move || {
                launch::run_with_picture(options, Some(&lifecycle), Some(pin), picture)
            })?
    };
    let mut controller = Controller {
        lifecycle,
        launch: id,
        requests: Vec::new(),
    };
    let driven = drive(app, &mut controller, address, &runtime);
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

/// Start, then Stop when the user leaves, until the runtime ends.
fn drive(
    app: &AndroidApp,
    controller: &mut Controller,
    address: String,
    runtime: &JoinHandle<Result<()>>,
) -> Result<()> {
    controller.send(ViewerCommand::Start {
        address,
        display: None,
    })?;
    let mut stopping = false;
    while !runtime.is_finished() {
        let mut stop = false;
        app.poll_events(Some(Duration::from_millis(100)), |event| {
            // Leaving the app ends the stream, as Artemis's Game activity
            // does in onStop: its window and the layer's parent go.
            if let PollEvent::Main(MainEvent::Destroy | MainEvent::TerminateWindow { .. }) = event {
                stop = true;
            }
        });
        stop |= back_pressed(app);
        if stop && !stopping {
            stopping = true;
            controller.send(ViewerCommand::Stop)?;
        }
    }
    Ok(())
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    logcat::panics();
    match run(&app) {
        Ok(()) => logcat::info("viewer ended"),
        Err(error) => logcat::error(&format!("viewer failed: {error:#}")),
    }
}
