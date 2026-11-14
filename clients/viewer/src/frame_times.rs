// SPDX-License-Identifier: GPL-3.0-or-later
//! The frame record that `nix/stream-cadence` asks for (#435).
//!
//! The tool names new files in its run directory with the two environment
//! names of its stage-0 client shim: `CADENCE_FRAME_TIMES` for one row for
//! each frame, `CADENCE_DUMP` for the stream as it arrived. A korrid launch
//! sets neither, and then nothing is recorded. `picture/frame_times.c`
//! writes both; its head names the fields.
use crate::{frame_record::create, network as ffi};
use anyhow::{Result, ensure};
use std::{fs::File, os::fd::IntoRawFd, path::Path};

const TIMES: &str = "CADENCE_FRAME_TIMES";
const RECORDING: &str = "CADENCE_DUMP";

/// The record of one stream. Close it after the core has stopped.
pub(crate) struct FrameTimes {
    open: bool,
}

impl FrameTimes {
    pub(crate) fn from_environment() -> Result<Self> {
        let times = std::env::var_os(TIMES);
        let recording = std::env::var_os(RECORDING);
        if times.is_none() && recording.is_none() {
            return Ok(Self { open: false });
        }
        let times = times.map(|path| create(Path::new(&path))).transpose()?;
        let recording = recording.map(|path| create(Path::new(&path))).transpose()?;
        let descriptor = |file: Option<File>| file.map_or(-1, IntoRawFd::into_raw_fd);
        // SAFETY: C takes both descriptors; no stream runs yet.
        let result =
            unsafe { ffi::picture_frame_times_open(descriptor(times), descriptor(recording)) };
        ensure!(result == 0, "cannot start the frame record");
        Ok(Self { open: true })
    }

    /// Write the rows still open and report whether the whole record was written.
    pub(crate) fn close(mut self) -> Result<()> {
        self.finish()
    }

    fn finish(&mut self) -> Result<()> {
        if !std::mem::take(&mut self.open) {
            return Ok(());
        }
        // SAFETY: the caller has stopped the core, so no callback writes now.
        let result = unsafe { ffi::picture_frame_times_close() };
        ensure!(
            result == 0,
            "the frame record is incomplete; its error is above"
        );
        Ok(())
    }
}

impl Drop for FrameTimes {
    fn drop(&mut self) {
        // Only an early return gets here; its own error is the one reported.
        let _ = self.finish();
    }
}
