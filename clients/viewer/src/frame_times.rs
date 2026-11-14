// SPDX-License-Identifier: GPL-3.0-or-later
//! The frame record that `nix/stream-cadence` asks for (#435).
//!
//! The tool names new files in its run directory with the two environment
//! names of its stage-0 client shim: `CADENCE_FRAME_TIMES` for one row for
//! each frame, `CADENCE_DUMP` for the stream as it arrived. A korrid launch
//! sets neither, and then nothing is recorded. `picture/frame_times.c`
//! writes both; its head names the fields.
use crate::network as ffi;
use anyhow::{Context, Result, ensure};
use std::{
    fs::{File, OpenOptions},
    os::{fd::IntoRawFd, unix::fs::OpenOptionsExt},
    path::Path,
};

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

/// A new file only: an existing name, a link included, is refused.
fn create(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("cannot create the frame record {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::create;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn the_record_takes_only_a_new_name() {
        let directory = tempfile::tempdir().unwrap();
        let existing = directory.path().join("times.csv");
        std::fs::write(&existing, "kept").unwrap();
        assert!(create(&existing).is_err());
        assert_eq!(std::fs::read_to_string(&existing).unwrap(), "kept");
        let link = directory.path().join("recording.video");
        let target = directory.path().join("elsewhere");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(create(&link).is_err());
        assert!(!target.exists());
        let new = directory.path().join("new.csv");
        create(&new).unwrap();
        let mode = std::fs::metadata(&new).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
