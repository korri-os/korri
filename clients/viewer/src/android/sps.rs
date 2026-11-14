// SPDX-License-Identifier: GPL-3.0-or-later
//! The H.264 SPS rewrite that Artemis applies before Android's decoder sees
//! the stream (`MediaCodecDecoderRenderer.submitDecodeUnit`, jcodec's
//! `SeqParameterSet`), so that the decoder holds no more frames than the
//! stream needs. Pure: the decoder calls it with each SPS that arrives.
//!
//! The rewrite reads every syntax element of the SPS (ITU-T H.264, 7.3.2.1.1
//! and E.1.1) and writes it again, changing only the fields of a [`Patch`].
//! A patch that changes nothing gives the same bytes back.
use anyhow::{Result, bail, ensure};

/// What Artemis changes, for a stream without reference frame invalidation
/// and on Android 8 or newer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Patch {
    /// `level_idc`: the lowest level for the stream's size, as some decoders
    /// size their buffer queue by level.
    pub level_idc: Option<u8>,
    /// `max_num_ref_frames`.
    pub num_ref_frames: Option<u32>,
    /// Add VUI and its bitstream restriction when missing, and set
    /// `max_dec_frame_buffering` to the reference frame count.
    pub restrict_buffering: bool,
    /// `constraint_set4_flag` and `constraint_set5_flag` together: Constrained
    /// High for profile 100 on the decoders that want it, else unset.
    pub constrained_high: Option<bool>,
}

impl Patch {
    /// A patch that changes nothing.
    #[cfg(test)]
    pub const NONE: Patch = Patch {
        level_idc: None,
        num_ref_frames: None,
        restrict_buffering: false,
        constrained_high: None,
    };

    /// Artemis's patch for a stream of this size and rate, without reference
    /// frame invalidation. `constrained_high` decoders: `omx.intel`.
    pub fn for_stream(width: i32, height: i32, fps: i32, decoder: &str) -> Patch {
        let level_idc = if width <= 720 && height <= 480 && fps <= 60 {
            // Up to 5 buffered frames at 720x480x60.
            Some(31)
        } else if width <= 1280 && height <= 720 && fps <= 60 {
            Some(32)
        } else if width <= 1920 && height <= 1080 && fps <= 60 {
            Some(42)
        } else {
            None
        };
        Patch {
            level_idc,
            num_ref_frames: Some(1),
            restrict_buffering: true,
            constrained_high: Some(decoder.to_ascii_lowercase().starts_with("omx.intel")),
        }
    }
}

/// The fields of an SPS that a patch can change, as read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fields {
    pub profile_idc: u8,
    pub constraint_set4: bool,
    pub constraint_set5: bool,
    pub level_idc: u8,
    pub num_ref_frames: u32,
    pub vui: bool,
    /// `max_num_reorder_frames` and `max_dec_frame_buffering`, when the VUI
    /// has a bitstream restriction.
    pub restriction: Option<(u32, u32)>,
}

/// Read an SPS NAL unit (its header byte and escaped payload, no start code).
#[cfg(test)]
pub fn read(nal: &[u8]) -> Result<Fields> {
    Ok(rewrite_with(nal, Patch::NONE)?.1)
}

/// Rewrite an SPS NAL unit (its header byte and escaped payload, no start
/// code) with a patch. The result is a NAL unit in the same form.
pub fn rewrite(nal: &[u8], patch: Patch) -> Result<Vec<u8>> {
    Ok(rewrite_with(nal, patch)?.0)
}

fn rewrite_with(nal: &[u8], patch: Patch) -> Result<(Vec<u8>, Fields)> {
    ensure!(
        nal.first().map(|header| header & 0x1f) == Some(7),
        "not an SPS NAL unit"
    );
    let rbsp = unescape(&nal[1..]);
    let mut s = Copy {
        reader: Reader::new(&rbsp),
        writer: Writer::default(),
    };
    let fields = sps(&mut s, patch)?;
    s.writer.trailing_bits();
    let mut out = vec![nal[0]];
    out.extend(escape(&s.writer.bytes));
    Ok((out, fields))
}

const HIGH_PROFILES: [u8; 13] = [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135];

