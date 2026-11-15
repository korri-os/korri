// SPDX-License-Identifier: GPL-3.0-only
//! The gameplay overlay's keys (#391 step D, slice D5): which keys open and
//! close the overlay, and which go to the stream, to Android or to the
//! overlay, as Artemis routes them in Korri mode. Pure; the app shows the
//! overlay's layer and holds the stream's input (`input::Input::hold`).
//!
//! Guide and Back toggle the overlay on their release, from any device. A
//! whole press belongs to the state it began in, so the release of the
//! press that opened the overlay does not close it again. While the overlay
//! is open, the volume and power keys stay Android's and every other key is
//! the overlay's: none reaches the stream.
//!
//! Two things can show the overlay ([`Presenter`]): slice D5's stand-in in
//! the viewer's own app, which Guide or Back closes, and the shell's
//! gameplay overlay in the Korri app (#434 slice D5b). The shell's overlay
//! takes its input as Artemis's `KorriSurfaceHost` hands it over
//! ([`Forwarded`]), Guide's and Back's presses too, and closes when korrid
//! resumes the stream.
//!
//! Ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`): `KorriOverlayHomeKey.java`,
//! `KorriSurfaceHost.java` (`isSystemKey`, `forward`, `forwardTouch`) and
//! `Game.java`'s Korri-mode routing (`dispatchKeyEvent`, `handleKeyDown`,
//! `handleKeyUp`, `handleKorriHomeKey`, `dispatchGenericMotionEvent`,
//! `dispatchTouchEvent`). Artemis ships under the GNU GPL version 3 with no
//! "or later" grant, so this file does too. The Rust adaptation is Korri
//! work dated 2026-11-14 and 2026-11-15.
use super::input::{KeyAction, KeyEvent, MotionEvent, action, axis, source};
use std::collections::BTreeMap;

/// Android's key codes that this file names.
mod code {
    pub const BACK: u32 = 4;
    pub const MENU: u32 = 82;
    pub const VOLUME_UP: u32 = 24;
    pub const VOLUME_DOWN: u32 = 25;
    pub const POWER: u32 = 26;
    pub const BUTTON_MODE: u32 = 110;
    pub const VOLUME_MUTE: u32 = 164;
}

/// `KeyEvent.FLAG_CANCELED`.
const FLAG_CANCELED: u32 = 0x20;

/// The overlay over the stream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overlay {
    #[default]
    Closed,
    Open,
}

/// What shows the overlay over the stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presenter {
    /// Slice D5's stand-in, in the viewer's own app: the overlay layer shows
    /// it, the overlay drops its input, and Guide or Back closes it.
    StandIn,
    /// The shell's gameplay overlay, in the Korri app (slice D5b): the app
    /// lends the overlay layer's Surface to the shell, the overlay's input
    /// waits for the shell ([`Forwarded`]), and the app closes it when
    /// korrid resumes the stream.
    Shell,
}

/// Where a key goes while the stream runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// The stream's input.
    Stream,
    /// Android's own: the volume and power keys while the overlay is open.
    Android,
    /// The open overlay's key: the stream never sees it.
    Overlay,
    /// An edge of Guide or Back that does nothing by itself (a down, a
    /// repeat, an up that no press of this device began); nobody sees it.
    Taken,
    /// Guide or Back came up after a press that began with the overlay
    /// closed: open it.
    Open,
    /// Guide or Back came up after a press that began with the overlay
    /// open: close it.
    Close,
}

/// The keys that Android keeps whatever overlay shows
/// (`KorriSurfaceHost.isSystemKey`).
pub fn is_system_key(key_code: u32) -> bool {
    matches!(
        key_code,
        code::VOLUME_UP | code::VOLUME_DOWN | code::VOLUME_MUTE | code::POWER
    )
}

