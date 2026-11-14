// SPDX-License-Identifier: GPL-3.0-or-later
//! One stream's picture on Android (#391 step D, slice D2): Android's own
//! H.264 decoder (`AMediaCodec`) renders each frame straight into the
//! Surface that the app gives, the Surface of its video layer. No frame
//! passes through this code: the decoder writes into the layer's buffer
//! queue, and the compositor reads the same buffer.
//!
//! The decoder and its keys follow Artemis, as the research brief lists them
//! (`codec.rs`); the SPS rewrite too (`sps.rs`). Each frame is handed to the
//! display as soon as the decoder returns it, and a frame that a newer one
//! overtook is dropped: upstream Moonlight's lowest-latency pacing.
//!
//! The network core's decoder callbacks carry no context, so one stream's
//! state is process-wide, as the core itself is.
use crate::{
    codec, frame_record,
    network::{self as ffi, DECODE_UNIT, DECODER_RENDERER_CALLBACKS},
    sps,
};
use anyhow::{Context, Result, anyhow, bail};
use jni::{
    JNIEnv, JavaVM,
    objects::{JObject, JObjectArray, JString, JValue},
};
use ndk::{
    media::{
        media_codec::{
            DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec,
            MediaCodecDirection,
        },
        media_format::MediaFormat,
    },
    native_window::NativeWindow,
};
use std::{
    collections::VecDeque,
    ffi::{CString, c_char, c_int, c_void},
    fs::File,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

/// Where one stream's picture goes, and the frame record when asked.
pub struct Picture {
    /// The Surface of the app's video layer.
    pub surface: NativeWindow,
    pub record: FrameRecord,
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
    decoder: Option<Arc<Decoder>>,
}

#[derive(Default)]
struct Records {
    times: Option<Mutex<frame_record::Record<File>>>,
    recording: Option<Mutex<frame_record::Recording<File>>>,
}

static STREAM: Mutex<Option<Stream>> = Mutex::new(None);

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
    let mut stream = STREAM.lock().unwrap();
    anyhow::ensure!(stream.is_none(), "a picture is already open");
    *stream = Some(Stream {
        surface: picture.surface,
        records: Arc::new(Records { times, recording }),
        decoder: None,
    });
    Ok(())
}

