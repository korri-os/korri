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
//! steps of a recovery after a decoder failure and its fall-through from one
//! to the next (flush, restart, reset; `doCodecRecoveryIfRequired`), and a
//! new picture size in the middle of the stream: within adaptive
//! playback's maximum the codec follows the new SPS by itself and the decoded
//! size comes from the output format; otherwise the codec restarts as
//! Artemis's recovery restarts it (stop, configure, start). Which step a
//! failure takes, the new start on a decoded frame, the cap on recoveries
//! and when the stream ends are Korri's (`Recovery`). The lowest-latency
//! pacing is upstream Moonlight's. Artemis ships under the GNU
//! GPL version 3 with no "or later" grant, so this file does too. The Rust
//! adaptation is Korri work dated 2026-11-14 and 2026-11-15.
use crate::{
    frame_record, log, sps,
    video_stats::{Sample, VideoStats},
};
use std::{
    collections::VecDeque,
    ffi::c_int,
    fmt,
    fs::File,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::NonNull,
    sync::{
        Arc, Mutex, OnceLock, PoisonError, RwLock, RwLockWriteGuard,
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
    fn gone(self) -> bool {
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
    /// Adaptive playback's largest picture for a stream of `size`, from the
    /// decoder's capabilities (`codec::adaptive_max`); `size` itself when the
    /// decoder cannot tell.
    fn adaptive_max(&self, size: (i32, i32)) -> (i32, i32);
    /// Stop, configure for a picture of `size` (with adaptive playback up to
    /// `max`, when some, or without it when the decoder refuses it), and
    /// start again; the maximum it was configured with. No other call to
    /// the codec runs meanwhile.
    fn restart(
        &self,
        size: (i32, i32),
        max: Option<(i32, i32)>,
    ) -> Result<Option<(i32, i32)>, Status>;
    fn stop(&self);
    /// One step of the recovery after a decoder failure (`Step`). No other
    /// call to the codec runs meanwhile, and the buffer indexes that it gave
    /// before are void after it.
    fn recover(&self, step: Step) -> Result<(), Status>;
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

/// A failure that lasts: each one backs off, so that a failed decoder costs
/// no CPU, and failures that still come a window (3 s) after the first end
/// it. A success opens a new window. The 3 s are Artemis's
/// `EXCEPTION_REPORT_DELAY_MS`; the rest is Korri's (Artemis applies its 3 s
/// only after its recovery tries, and a success does not reset it). The
/// decoder's recovery (`Recovery`) and the sound's output (`audio.rs`, when
/// its device stays lost) use this rule.
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

    pub(crate) fn failure(&mut self, now: Instant) -> Verdict {
        let since = *self.since.get_or_insert(now);
        if now.duration_since(since) >= self.window {
            return Verdict::End;
        }
        let back_off = self.back_off;
        self.back_off = (back_off * 2).min(LAST_BACK_OFF);
        Verdict::BackOff(back_off)
    }
}

/// The recovery's steps, lightest first, in the order that Artemis's
/// `doCodecRecoveryIfRequired` falls through them when a step's own call
/// fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Step {
    /// Drop every frame that the codec holds; it keeps its configuration.
    Flush,
    /// Stop the codec, configure it with the format it had, and start it.
    Restart,
    /// Release the codec, then make, configure and start a new one of the
    /// same name with that format.
    Reset,
}

impl Step {
    fn next(self) -> Self {
        match self {
            Step::Flush => Step::Restart,
            Step::Restart | Step::Reset => Step::Reset,
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Step::Flush => "flush",
            Step::Restart => "restart",
            Step::Reset => "reset",
        })
    }
}

/// Which step a decoder failure takes, or whether the stream ends instead.
/// This rule is Korri's; Artemis picks a restart or a reset from the
/// exception (`isRecoverable`), which the NDK's calls do not give.
///
/// A failure takes the step after the one that worked last; a codec that is
/// gone (reclaimed or deleted) goes straight to a reset. Failures back off
/// as `Health` says. The stream ends when failures still come after the
/// window and a reset has run since the first. A decoded frame ends the
/// run of failures: the next failure starts with a flush again. A stream
/// recovers from `MAX_RECOVERIES` runs at most.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Recovery {
    health: Health,
    /// The next failure's first step.
    next: Step,
    /// The step that worked last, since the last decoded frame.
    worked: Option<Step>,
    /// A reset ran since the last decoded frame.
    reset_ran: bool,
    /// A failure came since the last decoded frame.
    failing: bool,
    /// The runs of failures in this stream so far.
    runs: u32,
}

/// The runs of failures that one stream recovers from; the next one ends
/// it. Without a cap, a decoder that fails every few frames would flush and
/// ask for IDR frames for ever. The number is Artemis's `CR_MAX_TRIES`,
/// which counts each restart and reset; Korri counts each run of failures.
pub(crate) const MAX_RECOVERIES: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    /// Run the steps from this one up, then back off for that long.
    Recover(Step, Duration),
    End(Exhausted),
}

/// Why the recovery gives up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Exhausted {
    /// Failures still came after the window, and a reset had run.
    Window,
    /// The stream had recovered from `MAX_RECOVERIES` runs of failures.
    Tries,
}

impl fmt::Display for Exhausted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Exhausted::Window => {
                write!(f, "failures outlasted the window, a reset included")
            }
            Exhausted::Tries => write!(
                f,
                "the decoder already recovered {MAX_RECOVERIES} times in this stream"
            ),
        }
    }
}

impl Recovery {
    pub(crate) fn new(health: Health) -> Self {
        Self {
            health,
            next: Step::Flush,
            worked: None,
            reset_ran: false,
            failing: false,
            runs: 0,
        }
    }