/// The overlay's key routing: `KorriOverlayHomeKey`, with the state of each
/// Guide and Back press that is down.
#[derive(Debug, Default)]
pub struct Keys {
    /// Each press that is down, by device and key: was the overlay open
    /// when it began?
    pressed: BTreeMap<(i32, u32), Overlay>,
}

impl Keys {
    pub fn route(&mut self, event: &KeyEvent, overlay: Overlay) -> Route {
        if overlay == Overlay::Open && is_system_key(event.key_code) {
            return Route::Android;
        }
        if event.key_code != code::BUTTON_MODE && event.key_code != code::BACK {
            return match overlay {
                Overlay::Open => Route::Overlay,
                Overlay::Closed => Route::Stream,
            };
        }
        let press = (event.device, event.key_code);
        match event.action {
            KeyAction::Down => {
                if event.repeat_count == 0 {
                    self.pressed.entry(press).or_insert(overlay);
                }
                Route::Taken
            }
            KeyAction::Up => match self.pressed.remove(&press) {
                None => Route::Taken,
                Some(_) if event.flags & FLAG_CANCELED != 0 => Route::Taken,
                Some(Overlay::Closed) => Route::Open,
                Some(Overlay::Open) => Route::Close,
            },
            KeyAction::Multiple => Route::Taken,
        }
    }
}

/// `MotionEvent.ACTION_OUTSIDE`: the input's actions do not name it.
const OUTSIDE: u32 = 4;
/// `InputDevice.SOURCE_DPAD`.
const DPAD: u32 = 0x0000_0201;

/// The shell's open overlay's input, in the form that its host hands it to
/// the shell (`korri_shell::backend::android::Overlay`'s `take_keys`,
/// `take_axes` and `take_pointers`): Artemis's `KorriSurfaceHost` queues.
/// The shell maps the keys and the sticks to its buttons, and gives the
/// touches to the overlay's window.
#[derive(Debug, Default)]
pub struct Forwarded {
    /// Triples: device, Android key code, pressed (1) or released (0).
    keys: Vec<i32>,
    /// Quintuples: device, then X, Y, hat X and hat Y in thousandths.
    axes: Vec<i32>,
    /// Quadruples: Android action, pointer, X and Y in the window's pixels.
    pointers: Vec<f32>,
}

impl Forwarded {
    /// A key that the open overlay takes (`KorriSurfaceHost.forward`): its
    /// first down and its up. Guide is the host's own key, so the overlay
    /// hears Menu, which closes it even over a question. Android's own keys
    /// never reach it.
    pub fn key(&mut self, event: &KeyEvent) {
        if is_system_key(event.key_code) {
            return;
        }
        let pressed = match event.action {
            KeyAction::Down if event.repeat_count == 0 => 1,
            KeyAction::Up => 0,
            _ => return,
        };
        let key_code = if event.key_code == code::BUTTON_MODE {
            code::MENU
        } else {
            event.key_code
        };
        self.keys.extend([event.device, key_code as i32, pressed]);
    }

    /// Guide or Back came up after a press that began with the overlay open
    /// ([`Route::Close`]): the whole press goes to the overlay now, its down
    /// and then this up (`Game.handleKorriHomeKey`), and the overlay decides.
    pub fn press(&mut self, up: &KeyEvent) {
        let down = KeyEvent {
            action: KeyAction::Down,
            repeat_count: 0,
            ..up.clone()
        };
        self.key(&down);
        self.key(up);
    }

    /// A motion event that the open overlay takes. A touch, or a mouse's
    /// press, drag and release, goes to the overlay's window
    /// (`Game.dispatchTouchEvent`); any other motion is a controller's stick
    /// or nothing (`Game.dispatchGenericMotionEvent`), as Android splits them
    /// (`MotionEvent.isTouchEvent`).
    pub fn motion(&mut self, event: &MotionEvent) {
        let touch = matches!(
            event.action,
            action::DOWN
                | action::UP
                | action::MOVE
                | action::CANCEL
                | OUTSIDE
                | action::POINTER_DOWN
                | action::POINTER_UP
        );
        if event.source & source::CLASS_POINTER != 0 && touch {
            self.touch(event);
        } else {
            self.stick(event);
        }
    }

