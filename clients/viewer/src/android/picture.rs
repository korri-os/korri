// SPDX-License-Identifier: GPL-3.0-only
//! One stream's picture on Android (#391 step D, slice D2): Android's own
//! H.264 decoder (`AMediaCodec`) renders each frame straight into the
//! Surface that the app gives, the Surface of its video layer. No frame
//! passes through this code: the decoder writes into the layer's buffer
//! queue, and the compositor reads the same buffer.
//!
//! This file holds the core's decoder callbacks, the decoder's making, and
//! `AMediaCodec` itself, with the recovery's steps. The decoder's two loops
//! and when to recover are in `decoder.rs`, the choice of decoder and its
//! keys in `codec.rs`, the SPS rewrite in `sps.rs`; those follow Artemis.
//!
//! The network core's decoder callbacks carry no context, so one stream's
//! state is process-wide, as the core itself is. No panic leaves a callback
//! (`callback::guarded`): it would abort the app.
//!
//! A new picture size in the middle of the stream: the decoder is
//! configured for adaptive playback up to Korri's live-resolution maximum
//! when it supports it, so that it follows a new SPS by itself; otherwise
//! `decoder.rs` restarts it at the new size. Each new decoded size, from the
//! output format, goes to the app, which places its video layer for it.
//!
//! The decoder list's queries, the configure tries and the base format's
//! colour keys are ported from Korri's copy of Artemis (`clients/android`,
//! from `simonwjackson/artemis` at `cf12c432`, a fork of
//! `ClassicOldSong/moonlight-android`, itself a fork of
//! `moonlight-stream/moonlight-android`), `MediaCodecHelper.java` and
//! `MediaCodecDecoderRenderer.java` (`createBaseMediaFormat`, the tries in
//! `initializeDecoder`, the decoded size on `INFO_OUTPUT_FORMAT_CHANGED`, the
//! recovery's steps in `doCodecRecoveryIfRequired`).
//! The live-resolution maximum's capability query is Korri's own, from
//! `cbfeff25b` in that copy. Artemis ships under the GNU GPL version 3 with
//! no "or later" grant, so this file does too. The Rust adaptation is Korri
//! work dated 2026-11-14 and 2026-11-15.
use crate::{
    callback::guarded,
    codec,
    decoder::{
        self, Codec, Configured, Decoder, Frame, Health, Output, Queued, Records, Resized, Status,
        Step,
    },
    frame_record, log,
    network::{self as ffi, DECODE_UNIT, DECODER_RENDERER_CALLBACKS},
};
use anyhow::{Context, Result, anyhow, bail};
use jni::{
    JNIEnv, JavaVM,
    objects::{JObject, JObjectArray, JString, JValue},
};
use ndk::{media::media_format::MediaFormat, native_window::NativeWindow};
use std::{
    ffi::{CString, c_int, c_void},
    path::PathBuf,
    ptr::{NonNull, null_mut},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicPtr, Ordering},
    },
    time::Duration,
};

/// Where one stream's picture goes, the frame record when asked, and who
/// hears of a new decoded size.
pub struct Picture {
    /// The Surface of the app's video layer.
    pub surface: NativeWindow,
    pub record: FrameRecord,
    /// Each new size of the decoded picture (width, height), the first one
    /// included, on the decoder's output thread: the app places its video
    /// layer for it. It must not block.
    pub resized: Arc<dyn Fn((i32, i32)) + Send + Sync>,
}

/// The frame record's two files (`frame_record.rs`), by the names of the
/// measure's ask: `CADENCE_FRAME_TIMES` for `times.csv`, `CADENCE_DUMP` for
/// the recording. Each must be a new name.
#[derive(Default)]
pub struct FrameRecord {
    pub times: Option<PathBuf>,
    pub recording: Option<PathBuf>,
}

/// The stream's picture, from `PlayerIo::new` to its drop.
struct Stream {
    surface: NativeWindow,
    records: Arc<Records>,
    resized: Resized,
    decoder: Option<Arc<Decoder<NdkCodec>>>,
}

static STREAM: Mutex<Option<Stream>> = Mutex::new(None);

/// The stream's state. A panic that a callback caught leaves it usable.
fn stream() -> MutexGuard<'static, Option<Stream>> {
    STREAM.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Make the record's files and hold the picture for the core's callbacks.
