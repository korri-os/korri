// SPDX-License-Identifier: GPL-3.0-or-later
//! nightjar-core (`vendor/nightjar`, GPL-3.0-only), a Rust port of
//! moonlight-common-c, behind the seam (#391 step E). Built with the
//! `nightjar` feature, on Linux and Android; `-networkcore nightjar` selects
//! it. Android's picture, sound and feedback are Rust behind the same C
//! callback tables as Linux's C code, so the adapter is one for both.
//!
//! nightjar-core's `Session` is `LiStartConnection` and `LiStopConnection`
//! in shape: it owns its sockets and threads and calls a `VideoDecoder`, an
//! `AudioRenderer` and a `Listener`. Here each of those calls the platform's
//! C callback tables as the C core calls them, so the picture, sound and
//! input code is the same for both cores:
//!
//! - a frame becomes a `DECODE_UNIT` with one `LENTRY` for each of its
//!   buffers, in order, with the frame's receive, enqueue and presentation
//!   times on nightjar-core's clock (`now_us`, which `NetworkCore::now_us`
//!   also gives the frame record);
//! - the audio stream's Opus setup becomes `OPUS_MULTISTREAM_CONFIGURATION`,
//!   and a lost packet is `decodeAndPlaySample(NULL, 0)`, as in the C core;
//! - stages, the start, the termination, the log and the host's rumble,
//!   trigger rumble, motion, LED, HDR and connection status go to the
//!   `CONNECTION_LISTENER_CALLBACKS` that are set.
//!
//! The session's settings are nightjar-cli's (its headless Linux client):
//! datagrams grouped by the kernel (`receive_coalescing`) and
//! `receive_decode_priority`, which changes nothing without "prioritize
//! stream traffic" (`network_priority`), left off as the C core raises no
//! thread either. A decoder table without direct submit, as Android's, gets
//! nightjar-core's own decode thread, which nightjar-core always raises on
//! Linux and Android (`priority.rs:447`; on Android with `setpriority` only).
use super::{Connection, NetworkCore, Packet};
use crate::network as ffi;
use anyhow::Result;
use nightjar_core::{
    control::{ConnectionStatus, ControlEvent},
    input::{InputError, InputQueue},
    sdp::{AudioConfiguration, StreamConfig},
    session::{
        AudioRenderer, AudioSetup, DecodeStatus, HostInfo, InputHandle, Interrupt, Listener,
        Location, Session, SessionConfig, Stage, VideoDecoder, VideoSetup,
    },
    video::{BufferType, Frame, FrameType},
};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    ptr::null_mut,
    sync::{Arc, Mutex, PoisonError, RwLock},
};

/// moonlight-common-c's `LI_ERR_UNSUPPORTED`.
const UNSUPPORTED: i32 = -5501;

pub(crate) struct Nightjar {
    session: Mutex<Option<Session>>,
    /// The running session's input queue, for every thread that sends.
    input: RwLock<Option<InputHandle>>,
    /// The start under way, for `interrupt`.
    interrupt: Mutex<Option<Interrupt>>,
}

impl Nightjar {
    pub(crate) const fn new() -> Self {
        Self {
            session: Mutex::new(None),
            input: RwLock::new(None),
            interrupt: Mutex::new(None),
        }
    }
}

impl NetworkCore for Nightjar {
    fn launch_parameters(&self) -> Result<&'static str> {
        Ok(nightjar_core::http::CORE_LAUNCH_PARAMETERS)
    }

    fn start(&self, c: Connection<'_>) -> i32 {
        let interrupt = Interrupt::default();
        *self
            .interrupt
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(interrupt.clone());
        // SAFETY: the caller keeps each table alive until `stop` returns.
        let video = unsafe { c.video.as_ref() }.copied();
        let audio = unsafe { c.audio.as_ref() }.copied();
        let listener = Arc::new(Callbacks(*c.listener));
        // SAFETY: the server's strings are live C strings or null.
        let host = match unsafe { host_info(c.server) } {
            Ok(host) => host,
            Err(error) => {
                listener.log(&format!("nightjar: {error}"));
                listener.stage_failed(Stage::PlatformInit, -1);
                return -1;
            }
        };
        let config = session_config(
            c.stream,
            video.map_or(0, |t| t.capabilities),
            audio.map_or(0, |t| t.capabilities),
        );
        let result = Session::start(
            &host,
            config,
            Box::new(Decoder {
                table: video,
                flags: c.video_flags,
                entries: Vec::new(),
            }),
            Box::new(Audio {
                table: audio,
                context: Context(c.audio_context),
            }),
            listener,
            &interrupt,
        );
        *self
            .interrupt
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
        match result {
            Ok(session) => {
                *self.input.write().unwrap_or_else(PoisonError::into_inner) =
                    Some(session.input_handle());
                *self.session.lock().unwrap_or_else(PoisonError::into_inner) = Some(session);
                0
            }
            // The listener had the stage's failure; a code of 0 is still a failure.
            Err(error) if error.code == 0 => -1,
            Err(error) => error.code,
        }
    }

    fn interrupt(&self) {
        if let Some(interrupt) = &*self
            .interrupt
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
        {
            interrupt.interrupt();
        }
    }

    fn stop(&self) {
        // No input enters the queue after this; a send under way ends first.
        *self.input.write().unwrap_or_else(PoisonError::into_inner) = None;
        let session = self
            .session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(session) = session {
            crate::log::line(&format!("nightjar: at the stop: {:?}", session.stats()));
            let (idr, rfi) = session.with_control(|control| recovery_requests(control));
            crate::log::line(&format!(
                "nightjar: recovery requests sent: {idr} IDR, {rfi} reference frame invalidations"
            ));
            session.stop();
        }
    }

    fn send(&self, packet: Packet) -> i32 {
        match &*self.input.read().unwrap_or_else(PoisonError::into_inner) {
            Some(input) => code(input.input(|queue| queue_packet(queue, packet))),
            // The C core's answer with no stream.
            None => -2,
        }
    }

    fn now_us(&self) -> u64 {
        nightjar_core::session::now_us()
    }

    fn rtt_ms(&self) -> Option<u32> {
        let session = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        let (rtt, _) = session.as_ref()?.rtt()?;
        Some(rtt.as_millis().min(u128::from(u32::MAX)) as u32)
    }

    fn pending_audio_ms(&self) -> i32 {
        let session = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        session
            .as_ref()
            .map_or(0, |s| s.pending_audio_ms().min(i32::MAX as u32) as i32)
    }
}

