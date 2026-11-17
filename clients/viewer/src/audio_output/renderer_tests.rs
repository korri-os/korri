// SPDX-License-Identifier: GPL-3.0-or-later
//! The Rust audio output against real libopus packets and real outputs:
//! ALSA's file plugin (beside the carried C, byte for byte), a private
//! PulseAudio server, and SDL's disk driver.
use super::*;
use std::{
    ffi::{CString, OsStr},
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread::sleep,
    time::{Duration, Instant},
};

// libopus's encoder, for real packets, declared as the Android sound's tests
// declare it (src/android/audio.rs): one symbol has one signature.
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

/// The core's audio tables and the Rust renderer are process-wide.
static SOUND: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    SOUND.lock().unwrap_or_else(PoisonError::into_inner)
}

const RATE: f64 = 48000.0;
/// Sunshine's packets last 5 ms.
const FRAMES: c_int = 240;
const PACKET: Duration = Duration::from_millis(5);

/// The core's configuration for 2, 6 or 8 channels.
fn config(channels: c_int) -> OPUS_MULTISTREAM_CONFIGURATION {
    let (streams, coupled) = match channels {
        6 => (4, 2),
        8 => (5, 3),
        _ => (1, 1),
    };
    OPUS_MULTISTREAM_CONFIGURATION {
        sampleRate: RATE as c_int,
        channelCount: channels,
        streams,
        coupledStreams: coupled,
        samplesPerFrame: FRAMES,
        mapping: [0, 1, 2, 3, 4, 5, 6, 7],
    }
}

/// The tone of the core's channel `channel`, as tests/test-audio.c plays it.
fn hz(channel: u8) -> f64 {
    440.0 + 100.0 * f64::from(channel)
}

/// Real Opus packets of `count` × 5 ms, each channel at its own tone.
fn packets(config: &OPUS_MULTISTREAM_CONFIGURATION, count: usize) -> Vec<Option<Vec<u8>>> {
    let mut error = 0;
    // SAFETY: the mapping holds 8 entries.
    let encoder = unsafe {
        opus_multistream_encoder_create(
            config.sampleRate,
            config.channelCount,
            config.streams,
            config.coupledStreams,
            config.mapping.as_ptr(),
            ffi::OPUS_APPLICATION_AUDIO as c_int,
            &mut error,
        )
    };
    assert!(
        !encoder.is_null() && error == ffi::OPUS_OK as c_int,
        "the encoder: {error}"
    );
    let (frames, channels) = (FRAMES as usize, config.channelCount as usize);
    let encoded = (0..count)
        .map(|packet| {
            let pcm: Vec<i16> = (0..frames * channels)
                .map(|at| {
                    let t = (packet * frames + at / channels) as f64 / RATE;
                    let tone = hz((at % channels) as u8);
                    (12000.0 * (2.0 * std::f64::consts::PI * tone * t).sin()) as i16
                })
                .collect();
            let mut data = vec![0u8; 8192];
            // SAFETY: one packet of PCM in, at most 8192 bytes out.
            let length = unsafe {
                opus_multistream_encode(
                    encoder,
                    pcm.as_ptr(),
                    FRAMES,
                    data.as_mut_ptr(),
                    data.len() as i32,
                )
            };
            assert!(length > 0, "encode: {length}");
            data.truncate(length as usize);
            Some(data)
        })
        .collect();
    // SAFETY: ours, no longer used.
    unsafe { opus_multistream_encoder_destroy(encoder) };
    encoded
}

/// The core's calls into one table, in its order, on one thread: init with
/// the device as context, each packet every `pace` (`None` is a lost one),
/// then cleanup after `linger`. Init's result.
fn play(
    table: *mut AUDIO_RENDERER_CALLBACKS,
    config: &OPUS_MULTISTREAM_CONFIGURATION,
    device: Option<&CStr>,
    packets: &[Option<Vec<u8>>],
    pace: Duration,
    linger: Duration,
) -> c_int {
    // SAFETY: a table that the core would take; it lives for the process.
    let table = unsafe { &*table };
    let mut config = *config;
    let context = device.map_or(std::ptr::null_mut(), |d| d.as_ptr().cast_mut().cast());
    let channels = config.channelCount;
    // Limelight.h's MAKE_AUDIO_CONFIGURATION, as stream.rs makes it.
    let mask = match channels {
        6 => 0x3f,
        8 => 0x63f,
        _ => 3,
    };
    // SAFETY: as the core calls it, with a configuration that outlives it.
    let started = unsafe {
        (table.init.unwrap())(
            (mask << 16) | (channels << 8) | 0xCA,
            &mut config,
            context,
            0,
        )
    };
    if started != 0 {
        return started;
    }
    let begun = Instant::now();
    for (at, packet) in packets.iter().enumerate() {
        sleep((begun + pace * at as u32).saturating_duration_since(Instant::now()));
        let (data, length) = packet.as_ref().map_or((std::ptr::null_mut(), 0), |p| {
            (p.as_ptr().cast_mut().cast(), p.len() as c_int)
        });
        // SAFETY: a packet that lives for the call, or NULL for a lost one.
        unsafe { (table.decodeAndPlaySample.unwrap())(data, length) };
    }
    sleep(linger);
    // SAFETY: after the last packet, as the core's stop does.
    unsafe { (table.cleanup.unwrap())() };
    0
}

