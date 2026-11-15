// SPDX-License-Identifier: GPL-3.0-only
//! A keyboard key, from Android's key code to the Windows virtual-key code
//! that the stream carries. Pure; `input.rs` sends the result.
//!
//! Ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`, a fork of
//! `ClassicOldSong/moonlight-android`, itself a fork of
//! `moonlight-stream/moonlight-android`): `KeyboardTranslator.translate`,
//! `KeyMapper.getWindowsKeyCode` (the scan code table), and `Game.java`'s
//! modifier state and its Ctrl+Alt+Shift combinations
//! (`handleSpecialKeys`). Artemis ships under the GNU GPL version 3 with no
//! "or later" grant, so this file does too. The Rust adaptation is Korri
//! work dated 2026-11-14.
//!
//! Artemis maps a key to the QWERTY layout first on Android 13 and newer
//! (`InputDevice.getKeyCodeForKeyLocation`, API 33). At API 31 and 32 it
//! has no such map, so it sends every key with the non-normalized flag;
//! this file does the same at every API level.

/// Android key codes (`android.view.KeyEvent`) that the viewer reads.
pub(crate) mod code {
    pub const UNKNOWN: u32 = 0;
    pub const BACK: u32 = 4;
    pub const KEY_0: u32 = 7;
    pub const KEY_9: u32 = 16;
    pub const DPAD_UP: u32 = 19;
    pub const DPAD_DOWN: u32 = 20;
    pub const DPAD_LEFT: u32 = 21;
    pub const DPAD_RIGHT: u32 = 22;
    pub const DPAD_CENTER: u32 = 23;
    pub const CLEAR: u32 = 28;
    pub const A: u32 = 29;
    pub const C: u32 = 31;
    pub const Q: u32 = 45;
    pub const Z: u32 = 54;
    pub const COMMA: u32 = 55;
    pub const PERIOD: u32 = 56;
    pub const ALT_LEFT: u32 = 57;
    pub const ALT_RIGHT: u32 = 58;
    pub const SHIFT_LEFT: u32 = 59;
    pub const SHIFT_RIGHT: u32 = 60;
    pub const TAB: u32 = 61;
    pub const SPACE: u32 = 62;
    pub const ENTER: u32 = 66;
    pub const DEL: u32 = 67;
    pub const GRAVE: u32 = 68;
    pub const MINUS: u32 = 69;
    pub const EQUALS: u32 = 70;
    pub const LEFT_BRACKET: u32 = 71;
    pub const RIGHT_BRACKET: u32 = 72;
    pub const BACKSLASH: u32 = 73;
    pub const SEMICOLON: u32 = 74;
    pub const APOSTROPHE: u32 = 75;
    pub const SLASH: u32 = 76;
    pub const PLUS: u32 = 81;
    pub const MENU: u32 = 82;
    pub const SEARCH: u32 = 84;
    pub const PAGE_UP: u32 = 92;
    pub const PAGE_DOWN: u32 = 93;
    pub const BUTTON_A: u32 = 96;
    pub const BUTTON_B: u32 = 97;
    pub const BUTTON_C: u32 = 98;
    pub const BUTTON_X: u32 = 99;
    pub const BUTTON_Y: u32 = 100;
    pub const BUTTON_L1: u32 = 102;
    pub const BUTTON_R1: u32 = 103;
    pub const BUTTON_L2: u32 = 104;
    pub const BUTTON_R2: u32 = 105;
    pub const BUTTON_THUMBL: u32 = 106;
    pub const BUTTON_THUMBR: u32 = 107;
    pub const BUTTON_START: u32 = 108;
    pub const BUTTON_SELECT: u32 = 109;
    pub const BUTTON_MODE: u32 = 110;
    pub const ESCAPE: u32 = 111;
    pub const FORWARD_DEL: u32 = 112;
    pub const CTRL_LEFT: u32 = 113;
    pub const CTRL_RIGHT: u32 = 114;
    pub const CAPS_LOCK: u32 = 115;
    pub const SCROLL_LOCK: u32 = 116;
    pub const META_LEFT: u32 = 117;
    pub const META_RIGHT: u32 = 118;
    pub const SYSRQ: u32 = 120;
    pub const BREAK: u32 = 121;
    pub const MOVE_HOME: u32 = 122;
    pub const MOVE_END: u32 = 123;
    pub const INSERT: u32 = 124;
    pub const MEDIA_RECORD: u32 = 130;
    pub const F1: u32 = 131;
    pub const F12: u32 = 142;
    pub const NUM_LOCK: u32 = 143;
    pub const NUMPAD_0: u32 = 144;
    pub const NUMPAD_9: u32 = 153;
    pub const NUMPAD_DIVIDE: u32 = 154;
    pub const NUMPAD_MULTIPLY: u32 = 155;
    pub const NUMPAD_SUBTRACT: u32 = 156;
    pub const NUMPAD_ADD: u32 = 157;
    pub const NUMPAD_DOT: u32 = 158;
    pub const BUTTON_1: u32 = 188;
    pub const DPAD_UP_LEFT: u32 = 268;
    pub const DPAD_DOWN_LEFT: u32 = 269;
    pub const DPAD_UP_RIGHT: u32 = 270;
    pub const DPAD_DOWN_RIGHT: u32 = 271;
}

