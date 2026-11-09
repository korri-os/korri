//! The window-backend boundary. The shell core never names a window system;
//! a backend routes each window's host events (keys, focus, close, frames)
//! to the shell and places windows.
//!
//! | Backend | Feature | Windows |
//! | --- | --- | --- |
//! | [`winit`] | `desktop` (default) | one per component: Wayland (sway) or X11 |
//! | [`sdl`] | `webos` | one shared SDL window (`window-sdl/`) |
//! | Android | `android` | one NativeActivity window |
//! | [`headless`] | tests | one software buffer per component, no display |

#[cfg(feature = "desktop")]
pub mod winit;

#[cfg(feature = "webos")]
pub mod sdl;

#[cfg(all(feature = "android", target_os = "android"))]
pub mod android;

pub mod headless;

use crate::input::Key;
use std::rc::Rc;

#[cfg(any(
    all(feature = "desktop", feature = "webos"),
    all(feature = "android", any(feature = "desktop", feature = "webos"))
))]
compile_error!(
    "choose one window system: desktop, or --no-default-features --features webos or android"
);

/// What a window reports to the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostEvent {
    Key {
        key: Key,
        pressed: bool,
    },
    Focus(bool),
    Close,
    /// The window finished drawing a frame. Sent once per window, and once more each
    /// time the shell attaches it again.
    FirstFrame,
    /// The window's size or scale factor changed. The shell reads the new
    /// logical size from the window when it handles this.
    Resized,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Fullscreen,
    Windowed { width: u32, height: u32 },
}

impl Placement {
    pub fn parse(text: &str) -> Option<Self> {
        if text == "fullscreen" {
            return Some(Self::Fullscreen);
        }
        let (width, height) = text.split_once('x')?;
        let (width, height) = (width.parse().ok()?, height.parse().ok()?);
        (width > 0 && height > 0 && width <= 16384 && height <= 16384)
            .then_some(Self::Windowed { width, height })
    }
}

/// Counts every frame drawn by any window, for `--stats`.
#[derive(Clone, Default)]
pub struct FrameCounter(Rc<std::cell::Cell<u64>>, Rc<std::cell::RefCell<Intervals>>);

/// Times between frames, for `--stats`: a frame that takes 4 ms to draw but
/// arrives every 33 ms waits somewhere else (present, compositor, vsync).
#[derive(Default)]
struct Intervals {
    last: Option<std::time::Instant>,
    millis: Vec<f32>,
}

impl FrameCounter {
    pub fn add(&self) {
        self.0.set(self.0.get() + 1);
        let mut intervals = self.1.borrow_mut();
        let now = std::time::Instant::now();
        if let Some(last) = intervals.last.replace(now) {
            let ms = now.duration_since(last).as_secs_f32() * 1000.0;
            if intervals.millis.len() < 4096 {
                intervals.millis.push(ms);
            }
        }
    }
    pub fn get(&self) -> u64 {
        self.0.get()
    }
    /// p50, p95 and max of the frame intervals since the last call, in ms.
    pub fn take_intervals(&self) -> Option<(f32, f32, f32)> {
        let mut intervals = self.1.borrow_mut();
        let mut values = std::mem::take(&mut intervals.millis);
        if values.is_empty() {
            return None;
        }
        values.sort_by(f32::total_cmp);
        let at = |p: f32| values[((values.len() - 1) as f32 * p).round() as usize];
        Some((at(0.5), at(0.95), *values.last().expect("not empty")))
    }
}

pub trait Backend {
    /// Only an embedded graphics-plane backend needs to react to local video.
    /// Desktop viewers retain their own window and existing focus lifecycle.
    fn local_viewer(&self, _presenting: bool, _gameplay_overlay: bool) {}

    /// True when every component draws into one native window, so a switch
    /// replaces the window's content instead of opening another window.
    fn shared_window(&self) -> bool;

    /// Route the window's host events to `sink`. The sink must not re-enter
    /// the shell synchronously; implementations call it from window
    /// callbacks.
    /// Attaching a window again replaces its sink and makes its next drawn frame a
    /// `FirstFrame` again (the shell does so when it maps the window anew).
    fn attach(&self, window: &slint::Window, sink: Rc<dyn Fn(HostEvent)>, frames: FrameCounter);

    fn place(&self, window: &slint::Window, placement: Placement);
}
