// SPDX-License-Identifier: GPL-3.0-or-later
//! One line of the viewer's own log: logcat on Android, with the tag of the
//! core's and the app's lines, and standard error elsewhere.

#[cfg(target_os = "android")]
pub(crate) fn line(text: &str) {
    use std::ffi::{CString, c_char, c_int};
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
