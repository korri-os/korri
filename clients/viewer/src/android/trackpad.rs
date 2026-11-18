// SPDX-License-Identifier: GPL-3.0-only
//! One finger of the touch screen as a laptop's touch pad: Artemis's mouse
//! mode "Track pad(Natural/Double tap to drag)" (mode 2), its
//! `TrackpadContext`. A move moves the host's pointer, faster for faster
//! moves; a tap clicks; a tap and a touch drag; two fingers scroll, and a
//! tap of two fingers right-clicks; a fast move goes on after the finger
//! lifts (a flick). Pure; `touch.rs` routes the events.
//!
//! Ported from Korri's copy of Artemis (`clients/android` at the tag `artemis-final`, from
//! `simonwjackson/artemis` at `cf12c432`): `TrackpadContext.java`, as
//! `Game.applyMouseMode` makes it for the touch screen (no swapped axes,
//! sensitivity 1 on each). Its `Handler` callbacks are deadlines here, run by
//! the input's tick: a callback posted with no delay is due at once.
//! Artemis ships under the GNU GPL version 3 with no "or later" grant, so
//! this file does too. The Rust adaptation is Korri work dated 2026-11-17.
use super::{
    gamepad::{MOUSE_LEFT, MOUSE_RIGHT},
    input::{Packet, Sink},
    touch::{Context, MOUSE_MIDDLE},
};
use crate::placement::Rect;

const TAP_MOVEMENT_THRESHOLD: i32 = 30;
const TAP_TIME_THRESHOLD: i64 = 230;
const CLICK_RELEASE_DELAY: i64 = TAP_TIME_THRESHOLD;
const SCROLL_SPEED_FACTOR_X: f64 = 2.0;
const SCROLL_SPEED_FACTOR_Y: f64 = 3.0;
const ACCELERATION_THRESHOLD: f64 = 8.0;
const FLICK_FRICTION: f64 = 0.93;
/// Pixels per millisecond.
const FLICK_THRESHOLD: f64 = 0.8;
const MOMENTUM_FRAME_INTERVAL_MS: i64 = 10;
const FLICK_VELOCITY_DECAY_TIMEOUT_MS: f64 = 50.0;
const SCROLL_TRANSITION_TIMEOUT_MS: i64 = 200;

/// `TrackpadContext` for the finger at `index`.
#[derive(Debug, Default)]
pub(crate) struct Trackpad {
    index: usize,
    pending: (f64, f64),
    last: (i32, i32),
    original: (i32, i32),
    original_ms: i64,
    cancelled: bool,
    confirmed_move: bool,
    confirmed_drag: bool,
    confirmed_scroll: bool,
    distance_moved: f64,
    pointer_count: usize,
    clicked_middle: bool,
    max_pointers: usize,
    click_pending: bool,
    double_click_pending: bool,
    flicking: bool,
    velocity: (f64, f64),
    last_move_ms: i64,
    scroll_transitioning: bool,
    /// `scrollTransitionRunnable`.
    scroll_transition_at: Option<i64>,
    /// `momentumRunnable`.
    momentum_at: Option<i64>,
    /// `scrollMomentumRunnable`.
    scroll_momentum_at: Option<i64>,
    /// The tap's release, with the button that it lets go.
    click_release: Option<(i64, u8)>,
}

fn button(sink: &mut dyn Sink, pressed: bool, button: u8) {
    sink.send(Packet::MouseButton { pressed, button });
}

/// Java's `(short)` of a double: towards zero, through an int.
fn short(value: f64) -> i16 {
    value as i32 as i16
}

impl Trackpad {
    pub(crate) fn new(index: usize) -> Self {
        Self {
            index,
            ..Self::default()
        }
    }

    /// `handler.removeCallbacksAndMessages(null)`: every callback goes.
    fn clear_timers(&mut self) {
        self.scroll_transition_at = None;
        self.momentum_at = None;
        self.scroll_momentum_at = None;
        self.click_release = None;
    }

    fn within_tap_bounds(&self, (x, y): (i32, i32)) -> bool {
        (x - self.original.0).abs() <= TAP_MOVEMENT_THRESHOLD
            && (y - self.original.1).abs() <= TAP_MOVEMENT_THRESHOLD
    }

