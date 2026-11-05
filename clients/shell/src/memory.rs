//! Allocator settings for a long-running shell.
//!
//! glibc serves large blocks (at least 128 KiB) with `mmap` and returns them
//! on free, but it raises that threshold to the size of each large block it
//! frees. After the first surface switch, a window's frame buffer and its
//! cached layer images (about 1.2 MiB each at 640x480) then come from the heap,
//! and the freed space stays in the process. The memory probe saw this as a
//! 2.5 MiB step at a random round. Fixing the threshold keeps large blocks on
//! `mmap`, so a dropped surface returns them.

/// The glibc default before its dynamic adjustment.
pub const MMAP_THRESHOLD: i32 = 128 * 1024;

/// Fix glibc's mmap threshold. Call once at start, before any window exists.
pub fn fix_mmap_threshold() {
    // SAFETY: mallopt only changes allocator parameters; setting
    // M_MMAP_THRESHOLD also disables its dynamic adjustment.
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, MMAP_THRESHOLD);
    }
}

/// Drop this program's resident code pages that hold no change of its own, after a compile.
///
/// A compile runs most of the interpreter's compiler once; its code then stays mapped and
/// counts in the shell's memory (about 17 MB of the binary's 20 MB `.text` on a
/// workstation) although nothing runs it until the next compile. The pages mapped from the
/// file and never written (no anonymous page in the mapping) are dropped; the code the
/// shell runs from then on is read back from the page cache as it runs. Mappings the
/// loader wrote to (relocations) are left alone: dropping those would lose the writes.
pub fn release_code() {
    #[cfg(target_os = "linux")]
    {
        let (Ok(exe), Ok(smaps)) = (
            std::fs::read_link("/proc/self/exe"),
            std::fs::read_to_string("/proc/self/smaps"),
        ) else {
            return;
        };
        let exe = exe.to_string_lossy();
        let mut current: Option<(usize, usize)> = None;
        for line in smaps.lines() {
            let mut fields = line.split_whitespace();
            let Some(first) = fields.next() else { continue };
            if first == "Anonymous:" {
                if let (Some((start, end)), Some("0")) = (current.take(), fields.next()) {
                    // SAFETY: the range is a whole mapping of this program's file, not
                    // writable and with no anonymous (written) page. For such a private file
                    // mapping MADV_DONTNEED only drops page table entries; the next access
                    // reads the same bytes back from the file.
                    unsafe {
                        let at = start as *mut libc::c_void;
                        libc::madvise(at, end - start, libc::MADV_DONTNEED);
                        libc::madvise(at, end - start, libc::MADV_RANDOM);
                    }
                }
            } else if let Some((start, end)) =
                first.split_once('-').filter(|_| !first.ends_with(':'))
            {
                // A mapping's first line: range, permissions, offset, device, inode, path.
                let perms = fields.next().unwrap_or("");
                let path = fields.nth(3).unwrap_or("");
                current = if path == exe && !perms.contains('w') {
                    usize::from_str_radix(start, 16)
                        .ok()
                        .zip(usize::from_str_radix(end, 16).ok())
                } else {
                    None
                };
            }
        }
    }
}

/// Give the heap's free pages back to the system. glibc returns only the free
/// space at the top of its heap by itself; a compile frees the compiler's
/// working trees and an unloaded surface frees its instance in the middle of
/// it, and those pages stayed in the process (RSS) until reused.
pub fn release_free_heap() {
    // SAFETY: malloc_trim only releases free memory; it touches no live block.
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0);
    }
}
