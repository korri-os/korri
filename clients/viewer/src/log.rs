// SPDX-License-Identifier: GPL-3.0-or-later
//! One line of the viewer's own log: logcat on Android, with the tag of the
//! core's and the app's lines, and standard error elsewhere. The network
//! core's own lines go through `core_log.c`.
use std::ffi::c_char;

unsafe extern "C" {
    /// `core_log.c`: the network core's log callback (`logMessage`).
    pub(crate) fn viewer_core_log(format: *const c_char, ...);
    /// `core_log.c`: who hears each of the core's lines after it is written.
    fn viewer_core_log_hook(marks: Option<unsafe extern "C" fn(*const c_char)>);
}

/// The start's marks hear each of the core's lines (`marks::core_line`).
/// Before the core starts: it reads no hook while it does not run.
pub(crate) fn mark_core_lines() {
    // SAFETY: a function that lives as long as the process.
    unsafe { viewer_core_log_hook(Some(crate::marks::core_line)) };
}

#[cfg(target_os = "android")]
pub(crate) fn line(text: &str) {
    use std::ffi::{CString, c_int};
    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }
    let text = CString::new(text.replace('\0', "")).unwrap_or_default();
    // SAFETY: both are live C strings. 4: ANDROID_LOG_INFO.
    unsafe { __android_log_write(4, c"korri-viewer".as_ptr(), text.as_ptr()) };
}

#[cfg(not(target_os = "android"))]
pub(crate) fn line(text: &str) {
    eprintln!("{text}");
}
