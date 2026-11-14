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
//! Ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`): `KorriOverlayHomeKey.java`,
//! `KorriSurfaceHost.isSystemKey` and `Game.java`'s Korri-mode routing
//! (`dispatchKeyEvent`, `handleKeyDown`, `handleKeyUp`). Artemis ships under
//! the GNU GPL version 3 with no "or later" grant, so this file does too.
//! The Rust adaptation is Korri work dated 2026-11-14.
use super::input::{KeyAction, KeyEvent};
use std::collections::BTreeMap;

/// Android's key codes that this file names.
mod code {
    pub const BACK: u32 = 4;
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

/// Where a key goes while the stream runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// The stream's input.
    Stream,
    /// Android's own: the volume and power keys while the overlay is open.
    Android,
    /// The overlay's; the stream never sees it.
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
                Overlay::Open => Route::Taken,
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
                Route::Taken
            );
            assert_eq!(
                keys.route(&key(PAD, BUTTON_A, action), Overlay::Closed),
                Route::Stream
            );
        }
    }
}
