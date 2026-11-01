//! The window-backend boundary. The shell core never names a window system;
//! a backend routes each window's host events (keys, focus, close, frames)
//! to the shell and places windows.
//!
//! | Backend | Feature | Windows |
//! | --- | --- | --- |
//! | [`winit`] | `desktop` (default) | one per component: Wayland (sway) or X11 |
//! | [`sdl`] | `webos` | one shared SDL window (`window-sdl/`) |
//! | [`headless`] | tests | one software buffer per component, no display |

#[cfg(feature = "desktop")]
pub mod winit;

#[cfg(feature = "webos")]
pub mod sdl;

pub mod headless;

use crate::input::Key;
use std::rc::Rc;

#[cfg(all(feature = "desktop", feature = "webos"))]
compile_error!(
    "choose one window system: the default `desktop`, or `--no-default-features --features webos`"
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
    /// The window finished drawing a frame. Sent once per window.
    FirstFrame,
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
pub struct FrameCounter(Rc<std::cell::Cell<u64>>);

impl FrameCounter {
    pub fn add(&self) {
        self.0.set(self.0.get() + 1);
    }
    pub fn get(&self) -> u64 {
        self.0.get()
    }
}

pub trait Backend {
    /// True when every component draws into one native window, so a switch
    /// replaces the window's content instead of opening another window.
    fn shared_window(&self) -> bool;

    /// Route the window's host events to `sink`. The sink must not re-enter
    /// the shell synchronously; implementations call it from window
    /// callbacks.
    fn attach(&self, window: &slint::Window, sink: Rc<dyn Fn(HostEvent)>, frames: FrameCounter);

    fn place(&self, window: &slint::Window, placement: Placement);
}
