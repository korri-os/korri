// SPDX-License-Identifier: GPL-3.0-or-later
#[cfg(not(target_os = "android"))]
use crate::frame_times::FrameTimes;
use crate::{
    network as ffi,
    network_core::{self, Connection, NetworkCore},
    options::Options,
    pair::hex,
    player_io::{Platform, PlayerIo},
    session::{Transport, app_id, field, required},
};
use anyhow::{Result, ensure};
use std::{
    ffi::CString,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
    time::Duration,
};

/// The stream's lifecycle, the size it asked for, and whether its picture
/// follows a new stream size (#476), for the core's callbacks.
static LIFECYCLE: Mutex<
    Option<(
        crate::lifecycle::Lifecycle,
        korri_viewer_treaty::ViewerDisplay,
        bool,
    )>,
> = Mutex::new(None);
unsafe extern "C" fn connection_started() {
    crate::callback::guarded("connection started", (), || {
        if let Some((lifecycle, display, follows)) = &*LIFECYCLE.lock().unwrap()
            && lifecycle.connected(display.clone(), *follows).is_err()
        {
            STOP.store(true, Ordering::Relaxed);
        }
    });
}
struct LifecycleGuard;
impl Drop for LifecycleGuard {
    fn drop(&mut self) {
        *LIFECYCLE.lock().unwrap() = None;
    }
}
static CONNECTION: Mutex<()> = Mutex::new(());
static STOP: AtomicBool = AtomicBool::new(false);
static TERMINATED: AtomicI32 = AtomicI32::new(0);
#[cfg(not(target_os = "android"))]
extern "C" fn signal_stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}
unsafe extern "C" fn terminated(code: i32) {
    crate::callback::guarded("connection terminated", (), || {
        if let Some((lifecycle, _, _)) = &*LIFECYCLE.lock().unwrap() {
            lifecycle.disconnected();
        }
    });
    TERMINATED.store(code, Ordering::Relaxed);
    STOP.store(true, Ordering::Relaxed);
}
unsafe extern "C" fn stage_failed(stage: i32, error: i32) {
    crate::callback::guarded("stage failed", (), || {
        eprintln!("connection stage {stage} failed: {error}");
    });
}
/// A core's start blocks for its whole handshake. Interrupted, the C core
/// ends it at its own points: before each RTSP exchange, between RTSP connect
/// retries, and in its ENet waits. One TCP connect or RTSP exchange can still
/// take its full timeout.
fn interrupt_start_on_stop(
    core: &dyn NetworkCore,
    lifecycle: &crate::lifecycle::Lifecycle,
    returned: &AtomicBool,
) {
    while !returned.load(Ordering::Relaxed) {
        if lifecycle.stop_requested() {
            // The C core clears the flag as its start begins: repeat it.
            core.interrupt();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A stream with no lifecycle. On Android it has no picture: the core runs
/// its own empty decoder.
#[allow(
    clippy::default_constructed_unit_structs,
    reason = "Platform is a unit on Linux only; Android's is an Option"
)]
pub fn run(t: &mut Transport, o: &Options) -> Result<()> {
    run_with_lifecycle(t, o, None, Platform::default())
}
pub(crate) fn run_with_lifecycle(
    t: &mut Transport,
    o: &Options,
    lifecycle: Option<&crate::lifecycle::Lifecycle>,
    platform: Platform,
) -> Result<()> {
    let _connection = CONNECTION
        .try_lock()
        .map_err(|_| anyhow::anyhow!("a stream connection is already active"))?;
    STOP.store(false, Ordering::Relaxed);
    TERMINATED.store(0, Ordering::Relaxed);
    // From here every input packet and reading goes to this stream's core.
    let core = network_core::select(o.network_core);
    // Between the connection steps: a stop since Start ends the launch here.
    let running = || lifecycle.map_or(Ok(()), crate::lifecycle::Lifecycle::ensure_running);
    running()?;
    let info = t.get(true, "serverinfo", &[])?;
    ensure!(
        required(&info, "PairStatus")? == "1",
        "client is not paired"
    );
    crate::marks::mark("paired");
    let app = app_id(&t.get(true, "applist", &[])?, &o.app)?;
    running()?;
    #[cfg_attr(not(target_os = "android"), allow(unused_mut))]
    let mut io = PlayerIo::new(o, platform)?;
    crate::marks::mark("picture and input ready");
    // Before the launch: a record that cannot be created stops nothing that ran.
    #[cfg(not(target_os = "android"))]
    let frame_times = FrameTimes::from_environment()?;
    let app_version = CString::new(required(&info, "appversion")?)?;
    let gfe_version = CString::new(field(&info, "GfeVersion")?.unwrap_or_default())?;
    let host = CString::new(o.host.as_str())?;
    let codec_support: i32 = field(&info, "ServerCodecModeSupport")?
        .unwrap_or_else(|| "1".into())
        .parse()?;
    let mut config = ffi::STREAM_CONFIGURATION::default();
    // SAFETY: initialized POD; the core writes only the supplied structure.
    unsafe {
        ffi::LiInitializeStreamConfiguration(&mut config);
    }
    config.width = o.width;
    config.height = o.height;
    config.fps = o.fps;
    config.bitrate = o.bitrate;
    config.packetSize = o.packet_size;
    config.streamingRemotely = o.remote;
    // Limelight.h MAKE_AUDIO_CONFIGURATION(channel_count, channel_mask).
    let channel_mask = match o.audio_channels {
        6 => 0x3f,
        8 => 0x63f,
        _ => 3,
    };
    config.audioConfiguration = (channel_mask << 16) | (o.audio_channels << 8) | 0xCA;
    config.supportedVideoFormats = match o.codec.as_str() {
        "h265" => ffi::VIDEO_FORMAT_H264 | ffi::VIDEO_FORMAT_H265,
        "av1" => ffi::VIDEO_FORMAT_H264 | ffi::VIDEO_FORMAT_AV1_MAIN8,
        "auto" if matches!(o.platform.as_str(), "x11_vaapi" | "v4l2m2m") => {
            ffi::VIDEO_FORMAT_H264 | ffi::VIDEO_FORMAT_H265
        }
        _ => ffi::VIDEO_FORMAT_H264,
    } as i32;
    if o.hdr {
        ensure!(
            config.supportedVideoFormats != ffi::VIDEO_FORMAT_H264 as i32,
            "HDR streaming requires HEVC or AV1 codec"
        );
        if config.supportedVideoFormats & ffi::VIDEO_FORMAT_H265 as i32 != 0 {
            config.supportedVideoFormats |= ffi::VIDEO_FORMAT_H265_MAIN10 as i32;
        }
        if config.supportedVideoFormats & ffi::VIDEO_FORMAT_AV1_MAIN8 as i32 != 0 {
            config.supportedVideoFormats |= ffi::VIDEO_FORMAT_AV1_MAIN10 as i32;
        }
    }
    if !o.allow_unsupported {
        let doc = roxmltree::Document::parse(&info)?;
        ensure!(
            doc.descendants()
                .filter(|n| n.has_tag_name("DisplayMode"))
                .any(|mode| {
                    [
                        ("Width", o.width),
                        ("Height", o.height),
                        ("RefreshRate", o.fps),
                    ]
                    .iter()
                    .all(|(name, value)| {
                        mode.children().any(|n| {
                            n.has_tag_name(*name)
                                && n.text().and_then(|s| s.parse::<i32>().ok()) == Some(*value)
                        })
                    })
                }),
            "server does not support requested display mode"
        );
    }
    // Match Moonlight's has_fast_aes decision (src/cpu.c). It never reports
    // slow AES on these architectures, so the fallback keeps audio encrypted.
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let fast_aes = std::arch::is_x86_feature_detected!("aes");
    // On aarch64 Moonlight reads the AES bit of the auxiliary vector.
    #[cfg(target_arch = "aarch64")]
    // SAFETY: getauxval has no preconditions.
    let fast_aes = unsafe { libc::getauxval(libc::AT_HWCAP) } & libc::HWCAP_AES != 0;
    config.encryptionFlags = if fast_aes {
        ffi::ENCFLG_ALL
    } else {
        ffi::ENCFLG_AUDIO
    } as i32;
    let mut key = [0u8; 16];
    let mut iv = [0u8; 4];
    openssl::rand::rand_bytes(&mut key)?;
    openssl::rand::rand_bytes(&mut iv)?;
    for (to, from) in config.remoteInputAesKey.iter_mut().zip(key) {
        *to = from as libc::c_char;
    }
    for (to, from) in config.remoteInputAesIv.iter_mut().zip(iv) {
        *to = from as libc::c_char;
    }
    let current: u32 = field(&info, "currentgame")?
        .unwrap_or_else(|| "0".into())
        .parse()?;
    let busy = required(&info, "state")?.contains("_SERVER_BUSY") && current != 0;
    let endpoint = if busy { "resume" } else { "launch" };
    let extra = core.launch_parameters()?;
    let mut params = vec![
        ("appid", app.to_string()),
        ("mode", format!("{}x{}x{}", o.width, o.height, o.fps)),
        ("additionalStates", "1".into()),
        ("sops", i32::from(o.sops).to_string()),
        ("rikey", hex(&key)),
        ("rikeyid", u32::from_be_bytes(iv).to_string()),
        ("localAudioPlayMode", i32::from(o.local_audio).to_string()),
        (
            "surroundAudioInfo",
            ((channel_mask << 16) | o.audio_channels).to_string(),
        ),
        ("remoteControllersBitmap", io.gamepad_mask().to_string()),
        ("gcmap", io.gamepad_mask().to_string()),
    ];
    for part in extra
        .trim_start_matches('&')
        .split('&')
        .filter(|s| !s.is_empty())
    {
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid core launch parameters"))?;
        params.push((key, value.into()));
    }
    if o.hdr {
        params.extend([
            ("hdrMode", "1".into()),
            ("clientHdrCapVersion", "0".into()),
            ("clientHdrCapSupportedFlagsInUint32", "0".into()),
            ("clientHdrCapMetaDataId", "NV_STATIC_METADATA_TYPE_1".into()),
            ("clientHdrCapDisplayData", "0x0x0x0x0x0x0x0x0x0x0".into()),
        ]);
    }
    // After a stop the host must not start the application for nobody.
    running()?;
    let launched = t.get(true, endpoint, &params)?;
    let session = field(&launched, "gamesession")?
        .or(field(&launched, "resume")?)
        .ok_or_else(|| anyhow::anyhow!("missing launch result"))?;
    ensure!(session != "0", "stream launch refused");
    let rtsp = CString::new(required(&launched, "sessionUrl0")?)?;
    let mut server = ffi::SERVER_INFORMATION::default();
    // SAFETY: every CString and callback/configuration object below remains
    // live until the core's stop joins all its threads.
    unsafe {
        ffi::LiInitializeServerInformation(&mut server);
    }
    server.address = host.as_ptr();
    server.serverInfoAppVersion = app_version.as_ptr();
    server.serverInfoGfeVersion = gfe_version.as_ptr();
    server.rtspSessionUrl = rtsp.as_ptr();
    server.serverCodecModeSupport = codec_support;
    let mut callbacks = ffi::CONNECTION_LISTENER_CALLBACKS::default();
    unsafe {
        ffi::LiInitializeConnectionCallbacks(&mut callbacks);
    }
    callbacks.connectionStarted = Some(connection_started);
    callbacks.connectionTerminated = Some(terminated);
    callbacks.stageFailed = Some(stage_failed);
    callbacks.stageStarting = Some(crate::marks::stage_starting);
    callbacks.stageComplete = Some(crate::marks::stage_complete);
    *LIFECYCLE.lock().unwrap() = lifecycle.map(|l| {
        (
            l.clone(),
            korri_viewer_treaty::ViewerDisplay {
                width: o.width as u32,
                height: o.height as u32,
            },
            io.follows_new_sizes(),
        )
    });
    let _lifecycle_guard = LifecycleGuard;
    io.connection_callbacks(&mut callbacks);
    crate::log::mark_core_lines();
    // With a lifecycle socket these signals stay blocked from before its
    // creation; lifecycle's receiver requests the stop instead. On Android
    // the viewer is inside the app, and the app owns its process's signals.
    #[cfg(not(target_os = "android"))]
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT] {
        unsafe { libc::signal(signal, signal_stop as *const () as libc::sighandler_t) };
    }
    running()?;
    let returned = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        if let Some(lifecycle) = lifecycle {
            scope.spawn(|| interrupt_start_on_stop(core, lifecycle, &returned));
        }
        let result = core.start(Connection {
            server: &mut server,
            stream: &mut config,
            listener: &mut callbacks,
            video: io.decoder_callbacks(),
            audio: io.audio_callbacks(),
            video_flags: i32::from(o.fullscreen)
                | match o.rotation {
                    90 => 8,
                    180 => 16,
                    270 => 24,
                    _ => 0,
                },
            audio_context: io.audio_context(),
        });
        returned.store(true, Ordering::Relaxed);
        result
    });
    if result != 0 {
        // An interrupted start is the requested stop, not a failure.
        running()?;
        anyhow::bail!("stream connection failed: {result}");
    }
    // On Android the app's thread sends input into the core: from here until
    // the stop begins (`input::CORE_GATE`).
    #[cfg(target_os = "android")]
    crate::input::CORE_GATE.open();
    let mut poll_error = false;
    let mut shown = None;
    io.start();
    while !STOP.load(Ordering::Relaxed) && !lifecycle.is_some_and(|l| l.stop_requested()) {
        let event = io.poll(100);
        if event != 0 {
            poll_error = event < 0;
            break;
        }
        // The sender can change the stream's size in the middle of the
        // stream (#476): the controller hears each new size shown.
        if let Some(size) = io.shown_size().filter(|size| shown.as_ref() != Some(size)) {
            crate::log::line(&format!(
                "picture: the picture shown is {}x{}",
                size.width, size.height
            ));
            if let Some(lifecycle) = lifecycle {
                lifecycle.resized(size.clone());
            }
            shown = Some(size);
        }
    }
    io.stop();
    // No send is inside the core when it stops, and none enters after:
    // its stop destroys what a send uses. Then the controllers'
    // vibration and lights stop, before the stream ends, as Artemis
    // stops its controller handler before the connection.
    #[cfg(target_os = "android")]
    {
        crate::input::CORE_GATE.close();
        crate::log::line("input: the core takes no more input");
        io.stop_feedback();
    }
    core.stop();
    #[cfg(not(target_os = "android"))]
    let recorded = frame_times.close();
    #[cfg(target_os = "android")]
    let recorded = io.close();
    ensure!(!poll_error, "picture or input event loop failed");
    let error = TERMINATED.load(Ordering::Relaxed);
    ensure!(error == 0, "stream terminated: {error}");
    recorded
}

/// Called directly by the Linux presenter after its first successful frame.
#[cfg(not(target_os = "android"))]
#[unsafe(no_mangle)]
pub extern "C" fn korri_viewer_first_frame() {
    crate::callback::guarded("first frame", (), || {
        if let Some((lifecycle, _, _)) = &*LIFECYCLE.lock().unwrap() {
            lifecycle.first_frame();
        }
    });
}