/// Before the launch: a record that cannot be made stops nothing that ran.
pub(crate) fn open(picture: Picture) -> Result<()> {
    let times = picture
        .record
        .times
        .as_deref()
        .map(|path| -> Result<_> {
            let record = frame_record::Record::new(frame_record::create(path)?)
                .context("cannot start the frame record")?;
            Ok(Mutex::new(record))
        })
        .transpose()?;
    let recording = picture
        .record
        .recording
        .as_deref()
        .map(|path| -> Result<_> {
            Ok(Mutex::new(frame_record::Recording::new(
                frame_record::create(path)?,
            )))
        })
        .transpose()?;
    let mut stream = stream();
    anyhow::ensure!(stream.is_none(), "a picture is already open");
    *stream = Some(Stream {
        surface: picture.surface,
        records: Arc::new(Records { times, recording }),
        resized: picture.resized,
        decoder: None,
    });
    Ok(())
}

/// After the core has stopped: write the record's last rows, report whether
/// the whole record was written, and let the Surface go.
pub(crate) fn close() -> Result<()> {
    let Some(stream) = stream().take() else {
        return Ok(());
    };
    // The core calls cleanup after a setup; this covers a stream it did not.
    if let Some(decoder) = stream.decoder {
        decoder.finish();
    }
    let records = Arc::try_unwrap(stream.records)
        .map_err(|_| anyhow!("the decoder still holds the frame record"))?;
    let times = records
        .times
        .map(|times| times.into_inner().unwrap().close())
        .transpose();
    let recording = records
        .recording
        .map(|recording| recording.into_inner().unwrap().close())
        .transpose();
    times
        .and(recording)
        .context("the frame record is incomplete")?;
    Ok(())
}

/// Why the decoder failed, once it has: the stream then ends.
pub(crate) fn failure() -> Option<String> {
    stream().as_ref()?.decoder.as_ref()?.failure()
}

/// The stream's health now (slice D11): the decoder's fresh sample, and the
/// network core's estimated round trip in milliseconds when it has one
/// (`MediaCodecDecoderRenderer.viewerHealth`). None without a decoder, while
/// the stream stops, or with no fresh sample.
pub(crate) fn health() -> Option<(crate::video_stats::Sample, Option<u32>)> {
    // Under the picture's lock: the core destroys its control stream only
    // after the decoder's cleanup, which takes this lock, and the decoder
    // gives no sample once the core has asked it to stop.
    let stream = stream();
    let decoder = stream.as_ref()?.decoder.as_ref()?;
    let sample = decoder.sample(decoder::monotonic_ns() / 1_000_000)?;
    let mut rtt = 0;
    // SAFETY: the core reads its peer under its own lock and writes only
    // the one estimate asked for.
    let known = unsafe { ffi::LiGetEstimatedRttInfo(&mut rtt, null_mut()) };
    Some((sample, known.then_some(rtt)))
}

/// The core's decoder callbacks for this picture.
pub(crate) fn callbacks() -> *mut DECODER_RENDERER_CALLBACKS {
    // The core reads the table during LiStartConnection and keeps the pointer.
    static mut CALLBACKS: DECODER_RENDERER_CALLBACKS = DECODER_RENDERER_CALLBACKS {
        setup: Some(setup),
        start: Some(start),
        stop: Some(stop),
        cleanup: Some(cleanup),
        submitDecodeUnit: Some(submit),
        capabilities: 0,
    };
    &raw mut CALLBACKS
}

/// One H.264 frame: the stream's size and rate; the codec's own.
const MIME: &str = "video/avc";
/// `AMEDIA_ERROR_INVALID_OPERATION`.
const INVALID_OPERATION: Status = Status(-10005);
/// `AMEDIA_ERROR_INVALID_OBJECT`: there is no codec.
const NO_CODEC: Status = Status(-10003);
/// `MediaFormat`: `COLOR_RANGE_LIMITED`, `COLOR_STANDARD_BT601_NTSC` and
/// `COLOR_TRANSFER_SDR_VIDEO`, the core's default colour space and range.
const COLOR_KEYS: [(&str, i32); 3] = [
    ("color-range", 2),
    ("color-standard", 4),
    ("color-transfer", 3),
];

