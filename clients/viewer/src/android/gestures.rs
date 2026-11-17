// SPDX-License-Identifier: Apache-2.0
//! The touch gestures of Artemis's pan and zoom (`PanZoomHandler`): its
//! `ScaleGestureDetector` (a pinch, and a double tap then a drag, "quick
//! scale") and its `GestureDetector` (a drag, "scroll"). Pure; the input
//! hands them the touch screen's events while "Pan & zoom" is on.
//!
//! Ported from Android 12's own classes (`android-12.0.0_r34`,
//! `core/java/android/view/ScaleGestureDetector.java` and
//! `GestureDetector.java`), Copyright (C) 2008 and 2010 The Android Open
//! Source Project, under the Apache License, Version 2.0. Korri's
//! adaptation, dated 2026-11-17: only what `PanZoomHandler` uses (no fling,
//! no single tap, no stylus scale, no deep press, no ambiguous-gesture
//! classification), and each `Handler` message is the moment it would
//! have run, read against the next event's time.

/// The device's `ViewConfiguration`, in pixels and milliseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureConfig {
    /// `getScaledTouchSlop`; also the double tap's touch slop, which
    /// Android sets to the same.
    pub touch_slop: i32,
    /// `getScaledDoubleTapSlop`.
    pub double_tap_slop: i32,
    /// `getScaledMinimumScalingSpan`.
    pub min_span: i32,
    /// `getLongPressTimeout`.
    pub long_press_ms: i64,
    /// `getDoubleTapTimeout`.
    pub double_tap_ms: i64,
}

/// `ViewConfiguration.DOUBLE_TAP_MIN_TIME`.
const DOUBLE_TAP_MIN_MS: i64 = 40;
/// `ScaleGestureDetector.SCALE_FACTOR`.
const QUICK_SCALE_FACTOR: f32 = 0.5;

use super::input::{MotionEvent, action};

/// What the detectors report to `PanZoomHandler`'s listeners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    /// `onScale`: scale by `factor` about `focus` (window pixels).
    Scale { focus: (f32, f32), factor: f32 },
    /// `onScaleEnd`.
    ScaleEnd,
    /// `onScroll`: the focus moved back by `distance` (window pixels).
    Scroll { distance: (f32, f32) },
}

/// What one `GestureDetector` saw in one event.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Detected {
    /// `onDoubleTap`, with the first tap's place.
    DoubleTap((f32, f32)),
    /// `onScroll`.
    Scroll((f32, f32)),
}

/// The part of `GestureDetector` that `PanZoomHandler` and quick scale
/// need: scroll, long press and double tap. Its `Handler` messages are
/// deadlines: `TAP` and `LONG_PRESS`.
#[derive(Debug, Default)]
struct Detector {
    down_focus: (f32, f32),
    last_focus: (f32, f32),
    /// `mCurrentDownEvent`'s first pointer's place.
    current_down: Option<(f32, f32)>,
    /// `mPreviousUpEvent`'s time.
    previous_up_ms: Option<i64>,
    /// `TAP`, when it is due.
    tap_at: Option<i64>,
    /// `LONG_PRESS`, when it is due.
    long_press_at: Option<i64>,
    always_in_tap_region: bool,
    always_in_bigger_tap_region: bool,
    in_long_press: bool,
    double_tapping: bool,
}

/// The focus of the pointers that stay: their middle.
fn focus(event: &MotionEvent) -> (f32, f32) {
    let skip = (event.action == action::POINTER_UP).then_some(event.action_index);
    let (mut x, mut y, mut count) = (0.0f32, 0.0f32, 0.0f32);
    for (index, pointer) in event.pointers.iter().enumerate() {
        if Some(index) == skip {
            continue;
        }
        x += pointer.x;
        y += pointer.y;
        count += 1.0;
    }
    if count == 0.0 {
        return (0.0, 0.0);
    }
    (x / count, y / count)
}

