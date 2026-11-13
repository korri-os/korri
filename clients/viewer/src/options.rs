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
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
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
                // Existing picture and input metadata. This slice is view-only.
                "autowindowresize" | "viewonly" | "verbose" | "debug" | "nomouseemulation" => (),
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
                | "rotate" => {
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
                        // No controller or audio output in this slice.
                        "mapping" | "audio" => (),
                        _ => unreachable!(),
                    }
                }
                _ => bail!(
                    "unsupported argument: {arg} (input, audio output and handheld paths are not in this slice)"
                ),
            }
        }
        ensure!(
            !o.host.is_empty() && !o.keydir.as_os_str().is_empty(),
            "host and -keydir are required"
        );
        ensure!(
            o.host.parse::<std::net::IpAddr>().is_ok()
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
            matches!(o.platform.as_str(), "x11" | "x11_vaapi" | "sdl" | "auto"),
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
    fn refuses_bad_boundary_values() {
        for (flag, value) in [
            ("fps", "0"),
            ("width", "-1"),
            ("port", "65536"),
            ("codec", "vp9"),
            ("platform", "v4l2request"),
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
}
