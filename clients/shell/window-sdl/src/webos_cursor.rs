//! App-owned Magic Remote cursor handover using the system SDL only.
//!
//! SDL-webOS's visibility request does not exist on the B7. SDL_ShowCursor(0)
//! does hide this app's cursor there, but SDL keeps it hidden even after a
//! shake. Pair that proven hide with SDL_ShowCursor(1) on the next motion or
//! LG's cursor-shown key (484). A compositor-hidden key (485) must not latch
//! SDL's own visibility off. No Wayland roundtrip, timer, Luna call or power
//! subscription is needed. See the 2026-11-06 cursor source report.

use std::cell::Cell;
use std::ffi::c_int;

/// Why SDL's cursor control is unavailable, as a fixed probe label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    /// SDL could not query or change its cursor visibility.
    Sdl,
}

#[derive(Clone, Copy)]
enum Input {
    Key { scancode: i32, pressed: bool },
    Motion,
}

/// Only a D-pad press owns a hide. Compositor idle/hide notifications do not:
/// keeping SDL enabled lets LG show its pointer again by itself.
fn visibility_request(hidden_by_keys: bool, input: Input) -> Option<bool> {
    match input {
        Input::Key {
            scancode: 79..=82,
            pressed: true,
        } if !hidden_by_keys => Some(false),
        Input::Key {
            scancode: 484,
            pressed: true,
        } => Some(true),
        Input::Motion if hidden_by_keys => Some(true),
        _ => None,
    }
}

pub struct WebosCursor {
    show_cursor: unsafe extern "C" fn(c_int) -> c_int,
    /// SDL visibility that this adapter changed, not LG's compositor state.
    hidden_by_keys: Cell<bool>,
}

impl WebosCursor {
    /// Query only: opening the adapter does not hide the pointer.
    pub fn open(show_cursor: unsafe extern "C" fn(c_int) -> c_int) -> Result<Self, Unavailable> {
        // SAFETY: called with SDL's function while video is initialized, on
        // the SDL event-loop thread. -1 reads state without changing it.
        if unsafe { show_cursor(-1) } < 0 {
            return Err(Unavailable::Sdl);
        }
        Ok(Self {
            show_cursor,
            hidden_by_keys: Cell::new(false),
        })
    }

    pub fn key(&self, scancode: i32, pressed: bool) -> Result<(), Unavailable> {
        self.update(Input::Key { scancode, pressed }).map(|_| ())
    }

    /// True when this motion successfully restored our D-pad hide.
    pub fn motion(&self) -> Result<bool, Unavailable> {
        self.update(Input::Motion)
    }