    /// `KorriSurfaceHost.forward(MotionEvent)`: a controller's raw stick and
    /// hat, which the shell turns into buttons with its own threshold and
    /// repeat. A cancel centres them.
    fn stick(&mut self, event: &MotionEvent) {
        let controller = event.source & source::CLASS_JOYSTICK != 0 || event.source & DPAD == DPAD;
        if !controller || !matches!(event.action, action::MOVE | action::CANCEL) {
            return;
        }
        self.axes.push(event.device);
        for id in [axis::X, axis::Y, axis::HAT_X, axis::HAT_Y] {
            let value = if event.action == action::CANCEL {
                0.0
            } else {
                event.axis(id)
            };
            let value = if value.is_finite() { value } else { 0.0 };
            self.axes
                .push((value.clamp(-1.0, 1.0) * 1000.0).round() as i32);
        }
    }

    /// `KorriSurfaceHost.forwardTouch`: the finger that went down or up, or
    /// every finger of a move or a cancel, in the window's pixels. The
    /// overlay's layer covers the window from its corner, so they are the
    /// layer's pixels too. A controller's touch never comes here.
    fn touch(&mut self, event: &MotionEvent) {
        if event.source & source::CLASS_JOYSTICK != 0
            || event.source & DPAD == DPAD
            || event.source & source::GAMEPAD == source::GAMEPAD
        {
            return;
        }
        let phase = match event.action {
            action::DOWN => 0,
            action::POINTER_DOWN => 5,
            action::UP => 1,
            action::POINTER_UP => 6,
            action::MOVE => 2,
            action::CANCEL | OUTSIDE => 3,
            _ => return,
        };
        // One finger changes on a down or an up: the action's own.
        let one = matches!(phase, 0 | 1 | 5 | 6);
        for (at, pointer) in event.pointers.iter().enumerate() {
            if (one && at != event.action_index) || !pointer.x.is_finite() || !pointer.y.is_finite()
            {
                continue;
            }
            self.pointers
                .extend([phase as f32, pointer.id as f32, pointer.x, pointer.y]);
        }
    }

