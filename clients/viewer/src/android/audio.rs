// SPDX-License-Identifier: GPL-3.0-only
//! One stream's sound on Android (#391 step D, slice D3), over any audio
//! output with AAudio's stream calls (`Output`). `sound.rs` runs it over
//! AAudio; the tests run it over `ScriptedOutput`.
//!
//! The core's audio thread hands each Opus packet to `Player::play`. The
//! player decodes it with libopus, as the Linux viewer and Artemis do, and
//! puts its PCM in `Feed`: a ring that the output's own high-priority
//! callback empties. That callback never waits and takes no lock: it takes
//! what is there and plays silence for the rest.
//!
//! Ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`, a fork of
//! `ClassicOldSong/moonlight-android`, itself a fork of
//! `moonlight-stream/moonlight-android`): `AndroidAudioRenderer.java` (the
//! low-latency mode first, a buffer of two packets, no more than 40 ms of
//! pending sound, the game usage) and `callbacks.c` (`BridgeArInit`,
//! `BridgeArDecodeAndPlaySample`: the decoder from the core's configuration,
//! with the core's channel order, which is Android's). Artemis ships under the
//! GNU GPL version 3 with no "or later" grant, so this file does too. The
//! Rust adaptation is Korri work dated 2026-11-14.
use crate::{
    decoder::{Health, Verdict},
    log,
};
use std::{
    ffi::c_int,
    fmt,
    ptr::{NonNull, null},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI16, AtomicI32, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

/// Artemis writes a packet only while less than 40 ms of sound is pending
/// beyond the output's own buffer; it drops the packet otherwise.
pub(crate) const PENDING_LIMIT_MS: u32 = 40;

/// The core's Opus configuration (`OPUS_MULTISTREAM_CONFIGURATION`). The
/// mapping's channel order is front left, front right, centre, LFE, back
/// left, back right, side left, side right: Android's own order for stereo,
/// 5.1 and 7.1, so the PCM goes out as it comes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) sample_rate: i32,
    pub(crate) channels: i32,
    pub(crate) streams: i32,
    pub(crate) coupled_streams: i32,
    pub(crate) samples_per_frame: i32,
    pub(crate) mapping: [u8; 8],
}

impl Config {
    /// What this player can play: the core's own limits.
    fn check(&self) -> Result<(), String> {
        let fits = (1..=8).contains(&self.channels)
            && self.sample_rate > 0
            // Opus frames last at most 120 ms.
            && (1..=self.sample_rate / 1000 * 120).contains(&self.samples_per_frame);
        if fits {
            Ok(())
        } else {
            Err(format!(
                "an Opus configuration this player cannot play: {self:?}"
            ))
        }
    }

    fn ms(&self, frames: usize) -> u32 {
        (frames as u64 * 1000 / self.sample_rate as u64) as u32
    }
}

#[repr(C)]
struct OpusMSDecoder {
    _private: [u8; 0],
}

// libopus: Linux links it through pkg-config (build.rs), Android through
// the static library that android.nix builds.
unsafe extern "C" {
    fn opus_multistream_decoder_create(
        sample_rate: i32,
        channels: c_int,
        streams: c_int,
        coupled_streams: c_int,
        mapping: *const u8,
        error: *mut c_int,
    ) -> *mut OpusMSDecoder;
    fn opus_multistream_decode(
        decoder: *mut OpusMSDecoder,
        data: *const u8,
        length: i32,
        pcm: *mut i16,
        frame_size: c_int,
        decode_fec: c_int,
    ) -> c_int;
    fn opus_multistream_decoder_destroy(decoder: *mut OpusMSDecoder);
}

/// `OPUS_BAD_ARG`.
const OPUS_BAD_ARG: i32 = -1;

/// The stream's Opus decoder, with its PCM buffer of one packet.
pub(crate) struct Opus {
    decoder: NonNull<OpusMSDecoder>,
    channels: usize,
    frame: c_int,
    pcm: Vec<i16>,
}

// SAFETY: the decoder is used by one thread at a time, behind the player.
unsafe impl Send for Opus {}

impl Opus {
    pub(crate) fn new(config: &Config) -> Result<Self, String> {
        config.check()?;
        let mut error = 0;
        // SAFETY: the mapping holds 8 entries, as many as the channels allow.
        let decoder = unsafe {
            opus_multistream_decoder_create(
                config.sample_rate,
                config.channels,
                config.streams,
                config.coupled_streams,
                config.mapping.as_ptr(),
                &mut error,
            )
        };
        let decoder = NonNull::new(decoder)
            .ok_or_else(|| format!("Opus refused the stream's configuration ({error})"))?;
        let channels = config.channels as usize;
        Ok(Self {
            decoder,
            channels,
            frame: config.samples_per_frame,
            pcm: vec![0; channels * config.samples_per_frame as usize],
        })
    }

