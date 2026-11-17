// SPDX-License-Identifier: GPL-3.0-or-later
//! The start's timeline (#391): one line for each step from Start to the
//! first frame, in the viewer's own log (`log::line`), on Linux and Android
//! alike:
//!
//! ```text
//! start: <step> at <time> ms
//! start: <step> at <time> ms (<the step's own times>)
//! ```
//!
//! `<time>` is CLOCK_MONOTONIC in milliseconds. stream-cadence's start and
//! display times and the frame record's times are on the same clock, and on
//! Android it is the clock of the app's own marks, so a proof can line the
//! steps up with what it measured. The Linux presenters' first frame writes
//! the same line from C (`picture/frame_times.c`).
//!
//! The steps: Start; each HTTP request, with libcurl's connect, TLS, first
//! byte and total times; the pair check; the network core's stages
//! (`stageStarting`, `stageComplete`); the RTSP answers that the core names
//! in its log (`core_step`); the decoder; the first frame. The core logs no
//! line after OPTIONS or ANNOUNCE, so those two answers have no mark of
//! their own.
use std::{
    ffi::{CStr, c_char},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

/// CLOCK_MONOTONIC now, in milliseconds.
pub fn now_ms() -> f64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: a valid clock and a live timespec.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    now.tv_sec as f64 * 1e3 + now.tv_nsec as f64 / 1e6
}

/// One step, now.
pub fn mark(step: &str) {
    crate::log::line(&line(step, now_ms(), None));
}

/// One step, now, with its own times.
pub fn mark_with(step: &str, detail: &str) {
    crate::log::line(&line(step, now_ms(), Some(detail)));
}

/// The line of one step at `at_ms`.
pub(crate) fn line(step: &str, at_ms: f64, detail: Option<&str>) -> String {
    match detail {
        Some(detail) => format!("start: {step} at {at_ms:.3} ms ({detail})"),
        None => format!("start: {step} at {at_ms:.3} ms"),
    }
}

/// One start's open end: the first frame is marked once, after its Start.
pub(crate) struct Timeline {
    first_frame: AtomicBool,
}

impl Timeline {
    pub(crate) const fn new() -> Self {
        Self {
            first_frame: AtomicBool::new(false),
        }
    }

    /// Start: the timeline opens, and its first frame is still to come.
    pub(crate) fn begin(&self) {
        self.first_frame.store(true, Ordering::Release);
        mark("Start");
    }

    /// A frame reached the display. True for the first after Start only,
    /// which it marks.
    #[cfg_attr(
        not(any(test, target_os = "android")),
        expect(
            dead_code,
            reason = "Linux's presenters are C: picture/frame_times.c marks their first frame"
        )
    )]
    pub(crate) fn frame_shown(&self) -> bool {
        let first = self.first_frame.swap(false, Ordering::AcqRel);
        if first {
            mark("first frame shown");
        }
        first
    }
}

/// The process's one stream at a time (`stream::CONNECTION`).
pub(crate) static TIMELINE: Timeline = Timeline::new();

/// One HTTP request's own times, as libcurl gives them: each from the
/// request's start. A request without TLS has no TLS time.
pub(crate) fn request_detail(
    connect: Duration,
    tls: Duration,
    first_byte: Duration,
    total: Duration,
) -> String {
    let ms = |time: Duration| time.as_secs_f64() * 1e3;
    let tls = if tls.is_zero() {
        String::new()
    } else {
        format!(", TLS done {:.1} ms", ms(tls))
    };
    format!(
        "connected {:.1} ms{tls}, first byte {:.1} ms, total {:.1} ms",
        ms(connect),
        ms(first_byte),
        ms(total)
    )
}

/// The network core's stage, by `Limelight.h`'s numbers.
pub(crate) fn stage(stage: i32) -> &'static str {
    match stage {
        1 => "platform init",
        2 => "name resolution",
        3 => "audio stream init",
        4 => "RTSP handshake",
        5 => "control stream init",
        6 => "video stream init",
        7 => "input stream init",
        8 => "control stream start",
        9 => "video stream start",
        10 => "audio stream start",
        11 => "input stream start",
        _ => "unknown stage",
    }
}

/// The step that one line of the network core names: the RTSP answers it
/// logs (`RtspConnection.c`) and its first packets. The core at its pin
/// writes these words; nothing else in the line matters.
pub(crate) fn core_step(text: &str) -> Option<&'static str> {
    let text = text.trim_start();
    [
        (
            "Reference frame invalidation is not supported",
            "RTSP DESCRIBE answered",
        ),
        ("Audio port: ", "RTSP SETUP audio answered"),
        ("Video port: ", "RTSP SETUP video answered"),
        ("Control port: ", "RTSP SETUP control answered"),
        ("Received first video packet", "first video packet"),
        ("Received first audio packet", "first audio packet"),
    ]
    .into_iter()
    .find(|(words, _)| text.starts_with(words))
    .map(|(_, step)| step)
}