/// The stream's modifier bits (`Limelight.h`, `MODIFIER_*`).
pub(crate) const MODIFIER_SHIFT: u8 = 0x01;
pub(crate) const MODIFIER_CTRL: u8 = 0x02;
pub(crate) const MODIFIER_ALT: u8 = 0x04;
pub(crate) const MODIFIER_META: u8 = 0x08;
/// `SS_KBE_FLAG_NON_NORMALIZED`: the key is not mapped to QWERTY.
pub(crate) const NON_NORMALIZED: u8 = 0x01;

/// GFE's prefix of every key code.
const KEY_PREFIX: u16 = 0x80;

/// The stream's key code for an Android key: its virtual-key code with
/// GFE's prefix. A key with no code of its own falls back to its Linux scan
/// code. None: the key has neither.
pub(crate) fn translate(key_code: u32, scan_code: i32) -> Option<i16> {
    let vk = virtual_key(key_code).or_else(|| scan_code_key(scan_code))?;
    Some(((KEY_PREFIX << 8) | u16::from(vk)) as i16)
}

/// `KeyboardTranslator.translate`'s table, Android key code to Windows
/// virtual-key code.
fn virtual_key(key_code: u32) -> Option<u8> {
    use code::*;
    let offset = |base: u32, vk: u8| Some(vk + (key_code - base) as u8);
    match key_code {
        KEY_0..=KEY_9 => offset(KEY_0, 0x30),
        A..=Z => offset(A, 0x41),
        NUMPAD_0..=NUMPAD_9 => offset(NUMPAD_0, 0x60),
        F1..=F12 => offset(F1, 0x70),
        ALT_LEFT => Some(0xa4),
        ALT_RIGHT => Some(0xa5),
        BACKSLASH => Some(0xdc),
        CAPS_LOCK => Some(0x14),
        CLEAR => Some(0x0c),
        COMMA => Some(0xbc),
        CTRL_LEFT => Some(0xa2),
        CTRL_RIGHT => Some(0xa3),
        DEL => Some(0x08),
        ENTER => Some(0x0d),
        PLUS | EQUALS => Some(0xbb),
        ESCAPE => Some(0x1b),
        FORWARD_DEL => Some(0x2e),
        INSERT => Some(0x2d),
        LEFT_BRACKET => Some(0xdb),
        META_LEFT => Some(0x5b),
        META_RIGHT => Some(0x5c),
        MENU => Some(0x5d),
        MINUS => Some(0xbd),
        MOVE_END => Some(0x23),
        MOVE_HOME => Some(0x24),
        NUM_LOCK => Some(0x90),
        PAGE_DOWN => Some(0x22),
        PAGE_UP => Some(0x21),
        PERIOD => Some(0xbe),
        RIGHT_BRACKET => Some(0xdd),
        SCROLL_LOCK => Some(0x91),
        SEMICOLON => Some(0xba),
        SHIFT_LEFT => Some(0xa0),
        SHIFT_RIGHT => Some(0xa1),
        SLASH => Some(0xbf),
        SPACE => Some(0x20),
        // Android defines this as SysRq/PrntScrn.
        SYSRQ => Some(0x9a),
        TAB => Some(0x09),
        DPAD_LEFT => Some(0x25),
        DPAD_RIGHT => Some(0x27),
        DPAD_UP => Some(0x26),
        DPAD_DOWN => Some(0x28),
        GRAVE => Some(0xc0),
        APOSTROPHE => Some(0xde),
        BREAK => Some(0x13),
        NUMPAD_DIVIDE => Some(0x6f),
        NUMPAD_MULTIPLY => Some(0x6a),
        NUMPAD_SUBTRACT => Some(0x6d),
        NUMPAD_ADD => Some(0x6b),
        NUMPAD_DOT => Some(0x6e),
        _ => None,
    }
}

