//! The PICO-8 palette quantiser, ported unchanged from
//! `surfaces/pico/src/pico8-remap.ts` (and its palette from
//! `surfaces/pico/src/pico-palette.ts`). Contract 3.8: `{"$image": url,
//! "pico8": "flat" | "vivid"}`. The arithmetic follows the TypeScript: channel
//! values stay in floating point between the vivid lift and the distance, and
//! only the final palette entry is written back.

/// How hard to push an image toward the palette (`PicoPaletteMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Nearest palette entry.
    Flat,
    /// Lift saturation and contrast first, and penalise the greys.
    Vivid,
}

impl Mode {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "flat" => Some(Self::Flat),
            "vivid" => Some(Self::Vivid),
            _ => None,
        }
    }
}

/// The sixteen, in `PICO_PALETTE` order.
pub const PALETTE: [[u8; 3]; 16] = [
    [0x00, 0x00, 0x00], // black
    [0x1d, 0x2b, 0x53], // navy
    [0x7e, 0x25, 0x53], // maroon
    [0x00, 0x87, 0x51], // green
    [0xab, 0x52, 0x36], // brown
    [0x5f, 0x57, 0x4f], // slate
    [0xc2, 0xc3, 0xc7], // silver
    [0xff, 0xf1, 0xe8], // white
    [0xff, 0x00, 0x4d], // red
    [0xff, 0xa3, 0x00], // orange
    [0xff, 0xec, 0x27], // yellow
    [0x00, 0xe4, 0x36], // lime
    [0x29, 0xad, 0xff], // blue
    [0x83, 0x76, 0x9c], // lilac
    [0xff, 0x77, 0xa8], // pink
    [0xff, 0xcc, 0xaa], // peach
];

const SATURATION_BOOST: f64 = 1.55;
const CONTRAST: f64 = 1.12;
/// Distance added to grey candidates so colour wins ties in `vivid`.
const GREY_PENALTY: f64 = 26000.0;

fn saturation(rgb: [u8; 3]) -> f64 {
    let max = rgb.iter().copied().max().unwrap_or(0) as f64;
    let min = rgb.iter().copied().min().unwrap_or(0) as f64;
    if max == 0.0 {
        0.0
    } else {
        (max - min) / max
    }
}

fn clamp(value: f64) -> f64 {
    value.clamp(0.0, 255.0)
}

/// Rewrite RGBA pixels in place to their nearest PICO-8 colour. Fully
/// transparent pixels are left alone so cut-out art keeps its transparency.
pub fn quantize(data: &mut [u8], mode: Mode) {
    let vivid = mode == Mode::Vivid;
    let saturations = PALETTE.map(saturation);
    for pixel in data.chunks_exact_mut(4) {
        if pixel[3] == 0 {
            continue;
        }
        let (mut r, mut g, mut b) = (pixel[0] as f64, pixel[1] as f64, pixel[2] as f64);
        if vivid {
            let mean = (r + g + b) / 3.0;
            r = clamp((r - mean) * SATURATION_BOOST + mean);
            g = clamp((g - mean) * SATURATION_BOOST + mean);
            b = clamp((b - mean) * SATURATION_BOOST + mean);
            r = clamp((r - 128.0) * CONTRAST + 128.0);
            g = clamp((g - 128.0) * CONTRAST + 128.0);
            b = clamp((b - 128.0) * CONTRAST + 128.0);
        }
        let mut best = 0;
        let mut best_distance = f64::INFINITY;
        for (index, [pr, pg, pb]) in PALETTE.iter().enumerate() {
            let (dr, dg, db) = (r - *pr as f64, g - *pg as f64, b - *pb as f64);
            let mut distance = dr * dr + dg * dg + db * db;
            if vivid && saturations[index] < 0.2 {
                distance += GREY_PENALTY;
            }
            if distance < best_distance {
                best_distance = distance;
                best = index;
            }
        }
        pixel[..3].copy_from_slice(&PALETTE[best]);
    }
}

#[cfg(test)]
mod tests {
    //! The cases of `surfaces/pico/test/pico8-remap.test.ts`, as bytes.
    use super::*;

    fn hex(data: &[u8]) -> String {
        format!("#{:02x}{:02x}{:02x}", data[0], data[1], data[2])
    }

    #[test]
    fn snaps_a_near_match_to_the_exact_palette_entry() {
        let mut data = [0x2b, 0xac, 0xfd, 255];
        quantize(&mut data, Mode::Flat);
        assert_eq!(hex(&data), "#29adff");
    }

    #[test]
    fn leaves_an_exact_palette_colour_untouched() {
        let mut data = [0xff, 0x00, 0x4d, 255];
        quantize(&mut data, Mode::Flat);
        assert_eq!(hex(&data), "#ff004d");
    }

    #[test]
    fn maps_every_output_to_a_colour_in_the_palette() {
        let mut data = vec![0u8; 256 * 4];
        for i in 0..256 {
            data[i * 4] = ((i * 7) % 256) as u8;
            data[i * 4 + 1] = ((i * 13) % 256) as u8;
            data[i * 4 + 2] = ((i * 29) % 256) as u8;
            data[i * 4 + 3] = 255;
        }
        quantize(&mut data, Mode::Vivid);
        for pixel in data.chunks_exact(4) {
            assert!(PALETTE.iter().any(|entry| entry[..] == pixel[..3]));
        }
    }

    #[test]
    fn leaves_fully_transparent_pixels_alone() {
        let mut data = [12, 34, 56, 0];
        quantize(&mut data, Mode::Vivid);
        assert_eq!(data, [12, 34, 56, 0]);
    }

    #[test]
    fn vivid_pushes_a_muddy_colour_off_the_greys_flat_does_not() {
        let mut flat = [120, 110, 100, 255];
        let mut vivid = flat;
        quantize(&mut flat, Mode::Flat);
        quantize(&mut vivid, Mode::Vivid);
        assert_eq!(hex(&flat), "#5f574f");
        assert_ne!(hex(&vivid), "#5f574f");
    }

    #[test]
    fn palette_matches_pico_palette_ts_byte_for_byte() {
        let source = include_str!("../../../surfaces/pico/src/pico-palette.ts");
        let hexes: Vec<String> = source
            .lines()
            .filter_map(|line| line.split('"').nth(3).filter(|h| h.starts_with('#')))
            .map(str::to_owned)
            .collect();
        let ours: Vec<String> = PALETTE.iter().map(|rgb| hex(rgb)).collect();
        assert_eq!(hexes, ours);
    }
}
