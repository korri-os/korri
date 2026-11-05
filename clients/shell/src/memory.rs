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
