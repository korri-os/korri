// SPDX-License-Identifier: GPL-3.0-only
//! The touch screen and a physical mouse. Pure; `input.rs` routes the
//! events and sends the packets.
//!
//! The touch screen follows Artemis's default mouse mode, "multi-touch"
//! (mode 0): each finger goes to the host as a touch, and a host that takes
//! no touch gets an absolute mouse instead (`AbsoluteTouchContext`): a tap
//! clicks, a long press right-clicks, a second finger scrolls. A mouse
//! moves the host's pointer to the same place in the picture.
//!
//! Ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`, a fork of
//! `ClassicOldSong/moonlight-android`, itself a fork of
//! `moonlight-stream/moonlight-android`): `Game.java` (`handleMotionEvent`,
//! `trySendTouchEvent`, `handleTouchInput`, `updateMousePosition`) and
//! `AbsoluteTouchContext.java`. Artemis ships under the GNU GPL version 3
//! with no "or later" grant, so this file does too. The Rust adaptation is
//! Korri work dated 2026-11-14.
use super::{
    gamepad::{MOUSE_LEFT, MOUSE_RIGHT},
    input::{Device, MotionEvent, Packet, Sink, action, axis, tool},
};
use crate::placement::Rect;
use std::f32::consts::FRAC_PI_4;

/// `LiSendTouchEvent`'s event types (`Limelight.h`).
pub(crate) mod kind {
    pub const HOVER: u8 = 0x00;
    pub const DOWN: u8 = 0x01;
    pub const UP: u8 = 0x02;
    pub const MOVE: u8 = 0x03;
    pub const CANCEL: u8 = 0x04;
    pub const BUTTON_ONLY: u8 = 0x05;
    pub const HOVER_LEAVE: u8 = 0x06;
    pub const CANCEL_ALL: u8 = 0x07;
}
/// `LI_ROT_UNKNOWN`.
pub(crate) const ROTATION_UNKNOWN: u16 = 0xffff;
/// `LI_ERR_UNSUPPORTED`: the host takes no touch.
pub(crate) const UNSUPPORTED: i32 = -5501;

const MOUSE_MIDDLE: u8 = 0x02;

/// `MotionEvent.FLAG_CANCELED` (API 33; never set before).
const FLAG_CANCELED: u32 = 0x20;

/// The picture's size and origin in the window, for the mouse's and the
/// touch's coordinates.
fn size(video: Rect) -> (f32, f32) {
    (
        (video.right - video.left).max(1) as f32,
        (video.bottom - video.top).max(1) as f32,
    )
}

/// A point in the picture, clamped to it, in window pixels from its corner.
fn in_picture(video: Rect, x: f32, y: f32) -> (f32, f32) {
    let (width, height) = size(video);
    (
        (x - video.left as f32).clamp(0.0, width),
        (y - video.top as f32).clamp(0.0, height),
    )
}

/// `getLiTouchTypeFromEvent`.
fn touch_type(event: &MotionEvent) -> Option<u8> {
    Some(match event.action {
        action::DOWN | action::POINTER_DOWN => kind::DOWN,
        action::UP | action::POINTER_UP if event.flags & FLAG_CANCELED != 0 => kind::CANCEL,
        action::UP | action::POINTER_UP => kind::UP,
        action::MOVE => kind::MOVE,
        // A cancel ends every pointer of the gesture.
        action::CANCEL => kind::CANCEL_ALL,
        action::HOVER_ENTER | action::HOVER_MOVE => kind::HOVER,
        action::HOVER_EXIT => kind::HOVER_LEAVE,
        action::BUTTON_PRESS | action::BUTTON_RELEASE => kind::BUTTON_ONLY,
        _ => return None,
    })
}

/// The touch screen in Artemis's multi-touch mode.
#[derive(Debug, Default)]
pub(crate) struct Touch {
    /// The absolute mouse, one context for each of the first two fingers.
    fingers: [Finger; 2],
}