    /// One packet's PCM, interleaved. `None` is a packet that the network
    /// lost: Opus conceals it, as the core asks.
    pub(crate) fn decode(&mut self, packet: Option<&[u8]>) -> Result<&[i16], i32> {
        let (data, length) = match packet {
            Some(packet) => (
                packet.as_ptr(),
                i32::try_from(packet.len()).map_err(|_| OPUS_BAD_ARG)?,
            ),
            None => (null(), 0),
        };
        // SAFETY: a live decoder; the PCM buffer holds one packet's frames.
        let frames = unsafe {
            opus_multistream_decode(
                self.decoder.as_ptr(),
                data,
                length,
                self.pcm.as_mut_ptr(),
                self.frame,
                0,
            )
        };
        if frames < 0 {
            return Err(frames);
        }
        Ok(&self.pcm[..frames as usize * self.channels])
    }
}

impl Drop for Opus {
    fn drop(&mut self) {
        // SAFETY: the decoder is ours and no longer used.
        unsafe { opus_multistream_decoder_destroy(self.decoder.as_ptr()) };
    }
}

/// The PCM between the decoder and the output's callback: a ring with one
/// writer (the core's audio thread) and one reader (the callback of the one
/// open stream), and what the callbacks saw.
pub(crate) struct Feed {
    channels: usize,
    ring: Box<[AtomicI16]>,
    /// Samples ever written and ever read; the difference is in the ring.
    written: AtomicUsize,
    read: AtomicUsize,
    callbacks: AtomicU64,
    played: AtomicU64,
    silent: AtomicU64,
    short: AtomicU64,
    sounded: AtomicBool,
    /// The output's error callback: the stream is lost, with this code.
    lost: AtomicBool,
    lost_code: AtomicI32,
    /// A callback panicked.
    broken: AtomicBool,
}

impl Feed {
    pub(crate) fn new(config: &Config) -> Self {
        // The limit, and two packets more: a push under the limit always fits.
        let frames = config.sample_rate as usize * PENDING_LIMIT_MS as usize / 1000
            + 2 * config.samples_per_frame as usize;
        Self {
            channels: config.channels as usize,
            ring: (0..frames * config.channels as usize)
                .map(|_| AtomicI16::new(0))
                .collect(),
            written: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
            callbacks: AtomicU64::new(0),
            played: AtomicU64::new(0),
            silent: AtomicU64::new(0),
            short: AtomicU64::new(0),
            sounded: AtomicBool::new(false),
            lost: AtomicBool::new(false),
            lost_code: AtomicI32::new(0),
            broken: AtomicBool::new(false),
        }
    }

    pub(crate) fn channels(&self) -> usize {
        self.channels
    }

    /// The writer: all of `pcm`, or nothing when it does not fit.
    fn push(&self, pcm: &[i16]) -> bool {
        let written = self.written.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Acquire);
        let size = self.ring.len();
        if pcm.len() > size - (written - read) {
            return false;
        }
        for (at, &sample) in pcm.iter().enumerate() {
            self.ring[(written + at) % size].store(sample, Ordering::Relaxed);
        }
        self.written.store(written + pcm.len(), Ordering::Release);
        true
    }

    /// Frames in the ring, as the writer sees them.
    fn buffered(&self) -> usize {
        (self.written.load(Ordering::Relaxed) - self.read.load(Ordering::Acquire)) / self.channels
    }

    /// The output's data callback: fill `out` (interleaved frames) with the
    /// PCM that is ready, and silence for the rest. No lock, no wait.
    pub(crate) fn fill(&self, out: &mut [i16]) {
        let read = self.read.load(Ordering::Relaxed);
        let written = self.written.load(Ordering::Acquire);
        let size = self.ring.len();
        let taken = (written - read).min(out.len());
        for (at, sample) in out[..taken].iter_mut().enumerate() {
            *sample = self.ring[(read + at) % size].load(Ordering::Relaxed);
        }
        out[taken..].fill(0);
        self.read.store(read + taken, Ordering::Release);
        let (played, silent) = (taken / self.channels, (out.len() - taken) / self.channels);
        self.callbacks.fetch_add(1, Ordering::Relaxed);
        self.played.fetch_add(played as u64, Ordering::Relaxed);
        self.silent.fetch_add(silent as u64, Ordering::Relaxed);
        // Silence before the first sound is the stream's start, not a gap.
        let sounded = played > 0 || self.sounded.load(Ordering::Relaxed);
        if silent > 0 && sounded {
            self.short.fetch_add(1, Ordering::Relaxed);
        }
        if played > 0 {
            self.sounded.store(true, Ordering::Relaxed);
        }
    }

    /// The output's error callback: the stream is lost. Whatever the code,
    /// AAudio says to stop and close it, from another thread.
    pub(crate) fn lose(&self, code: i32) {
        self.lost_code.store(code, Ordering::Relaxed);
        self.lost.store(true, Ordering::Release);
    }

    fn take_lost(&self) -> Option<i32> {
        self.lost
            .swap(false, Ordering::Acquire)
            .then(|| self.lost_code.load(Ordering::Relaxed))
    }

    /// A callback panicked: the sound ends.
    pub(crate) fn broke(&self) {
        self.broken.store(true, Ordering::Release);
    }
}

