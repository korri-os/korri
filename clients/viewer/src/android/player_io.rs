// SPDX-License-Identifier: GPL-3.0-or-later
//! One session's picture, sound and input on Android, with the interface
//! that `player_io.rs` gives Linux. The picture is Android's hardware
//! decoder into the app's video layer (`picture.rs`, slice D2); the sound is
//! Opus into AAudio (`sound.rs`, slice D3). Without the app's picture, as in
//! the session tests, the core runs its own empty decoder and audio
//! callbacks. The input (`input.rs`, slice D4) runs on the app's own thread
//! and sends through the core's calls; here it gives only the launch its
//! controller mask. The core's log goes to logcat.
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

/// What the app gives one stream; none in the session tests.
pub(crate) type Platform = Option<App>;

/// The app's part of one stream: its picture, and the controllers attached
/// when it starts (`input::attached`), the launch's controller mask.
pub(crate) struct App {
    pub picture: picture::Picture,
    pub controllers: u16,
}

pub(crate) struct PlayerIo {
    picture: bool,
    controllers: u16,
}
impl PlayerIo {
    /// Makes the picture's frame record files, when asked: before the
    /// launch, so a record that cannot be made stops nothing that ran.
    pub(crate) fn new(_: &Options, platform: Platform) -> Result<Self> {
        let picture = platform.is_some();
        let mut controllers = 0;
        if let Some(app) = platform {
            controllers = app.controllers;
            picture::open(app.picture)?;
        }
        Ok(Self {
            picture,
            controllers,
        })
    }
    /// `remoteControllersBitmap` and `gcmap` of the launch, as Artemis sends
    /// its attached controllers.
    pub(crate) fn gamepad_mask(&self) -> i32 {
        i32::from(self.controllers)
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
