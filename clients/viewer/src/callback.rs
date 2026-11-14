// SPDX-License-Identifier: GPL-3.0-or-later
//! The network core calls Rust through `extern "C"` functions. A panic must
//! not unwind into C: Rust aborts the process there, and on Android that
//! process is the app.

use std::panic::{AssertUnwindSafe, catch_unwind};

/// Run a callback's body; a panic in it gives `fallback` and a log line.
/// The panic hook has already logged the panic itself.
pub(crate) fn guarded<T>(callback: &str, fallback: T, body: impl FnOnce() -> T) -> T {
    // Unwind safety: a callback's state is behind locks or atomics, and a
    // lock that a panic poisons fails the next call, which is guarded too.
    catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|_| {
        crate::log::line(&format!("{callback}: panicked; the core gets its fallback"));
        fallback
    })
}

#[cfg(test)]
mod tests {
    use super::guarded;

    #[test]
    fn a_panic_in_a_callback_gives_its_fallback() {
        assert_eq!(guarded("submit", -1, || panic!("a codec fault")), -1);
    }

    #[test]
    fn a_callback_that_returns_gives_its_value() {
        assert_eq!(guarded("submit", -1, || 0), 0);
    }
}
