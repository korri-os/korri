//! App-owned Magic Remote cursor handover using the system SDL only.
//!
//! SDL-webOS's visibility request does not exist on the B7. Pair the proven
//! SDL_ShowCursor(0) hide with SDL_ShowCursor(1) only on LG key 484. Motion
//! includes gyro jitter and is diagnostic-only. LG SDL 2.0.4
//! skips its backend when its flag already matches the request; refresh a
//! repeated hide with SDL_SetCursor(NULL), not a visible toggle. No Wayland
//! roundtrip, timer, Luna call or power subscription is added.

use std::cell::Cell;
use std::ffi::{c_int, c_void};
use std::io::Write;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    /// SDL could not query or change its cursor visibility flag.
    Sdl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Input {
    Dpad { scancode: i32, repeat: bool },
    Shown484,
    Hidden485,
    Motion { xrel: i32, yrel: i32 },
}

fn key_input(scancode: i32, pressed: bool, repeat: bool) -> Option<Input> {
    if !pressed {
        return None;
    }
    match scancode {
        79..=82 => Some(Input::Dpad { scancode, repeat }),
        484 => Some(Input::Shown484),
        485 => Some(Input::Hidden485),
        _ => None,
    }
}

fn visibility_request(input: Input) -> Option<bool> {
    match input {
        // Always honor a physical D-pad press, even if our state says hidden.
        Input::Dpad { .. } => Some(false),
        Input::Shown484 => Some(true),
        _ => None,
    }
}

const TRACE_LIMIT: u16 = 200;

struct Transition {
    input: Input,
    owned_before: bool,
    owned_after: bool,
    before: c_int,
    request: Option<c_int>,
    returned: Option<c_int>,
    after: c_int,
    refreshed: bool,
    flag_ok: bool,
}

struct Trace {
    started: Instant,
    written: Cell<u16>,
}

impl Trace {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            written: Cell::new(0),
        }
    }

    fn record(&self, output: &mut impl Write, transition: &Transition) {
        let written = self.written.get();
        if written >= TRACE_LIMIT {
            return;
        }
        let sequence = written + 1;
        self.written.set(sequence);
        let Transition {
            input,
            owned_before,
            owned_after,
            before,
            request,
            returned,
            after,
            refreshed,
            flag_ok,
        } = transition;
        // One write per transition, no keys as text, addresses, or RPC data.
        // A logging failure never changes input handling.
        let _ = writeln!(output, "sdl: cursor trace n={sequence} ms={} event={input:?} owned={owned_before}->{owned_after} before={before} request={request:?} returned={returned:?} after={after} refresh={refreshed} flag_ok={flag_ok}", self.started.elapsed().as_millis());
    }
}

pub struct WebosCursor {
    show_cursor: unsafe extern "C" fn(c_int) -> c_int,
    set_cursor: unsafe extern "C" fn(*mut c_void),
    /// Our hide ownership, not the compositor's visibility state.
    hidden_by_keys: Cell<bool>,
    /// Diagnostic only: capture motion even if 484 restored the cursor first.
    trace_motion: Cell<bool>,
    last_visibility_key: Cell<Option<Input>>,
    trace: Trace,
}

impl WebosCursor {
    /// Query only: opening the adapter does not hide the pointer.
    pub fn open(
        show_cursor: unsafe extern "C" fn(c_int) -> c_int,
        set_cursor: unsafe extern "C" fn(*mut c_void),
    ) -> Result<Self, Unavailable> {
        // SAFETY: SDL video is initialized on the event-loop thread.
        if unsafe { show_cursor(-1) } < 0 {
            return Err(Unavailable::Sdl);
        }
        Ok(Self {
            show_cursor,
            set_cursor,
            hidden_by_keys: Cell::new(false),
            trace_motion: Cell::new(false),
            last_visibility_key: Cell::new(None),
            trace: Trace::new(),
        })
    }

    pub fn key(&self, scancode: i32, pressed: bool, repeat: bool) -> Result<(), Unavailable> {
        let Some(input) = key_input(scancode, pressed, repeat) else {
            return Ok(());
        };
        self.update(input)
    }

    /// Record only the first motion after a hide; never change visibility.
    pub fn motion(&self, xrel: i32, yrel: i32) -> Result<(), Unavailable> {
        self.update(Input::Motion { xrel, yrel })
    }

