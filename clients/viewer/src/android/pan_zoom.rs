// SPDX-License-Identifier: GPL-3.0-only
//! Pan and zoom of the stream's picture: the stream control "Pan & zoom",
//! Artemis's `PanZoomHandler`. While it is on, the touch screen's gestures
//! (`gestures.rs`) scale the picture about the fingers' middle, from 1 to 10
//! times, and drag it; the picture always covers the place it had, or stays
//! centred in it. The zoom stays when the control goes off. Pure; the video
//! layer goes where [`PanZoom::rect`] says (`layer.rs`), and the input maps
//! touches into that place.
//!
//! Ported from Korri's copy of Artemis (`clients/android` at the tag `artemis-final`, from
//! `simonwjackson/artemis` at `cf12c432`): `utils/PanZoomHandler.java`.
//! Artemis moves and scales its stream view inside its stream container,
//! with the pivot at the view's corner; here the container is the picture's
//! place without zoom (fitted or filled), and the view the zoomed place.
//! Artemis ships under the GNU GPL version 3 with no "or later" grant, so
//! this file does too. The Rust adaptation is Korri work dated 2026-11-17.

use crate::{gestures::Gesture, placement::Rect};

/// `PanZoomHandler.MAX_SCALE`.
const MAX_SCALE: f32 = 10.0;

/// The picture's zoom: its scale, and its corner's place in its container,
/// as Artemis's stream view has them (`scaleFactor`, `childX`, `childY`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanZoom {
    scale: f32,
    child: (f32, f32),
    /// The container's size, as `updateDimensions` last read it.
    parent: (f32, f32),
    /// A gesture measured the picture (`childWidth` is not 0): a new place
    /// keeps its zoom from then on.
    measured: bool,
}

impl Default for PanZoom {
    fn default() -> Self {
        Self {
            scale: 1.0,
            child: (0.0, 0.0),
            parent: (0.0, 0.0),
            measured: false,
        }
    }
}

fn size(rect: Rect) -> (f32, f32) {
    (
        (rect.right - rect.left) as f32,
        (rect.bottom - rect.top) as f32,
    )
}

impl PanZoom {
    /// `getScaleFactor`.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Where the picture goes for a container at `container`: its corner
    /// moved and its size scaled.
    pub fn rect(&self, container: Rect) -> Rect {
        let (width, height) = size(container);
        let left = container.left + self.child.0.round() as i32;
        let top = container.top + self.child.1.round() as i32;
        Rect {
            left,
            top,
            right: left + (width * self.scale).round() as i32,
            bottom: top + (height * self.scale).round() as i32,
        }
    }

    /// `constrainToBounds`: a larger picture covers its container, a
    /// smaller one is centred in it.
    fn constrain(&mut self, parent: (f32, f32)) {
        self.parent = parent;
        self.measured = true;
        let child = (parent.0 * self.scale, parent.1 * self.scale);
        self.child.0 = if parent.0 >= child.0 {
            (parent.0 - child.0) / 2.0
        } else {
            self.child.0.min(0.0).max(parent.0 - child.0)
        };
        self.child.1 = if parent.1 >= child.1 {
            (parent.1 - child.1) / 2.0
        } else {
            self.child.1.min(0.0).max(parent.1 - child.1)
        };
    }

    /// One gesture of the touch screen, for a container at `container`;
    /// its focus is in window pixels. True when the picture moved.
    pub fn gesture(&mut self, gesture: Gesture, container: Rect) -> bool {
        let before = (self.scale, self.child);
        let parent = size(container);
        match gesture {
            // `ScaleListener.onScale`: the point under the focus stays.
            Gesture::Scale { focus, factor } => {
                let focus = (
                    focus.0 - container.left as f32,
                    focus.1 - container.top as f32,
                );
                let scale = (self.scale * factor).clamp(1.0, MAX_SCALE);
                self.child.0 = focus.0 + (self.child.0 - focus.0) / self.scale * scale;
                self.child.1 = focus.1 + (self.child.1 - focus.1) / self.scale * scale;
                self.scale = scale;
                self.constrain(parent);
            }
            // `GestureListener.onScroll`.
            Gesture::Scroll { distance } => {
                self.child.0 -= distance.0;
                self.child.1 -= distance.1;
                self.constrain(parent);
            }
            // `onScaleEnd` updates picture-in-picture, which is not carried.
            Gesture::ScaleEnd => {}
        }
        (self.scale, self.child) != before
    }