    fn is_tap(&self, time_ms: i64) -> bool {
        if self.confirmed_drag || self.confirmed_move || self.confirmed_scroll {
            return false;
        }
        if self.index + 1 != self.max_pointers {
            return false;
        }
        self.within_tap_bounds(self.last) && time_ms - self.original_ms <= TAP_TIME_THRESHOLD
    }

    /// `getMouseButtonIndex`: right for two fingers, middle for three.
    fn button(&mut self) -> u8 {
        match self.pointer_count {
            2 => MOUSE_RIGHT,
            3 => {
                self.clicked_middle = true;
                MOUSE_MIDDLE
            }
            _ => MOUSE_LEFT,
        }
    }

    fn speed(&self) -> f64 {
        (self.velocity.0.powi(2) + self.velocity.1.powi(2)).sqrt()
    }

    fn check_for_confirmed_move(&mut self, (x, y): (i32, i32)) {
        if self.confirmed_move || self.confirmed_drag {
            return;
        }
        if !self.within_tap_bounds((x, y)) {
            self.confirmed_move = true;
            return;
        }
        self.distance_moved +=
            (f64::from(x - self.last.0).powi(2) + f64::from(y - self.last.1).powi(2)).sqrt();
        if self.distance_moved >= f64::from(TAP_MOVEMENT_THRESHOLD) {
            self.confirmed_move = true;
        }
    }

    /// One scroll of `x` and `y`: the larger axis first, the other only when
    /// it is nearly as large.
    fn scroll(sink: &mut dyn Sink, (x, y): (f64, f64), (ax, ay): (f64, f64)) {
        let horizontal = |sink: &mut dyn Sink| {
            sink.send(Packet::HScroll {
                amount: short(-x * SCROLL_SPEED_FACTOR_X),
            });
        };
        let vertical = |sink: &mut dyn Sink| {
            sink.send(Packet::Scroll {
                amount: short(y * SCROLL_SPEED_FACTOR_Y),
            });
        };
        if ax > ay {
            horizontal(sink);
            if ay * 1.05 > ax {
                vertical(sink);
            }
        } else {
            vertical(sink);
            if ax * 1.05 >= ay {
                horizontal(sink);
            }
        }
    }

    /// `momentumRunnable`: the pointer goes on and slows.
    fn momentum(&mut self, now_ms: i64, sink: &mut dyn Sink) {
        if !self.flicking {
            return;
        }
        let frame = MOMENTUM_FRAME_INTERVAL_MS as f64;
        self.pending.0 += self.velocity.0 * frame;
        self.pending.1 += self.velocity.1 * frame;
        let (dx, dy) = (short(self.pending.0), short(self.pending.1));
        if dx != 0 || dy != 0 {
            sink.send(Packet::MouseMove { dx, dy });
            self.pending.0 -= f64::from(dx);
            self.pending.1 -= f64::from(dy);
        }
        self.velocity.0 *= FLICK_FRICTION;
        self.velocity.1 *= FLICK_FRICTION;
        if self.speed() * frame < 0.5 {
            self.flicking = false;
            if self.confirmed_drag {
                let pressed = self.button();
                button(sink, false, pressed);
                self.confirmed_drag = false;
            }
        }
        if self.flicking {
            self.momentum_at = Some(now_ms + MOMENTUM_FRAME_INTERVAL_MS);
        }
    }

    /// `scrollMomentumRunnable`: the scroll goes on and slows.
    fn scroll_momentum(&mut self, now_ms: i64, sink: &mut dyn Sink) {
        if !self.flicking {
            return;
        }
        let frame = MOMENTUM_FRAME_INTERVAL_MS as f64;
        let (x, y) = (self.velocity.0 * frame, self.velocity.1 * frame);
        Self::scroll(sink, (x, y), (x.abs(), y.abs()));
        self.velocity.0 *= FLICK_FRICTION;
        self.velocity.1 *= FLICK_FRICTION;
        if self.speed() * frame < 0.5 {
            self.flicking = false;
        }
        if self.flicking {
            self.scroll_momentum_at = Some(now_ms + MOMENTUM_FRAME_INTERVAL_MS);
        }
    }

    fn timers(&self) -> impl Iterator<Item = i64> + '_ {
        [
            self.scroll_transition_at,
            self.momentum_at,
            self.scroll_momentum_at,
            self.click_release.map(|(at, _)| at),
        ]
        .into_iter()
        .flatten()
    }
}

