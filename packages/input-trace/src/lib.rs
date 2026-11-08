//! Opt-in, bounded input diagnostics. No payloads or credentials belong here.
use std::{
    fmt,
    io::Write,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        OnceLock,
    },
};

const LIMIT: usize = 300;
static ENABLED: OnceLock<bool> = OnceLock::new();
static LINES: AtomicUsize = AtomicUsize::new(0);
static FRAME_PENDING: AtomicBool = AtomicBool::new(false);

pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var_os("KORRI_INPUT_TRACE").is_some_and(|v| v == "1"))
}

pub fn record(stage: &str, detail: fmt::Arguments<'_>) {
    if !enabled()
        || LINES
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                (n < LIMIT).then_some(n + 1)
            })
            .is_err()
    {
        return;
    }
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // CLOCK_MONOTONIC has one origin across both processes on the same device.
    // SAFETY: valid output pointer; no clock state is changed.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } != 0 {
        return;
    }
    let micros = time.tv_sec as u64 * 1_000_000 + time.tv_nsec as u64 / 1_000;
    // One locked write keeps the shared launch log readable. Logging failure
    // must not change input delivery (including a closed diagnostic pipe).
    let line = format!(
        "INPUT_TRACE pid={} mono_us={micros} stage={stage} {detail}\n",
        std::process::id()
    );
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
}

/// Only log a presentation after semantic input, not every animation frame.
pub fn expect_frame() {
    if enabled() {
        FRAME_PENDING.store(true, Ordering::Relaxed);
    }
}

/// Called after RenderPresent returns; not a claim about display scanout.
pub fn presented() {
    if enabled() && FRAME_PENDING.swap(false, Ordering::Relaxed) {
        record("frame_presented", format_args!(""));
    }
}