impl Detector {
    /// The messages that would have run before this event.
    fn run_due(&mut self, now_ms: i64) {
        if self.tap_at.is_some_and(|at| at <= now_ms) {
            self.tap_at = None;
        }
        if self.long_press_at.is_some_and(|at| at <= now_ms) {
            // `dispatchLongPress`.
            self.long_press_at = None;
            self.tap_at = None;
            self.in_long_press = true;
        }
    }

    fn cancel_taps(&mut self) {
        self.long_press_at = None;
        self.tap_at = None;
        self.double_tapping = false;
        self.always_in_tap_region = false;
        self.always_in_bigger_tap_region = false;
        self.in_long_press = false;
    }

    fn is_considered_double_tap(
        &self,
        config: &GestureConfig,
        second: (f32, f32),
        now_ms: i64,
    ) -> bool {
        let (Some(first), Some(up_ms)) = (self.current_down, self.previous_up_ms) else {
            return false;
        };
        if !self.always_in_bigger_tap_region {
            return false;
        }
        let delta_ms = now_ms - up_ms;
        if delta_ms > config.double_tap_ms || delta_ms < DOUBLE_TAP_MIN_MS {
            return false;
        }
        let dx = first.0 as i32 - second.0 as i32;
        let dy = first.1 as i32 - second.1 as i32;
        dx * dx + dy * dy < config.double_tap_slop * config.double_tap_slop
    }

    fn event(&mut self, event: &MotionEvent, config: &GestureConfig) -> Option<Detected> {
        let now_ms = event.event_time_ms;
        self.run_due(now_ms);
        let focus = focus(event);
        match event.action {
            action::POINTER_DOWN => {
                self.down_focus = focus;
                self.last_focus = focus;
                self.cancel_taps();
                None
            }
            action::POINTER_UP => {
                self.down_focus = focus;
                self.last_focus = focus;
                None
            }
            action::DOWN => {
                let first = event.pointers.first().map_or((0.0, 0.0), |p| (p.x, p.y));
                let had_tap = self.tap_at.take().is_some();
                let mut detected = None;
                if had_tap && self.is_considered_double_tap(config, first, now_ms) {
                    // The second tap: the double tap is the first one's.
                    self.double_tapping = true;
                    detected = self.current_down.map(Detected::DoubleTap);
                } else {
                    self.tap_at = Some(now_ms + config.double_tap_ms);
                }
                self.down_focus = focus;
                self.last_focus = focus;
                self.current_down = Some(first);
                self.always_in_tap_region = true;
                self.always_in_bigger_tap_region = true;
                self.in_long_press = false;
                self.long_press_at = Some(now_ms + config.long_press_ms);
                detected
            }
            action::MOVE => {
                if self.in_long_press || self.double_tapping {
                    return None;
                }
                let scroll = (self.last_focus.0 - focus.0, self.last_focus.1 - focus.1);
                if self.always_in_tap_region {
                    let dx = (focus.0 - self.down_focus.0) as i32;
                    let dy = (focus.1 - self.down_focus.1) as i32;
                    let distance = dx * dx + dy * dy;
                    let slop = config.touch_slop * config.touch_slop;
                    let mut detected = None;
                    if distance > slop {
                        detected = Some(Detected::Scroll(scroll));
                        self.last_focus = focus;
                        self.always_in_tap_region = false;
                        self.tap_at = None;
                        self.long_press_at = None;
                    }
                    // The double tap's touch slop is the touch slop.
                    if distance > slop {
                        self.always_in_bigger_tap_region = false;
                    }
                    detected
                } else if scroll.0.abs() >= 1.0 || scroll.1.abs() >= 1.0 {
                    self.last_focus = focus;
                    Some(Detected::Scroll(scroll))
                } else {
                    None
                }
            }
            action::UP => {
                if self.in_long_press {
                    self.tap_at = None;
                    self.in_long_press = false;
                }
                self.previous_up_ms = Some(now_ms);
                self.double_tapping = false;
                self.long_press_at = None;
                None
            }
            action::CANCEL => {
                self.cancel_taps();
                None
            }
            _ => None,
        }
    }
}