impl Touch {
    /// A touch screen's event. False: not one this mode takes.
    pub(crate) fn event(
        &mut self,
        event: &MotionEvent,
        device: Option<&Device>,
        video: Rect,
        now_ms: i64,
        sink: &mut dyn Sink,
    ) -> bool {
        if send_touch(event, device, video, sink) {
            return true;
        }
        self.absolute(event, video, now_ms, sink)
    }

    /// `handleTouchInput` with the absolute contexts, for a host that takes
    /// no touch.
    fn absolute(
        &mut self,
        event: &MotionEvent,
        video: Rect,
        now_ms: i64,
        sink: &mut dyn Sink,
    ) -> bool {
        let index = event.action_index;
        let count = event.pointers.len();
        if index >= self.fingers.len() {
            return false;
        }
        let at = |pointer: usize| -> (i32, i32) {
            event.pointers.get(pointer).map_or((0, 0), |p| {
                (
                    (p.x - video.left as f32) as i32,
                    (p.y - video.top as f32) as i32,
                )
            })
        };
        match event.action {
            action::MOVE => {
                for sample in &event.history {
                    for (finger, f) in self.fingers.iter_mut().enumerate() {
                        if let Some(&(x, y)) =
                            sample.positions.get(finger).filter(|_| finger < count)
                        {
                            let (x, y) = (
                                (x - video.left as f32) as i32,
                                (y - video.top as f32) as i32,
                            );
                            f.moved(finger, x, y, video, sink);
                        }
                    }
                }
                for (finger, f) in self.fingers.iter_mut().enumerate() {
                    if finger < count {
                        let (x, y) = at(finger);
                        f.moved(finger, x, y, video, sink);
                    }
                }
            }
            action::DOWN | action::POINTER_DOWN => {
                for (finger, f) in self.fingers.iter_mut().enumerate() {
                    f.pointers(finger, count, sink);
                }
                let (x, y) = at(index);
                self.fingers[index].down(index, x, y, event.event_time_ms, now_ms, true);
            }
            action::UP | action::POINTER_UP => {
                let (x, y) = at(index);
                if event.flags & FLAG_CANCELED != 0 {
                    self.fingers[index].cancel(sink);
                } else {
                    self.fingers[index].up(index, x, y, event.event_time_ms, now_ms, video, sink);
                }
                for (finger, f) in self.fingers.iter_mut().enumerate() {
                    f.pointers(finger, count.saturating_sub(1), sink);
                }
                if index == 0 && count > 1 && !self.fingers[0].cancelled {
                    // The second finger is now the first.
                    let (x, y) = at(1);
                    self.fingers[0].down(0, x, y, event.event_time_ms, now_ms, false);
                }
            }
            action::CANCEL => {
                for (finger, f) in self.fingers.iter_mut().enumerate() {
                    f.cancel(sink);
                    f.pointers(finger, 0, sink);
                }
            }
            _ => return false,
        }
        true
    }

    /// The absolute mouse's timers that are due, in their order.
    pub(crate) fn tick(&mut self, now_ms: i64, video: Rect, sink: &mut dyn Sink) {
        while let Some((finger, timer)) = self
            .fingers
            .iter()
            .enumerate()
            .flat_map(|(finger, f)| f.due(now_ms).map(|timer| (finger, timer)))
            .min_by_key(|&(_, (at, _))| at)
        {
            self.fingers[finger].fire(timer.1, video, sink);
        }
    }

    pub(crate) fn next_deadline(&self) -> Option<i64> {
        self.fingers.iter().flat_map(Finger::deadline).min()
    }
}

/// `trySendTouchEvent`. False: the event is not a touch, or the host takes
/// none.
fn send_touch(
    event: &MotionEvent,
    device: Option<&Device>,
    video: Rect,
    sink: &mut dyn Sink,
) -> bool {
    let Some(kind) = touch_type(event) else {
        return false;
    };
    let sent = |sink: &mut dyn Sink, packet| sink.send(packet) != UNSUPPORTED;
    match event.action {
        action::MOVE => (0..event.pointers.len())
            .all(|pointer| sent(sink, touch_packet(event, device, video, kind, pointer))),
        action::CANCEL => sent(
            sink,
            Packet::Touch {
                kind: kind::CANCEL_ALL,
                pointer: 0,
                x: 0.0,
                y: 0.0,
                pressure: 0.0,
                contact_major: 0.0,
                contact_minor: 0.0,
                rotation: ROTATION_UNKNOWN,
            },
        ),
        _ => sent(
            sink,
            touch_packet(event, device, video, kind, event.action_index),
        ),
    }
}

