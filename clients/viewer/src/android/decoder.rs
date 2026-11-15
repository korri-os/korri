// SPDX-License-Identifier: GPL-3.0-only
//! The Android decoder's two loops, over any codec with `AMediaCodec`'s
//! buffer calls (`Codec`): the core's decoder thread queues each frame, and
//! an output thread hands each decoded frame to the display. `picture.rs`
//! runs them over `AMediaCodec`; the tests run them over `ScriptedCodec`.
//!
//! Ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`, a fork of
//! `ClassicOldSong/moonlight-android`, itself a fork of
//! `moonlight-stream/moonlight-android`), `MediaCodecDecoderRenderer.java`:
//! the parameter sets in their own buffer, the wait for an input buffer, the
//! end of the stream after decoder errors, and a new picture size in the
//! middle of the stream: within adaptive playback's maximum the codec follows
//! the new SPS by itself and the decoded size comes from the output format;
//! otherwise the codec restarts as Artemis's recovery restarts it (stop,
//! configure, start). The lowest-latency pacing is upstream Moonlight's.
//! Artemis ships under the GNU GPL version 3 with no "or later" grant, so
//! this file does too. The Rust adaptation is Korri work dated 2026-11-14
//! and 2026-11-15.
use crate::{frame_record, log, sps};
use std::{
    collections::VecDeque,
    ffi::c_int,
    fmt,
    fs::File,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::NonNull,
    sync::{
        Arc, Mutex, OnceLock, PoisonError, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// A status that a codec call returned (`media_status_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Status(pub i32);

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.0 {
            1100 => "AMEDIACODEC_ERROR_INSUFFICIENT_RESOURCE",
            1101 => "AMEDIACODEC_ERROR_RECLAIMED",
            -10000 => "AMEDIA_ERROR_UNKNOWN",
            -10003 => "AMEDIA_ERROR_INVALID_OBJECT",
            -10005 => "AMEDIA_ERROR_INVALID_OPERATION",
            _ => "media status",
        };
        write!(f, "{name} ({})", self.0)
    }
}

impl Status {
    /// The codec is gone: the system took it back, or it no longer exists.
    fn fatal(self) -> bool {
        matches!(self.0, 1101 | -10003)
    }
}

/// What the codec's output queue gave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Output {
    Frame { index: usize, pts: u64 },
    FormatChanged,
    TryAgain,
}

/// The buffer calls of `AMediaCodec` that the two loops use. Input and
/// output may be used from two threads at once.
pub(crate) trait Codec: Send + Sync {
    /// An input buffer's index, or none within `timeout`.
    fn dequeue_input(&self, timeout: Duration) -> Result<Option<usize>, Status>;
    /// The memory of a dequeued input buffer: none when the codec gives none,
    /// as `AMediaCodec_getInputBuffer` does after a decoder error.
    fn input_buffer(&self, index: usize) -> Option<(NonNull<u8>, usize)>;
    fn queue_input(&self, index: usize, size: usize, pts: u64, flags: u32) -> Result<(), Status>;
    fn dequeue_output(&self, timeout: Duration) -> Result<Output, Status>;
    /// `None` drops the frame; `Some(ns)` hands it to the display at that
    /// `CLOCK_MONOTONIC` time.
    fn release_output(&self, index: usize, render: Option<i64>) -> Result<(), Status>;
    /// For the log.
    fn output_format(&self) -> String;
    /// The decoded picture's size, from the output format: its crop when it
    /// has one.
    fn output_size(&self) -> Option<(i32, i32)>;
    /// Stop, configure for a picture of `size` (with adaptive playback up to
    /// `max`, when some), and start again. No other call to the codec runs
    /// meanwhile.
    fn restart(&self, size: (i32, i32), max: Option<(i32, i32)>) -> Result<(), Status>;
    fn stop(&self);
}

/// What the codec was configured for, and how it follows a new picture size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Configured {
    pub(crate) size: (i32, i32),
    pub(crate) fps: i32,
    /// The decoder's name, for the SPS patch (`sps::Patch::for_stream`).
    pub(crate) decoder: String,
    /// Adaptive playback's largest picture (`codec::adaptive_max`): a new
    /// size within it goes to the codec as it comes. None, or a size beyond
    /// it: the codec restarts.
    pub(crate) adaptive: Option<(i32, i32)>,
}

/// Hears each new decoded size, on the output thread; it must not block.
pub(crate) type Resized = Arc<dyn Fn((i32, i32)) + Send + Sync>;

pub(crate) const CODEC_CONFIG: u32 = 2;
pub(crate) const KEY_FRAME: u32 = 1;
// Limelight.h's buffer types.
const BUFFER_TYPE_SPS: c_int = 1;
const BUFFER_TYPE_PPS: c_int = 2;
const BUFFER_TYPE_VPS: c_int = 3;

/// The frame record's files, when asked (`frame_record.rs`).
#[derive(Default)]
pub(crate) struct Records {
    pub(crate) times: Option<Mutex<frame_record::Record<File>>>,
    pub(crate) recording: Option<Mutex<frame_record::Recording<File>>>,
}

impl Records {
    /// A decode unit arrived: its bytes to the recording, its row opened.
    /// `enqueue_ns` is asked only when the times are recorded.
    pub(crate) fn unit(
        &self,
        number: i32,
        enqueue_ns: impl FnOnce() -> (u64, bool),
        parts: &[(c_int, &[u8])],
    ) -> Option<u64> {
        if let Some(recording) = &self.recording {
            recording
                .lock()
                .unwrap()
                .unit(parts.iter().map(|(_, data)| *data));
        }
        let times = self.times.as_ref()?;
        let (enqueue_ns, in_range) = enqueue_ns();
        let mut times = times.lock().unwrap();
        if !in_range {
            times.fail(std::io::Error::other(
                "the core's enqueue time is later than its clock",
            ));
        }
        Some(times.unit(number, enqueue_ns))
    }
}

/// One decode unit, as the core gives it.
pub(crate) struct Frame<'a> {
    pub(crate) number: i32,
    pub(crate) idr: bool,
    pub(crate) enqueue_us: u64,
    /// Its buffers, in order, with their types.
    pub(crate) parts: Vec<(c_int, &'a [u8])>,
    /// Its row in the frame record.
    pub(crate) record: Option<u64>,
}

