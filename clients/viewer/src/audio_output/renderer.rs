// SPDX-License-Identifier: GPL-3.0-or-later
//! The Rust port of the carried C audio renderers (#391 step F, slice F1).
//!
//! Ported from Moonlight Embedded's `src/audio/{alsa,pulse,sdl}.c`
//! (Copyright (C) 2015-2017 Iwan Timmer, GPL-3.0-or-later), carried
//! unchanged in `io/audio/`, and from the audio choice in `io/bridge.c`
//! (Moonlight's `platform.c`). The Rust adaptation is Korri work dated
//! 2026-11-17.
//!
//! What stays as in C, so that the measure judges the language alone:
//! - the choice: SDL audio on the SDL platform (and `v4l2request`); on X11
//!   VA-API, PulseAudio when a probe stream opens on the device, else ALSA;
//! - libopus decodes each packet on the core's thread
//!   (`CAPABILITY_DIRECT_SUBMIT`), and a lost packet is decoded as NULL,
//!   which Opus conceals;
//! - ALSA: the device or `sysdefault`, non-blocking, a 20 ms period and a
//!   60 ms buffer, start and wake at one period, one recovery and one retry
//!   of a failed write; PulseAudio: the simple API with its default buffer
//!   and ALSA's channel map; SDL: a queue with a 4096-frame device buffer;
//! - the decoder's mapping in ALSA's channel order for ALSA and PulseAudio.
//!
//! What differs: the decoder's configuration is checked, and a decoder that
//! libopus refuses fails the start (C went on with none); the log lines
//! start with `sound:` and go to standard error.
use super::ffi;
use crate::{
    callback::guarded,
    log,
    network::{self, AUDIO_RENDERER_CALLBACKS, OPUS_MULTISTREAM_CONFIGURATION},
};
use std::{
    ffi::{CStr, c_char, c_int, c_void},
    fmt,
    ptr::{NonNull, null},
    sync::{Mutex, MutexGuard, PoisonError},
};

/// The output that plays, as `viewer_audio_callbacks` chooses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Output {
    Sdl,
    Pulse,
    Alsa,
}

impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Sdl => "SDL audio",
            Self::Pulse => "PulseAudio",
            Self::Alsa => "ALSA",
        })
    }
}

/// `io/bridge.c`'s `viewer_audio_callbacks`, from Moonlight's `platform.c`.
pub(crate) fn choose(vaapi: bool, device: Option<&CStr>) -> Output {
    if !vaapi {
        Output::Sdl
    } else if pulse_probe(device) {
        Output::Pulse
    } else {
        Output::Alsa
    }
}

/// The table of the output that `choose` gives. The core keeps the table
/// from its start to its stop, and gives `device` back as each `init`'s
/// context.
pub(crate) fn callbacks(vaapi: bool, device: Option<&CStr>) -> *mut AUDIO_RENDERER_CALLBACKS {
    let output = choose(vaapi, device);
    log::line(&format!("sound: the Rust audio output chose {output}"));
    table(output)
}

/// The capabilities of all three C tables.
const CAPABILITIES: c_int = (network::CAPABILITY_DIRECT_SUBMIT
    | network::CAPABILITY_SUPPORTS_ARBITRARY_AUDIO_DURATION) as c_int;

type Init =
    unsafe extern "C" fn(c_int, *mut OPUS_MULTISTREAM_CONFIGURATION, *mut c_void, c_int) -> c_int;

const fn table_for(init: Init) -> AUDIO_RENDERER_CALLBACKS {
    AUDIO_RENDERER_CALLBACKS {
        init: Some(init),
        start: None,
        stop: None,
        cleanup: Some(cleanup),
        decodeAndPlaySample: Some(decode_and_play),
        capabilities: CAPABILITIES,
    }
}