/// The IDR requests and reference frame invalidations that the control
/// session has sent to the host: the C core's "IDR frame request sent" and
/// "Invalidate reference frame request sent" lines, as counts.
fn recovery_requests(control: &nightjar_core::control::ControlSession) -> (u64, u64) {
    (control.idr_requests_sent(), control.rfi_requests_sent())
}

/// `SERVER_INFORMATION` as nightjar-core's `HostInfo`.
///
/// # Safety
/// Each string of `server` is a live C string or null.
unsafe fn host_info(server: &ffi::SERVER_INFORMATION) -> Result<HostInfo> {
    let text = |p: *const c_char| -> Result<Option<String>> {
        if p.is_null() {
            return Ok(None);
        }
        // SAFETY: the caller's promise.
        Ok(Some(unsafe { CStr::from_ptr(p) }.to_str()?.to_owned()))
    };
    Ok(HostInfo {
        address: text(server.address)?.ok_or_else(|| anyhow::anyhow!("no host address"))?,
        app_version: text(server.serverInfoAppVersion)?
            .ok_or_else(|| anyhow::anyhow!("no host appversion"))?,
        rtsp_session_url: text(server.rtspSessionUrl)?,
        server_codec_mode_support: server.serverCodecModeSupport as u32,
    })
}

/// `STREAM_CONFIGURATION` and the callback tables' capabilities as the
/// session's settings.
fn session_config(
    s: &ffi::STREAM_CONFIGURATION,
    video_capabilities: c_int,
    audio_capabilities: c_int,
) -> SessionConfig {
    let bytes = |chars: [c_char; 16]| chars.map(|c| c as u8);
    SessionConfig {
        stream: StreamConfig {
            width: s.width,
            height: s.height,
            fps: s.fps,
            bitrate_kbps: s.bitrate,
            packet_size: s.packetSize,
            // Limelight.h's CHANNEL_COUNT_FROM_AUDIO_CONFIGURATION and
            // CHANNEL_MASK_FROM_AUDIO_CONFIGURATION.
            audio: AudioConfiguration {
                channel_count: ((s.audioConfiguration >> 8) & 0xff) as u8,
                channel_mask: ((s.audioConfiguration >> 16) & 0xffff) as u16,
            },
            supported_video_formats: s.supportedVideoFormats as u32,
            client_refresh_rate_x100: s.clientRefreshRateX100,
            color_space: s.colorSpace,
            color_range: s.colorRange,
            encryption_flags: s.encryptionFlags as u32,
            remote_input_aes_key: bytes(s.remoteInputAesKey),
            remote_input_aes_iv: bytes(s.remoteInputAesIv),
            video_capabilities: video_capabilities as u32,
            audio_capabilities: audio_capabilities as u32,
            ..Default::default()
        },
        location: match s.streamingRemotely as u32 {
            ffi::STREAM_CFG_LOCAL => Location::Local,
            ffi::STREAM_CFG_REMOTE => Location::Remote,
            _ => Location::Auto,
        },
        receive_decode_priority: true,
        receive_coalescing: true,
        ..Default::default()
    }
}

/// Limelight.h's `MAKE_AUDIO_CONFIGURATION`.
fn audio_configuration(configuration: AudioConfiguration) -> c_int {
    (c_int::from(configuration.channel_mask) << 16)
        | (c_int::from(configuration.channel_count) << 8)
        | 0xCA
}

/// The Opus setup that the C core gives `init`.
fn opus_config(setup: &AudioSetup) -> ffi::OPUS_MULTISTREAM_CONFIGURATION {
    let mut mapping = [0; 8];
    for (to, from) in mapping.iter_mut().zip(setup.opus.mapping) {
        *to = from;
    }
    ffi::OPUS_MULTISTREAM_CONFIGURATION {
        sampleRate: setup.opus.sample_rate as c_int,
        channelCount: c_int::from(setup.opus.channel_count),
        streams: c_int::from(setup.opus.streams),
        coupledStreams: c_int::from(setup.opus.coupled_streams),
        samplesPerFrame: setup.samples_per_frame as c_int,
        mapping,
    }
}

