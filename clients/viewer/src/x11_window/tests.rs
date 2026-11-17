// SPDX-License-Identifier: GPL-3.0-or-later
//! The Rust X11 window beside the carried C, each on a private Xvfb of its
//! own: the same window as another client sees it, the same request to the
//! window manager, and the same packets from the same events. The events
//! come from another client (`XSendEvent`), as a window manager's and the
//! measure's key presses reach the window's owner; the window's handler
//! runs in the descriptor loop, as on the X11 VA-API platform.
use super::{Kind, chosen, ffi as xlib, keyboard::KEY_CODES, select};
use crate::{
    network as ffi,
    network_core::{
        Packet,
        tests::{Recording, installed},
    },
};
use std::{
    ffi::{CString, c_int, c_ulong},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{LazyLock, Mutex, MutexGuard, PoisonError},
    thread::sleep,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{
            AtomEnum, ButtonPressEvent, ChangeWindowAttributesAux, ClientMessageEvent,
            ConnectionExt as _, EventMask, KeyButMask, KeyPressEvent, MapState, Motion,
            MotionNotifyEvent,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

/// One window at a time: the C keeps its input's state for the process, and
/// both add handlers to the one descriptor loop.
static TURN: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    TURN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A private Xvfb with a screen of 1920x1080, so that the grabbed pointer's
/// warp to (640, 360) lands inside a 1280x720 window.
struct Server {
    child: Child,
    name: String,
}

impl Server {
    fn start() -> Self {
        for number in 140..200 {
            let socket = format!("/tmp/.X11-unix/X{number}");
            let lock = format!("/tmp/.X{number}-lock");
            if Path::new(&socket).exists() || Path::new(&lock).exists() {
                continue;
            }
            let name = format!(":{number}");
            let child = Command::new("Xvfb")
                .args([
                    &name,
                    "-screen",
                    "0",
                    "1920x1080x24",
                    "-nolisten",
                    "tcp",
                    "-noreset",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("Xvfb on PATH (the package's check inputs, the viewer shell)");
            let deadline = Instant::now() + Duration::from_secs(10);
            while !Path::new(&socket).exists() {
                assert!(Instant::now() < deadline, "Xvfb {name} did not start");
                sleep(Duration::from_millis(20));
            }
            // The socket can come before the server answers.
            while RustConnection::connect(Some(&name)).is_err() {
                assert!(Instant::now() < deadline, "Xvfb {name} did not answer");
                sleep(Duration::from_millis(20));
            }
            return Self { child, name };
        }
        panic!("no free X display number");
    }

    /// Another client of the server, as a window manager or the measure is.
    fn client(&self) -> RustConnection {
        RustConnection::connect(Some(&self.name)).unwrap().0
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // SIGTERM: Xvfb removes its socket and lock file as it ends, so that
        // runs outside a build sandbox leave no display number taken.
        // SAFETY: a signal to our own child.
        unsafe { libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.child.try_wait().ok().flatten().is_none() {
            if Instant::now() > deadline {
                let _ = self.child.kill();
                break;
            }
            sleep(Duration::from_millis(10));
        }
        let _ = self.child.wait();
    }
}

/// The viewer's side: the Xlib display that x11.c opens, and the window
/// that `x11_window_open` gives on it.
struct Viewer {
    kind: Kind,
    display: *mut xlib::Display,
    window: u32,
}

impl Viewer {
    fn open(kind: Kind, server: &Server, size: (c_int, c_int), flags: c_int) -> Self {
        select(kind);
        let name = CString::new(server.name.as_str()).unwrap();
        // SAFETY: Xlib's own calls, then the call x11.c makes, with the live
        // display.
        unsafe {
            let display = xlib::XOpenDisplay(name.as_ptr());
            assert!(!display.is_null());
            let window = chosen::x11_window_open(display.cast(), size.0, size.1, flags);
            assert_ne!(window, 0, "{kind:?}: a window");
            // The C's last requests wait for the display's next flush, which
            // x11.c's first frame or the input handler makes.
            xlib::XSync(display, 0);
            Self {
                kind,
                display,
                window: window as u32,
            }
        }
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        match self.kind {
            Kind::Rust => super::close(),
            // SAFETY: the display's descriptor leaves the loop before the
            // display closes, and the window with it.
            Kind::Carried => unsafe {
                ffi::loop_remove_fd(xlib::XConnectionNumber(self.display));
            },
        }
        // SAFETY: ours, closed once.
        unsafe { xlib::XCloseDisplay(self.display) };
    }
}

/// What another client sees of a window.
#[derive(Debug, PartialEq)]
struct Seen {
    size: (u16, u16),
    map_state: MapState,
    event_masks: EventMask,
    name: Vec<u8>,
    protocols: Vec<String>,
}

fn atom_name(client: &RustConnection, atom: u32) -> String {
    String::from_utf8(client.get_atom_name(atom).unwrap().reply().unwrap().name).unwrap()
}

fn seen(client: &RustConnection, window: u32) -> Seen {
    let geometry = client.get_geometry(window).unwrap().reply().unwrap();
    let attributes = client
        .get_window_attributes(window)
        .unwrap()
        .reply()
        .unwrap();
    let name = client
        .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 64)
        .unwrap()
        .reply()
        .unwrap();
    let protocols_atom = client
        .intern_atom(false, b"WM_PROTOCOLS")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    let protocols = client
        .get_property(false, window, protocols_atom, AtomEnum::ATOM, 0, 64)
        .unwrap()
        .reply()
        .unwrap();
    Seen {
        size: (geometry.width, geometry.height),
        map_state: attributes.map_state,
        event_masks: attributes.all_event_masks,
        name: name.value,
        protocols: protocols
            .value32()
            .unwrap()
            .map(|atom| atom_name(client, atom))
            .collect(),
    }
}

#[test]
fn the_rust_window_is_the_one_the_carried_c_makes() {
    let _turn = exclusive();
    let seen_by = |kind| {
        let server = Server::start();
        let viewer = Viewer::open(kind, &server, (1280, 720), 0);
        let client = server.client();
        let seen = seen(&client, viewer.window);
        drop(viewer);
        seen
    };
    let carried = seen_by(Kind::Carried);
    assert_eq!(
        carried,
        Seen {
            size: (1280, 720),
            map_state: MapState::VIEWABLE,
            event_masks: EventMask::POINTER_MOTION
                | EventMask::BUTTON_PRESS
                | EventMask::BUTTON_RELEASE
                | EventMask::KEY_PRESS
                | EventMask::KEY_RELEASE,
            name: b"Moonlight".to_vec(),
            protocols: vec!["WM_DELETE_WINDOW".into()],
        }
    );
    assert_eq!(seen_by(Kind::Rust), carried);
}

/// What a window manager receives when the viewer opens a fullscreen
/// window: the window's id is `W` in each.
fn manager_receives(kind: Kind) -> Vec<String> {
    let server = Server::start();
    let manager = server.client();
    let root = manager.setup().roots[0].root;
    manager
        .change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new()
                .event_mask(EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY),
        )
        .unwrap()
        .check()
        .unwrap();
    let viewer = Viewer::open(kind, &server, (1920, 1080), 1);
    manager.sync().unwrap();
    let window = viewer.window;
    let mut received = Vec::new();
    while let Some(event) = manager.poll_for_event().unwrap() {
        received.push(match event {
            Event::CreateNotify(e) => {
                assert_eq!(e.window, window);
                format!("create W {}x{}", e.width, e.height)
            }
            Event::MapRequest(e) => {
                assert_eq!(e.window, window);
                "map request W".into()
            }
            Event::ClientMessage(e) => {
                assert_eq!(e.window, window);
                let [action, first, second, ..] = e.data.as_data32();
                format!(
                    "{} format {} [{action}, {}, {second}]",
                    atom_name(&manager, e.type_),
                    e.format,
                    atom_name(&manager, first)
                )
            }
            other => format!("{other:?}"),
        });
    }
    drop(viewer);
    received
}

#[test]
fn fullscreen_asks_the_window_manager_as_the_carried_c_does() {
    let _turn = exclusive();
    let carried = manager_receives(Kind::Carried);
    assert_eq!(
        carried,
        [
            "create W 1920x1080",
            "map request W",
            "_NET_WM_STATE format 32 [1, _NET_WM_STATE_FULLSCREEN, 0]"
        ]
    );
    assert_eq!(manager_receives(Kind::Rust), carried);
}

#[test]
fn the_rust_key_table_is_the_carried_cs() {
    let source = include_str!("../../io/input/keyboard.h");
    let table = &source[source.find('{').unwrap() + 1..source.find("};").unwrap()];
    let carried: Vec<i16> = table
        .lines()
        .filter_map(|line| line.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.starts_with("//"))
        .map(|value| match value.strip_prefix("0x") {
            Some(hex) => i16::from_str_radix(hex, 16).unwrap(),
            None => value.parse().unwrap(),
        })
        .collect();
    assert_eq!(carried.len(), 128);
    assert_eq!(KEY_CODES.to_vec(), carried);
}

/// An event that another client sends to the window, as `XSendEvent` does.
#[derive(Clone, Copy)]
enum Input {
    Press(u8),
    Release(u8),
    ButtonPress(u8),
    ButtonRelease(u8),
    Motion(i16, i16),
    Close,
}

fn send(client: &RustConnection, window: u32, input: Input) {
    let root = client.setup().roots[0].root;
    let key = |response_type, detail| KeyPressEvent {
        response_type,
        detail,
        sequence: 0,
        time: 0,
        root,
        event: window,
        child: x11rb::NONE,
        root_x: 0,
        root_y: 0,
        event_x: 0,
        event_y: 0,
        state: KeyButMask::default(),
        same_screen: true,
    };
    let button = |response_type, detail| ButtonPressEvent {
        response_type,
        detail,
        sequence: 0,
        time: 0,
        root,
        event: window,
        child: x11rb::NONE,
        root_x: 0,
        root_y: 0,
        event_x: 0,
        event_y: 0,
        state: KeyButMask::default(),
        same_screen: true,
    };
    use x11rb::protocol::xproto::{
        BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
        MOTION_NOTIFY_EVENT,
    };
    let sent = match input {
        Input::Press(code) => client.send_event(
            false,
            window,
            EventMask::KEY_PRESS,
            key(KEY_PRESS_EVENT, code),
        ),
        Input::Release(code) => client.send_event(
            false,
            window,
            EventMask::KEY_RELEASE,
            key(KEY_RELEASE_EVENT, code),
        ),
        Input::ButtonPress(number) => client.send_event(
            false,
            window,
            EventMask::BUTTON_PRESS,
            button(BUTTON_PRESS_EVENT, number),
        ),
        Input::ButtonRelease(number) => client.send_event(
            false,
            window,
            EventMask::BUTTON_RELEASE,
            button(BUTTON_RELEASE_EVENT, number),
        ),
        Input::Motion(x, y) => client.send_event(
            false,
            window,
            EventMask::POINTER_MOTION,
            MotionNotifyEvent {
                response_type: MOTION_NOTIFY_EVENT,
                detail: Motion::NORMAL,
                sequence: 0,
                time: 0,
                root,
                event: window,
                child: x11rb::NONE,
                root_x: x,
                root_y: y,
                event_x: x,
                event_y: y,
                state: KeyButMask::default(),
                same_screen: true,
            },
        ),
        Input::Close => {
            let atom = |name: &[u8]| {
                client
                    .intern_atom(false, name)
                    .unwrap()
                    .reply()
                    .unwrap()
                    .atom
            };
            // No mask: the event goes to the window's owner, as a window
            // manager's close does.
            client.send_event(
                false,
                window,
                EventMask::NO_EVENT,
                ClientMessageEvent::new(
                    32,
                    window,
                    atom(b"WM_PROTOCOLS"),
                    [atom(b"WM_DELETE_WINDOW"), 0, 0, 0, 0],
                ),
            )
        }
    };
    sent.unwrap();
}

static CORE: LazyLock<Recording> = LazyLock::new(|| Recording {
    packets: Mutex::new(Vec::new()),
    answer: 0,
    clock_us: 0,
});

/// The descriptor loop until it has been quiet for 60 ms: each turn's
/// result other than 0, and the packets that reached the core. A loop that
/// never goes quiet within 10 s fails the test instead of spinning.
fn pump() -> (Vec<c_int>, Vec<Packet>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut results = Vec::new();
    let mut quiet = 0;
    let mut seen = CORE.packets.lock().unwrap().len();
    while quiet < 3 {
        assert!(
            Instant::now() < deadline,
            "the descriptor loop did not go quiet in 10 s: {} results, {} packets",
            results.len(),
            seen
        );
        // SAFETY: the descriptor loop's own call, on this thread.
        let result = unsafe { ffi::picture_poll_descriptors(20) };
        let now = CORE.packets.lock().unwrap().len();
        if result != 0 {
            results.push(result);
        }
        if result == 0 && now == seen {
            quiet += 1;
        } else {
            quiet = 0;
        }
        seen = now;
    }
    (results, CORE.packets.lock().unwrap().drain(..).collect())
}

/// One script: each group of events, sent together, then the loop until it
/// is quiet. Every key and button that the C names, and the pointer
/// grabbed, ungrabbed and grabbed again.
fn script() -> Vec<Vec<Input>> {
    use Input::*;
    let (ctrl, alt, shift, q, a) = (0x25, 0x40, 0x32, 0x18, 38);
    vec![
        vec![Press(a), Release(a)],
        vec![Press(shift), Press(a), Release(a), Release(shift)],
        (1..=10)
            .flat_map(|number| [ButtonPress(number), ButtonRelease(number)])
            .collect(),
        // Below the table and past its end: no packet.
        vec![Press(7), Release(7), Press(200), Release(200)],
        vec![Motion(100, 100)],
        vec![Motion(700, 400)],
        // Ctrl+Alt+Shift and A: the grab goes.
        vec![Press(ctrl), Press(alt), Press(shift), Press(a), Release(a)],
        vec![Motion(800, 500)],
        vec![Motion(810, 505)],
        // The shift's release, with all three still down, toggles it back.
        vec![Release(shift), Release(alt), Release(ctrl)],
        vec![Motion(900, 600)],
        vec![Close],
        vec![Press(ctrl), Press(alt), Press(shift), Press(q), Release(q)],
        vec![Release(shift), Release(alt), Release(ctrl)],
    ]
}

/// One group's turns that did not return 0, its packets, and where the real
/// pointer is in the window afterwards.
type Played = (Vec<c_int>, Vec<Packet>, (i16, i16));

/// The script's results, group by group, for one kind.
fn played(kind: Kind) -> Vec<Played> {
    let server = Server::start();
    let viewer = Viewer::open(kind, &server, (1280, 720), 0);
    let client = server.client();
    let played = script()
        .into_iter()
        .map(|group| {
            for input in group {
                send(&client, viewer.window, input);
            }
            client.sync().unwrap();
            let (results, packets) = pump();
            let pointer = client
                .query_pointer(viewer.window)
                .unwrap()
                .reply()
                .unwrap();
            (results, packets, (pointer.win_x, pointer.win_y))
        })
        .collect();
    drop(viewer);
    played
}

#[test]
fn the_rust_input_sends_the_packets_that_the_carried_c_sends() {
    let _turn = exclusive();
    let _core = installed(&*CORE);
    CORE.packets.lock().unwrap().clear();
    let carried = played(Kind::Carried);
    let rust = played(Kind::Rust);
    let key = |vk: u16, down, modifiers| Packet::Keyboard {
        code: (0x8000 | vk) as i16,
        down,
        modifiers,
        flags: 0,
    };
    // Spot checks of the C's own answers, then the Rust's against them.
    assert!(carried[0].0.is_empty());
    assert_eq!(carried[0].1, [key(0x41, true, 0), key(0x41, false, 0)]);
    assert_eq!(carried[1].1[1], key(0x41, true, ffi::MODIFIER_SHIFT as u8));
    // Five buttons, a scroll on both edges of buttons 4 to 7, and the C's
    // stale button: after button 3, each edge of buttons 4 to 7 sends the
    // right button again, and after button 9 each edge of button 10 the X2.
    assert_eq!(carried[2].1.len(), 2 * 5 + 2 * 4 + 2 * 4 + 2);
    assert!(carried[3].1.is_empty());
    assert!(
        carried[4].1.is_empty(),
        "the first move only sets the start"
    );
    assert_eq!(carried[5].1, [Packet::MouseMove { dx: 60, dy: 40 }]);
    // Grabbed, the real pointer goes back to the window's (640, 360).
    assert_eq!(carried[5].2, (640, 360));
    assert_eq!(carried[7].1, [Packet::MouseMove { dx: 160, dy: 140 }]);
    assert_eq!(carried[8].1, [Packet::MouseMove { dx: 10, dy: 5 }]);
    assert_eq!(carried[10].1, [Packet::MouseMove { dx: 90, dy: 95 }]);
    assert_eq!(carried[11].0, [ffi::LOOP_RETURN as c_int]);
    assert!(carried[11].1.is_empty());
    assert_eq!(carried[12].0, [ffi::LOOP_RETURN as c_int]);
    for (group, (carried, rust)) in carried.iter().zip(&rust).enumerate() {
        assert_eq!(rust, carried, "group {group}");
    }
}

#[test]
fn a_closed_rust_window_is_gone_and_leaves_the_loop() {
    let _turn = exclusive();
    let server = Server::start();
    let viewer = Viewer::open(Kind::Rust, &server, (640, 360), 0);
    let window = viewer.window;
    let client = server.client();
    assert!(client.get_geometry(window).unwrap().reply().is_ok());
    drop(viewer);
    client.sync().unwrap();
    assert!(
        client.get_geometry(window).unwrap().reply().is_err(),
        "the window went with its connection"
    );
    // The window's descriptor left the loop. Were it still there, closed,
    // poll would mark it invalid and call its handler, which, with no
    // window open, fails the loop (-1).
    // SAFETY: the descriptor loop's own call, on this thread.
    assert_eq!(unsafe { ffi::picture_poll_descriptors(0) }, 0);
}

#[test]
fn a_panic_in_the_open_ends_the_stream() {
    let _turn = exclusive();
    let server = Server::start();
    select(Kind::Rust);
    super::window::PANIC_IN_OPEN.store(true, std::sync::atomic::Ordering::Relaxed);
    let name = CString::new(server.name.as_str()).unwrap();
    // SAFETY: Xlib's own calls; then the call x11.c makes.
    unsafe {
        let display = xlib::XOpenDisplay(name.as_ptr());
        assert!(!display.is_null());
        let window: c_ulong = chosen::x11_window_open(display.cast(), 1280, 720, 0);
        assert_eq!(window, 0);
        assert!(super::failed(), "PlayerIo::poll ends the stream");
        xlib::XCloseDisplay(display);
    }
    select(Kind::Rust);
}

#[test]
fn a_window_that_cannot_open_ends_the_stream() {
    let _turn = exclusive();
    let server = Server::start();
    select(Kind::Rust);
    let name = CString::new(server.name.as_str()).unwrap();
    // SAFETY: Xlib's own calls; then the call x11.c makes, with a size that
    // no X11 window can have.
    unsafe {
        let display = xlib::XOpenDisplay(name.as_ptr());
        assert!(!display.is_null());
        let window: c_ulong = chosen::x11_window_open(display.cast(), 70_000, 720, 0);
        assert_eq!(window, 0);
        assert!(super::failed(), "PlayerIo::poll ends the stream");
        xlib::XCloseDisplay(display);
    }
    select(Kind::Rust);
    assert!(!super::failed(), "a new stream starts with no failure");
}
