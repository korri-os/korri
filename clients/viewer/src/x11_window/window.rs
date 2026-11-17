// SPDX-License-Identifier: GPL-3.0-or-later
//! The X11 VA-API platform's window and its keyboard and mouse: the Rust
//! port of `picture/video/x11_window.c` and `io/input/x11.c` (#391 step F,
//! slice F4).
//!
//! Ported from Moonlight Embedded's `src/video/x11.c` (the window of
//! `x11_setup`) and `src/input/x11.c` (Copyright (C) 2017 Iwan Timmer,
//! GPL-3.0-or-later). The Rust adaptation is Korri work dated 2026-11-17.
//!
//! What stays as in C: the window (its size, event mask, `WM_NAME`
//! "Moonlight", `WM_PROTOCOLS` with `WM_DELETE_WINDOW`, the blank cursor)
//! and the `_NET_WM_STATE` fullscreen request to the root; the keys (Linux
//! key code to Windows virtual key, shift, alt and control tracked by key
//! code, Ctrl+Alt+Shift+Q to quit, Ctrl+Alt+Shift with another key's
//! release to toggle the grab); the buttons, with a scroll on both the
//! press and the release of buttons 4 to 7, and the C's one `button` of a
//! whole read (below); the relative mouse, warped back to (640, 360) while
//! grabbed; `WM_DELETE_WINDOW` to quit.
//!
//! The C's `button` is set to 0 once for each call of its handler, not for
//! each event. A scroll (buttons 4 to 7), or a button past 9, that follows
//! another button in the same read sends that button's press or release
//! again. The port keeps this, so that the measure judges the language
//! alone.
//!
//! What differs: the window has an X11 connection of its own, through
//! x11rb, where the C used the display that VA-API also uses; its requests
//! go out at the end of the open, where the C's last ones wait for the
//! display's next flush; an X11 error is logged, where Xlib's default
//! handler ends the process; a lost connection ends the stream through its
//! teardown, where Xlib's ends the process.
use super::{ffi as xlib, keyboard::KEY_CODES};
use crate::{
    log, network as ffi,
    network_core::{Packet, active},
};
use std::{
    ffi::{CStr, c_int, c_ulong, c_void},
    os::fd::AsRawFd,
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{
            AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt as _,
            CreateGCAux, CreateWindowAux, EventMask, PropMode, Rectangle, WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

/// video.h's `DISPLAY_FULLSCREEN` flag.
const DISPLAY_FULLSCREEN: c_int = 1;
/// io/input/x11.c's `QUIT_KEY`: Q, with Ctrl+Alt+Shift.
const QUIT_KEY: u8 = 0x18;
const ACTION_MODIFIERS: i32 = (ffi::MODIFIER_SHIFT | ffi::MODIFIER_ALT | ffi::MODIFIER_CTRL) as i32;
/// Where the grabbed pointer goes back to after each move.
const CENTRE: (i16, i16) = (640, 360);

type Failure = Box<dyn std::error::Error>;

/// The window, its connection and its input's state.
struct Window {
    connection: RustConnection,
    window: u32,
    cursor: u32,
    wm_delete: u32,
    fd: c_int,
    last_x: i32,
    last_y: i32,
    modifiers: i32,
    grabbed: bool,
}

static WINDOW: Mutex<Option<Window>> = Mutex::new(None);
static FAILED: AtomicBool = AtomicBool::new(false);

fn window() -> MutexGuard<'static, Option<Window>> {
    WINDOW.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) fn reset() {
    FAILED.store(false, Ordering::Relaxed);
}

pub(super) fn failed() -> bool {
    FAILED.load(Ordering::Relaxed)
}

/// `x11_window_open`: the window's XID, or 0 when it did not open.
///
/// # Safety
/// `display` is a live Xlib `Display*`.
pub(super) unsafe fn open(
    display: *mut c_void,
    width: c_int,
    height: c_int,
    flags: c_int,
) -> c_ulong {
    // SAFETY: Xlib's name of the caller's display, a C string it owns.
    let name = unsafe { CStr::from_ptr(xlib::XDisplayString(display.cast())) }
        .to_string_lossy()
        .into_owned();
    match Window::open(&name, width, height, flags & DISPLAY_FULLSCREEN != 0) {
        Ok(new) => {
            let (xid, fd) = (new.window, new.fd);
            let old = window().replace(new);
            if let Some(old) = old {
                old.close();
            }
            // SAFETY: the registry of the descriptor loop that the stream's
            // thread polls; the handler is ours and lives for the process.
            unsafe {
                ffi::loop_add_fd(
                    fd,
                    Some(handle),
                    c_int::from(libc::POLLIN | libc::POLLERR | libc::POLLHUP),
                )
            };
            log::line(&format!(
                "picture: the Rust X11 window 0x{xid:x}, {width}x{height}, on its own connection to {name}"
            ));
            c_ulong::from(xid)
        }
        Err(error) => {
            log::line(&format!(
                "picture: the Rust X11 window did not open on {name}: {error}; the stream ends"
            ));
            FAILED.store(true, Ordering::Relaxed);
            0
        }
    }
}

/// The window and its connection go; the server frees what they held.
pub(super) fn close() {
    let old = window().take();
    if let Some(old) = old {
        old.close();
    }
}

/// io/input/x11.c's `x11_handler`, in the descriptor loop: `LOOP_RETURN`
/// to quit, -1 when the connection is lost.
unsafe extern "C" fn handle(_: c_int) -> c_int {
    crate::callback::guarded("input: X11", -1, || {
        window()
            .as_mut()
            .map_or(ffi::LOOP_OK as c_int, Window::handle)
    })
}

fn send(packet: Packet) {
    active().send(packet);
}

impl Window {
    fn open(name: &str, width: c_int, height: c_int, fullscreen: bool) -> Result<Self, Failure> {
        let (connection, screen) = RustConnection::connect(Some(name))?;
        let root = connection.setup().roots[screen].root;
        let window = connection.generate_id()?;
        connection.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            u16::try_from(width)?,
            u16::try_from(height)?,
            0,
            WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new().event_mask(
                EventMask::POINTER_MOTION
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::KEY_PRESS
                    | EventMask::KEY_RELEASE,
            ),
        )?;
        connection.map_window(window)?;
        connection.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"Moonlight",
        )?;
        if fullscreen {
            let state = connection
                .intern_atom(false, b"_NET_WM_STATE")?
                .reply()?
                .atom;
            let full = connection
                .intern_atom(false, b"_NET_WM_STATE_FULLSCREEN")?
                .reply()?
                .atom;
            connection.send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                ClientMessageEvent::new(32, window, state, [1, full, 0, 0, 0]),
            )?;
        }
        connection.flush()?;
        // io/input/x11.c's x11_input_init: the window manager's close, and
        // a blank cursor.
        let wm_delete = connection
            .intern_atom(false, b"WM_DELETE_WINDOW")?
            .reply()?
            .atom;
        let protocols = connection
            .intern_atom(false, b"WM_PROTOCOLS")?
            .reply()?
            .atom;
        connection.change_property32(
            PropMode::REPLACE,
            window,
            protocols,
            AtomEnum::ATOM,
            &[wm_delete],
        )?;
        let blank = connection.generate_id()?;
        connection.create_pixmap(1, blank, window, 1, 1)?;
        let clear = connection.generate_id()?;
        connection.create_gc(clear, blank, &CreateGCAux::new().foreground(0))?;
        connection.poly_fill_rectangle(
            blank,
            clear,
            &[Rectangle {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }],
        )?;
        connection.free_gc(clear)?;
        let cursor = connection.generate_id()?;
        connection.create_cursor(cursor, blank, blank, 0, 0, 0, 0, 0, 0, 0, 0)?;
        connection.free_pixmap(blank)?;
        connection
            .change_window_attributes(window, &ChangeWindowAttributesAux::new().cursor(cursor))?;
        connection.flush()?;
        let fd = connection.stream().as_raw_fd();
        Ok(Self {
            connection,
            window,
            cursor,
            wm_delete,
            fd,
            last_x: -1,
            last_y: -1,
            modifiers: 0,
            grabbed: true,
        })
    }

    fn close(self) {
        // SAFETY: the registry of the descriptor loop; the descriptor is
        // ours until the connection drops below.
        unsafe { ffi::loop_remove_fd(self.fd) };
        drop(self.connection);
    }

    /// Every event that has come, as `x11_handler` reads them while
    /// `XPending` says more are there.
    fn handle(&mut self) -> c_int {
        // x11_handler's `button`, set once for the whole read.
        let mut button = 0;
        loop {
            let event = match self.connection.poll_for_event() {
                Ok(Some(event)) => event,
                Ok(None) => return ffi::LOOP_OK as c_int,
                Err(error) => {
                    log::line(&format!("input: the X11 connection is lost: {error}"));
                    return -1;
                }
            };
            let quit = match event {
                Event::KeyPress(key) => self.key(key.detail, true),
                Event::KeyRelease(key) => self.key(key.detail, false),
                Event::ButtonPress(event) => self.button(event.detail, true, &mut button),
                Event::ButtonRelease(event) => self.button(event.detail, false, &mut button),
                Event::MotionNotify(motion) => self.motion(motion.event_x, motion.event_y),
                Event::ClientMessage(message) => message.data.as_data32()[0] == self.wm_delete,
                Event::Error(error) => {
                    log::line(&format!("input: X11 error {error:?}"));
                    false
                }
                _ => false,
            };
            if quit {
                return ffi::LOOP_RETURN as c_int;
            }
            // XPending sends what the event asked for (a warp, a cursor)
            // before it reads again.
            if let Err(error) = self.connection.flush() {
                log::line(&format!("input: the X11 connection is lost: {error}"));
                return -1;
            }
        }
    }

    /// One key; true to quit.
    fn key(&mut self, keycode: u8, press: bool) -> bool {
        let Some(&code) = usize::from(keycode)
            .checked_sub(8)
            .and_then(|at| KEY_CODES.get(at))
        else {
            return false;
        };
        if self.modifiers & ACTION_MODIFIERS == ACTION_MODIFIERS && !press {
            if keycode == QUIT_KEY {
                return true;
            }
            self.grabbed = !self.grabbed;
            let cursor = if self.grabbed { self.cursor } else { 0 };
            let _ = self.connection.change_window_attributes(
                self.window,
                &ChangeWindowAttributesAux::new().cursor(cursor),
            );
        }
        let modifier = match keycode {
            0x32 | 0x3e => ffi::MODIFIER_SHIFT,
            0x40 | 0x6c => ffi::MODIFIER_ALT,
            0x25 | 0x69 => ffi::MODIFIER_CTRL,
            _ => 0,
        } as i32;
        if press {
            self.modifiers |= modifier;
        } else {
            self.modifiers &= !modifier;
        }
        send(Packet::Keyboard {
            // C's `short code = 0x80 << 8 | keyCodes[...]`.
            code: (0x8000_u16 | code as u16) as i16,
            down: press,
            modifiers: self.modifiers as u8,
            flags: 0,
        });
        false
    }

    /// One button; never quits. `button` is the read's, as in the C.
    fn button(&mut self, detail: u8, press: bool, button: &mut u32) -> bool {
        // LiSendScrollEvent and LiSendHScrollEvent, as the C core makes them.
        let scroll = |clicks: i16| clicks * 120;
        match detail {
            1 => *button = ffi::BUTTON_LEFT,
            2 => *button = ffi::BUTTON_MIDDLE,
            3 => *button = ffi::BUTTON_RIGHT,
            4 | 5 => send(Packet::Scroll {
                amount: scroll(if detail == 4 { 1 } else { -1 }),
            }),
            6 | 7 => send(Packet::HScroll {
                amount: scroll(if detail == 6 { -1 } else { 1 }),
            }),
            8 => *button = ffi::BUTTON_X1,
            9 => *button = ffi::BUTTON_X2,
            _ => {}
        }
        if *button != 0 {
            send(Packet::MouseButton {
                pressed: press,
                button: *button as u8,
            });
        }
        false
    }

    /// A move of the pointer; never quits.
    fn motion(&mut self, x: i16, y: i16) -> bool {
        let (x, y) = (i32::from(x), i32::from(y));
        let (dx, dy) = (x - self.last_x, y - self.last_y);
        if dx != 0 || dy != 0 {
            if self.last_x >= 0 && self.last_y >= 0 {
                send(Packet::MouseMove {
                    dx: dx as i16,
                    dy: dy as i16,
                });
            }
            if self.grabbed {
                let _ = self.connection.warp_pointer(
                    x11rb::NONE,
                    self.window,
                    0,
                    0,
                    0,
                    0,
                    CENTRE.0,
                    CENTRE.1,
                );
            }
        }
        if self.grabbed {
            (self.last_x, self.last_y) = (i32::from(CENTRE.0), i32::from(CENTRE.1));
        } else {
            (self.last_x, self.last_y) = (x, y);
        }
        false
    }
}