/// `ScaleGestureDetector` with its quick scale (no stylus).
#[derive(Debug, Default)]
struct Scale {
    in_progress: bool,
    initial_span: f32,
    prev_span: f32,
    curr_span: f32,
    /// The anchored scale's start: the double tap's first tap.
    anchored: Option<(f32, f32)>,
    before_or_above: bool,
    /// Its own `GestureDetector`, for the double tap.
    quick: Detector,
}

impl Scale {
    fn factor(&self, span_slop: f32) -> f32 {
        if self.anchored.is_some() {
            // Moving up from the start shrinks, moving down grows.
            let up = (self.before_or_above && self.curr_span < self.prev_span)
                || (!self.before_or_above && self.curr_span > self.prev_span);
            let diff = (1.0 - self.curr_span / self.prev_span).abs() * QUICK_SCALE_FACTOR;
            return if self.prev_span <= span_slop {
                1.0
            } else if up {
                1.0 + diff
            } else {
                1.0 - diff
            };
        }
        if self.prev_span > 0.0 {
            self.curr_span / self.prev_span
        } else {
            1.0
        }
    }

    fn event(&mut self, event: &MotionEvent, config: &GestureConfig, out: &mut Vec<Gesture>) {
        if let Some(Detected::DoubleTap(at)) = self.quick.event(event, config) {
            self.anchored = Some(at);
        }
        let span_slop = (config.touch_slop * 2) as f32;
        let complete = matches!(event.action, action::UP | action::CANCEL);
        if event.action == action::DOWN || complete {
            if self.in_progress {
                out.push(Gesture::ScaleEnd);
                self.in_progress = false;
                self.initial_span = 0.0;
                self.anchored = None;
            } else if self.anchored.is_some() && complete {
                self.initial_span = 0.0;
                self.anchored = None;
            }
            if complete {
                return;
            }
        }
        let changed = matches!(
            event.action,
            action::DOWN | action::POINTER_UP | action::POINTER_DOWN
        );
        let skip = (event.action == action::POINTER_UP).then_some(event.action_index);
        let focus = match self.anchored {
            Some(at) => {
                self.before_or_above = event.pointers.first().is_some_and(|p| p.y < at.1);
                at
            }
            None => focus(event),
        };
        let (mut dev_x, mut dev_y, mut count) = (0.0f32, 0.0f32, 0.0f32);
        for (index, pointer) in event.pointers.iter().enumerate() {
            if Some(index) == skip {
                continue;
            }
            dev_x += (pointer.x - focus.0).abs();
            dev_y += (pointer.y - focus.1).abs();
            count += 1.0;
        }
        if count == 0.0 {
            return;
        }
        let (span_x, span_y) = (dev_x / count * 2.0, dev_y / count * 2.0);
        let span = match self.anchored {
            Some(_) => span_y,
            None => (span_x as f64).hypot(span_y as f64) as f32,
        };
        let was = self.in_progress;
        if self.anchored.is_none() && self.in_progress && (span < config.min_span as f32 || changed)
        {
            out.push(Gesture::ScaleEnd);
            self.in_progress = false;
            self.initial_span = span;
        }
        if changed {
            self.initial_span = span;
            self.prev_span = span;
            self.curr_span = span;
        }
        let min_span = match self.anchored {
            Some(_) => span_slop,
            None => config.min_span as f32,
        };
        if !self.in_progress
            && span >= min_span
            && (was || (span - self.initial_span).abs() > span_slop)
        {
            self.prev_span = span;
            self.curr_span = span;
            // `onScaleBegin`: PanZoomHandler's listener takes it.
            self.in_progress = true;
        }
        if event.action == action::MOVE {
            self.curr_span = span;
            if self.in_progress {
                out.push(Gesture::Scale {
                    focus,
                    factor: self.factor(span_slop),
                });
            }
            self.prev_span = self.curr_span;
        }
    }
}

/// `PanZoomHandler`'s two detectors: the scale detector first, then the
/// gesture detector, as `handleTouchEvent` calls them.
#[derive(Debug)]
pub struct PanZoomGestures {
    config: GestureConfig,
    scale: Scale,
    scroll: Detector,
}

