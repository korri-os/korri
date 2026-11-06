//! Minimal touch browsing: a drag moves the surface's own cursor. The
//! surface still chooses the focus target and brings it into view.
use super::Key;

#[derive(Default)]
pub struct Drag {
    origin: Option<(f32, f32)>,
    horizontal: Option<bool>,
}

impl Drag {
    pub fn begin(&mut self, x: f32, y: f32) {
        self.origin = Some((x, y));
        self.horizontal = None;
    }

    pub fn cancel(&mut self) {
        self.origin = None;
        self.horizontal = None;
    }

    /// Eight cursor steps across an axis, independent of pixel density.
    /// Lock to the first dragged axis until release. Taps do nothing in M1.
    pub fn moved(&mut self, x: f32, y: f32, width: f32, height: f32) -> Vec<Key> {
        let Some((ox, oy)) = self.origin else {
            return Vec::new();
        };
        if width <= 0.0 || height <= 0.0 {
            return Vec::new();
        }
        let dx = (x - ox) / width;
        let dy = (y - oy) / height;
        let horizontal = *self.horizontal.get_or_insert_with(|| dx.abs() > dy.abs());
        let delta = if horizontal { dx } else { dy };
        let count = (delta.abs() * 8.0).floor().min(8.0) as usize;
        if count == 0 {
            // Do not lock an axis before the gesture crosses the threshold.
            if dx.abs().max(dy.abs()) < 0.125 {
                self.horizontal = None;
            }
            return Vec::new();
        }
        let step = delta.signum() * count as f32 / 8.0;
        self.origin = Some(if horizontal {
            (ox + step * width, oy)
        } else {
            (ox, oy + step * height)
        });
        let key = match (horizontal, delta > 0.0) {
            (true, true) => Key::Left,
            (true, false) => Key::Right,
            (false, true) => Key::Up,
            (false, false) => Key::Down,
        };
        vec![key; count]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drag_is_proportional_and_follows_the_finger() {
        for (w, h) in [(320.0, 800.0), (1600.0, 640.0)] {
            let mut drag = Drag::default();
            drag.begin(w / 2.0, h / 2.0);
            assert!(drag.moved(w / 2.0, h * 0.49, w, h).is_empty());
            assert_eq!(drag.moved(w / 2.0, h / 4.0, w, h), vec![Key::Down; 2]);
            assert_eq!(drag.moved(w / 2.0, h / 2.0, w, h), vec![Key::Up; 2]);
            drag.cancel();
            assert!(drag.moved(0.0, 0.0, w, h).is_empty());
        }
    }
    #[test]
    fn horizontal_drag_locks_axis_and_new_gesture_resets_it() {
        let mut drag = Drag::default();
        drag.begin(400.0, 400.0);
        assert_eq!(drag.moved(300.0, 400.0, 800.0, 800.0), vec![Key::Right]);
        assert!(drag.moved(300.0, 100.0, 800.0, 800.0).is_empty());
        drag.begin(400.0, 400.0);
        assert_eq!(drag.moved(400.0, 500.0, 800.0, 800.0), vec![Key::Up]);
    }
}