unsafe extern "C" fn setup(
    format: c_int,
    width: c_int,
    height: c_int,
    fps: c_int,
    _: *mut c_void,
    _: c_int,
) -> c_int {
    guarded("picture setup", -1, || {
        match make_decoder(format, width, height, fps) {
            Ok(()) => 0,
            Err(error) => {
                log::line(&format!("picture: no decoder: {error:#}"));
                -1
            }
        }
    })
}

fn make_decoder(format: c_int, width: c_int, height: c_int, fps: c_int) -> Result<()> {
    if format & ffi::VIDEO_FORMAT_MASK_H264 as c_int == 0 {
        bail!("the stream is not H.264 (format {format:#x}); this slice decodes H.264 only");
    }
    let decoders = decoder_list()?;
    log::line(&format!(
        "picture: decoders for {MIME}: {}",
        decoders
            .iter()
            .map(|d| format!(
                "{} (hardware {}, software-only {}, alias {}, low-latency {}, high {}, adaptive {})",
                d.name,
                d.hardware,
                d.software_only,
                d.alias,
                d.low_latency,
                d.high_profile,
                d.adaptive_playback
            ))
            .collect::<Vec<_>>()
            .join("; ")
    ));
    let emulator = system_property("ro.hardware").as_deref() == Some("ranchu");
    let chosen = codec::choose(&decoders, emulator)
        .with_context(|| format!("no usable {MIME} decoder (emulator {emulator})"))?;
    let size = (width, height);
    // Adaptive playback's maximum, asked of the decoder.
    let adaptive =
        codec::adaptive_playback(chosen).then(|| asked_adaptive_max(&chosen.name, size, fps));
    let mut stream = stream();
    let stream = stream.as_mut().context("no picture is open")?;
    let mut tries = codec::tries(chosen, adaptive).into_iter().peekable();
    let (codec, configured) = loop {
        let attempt = tries.next().context("no configure to try")?;
        let mut codec = NdkCodec::by_name(&chosen.name, &stream.surface)
            .with_context(|| format!("cannot make the decoder {}", chosen.name))?;
        match codec.configure(media_format(size, attempt.max, fps, &attempt.keys)) {
            Ok(()) => {
                codec.setup = Some(Setup {
                    fps,
                    keys: attempt.keys.clone(),
                });
                break (codec, attempt);
            }
            Err(status) if tries.peek().is_some() => {
                log::line(&format!(
                    "picture: {} try {} {} refused: {status}",
                    chosen.name,
                    attempt.number,
                    adaptive_text(attempt.max)
                ));
            }
            Err(status) => bail!("{} refused its last configuration: {status}", chosen.name),
        }
    };
    codec
        .start()
        .map_err(|status| anyhow!("cannot start the decoder: {status}"))?;
    let (number, adaptive) = (configured.number, configured.max);
    let keys: Vec<_> = configured
        .keys
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    // The picture path, as the Linux viewer reports its own: `decode`.
    log::line(&format!(
        "picture: decode {} decoder {} hardware {} software-only {} low-latency-feature {} try {number} keys {} size {width}x{height}@{fps} adaptive {}",
        chosen.decode(),
        chosen.name,
        chosen.hardware,
        chosen.software_only,
        chosen.low_latency,
        if keys.is_empty() {
            "none".into()
        } else {
            keys.join(",")
        },
        adaptive.map_or("none".into(), |(w, h)| format!("{w}x{h}")),
    ));
    let decoder = Decoder::new(
        codec,
        Configured {
            size,
            fps,
            decoder: chosen.name.clone(),
            adaptive,
        },
        stream.records.clone(),
        Health::default(),
        stream.resized.clone(),
    );
    decoder.start_output()?;
    stream.decoder = Some(decoder);
    Ok(())
}

unsafe extern "C" fn start() {
    guarded("picture start", (), || {});
}

unsafe extern "C" fn stop() {
    guarded("picture stop", (), || {
        if let Some(decoder) = current() {
            decoder.stop();
        }
    });
}

unsafe extern "C" fn cleanup() {
    guarded("picture cleanup", (), || {
        let decoder = stream().as_mut().and_then(|stream| stream.decoder.take());
        if let Some(decoder) = decoder {
            decoder.finish();
        }
    });
}