impl Context for Trackpad {
    fn down(
        &mut self,
        at: (i32, i32),
        time_ms: i64,
        _now_ms: i64,
        new: bool,
        _video: Rect,
        sink: &mut dyn Sink,
    ) {
        if self.flicking {
            self.flicking = false;
            self.clear_timers();
        }
        if new {
            // A new gesture: a pending move after a scroll goes.
            self.clicked_middle = false;
            self.scroll_transition_at = None;
            self.scroll_transitioning = false;
            self.max_pointers = self.pointer_count;
            self.original_ms = time_ms;
            self.cancelled = false;
            self.confirmed_move = false;
            self.confirmed_scroll = false;
            self.distance_moved = 0.0;
            self.velocity = (0.0, 0.0);
            self.last_move_ms = time_ms;
            if self.click_pending {
                // A touch after a tap: a double tap, which drags.
                self.click_pending = false;
                self.double_click_pending = true;
                self.confirmed_drag = true;
            }
        } else if self.pointer_count == 2 && !self.confirmed_move {
            // A third finger lifted: a middle click.
            button(sink, true, MOUSE_MIDDLE);
            button(sink, false, MOUSE_MIDDLE);
            self.click_pending = false;
            self.double_click_pending = false;
            self.confirmed_drag = false;
            self.clicked_middle = true;
        } else if self.pointer_count == 1 && !self.confirmed_move && !self.clicked_middle {
            // The second finger lifted: a right click at once.
            button(sink, true, MOUSE_RIGHT);
            button(sink, false, MOUSE_RIGHT);
            self.click_pending = false;
            self.double_click_pending = false;
            self.confirmed_drag = false;
        }
        self.original = at;
        self.last = at;
        self.pending = (0.0, 0.0);
    }

    fn moved(
        &mut self,
        (x, y): (i32, i32),
        time_ms: i64,
        _now_ms: i64,
        _video: Rect,
        sink: &mut dyn Sink,
    ) {
        if self.cancelled || (x, y) == self.last {
            return;
        }
        let delta_ms = time_ms - self.last_move_ms;
        self.check_for_confirmed_move((x, y));
        if self.double_click_pending {
            self.double_click_pending = false;
            self.confirmed_drag = true;
        }
        let raw = (x - self.last.0, y - self.last.1);
        let magnitude = f64::from(raw.0 * raw.0 + raw.1 * raw.1).sqrt();
        let precision = (magnitude / ACCELERATION_THRESHOLD).cbrt();
        let absolute = (f64::from(raw.0.abs()), f64::from(raw.1.abs()));
        // Java's floats, scaled by the precision (sensitivity 1).
        let delta = (
            (f64::from(raw.0) * precision) as f32,
            (f64::from(raw.1) * precision) as f32,
        );
        if delta_ms > 0 && (self.confirmed_move || self.confirmed_drag) {
            let now = (
                f64::from(delta.0 / delta_ms as f32),
                f64::from(delta.1 / delta_ms as f32),
            );
            if self.velocity == (0.0, 0.0) {
                self.velocity = now;
            } else {
                // A moving average smooths it.
                self.velocity.0 = self.velocity.0 * 0.8 + now.0 * 0.2;
                self.velocity.1 = self.velocity.1 * 0.8 + now.1 * 0.2;
            }
        }
        self.last_move_ms = time_ms;
        self.pending.0 += f64::from(delta.0);
        self.pending.1 += f64::from(delta.1);
        self.last = (x, y);
        let send = (short(self.pending.0), short(self.pending.1));
        if self.pointer_count == 1 {
            // No move just after a scroll.
            if !self.scroll_transitioning && send != (0, 0) {
                sink.send(Packet::MouseMove {
                    dx: send.0,
                    dy: send.1,
                });
            }
        } else if self.index == 1 {
            if self.confirmed_drag {
                if send != (0, 0) {
                    sink.send(Packet::MouseMove {
                        dx: send.0,
                        dy: send.1,
                    });
                }
            } else if self.pointer_count == 2 {
                // `checkForConfirmedScroll`.
                self.confirmed_scroll = self.confirmed_move;
                if self.confirmed_scroll {
                    Self::scroll(sink, (f64::from(send.0), f64::from(send.1)), absolute);
                }
            }
        }
        self.pending.0 -= f64::from(send.0);
        self.pending.1 -= f64::from(send.1);
    }

