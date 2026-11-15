// SPDX-License-Identifier: GPL-3.0-only
//! Which Android decoder takes the H.264 stream, and the low-latency format
//! keys it is configured with: Artemis's `MediaCodecHelper`, as the research
//! brief lists it (`docs/research/2026-11-13-viewer-rust-android.md`,
//! findings 20 and 21). Pure: `picture.rs` reads the decoder list through JNI
//! and configures the codec with each try in turn.
//!
//! Ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`, a fork of
//! `ClassicOldSong/moonlight-android`, itself a fork of
//! `moonlight-stream/moonlight-android`), `MediaCodecHelper.java`: the decoder rounds
//! (`findProbableSafeDecoder`, `findKnownSafeDecoder`, `findFirstDecoder`,
//! `isCodecBlacklisted`), the decoder name lists,
//! `setDecoderLowLatencyOptions` and `decoderSupportsAdaptivePlayback`, and
//! from `MediaCodecDecoderRenderer.java` the live-resolution maximum of
//! `initializeDecoder`. Artemis ships under the GNU GPL version 3 with no "or later" grant, so
//! this file does too. The Rust adaptation is Korri work dated 2026-11-14.

/// One entry of Android's `MediaCodecList` (REGULAR_CODECS) that decodes H.264.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoder {
    pub name: String,
    /// `MediaCodecInfo.isHardwareAccelerated()`.
    pub hardware: bool,
    /// `MediaCodecInfo.isSoftwareOnly()`.
    pub software_only: bool,
    /// `MediaCodecInfo.isAlias()`: another name of a decoder in the list.
    pub alias: bool,
    /// `CodecCapabilities.FEATURE_LowLatency` for `video/avc`.
    pub low_latency: bool,
    /// The decoder lists `AVCProfileHigh` among its profile levels.
    pub high_profile: bool,
    /// `CodecCapabilities.FEATURE_AdaptivePlayback` for `video/avc`.
    pub adaptive_playback: bool,
}

impl Decoder {
    /// The `decode` record of the measure (`nix/stream-cadence`,
    /// `viewer_ran.decode`): the hardware API, or `software`.
    pub fn decode(&self) -> &'static str {
        if self.hardware && !self.software_only {
            "mediacodec"
        } else {
            "software"
        }
    }
}

/// `MediaCodecHelper.findProbableSafeDecoder("video/avc", AVCProfileHigh)`,
/// then `findFirstDecoder`. Round 1 takes only decoders with
/// `FEATURE_LowLatency`, which finds the separate low-latency decoders that
/// some phones list after the ordinary ones; round 2 takes any decoder with
/// High profile; the last resort is the first decoder at all. Aliases are
/// skipped, and so are software decoders, except on the emulator
/// (`SHOULD_BYPASS_SOFTWARE_BLOCK`), which may have only those.
pub fn choose(decoders: &[Decoder], emulator: bool) -> Option<&Decoder> {
    let allowed = |decoder: &&Decoder| !decoder.alias && !blocked(decoder, emulator);
    let candidates = || decoders.iter().filter(allowed);
    candidates()
        .find(|decoder| decoder.low_latency && decoder.high_profile)
        .or_else(|| candidates().find(|decoder| decoder.high_profile))
        .or_else(|| candidates().next())
}

/// `isCodecBlacklisted`: software-only decoders, and the blocked names.
fn blocked(decoder: &Decoder, emulator: bool) -> bool {
    if emulator {
        return false;
    }
    decoder.software_only || prefixed(&decoder.name, &["omx.google", "AVCDecoder"])
}

/// `decoderSupportsAdaptivePlayback`: the feature, except on the decoders
/// that Artemis keeps from it (Intel's added latency on the Nexus Player,
/// MediaTek's crash at 1080p).
pub fn adaptive_playback(decoder: &Decoder) -> bool {
    decoder.adaptive_playback && !prefixed(&decoder.name, &["omx.intel", "omx.mtk"])
}

/// The largest picture that adaptive playback is configured for
/// (`max-width`, `max-height`), as Artemis reserves it for live resolution:
/// the largest square, from 4096 down to the stream's longer side in steps
/// of 8, that the decoder supports at the stream's rate, so that either
/// orientation fits. `supported` asks
/// `VideoCapabilities.areSizeAndRateSupported(side, side, rate)`. With no
/// such square, the stream's own size.
pub fn adaptive_max(stream: (i32, i32), mut supported: impl FnMut(i32) -> bool) -> (i32, i32) {
    let longer = stream.0.max(stream.1);
    (longer..=4096)
        .rev()
        .step_by(8)
        .find(|&side| supported(side))
        .map_or(stream, |side| (side, side))
}