/// `KeyMapper.getWindowsKeyCode`: a Linux key's scan code
/// (`input-event-codes.h`) to its Windows virtual-key code.
fn scan_code_key(scan_code: i32) -> Option<u8> {
    let vk = match scan_code {
        1 => 0x1b,                                   // ESC
        2..=10 => 0x31 + (scan_code - 2) as u8,      // 1 to 9
        11 => 0x30,                                  // 0
        12 => 0xbd,                                  // MINUS
        13 => 0xbb,                                  // EQUAL
        14 => 0x08,                                  // BACKSPACE
        15 => 0x09,                                  // TAB
        16 => 0x51,                                  // Q
        17 => 0x57,                                  // W
        18 => 0x45,                                  // E
        19 => 0x52,                                  // R
        20 => 0x54,                                  // T
        21 => 0x59,                                  // Y
        22 => 0x55,                                  // U
        23 => 0x49,                                  // I
        24 => 0x4f,                                  // O
        25 => 0x50,                                  // P
        26 => 0xdb,                                  // LEFTBRACE
        27 => 0xdd,                                  // RIGHTBRACE
        28 => 0x0d,                                  // ENTER
        29 => 0xa2,                                  // LEFTCTRL
        30 => 0x41,                                  // A
        31 => 0x53,                                  // S
        32 => 0x44,                                  // D
        33 => 0x46,                                  // F
        34 => 0x47,                                  // G
        35 => 0x48,                                  // H
        36 => 0x4a,                                  // J
        37 => 0x4b,                                  // K
        38 => 0x4c,                                  // L
        39 => 0xba,                                  // SEMICOLON
        40 => 0xde,                                  // APOSTROPHE
        41 => 0xc0,                                  // GRAVE
        42 => 0xa0,                                  // LEFTSHIFT
        43 => 0xdc,                                  // BACKSLASH
        44 => 0x5a,                                  // Z
        45 => 0x58,                                  // X
        46 => 0x43,                                  // C
        47 => 0x56,                                  // V
        48 => 0x42,                                  // B
        49 => 0x4e,                                  // N
        50 => 0x4d,                                  // M
        51 => 0xbc,                                  // COMMA
        52 => 0xbe,                                  // DOT
        53 => 0xbf,                                  // SLASH
        54 => 0xa1,                                  // RIGHTSHIFT
        55 => 0x6a,                                  // KPASTERISK
        56 => 0xa4,                                  // LEFTALT
        57 => 0x20,                                  // SPACE
        58 => 0x14,                                  // CAPSLOCK
        59..=68 => 0x70 + (scan_code - 59) as u8,    // F1 to F10
        69 => 0x90,                                  // NUMLOCK
        70 => 0x91,                                  // SCROLLLOCK
        71 => 0x67,                                  // KP7
        72 => 0x68,                                  // KP8
        73 => 0x69,                                  // KP9
        74 => 0x6d,                                  // KPMINUS
        75 => 0x64,                                  // KP4
        76 => 0x65,                                  // KP5
        77 => 0x66,                                  // KP6
        78 => 0x6b,                                  // KPPLUS
        79 => 0x61,                                  // KP1
        80 => 0x62,                                  // KP2
        81 => 0x63,                                  // KP3
        82 => 0x60,                                  // KP0
        83 => 0x6e,                                  // KPDOT
        86 => 0xe2,                                  // 102ND
        87 => 0x7a,                                  // F11
        88 => 0x7b,                                  // F12
        97 => 0xa3,                                  // RIGHTCTRL
        99 => 0x2a,                                  // SYSRQ, as VK_PRINT
        100 => 0xa5,                                 // RIGHTALT
        102 => 0x24,                                 // HOME
        103 => 0x26,                                 // UP
        104 => 0x21,                                 // PAGEUP
        105 => 0x25,                                 // LEFT
        106 => 0x27,                                 // RIGHT
        107 => 0x23,                                 // END
        108 => 0x28,                                 // DOWN
        109 => 0x22,                                 // PAGEDOWN
        110 => 0x2d,                                 // INSERT
        111 => 0x2e,                                 // DELETE
        119 => 0x13,                                 // PAUSE
        125 => 0x5b,                                 // LEFTMETA
        126 => 0x5c,                                 // RIGHTMETA
        127 => 0xe5,                                 // COMPOSE
        183..=194 => 0x7c + (scan_code - 183) as u8, // F13 to F24
        _ => return None,
    };
    Some(vk)
}