unsafe extern "C" fn submit(unit: *mut DECODE_UNIT) -> c_int {
    guarded("picture submit", ffi::DR_NEED_IDR, || {
        let Some(decoder) = current() else {
            return ffi::DR_NEED_IDR;
        };
        // SAFETY: the core keeps the unit and its buffer list for this call.
        let unit = unsafe { &*unit };
        let parts = parts(unit);
        let record =
            decoder
                .records
                .unit(unit.frameNumber, || enqueue_ns(unit.enqueueTimeUs), &parts);
        let frame = Frame {
            number: unit.frameNumber,
            idr: unit.frameType == ffi::FRAME_TYPE_IDR as c_int,
            enqueue_us: unit.enqueueTimeUs,
            parts,
            record,
        };
        match decoder.queue(frame) {
            Queued::Taken => ffi::DR_OK as c_int,
            Queued::NeedIdr => ffi::DR_NEED_IDR,
        }
    })
}

fn current() -> Option<Arc<Decoder<NdkCodec>>> {
    stream().as_ref()?.decoder.clone()
}

/// The core's clock is its own: microseconds of MONOTONIC_RAW since the
/// process first asked. Carry the enqueue time to CLOCK_MONOTONIC through the
/// short delay since then, as frame_times.c does. False when the enqueue
/// time is later than the core's clock.
fn enqueue_ns(enqueue_us: u64) -> (u64, bool) {
    let before = decoder::monotonic_ns();
    // SAFETY: no preconditions.
    let native_us = unsafe { ffi::LiGetMicroseconds() };
    let after = decoder::monotonic_ns();
    let ns =
        (before + (after - before) / 2).saturating_sub(native_us.saturating_sub(enqueue_us) * 1000);
    (ns, native_us >= enqueue_us)
}

/// The unit's buffers, in order, with their types.
fn parts(unit: &DECODE_UNIT) -> Vec<(c_int, &[u8])> {
    let mut parts = Vec::new();
    let mut entry = unit.bufferList;
    while !entry.is_null() {
        // SAFETY: the core's list of live entries, each with its own length.
        let item = unsafe { &*entry };
        let data =
            unsafe { std::slice::from_raw_parts(item.data.cast::<u8>(), item.length as usize) };
        parts.push((item.bufferType, data));
        entry = item.next;
    }
    parts
}