/// An error code of the output (`aaudio_result_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AudioError(pub(crate) i32);

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // AAudio.h's names.
        let name = match self.0 {
            -899 => "AAUDIO_ERROR_DISCONNECTED",
            -898 => "AAUDIO_ERROR_ILLEGAL_ARGUMENT",
            -896 => "AAUDIO_ERROR_INTERNAL",
            -895 => "AAUDIO_ERROR_INVALID_STATE",
            -889 => "AAUDIO_ERROR_UNAVAILABLE",
            -887 => "AAUDIO_ERROR_NO_MEMORY",
            -885 => "AAUDIO_ERROR_TIMEOUT",
            -883 => "AAUDIO_ERROR_INVALID_FORMAT",
            -882 => "AAUDIO_ERROR_OUT_OF_RANGE",
            -881 => "AAUDIO_ERROR_NO_SERVICE",
            -880 => "AAUDIO_ERROR_INVALID_RATE",
            _ => "aaudio result",
        };
        write!(f, "{name} ({})", self.0)
    }
}

/// What the player asks of the output for one stream: PCM 16-bit in the
/// core's rate and channel count, for a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    pub(crate) sample_rate: i32,
    pub(crate) channels: i32,
    pub(crate) low_latency: bool,
}

/// What the output gave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Granted {
    pub(crate) low_latency: bool,
    pub(crate) exclusive: bool,
    pub(crate) sample_rate: i32,
    pub(crate) channels: i32,
    pub(crate) burst: i32,
    pub(crate) capacity: i32,
}

/// An audio output: AAudio, or a scripted one in the tests.
pub(crate) trait Output {
    type Stream: Stream;
    /// A stream whose callbacks read `feed`. It is closed when dropped.
    fn open(&self, request: Request, feed: &Arc<Feed>) -> Result<Self::Stream, AudioError>;
}

/// One open stream of an output.
pub(crate) trait Stream: Send {
    fn granted(&self) -> Granted;
    /// The buffer's size in frames as the output set it.
    fn set_buffer_size(&mut self, frames: i32) -> i32;
    fn start(&mut self) -> Result<(), AudioError>;
    fn stop(&mut self);
    /// Underruns and overruns of the device; negative when unknown.
    fn xruns(&self) -> i32;
}

/// Artemis's tries, the low-latency mode first. Its second dimension, a
/// larger buffer when the small one fails, has no AAudio counterpart:
/// AAudio does not refuse a buffer size, it clamps it.
const LOW_LATENCY_TRIES: [bool; 2] = [true, false];

/// Artemis's small buffer is two packets; AAudio's double buffering is two
/// bursts. The larger of them, which AAudio rounds to whole bursts.
fn buffer_size(config: &Config, granted: &Granted) -> i32 {
    (2 * config.samples_per_frame).max(2 * granted.burst)
}

enum Device<S> {
    Open(S),
    /// The stream was lost; a new one opens at `next`.
    Lost {
        next: Instant,
        why: String,
    },
    /// The sound has ended; the stream goes on without it.
    Failed,
}

/// A log line at most once a second, with how often it happened since.
#[derive(Default)]
struct Throttle {
    last: Option<Instant>,
    held: u64,
}

impl Throttle {
    fn note(&mut self, now: Instant, line: impl FnOnce(u64) -> String) {
        self.held += 1;
        if self
            .last
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1))
        {
            log::line(&line(self.held));
            self.held = 0;
            self.last = Some(now);
        }
    }
}

/// What the stream's sound did, for the log and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Summary {
    pub(crate) callbacks: u64,
    pub(crate) played: u64,
    pub(crate) silent: u64,
    pub(crate) short: u64,
    pub(crate) packets: u64,
    pub(crate) concealed: u64,
    pub(crate) dropped: u64,
    pub(crate) decode_errors: u64,
    pub(crate) xruns: i64,
    pub(crate) reopened: u64,
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} callbacks, {} frames played, {} frames silent, {} short callbacks after the first sound, \
             {} packets, {} concealed, {} dropped over {PENDING_LIMIT_MS} ms pending, {} not decoded, \
             {} device xruns, {} reopened",
            self.callbacks,
            self.played,
            self.silent,
            self.short,
            self.packets,
            self.concealed,
            self.dropped,
            self.decode_errors,
            self.xruns,
            self.reopened
        )
    }
}

/// One stream's sound: its decoder, its feed and its output's stream.
pub(crate) struct Player<O: Output> {
    output: O,
    config: Config,
    feed: Arc<Feed>,
    opus: Opus,
    device: Device<O::Stream>,
    health: Health,
    started: bool,
    stopping: bool,
    summary: Summary,
    drops: Throttle,
    errors: Throttle,
}

