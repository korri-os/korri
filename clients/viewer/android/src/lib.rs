// SPDX-License-Identifier: GPL-3.0-or-later
//! The Rust viewer as an Android app, slice D1 of #391 step D: Android's own
//! `NativeActivity` runs one launch of the shared viewer in this process
//! (ADR 0060), with the in-process lifecycle in the place of korrid's socket.
//! The app is that lifecycle's controller: it sends Start when it starts and
//! Stop on Back, and writes every event and reply to logcat. The network
//! core's empty decoder and audio callbacks run; there is no picture, sound
//! or input yet (D2 to D4).
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
#![cfg(target_os = "android")]

mod intent;
mod logcat;

use android_activity::{
    AndroidApp, InputStatus, MainEvent, PollEvent,
    input::{InputEvent, KeyAction, Keycode},
};
use anyhow::{Context, Result, anyhow};
use korri_viewer::{launch, lifecycle::Lifecycle, options::Options};
use korri_viewer_treaty::{ViewerCommand, ViewerRequest};
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

fn run(app: &AndroidApp) -> Result<()> {
    let Launch {
        id,
        address,
        options,
        pin,
    } = Launch::from_intent(app)?;
    let lifecycle = Lifecycle::in_process(&id, |event| {
        let event = serde_json::to_string(event).unwrap_or_default();
        logcat::info(&format!("event {event}"));
    })?;
    let runtime = {
        let lifecycle = lifecycle.clone();
        thread::Builder::new()
            .name("viewer".into())
            .spawn(move || launch::run(options, Some(&lifecycle), Some(pin)))?
    };
    let mut controller = Controller {
        lifecycle,
        launch: id,
        requests: Vec::new(),
    };
    controller.send(ViewerCommand::Start {
        address,
        display: None,
    })?;
    let mut stopping = false;
    while !runtime.is_finished() {
        let mut stop = false;
        app.poll_events(Some(Duration::from_millis(100)), |event| {
            if let PollEvent::Main(MainEvent::Destroy) = event {
                stop = true;
            }
        });
        stop |= back_pressed(app);
        if stop && !stopping {
            stopping = true;
            controller.send(ViewerCommand::Stop)?;
        }
    }
    let ended = runtime
        .join()
        .map_err(|_| anyhow!("the viewer thread panicked"))?;
    controller.join();
    ended
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    logcat::panics();
    match run(&app) {
        Ok(()) => logcat::info("viewer ended"),
        Err(error) => logcat::error(&format!("viewer failed: {error:#}")),
    }
}