/// The modifier bits that Android's meta state holds (`KeyEvent.META_*`).
pub(crate) fn meta_modifiers(meta_state: u32) -> u8 {
    let mut modifiers = 0;
    for (meta, modifier) in [
        (0x1, MODIFIER_SHIFT),
        (0x1000, MODIFIER_CTRL),
        (0x2, MODIFIER_ALT),
        (0x10000, MODIFIER_META),
    ] {
        if meta_state & meta != 0 {
            modifiers |= modifier;
        }
    }
    modifiers
}

/// What a key means to the keyboard's own state, before it is sent.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Special {
    /// Not a combination: send the key.
    None,
    /// Eaten by a combination: send nothing.
    Eaten,
    /// Ctrl+Alt+Shift+Q came up: leave the stream.
    Quit,
}

/// The keyboard's modifiers and its Ctrl+Alt+Shift combinations, as
/// `Game.java` keeps them. The modifiers come from the keys themselves,
/// because some keyboards set no meta state for Shift.
#[derive(Debug, Default)]
pub(crate) struct Keyboard {
    pub(crate) modifiers: u8,
    special: u32,
}

impl Keyboard {
    /// No modifier is down and no combination waits (`resetStreamInputState`).
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// `handleSpecialKeys`. Of Artemis's three combinations only Q (quit)
    /// acts; Z (input grab) and C (cursor) need the pointer capture that
    /// the viewer does not take, and are eaten as in Artemis.
    pub(crate) fn special(&mut self, key_code: u32, down: bool) -> Special {
        use code::*;
        let modifier = match key_code {
            CTRL_LEFT | CTRL_RIGHT => MODIFIER_CTRL,
            SHIFT_LEFT | SHIFT_RIGHT => MODIFIER_SHIFT,
            ALT_LEFT | ALT_RIGHT => MODIFIER_ALT,
            META_LEFT | META_RIGHT => MODIFIER_META,
            _ => 0,
        };
        if down {
            self.modifiers |= modifier;
        } else {
            self.modifiers &= !modifier;
        }
        if self.special != UNKNOWN {
            if self.special == key_code {
                // The host never saw this key go down.
                return Special::Eaten;
            }
            if self.modifiers != 0 {
                // Until every modifier is up, key downs are eaten.
                return if down { Special::Eaten } else { Special::None };
            }
            let special = std::mem::replace(&mut self.special, UNKNOWN);
            return if special == Q {
                Special::Quit
            } else {
                Special::None
            };
        }
        let all = MODIFIER_CTRL | MODIFIER_ALT | MODIFIER_SHIFT;
        if self.modifiers & all == all && down && modifier == 0 && matches!(key_code, Z | Q | C) {
            self.special = key_code;
            return Special::Eaten;
        }
        Special::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vk(key_code: u32) -> Option<u8> {
        translate(key_code, 0).map(|code| code as u16 as u8)
    }

    #[test]
    fn letters_digits_and_function_keys_are_their_windows_codes() {
        assert_eq!(translate(code::A, 30), Some(0x8041u16 as i16));
        assert_eq!(vk(code::Z), Some(b'Z'));
        assert_eq!(vk(code::KEY_0), Some(b'0'));
        assert_eq!(vk(code::KEY_9), Some(b'9'));
        assert_eq!(vk(code::F1), Some(0x70));
        assert_eq!(vk(code::F12), Some(0x7b));
        assert_eq!(vk(code::NUMPAD_0 + 5), Some(0x65));
    }

    #[test]
    fn the_named_keys_follow_artemis() {
        for (key, expected) in [
            (code::ENTER, 0x0d),
            (code::DEL, 0x08),
            (code::FORWARD_DEL, 0x2e),
            (code::ESCAPE, 0x1b),
            (code::SPACE, 0x20),
            (code::TAB, 0x09),
            (code::SHIFT_LEFT, 0xa0),
            (code::CTRL_RIGHT, 0xa3),
            (code::META_LEFT, 0x5b),
            (code::DPAD_LEFT, 0x25),
            (code::DPAD_UP, 0x26),
            (code::DPAD_RIGHT, 0x27),
            (code::DPAD_DOWN, 0x28),
            (code::GRAVE, 0xc0),
            (code::SYSRQ, 0x9a),
            (code::PLUS, 0xbb),
            (code::EQUALS, 0xbb),
            (code::MENU, 0x5d),
        ] {
            assert_eq!(vk(key), Some(expected), "key code {key}");
        }
    }

    #[test]
    fn a_key_with_no_code_falls_back_to_its_scan_code() {
        // KEYCODE_UNKNOWN with KEY_102ND, then F13.
        assert_eq!(translate(code::UNKNOWN, 86), Some(0x80e2u16 as i16));
        assert_eq!(translate(code::UNKNOWN, 183), Some(0x807cu16 as i16));
        // The key code wins over the scan code.
        assert_eq!(translate(code::A, 44), Some(0x8041u16 as i16));
    }

    #[test]
    fn a_key_with_neither_is_not_sent() {
        assert_eq!(translate(code::BACK, 158), None);
        assert_eq!(translate(code::BUTTON_A, 304), None);
        assert_eq!(translate(code::UNKNOWN, -1), None);
    }

    #[test]
    fn the_meta_state_gives_the_modifiers() {
        assert_eq!(meta_modifiers(0), 0);
        assert_eq!(meta_modifiers(0x1 | 0x40), MODIFIER_SHIFT);
        assert_eq!(
            meta_modifiers(0x1000 | 0x2 | 0x10000),
            MODIFIER_CTRL | MODIFIER_ALT | MODIFIER_META
        );
    }

    #[test]
    fn ctrl_alt_shift_q_quits_when_every_modifier_is_up() {
        let mut keyboard = Keyboard::default();
        for key in [code::CTRL_LEFT, code::ALT_LEFT, code::SHIFT_LEFT] {
            assert_eq!(keyboard.special(key, true), Special::None);
        }
        assert_eq!(keyboard.special(code::Q, true), Special::Eaten);
        // The host never saw Q go down: its up is eaten too.
        assert_eq!(keyboard.special(code::Q, false), Special::Eaten);
        assert_eq!(keyboard.special(code::CTRL_LEFT, false), Special::None);
        assert_eq!(keyboard.special(code::ALT_LEFT, false), Special::None);
        assert_eq!(keyboard.special(code::SHIFT_LEFT, false), Special::Quit);
        assert_eq!(keyboard.modifiers, 0);
    }

    #[test]
    fn other_keys_with_the_modifiers_go_to_the_host() {
        let mut keyboard = Keyboard::default();
        for key in [code::CTRL_LEFT, code::ALT_LEFT, code::SHIFT_LEFT] {
            keyboard.special(key, true);
        }
        assert_eq!(keyboard.special(code::A, true), Special::None);
        assert_eq!(
            keyboard.modifiers,
            MODIFIER_CTRL | MODIFIER_ALT | MODIFIER_SHIFT
        );
    }

    #[test]
    fn the_grab_combination_is_eaten_and_does_nothing() {
        let mut keyboard = Keyboard::default();
        for key in [code::CTRL_LEFT, code::ALT_LEFT, code::SHIFT_LEFT] {
            keyboard.special(key, true);
        }
        assert_eq!(keyboard.special(code::Z, true), Special::Eaten);
        // A key down while the modifiers are held is eaten, its up is not.
        assert_eq!(keyboard.special(code::A, true), Special::Eaten);
        assert_eq!(keyboard.special(code::A, false), Special::None);
        keyboard.special(code::Z, false);
        keyboard.special(code::CTRL_LEFT, false);
        keyboard.special(code::ALT_LEFT, false);
        assert_eq!(keyboard.special(code::SHIFT_LEFT, false), Special::None);
    }
}