/// After the core has stopped: write the record's last rows, report whether
/// the whole record was written, and let the Surface go.
pub(crate) fn close() -> Result<()> {
    let Some(stream) = STREAM.lock().unwrap().take() else {
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

/// `AMediaCodec` is safe to use from two threads at once: one queues input,
/// the other releases output, as Android's own MediaCodec users do.
struct Codec(MediaCodec);
// SAFETY: see above; the codec is deleted only after both threads stop.
unsafe impl Send for Codec {}
unsafe impl Sync for Codec {}

struct Decoder {
    codec: Codec,
    patch: sps::Patch,
    input: Mutex<Input>,
    records: Arc<Records>,
    stopping: AtomicBool,
    output: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Default)]
struct Input {
    last_pts: u64,
    /// Each queued frame's presentation time and record index, in order.
    queued: VecDeque<(u64, Option<u64>)>,
}

/// One H.264 frame: the stream's size and rate; the codec's own.
const MIME: &str = "video/avc";
/// `MediaFormat`: `COLOR_RANGE_LIMITED`, `COLOR_STANDARD_BT601_NTSC` and
/// `COLOR_TRANSFER_SDR_VIDEO`, the core's default colour space and range.
const COLOR_KEYS: [(&str, i32); 3] = [
    ("color-range", 2),
    ("color-standard", 4),
    ("color-transfer", 3),
];
const CODEC_CONFIG: u32 = 2;
const KEY_FRAME: u32 = 1;

unsafe extern "C" fn setup(
    format: c_int,
    width: c_int,
    height: c_int,
    fps: c_int,
    _: *mut c_void,
    _: c_int,
) -> c_int {
    match make_decoder(format, width, height, fps) {
        Ok(()) => 0,
        Err(error) => {
            log(&format!("picture: no decoder: {error:#}"));
            -1
        }
    }
}

fn make_decoder(format: c_int, width: c_int, height: c_int, fps: c_int) -> Result<()> {
    if format & ffi::VIDEO_FORMAT_MASK_H264 as c_int == 0 {
        bail!("the stream is not H.264 (format {format:#x}); this slice decodes H.264 only");
    }
    let decoders = decoder_list()?;
    log(&format!(
        "picture: decoders for {MIME}: {}",
        decoders
            .iter()
            .map(|d| format!(
                "{} (hardware {}, software-only {}, alias {}, low-latency {}, high {})",
                d.name, d.hardware, d.software_only, d.alias, d.low_latency, d.high_profile
            ))
            .collect::<Vec<_>>()
            .join("; ")
    ));
    let emulator = system_property("ro.hardware").as_deref() == Some("ranchu");
    let chosen = codec::choose(&decoders, emulator)
        .with_context(|| format!("no usable {MIME} decoder (emulator {emulator})"))?;
    let mut stream = STREAM.lock().unwrap();
    let stream = stream.as_mut().context("no picture is open")?;
    let mut number = 0;
    let (codec, keys) = loop {
        let attempt = codec::low_latency_try(chosen, number);
        let codec = MediaCodec::from_codec_name(&chosen.name)
            .with_context(|| format!("cannot make the decoder {}", chosen.name))?;
        let mut media = MediaFormat::new();
        media.set_str("mime", MIME);
        media.set_i32("width", width);
        media.set_i32("height", height);
        media.set_i32("frame-rate", fps);
        for (key, value) in COLOR_KEYS.iter().chain(&attempt.keys) {
            media.set_i32(key, *value);
        }
        match codec.configure(&media, Some(&stream.surface), MediaCodecDirection::Decoder) {
            Ok(()) => break (codec, attempt.keys),
            Err(error) if !attempt.last => {
                log(&format!(
                    "picture: {} try {number} refused: {error:?}",
                    chosen.name
                ));
                number += 1;
            }
            Err(error) => bail!("{} refused its last configuration: {error:?}", chosen.name),
        }
    };
    codec.start().context("cannot start the decoder")?;
    let keys: Vec<_> = keys
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    // The picture path, as the Linux viewer reports its own: `decode`.
    log(&format!(
        "picture: decode {} decoder {} hardware {} software-only {} low-latency-feature {} try {number} keys {} size {width}x{height}@{fps}",
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
    ));
    let decoder = Arc::new(Decoder {
        codec: Codec(codec),
        patch: sps::Patch::for_stream(width, height, fps, &chosen.name),
        input: Mutex::default(),
        records: stream.records.clone(),
        stopping: AtomicBool::new(false),
        output: Mutex::new(None),
    });
    let output = {
        let decoder = decoder.clone();
        std::thread::Builder::new()
            .name("viewer-picture".into())
            .spawn(move || decoder.release_frames())?
    };
    *decoder.output.lock().unwrap() = Some(output);
    stream.decoder = Some(decoder);
    Ok(())
}

unsafe extern "C" fn start() {}

unsafe extern "C" fn stop() {
    if let Some(decoder) = current() {
        decoder.stopping.store(true, Ordering::Relaxed);
    }
}

unsafe extern "C" fn cleanup() {
    let decoder = STREAM
        .lock()
        .unwrap()
        .as_mut()
        .and_then(|stream| stream.decoder.take());
    if let Some(decoder) = decoder {
        decoder.finish();
    }
}

unsafe extern "C" fn submit(unit: *mut DECODE_UNIT) -> c_int {
    let Some(decoder) = current() else {
        return ffi::DR_NEED_IDR;
    };
    // SAFETY: the core keeps the unit and its buffer list for this call.
    let unit = unsafe { &*unit };
    match decoder.queue(unit) {
        Ok(()) => ffi::DR_OK as c_int,
        Err(error) => {
            log(&format!(
                "picture: frame {} not decoded: {error:#}",
                unit.frameNumber
            ));
            ffi::DR_NEED_IDR
        }
    }
}

fn current() -> Option<Arc<Decoder>> {
    STREAM.lock().unwrap().as_ref()?.decoder.clone()
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

impl Decoder {
    /// Stop the output thread and the codec. The codec is deleted with the
    /// last reference, after the output thread's.
    fn finish(&self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(output) = self.output.lock().unwrap().take() {
            let _ = output.join();
        }
        let _ = self.codec.0.stop();
        log("picture: decoder stopped");
    }

    fn queue(&self, unit: &DECODE_UNIT) -> Result<()> {
        let parts = parts(unit);
        let index = self.record_unit(unit, &parts);
        if self.stopping.load(Ordering::Relaxed) {
            return Ok(());
        }
        let idr = unit.frameType == ffi::FRAME_TYPE_IDR as c_int;
        let mut config = Vec::new();
        let mut picture = Vec::with_capacity(unit.fullLength.max(0) as usize);
        for (kind, data) in parts {
            match kind as u32 {
                ffi::BUFFER_TYPE_SPS => config.extend(self.rewrite_sps(data)),
                ffi::BUFFER_TYPE_PPS | ffi::BUFFER_TYPE_VPS => config.extend_from_slice(data),
                _ => picture.extend_from_slice(data),
            }
        }
        // The frame is known to the output thread before the decoder has it.
        // The lock is not held while this thread waits for an input buffer:
        // the output thread needs it to release the frames that free one.
        let (config_pts, pts) = {
            let mut input = self.input.lock().unwrap();
            let config_pts =
                (idr && !config.is_empty()).then(|| input.next_pts(unit.enqueueTimeUs));
            let pts = input.next_pts(unit.enqueueTimeUs);
            input.queued.push_back((pts, index));
            if input.queued.len() > 256 {
                input.queued.pop_front();
            }
            (config_pts, pts)
        };
        // The parameter sets go first, in their own buffer, per Android's
        // MediaCodec documentation; they produce no frame.
        if let Some(config_pts) = config_pts {
            self.queue_buffer(&config, config_pts, CODEC_CONFIG)?;
        }
        self.queue_buffer(&picture, pts, if idr { KEY_FRAME } else { 0 })
    }

    fn queue_buffer(&self, data: &[u8], pts: u64, flags: u32) -> Result<()> {
        let codec = &self.codec.0;
        // Wait for a buffer as Artemis does, in 10 ms steps until the stream
        // stops: a frame given up here would cost a new IDR frame.
        let mut buffer = loop {
            match codec
                .dequeue_input_buffer(Duration::from_millis(10))
                .map_err(|error| anyhow!("no input buffer: {error:?}"))?
            {
                DequeuedInputBufferResult::Buffer(buffer) => break buffer,
                DequeuedInputBufferResult::TryAgainLater
                    if self.stopping.load(Ordering::Relaxed) =>
                {
                    bail!("the stream stopped before the decoder took the frame")
                }
                DequeuedInputBufferResult::TryAgainLater => {}
            }
        };
        let room = buffer.buffer_mut();
        anyhow::ensure!(
            data.len() <= room.len(),
            "a frame of {} bytes is too large for its buffer",
            data.len()
        );
        // SAFETY: `room` holds at least `data.len()` bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), room.as_mut_ptr().cast(), data.len())
        };
        codec
            .queue_input_buffer(buffer, 0, data.len(), pts, flags)
            .map_err(|error| anyhow!("cannot queue the frame: {error:?}"))
    }

    /// Artemis's SPS patch; an SPS that cannot be read goes as it came.
    fn rewrite_sps(&self, data: &[u8]) -> Vec<u8> {
        let start = if data.starts_with(&[0, 0, 1]) { 3 } else { 4 };
        match data.get(start..).map(|nal| sps::rewrite(nal, self.patch)) {
            Some(Ok(nal)) => [&data[..start], &nal].concat(),
            Some(Err(error)) => {
                log(&format!("picture: SPS left as it came: {error:#}"));
                data.to_vec()
            }
            None => data.to_vec(),
        }
    }

    fn record_unit(&self, unit: &DECODE_UNIT, parts: &[(c_int, &[u8])]) -> Option<u64> {
        if let Some(recording) = &self.records.recording {
            recording
                .lock()
                .unwrap()
                .unit(parts.iter().map(|(_, data)| *data));
        }
        let times = self.records.times.as_ref()?;
        // The core's clock is its own: microseconds of MONOTONIC_RAW since
        // the process first asked. Carry the enqueue time to CLOCK_MONOTONIC
        // through the short delay since then, as frame_times.c does.
        let before = monotonic_ns();
        // SAFETY: no preconditions.
        let native_us = unsafe { ffi::LiGetMicroseconds() };
        let after = monotonic_ns();
        let mut times = times.lock().unwrap();
        if native_us < unit.enqueueTimeUs {
            times.fail(std::io::Error::other(
                "the core's enqueue time is later than its clock",
            ));
        }
        let enqueue_ns = (before + (after - before) / 2)
            .saturating_sub(native_us.saturating_sub(unit.enqueueTimeUs) * 1000);
        Some(times.unit(unit.frameNumber, enqueue_ns))
    }

    /// The output thread: hand each frame to the display at once. When more
    /// than one frame is ready, only the newest goes; the older ones were
    /// overtaken and are dropped.
    fn release_frames(&self) {
        let codec = &self.codec.0;
        while !self.stopping.load(Ordering::Relaxed) {
            let mut newest = None;
            let mut wait = Duration::from_millis(20);
            loop {
                match codec.dequeue_output_buffer(wait) {
                    Ok(DequeuedOutputBufferInfoResult::Buffer(buffer)) => {
                        let index = self.decoded(buffer.info().presentation_time_us() as u64);
                        if let Some((older, _)) = newest.replace((buffer, index)) {
                            let _ = codec.release_output_buffer(older, false);
                        }
                        wait = Duration::ZERO;
                    }
                    Ok(DequeuedOutputBufferInfoResult::OutputFormatChanged) => {
                        log(&format!("picture: output format {}", codec.output_format()));
                    }
                    Ok(_) => break,
                    Err(error) => {
                        log(&format!("picture: output failed: {error:?}"));
                        break;
                    }
                }
            }
            if let Some((buffer, index)) = newest {
                let released = codec.release_output_buffer_at_time(buffer, monotonic_ns() as i64);
                let handed = monotonic_ns();
                match released {
                    Ok(()) => self.displayed(index, handed),
                    Err(error) => log(&format!("picture: frame not released: {error:?}")),
                }
            }
        }
    }

    /// The decoder returned the frame of this presentation time: its record index.
    fn decoded(&self, pts: u64) -> Option<u64> {
        let now = monotonic_ns();
        let index = {
            let mut input = self.input.lock().unwrap();
            // Frames come out in the order they went in; one that the
            // decoder dropped has no output and leaves the queue here.
            loop {
                match input.queued.front() {
                    Some(&(queued, index)) if queued <= pts => {
                        input.queued.pop_front();
                        if queued == pts {
                            break index;
                        }
                    }
                    _ => break None,
                }
            }
        };
        if let (Some(index), Some(times)) = (index, &self.records.times) {
            times.lock().unwrap().decoded(index, now);
        }
        index
    }

    fn displayed(&self, index: Option<u64>, now: u64) {
        if let (Some(index), Some(times)) = (index, &self.records.times) {
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
            let (low_latency, high_profile) = capabilities(env, &info).unwrap_or_else(|_| {
                let _ = env.exception_clear();
                (false, false)
            });
            decoders.push(codec::Decoder {
                name,
                hardware,
                software_only,
                alias,
                low_latency,
                high_profile,
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

/// `FEATURE_LowLatency` and `AVCProfileHigh` (8) for `video/avc`.
fn capabilities(env: &mut JNIEnv, info: &JObject) -> jni::errors::Result<(bool, bool)> {
    let mime = env.new_string(MIME)?;
    let caps = env
        .call_method(
            info,
            "getCapabilitiesForType",
            "(Ljava/lang/String;)Landroid/media/MediaCodecInfo$CodecCapabilities;",
            &[(&mime).into()],
        )?
        .l()?;
    let feature = env.new_string("low-latency")?;
    let low_latency = env
        .call_method(
            &caps,
            "isFeatureSupported",
            "(Ljava/lang/String;)Z",
            &[(&feature).into()],
        )?
        .z()?;
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
    Ok((low_latency, high))
}

fn system_property(name: &str) -> Option<String> {
    let name = CString::new(name).ok()?;
    let mut value = [0 as c_char; 92]; // PROP_VALUE_MAX
    // SAFETY: the value buffer has PROP_VALUE_MAX bytes, as the call needs.
    let length = unsafe { libc::__system_property_get(name.as_ptr(), value.as_mut_ptr()) };
    (length > 0).then(|| {
        let bytes: Vec<u8> = value[..length as usize].iter().map(|&c| c as u8).collect();
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn monotonic_ns() -> u64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: a valid timespec to fill.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    now.tv_sec as u64 * 1_000_000_000 + now.tv_nsec as u64
}

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

/// A line in logcat with the tag of the core's and the app's lines.
fn log(text: &str) {
    let text = CString::new(text.replace('\0', "")).unwrap_or_default();
    // SAFETY: both are live C strings. 4: ANDROID_LOG_INFO.
    unsafe { __android_log_write(4, c"korri-viewer".as_ptr(), text.as_ptr()) };
}
