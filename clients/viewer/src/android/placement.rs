// SPDX-License-Identifier: GPL-3.0-or-later
//! Where the video layer goes in the app's window. Pure; the app applies it
//! with one `SurfaceControl.Transaction.setGeometry` call.

/// A rectangle in window pixels, as `android.graphics.Rect` holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// The largest rectangle with the stream's shape that fits the window,
/// centred: bars above and below, or at the sides. The stream is scaled up
/// or down; it is never cut or stretched.
pub fn letterbox(stream: (i32, i32), window: (i32, i32)) -> Rect {
    let (width, height) = (i64::from(stream.0.max(1)), i64::from(stream.1.max(1)));
    let (window_width, window_height) = (i64::from(window.0.max(0)), i64::from(window.1.max(0)));
    // Compare the two scales without floating point: width/height against
    // window_width/window_height.
    let (shown_width, shown_height) = if window_width * height <= window_height * width {
        (window_width, (window_width * height + width / 2) / width)
    } else {
        ((window_height * width + height / 2) / height, window_height)
    };
    let left = (window_width - shown_width) / 2;
    let top = (window_height - shown_height) / 2;
    Rect {
        left: left as i32,
        top: top as i32,
        right: (left + shown_width) as i32,
        bottom: (top + shown_height) as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn a_wide_window_gets_bars_at_the_sides() {
        // The proof's emulator: a 1080x2400 panel turned to landscape.
        assert_eq!(
            letterbox((1280, 720), (2400, 1080)),
            rect(240, 0, 2160, 1080)
        );
    }

    #[test]
    fn a_tall_window_gets_bars_above_and_below() {
        assert_eq!(
            letterbox((1280, 720), (1080, 2400)),
            rect(0, 896, 1080, 1504)
        );
    }

    #[test]
    fn a_window_of_the_same_shape_is_filled() {
        assert_eq!(letterbox((1280, 720), (1920, 1080)), rect(0, 0, 1920, 1080));
        assert_eq!(letterbox((1920, 1080), (1280, 720)), rect(0, 0, 1280, 720));
    }

    #[test]
    fn odd_sizes_stay_centred_and_inside() {
        let placed = letterbox((1280, 720), (2401, 1081));
        assert!(placed.left >= 0 && placed.right <= 2401 && placed.bottom <= 1081);
        assert!((placed.left - (2401 - placed.right)).abs() <= 1);
    }

    #[test]
    fn an_empty_window_gives_an_empty_rectangle() {
        assert_eq!(letterbox((1280, 720), (0, 0)), rect(0, 0, 0, 0));
    }
}