fn samples(bytes: &[u8]) -> Vec<i16> {
    let (pairs, _) = bytes.as_chunks::<2>();
    pairs.iter().map(|&pair| i16::from_le_bytes(pair)).collect()
}

/// The frames from the first to the last loud one, less 50 ms at each end:
/// the tone, without the output's silence around it.
fn sounded(pcm: &[i16], channels: usize) -> &[i16] {
    let loud = |frame: &[i16]| frame.iter().any(|s| s.unsigned_abs() > 1000);
    let frames: Vec<&[i16]> = pcm.chunks_exact(channels).collect();
    let first = frames.iter().position(|f| loud(f));
    let last = frames.iter().rposition(|f| loud(f));
    let (Some(first), Some(last)) = (first, last) else {
        return &[];
    };
    let margin = (RATE * 0.05) as usize;
    if last < first + 2 * margin {
        return &[];
    }
    &pcm[(first + margin) * channels..(last - margin) * channels]
}

/// The frequency of one channel of interleaved PCM, from its rising zero
/// crossings.
fn frequency(pcm: &[i16], channels: usize, channel: usize) -> f64 {
    let samples: Vec<i16> = pcm
        .iter()
        .skip(channel)
        .step_by(channels)
        .copied()
        .collect();
    let rising = samples.windows(2).filter(|w| w[0] < 0 && w[1] >= 0).count();
    rising as f64 * RATE / samples.len() as f64
}

/// Each position of `pcm` carries the tone of the core's channel that
/// `order` names there.
fn assert_tones(pcm: &[i16], channels: usize, order: [u8; 8], what: &str) {
    assert!(
        pcm.len() >= channels * (RATE * 0.2) as usize,
        "{what}: too little sound"
    );
    for (position, &source) in order.iter().take(channels).enumerate() {
        let (found, wanted) = (frequency(pcm, channels, position), hz(source));
        assert!(
            (found - wanted).abs() < wanted * 0.03,
            "{what}: position {position} plays {found:.0} Hz, not channel {source}'s {wanted:.0} Hz"
        );
    }
}

/// One `#[ignore]`d test of this binary in a new process with `environment`.
/// libpulse and SDL read their server and driver from the environment, and a
/// test must not change its own process's.
fn child(test: &str, environment: &[(&str, &OsStr)]) {
    let module = module_path!().split_once("::").unwrap().1;
    let name = format!("{module}::{test}");
    let ran = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &name,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .envs(environment.iter().copied())
        .output()
        .unwrap();
    let (out, err) = (
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr),
    );
    // A name that matches no test runs nothing and passes.
    assert!(
        ran.status.success() && out.contains("1 passed"),
        "{name}: {}\n{out}\n{err}",
        ran.status
    );
}

#[test]
fn alsa_and_pulseaudio_take_alsas_channel_order() {
    let core = [0, 1, 2, 3, 4, 5, 6, 7];
    assert_eq!(alsa_order(core, 2), core);
    assert_eq!(alsa_order(core, 6), [0, 1, 4, 5, 2, 3, 6, 7]);
    assert_eq!(alsa_order(core, 8), [0, 1, 4, 5, 2, 3, 6, 7]);
    // Sunshine's own mapping for 5.1 is not the identity; the swap moves
    // entries, whatever they hold.
    assert_eq!(
        alsa_order([0, 4, 1, 5, 2, 3, 6, 7], 6),
        [0, 4, 2, 3, 1, 5, 6, 7]
    );
}

