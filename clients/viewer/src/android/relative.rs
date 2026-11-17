// SPDX-License-Identifier: GPL-3.0-only
//! One finger of the touch screen as a relative mouse: Artemis's mouse mode
//! "Track pad(Gaming/Long press to drag)" (mode 3), its `RelativeTouchContext`.
//! A move moves the host's pointer by the finger's way, scaled to a
//! 1280x720 picture; a tap clicks; a press held for 650 ms drags; two
//! fingers scroll. Pure; `touch.rs` routes the events.
//!
//! Ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`): `RelativeTouchContext.java`, with
//! Artemis's defaults for its settings (touch pad sensitivity 100 % on each
//! axis, no absolute mouse mode). Artemis ships under the GNU GPL version 3
//! with no "or later" grant, so this file does too. The Rust adaptation is
//! Korri work dated 2026-11-17.
use super::{
    gamepad::{MOUSE_LEFT, MOUSE_RIGHT},
    input::{Packet, Sink},
    touch::Context,
};
use crate::placement::Rect;

const TAP_MOVEMENT_THRESHOLD: i32 = 20;
const TAP_DISTANCE_THRESHOLD: f64 = 25.0;
const TAP_TIME_THRESHOLD: i64 = 250;
const DRAG_TIME_THRESHOLD: i64 = 650;
const SCROLL_SPEED_FACTOR: i32 = 5;
/// `Game.REFERENCE_HORIZ_RES` and `REFERENCE_VERT_RES`.
const REFERENCE: (f64, f64) = (1280.0, 720.0);
/// A tap's button goes up 100 ms after it went down, for hosts that poll.
const BUTTON_UP_MS: i64 = 100;
/// `MouseButtonPacket.BUTTON_X2`: the buttons are 1 to 5.
const BUTTONS: usize = 5;

/// `RelativeTouchContext` for the finger at `index`.
#[derive(Debug, Default)]
pub(crate) struct Relative {
    index: usize,
    last: (i32, i32),
    original: (i32, i32),
    original_ms: i64,
    cancelled: bool,
    confirmed_move: bool,
    confirmed_drag: bool,
    confirmed_scroll: bool,
    distance_moved: f64,
    factor: (f64, f64),
    pointer_count: usize,
    max_pointers: usize,
    /// `dragTimerRunnable`, when it is due.
    drag_at: Option<i64>,
    /// `buttonUpRunnables`, by button (1 to 5), when each is due.
    button_up_at: [Option<i64>; BUTTONS],
}

impl Relative {
    pub(crate) fn new(index: usize) -> Self {
        Self {
            index,
            ..Self::default()
        }
    }

    fn within_tap_bounds(&self, (x, y): (i32, i32)) -> bool {
        (x - self.original.0).abs() <= TAP_MOVEMENT_THRESHOLD
            && (y - self.original.1).abs() <= TAP_MOVEMENT_THRESHOLD
    }

    fn is_tap(&self, time_ms: i64) -> bool {
        if self.confirmed_drag || self.confirmed_move || self.confirmed_scroll {
            return false;
        }
        // Only the last finger down of a multi-finger tap reports it.
        if self.index + 1 != self.max_pointers {
            return false;
        }
        self.within_tap_bounds(self.last) && time_ms - self.original_ms <= TAP_TIME_THRESHOLD
    }

    /// `getMouseButtonIndex`.
    fn button(&self) -> u8 {
        if self.index == 1 {
            MOUSE_RIGHT
        } else {
            MOUSE_LEFT
        }
    }

    fn check_for_confirmed_move(&mut self, (x, y): (i32, i32)) {
        if self.confirmed_move || self.confirmed_drag {
            return;
        }
        // Out of the tap's bounds before the drag time: a move.
        if !self.within_tap_bounds((x, y)) {
            self.confirmed_move = true;
            self.drag_at = None;
            return;
        }
        self.distance_moved +=
            (f64::from(x - self.last.0).powi(2) + f64::from(y - self.last.1).powi(2)).sqrt();
        if self.distance_moved >= TAP_DISTANCE_THRESHOLD {
            self.confirmed_move = true;
            self.drag_at = None;
        }
    }

    fn timers(&self) -> impl Iterator<Item = i64> + '_ {
        self.drag_at
            .into_iter()
            .chain(self.button_up_at.iter().flatten().copied())
    }
}

