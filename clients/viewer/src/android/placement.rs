// SPDX-License-Identifier: GPL-3.0-or-later
//! Where the video layer goes in the app's window. Pure; the app applies it
//! with one `SurfaceControl.Transaction.setGeometry` call.
//!
//! Two placements, as Artemis's stream view has (`StreamContainer.onMeasure`):
//! the picture fits the window with bars (its default, `ScaleMode.FIT`), or
//! it fills the window and its edges are cut ("Crop to fill", slice D11).

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

/// The smallest rectangle with the stream's shape that covers the window,
/// centred: Artemis's fill (`StreamContainer.setFillDisplay`). It reaches
/// past the window on two sides; those parts are cut.
pub fn fill(stream: (i32, i32), window: (i32, i32)) -> Rect {
    let (width, height) = (i64::from(stream.0.max(1)), i64::from(stream.1.max(1)));
    let (window_width, window_height) = (i64::from(window.0.max(0)), i64::from(window.1.max(0)));
    let (shown_width, shown_height) = if window_width * height >= window_height * width {
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

/// The picture's place: filled, or fitted with bars.
pub fn place(stream: (i32, i32), window: (i32, i32), filled: bool) -> Rect {
    if filled {
        fill(stream, window)
    } else {
        letterbox(stream, window)
    }
}

/// What `setGeometry` takes for a picture placed at `placed` in the window:
/// the part of the stream that shows (its source crop, in stream pixels) and
/// where that part goes, inside the window. A fitted picture shows all of
/// the stream; a filled one shows its middle across the whole window.
pub fn geometry(stream: (i32, i32), placed: Rect, window: (i32, i32)) -> (Rect, Rect) {
    let whole = Rect {
        left: 0,
        top: 0,
        right: stream.0,
        bottom: stream.1,
    };
    let shown = Rect {
        left: placed.left.max(0),
        top: placed.top.max(0),
        right: placed.right.min(window.0),
        bottom: placed.bottom.min(window.1),
    };
    let (placed_width, placed_height) = (placed.right - placed.left, placed.bottom - placed.top);
    if shown.right <= shown.left
        || shown.bottom <= shown.top
        || placed_width <= 0
        || placed_height <= 0
    {
        return (whole, placed);
    }
    // A window position, `from` pixels into the picture's `extent`, in the
    // stream's `size`, rounded.
    let source = |at: i32, from: i32, extent: i32, size: i32| {
        let extent = i64::from(extent);
        ((i64::from(at - from) * i64::from(size) + extent / 2) / extent) as i32
    };
    (
        Rect {
            left: source(shown.left, placed.left, placed_width, stream.0),
            top: source(shown.top, placed.top, placed_height, stream.1),
            right: source(shown.right, placed.left, placed_width, stream.0),
            bottom: source(shown.bottom, placed.top, placed_height, stream.1),
        },
        shown,
    )
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

    #[test]
    fn a_filled_picture_covers_the_window_and_is_cut_above_and_below() {
        // The proof's emulator in landscape: 2400 / 1280 = 1.875, so 720
        // rows become 1350 and 135 go above and below the window.
        let placed = fill((1280, 720), (2400, 1080));
        assert_eq!(placed, rect(0, -135, 2400, 1215));
        let (source, shown) = geometry((1280, 720), placed, (2400, 1080));
        assert_eq!(shown, rect(0, 0, 2400, 1080));
        // 1080 / 1.875 = 576 of the stream's middle rows.
        assert_eq!(source, rect(0, 72, 1280, 648));
    }

    #[test]
    fn a_filled_picture_in_a_tall_window_is_cut_at_the_sides() {
        let placed = fill((1280, 720), (1080, 2400));
        assert_eq!((placed.top, placed.bottom), (0, 2400));
        assert!(placed.left < 0 && placed.right > 1080);
        let (source, shown) = geometry((1280, 720), placed, (1080, 2400));
        assert_eq!(shown, rect(0, 0, 1080, 2400));
        assert_eq!((source.top, source.bottom), (0, 720));
        // 1080 of 4267 window pixels: 324 of the stream's middle columns.
        assert_eq!((source.left, source.right), (478, 802));
    }

    #[test]
    fn a_window_of_the_streams_shape_is_filled_either_way() {
        assert_eq!(fill((1280, 720), (1920, 1080)), rect(0, 0, 1920, 1080));
        assert_eq!(
            place((1280, 720), (1920, 1080), true),
            place((1280, 720), (1920, 1080), false)
        );
    }

    #[test]
    fn a_fitted_picture_shows_the_whole_stream_where_it_is_placed() {
        for window in [(2400, 1080), (1080, 2400), (2401, 1081), (1280, 720)] {
            let placed = letterbox((1280, 720), window);
            assert_eq!(
                geometry((1280, 720), placed, window),
                (rect(0, 0, 1280, 720), placed),
                "{window:?}"
            );
        }
    }

    #[test]
    fn place_chooses_fill_or_fit() {
        assert_eq!(
            place((1280, 720), (2400, 1080), true),
            fill((1280, 720), (2400, 1080))
        );
        assert_eq!(
            place((1280, 720), (2400, 1080), false),
            letterbox((1280, 720), (2400, 1080))
        );
    }
}