#[test]
fn on_alsa_the_rust_output_writes_the_pcm_that_the_carried_c_writes() {
    let _sound = exclusive();
    let directory = tempfile::tempdir().unwrap();
    let file = |name: &str| -> (PathBuf, CString) {
        let path = directory.path().join(name);
        // alsa.conf's own `file` PCM: a file over the null PCM.
        let device = CString::new(format!("file:FILE={},FORMAT=raw", path.display())).unwrap();
        (path, device)
    };
    for channels in [2, 6, 8] {
        let config = config(channels);
        // 400 ms, with a lost packet that Opus conceals.
        let mut packets = packets(&config, 80);
        packets[10] = None;
        let (carried, carried_device) = file(&format!("carried-{channels}.raw"));
        let (rust, rust_device) = file(&format!("rust-{channels}.raw"));
        // No PulseAudio sink has a file's name: both take ALSA.
        assert_eq!(choose(true, Some(&rust_device)), Output::Alsa);
        // SAFETY: the device outlives the stream, as PlayerIo's does.
        let c_table =
            unsafe { network::viewer_audio_callbacks(1, carried_device.as_ptr().cast_mut()) };
        let none = Duration::ZERO;
        assert_eq!(
            play(
                c_table,
                &config,
                Some(&carried_device),
                &packets,
                none,
                none
            ),
            0
        );
        let rust_table = callbacks(true, Some(&rust_device));
        assert_eq!(
            play(
                rust_table,
                &config,
                Some(&rust_device),
                &packets,
                none,
                none
            ),
            0
        );
        let (carried, rust) = (fs::read(carried).unwrap(), fs::read(rust).unwrap());
        assert_eq!(
            rust.len(),
            packets.len() * FRAMES as usize * channels as usize * 2
        );
        assert!(
            rust == carried,
            "{channels} channels: the PCM differs from the carried C's"
        );
        // Past the decoder's start and the concealed packet.
        let pcm = samples(&rust);
        let channels = channels as usize;
        let tail = &pcm[20 * FRAMES as usize * channels..];
        assert_tones(
            tail,
            channels,
            alsa_order(config.mapping, channels as c_int),
            "ALSA",
        );
        assert!(renderer().is_none(), "cleanup closes the renderer");
    }
}

#[test]
fn an_output_that_does_not_open_fails_the_start_as_in_c() {
    let _sound = exclusive();
    let config = config(2);
    let packets = packets(&config, 2);
    let none = Duration::ZERO;
    let missing = c"korri_missing";
    // SAFETY: as in the test above.
    let c_table = unsafe { network::viewer_audio_callbacks(1, missing.as_ptr().cast_mut()) };
    assert_eq!(play(c_table, &config, Some(missing), &[], none, none), -1);
    // SAFETY: alsa.c's cleanup frees what its failed init made.
    unsafe { ((*c_table).cleanup.unwrap())() };
    let rust_table = callbacks(true, Some(missing));
    assert_eq!(
        play(rust_table, &config, Some(missing), &packets, none, none),
        -1
    );
    assert!(renderer().is_none());
    // A configuration past the core's limits: refused, not read past the
    // mapping's 8 entries.
    let mut nine = config;
    nine.channelCount = 9;
    assert_eq!(play(rust_table, &nine, Some(missing), &[], none, none), -1);
    // A packet with no renderer plays nothing.
    // SAFETY: a packet that lives for the call.
    unsafe {
        let packet = packets[0].as_ref().unwrap();
        ((*rust_table).decodeAndPlaySample.unwrap())(
            packet.as_ptr().cast_mut().cast(),
            packet.len() as c_int,
        );
    }
}

/// A private PulseAudio server: one socket and one null sink in ALSA's
/// 5.1 order, nothing else, as stream-cadence's tone server
/// (`nix/stream-cadence/sender.py`, `tone_script`).
struct Pulse {
    server: Child,
    socket: PathBuf,
}

const SINK: &str = "korri_test";
const ALSA_51: &str = "front-left,front-right,rear-left,rear-right,front-center,lfe";

