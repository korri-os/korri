// SPDX-License-Identifier: GPL-3.0-or-later
//! One session's picture, sound and input on Android, with the interface
//! that `player_io.rs` gives Linux. The picture is Android's hardware
//! decoder into the app's video layer (`picture.rs`, slice D2); the sound is
//! Opus into AAudio (`sound.rs`, slice D3). Without the app's picture, as in
//! the session tests, the core runs its own empty decoder and audio
//! callbacks. The input (`input.rs`, slice D4) runs on the app's own thread
//! and sends through the core's calls; here it gives only the launch its
//! controller mask. The controllers' feedback (`feedback.rs`, slice D4b)
//! runs on its own thread; here the core gets its rumble and light
//! callbacks, and the feedback stops before the core does. The core's log
//! goes to logcat.
use crate::{feedback, network as ffi, options::Options, picture, sound};
use anyhow::Result;
use std::{ffi::c_void, ptr::null_mut, time::Duration};

/// What the app gives one stream; none in the session tests.
pub(crate) type Platform = Option<App>;

/// The app's part of one stream: its picture, the controllers attached
/// when it starts (`input::attached`), the launch's controller mask, and the
/// controllers' feedback thread.
pub(crate) struct App {
    pub picture: picture::Picture,
    pub controllers: u16,
    pub feedback: feedback::Worker,
}

pub(crate) struct PlayerIo {
    picture: bool,
    controllers: u16,
    feedback: Option<feedback::Worker>,
}
impl PlayerIo {
    /// Makes the picture's frame record files, when asked: before the
    /// launch, so a record that cannot be made stops nothing that ran. The
    /// decoder starts to be made for the asked picture.
    pub(crate) fn new(o: &Options, platform: Platform) -> Result<Self> {
        let picture = platform.is_some();
        let mut controllers = 0;
        let mut feedback = None;
        if let Some(app) = platform {
            controllers = app.controllers;
            feedback = Some(app.feedback);
            // The decoder for the asked picture is made while the stream
            // connects (#391).
            let asked = crate::decoder::Asked {
                size: (o.width, o.height),
                fps: o.fps,
            };
            picture::open(app.picture, asked)?;
        }
        Ok(Self {
            picture,
            controllers,
            feedback,
        })
    }
    /// Whether the picture follows a new stream size in the middle of the
    /// stream: the app's decoder and video layer do (#476, slice 4). The
    /// session tests have no picture.
    pub(crate) fn follows_new_sizes(&self) -> bool {
        self.picture
    }
    /// The newest decoded size of the picture; none before the first, and
    /// none without a picture. The app's video layer places each one.
    pub(crate) fn shown_size(&self) -> Option<korri_viewer_treaty::ViewerDisplay> {
        if !self.picture {
            return None;
        }
        picture::decoded_size()
            .map(|(width, height)| korri_viewer_treaty::ViewerDisplay { width, height })
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
        callbacks.logMessage = Some(crate::log::viewer_core_log);
        // Without the app, as in the session tests, the core's own empty
        // callbacks take rumble and light.
        if let Some(worker) = &self.feedback {
            callbacks.rumble = Some(feedback::rumble);
            callbacks.rumbleTriggers = Some(feedback::rumble_triggers);
            callbacks.setControllerLED = Some(feedback::set_controller_led);
            worker.serve_core();
        }
    }
    /// Before the core stops: its callbacks reach the feedback no more, and
    /// every vibration and light that the stream started stops.
    pub(crate) fn stop_feedback(&mut self) {
        if let Some(mut worker) = self.feedback.take()
            && worker.stop(feedback::STOP_WAIT)
        {
            crate::log::line("feedback: vibration and lights stopped");
        }
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
        self.stop_feedback();
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
