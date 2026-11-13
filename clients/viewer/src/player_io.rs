// SPDX-License-Identifier: GPL-3.0-or-later
//! One PC session's carried presenters, audio callbacks and input discovery.
use crate::{network as ffi, options::Options};
use anyhow::{Result, ensure};
use std::ffi::CString;

pub(crate) struct PlayerIo {
    // C keeps these pointers through connection teardown.
    _mapping: Option<CString>,
    audio_device: Option<CString>,
    vaapi: bool,
}
impl PlayerIo {
    pub(crate) fn new(o: &Options) -> Result<Self> {
        let mapping = o.mapping.as_deref().map(CString::new).transpose()?;
        let audio_device = o.audio_device.as_deref().map(CString::new).transpose()?;
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
    pub(crate) fn gamepad_mask(&self) -> i32 {
        unsafe { ffi::viewer_input_gamepad_mask() }
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
        unsafe { ffi::viewer_input_callbacks(callbacks) };
    }
    pub(crate) fn start(&self) {
        unsafe { ffi::viewer_input_start() };
    }
    pub(crate) fn stop(&self) {
        unsafe { ffi::viewer_input_stop() };
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
