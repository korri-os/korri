//! ADR 0030: the free-space floor for store downloads and their unpacked
//! result. There is no size cap. Before each write korrid checks that the
//! space available to its own user, minus that write, stays at or above 5% of
//! the file system's size: the ext4 default reserved-blocks percentage
//! (mke2fs(8), `-m`). A size that a server or an archive declares never allows
//! a write; it can only refuse one early.
use super::Failure;
use std::{
    fs::File,
    io,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::Path,
    sync::Arc,
};

/// The part of the file system that a store download leaves free, in percent.
const FLOOR_PERCENT: u128 = 5;

/// The largest single write that the archive reader makes.
pub(super) const WRITE_CHUNK: usize = 64 * 1024;

/// What `fstatvfs` reports: bytes available to an unprivileged writer
/// (`f_bavail × f_frsize`) and the file system's size (`f_blocks × f_frsize`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Space {
    pub available: u64,
    pub size: u64,
    /// `f_frsize`, the allocation unit: the least a folder or an empty
    /// file can cost (review P2-1).
    pub block: u64,
}

/// Reads the space of the file system that holds an open folder.
pub(super) type Probe = Arc<dyn Fn(&File) -> io::Result<Space> + Send + Sync>;

pub(super) fn fstatvfs(file: &File) -> io::Result<Space> {
    // SAFETY: an all-zero statvfs is a valid value for the kernel to fill.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: the descriptor is open for the whole call and `stat` is writable.
    if unsafe { libc::fstatvfs(file.as_raw_fd(), &mut stat) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let unit = stat.f_frsize as u64;
    Ok(Space {
        available: (stat.f_bavail as u64).saturating_mul(unit),
        size: (stat.f_blocks as u64).saturating_mul(unit),
        block: unit.max(1),
    })
}

/// Counts the bytes one download or one unpack writes into a folder.
pub(super) struct Floor {
    folder: File,
    probe: Probe,
    start: u64,
    written: u64,
}

impl Floor {
    pub(super) fn new(folder: &Path, probe: Option<Probe>) -> Result<Self, Failure> {
        let folder = File::options()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(folder)?;
        let probe = probe.unwrap_or_else(|| Arc::new(fstatvfs));
        let start = probe(&folder)?.available;
        Ok(Self {
            folder,
            probe,
            start,
            written: 0,
        })
    }

    /// Bytes that can still be written before the floor. The file system's
    /// own figure can lag behind delayed allocation, so the bytes counted
    /// since the start bound it too.
    pub(super) fn room(&self) -> Result<u64, Failure> {
        let (available, floor, _) = self.measure()?;
        Ok(available.saturating_sub(floor))
    }

    /// (available, floor, block) now, the available figure bounded by the
    /// bytes counted since the start.
    fn measure(&self) -> Result<(u64, u64, u64), Failure> {
        let space = (self.probe)(&self.folder)?;
        let floor = (u128::from(space.size) * FLOOR_PERCENT).div_ceil(100) as u64;
        let available = space.available.min(self.start.saturating_sub(self.written));
        Ok((available, floor, space.block))
    }

    /// Call before each folder or file is created: one allocation unit for
    /// its metadata and first block, so many empty entries count too.
    pub(super) fn reserve_entry(&mut self) -> Result<(), Failure> {
        let (_, _, block) = self.measure()?;
        self.reserve(block)
    }

    /// After the last entry: the file system itself must still be at or
    /// above the floor, whatever the counts said.
    pub(super) fn check(&self) -> Result<(), Failure> {
        let space = (self.probe)(&self.folder)?;
        let floor = (u128::from(space.size) * FLOOR_PERCENT).div_ceil(100) as u64;
        if space.available < floor {
            return Err(Failure::NoSpace);
        }
        Ok(())
    }

    /// Call before each write of `bytes`.
    pub(super) fn reserve(&mut self, bytes: u64) -> Result<(), Failure> {
        if self.room()? < bytes {
            return Err(Failure::NoSpace);
        }
        self.written += bytes;
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn fixed(room: u64) -> Probe {
    // A file system of 100,000 bytes has a floor of 5,000 bytes.
    Arc::new(move |_| {
        Ok(Space {
            available: 5_000 + room,
            size: 100_000,
            block: 4096,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_floor_is_five_percent_and_counts_written_bytes() {
        let folder = tempfile::tempdir().unwrap();
        let mut floor = Floor::new(folder.path(), Some(fixed(3_000))).unwrap();
        assert_eq!(floor.room().unwrap(), 3_000);
        floor.reserve(2_000).unwrap();
        // The probe still says 3,000; the 2,000 counted bytes win.
        assert_eq!(floor.room().unwrap(), 1_000);
        assert_eq!(floor.reserve(1_001), Err(Failure::NoSpace));
        floor.reserve(1_000).unwrap();
        assert_eq!(floor.reserve(1), Err(Failure::NoSpace));

        // The floor rounds up and never underflows.
        let tiny: Probe = Arc::new(|_| {
            Ok(Space {
                available: 1,
                size: 19,
                block: 1,
            })
        });
        assert_eq!(
            Floor::new(folder.path(), Some(tiny))
                .unwrap()
                .room()
                .unwrap(),
            0
        );
    }

    #[test]
    fn the_real_probe_reads_the_folders_file_system() {
        let folder = tempfile::tempdir().unwrap();
        let space = fstatvfs(&File::open(folder.path()).unwrap()).unwrap();
        assert!(space.size > 0 && space.available <= space.size);
        let link = folder.path().join("link");
        std::os::unix::fs::symlink(folder.path(), &link).unwrap();
        assert!(Floor::new(&link, None).is_err(), "never follows a link");
    }
}