    /// `handleSurfaceChange`: the container took a new size. The picture
    /// keeps its zoom, and its corner's offset from the middle scales with
    /// the container; then it is constrained again. Before any gesture there
    /// is nothing to keep.
    pub fn resized(&mut self, container: Rect) {
        let parent = size(container);
        if !self.measured {
            self.parent = parent;
            return;
        }
        let (prev_parent, prev_child) = (
            self.parent,
            (self.parent.0 * self.scale, self.parent.1 * self.scale),
        );
        let child = (parent.0 * self.scale, parent.1 * self.scale);
        if prev_child.0 > 0.0 && prev_child.1 > 0.0 {
            let view_scale = (child.0 / prev_child.0, child.1 / prev_child.1);
            self.child.0 = (self.child.0 - prev_parent.0 / 2.0) * view_scale.0 + parent.0 / 2.0;
            self.child.1 = (self.child.1 - prev_parent.1 / 2.0) * view_scale.1 + parent.1 / 2.0;
        }
        self.constrain(parent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The emulator's window in landscape, and a 16:9 picture fitted in it.
    const CONTAINER: Rect = Rect {
        left: 240,
        top: 0,
        right: 2160,
        bottom: 1080,
    };

    fn scale(focus: (f32, f32), factor: f32) -> Gesture {
        Gesture::Scale { focus, factor }
    }

    fn scroll(x: f32, y: f32) -> Gesture {
        Gesture::Scroll { distance: (x, y) }
    }

    #[test]
    fn with_no_zoom_the_picture_keeps_its_place() {
        let zoom = PanZoom::default();
        assert_eq!(zoom.rect(CONTAINER), CONTAINER);
        assert_eq!(zoom.scale(), 1.0);
    }

    // `onScale`: the point under the fingers stays under them.
    #[test]
    fn a_scale_keeps_the_point_under_the_fingers() {
        let mut zoom = PanZoom::default();
        // The window's middle, the container's (960, 540).
        assert!(zoom.gesture(scale((1200.0, 540.0), 2.0), CONTAINER));
        assert_eq!(
            zoom.rect(CONTAINER),
            Rect {
                left: 240 - 960,
                top: -540,
                right: 240 - 960 + 3840,
                bottom: -540 + 2160,
            }
        );
        // Off the middle: (440, 270) in the window is (200, 270) in the
        // container, and stays where it is.
        let mut zoom = PanZoom::default();
        zoom.gesture(scale((440.0, 270.0), 2.0), CONTAINER);
        assert_eq!(
            zoom.rect(CONTAINER),
            Rect {
                left: 240 - 200,
                top: -270,
                right: 240 - 200 + 3840,
                bottom: -270 + 2160,
            }
        );
    }

    // `Math.max(1, Math.min(newScaleFactor, MAX_SCALE))`.
    #[test]
    fn the_scale_stays_from_one_to_ten_times() {
        let mut zoom = PanZoom::default();
        zoom.gesture(scale((1200.0, 540.0), 30.0), CONTAINER);
        assert_eq!(zoom.scale(), 10.0);
        zoom.gesture(scale((1200.0, 540.0), 0.01), CONTAINER);
        assert_eq!(zoom.scale(), 1.0);
        assert_eq!(zoom.rect(CONTAINER), CONTAINER);
    }

    // `onScroll` then `constrainToBounds`: a zoomed picture moves, but always
    // covers the container.
    #[test]
    fn a_drag_moves_the_zoomed_picture_within_its_place() {
        let mut zoom = PanZoom::default();
        zoom.gesture(scale((1200.0, 540.0), 2.0), CONTAINER);
        // The finger moves 100 pixels right: the picture follows.
        assert!(zoom.gesture(scroll(-100.0, 0.0), CONTAINER));
        assert_eq!(zoom.rect(CONTAINER).left, 240 - 860);
        // Far right: its left edge stops at the container's.
        zoom.gesture(scroll(-5000.0, -5000.0), CONTAINER);
        let rect = zoom.rect(CONTAINER);
        assert_eq!((rect.left, rect.top), (240, 0));
        // Far left: its right edge stops at the container's.
        zoom.gesture(scroll(9000.0, 9000.0), CONTAINER);
        let rect = zoom.rect(CONTAINER);
        assert_eq!((rect.right, rect.bottom), (2160, 1080));
    }

    // At no zoom a drag changes nothing: the picture is centred.
    #[test]
    fn with_no_zoom_a_drag_changes_nothing() {
        let mut zoom = PanZoom::default();
        assert!(!zoom.gesture(scroll(-100.0, 40.0), CONTAINER));
        assert_eq!(zoom.rect(CONTAINER), CONTAINER);
        assert!(!zoom.gesture(Gesture::ScaleEnd, CONTAINER));
    }

    // `handleSurfaceChange`: a new place keeps the zoom and the middle's
    // offset, scaled to the new size, then constrains.
    #[test]
    fn a_new_place_keeps_the_zoom() {
        let mut zoom = PanZoom::default();
        // Before any gesture there is nothing to keep.
        zoom.resized(CONTAINER);
        zoom.gesture(scale((1200.0, 540.0), 2.0), CONTAINER);
        let filled = Rect {
            left: 0,
            top: -135,
            right: 2400,
            bottom: 1215,
        };
        zoom.resized(filled);
        assert_eq!(zoom.scale(), 2.0);
        assert_eq!(
            zoom.rect(filled),
            Rect {
                left: -1200,
                top: -135 - 675,
                right: -1200 + 4800,
                bottom: -135 - 675 + 2700,
            }
        );
    }
}