    pub(crate) fn failure(&mut self, gone: bool, now: Instant) -> Plan {
        if !self.failing {
            if self.runs >= MAX_RECOVERIES {
                return Plan::End(Exhausted::Tries);
            }
            self.runs += 1;
            self.failing = true;
        }
        let back_off = match self.health.failure(now) {
            Verdict::BackOff(back_off) => back_off,
            Verdict::End if self.reset_ran => return Plan::End(Exhausted::Window),
            // Past the window, but every step runs before the end.
            Verdict::End => LAST_BACK_OFF,
        };
        Plan::Recover(if gone { Step::Reset } else { self.next }, back_off)
    }

    /// The steps ran: the one that worked, or none when the reset failed too.
    pub(crate) fn ran(&mut self, worked: Option<Step>) {
        self.reset_ran |= worked.is_none_or(|step| step == Step::Reset);
        self.next = worked.map_or(Step::Reset, Step::next);
        self.worked = worked;
    }

    /// The decoder returned a frame: the step that worked last, when one
    /// ran since the frame before.
    pub(crate) fn decoded(&mut self) -> Option<Step> {
        self.health.success();
        self.next = Step::Flush;
        self.reset_ran = false;
        self.failing = false;
        self.worked.take()
    }
}

/// A decoder failure: what failed, and whether the codec is gone.
struct Failure {
    why: String,
    gone: bool,
}