fn table(output: Output) -> *mut AUDIO_RENDERER_CALLBACKS {
    // The core reads a table during its start and keeps the pointer.
    static mut SDL: AUDIO_RENDERER_CALLBACKS = table_for(init_sdl);
    static mut PULSE: AUDIO_RENDERER_CALLBACKS = table_for(init_pulse);
    static mut ALSA: AUDIO_RENDERER_CALLBACKS = table_for(init_alsa);
    match output {
        Output::Sdl => &raw mut SDL,
        Output::Pulse => &raw mut PULSE,
        Output::Alsa => &raw mut ALSA,
    }
}

/// The stream's renderer, between the core's `init` and `cleanup`. The core
/// calls them and each packet on its own threads, one at a time.
static RENDERER: Mutex<Option<Renderer>> = Mutex::new(None);

fn renderer() -> MutexGuard<'static, Option<Renderer>> {
    RENDERER.lock().unwrap_or_else(PoisonError::into_inner)
}

unsafe extern "C" fn init_sdl(
    _: c_int,
    config: *mut OPUS_MULTISTREAM_CONFIGURATION,
    _: *mut c_void,
    _: c_int,
) -> c_int {
    // sdl.c ignores the device: Moonlight refuses one on its SDL platform,
    // and v4l2request accepts it and plays on SDL's default.
    // SAFETY: the core's configuration, live for this call.
    unsafe { init(Output::Sdl, config, None) }
}

unsafe extern "C" fn init_pulse(
    _: c_int,
    config: *mut OPUS_MULTISTREAM_CONFIGURATION,
    context: *mut c_void,
    _: c_int,
) -> c_int {
    // SAFETY: as in `init_sdl`; the context is the device's C string or NULL.
    unsafe { init(Output::Pulse, config, device(context)) }
}

unsafe extern "C" fn init_alsa(
    _: c_int,
    config: *mut OPUS_MULTISTREAM_CONFIGURATION,
    context: *mut c_void,
    _: c_int,
) -> c_int {
    // SAFETY: as in `init_pulse`.
    unsafe { init(Output::Alsa, config, device(context)) }
}

/// The context that `PlayerIo` gives the core: `-audio`'s device, or NULL.
///
/// # Safety
/// `context` is NULL or a C string that outlives the stream.
unsafe fn device<'a>(context: *mut c_void) -> Option<&'a CStr> {
    // SAFETY: the caller's promise.
    (!context.is_null()).then(|| unsafe { CStr::from_ptr(context.cast::<c_char>()) })
}

/// # Safety
/// `config` is NULL or the core's configuration, live for this call.
unsafe fn init(
    output: Output,
    config: *mut OPUS_MULTISTREAM_CONFIGURATION,
    device: Option<&CStr>,
) -> c_int {
    guarded("sound init", -1, || {
        // SAFETY: the caller's promise.
        let Some(config) = (unsafe { config.as_ref() }) else {
            return -1;
        };
        match Renderer::open(output, config, device) {
            Ok(new) => {
                // A renderer that no cleanup closed closes now.
                let old = renderer().replace(new);
                drop(old);
                0
            }
            Err(error) => {
                log::line(&format!("sound: {output} did not open: {error}"));
                -1
            }
        }
    })
}

unsafe extern "C" fn cleanup() {
    guarded("sound cleanup", (), || drop(renderer().take()));
}

unsafe extern "C" fn decode_and_play(data: *mut c_char, length: c_int) {
    guarded("sound decode", (), || {
        if let Some(renderer) = renderer().as_mut() {
            // SAFETY: the core's packet of `length` bytes, or NULL for a lost
            // one, live for this call.
            unsafe { renderer.play(data.cast_const().cast(), length) };
        }
    });
}

