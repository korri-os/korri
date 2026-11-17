// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::{Result, bail, ensure};
use std::path::PathBuf;

#[derive(Debug)]
pub struct Options {
    pub action: String,
    pub host: String,
    pub port: u16,
    pub keydir: PathBuf,
    pub app: String,
    pub platform: String,
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub bitrate: i32,
    pub packet_size: i32,
    pub codec: String,
    pub fullscreen: bool,
    pub sops: bool,
    pub local_audio: bool,
    pub remote: i32,
    pub hdr: bool,
    pub rotation: i32,
    pub audio_channels: i32,
    pub allow_unsupported: bool,
    pub viewonly: bool,
    pub mouse_emulation: bool,
    pub mapping: Option<String>,
    pub audio_device: Option<String>,
    pub verbose: bool,
    pub auto_window_resize: bool,
    /// Explicit evdev nodes, only with `-platform v4l2request`.
    pub inputs: Vec<String>,
    /// The network core that streams (`-networkcore`, #391 step E).
    pub network_core: crate::network_core::Kind,
}

/// Moonlight's `MAX_INPUTS` (config.h).
#[cfg(v4l2request)]
const MAX_INPUTS: usize = 6;
#[cfg(v4l2request)]
const PLATFORMS: &[&str] = &["x11", "x11_vaapi", "sdl", "auto", "v4l2request"];
#[cfg(not(v4l2request))]
const PLATFORMS: &[&str] = &["x11", "x11_vaapi", "sdl", "auto"];
#[cfg(v4l2request)]
const UNSUPPORTED: &str = "";
#[cfg(not(v4l2request))]
const UNSUPPORTED: &str = " (explicit input devices and handheld paths are not in this slice)";

impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        Self::parse_for(args, true)
    }

    /// A stream under a lifecycle in its controller's process (Android,
    /// #434 D10): the controller's Start names the host and port
    /// (`lifecycle::Start::configure`), so none is given here. `size` is the
    /// display that the controller gives the launch; without one the stream
    /// keeps Moonlight's default size.
    pub fn for_lifecycle(keydir: PathBuf, size: Option<(u32, u32)>) -> Result<Self> {
        let keydir = keydir
            .into_os_string()
            .into_string()
            .map_err(|_| anyhow::anyhow!("the key directory is not UTF-8"))?;
        let mut args = vec!["stream".to_owned(), "-keydir".to_owned(), keydir];
        if let Some((width, height)) = size {
            args.extend([
                "-width".to_owned(),
                width.to_string(),
                "-height".to_owned(),
                height.to_string(),
            ]);
        }
        Self::parse_for(args, false)
    }

    fn parse_for(args: impl IntoIterator<Item = String>, with_host: bool) -> Result<Self> {
        let mut args = args.into_iter();
        let action = args.next().unwrap_or_default();
        ensure!(
            matches!(action.as_str(), "stream" | "pair"),
            "usage: korri-viewer stream|pair [Moonlight arguments] HOST"
        );
        let mut o = Self {
            action,
            host: String::new(),
            port: 47989,
            keydir: PathBuf::new(),
            app: "Korri Stream".into(),
            platform: "x11".into(),
            width: 1280,
            height: 720,
            fps: 60,
            bitrate: 10000,
            packet_size: 1392,
            codec: "auto".into(),
            fullscreen: true,
            sops: true,
            local_audio: false,
            remote: 2,
            hdr: false,
            rotation: 0,
            audio_channels: 2,
            allow_unsupported: true,
            viewonly: false,
            mouse_emulation: true,
            mapping: None,
            audio_device: None,
            verbose: false,
            auto_window_resize: false,
            inputs: Vec::new(),
            network_core: Default::default(),
        };
        while let Some(arg) = args.next() {
            if !arg.starts_with('-') {
                ensure!(o.host.is_empty(), "more than one host");
                o.host = arg;
                continue;
            }
            let flag = arg.trim_start_matches('-');
            match flag {
                "windowed" => o.fullscreen = false,
                "fullscreen" => o.fullscreen = true,
                "autowindowresize" => o.auto_window_resize = true,
                "viewonly" => o.viewonly = true,
                "nomouseemulation" => o.mouse_emulation = false,
                "verbose" | "debug" => o.verbose = true,
                "hdr" => o.hdr = true,
                "nounsupported" => o.allow_unsupported = false,
                "nosops" => o.sops = false,
                "localaudio" => o.local_audio = true,
                "720" => {
                    o.width = 1280;
                    o.height = 720;
                }
                "1080" => {
                    o.width = 1920;
                    o.height = 1080;
                }
                "4k" => {
                    o.width = 3840;
                    o.height = 2160;
                }
                "keydir" | "app" | "platform" | "width" | "height" | "fps" | "bitrate"
                | "packetsize" | "codec" | "port" | "mapping" | "audio" | "remote" | "surround"
                | "rotate" | "networkcore" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("missing value for {arg}"))?;
                    match flag {
                        "keydir" => o.keydir = value.into(),
                        "app" => o.app = value,
                        "platform" => o.platform = value,
                        "codec" => o.codec = value,
                        "width" => o.width = value.parse()?,
                        "height" => o.height = value.parse()?,
                        "fps" => o.fps = value.parse()?,
                        "bitrate" => o.bitrate = value.parse()?,
                        "packetsize" => o.packet_size = value.parse()?,
                        "port" => o.port = value.parse()?,
                        "rotate" => {
                            o.rotation = value.parse()?;
                            ensure!(matches!(o.rotation, 0 | 90 | 180 | 270), "invalid rotation");
                        }
                        "surround" => {
                            o.audio_channels = match value.as_str() {
                                "stereo" => 2,
                                "5.1" => 6,
                                "7.1" => 8,
                                _ => bail!("invalid surround mode"),
                            }
                        }
                        "remote" => {
                            o.remote = match value.as_str() {
                                "no" => 0,
                                "yes" => 1,
                                "auto" => 2,
                                _ => bail!("invalid remote mode"),
                            }
                        }
                        "mapping" => o.mapping = Some(value),
                        "networkcore" => o.network_core = crate::network_core::Kind::parse(&value)?,
                        "audio" => o.audio_device = Some(value),
                        _ => unreachable!(),
                    }
                }
                #[cfg(v4l2request)]
                "input" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("missing value for {arg}"))?;
                    ensure!(o.inputs.len() < MAX_INPUTS, "Too many inputs specified");
                    o.inputs.push(value);
                }
                _ => bail!("unsupported argument: {arg}{UNSUPPORTED}"),
            }
        }
        // Moonlight opens explicit nodes on its embedded platforms. On PC,
        // korrid passes none and this viewer keeps udev discovery only.
        ensure!(
            o.inputs.is_empty() || o.platform == "v4l2request",
            "explicit input devices need -platform v4l2request"
        );
        ensure!(
            (!with_host || !o.host.is_empty()) && !o.keydir.as_os_str().is_empty(),
            "host and -keydir are required"
        );
        ensure!(
            with_host || o.host.is_empty(),
            "the lifecycle's Start names the host"
        );
        ensure!(
            !with_host
                || o.host.parse::<std::net::IpAddr>().is_ok()
                || o.host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-'),
            "invalid host"
        );
        ensure!(
            o.port > 0
                && (1..=16384).contains(&o.width)
                && (1..=16384).contains(&o.height)
                && (1..=1000).contains(&o.fps)
                && o.bitrate > 0
                && (512..=65507).contains(&o.packet_size),
            "invalid stream dimensions, rate, bitrate, port or packet size"
        );
        ensure!(
            PLATFORMS.contains(&o.platform.as_str()),
            "unsupported platform {}",
            o.platform
        );
        ensure!(
            matches!(o.codec.as_str(), "h264" | "h265" | "av1" | "auto"),
            "unsupported codec {}",
            o.codec
        );
        Ok(o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retains_player_io_options() {
        let o = Options::parse(
            [
                "stream",
                "-keydir",
                "/tmp/keys",
                "-viewonly",
                "-nomouseemulation",
                "-mapping",
                "/tmp/controllers",
                "-audio",
                "null",
                "host",
            ]
            .map(str::to_owned),
        )
        .unwrap();
        assert!(o.viewonly);
        assert!(!o.mouse_emulation);
        assert_eq!(o.mapping.as_deref(), Some("/tmp/controllers"));
        assert_eq!(o.audio_device.as_deref(), Some("null"));
        let o =
            Options::parse(["stream", "-keydir", "/tmp/keys", "host"].map(str::to_owned)).unwrap();
        assert!(!o.viewonly);
        assert!(o.mouse_emulation);
        assert!(o.audio_device.is_none());
    }

    // #434 D10: korrid starts the viewer in its own process. Its Start names
    // the host and port; the stream's size is korrid's display, or the
    // viewer's own default.
    #[test]
    fn a_lifecycle_launch_takes_its_size_from_korrid_and_its_host_from_start() {
        let keys = PathBuf::from("/data/keys");
        let o = Options::for_lifecycle(keys.clone(), Some((1920, 1080))).unwrap();
        assert_eq!(o.action, "stream");
        assert_eq!(o.keydir, keys);
        assert_eq!((o.width, o.height), (1920, 1080));
        assert_eq!(o.app, "Korri Stream");
        assert!(o.host.is_empty(), "Start::configure names the host");
        let mut o = Options::for_lifecycle(keys.clone(), None).unwrap();
        assert_eq!(
            (o.width, o.height, o.fps, o.bitrate),
            (1280, 720, 60, 10000)
        );
        crate::lifecycle::Start {
            address: "192.0.2.7:47984".into(),
            display: None,
        }
        .configure(&mut o)
        .unwrap();
        assert_eq!((&*o.host, o.port), ("192.0.2.7", 47984));
        for refused in [(0, 720), (1280, 0), (16385, 720)] {
            assert!(Options::for_lifecycle(keys.clone(), Some(refused)).is_err());
        }
        assert!(Options::for_lifecycle(PathBuf::new(), None).is_err());
    }

    // #391 step E: the C core streams unless -networkcore names another,
    // anywhere among the arguments, as a wrapper may append it.
    #[test]
    fn the_network_core_is_c_unless_named() {
        use crate::network_core::Kind;
        let parse = |extra: &[&str]| {
            let mut args = vec!["stream", "-keydir", "/k", "host"];
            args.extend_from_slice(extra);
            Options::parse(args.into_iter().map(str::to_owned))
        };
        assert_eq!(parse(&[]).unwrap().network_core, Kind::MoonlightCommonC);
        assert_eq!(
            parse(&["-networkcore", "c"]).unwrap().network_core,
            Kind::MoonlightCommonC
        );
        assert!(parse(&["-networkcore", "rust"]).is_err());
        assert!(parse(&["-networkcore"]).is_err());
        assert!(
            Options::parse(
                ["pair", "-keydir", "/k", "host", "-networkcore", "c"].map(str::to_owned)
            )
            .is_ok()
        );
    }

    #[test]
    fn refuses_bad_boundary_values() {
        for (flag, value) in [
            ("fps", "0"),
            ("width", "-1"),
            ("port", "65536"),
            ("codec", "vp9"),
            #[cfg(not(v4l2request))]
            ("platform", "v4l2request"),
            ("platform", "v4l2m2m"),
        ] {
            assert!(
                Options::parse(
                    [
                        "stream",
                        "-keydir",
                        "/tmp/keys",
                        "host",
                        &format!("-{flag}"),
                        value
                    ]
                    .map(str::to_owned)
                )
                .is_err()
            );
        }
    }

    #[cfg(v4l2request)]
    #[test]
    fn handheld_arguments_from_korrid() {
        let parse = |extra: &[&str]| {
            let mut args = vec!["stream", "-platform", "v4l2request", "-keydir", "/k"];
            args.extend_from_slice(extra);
            args.push("host");
            Options::parse(args.into_iter().map(str::to_owned))
        };
        let o = parse(&[
            "-mapping",
            "/m",
            "-windowed",
            "-autowindowresize",
            "-input",
            "/dev/input/event3",
            "-input",
            "/dev/input/event7",
        ])
        .unwrap();
        assert_eq!(o.platform, "v4l2request");
        assert_eq!(o.inputs, ["/dev/input/event3", "/dev/input/event7"]);
        assert!(o.auto_window_resize && !o.fullscreen && !o.viewonly);
        assert!(parse(&["-viewonly"]).unwrap().inputs.is_empty());
        let six: Vec<&str> = ["-input", "/dev/input/event1"].repeat(6);
        assert_eq!(parse(&six).unwrap().inputs.len(), 6);
        let seven = ["-input", "/dev/input/event1"].repeat(7);
        assert!(parse(&seven).is_err());
        let pc = Options::parse(
            [
                "stream",
                "-platform",
                "x11_vaapi",
                "-input",
                "/dev/input/event1",
                "-keydir",
                "/k",
                "host",
            ]
            .map(str::to_owned),
        );
        assert!(pc.is_err());
    }
}