/// Artemis ends the stream when decoder errors still come 3 s after the
/// first (`EXCEPTION_REPORT_DELAY_MS`, after its recovery tries). Until then
/// each side backs off, so that a failed decoder costs no CPU. The sound's
/// output (`audio.rs`) uses the same rule when its device stays lost.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Health {
    window: Duration,
    since: Option<Instant>,
    back_off: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    BackOff(Duration),
    End,
}

const FIRST_BACK_OFF: Duration = Duration::from_millis(10);
const LAST_BACK_OFF: Duration = Duration::from_millis(160);

impl Default for Health {
    fn default() -> Self {
        Self::new(Duration::from_secs(3))
    }
}

impl Health {
    pub(crate) fn new(window: Duration) -> Self {
        Self {
            window,
            since: None,
            back_off: FIRST_BACK_OFF,
        }
    }

    pub(crate) fn success(&mut self) {
        self.since = None;
        self.back_off = FIRST_BACK_OFF;
    }

    pub(crate) fn failure(&mut self, fatal: bool, now: Instant) -> Verdict {
        let since = *self.since.get_or_insert(now);
        if fatal || now.duration_since(since) >= self.window {
            return Verdict::End;
        }
        let back_off = self.back_off;
        self.back_off = (back_off * 2).min(LAST_BACK_OFF);
        Verdict::BackOff(back_off)
    }
}

/// Why a frame did not reach the decoder.
enum Lost {
    Stopped,
    Codec(&'static str, Status),
    /// `AMediaCodec_getInputBuffer` gave NULL: the decoder is in error.
    NoMemory,
    TooLarge {
        size: usize,
        room: usize,
    },
}

impl fmt::Display for Lost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Lost::Stopped => write!(f, "the stream stopped before the decoder took the frame"),
            Lost::Codec(what, status) => write!(f, "{what}: {status}"),
            Lost::NoMemory => write!(f, "the decoder gave no memory for its input buffer"),
            Lost::TooLarge { size, room } => {
                write!(
                    f,
                    "a frame of {size} bytes is too large for its buffer of {room}"
                )
            }
        }
    }
}

/// What `submit` answers the core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Queued {
    /// The decoder has the frame, or the frame is not needed.
    Taken,
    /// The frame was lost; the core asks the host for an IDR frame.
    NeedIdr,
}

pub(crate) struct Decoder<C> {
    codec: C,
    /// Held for reading around the output thread's calls, and for writing
    /// around a restart and the codec's stop.
    gate: RwLock<()>,
    input: Mutex<Input>,
    resized: Resized,
    pub(crate) records: Arc<Records>,
    stopping: AtomicBool,
    output: Mutex<Option<JoinHandle<()>>>,
    input_health: Mutex<Health>,
    output_health: Mutex<Health>,
    failed: OnceLock<String>,
}

struct Input {
    last_pts: u64,
    /// Each queued frame's presentation time and record index, in order.
    queued: VecDeque<(u64, Option<u64>)>,
    /// What the codec decodes now.
    configured: Configured,
}

/// A new picture size that an SPS brings, and how the codec follows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NewSize {
    size: (i32, i32),
    follow: Follow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Follow {
    /// Within adaptive playback's maximum: the codec takes the new SPS as it
    /// comes.
    Itself,
    /// The codec restarts at the new size, with adaptive playback up to
    /// `max` when it had it.
    Restart { max: Option<(i32, i32)> },
}

