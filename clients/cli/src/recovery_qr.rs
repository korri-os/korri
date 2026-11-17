//! The recovery QR code (ADR 0072) as terminal text. The code holds the 12
//! recovery words as plain text, so any phone camera shows them.
//!
//! The text that draws the code is as secret as the words. It lives in
//! memory that is erased on drop. The `qrcode` library's own buffers are
//! not erased; it gives no way to.
use crate::error::CliError;
use qrcode::{Color, EcLevel, QrCode};
use zeroize::Zeroizing;

/// The light border that the QR standard asks for, in modules.
const QUIET_ZONE: usize = 4;
/// Medium error correction: a smudged or folded paper copy still scans.
const ERROR_CORRECTION: EcLevel = EcLevel::M;

/// Draw the QR code of `text` with half blocks, two module rows to a line.
/// Light modules are drawn and dark modules stay blank, so the code reads
/// right on a terminal with light text on a dark background.
pub fn render(text: &str) -> Result<Zeroizing<String>, CliError> {
    let code =
        QrCode::with_error_correction_level(text.as_bytes(), ERROR_CORRECTION).map_err(|_| {
            CliError::failure(
                "RecoveryQrUnavailable",
                "Korri could not draw the QR code of the recovery words.",
                "Write down the words. They are the same key as the QR code.",
            )
        })?;
    let modules = code.width();
    let size = modules + 2 * QUIET_ZONE;
    let light = |x: usize, y: usize| {
        let inside = |place: usize| (QUIET_ZONE..QUIET_ZONE + modules).contains(&place);
        !(inside(x) && inside(y)) || code[(x - QUIET_ZONE, y - QUIET_ZONE)] == Color::Light
    };
    // Each half block is 3 bytes of UTF-8. Reserve all of it, so the buffer
    // never moves and leaves no copy behind.
    let lines = size.div_ceil(2);
    let mut drawn = Zeroizing::new(String::with_capacity(lines * (size * 3 + 1)));
    for y in (0..size).step_by(2) {
        for x in 0..size {
            let top = light(x, y);
            let bottom = y + 1 < size && light(x, y + 1);
            drawn.push(match (top, bottom) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        drawn.push('\n');
    }
    Ok(drawn)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORDS: &str =
        "leader monkey parrot ring guide accident before fence cannon height naive bean";
    /// Pixels per module in the picture the decoder reads.
    const SCALE: usize = 4;

    /// Turn the drawn text back into a picture, as a camera sees a dark
    /// terminal: a drawn half block is light, a blank is dark.
    fn picture(drawn: &str) -> (usize, usize, Vec<u8>) {
        let lines: Vec<Vec<char>> = drawn.lines().map(|line| line.chars().collect()).collect();
        let width = lines[0].len();
        let height = lines.len() * 2;
        let mut modules = vec![false; width * height];
        for (line, characters) in lines.iter().enumerate() {
            assert_eq!(characters.len(), width);
            for (x, character) in characters.iter().enumerate() {
                let (top, bottom) = match character {
                    '█' => (true, true),
                    '▀' => (true, false),
                    '▄' => (false, true),
                    ' ' => (false, false),
                    other => panic!("unexpected character {other:?}"),
                };
                modules[(line * 2) * width + x] = top;
                modules[(line * 2 + 1) * width + x] = bottom;
            }
        }
        let pixels = (0..height * SCALE)
            .flat_map(|y| (0..width * SCALE).map(move |x| (x, y)))
            .map(|(x, y)| {
                if modules[(y / SCALE) * width + x / SCALE] {
                    255
                } else {
                    0
                }
            })
            .collect();
        (width * SCALE, height * SCALE, pixels)
    }

    fn decode(drawn: &str) -> Vec<String> {
        let (width, height, pixels) = picture(drawn);
        let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| {
            pixels[y * width + x]
        });
        image
            .detect_grids()
            .into_iter()
            .map(|grid| grid.decode().unwrap().1)
            .collect()
    }

    #[test]
    fn a_camera_reads_the_words_as_plain_text() {
        let drawn = render(WORDS).unwrap();
        assert_eq!(decode(&drawn), vec![WORDS.to_owned()]);
    }

    #[test]
    fn the_longest_words_still_fit_a_terminal() {
        // Twelve 8-letter BIP39 words: the most text a set of words can be.
        let longest = ["abstract"; 12].join(" ");
        let drawn = render(&longest).unwrap();
        assert_eq!(decode(&drawn), vec![longest]);
        let columns = drawn.lines().map(|line| line.chars().count()).max();
        assert!(columns.is_some_and(|columns| columns <= 80));
        assert!(drawn.lines().count() <= 40);
    }

    #[test]
    fn the_code_has_its_light_border() {
        let drawn = render(WORDS).unwrap();
        let lines: Vec<&str> = drawn.lines().collect();
        // Two lines of full blocks are the top 4 module rows.
        for line in &lines[..QUIET_ZONE / 2] {
            assert!(line.chars().all(|character| character == '█'));
        }
        for line in &lines {
            assert!(line.chars().take(QUIET_ZONE).all(|c| c != ' ' && c != '▄'));
        }
    }
}
