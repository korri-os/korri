// SPDX-License-Identifier: GPL-3.0-only
//! One stream's sound on Android (#391 step D, slice D3): the core's audio
//! callbacks, and AAudio as the player's output (`audio.rs`). AAudio is
//! Android's own C audio API (`libaaudio.so`, API 26); at API 31 it needs no
//! fallback, so no Oboe.
//!
//! The stream asks AAudio for its low-latency mode with a data callback, the
//! way AAudio's guide gives the lowest latency. The callbacks here are Rust
//! `extern "C"` functions; each body runs under `catch_unwind`
//! (`callback::guarded`), so no panic reaches C. A panic in the data callback
//! plays silence, stops that stream and ends the sound; the picture goes on.
//!
//! The network core's audio callbacks carry no context, so one stream's
//! sound is process-wide, as the core itself is. They follow `callbacks.c`
//! (`BridgeAr*`) of Korri's copy of Artemis, as `audio.rs` says; that is why
//! this file is GPL-3.0-only. The Rust adaptation is Korri work dated
//! 2026-11-14.
use crate::{
    audio::{AudioError, Config, Feed, Granted, Output, Player, Request, Stream},
    callback::guarded,
    log,
    network::{self as ffi, AUDIO_RENDERER_CALLBACKS, OPUS_MULTISTREAM_CONFIGURATION},
};
use ndk_sys as aaudio;
use std::{
    ffi::{c_char, c_int, c_void},
    ptr::{NonNull, null_mut},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Instant,
};

static PLAYER: Mutex<Option<Player<Aaudio>>> = Mutex::new(None);

/// The stream's sound. A panic that a callback caught leaves it usable.
fn player() -> MutexGuard<'static, Option<Player<Aaudio>>> {
    PLAYER.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The core's audio callbacks. The core decodes on its own audio thread, as
/// it does for Artemis (no `CAPABILITY_DIRECT_SUBMIT`), so its pending sound
/// counts in the 40 ms limit.
pub(crate) fn callbacks() -> *mut AUDIO_RENDERER_CALLBACKS {
    // The core reads the table during LiStartConnection and keeps the pointer.
    static mut CALLBACKS: AUDIO_RENDERER_CALLBACKS = AUDIO_RENDERER_CALLBACKS {
        init: Some(init),
        start: Some(start),
        stop: Some(stop),
        cleanup: Some(cleanup),
        decodeAndPlaySample: Some(decode_and_play),
        capabilities: ffi::CAPABILITY_SUPPORTS_ARBITRARY_AUDIO_DURATION as c_int,
    };
    &raw mut CALLBACKS
}

/// After the core has stopped: close a stream that its cleanup did not.
pub(crate) fn close() {
    let left = player().take();
    if let Some(player) = left {
        player.finish();
    }
}

unsafe extern "C" fn init(
    _: c_int,
    config: *mut OPUS_MULTISTREAM_CONFIGURATION,
    _: *mut c_void,
    _: c_int,
) -> c_int {
    guarded("sound init", -1, || {
        // SAFETY: the core's configuration, live for this call.
        let Some(config) = (unsafe { config.as_ref() }) else {
            return -1;
        };
        let config = Config {
            sample_rate: config.sampleRate,
            channels: config.channelCount,
            streams: config.streams,
            coupled_streams: config.coupledStreams,
            samples_per_frame: config.samplesPerFrame,
            mapping: config.mapping,
        };
        match Player::new(Aaudio, config) {
            Ok(new) => {
                let old = player().replace(new);
                if let Some(old) = old {
                    old.finish();
                }
                0
            }
            Err(error) => {
                // Artemis's answer when it makes no AudioTrack: the core
                // then fails the stream's start.
                log::line(&format!("sound: no output: {error}"));
                -2
            }
        }
    })
}

unsafe extern "C" fn start() {
    guarded("sound start", (), || {
        if let Some(player) = player().as_mut() {
            player.start(Instant::now());
        }
    });
}

unsafe extern "C" fn stop() {
    guarded("sound stop", (), || {
        if let Some(player) = player().as_mut() {
            player.stop();
        }
    });
}

unsafe extern "C" fn cleanup() {
    guarded("sound cleanup", (), close);
}

