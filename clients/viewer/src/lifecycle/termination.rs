// SPDX-License-Identifier: GPL-3.0-or-later
//! Termination signals for a process that serves a lifecycle socket.
use anyhow::{Result, ensure};

/// The signals that stop a stream (`stream.rs`), and so stop a launch.
const SIGNALS: [libc::c_int; 4] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT];

/// The termination signals, blocked in this thread and in every thread it
/// starts later. A blocked signal waits; its default action never runs.
pub(super) struct Held(libc::sigset_t);

impl Held {
    /// Call before the process starts a thread: a thread inherits the mask of
    /// the thread that starts it. A signal that arrives before `receive` waits
    /// for it.
    pub(super) fn block() -> Result<Self> {
        // SAFETY: sigemptyset initializes the set before any other use.
        let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
        unsafe { libc::sigemptyset(&mut set) };
        for signal in SIGNALS {
            // SAFETY: the set is initialized and the signal is valid.
            unsafe { libc::sigaddset(&mut set, signal) };
        }
        // SAFETY: the set is initialized; no old mask is requested.
        let result = unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) };
        ensure!(
            result == 0,
            "block termination signals: {}",
            std::io::Error::from_raw_os_error(result)
        );
        Ok(Self(set))
    }

    /// Take each signal, already waiting or later, on a thread of its own.
    pub(super) fn receive(self, stop: impl Fn() + Send + 'static) -> Result<()> {
        std::thread::Builder::new()
            .name("viewer-signals".into())
            .spawn(move || {
                loop {
                    let mut signal = 0;
                    // SAFETY: the set is initialized and outlives the call.
                    if unsafe { libc::sigwait(&self.0, &mut signal) } == 0 {
                        stop();
                    }
                }
            })?;
        Ok(())
    }
}
