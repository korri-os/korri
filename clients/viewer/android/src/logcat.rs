// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's lines in logcat, with the tag of the network core's lines
//! (`../../src/android/log.c`).
use std::ffi::{CString, c_char, c_int};

const TAG: &std::ffi::CStr = c"korri-viewer";
const INFO: c_int = 4;
const ERROR: c_int = 6;

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

fn write(priority: c_int, text: &str) {
    // A NUL inside the text would end the line there; drop it instead.
    let text = CString::new(text.replace('\0', "")).unwrap_or_default();
    // SAFETY: both are live C strings.
    unsafe { __android_log_write(priority, TAG.as_ptr(), text.as_ptr()) };
}

pub fn info(text: &str) {
    write(INFO, text);
}

pub fn error(text: &str) {
    write(ERROR, text);
}

/// A panic in any thread of the app is a logcat line before the abort.
pub fn panics() {
    std::panic::set_hook(Box::new(|panic| error(&format!("panic: {panic}"))));
}
