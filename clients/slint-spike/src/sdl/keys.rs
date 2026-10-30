//! Host-edge input translation: SDL key codes to the semantic keys the view
//! already understands. The view never sees a remote's key code.
//!
//! - direction: Up and Left move focus back, Down and Right move it forward.
//!   The view is one linear focus chain (Refresh, Close), so focus traversal
//!   is the honest meaning of a direction here. Slint moves focus on Tab and
//!   Backtab when no element accepts them.
//! - confirm: Return or keypad Enter (Moonlight TV `navkey_sdl.c` maps
//!   `SDLK_RETURN` to confirm), plus Space for a desktop keyboard.
//! - back: Escape (Moonlight: `SDLK_ESCAPE` is cancel) or `SDL_SCANCODE_AC_BACK`,
//!   which upstream SDL 2.0.4 gives for evdev `KEY_BACK` (158) through
//!   `xfree86_scancode_table2`. Whether LG's 2.0.4 build delivers the remote's
//!   Back key this way is not verified; the adapter logs every key code so the
//!   TV run can show it.
//!
//! Home, Exit and the coloured keys are not mapped and are not grabbed: LG
//! keeps them.

use slint::platform::Key;

// SDL_keycode.h (2.0.4): SDLK_x = SDL_SCANCODE_x | (1 << 30) for non-characters.
const SCANCODE_MASK: i32 = 1 << 30;
const SDLK_RETURN: i32 = 13;
const SDLK_ESCAPE: i32 = 27;
const SDLK_TAB: i32 = 9;
const SDLK_SPACE: i32 = 32;
const SDLK_RIGHT: i32 = 79 | SCANCODE_MASK;
const SDLK_LEFT: i32 = 80 | SCANCODE_MASK;
const SDLK_DOWN: i32 = 81 | SCANCODE_MASK;
const SDLK_UP: i32 = 82 | SCANCODE_MASK;
const SDLK_KP_ENTER: i32 = 88 | SCANCODE_MASK;
const SDLK_AC_BACK: i32 = 270 | SCANCODE_MASK;
// SDL_scancode.h (2.0.4).
const SDL_SCANCODE_AC_BACK: i32 = 270;
/// webOS remote Back in webosbrew SDL-webOS (`SDL_SCANCODE_WEBOS_BACK`).
/// Not yet seen from LG's 2.0.4 build on the B7; the probe records it.
const SDL_SCANCODE_WEBOS_BACK: i32 = 482;
// SDL_keycode.h KMOD_LSHIFT | KMOD_RSHIFT.
const KMOD_SHIFT: u16 = 0x0001 | 0x0002;

/// What one SDL key means at the host edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Deliver this Slint key text to the view.
    Key(char),
    /// Leave the inspector, through the same path as Close.
    Back,
    /// Not a key this host translates. Logged, then dropped.
    Unmapped,
}

pub fn translate(keycode: i32, scancode: i32, modifiers: u16) -> Action {
    let shift = modifiers & KMOD_SHIFT != 0;
    match keycode {
        SDLK_UP | SDLK_LEFT => Action::Key(Key::Backtab.into()),
        SDLK_DOWN | SDLK_RIGHT => Action::Key(Key::Tab.into()),
        SDLK_TAB if shift => Action::Key(Key::Backtab.into()),
        SDLK_TAB => Action::Key(Key::Tab.into()),
        SDLK_RETURN | SDLK_KP_ENTER => Action::Key(Key::Return.into()),
        SDLK_SPACE => Action::Key(' '),
        SDLK_ESCAPE | SDLK_AC_BACK => Action::Back,
        _ if scancode == SDL_SCANCODE_AC_BACK || scancode == SDL_SCANCODE_WEBOS_BACK => {
            Action::Back
        }
        _ => Action::Unmapped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key) -> Action {
        Action::Key(key.into())
    }

    #[test]
    fn directions_move_focus_along_the_chain() {
        assert_eq!(translate(SDLK_DOWN, 81, 0), key(Key::Tab));
        assert_eq!(translate(SDLK_RIGHT, 79, 0), key(Key::Tab));
        assert_eq!(translate(SDLK_UP, 82, 0), key(Key::Backtab));
        assert_eq!(translate(SDLK_LEFT, 80, 0), key(Key::Backtab));
    }

    #[test]
    fn tab_keeps_its_desktop_meaning() {
        assert_eq!(translate(SDLK_TAB, 43, 0), key(Key::Tab));
        assert_eq!(translate(SDLK_TAB, 43, 0x0002), key(Key::Backtab));
    }

    #[test]
    fn confirm_and_back() {
        assert_eq!(translate(SDLK_RETURN, 40, 0), key(Key::Return));
        assert_eq!(translate(SDLK_KP_ENTER, 88, 0), key(Key::Return));
        assert_eq!(translate(SDLK_SPACE, 44, 0), Action::Key(' '));
        assert_eq!(translate(SDLK_ESCAPE, 41, 0), Action::Back);
        assert_eq!(translate(SDLK_AC_BACK, 270, 0), Action::Back);
        // A build that gives AC_BACK a different key code keeps the scancode.
        assert_eq!(translate(0, 270, 0), Action::Back);
        assert_eq!(translate(0, 482, 0), Action::Back);
    }

    #[test]
    fn other_keys_are_not_translated() {
        assert_eq!(translate('a' as i32, 4, 0), Action::Unmapped);
        // SDL_SCANCODE_AC_HOME: Home stays with LG.
        assert_eq!(translate(269 | SCANCODE_MASK, 269, 0), Action::Unmapped);
    }
}