/// One packet into nightjar-core's input queue: the `LiSend*` call that
/// nightjar-core ports.
fn queue_packet(queue: &mut InputQueue, packet: Packet) -> Result<(), InputError> {
    match packet {
        Packet::Keyboard {
            code,
            down,
            modifiers,
            flags,
        } => queue.send_keyboard(code as u16, down, modifiers, flags),
        Packet::Controller(s) => queue.send_controller(
            s.number,
            s.active_mask,
            s.buttons,
            s.left_trigger,
            s.right_trigger,
            (s.left_stick_x, s.left_stick_y),
            (s.right_stick_x, s.right_stick_y),
        ),
        Packet::Arrival {
            number,
            active_mask,
            kind,
            buttons,
            capabilities,
        } => queue.send_controller_arrival(number, active_mask, kind, buttons, capabilities),
        Packet::MouseMove { dx, dy } => queue.send_mouse_move(dx, dy),
        Packet::MousePosition {
            x,
            y,
            width,
            height,
        } => queue.send_mouse_position(x, y, width, height),
        Packet::MouseButton { pressed, button } => queue.send_mouse_button(button, pressed),
        Packet::Scroll { amount } => queue.send_high_res_scroll(amount),
        Packet::HScroll { amount } => queue.send_high_res_hscroll(amount),
        Packet::Touch {
            kind,
            pointer,
            x,
            y,
            pressure,
            contact_major,
            contact_minor,
            rotation,
        } => queue.send_touch(
            kind,
            pointer,
            x,
            y,
            pressure,
            contact_major,
            contact_minor,
            rotation,
        ),
        Packet::ControllerTouch {
            number,
            kind,
            pointer,
            x,
            y,
            pressure,
            // The C core's call names no touchpad: the controller's first.
        } => queue.send_controller_touch(number, kind, 0, pointer, x, y, pressure),
        Packet::ControllerMotion {
            number,
            kind,
            x,
            y,
            z,
        } => queue.send_controller_motion(number, kind, x, y, z),
        Packet::Battery {
            number,
            state,
            percentage,
        } => queue.send_controller_battery(number, state, percentage),
    }
}

/// The C core's return codes for an input result.
fn code(result: Result<(), InputError>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(InputError::Stopped) => -2,
        Err(InputError::Invalid) => -3,
        Err(InputError::Unsupported) => UNSUPPORTED,
        Err(InputError::QueueFull) => -1,
    }
}

/// The platform's decoder table, as nightjar-core's decoder.
struct Decoder {
    table: Option<ffi::DECODER_RENDERER_CALLBACKS>,
    flags: c_int,
    /// One `LENTRY` for each buffer of the frame being submitted.
    entries: Vec<ffi::LENTRY>,
}

// SAFETY: the table holds C function pointers, which any thread may call as
// the C core's threads do; `entries` points into a frame only during
// `submit`, and is rebuilt for each frame.
unsafe impl Send for Decoder {}

impl VideoDecoder for Decoder {
    fn setup(&mut self, s: &VideoSetup) -> Result<(), i32> {
        let Some(setup) = self.table.and_then(|t| t.setup) else {
            return Ok(());
        };
        // SAFETY: the C core's call, with its render context (none here).
        match unsafe {
            setup(
                s.format as c_int,
                s.width,
                s.height,
                s.fps,
                null_mut(),
                self.flags,
            )
        } {
            0 => Ok(()),
            error => Err(error),
        }
    }

    fn start(&mut self) {
        if let Some(start) = self.table.and_then(|t| t.start) {
            // SAFETY: after a setup that succeeded, as the C core calls it.
            unsafe { start() }
        }
    }

    fn submit(&mut self, frame: &Frame) -> DecodeStatus {
        let Some(submit) = self.table.and_then(|t| t.submitDecodeUnit) else {
            return DecodeStatus::Ok;
        };
        let Some(mut unit) = decode_unit(frame, &mut self.entries) else {
            return DecodeStatus::Ok;
        };
        // SAFETY: the unit and its entries point into `frame`, which lives
        // for this call; the C decoders copy what they keep.
        if unsafe { submit(&mut unit) } == ffi::DR_NEED_IDR {
            DecodeStatus::NeedIdr
        } else {
            DecodeStatus::Ok
        }
    }

    fn stop(&mut self) {
        if let Some(stop) = self.table.and_then(|t| t.stop) {
            // SAFETY: after the receive and decode threads ended.
            unsafe { stop() }
        }
    }

    fn cleanup(&mut self) {
        if let Some(cleanup) = self.table.and_then(|t| t.cleanup) {
            // SAFETY: last, as the C core calls it.
            unsafe { cleanup() }
        }
    }
}

