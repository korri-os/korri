// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    network as ffi,
    options::Options,
    pair::hex,
    player_io::PlayerIo,
    session::{Transport, app_id, field, required},
};
use anyhow::{Result, ensure};
use std::{
    ffi::{CStr, CString},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
};

static LIFECYCLE: Mutex<
    Option<(
        crate::lifecycle::Lifecycle,
        korri_viewer_treaty::ViewerDisplay,
    )>,
> = Mutex::new(None);
unsafe extern "C" fn connection_started() {
    if let Some((lifecycle, display)) = &*LIFECYCLE.lock().unwrap()
        && lifecycle.connected(display.clone()).is_err()
    {
        STOP.store(true, Ordering::Relaxed);
    }
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
extern "C" fn signal_stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}
unsafe extern "C" fn terminated(code: i32) {
    if let Some((lifecycle, _)) = &*LIFECYCLE.lock().unwrap() {
        lifecycle.disconnected();
    }
    TERMINATED.store(code, Ordering::Relaxed);
    STOP.store(true, Ordering::Relaxed);
}
unsafe extern "C" fn stage_failed(stage: i32, error: i32) {
    eprintln!("connection stage {stage} failed: {error}");
}

pub fn run(t: &mut Transport, o: &Options) -> Result<()> {
    run_with_lifecycle(t, o, None)
}
pub fn run_with_lifecycle(
    t: &mut Transport,
    o: &Options,
    lifecycle: Option<&crate::lifecycle::Lifecycle>,
) -> Result<()> {
    let _connection = CONNECTION
        .try_lock()
        .map_err(|_| anyhow::anyhow!("a stream connection is already active"))?;
    STOP.store(false, Ordering::Relaxed);
    TERMINATED.store(0, Ordering::Relaxed);
    let info = t.get(true, "serverinfo", &[])?;
    ensure!(
        required(&info, "PairStatus")? == "1",
        "client is not paired"
    );
    let app = app_id(&t.get(true, "applist", &[])?, &o.app)?;
    let io = PlayerIo::new(o)?;
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
        "auto" if o.platform == "x11_vaapi" => ffi::VIDEO_FORMAT_H264 | ffi::VIDEO_FORMAT_H265,
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
    // Match Moonlight's has_fast_aes decision on the PC path.
    config.encryptionFlags = if std::arch::is_x86_feature_detected!("aes") {
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
    // SAFETY: this returns a process-lifetime immutable string, owned by C.
    let extra = unsafe { CStr::from_ptr(ffi::LiGetLaunchUrlQueryParameters()) }.to_str()?;
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
    let launched = t.get(true, endpoint, &params)?;
    let session = field(&launched, "gamesession")?
        .or(field(&launched, "resume")?)
        .ok_or_else(|| anyhow::anyhow!("missing launch result"))?;
    ensure!(session != "0", "stream launch refused");
    let rtsp = CString::new(required(&launched, "sessionUrl0")?)?;
    let mut server = ffi::SERVER_INFORMATION::default();
    // SAFETY: every CString and callback/configuration object below remains
    // live until LiStopConnection joins all the core's threads.
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
    callbacks.logMessage = Some(ffi::picture_log);
    *LIFECYCLE.lock().unwrap() = lifecycle.map(|l| {
        (
            l.clone(),
            korri_viewer_treaty::ViewerDisplay {
                width: o.width as u32,
                height: o.height as u32,
            },
        )
    });
    let _lifecycle_guard = LifecycleGuard;
    io.connection_callbacks(&mut callbacks);
    unsafe {
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT] {
            libc::signal(signal, signal_stop as *const () as libc::sighandler_t);
        }
        let result = ffi::LiStartConnection(
            &mut server,
            &mut config,
            &mut callbacks,
            ffi::picture_callbacks(),
            io.audio_callbacks(),
            std::ptr::null_mut(),
            i32::from(o.fullscreen)
                | match o.rotation {
                    90 => 8,
                    180 => 16,
                    270 => 24,
                    _ => 0,
                },
            io.audio_context(),
            0,
        );
        if result != 0 {
            anyhow::bail!("stream connection failed: {result}");
        }
        io.start();
        let mut poll_error = false;
        while !STOP.load(Ordering::Relaxed) && !lifecycle.is_some_and(|l| l.stop_requested()) {
            let event = ffi::picture_poll(100);
            if event != 0 {
                poll_error = event < 0;
                break;
            }
        }
        io.stop();
        ffi::LiStopConnection();
        ensure!(!poll_error, "picture event loop failed");
    }
    let error = TERMINATED.load(Ordering::Relaxed);
    ensure!(error == 0, "stream terminated: {error}");
    Ok(())
}
