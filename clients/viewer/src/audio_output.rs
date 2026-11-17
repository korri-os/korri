// SPDX-License-Identifier: GPL-3.0-or-later
//! Which audio output plays a Linux stream's sound (#391 step F, slice F1).
//!
//! The carried C (`io/audio/`, chosen by `io/bridge.c`) is the default. Its
//! Rust port (`audio_output/renderer.rs`) is built with the
//! `rust-audio-output` feature and chosen with `-audiooutput rust`. Both are
//! the core's `AUDIO_RENDERER_CALLBACKS`: the same libopus decode, the same
//! outputs (SDL audio on the SDL platform; on X11 VA-API, PulseAudio when a
//! probe stream opens, else ALSA), the same buffering and channel order.
//! Only the language changes, so the measure judges one cause.
#[cfg(all(feature = "rust-audio-output", not(target_os = "android")))]
mod ffi;
#[cfg(all(feature = "rust-audio-output", not(target_os = "android")))]
pub(crate) mod renderer;

use anyhow::{Result, bail};

/// `-audiooutput`'s value: `c`, the default, or `rust`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// The carried C renderers of Moonlight Embedded (`io/audio/`).
    #[default]
    Carried,
    /// Their Rust port, over the `alsa` and `libpulse-simple-binding`
    /// crates and SDL's own header.
    Rust,
}

impl Kind {
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "c" => Ok(Self::Carried),
            #[cfg(all(feature = "rust-audio-output", not(target_os = "android")))]
            "rust" => Ok(Self::Rust),
            #[cfg(not(all(feature = "rust-audio-output", not(target_os = "android"))))]
            "rust" => bail!(
                "this build has no Rust audio output (cargo feature rust-audio-output, Linux only)"
            ),
            _ => bail!("unknown audio output {name}: c or rust"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Kind;

    #[test]
    fn the_carried_c_plays_unless_rust_is_named() {
        assert_eq!(Kind::default(), Kind::Carried);
        assert_eq!(Kind::parse("c").unwrap(), Kind::Carried);
        assert!(Kind::parse("cpal").is_err());
        assert!(Kind::parse("").is_err());
    }

    #[cfg(not(feature = "rust-audio-output"))]
    #[test]
    fn a_build_without_the_feature_refuses_rust_by_name() {
        let error = Kind::parse("rust").unwrap_err().to_string();
        assert!(error.contains("rust-audio-output"), "{error}");
    }

    #[cfg(feature = "rust-audio-output")]
    #[test]
    fn a_build_with_the_feature_takes_rust() {
        assert_eq!(Kind::parse("rust").unwrap(), Kind::Rust);
    }
}