impl<C: Codec + 'static> Decoder<C> {
    pub(crate) fn new(
        codec: C,
        configured: Configured,
        records: Arc<Records>,
        health: Health,
        resized: Resized,
    ) -> Arc<Self> {
        Arc::new(Self {
            codec,
            gate: RwLock::new(()),
            input: Mutex::new(Input {
                last_pts: 0,
                queued: VecDeque::new(),
                configured,
            }),
            resized,
            records,
            stopping: AtomicBool::new(false),
            output: Mutex::new(None),
            input_health: Mutex::new(health),
            output_health: Mutex::new(health),
            failed: OnceLock::new(),
        })
    }

    /// Start the output thread. A panic there ends the stream, as a failed
    /// decoder does.
    pub(crate) fn start_output(self: &Arc<Self>) -> std::io::Result<()> {
        let decoder = self.clone();
        let output = std::thread::Builder::new()
            .name("viewer-picture".into())
            .spawn(move || {
                if catch_unwind(AssertUnwindSafe(|| decoder.release_frames())).is_err() {
                    decoder.fail("the output thread panicked".into());
                }
            })?;
        *self.output.lock().unwrap() = Some(output);
        Ok(())
    }

    /// Why the decoder failed, once it has. The stream then ends.
    pub(crate) fn failure(&self) -> Option<String> {
        self.failed.get().cloned()
    }

    fn fail(&self, why: String) {
        if self.failed.set(why.clone()).is_ok() {
            log::line(&format!(
                "picture: the decoder failed; the stream ends: {why}"
            ));
        }
    }

    /// Wait out a back-off, in short steps, so that a stop is not delayed.
    fn pause(&self, back_off: Duration) {
        let until = Instant::now() + back_off;
        while !self.stopping.load(Ordering::Relaxed) && Instant::now() < until {
            std::thread::sleep(
                until
                    .saturating_duration_since(Instant::now())
                    .min(FIRST_BACK_OFF),
            );
        }
    }

    /// One side's error: back off, or end the stream.
    fn failed_once(&self, health: &Mutex<Health>, fatal: bool, why: String) {
        let verdict = health.lock().unwrap().failure(fatal, Instant::now());
        match verdict {
            Verdict::BackOff(back_off) => self.pause(back_off),
            Verdict::End => self.fail(why),
        }
    }

    /// The core is stopping: frames that still come are not needed.
    pub(crate) fn stop(&self) {
        self.stopping.store(true, Ordering::Relaxed);
    }

    /// Stop the output thread and the codec. The codec is deleted with the
    /// last reference, after the output thread's.
    pub(crate) fn finish(&self) {
        self.stop();
        if let Some(output) = self.output.lock().unwrap().take() {
            let _ = output.join();
        }
        let _codec = self.gate.write().unwrap_or_else(PoisonError::into_inner);
        self.codec.stop();
        log::line("picture: decoder stopped");
    }

    pub(crate) fn queue(&self, frame: Frame) -> Queued {
        // A stream that stops or ends needs no frame and no IDR frame.
        if self.stopping.load(Ordering::Relaxed) || self.failed.get().is_some() {
            return Queued::Taken;
        }
        let mut config = Vec::new();
        let mut picture = Vec::new();
        let mut new_size = None;
        for (kind, data) in &frame.parts {
            match *kind {
                BUFFER_TYPE_SPS => {
                    let (sps, size) = self.parameter_set(data);
                    config.extend(sps);
                    new_size = new_size.or(size);
                }
                BUFFER_TYPE_PPS | BUFFER_TYPE_VPS => config.extend_from_slice(data),
                _ => picture.extend_from_slice(data),
            }
        }
        // A new size takes effect with the IDR frame that carries it.
        if let Some(new_size) = new_size.filter(|_| frame.idr)
            && !self.follow(new_size, frame.number)
        {
            return Queued::Taken;
        }
        // The frame is known to the output thread before the decoder has it.
        // The lock is not held while this thread waits for an input buffer:
        // the output thread needs it to release the frames that free one.
        let (config_pts, pts) = {
            let mut input = self.input.lock().unwrap();
            let config_pts =
                (frame.idr && !config.is_empty()).then(|| input.next_pts(frame.enqueue_us));
            let pts = input.next_pts(frame.enqueue_us);
            input.queued.push_back((pts, frame.record));
            if input.queued.len() > 256 {
                input.queued.pop_front();
            }
            (config_pts, pts)
        };
        // The parameter sets go first, in their own buffer, per Android's
        // MediaCodec documentation; they produce no frame.
        let queued = config_pts
            .map_or(Ok(()), |config_pts| {
                self.queue_buffer(&config, config_pts, CODEC_CONFIG)
            })
            .and_then(|()| self.queue_buffer(&picture, pts, if frame.idr { KEY_FRAME } else { 0 }));
        let lost = match queued {
            Ok(()) => {
                self.input_health.lock().unwrap().success();
                return Queued::Taken;
            }
            Err(Lost::Stopped) => return Queued::Taken,
            Err(lost) => lost,
        };
        let why = format!("frame {} not decoded: {lost}", frame.number);
        log::line(&format!("picture: {why}"));
        match lost {
            Lost::Codec(_, status) => self.failed_once(&self.input_health, status.fatal(), why),
            Lost::NoMemory => self.failed_once(&self.input_health, false, why),
            Lost::Stopped | Lost::TooLarge { .. } => {}
        }
        if self.failed.get().is_some() {
            Queued::Taken
        } else {
            Queued::NeedIdr
        }
    }

    fn queue_buffer(&self, data: &[u8], pts: u64, flags: u32) -> Result<(), Lost> {
        let codec = &self.codec;
        // Wait for a buffer as Artemis does, in 10 ms steps until the stream
        // stops: a frame given up here would cost a new IDR frame.
        let index = loop {
            match codec.dequeue_input(Duration::from_millis(10)) {
                Ok(Some(index)) => break index,
                Ok(None) if self.stopping.load(Ordering::Relaxed) => return Err(Lost::Stopped),
                Ok(None) => {}
                Err(status) => return Err(Lost::Codec("no input buffer", status)),
            }
        };
        // A dequeued buffer that takes no frame goes back empty, or the codec
        // would lose it.
        let give_back = || {
            let _ = codec.queue_input(index, 0, pts, 0);
        };
        let Some((room, size)) = codec.input_buffer(index) else {
            give_back();
            return Err(Lost::NoMemory);
        };
        if data.len() > size {
            give_back();
            return Err(Lost::TooLarge {
                size: data.len(),
                room: size,
            });
        }
        // SAFETY: the codec's buffer holds `size` bytes, at least `data.len()`.
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), room.as_ptr(), data.len()) };
        codec
            .queue_input(index, data.len(), pts, flags)
            .map_err(|status| Lost::Codec("cannot queue the frame", status))
    }

    /// An SPS, with Artemis's patch for its own picture size, and that size
    /// when it is not the codec's. An SPS that cannot be read goes as it came.
    fn parameter_set(&self, data: &[u8]) -> (Vec<u8>, Option<NewSize>) {
        let start = if data.starts_with(&[0, 0, 1]) { 3 } else { 4 };
        let read = data.get(start..).map(|nal| {
            let fields = sps::read(nal)?;
            let size = (fields.width as i32, fields.height as i32);
            let (patch, new_size) = {
                let input = self.input.lock().unwrap();
                let configured = &input.configured;
                let patch =
                    sps::Patch::for_stream(size.0, size.1, configured.fps, &configured.decoder);
                let new_size = (size != configured.size).then(|| NewSize {
                    size,
                    follow: match configured.adaptive {
                        Some(max) if size.0 <= max.0 && size.1 <= max.1 => Follow::Itself,
                        // A restart keeps adaptive playback, grown to hold the new size.
                        adaptive => Follow::Restart {
                            max: adaptive.map(|max| (max.0.max(size.0), max.1.max(size.1))),
                        },
                    },
                });
                (patch, new_size)
            };
            anyhow::Ok((sps::rewrite(nal, patch)?, new_size))
        });
        match read {
            Some(Ok((nal, new_size))) => ([&data[..start], &nal].concat(), new_size),
            Some(Err(error)) => {
                log::line(&format!("picture: SPS left as it came: {error:#}"));
                (data.to_vec(), None)
            }
            None => (data.to_vec(), None),
        }
    }

    /// The codec follows a new picture size: by itself within adaptive
    /// playback, else by a restart. False: the restart failed, and the
    /// stream ends.
    fn follow(&self, new_size: NewSize, frame: i32) -> bool {
        let (from, adaptive) = {
            let input = self.input.lock().unwrap();
            (input.configured.size, input.configured.adaptive)
        };
        let (w, h) = new_size.size;
        let how = match (new_size.follow, adaptive) {
            (Follow::Itself, Some((mw, mh))) => {
                format!("the decoder follows it (adaptive playback up to {mw}x{mh})")
            }
            (Follow::Restart { .. }, Some((mw, mh))) => {
                format!("the decoder restarts (beyond adaptive playback's {mw}x{mh})")
            }
            (_, None) => "the decoder restarts (no adaptive playback)".into(),
        };
        log::line(&format!(
            "picture: the stream's size changes from {}x{} to {w}x{h} at frame {frame}; {how}",
            from.0, from.1
        ));
        if let Follow::Restart { max } = new_size.follow {
            let restarted = {
                let _codec = self.gate.write().unwrap_or_else(PoisonError::into_inner);
                self.codec.restart(new_size.size, max)
            };
            if let Err(status) = restarted {
                self.fail(format!("cannot restart the decoder at {w}x{h}: {status}"));
                return false;
            }
            log::line(&format!("picture: the decoder restarted at {w}x{h}"));
            self.input.lock().unwrap().configured.adaptive = max;
        }
        self.input.lock().unwrap().configured.size = new_size.size;
        true
    }

    /// The output thread: hand each frame to the display at once. When more
    /// than one frame is ready, only the newest goes; the older ones were
    /// overtaken and are dropped.
    pub(crate) fn release_frames(&self) {
        let codec = &self.codec;
        // The decoded size last told.
        let mut told = None;
        while !self.stopping.load(Ordering::Relaxed) && self.failed.get().is_none() {
            let mut error = None;
            {
                // A restart waits while this thread uses the codec, and not
                // while it backs off.
                let _codec = self.gate.read().unwrap_or_else(PoisonError::into_inner);
                let mut newest: Option<(usize, Option<u64>)> = None;
                let mut wait = Duration::from_millis(20);
                loop {
                    match codec.dequeue_output(wait) {
                        Ok(Output::Frame { index, pts }) => {
                            let record = self.decoded(pts);
                            if let Some((older, _)) = newest.replace((index, record)) {
                                let _ = codec.release_output(older, None);
                            }
                            wait = Duration::ZERO;
                        }
                        Ok(Output::FormatChanged) => {
                            log::line(&format!("picture: output format {}", codec.output_format()));
                            if let Some(size) =
                                codec.output_size().filter(|&size| told != Some(size))
                            {
                                told = Some(size);
                                log::line(&format!("picture: decoded size {}x{}", size.0, size.1));
                                (self.resized)(size);
                            }
                        }
                        Ok(Output::TryAgain) => break,
                        Err(status) => {
                            error = Some(("output failed", status));
                            break;
                        }
                    }
                }
                if let Some((index, record)) = newest {
                    let released = codec.release_output(index, Some(monotonic_ns() as i64));
                    let handed = monotonic_ns();
                    match released {
                        Ok(()) => self.displayed(record, handed),
                        Err(status) => error = error.or(Some(("frame not released", status))),
                    }
                }
            }
            match error {
                Some((what, status)) => {
                    let why = format!("{what}: {status}");
                    log::line(&format!("picture: {why}"));
                    self.failed_once(&self.output_health, status.fatal(), why);
                }
                None => self.output_health.lock().unwrap().success(),
            }
        }
    }

    /// The decoder returned the frame of this presentation time: its record index.
    fn decoded(&self, pts: u64) -> Option<u64> {
        let now = monotonic_ns();
        let record = {
            let mut input = self.input.lock().unwrap();
            // Frames come out in the order they went in; one that the
            // decoder dropped has no output and leaves the queue here.
            loop {
                match input.queued.front() {
                    Some(&(queued, record)) if queued <= pts => {
                        input.queued.pop_front();
                        if queued == pts {
                            break record;
                        }
                    }
                    _ => break None,
                }
            }
        };
        if let (Some(index), Some(times)) = (record, &self.records.times) {
            times.lock().unwrap().decoded(index, now);
        }
        record
    }

    fn displayed(&self, record: Option<u64>, now: u64) {
        if let (Some(index), Some(times)) = (record, &self.records.times) {
            times.lock().unwrap().displayed(index, now);
        }
    }
}