/// The core's log line, from `core_log.c`, after it was written: its step
/// is marked.
pub(crate) unsafe extern "C" fn core_line(text: *const c_char) {
    crate::callback::guarded("core log mark", (), || {
        if text.is_null() {
            return;
        }
        // SAFETY: core_log.c passes its own NUL-terminated buffer.
        let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
        if let Some(step) = core_step(&text) {
            mark(step);
        }
    });
}

/// The network core's stage callbacks.
pub(crate) unsafe extern "C" fn stage_starting(number: i32) {
    crate::callback::guarded("stage starting", (), || {
        mark(&format!("core {} begins", stage(number)));
    });
}

pub(crate) unsafe extern "C" fn stage_complete(number: i32) {
    crate::callback::guarded("stage complete", (), || {
        mark(&format!("core {} done", stage(number)));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> (String, f64, Option<String>) {
        let rest = line.strip_prefix("start: ").unwrap();
        let (step, rest) = rest.rsplit_once(" at ").unwrap();
        let (time, detail) = rest.split_once(" ms").unwrap();
        let detail = detail
            .strip_prefix(" (")
            .map(|d| d.strip_suffix(')').unwrap().to_owned());
        (step.to_owned(), time.parse().unwrap(), detail)
    }

    #[test]
    fn a_line_names_the_step_and_its_monotonic_time() {
        assert_eq!(
            line("https launch answered", 1234.56789, None),
            "start: https launch answered at 1234.568 ms"
        );
        let (step, at, detail) = parse(&line("x", 5.0, Some("connected 1.0 ms")));
        assert_eq!(
            (step.as_str(), at, detail.as_deref()),
            ("x", 5.0, Some("connected 1.0 ms"))
        );
    }

    #[test]
    fn the_clock_is_monotonic_milliseconds() {
        let mut now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let before = now_ms();
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
        let after = now_ms();
        let read = now.tv_sec as f64 * 1e3 + now.tv_nsec as f64 / 1e6;
        assert!(before <= read && read <= after);
    }

    #[test]
    fn only_the_first_frame_after_start_is_marked() {
        let timeline = Timeline::new();
        assert!(!timeline.frame_shown(), "no start, no first frame");
        timeline.begin();
        assert!(timeline.frame_shown());
        assert!(!timeline.frame_shown());
        // The next start has its own first frame.
        timeline.begin();
        assert!(timeline.frame_shown());
    }

    #[test]
    fn a_request_gives_its_curl_times_in_milliseconds() {
        let ms = Duration::from_micros;
        assert_eq!(
            request_detail(ms(1_002_100), ms(1_040_000), ms(1_100_300), ms(1_100_900)),
            "connected 1002.1 ms, TLS done 1040.0 ms, first byte 1100.3 ms, total 1100.9 ms"
        );
        // Plain HTTP: no TLS.
        assert_eq!(
            request_detail(ms(500), Duration::ZERO, ms(900), ms(1000)),
            "connected 0.5 ms, first byte 0.9 ms, total 1.0 ms"
        );
    }

    #[test]
    fn every_stage_of_the_core_has_its_name() {
        // Limelight.h at the pin: STAGE_PLATFORM_INIT (1) to STAGE_INPUT_STREAM_START (11).
        let names: Vec<_> = (1..=11).map(stage).collect();
        assert!(names.iter().all(|name| *name != "unknown stage"));
        assert_eq!(stage(4), "RTSP handshake");
        assert_eq!(stage(12), "unknown stage");
        assert_eq!(
            names.len(),
            names.iter().collect::<std::collections::HashSet<_>>().len()
        );
    }

    #[test]
    fn the_cores_rtsp_lines_name_their_steps() {
        // The core's own words at its pin (RtspConnection.c, VideoStream.c,
        // AudioStream.c), as its log callback gets them.
        for (text, step) in [
            (
                "Reference frame invalidation is not supported by this host\n",
                Some("RTSP DESCRIBE answered"),
            ),
            ("Audio port: 37748\n", Some("RTSP SETUP audio answered")),
            ("Video port: 37746\n", Some("RTSP SETUP video answered")),
            ("Control port: 37747\n", Some("RTSP SETUP control answered")),
            (
                "Received first video packet after 0 ms\n",
                Some("first video packet"),
            ),
            (
                "Received first audio packet after 0 ms\n",
                Some("first audio packet"),
            ),
            ("Starting RTSP handshake...", None),
            ("done\n", None),
            ("RTSP port: 37758\n", None),
        ] {
            assert_eq!(core_step(text), step, "{text:?}");
        }
    }
}