impl PanZoomGestures {
    pub fn new(config: GestureConfig) -> Self {
        Self {
            config,
            scale: Scale::default(),
            scroll: Detector::default(),
        }
    }

    /// One touch screen event: what the listeners heard, in their order.
    pub fn event(&mut self, event: &MotionEvent) -> Vec<Gesture> {
        let mut out = Vec::new();
        self.scale.event(event, &self.config, &mut out);
        if let Some(Detected::Scroll(distance)) = self.scroll.event(event, &self.config) {
            out.push(Gesture::Scroll { distance });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Pointer, source, tool};

    /// The emulator's Pixel 5 at 420 dpi: 8 dp, 100 dp and 27 mm.
    const CONFIG: GestureConfig = GestureConfig {
        touch_slop: 21,
        double_tap_slop: 263,
        min_span: 446,
        long_press_ms: 400,
        double_tap_ms: 300,
    };

    fn event(action: u32, index: usize, points: &[(f32, f32)], time_ms: i64) -> MotionEvent {
        MotionEvent {
            device: 3,
            source: source::TOUCHSCREEN,
            action,
            action_index: index,
            event_time_ms: time_ms,
            pointers: points
                .iter()
                .enumerate()
                .map(|(id, &(x, y))| Pointer {
                    id: id as i32,
                    tool_type: tool::FINGER,
                    x,
                    y,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    fn run(detector: &mut PanZoomGestures, events: &[MotionEvent]) -> Vec<Gesture> {
        events.iter().flat_map(|e| detector.event(e)).collect()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    // A pinch: it begins once its span is at least the minimum (27 mm) and
    // has changed by more than twice the touch slop; then each move scales by
    // its span over the last, about the fingers' middle.
    #[test]
    fn a_pinch_scales_about_the_fingers_middle() {
        let mut detector = PanZoomGestures::new(CONFIG);
        let gestures = run(
            &mut detector,
            &[
                event(action::DOWN, 0, &[(1200.0, 240.0)], 0),
                event(
                    action::POINTER_DOWN,
                    1,
                    &[(1200.0, 240.0), (1200.0, 840.0)],
                    10,
                ),
                event(action::MOVE, 0, &[(1200.0, 200.0), (1200.0, 880.0)], 20),
                event(action::MOVE, 0, &[(1200.0, 140.0), (1200.0, 940.0)], 30),
                event(action::MOVE, 0, &[(1200.0, 40.0), (1200.0, 1040.0)], 40),
                event(
                    action::POINTER_UP,
                    1,
                    &[(1200.0, 40.0), (1200.0, 1040.0)],
                    50,
                ),
                event(action::UP, 0, &[(1200.0, 40.0)], 60),
            ],
        );
        let scales: Vec<_> = gestures
            .iter()
            .filter_map(|g| match *g {
                Gesture::Scale { focus, factor } => Some((focus, factor)),
                _ => None,
            })
            .collect();
        // Spans 600 (no change yet), 680 (begins), 800, 1000.
        assert_eq!(scales.len(), 3, "{gestures:?}");
        assert!(scales.iter().all(|&(focus, _)| focus == (1200.0, 540.0)));
        assert!(close(scales[0].1, 1.0) && close(scales[1].1, 800.0 / 680.0));
        assert!(close(scales[2].1, 1000.0 / 800.0));
        assert!(gestures.contains(&Gesture::ScaleEnd));
        assert!(!gestures.iter().any(|g| matches!(g, Gesture::Scroll { .. })));
    }

    // Fingers closer than the minimum span do not scale.
    #[test]
    fn fingers_closer_than_the_minimum_span_do_not_scale() {
        let mut detector = PanZoomGestures::new(CONFIG);
        let gestures = run(
            &mut detector,
            &[
                event(action::DOWN, 0, &[(1200.0, 440.0)], 0),
                event(
                    action::POINTER_DOWN,
                    1,
                    &[(1200.0, 440.0), (1200.0, 640.0)],
                    10,
                ),
                event(action::MOVE, 0, &[(1200.0, 400.0), (1200.0, 680.0)], 20),
            ],
        );
        assert!(gestures.is_empty(), "{gestures:?}");
    }

    // A drag scrolls once it leaves the touch slop, by the focus's way back;
    // after that by each move of a pixel or more.
    #[test]
    fn a_drag_scrolls_past_the_touch_slop() {
        let mut detector = PanZoomGestures::new(CONFIG);
        let gestures = run(
            &mut detector,
            &[
                event(action::DOWN, 0, &[(1000.0, 500.0)], 0),
                event(action::MOVE, 0, &[(1010.0, 500.0)], 10),
                event(action::MOVE, 0, &[(1030.0, 500.0)], 20),
                event(action::MOVE, 0, &[(1031.5, 500.0)], 30),
                event(action::MOVE, 0, &[(1032.0, 500.0)], 40),
                event(action::UP, 0, &[(1032.0, 500.0)], 50),
            ],
        );
        assert_eq!(
            gestures,
            [
                Gesture::Scroll {
                    distance: (-30.0, 0.0)
                },
                Gesture::Scroll {
                    distance: (-1.5, 0.0)
                },
            ]
        );
    }

    // A finger held still for the long press time scrolls no more.
    #[test]
    fn a_long_press_does_not_scroll() {
        let mut detector = PanZoomGestures::new(CONFIG);
        let held = run(
            &mut detector,
            &[
                event(action::DOWN, 0, &[(1000.0, 500.0)], 0),
                event(action::MOVE, 0, &[(1100.0, 500.0)], 450),
                event(action::UP, 0, &[(1100.0, 500.0)], 460),
            ],
        );
        assert!(held.is_empty(), "{held:?}");
        let next = run(
            &mut detector,
            &[
                event(action::DOWN, 0, &[(1000.0, 500.0)], 2000),
                event(action::MOVE, 0, &[(1100.0, 500.0)], 2050),
            ],
        );
        assert_eq!(
            next,
            [Gesture::Scroll {
                distance: (-100.0, 0.0)
            }]
        );
    }

    // A double tap then a drag is a quick scale, anchored at the first tap:
    // down scales up, and it does not scroll.
    #[test]
    fn a_double_tap_and_drag_scales_about_the_first_tap() {
        let mut detector = PanZoomGestures::new(CONFIG);
        let gestures = run(
            &mut detector,
            &[
                event(action::DOWN, 0, &[(1000.0, 500.0)], 0),
                event(action::UP, 0, &[(1000.0, 500.0)], 50),
                event(action::DOWN, 0, &[(1005.0, 505.0)], 150),
                event(action::MOVE, 0, &[(1005.0, 600.0)], 200),
                event(action::MOVE, 0, &[(1005.0, 700.0)], 220),
                event(action::UP, 0, &[(1005.0, 700.0)], 300),
            ],
        );
        assert_eq!(
            gestures,
            [
                Gesture::Scale {
                    focus: (1000.0, 500.0),
                    factor: 1.0
                },
                Gesture::Scale {
                    focus: (1000.0, 500.0),
                    factor: 1.5
                },
                Gesture::ScaleEnd,
            ]
        );
    }

    // Two taps too far apart in time are no double tap: the second drag
    // scrolls.
    #[test]
    fn a_late_second_tap_is_a_new_drag() {
        let mut detector = PanZoomGestures::new(CONFIG);
        let gestures = run(
            &mut detector,
            &[
                event(action::DOWN, 0, &[(1000.0, 500.0)], 0),
                event(action::UP, 0, &[(1000.0, 500.0)], 50),
                event(action::DOWN, 0, &[(1000.0, 500.0)], 500),
                event(action::MOVE, 0, &[(1000.0, 600.0)], 550),
            ],
        );
        assert_eq!(
            gestures,
            [Gesture::Scroll {
                distance: (0.0, -100.0)
            }]
        );
    }
}
