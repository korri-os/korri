// SPDX-License-Identifier: GPL-3.0-or-later
//! One session's picture, sound and input on Android: none yet. The network
//! core then runs its own empty decoder and audio callbacks, and the stream
//! offers no controller. Later slices of #391 step D add decode (D2), sound
//! (D3) and input (D4) here, with the interface that `player_io.rs` gives
//! Linux. The core's log goes to logcat.
use crate::{network as ffi, options::Options};
use anyhow::Result;
use std::{
    ffi::{c_char, c_void},
    ptr::null_mut,
    time::Duration,
};

unsafe extern "C" {
    /// `log.c`: the core's log line, to logcat.
    fn viewer_android_log(format: *const c_char, ...);
}

pub(crate) struct PlayerIo;
impl PlayerIo {
    pub(crate) fn new(_: &Options) -> Result<Self> {
        Ok(Self)
    }
    pub(crate) fn gamepad_mask(&self) -> i32 {
        0
    }
    /// No callbacks: `LiStartConnection` uses the core's empty ones.
    pub(crate) fn decoder_callbacks(&self) -> *mut ffi::DECODER_RENDERER_CALLBACKS {
        null_mut()
    }
    pub(crate) fn audio_callbacks(&self) -> *mut ffi::AUDIO_RENDERER_CALLBACKS {
        null_mut()
    }
    pub(crate) fn audio_context(&self) -> *mut c_void {
        null_mut()
    }
    pub(crate) fn connection_callbacks(&self, callbacks: &mut ffi::CONNECTION_LISTENER_CALLBACKS) {
        callbacks.logMessage = Some(viewer_android_log);
    }
    pub(crate) fn start(&self) {}
    pub(crate) fn stop(&self) {}
    /// No event loop to turn: wait until the stream's stop.
    pub(crate) fn poll(&self, timeout_ms: i32) -> i32 {
        std::thread::sleep(Duration::from_millis(timeout_ms.max(0) as u64));
        0
    }
}
