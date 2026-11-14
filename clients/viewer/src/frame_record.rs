// SPDX-License-Identifier: GPL-3.0-or-later
//! The frame record's files and, for a decoder written in Rust, its rows.
//!
//! The record (#435) is the one that `picture/frame_times.c` writes for the
//! Linux presenters, with the same file rules and the same rows; the README
//! ("Frame record") names the fields and their grounding. Android's decoder
//! (`android/picture.rs`) is Rust, so it writes the rows with [`Record`].
use anyhow::{Context, Result};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

/// A new file only: an existing name, a link included, is refused.
pub(crate) fn create(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("cannot create the frame record {}", path.display()))
}

#[cfg(any(test, target_os = "android"))]
pub(crate) use rows::{Record, Recording};

#[cfg(any(test, target_os = "android"))]
mod rows {
    use std::io::{self, Write};

    /// Units between arrival and their row, as in `frame_times.c`: a decoder
    /// keeps a few frames, so a unit this far back was not shown.
    const SLOTS: u64 = 256;

    #[derive(Clone, Copy, Default)]
    struct Slot {
        frame_number: i32,
        enqueue_ns: u64,
        decoded_ns: Option<u64>,
    }

    /// One row for each decode unit, in index order:
    /// `index,frame_number,enqueue_ns,decoded_ns,display_ns`. A row is written
    /// once its unit is handed to the display or known to be passed over.
    /// The first write error stops the record; [`Record::close`] reports it.
    pub(crate) struct Record<W: Write> {
        times: io::BufWriter<W>,
        received: u64,
        written: u64,
        slots: Vec<Slot>,
        failed: Option<io::Error>,
    }

    impl<W: Write> Record<W> {
        pub(crate) fn new(times: W) -> io::Result<Self> {
            // One write for many rows: the record must cost the stream nothing.
            let mut times = io::BufWriter::with_capacity(1 << 16, times);
            times.write_all(b"index,frame_number,enqueue_ns,decoded_ns,display_ns\n")?;
            Ok(Self {
                times,
                received: 0,
                written: 0,
                slots: vec![Slot::default(); SLOTS as usize],
                failed: None,
            })
        }

        /// A decode unit arrived; returns its index.
        pub(crate) fn unit(&mut self, frame_number: i32, enqueue_ns: u64) -> u64 {
            let index = self.received;
            self.received += 1;
            if index - self.written >= SLOTS {
                self.row(None);
            }
            self.slots[(index % SLOTS) as usize] = Slot {
                frame_number,
                enqueue_ns,
                decoded_ns: None,
            };
            index
        }

        /// The decoder returned the frame of this unit.
        pub(crate) fn decoded(&mut self, index: u64, now_ns: u64) {
            if self.open(index) {
                self.slots[(index % SLOTS) as usize].decoded_ns = Some(now_ns);
            }
        }

        /// The frame of this unit was handed to the display. Frames reach the
        /// display in stream order, so every older unit was passed over.
        pub(crate) fn displayed(&mut self, index: u64, now_ns: u64) {
            if self.open(index) {
                while self.written < index {
                    self.row(None);
                }
                self.row(Some(now_ns));
            }
        }

        /// Stop the record with an error of its own; [`Record::close`]
        /// reports the first error.
        pub(crate) fn fail(&mut self, error: io::Error) {
            self.failed.get_or_insert(error);
        }

        /// Write the rows still open, as not shown, and report the record.
        pub(crate) fn close(mut self) -> io::Result<W> {
            while self.written < self.received {
                self.row(None);
            }
            if let Some(error) = self.failed.take() {
                return Err(error);
            }
            self.times
                .into_inner()
                .map_err(io::IntoInnerError::into_error)
        }

        fn open(&self, index: u64) -> bool {
            index >= self.written && index < self.received
        }

        fn row(&mut self, display_ns: Option<u64>) {
            let slot = self.slots[(self.written % SLOTS) as usize];
            let time = |value: Option<u64>| value.map(|ns| ns.to_string()).unwrap_or_default();
            if self.failed.is_none()
                && let Err(error) = writeln!(
                    self.times,
                    "{},{},{},{},{}",
                    self.written,
                    slot.frame_number,
                    slot.enqueue_ns,
                    time(slot.decoded_ns),
                    time(display_ns)
                )
            {
                self.failed = Some(error);
            }
            self.written += 1;
        }
    }

