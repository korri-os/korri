// SPDX-License-Identifier: GPL-3.0-or-later
//! One session's picture, sound and input on Android, with the interface
//! that `player_io.rs` gives Linux. The picture is Android's hardware
//! decoder into the app's video layer (`picture.rs`, slice D2); the sound is
//! Opus into AAudio (`sound.rs`, slice D3). Without the app's picture, as in
//! the session tests, the core runs its own empty decoder and audio
//! callbacks. Input (D4) is not here yet: the stream offers no controller.
//! The core's log goes to logcat.
use crate::{network as ffi, options::Options, picture, sound};
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

/// What the app gives one stream: its picture, or none.
pub(crate) type Platform = Option<picture::Picture>;

pub(crate) struct PlayerIo {
    picture: bool,
}
impl PlayerIo {
    /// Makes the picture's frame record files, when asked: before the
    /// launch, so a record that cannot be made stops nothing that ran.
    pub(crate) fn new(_: &Options, platform: Platform) -> Result<Self> {
        let picture = platform.is_some();
        if let Some(platform) = platform {
            picture::open(platform)?;
        }
        Ok(Self { picture })
    }
    pub(crate) fn gamepad_mask(&self) -> i32 {
        0
    }
    /// Without a picture, no callbacks: `LiStartConnection` uses the core's
    /// empty ones.
    pub(crate) fn decoder_callbacks(&self) -> *mut ffi::DECODER_RENDERER_CALLBACKS {
        if self.picture {
            picture::callbacks()
        } else {
            null_mut()
        }
    }
    /// The app's stream has sound; the session tests have none.
    pub(crate) fn audio_callbacks(&self) -> *mut ffi::AUDIO_RENDERER_CALLBACKS {
        if self.picture {
            sound::callbacks()
        } else {
            null_mut()
        }
    }
    pub(crate) fn audio_context(&self) -> *mut c_void {
        null_mut()
    }
    pub(crate) fn connection_callbacks(&self, callbacks: &mut ffi::CONNECTION_LISTENER_CALLBACKS) {
        callbacks.logMessage = Some(viewer_android_log);
    }
    pub(crate) fn start(&self) {}
    pub(crate) fn stop(&self) {}
    /// No event loop to turn: wait until the stream's stop. A failed
    /// decoder ends the stream (negative: the picture failed); its reason is
    /// in the log.
    pub(crate) fn poll(&self, timeout_ms: i32) -> i32 {
        std::thread::sleep(Duration::from_millis(timeout_ms.max(0) as u64));
        if self.picture && picture::failure().is_some() {
            return -1;
        }
        0
    }
    /// After the core has stopped: a sound stream that the core's cleanup
    /// did not close, the frame record's last rows, and whether the whole
    /// record was written.
    pub(crate) fn close(&mut self) -> Result<()> {
        if std::mem::take(&mut self.picture) {
            sound::close();
            picture::close()
        } else {
            Ok(())
        }
    }
}
impl Drop for PlayerIo {
    fn drop(&mut self) {
        // An early return: its own error is the one reported.
        let _ = self.close();
    }
}