/// `sendTouchEventForPointer`: the place in the picture from 0 to 1, the
/// pressure (or a hover's distance), the contact area and the rotation.
fn touch_packet(
    event: &MotionEvent,
    device: Option<&Device>,
    video: Rect,
    kind: u8,
    index: usize,
) -> Packet {
    let p = event.pointers.get(index).copied().unwrap_or_default();
    let (width, height) = size(video);
    let (x, y) = in_picture(video, p.x, p.y);
    let hover = matches!(
        event.action,
        action::HOVER_ENTER | action::HOVER_MOVE | action::HOVER_EXIT
    );
    let pointer_range = |axis: u32| device.and_then(|device| device.pointer_range(axis));
    let pressure = if hover {
        pointer_range(axis::DISTANCE).map_or(0.0, |range| {
            (p.distance - range.min) / (range.max - range.min)
        })
    } else {
        p.pressure
    };
    let oriented = pointer_range(axis::ORIENTATION).is_some();
    let orientation = if oriented { p.orientation } else { FRAC_PI_4 };
    let (major, minor) = if hover {
        (p.tool_major, p.tool_minor)
    } else {
        (p.touch_major, p.touch_minor)
    };
    // The contact area's axes, scaled to the picture's size.
    let scaled = |r: f32, theta: f32| {
        let (cx, cy) = (r * theta.cos(), r * theta.sin());
        let (nx, ny) = (cx.abs().min(width) / width, cy.abs().min(height) / height);
        (nx * nx + ny * ny).sqrt()
    };
    let rotation = if oriented {
        let degrees = p.orientation.to_degrees() as i16;
        (if degrees < 0 { degrees + 360 } else { degrees }) as u16
    } else {
        ROTATION_UNKNOWN
    };
    Packet::Touch {
        kind,
        pointer: p.id as u32,
        x: x / width,
        y: y / height,
        pressure,
        contact_major: scaled(major, orientation),
        contact_minor: scaled(minor, orientation + std::f32::consts::FRAC_PI_2),
        rotation,
    }
}

const LONG_PRESS_MS: i64 = 650;
const LONG_PRESS_DISTANCE: f64 = 30.0;
const DOUBLE_TAP_MS: i64 = 250;
const DOUBLE_TAP_DISTANCE: f64 = 60.0;
const TOUCH_DOWN_DEAD_ZONE_MS: i64 = 100;
const TOUCH_DOWN_DEAD_ZONE_DISTANCE: f64 = 20.0;
const LEFT_BUTTON_UP_MS: i64 = 100;
const SCROLL_SPEED_FACTOR: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Timer {
    TapDown,
    LongPress,
    LeftUp,
}

/// `AbsoluteTouchContext`: one finger as an absolute mouse.
#[derive(Debug, Default)]
struct Finger {
    down_at: (i32, i32),
    down_ms: i64,
    up_at: (i32, i32),
    up_ms: i64,
    last: (i32, i32),
    cancelled: bool,
    long_press: bool,
    tap: bool,
    tap_down_ms: Option<i64>,
    long_press_ms: Option<i64>,
    left_up_ms: Option<i64>,
}

fn distance(a: (i32, i32), b: (i32, i32)) -> f64 {
    (f64::from(a.0 - b.0).powi(2) + f64::from(a.1 - b.1).powi(2)).sqrt()
}