    /// Each decode unit's bytes as they arrived, in index order
    /// (`CADENCE_DUMP`). The first write error stops it.
    pub(crate) struct Recording<W: Write> {
        file: W,
        failed: Option<io::Error>,
    }

    impl<W: Write> Recording<W> {
        pub(crate) fn new(file: W) -> Self {
            Self { file, failed: None }
        }

        pub(crate) fn unit<'a>(&mut self, parts: impl IntoIterator<Item = &'a [u8]>) {
            for part in parts {
                if self.failed.is_some() {
                    return;
                }
                if let Err(error) = self.file.write_all(part) {
                    self.failed = Some(error);
                }
            }
        }

        pub(crate) fn close(mut self) -> io::Result<W> {
            match self.failed.take() {
                Some(error) => Err(error),
                None => self.file.flush().map(|()| self.file),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn text(record: Record<Vec<u8>>) -> String {
        String::from_utf8(record.close().unwrap()).unwrap()
    }

    #[test]
    fn rows_are_in_index_order_with_empty_times_for_frames_not_shown() {
        let mut record = Record::new(Vec::new()).unwrap();
        let first = record.unit(7, 1000);
        let second = record.unit(8, 2000);
        let third = record.unit(9, 3000);
        record.decoded(first, 1100);
        record.decoded(second, 2100);
        // The newer frame was ready, so the older one was passed over.
        record.displayed(second, 2200);
        record.decoded(third, 3100);
        record.displayed(third, 3200);
        record.unit(10, 4000);
        assert_eq!(
            text(record),
            "index,frame_number,enqueue_ns,decoded_ns,display_ns\n\
             0,7,1000,1100,\n\
             1,8,2000,2100,2200\n\
             2,9,3000,3100,3200\n\
             3,10,4000,,\n"
        );
    }

    #[test]
    fn late_reports_for_written_rows_change_nothing() {
        let mut record = Record::new(Vec::new()).unwrap();
        let first = record.unit(1, 10);
        let second = record.unit(2, 20);
        record.displayed(second, 25);
        record.decoded(first, 11);
        record.displayed(first, 12);
        record.displayed(99, 13);
        assert_eq!(
            text(record),
            "index,frame_number,enqueue_ns,decoded_ns,display_ns\n0,1,10,,\n1,2,20,,25\n"
        );
    }

    #[test]
    fn a_unit_that_waits_for_256_units_is_written_as_not_shown() {
        let mut record = Record::new(Vec::new()).unwrap();
        for frame in 0..257 {
            record.unit(frame, frame as u64);
        }
        record.displayed(256, 999);
        let rows: Vec<_> = text(record).lines().skip(1).map(str::to_owned).collect();
        assert_eq!(rows.len(), 257);
        assert_eq!(rows[0], "0,0,0,,");
        assert_eq!(rows[256], "256,256,256,,999");
    }

    struct FullDisk;
    impl std::io::Write for FullDisk {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_failed_write_is_reported_at_close() {
        let mut record = Record::new(FullDisk).unwrap();
        for frame in 0..5000 {
            let index = record.unit(frame, 1);
            record.displayed(index, 2);
        }
        assert!(record.close().is_err());
        // An error of the record's own, such as a clock out of range.
        let mut record = Record::new(Vec::new()).unwrap();
        record.unit(1, 1);
        record.fail(std::io::Error::other("clock"));
        assert_eq!(record.close().unwrap_err().to_string(), "clock");
        let mut recording = Recording::new(FullDisk);
        recording.unit([&b"\0\0\0\x01"[..], b"data"]);
        assert!(recording.close().is_err());
        let mut recording = Recording::new(Vec::new());
        recording.unit([&b"ab"[..], b"c"]);
        recording.unit([&b"d"[..]]);
        assert_eq!(recording.close().unwrap(), b"abcd");
    }
}
