// SPDX-License-Identifier: GPL-3.0-or-later
//! Which audio output plays a Linux stream's sound (#391 step F, slice F1).
//!
//! The carried C is `io/audio/`, chosen by `io/bridge.c`. Its Rust port
//! (`audio_output/renderer.rs`) is built with the `rust-audio-output`
//! feature, and is then the default (the owner's call of 2026-11-18, ahead
//! of a quiet measurement); `-audiooutput c` chooses the carried C. Both are
//! the core's `AUDIO_RENDERER_CALLBACKS`: the same libopus decode, the same
//! outputs (SDL audio on the SDL platform; on X11 VA-API, PulseAudio when a
//! probe stream opens, else ALSA), the same buffering and channel order.
//! Only the language changes, so the measure judges one cause.
#[cfg(all(feature = "rust-audio-output", not(target_os = "android")))]
mod ffi;
#[cfg(all(feature = "rust-audio-output", not(target_os = "android")))]
pub(crate) mod renderer;

use anyhow::{Result, bail};

/// `-audiooutput`'s value: `c` or `rust`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The carried C renderers of Moonlight Embedded (`io/audio/`).
    Carried,
    /// Their Rust port, over the `alsa` and `libpulse-simple-binding`
    /// crates and SDL's own header.
    Rust,
}

impl Default for Kind {
    /// The Rust port where the build has it (#391 step F1: the owner's call
    /// of 2026-11-18, ahead of a quiet measurement); else the carried C.
    fn default() -> Self {
        if cfg!(all(
            feature = "rust-audio-output",
            not(target_os = "android")
        )) {
            Self::Rust
        } else {
            Self::Carried
        }
    }
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

    // The owner's call of 2026-11-18, ahead of a quiet measurement (#391
    // step F1): a build with the port plays through it; `-audiooutput c`
    // keeps the carried C.
    #[test]
    fn the_rust_port_plays_where_built_unless_c_is_named() {
        let built = cfg!(all(
            feature = "rust-audio-output",
            not(target_os = "android")
        ));
        assert_eq!(
            Kind::default(),
            if built { Kind::Rust } else { Kind::Carried }
        );
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