    fn update(&self, input: Input) -> Result<(), Unavailable> {
        let owned_before = self.hidden_by_keys.get();
        let request = visibility_request(input);
        let notification_changed = match input {
            Input::Dpad { .. } => {
                self.trace_motion.set(true);
                self.last_visibility_key.set(None);
                false
            }
            Input::Shown484 | Input::Hidden485 => {
                self.last_visibility_key.replace(Some(input)) != Some(input)
            }
            Input::Motion { .. } => false,
        };
        let first_motion = matches!(input, Input::Motion { .. })
            && self.trace_motion.replace(false)
            && self.trace.written.get() < TRACE_LIMIT;
        if request.is_none() && !notification_changed && !first_motion {
            return Ok(());
        }
        // SAFETY: these SDL 2.0.4 calls remain on the live video thread.
        // ShowCursor returns the PREVIOUS flag, not the requested/current one.
        let before = unsafe { (self.show_cursor)(-1) };
        let returned = request
            .filter(|_| before >= 0)
            .map(|visible| unsafe { (self.show_cursor)(c_int::from(visible)) });
        let refreshed = request == Some(false)
            && before == 0
            && !matches!(input, Input::Dpad { repeat: true, .. })
            && returned.is_some_and(|value| value >= 0);
        if refreshed {
            // SAFETY: NULL reapplies the current/default cursor. In LG's
            // SDL_SetCursor at 0x34608, flag 0 calls the backend with NULL
            // even when SDL_ShowCursor(0) at 0x34db8 skipped it. Never toggle
            // visible to force a hide, which could flash the cursor.
            unsafe { (self.set_cursor)(std::ptr::null_mut()) };
        }
        let after = unsafe { (self.show_cursor)(-1) };
        let ok = before >= 0
            && after >= 0
            && returned.is_none_or(|value| value >= 0)
            && request.is_none_or(|visible| after == c_int::from(visible));
        if ok {
            if let Some(visible) = request {
                self.hidden_by_keys.set(!visible);
            }
        }
        let owned_after = self.hidden_by_keys.get();
        if matches!(
            input,
            Input::Dpad { repeat: false, .. } | Input::Motion { .. }
        ) || notification_changed
            || before != after
            || owned_before != owned_after
        {
            self.trace.record(
                &mut std::io::stderr().lock(),
                &Transition {
                    input,
                    owned_before,
                    owned_after,
                    before,
                    request: request.map(c_int::from),
                    returned,
                    after,
                    refreshed,
                    flag_ok: ok,
                },
            );
        }
        if ok {
            Ok(())
        } else {
            Err(Unavailable::Sdl)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These adapters count and delegate to real workstation SDL functions.
    // They never replace SDL's return values or cursor behaviour.
    std::thread_local! {
        static NATIVE_SHOW_CURSOR: Cell<Option<unsafe extern "C" fn(c_int) -> c_int>> = const { Cell::new(None) };
        static NATIVE_SET_CURSOR: Cell<Option<unsafe extern "C" fn(*mut c_void)>> = const { Cell::new(None) };
        static CURSOR_API_CALLS: Cell<usize> = const { Cell::new(0) };
    }

    unsafe extern "C" fn counted_show_cursor(toggle: c_int) -> c_int {
        CURSOR_API_CALLS.with(|count| count.set(count.get() + 1));
        let show = NATIVE_SHOW_CURSOR.with(Cell::get).unwrap();
        // SAFETY: configured from live SDL before this adapter is installed.
        unsafe { show(toggle) }
    }

    unsafe extern "C" fn counted_set_cursor(cursor: *mut c_void) {
        CURSOR_API_CALLS.with(|count| count.set(count.get() + 1));
        let set = NATIVE_SET_CURSOR.with(Cell::get).unwrap();
        // SAFETY: delegates the unchanged argument to the live SDL function.
        unsafe { set(cursor) };
    }

    #[test]
    fn only_pressed_directions_request_a_hide() {
        for scancode in 79..=82 {
            assert_eq!(
                key_input(scancode, true, false),
                Some(Input::Dpad {
                    scancode,
                    repeat: false
                })
            );
            assert_eq!(key_input(scancode, false, false), None);
            assert_eq!(
                key_input(scancode, true, true),
                Some(Input::Dpad {
                    scancode,
                    repeat: true
                })
            );
        }
        for scancode in [40, 59, 270, 482, 486] {
            assert_eq!(key_input(scancode, true, false), None);
        }
        assert_eq!(key_input(484, true, false), Some(Input::Shown484));
        assert_eq!(key_input(485, true, false), Some(Input::Hidden485));
        assert_eq!(key_input(484, false, false), None);
        assert_eq!(key_input(485, false, false), None);
    }

    #[test]
    fn dpad_hide_is_not_skipped_after_a_previous_hide() {
        for repeat in [false, false, true] {
            assert_eq!(
                visibility_request(Input::Dpad {
                    scancode: 79,
                    repeat
                }),
                Some(false)
            );
        }
    }

    #[test]
    fn motion_never_restores_a_dpad_hide() {
        for (xrel, yrel) in [(0, 0), (2, 0), (7, 0), (-2, 0), (1914, 822)] {
            assert_eq!(visibility_request(Input::Motion { xrel, yrel }), None);
        }
        assert_eq!(visibility_request(Input::Shown484), Some(true));
        assert_eq!(visibility_request(Input::Hidden485), None);
    }

    #[test]
    fn transition_trace_is_capped_at_two_hundred_lines() {
        let trace = Trace::new();
        let mut output = Vec::new();
        let record = Transition {
            input: Input::Dpad {
                scancode: 79,
                repeat: false,
            },
            owned_before: true,
            owned_after: true,
            before: 0,
            request: Some(0),
            returned: Some(0),
            after: 0,
            refreshed: true,
            flag_ok: true,
        };
        for _ in 0..TRACE_LIMIT + 20 {
            trace.record(&mut output, &record);
        }
        let output = String::from_utf8(output).unwrap();
        let lines: Vec<_> = output.lines().collect();
        assert_eq!(lines.len(), usize::from(TRACE_LIMIT));
        assert!(lines[0].contains("n=1 "));
        assert!(lines.last().unwrap().contains("n=200 "));
        assert!(lines.iter().all(|line| line
            .contains("event=Dpad { scancode: 79, repeat: false }")
            && line.contains("before=0 request=Some(0) returned=Some(0) after=0 refresh=true")));
    }

    /// Runs against real workstation SDL with SDL_VIDEODRIVER=dummy. Never
    /// load a copied TV library into this test process.
    #[test]
    fn system_sdl_ignores_jitter_and_restores_only_on_lg_484() {
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
        NATIVE_SHOW_CURSOR.with(|function| function.set(Some(platform.sdl().SDL_ShowCursor)));
        NATIVE_SET_CURSOR.with(|function| function.set(Some(platform.sdl().SDL_SetCursor)));
        CURSOR_API_CALLS.with(|count| count.set(0));
        platform.webos_cursor =
            Some(WebosCursor::open(counted_show_cursor, counted_set_cursor).unwrap());
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
        motion.0[28..32].copy_from_slice(&7_i32.to_ne_bytes());
        let before_first = CURSOR_API_CALLS.with(Cell::get);
        platform.handle(&motion).unwrap();
        assert_eq!(unsafe { show(-1) }, 0);
        assert_eq!(platform.mode.get(), crate::keys::Mode::Keys);
        // First motion has two diagnostic flag queries, but no setter call.
        assert_eq!(CURSOR_API_CALLS.with(Cell::get) - before_first, 2);
        let calls = CURSOR_API_CALLS.with(Cell::get);
        let cursor = platform.webos_cursor.as_ref().unwrap();
        let trace_lines = cursor.trace.written.get();
        for xrel in [2_i32, 7, -2, 0, 1914].into_iter().cycle().take(10_000) {
            motion.0[28..32].copy_from_slice(&xrel.to_ne_bytes());
            platform.handle(&motion).unwrap();
        }
        assert_eq!(CURSOR_API_CALLS.with(Cell::get), calls);
        assert_eq!(cursor.trace.written.get(), trace_lines);
        assert_eq!(unsafe { show(-1) }, 0);
        assert_eq!(platform.mode.get(), crate::keys::Mode::Keys);
        for kind in [ffi::SDL_MOUSEBUTTONDOWN, ffi::SDL_MOUSEBUTTONUP] {
            click.set_kind(kind);
            platform.handle(&click).unwrap();
        }
        key(484, true);
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
                (HostKey::Confirm, true),
                (HostKey::Confirm, false),
                (HostKey::Up, true)
            ]
        );
        // SDL/backend state can change without updating our hide ownership.
        // A real SDL state change must not make the next D-pad press a no-op.
        key(79, true);
        key(79, false);
        assert_eq!(unsafe { show(-1) }, 0);
        unsafe { show(1) };
        assert_eq!(unsafe { show(-1) }, 1);
        key(79, true);
        key(79, false);
        assert_eq!(unsafe { show(-1) }, 0);
        for cycle in 0..20 {
            key(79, true);
            key(79, false);
            assert_eq!(unsafe { show(-1) }, 0);
            // Even a second physical press while SDL's flag is already off
            // must reapply the backend cursor, not skip on our hide state.
            key(79, true);
            key(79, false);
            assert_eq!(unsafe { show(-1) }, 0);
            motion.0[28..32]
                .copy_from_slice(&(if cycle % 2 == 0 { 7_i32 } else { -2_i32 }).to_ne_bytes());
            platform.handle(&motion).unwrap();
            assert_eq!(unsafe { show(-1) }, 0);
            assert_eq!(platform.mode.get(), crate::keys::Mode::Keys);
            key(484, true);
            assert_eq!(unsafe { show(-1) }, 1);
            assert_eq!(platform.mode.get(), crate::keys::Mode::Pointer);
            platform.handle(&motion).unwrap();
            assert_eq!(unsafe { show(-1) }, 1);
        }
        platform.close();
    }
}
