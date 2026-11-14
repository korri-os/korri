// SPDX-License-Identifier: GPL-3.0-or-later
//! One session's carried presenters, audio callbacks and input discovery.
use crate::{network as ffi, options::Options};
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
            #[cfg(v4l2request)]
            _inputs: Vec::new(),
        };
        ensure!(
            vaapi || io.audio_device.is_none(),
            "You can't select an audio device for SDL"
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
    pub(crate) fn gamepad_mask(&self) -> i32 {
        unsafe { ffi::viewer_input_gamepad_mask() }
    }
    pub(crate) fn decoder_callbacks(&self) -> *mut ffi::DECODER_RENDERER_CALLBACKS {
        unsafe { ffi::picture_callbacks() }
    }
    pub(crate) fn audio_callbacks(&self) -> *mut ffi::AUDIO_RENDERER_CALLBACKS {
        unsafe { ffi::viewer_audio_callbacks(i32::from(self.vaapi), self.audio_context().cast()) }
    }
    pub(crate) fn audio_context(&self) -> *mut std::ffi::c_void {
        self.audio_device
            .as_ref()
            .map_or(std::ptr::null_mut(), |s| s.as_ptr().cast_mut().cast())
    }
    pub(crate) fn connection_callbacks(&self, callbacks: &mut ffi::CONNECTION_LISTENER_CALLBACKS) {
        callbacks.logMessage = Some(ffi::picture_log);
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
        unsafe { ffi::picture_poll(timeout_ms) }
    }
}
impl Drop for PlayerIo {
    fn drop(&mut self) {
        // The caller stops and joins the core before dropping the session's I/O.
        unsafe {
            ffi::viewer_input_destroy();
            ffi::picture_destroy();
        }
    }
}