/// The core's channel order is FL FR C LFE RL RR SL SR. ALSA's, and
/// PulseAudio's ALSA map, is FL FR RL RR C LFE SL SR: alsa.c and pulse.c
/// give the decoder a mapping in that order for 5.1 and 7.1.
fn alsa_order(mapping: [u8; 8], channels: c_int) -> [u8; 8] {
    let mut ordered = mapping;
    if channels >= 6 {
        ordered[2] = mapping[4];
        ordered[3] = mapping[5];
        ordered[4] = mapping[2];
        ordered[5] = mapping[3];
    }
    ordered
}

/// libopus's multistream decoder.
struct Opus(NonNull<ffi::OpusMSDecoder>);

// SAFETY: the decoder is used by one thread at a time, behind `RENDERER`.
unsafe impl Send for Opus {}

impl Opus {
    fn new(config: &OPUS_MULTISTREAM_CONFIGURATION, mapping: &[u8; 8]) -> Result<Self, String> {
        let mut error = 0;
        // SAFETY: the mapping holds 8 entries; the caller checked that the
        // channels are at most 8.
        let decoder = unsafe {
            ffi::opus_multistream_decoder_create(
                config.sampleRate,
                config.channelCount,
                config.streams,
                config.coupledStreams,
                mapping.as_ptr(),
                &mut error,
            )
        };
        NonNull::new(decoder)
            .map(Self)
            .ok_or_else(|| format!("Opus refused the stream's configuration ({error})"))
    }
}

impl Drop for Opus {
    fn drop(&mut self) {
        // SAFETY: ours, no longer used.
        unsafe { ffi::opus_multistream_decoder_destroy(self.0.as_ptr()) };
    }
}

/// One stream's decoder, its PCM buffer of one packet, and its output.
/// The fields drop in order: the decoder, then the device, as in C.
struct Renderer {
    decoder: Opus,
    pcm: Vec<i16>,
    channels: usize,
    frames: c_int,
    sink: Sink,
}

enum Sink {
    Sdl(Sdl),
    Pulse(Pulse),
    Alsa(Alsa),
}

impl Renderer {
    fn open(
        output: Output,
        config: &OPUS_MULTISTREAM_CONFIGURATION,
        device: Option<&CStr>,
    ) -> Result<Self, String> {
        // The core's limits: at most 8 channels, and an Opus frame lasts at
        // most 120 ms.
        let fits = (1..=8).contains(&config.channelCount)
            && config.sampleRate > 0
            && (1..=config.sampleRate / 1000 * 120).contains(&config.samplesPerFrame);
        if !fits {
            return Err(format!(
                "an Opus configuration of {} channels, {} Hz and {} frames a packet",
                config.channelCount, config.sampleRate, config.samplesPerFrame
            ));
        }
        let mapping = match output {
            Output::Sdl => config.mapping,
            Output::Pulse | Output::Alsa => alsa_order(config.mapping, config.channelCount),
        };
        let decoder = Opus::new(config, &mapping)?;
        let (rate, channels) = (config.sampleRate as u32, config.channelCount as u8);
        let sink = match output {
            Output::Sdl => Sink::Sdl(Sdl::open(rate, channels)?),
            Output::Pulse => Sink::Pulse(Pulse::open(device, rate, channels)?),
            Output::Alsa => Sink::Alsa(Alsa::open(device, rate, channels)?),
        };
        let channels = usize::from(channels);
        Ok(Self {
            decoder,
            pcm: vec![0; channels * config.samplesPerFrame as usize],
            channels,
            frames: config.samplesPerFrame,
            sink,
        })
    }

    /// # Safety
    /// `data` is NULL, or `length` readable bytes.
    unsafe fn play(&mut self, data: *const u8, length: c_int) {
        // SAFETY: the caller's packet; the buffer holds `frames` frames.
        let decoded = unsafe {
            ffi::opus_multistream_decode(
                self.decoder.0.as_ptr(),
                data,
                length,
                self.pcm.as_mut_ptr(),
                self.frames,
                0,
            )
        };
        if decoded < 0 {
            log::line(&format!("sound: Opus error from decode: {decoded}"));
            return;
        }
        let frames = decoded as usize;
        let pcm = &self.pcm[..frames * self.channels];
        match &mut self.sink {
            Sink::Sdl(sdl) => sdl.play(pcm),
            Sink::Pulse(pulse) => pulse.play(pcm),
            Sink::Alsa(alsa) => alsa.play(pcm, frames),
        }
    }
}