/// `frame` as the C core's decode unit, its buffers chained in `entries`;
/// none for a frame with no buffer (the chain is never empty).
fn decode_unit(frame: &Frame, entries: &mut Vec<ffi::LENTRY>) -> Option<ffi::DECODE_UNIT> {
    chain(frame.buffer_slices(), entries)?;
    let full_length = entries.iter().map(|e| e.length).sum();
    Some(ffi::DECODE_UNIT {
        frameNumber: frame.number as c_int,
        frameType: match frame.frame_type {
            FrameType::Idr => ffi::FRAME_TYPE_IDR,
            FrameType::P => ffi::FRAME_TYPE_PFRAME,
        } as c_int,
        frameHostProcessingLatency: frame.host_processing_latency,
        receiveTimeUs: frame.receive_time_us,
        enqueueTimeUs: frame.enqueue_time_us,
        presentationTimeUs: frame.presentation_time_us,
        rtpTimestamp: frame.rtp_timestamp,
        fullLength: full_length,
        bufferList: entries.as_mut_ptr(),
        hdrActive: frame.hdr_active,
        colorspace: frame.colorspace,
    })
}

/// One `LENTRY` for each buffer, in order, each linked to the next.
fn chain<'a>(
    buffers: impl Iterator<Item = (BufferType, &'a [u8])>,
    entries: &mut Vec<ffi::LENTRY>,
) -> Option<()> {
    entries.clear();
    entries.extend(buffers.map(|(kind, bytes)| ffi::LENTRY {
        next: null_mut(),
        data: bytes.as_ptr().cast_mut().cast(),
        length: bytes.len() as c_int,
        bufferType: kind as c_int,
    }));
    if entries.is_empty() {
        return None;
    }
    // Linked only once the vector no longer grows.
    let base = entries.as_mut_ptr();
    for i in 1..entries.len() {
        // SAFETY: both indexes are inside the vector.
        unsafe { (*base.add(i - 1)).next = base.add(i) };
    }
    Some(())
}

/// The audio table's context: Linux's audio device name, or null.
struct Context(*mut c_void);

// SAFETY: the context is an immutable C string that outlives the stream.
unsafe impl Send for Context {}

/// The platform's audio table, as nightjar-core's audio renderer.
struct Audio {
    table: Option<ffi::AUDIO_RENDERER_CALLBACKS>,
    context: Context,
}

impl AudioRenderer for Audio {
    fn init(&mut self, setup: &AudioSetup) -> Result<(), i32> {
        // The platform decodes Opus only; nothing else is asked for.
        if setup.codec != rubylight_protocol::audio_ext::CODEC_OPUS {
            return Err(-1);
        }
        let Some(init) = self.table.and_then(|t| t.init) else {
            return Ok(());
        };
        let mut opus = opus_config(setup);
        // SAFETY: the C core's call; `opus` lives for it.
        match unsafe {
            init(
                audio_configuration(setup.configuration),
                &mut opus,
                self.context.0,
                0,
            )
        } {
            0 => Ok(()),
            error => Err(error),
        }
    }

    fn start(&mut self) {
        if let Some(start) = self.table.and_then(|t| t.start) {
            // SAFETY: after a successful init.
            unsafe { start() }
        }
    }

    fn play(&mut self, packet: Option<&[u8]>) {
        let Some(play) = self.table.and_then(|t| t.decodeAndPlaySample) else {
            return;
        };
        let (data, length) = match packet {
            Some(bytes) => (bytes.as_ptr().cast_mut().cast(), bytes.len() as c_int),
            // A lost packet: the decoder conceals it, as from the C core.
            None => (null_mut(), 0),
        };
        // SAFETY: the packet lives for this call; the decoder copies it.
        unsafe { play(data, length) }
    }

    fn stop(&mut self) {
        if let Some(stop) = self.table.and_then(|t| t.stop) {
            // SAFETY: after the audio threads ended.
            unsafe { stop() }
        }
    }

    fn cleanup(&mut self) {
        if let Some(cleanup) = self.table.and_then(|t| t.cleanup) {
            // SAFETY: last.
            unsafe { cleanup() }
        }
    }
}

/// The platform's listener table, as nightjar-core's listener.
struct Callbacks(ffi::CONNECTION_LISTENER_CALLBACKS);

impl Listener for Callbacks {
    fn stage_starting(&self, stage: Stage) {
        if let Some(f) = self.0.stageStarting {
            // SAFETY: the C core's call.
            unsafe { f(stage as c_int) }
        }
    }

    fn stage_complete(&self, stage: Stage) {
        if let Some(f) = self.0.stageComplete {
            // SAFETY: the C core's call.
            unsafe { f(stage as c_int) }
        }
    }

    fn stage_failed(&self, stage: Stage, error: i32) {
        if let Some(f) = self.0.stageFailed {
            // SAFETY: the C core's call.
            unsafe { f(stage as c_int, error) }
        }
    }

    fn connection_started(&self) {
        if let Some(f) = self.0.connectionStarted {
            // SAFETY: the C core's call.
            unsafe { f() }
        }
    }

    fn connection_terminated(&self, error: i32) {
        if let Some(f) = self.0.connectionTerminated {
            // SAFETY: once, on a thread of its own, as nightjar-core promises.
            unsafe { f(error) }
        }
    }