impl Finger {
    fn down(&mut self, index: usize, x: i32, y: i32, time_ms: i64, now_ms: i64, new: bool) {
        if !new {
            // A finger that moves up to first place is no new touch.
            return;
        }
        self.down_at = (x, y);
        self.last = (x, y);
        self.down_ms = time_ms;
        self.cancelled = false;
        self.tap = false;
        self.long_press = false;
        if index == 0 {
            self.tap_down_ms = Some(now_ms + TOUCH_DOWN_DEAD_ZONE_MS);
            self.long_press_ms = Some(now_ms + LONG_PRESS_MS);
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Artemis's touchUpEvent and its context"
    )]
    fn up(
        &mut self,
        index: usize,
        x: i32,
        y: i32,
        time_ms: i64,
        now_ms: i64,
        video: Rect,
        sink: &mut dyn Sink,
    ) {
        if self.cancelled {
            return;
        }
        if index == 0 {
            self.long_press_ms = None;
            self.tap_down_ms = None;
            if self.long_press {
                button(sink, false, MOUSE_RIGHT);
            } else if self.tap {
                button(sink, false, MOUSE_LEFT);
            } else {
                // A tap inside the dead zone's time: click where it went
                // down, and let go 100 ms later for hosts that poll.
                self.confirm_tap(video, sink);
                self.left_up_ms = Some(now_ms + LEFT_BUTTON_UP_MS);
            }
        }
        self.last = (x, y);
        self.up_at = (x, y);
        self.up_ms = time_ms;
    }

    fn moved(&mut self, index: usize, x: i32, y: i32, video: Rect, sink: &mut dyn Sink) {
        if self.cancelled {
            return;
        }
        if index == 0 {
            if distance((x, y), self.down_at) > LONG_PRESS_DISTANCE {
                self.long_press_ms = None;
            }
            // Motion inside the dead zone after the touch is ignored.
            if self.tap || distance((x, y), self.down_at) > TOUCH_DOWN_DEAD_ZONE_DISTANCE {
                self.confirm_tap(video, sink);
                position(video, x, y, sink);
            }
        } else if index == 1 {
            sink.send(Packet::Scroll {
                amount: ((y - self.last.1) * SCROLL_SPEED_FACTOR) as i16,
            });
        }
        self.last = (x, y);
    }

    /// `tapConfirmed`: the left button goes down, at the touch's place
    /// unless it is a double tap.
    fn confirm_tap(&mut self, video: Rect, sink: &mut dyn Sink) {
        if self.tap || self.long_press {
            return;
        }
        self.tap = true;
        self.tap_down_ms = None;
        if self.down_ms - self.up_ms > DOUBLE_TAP_MS
            || distance(self.down_at, self.up_at) > DOUBLE_TAP_DISTANCE
        {
            position(video, self.down_at.0, self.down_at.1, sink);
        }
        button(sink, true, MOUSE_LEFT);
    }

    fn cancel(&mut self, sink: &mut dyn Sink) {
        self.cancelled = true;
        self.long_press_ms = None;
        self.tap_down_ms = None;
        if self.long_press {
            button(sink, false, MOUSE_RIGHT);
        } else if self.tap {
            button(sink, false, MOUSE_LEFT);
        }
    }

    /// `setPointerCount`: a second finger ends the first one's click.
    fn pointers(&mut self, index: usize, count: usize, sink: &mut dyn Sink) {
        if index == 0 && count > 1 {
            self.cancel(sink);
        }
    }

    fn timers(&self) -> impl Iterator<Item = (i64, Timer)> + '_ {
        [
            self.tap_down_ms.map(|at| (at, Timer::TapDown)),
            self.long_press_ms.map(|at| (at, Timer::LongPress)),
            self.left_up_ms.map(|at| (at, Timer::LeftUp)),
        ]
        .into_iter()
        .flatten()
    }

    fn due(&self, now_ms: i64) -> Option<(i64, Timer)> {
        self.timers()
            .filter(|&(at, _)| at <= now_ms)
            .min_by_key(|&(at, _)| at)
    }

    fn deadline(&self) -> Option<i64> {
        self.timers().map(|(at, _)| at).min()
    }

    fn fire(&mut self, timer: Timer, video: Rect, sink: &mut dyn Sink) {
        match timer {
            Timer::TapDown => {
                self.tap_down_ms = None;
                self.confirm_tap(video, sink);
            }
            Timer::LongPress => {
                // A long press is a right click, not a left one.
                self.long_press_ms = None;
                self.tap_down_ms = None;
                self.long_press = true;
                if self.tap {
                    button(sink, false, MOUSE_LEFT);
                }
                button(sink, true, MOUSE_RIGHT);
            }
            Timer::LeftUp => {
                self.left_up_ms = None;
                button(sink, false, MOUSE_LEFT);
            }
        }
    }
}