/// The PCM as the bytes that PulseAudio and SDL take.
fn bytes(pcm: &[i16]) -> &[u8] {
    // SAFETY: any i16 is two initialized bytes; the borrow stays the same.
    unsafe { std::slice::from_raw_parts(pcm.as_ptr().cast::<u8>(), size_of_val(pcm)) }
}

/// alsa.c: interleaved S16_LE writes into a non-blocking PCM.
struct Alsa {
    pcm: alsa::PCM,
}

impl Alsa {
    fn open(device: Option<&CStr>, rate: u32, channels: u8) -> Result<Self, String> {
        use alsa::{
            Direction, ValueOr,
            pcm::{Access, Format, HwParams},
        };
        let name = device.unwrap_or(c"sysdefault");
        let error = |error: alsa::Error| format!("{}: {error}", name.to_string_lossy());
        let pcm = alsa::PCM::open(name, Direction::Playback, true).map_err(error)?;
        let period = alsa::pcm::Frames::from(rate * 20 / 1000);
        let buffer = 3 * period;
        let period = (|| -> alsa::Result<alsa::pcm::Frames> {
            let hw = HwParams::any(&pcm)?;
            hw.set_access(Access::RWInterleaved)?;
            hw.set_format(Format::S16LE)?;
            hw.set_rate_near(rate, ValueOr::Nearest)?;
            hw.set_channels(u32::from(channels))?;
            // As in alsa.c, the granted period sets the wake and the start.
            let period = hw.set_period_size_near(period, ValueOr::Nearest)?;
            hw.set_buffer_size_near(buffer)?;
            pcm.hw_params(&hw)?;
            let sw = pcm.sw_params_current()?;
            sw.set_avail_min(period)?;
            sw.set_start_threshold(period)?;
            pcm.sw_params(&sw)?;
            pcm.prepare()?;
            // The writes go unchecked (`play`): check the format once.
            drop(pcm.io_i16()?);
            Ok(period)
        })()
        .map_err(error)?;
        log::line(&format!(
            "sound: ALSA {}: {channels} channels at {rate} Hz, period {period} frames, buffer {buffer} asked",
            name.to_string_lossy()
        ));
        Ok(Self { pcm })
    }

    fn play(&mut self, pcm: &[i16], frames: usize) {
        // SAFETY: `open` checked that the PCM takes i16 samples, and this is
        // its only IO object.
        let io = unsafe { self.pcm.io_unchecked::<i16>() };
        let written = io.writei(pcm).or_else(|error| {
            self.pcm
                .try_recover(error, false)
                .and_then(|()| io.writei(pcm))
        });
        match written {
            Err(error) => log::line(&format!("sound: ALSA error from writei: {error}")),
            Ok(written) if written != frames => {
                log::line(&format!("sound: ALSA short write, {written} frames"))
            }
            Ok(_) => {}
        }
    }
}

impl Drop for Alsa {
    fn drop(&mut self) {
        // alsa.c drains, then closes; non-blocking, the drain does not wait.
        let _ = self.pcm.drain();
    }
}

/// PulseAudio's names for the stream, as pulse.c gives them.
const PULSE_APPLICATION: &str = "Korri viewer";
const PULSE_STREAM: &str = "Streaming";