    fn control_event(&self, event: ControlEvent) {
        let table = &self.0;
        // SAFETY: each is the C core's call for the same host message.
        unsafe {
            match event {
                ControlEvent::Rumble {
                    controller,
                    low_frequency,
                    high_frequency,
                } => {
                    if let Some(f) = table.rumble {
                        f(controller, low_frequency, high_frequency)
                    }
                }
                ControlEvent::RumbleTriggers {
                    controller,
                    left,
                    right,
                } => {
                    if let Some(f) = table.rumbleTriggers {
                        f(controller, left, right)
                    }
                }
                ControlEvent::SetMotionEventState {
                    controller,
                    motion_type,
                    report_rate_hz,
                } => {
                    if let Some(f) = table.setMotionEventState {
                        f(controller, motion_type, report_rate_hz)
                    }
                }
                ControlEvent::SetControllerLed {
                    controller,
                    r,
                    g,
                    b,
                } => {
                    if let Some(f) = table.setControllerLED {
                        f(controller, r, g, b)
                    }
                }
                ControlEvent::AdaptiveTriggers {
                    controller,
                    event_flags,
                    type_left,
                    type_right,
                    mut left,
                    mut right,
                } => {
                    if let Some(f) = table.setAdaptiveTriggers {
                        f(
                            controller,
                            event_flags,
                            type_left,
                            type_right,
                            left.as_mut_ptr(),
                            right.as_mut_ptr(),
                        )
                    }
                }
                ControlEvent::HdrMode { enabled, .. } => {
                    if let Some(f) = table.setHdrMode {
                        f(enabled)
                    }
                }
                ControlEvent::ConnectionStatus(status) => {
                    if let Some(f) = table.connectionStatusUpdate {
                        f(match status {
                            ConnectionStatus::Okay => ffi::CONN_STATUS_OKAY,
                            ConnectionStatus::Poor => ffi::CONN_STATUS_POOR,
                        } as c_int)
                    }
                }
                // The C core's listener has no others; the termination
                // comes through `connection_terminated`.
                _ => {}
            }
        }
    }

