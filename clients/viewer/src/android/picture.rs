// SPDX-License-Identifier: GPL-3.0-only
//! One stream's picture on Android (#391 step D, slice D2): Android's own
//! H.264 decoder (`AMediaCodec`) renders each frame straight into the
//! Surface that the app gives, the Surface of its video layer. No frame
//! passes through this code: the decoder writes into the layer's buffer
//! queue, and the compositor reads the same buffer.
//!
//! This file holds the core's decoder callbacks, the decoder's making, and
//! `AMediaCodec` itself. The decoder's two loops are in `decoder.rs`, the
//! choice of decoder and its keys in `codec.rs`, the SPS rewrite in
//! `sps.rs`; those follow Artemis.
//!
//! The network core's decoder callbacks carry no context, so one stream's
//! state is process-wide, as the core itself is. No panic leaves a callback
//! (`callback::guarded`): it would abort the app.
//!
//! The decoder list's queries, the configure tries and the base format's
//! colour keys are ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`, a fork of
//! `ClassicOldSong/moonlight-android`, itself a fork of
//! `moonlight-stream/moonlight-android`), `MediaCodecHelper.java` and
//! `MediaCodecDecoderRenderer.java` (`createBaseMediaFormat`, the tries in
//! `initializeDecoder`). Artemis ships under the GNU GPL version 3 with no "or later" grant, so
//! this file does too. The Rust adaptation is Korri work dated 2026-11-14.
use crate::{
    callback::guarded,
    codec,
    decoder::{self, Codec, Decoder, Frame, Health, Output, Queued, Records, Status},
    frame_record, log,
    network::{self as ffi, DECODE_UNIT, DECODER_RENDERER_CALLBACKS},
    sps,
};
use anyhow::{Context, Result, anyhow, bail};
use jni::{
    JNIEnv, JavaVM,
    objects::{JObject, JObjectArray, JString, JValue},
};
use ndk::{media::media_format::MediaFormat, native_window::NativeWindow};
use std::{
    ffi::{CString, c_char, c_int, c_void},
    path::PathBuf,
    ptr::{NonNull, null_mut},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
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
                "{} (hardware {}, software-only {}, alias {}, low-latency {}, high {})",
                d.name, d.hardware, d.software_only, d.alias, d.low_latency, d.high_profile
            ))
            .collect::<Vec<_>>()
            .join("; ")
    ));
    let emulator = system_property("ro.hardware").as_deref() == Some("ranchu");
    let chosen = codec::choose(&decoders, emulator)
        .with_context(|| format!("no usable {MIME} decoder (emulator {emulator})"))?;
    let mut stream = stream();
    let stream = stream.as_mut().context("no picture is open")?;
    let mut number = 0;
    let (codec, keys) = loop {
        let attempt = codec::low_latency_try(chosen, number);
        let codec = NdkCodec::by_name(&chosen.name)
            .with_context(|| format!("cannot make the decoder {}", chosen.name))?;
        let mut media = MediaFormat::new();
        media.set_str("mime", MIME);
        media.set_i32("width", width);
        media.set_i32("height", height);
        media.set_i32("frame-rate", fps);
        for (key, value) in COLOR_KEYS.iter().chain(&attempt.keys) {
            media.set_i32(key, *value);
        }
        match codec.configure(&media, &stream.surface) {
            Ok(()) => break (codec, attempt.keys),
            Err(status) if !attempt.last => {
                log::line(&format!(
                    "picture: {} try {number} refused: {status}",
                    chosen.name
                ));
                number += 1;
            }
            Err(status) => bail!("{} refused its last configuration: {status}", chosen.name),
        }
    };
    codec
        .start()
        .map_err(|status| anyhow!("cannot start the decoder: {status}"))?;
    let keys: Vec<_> = keys
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    // The picture path, as the Linux viewer reports its own: `decode`.
    log::line(&format!(
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
    let decoder = Decoder::new(
        codec,
        sps::Patch::for_stream(width, height, fps, &chosen.name),
        stream.records.clone(),
        Health::default(),
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

/// `AMediaCodec`, through ndk-sys. The ndk crate's input buffer panics when
/// `AMediaCodec_getInputBuffer` gives NULL, as it does after a decoder error;
/// this gives none instead.
struct NdkCodec(NonNull<ndk_sys::AMediaCodec>);

// SAFETY: AMediaCodec is safe to use from two threads at once: one queues
// input, the other releases output, as Android's own MediaCodec users do.
// It is deleted only after both threads stop (the last reference).
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

impl NdkCodec {
    fn by_name(name: &str) -> Option<Self> {
        let name = CString::new(name).ok()?;
        // SAFETY: a live C string.
        NonNull::new(unsafe { ndk_sys::AMediaCodec_createCodecByName(name.as_ptr()) }).map(Self)
    }

    fn configure(&self, format: &MediaFormat, surface: &NativeWindow) -> Result<(), Status> {
        // SAFETY: a live codec, format and window; no crypto; a decoder.
        checked(unsafe {
            ndk_sys::AMediaCodec_configure(
                self.0.as_ptr(),
                format.as_ptr(),
                surface.ptr().as_ptr(),
                null_mut(),
                0,
            )
        })
    }

    fn start(&self) -> Result<(), Status> {
        // SAFETY: a live, configured codec.
        checked(unsafe { ndk_sys::AMediaCodec_start(self.0.as_ptr()) })
    }
}

impl Drop for NdkCodec {
    fn drop(&mut self) {
        // SAFETY: the last reference; no thread uses the codec any more.
        unsafe { ndk_sys::AMediaCodec_delete(self.0.as_ptr()) };
    }
}

impl Codec for NdkCodec {
    fn dequeue_input(&self, timeout: Duration) -> Result<Option<usize>, Status> {
        // SAFETY: a live codec.
        let index =
            unsafe { ndk_sys::AMediaCodec_dequeueInputBuffer(self.0.as_ptr(), micros(timeout)) };
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
            unsafe { ndk_sys::AMediaCodec_getInputBuffer(self.0.as_ptr(), index, &mut size) };
        NonNull::new(memory).map(|memory| (memory, size))
    }

    fn queue_input(&self, index: usize, size: usize, pts: u64, flags: u32) -> Result<(), Status> {
        // SAFETY: a live codec and an index it gave.
        checked(unsafe {
            ndk_sys::AMediaCodec_queueInputBuffer(self.0.as_ptr(), index, 0, size, pts, flags)
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
            ndk_sys::AMediaCodec_dequeueOutputBuffer(self.0.as_ptr(), &mut info, micros(timeout))
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
        // SAFETY: a live codec and an index it gave.
        checked(unsafe {
            match render {
                None => ndk_sys::AMediaCodec_releaseOutputBuffer(self.0.as_ptr(), index, false),
                Some(at) => {
                    ndk_sys::AMediaCodec_releaseOutputBufferAtTime(self.0.as_ptr(), index, at)
                }
            }
        })
    }

    fn output_format(&self) -> String {
        // SAFETY: a live codec; the format is ours to delete.
        NonNull::new(unsafe { ndk_sys::AMediaCodec_getOutputFormat(self.0.as_ptr()) })
            .map(|format| unsafe { MediaFormat::from_ptr(format) }.to_string())
            .unwrap_or_default()
    }

    fn stop(&self) {
        // SAFETY: a live codec.
        unsafe { ndk_sys::AMediaCodec_stop(self.0.as_ptr()) };
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