impl Input {
    /// A presentation time from the core's enqueue time, never repeated:
    /// MediaCodec refuses two buffers at the same time.
    fn next_pts(&mut self, enqueue_us: u64) -> u64 {
        self.last_pts = enqueue_us.max(self.last_pts + 1);
        self.last_pts
    }
}

pub(crate) fn monotonic_ns() -> u64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: a valid timespec to fill.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    now.tv_sec as u64 * 1_000_000_000 + now.tv_nsec as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sps::tests::{LANDSCAPE, PORTRAIT, decode_hex};
    use std::{sync::mpsc, time::Instant};

    /// A codec that gives what its script says and records what it was
    /// asked. Each queue is popped in turn; an empty one repeats `then`.
    #[derive(Default)]
    struct ScriptedCodec {
        script: Mutex<Script>,
        /// The output thread is inside a codec call.
        in_output: AtomicBool,
        /// A restart ran while the output thread was inside a codec call.
        overlapped: AtomicBool,
    }

    #[derive(Default)]
    struct Script {
        inputs: VecDeque<Result<Option<usize>, Status>>,
        inputs_then: Option<Result<Option<usize>, Status>>,
        /// The size of each input buffer; none gives no buffer.
        buffer: Option<usize>,
        memory: Vec<Vec<u8>>,
        outputs: VecDeque<Result<Output, Status>>,
        outputs_then: Option<Result<Output, Status>>,
        // What the codec was asked.
        input_calls: usize,
        output_calls: usize,
        queued: Vec<(usize, Vec<u8>, u64, u32)>,
        released: Vec<(usize, bool)>,
        stopped: bool,
        /// The size that each output format gives, in turn.
        formats: VecDeque<(i32, i32)>,
        /// A restart fails with this status.
        restart_fails: Option<Status>,
        restarts: Vec<Restarted>,
    }

    /// A restart: how many buffers were queued before it, its size and its
    /// maximum.
    type Restarted = (usize, (i32, i32), Option<(i32, i32)>);

    impl Codec for ScriptedCodec {
        fn dequeue_input(&self, _: Duration) -> Result<Option<usize>, Status> {
            let mut s = self.script.lock().unwrap();
            s.input_calls += 1;
            s.inputs.pop_front().or(s.inputs_then).unwrap_or(Ok(None))
        }
        fn input_buffer(&self, index: usize) -> Option<(NonNull<u8>, usize)> {
            let mut s = self.script.lock().unwrap();
            let size = s.buffer?;
            while s.memory.len() <= index {
                s.memory.push(Vec::new());
            }
            s.memory[index] = vec![0; size];
            NonNull::new(s.memory[index].as_mut_ptr()).map(|room| (room, size))
        }
        fn queue_input(
            &self,
            index: usize,
            size: usize,
            pts: u64,
            flags: u32,
        ) -> Result<(), Status> {
            let mut s = self.script.lock().unwrap();
            let data = s
                .memory
                .get(index)
                .map_or(Vec::new(), |memory| memory[..size].to_vec());
            s.queued.push((index, data, pts, flags));
            Ok(())
        }
        fn dequeue_output(&self, timeout: Duration) -> Result<Output, Status> {
            self.in_output.store(true, Ordering::SeqCst);
            let next = {
                let mut s = self.script.lock().unwrap();
                s.output_calls += 1;
                s.outputs.pop_front().or(s.outputs_then)
            };
            let output = match next {
                Some(next) => next,
                None => {
                    // As a codec with nothing ready: wait out the timeout.
                    std::thread::sleep(timeout.min(Duration::from_millis(5)));
                    Ok(Output::TryAgain)
                }
            };
            self.in_output.store(false, Ordering::SeqCst);
            output
        }
        fn release_output(&self, index: usize, render: Option<i64>) -> Result<(), Status> {
            self.script
                .lock()
                .unwrap()
                .released
                .push((index, render.is_some()));
            Ok(())
        }
        fn output_format(&self) -> String {
            "scripted".into()
        }
        fn output_size(&self) -> Option<(i32, i32)> {
            self.script.lock().unwrap().formats.pop_front()
        }
        fn restart(&self, size: (i32, i32), max: Option<(i32, i32)>) -> Result<(), Status> {
            // Long enough for an output call to begin if nothing held it off.
            std::thread::sleep(Duration::from_millis(30));
            if self.in_output.load(Ordering::SeqCst) {
                self.overlapped.store(true, Ordering::SeqCst);
            }
            let mut s = self.script.lock().unwrap();
            let queued = s.queued.len();
            s.restarts.push((queued, size, max));
            s.restart_fails.map_or(Ok(()), Err)
        }
        fn stop(&self) {
            self.script.lock().unwrap().stopped = true;
        }
    }

    fn decoder(script: Script) -> Arc<Decoder<ScriptedCodec>> {
        recording_decoder(script, Records::default())
    }

    fn configured(adaptive: Option<(i32, i32)>) -> Configured {
        Configured {
            size: (1280, 720),
            fps: 60,
            decoder: "c2.test".into(),
            adaptive,
        }
    }

    fn recording_decoder(script: Script, records: Records) -> Arc<Decoder<ScriptedCodec>> {
        shaped_decoder(script, records, configured(None), Arc::new(|_| {}))
    }

    fn shaped_decoder(
        script: Script,
        records: Records,
        configured: Configured,
        resized: Resized,
    ) -> Arc<Decoder<ScriptedCodec>> {
        Decoder::new(
            ScriptedCodec {
                script: Mutex::new(script),
                ..ScriptedCodec::default()
            },
            configured,
            Arc::new(records),
            Health::default(),
            resized,
        )
    }

    /// An SPS NAL unit with its start code.
    fn sps_unit(hex: &str) -> Vec<u8> {
        [vec![0, 0, 0, 1], decode_hex(hex)].concat()
    }

    const PPS: &[u8] = &[0, 0, 0, 1, 0x68, 0xeb, 0xe3, 0xcb, 0x22, 0xc0];
    /// libx264's High 1920x1080@60 SPS (`sps.rs`, `high-1080p-cropped`).
    const FULL_HD: &str = "6764002aacd940780227e5c044000003000400000301e03c60c658";

    fn idr<'a>(number: i32, sps: &'a [u8]) -> Frame<'a> {
        Frame {
            number,
            idr: true,
            enqueue_us: 1000 * number as u64,
            parts: vec![(BUFFER_TYPE_SPS, sps), (BUFFER_TYPE_PPS, PPS), (0, PICTURE)],
            record: None,
        }
    }

    fn delta(number: i32) -> Frame<'static> {
        Frame {
            number,
            idr: false,
            enqueue_us: 1000 * number as u64,
            parts: vec![(0, PICTURE)],
            record: None,
        }
    }

    /// The SPS that the codec got in each parameter-set buffer.
    fn parameter_sets(decoder: &Decoder<ScriptedCodec>) -> Vec<sps::Fields> {
        script(decoder)
            .queued
            .iter()
            .filter(|queued| queued.3 == CODEC_CONFIG)
            .map(|queued| {
                let end = queued
                    .1
                    .windows(4)
                    .skip(1)
                    .position(|w| w == [0, 0, 0, 1])
                    .unwrap()
                    + 1;
                sps::read(&queued.1[4..end]).unwrap()
            })
            .collect()
    }

    fn many_inputs() -> Script {
        Script {
            inputs_then: Some(Ok(Some(0))),
            buffer: Some(4096),
            ..Script::default()
        }
    }

    fn frame(idr: bool, parts: Vec<(c_int, &[u8])>) -> Frame<'_> {
        Frame {
            number: 1,
            idr,
            enqueue_us: 1000,
            parts,
            record: None,
        }
    }

    const PICTURE: &[u8] = &[0, 0, 0, 1, 0x65, 0x88, 0x84, 0x00, 0x21, 0xff];

    fn script(decoder: &Decoder<ScriptedCodec>) -> std::sync::MutexGuard<'_, Script> {
        decoder.codec.script.lock().unwrap()
    }

    /// Run the output thread until it ends or `limit` passes; then stop it.
    fn run_output(decoder: &Arc<Decoder<ScriptedCodec>>, limit: Duration) -> bool {
        let (ended, done) = mpsc::channel();
        let thread = {
            let decoder = decoder.clone();
            std::thread::spawn(move || {
                decoder.release_frames();
                let _ = ended.send(());
            })
        };
        let finished = done.recv_timeout(limit).is_ok();
        decoder.stop();
        thread.join().unwrap();
        finished
    }

    #[test]
    fn a_frame_goes_into_its_buffer_and_parameter_sets_into_their_own() {
        // SPS (libx264, High 1280x720) and PPS, then the picture data.
        let sps: Vec<u8> = [0, 0, 0, 1]
            .into_iter()
            .chain((0..48).step_by(2).map(|at| {
                u8::from_str_radix(
                    &"67640020acb200a00b760220000003002000000f11e30649"[at..at + 2],
                    16,
                )
                .unwrap()
            }))
            .collect();
        let pps = [0, 0, 0, 1, 0x68, 0xeb, 0xe3, 0xcb, 0x22, 0xc0];
        let decoder = decoder(Script {
            inputs: [Ok(Some(0)), Ok(Some(1))].into(),
            buffer: Some(4096),
            ..Script::default()
        });
        let parts = vec![
            (BUFFER_TYPE_SPS, &sps[..]),
            (BUFFER_TYPE_PPS, &pps[..]),
            (0, PICTURE),
        ];
        assert_eq!(decoder.queue(frame(true, parts)), Queued::Taken);
        let queued = &script(&decoder).queued;
        assert_eq!(queued.len(), 2);
        let (_, config, _, flags) = &queued[0];
        assert_eq!(*flags, CODEC_CONFIG);
        assert_eq!(config[4] & 0x1f, 7, "the SPS leads the parameter sets");
        assert!(config.ends_with(&pps));
        assert_eq!((&queued[1].1[..], queued[1].3), (PICTURE, KEY_FRAME));
        assert!(queued[1].2 > queued[0].2, "presentation times rise");
    }

    #[test]
    fn a_new_size_restarts_a_codec_without_adaptive_playback() {
        let decoder = decoder(many_inputs());
        let (landscape, portrait) = (sps_unit(LANDSCAPE), sps_unit(PORTRAIT));
        assert_eq!(decoder.queue(idr(1, &landscape)), Queued::Taken);
        assert_eq!(decoder.queue(delta(2)), Queued::Taken);
        assert!(
            script(&decoder).restarts.is_empty(),
            "the configured size restarts nothing"
        );
        // The sender's new size, after an IDR frame.
        assert_eq!(decoder.queue(idr(3, &portrait)), Queued::Taken);
        assert_eq!(decoder.queue(delta(4)), Queued::Taken);
        let s = script(&decoder);
        // Restarted at the new size before its parameter sets went in.
        assert_eq!(s.restarts, [(3, (720, 1280), None)]);
        let flags: Vec<u32> = s.queued.iter().map(|queued| queued.3).collect();
        assert_eq!(
            flags,
            [CODEC_CONFIG, KEY_FRAME, 0, CODEC_CONFIG, KEY_FRAME, 0]
        );
        drop(s);
        let sizes: Vec<_> = parameter_sets(&decoder)
            .iter()
            .map(|sps| (sps.width, sps.height))
            .collect();
        assert_eq!(sizes, [(1280, 720), (720, 1280)]);
        // The next IDR frame at that size restarts nothing.
        decoder.queue(idr(5, &portrait));
        assert_eq!(script(&decoder).restarts.len(), 1);
        assert!(decoder.failure().is_none());
    }

    #[test]
    fn within_adaptive_playback_the_codec_follows_a_new_size_by_itself() {
        let decoder = shaped_decoder(
            many_inputs(),
            Records::default(),
            configured(Some((4096, 4096))),
            Arc::new(|_| {}),
        );
        let (landscape, portrait) = (sps_unit(LANDSCAPE), sps_unit(PORTRAIT));
        decoder.queue(idr(1, &landscape));
        decoder.queue(idr(2, &portrait));
        decoder.queue(idr(3, &landscape));
        assert!(script(&decoder).restarts.is_empty());
        let sizes: Vec<_> = parameter_sets(&decoder)
            .iter()
            .map(|sps| (sps.width, sps.height))
            .collect();
        assert_eq!(sizes, [(1280, 720), (720, 1280), (1280, 720)]);
    }

    #[test]
    fn a_size_beyond_the_adaptive_maximum_restarts_with_a_larger_one() {
        let decoder = shaped_decoder(
            many_inputs(),
            Records::default(),
            configured(Some((1280, 1280))),
            Arc::new(|_| {}),
        );
        let (landscape, full_hd) = (sps_unit(LANDSCAPE), sps_unit(FULL_HD));
        decoder.queue(idr(1, &full_hd));
        assert_eq!(
            script(&decoder).restarts,
            [(0, (1920, 1080), Some((1920, 1280)))]
        );
        // Back within the grown maximum: no restart.
        decoder.queue(idr(2, &landscape));
        assert_eq!(script(&decoder).restarts.len(), 1);
    }

    #[test]
    fn the_sps_patch_follows_the_new_size() {
        let decoder = decoder(many_inputs());
        let (landscape, full_hd) = (sps_unit(LANDSCAPE), sps_unit(FULL_HD));
        decoder.queue(idr(1, &landscape));
        decoder.queue(idr(2, &full_hd));
        let levels: Vec<u8> = parameter_sets(&decoder)
            .iter()
            .map(|sps| sps.level_idc)
            .collect();
        // Level 3.2 holds 1280x720 and not 1920x1080 (H.264, table A-1).
        assert_eq!(levels, [32, 42]);
    }

    #[test]
    fn a_failed_restart_ends_the_stream() {
        let decoder = decoder(Script {
            restart_fails: Some(Status(-10000)),
            ..many_inputs()
        });
        let portrait = sps_unit(PORTRAIT);
        assert_eq!(decoder.queue(idr(1, &portrait)), Queued::Taken);
        let why = decoder.failure().expect("the stream ends");
        assert!(
            why.contains("cannot restart the decoder at 720x1280"),
            "{why}"
        );
        // Nothing went to a codec that did not start again.
        assert!(script(&decoder).queued.is_empty());
        assert_eq!(decoder.queue(delta(2)), Queued::Taken);
        assert!(script(&decoder).queued.is_empty());
    }

    #[test]
    fn a_restart_never_runs_beside_the_output_threads_codec_calls() {
        let decoder = decoder(many_inputs());
        decoder.start_output().unwrap();
        let portrait = sps_unit(PORTRAIT);
        let landscape = sps_unit(LANDSCAPE);
        for number in 0..6 {
            let sps = if number % 2 == 0 {
                &portrait
            } else {
                &landscape
            };
            decoder.queue(idr(number, sps));
        }
        decoder.finish();
        let codec = &decoder.codec;
        assert_eq!(script(&decoder).restarts.len(), 6);
        assert!(script(&decoder).output_calls > 0, "the output thread ran");
        assert!(!codec.overlapped.load(Ordering::SeqCst));
    }

    #[test]
    fn each_new_decoded_size_reaches_the_app_once() {
        let told = Arc::new(Mutex::new(Vec::new()));
        let decoder = shaped_decoder(
            Script {
                inputs: [Ok(Some(0)), Ok(Some(1))].into(),
                buffer: Some(64),
                formats: [(1280, 720), (1280, 720), (720, 1280)].into(),
                ..Script::default()
            },
            Records::default(),
            configured(None),
            {
                let told = told.clone();
                Arc::new(move |size| told.lock().unwrap().push(size))
            },
        );
        decoder.queue(delta(1));
        decoder.queue(delta(2));
        let pts: Vec<u64> = script(&decoder).queued.iter().map(|q| q.2).collect();
        script(&decoder).outputs = [
            Ok(Output::FormatChanged),
            Ok(Output::Frame {
                index: 0,
                pts: pts[0],
            }),
            Ok(Output::TryAgain),
            // A format change with the same size: nothing to tell.
            Ok(Output::FormatChanged),
            Ok(Output::FormatChanged),
            Ok(Output::Frame {
                index: 1,
                pts: pts[1],
            }),
        ]
        .into();
        assert!(!run_output(&decoder, Duration::from_millis(200)));
        assert_eq!(*told.lock().unwrap(), [(1280, 720), (720, 1280)]);
        assert_eq!(script(&decoder).released, [(0, true), (1, true)]);
    }

    #[test]
    fn the_frame_record_goes_on_across_a_restart() {
        let directory = tempfile::tempdir().unwrap();
        let times = directory.path().join("times.csv");
        let records = Records {
            times: Some(Mutex::new(
                frame_record::Record::new(frame_record::create(&times).unwrap()).unwrap(),
            )),
            recording: None,
        };
        let decoder = recording_decoder(many_inputs(), records);
        let (landscape, portrait) = (sps_unit(LANDSCAPE), sps_unit(PORTRAIT));
        for number in 1..=4 {
            let sps = if number < 3 { &landscape } else { &portrait };
            let mut frame = if number % 2 == 1 {
                idr(number, sps)
            } else {
                delta(number)
            };
            frame.record =
                decoder
                    .records
                    .unit(number, || (1000 * number as u64, true), &frame.parts);
            decoder.queue(frame);
        }
        assert_eq!(script(&decoder).restarts.len(), 1);
        // The picture frames' presentation times (parameter sets make no frame).
        let pts: Vec<u64> = script(&decoder)
            .queued
            .iter()
            .filter(|queued| queued.3 != CODEC_CONFIG)
            .map(|queued| queued.2)
            .collect();
        // Frame 2 was in the codec when it restarted and never comes out.
        script(&decoder).outputs = [
            Ok(Output::Frame {
                index: 0,
                pts: pts[0],
            }),
            Ok(Output::TryAgain),
            Ok(Output::FormatChanged),
            Ok(Output::Frame {
                index: 0,
                pts: pts[2],
            }),
            Ok(Output::TryAgain),
            Ok(Output::Frame {
                index: 1,
                pts: pts[3],
            }),
        ]
        .into();
        script(&decoder).formats = [(720, 1280)].into();
        decoder.start_output().unwrap();
        let until = Instant::now() + Duration::from_secs(2);
        while script(&decoder).released.len() < 3 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        decoder.finish();
        let records = Arc::try_unwrap(Arc::try_unwrap(decoder).ok().unwrap().records)
            .ok()
            .unwrap();
        records
            .times
            .unwrap()
            .into_inner()
            .unwrap()
            .close()
            .unwrap();
        let rows: Vec<Vec<String>> = std::fs::read_to_string(&times)
            .unwrap()
            .lines()
            .skip(1)
            .map(|row| row.split(',').map(str::to_owned).collect())
            .collect();
        let numbers: Vec<&str> = rows.iter().map(|row| row[1].as_str()).collect();
        assert_eq!(numbers, ["1", "2", "3", "4"], "every unit, in order");
        let shown: Vec<bool> = rows.iter().map(|row| !row[4].is_empty()).collect();
        assert_eq!(
            shown,
            [true, false, true, true],
            "the record goes on after the restart"
        );
    }

    #[test]
    fn a_missing_input_buffer_is_given_back_and_asks_for_an_idr_frame() {
        let decoder = decoder(Script {
            inputs: [Ok(Some(3))].into(),
            buffer: None,
            ..Script::default()
        });
        assert_eq!(
            decoder.queue(frame(false, vec![(0, PICTURE)])),
            Queued::NeedIdr
        );
        let queued = &script(&decoder).queued;
        assert_eq!(queued.len(), 1, "the dequeued buffer goes back");
        assert_eq!((queued[0].0, queued[0].1.len()), (3, 0));
    }

    #[test]
    fn a_frame_too_large_for_its_buffer_gives_the_buffer_back_empty() {
        let decoder = decoder(Script {
            inputs: [Ok(Some(2))].into(),
            buffer: Some(4),
            ..Script::default()
        });
        assert_eq!(
            decoder.queue(frame(false, vec![(0, PICTURE)])),
            Queued::NeedIdr
        );
        let queued = &script(&decoder).queued;
        assert_eq!(queued.len(), 1, "the dequeued buffer goes back");
        assert_eq!((queued[0].0, queued[0].1.len(), queued[0].3), (2, 0, 0));
    }

    #[test]
    fn output_errors_back_off_and_then_end_the_stream() {
        let decoder = decoder(Script {
            outputs_then: Some(Err(Status(-10000))),
            ..Script::default()
        });
        let ended = run_output(&decoder, Duration::from_secs(5));
        let calls = script(&decoder).output_calls;
        assert!(
            ended,
            "the output thread ends after errors that last ({calls} calls)"
        );
        assert!(decoder.failure().is_some());
        // 3 s of errors with backing off: far fewer calls than a spin.
        assert!(calls < 100, "{calls} calls in the error window");
    }

    #[test]
    fn a_reclaimed_codec_ends_the_stream_at_once() {
        let decoder = decoder(Script {
            outputs_then: Some(Err(Status(1101))),
            ..Script::default()
        });
        let started = Instant::now();
        assert!(run_output(&decoder, Duration::from_secs(1)));
        assert!(started.elapsed() < Duration::from_millis(500));
        assert_eq!(script(&decoder).output_calls, 1);
        assert!(decoder.failure().unwrap().contains("RECLAIMED"));
    }

    #[test]
    fn input_errors_back_off_and_then_frames_stop_asking_for_idr() {
        let decoder = decoder(Script {
            inputs_then: Some(Err(Status(-10000))),
            buffer: Some(4096),
            ..Script::default()
        });
        let started = Instant::now();
        let mut answers = Vec::new();
        while started.elapsed() < Duration::from_secs(4)
            && answers.len() < 10_000
            && decoder.failure().is_none()
        {
            answers.push(decoder.queue(frame(false, vec![(0, PICTURE)])));
        }
        assert!(
            decoder.failure().is_some(),
            "errors that last end the stream"
        );
        let calls = script(&decoder).input_calls;
        assert!(calls < 100, "{calls} calls in the error window");
        // Once the stream ends, a frame asks for nothing and touches no codec.
        assert_eq!(
            decoder.queue(frame(false, vec![(0, PICTURE)])),
            Queued::Taken
        );
        assert_eq!(script(&decoder).input_calls, calls);
    }

    #[test]
    fn backing_off_doubles_up_to_its_limit_and_a_success_resets_the_window() {
        let start = Instant::now();
        let mut health = Health::new(Duration::from_secs(3));
        let waits: Vec<_> = (0..7).map(|_| health.failure(false, start)).collect();
        let ms = |ms| Verdict::BackOff(Duration::from_millis(ms));
        assert_eq!(
            waits,
            [ms(10), ms(20), ms(40), ms(80), ms(160), ms(160), ms(160)]
        );
        assert_eq!(
            health.failure(false, start + Duration::from_secs(3)),
            Verdict::End
        );
        health.success();
        assert_eq!(
            health.failure(false, start + Duration::from_secs(4)),
            ms(10)
        );
        assert_eq!(
            health.failure(true, start + Duration::from_secs(4)),
            Verdict::End
        );
    }

    #[test]
    fn only_the_newest_ready_frame_goes_to_the_display() {
        let decoder = decoder(Script {
            inputs: [Ok(Some(0)), Ok(Some(1))].into(),
            buffer: Some(64),
            ..Script::default()
        });
        decoder.queue(frame(false, vec![(0, PICTURE)]));
        decoder.queue(frame(false, vec![(0, PICTURE)]));
        let pts: Vec<u64> = script(&decoder).queued.iter().map(|q| q.2).collect();
        script(&decoder).outputs = [
            Ok(Output::FormatChanged),
            Ok(Output::Frame {
                index: 5,
                pts: pts[0],
            }),
            Ok(Output::Frame {
                index: 6,
                pts: pts[1],
            }),
        ]
        .into();
        assert!(!run_output(&decoder, Duration::from_millis(200)));
        assert_eq!(script(&decoder).released, [(5, false), (6, true)]);
    }

    #[test]
    fn the_frame_record_gets_each_frame_through_both_loops() {
        let directory = tempfile::tempdir().unwrap();
        let times = directory.path().join("times.csv");
        let recording = directory.path().join("recording.video");
        let records = Records {
            times: Some(Mutex::new(
                frame_record::Record::new(frame_record::create(&times).unwrap()).unwrap(),
            )),
            recording: Some(Mutex::new(frame_record::Recording::new(
                frame_record::create(&recording).unwrap(),
            ))),
        };
        let decoder = recording_decoder(
            Script {
                inputs: [Ok(Some(0)), Ok(Some(1))].into(),
                buffer: Some(64),
                ..Script::default()
            },
            records,
        );
        for number in [7, 8] {
            let parts = vec![(0, PICTURE)];
            let record = decoder
                .records
                .unit(number, || (1000 * number as u64, true), &parts);
            decoder.queue(Frame {
                number,
                idr: false,
                enqueue_us: number as u64,
                parts,
                record,
            });
        }
        let pts: Vec<u64> = script(&decoder).queued.iter().map(|q| q.2).collect();
        script(&decoder).outputs = [
            Ok(Output::Frame {
                index: 0,
                pts: pts[0],
            }),
            Ok(Output::TryAgain),
            Ok(Output::Frame {
                index: 1,
                pts: pts[1],
            }),
        ]
        .into();
        decoder.start_output().unwrap();
        let until = Instant::now() + Duration::from_secs(2);
        while script(&decoder).released.len() < 2 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        decoder.finish();
        assert!(script(&decoder).stopped, "finish stops the codec");
        let records = Arc::try_unwrap(Arc::try_unwrap(decoder).ok().unwrap().records)
            .ok()
            .unwrap();
        records
            .times
            .unwrap()
            .into_inner()
            .unwrap()
            .close()
            .unwrap();
        records
            .recording
            .unwrap()
            .into_inner()
            .unwrap()
            .close()
            .unwrap();
        let rows: Vec<Vec<String>> = std::fs::read_to_string(&times)
            .unwrap()
            .lines()
            .skip(1)
            .map(|row| row.split(',').map(str::to_owned).collect())
            .collect();
        assert_eq!(rows.len(), 2);
        for (row, number) in rows.iter().zip([7, 8]) {
            assert_eq!(row[1], number.to_string());
            assert_eq!(row[2], (1000 * number).to_string());
            assert!(
                !row[3].is_empty() && !row[4].is_empty(),
                "decoded and handed off: {row:?}"
            );
        }
        assert_eq!(
            std::fs::read(&recording).unwrap(),
            [PICTURE, PICTURE].concat()
        );
    }
}