fn button(sink: &mut dyn Sink, pressed: bool, button: u8) {
    sink.send(Packet::MouseButton { pressed, button });
}

/// `updatePosition`: the host's pointer to this place in the picture.
fn position(video: Rect, x: i32, y: i32, sink: &mut dyn Sink) {
    let (width, height) = size(video);
    let (x, y) = (
        (x as f32).clamp(0.0, width) as i16,
        (y as f32).clamp(0.0, height) as i16,
    );
    sink.send(Packet::MousePosition {
        x,
        y,
        width: width as i16,
        height: height as i16,
    });
}

/// A mouse, or a pen or eraser used as one: whether `handleMotionEvent`
/// takes the event as a mouse's.
pub(crate) fn is_mouse(event: &MotionEvent) -> bool {
    use super::input::source;
    let tool = event.pointers.first().map(|p| p.tool_type);
    event.source == source::MOUSE
        || event.source == source::MOUSE_RELATIVE
        || matches!(tool, Some(tool::MOUSE | tool::STYLUS | tool::ERASER))
        // Samsung DeX's desktop mouse.
        || event.source == 12290
}

/// A mouse's buttons, by Android's button state.
const BUTTON_PRIMARY: u32 = 0x01;
const BUTTON_SECONDARY: u32 = 0x02;
const BUTTON_TERTIARY: u32 = 0x04;
const BUTTON_STYLUS_PRIMARY: u32 = 0x20;
const BUTTON_STYLUS_SECONDARY: u32 = 0x40;

/// A mouse without pointer capture: its place in the picture, its buttons
/// and its wheel (`handleMotionEvent`'s mouse branch).
#[derive(Debug, Default)]
pub(crate) struct Mouse {
    buttons: u32,
}

impl Mouse {
    pub(crate) fn event(&mut self, event: &MotionEvent, video: Rect, sink: &mut dyn Sink) {
        let state = event.button_state;
        let changed = state ^ self.buttons;
        if let Some(p) = event.pointers.first() {
            // The place first: a pen without hover may have moved far.
            let (x, y) = in_picture(video, p.x, p.y);
            let (width, height) = size(video);
            sink.send(Packet::MousePosition {
                x: x as i16,
                y: y as i16,
                width: width as i16,
                height: height as i16,
            });
        }
        if event.action == action::SCROLL {
            sink.send(Packet::Scroll {
                amount: (event.axis(axis::VSCROLL) * 120.0) as i16,
            });
            sink.send(Packet::HScroll {
                amount: (event.axis(axis::HSCROLL) * 120.0) as i16,
            });
        }
        for (mask, mouse_button) in [
            (BUTTON_PRIMARY, MOUSE_LEFT),
            // A pen's first button is a right click, its second a middle one.
            (BUTTON_SECONDARY | BUTTON_STYLUS_PRIMARY, MOUSE_RIGHT),
            (BUTTON_TERTIARY | BUTTON_STYLUS_SECONDARY, MOUSE_MIDDLE),
        ] {
            if changed & mask != 0 {
                button(sink, state & mask != 0, mouse_button);
            }
        }
        // A pen touching is a left click, an eraser a right one.
        if event.pointers.len() == 1 && event.action_index == 0 {
            let tool = event.pointers[0].tool_type;
            let pressed = match event.action {
                action::DOWN => Some(true),
                action::UP | action::CANCEL => Some(false),
                _ => None,
            };
            if let Some(pressed) = pressed {
                match tool {
                    tool::STYLUS => button(sink, pressed, MOUSE_LEFT),
                    tool::ERASER => button(sink, pressed, MOUSE_RIGHT),
                    _ => {}
                }
            }
        }
        self.buttons = state;
    }

    /// The right button that Android sends as a Back key from a mouse.
    pub(crate) fn back(&mut self, pressed: bool, sink: &mut dyn Sink) {
        button(sink, pressed, MOUSE_RIGHT);
    }
}