/// The format of one configure: the picture's size, adaptive playback's
/// maximum (`max-width`, `max-height`) when some, the stream's rate, the
/// core's colour keys and the try's low-latency keys.
fn media_format(
    size: (i32, i32),
    max: Option<(i32, i32)>,
    fps: i32,
    keys: &[(&'static str, i32)],
) -> MediaFormat {
    let mut media = MediaFormat::new();
    media.set_str("mime", MIME);
    media.set_i32("width", size.0);
    media.set_i32("height", size.1);
    media.set_i32("frame-rate", fps);
    if let Some((max_width, max_height)) = max {
        media.set_i32("max-width", max_width);
        media.set_i32("max-height", max_height);
    }
    for (key, value) in COLOR_KEYS.iter().chain(keys) {
        media.set_i32(key, *value);
    }
    media
}

/// What a size restart configures again: the try that worked.
struct Setup {
    fps: i32,
    keys: Vec<(&'static str, i32)>,
}

/// `AMediaCodec`, through ndk-sys. The ndk crate's input buffer panics when
/// `AMediaCodec_getInputBuffer` gives NULL, as it does after a decoder error;
/// this gives none instead. It keeps what the recovery makes it again with:
/// its name, the Surface, and the format it was last configured with
/// (Artemis's `configuredFormat`), which a size restart also sets.
struct NdkCodec {
    /// Null after a reset that made no new codec.
    codec: AtomicPtr<ndk_sys::AMediaCodec>,
    name: String,
    surface: NativeWindow,
    configured: Mutex<Option<MediaFormat>>,
    /// Set once the codec is configured.
    setup: Option<Setup>,
}

// SAFETY: AMediaCodec is safe to use from two threads at once: one queues
// input, the other releases output, as Android's own MediaCodec users do.
// A restart or a recovery step changes or replaces it only while no other
// call runs (the decoder's gate), and it is deleted only after both threads
// stop (the last reference). The format is used under its lock only.
unsafe impl Send for NdkCodec {}
unsafe impl Sync for NdkCodec {}

/// `AMEDIACODEC_INFO_*`.
const TRY_AGAIN_LATER: isize = -1;
const OUTPUT_FORMAT_CHANGED: isize = -2;
const OUTPUT_BUFFERS_CHANGED: isize = -3;

fn checked(status: ndk_sys::media_status_t) -> Result<(), Status> {
    if status.0 == 0 {
        Ok(())
    } else {
        Err(Status(status.0))
    }
}

fn micros(timeout: Duration) -> i64 {
    timeout.as_micros().try_into().unwrap_or(i64::MAX)
}

/// A new codec of this name; none when Android makes none.
fn make_codec(name: &str) -> Option<NonNull<ndk_sys::AMediaCodec>> {
    let name = CString::new(name).ok()?;
    // SAFETY: a live C string.
    NonNull::new(unsafe { ndk_sys::AMediaCodec_createCodecByName(name.as_ptr()) })
}

impl NdkCodec {
    /// A codec of this name, for the Surface.
    fn by_name(name: &str, surface: &NativeWindow) -> Option<Self> {
        make_codec(name).map(|codec| Self {
            codec: AtomicPtr::new(codec.as_ptr()),
            name: name.to_owned(),
            surface: surface.clone(),
            configured: Mutex::new(None),
            setup: None,
        })
    }

    /// The codec; none after a reset that made no new one.
    fn live(&self) -> Result<*mut ndk_sys::AMediaCodec, Status> {
        let codec = self.codec.load(Ordering::Acquire);
        if codec.is_null() {
            Err(NO_CODEC)
        } else {
            Ok(codec)
        }
    }

    /// Configure for the Surface; the format is then the one that the
    /// recovery configures again.
    fn configure(&self, format: MediaFormat) -> Result<(), Status> {
        self.configure_with(&format)?;
        *self
            .configured
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(format);
        Ok(())
    }

    fn configure_with(&self, format: &MediaFormat) -> Result<(), Status> {
        // SAFETY: a live codec, format and window; no crypto; a decoder.
        checked(unsafe {
            ndk_sys::AMediaCodec_configure(
                self.live()?,
                format.as_ptr(),
                self.surface.ptr().as_ptr(),
                null_mut(),
                0,
            )
        })
    }

    fn start(&self) -> Result<(), Status> {
        // SAFETY: a live, configured codec.
        checked(unsafe { ndk_sys::AMediaCodec_start(self.live()?) })
    }

    /// Configure with the last format again, and start.
    fn configure_again(&self) -> Result<(), Status> {
        let format = self
            .configured
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.configure_with(format.as_ref().ok_or(INVALID_OPERATION)?)?;
        self.start()
    }
}

impl Drop for NdkCodec {
    fn drop(&mut self) {
        if let Ok(codec) = self.live() {
            // SAFETY: the last reference; no thread uses the codec any more.
            unsafe { ndk_sys::AMediaCodec_delete(codec) };
        }
    }
}

impl Codec for NdkCodec {
    fn dequeue_input(&self, timeout: Duration) -> Result<Option<usize>, Status> {
        // SAFETY: a live codec.
        let index =
            unsafe { ndk_sys::AMediaCodec_dequeueInputBuffer(self.live()?, micros(timeout)) };
        match index {
            0.. => Ok(Some(index as usize)),
            TRY_AGAIN_LATER => Ok(None),
            status => Err(Status(status as i32)),
        }
    }

    fn input_buffer(&self, index: usize) -> Option<(NonNull<u8>, usize)> {
        let mut size = 0;
        // SAFETY: a live codec and an index it gave; it writes the size.
        let memory =
            unsafe { ndk_sys::AMediaCodec_getInputBuffer(self.live().ok()?, index, &mut size) };
        NonNull::new(memory).map(|memory| (memory, size))
    }

    fn queue_input(&self, index: usize, size: usize, pts: u64, flags: u32) -> Result<(), Status> {
        // SAFETY: a live codec and an index it gave.
        checked(unsafe {
            ndk_sys::AMediaCodec_queueInputBuffer(self.live()?, index, 0, size, pts, flags)
        })
    }

    fn dequeue_output(&self, timeout: Duration) -> Result<Output, Status> {
        let mut info = ndk_sys::AMediaCodecBufferInfo {
            offset: 0,
            size: 0,
            presentationTimeUs: 0,
            flags: 0,
        };
        // SAFETY: a live codec and a buffer info to fill.
        let index = unsafe {
            ndk_sys::AMediaCodec_dequeueOutputBuffer(self.live()?, &mut info, micros(timeout))
        };
        match index {
            0.. => Ok(Output::Frame {
                index: index as usize,
                pts: info.presentationTimeUs.max(0) as u64,
            }),
            OUTPUT_FORMAT_CHANGED => Ok(Output::FormatChanged),
            TRY_AGAIN_LATER | OUTPUT_BUFFERS_CHANGED => Ok(Output::TryAgain),
            status => Err(Status(status as i32)),
        }
    }

    fn release_output(&self, index: usize, render: Option<i64>) -> Result<(), Status> {
        let codec = self.live()?;
        // SAFETY: a live codec and an index it gave.
        checked(unsafe {
            match render {
                None => ndk_sys::AMediaCodec_releaseOutputBuffer(codec, index, false),
                Some(at) => ndk_sys::AMediaCodec_releaseOutputBufferAtTime(codec, index, at),
            }
        })
    }

    fn output_format(&self) -> String {
        self.format()
            .map(|format| format.to_string())
            .unwrap_or_default()
    }

    fn output_size(&self) -> Option<(i32, i32)> {
        let format = self.format()?;
        let (mut left, mut top, mut right, mut bottom) = (0, 0, 0, 0);
        let key = c"crop";
        // SAFETY: a live format and a C string key; the call fills the four
        // values. AMEDIAFORMAT_KEY_DISPLAY_CROP, API 28: an inclusive rect.
        let cropped = unsafe {
            ndk_sys::AMediaFormat_getRect(
                format.as_ptr(),
                key.as_ptr(),
                &mut left,
                &mut top,
                &mut right,
                &mut bottom,
            )
        };
        let size = if cropped {
            (right - left + 1, bottom - top + 1)
        } else {
            (format.i32("width")?, format.i32("height")?)
        };
        (size.0 > 0 && size.1 > 0).then_some(size)
    }

    fn adaptive_max(&self, size: (i32, i32)) -> (i32, i32) {
        match &self.setup {
            Some(setup) => asked_adaptive_max(&self.name, size, setup.fps),
            None => size,
        }
    }

    fn restart(
        &self,
        size: (i32, i32),
        max: Option<(i32, i32)>,
    ) -> Result<Option<(i32, i32)>, Status> {
        let setup = self.setup.as_ref().ok_or(INVALID_OPERATION)?;
        // SAFETY: a live codec; Executing to Uninitialized.
        checked(unsafe { ndk_sys::AMediaCodec_stop(self.live()?) })?;
        // The format that works is the one that the recovery configures again.
        let configure = |max| self.configure(media_format(size, max, setup.fps, &setup.keys));
        // As the first configure: without max-width and max-height when the
        // decoder refuses them.
        let max = match (configure(max), max) {
            (Ok(()), max) => max,
            (Err(status), Some(_)) => {
                log::line(&format!(
                    "picture: {} refused {} at {}x{}: {status}; configuring without it",
                    self.name,
                    adaptive_text(max),
                    size.0,
                    size.1
                ));
                configure(None)?;
                None
            }
            (Err(status), None) => return Err(status),
        };
        self.start()?;
        Ok(max)
    }

    fn stop(&self) {
        if let Ok(codec) = self.live() {
            // SAFETY: a live codec.
            unsafe { ndk_sys::AMediaCodec_stop(codec) };
        }
    }

    fn recover(&self, step: Step) -> Result<(), Status> {
        match step {
            // SAFETY: a live codec.
            Step::Flush => checked(unsafe { ndk_sys::AMediaCodec_flush(self.live()?) }),
            Step::Restart => {
                // The configure decides: a codec that an error has already
                // stopped refuses the stop and takes the configure.
                // SAFETY: a live codec.
                let _ = unsafe { ndk_sys::AMediaCodec_stop(self.live()?) };
                self.configure_again()
            }
            Step::Reset => {
                // The old codec goes first, as in Artemis: a device can have
                // one decoder instance only, and the Surface takes one codec
                // at a time.
                let old = self.codec.swap(null_mut(), Ordering::AcqRel);
                if !old.is_null() {
                    // SAFETY: the codec that this held; no call uses it now.
                    unsafe { ndk_sys::AMediaCodec_delete(old) };
                }
                let new = make_codec(&self.name).ok_or(NO_CODEC)?;
                self.codec.store(new.as_ptr(), Ordering::Release);
                self.configure_again()
            }
        }
    }
}

impl NdkCodec {
    /// The codec's output format, ours to delete.
    fn format(&self) -> Option<MediaFormat> {
        // SAFETY: a live codec; the format it gives is ours.
        NonNull::new(unsafe { ndk_sys::AMediaCodec_getOutputFormat(self.live().ok()?) })
            .map(|format| unsafe { MediaFormat::from_ptr(format) })
    }
}

/// Android's decoders for `video/avc`, from `android.media.MediaCodecList`
/// through JNI: Android 12 has no NDK call for the list (ADR 0064).
fn decoder_list() -> Result<Vec<codec::Decoder>> {
    let context = ndk_context::android_context();
    // SAFETY: android-activity sets the process's VM before android_main.
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let result = env.with_local_frame(64, |env| -> jni::errors::Result<_> {
        let list = env.new_object("android/media/MediaCodecList", "(I)V", &[JValue::Int(0)])?;
        let infos = JObjectArray::from(
            env.call_method(
                &list,
                "getCodecInfos",
                "()[Landroid/media/MediaCodecInfo;",
                &[],
            )?
            .l()?,
        );
        let mut decoders = Vec::new();
        for at in 0..env.get_array_length(&infos)? {
            let info = env.get_object_array_element(&infos, at)?;
            if env.call_method(&info, "isEncoder", "()Z", &[])?.z()? || !decodes_avc(env, &info)? {
                continue;
            }
            let flag = |env: &mut JNIEnv, name| -> jni::errors::Result<bool> {
                env.call_method(&info, name, "()Z", &[])?.z()
            };
            let name = env
                .call_method(&info, "getName", "()Ljava/lang/String;", &[])?
                .l()?;
            let name: String = env.get_string(&JString::from(name))?.into();
            let hardware = flag(env, "isHardwareAccelerated")?;
            let software_only = flag(env, "isSoftwareOnly")?;
            let alias = flag(env, "isAlias")?;
            // Some decoders throw here; they stay last resorts, as in Artemis.
            let (low_latency, high_profile, adaptive_playback) = capabilities(env, &info)
                .unwrap_or_else(|_| {
                    let _ = env.exception_clear();
                    (false, false, false)
                });
            decoders.push(codec::Decoder {
                name,
                hardware,
                software_only,
                alias,
                low_latency,
                high_profile,
                adaptive_playback,
            });
        }
        Ok(decoders)
    });
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result.map_err(|error| anyhow!("cannot list the decoders: {error}"))
}

fn decodes_avc(env: &mut JNIEnv, info: &JObject) -> jni::errors::Result<bool> {
    let types = JObjectArray::from(
        env.call_method(info, "getSupportedTypes", "()[Ljava/lang/String;", &[])?
            .l()?,
    );
    for at in 0..env.get_array_length(&types)? {
        let kind = JString::from(env.get_object_array_element(&types, at)?);
        if env
            .get_string(&kind)?
            .to_str()
            .is_ok_and(|kind| kind.eq_ignore_ascii_case(MIME))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `FEATURE_LowLatency`, `AVCProfileHigh` (8) and `FEATURE_AdaptivePlayback`
/// for `video/avc`.
fn capabilities(env: &mut JNIEnv, info: &JObject) -> jni::errors::Result<(bool, bool, bool)> {
    let caps = avc_capabilities(env, info)?;
    let feature = |env: &mut JNIEnv, name: &str| -> jni::errors::Result<bool> {
        let name = env.new_string(name)?;
        env.call_method(
            &caps,
            "isFeatureSupported",
            "(Ljava/lang/String;)Z",
            &[(&name).into()],
        )?
        .z()
    };
    let low_latency = feature(env, "low-latency")?;
    let adaptive_playback = feature(env, "adaptive-playback")?;
    let levels = JObjectArray::from(
        env.get_field(
            &caps,
            "profileLevels",
            "[Landroid/media/MediaCodecInfo$CodecProfileLevel;",
        )?
        .l()?,
    );
    let mut high = false;
    for at in 0..env.get_array_length(&levels)? {
        let level = env.get_object_array_element(&levels, at)?;
        high |= env.get_field(&level, "profile", "I")?.i()? == 8;
    }
    Ok((low_latency, high, adaptive_playback))
}

/// `MediaCodecInfo.getCapabilitiesForType("video/avc")`.
fn avc_capabilities<'a>(env: &mut JNIEnv<'a>, info: &JObject) -> jni::errors::Result<JObject<'a>> {
    let mime = env.new_string(MIME)?;
    env.call_method(
        info,
        "getCapabilitiesForType",
        "(Ljava/lang/String;)Landroid/media/MediaCodecInfo$CodecCapabilities;",
        &[(&mime).into()],
    )?
    .l()
}

/// For the log: `adaptive up to WxH` or `without adaptive playback`.
fn adaptive_text(max: Option<(i32, i32)>) -> String {
    max.map_or("without adaptive playback".into(), |(w, h)| {
        format!("adaptive up to {w}x{h}")
    })
}

/// `adaptive_max`, or the stream's own size when the decoder cannot answer.
fn asked_adaptive_max(name: &str, size: (i32, i32), fps: i32) -> (i32, i32) {
    adaptive_max(name, size, fps).unwrap_or_else(|error| {
        log::line(&format!(
            "picture: adaptive playback up to the stream's size {}x{}: {error:#}",
            size.0, size.1
        ));
        size
    })
}

/// Adaptive playback's maximum for the decoder of this name
/// (`codec::adaptive_max`), from its `VideoCapabilities` through JNI.
fn adaptive_max(name: &str, stream: (i32, i32), fps: i32) -> Result<(i32, i32)> {
    let context = ndk_context::android_context();
    // SAFETY: android-activity sets the process's VM before android_main.
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let result = env.with_local_frame(64, |env| -> jni::errors::Result<_> {
        let list = env.new_object("android/media/MediaCodecList", "(I)V", &[JValue::Int(0)])?;
        let infos = JObjectArray::from(
            env.call_method(
                &list,
                "getCodecInfos",
                "()[Landroid/media/MediaCodecInfo;",
                &[],
            )?
            .l()?,
        );
        for at in 0..env.get_array_length(&infos)? {
            let info = env.get_object_array_element(&infos, at)?;
            let found = env
                .call_method(&info, "getName", "()Ljava/lang/String;", &[])?
                .l()?;
            let found: String = env.get_string(&JString::from(found))?.into();
            if found != name || env.call_method(&info, "isEncoder", "()Z", &[])?.z()? {
                continue;
            }
            let caps = avc_capabilities(env, &info)?;
            let video = env
                .call_method(
                    &caps,
                    "getVideoCapabilities",
                    "()Landroid/media/MediaCodecInfo$VideoCapabilities;",
                    &[],
                )?
                .l()?;
            if video.is_null() {
                return Ok(None);
            }
            let max = codec::adaptive_max(stream, |side| {
                let supported = env
                    .call_method(
                        &video,
                        "areSizeAndRateSupported",
                        "(IID)Z",
                        &[side.into(), side.into(), f64::from(fps).into()],
                    )
                    .and_then(|value| value.z());
                supported.unwrap_or_else(|_| {
                    let _ = env.exception_clear();
                    false
                })
            });
            return Ok(Some(max));
        }
        Ok(None)
    });
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result
        .map_err(|error| anyhow!("cannot read the decoder's video capabilities: {error}"))?
        .with_context(|| format!("the decoder {name} has no video capabilities"))
}

fn system_property(name: &str) -> Option<String> {
    let name = CString::new(name).ok()?;
    // Bytes, not `c_char`: that is `i8` on x86_64 and `u8` on arm64.
    let mut value = [0u8; 92]; // PROP_VALUE_MAX
    // SAFETY: the value buffer has PROP_VALUE_MAX bytes, as the call needs.
    let length = unsafe { libc::__system_property_get(name.as_ptr(), value.as_mut_ptr().cast()) };
    (length > 0).then(|| String::from_utf8_lossy(&value[..length as usize]).into_owned())
}