impl Pulse {
    fn start(directory: &Path) -> Self {
        let socket = directory.join("native");
        let script = directory.join("server.pa");
        fs::write(
            &script,
            format!(
                "load-module module-native-protocol-unix socket={} auth-anonymous=1\n\
                 load-module module-null-sink sink_name={SINK} rate=48000 channels=6 channel_map={ALSA_51}\n\
                 set-default-sink {SINK}\n",
                socket.display()
            ),
        )
        .unwrap();
        let home = directory.join("home");
        fs::create_dir_all(&home).unwrap();
        let server = Command::new("pulseaudio")
            .args([
                "--daemonize=no",
                "--system=no",
                "--use-pid-file=no",
                "--exit-idle-time=-1",
                "--disallow-exit",
                "--disallow-module-loading",
                "--realtime=no",
                "--high-priority=no",
                "--log-target=stderr",
                "-n",
                "-F",
            ])
            .arg(&script)
            .env("HOME", &home)
            .env("XDG_RUNTIME_DIR", &home)
            .env("XDG_CONFIG_HOME", &home)
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/korri-no-bus")
            .env("DBUS_SYSTEM_BUS_ADDRESS", "unix:path=/korri-no-bus")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("pulseaudio on PATH (the package's check inputs, the viewer shell)");
        let pulse = Self { server, socket };
        let deadline = Instant::now() + Duration::from_secs(10);
        while !pulse.socket.exists() {
            assert!(
                Instant::now() < deadline,
                "the private PulseAudio did not start"
            );
            sleep(Duration::from_millis(50));
        }
        pulse
    }

    fn address(&self) -> String {
        format!("unix:{}", self.socket.display())
    }

    /// The sink's monitor, as raw PCM into `path`.
    fn record(&self, path: &Path) -> Child {
        Command::new("parec")
            .args([
                &format!("--server={}", self.address()),
                &format!("--device={SINK}.monitor"),
                "--raw",
                "--format=s16le",
                "--rate=48000",
                "--channels=6",
                &format!("--channel-map={ALSA_51}"),
            ])
            .stdout(fs::File::create(path).unwrap())
            .stderr(Stdio::null())
            .spawn()
            .expect("parec on PATH, beside pulseaudio")
    }
}

impl Drop for Pulse {
    fn drop(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
    }
}

#[test]
fn on_x11_vaapi_the_rust_output_plays_into_pulseaudio_when_it_answers() {
    let directory = tempfile::tempdir().unwrap();
    let pulse = Pulse::start(directory.path());
    let monitor = directory.path().join("monitor.raw");
    let mut recording = pulse.record(&monitor);
    sleep(Duration::from_millis(300));
    child(
        "child_plays_into_pulseaudio",
        &[("PULSE_SERVER", OsStr::new(&pulse.address()))],
    );
    sleep(Duration::from_millis(300));
    recording.kill().unwrap();
    recording.wait().unwrap();
    let pcm = samples(&fs::read(monitor).unwrap());
    assert_tones(
        sounded(&pcm, 6),
        6,
        alsa_order(config(6).mapping, 6),
        "PulseAudio",
    );
}

#[test]
#[ignore = "on_x11_vaapi_the_rust_output_plays_into_pulseaudio_when_it_answers runs it"]
fn child_plays_into_pulseaudio() {
    assert_eq!(choose(true, None), Output::Pulse);
    let config = config(6);
    // One second at the network's pace. pulse.c frees its stream at cleanup
    // without a drain, so what PulseAudio still holds then is not heard.
    let packets = packets(&config, 200);
    let table = callbacks(true, None);
    let linger = Duration::from_millis(300);
    assert_eq!(play(table, &config, None, &packets, PACKET, linger), 0);
}

#[test]
fn on_the_sdl_platform_the_rust_output_plays_through_sdl_audio() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("sdl.raw");
    child(
        "child_plays_through_sdl_audio",
        &[
            ("SDL_AUDIODRIVER", OsStr::new("disk")),
            ("SDL_DISKAUDIOFILE", file.as_os_str()),
        ],
    );
    let pcm = samples(&fs::read(file).unwrap());
    // sdl.c gives the decoder the core's own order.
    assert_tones(sounded(&pcm, 2), 2, config(2).mapping, "SDL audio");
}

#[test]
#[ignore = "on_the_sdl_platform_the_rust_output_plays_through_sdl_audio runs it"]
fn child_plays_through_sdl_audio() {
    assert_eq!(choose(false, None), Output::Sdl);
    let config = config(2);
    let packets = packets(&config, 200);
    // The device's 4096 frames (85 ms) play out before the cleanup closes it.
    let linger = Duration::from_millis(400);
    assert_eq!(
        play(
            callbacks(false, None),
            &config,
            None,
            &packets,
            PACKET,
            linger
        ),
        0
    );
}