impl<O: Output> Player<O> {
    /// The decoder and an open stream, or why there is none: the core then
    /// fails the stream's start, as Artemis does when it makes no AudioTrack.
    pub(crate) fn new(output: O, config: Config) -> Result<Self, String> {
        let opus = Opus::new(&config)?;
        let feed = Arc::new(Feed::new(&config));
        let stream = open(&output, &config, &feed)?;
        Ok(Self {
            output,
            config,
            feed,
            opus,
            device: Device::Open(stream),
            health: Health::default(),
            started: false,
            stopping: false,
            summary: Summary::default(),
            drops: Throttle::default(),
            errors: Throttle::default(),
        })
    }

    #[cfg(test)]
    pub(crate) fn feed(&self) -> &Arc<Feed> {
        &self.feed
    }

    /// The core starts the stream: the output's callbacks begin.
    pub(crate) fn start(&mut self, now: Instant) {
        self.started = true;
        if let Device::Open(stream) = &mut self.device {
            match stream.start() {
                Ok(()) => log::line("sound: playing"),
                Err(error) => self.lose(now, format!("the output did not start: {error}")),
            }
        }
    }

    /// The core stops the stream: packets that still come are not played.
    pub(crate) fn stop(&mut self) {
        self.stopping = true;
        if let Device::Open(stream) = &mut self.device {
            stream.stop();
        }
    }

    /// One packet from the core's audio thread; `None` is a lost packet.
    /// `pending_ms` is the sound that the core still holds.
    pub(crate) fn play(&mut self, packet: Option<&[u8]>, pending_ms: i32, now: Instant) {
        if self.stopping {
            return;
        }
        self.recover(now);
        if matches!(self.device, Device::Failed) {
            return;
        }
        self.summary.packets += 1;
        self.summary.concealed += u64::from(packet.is_none());
        // Every packet is decoded, also one that is then dropped: Opus
        // carries state from one packet to the next.
        let pcm = match self.opus.decode(packet) {
            Ok(pcm) => pcm,
            Err(code) => {
                self.summary.decode_errors += 1;
                self.errors.note(now, |n| {
                    format!("sound: Opus error {code}; {n} packets since the last line")
                });
                return;
            }
        };
        let pending = pending_ms.max(0) as u32 + self.config.ms(self.feed.buffered());
        if pending >= PENDING_LIMIT_MS || !self.feed.push(pcm) {
            self.summary.dropped += 1;
            self.drops.note(now, |n| {
                format!("sound: {pending} ms pending; {n} packets dropped since the last line")
            });
        }
    }

    /// A lost stream is closed and opened again; a device that stays lost
    /// ends the sound after the decoder's window (`Health`). Attempts wait
    /// out a back-off: no busy loop.
    fn recover(&mut self, now: Instant) {
        if self.feed.broken.load(Ordering::Acquire) && !matches!(self.device, Device::Failed) {
            self.fail("a callback of the output panicked".into());
            return;
        }
        if matches!(self.device, Device::Open(_)) {
            let Some(code) = self.feed.take_lost() else {
                return;
            };
            self.lose(
                now,
                format!("the output lost the stream: {}", AudioError(code)),
            );
        }
        let Device::Lost { next, why } = &self.device else {
            return;
        };
        if now < *next {
            return;
        }
        let why = why.clone();
        let reopened = open(&self.output, &self.config, &self.feed).and_then(|mut stream| {
            if self.started {
                stream
                    .start()
                    .map_err(|error| format!("the output did not start: {error}"))?;
            }
            Ok(stream)
        });
        match reopened {
            Ok(stream) => {
                self.health.success();
                self.summary.reopened += 1;
                log::line(&format!("sound: the stream opened again after: {why}"));
                self.device = Device::Open(stream);
            }
            Err(error) => match self.health.failure(false, now) {
                Verdict::BackOff(wait) => {
                    self.device = Device::Lost {
                        next: now + wait,
                        why: error,
                    }
                }
                Verdict::End => self.fail(format!("{why}; then: {error}")),
            },
        }
    }

    /// Stop and close the stream, from this thread, never from a callback.
    fn close(&mut self) {
        if let Device::Open(mut stream) = std::mem::replace(&mut self.device, Device::Failed) {
            self.summary.xruns += i64::from(stream.xruns().max(0));
            stream.stop();
            drop(stream);
            log::line("sound: stream closed");
        }
        // The closed stream's callbacks have ended: a flag still set is its own.
        self.feed.take_lost();
    }

    fn lose(&mut self, now: Instant, why: String) {
        self.close();
        log::line(&format!("sound: {why}; the stream opens again"));
        self.device = Device::Lost { next: now, why };
    }

    fn fail(&mut self, why: String) {
        self.close();
        log::line(&format!("sound: the stream goes on without sound: {why}"));
    }