/// Why a frame did not reach the decoder.
enum Lost {
    Stopped,
    Codec(&'static str, Status),
    /// `AMediaCodec_getInputBuffer` gave NULL: the decoder is in error.
    NoMemory,
    /// The output thread found the decoder failed while this thread waited
    /// for an input buffer.
    Asked,
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
            Lost::Asked => write!(f, "the output thread found the decoder failed"),
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
    /// around a restart or a recovery, and the codec's stop.
    gate: RwLock<()>,
    /// The core's decoder thread waits for the gate: the output thread
    /// does not take it again before it has had it (`exclusive`).
    wanted: AtomicBool,
    input: Mutex<Input>,
    resized: Resized,
    pub(crate) records: Arc<Records>,
    stopping: AtomicBool,
    output: Mutex<Option<JoinHandle<()>>>,
    recovery: Mutex<Recovery>,
    /// A failure that the output thread found, for the core's decoder
    /// thread to recover: only that thread changes the codec.
    asked: Mutex<Option<Failure>>,
    failed: OnceLock<String>,
    /// The stream's health for its snapshot (slice D11): each decode unit's
    /// bytes and each frame that the display drops, as Artemis counts them.
    stats: Mutex<VideoStats>,
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
    /// The codec restarts at the new size; with adaptive playback when it
    /// had it, up to the maximum that its capabilities give for that size.
    Restart { adaptive: bool },
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
            wanted: AtomicBool::new(false),
            input: Mutex::new(Input {
                last_pts: 0,
                queued: VecDeque::new(),
                configured,
            }),
            resized,
            records,
            stopping: AtomicBool::new(false),
            output: Mutex::new(None),
            recovery: Mutex::new(Recovery::new(health)),
            asked: Mutex::new(None),
            failed: OnceLock::new(),
            stats: Mutex::new(VideoStats::default()),
        })
    }

    /// The stream's health now (`MediaCodecDecoderRenderer.viewerHealth`):
    /// none while the stream stops, or when no sample is fresh.
    pub(crate) fn sample(&self, now_ms: u64) -> Option<Sample> {
        if self.stopping.load(Ordering::Relaxed) {
            return None;
        }
        self.stats
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .sample(now_ms)
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

    /// The codec for this thread alone, once the output thread is out of its
    /// calls. The lock does not keep the output thread from taking it again
    /// at once, which kept a restart waiting for seconds; `wanted` does.
    fn exclusive(&self) -> RwLockWriteGuard<'_, ()> {
        self.wanted.store(true, Ordering::Release);
        let codec = self.gate.write().unwrap_or_else(PoisonError::into_inner);
        self.wanted.store(false, Ordering::Release);
        codec
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
        let _codec = self.exclusive();
        self.codec.stop();
        log::line("picture: decoder stopped");
    }

    pub(crate) fn queue(&self, frame: Frame) -> Queued {
        // A stream that stops or ends needs no frame and no IDR frame.
        if self.stopping.load(Ordering::Relaxed) || self.failed.get().is_some() {
            return Queued::Taken;
        }
        // Every unit that arrives, as Artemis counts `decodeUnitLength`
        // before it decodes.
        let bytes = frame.parts.iter().map(|(_, data)| data.len()).sum();
        self.stats
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .encoded(bytes, monotonic_ns() / 1_000_000);
        // The output thread found the codec failed: recover it first.
        if self.asked.lock().unwrap().is_some() {
            return self.recover(None);
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
        // A new size takes effect with the IDR frame that carries it. A
        // restart that fails is a decoder failure: the recovery takes it,
        // and the next IDR frame brings the new size again.
        if let Some(new_size) = new_size.filter(|_| frame.idr)
            && let Err(failure) = self.follow(new_size, frame.number)
        {
            return self.recover(Some(failure));
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
            Ok(()) | Err(Lost::Stopped) => return Queued::Taken,
            Err(lost) => lost,
        };
        let why = format!("frame {} not decoded: {lost}", frame.number);
        log::line(&format!("picture: {why}"));
        match lost {
            Lost::Codec(_, status) => self.recover(Some(Failure {
                why,
                gone: status.gone(),
            })),
            Lost::NoMemory => self.recover(Some(Failure { why, gone: false })),
            Lost::Asked => self.recover(None),
            Lost::Stopped | Lost::TooLarge { .. } => Queued::NeedIdr,
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
                // No buffer frees up before the recovery that was asked for.
                Ok(None) if self.asked.lock().unwrap().is_some() => return Err(Lost::Asked),
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
                        adaptive => Follow::Restart {
                            adaptive: adaptive.is_some(),
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
    /// playback, else by a restart. A restart that fails leaves the size as
    /// it was, and gives its failure.
    fn follow(&self, new_size: NewSize, frame: i32) -> Result<(), Failure> {
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
        if let Follow::Restart { adaptive } = new_size.follow {
            // The decoder's capabilities for the new size, asked again; never
            // a maximum that they did not give.
            let max = adaptive.then(|| self.codec.adaptive_max(new_size.size));
            let (restarted, asked) = {
                let _codec = self.exclusive();
                // The output thread may have found the codec before the
                // restart failed: a restart that works needs no recovery for
                // it; one that fails takes it into its own.
                let asked = self.asked.lock().unwrap().take();
                (self.codec.restart(new_size.size, max), asked)
            };
            let max = restarted.map_err(|status| Failure {
                why: format!("cannot restart the decoder at {w}x{h}: {status}"),
                gone: status.gone() || asked.as_ref().is_some_and(|asked| asked.gone),
            })?;
            log::line(&format!(
                "picture: the decoder restarted at {w}x{h} with adaptive playback {}",
                max.map_or("none".into(), |(mw, mh)| format!("up to {mw}x{mh}"))
            ));
            self.input.lock().unwrap().configured.adaptive = max;
        }
        self.input.lock().unwrap().configured.size = new_size.size;
        Ok(())
    }

    /// The output thread: hand each frame to the display at once. When more
    /// than one frame is ready, only the newest goes; the older ones were
    /// overtaken and are dropped.
    pub(crate) fn release_frames(&self) {
        let codec = &self.codec;
        // The decoded size last told.
        let mut told = None;
        while !self.stopping.load(Ordering::Relaxed) && self.failed.get().is_none() {
            // The core's decoder thread recovers the codec, or wants it for a
            // restart; until it has it, this thread leaves the codec alone.
            if self.asked.lock().unwrap().is_some() || self.wanted.load(Ordering::Acquire) {
                self.pause(FIRST_BACK_OFF);
                continue;
            }
            // A restart or a recovery waits while this thread uses the codec,
            // and not while it waits for one.
            let _codec = self.gate.read().unwrap_or_else(PoisonError::into_inner);
            let mut newest: Option<(usize, Option<u64>)> = None;
            let mut wait = Duration::from_millis(20);
            let mut error = None;
            loop {
                match codec.dequeue_output(wait) {
                    Ok(Output::Frame { index, pts }) => {
                        self.decodes_again();
                        let record = self.decoded(pts);
                        if let Some((older, _)) = newest.replace((index, record)) {
                            let _ = codec.release_output(older, None);
                            // Artemis's `discardOutputBuffer`.
                            self.stats
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .dropped();
                        }
                        wait = Duration::ZERO;
                    }
                    Ok(Output::FormatChanged) => {
                        log::line(&format!("picture: output format {}", codec.output_format()));
                        if let Some(size) = codec.output_size().filter(|&size| told != Some(size)) {
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
            if let Some((what, status)) = error {
                let why = format!("{what}: {status}");
                log::line(&format!("picture: {why}"));
                // Asked with the gate held: the failure is this codec's, not
                // that of a codec that a recovery has made since.
                self.ask(Failure {
                    why,
                    gone: status.gone(),
                });
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

/// The recovery after a decoder failure. Only the core's decoder thread
/// changes the codec; the output thread asks it to, and leaves the codec
/// alone until it has.
impl<C: Codec + 'static> Decoder<C> {
    /// The output thread found the codec failed. The core's decoder thread
    /// recovers it, with its next frame or from its wait for an input buffer.
    fn ask(&self, failure: Failure) {
        self.asked.lock().unwrap().get_or_insert(failure);
    }

    /// Recover the codec from a failure: this thread's own, the one that the
    /// output thread asked about, or both. The frame is lost, and the core
    /// asks the host for an IDR frame: a recovered codec needs one.
    fn recover(&self, own: Option<Failure>) -> Queued {
        // A stream that stops needs no codec: Artemis ignores decoder errors
        // then, and so does this.
        if self.stopping.load(Ordering::Relaxed) {
            return Queued::Taken;
        }
        let back_off = {
            // The output thread stays out of the codec until the steps end.
            let _codec = self.exclusive();
            let asked = self.asked.lock().unwrap().take();
            let failure = match (own, asked) {
                (Some(own), Some(asked)) => Failure {
                    gone: own.gone || asked.gone,
                    ..own
                },
                (own, asked) => match own.or(asked) {
                    Some(failure) => failure,
                    None => return Queued::NeedIdr,
                },
            };
            let plan = self
                .recovery
                .lock()
                .unwrap()
                .failure(failure.gone, Instant::now());
            let (from, back_off) = match plan {
                Plan::Recover(from, back_off) => (from, back_off),
                Plan::End(exhausted) => {
                    self.fail(format!("{}; {exhausted}", failure.why));
                    return Queued::Taken;
                }
            };
            log::line(&format!("picture: {}; recovery from a {from}", failure.why));
            let worked = self.run_steps(from);
            self.recovery.lock().unwrap().ran(worked);
            // The frames that the codec held are lost. Their rows stay open,
            // and the next frame handed to the display writes them as not
            // shown.
            self.input.lock().unwrap().queued.clear();
            back_off
        };
        self.pause(back_off);
        Queued::NeedIdr
    }

    /// Run the steps from `from` up until one works: a step whose own call
    /// fails leads to the next at once, as Artemis falls through its steps.
    /// The step that worked; none when the reset failed too.
    fn run_steps(&self, from: Step) -> Option<Step> {
        let mut step = from;
        loop {
            match self.codec.recover(step) {
                Ok(()) => {
                    log::line(&format!(
                        "picture: {step} done; the core asks for an IDR frame"
                    ));
                    return Some(step);
                }
                Err(status) => {
                    log::line(&format!("picture: {step} failed: {status}"));
                    if step == Step::Reset {
                        return None;
                    }
                    step = step.next();
                }
            }
        }
    }

    /// The decoder returned a frame: a recovery that ran is over.
    fn decodes_again(&self) {
        if let Some(step) = self.recovery.lock().unwrap().decoded() {
            log::line(&format!(
                "picture: the decoder returns frames again after a {step}"
            ));
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
        /// A restart or a recovery step ran while the output thread was
        /// inside a codec call.
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
        /// Each buffer call fails with this status until a recovery step of
        /// at least `heals` runs; with `heals` none, no step heals it.
        broken: Option<Status>,
        heals: Option<Step>,
        /// The steps whose own call fails.
        refuses: Vec<Step>,
        // What the codec was asked.
        input_calls: usize,
        output_calls: usize,
        queued: Vec<(usize, Vec<u8>, u64, u32)>,
        released: Vec<(usize, bool)>,
        /// Each recovery step, and whether its call worked.
        recovered: Vec<(Step, bool)>,
        stopped: bool,
        /// The size that each output format gives, in turn.
        formats: VecDeque<(i32, i32)>,
        /// A restart fails with this status.
        restart_fails: Option<Status>,
        /// A restart's configure refuses `max-width` and `max-height`.
        refuses_max: bool,
        restarts: Vec<Restarted>,
        /// The largest square side that the decoder supports; none: it
        /// cannot tell.
        largest_square: Option<i32>,
        /// The sizes that `adaptive_max` was asked for.
        capability_asks: Vec<(i32, i32)>,
        /// How long `adaptive_max` takes, after it records the ask.
        capabilities_take: Duration,
    }

    /// A restart: how many buffers were queued before it, its size and its
    /// maximum.
    type Restarted = (usize, (i32, i32), Option<(i32, i32)>);

    impl Codec for ScriptedCodec {
        fn dequeue_input(&self, _: Duration) -> Result<Option<usize>, Status> {
            let mut s = self.script.lock().unwrap();
            s.input_calls += 1;
            if let Some(status) = s.broken {
                return Err(status);
            }
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
                match s.broken {
                    Some(status) => Some(Err(status)),
                    None => s.outputs.pop_front().or(s.outputs_then),
                }
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
        fn adaptive_max(&self, size: (i32, i32)) -> (i32, i32) {
            let (largest, takes) = {
                let mut s = self.script.lock().unwrap();
                s.capability_asks.push(size);
                (s.largest_square, s.capabilities_take)
            };
            std::thread::sleep(takes);
            crate::codec::adaptive_max(size, |side| largest.is_some_and(|largest| side <= largest))
        }
        fn restart(
            &self,
            size: (i32, i32),
            max: Option<(i32, i32)>,
        ) -> Result<Option<(i32, i32)>, Status> {
            // Long enough for an output call to begin if nothing held it off.
            std::thread::sleep(Duration::from_millis(30));
            if self.in_output.load(Ordering::SeqCst) {
                self.overlapped.store(true, Ordering::SeqCst);
            }
            let mut s = self.script.lock().unwrap();
            let queued = s.queued.len();
            let max = max.filter(|_| !s.refuses_max);
            s.restarts.push((queued, size, max));
            s.restart_fails.map_or(Ok(max), Err)
        }
        fn stop(&self) {
            self.script.lock().unwrap().stopped = true;
        }
        fn recover(&self, step: Step) -> Result<(), Status> {
            // Long enough for an output call to begin if nothing held it off.
            std::thread::sleep(Duration::from_millis(5));
            if self.in_output.load(Ordering::SeqCst) {
                self.overlapped.store(true, Ordering::SeqCst);
            }
            let mut s = self.script.lock().unwrap();
            let works = !s.refuses.contains(&step);
            s.recovered.push((step, works));
            if !works {
                return Err(Status(-10005));
            }
            if s.heals.is_some_and(|heals| step >= heals) {
                s.broken = None;
            }
            Ok(())
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

    /// A decoder whose output thread runs; `finish` stops it.
    fn running(script: Script) -> Arc<Decoder<ScriptedCodec>> {
        let decoder = decoder(script);
        decoder.start_output().unwrap();
        decoder
    }

    /// A codec that takes every frame, broken until a step of at least
    /// `heals` runs.
    fn broken(heals: Option<Step>) -> Script {
        Script {
            broken: Some(Status(-10000)),
            heals,
            inputs_then: Some(Ok(Some(0))),
            buffer: Some(4096),
            ..Script::default()
        }
    }

    fn key(number: i32) -> Frame<'static> {
        Frame {
            idr: true,
            ..delta(number)
        }
    }

    fn steps(decoder: &Decoder<ScriptedCodec>) -> Vec<(Step, bool)> {
        script(decoder).recovered.clone()
    }

    /// Wait up to 2 s for the script to show `done`.
    fn until(decoder: &Decoder<ScriptedCodec>, done: impl Fn(&Script) -> bool) -> bool {
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            if done(&script(decoder)) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        false
    }

    /// The codec returns the last frame queued; true once the running
    /// output thread has handed it to the display.
    fn decodes(decoder: &Decoder<ScriptedCodec>) -> bool {
        let index = {
            let mut s = script(decoder);
            let index = 100 + s.released.len();
            let pts = s.queued.last().expect("a queued frame").2;
            s.outputs.push_back(Ok(Output::Frame { index, pts }));
            index
        };
        until(decoder, |s| s.released.contains(&(index, true)))
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
    fn a_size_beyond_the_adaptive_maximum_restarts_with_the_decoders_maximum_for_it() {
        let decoder = shaped_decoder(
            Script {
                largest_square: Some(2304),
                ..many_inputs()
            },
            Records::default(),
            configured(Some((1280, 1280))),
            Arc::new(|_| {}),
        );
        let (landscape, full_hd) = (sps_unit(LANDSCAPE), sps_unit(FULL_HD));
        decoder.queue(idr(1, &full_hd));
        // The decoder's capabilities are asked again, for the new size.
        assert_eq!(script(&decoder).capability_asks, [(1920, 1080)]);
        assert_eq!(
            script(&decoder).restarts,
            [(0, (1920, 1080), Some((2304, 2304)))]
        );
        // Back within the new maximum: no restart, and no question.
        decoder.queue(idr(2, &landscape));
        assert_eq!(script(&decoder).restarts.len(), 1);
        assert_eq!(script(&decoder).capability_asks.len(), 1);
    }

    #[test]
    fn a_restart_that_the_decoder_takes_without_its_maximum_drops_adaptive_playback() {
        let decoder = shaped_decoder(
            Script {
                refuses_max: true,
                largest_square: Some(2304),
                ..many_inputs()
            },
            Records::default(),
            configured(Some((1280, 1280))),
            Arc::new(|_| {}),
        );
        let (landscape, full_hd) = (sps_unit(LANDSCAPE), sps_unit(FULL_HD));
        decoder.queue(idr(1, &full_hd));
        // 1280x720 was within the old maximum, but the codec has none now.
        decoder.queue(idr(2, &landscape));
        assert_eq!(
            script(&decoder).restarts,
            [(0, (1920, 1080), None), (2, (1280, 720), None)]
        );
        // Without adaptive playback, no capabilities are asked.
        assert_eq!(script(&decoder).capability_asks, [(1920, 1080)]);
    }

    #[test]
    fn with_no_larger_square_the_restart_maximum_is_the_new_size() {
        let decoder = shaped_decoder(
            many_inputs(),
            Records::default(),
            configured(Some((1280, 1280))),
            Arc::new(|_| {}),
        );
        let (portrait, full_hd) = (sps_unit(PORTRAIT), sps_unit(FULL_HD));
        decoder.queue(idr(1, &full_hd));
        assert_eq!(
            script(&decoder).restarts,
            [(0, (1920, 1080), Some((1920, 1080)))]
        );
        // 720x1280 is not within 1920x1080: a restart again, asked again.
        decoder.queue(idr(2, &portrait));
        assert_eq!(
            script(&decoder).restarts[1..],
            [(2, (720, 1280), Some((720, 1280)))]
        );
        assert_eq!(
            script(&decoder).capability_asks,
            [(1920, 1080), (720, 1280)]
        );
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
    fn a_failed_size_restart_goes_through_the_recovery_and_the_next_idr_frame_tries_again() {
        let decoder = decoder(Script {
            restart_fails: Some(Status(-10000)),
            ..many_inputs()
        });
        let portrait = sps_unit(PORTRAIT);
        // The frame is lost, the recovery runs, and the core asks for an IDR
        // frame; the stream goes on.
        assert_eq!(decoder.queue(idr(1, &portrait)), Queued::NeedIdr);
        assert!(decoder.failure().is_none());
        assert_eq!(steps(&decoder), [(Step::Flush, true)]);
        assert!(script(&decoder).queued.is_empty());
        // The codec's size stayed as it was, so the next IDR frame restarts
        // at the new size again.
        script(&decoder).restart_fails = None;
        assert_eq!(decoder.queue(idr(2, &portrait)), Queued::Taken);
        let s = script(&decoder);
        let sizes: Vec<_> = s.restarts.iter().map(|restart| restart.1).collect();
        assert_eq!(sizes, [(720, 1280), (720, 1280)]);
        let flags: Vec<u32> = s.queued.iter().map(|queued| queued.3).collect();
        assert_eq!(flags, [CODEC_CONFIG, KEY_FRAME]);
    }

    #[test]
    fn a_size_restart_that_always_fails_ends_the_stream_after_every_step() {
        let decoder = running(Script {
            restart_fails: Some(Status(-10000)),
            ..many_inputs()
        });
        let portrait = sps_unit(PORTRAIT);
        let started = Instant::now();
        let mut number = 0;
        while started.elapsed() < Duration::from_secs(5) && decoder.failure().is_none() {
            number += 1;
            decoder.queue(idr(number, &portrait));
        }
        let why = decoder
            .failure()
            .expect("restarts that keep failing end the stream");
        assert!(
            why.contains("cannot restart the decoder at 720x1280"),
            "{why}"
        );
        assert!(started.elapsed() >= Duration::from_secs(3), "the window");
        let order: Vec<Step> = steps(&decoder).iter().map(|(step, _)| *step).collect();
        assert_eq!(order[..3], [Step::Flush, Step::Restart, Step::Reset]);
        assert!(order[3..].iter().all(|step| *step == Step::Reset));
        // Nothing went to a codec that did not restart at the stream's size.
        assert!(script(&decoder).queued.is_empty());
        decoder.finish();
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
    fn a_flush_that_heals_ends_the_recovery_and_its_frame_asks_for_an_idr_frame() {
        let decoder = running(broken(Some(Step::Flush)));
        // The frame that met the failure is lost: the core asks for an IDR
        // frame, which the flushed codec needs.
        assert_eq!(decoder.queue(delta(1)), Queued::NeedIdr);
        assert_eq!(steps(&decoder), [(Step::Flush, true)]);
        assert_eq!(decoder.queue(key(2)), Queued::Taken);
        assert!(decodes(&decoder), "the picture goes on");
        assert!(decoder.failure().is_none());
        // The decoded frame ended the recovery: a new failure flushes again.
        script(&decoder).broken = Some(Status(-10000));
        assert_eq!(decoder.queue(delta(3)), Queued::NeedIdr);
        assert_eq!(steps(&decoder), [(Step::Flush, true), (Step::Flush, true)]);
        decoder.finish();
    }

    #[test]
    fn when_the_failure_outlasts_the_flush_a_restart_heals() {
        let decoder = running(broken(Some(Step::Restart)));
        assert_eq!(decoder.queue(delta(1)), Queued::NeedIdr);
        assert_eq!(decoder.queue(key(2)), Queued::NeedIdr);
        assert_eq!(
            steps(&decoder),
            [(Step::Flush, true), (Step::Restart, true)]
        );
        assert_eq!(decoder.queue(key(3)), Queued::Taken);
        assert!(decodes(&decoder), "the picture goes on");
        assert!(decoder.failure().is_none());
        decoder.finish();
    }

    #[test]
    fn when_the_failure_outlasts_the_restart_a_reset_heals() {
        let decoder = running(broken(Some(Step::Reset)));
        for number in 1..=3 {
            assert_eq!(decoder.queue(key(number)), Queued::NeedIdr);
        }
        assert_eq!(
            steps(&decoder),
            [
                (Step::Flush, true),
                (Step::Restart, true),
                (Step::Reset, true)
            ]
        );
        assert_eq!(decoder.queue(key(4)), Queued::Taken);
        assert!(decodes(&decoder), "the picture goes on");
        assert!(decoder.failure().is_none());
        decoder.finish();
    }

    #[test]
    fn a_step_that_the_codec_refuses_leads_to_the_next_at_once() {
        let decoder = running(Script {
            refuses: vec![Step::Flush],
            ..broken(Some(Step::Restart))
        });
        assert_eq!(decoder.queue(delta(1)), Queued::NeedIdr);
        assert_eq!(
            steps(&decoder),
            [(Step::Flush, false), (Step::Restart, true)]
        );
        assert_eq!(decoder.queue(key(2)), Queued::Taken);
        assert!(decodes(&decoder), "the picture goes on");
        decoder.finish();
    }

    #[test]
    fn a_reclaimed_codec_goes_straight_to_a_reset() {
        let decoder = running(Script {
            broken: Some(Status(1101)),
            ..broken(Some(Step::Reset))
        });
        assert_eq!(decoder.queue(delta(1)), Queued::NeedIdr);
        assert_eq!(steps(&decoder), [(Step::Reset, true)]);
        assert_eq!(decoder.queue(key(2)), Queued::Taken);
        assert!(decodes(&decoder), "the picture goes on");
        assert!(decoder.failure().is_none());
        decoder.finish();
    }

    /// The core keeps sending frames until the stream ends or `limit` passes.
    fn feed_until_the_end(decoder: &Decoder<ScriptedCodec>, limit: Duration) -> Duration {
        let started = Instant::now();
        let mut number = 0;
        while started.elapsed() < limit && decoder.failure().is_none() {
            number += 1;
            decoder.queue(key(number));
        }
        started.elapsed()
    }

    /// `steps_work`: each step's own call works, and the failure comes back
    /// after it; else each call fails.
    fn ends_after_every_step(codec: Script, steps_work: bool) {
        let window = Duration::from_secs(3);
        let decoder = running(codec);
        let took = feed_until_the_end(&decoder, Duration::from_secs(5));
        let why = decoder
            .failure()
            .expect("failures that last end the stream");
        assert!(why.contains("AMEDIA_ERROR_UNKNOWN"), "{why}");
        assert!(took >= window, "the stream ended within the window");
        let steps = steps(&decoder);
        let order: Vec<Step> = steps.iter().map(|(step, _)| *step).collect();
        // A flush, a restart, then resets until the end.
        assert_eq!(order[..3], [Step::Flush, Step::Restart, Step::Reset]);
        assert!(order[3..].iter().all(|step| *step == Step::Reset));
        assert!(steps.iter().all(|(_, worked)| *worked == steps_work));
        let calls = {
            let s = script(&decoder);
            s.input_calls + s.output_calls + s.recovered.len()
        };
        // Backing off between failures, 160 ms at most: far fewer calls
        // than a spin, which makes thousands.
        assert!(calls < 200, "{calls} codec calls in the window");
        // Once the stream ends, a frame asks for nothing and touches no codec.
        let input_calls = script(&decoder).input_calls;
        assert_eq!(decoder.queue(delta(1_000)), Queued::Taken);
        assert_eq!(script(&decoder).input_calls, input_calls);
        decoder.finish();
    }

    #[test]
    fn when_no_step_heals_the_stream_ends_after_the_window() {
        ends_after_every_step(broken(None), true);
    }

    #[test]
    fn when_the_codec_refuses_every_step_the_stream_ends_after_the_window() {
        ends_after_every_step(
            Script {
                refuses: vec![Step::Flush, Step::Restart, Step::Reset],
                ..broken(None)
            },
            false,
        );
    }

    #[test]
    fn an_output_failure_brings_the_cores_thread_out_of_its_wait_for_a_buffer() {
        let decoder = decoder(Script {
            inputs_then: Some(Ok(None)),
            buffer: Some(4096),
            ..Script::default()
        });
        let core = {
            let decoder = decoder.clone();
            std::thread::spawn(move || decoder.queue(delta(1)))
        };
        // The core's thread waits for an input buffer that never frees up.
        assert!(until(&decoder, |s| s.input_calls >= 3));
        script(&decoder).outputs = [Err(Status(-10000))].into();
        decoder.start_output().unwrap();
        // The output thread found the failure; the core's thread recovers.
        assert_eq!(core.join().unwrap(), Queued::NeedIdr);
        assert_eq!(steps(&decoder), [(Step::Flush, true)]);
        assert!(decoder.failure().is_none());
        decoder.finish();
    }

    #[test]
    fn a_recovery_gets_the_codec_soon_while_the_output_thread_keeps_calling_it() {
        // Only the input side fails: the output thread goes on waiting in
        // the codec for frames, and leaves it between calls only.
        let decoder = running(Script {
            inputs: (0..5).map(|_| Err(Status(-10000))).collect(),
            inputs_then: Some(Ok(Some(0))),
            buffer: Some(4096),
            ..Script::default()
        });
        let started = Instant::now();
        for number in 1..=5 {
            assert_eq!(decoder.queue(delta(number)), Queued::NeedIdr);
        }
        let took = started.elapsed();
        assert_eq!(steps(&decoder).len(), 5);
        // Five back-offs (10 to 160 ms) and five steps; a recovery that
        // waited for the gate behind the output thread took seconds.
        assert!(
            took < Duration::from_secs(2),
            "five recoveries took {took:?}"
        );
        assert!(!decoder.codec.overlapped.load(Ordering::SeqCst));
        decoder.finish();
    }

    #[test]
    fn a_decoder_that_keeps_failing_now_and_then_ends_the_stream_after_its_recoveries() {
        let decoder = running(Script {
            heals: Some(Step::Flush),
            ..many_inputs()
        });
        let mut number = 0;
        for run in 1..=MAX_RECOVERIES {
            // A failure, a flush that heals, and a decoded frame.
            script(&decoder).broken = Some(Status(-10000));
            number += 1;
            assert_eq!(decoder.queue(delta(number)), Queued::NeedIdr, "run {run}");
            number += 1;
            assert_eq!(decoder.queue(key(number)), Queued::Taken);
            assert!(decodes(&decoder), "run {run}");
        }
        assert!(decoder.failure().is_none());
        script(&decoder).broken = Some(Status(-10000));
        assert_eq!(decoder.queue(delta(number + 1)), Queued::Taken);
        let why = decoder.failure().expect("the cap ends the stream");
        assert!(why.contains("already recovered 10 times"), "{why}");
        assert_eq!(steps(&decoder).len(), MAX_RECOVERIES as usize);
        decoder.finish();
    }

    #[test]
    fn a_failure_while_the_stream_stops_runs_no_recovery() {
        let decoder = decoder(many_inputs());
        decoder.ask(Failure {
            why: "output failed: AMEDIA_ERROR_UNKNOWN (-10000)".into(),
            gone: false,
        });
        decoder.stop();
        assert_eq!(decoder.recover(None), Queued::Taken);
        assert!(steps(&decoder).is_empty());
        assert!(decoder.failure().is_none());
    }

    /// The core's thread brings a frame of a size beyond adaptive playback's
    /// maximum; while it asks the decoder's capabilities for that size, the
    /// output thread finds the old codec failed. The frame's answer.
    fn failure_found_during_a_size_restart(
        restart_fails: Option<Status>,
    ) -> (Arc<Decoder<ScriptedCodec>>, Queued) {
        let decoder = shaped_decoder(
            Script {
                largest_square: Some(2304),
                capabilities_take: Duration::from_millis(200),
                restart_fails,
                ..many_inputs()
            },
            Records::default(),
            configured(Some((1280, 1280))),
            Arc::new(|_| {}),
        );
        let core = {
            let decoder = decoder.clone();
            std::thread::spawn(move || decoder.queue(idr(1, &sps_unit(FULL_HD))))
        };
        assert!(until(&decoder, |s| !s.capability_asks.is_empty()));
        decoder.ask(Failure {
            why: "output failed: AMEDIACODEC_ERROR_RECLAIMED (1101)".into(),
            gone: true,
        });
        let answer = core.join().unwrap();
        (decoder, answer)
    }

    #[test]
    fn a_failure_found_during_a_size_restart_costs_nothing_once_the_restart_works() {
        let (decoder, answer) = failure_found_during_a_size_restart(None);
        assert_eq!(answer, Queued::Taken);
        assert_eq!(script(&decoder).restarts.len(), 1);
        assert!(
            steps(&decoder).is_empty(),
            "no flush on the restarted codec"
        );
        assert!(decoder.asked.lock().unwrap().is_none());
        // The next frame needs no recovery either.
        assert_eq!(decoder.queue(delta(2)), Queued::Taken);
        assert!(steps(&decoder).is_empty());
    }

    #[test]
    fn a_failure_found_during_a_size_restart_that_fails_joins_its_recovery() {
        let (decoder, answer) = failure_found_during_a_size_restart(Some(Status(-10000)));
        assert_eq!(answer, Queued::NeedIdr);
        // The codec was found gone: straight to a reset, once.
        assert_eq!(steps(&decoder), [(Step::Reset, true)]);
        assert!(decoder.asked.lock().unwrap().is_none());
    }

    #[test]
    fn a_recovery_never_runs_while_the_output_thread_is_in_a_codec_call() {
        let decoder = running(Script {
            heals: Some(Step::Flush),
            inputs_then: Some(Ok(Some(0))),
            buffer: Some(4096),
            ..Script::default()
        });
        for number in 1..=20 {
            // The output thread waits in the codec for a frame meanwhile.
            std::thread::sleep(Duration::from_millis(3));
            script(&decoder).broken = Some(Status(-10000));
            assert_eq!(decoder.queue(delta(number)), Queued::NeedIdr);
        }
        assert_eq!(steps(&decoder).len(), 20);
        assert!(!decoder.codec.overlapped.load(Ordering::SeqCst));
        decoder.finish();
    }

    #[test]
    fn the_recovery_climbs_from_a_flush_to_a_reset_and_ends_only_after_a_reset() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let ms = Duration::from_millis;
        let mut recovery = Recovery::new(Health::new(Duration::from_secs(3)));
        assert_eq!(
            recovery.failure(false, at(0)),
            Plan::Recover(Step::Flush, ms(10))
        );
        recovery.ran(Some(Step::Flush));
        assert_eq!(
            recovery.failure(false, at(100)),
            Plan::Recover(Step::Restart, ms(20))
        );
        recovery.ran(Some(Step::Restart));
        // Past the window, but no reset has run yet: it runs first.
        assert_eq!(
            recovery.failure(false, at(3_000)),
            Plan::Recover(Step::Reset, ms(160))
        );
        recovery.ran(None);
        assert_eq!(
            recovery.failure(false, at(3_100)),
            Plan::End(Exhausted::Window)
        );
        // A decoded frame ends the recovery: the next failure flushes again.
        recovery.decoded();
        assert_eq!(
            recovery.failure(false, at(4_000)),
            Plan::Recover(Step::Flush, ms(10))
        );
        recovery.ran(Some(Step::Flush));
        assert_eq!(recovery.decoded(), Some(Step::Flush));
        assert_eq!(recovery.decoded(), None, "no recovery since that frame");
        // A codec that is gone goes straight to a reset; failures that come
        // through the window after it end the stream.
        assert_eq!(
            recovery.failure(true, at(5_000)),
            Plan::Recover(Step::Reset, ms(10))
        );
        recovery.ran(Some(Step::Reset));
        assert_eq!(
            recovery.failure(false, at(5_100)),
            Plan::Recover(Step::Reset, ms(20))
        );
        recovery.ran(Some(Step::Reset));
        assert_eq!(
            recovery.failure(false, at(8_000)),
            Plan::End(Exhausted::Window)
        );
    }

    #[test]
    fn a_stream_recovers_from_ten_runs_of_failures_and_ends_on_the_eleventh() {
        let start = Instant::now();
        let mut recovery = Recovery::new(Health::new(Duration::from_secs(3)));
        for run in 0..u64::from(MAX_RECOVERIES) {
            let at = start + Duration::from_secs(run);
            assert!(matches!(
                recovery.failure(false, at),
                Plan::Recover(Step::Flush, _)
            ));
            recovery.ran(Some(Step::Flush));
            // More failures in the same run count once.
            assert!(matches!(
                recovery.failure(false, at),
                Plan::Recover(Step::Restart, _)
            ));
            recovery.ran(Some(Step::Restart));
            recovery.decoded();
        }
        let at = start + Duration::from_secs(60);
        assert_eq!(recovery.failure(false, at), Plan::End(Exhausted::Tries));
    }

    #[test]
    fn backing_off_doubles_up_to_its_limit_and_a_success_resets_the_window() {
        let start = Instant::now();
        let mut health = Health::new(Duration::from_secs(3));
        let waits: Vec<_> = (0..7).map(|_| health.failure(start)).collect();
        let ms = |ms| Verdict::BackOff(Duration::from_millis(ms));
        assert_eq!(
            waits,
            [ms(10), ms(20), ms(40), ms(80), ms(160), ms(160), ms(160)]
        );
        assert_eq!(health.failure(start + Duration::from_secs(3)), Verdict::End);
        health.success();
        assert_eq!(health.failure(start + Duration::from_secs(4)), ms(10));
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

    // Slice D11: the health counts each unit's bytes and the frame that the
    // display dropped.
    #[test]
    fn the_health_counts_each_units_bytes_and_each_dropped_frame() {
        let decoder = decoder(Script {
            inputs: [Ok(Some(0)), Ok(Some(1))].into(),
            buffer: Some(64),
            ..Script::default()
        });
        let began = monotonic_ns() / 1_000_000;
        decoder.queue(frame(false, vec![(0, PICTURE)]));
        decoder.queue(frame(false, vec![(0, PICTURE), (0, PICTURE)]));
        let pts: Vec<u64> = script(&decoder).queued.iter().map(|q| q.2).collect();
        script(&decoder).outputs = [
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
        // The interval ends with the next unit, more than a second after
        // the first.
        let ended = began + 1500;
        decoder.stats.lock().unwrap().encoded(PICTURE.len(), ended);
        let sample = decoder.stats.lock().unwrap().sample(ended).unwrap();
        assert_eq!(sample.frames_dropped, 1);
        let bits = (4 * PICTURE.len() * 8) as f64;
        assert!(sample.delivered_bitrate_kbps >= bits / 1500.0, "{sample:?}");
        assert!(sample.delivered_bitrate_kbps < bits / 1000.0, "{sample:?}");
        // A stream that stops has no health.
        assert_eq!(decoder.sample(ended), None);
    }

    #[test]
    fn the_frame_record_stays_in_order_across_a_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let times = directory.path().join("times.csv");
        let records = Records {
            times: Some(Mutex::new(
                frame_record::Record::new(frame_record::create(&times).unwrap()).unwrap(),
            )),
            recording: None,
        };
        let decoder = recording_decoder(
            Script {
                heals: Some(Step::Flush),
                inputs_then: Some(Ok(Some(0))),
                buffer: Some(64),
                ..Script::default()
            },
            records,
        );
        decoder.start_output().unwrap();
        let queue = |number: i32, idr: bool| {
            let parts = vec![(0, PICTURE)];
            let record = decoder
                .records
                .unit(number, || (1000 * number as u64, true), &parts);
            decoder.queue(Frame {
                number,
                idr,
                enqueue_us: number as u64,
                parts,
                record,
            })
        };
        // Frame 1 is shown; frame 2 is still in the codec when the codec
        // fails on frame 3.
        assert_eq!(queue(1, true), Queued::Taken);
        assert!(decodes(&decoder));
        assert_eq!(queue(2, false), Queued::Taken);
        script(&decoder).broken = Some(Status(-10000));
        assert_eq!(queue(3, false), Queued::NeedIdr);
        // The flushed codec decodes the IDR frame after it.
        assert_eq!(queue(4, true), Queued::Taken);
        assert!(decodes(&decoder));
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
        assert_eq!(numbers, ["1", "2", "3", "4"], "one row per unit, in order");
        let timed = |row: &Vec<String>| (!row[3].is_empty(), !row[4].is_empty());
        assert_eq!(timed(&rows[0]), (true, true), "{:?}", rows[0]);
        // Lost in the recovery: neither decoded nor shown.
        assert_eq!(timed(&rows[1]), (false, false), "{:?}", rows[1]);
        assert_eq!(timed(&rows[2]), (false, false), "{:?}", rows[2]);
        assert_eq!(timed(&rows[3]), (true, true), "{:?}", rows[3]);
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