fn button(sink: &mut dyn Sink, pressed: bool, button: u8) {
    sink.send(Packet::MouseButton { pressed, button });
}

/// Java's `Math.round` of a non-negative value, for the scaled deltas.
fn round(value: f64) -> i32 {
    (value + 0.5).floor() as i32
}

impl Context for Relative {
    fn down(
        &mut self,
        (x, y): (i32, i32),
        time_ms: i64,
        now_ms: i64,
        new: bool,
        video: Rect,
        _sink: &mut dyn Sink,
    ) {
        // The view's size scales the inputs of this touch.
        let (width, height) = (
            f64::from((video.right - video.left).max(1)),
            f64::from((video.bottom - video.top).max(1)),
        );
        self.factor = (REFERENCE.0 / width, REFERENCE.1 / height);
        self.original = (x, y);
        self.last = (x, y);
        if new {
            self.max_pointers = self.pointer_count;
            self.original_ms = time_ms;
            self.cancelled = false;
            self.confirmed_drag = false;
            self.confirmed_move = false;
            self.confirmed_scroll = false;
            self.distance_moved = 0.0;
            if self.index == 0 {
                self.drag_at = Some(now_ms + DRAG_TIME_THRESHOLD);
            }
        }
    }

    fn moved(
        &mut self,
        (x, y): (i32, i32),
        _time_ms: i64,
        _now_ms: i64,
        _video: Rect,
        sink: &mut dyn Sink,
    ) {
        if self.cancelled || (x, y) == self.last {
            return;
        }
        self.check_for_confirmed_move((x, y));
        // `checkForConfirmedScroll`: two fingers that left the tap's zone.
        self.confirmed_scroll = self.index == 0 && self.pointer_count == 2 && self.confirmed_move;
        if self.index != 0 {
            self.last = (x, y);
            return;
        }
        // Only the first finger moves and drags, scaled to the reference.
        let mut dx = round(f64::from((x - self.last.0).abs()) * self.factor.0);
        let mut dy = round(f64::from((y - self.last.1).abs()) * self.factor.1);
        if x < self.last.0 {
            dx = -dx;
        }
        if y < self.last.1 {
            dy = -dy;
        }
        if self.pointer_count == 2 {
            if self.confirmed_scroll {
                sink.send(Packet::Scroll {
                    amount: (dy * SCROLL_SPEED_FACTOR) as i16,
                });
            }
        } else {
            sink.send(Packet::MouseMove {
                dx: dx as i16,
                dy: dy as i16,
            });
        }
        // A delta that scaled to 0 keeps its place until it is not 0, for
        // devices that report small moves often.
        if dx != 0 {
            self.last.0 = x;
        }
        if dy != 0 {
            self.last.1 = y;
        }
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
        self.drag_at = None;
        let pressed = self.button();
        if self.confirmed_drag {
            button(sink, false, pressed);
        } else if self.is_tap(time_ms) {
            button(sink, true, pressed);
            self.button_up_at[usize::from(pressed) - 1] = Some(now_ms + BUTTON_UP_MS);
        }
    }

    fn cancel(&mut self, sink: &mut dyn Sink) {
        self.cancelled = true;
        self.drag_at = None;
        if self.confirmed_drag {
            button(sink, false, self.button());
        }
        // A tap's button that waits for its release lets go now: a new mouse
        // mode drops this context and its timers (review P2).
        for (slot, due) in self.button_up_at.iter_mut().enumerate() {
            if due.take().is_some() {
                button(sink, false, slot as u8 + 1);
            }
        }
    }

    fn cancelled(&self) -> bool {
        self.cancelled
    }

    fn pointers(&mut self, count: usize, _now_ms: i64, _sink: &mut dyn Sink) {
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
        if self.drag_at == Some(at) {
            self.drag_at = None;
            // A move came first, or this is not the gesture's first finger.
            if self.confirmed_move || self.index + 1 != self.max_pointers {
                return true;
            }
            // Not cancelled before the time: the drag begins.
            self.confirmed_drag = true;
            button(sink, true, self.button());
            return true;
        }
        if let Some(slot) = self.button_up_at.iter().position(|&due| due == Some(at)) {
            self.button_up_at[slot] = None;
            button(sink, false, slot as u8 + 1);
        }
        true
    }
}