    pub fn take_keys(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.keys)
    }

    pub fn take_axes(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.axes)
    }

    pub fn take_pointers(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.pointers)
    }

    /// Drop what waits: the overlay is closing or closed.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAD: i32 = 16;
    const KEYBOARD: i32 = -1;
    const BUTTON_A: u32 = 96;

    fn key(device: i32, key_code: u32, action: KeyAction) -> KeyEvent {
        KeyEvent {
            device,
            action,
            key_code,
            ..Default::default()
        }
    }

    #[test]
    fn guide_opens_the_overlay_when_it_comes_up_and_never_reaches_the_stream() {
        let mut keys = Keys::default();
        let guide = |action| key(PAD, code::BUTTON_MODE, action);
        assert_eq!(
            keys.route(&guide(KeyAction::Down), Overlay::Closed),
            Route::Taken
        );
        assert_eq!(
            keys.route(&guide(KeyAction::Up), Overlay::Closed),
            Route::Open
        );
        assert_eq!(
            keys.route(&guide(KeyAction::Down), Overlay::Open),
            Route::Taken
        );
        assert_eq!(
            keys.route(&guide(KeyAction::Up), Overlay::Open),
            Route::Close
        );
    }

    #[test]
    fn back_toggles_the_overlay_as_guide_does_from_any_device() {
        let mut keys = Keys::default();
        for device in [PAD, KEYBOARD] {
            let back = |action| key(device, code::BACK, action);
            keys.route(&back(KeyAction::Down), Overlay::Closed);
            assert_eq!(
                keys.route(&back(KeyAction::Up), Overlay::Closed),
                Route::Open
            );
            keys.route(&back(KeyAction::Down), Overlay::Open);
            assert_eq!(
                keys.route(&back(KeyAction::Up), Overlay::Open),
                Route::Close
            );
        }
    }

    #[test]
    fn a_press_belongs_to_the_state_it_began_in() {
        let mut keys = Keys::default();
        let guide = |action| key(PAD, code::BUTTON_MODE, action);
        // Down with the overlay closed; it opened another way before the up.
        keys.route(&guide(KeyAction::Down), Overlay::Closed);
        assert_eq!(
            keys.route(&guide(KeyAction::Up), Overlay::Open),
            Route::Open
        );
        // Down with the overlay open; it closed another way before the up.
        keys.route(&guide(KeyAction::Down), Overlay::Open);
        assert_eq!(
            keys.route(&guide(KeyAction::Up), Overlay::Closed),
            Route::Close
        );
        // Another device's press is its own.
        keys.route(&guide(KeyAction::Down), Overlay::Closed);
        assert_eq!(
            keys.route(
                &key(PAD + 1, code::BUTTON_MODE, KeyAction::Up),
                Overlay::Closed
            ),
            Route::Taken
        );
        assert_eq!(
            keys.route(&guide(KeyAction::Up), Overlay::Closed),
            Route::Open
        );
    }

    #[test]
    fn a_repeat_an_unknown_up_and_a_cancelled_up_do_nothing() {
        let mut keys = Keys::default();
        let mut repeat = key(PAD, code::BUTTON_MODE, KeyAction::Down);
        repeat.repeat_count = 1;
        assert_eq!(keys.route(&repeat, Overlay::Closed), Route::Taken);
        assert_eq!(
            keys.route(&key(PAD, code::BUTTON_MODE, KeyAction::Up), Overlay::Closed),
            Route::Taken
        );
        keys.route(&key(PAD, code::BACK, KeyAction::Down), Overlay::Closed);
        let mut cancelled = key(PAD, code::BACK, KeyAction::Up);
        cancelled.flags = FLAG_CANCELED;
        assert_eq!(keys.route(&cancelled, Overlay::Closed), Route::Taken);
    }

    #[test]
    fn while_open_the_volume_and_power_stay_androids_and_the_rest_is_the_overlays() {
        let mut keys = Keys::default();
        for key_code in [
            code::VOLUME_UP,
            code::VOLUME_DOWN,
            code::VOLUME_MUTE,
            code::POWER,
        ] {
            assert_eq!(
                keys.route(&key(KEYBOARD, key_code, KeyAction::Down), Overlay::Open),
                Route::Android
            );
            // Closed, the stream's input decides, as before the overlay.
            assert_eq!(
                keys.route(&key(KEYBOARD, key_code, KeyAction::Down), Overlay::Closed),
                Route::Stream
            );
        }
        for action in [KeyAction::Down, KeyAction::Up] {
            assert_eq!(
                keys.route(&key(PAD, BUTTON_A, action), Overlay::Open),
                Route::Overlay
            );
            assert_eq!(
                keys.route(&key(PAD, BUTTON_A, action), Overlay::Closed),
                Route::Stream
            );
        }
    }

    // The shell's overlay's input (slice D5b), as `KorriSurfaceHost` queues it.

    const MOVE_X_Y: [(u32, f32); 4] = [
        (axis::X, 0.5),
        (axis::Y, -1.2),
        (axis::HAT_X, 1.0),
        (axis::HAT_Y, f32::NAN),
    ];

    fn motion(
        source: u32,
        action: u32,
        action_index: usize,
        pointers: &[(i32, f32, f32)],
    ) -> MotionEvent {
        MotionEvent {
            device: PAD,
            source,
            action,
            action_index,
            button_state: 0,
            flags: 0,
            event_time_ms: 0,
            pointers: pointers
                .iter()
                .map(|&(id, x, y)| crate::input::Pointer {
                    id,
                    x,
                    y,
                    ..Default::default()
                })
                .collect(),
            history: Vec::new(),
            axes: [0.0; axis::COUNT],
        }
    }

    fn stick(source: u32, action: u32) -> MotionEvent {
        let mut event = motion(source, action, 0, &[(0, 0.0, 0.0)]);
        for (id, value) in MOVE_X_Y {
            event.axes[id as usize] = value;
        }
        event
    }

    #[test]
    fn the_shells_overlay_hears_a_keys_first_down_and_its_up() {
        let mut forwarded = Forwarded::default();
        forwarded.key(&key(PAD, BUTTON_A, KeyAction::Down));
        let mut repeat = key(PAD, BUTTON_A, KeyAction::Down);
        repeat.repeat_count = 1;
        forwarded.key(&repeat);
        forwarded.key(&key(PAD, BUTTON_A, KeyAction::Multiple));
        forwarded.key(&key(PAD, BUTTON_A, KeyAction::Up));
        assert_eq!(
            forwarded.take_keys(),
            [PAD, BUTTON_A as i32, 1, PAD, BUTTON_A as i32, 0]
        );
        assert!(forwarded.take_keys().is_empty(), "taken once");
    }

    #[test]
    fn guide_reaches_the_shells_overlay_as_menu_and_androids_keys_never_do() {
        let mut forwarded = Forwarded::default();
        forwarded.key(&key(PAD, code::BUTTON_MODE, KeyAction::Down));
        for key_code in [
            code::VOLUME_UP,
            code::VOLUME_DOWN,
            code::VOLUME_MUTE,
            code::POWER,
        ] {
            forwarded.key(&key(KEYBOARD, key_code, KeyAction::Down));
        }
        assert_eq!(forwarded.take_keys(), [PAD, code::MENU as i32, 1]);
    }

    #[test]
    fn a_guide_or_back_press_that_began_open_reaches_the_overlay_whole_at_its_up() {
        let mut forwarded = Forwarded::default();
        let mut keys = Keys::default();
        for (key_code, heard) in [(code::BUTTON_MODE, code::MENU), (code::BACK, code::BACK)] {
            assert_eq!(
                keys.route(&key(PAD, key_code, KeyAction::Down), Overlay::Open),
                Route::Taken
            );
            let up = key(PAD, key_code, KeyAction::Up);
            assert_eq!(keys.route(&up, Overlay::Open), Route::Close);
            forwarded.press(&up);
            assert_eq!(
                forwarded.take_keys(),
                [PAD, heard as i32, 1, PAD, heard as i32, 0]
            );
        }
    }

    #[test]
    fn a_controllers_stick_and_hat_reach_the_overlay_in_thousandths() {
        let mut forwarded = Forwarded::default();
        forwarded.motion(&stick(source::JOYSTICK, action::MOVE));
        // A D-pad that reports a hat is a controller too.
        forwarded.motion(&stick(DPAD, action::MOVE));
        // A cancel centres them.
        forwarded.motion(&stick(source::JOYSTICK, action::CANCEL));
        assert_eq!(
            forwarded.take_axes(),
            [
                PAD, 500, -1000, 1000, 0, PAD, 500, -1000, 1000, 0, PAD, 0, 0, 0, 0
            ]
        );
        assert!(forwarded.take_pointers().is_empty());
    }

    #[test]
    fn other_motions_that_are_no_touch_go_nowhere() {
        let mut forwarded = Forwarded::default();
        // A controller's other actions, a mouse's hover and scroll, a trackball.
        forwarded.motion(&stick(source::JOYSTICK, action::DOWN));
        forwarded.motion(&motion(
            source::MOUSE,
            action::HOVER_MOVE,
            0,
            &[(0, 1.0, 1.0)],
        ));
        forwarded.motion(&motion(source::MOUSE, action::SCROLL, 0, &[(0, 1.0, 1.0)]));
        forwarded.motion(&motion(
            source::MOUSE_RELATIVE,
            action::MOVE,
            0,
            &[(0, 1.0, 1.0)],
        ));
        assert!(forwarded.take_axes().is_empty());
        assert!(forwarded.take_pointers().is_empty());
    }

    #[test]
    fn a_touch_reaches_the_overlay_in_pixels_with_androids_action() {
        let mut forwarded = Forwarded::default();
        let screen = source::TOUCHSCREEN;
        forwarded.motion(&motion(screen, action::DOWN, 0, &[(7, 100.5, 200.0)]));
        // A second finger: only it went down.
        forwarded.motion(&motion(
            screen,
            action::POINTER_DOWN,
            1,
            &[(7, 101.0, 201.0), (9, 300.0, 400.0)],
        ));
        // A move: every finger.
        forwarded.motion(&motion(
            screen,
            action::MOVE,
            0,
            &[(7, 102.0, 202.0), (9, 301.0, 401.0)],
        ));
        // The first finger lifts, then the second.
        forwarded.motion(&motion(
            screen,
            action::POINTER_UP,
            0,
            &[(7, 102.0, 202.0), (9, 301.0, 401.0)],
        ));
        forwarded.motion(&motion(screen, action::UP, 0, &[(9, 302.0, 402.0)]));
        assert_eq!(
            forwarded.take_pointers(),
            [
                0.0, 7.0, 100.5, 200.0, 5.0, 9.0, 300.0, 400.0, 2.0, 7.0, 102.0, 202.0, 2.0, 9.0,
                301.0, 401.0, 6.0, 7.0, 102.0, 202.0, 1.0, 9.0, 302.0, 402.0,
            ]
        );
        // A cancel and an outside touch cancel every finger; a mouse's
        // press is a touch, as Artemis's activity receives it.
        forwarded.motion(&motion(
            screen,
            action::CANCEL,
            0,
            &[(1, 1.0, 2.0), (2, 3.0, 4.0)],
        ));
        forwarded.motion(&motion(screen, OUTSIDE, 0, &[(1, 5.0, 6.0)]));
        forwarded.motion(&motion(source::MOUSE, action::DOWN, 0, &[(0, 7.0, 8.0)]));
        assert_eq!(
            forwarded.take_pointers(),
            [
                3.0, 1.0, 1.0, 2.0, 3.0, 2.0, 3.0, 4.0, 3.0, 1.0, 5.0, 6.0, 0.0, 0.0, 7.0, 8.0
            ]
        );
        assert!(forwarded.take_axes().is_empty());
    }

    #[test]
    fn a_touch_off_the_numbers_or_from_a_controller_goes_nowhere() {
        let mut forwarded = Forwarded::default();
        let screen = source::TOUCHSCREEN;
        forwarded.motion(&motion(screen, action::DOWN, 0, &[(0, f32::NAN, 1.0)]));
        forwarded.motion(&motion(
            screen,
            action::MOVE,
            0,
            &[(0, 1.0, f32::INFINITY), (1, 2.0, 3.0)],
        ));
        forwarded.motion(&motion(
            screen | source::GAMEPAD,
            action::DOWN,
            0,
            &[(0, 1.0, 1.0)],
        ));
        forwarded.motion(&motion(screen, action::BUTTON_PRESS, 0, &[(0, 1.0, 1.0)]));
        assert_eq!(forwarded.take_pointers(), [2.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn clear_drops_what_waits() {
        let mut forwarded = Forwarded::default();
        forwarded.key(&key(PAD, BUTTON_A, KeyAction::Down));
        forwarded.motion(&stick(source::JOYSTICK, action::MOVE));
        forwarded.motion(&motion(
            source::TOUCHSCREEN,
            action::DOWN,
            0,
            &[(0, 1.0, 1.0)],
        ));
        forwarded.clear();
        assert!(forwarded.take_keys().is_empty());
        assert!(forwarded.take_axes().is_empty());
        assert!(forwarded.take_pointers().is_empty());
    }
}
