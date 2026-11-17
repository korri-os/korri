// SPDX-License-Identifier: GPL-3.0-or-later
//! Which code draws the X11 platform's pictures through EGL (#391 step F,
//! slice F3).
//!
//! The carried C is the default: `picture/video/egl.c`, the software
//! picture, and `picture/video/vaapi_egl.c`, the VA-API surface that
//! `vaPutSurface` cannot show (Intel under Xwayland, #477) or that has bars
//! (#476). Their Rust ports (`software.rs`, `vaapi.rs`) are built with the
//! `rust-egl-presenter` feature and chosen with `-eglpresenter rust`. Then
//! Rust owns the five C names (`egl_init`, `egl_draw`, `egl_destroy`,
//! `vaapi_egl_present`, `vaapi_egl_cleanup`), and the C keeps its code under
//! `carried_` names (`build.rs`); the callers, `video/x11.c` and
//! `video/ffmpeg_vaapi.c`, do not change. The window stays Xlib, the place
//! comes from `place.c`, and the frame record's calls stay in the callers.
#[cfg(all(feature = "rust-egl-presenter", not(target_os = "android")))]
mod egl;
#[cfg(all(feature = "rust-egl-presenter", not(target_os = "android")))]
mod ffi;
#[cfg(all(feature = "rust-egl-presenter", not(target_os = "android")))]
mod software;
#[cfg(all(feature = "rust-egl-presenter", not(target_os = "android")))]
mod vaapi;

use anyhow::{Result, bail};

/// `-eglpresenter`'s value: `c`, the default, or `rust`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// The carried C presenters (`picture/video/egl.c`, `vaapi_egl.c`).
    #[default]
    Carried,
    /// Their Rust ports, over the `khronos-egl` and `glow` crates.
    Rust,
}

impl Kind {
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "c" => Ok(Self::Carried),
            #[cfg(all(feature = "rust-egl-presenter", not(target_os = "android")))]
            "rust" => Ok(Self::Rust),
            #[cfg(not(all(feature = "rust-egl-presenter", not(target_os = "android"))))]
            "rust" => bail!(
                "this build has no Rust EGL presenter (cargo feature rust-egl-presenter, Linux only)"
            ),
            _ => bail!("unknown EGL presenter {name}: c or rust"),
        }
    }
}

#[cfg(all(feature = "rust-egl-presenter", not(target_os = "android")))]
pub(crate) use chosen::{failed, select};

#[cfg(all(feature = "rust-egl-presenter", not(target_os = "android")))]
mod chosen {
    use super::{Kind, ffi, software, vaapi};
    use crate::callback::guarded;
    use std::{
        ffi::{c_int, c_ulong, c_void},
        sync::atomic::{AtomicBool, Ordering},
    };

    static RUST: AtomicBool = AtomicBool::new(false);

    /// The presenters of the next stream. Before its decoder's setup, which
    /// calls `egl_init`.
    pub(crate) fn select(kind: Kind) {
        RUST.store(kind == Kind::Rust, Ordering::Relaxed);
        software::reset();
    }

    fn rust() -> bool {
        RUST.load(Ordering::Relaxed)
    }

    /// The Rust software presenter could not start. `egl.c` calls `exit()`
    /// then; the stream's loop ends the stream through its own teardown
    /// instead (`PlayerIo::poll`).
    pub(crate) fn failed() -> bool {
        rust() && software::failed()
    }

    // The carried C, under the names that build.rs gives it.
    unsafe extern "C" {
        pub(super) fn carried_egl_init(
            display: *mut c_void,
            window: c_ulong,
            width: c_int,
            height: c_int,
        );
        pub(super) fn carried_egl_draw(frame: *const ffi::AVFrame, place: ffi::picture_place);
        pub(super) fn carried_egl_destroy();
        pub(super) fn carried_vaapi_egl_present(
            x11: *mut c_void,
            window: c_ulong,
            place: ffi::picture_place,
            picture: *const ffi::VADRMPRIMESurfaceDescriptor,
            frame: *const ffi::AVFrame,
        ) -> bool;
        pub(super) fn carried_vaapi_egl_cleanup();
    }

    // egl.h's and vaapi_egl.h's functions. On X11, EGLNativeDisplayType is
    // `Display*` and NativeWindowType is `Window`, an XID.

    #[unsafe(no_mangle)]
    pub(super) unsafe extern "C" fn egl_init(
        display: *mut c_void,
        window: c_ulong,
        width: c_int,
        height: c_int,
    ) {
        if rust() {
            // SAFETY: x11.c's display and window, live for the stream.
            guarded("picture: EGL init", (), || unsafe {
                software::init(display, window, width, height)
            });
        } else {
            // SAFETY: the same call as x11.c makes without the feature.
            unsafe { carried_egl_init(display, window, width, height) }
        }
    }

    #[unsafe(no_mangle)]
    pub(super) unsafe extern "C" fn egl_draw(
        frame: *const ffi::AVFrame,
        place: ffi::picture_place,
    ) {
        if rust() {
            // SAFETY: x11.c's decoded frame, live for this call.
            guarded("picture: EGL draw", (), || unsafe {
                if let Some(frame) = frame.as_ref() {
                    software::draw(frame, place);
                }
            });
        } else {
            // SAFETY: as in `egl_init`.
            unsafe { carried_egl_draw(frame, place) }
        }
    }

    #[unsafe(no_mangle)]
    pub(super) unsafe extern "C" fn egl_destroy() {
        if rust() {
            guarded("picture: EGL destroy", (), software::destroy);
        } else {
            // SAFETY: as in `egl_init`.
            unsafe { carried_egl_destroy() }
        }
    }

    #[unsafe(no_mangle)]
    pub(super) unsafe extern "C" fn vaapi_egl_present(
        x11: *mut c_void,
        window: c_ulong,
        place: ffi::picture_place,
        picture: *const ffi::VADRMPRIMESurfaceDescriptor,
        frame: *const ffi::AVFrame,
    ) -> bool {
        if rust() {
            // SAFETY: ffmpeg_vaapi.c's display, window, exported surface and
            // frame, live for this call.
            guarded("picture: VA-API EGL present", false, || unsafe {
                match (picture.as_ref(), frame.as_ref()) {
                    (Some(picture), Some(frame)) => {
                        vaapi::present(x11, window, place, picture, frame)
                    }
                    _ => false,
                }
            })
        } else {
            // SAFETY: as in `egl_init`.
            unsafe { carried_vaapi_egl_present(x11, window, place, picture, frame) }
        }
    }

    #[unsafe(no_mangle)]
    pub(super) unsafe extern "C" fn vaapi_egl_cleanup() {
        if rust() {
            guarded("picture: VA-API EGL cleanup", (), vaapi::cleanup);
        } else {
            // SAFETY: as in `egl_init`.
            unsafe { carried_vaapi_egl_cleanup() }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Kind;

    #[test]
    fn the_carried_c_presents_unless_rust_is_named() {
        assert_eq!(Kind::default(), Kind::Carried);
        assert_eq!(Kind::parse("c").unwrap(), Kind::Carried);
        assert!(Kind::parse("glutin").is_err());
        assert_eq!(
            Kind::parse("rust").is_ok(),
            cfg!(feature = "rust-egl-presenter")
        );
        if !cfg!(feature = "rust-egl-presenter") {
            let error = Kind::parse("rust").unwrap_err().to_string();
            assert!(error.contains("rust-egl-presenter"), "{error}");
        }
    }
}

#[cfg(all(test, feature = "rust-egl-presenter"))]
#[path = "egl_presenter/tests.rs"]
mod presenter_tests;