fn sps(s: &mut Copy, patch: Patch) -> Result<Fields> {
    let mut f = Fields {
        profile_idc: s.u(8)? as u8,
        ..Fields::default()
    };
    s.u(4)?; // constraint_set0..3_flag
    let flags = s.reader.u(2)?;
    let (set4, set5) = match patch.constrained_high {
        Some(on) => {
            let on = on && f.profile_idc == 100;
            (on, on)
        }
        None => (flags & 2 != 0, flags & 1 != 0),
    };
    s.writer.u(2, u64::from(set4) << 1 | u64::from(set5));
    (f.constraint_set4, f.constraint_set5) = (set4, set5);
    s.u(2)?; // reserved_zero_2bits
    let level = s.reader.u(8)? as u8;
    f.level_idc = patch.level_idc.unwrap_or(level);
    s.writer.u(8, f.level_idc.into());
    s.ue()?; // seq_parameter_set_id
    if HIGH_PROFILES.contains(&f.profile_idc) {
        let chroma_format_idc = s.ue()?;
        if chroma_format_idc == 3 {
            s.u(1)?; // separate_colour_plane_flag
        }
        s.ue()?; // bit_depth_luma_minus8
        s.ue()?; // bit_depth_chroma_minus8
        s.u(1)?; // qpprime_y_zero_transform_bypass_flag
        if s.u(1)? == 1 {
            // seq_scaling_matrix_present_flag
            for list in 0..if chroma_format_idc == 3 { 12 } else { 8 } {
                if s.u(1)? == 1 {
                    scaling_list(s, if list < 6 { 16 } else { 64 })?;
                }
            }
        }
    }
    s.ue()?; // log2_max_frame_num_minus4
    match s.ue()? {
        // pic_order_cnt_type
        0 => {
            s.ue()?; // log2_max_pic_order_cnt_lsb_minus4
        }
        1 => {
            s.u(1)?; // delta_pic_order_always_zero_flag
            s.se()?; // offset_for_non_ref_pic
            s.se()?; // offset_for_top_to_bottom_field
            for _ in 0..s.ue()? {
                s.se()?; // offset_for_ref_frame
            }
        }
        2 => {}
        other => bail!("pic_order_cnt_type {other} is out of range"),
    }
    let refs = s.reader.ue()?;
    f.num_ref_frames = patch.num_ref_frames.unwrap_or(refs);
    s.writer.ue(f.num_ref_frames);
    s.u(1)?; // gaps_in_frame_num_value_allowed_flag
    s.ue()?; // pic_width_in_mbs_minus1
    s.ue()?; // pic_height_in_map_units_minus1
    if s.u(1)? == 0 {
        // frame_mbs_only_flag
        s.u(1)?; // mb_adaptive_frame_field_flag
    }
    s.u(1)?; // direct_8x8_inference_flag
    if s.u(1)? == 1 {
        // frame_cropping_flag: left, right, top, bottom offsets
        for _ in 0..4 {
            s.ue()?;
        }
    }
    let vui = s.reader.u(1)? == 1;
    f.vui = vui || patch.restrict_buffering;
    s.writer.u(1, f.vui.into());
    if vui {
        vui_parameters(s, patch, &mut f)?;
    } else if patch.restrict_buffering {
        // Artemis's new VUIParameters: every flag clear but the restriction.
        for _ in 0..8 {
            // aspect ratio, overscan, video signal type, chroma location,
            // timing, NAL HRD, VCL HRD, pic_struct
            s.writer.u(1, 0);
        }
        s.writer.u(1, 1);
        f.restriction = Some(new_restriction(s, f.num_ref_frames));
    }
    Ok(f)
}

/// Artemis's new `BitstreamRestriction`, written.
fn new_restriction(s: &mut Copy, num_ref_frames: u32) -> (u32, u32) {
    s.writer.u(1, 1); // motion_vectors_over_pic_boundaries_flag
    s.writer.ue(2); // max_bytes_per_pic_denom
    s.writer.ue(1); // max_bits_per_mb_denom
    s.writer.ue(16); // log2_max_mv_length_horizontal
    s.writer.ue(16); // log2_max_mv_length_vertical
    s.writer.ue(0); // max_num_reorder_frames
    s.writer.ue(num_ref_frames); // max_dec_frame_buffering
    (0, num_ref_frames)
}

fn scaling_list(s: &mut Copy, size: usize) -> Result<()> {
    let (mut last, mut next) = (8i64, 8i64);
    for _ in 0..size {
        if next != 0 {
            let delta = s.se()?;
            next = (last + delta + 256).rem_euclid(256);
        }
        if next != 0 {
            last = next;
        }
    }
    Ok(())
}