    /// The core's cleanup: the stream closes before the session ends.
    pub(crate) fn finish(mut self) -> Summary {
        self.close();
        let feed = &self.feed;
        let summary = Summary {
            callbacks: feed.callbacks.load(Ordering::Relaxed),
            played: feed.played.load(Ordering::Relaxed),
            silent: feed.silent.load(Ordering::Relaxed),
            short: feed.short.load(Ordering::Relaxed),
            ..self.summary
        };
        log::line(&format!("sound: ended: {summary}"));
        summary
    }
}

/// A stream in the core's format, low latency first.
fn open<O: Output>(output: &O, config: &Config, feed: &Arc<Feed>) -> Result<O::Stream, String> {
    let mut refused = Vec::new();
    for (try_number, low_latency) in LOW_LATENCY_TRIES.into_iter().enumerate() {
        let request = Request {
            sample_rate: config.sample_rate,
            channels: config.channels,
            low_latency,
        };
        let mut stream = match output.open(request, feed) {
            Ok(stream) => stream,
            Err(error) => {
                refused.push(format!("try {try_number}: {error}"));
                continue;
            }
        };
        let granted = stream.granted();
        // The callback reads interleaved frames of the core's channel count.
        if (granted.sample_rate, granted.channels) != (config.sample_rate, config.channels) {
            refused.push(format!(
                "try {try_number}: {} Hz, {} channels",
                granted.sample_rate, granted.channels
            ));
            continue;
        }
        let asked = buffer_size(config, &granted);
        let buffer = stream.set_buffer_size(asked);
        log::line(&format!(
            "sound: output try {try_number} asked {} granted {} sharing {} rate {} channels {} burst {} capacity {} buffer {buffer} (asked {asked}) packet {} frames",
            if low_latency { "low-latency" } else { "none" },
            if granted.low_latency {
                "low-latency"
            } else {
                "none"
            },
            if granted.exclusive {
                "exclusive"
            } else {
                "shared"
            },
            granted.sample_rate,
            granted.channels,
            granted.burst,
            granted.capacity,
            config.samples_per_frame,
        ));
        return Ok(stream);
    }
    Err(format!("no audio stream: {}", refused.join("; ")))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{collections::VecDeque, ffi::c_void, sync::Mutex};

    unsafe extern "C" {
        fn opus_multistream_encoder_create(
            sample_rate: i32,
            channels: c_int,
            streams: c_int,
            coupled_streams: c_int,
            mapping: *const u8,
            application: c_int,
            error: *mut c_int,
        ) -> *mut c_void;
        fn opus_multistream_encode(
            encoder: *mut c_void,
            pcm: *const i16,
            frame_size: c_int,
            data: *mut u8,
            max_bytes: i32,
        ) -> i32;
        fn opus_multistream_encoder_destroy(encoder: *mut c_void);
    }
    const OPUS_APPLICATION_AUDIO: c_int = 2049;

    /// The core's configuration for 2, 6 or 8 channels at 5 ms a packet.
    pub(crate) fn config(channels: i32) -> Config {
        let (streams, coupled_streams) = match channels {
            6 => (4, 2),
            8 => (5, 3),
            _ => (1, 1),
        };
        Config {
            sample_rate: 48000,
            channels,
            streams,
            coupled_streams,
            samples_per_frame: 240,
            mapping: [0, 1, 2, 3, 4, 5, 6, 7],
        }
    }

    /// Real Opus packets of a sine at `hz` on every channel, phase continuous.
    pub(crate) fn tone(config: &Config, hz: f64, packets: usize) -> Vec<Vec<u8>> {
        let mut error = 0;
        // SAFETY: the mapping holds 8 entries.
        let encoder = unsafe {
            opus_multistream_encoder_create(
                config.sample_rate,
                config.channels,
                config.streams,
                config.coupled_streams,
                config.mapping.as_ptr(),
                OPUS_APPLICATION_AUDIO,
                &mut error,
            )
        };
        assert!(!encoder.is_null(), "the encoder: {error}");
        let (frame, channels) = (config.samples_per_frame as usize, config.channels as usize);
        let encoded = (0..packets)
            .map(|packet| {
                let pcm: Vec<i16> = (0..frame * channels)
                    .map(|at| {
                        let t = (packet * frame + at / channels) as f64 / config.sample_rate as f64;
                        (12000.0 * (2.0 * std::f64::consts::PI * hz * t).sin()) as i16
                    })
                    .collect();
                let mut data = vec![0u8; 4000];
                // SAFETY: one packet of PCM in, at most 4000 bytes out.
                let length = unsafe {
                    opus_multistream_encode(
                        encoder,
                        pcm.as_ptr(),
                        frame as c_int,
                        data.as_mut_ptr(),
                        data.len() as i32,
                    )
                };
                assert!(length > 0, "encode: {length}");
                data.truncate(length as usize);
                data
            })
            .collect();
        // SAFETY: ours, no longer used.
        unsafe { opus_multistream_encoder_destroy(encoder) };
        encoded
    }

    /// The tone's frequency in one channel of interleaved PCM, from its
    /// rising zero crossings, and its RMS.
    pub(crate) fn measure(pcm: &[i16], channels: usize, channel: usize, rate: f64) -> (f64, f64) {
        let samples: Vec<f64> = pcm
            .iter()
            .skip(channel)
            .step_by(channels)
            .map(|&s| s as f64)
            .collect();
        let rising = samples
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        let rms = (samples.iter().map(|s| s * s).sum::<f64>() / samples.len() as f64).sqrt();
        (rising as f64 * rate / samples.len() as f64, rms)
    }

    /// An output that answers what its script says and records what it was
    /// asked. Each queue is popped in turn; an empty one gives `then`, and
    /// with no `then` the request is granted as asked.
    #[derive(Clone, Default)]
    struct ScriptedOutput(Arc<Mutex<Script>>);

    #[derive(Default)]
    struct Script {
        opens: VecDeque<Result<Granted, AudioError>>,
        opens_then: Option<Result<Granted, AudioError>>,
        starts: VecDeque<Result<(), AudioError>>,
        xruns: i32,
        // What the output was asked.
        requests: Vec<Request>,
        streams: Vec<Record>,
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct Record {
        buffer: Vec<i32>,
        started: bool,
        stopped: bool,
        closed: bool,
    }

    struct ScriptedStream {
        script: Arc<Mutex<Script>>,
        at: usize,
        granted: Granted,
    }

    fn as_asked(request: Request) -> Granted {
        Granted {
            low_latency: request.low_latency,
            exclusive: false,
            sample_rate: request.sample_rate,
            channels: request.channels,
            burst: 96,
            capacity: 3840,
        }
    }

    impl Output for ScriptedOutput {
        type Stream = ScriptedStream;
        fn open(&self, request: Request, _: &Arc<Feed>) -> Result<ScriptedStream, AudioError> {
            let mut script = self.0.lock().unwrap();
            script.requests.push(request);
            let answer = script
                .opens
                .pop_front()
                .or(script.opens_then)
                .unwrap_or(Ok(as_asked(request)));
            let granted = answer?;
            script.streams.push(Record::default());
            Ok(ScriptedStream {
                script: self.0.clone(),
                at: script.streams.len() - 1,
                granted,
            })
        }
    }

    impl Stream for ScriptedStream {
        fn granted(&self) -> Granted {
            self.granted
        }
        fn set_buffer_size(&mut self, frames: i32) -> i32 {
            self.script.lock().unwrap().streams[self.at]
                .buffer
                .push(frames);
            // As AAudio: whole bursts, no more than the capacity.
            let burst = self.granted.burst;
            ((frames + burst - 1) / burst * burst).min(self.granted.capacity)
        }
        fn start(&mut self) -> Result<(), AudioError> {
            let mut script = self.script.lock().unwrap();
            script.streams[self.at].started = true;
            script.starts.pop_front().unwrap_or(Ok(()))
        }
        fn stop(&mut self) {
            self.script.lock().unwrap().streams[self.at].stopped = true;
        }
        fn xruns(&self) -> i32 {
            self.script.lock().unwrap().xruns
        }
    }

    impl Drop for ScriptedStream {
        fn drop(&mut self) {
            self.script.lock().unwrap().streams[self.at].closed = true;
        }
    }

    fn player(output: &ScriptedOutput, channels: i32) -> Player<ScriptedOutput> {
        Player::new(output.clone(), config(channels)).unwrap()
    }

    /// What the device would pull: `frames` frames, as its callback does.
    fn pull(feed: &Feed, frames: usize) -> Vec<i16> {
        let mut out = vec![1; frames * feed.channels()];
        feed.fill(&mut out);
        out
    }

    const DISCONNECTED: i32 = -899;

    #[test]
    fn real_opus_packets_reach_the_device_as_their_tone() {
        for channels in [2, 6, 8] {
            let output = ScriptedOutput::default();
            let mut player = player(&output, channels);
            let now = Instant::now();
            player.start(now);
            let mut heard = Vec::new();
            // As the stream runs: a packet, then the device takes as much.
            for packet in tone(&config(channels), 1000.0, 40) {
                player.play(Some(&packet), 0, now);
                heard.extend(pull(player.feed(), 240));
            }
            // Past Opus's own start-up delay.
            let tail = &heard[10 * 240 * channels as usize..];
            for channel in 0..channels as usize {
                let (hz, rms) = measure(tail, channels as usize, channel, 48000.0);
                assert!(
                    (hz - 1000.0).abs() < 30.0,
                    "{channels} channels, channel {channel}: {hz} Hz"
                );
                assert!(
                    rms > 3000.0,
                    "{channels} channels, channel {channel}: RMS {rms}"
                );
            }
            let summary = player.finish();
            assert_eq!(
                (summary.packets, summary.dropped, summary.decode_errors),
                (40, 0, 0)
            );
            assert_eq!(summary.played, 40 * 240);
            assert_eq!(summary.short, 0, "{summary}");
            assert!(output.0.lock().unwrap().streams[0].closed);
        }
    }

    #[test]
    fn a_lost_packet_is_concealed_by_opus() {
        let output = ScriptedOutput::default();
        let mut player = player(&output, 2);
        player.start(Instant::now());
        let packets = tone(&config(2), 1000.0, 3);
        player.play(Some(&packets[0]), 0, Instant::now());
        player.play(None, 0, Instant::now());
        assert_eq!(pull(player.feed(), 480).len(), 960);
        let summary = player.finish();
        assert_eq!(
            (summary.packets, summary.concealed, summary.played),
            (2, 1, 480)
        );
    }

    #[test]
    fn the_stream_asks_low_latency_in_the_cores_format_with_a_buffer_of_two_packets() {
        let output = ScriptedOutput::default();
        let player = player(&output, 2);
        {
            let script = output.0.lock().unwrap();
            assert_eq!(
                script.requests,
                [Request {
                    sample_rate: 48000,
                    channels: 2,
                    low_latency: true
                }]
            );
            // Two packets of 240 frames; two bursts of 96 are less.
            assert_eq!(script.streams[0].buffer, [480]);
        }
        player.finish();
    }

    #[test]
    fn a_large_burst_sets_a_buffer_of_two_bursts() {
        let output = ScriptedOutput::default();
        output.0.lock().unwrap().opens_then = Some(Ok(Granted {
            burst: 960,
            ..as_asked(Request {
                sample_rate: 48000,
                channels: 2,
                low_latency: true,
            })
        }));
        player(&output, 2).finish();
        assert_eq!(output.0.lock().unwrap().streams[0].buffer, [1920]);
    }

    #[test]
    fn a_refused_low_latency_stream_falls_back_to_the_standard_mode() {
        let output = ScriptedOutput::default();
        output.0.lock().unwrap().opens = [Err(AudioError(-889))].into();
        player(&output, 2).finish();
        let lows: Vec<bool> = output
            .0
            .lock()
            .unwrap()
            .requests
            .iter()
            .map(|r| r.low_latency)
            .collect();
        assert_eq!(lows, [true, false]);
    }

    #[test]
    fn no_stream_fails_the_start_and_a_wrong_format_is_closed() {
        let output = ScriptedOutput::default();
        let other = Granted {
            sample_rate: 44100,
            ..as_asked(Request {
                sample_rate: 48000,
                channels: 2,
                low_latency: true,
            })
        };
        output.0.lock().unwrap().opens = [Ok(other), Err(AudioError(-889))].into();
        let refused = Player::new(output.clone(), config(2)).err().unwrap();
        assert!(
            refused.contains("44100 Hz") && refused.contains("AAUDIO_ERROR_UNAVAILABLE"),
            "{refused}"
        );
        let script = output.0.lock().unwrap();
        assert!(
            script.streams[0].closed,
            "the stream in another format is closed"
        );
    }

    #[test]
    fn sound_pending_over_40_ms_is_dropped() {
        let output = ScriptedOutput::default();
        let mut player = player(&output, 2);
        player.start(Instant::now());
        // No device pulls: 5 ms packets pile up to the limit, then drop.
        for packet in tone(&config(2), 1000.0, 12) {
            player.play(Some(&packet), 0, Instant::now());
        }
        assert_eq!(player.summary.dropped, 4, "8 packets make 40 ms");
        // The core's own pending sound counts too.
        player.pull_all();
        player.play(Some(&tone(&config(2), 1000.0, 1)[0]), 40, Instant::now());
        assert_eq!(player.summary.dropped, 5);
        player.play(Some(&tone(&config(2), 1000.0, 1)[0]), 35, Instant::now());
        assert_eq!(player.summary.dropped, 5);
        player.finish();
    }

    impl<O: Output> Player<O> {
        fn pull_all(&self) {
            let frames = self.feed.buffered();
            pull(&self.feed, frames);
        }
    }

    #[test]
    fn the_device_gets_silence_when_no_sound_is_ready() {
        let output = ScriptedOutput::default();
        let mut player = player(&output, 2);
        player.start(Instant::now());
        // Before the first packet: silence, and no gap.
        assert!(pull(player.feed(), 96).iter().all(|&s| s == 0));
        let packet = &tone(&config(2), 1000.0, 1)[0];
        player.play(Some(packet), 0, Instant::now());
        // 240 frames ready, 336 asked: the rest is silence, one short callback.
        let heard = pull(player.feed(), 336);
        assert!(heard[..480].iter().any(|&s| s != 0));
        assert!(heard[480..].iter().all(|&s| s == 0));
        let summary = player.finish();
        assert_eq!(
            (
                summary.callbacks,
                summary.played,
                summary.silent,
                summary.short
            ),
            (2, 240, 192, 1)
        );
    }

    #[test]
    fn a_lost_stream_is_closed_and_a_new_one_starts() {
        let output = ScriptedOutput::default();
        let mut player = player(&output, 2);
        let now = Instant::now();
        player.start(now);
        player.feed().lose(DISCONNECTED);
        player.play(Some(&tone(&config(2), 1000.0, 1)[0]), 0, now);
        {
            let script = output.0.lock().unwrap();
            assert_eq!(script.streams.len(), 2);
            assert!(script.streams[0].stopped && script.streams[0].closed);
            assert!(script.streams[1].started && !script.streams[1].closed);
        }
        let summary = player.finish();
        assert_eq!((summary.reopened, summary.packets), (1, 1));
        assert!(output.0.lock().unwrap().streams[1].closed);
    }

    #[test]
    fn a_device_that_stays_lost_ends_the_sound_without_a_busy_loop() {
        let output = ScriptedOutput::default();
        let mut player = player(&output, 2);
        let start = Instant::now();
        player.start(start);
        output.0.lock().unwrap().opens_then = Some(Err(AudioError(-889)));
        player.feed().lose(DISCONNECTED);
        let packet = &tone(&config(2), 1000.0, 1)[0];
        // A packet every millisecond for 5 s of the stream's time.
        for ms in 0..5000 {
            player.play(Some(packet), 0, start + Duration::from_millis(ms));
        }
        let attempts = output.0.lock().unwrap().requests.len() - 1;
        // Each attempt makes two tries; they back off from 10 ms to 160 ms.
        assert!(attempts <= 2 * 30, "{attempts} tries in 5 s");
        assert!(matches!(player.device, Device::Failed));
        let packets = player.summary.packets;
        player.play(Some(packet), 0, start + Duration::from_secs(6));
        assert_eq!(
            player.summary.packets, packets,
            "no decode once the sound ended"
        );
        assert_eq!(output.0.lock().unwrap().requests.len() - 1, attempts);
        player.finish();
    }

    #[test]
    fn a_stream_that_does_not_start_is_opened_again() {
        let output = ScriptedOutput::default();
        output.0.lock().unwrap().starts = [Err(AudioError(-895))].into();
        let mut player = player(&output, 2);
        let now = Instant::now();
        player.start(now);
        player.play(Some(&tone(&config(2), 1000.0, 1)[0]), 0, now);
        let script = output.0.lock().unwrap();
        assert_eq!(script.streams.len(), 2);
        assert!(script.streams[0].closed && script.streams[1].started);
    }

    #[test]
    fn a_panic_in_a_callback_ends_the_sound() {
        let output = ScriptedOutput::default();
        let mut player = player(&output, 2);
        player.start(Instant::now());
        player.feed().broke();
        player.play(Some(&tone(&config(2), 1000.0, 1)[0]), 0, Instant::now());
        assert!(matches!(player.device, Device::Failed));
        assert!(output.0.lock().unwrap().streams[0].closed);
        assert_eq!(player.finish().packets, 0);
    }

    #[test]
    fn after_stop_packets_are_not_played_and_cleanup_closes_the_stream() {
        let output = ScriptedOutput::default();
        let mut player = player(&output, 2);
        player.start(Instant::now());
        player.stop();
        player.feed().lose(DISCONNECTED);
        player.play(Some(&tone(&config(2), 1000.0, 1)[0]), 0, Instant::now());
        {
            let script = output.0.lock().unwrap();
            assert!(script.streams[0].stopped && !script.streams[0].closed);
            assert_eq!(script.streams.len(), 1, "no new stream after the stop");
        }
        output.0.lock().unwrap().xruns = 3;
        let summary = player.finish();
        assert_eq!((summary.packets, summary.xruns), (0, 3));
        assert!(output.0.lock().unwrap().streams[0].closed);
    }

    #[test]
    fn the_ring_keeps_every_frame_in_order_across_two_threads() {
        let config = Config {
            channels: 2,
            ..config(2)
        };
        let feed = Arc::new(Feed::new(&config));
        // Whole batches of 60 frames.
        let frames = 60 * 300;
        let writer = {
            let feed = feed.clone();
            std::thread::spawn(move || {
                let mut next = 0;
                while next < frames {
                    let pcm: Vec<i16> = (next..next + 60)
                        .flat_map(|frame| [frame as i16, !(frame as i16)])
                        .collect();
                    if feed.push(&pcm) {
                        next += 60;
                    } else {
                        std::thread::yield_now();
                    }
                }
            })
        };
        let mut heard = Vec::new();
        let until = Instant::now() + Duration::from_secs(20);
        while heard.len() < frames && Instant::now() < until {
            let mut out = vec![0i16; 2 * 37];
            feed.fill(&mut out);
            heard.extend(out.chunks(2).filter(|f| *f != [0, 0]).map(|f| (f[0], f[1])));
        }
        writer.join().unwrap();
        // Frame 0 is [0, -1], never silence; every frame comes once, in order.
        assert_eq!(heard.len(), frames);
        for (at, (left, right)) in heard.into_iter().enumerate() {
            assert_eq!((left, right), (at as i16, !(at as i16)));
        }
    }
}