unsafe extern "C" fn decode_and_play(data: *mut c_char, length: c_int) {
    guarded("sound decode", (), || {
        // NULL is a packet that the network lost: Opus conceals it.
        let packet = (!data.is_null() && length > 0).then(|| {
            // SAFETY: the core's packet of `length` bytes, live for this call.
            unsafe { std::slice::from_raw_parts(data.cast::<u8>(), length as usize) }
        });
        let pending = crate::network_core::active().pending_audio_ms();
        if let Some(player) = player().as_mut() {
            player.play(packet, pending, Instant::now());
        }
    });
}

/// AAudio: one output stream at a time, PCM 16-bit, for a game.
pub(crate) struct Aaudio;

pub(crate) struct AaudioStream {
    stream: NonNull<aaudio::AAudioStream>,
    /// The callbacks read the feed until the stream is closed.
    _feed: Arc<Feed>,
}

// SAFETY: the stream is used by one thread at a time, behind the player;
// AAudio's own callback threads only read the feed.
unsafe impl Send for AaudioStream {}

fn checked(result: aaudio::aaudio_result_t) -> Result<(), AudioError> {
    if result == aaudio::AAUDIO_OK {
        Ok(())
    } else {
        Err(AudioError(result))
    }
}

struct Builder(*mut aaudio::AAudioStreamBuilder);

impl Drop for Builder {
    fn drop(&mut self) {
        // SAFETY: a live builder, ours.
        unsafe { aaudio::AAudioStreamBuilder_delete(self.0) };
    }
}

impl Output for Aaudio {
    type Stream = AaudioStream;

    fn open(&self, request: Request, feed: &Arc<Feed>) -> Result<AaudioStream, AudioError> {
        let mut builder = null_mut();
        // SAFETY: AAudio writes the builder.
        checked(unsafe { aaudio::AAudio_createStreamBuilder(&mut builder) })?;
        let builder = Builder(builder);
        let b = builder.0;
        let user = Arc::as_ptr(feed).cast_mut().cast::<c_void>();
        let mode = if request.low_latency {
            aaudio::AAUDIO_PERFORMANCE_MODE_LOW_LATENCY
        } else {
            aaudio::AAUDIO_PERFORMANCE_MODE_NONE
        };
        // SAFETY: a live builder. The feed outlives the stream: the stream
        // holds it until AAudioStream_close returns.
        unsafe {
            aaudio::AAudioStreamBuilder_setDirection(b, aaudio::AAUDIO_DIRECTION_OUTPUT as i32);
            aaudio::AAudioStreamBuilder_setSharingMode(
                b,
                aaudio::AAUDIO_SHARING_MODE_SHARED as i32,
            );
            aaudio::AAudioStreamBuilder_setPerformanceMode(b, mode as i32);
            aaudio::AAudioStreamBuilder_setFormat(b, aaudio::AAUDIO_FORMAT_PCM_I16);
            aaudio::AAudioStreamBuilder_setChannelCount(b, request.channels);
            aaudio::AAudioStreamBuilder_setSampleRate(b, request.sample_rate);
            // Artemis's AudioTrack has USAGE_GAME.
            aaudio::AAudioStreamBuilder_setUsage(b, aaudio::AAUDIO_USAGE_GAME as i32);
            aaudio::AAudioStreamBuilder_setDataCallback(b, Some(data), user);
            aaudio::AAudioStreamBuilder_setErrorCallback(b, Some(error), user);
        }
        let mut stream = null_mut();
        // SAFETY: a live builder; AAudio writes the stream.
        checked(unsafe { aaudio::AAudioStreamBuilder_openStream(b, &mut stream) })?;
        let stream = NonNull::new(stream).ok_or(AudioError(aaudio::AAUDIO_ERROR_NULL))?;
        Ok(AaudioStream {
            stream,
            _feed: feed.clone(),
        })
    }
}

impl Stream for AaudioStream {
    fn granted(&self) -> Granted {
        let s = self.stream.as_ptr();
        // SAFETY: a live stream; AAudio's getters are thread safe.
        unsafe {
            Granted {
                low_latency: aaudio::AAudioStream_getPerformanceMode(s)
                    == aaudio::AAUDIO_PERFORMANCE_MODE_LOW_LATENCY as i32,
                exclusive: aaudio::AAudioStream_getSharingMode(s)
                    == aaudio::AAUDIO_SHARING_MODE_EXCLUSIVE as i32,
                sample_rate: aaudio::AAudioStream_getSampleRate(s),
                // The callback reads frames of this many samples, in I16.
                channels: if aaudio::AAudioStream_getFormat(s) == aaudio::AAUDIO_FORMAT_PCM_I16 {
                    aaudio::AAudioStream_getChannelCount(s)
                } else {
                    0
                },
                burst: aaudio::AAudioStream_getFramesPerBurst(s),
                capacity: aaudio::AAudioStream_getBufferCapacityInFrames(s),
            }
        }
    }