/// One configure attempt's format keys, most risky first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Try {
    pub keys: Vec<(&'static str, i32)>,
    /// No key left to drop: if this try fails, the decoder cannot be used.
    pub last: bool,
}

/// `MediaCodecHelper.setDecoderLowLatencyOptions` for try `number`, as the
/// brief lists its keys. Each later try drops the riskiest keys of the one
/// before; the last has none.
pub fn low_latency_try(decoder: &Decoder, number: u32) -> Try {
    let name = decoder.name.as_str();
    let mut keys = Vec::new();
    if number < 1 {
        // Android 11's KEY_LOW_LATENCY. A decoder that has the feature gets
        // it alone in this try.
        keys.push(("low-latency", 1));
        if decoder.low_latency {
            return Try { keys, last: false };
        }
    }
    if number < 2 {
        // MediaTek and Amlogic: OMX.MTK.index.param.video.LowLatencyDecode.
        keys.push(("vdec-lowlatency", 1));
    }
    if number < 3 {
        if max_operating_rate(name) {
            keys.push(("operating-rate", i32::from(i16::MAX)));
        } else {
            keys.push(("priority", 0));
        }
    }
    if prefixed(name, QUALCOMM) {
        if number < 4 {
            // 0 turns reordering off on OMX.qcom; C2 wants 1.
            let omx = prefixed(name, &["omx.qcom"]);
            keys.push(("vendor.qti-ext-dec-picture-order.enable", i32::from(!omx)));
        }
        if number < 5 {
            keys.push(("vendor.qti-ext-dec-low-latency.enable", 1));
            // Software output fences (Snapdragon 8 Gen 2, 8s Gen 3, Elite).
            keys.push(("vendor.qti-ext-output-sw-fence-enable.value", 1));
            keys.push(("vendor.qti-ext-output-fence.enable", 1));
            keys.push(("vendor.qti-ext-output-fence.fence_type", 1));
        }
    }
    Try {
        last: keys.is_empty(),
        keys,
    }
}

const QUALCOMM: &[&str] = &["omx.qcom", "c2.qti", "c2.qcom"];

/// `decoderSupportsMaxOperatingRate`: Qualcomm and the decoders with
/// reference frame invalidation at API 31 (GLES 3). Artemis also excludes
/// the Adreno 620, which needs the GL renderer's name; this does not.
fn max_operating_rate(name: &str) -> bool {
    prefixed(name, QUALCOMM)
        || prefixed(
            name,
            &[
                "omx.nvidia",
                "c2.nvidia",
                "c2.mtk",
                "omx.mtk",
                "omx.exynos",
                "c2.exynos",
            ],
        )
}