    fn update(&self, input: Input) -> Result<bool, Unavailable> {
        let Some(visible) = visibility_request(self.hidden_by_keys.get(), input) else {
            return Ok(false);
        };
        // SAFETY: SDL video remains live until the event loop ends; no
        // pointer or window ownership is transferred by this SDL 2.0.4 API.
        if unsafe { (self.show_cursor)(c_int::from(visible)) } < 0 {
            return Err(Unavailable::Sdl);
        }
        self.hidden_by_keys.set(!visible);
        Ok(visible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pressed_directions_request_a_hide() {
        for scancode in 79..=82 {
            assert_eq!(
                visibility_request(
                    false,
                    Input::Key {
                        scancode,
                        pressed: true
                    }
                ),
                Some(false)
            );
            assert_eq!(
                visibility_request(
                    false,
                    Input::Key {
                        scancode,
                        pressed: false
                    }
                ),
                None
            );
        }
        assert_eq!(
            visibility_request(
                true,
                Input::Key {
                    scancode: 484,
                    pressed: false
                }
            ),
            None
        );
        for scancode in [40, 59, 270, 482, 485, 486] {
            assert_eq!(
                visibility_request(
                    false,
                    Input::Key {
                        scancode,
                        pressed: true
                    }
                ),
                None
            );
        }
    }

    #[test]
    fn motion_restores_only_an_app_owned_hide() {
        assert_eq!(visibility_request(false, Input::Motion), None);
        assert_eq!(visibility_request(true, Input::Motion), Some(true));
        assert_eq!(
            visibility_request(
                true,
                Input::Key {
                    scancode: 79,
                    pressed: true
                }
            ),
            None
        );
        for hidden in [false, true] {
            assert_eq!(
                visibility_request(
                    hidden,
                    Input::Key {
                        scancode: 484,
                        pressed: true
                    }
                ),
                Some(true)
            );
            assert_eq!(
                visibility_request(
                    hidden,
                    Input::Key {
                        scancode: 485,
                        pressed: true
                    }
                ),
                None
            );
        }
    }

    /// Runs against real workstation SDL with SDL_VIDEODRIVER=dummy. Never
    /// load a copied TV library into this test process.
    #[test]
    fn system_sdl_hide_is_paired_with_motion_and_shown_key_restore() {
        use crate::{ffi, HostKey, Placement, SdlPlatform};
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut platform = SdlPlatform::open(
            "cursor check",
            Placement::Windowed {
                width: 64,
                height: 64,
            },
        )
        .unwrap();
        // The workstation can have DISPLAY set. Exercise the LG keyboard
        // layout and the real shell's host-key route regardless.
        platform.webos_abi = true;
        platform.webos_cursor = Some(WebosCursor::open(platform.sdl().SDL_ShowCursor).unwrap());
        let delivered = Rc::new(RefCell::new(Vec::new()));
        let keys = delivered.clone();
        platform.host_keys = Some(Box::new(move |key, pressed| {
            keys.borrow_mut().push((key, pressed))
        }));
        let platform = Box::new(platform);
        let address = std::ptr::from_ref(platform.as_ref());
        slint::platform::set_platform(platform).unwrap();
        // SAFETY: Slint owns the unchanged Box on this test thread until its
        // thread-local context is dropped, after all accesses below finish.
        let platform = unsafe { &*address };
        let show = platform.sdl().SDL_ShowCursor;
        // Opening the adapter must not hide a pointer before a D-pad press.
        assert_eq!(unsafe { show(-1) }, 1);
        let key = |scancode: i32, pressed: bool| {
            let mut event = ffi::Event::zeroed();
            event.set_kind(if pressed {
                ffi::SDL_KEYDOWN
            } else {
                ffi::SDL_KEYUP
            });
            event.0[20..24].copy_from_slice(&scancode.to_ne_bytes());
            event.0[24..28].copy_from_slice(&(scancode | (1 << 30)).to_ne_bytes());
            platform.key_event(&event, pressed);
        };
        key(79, true);
        assert_eq!(unsafe { show(-1) }, 0);
        key(79, false);
        key(485, true);
        assert_eq!(unsafe { show(-1) }, 0);
        // A locally hidden cursor need not change LG's OK-as-click policy.
        // In key mode the shell, not the view, must receive that confirm.
        let mut click = ffi::Event::zeroed();
        click.0[16] = ffi::SDL_BUTTON_LEFT;
        for kind in [ffi::SDL_MOUSEBUTTONDOWN, ffi::SDL_MOUSEBUTTONUP] {
            click.set_kind(kind);
            platform.handle(&click).unwrap();
        }
        assert_eq!(unsafe { show(-1) }, 0);
        let mut motion = ffi::Event::zeroed();
        motion.set_kind(ffi::SDL_MOUSEMOTION);
        platform.handle(&motion).unwrap();
        assert_eq!(unsafe { show(-1) }, 1);
        assert_eq!(platform.mode.get(), crate::keys::Mode::Pointer);
        key(82, true);
        assert_eq!(unsafe { show(-1) }, 0);
        key(484, true);
        assert_eq!(unsafe { show(-1) }, 1);
        key(485, true);
        assert_eq!(unsafe { show(-1) }, 1);
        assert_eq!(
            *delivered.borrow(),
            [
                (HostKey::Right, true),
                (HostKey::Right, false),
                (HostKey::Confirm, true),
                (HostKey::Confirm, false),
                (HostKey::Up, true)
            ]
        );
        platform.close();
    }
}