    fn set_buffer_size(&mut self, frames: i32) -> i32 {
        // SAFETY: a live stream, not yet started.
        let set =
            unsafe { aaudio::AAudioStream_setBufferSizeInFrames(self.stream.as_ptr(), frames) };
        if set < 0 {
            // SAFETY: as above.
            unsafe { aaudio::AAudioStream_getBufferSizeInFrames(self.stream.as_ptr()) }
        } else {
            set
        }
    }

    fn start(&mut self) -> Result<(), AudioError> {
        // SAFETY: a live stream.
        checked(unsafe { aaudio::AAudioStream_requestStart(self.stream.as_ptr()) })
    }

    fn stop(&mut self) {
        // SAFETY: a live stream. A stop of a stopped or lost stream fails
        // harmlessly.
        unsafe { aaudio::AAudioStream_requestStop(self.stream.as_ptr()) };
    }

    fn xruns(&self) -> i32 {
        // SAFETY: a live stream.
        unsafe { aaudio::AAudioStream_getXRunCount(self.stream.as_ptr()) }
    }
}

impl Drop for AaudioStream {
    fn drop(&mut self) {
        // SAFETY: the player's own stream, closed once, from its thread and
        // never from a callback. AAudio returns once the callbacks are done.
        unsafe { aaudio::AAudioStream_close(self.stream.as_ptr()) };
    }
}

/// AAudio's data callback, on its high-priority thread: the feed's PCM.
unsafe extern "C" fn data(
    _: *mut aaudio::AAudioStream,
    user: *mut c_void,
    audio: *mut c_void,
    frames: i32,
) -> aaudio::aaudio_data_callback_result_t {
    // SAFETY: the stream's feed, which the stream holds until it is closed.
    let feed = unsafe { &*user.cast::<Feed>() };
    let samples = frames.max(0) as usize * feed.channels();
    // SAFETY: AAudio gives `frames` frames of the stream's format: I16 with
    // the feed's channel count (`open` refuses any other).
    let out = unsafe { std::slice::from_raw_parts_mut(audio.cast::<i16>(), samples) };
    if guarded("sound data", false, || {
        feed.fill(out);
        true
    }) {
        aaudio::AAUDIO_CALLBACK_RESULT_CONTINUE as i32
    } else {
        out.fill(0);
        feed.broke();
        aaudio::AAUDIO_CALLBACK_RESULT_STOP as i32
    }
}

/// AAudio's error callback, on a thread of its own: the stream is lost. The
/// core's audio thread closes it and opens another.
unsafe extern "C" fn error(
    _: *mut aaudio::AAudioStream,
    user: *mut c_void,
    code: aaudio::aaudio_result_t,
) {
    guarded("sound error", (), || {
        // SAFETY: as in `data`.
        let feed = unsafe { &*user.cast::<Feed>() };
        feed.lose(code);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::tests::{config, tone};
    use std::time::Duration;

    /// Real Opus packets through real AAudio, on the device or emulator that
    /// runs the test: a 440 Hz tone for two seconds, paced as the network
    /// would. The proof's capture of the emulator's sound hears it.
    #[test]
    fn real_opus_plays_through_aaudio() {
        let config = config(2);
        let mut player = Player::new(Aaudio, config).expect("an AAudio stream");
        player.start(Instant::now());
        let started = Instant::now();
        for (at, packet) in tone(&config, 440.0, 400).iter().enumerate() {
            // 5 ms a packet, as Sunshine sends them.
            let due = started + Duration::from_millis(5 * at as u64);
            std::thread::sleep(due.saturating_duration_since(Instant::now()));
            player.play(Some(packet), 0, Instant::now());
        }
        std::thread::sleep(Duration::from_millis(200));
        let summary = player.finish();
        log::line(&format!("sound test: {summary}"));
        assert!(summary.callbacks > 0, "{summary}");
        // Most of the two seconds went to the device.
        assert!(summary.played > 48000, "{summary}");
        assert_eq!(summary.decode_errors, 0, "{summary}");
    }
}
