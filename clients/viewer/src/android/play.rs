// SPDX-License-Identifier: GPL-3.0-or-later
//! One stream's input on an app's thread, with the gameplay overlay
//! (slices D4 and D5): each key and motion event goes to the stream's input
//! (`input.rs`, Artemis's mapping) or to the overlay, which Guide or Back
//! opens and closes as Artemis's Korri mode does. While the overlay is open
//! the stream's input is held (`Input::hold`), and the overlay takes every
//! key and motion event but Android's volume and power keys.
//!
//! The thread that owns the app's events calls it: the viewer's own loop
//! (`clients/viewer/android`), or the shell's input filter and timer in the
//! Korri app (`clients/android-app`). Either way it runs on one thread, and
//! never waits.
use crate::{
    devices::{AndroidDevices, now_ms},
    events,
    feedback::Seats,
    input::{CoreSink, Input, Outcome, Settings, core_sink},
    layer::Layers,
    log,
    overlay::{Keys, Overlay, Route},
    placement::Rect,
};
use android_activity::{
    AndroidApp,
    input::{KeyEvent, Keycode, MotionEvent},
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    time::Instant,
};

/// What became of one event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Routed {
    /// The stream or the overlay took it.
    Handled,
    /// Neither did: Android's (the volume and power keys under the overlay,
    /// or a key that the mapping does not know).
    Unhandled,
    /// The person leaves the stream (Ctrl+Alt+Shift+Q).
    Leave,
}

/// The stream's input and overlay, from the stream's start to its stop.
pub struct Play {
    input: Guarded,
    // The core's input behind its gate: closed before the core stops, also
    // when the host or the network ends the stream while this thread sends.
    sink: CoreSink,
    devices: AndroidDevices,
    gameplay: Gameplay,
    /// When the attached devices were last asked for.
    checked: i64,
    ended: bool,
}

impl Play {
    /// `attached` is the launch's controller mask (`input::attached`), read
    /// from `devices` before the runtime started. `seats`: where the input
    /// tells the controllers' feedback which controller is which
    /// (`feedback::Worker::seats`).
    pub fn new(
        settings: Settings,
        attached: u16,
        placed: Rect,
        devices: AndroidDevices,
        seats: Box<dyn Seats>,
    ) -> Self {
        Self {
            input: Guarded(Some(Input::new(settings, attached, placed, seats))),
            sink: core_sink(),
            devices,
            gameplay: Gameplay::default(),
            checked: now_ms(),
            ended: false,
        }
    }

    pub fn key(&mut self, app: &AndroidApp, layers: &Layers, key: &KeyEvent) -> Routed {
        let event = events::key(key);
        let Self {
            input,
            sink,
            devices,
            gameplay,
            ..
        } = self;
        match gameplay.keys.route(&event, gameplay.overlay) {
            Route::Android => Routed::Unhandled,
            Route::Taken => {
                if gameplay.overlay == Overlay::Open {
                    gameplay.taken_keys += 1;
                }
                Routed::Handled
            }
            Route::Open => {
                gameplay.open(toggle_name(key.key_code()), app, layers, input, sink);
                Routed::Handled
            }
            Route::Close => {
                gameplay.close(toggle_name(key.key_code()), app, layers, input, sink);
                Routed::Handled
            }
            Route::Stream => {
                let outcome = input
                    .run("key", |input| input.key(&event, now_ms(), devices, sink))
                    .unwrap_or(Outcome::Unhandled);
                match outcome {
                    Outcome::Handled => Routed::Handled,
                    Outcome::Leave => Routed::Leave,
                    Outcome::Overlay => {
                        gameplay.open("Start pressed twice and held", app, layers, input, sink);
                        Routed::Handled
                    }
                    Outcome::Unhandled => Routed::Unhandled,
                }
            }
        }
    }

    pub fn motion(&mut self, motion: &MotionEvent) -> Routed {
        // The overlay takes every motion event while it is open.
        if self.gameplay.overlay == Overlay::Open {
            self.gameplay.taken_motions += 1;
            return Routed::Handled;
        }
        let event = events::motion(motion);
        let Self {
            input,
            sink,
            devices,
            ..
        } = self;
        let handled = input
            .run("motion", |input| {
                input.motion(&event, now_ms(), devices, sink)
            })
            .unwrap_or(false);
        if handled {
            Routed::Handled
        } else {
            Routed::Unhandled
        }
    }

    /// The input's timers (a touch's click, the emulated mouse, the
    /// controllers' heartbeat), and twice a second the attached devices:
    /// Android tells no native code that a device went.
    pub fn tick(&mut self) {
        let Self {
            input,
            sink,
            devices,
            checked,
            ..
        } = self;
        input.run("tick", |input| input.tick(now_ms(), sink));
        if now_ms() - *checked >= 500 {
            *checked = now_ms();
            input.run("devices", |input| input.check_devices(devices, sink));
        }
    }

    /// When the input's next timer is due, in `now_ms` time.
    pub fn next_deadline(&self) -> Option<i64> {
        self.input.0.as_ref().and_then(Input::next_deadline)
    }

    /// The video's new place in the window.
    pub fn place(&mut self, placed: Rect) {
        self.input.run("place", |input| input.place(placed));
    }

    /// The stream stops: nothing more goes to it. The first call logs what
    /// the input did; a later one does nothing.
    pub fn end(&mut self) {
        if std::mem::replace(&mut self.ended, true) {
            return;
        }
        if self.gameplay.overlay == Overlay::Open {
            log::line("overlay: open when the stream stopped");
        }
        if let Some(input) = self.input.0.take() {
            log::line(&format!("input: ended: {}", input.report()));
        }
    }

    /// Whether the gameplay overlay is open.
    pub fn overlay_open(&self) -> bool {
        self.gameplay.overlay == Overlay::Open
    }
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
                log::line(&format!(
                    "input: {what} panicked; the stream goes on without input"
                ));
                self.0 = None;
                None
            }
        }
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
        layers: &Layers,
        input: &mut Guarded,
        sink: &mut CoreSink,
    ) {
        if self.overlay == Overlay::Open {
            return;
        }
        if !layers.has_overlay() {
            log::line(&format!("overlay: {why}, but there is no overlay layer"));
            return;
        }
        self.overlay = Overlay::Open;
        self.opened = Some(Instant::now());
        input.run("hold", |input| input.hold(sink));
        layers.show_overlay(app, true);
        log::line(&format!(
            "overlay: open on {why}; the stream's input is held"
        ));
    }

    fn close(
        &mut self,
        why: &str,
        app: &AndroidApp,
        layers: &Layers,
        input: &mut Guarded,
        sink: &mut CoreSink,
    ) {
        if self.overlay == Overlay::Closed {
            return;
        }
        layers.show_overlay(app, false);
        input.run("release", |input| input.release(sink));
        self.overlay = Overlay::Closed;
        let open = self
            .opened
            .take()
            .map_or(0.0, |at| at.elapsed().as_secs_f64());
        log::line(&format!(
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