/// pulse.c's `audio_pulse_init`: a stereo stream at 48 kHz opens on the
/// device, or there is no PulseAudio.
fn pulse_probe(device: Option<&CStr>) -> bool {
    use libpulse_binding::{
        sample::{Format, Spec},
        stream::Direction,
    };
    let Ok(device) = device.map(CStr::to_str).transpose() else {
        return false;
    };
    let spec = Spec {
        format: Format::S16le,
        rate: 48000,
        channels: 2,
    };
    libpulse_simple_binding::Simple::new(
        None,
        PULSE_APPLICATION,
        Direction::Playback,
        device,
        PULSE_STREAM,
        &spec,
        None,
        None,
    )
    .is_ok()
}

/// pulse.c: blocking writes into a simple playback stream.
struct Pulse {
    stream: libpulse_simple_binding::Simple,
}

impl Pulse {
    fn open(device: Option<&CStr>, rate: u32, channels: u8) -> Result<Self, String> {
        use libpulse_binding::{
            channelmap::{Map, MapDef},
            sample::{Format, Spec},
            stream::Direction,
        };
        let device = device
            .map(CStr::to_str)
            .transpose()
            .map_err(|_| "the device name is not UTF-8".to_owned())?;
        let spec = Spec {
            format: Format::S16le,
            rate,
            channels,
        };
        let mut map = Map::default();
        map.init_auto(channels, MapDef::ALSA)
            .ok_or_else(|| format!("PulseAudio has no ALSA map of {channels} channels"))?;
        let stream = libpulse_simple_binding::Simple::new(
            None,
            PULSE_APPLICATION,
            Direction::Playback,
            device,
            PULSE_STREAM,
            &spec,
            Some(&map),
            None,
        )
        .map_err(|error| format!("Pulseaudio error: {error}"))?;
        log::line(&format!(
            "sound: PulseAudio {}: {channels} channels at {rate} Hz",
            device.unwrap_or("default")
        ));
        Ok(Self { stream })
    }

    fn play(&mut self, pcm: &[i16]) {
        if let Err(error) = self.stream.write(bytes(pcm)) {
            log::line(&format!("sound: Pulseaudio error: {error}"));
        }
    }
}

/// sdl.c: the default device, fed by SDL's queue.
struct Sdl {
    device: ffi::SDL_AudioDeviceID,
}

impl Sdl {
    fn open(rate: u32, channels: u8) -> Result<Self, String> {
        // SAFETY: SDL's own calls, with specs that live for the call. As in
        // sdl.c, the audio subsystem stays initialized after the stream.
        let device = unsafe {
            ffi::SDL_InitSubSystem(ffi::SDL_INIT_AUDIO);
            let want = ffi::SDL_AudioSpec {
                freq: rate as c_int,
                format: ffi::AUDIO_S16LSB as ffi::SDL_AudioFormat,
                channels,
                samples: 4096,
                ..Default::default()
            };
            let mut have = ffi::SDL_AudioSpec::default();
            ffi::SDL_OpenAudioDevice(null(), 0, &want, &mut have, 0)
        };
        if device == 0 {
            // SAFETY: SDL's message of the last failure, a C string.
            let why = unsafe { CStr::from_ptr(ffi::SDL_GetError()) };
            return Err(format!("Failed to open audio: {}", why.to_string_lossy()));
        }
        // SAFETY: the device that just opened; 0 starts it playing.
        unsafe { ffi::SDL_PauseAudioDevice(device, 0) };
        log::line(&format!(
            "sound: SDL audio: {channels} channels at {rate} Hz, 4096 frames"
        ));
        Ok(Self { device })
    }

    fn play(&mut self, pcm: &[i16]) {
        let pcm = bytes(pcm);
        // SAFETY: SDL copies the bytes. sdl.c ignores the result too.
        unsafe { ffi::SDL_QueueAudio(self.device, pcm.as_ptr().cast(), pcm.len() as u32) };
    }
}

impl Drop for Sdl {
    fn drop(&mut self) {
        // SAFETY: ours, closed once.
        unsafe { ffi::SDL_CloseAudioDevice(self.device) };
    }
}

#[cfg(test)]
#[path = "renderer_tests.rs"]
mod tests;