    fn log(&self, message: &str) {
        let Some(f) = self.0.logMessage else {
            return;
        };
        let Ok(line) = CString::new(format!("{}\n", message.replace('\0', ""))) else {
            return;
        };
        // SAFETY: a C-variadic log callback, given a format with one string.
        unsafe { f(c"%s".as_ptr(), line.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nightjar_core::control::HostCapabilities;
    use std::{
        sync::Mutex as StdMutex,
        time::{Duration, Instant},
    };

    #[test]
    fn the_stream_configuration_carries_every_field_to_the_session() {
        let mut s = ffi::STREAM_CONFIGURATION {
            width: 1920,
            height: 1080,
            fps: 60,
            bitrate: 10_000,
            packetSize: 1392,
            streamingRemotely: ffi::STREAM_CFG_AUTO as c_int,
            // 5.1 as stream.rs makes it: mask 0x3f, 6 channels.
            audioConfiguration: (0x3f << 16) | (6 << 8) | 0xCA,
            supportedVideoFormats: (ffi::VIDEO_FORMAT_H264 | ffi::VIDEO_FORMAT_H265) as c_int,
            clientRefreshRateX100: 6000,
            colorSpace: 1,
            colorRange: 0,
            encryptionFlags: ffi::ENCFLG_ALL as c_int,
            ..Default::default()
        };
        for (i, b) in s.remoteInputAesKey.iter_mut().enumerate() {
            *b = (0xf0 + i as u8) as c_char;
        }
        // c_char is unsigned on aarch64 Android: the byte, not -4.
        s.remoteInputAesIv[..4].copy_from_slice(&[1, 2, 3, 0xfc_u8 as c_char]);
        let config = session_config(&s, 0x0300_0003, 0x5);
        let stream = &config.stream;
        assert_eq!((stream.width, stream.height, stream.fps), (1920, 1080, 60));
        assert_eq!((stream.bitrate_kbps, stream.packet_size), (10_000, 1392));
        assert_eq!(config.location, Location::Auto);
        assert_eq!(
            stream.audio,
            AudioConfiguration {
                channel_count: 6,
                channel_mask: 0x3f
            }
        );
        assert_eq!(audio_configuration(stream.audio), s.audioConfiguration);
        assert_eq!(
            stream.supported_video_formats,
            ffi::VIDEO_FORMAT_H264 | ffi::VIDEO_FORMAT_H265
        );
        assert_eq!(stream.client_refresh_rate_x100, 6000);
        assert_eq!((stream.color_space, stream.color_range), (1, 0));
        assert_eq!(stream.encryption_flags, ffi::ENCFLG_ALL);
        assert_eq!(stream.remote_input_aes_key[0], 0xf0);
        assert_eq!(stream.remote_input_aes_key[15], 0xff);
        assert_eq!(stream.remote_input_aes_iv[..5], [1, 2, 3, 0xfc, 0]);
        assert_eq!(
            (stream.video_capabilities, stream.audio_capabilities),
            (0x0300_0003, 0x5)
        );
        assert!(config.receive_coalescing && !config.network_priority);
        for (remote, location) in [
            (ffi::STREAM_CFG_LOCAL, Location::Local),
            (ffi::STREAM_CFG_REMOTE, Location::Remote),
        ] {
            s.streamingRemotely = remote as c_int;
            assert_eq!(session_config(&s, 0, 0).location, location);
        }
    }

    #[test]
    fn a_frame_is_one_entry_for_each_buffer_in_order() {
        let sps = [0, 0, 0, 1, 0x67, 1];
        let pps = [0, 0, 0, 1, 0x68];
        let picture = [0, 0, 0, 1, 0x65, 9, 9, 9];
        let mut entries = Vec::new();
        chain(
            [
                (BufferType::Sps, &sps[..]),
                (BufferType::Pps, &pps[..]),
                (BufferType::PicData, &picture[..]),
            ]
            .into_iter(),
            &mut entries,
        )
        .unwrap();
        let mut seen = Vec::new();
        let mut entry: *const ffi::LENTRY = entries.as_ptr();
        while !entry.is_null() {
            // SAFETY: the chain points into `entries` and the three arrays.
            let e = unsafe { &*entry };
            let bytes =
                unsafe { std::slice::from_raw_parts(e.data.cast::<u8>(), e.length as usize) };
            seen.push((e.bufferType, bytes.to_vec()));
            entry = e.next;
        }
        assert_eq!(
            seen,
            [
                (ffi::BUFFER_TYPE_SPS as c_int, sps.to_vec()),
                (ffi::BUFFER_TYPE_PPS as c_int, pps.to_vec()),
                (ffi::BUFFER_TYPE_PICDATA as c_int, picture.to_vec()),
            ]
        );
        assert!(chain(std::iter::empty(), &mut entries).is_none());
    }

    #[test]
    fn the_opus_setup_is_the_one_the_c_core_gives_init() {
        let setup = AudioSetup {
            configuration: AudioConfiguration::SURROUND_51,
            opus: nightjar_core::rtsp::OpusConfig {
                sample_rate: 48_000,
                channel_count: 6,
                streams: 4,
                coupled_streams: 2,
                mapping: [0, 4, 1, 5, 2, 3, 0, 0, 0, 0, 0, 0],
            },
            codec: rubylight_protocol::audio_ext::CODEC_OPUS,
            packet_duration_ms: 5,
            samples_per_frame: 240,
        };
        let opus = opus_config(&setup);
        assert_eq!(
            (
                opus.sampleRate,
                opus.channelCount,
                opus.streams,
                opus.coupledStreams,
                opus.samplesPerFrame
            ),
            (48_000, 6, 4, 2, 240)
        );
        assert_eq!(opus.mapping, [0, 4, 1, 5, 2, 3, 0, 0]);
        assert_eq!(
            audio_configuration(setup.configuration),
            (0x3f << 16) | (6 << 8) | 0xCA
        );
    }

    /// A Sunshine host's queue: the same packets go out whether a packet
    /// comes through the seam or straight from nightjar-core's API.
    #[test]
    fn each_packet_is_the_matching_call_of_the_input_queue() {
        let caps = HostCapabilities {
            app_version: [7, 1, 431, -1],
            feature_flags: 0xff,
            encryption_enabled: 0,
            control_messages: Vec::new(),
            client_roaming: false,
        };
        type Call = dyn Fn(&mut InputQueue) -> Result<(), InputError>;
        let drained = |send: &Call| {
            let mut queue = InputQueue::new(&caps);
            queue.set_unbatched(true);
            let result = send(&mut queue);
            let mut out = Vec::new();
            queue.drain(Instant::now() + Duration::from_secs(1), false, &mut out);
            (code(result), out)
        };
        let state = crate::network_core::ControllerState {
            number: 1,
            active_mask: 3,
            buttons: 0x1000,
            left_trigger: 200,
            right_trigger: 0,
            left_stick_x: -100,
            left_stick_y: 100,
            right_stick_x: 7,
            right_stick_y: -7,
        };
        let cases: [(Packet, &Call); 6] = [
            (
                Packet::Keyboard {
                    code: -32_704,
                    down: true,
                    modifiers: 1,
                    flags: 0,
                },
                &|q| q.send_keyboard(0x8040, true, 1, 0),
            ),
            (Packet::Controller(state), &|q| {
                q.send_controller(1, 3, 0x1000, 200, 0, (-100, 100), (7, -7))
            }),
            (
                Packet::MouseButton {
                    pressed: false,
                    button: 3,
                },
                &|q| q.send_mouse_button(3, false),
            ),
            (Packet::Scroll { amount: -240 }, &|q| {
                q.send_high_res_scroll(-240)
            }),
            (Packet::MouseMove { dx: 5, dy: -6 }, &|q| {
                q.send_mouse_move(5, -6)
            }),
            (
                Packet::Battery {
                    number: 0,
                    state: 3,
                    percentage: 60,
                },
                &|q| q.send_controller_battery(0, 3, 60),
            ),
        ];
        for (packet, direct) in cases {
            let through_seam = drained(&move |q| queue_packet(q, packet));
            assert!(!through_seam.1.is_empty(), "{packet:?} sent nothing");
            assert_eq!(through_seam, drained(direct), "{packet:?}");
        }
        assert_eq!(code(Err(InputError::Unsupported)), -5501);
        assert_eq!(code(Err(InputError::Stopped)), -2);
    }

    /// The stop line's recovery counts (Korri's edit to the vendored
    /// control session, vendor/README.md): one for each IDR request and
    /// each reference frame invalidation that goes to the host, as the C
    /// core logs one line for each ("IDR frame request sent", "Invalidate
    /// reference frame request sent").
    #[test]
    fn each_idr_request_and_invalidation_sent_is_counted_once() {
        use nightjar_core::control::{ControlSession, SessionOptions, negotiation::encryption};
        let caps = HostCapabilities {
            app_version: [7, 1, 431, -1],
            feature_flags: 0,
            encryption_enabled: encryption::CONTROL_V2,
            control_messages: Vec::new(),
            client_roaming: false,
        };
        let options = SessionOptions {
            reference_frame_invalidation: true,
            ..Default::default()
        };
        let mut control = ControlSession::new(caps, b"0011223344556677", options);
        let now = Instant::now();
        control.start(now);
        control.poll(now);
        assert_eq!(recovery_requests(&control), (0, 0));
        // Two losses before the next poll go as one invalidation.
        control.frame_loss(10, 12);
        control.frame_loss(13, 13);
        control.poll(now);
        assert_eq!(recovery_requests(&control), (0, 1));
        control.request_idr_frame();
        control.request_idr_frame();
        control.poll(now);
        assert_eq!(recovery_requests(&control), (1, 1));
        // Nothing waits: no request goes.
        control.poll(now);
        assert_eq!(recovery_requests(&control), (1, 1));
    }

    // The platform's tables as Android's are made: Rust `extern "C"`
    // functions behind the C core's callback tables (src/android/picture.rs,
    // sound.rs). Each records its call.
    static PLATFORM: StdMutex<Vec<String>> = StdMutex::new(Vec::new());
    /// The platform tests take turns: they share `PLATFORM`.
    static PLATFORM_TURN: StdMutex<()> = StdMutex::new(());

    fn platform(call: String) {
        PLATFORM.lock().unwrap().push(call);
    }
    unsafe extern "C" fn video_setup(
        format: c_int,
        width: c_int,
        height: c_int,
        fps: c_int,
        context: *mut c_void,
        flags: c_int,
    ) -> c_int {
        platform(format!(
            "setup {format:#x} {width}x{height}@{fps} context {} flags {flags}",
            context.is_null()
        ));
        if width == 0 { -7 } else { 0 }
    }
    unsafe extern "C" fn video_start() {
        platform("video start".into());
    }
    unsafe extern "C" fn video_stop() {
        platform("video stop".into());
    }
    unsafe extern "C" fn video_cleanup() {
        platform("video cleanup".into());
    }
    unsafe extern "C" fn audio_init(
        configuration: c_int,
        opus: *mut ffi::OPUS_MULTISTREAM_CONFIGURATION,
        context: *mut c_void,
        flags: c_int,
    ) -> c_int {
        // SAFETY: the adapter's configuration, live for this call.
        let opus = unsafe { &*opus };
        platform(format!(
            "init {configuration:#x} {} Hz {} channels {}/{} streams {} samples {:?} context {} flags {flags}",
            opus.sampleRate,
            opus.channelCount,
            opus.coupledStreams,
            opus.streams,
            opus.samplesPerFrame,
            opus.mapping,
            context as usize
        ));
        0
    }
    unsafe extern "C" fn audio_start() {
        platform("audio start".into());
    }
    unsafe extern "C" fn audio_play(data: *mut c_char, length: c_int) {
        let bytes = if data.is_null() {
            None
        } else {
            // SAFETY: the packet of `length` bytes, live for this call.
            Some(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), length as usize) }.to_vec())
        };
        platform(format!("play {bytes:?} {length}"));
    }
    unsafe extern "C" fn audio_stop() {
        platform("audio stop".into());
    }
    unsafe extern "C" fn audio_cleanup() {
        platform("audio cleanup".into());
    }

