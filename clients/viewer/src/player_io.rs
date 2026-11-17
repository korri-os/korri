// SPDX-License-Identifier: GPL-3.0-or-later
//! One session's carried presenters, audio callbacks and input discovery.
use crate::{audio_output, network as ffi, options::Options};
use anyhow::{Result, ensure};
use std::ffi::CString;

/// What the platform gives one stream: nothing on Linux. The presenters
/// make their own window, and the frame record's ask is in the environment.
#[derive(Default)]
pub(crate) struct Platform;

pub(crate) struct PlayerIo {
    // C keeps these pointers through connection teardown.
    _mapping: Option<CString>,
    audio_device: Option<CString>,
    vaapi: bool,
    /// Moonlight's `-platform v4l2request`, the handheld path.
    request: bool,
    /// The carried C audio renderers, or their Rust port (#391 step F).
    audio_output: audio_output::Kind,
    #[cfg(v4l2request)]
    _inputs: Vec<CString>,
}
impl PlayerIo {
    pub(crate) fn new(o: &Options, _: Platform) -> Result<Self> {
        let mapping = o.mapping.as_deref().map(CString::new).transpose()?;
        let audio_device = o.audio_device.as_deref().map(CString::new).transpose()?;
        #[cfg(v4l2request)]
        if o.platform == "v4l2request" {
            return Self::request(o, mapping, audio_device);
        }
        // The X11 VA-API presenter's EGL code for this stream (#391 step F):
        // its first frame calls it, after this.
        #[cfg(feature = "rust-egl-presenter")]
        crate::egl_presenter::select(o.egl_presenter);
        // The X11 VA-API window's code for this stream (#391 step F): the
        // decoder's setup calls it, after this.
        #[cfg(feature = "rust-x11-window")]
        crate::x11_window::select(o.x11_window.unwrap_or_default());
        // What shows a picture that fills the window, for this stream (#391
        // step F5): vaPutSurface, or EGL as for one with bars.
        // SAFETY: a flag that ffmpeg_vaapi.c reads at each frame, set before
        // the first.
        unsafe {
            ffi::picture_vaapi_egl_for_filled(i32::from(
                o.filled_picture == crate::egl_presenter::Filled::Egl,
            ))
        };
        // SAFETY: called under stream's connection lock, on its event thread.
        let vaapi = unsafe {
            ffi::picture_init(
                i32::from(o.platform == "x11_vaapi"),
                o.width,
                o.height,
                i32::from(o.fullscreen),
            ) != 0
        };
        let io = Self {
            _mapping: mapping,
            audio_device,
            vaapi,
            request: false,
            audio_output: o.audio_output,
            #[cfg(v4l2request)]
            _inputs: Vec::new(),
        };
        ensure!(
            vaapi || io.audio_device.is_none(),
            "You can't select an audio device for SDL"
        );
        ensure!(
            x11_window_fits(o.x11_window, vaapi),
            "the Rust X11 window serves X11 VA-API, and this stream's picture is SDL's"
        );
        ensure!(
            vaapi || o.filled_picture == crate::egl_presenter::Filled::PutSurface,
            "-filledpicture egl serves X11 VA-API, and this stream's picture is SDL's"
        );
        let mapping = io
            ._mapping
            .as_ref()
            .map_or(std::ptr::null_mut(), |s| s.as_ptr().cast_mut());
        let result = unsafe {
            ffi::viewer_input_init(
                i32::from(vaapi),
                i32::from(o.viewonly),
                mapping,
                i32::from(o.mouse_emulation),
                i32::from(o.verbose),
                o.rotation,
            )
        };
        ensure!(result == 0, "input initialization failed");
        Ok(io)
    }
    /// Moonlight's `-platform v4l2request`: the v4l2request decoder, SDL
    /// audio, and evdev input from korrid's explicit nodes. Moonlight refuses
    /// an audio device only on its SDL platform; SDL audio ignores it here.
    #[cfg(v4l2request)]
    fn request(
        o: &Options,
        mapping: Option<CString>,
        audio_device: Option<CString>,
    ) -> Result<Self> {
        ensure!(
            x11_window_fits(o.x11_window, false),
            "the Rust X11 window serves X11 VA-API, and v4l2request makes its own window"
        );
        ensure!(
            o.filled_picture == crate::egl_presenter::Filled::PutSurface,
            "-filledpicture egl serves X11 VA-API, and v4l2request shows its own pictures"
        );
        let inputs = o
            .inputs
            .iter()
            .map(|i| CString::new(i.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        // SAFETY: called under stream's connection lock, before the core and
        // the decoder's display thread start.
        unsafe { ffi::picture_init_request(i32::from(o.auto_window_resize)) };
        let io = Self {
            _mapping: mapping,
            audio_device,
            vaapi: false,
            request: true,
            audio_output: o.audio_output,
            _inputs: inputs,
        };
        let mapping = io
            ._mapping
            .as_ref()
            .map_or(std::ptr::null_mut(), |s| s.as_ptr().cast_mut());
        let mut nodes: Vec<_> = io._inputs.iter().map(|s| s.as_ptr().cast_mut()).collect();
        // SAFETY: C reads the node strings during this call only.
        let result = unsafe {
            ffi::viewer_input_init_evdev(
                i32::from(o.viewonly),
                mapping,
                nodes.as_mut_ptr(),
                nodes.len() as i32,
                i32::from(o.mouse_emulation),
                i32::from(o.verbose),
                o.rotation,
            )
        };
        ensure!(result == 0, "input initialization failed");
        Ok(io)
    }
    /// Whether the picture follows a new stream size in the middle of the
    /// stream (#476): X11 VA-API and the SDL fallback fit each new size in
    /// their window. The handheld path keeps the size it started with.
    pub(crate) fn follows_new_sizes(&self) -> bool {
        !self.request
    }
    /// The size of the picture that the presenter showed last; none before
    /// the first. The presenters run on the stream's thread, inside `poll`.
    pub(crate) fn shown_size(&self) -> Option<korri_viewer_treaty::ViewerDisplay> {
        let (mut width, mut height) = (0, 0);
        // SAFETY: C writes the two integers only.
        let shown = unsafe { ffi::picture_shown_size(&mut width, &mut height) } != 0;
        (shown && width > 0 && height > 0).then_some(korri_viewer_treaty::ViewerDisplay {
            width: width as u32,
            height: height as u32,
        })
    }
    pub(crate) fn gamepad_mask(&self) -> i32 {
        unsafe { ffi::viewer_input_gamepad_mask() }
    }
    pub(crate) fn decoder_callbacks(&self) -> *mut ffi::DECODER_RENDERER_CALLBACKS {
        unsafe { ffi::picture_callbacks() }
    }
    /// The audio renderer table, chosen as Moonlight's `platform.c` chooses:
    /// SDL audio unless the platform is X11 VA-API, then PulseAudio when a
    /// probe stream opens, else ALSA. The Rust port chooses the same way.
    pub(crate) fn audio_callbacks(&self) -> *mut ffi::AUDIO_RENDERER_CALLBACKS {
        match self.audio_output {
            audio_output::Kind::Carried => unsafe {
                ffi::viewer_audio_callbacks(i32::from(self.vaapi), self.audio_context().cast())
            },
            #[cfg(feature = "rust-audio-output")]
            audio_output::Kind::Rust => {
                audio_output::renderer::callbacks(self.vaapi, self.audio_device.as_deref())
            }
            #[cfg(not(feature = "rust-audio-output"))]
            audio_output::Kind::Rust => {
                unreachable!("audio_output::Kind::parse refuses rust in this build")
            }
        }
    }
    pub(crate) fn audio_context(&self) -> *mut std::ffi::c_void {
        self.audio_device
            .as_ref()
            .map_or(std::ptr::null_mut(), |s| s.as_ptr().cast_mut().cast())
    }
    pub(crate) fn connection_callbacks(&self, callbacks: &mut ffi::CONNECTION_LISTENER_CALLBACKS) {
        callbacks.logMessage = Some(crate::log::viewer_core_log);
        unsafe { ffi::viewer_input_callbacks(callbacks) };
    }
    pub(crate) fn start(&self) {
        unsafe { ffi::viewer_input_start() };
    }
    pub(crate) fn stop(&self) {
        unsafe { ffi::viewer_input_stop() };
    }
    /// One turn of the presenter's or the input's event loop: 0 to go on, a
    /// positive value when the window asks to quit, negative on failure.
    pub(crate) fn poll(&self, timeout_ms: i32) -> i32 {
        // The Rust X11 window did not open: x11.c would show frames on
        // window 0, and Xlib would end the process. The stream ends here,
        // through its teardown, before a frame comes.
        #[cfg(feature = "rust-x11-window")]
        if crate::x11_window::failed() {
            return -1;
        }
        unsafe { ffi::picture_poll(timeout_ms) }
    }
}
/// Whether the X11 window that `-x11window` names fits a stream whose
/// picture is X11 VA-API's (`vaapi`) or not: SDL's (`-platform x11`, or a
/// VA-API start that failed) and v4l2request's make no X11 window, so only a
/// named `-x11window rust` is refused there. The build's default, the Rust
/// window where it is built (#391 step F4), goes unused.
fn x11_window_fits(named: Option<crate::x11_window::Kind>, vaapi: bool) -> bool {
    vaapi || named != Some(crate::x11_window::Kind::Rust)
}

impl Drop for PlayerIo {
    fn drop(&mut self) {
        // The caller stops and joins the core before dropping the session's I/O.
        unsafe {
            ffi::viewer_input_destroy();
        }
        // The Rust X11 window's connection leaves the descriptor loop before
        // the loop goes; the C's window went with x11.c's display.
        #[cfg(feature = "rust-x11-window")]
        crate::x11_window::close();
        unsafe {
            ffi::picture_destroy();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::x11_window_fits;
    use crate::x11_window::Kind;

    #[test]
    fn a_picture_that_is_not_x11_vaapis_refuses_only_a_named_rust_window() {
        for named in [None, Some(Kind::Carried), Some(Kind::Rust)] {
            assert!(x11_window_fits(named, true), "{named:?}");
        }
        assert!(x11_window_fits(None, false));
        assert!(x11_window_fits(Some(Kind::Carried), false));
        assert!(!x11_window_fits(Some(Kind::Rust), false));
    }
}
