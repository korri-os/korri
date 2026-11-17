// SPDX-License-Identifier: GPL-3.0-or-later
//! What `vaapi_egl.c` sets up: an EGL display of an X11 connection, then an
//! OpenGL ES 2 context and a window surface on an X11 window, with OpenGL
//! ES's calls through glow. Korri's own code.
use khronos_egl as egl;
use std::ffi::{c_ulong, c_void};

/// libEGL, linked as the carried C links it.
pub(super) fn api() -> &'static egl::Instance<egl::Static> {
    &egl::API
}

/// The step that gave no window. The presenter words it as its C does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Display,
    Initialize,
    Config,
    Api,
    Context,
    Surface,
    Current,
}

/// A step that failed, and EGL's error code for it. khronos-egl reads
/// `eglGetError` itself when a call fails, which clears it: the code comes
/// with the call's error, not from a later `eglGetError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Failure {
    pub(super) step: Step,
    pub(super) code: i32,
}

impl Failure {
    /// A call that failed with `error`.
    fn at(step: Step) -> impl FnOnce(egl::Error) -> Self {
        move |error| Self {
            step,
            code: error.native(),
        }
    }

    /// A call whose error khronos-egl did not read: it is still pending.
    fn pending(step: Step) -> Self {
        Self {
            step,
            code: error_code(),
        }
    }
}

/// The pending EGL error of this thread, as `eglGetError` gives it.
pub(super) fn error_code() -> i32 {
    api()
        .get_error()
        .map_or(egl::SUCCESS, |error| error.native())
}

/// An initialized EGL display. It is terminated when dropped, unless it
/// became a window's.
pub(super) struct Display(Option<egl::Display>);

impl Display {
    /// # Safety
    /// `x11` is a live Xlib `Display*`.
    pub(super) unsafe fn open(x11: *mut c_void) -> Result<Self, Failure> {
        // SAFETY: the caller's display.
        let display =
            unsafe { api().get_display(x11) }.ok_or_else(|| Failure::pending(Step::Display))?;
        api()
            .initialize(display)
            .map_err(Failure::at(Step::Initialize))?;
        Ok(Self(Some(display)))
    }

    fn get(&self) -> egl::Display {
        self.0.expect("a display until it becomes a window's")
    }

    /// The display's extensions, or EGL's error code.
    pub(super) fn extensions(&self) -> Result<String, i32> {
        api()
            .query_string(Some(self.get()), egl::EXTENSIONS)
            .map(|s| s.to_string_lossy().into_owned())
            .map_err(|error| error.native())
    }

    /// A context of OpenGL ES 2 from the first config that matches
    /// `config`, and a surface on `window`, current on this thread.
    pub(super) fn window(mut self, window: c_ulong, config: &[i32]) -> Result<Window, Failure> {
        let display = self.get();
        let config = api()
            .choose_first_config(display, config)
            .map_err(Failure::at(Step::Config))?
            .ok_or_else(|| Failure::pending(Step::Config))?;
        api()
            .bind_api(egl::OPENGL_ES_API)
            .map_err(Failure::at(Step::Api))?;
        let context = api()
            .create_context(
                display,
                config,
                None,
                &[egl::CONTEXT_CLIENT_VERSION, 2, egl::NONE],
            )
            .map_err(Failure::at(Step::Context))?;
        // SAFETY: the caller's X11 window on this display; an XID.
        let surface = unsafe {
            api().create_window_surface(display, config, window as egl::NativeWindowType, None)
        };
        let surface = match surface {
            Ok(surface) => surface,
            Err(error) => {
                let failure = Failure::at(Step::Surface)(error);
                let _ = api().destroy_context(display, context);
                return Err(failure);
            }
        };
        if let Err(error) = api().make_current(display, Some(surface), Some(surface), Some(context))
        {
            let failure = Failure::at(Step::Current)(error);
            let _ = api().destroy_surface(display, surface);
            let _ = api().destroy_context(display, context);
            return Err(failure);
        }
        self.0 = None;
        // SAFETY: the context is current on this thread; glow reads its
        // version and extensions now.
        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                api()
                    .get_proc_address(name)
                    .map_or(std::ptr::null(), |f| f as *const c_void)
            })
        };
        Ok(Window {
            display,
            context,
            surface,
            gl,
        })
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        if let Some(display) = self.0 {
            let _ = api().terminate(display);
        }
    }
}

/// A context and a surface on one X11 window, and OpenGL ES through them.
/// Dropped, it releases both and terminates the display, as
/// `vaapi_egl_cleanup` does.
pub(super) struct Window {
    pub(super) display: egl::Display,
    pub(super) context: egl::Context,
    pub(super) surface: egl::Surface,
    pub(super) gl: glow::Context,
}

// SAFETY: EGL's handles belong to the process. One thread at a time uses a
// window, behind the presenter's lock.
unsafe impl Send for Window {}

impl Window {
    pub(super) fn release(&self) {
        let _ = api().make_current(self.display, None, None, None);
    }

    /// The frame went to the window, or EGL's error code.
    pub(super) fn swap(&self) -> Result<(), i32> {
        api()
            .swap_buffers(self.display, self.surface)
            .map_err(|error| error.native())
    }

    /// An EGL function that `eglGetProcAddress` gives.
    pub(super) fn proc_address(&self, name: &str) -> Option<extern "system" fn()> {
        api().get_proc_address(name)
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.release();
        let _ = api().destroy_surface(self.display, self.surface);
        let _ = api().destroy_context(self.display, self.context);
        let _ = api().terminate(self.display);
    }
}