fn vui_parameters(s: &mut Copy, patch: Patch, f: &mut Fields) -> Result<()> {
    if s.u(1)? == 1 {
        // aspect_ratio_info_present_flag
        if s.u(8)? == 255 {
            // Extended_SAR: sar_width, sar_height
            s.u(16)?;
            s.u(16)?;
        }
    }
    if s.u(1)? == 1 {
        s.u(1)?; // overscan_appropriate_flag
    }
    if s.u(1)? == 1 {
        // video_signal_type_present_flag
        s.u(3)?; // video_format
        s.u(1)?; // video_full_range_flag
        if s.u(1)? == 1 {
            // colour_primaries, transfer_characteristics, matrix_coefficients
            s.u(24)?;
        }
    }
    if s.u(1)? == 1 {
        // chroma_loc_info_present_flag
        s.ue()?;
        s.ue()?;
    }
    if s.u(1)? == 1 {
        // timing_info_present_flag
        s.u(32)?; // num_units_in_tick
        s.u(32)?; // time_scale
        s.u(1)?; // fixed_frame_rate_flag
    }
    let nal_hrd = s.u(1)? == 1;
    if nal_hrd {
        hrd_parameters(s)?;
    }
    let vcl_hrd = s.u(1)? == 1;
    if vcl_hrd {
        hrd_parameters(s)?;
    }
    if nal_hrd || vcl_hrd {
        s.u(1)?; // low_delay_hrd_flag
    }
    s.u(1)?; // pic_struct_present_flag
    let restriction = s.reader.u(1)? == 1;
    s.writer
        .u(1, (restriction || patch.restrict_buffering).into());
    if restriction {
        s.u(1)?; // motion_vectors_over_pic_boundaries_flag
        s.ue()?; // max_bytes_per_pic_denom
        s.ue()?; // max_bits_per_mb_denom
        s.ue()?; // log2_max_mv_length_horizontal
        s.ue()?; // log2_max_mv_length_vertical
        let reorder = s.ue()?;
        let buffering = s.reader.ue()?;
        let buffering = if patch.restrict_buffering {
            // Some decoders refuse max_dec_frame_buffering < num_ref_frames.
            f.num_ref_frames
        } else {
            buffering
        };
        s.writer.ue(buffering);
        f.restriction = Some((reorder, buffering));
    } else if patch.restrict_buffering {
        f.restriction = Some(new_restriction(s, f.num_ref_frames));
    }
    Ok(())
}

fn hrd_parameters(s: &mut Copy) -> Result<()> {
    let cpb_cnt_minus1 = s.ue()?;
    ensure!(cpb_cnt_minus1 <= 31, "cpb_cnt_minus1 is out of range");
    s.u(4)?; // bit_rate_scale
    s.u(4)?; // cpb_size_scale
    for _ in 0..=cpb_cnt_minus1 {
        s.ue()?; // bit_rate_value_minus1
        s.ue()?; // cpb_size_value_minus1
        s.u(1)?; // cbr_flag
    }
    // initial_cpb_removal_delay_length_minus1, cpb_removal_delay_length_minus1,
    // dpb_output_delay_length_minus1, time_offset_length
    s.u(20)?;
    Ok(())
}

/// Each syntax element read is written again unless the caller writes it.
struct Copy<'a> {
    reader: Reader<'a>,
    writer: Writer,
}

impl Copy<'_> {
    fn u(&mut self, bits: u32) -> Result<u32> {
        let value = self.reader.u(bits)?;
        self.writer.u(bits, value.into());
        Ok(value)
    }
    fn ue(&mut self) -> Result<u32> {
        let value = self.reader.ue()?;
        self.writer.ue(value);
        Ok(value)
    }
    fn se(&mut self) -> Result<i64> {
        let code = self.ue()?;
        // 9.1.1: 1, 2, 3, 4 … map to 1, −1, 2, −2 …
        let magnitude = i64::from(code.div_ceil(2));
        Ok(if code % 2 == 1 { magnitude } else { -magnitude })
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    bit: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit: 0 }
    }
    fn u(&mut self, bits: u32) -> Result<u32> {
        let mut value = 0u32;
        for _ in 0..bits {
            let byte = *self
                .bytes
                .get(self.bit / 8)
                .ok_or_else(|| anyhow::anyhow!("the SPS ends early"))?;
            value = value << 1 | u32::from(byte >> (7 - self.bit % 8) & 1);
            self.bit += 1;
        }
        Ok(value)
    }
    /// 9.1: Exp-Golomb, up to 32 leading zero bits.
    fn ue(&mut self) -> Result<u32> {
        let mut zeros = 0;
        while self.u(1)? == 0 {
            zeros += 1;
            ensure!(zeros < 32, "an Exp-Golomb code is too long");
        }
        Ok(((1u64 << zeros) - 1 + u64::from(self.u(zeros)?)) as u32)
    }
}