/// `isDecoderInList`: a case-insensitive name prefix.
fn prefixed(name: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| {
        name.get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoder(name: &str) -> Decoder {
        Decoder {
            name: name.into(),
            hardware: true,
            software_only: false,
            alias: false,
            low_latency: false,
            high_profile: true,
            adaptive_playback: true,
        }
    }

    fn software(name: &str) -> Decoder {
        Decoder {
            hardware: false,
            software_only: true,
            ..decoder(name)
        }
    }

    #[test]
    fn a_low_latency_decoder_listed_later_wins() {
        let list = [
            decoder("c2.qti.avc.decoder"),
            Decoder {
                low_latency: true,
                ..decoder("c2.qti.avc.decoder.low_latency")
            },
        ];
        assert_eq!(
            choose(&list, false).unwrap().name,
            "c2.qti.avc.decoder.low_latency"
        );
    }

    #[test]
    fn aliases_and_software_decoders_are_skipped_on_a_phone() {
        let list = [
            Decoder {
                alias: true,
                ..decoder("OMX.qcom.video.decoder.avc")
            },
            software("c2.android.avc.decoder"),
            software("OMX.google.h264.decoder"),
            decoder("c2.exynos.h264.decoder"),
        ];
        assert_eq!(choose(&list, false).unwrap().name, "c2.exynos.h264.decoder");
        assert_eq!(choose(&list[..3], false), None);
    }

    #[test]
    fn the_emulator_may_use_a_software_decoder() {
        let list = [software("c2.android.avc.decoder")];
        let chosen = choose(&list, true).unwrap();
        assert_eq!(chosen.decode(), "software");
        assert_eq!(decoder("c2.goldfish.h264.decoder").decode(), "mediacodec");
    }

    #[test]
    fn high_profile_comes_before_the_first_decoder() {
        let list = [
            Decoder {
                high_profile: false,
                ..decoder("c2.vendor.baseline")
            },
            decoder("c2.vendor.high"),
        ];
        assert_eq!(choose(&list, false).unwrap().name, "c2.vendor.high");
        assert_eq!(
            choose(&list[..1], false).unwrap().name,
            "c2.vendor.baseline"
        );
    }

    #[test]
    fn adaptive_playback_needs_the_feature_and_a_decoder_artemis_allows() {
        assert!(adaptive_playback(&decoder("c2.goldfish.h264.decoder")));
        let without = Decoder {
            adaptive_playback: false,
            ..decoder("c2.exynos.h264.decoder")
        };
        assert!(!adaptive_playback(&without));
        assert!(!adaptive_playback(&decoder("OMX.Intel.hw_vd.h264")));
        assert!(!adaptive_playback(&decoder("OMX.MTK.VIDEO.DECODER.AVC")));
        assert!(adaptive_playback(&decoder("c2.mtk.avc.decoder")));
    }

    #[test]
    fn the_adaptive_maximum_is_the_largest_supported_square() {
        // A decoder that supports up to 4096x2304 holds a square of 2304.
        let asked = std::cell::RefCell::new(Vec::new());
        let max = adaptive_max((1280, 720), |side| {
            asked.borrow_mut().push(side);
            side <= 2304
        });
        assert_eq!(max, (2304, 2304));
        let asked = asked.into_inner();
        assert_eq!(
            asked[..3],
            [4096, 4088, 4080],
            "from 4096 down in steps of 8"
        );
        // Either orientation fits: a portrait stream too.
        assert_eq!(adaptive_max((720, 1280), |side| side <= 1920), (1920, 1920));
        assert_eq!(adaptive_max((1280, 720), |_| true), (4096, 4096));
    }

    #[test]
    fn with_no_square_the_maximum_is_the_streams_own_size() {
        assert_eq!(adaptive_max((1920, 1080), |_| false), (1920, 1080));
        // The stream's longer side is the smallest square asked.
        let mut asked = Vec::new();
        adaptive_max((4000, 720), |side| {
            asked.push(side);
            false
        });
        assert_eq!(asked.last(), Some(&4000));
        assert_eq!(adaptive_max((5000, 720), |_| true), (5000, 720));
    }

    #[test]
    fn tries_drop_the_riskiest_keys_first() {
        let goldfish = decoder("c2.goldfish.h264.decoder");
        let keys = |number| low_latency_try(&goldfish, number);
        assert_eq!(
            keys(0).keys,
            [("low-latency", 1), ("vdec-lowlatency", 1), ("priority", 0)]
        );
        assert_eq!(keys(1).keys, [("vdec-lowlatency", 1), ("priority", 0)]);
        assert_eq!(keys(2).keys, [("priority", 0)]);
        assert_eq!(
            keys(3),
            Try {
                keys: vec![],
                last: true
            }
        );
    }

    #[test]
    fn a_decoder_with_the_feature_gets_low_latency_alone_first() {
        let decoder = Decoder {
            low_latency: true,
            ..decoder("c2.qti.avc.decoder.low_latency")
        };
        assert_eq!(
            low_latency_try(&decoder, 0),
            Try {
                keys: vec![("low-latency", 1)],
                last: false
            }
        );
        assert_eq!(low_latency_try(&decoder, 1).keys[0], ("vdec-lowlatency", 1));
    }

    #[test]
    fn qualcomm_gets_its_vendor_keys_until_the_last_tries() {
        let c2 = decoder("c2.qti.avc.decoder");
        let first = low_latency_try(&c2, 0).keys;
        assert!(first.contains(&("operating-rate", 32767)));
        assert!(first.contains(&("vendor.qti-ext-dec-picture-order.enable", 1)));
        assert!(first.contains(&("vendor.qti-ext-output-fence.fence_type", 1)));
        assert_eq!(
            low_latency_try(&c2, 3).keys,
            [
                ("vendor.qti-ext-dec-picture-order.enable", 1),
                ("vendor.qti-ext-dec-low-latency.enable", 1),
                ("vendor.qti-ext-output-sw-fence-enable.value", 1),
                ("vendor.qti-ext-output-fence.enable", 1),
                ("vendor.qti-ext-output-fence.fence_type", 1),
            ]
        );
        assert_eq!(low_latency_try(&c2, 4).keys.len(), 4);
        assert!(low_latency_try(&c2, 5).last);
        let omx = low_latency_try(&decoder("OMX.qcom.video.decoder.avc"), 3).keys;
        assert_eq!(omx[0], ("vendor.qti-ext-dec-picture-order.enable", 0));
    }
}
