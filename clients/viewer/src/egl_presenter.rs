// SPDX-License-Identifier: GPL-3.0-or-later
//! Which code draws the X11 VA-API platform's pictures through EGL (#391
//! step F, slice F3).
//!
//! The carried C, `picture/video/vaapi_egl.c`, presents the VA-API surface
//! that `vaPutSurface` cannot show (Intel under Xwayland, #477) or that has
//! bars (#476). Its Rust port (`vaapi.rs`) is built with the
//! `rust-egl-presenter` feature, and is then the default (ADR 0062: it
//! measured equal); `-eglpresenter c` chooses the carried C. With the Rust port,
//! Rust owns the two C names (`vaapi_egl_present`, `vaapi_egl_cleanup`), and
//! the C keeps its code under `carried_` names (`build.rs`); the caller,
//! `video/ffmpeg_vaapi.c`, does not change. The window stays Xlib
//! (`video/x11.c`), the place comes from `place.c`, and the frame record's
//! calls stay in the caller. `Filled` (slice F5) chooses whether the
//! presenter shows the pictures that fill the window too.
#[cfg(all(feature = "rust-egl-presenter", not(target_os = "android")))]
mod egl;
#[cfg(all(feature = "rust-egl-presenter", not(target_os = "android")))]
mod ffi;
#[cfg(all(feature = "rust-egl-presenter", not(target_os = "android")))]
mod vaapi;

use anyhow::{Result, bail};

/// `-eglpresenter`'s value: `c` or `rust`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The carried C presenter (`picture/video/vaapi_egl.c`).
    Carried,
    /// Its Rust port, over the `khronos-egl` and `glow` crates.
    Rust,
}

impl Default for Kind {
    /// The Rust port where the build has it (#391 step F3: it measured
    /// equal, ADR 0062); else the carried C.
    fn default() -> Self {
        if cfg!(all(
            feature = "rust-egl-presenter",
            not(target_os = "android")
        )) {
            Self::Rust
        } else {
            Self::Carried
        }
    }
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

/// `-filledpicture`'s value (#391 step F, slice F5): what shows an X11
/// VA-API picture that fills its window. A picture with bars goes through
/// the EGL presenter either way (#476), and so does every picture where
/// `vaPutSurface` cannot show one (#477).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filled {
    /// `vaPutSurface`, as before (`video/ffmpeg_vaapi.c`).
    #[default]
    PutSurface,
    /// The EGL presenter that `-eglpresenter` chooses, as for a picture
    /// with bars.
    Egl,
}

impl Filled {
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "putsurface" => Ok(Self::PutSurface),
            "egl" => Ok(Self::Egl),
            _ => bail!(
                "unknown presenter {name} for a picture that fills its window: putsurface or egl"
            ),
        }
    }
}

#[cfg(all(feature = "rust-egl-presenter", not(target_os = "android")))]
pub(crate) use chosen::select;

#[cfg(all(feature = "rust-egl-presenter", not(target_os = "android")))]
mod chosen {
    use super::{Kind, ffi, vaapi};
    use crate::callback::guarded;
    use std::{
        ffi::{c_ulong, c_void},
        sync::atomic::{AtomicBool, Ordering},
    };

    static RUST: AtomicBool = AtomicBool::new(false);

    /// The presenter of the next stream. Before its first frame.
    pub(crate) fn select(kind: Kind) {
        RUST.store(kind == Kind::Rust, Ordering::Relaxed);
    }

    fn rust() -> bool {
        RUST.load(Ordering::Relaxed)
    }

    // The carried C, under the names that build.rs gives it.
    unsafe extern "C" {
        pub(super) fn carried_vaapi_egl_present(
            x11: *mut c_void,
            window: c_ulong,
            place: ffi::picture_place,
            picture: *const ffi::VADRMPRIMESurfaceDescriptor,
            frame: *const ffi::AVFrame,
        ) -> bool;
        pub(super) fn carried_vaapi_egl_cleanup();
    }

    // vaapi_egl.h's functions. On X11 the display is an Xlib `Display*` and
    // the window an XID.

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
            // SAFETY: the same call as ffmpeg_vaapi.c makes without the
            // feature.
            unsafe { carried_vaapi_egl_present(x11, window, place, picture, frame) }
        }
    }

    #[unsafe(no_mangle)]
    pub(super) unsafe extern "C" fn vaapi_egl_cleanup() {
        if rust() {
            guarded("picture: VA-API EGL cleanup", (), vaapi::cleanup);
        } else {
            // SAFETY: as in `vaapi_egl_present`.
            unsafe { carried_vaapi_egl_cleanup() }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Kind;

    // ADR 0062: the Rust port measured equal (#391 step F3), so a build
    // with it presents with it; `-eglpresenter c` keeps the carried C.
    #[test]
    fn the_rust_port_presents_where_built_unless_c_is_named() {
        let built = cfg!(all(
            feature = "rust-egl-presenter",
            not(target_os = "android")
        ));
        assert_eq!(
            Kind::default(),
            if built { Kind::Rust } else { Kind::Carried }
        );
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