#[derive(Default)]
struct Writer {
    bytes: Vec<u8>,
    bit: usize,
}

impl Writer {
    fn u(&mut self, bits: u32, value: u64) {
        for index in (0..bits).rev() {
            if self.bit.is_multiple_of(8) {
                self.bytes.push(0);
            }
            let last = self.bytes.len() - 1;
            self.bytes[last] |= ((value >> index & 1) as u8) << (7 - self.bit % 8);
            self.bit += 1;
        }
    }
    fn ue(&mut self, value: u32) {
        let code = u64::from(value) + 1;
        let length = 64 - code.leading_zeros();
        self.u(length - 1, 0);
        self.u(length, code);
    }
    /// 7.3.2.11: rbsp_stop_one_bit, then zero bits to the byte boundary.
    fn trailing_bits(&mut self) {
        self.u(1, 1);
        while !self.bit.is_multiple_of(8) {
            self.u(1, 0);
        }
    }
}

/// 7.4.1: remove each emulation prevention byte (0x03 after two zero bytes).
fn unescape(escaped: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(escaped.len());
    let mut zeros = 0;
    for &byte in escaped {
        if zeros >= 2 && byte == 3 {
            zeros = 0;
            continue;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        out.push(byte);
    }
    out
}

/// 7.4.1: insert an emulation prevention byte where two zero bytes would be
/// followed by a byte of 3 or less.
fn escape(rbsp: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rbsp.len() + 4);
    let mut zeros = 0;
    for &byte in rbsp {
        if zeros >= 2 && byte <= 3 {
            out.push(3);
            zeros = 0;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        out.push(byte);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real SPS NAL units from libx264 (`ffmpeg -f lavfi -i testsrc
    /// -pix_fmt yuv420p -c:v libx264 …`), each chosen for a path of the
    /// syntax, then two that libx264 never writes, made with the writer.
    fn samples() -> Vec<(&'static str, Vec<u8>)> {
        let mut samples: Vec<_> = [
            // High, 1280x720@60, -tune zerolatency.
            (
                "high-720p",
                "67640020acb200a00b760220000003002000000f11e30649",
            ),
            // High, 1920x1080@60: frame cropping (1088 coded lines).
            (
                "high-1080p-cropped",
                "6764002aacd940780227e5c044000003000400000301e03c60c658",
            ),
            // Baseline, 1280x720@30: no high-profile fields.
            (
                "baseline",
                "6742c01fd9005005bb0110000003001000000303c0f1832480",
            ),
            // High with nal-hrd=vbr: NAL HRD parameters in the VUI.
            (
                "high-nal-hrd",
                "67640020acd9405005bb011000000300100000078646000f424001e84a2d38078c18cb",
            ),
            // High, 640x480 interlaced: frame_mbs_only_flag clear.
            (
                "high-interlaced",
                "6764001eacd940a07b6022000003000200000300783e28532c",
            ),
        ]
        .into_iter()
        .map(|(name, hex)| (name, decode_hex(hex)))
        .collect();
        samples.push(("scaling-lists-poc-1", synthetic()));
        samples
    }

    /// High profile with explicit scaling lists (one with a zero next scale)
    /// and pic_order_cnt_type 1, and no VUI.
    fn synthetic() -> Vec<u8> {
        let mut w = Writer::default();
        w.u(8, 100); // profile_idc
        w.u(8, 0); // constraint flags, reserved
        w.u(8, 40); // level_idc
        w.ue(0); // seq_parameter_set_id
        w.ue(1); // chroma_format_idc
        w.ue(0);
        w.ue(0);
        w.u(1, 0);
        w.u(1, 1); // seq_scaling_matrix_present_flag
        for list in 0..8 {
            let present = list % 3 != 2;
            w.u(1, present.into());
            if present {
                let size = if list < 6 { 16 } else { 64 };
                for j in 0..size {
                    // A delta that ends the list early (next scale 0) in list 4.
                    let delta: i64 = if list == 4 && j == 3 { -8 - 3 } else { 1 };
                    let code = if delta > 0 { 2 * delta - 1 } else { -2 * delta };
                    w.ue(code as u32);
                    if list == 4 && j == 3 {
                        break;
                    }
                }
            }
        }
        w.ue(0); // log2_max_frame_num_minus4
        w.ue(1); // pic_order_cnt_type
        w.u(1, 0);
        w.ue(3); // offset_for_non_ref_pic: se 2
        w.ue(4); // offset_for_top_to_bottom_field: se −2
        w.ue(2); // num_ref_frames_in_pic_order_cnt_cycle
        w.ue(1); // offset_for_ref_frame: se 1
        w.ue(2); // se −1
        w.ue(4); // max_num_ref_frames
        w.u(1, 0);
        w.ue(79); // 1280
        w.ue(44); // 720
        w.u(1, 1); // frame_mbs_only_flag
        w.u(1, 1);
        w.u(1, 0); // frame_cropping_flag
        w.u(1, 0); // vui_parameters_present_flag
        w.trailing_bits();
        let mut nal = vec![0x67];
        nal.extend(escape(&w.bytes));
        nal
    }

    fn decode_hex(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn a_patch_that_changes_nothing_gives_the_same_bytes() {
        for (name, nal) in samples() {
            assert_eq!(rewrite(&nal, Patch::NONE).unwrap(), nal, "{name}");
        }
    }

    #[test]
    fn artemis_patch_sets_level_references_and_buffering() {
        for (name, nal) in samples() {
            let before = read(&nal).unwrap();
            let patched = rewrite(
                &nal,
                Patch::for_stream(1280, 720, 60, "c2.goldfish.h264.decoder"),
            )
            .unwrap();
            let after = read(&patched).unwrap();
            assert_eq!(after.level_idc, 32, "{name}");
            assert_eq!(after.num_ref_frames, 1, "{name}");
            assert!(after.vui, "{name}");
            let reorder = before.restriction.map_or(0, |(reorder, _)| reorder);
            assert_eq!(after.restriction, Some((reorder, 1)), "{name}");
            assert!(!after.constraint_set4 && !after.constraint_set5, "{name}");
            assert_eq!(after.profile_idc, before.profile_idc, "{name}");
            // The rewrite is stable: patching again changes nothing.
            assert_eq!(
                rewrite(
                    &patched,
                    Patch::for_stream(1280, 720, 60, "c2.goldfish.h264.decoder")
                )
                .unwrap(),
                patched,
                "{name}"
            );
        }
    }

    #[test]
    fn the_synthetic_sample_reads_past_its_scaling_lists() {
        let fields = read(&synthetic()).unwrap();
        assert_eq!(fields.level_idc, 40);
        assert_eq!(fields.num_ref_frames, 4);
        assert!(!fields.vui);
    }

    #[test]
    fn an_sps_without_vui_gains_artemis_bitstream_restriction() {
        let patched = rewrite(&synthetic(), Patch::for_stream(1920, 1080, 60, "x")).unwrap();
        let after = read(&patched).unwrap();
        assert_eq!(after.level_idc, 42);
        assert!(after.vui);
        assert_eq!(after.restriction, Some((0, 1)));
    }

    #[test]
    fn constrained_high_is_set_only_for_profile_100_on_intel() {
        let (_, high) = samples().remove(0);
        let intel = read(
            &rewrite(
                &high,
                Patch::for_stream(1280, 720, 60, "OMX.Intel.hw_vd.h264"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(intel.constraint_set4 && intel.constraint_set5);
        let (_, baseline) = samples().remove(2);
        let other = read(
            &rewrite(
                &baseline,
                Patch::for_stream(1280, 720, 60, "OMX.Intel.hw_vd.h264"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!other.constraint_set4 && !other.constraint_set5);
    }

    #[test]
    fn levels_follow_the_stream_size_and_rate() {
        let level = |w, h, fps| Patch::for_stream(w, h, fps, "c2.x").level_idc;
        assert_eq!(level(720, 480, 60), Some(31));
        assert_eq!(level(1280, 720, 60), Some(32));
        assert_eq!(level(1920, 1080, 60), Some(42));
        assert_eq!(level(1920, 1080, 120), None);
        assert_eq!(level(2560, 1440, 60), None);
    }

    #[test]
    fn emulation_prevention_survives_the_round_trip() {
        let rbsp = [0, 0, 1, 0, 0, 0, 0, 0, 3, 0, 0, 2, 7];
        let escaped = escape(&rbsp);
        assert_eq!(escaped, [0, 0, 3, 1, 0, 0, 3, 0, 0, 3, 0, 3, 0, 0, 3, 2, 7]);
        assert_eq!(unescape(&escaped), rbsp);
    }

    #[test]
    fn a_short_or_foreign_nal_is_refused() {
        assert!(read(&[0x68, 0xee, 0x3c, 0x80]).is_err());
        assert!(read(&[0x67, 0x64]).is_err());
    }
}
