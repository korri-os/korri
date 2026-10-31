//! Word widths for the plan, shaped with the same font Slint draws.
//!
//! Zone sizes Anton so its cap height is the em of the pixel face it
//! stands in (zone-fonts.css `size-adjust`): a word "C px tall" is Anton
//! whose cap (1760 of 2048 units) is C px. Slint takes a font size, so
//! the size is `C * 2048 / 1760`, and the cap's top stands
//! `C * (ascent - cap) / cap` under the top of Slint's text box.

use harfrust::{FontRef, ShapeOptions, ShaperData, UnicodeBuffer};

pub const ANTON: &[u8] = include_bytes!("../ui/fonts/Anton-latin.ttf");

/// Anton's metrics in font units (head, OS/2 with USE_TYPO_METRICS).
const UPM: f32 = 2048.0;
const CAP: f32 = 1760.0;
const ASCENT: f32 = 2409.0;
const DESCENT: f32 = 674.0;

pub struct Measure {
    data: ShaperData,
    font: FontRef<'static>,
}

impl Measure {
    pub fn new() -> Self {
        let font = FontRef::new(ANTON).expect("Anton parses");
        let data = ShaperData::new(&font);
        Self { data, font }
    }

    /// Slint's font size for a cap `cap` px tall.
    pub fn font_size(&self, cap: f32) -> f32 {
        cap * UPM / CAP
    }

    /// From the top of Slint's text box to the top of the cap.
    pub fn above_cap(&self, cap: f32) -> f32 {
        cap * (ASCENT - CAP) / CAP
    }

    /// The height of one line of Slint's text box.
    pub fn line_height(&self, cap: f32) -> f32 {
        self.font_size(cap) * (ASCENT + DESCENT) / UPM
    }

    /// The advance of `text` at cap `cap`, kerned.
    pub fn width(&self, text: &str, cap: f32) -> f32 {
        let shaper = self.data.shaper(&self.font).build();
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let glyphs = shaper.shape(buffer, ShapeOptions::new());
        let units: i32 = glyphs.glyph_positions().iter().map(|g| g.x_advance).sum();
        units as f32 * self.font_size(cap) / UPM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Widths Chromium reported for the reference render (Caliper, rg353m
    /// and screen-720p), within a pixel.
    #[test]
    fn widths_match_chromium() {
        let m = Measure::new();
        for (t, cap, want) in [
            ("OPTIONS", 20.0, 70.42),
            ("SAVE DATA", 20.0, 90.16),
            ("GAMES", 40.0, 120.5),
            ("Hollow", 20.0, 62.47),
            ("hummingbird-lake", 30.0, 257.41),
            ("korri 0.4.1 \u{b7} 9a1fb22", 30.0, 269.2),
        ] {
            let got = m.width(t, cap);
            assert!((got - want).abs() < 1.0, "{t}: {got} against {want}");
        }
    }
}
