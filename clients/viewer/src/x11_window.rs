// SPDX-License-Identifier: GPL-3.0-or-later
//! Which code makes the X11 VA-API platform's window and reads its keyboard
//! and mouse (#391 step F, slice F4).
//!
//! The carried C is the default: `picture/video/x11_window.c`, the window
//! part of Moonlight's `x11_setup`, and `io/input/x11.c`, its input. Their
//! Rust port (`window.rs`) is built with the `rust-x11-window` feature and
//! chosen with `-x11window rust`. Then Rust owns `x11_window_open`, and the C
//! keeps it as `carried_x11_window_open` (`build.rs`); the caller,
//! `video/x11.c`, does not change. It keeps its Xlib display for VA-API and
//! for the presenter; the Rust window has an X11 connection of its own,
//! through x11rb, and adds its handler to the same descriptor loop.
#[cfg(all(feature = "rust-x11-window", not(target_os = "android")))]
mod ffi;
#[cfg(all(feature = "rust-x11-window", not(target_os = "android")))]
mod keyboard;
#[cfg(all(feature = "rust-x11-window", not(target_os = "android")))]
mod window;

use anyhow::{Result, bail};

/// `-x11window`'s value: `c`, the default, or `rust`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// The carried C (`picture/video/x11_window.c`, `io/input/x11.c`).
    #[default]
    Carried,
    /// Its Rust port, over the `x11rb` crate.
    Rust,
}

impl Kind {
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "c" => Ok(Self::Carried),
            #[cfg(all(feature = "rust-x11-window", not(target_os = "android")))]
            "rust" => Ok(Self::Rust),
            #[cfg(not(all(feature = "rust-x11-window", not(target_os = "android"))))]
            "rust" => bail!(
                "this build has no Rust X11 window (cargo feature rust-x11-window, Linux only)"
            ),
            _ => bail!("unknown X11 window {name}: c or rust"),
        }
    }
}

#[cfg(all(feature = "rust-x11-window", not(target_os = "android")))]
pub(crate) use chosen::{close, failed, select};

#[cfg(all(feature = "rust-x11-window", not(target_os = "android")))]
mod chosen {
    use super::{Kind, window};
    use crate::callback::guarded;
    use std::{
        ffi::{c_int, c_ulong, c_void},
        sync::atomic::{AtomicBool, Ordering},
    };

    static RUST: AtomicBool = AtomicBool::new(false);

    /// The window code of the next stream. Before its decoder's setup,
    /// which calls `x11_window_open`.
    pub(crate) fn select(kind: Kind) {
        RUST.store(kind == Kind::Rust, Ordering::Relaxed);
        window::reset();
    }

    fn rust() -> bool {
        RUST.load(Ordering::Relaxed)
    }

    /// The Rust window could not open. x11.c would go on with window 0,
    /// and Xlib would end the process at its first call on it: the stream's
    /// loop ends the stream through its teardown first (`PlayerIo::poll`).
    pub(crate) fn failed() -> bool {
        rust() && window::failed()
    }

    /// The Rust window and its connection go, as the C's go with its
    /// display (`XCloseDisplay` in `x11_cleanup`).
    pub(crate) fn close() {
        window::close();
    }

    // The carried C, under the name that build.rs gives it.
    unsafe extern "C" {
        pub(super) fn carried_x11_window_open(
            display: *mut c_void,
            width: c_int,
            height: c_int,
            flags: c_int,
        ) -> c_ulong;
    }

    /// x11_window.h's `x11_window_open`: `Display*` in, the window's XID
    /// out.
    #[unsafe(no_mangle)]
    pub(super) unsafe extern "C" fn x11_window_open(
        display: *mut c_void,
        width: c_int,
        height: c_int,
        flags: c_int,
    ) -> c_ulong {
        if rust() {
            // SAFETY: x11.c's live display.
            guarded("picture: X11 window", 0, || unsafe {
                window::open(display, width, height, flags)
            })
        } else {
            // SAFETY: the same call as x11.c makes without the feature.
            unsafe { carried_x11_window_open(display, width, height, flags) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Kind;

    #[test]
    fn the_carried_c_makes_the_window_unless_rust_is_named() {
        assert_eq!(Kind::default(), Kind::Carried);
        assert_eq!(Kind::parse("c").unwrap(), Kind::Carried);
        assert!(Kind::parse("winit").is_err());
        assert_eq!(
            Kind::parse("rust").is_ok(),
            cfg!(feature = "rust-x11-window")
        );
        if !cfg!(feature = "rust-x11-window") {
            let error = Kind::parse("rust").unwrap_err().to_string();
            assert!(error.contains("rust-x11-window"), "{error}");
        }
    }
}

#[cfg(all(test, feature = "rust-x11-window"))]
#[path = "x11_window/tests.rs"]
mod window_tests;