    fn up(
        &mut self,
        _at: (i32, i32),
        time_ms: i64,
        now_ms: i64,
        _video: Rect,
        sink: &mut dyn Sink,
    ) {
        if self.cancelled {
            return;
        }
        // A pause before the lift slows the flick.
        let since = time_ms - self.last_move_ms;
        if since > 0 {
            let decay = (1.0 - since as f64 / FLICK_VELOCITY_DECAY_TIMEOUT_MS).max(0.0);
            self.velocity.0 *= decay;
            self.velocity.1 *= decay;
        }
        let pressed = self.button();
        if self.double_click_pending {
            self.clear_timers();
            button(sink, false, pressed);
            button(sink, true, pressed);
            button(sink, false, pressed);
            self.click_pending = false;
            self.confirmed_drag = false;
        } else if self.confirmed_drag {
            self.clear_timers();
            if self.speed() > FLICK_THRESHOLD {
                self.flicking = true;
                self.momentum_at = Some(now_ms);
            } else {
                button(sink, false, pressed);
                self.confirmed_drag = false;
            }
        } else if self.is_tap(time_ms) {
            button(sink, true, pressed);
            self.click_pending = true;
            self.clear_timers();
            self.click_release = Some((now_ms + CLICK_RELEASE_DELAY, pressed));
        } else if self.confirmed_move && self.speed() > FLICK_THRESHOLD {
            self.flicking = true;
            if self.confirmed_scroll {
                self.scroll_momentum_at = Some(now_ms);
            } else if self.max_pointers == 1 {
                // A move of more fingers that was no scroll does not flick.
                self.momentum_at = Some(now_ms);
            }
        }
    }

    fn cancel(&mut self, sink: &mut dyn Sink) {
        self.cancelled = true;
        // A tap's button that waits for its release lets go now, as the
        // release would: a new mouse mode drops this context and its timers
        // (review P2).
        if let Some((_, pressed)) = self.click_release.take() {
            if self.click_pending {
                button(sink, false, pressed);
                self.click_pending = false;
            }
            self.double_click_pending = false;
        }
        if self.flicking {
            self.flicking = false;
            self.clear_timers();
        }
        if self.confirmed_drag {
            let pressed = self.button();
            button(sink, false, pressed);
        }
    }

    fn cancelled(&self) -> bool {
        self.cancelled
    }

    fn pointers(&mut self, count: usize, now_ms: i64, sink: &mut dyn Sink) {
        if self.pointer_count == 2 && count == 1 {
            // The end of a scroll: the finger that stays moves nothing for a
            // moment.
            self.scroll_transitioning = true;
            self.scroll_transition_at = Some(now_ms + SCROLL_TRANSITION_TIMEOUT_MS);
        } else if self.pointer_count == 1 && count == 2 {
            self.scroll_transition_at = None;
            self.scroll_transitioning = false;
        }
        if count < self.pointer_count && self.confirmed_drag && !self.flicking {
            let pressed = self.button();
            button(sink, false, pressed);
            self.confirmed_drag = false;
            self.confirmed_move = false;
            self.confirmed_scroll = false;
            self.click_pending = false;
            self.double_click_pending = false;
        }
        self.pointer_count = count;
        if count > self.max_pointers {
            self.max_pointers = count;
        }
    }

    fn deadline(&self) -> Option<i64> {
        self.timers().min()
    }

    fn fire(&mut self, now_ms: i64, _video: Rect, sink: &mut dyn Sink) -> bool {
        let Some(at) = self.timers().filter(|&at| at <= now_ms).min() else {
            return false;
        };
        if self.scroll_transition_at == Some(at) {
            self.scroll_transition_at = None;
            self.scroll_transitioning = false;
        } else if self.momentum_at == Some(at) {
            self.momentum_at = None;
            self.momentum(now_ms, sink);
        } else if self.scroll_momentum_at == Some(at) {
            self.scroll_momentum_at = None;
            self.scroll_momentum(now_ms, sink);
        } else if let Some((_, pressed)) = self.click_release.take() {
            if self.click_pending {
                button(sink, false, pressed);
                self.click_pending = false;
            }
            self.double_click_pending = false;
        }
        true
    }
}