    /// A decoder table like Android's: no direct submit, so nightjar-core
    /// runs its own decode thread, and the table's calls come in the C
    /// core's order.
    #[test]
    fn the_platforms_decoder_table_is_called_as_the_c_core_calls_it() {
        let _turn = PLATFORM_TURN.lock().unwrap_or_else(|p| p.into_inner());
        PLATFORM.lock().unwrap().clear();
        let mut decoder = Decoder {
            table: Some(ffi::DECODER_RENDERER_CALLBACKS {
                setup: Some(video_setup),
                start: Some(video_start),
                stop: Some(video_stop),
                cleanup: Some(video_cleanup),
                submitDecodeUnit: None,
                capabilities: 0,
            }),
            flags: 8,
            entries: Vec::new(),
        };
        let mut setup = VideoSetup {
            format: ffi::VIDEO_FORMAT_H264,
            width: 1280,
            height: 720,
            fps: 60,
            chroma_packing: 0,
            logical_width: 1280,
            logical_height: 720,
            color_space: 1,
            color_range: 0,
        };
        assert_eq!(decoder.setup(&setup), Ok(()));
        decoder.start();
        // A table without its submit takes every frame, as the C core's
        // empty decoder does.
        assert_eq!(decoder.submit(&Frame::default()), DecodeStatus::Ok);
        decoder.stop();
        decoder.cleanup();
        setup.width = 0;
        assert_eq!(
            decoder.setup(&setup),
            Err(-7),
            "the platform's error is the stage's"
        );
        assert_eq!(
            *PLATFORM.lock().unwrap(),
            [
                "setup 0x1 1280x720@60 context true flags 8",
                "video start",
                "video stop",
                "video cleanup",
                "setup 0x1 0x720@60 context true flags 8",
            ]
        );
        // The session tests' null table: nothing to call, nothing fails.
        let mut none = Decoder {
            table: None,
            flags: 0,
            entries: Vec::new(),
        };
        assert_eq!(none.setup(&setup), Ok(()));
        assert_eq!(none.submit(&Frame::default()), DecodeStatus::Ok);
    }

