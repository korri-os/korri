// SPDX-License-Identifier: GPL-3.0-or-later
//! The process's lines in logcat. An app's standard output and error go
//! nowhere on Android; korrid and the shell write theirs there, as they write
//! to the journal on Linux. [`capture`] sends both to logcat, one line each,
//! with the tag `korri`. Neither writes a capability (AGENTS.md).
use std::{
    ffi::{c_char, c_int, CString},
    fs::File,
    io::{BufRead, BufReader},
    os::fd::{FromRawFd, RawFd},
    sync::Once,
};

const TAG: &std::ffi::CStr = c"korri";
const INFO: c_int = 4;
const ERROR: c_int = 6;
const STDOUT: RawFd = 1;
const STDERR: RawFd = 2;

#[link(name = "log")]
extern "C" {
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

/// Once per process: standard output and error into logcat, and a panic in
/// any thread is a logcat line before the abort.
pub fn capture() {
    static CAPTURED: Once = Once::new();
    CAPTURED.call_once(|| {
        std::panic::set_hook(Box::new(|panic| error(&format!("panic: {panic}"))));
        // korrid's and the shell's standard error carries their ordinary
        // reports, not only failures: both streams are informational.
        for fd in [STDOUT, STDERR] {
            if let Err(failure) = redirect(fd, INFO) {
                error(&format!(
                    "korri-app: standard stream {fd} stays unread: {failure}"
                ));
            }
        }
    });
}

/// Point `fd` at a pipe and read the pipe's lines on a thread of their own.
fn redirect(fd: RawFd, priority: c_int) -> std::io::Result<()> {
    let mut ends = [0 as c_int; 2];
    // SAFETY: pipe writes two descriptors into the array; dup2 takes plain ints.
    if unsafe { pipe(ends.as_mut_ptr()) } != 0 || unsafe { dup2(ends[1], fd) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the write end now lives on as `fd`; this copy is closed once.
    unsafe { close(ends[1]) };
    // SAFETY: the read end is ours alone from here.
    let reader = BufReader::new(unsafe { File::from_raw_fd(ends[0]) });
    std::thread::Builder::new()
        .name(format!("logcat-{fd}"))
        .spawn(move || {
            for line in reader.lines() {
                match line {
                    Ok(line) => write(priority, &line),
                    Err(_) => break,
                }
            }
        })?;
    Ok(())
}

extern "C" {
    fn pipe(fds: *mut c_int) -> c_int;
    fn dup2(old: c_int, new: c_int) -> c_int;
    fn close(fd: c_int) -> c_int;
}