    /// An audio table like Android's sound: init with the Opus setup and
    /// the context, each packet in order, and a lost one as no data.
    #[test]
    fn the_platforms_audio_table_gets_each_packet_and_each_loss() {
        let _turn = PLATFORM_TURN.lock().unwrap_or_else(|p| p.into_inner());
        PLATFORM.lock().unwrap().clear();
        let mut audio = Audio {
            table: Some(ffi::AUDIO_RENDERER_CALLBACKS {
                init: Some(audio_init),
                start: Some(audio_start),
                stop: Some(audio_stop),
                cleanup: Some(audio_cleanup),
                decodeAndPlaySample: Some(audio_play),
                capabilities: ffi::CAPABILITY_SUPPORTS_ARBITRARY_AUDIO_DURATION as c_int,
            }),
            context: Context(0x1234 as *mut c_void),
        };
        let mut setup = AudioSetup {
            configuration: AudioConfiguration::STEREO,
            opus: nightjar_core::rtsp::OpusConfig {
                sample_rate: 48_000,
                channel_count: 2,
                streams: 1,
                coupled_streams: 1,
                mapping: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            },
            codec: rubylight_protocol::audio_ext::CODEC_OPUS,
            packet_duration_ms: 5,
            samples_per_frame: 240,
        };
        assert_eq!(audio.init(&setup), Ok(()));
        audio.start();
        audio.play(Some(&[1, 2, 3]));
        audio.play(None);
        audio.stop();
        audio.cleanup();
        // Only Opus reaches a platform that decodes Opus.
        setup.codec = rubylight_protocol::audio_ext::CODEC_PCM;
        assert!(audio.init(&setup).is_err());
        assert_eq!(
            *PLATFORM.lock().unwrap(),
            [
                "init 0x302ca 48000 Hz 2 channels 1/1 streams 240 samples [0, 1, 0, 0, 0, 0, 0, 0] context 4660 flags 0",
                "audio start",
                "play Some([1, 2, 3]) 3",
                "play None 0",
                "audio stop",
                "audio cleanup",
            ]
        );
    }

    static EVENTS: StdMutex<Vec<String>> = StdMutex::new(Vec::new());

    unsafe extern "C" fn rumble(controller: u16, low: u16, high: u16) {
        EVENTS
            .lock()
            .unwrap()
            .push(format!("rumble {controller} {low} {high}"));
    }
    unsafe extern "C" fn led(controller: u16, r: u8, g: u8, b: u8) {
        EVENTS
            .lock()
            .unwrap()
            .push(format!("led {controller} {r} {g} {b}"));
    }
    unsafe extern "C" fn status(status: c_int) {
        EVENTS.lock().unwrap().push(format!("status {status}"));
    }
    unsafe extern "C" fn terminated(error: c_int) {
        EVENTS.lock().unwrap().push(format!("terminated {error}"));
    }
    unsafe extern "C" fn failed(stage: c_int, error: c_int) {
        EVENTS
            .lock()
            .unwrap()
            .push(format!("failed {stage} {error}"));
    }

    #[test]
    fn the_hosts_events_reach_the_platforms_callbacks_that_are_set() {
        let callbacks = Callbacks(ffi::CONNECTION_LISTENER_CALLBACKS {
            rumble: Some(rumble),
            setControllerLED: Some(led),
            connectionStatusUpdate: Some(status),
            connectionTerminated: Some(terminated),
            stageFailed: Some(failed),
            ..Default::default()
        });
        EVENTS.lock().unwrap().clear();
        callbacks.control_event(ControlEvent::Rumble {
            controller: 1,
            low_frequency: 0x4000,
            high_frequency: 0,
        });
        // No trigger rumble callback is set: nothing happens.
        callbacks.control_event(ControlEvent::RumbleTriggers {
            controller: 1,
            left: 1,
            right: 1,
        });
        callbacks.control_event(ControlEvent::SetControllerLed {
            controller: 0,
            r: 255,
            g: 0,
            b: 7,
        });
        callbacks.control_event(ControlEvent::ConnectionStatus(ConnectionStatus::Poor));
        callbacks.stage_failed(Stage::RtspHandshake, 403);
        callbacks.connection_terminated(-100);
        callbacks.log("no log callback is set");
        assert_eq!(
            *EVENTS.lock().unwrap(),
            [
                "rumble 1 16384 0",
                "led 0 255 0 7",
                "status 1",
                "failed 4 403",
                "terminated -100"
            ]
        );
    }
}
