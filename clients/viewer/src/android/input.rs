// SPDX-License-Identifier: GPL-3.0-only
//! The stream's input on Android (#391 step D, slice D4): the app's key and
//! motion events become the core's input packets. The app turns
//! android-activity's events into the plain events here and hands them to
//! `Input` on its own thread; `Input` routes them as Artemis's `Game.java`
//! does (`handleKeyDown`, `handleKeyUp`, `handleMotionEvent`) to the
//! keyboard (`keyboard.rs`), the game controllers (`gamepad.rs`), the touch
//! screen and the mouse (`touch.rs`), and gives each packet to a `Sink`:
//! the core (`CoreSink`, behind `CORE_GATE`) in the app, a recording in
//! the tests.
//!
//! A device's identity (name, vendor, product, axes and keys) comes from
//! `android.view.InputDevice` through JNI (`devices.rs`): the NDK has none
//! of it. Timers (a touch's click, the emulated mouse, the controllers'
//! heartbeat) run when the app calls `tick`, by `next_deadline`.
//!
//! While the gameplay overlay is open the input is held (slice D5,
//! `hold`): one gate below every producer, as Artemis's
//! `NvConnection.setInputHeld` is, keeps everything but a controller's
//! arrival from the stream.
//!
//! Ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`, a fork of
//! `ClassicOldSong/moonlight-android`, itself a fork of
//! `moonlight-stream/moonlight-android`): `Game.java`'s input routing.
//! Artemis ships under the GNU GPL version 3 with no "or later" grant, so
//! this file does too. The Rust adaptation is Korri work dated 2026-11-14.
use super::{
    feedback::{Phone, Seats, Vibration},
    gamepad::{self, Controllers, Key},
    keyboard::{self, Keyboard, Special, code},
    touch::{Mouse, Touch},
};
use crate::{log, placement::Rect};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::{
        PoisonError, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

/// `MotionEvent`'s masked actions.
pub mod action {
    pub const DOWN: u32 = 0;
    pub const UP: u32 = 1;
    pub const MOVE: u32 = 2;
    pub const CANCEL: u32 = 3;
    pub const POINTER_DOWN: u32 = 5;
    pub const POINTER_UP: u32 = 6;
    pub const HOVER_MOVE: u32 = 7;
    pub const SCROLL: u32 = 8;
    pub const HOVER_ENTER: u32 = 9;
    pub const HOVER_EXIT: u32 = 10;
    pub const BUTTON_PRESS: u32 = 11;
    pub const BUTTON_RELEASE: u32 = 12;
}

/// `MotionEvent`'s axes that the viewer reads.
pub mod axis {
    pub const X: u32 = 0;
    pub const Y: u32 = 1;
    pub const ORIENTATION: u32 = 8;
    pub const VSCROLL: u32 = 9;
    pub const HSCROLL: u32 = 10;
    pub const Z: u32 = 11;
    pub const RX: u32 = 12;
    pub const RY: u32 = 13;
    pub const RZ: u32 = 14;
    pub const HAT_X: u32 = 15;
    pub const HAT_Y: u32 = 16;
    pub const LTRIGGER: u32 = 17;
    pub const RTRIGGER: u32 = 18;
    pub const THROTTLE: u32 = 19;
    pub const GAS: u32 = 22;
    pub const BRAKE: u32 = 23;
    pub const DISTANCE: u32 = 24;
    /// The joystick axes whose motion range a controller is asked for.
    pub const JOYSTICK: [u32; 13] = [
        X, Y, Z, RZ, RX, RY, LTRIGGER, RTRIGGER, BRAKE, GAS, THROTTLE, HAT_X, HAT_Y,
    ];
    /// One past the highest axis that an event carries here.
    pub const COUNT: usize = 25;
}

/// `InputDevice`'s sources and source classes.
pub mod source {
    pub const CLASS_BUTTON: u32 = 0x01;
    pub const CLASS_POINTER: u32 = 0x02;
    pub const CLASS_POSITION: u32 = 0x08;
    pub const CLASS_JOYSTICK: u32 = 0x10;
    pub const KEYBOARD: u32 = 0x0000_0101;
    pub const GAMEPAD: u32 = 0x0000_0401;
    pub const TOUCHSCREEN: u32 = 0x0000_1002;
    pub const MOUSE: u32 = 0x0000_2002;
    pub const MOUSE_RELATIVE: u32 = 0x0002_0004;
    pub const TOUCHPAD: u32 = 0x0010_0008;
    pub const JOYSTICK: u32 = 0x0100_0010;
}

/// `MotionEvent`'s tool types.
pub mod tool {
    pub const FINGER: u32 = 1;
    pub const STYLUS: u32 = 2;
    pub const MOUSE: u32 = 3;
    pub const ERASER: u32 = 4;
}

/// An axis's motion range (`InputDevice.MotionRange`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Range {
    pub axis: u32,
    pub min: f32,
    pub max: f32,
    pub flat: f32,
}

/// One input device as `android.view.InputDevice` describes it.
#[derive(Clone, Debug, Default)]
pub struct Device {
    pub id: i32,
    pub name: String,
    pub vendor: u16,
    pub product: u16,
    pub descriptor: String,
    /// `isExternal()`.
    pub external: bool,
    pub sources: u32,
    pub keyboard_type: i32,
    /// The joystick axes' ranges: `SOURCE_JOYSTICK`'s, else
    /// `SOURCE_GAMEPAD`'s (`getMotionRangeForJoystickAxis`).
    pub ranges: Vec<Range>,
    /// The touch screen's ranges of `ORIENTATION` and `DISTANCE`.
    pub pointer_ranges: Vec<Range>,
    /// The probed keys (`gamepad::probed_keys`) that the device has.
    pub keys: Vec<u32>,
    /// Its own vibrators (`getVibratorManager`, `getVibrator`).
    pub vibration: Vibration,
    /// Some light of its `LightsManager` reports RGB control.
    pub rgb_light: bool,
}

impl Device {
    pub fn range(&self, axis: u32) -> Option<Range> {
        self.ranges.iter().copied().find(|range| range.axis == axis)
    }
    pub fn pointer_range(&self, axis: u32) -> Option<Range> {
        self.pointer_ranges
            .iter()
            .copied()
            .find(|range| range.axis == axis)
    }
    pub fn has_key(&self, key: u32) -> bool {
        self.keys.contains(&key)
    }
}

/// The input devices Android has now.
pub trait Devices {
    /// The device with this ID; none when it is gone.
    fn describe(&mut self, id: i32) -> Option<Rc<Device>>;
    /// Every device's ID; none when Android could not be asked.
    fn ids(&mut self) -> Option<Vec<i32>>;
    /// The phone itself: its Android version and its own vibrators.
    fn phone(&mut self) -> Phone;
}

/// The key codes of every device that a controller is asked about.
pub fn probed_keys() -> Vec<u32> {
    gamepad::probed_keys()
}

/// The controllers attached at the start: the launch's controller mask.
pub fn attached(devices: &mut dyn Devices) -> u16 {
    let all: Vec<Rc<Device>> = devices
        .ids()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id| devices.describe(id))
        .collect();
    gamepad::attached_mask(all.iter().map(Rc::as_ref))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyAction {
    #[default]
    Down,
    Up,
    Multiple,
}

/// A key event (`android.view.KeyEvent`), times in milliseconds of
/// `CLOCK_MONOTONIC`.
#[derive(Clone, Debug, Default)]
pub struct KeyEvent {
    pub device: i32,
    pub source: u32,
    pub action: KeyAction,
    pub key_code: u32,
    pub scan_code: i32,
    pub meta_state: u32,
    pub flags: u32,
    pub repeat_count: i32,
    pub event_time_ms: i64,
    pub down_time_ms: i64,
}

/// One pointer of a motion event.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pointer {
    pub id: i32,
    pub tool_type: u32,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub touch_major: f32,
    pub touch_minor: f32,
    pub tool_major: f32,
    pub tool_minor: f32,
    pub orientation: f32,
    pub distance: f32,
}

/// The pointers' places at one earlier moment of a batched event.
#[derive(Clone, Debug, Default)]
pub struct HistorySample {
    pub time_ms: i64,
    pub positions: Vec<(f32, f32)>,
}

/// A motion event (`android.view.MotionEvent`).
#[derive(Clone, Debug, Default)]
pub struct MotionEvent {
    pub device: i32,
    pub source: u32,
    /// The masked action (`action::*`).
    pub action: u32,
    pub action_index: usize,
    pub button_state: u32,
    pub flags: u32,
    pub event_time_ms: i64,
    pub pointers: Vec<Pointer>,
    pub history: Vec<HistorySample>,
    /// The first pointer's axes, by axis ID (`axis::*`).
    pub axes: [f32; axis::COUNT],
}

impl MotionEvent {
    pub fn axis(&self, axis: u32) -> f32 {
        self.axes.get(axis as usize).copied().unwrap_or(0.0)
    }
}

/// The state of one controller as the stream carries it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ControllerState {
    pub number: u16,
    pub active_mask: u16,
    pub buttons: u32,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub left_stick_x: i16,
    pub left_stick_y: i16,
    pub right_stick_x: i16,
    pub right_stick_y: i16,
}

/// One input packet: one `LiSend*` call of the core.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Packet {
    /// `LiSendKeyboardEvent2`.
    Keyboard {
        code: i16,
        down: bool,
        modifiers: u8,
        flags: u8,
    },
    /// `LiSendMultiControllerEvent`.
    Controller(ControllerState),
    /// `LiSendControllerArrivalEvent`.
    Arrival {
        number: u8,
        active_mask: u16,
        kind: u8,
        buttons: u32,
        capabilities: u16,
    },
    /// `LiSendMouseMoveEvent`.
    MouseMove { dx: i16, dy: i16 },
    /// `LiSendMousePositionEvent`.
    MousePosition {
        x: i16,
        y: i16,
        width: i16,
        height: i16,
    },
    /// `LiSendMouseButtonEvent`.
    MouseButton { pressed: bool, button: u8 },
    /// `LiSendHighResScrollEvent`.
    Scroll { amount: i16 },
    /// `LiSendHighResHScrollEvent`.
    HScroll { amount: i16 },
    /// `LiSendTouchEvent`.
    Touch {
        kind: u8,
        pointer: u32,
        x: f32,
        y: f32,
        pressure: f32,
        contact_major: f32,
        contact_minor: f32,
        rotation: u16,
    },
    /// `LiSendControllerBatteryEvent`, from the feedback thread
    /// (`feedback.rs`).
    Battery {
        number: u8,
        state: u8,
        percentage: u8,
    },
}

/// Where packets go. The result is the core's: 0 when sent,
/// `LI_ERR_UNSUPPORTED` (-5501) when the host does not take that kind.
pub trait Sink {
    fn send(&mut self, packet: Packet) -> i32;
}

/// What a closed gate answers: the core's own answer before its input
/// stream starts.
pub const CLOSED: i32 = -2;

/// Whether the core takes input: open from a started connection until its
/// stop begins. The core's own check is a plain flag that a send reads once
/// (`InputStream.c`, `initialized`); its stop then destroys the mutex and the
/// queue that the send goes on to use. So each send holds the read lock
/// for its whole call into the core, and `close` takes the write lock: it
/// returns once no send is inside the core, and none enters after.
/// `closing` turns new sends away before `close` waits: a thread that sends
/// again and again would otherwise take the read lock back each time before
/// the woken writer runs, and hold the stop off.
pub struct Gate {
    open: RwLock<bool>,
    closing: AtomicBool,
}

impl Gate {
    pub const fn closed() -> Self {
        Self {
            open: RwLock::new(false),
            closing: AtomicBool::new(false),
        }
    }
    pub fn open(&self) {
        let mut open = self.open.write().unwrap_or_else(PoisonError::into_inner);
        self.closing.store(false, Ordering::SeqCst);
        *open = true;
    }
    /// Waits for the send in progress, if any.
    pub fn close(&self) {
        self.closing.store(true, Ordering::SeqCst);
        *self.open.write().unwrap_or_else(PoisonError::into_inner) = false;
    }
    fn pass(&self, send: impl FnOnce() -> i32) -> i32 {
        if self.closing.load(Ordering::SeqCst) {
            return CLOSED;
        }
        let open = self.open.read().unwrap_or_else(PoisonError::into_inner);
        if *open { send() } else { CLOSED }
    }
}

/// The gate of the core's input, for the process: the core is one per
/// process. `stream.rs` opens it after `LiStartConnection` succeeds and
/// closes it before `LiStopConnection`.
#[cfg(target_os = "android")]
pub(crate) static CORE_GATE: Gate = Gate::closed();

/// A sink behind a gate. Only this crate makes one: no sink of the app
/// reaches the core past its gate.
pub struct Gated<'g, S> {
    pub(crate) gate: &'g Gate,
    pub(crate) sink: S,
}

impl<S: Sink> Sink for Gated<'_, S> {
    fn send(&mut self, packet: Packet) -> i32 {
        let sink = &mut self.sink;
        self.gate.pass(|| sink.send(packet))
    }
}

/// What the app does with a key after `Input` saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Sent or eaten; Android does nothing more with it.
    Handled,
    /// Android's own: Back leaves the stream, a volume key sets the volume.
    Unhandled,
    /// Ctrl+Alt+Shift+Q: leave the stream.
    Leave,
    /// Start pressed twice quickly and held the second time: open the
    /// gameplay overlay (Artemis's quick menu).
    Overlay,
}

/// The settings of the stream's input. Artemis's defaults for the rest:
/// several controllers, a 5 % stick deadzone, multi-touch.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Start held for 750 ms turns a controller into a mouse
    /// (`-nomouseemulation` turns it off, as in Moonlight).
    pub mouse_emulation: bool,
}

/// Everything the stream's input holds, on the app's thread.
pub struct Input {
    keyboard: Keyboard,
    controllers: Controllers,
    touch: Touch,
    mouse: Mouse,
    /// Where the picture is in the window.
    video: Rect,
    hold: Hold,
    counts: Counts,
}

/// Artemis's input hold (`NvConnection.setInputHeld`): one gate below every
/// producer, so the controllers' heartbeat and the touch's and the emulated
/// mouse's timers are held too. It knows what the host holds down from what
/// the core took. It sends only into the sink it is given: in the app, the
/// core behind its gate (`CORE_GATE`).
#[derive(Debug, Default)]
struct Hold {
    held: bool,
    /// Each key that went down and not up, with the flags it went with.
    keys: BTreeMap<i16, u8>,
    /// Each mouse button that went down and not up.
    buttons: BTreeSet<u8>,
    /// Each controller that sent a state, with its last active mask.
    controllers: BTreeMap<u16, u16>,
    /// A touch went that was no cancel of every touch.
    touch: bool,
}

impl Hold {
    /// What the core took: the host holds it down now, or no longer.
    fn went(&mut self, packet: &Packet) {
        match *packet {
            Packet::Keyboard {
                code, down, flags, ..
            } => {
                if down {
                    self.keys.insert(code, flags);
                } else {
                    self.keys.remove(&code);
                }
            }
            Packet::MouseButton { pressed, button } => {
                if pressed {
                    self.buttons.insert(button);
                } else {
                    self.buttons.remove(&button);
                }
            }
            Packet::Controller(state) => {
                self.controllers.insert(state.number, state.active_mask);
            }
            Packet::Touch { kind, .. } => self.touch = kind != super::touch::kind::CANCEL_ALL,
            _ => {}
        }
    }

    /// `setInputHeld(true)`: the host lets go of every controller, key,
    /// mouse button and touch. Artemis releases a key with no flags; here it
    /// keeps the flags it went down with, so that the host releases the key
    /// it pressed.
    fn let_go(&mut self) -> Vec<Packet> {
        let mut packets: Vec<Packet> = self
            .controllers
            .iter()
            .map(|(&number, &active_mask)| {
                Packet::Controller(ControllerState {
                    number,
                    active_mask,
                    ..Default::default()
                })
            })
            .collect();
        packets.extend(
            std::mem::take(&mut self.keys)
                .into_iter()
                .map(|(code, flags)| Packet::Keyboard {
                    code,
                    down: false,
                    modifiers: 0,
                    flags,
                }),
        );
        packets.extend(std::mem::take(&mut self.buttons).into_iter().map(|button| {
            Packet::MouseButton {
                pressed: false,
                button,
            }
        }));
        if std::mem::take(&mut self.touch) {
            packets.push(Packet::Touch {
                kind: super::touch::kind::CANCEL_ALL,
                pointer: 0,
                x: 0.0,
                y: 0.0,
                pressure: 0.0,
                contact_major: 0.0,
                contact_minor: 0.0,
                rotation: 0,
            });
        }
        packets
    }
}

/// What `Input` sent, for the log at the end.
#[derive(Debug, Default)]
struct Counts {
    keys: u32,
    controllers: u32,
    arrivals: u32,
    mouse: u32,
    touches: u32,
    /// Kept from the stream by the hold.
    held: u32,
    unsupported: u32,
    /// Refused by the closed gate: the stream had not started or had ended.
    closed: u32,
    failed: u32,
}

/// The sink that holds and counts what passes through it.
struct Counting<'a> {
    sink: &'a mut dyn Sink,
    hold: &'a mut Hold,
    counts: &'a mut Counts,
}

impl Sink for Counting<'_> {
    fn send(&mut self, packet: Packet) -> i32 {
        let counts = &mut *self.counts;
        if self.hold.held && !matches!(packet, Packet::Arrival { .. }) {
            counts.held += 1;
            // As Artemis answers a held touch.
            return if matches!(packet, Packet::Touch { .. }) {
                super::touch::UNSUPPORTED
            } else {
                0
            };
        }
        let result = self.sink.send(packet);
        if result == 0 {
            self.hold.went(&packet);
        }
        match result {
            0 => {}
            touch_unsupported if touch_unsupported == super::touch::UNSUPPORTED => {
                counts.unsupported += 1
            }
            CLOSED => counts.closed += 1,
            _ => counts.failed += 1,
        }
        if result == 0 {
            match packet {
                Packet::Keyboard { .. } => counts.keys += 1,
                Packet::Controller(_) => counts.controllers += 1,
                Packet::Arrival { .. } => counts.arrivals += 1,
                Packet::Touch { .. } => counts.touches += 1,
                // The feedback thread sends these, with its own counts.
                Packet::Battery { .. } => {}
                _ => counts.mouse += 1,
            }
        }
        result
    }
}

impl Input {
    /// `attached`: the controller mask that the launch sent. `seats`: where
    /// each controller's seat goes for its rumble, light and battery
    /// (`feedback::Worker::seats`).
    pub fn new(settings: Settings, attached: u16, video: Rect, seats: Box<dyn Seats>) -> Self {
        Self {
            keyboard: Keyboard::default(),
            controllers: Controllers::new(attached, settings.mouse_emulation, seats),
            touch: Touch::default(),
            mouse: Mouse::default(),
            video,
            hold: Hold::default(),
            counts: Counts::default(),
        }
    }

    /// The gameplay overlay opened (`Game.showSurfaceOverlay`): the host
    /// lets go of everything it holds down, and nothing more goes to it but
    /// a controller's arrival until `release`. The mapping starts clean.
    pub fn hold(&mut self, sink: &mut dyn Sink) {
        if self.hold.held {
            return;
        }
        self.hold.held = true;
        let mut counted = Counting {
            sink,
            hold: &mut Hold::default(),
            counts: &mut self.counts,
        };
        for packet in self.hold.let_go() {
            counted.send(packet);
        }
        self.reset(sink);
    }

    /// The gameplay overlay closed (`Game.hideSurfaceOverlay`): the mapping
    /// starts clean again, then input goes to the stream. Nothing is sent.
    pub fn release(&mut self, sink: &mut dyn Sink) {
        self.reset(sink);
        self.hold.held = false;
    }

    /// `resetStreamInputState`, while held: what it would send is held.
    fn reset(&mut self, sink: &mut dyn Sink) {
        let sink = &mut Counting {
            sink,
            hold: &mut self.hold,
            counts: &mut self.counts,
        };
        self.controllers.reset();
        self.keyboard.reset();
        self.mouse = Mouse::default();
        self.touch.cancel(sink);
    }

    /// The picture moved in the window (a turn or a fold).
    pub fn place(&mut self, video: Rect) {
        self.video = video;
    }

    /// `handleKeyDown`, `handleKeyUp` and `handleKeyMultiple`.
    pub fn key(
        &mut self,
        event: &KeyEvent,
        now_ms: i64,
        devices: &mut dyn Devices,
        sink: &mut dyn Sink,
    ) -> Outcome {
        let sink = &mut Counting {
            sink,
            hold: &mut self.hold,
            counts: &mut self.counts,
        };
        // Android's own virtual navigation keys.
        if event.flags & VIRTUAL_HARD_KEY != 0 {
            return Outcome::Unhandled;
        }
        let down = match event.action {
            KeyAction::Down => true,
            KeyAction::Up => false,
            // Text from a soft keyboard: Artemis sends it as UTF-8; the NDK
            // gives no characters.
            KeyAction::Multiple => return Outcome::Unhandled,
        };
        // Android sends a mouse's right button as Back.
        if matches!(event.source, source::MOUSE | source::MOUSE_RELATIVE)
            && event.key_code == code::BACK
        {
            self.mouse.back(down, sink);
            return Outcome::Handled;
        }
        let device = devices.describe(event.device);
        if gamepad::is_game_controller(device.as_deref()) {
            match self
                .controllers
                .key(event, device.as_deref(), devices, now_ms, sink)
            {
                Key::Handled => return Outcome::Handled,
                Key::Overlay => return Outcome::Overlay,
                Key::Unhandled => {}
            }
        }
        match self.keyboard.special(event.key_code, down) {
            Special::Eaten => return Outcome::Handled,
            Special::Quit => {
                log::line("input: Ctrl+Alt+Shift+Q: leaving the stream");
                return Outcome::Leave;
            }
            Special::None => {}
        }
        let Some(code) = keyboard::translate(event.key_code, event.scan_code) else {
            return Outcome::Unhandled;
        };
        // The host repeats a held key itself.
        if down && event.repeat_count > 0 {
            return Outcome::Handled;
        }
        let modifiers = self.keyboard.modifiers | keyboard::meta_modifiers(event.meta_state);
        sink.send(Packet::Keyboard {
            code,
            down,
            modifiers,
            flags: keyboard::NON_NORMALIZED,
        });
        Outcome::Handled
    }

    /// `handleMotionEvent`. False: Android may do its own with it.
    pub fn motion(
        &mut self,
        event: &MotionEvent,
        now_ms: i64,
        devices: &mut dyn Devices,
        sink: &mut dyn Sink,
    ) -> bool {
        let sink = &mut Counting {
            sink,
            hold: &mut self.hold,
            counts: &mut self.counts,
        };
        let class = |class: u32| event.source & class != 0;
        if class(source::CLASS_JOYSTICK) {
            let device = devices.describe(event.device);
            self.controllers
                .motion(event, device.as_deref(), devices, sink);
            return true;
        }
        if !(class(source::CLASS_POINTER)
            || class(source::CLASS_POSITION)
            || event.source == source::MOUSE_RELATIVE)
        {
            return false;
        }
        if super::touch::is_mouse(event) {
            self.mouse.event(event, self.video, sink);
            return true;
        }
        if event.source == source::TOUCHPAD {
            // A touchpad's own gestures (Artemis's TrackpadContext) are not
            // carried.
            return false;
        }
        let device = devices.describe(event.device);
        self.touch
            .event(event, device.as_deref(), self.video, now_ms, sink);
        true
    }

    /// Ask Android which devices it has: the controllers of those that went
    /// go too. A list that cannot be had says nothing about any device, so
    /// every controller stays.
    pub fn check_devices(&mut self, devices: &mut dyn Devices, sink: &mut dyn Sink) {
        if let Some(present) = devices.ids() {
            self.present(&present, sink);
        }
    }

    /// The devices that Android no longer has: their controllers go.
    fn present(&mut self, ids: &[i32], sink: &mut dyn Sink) {
        let sink = &mut Counting {
            sink,
            hold: &mut self.hold,
            counts: &mut self.counts,
        };
        let gone: Vec<i32> = self
            .controllers
            .devices()
            .filter(|id| !ids.contains(id))
            .collect();
        for id in gone {
            self.controllers.removed(id, sink);
        }
    }

    /// Run the timers that are due.
    pub fn tick(&mut self, now_ms: i64, sink: &mut dyn Sink) {
        let sink = &mut Counting {
            sink,
            hold: &mut self.hold,
            counts: &mut self.counts,
        };
        self.touch.tick(now_ms, self.video, sink);
        self.controllers.tick(now_ms, sink);
    }

    /// When `tick` next has work.
    pub fn next_deadline(&self) -> Option<i64> {
        [self.touch.next_deadline(), self.controllers.next_deadline()]
            .into_iter()
            .flatten()
            .min()
    }

    /// What the input sent, for the log.
    pub fn report(&self) -> String {
        let c = &self.counts;
        format!(
            "{} keys, {} controller states, {} arrivals, {} mouse, {} touches sent; {} held; {} unsupported, {} outside the stream, {} failed",
            c.keys,
            c.controllers,
            c.arrivals,
            c.mouse,
            c.touches,
            c.held,
            c.unsupported,
            c.closed,
            c.failed
        )
    }
}

const VIRTUAL_HARD_KEY: u32 = 0x40;

/// Every device, for the checks that look at the others.
pub(crate) fn everything(devices: &mut dyn Devices) -> Vec<Device> {
    devices
        .ids()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id| devices.describe(id))
        .map(|device| (*device).clone())
        .collect()
}

/// The core's input, behind its gate (`CORE_GATE`): what the app and the
/// feedback thread send to. The only way to the core from outside this
/// crate.
#[cfg(target_os = "android")]
pub struct CoreSink(Gated<'static, Core>);

#[cfg(target_os = "android")]
impl Sink for CoreSink {
    fn send(&mut self, packet: Packet) -> i32 {
        self.0.send(packet)
    }
}

#[cfg(target_os = "android")]
pub fn core_sink() -> CoreSink {
    CoreSink(Gated {
        gate: &CORE_GATE,
        sink: Core,
    })
}

/// The core itself: each packet is one `LiSend*` call, which queues it for
/// the core's input thread. Only through the gate.
#[cfg(target_os = "android")]
pub(crate) struct Core;

#[cfg(target_os = "android")]
impl Sink for Core {
    fn send(&mut self, packet: Packet) -> i32 {
        use crate::network as ffi;
        // SAFETY: each call copies its arguments into the core's queue.
        unsafe {
            match packet {
                Packet::Keyboard {
                    code,
                    down,
                    modifiers,
                    flags,
                } => ffi::LiSendKeyboardEvent2(
                    code,
                    if down {
                        ffi::KEY_ACTION_DOWN
                    } else {
                        ffi::KEY_ACTION_UP
                    } as _,
                    modifiers as _,
                    flags as _,
                ),
                Packet::Controller(state) => ffi::LiSendMultiControllerEvent(
                    state.number as i16,
                    state.active_mask as i16,
                    state.buttons as i32,
                    state.left_trigger,
                    state.right_trigger,
                    state.left_stick_x,
                    state.left_stick_y,
                    state.right_stick_x,
                    state.right_stick_y,
                ),
                Packet::Arrival {
                    number,
                    active_mask,
                    kind,
                    buttons,
                    capabilities,
                } => ffi::LiSendControllerArrivalEvent(
                    number,
                    active_mask,
                    kind,
                    buttons,
                    capabilities,
                ),
                Packet::MouseMove { dx, dy } => ffi::LiSendMouseMoveEvent(dx, dy),
                Packet::MousePosition {
                    x,
                    y,
                    width,
                    height,
                } => ffi::LiSendMousePositionEvent(x, y, width, height),
                Packet::MouseButton { pressed, button } => ffi::LiSendMouseButtonEvent(
                    if pressed {
                        ffi::BUTTON_ACTION_PRESS
                    } else {
                        ffi::BUTTON_ACTION_RELEASE
                    } as _,
                    i32::from(button),
                ),
                Packet::Scroll { amount } => ffi::LiSendHighResScrollEvent(amount),
                Packet::HScroll { amount } => ffi::LiSendHighResHScrollEvent(amount),
                Packet::Touch {
                    kind,
                    pointer,
                    x,
                    y,
                    pressure,
                    contact_major,
                    contact_minor,
                    rotation,
                } => ffi::LiSendTouchEvent(
                    kind,
                    pointer,
                    x,
                    y,
                    pressure,
                    contact_major,
                    contact_minor,
                    rotation,
                ),
                Packet::Battery {
                    number,
                    state,
                    percentage,
                } => ffi::LiSendControllerBatteryEvent(number, state, percentage),
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        feedback::{Owner, Phone, Rumble, Seat, Seats, Vibration, Vibrators},
        gamepad::{
            CAPABILITY_ANALOG_TRIGGERS, CAPABILITY_BATTERY_STATE, CAPABILITY_RGB_LED,
            CAPABILITY_RUMBLE, CAPABILITY_TRIGGER_RUMBLE, HEARTBEAT_MS, MOUSE_LEFT, MOUSE_RIGHT,
            button,
        },
        touch::{UNSUPPORTED, kind},
    };
    use std::{
        cell::RefCell,
        collections::BTreeMap,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };

    /// A sink that records each packet. With `touch` false the host takes
    /// no touch, as `LiSendTouchEvent` answers then.
    pub(crate) struct Recorded {
        pub packets: Vec<Packet>,
        pub touch: bool,
    }

    impl Default for Recorded {
        fn default() -> Self {
            Self {
                packets: Vec::new(),
                touch: true,
            }
        }
    }

    impl Sink for Recorded {
        fn send(&mut self, packet: Packet) -> i32 {
            if matches!(packet, Packet::Touch { .. }) && !self.touch {
                return UNSUPPORTED;
            }
            self.packets.push(packet);
            0
        }
    }

    impl Recorded {
        fn take(&mut self) -> Vec<Packet> {
            std::mem::take(&mut self.packets)
        }
    }

    /// The devices of a test, as Android would describe them. With
    /// `unreadable`, the list of IDs cannot be had, as when a JNI call fails.
    #[derive(Default)]
    pub(crate) struct Known {
        pub devices: BTreeMap<i32, Rc<Device>>,
        pub unreadable: bool,
        pub phone: Phone,
    }

    impl Devices for Known {
        fn describe(&mut self, id: i32) -> Option<Rc<Device>> {
            self.devices.get(&id).cloned()
        }
        fn ids(&mut self) -> Option<Vec<i32>> {
            (!self.unreadable).then(|| self.devices.keys().copied().collect())
        }
        fn phone(&mut self) -> Phone {
            self.phone
        }
    }

    /// What the input told the feedback: each seat, and each device that left.
    #[derive(Debug, Default)]
    pub(crate) struct SeatLog {
        pub seated: Vec<Seat>,
        pub left: Vec<i32>,
    }

    /// Seats that keep a log the test reads.
    pub(crate) struct RecordedSeats(Rc<RefCell<SeatLog>>);

    impl Seats for RecordedSeats {
        fn seated(&mut self, seat: Seat) {
            self.0.borrow_mut().seated.push(seat);
        }
        fn left(&mut self, device: i32) {
            self.0.borrow_mut().left.push(device);
        }
    }

    pub(crate) fn seats() -> (Box<dyn Seats>, Rc<RefCell<SeatLog>>) {
        let log = Rc::new(RefCell::new(SeatLog::default()));
        (Box::new(RecordedSeats(log.clone())), log)
    }

    impl Known {
        fn with(devices: impl IntoIterator<Item = Device>) -> Self {
            Self {
                devices: devices
                    .into_iter()
                    .map(|device| (device.id, Rc::new(device)))
                    .collect(),
                unreadable: false,
                phone: Phone {
                    api_level: 31,
                    vibration: Vibration::default(),
                },
            }
        }
    }

    fn range(axis: u32, min: f32, flat: f32) -> Range {
        Range {
            axis,
            min,
            max: 1.0,
            flat,
        }
    }

    /// The emulator's uhid gamepad of the proof: Generic.kl, sticks on X, Y,
    /// Z and RZ, triggers on BRAKE and GAS, a hat.
    pub(crate) fn pad(id: i32, vendor: u16) -> Device {
        Device {
            id,
            name: "Korri test gamepad".into(),
            vendor,
            product: 0x7e57,
            descriptor: format!("pad-{id}"),
            external: true,
            sources: source::KEYBOARD | source::GAMEPAD | source::JOYSTICK,
            keyboard_type: 1,
            ranges: vec![
                range(axis::X, -1.0, 0.118),
                range(axis::Y, -1.0, 0.118),
                range(axis::Z, -1.0, 0.118),
                range(axis::RZ, -1.0, 0.118),
                range(axis::BRAKE, 0.0, 0.059),
                range(axis::GAS, 0.0, 0.059),
                range(axis::HAT_X, -1.0, 0.0),
                range(axis::HAT_Y, -1.0, 0.0),
            ],
            pointer_ranges: Vec::new(),
            keys: vec![
                code::BUTTON_A,
                code::BUTTON_B,
                code::BUTTON_X,
                code::BUTTON_Y,
                code::BUTTON_L1,
                code::BUTTON_R1,
                code::BUTTON_THUMBL,
                code::BUTTON_THUMBR,
                code::BUTTON_START,
                code::BUTTON_SELECT,
                code::BUTTON_MODE,
            ],
            ..Default::default()
        }
    }

    /// The emulator's own keyboard, and the virtual one of `input keyevent`.
    fn keyboard_device(id: i32) -> Device {
        Device {
            id,
            name: "Virtual".into(),
            sources: source::KEYBOARD,
            keyboard_type: gamepad::KEYBOARD_TYPE_ALPHABETIC,
            ..Default::default()
        }
    }

    fn touchscreen(id: i32) -> Device {
        Device {
            id,
            name: "virtio_input_multi_touch_1".into(),
            sources: source::TOUCHSCREEN,
            ..Default::default()
        }
    }

    const VIDEO: Rect = Rect {
        left: 0,
        top: 210,
        right: 1080,
        bottom: 818,
    };

    fn input() -> Input {
        Input::new(
            Settings {
                mouse_emulation: true,
            },
            0,
            VIDEO,
            seats().0,
        )
    }

    fn key(
        device: i32,
        key_code: u32,
        scan_code: i32,
        action: KeyAction,
        time_ms: i64,
        down_ms: i64,
    ) -> KeyEvent {
        KeyEvent {
            device,
            source: source::KEYBOARD,
            action,
            key_code,
            scan_code,
            event_time_ms: time_ms,
            down_time_ms: down_ms,
            ..Default::default()
        }
    }

    fn press(
        input: &mut Input,
        devices: &mut Known,
        sink: &mut Recorded,
        device: i32,
        key_code: u32,
        scan_code: i32,
        at_ms: i64,
    ) -> (Outcome, Outcome) {
        let down = input.key(
            &key(device, key_code, scan_code, KeyAction::Down, at_ms, at_ms),
            at_ms,
            devices,
            sink,
        );
        let up_at = at_ms + 50;
        let up = input.key(
            &key(device, key_code, scan_code, KeyAction::Up, up_at, at_ms),
            up_at,
            devices,
            sink,
        );
        (down, up)
    }

    fn joystick(device: i32, values: &[(u32, f32)], time_ms: i64) -> MotionEvent {
        let mut event = MotionEvent {
            device,
            source: source::JOYSTICK,
            action: action::MOVE,
            event_time_ms: time_ms,
            pointers: vec![Pointer::default()],
            ..Default::default()
        };
        for &(axis, value) in values {
            event.axes[axis as usize] = value;
        }
        event
    }

    fn controller(number: u16, active_mask: u16, buttons: u32) -> ControllerState {
        ControllerState {
            number,
            active_mask,
            buttons,
            ..Default::default()
        }
    }

    #[test]
    fn a_keyboard_key_goes_as_its_windows_code_with_the_modifiers() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([keyboard_device(-1)]);
        assert_eq!(
            press(&mut input, &mut devices, &mut sink, -1, code::A, 30, 1000),
            (Outcome::Handled, Outcome::Handled)
        );
        // Shift held: the key carries it, from the key and the meta state.
        let shift = key(-1, code::SHIFT_LEFT, 42, KeyAction::Down, 2000, 2000);
        input.key(&shift, 2000, &mut devices, &mut sink);
        let mut b = key(-1, code::A + 1, 48, KeyAction::Down, 2010, 2010);
        b.meta_state = 0x41;
        input.key(&b, 2010, &mut devices, &mut sink);
        assert_eq!(
            sink.take(),
            [
                Packet::Keyboard {
                    code: 0x8041u16 as i16,
                    down: true,
                    modifiers: 0,
                    flags: 1
                },
                Packet::Keyboard {
                    code: 0x8041u16 as i16,
                    down: false,
                    modifiers: 0,
                    flags: 1
                },
                Packet::Keyboard {
                    code: 0x80a0u16 as i16,
                    down: true,
                    modifiers: 1,
                    flags: 1
                },
                Packet::Keyboard {
                    code: 0x8042u16 as i16,
                    down: true,
                    modifiers: 1,
                    flags: 1
                },
            ]
        );
    }

    #[test]
    fn a_repeated_key_is_not_sent_again() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([keyboard_device(-1)]);
        let mut held = key(-1, code::SPACE, 57, KeyAction::Down, 1000, 1000);
        input.key(&held, 1000, &mut devices, &mut sink);
        held.repeat_count = 1;
        assert_eq!(
            input.key(&held, 1500, &mut devices, &mut sink),
            Outcome::Handled
        );
        assert_eq!(sink.take().len(), 1);
    }

    #[test]
    fn back_from_a_keyboard_and_the_volume_stay_androids() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([keyboard_device(-1)]);
        let (down, up) = press(
            &mut input,
            &mut devices,
            &mut sink,
            -1,
            code::BACK,
            158,
            1000,
        );
        assert_eq!((down, up), (Outcome::Unhandled, Outcome::Unhandled));
        // The phone's own buttons are no full keyboard: the controllers see
        // them first, and give them back.
        let mut buttons = keyboard_device(2);
        buttons.keyboard_type = 1;
        devices.devices.insert(2, Rc::new(buttons));
        let (down, _) = press(&mut input, &mut devices, &mut sink, 2, 24, 115, 2000);
        assert_eq!(down, Outcome::Unhandled);
        assert!(sink.take().is_empty());
    }

    #[test]
    fn ctrl_alt_shift_q_leaves() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([keyboard_device(-1)]);
        let mut at = 1000;
        let mut send = |input: &mut Input, key_code, action| {
            at += 30;
            input.key(
                &key(-1, key_code, 0, action, at, at),
                at,
                &mut devices,
                &mut sink,
            )
        };
        for key_code in [code::CTRL_LEFT, code::ALT_LEFT, code::SHIFT_LEFT] {
            send(&mut input, key_code, KeyAction::Down);
        }
        assert_eq!(send(&mut input, code::Q, KeyAction::Down), Outcome::Handled);
        send(&mut input, code::Q, KeyAction::Up);
        send(&mut input, code::CTRL_LEFT, KeyAction::Up);
        send(&mut input, code::ALT_LEFT, KeyAction::Up);
        assert_eq!(
            send(&mut input, code::SHIFT_LEFT, KeyAction::Up),
            Outcome::Leave
        );
    }

    #[test]
    fn a_gamepad_arrives_with_its_kind_and_buttons_then_sends_its_state() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        let (down, up) = press(
            &mut input,
            &mut devices,
            &mut sink,
            16,
            code::BUTTON_A,
            304,
            1000,
        );
        assert_eq!((down, up), (Outcome::Handled, Outcome::Handled));
        let buttons = button::A
            | button::B
            | button::X
            | button::Y
            | button::LB
            | button::RB
            | button::LS_CLK
            | button::RS_CLK
            | button::PLAY
            | button::BACK
            | button::SPECIAL
            | button::UP
            | button::DOWN
            | button::LEFT
            | button::RIGHT;
        assert_eq!(
            sink.take(),
            [
                Packet::Arrival {
                    number: 0,
                    active_mask: 1,
                    kind: 1,
                    buttons,
                    // External: its battery is reported. No vibrator, no light.
                    capabilities: CAPABILITY_ANALOG_TRIGGERS | CAPABILITY_BATTERY_STATE,
                },
                Packet::Controller(controller(0, 1, button::A)),
                Packet::Controller(controller(0, 1, 0)),
            ]
        );
    }

    #[test]
    fn two_gamepads_take_numbers_in_turn_and_the_mask_holds_both() {
        let (mut input, mut sink) = (
            Input::new(
                Settings {
                    mouse_emulation: true,
                },
                0b11,
                VIDEO,
                seats().0,
            ),
            Recorded::default(),
        );
        let mut devices = Known::with([pad(16, 0x054c), pad(17, 0x057e)]);
        press(
            &mut input,
            &mut devices,
            &mut sink,
            17,
            code::BUTTON_B,
            305,
            1000,
        );
        press(
            &mut input,
            &mut devices,
            &mut sink,
            16,
            code::BUTTON_X,
            307,
            2000,
        );
        let packets = sink.take();
        assert!(matches!(
            packets[0],
            Packet::Arrival {
                number: 0,
                active_mask: 0b11,
                kind: 3,
                ..
            }
        ));
        assert_eq!(
            packets[1],
            Packet::Controller(controller(0, 0b11, button::B))
        );
        assert!(matches!(
            packets[3],
            Packet::Arrival {
                number: 1,
                active_mask: 0b11,
                kind: 2,
                ..
            }
        ));
        assert_eq!(
            packets[4],
            Packet::Controller(controller(1, 0b11, button::X))
        );
    }

    #[test]
    fn sticks_have_a_deadzone_and_triggers_their_own() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        // Inside the 5 % stick deadzone and the 13 % trigger deadzone.
        let quiet = joystick(
            16,
            &[(axis::X, 0.03), (axis::Y, -0.03), (axis::BRAKE, 0.1)],
            1000,
        );
        assert!(input.motion(&quiet, 1000, &mut devices, &mut sink));
        // Outside: Y is up, so the stream's Y is positive; triggers to 0-255.
        let moved = joystick(
            16,
            &[
                (axis::X, 0.5),
                (axis::Y, -1.0),
                (axis::Z, -0.25),
                (axis::RZ, 0.04),
                (axis::BRAKE, 0.5),
                (axis::GAS, 1.0),
            ],
            1010,
        );
        input.motion(&moved, 1010, &mut devices, &mut sink);
        let packets: Vec<_> = sink
            .take()
            .into_iter()
            .filter(|p| matches!(p, Packet::Controller(_)))
            .collect();
        assert_eq!(packets[0], Packet::Controller(controller(0, 1, 0)));
        assert_eq!(
            packets[1],
            Packet::Controller(ControllerState {
                number: 0,
                active_mask: 1,
                buttons: 0,
                left_trigger: 127,
                right_trigger: 255,
                left_stick_x: 16383,
                left_stick_y: 32766,
                // The right stick is outside the deadzone as a whole.
                right_stick_x: -8191,
                right_stick_y: -1310,
            })
        );
    }

    #[test]
    fn the_hat_is_the_dpad_and_then_its_keys_are_not_sent_twice() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        input.motion(
            &joystick(16, &[(axis::HAT_X, -1.0), (axis::HAT_Y, 1.0)], 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        // Android's own d-pad key from the same hat.
        let down = input.key(
            &key(16, code::DPAD_LEFT, 0, KeyAction::Down, 1001, 1001),
            1001,
            &mut devices,
            &mut sink,
        );
        assert_eq!(down, Outcome::Handled);
        input.motion(&joystick(16, &[], 1100), 1100, &mut devices, &mut sink);
        let states: Vec<_> = sink
            .take()
            .into_iter()
            .filter_map(|p| match p {
                Packet::Controller(state) => Some(state.buttons),
                _ => None,
            })
            .collect();
        assert_eq!(states, [button::LEFT | button::DOWN, 0]);
    }

    #[test]
    fn start_and_select_together_are_guide_on_a_pad_without_one() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut no_mode = pad(16, 0x045e);
        no_mode.keys.retain(|&key| key != code::BUTTON_MODE);
        let mut devices = Known::with([no_mode]);
        let at = |t| key(16, code::BUTTON_START, 315, KeyAction::Down, t, t);
        input.key(&at(1000), 1000, &mut devices, &mut sink);
        input.key(
            &key(16, code::BUTTON_SELECT, 314, KeyAction::Down, 1010, 1010),
            1010,
            &mut devices,
            &mut sink,
        );
        input.key(
            &key(16, code::BUTTON_SELECT, 314, KeyAction::Up, 1100, 1010),
            1100,
            &mut devices,
            &mut sink,
        );
        let states: Vec<_> = sink
            .take()
            .into_iter()
            .filter_map(|p| match p {
                Packet::Controller(state) => Some(state.buttons),
                _ => None,
            })
            .collect();
        // Artemis does not give Start back while it is still held.
        assert_eq!(states, [button::PLAY, button::SPECIAL, 0]);
    }

    /// A DualShock 4 as Linux maps it: right stick on RX and RY, triggers
    /// on Z and RZ that idle at -1, and its touchpad as a second device.
    fn dualshock(id: i32) -> Device {
        let mut ds4 = pad(id, 0x054c);
        ds4.name = "Sony Interactive Entertainment Wireless Controller".into();
        ds4.product = 0x09cc;
        ds4.descriptor = "ds4".into();
        ds4.ranges = [
            axis::X,
            axis::Y,
            axis::RX,
            axis::RY,
            axis::Z,
            axis::RZ,
            axis::HAT_X,
            axis::HAT_Y,
        ]
        .into_iter()
        .map(|axis| range(axis, -1.0, 0.0))
        .collect();
        ds4
    }

    #[test]
    fn a_dualshock_4_gets_linuxs_face_buttons_and_its_triggers_idle_at_minus_one() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([dualshock(20)]);
        // Android's Generic.kl calls BTN_NORTH (307) X; it is Y on this pad.
        press(
            &mut input,
            &mut devices,
            &mut sink,
            20,
            code::BUTTON_X,
            307,
            1000,
        );
        // Untouched triggers send 0 until first used, then idle at -1.
        input.motion(
            &joystick(20, &[(axis::Z, 0.0)], 1100),
            1100,
            &mut devices,
            &mut sink,
        );
        input.motion(
            &joystick(20, &[(axis::Z, 1.0), (axis::RZ, -1.0)], 1200),
            1200,
            &mut devices,
            &mut sink,
        );
        input.motion(
            &joystick(20, &[(axis::Z, -1.0), (axis::RZ, -1.0)], 1300),
            1300,
            &mut devices,
            &mut sink,
        );
        let states: Vec<_> = sink
            .take()
            .into_iter()
            .filter_map(|p| match p {
                Packet::Controller(state) => {
                    Some((state.buttons, state.left_trigger, state.right_trigger))
                }
                Packet::Arrival { kind, .. } => {
                    assert_eq!(kind, crate::controller_list::PS);
                    None
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            states,
            [
                (button::Y, 0, 0),
                (0, 0, 0),
                (0, 0, 0),
                (0, 255, 0),
                (0, 0, 0)
            ]
        );
    }

    #[test]
    fn a_touchpad_device_takes_the_number_of_its_controller() {
        let mut input = Input::new(
            Settings {
                mouse_emulation: true,
            },
            0,
            VIDEO,
            seats().0,
        );
        let mut sink = Recorded::default();
        let other = pad(16, 0x045e);
        let mut touchpad = dualshock(21);
        touchpad.name = "Sony Interactive Entertainment Wireless Controller Touchpad".into();
        touchpad.sources = source::KEYBOARD | source::MOUSE;
        touchpad.ranges.clear();
        touchpad.keys = vec![code::BUTTON_1];
        let mut devices = Known::with([other, dualshock(20), touchpad]);
        press(
            &mut input,
            &mut devices,
            &mut sink,
            16,
            code::BUTTON_A,
            304,
            1000,
        );
        // The touchpad's button comes first: the pad next to it takes the
        // next number, and the touchpad shares it.
        press(
            &mut input,
            &mut devices,
            &mut sink,
            21,
            code::BUTTON_1,
            272,
            2000,
        );
        let packets = sink.take();
        assert!(
            packets.contains(&Packet::Controller(controller(1, 0b11, button::TOUCHPAD))),
            "{packets:?}"
        );
        assert!(packets.iter().any(|p| matches!(
            p,
            Packet::Arrival {
                number: 1,
                kind: 2,
                ..
            }
        )));
    }

    #[test]
    fn two_devices_without_axes_that_point_at_each_other_take_number_0() {
        let (mut input, mut sink) = (input(), Recorded::default());
        // Each one's neighbour is a joystick by its sources, with the same
        // descriptor and another name, and has no axes either: the search
        // for the neighbour's number would go round and round.
        let half = |id: i32, name: &str| Device {
            id,
            name: name.into(),
            descriptor: "one-controller".into(),
            external: true,
            sources: source::JOYSTICK | source::GAMEPAD,
            keys: vec![code::BUTTON_A],
            ..Default::default()
        };
        let mut devices = Known::with([half(30, "left half"), half(31, "right half")]);
        press(
            &mut input,
            &mut devices,
            &mut sink,
            30,
            code::BUTTON_A,
            304,
            1000,
        );
        let packets = sink.take();
        assert!(
            packets.contains(&Packet::Controller(controller(0, 0, button::A))),
            "{packets:?}"
        );
    }

    #[test]
    fn guide_reaches_the_host_without_the_overlay() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        press(
            &mut input,
            &mut devices,
            &mut sink,
            16,
            code::BUTTON_MODE,
            316,
            1000,
        );
        let packets = sink.take();
        assert_eq!(
            packets[1],
            Packet::Controller(controller(0, 1, button::SPECIAL))
        );
    }

    #[test]
    fn back_on_a_gamepad_is_select() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        let (down, _) = press(
            &mut input,
            &mut devices,
            &mut sink,
            16,
            code::BACK,
            158,
            1000,
        );
        assert_eq!(down, Outcome::Handled);
        assert_eq!(
            sink.take()[1],
            Packet::Controller(controller(0, 1, button::BACK))
        );
    }

    #[test]
    fn a_short_press_is_held_for_25_ms() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        input.key(
            &key(16, code::BUTTON_A, 304, KeyAction::Down, 1000, 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        let began = std::time::Instant::now();
        input.key(
            &key(16, code::BUTTON_A, 304, KeyAction::Up, 1000, 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        assert!(began.elapsed() >= std::time::Duration::from_millis(24));
    }

    #[test]
    fn a_press_with_a_strange_down_time_holds_no_longer_than_25_ms() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        // A down time after the event's own time: the press would seem to
        // have lasted -1 s.
        input.key(
            &key(16, code::BUTTON_A, 304, KeyAction::Down, 1000, 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        let began = std::time::Instant::now();
        input.key(
            &key(16, code::BUTTON_A, 304, KeyAction::Up, 1000, 2000),
            1000,
            &mut devices,
            &mut sink,
        );
        let held = began.elapsed();
        assert!(held <= std::time::Duration::from_millis(60), "{held:?}");
    }

    #[test]
    fn the_heartbeat_repeats_the_last_state_and_a_removed_pad_lets_go() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        input.key(
            &key(16, code::BUTTON_A, 304, KeyAction::Down, 1000, 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        sink.take();
        let mut now = 1000;
        input.tick(now, &mut sink);
        assert_eq!(input.next_deadline(), Some(now + HEARTBEAT_MS));
        now += HEARTBEAT_MS;
        input.tick(now, &mut sink);
        assert_eq!(
            sink.take(),
            [Packet::Controller(controller(0, 1, button::A))]
        );
        // The pad goes: its number is free, and the host sees it let go,
        // then four more times.
        devices.devices.clear();
        input.check_devices(&mut devices, &mut sink);
        assert_eq!(sink.take(), [Packet::Controller(controller(0, 0, 0))]);
        for _ in 0..6 {
            now += HEARTBEAT_MS;
            input.tick(now, &mut sink);
        }
        assert_eq!(
            sink.take(),
            vec![Packet::Controller(controller(0, 0, 0)); 4]
        );
        assert_eq!(input.next_deadline(), None);
    }

    #[test]
    fn the_heartbeat_has_no_deadline_before_its_first_tick() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        input.key(
            &key(16, code::BUTTON_A, 304, KeyAction::Down, 1000, 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        // The app waits `deadline - now`: a moment far in the past would
        // overflow it.
        let deadline = input.next_deadline();
        assert!(
            deadline.is_none_or(|at| at.checked_sub(1000).is_some()),
            "{deadline:?}"
        );
        input.tick(1000, &mut sink);
        assert_eq!(input.next_deadline(), Some(1000 + HEARTBEAT_MS));
    }

    #[test]
    fn a_device_list_that_cannot_be_read_releases_no_controller() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        input.key(
            &key(16, code::BUTTON_A, 304, KeyAction::Down, 1000, 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        sink.take();
        devices.unreadable = true;
        input.check_devices(&mut devices, &mut sink);
        assert_eq!(sink.take(), [], "the pad was let go");
        // It still holds its number and its button.
        input.tick(1000, &mut sink);
        input.tick(1000 + HEARTBEAT_MS, &mut sink);
        assert_eq!(
            sink.take(),
            [Packet::Controller(controller(0, 1, button::A))]
        );
    }

    #[test]
    fn start_held_turns_the_pad_into_a_mouse() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        input.key(
            &key(16, code::BUTTON_START, 315, KeyAction::Down, 1000, 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        input.key(
            &key(16, code::BUTTON_START, 315, KeyAction::Up, 1800, 1000),
            1800,
            &mut devices,
            &mut sink,
        );
        sink.take();
        // The left stick moves the mouse; A is the left button.
        input.motion(
            &joystick(16, &[(axis::X, 1.0)], 1810),
            1810,
            &mut devices,
            &mut sink,
        );
        input.key(
            &key(16, code::BUTTON_A, 304, KeyAction::Down, 1820, 1820),
            1820,
            &mut devices,
            &mut sink,
        );
        input.tick(1860, &mut sink);
        let packets = sink.take();
        assert!(packets.contains(&Packet::MouseButton {
            pressed: true,
            button: MOUSE_LEFT
        }));
        assert!(
            packets.contains(&Packet::MouseMove { dx: 64, dy: 0 }),
            "{packets:?}"
        );
        // The controller itself stays still while it is a mouse.
        assert!(
            packets.iter().all(
                |p| !matches!(p, Packet::Controller(s) if s.buttons != 0 || s.left_stick_x != 0)
            )
        );
    }

    #[test]
    fn without_mouse_emulation_start_is_only_start() {
        let mut input = Input::new(
            Settings {
                mouse_emulation: false,
            },
            0,
            VIDEO,
            seats().0,
        );
        let mut sink = Recorded::default();
        let mut devices = Known::with([pad(16, 0x045e)]);
        input.key(
            &key(16, code::BUTTON_START, 315, KeyAction::Down, 1000, 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        input.key(
            &key(16, code::BUTTON_START, 315, KeyAction::Up, 1800, 1000),
            1800,
            &mut devices,
            &mut sink,
        );
        input.motion(
            &joystick(16, &[(axis::X, 1.0)], 1810),
            1810,
            &mut devices,
            &mut sink,
        );
        assert!(
            sink.take()
                .iter()
                .all(|p| !matches!(p, Packet::MouseMove { .. }))
        );
    }

    fn touch_event(action: u32, index: usize, points: &[(f32, f32)], time_ms: i64) -> MotionEvent {
        MotionEvent {
            device: 3,
            source: source::TOUCHSCREEN,
            action,
            action_index: index,
            event_time_ms: time_ms,
            pointers: points
                .iter()
                .enumerate()
                .map(|(id, &(x, y))| Pointer {
                    id: id as i32,
                    tool_type: tool::FINGER,
                    x,
                    y,
                    pressure: 1.0,
                    touch_major: 10.0,
                    touch_minor: 10.0,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_tap_is_a_touch_in_the_picture() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([touchscreen(3)]);
        // The middle of the picture, and a point in the bar above it.
        assert!(input.motion(
            &touch_event(action::DOWN, 0, &[(540.0, 514.0)], 1000),
            1000,
            &mut devices,
            &mut sink
        ));
        input.motion(
            &touch_event(action::UP, 0, &[(540.0, 100.0)], 1050),
            1050,
            &mut devices,
            &mut sink,
        );
        let packets = sink.take();
        let Packet::Touch {
            kind,
            x,
            y,
            rotation,
            ..
        } = packets[0]
        else {
            panic!("{packets:?}")
        };
        assert_eq!((kind, rotation), (kind::DOWN, 0xffff));
        assert!((x - 0.5).abs() < 1e-6 && (y - 0.5).abs() < 1e-6);
        let Packet::Touch { kind, y, .. } = packets[1] else {
            panic!("{packets:?}")
        };
        assert_eq!((kind, y), (kind::UP, 0.0));
    }

    #[test]
    fn without_touch_on_the_host_a_tap_clicks_where_it_was() {
        let (mut input, mut sink) = (
            input(),
            Recorded {
                touch: false,
                ..Default::default()
            },
        );
        let mut devices = Known::with([touchscreen(3)]);
        input.motion(
            &touch_event(action::DOWN, 0, &[(540.0, 514.0)], 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        input.motion(
            &touch_event(action::UP, 0, &[(540.0, 514.0)], 1040),
            1040,
            &mut devices,
            &mut sink,
        );
        assert_eq!(
            sink.take(),
            [
                Packet::MousePosition {
                    x: 540,
                    y: 304,
                    width: 1080,
                    height: 608
                },
                Packet::MouseButton {
                    pressed: true,
                    button: MOUSE_LEFT
                },
            ]
        );
        assert_eq!(input.next_deadline(), Some(1140));
        input.tick(1140, &mut sink);
        assert_eq!(
            sink.take(),
            [Packet::MouseButton {
                pressed: false,
                button: MOUSE_LEFT
            }]
        );
    }

    #[test]
    fn without_touch_on_the_host_a_long_press_is_a_right_click() {
        let (mut input, mut sink) = (
            input(),
            Recorded {
                touch: false,
                ..Default::default()
            },
        );
        let mut devices = Known::with([touchscreen(3)]);
        input.motion(
            &touch_event(action::DOWN, 0, &[(100.0, 300.0)], 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        input.tick(1100, &mut sink);
        input.tick(1650, &mut sink);
        input.motion(
            &touch_event(action::UP, 0, &[(100.0, 300.0)], 1700),
            1700,
            &mut devices,
            &mut sink,
        );
        assert_eq!(
            sink.take(),
            [
                Packet::MousePosition {
                    x: 100,
                    y: 90,
                    width: 1080,
                    height: 608
                },
                Packet::MouseButton {
                    pressed: true,
                    button: MOUSE_LEFT
                },
                Packet::MouseButton {
                    pressed: false,
                    button: MOUSE_LEFT
                },
                Packet::MouseButton {
                    pressed: true,
                    button: MOUSE_RIGHT
                },
                Packet::MouseButton {
                    pressed: false,
                    button: MOUSE_RIGHT
                },
            ]
        );
    }

    #[test]
    fn a_mouse_points_into_the_picture_and_clicks() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::default();
        let mut event = MotionEvent {
            device: 5,
            source: source::MOUSE,
            action: action::HOVER_MOVE,
            pointers: vec![Pointer {
                tool_type: tool::MOUSE,
                x: 2000.0,
                y: 300.0,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(input.motion(&event, 1000, &mut devices, &mut sink));
        event.action = action::BUTTON_PRESS;
        event.button_state = 0x01;
        input.motion(&event, 1010, &mut devices, &mut sink);
        event.action = action::SCROLL;
        event.button_state = 0;
        event.axes[axis::VSCROLL as usize] = -1.0;
        input.motion(&event, 1020, &mut devices, &mut sink);
        let position = Packet::MousePosition {
            x: 1080,
            y: 90,
            width: 1080,
            height: 608,
        };
        assert_eq!(
            sink.take(),
            [
                position,
                position,
                Packet::MouseButton {
                    pressed: true,
                    button: MOUSE_LEFT
                },
                position,
                Packet::Scroll { amount: -120 },
                Packet::HScroll { amount: 0 },
                Packet::MouseButton {
                    pressed: false,
                    button: MOUSE_LEFT
                },
            ]
        );
    }

    #[test]
    fn the_report_counts_what_went() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([keyboard_device(-1), pad(16, 0x045e)]);
        press(&mut input, &mut devices, &mut sink, -1, code::A, 30, 1000);
        press(
            &mut input,
            &mut devices,
            &mut sink,
            16,
            code::BUTTON_A,
            304,
            2000,
        );
        assert_eq!(
            input.report(),
            "2 keys, 2 controller states, 1 arrivals, 0 mouse, 0 touches sent; 0 held; 0 unsupported, 0 outside the stream, 0 failed"
        );
    }

    /// A sink whose send takes 50 ms, and which notes a send that runs, or
    /// is still running, once the gate has closed.
    struct Slow {
        closed: Arc<AtomicBool>,
        after_close: Arc<AtomicBool>,
        sent: Arc<AtomicUsize>,
    }

    impl Sink for Slow {
        fn send(&mut self, _: Packet) -> i32 {
            let late = || self.closed.load(Ordering::SeqCst);
            if late() {
                self.after_close.store(true, Ordering::SeqCst);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            if late() {
                self.after_close.store(true, Ordering::SeqCst);
            }
            self.sent.fetch_add(1, Ordering::SeqCst);
            0
        }
    }

    #[test]
    fn no_send_reaches_the_core_once_the_gate_has_closed() {
        let gate = Gate::closed();
        let closed = Arc::new(AtomicBool::new(false));
        let after_close = Arc::new(AtomicBool::new(false));
        let sent = Arc::new(AtomicUsize::new(0));
        let packet = Packet::Controller(controller(0, 1, 0));
        let mut early = Gated {
            gate: &gate,
            sink: Recorded::default(),
        };
        // Before the connection starts, nothing passes.
        assert_eq!(early.send(packet), CLOSED);
        assert!(early.sink.packets.is_empty());
        gate.open();
        std::thread::scope(|scope| {
            // The app's thread: its heartbeat sends on and on.
            scope.spawn(|| {
                let mut sink = Gated {
                    gate: &gate,
                    sink: Slow {
                        closed: closed.clone(),
                        after_close: after_close.clone(),
                        sent: sent.clone(),
                    },
                };
                for _ in 0..10 {
                    sink.send(packet);
                }
            });
            // The viewer's thread, in the middle of a send: the stream ends,
            // and the core may stop once `close` returns. On a loaded
            // builder the app's thread can start late: wait for its sends,
            // not for a fixed time.
            let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while sent.load(Ordering::SeqCst) < 2 && std::time::Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            gate.close();
            closed.store(true, Ordering::SeqCst);
        });
        let sent = sent.load(Ordering::SeqCst);
        assert!(sent >= 2, "sends passed while open");
        // The stop waits for the send in progress, not for every send that
        // the app's thread would make: no send starves it.
        assert!(sent < 10, "the gate closed only after all {sent} sends");
        assert!(
            !after_close.load(Ordering::SeqCst),
            "a send ran into the core after the gate closed"
        );
    }

    fn mouse_button(time_ms: i64) -> MotionEvent {
        MotionEvent {
            device: 5,
            source: source::MOUSE,
            action: action::BUTTON_PRESS,
            button_state: 0x01,
            event_time_ms: time_ms,
            pointers: vec![Pointer {
                tool_type: tool::MOUSE,
                x: 540.0,
                y: 514.0,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn the_hold_lets_go_of_everything_the_host_holds_down() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([keyboard_device(-1), pad(16, 0x045e), touchscreen(3)]);
        input.key(
            &key(16, code::BUTTON_A, 304, KeyAction::Down, 1000, 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        input.key(
            &key(-1, code::A + 10, 37, KeyAction::Down, 1010, 1010),
            1010,
            &mut devices,
            &mut sink,
        );
        input.motion(&mouse_button(1020), 1020, &mut devices, &mut sink);
        input.motion(
            &touch_event(action::DOWN, 0, &[(540.0, 514.0)], 1030),
            1030,
            &mut devices,
            &mut sink,
        );
        sink.take();
        input.hold(&mut sink);
        assert_eq!(
            sink.take(),
            [
                Packet::Controller(controller(0, 1, 0)),
                // K, with the flags it went down with.
                Packet::Keyboard {
                    code: 0x804bu16 as i16,
                    down: false,
                    modifiers: 0,
                    flags: 1
                },
                Packet::MouseButton {
                    pressed: false,
                    button: MOUSE_LEFT
                },
                Packet::Touch {
                    kind: kind::CANCEL_ALL,
                    pointer: 0,
                    x: 0.0,
                    y: 0.0,
                    pressure: 0.0,
                    contact_major: 0.0,
                    contact_minor: 0.0,
                    rotation: 0
                },
            ]
        );
        // Held already: nothing more.
        input.hold(&mut sink);
        assert!(sink.take().is_empty());
    }

    #[test]
    fn while_held_only_a_controllers_arrival_reaches_the_stream() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([keyboard_device(-1), pad(16, 0x045e)]);
        press(
            &mut input,
            &mut devices,
            &mut sink,
            16,
            code::BUTTON_A,
            304,
            1000,
        );
        input.hold(&mut sink);
        sink.take();
        // The heartbeat, a key, the pad's buttons, a new pad and the first
        // pad going: only the new pad's arrival goes.
        input.tick(2000, &mut sink);
        input.tick(2000 + HEARTBEAT_MS, &mut sink);
        press(&mut input, &mut devices, &mut sink, -1, code::A, 30, 3000);
        press(
            &mut input,
            &mut devices,
            &mut sink,
            16,
            code::BUTTON_B,
            305,
            3100,
        );
        devices.devices.insert(18, Rc::new(pad(18, 0x054c)));
        press(
            &mut input,
            &mut devices,
            &mut sink,
            18,
            code::BUTTON_A,
            304,
            3200,
        );
        devices.devices.remove(&16);
        input.present(&[-1, 18], &mut sink);
        let packets = sink.take();
        assert!(
            matches!(packets.as_slice(), [Packet::Arrival { number: 1, .. }]),
            "{packets:?}"
        );
        assert_eq!(
            input.report(),
            "0 keys, 3 controller states, 2 arrivals, 0 mouse, 0 touches sent; 8 held; 0 unsupported, 0 outside the stream, 0 failed"
        );
    }

    #[test]
    fn a_new_picture_place_keeps_the_hold_and_takes_the_touches_after_it() {
        // The sender's new size (#476) while the overlay holds the input:
        // the app places the picture again.
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([keyboard_device(-1), touchscreen(3)]);
        input.hold(&mut sink);
        let portrait = Rect {
            left: 236,
            top: 0,
            right: 844,
            bottom: 1080,
        };
        input.place(portrait);
        // Still held: a key goes nowhere.
        press(&mut input, &mut devices, &mut sink, -1, code::A, 30, 1000);
        assert!(sink.take().is_empty());
        input.release(&mut sink);
        sink.take();
        // A quarter across and three quarters down the new picture.
        input.motion(
            &touch_event(action::DOWN, 0, &[(388.0, 810.0)], 2000),
            2000,
            &mut devices,
            &mut sink,
        );
        let packets = sink.take();
        let Packet::Touch { kind, x, y, .. } = packets[0] else {
            panic!("{packets:?}")
        };
        assert_eq!(kind, kind::DOWN);
        assert!(
            (x - 0.25).abs() < 1e-3 && (y - 0.75).abs() < 1e-3,
            "{x}, {y}"
        );
    }

    #[test]
    fn after_the_hold_the_mapping_starts_clean() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([keyboard_device(-1), pad(16, 0x045e)]);
        // A and Shift are down when the overlay opens; their ups go to the
        // overlay, never to the stream's input.
        input.key(
            &key(16, code::BUTTON_A, 304, KeyAction::Down, 1000, 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        input.key(
            &key(-1, code::SHIFT_LEFT, 42, KeyAction::Down, 1010, 1010),
            1010,
            &mut devices,
            &mut sink,
        );
        input.hold(&mut sink);
        input.release(&mut sink);
        sink.take();
        // The heartbeat repeats the pad's state with nothing pressed.
        input.tick(2000, &mut sink);
        input.tick(2000 + HEARTBEAT_MS, &mut sink);
        assert_eq!(sink.take(), [Packet::Controller(controller(0, 1, 0))]);
        // B alone, not A and B; a key with no Shift.
        input.key(
            &key(16, code::BUTTON_B, 305, KeyAction::Down, 3000, 3000),
            3000,
            &mut devices,
            &mut sink,
        );
        input.key(
            &key(-1, code::A, 30, KeyAction::Down, 3010, 3010),
            3010,
            &mut devices,
            &mut sink,
        );
        assert_eq!(
            sink.take(),
            [
                Packet::Controller(controller(0, 1, button::B)),
                Packet::Keyboard {
                    code: 0x8041u16 as i16,
                    down: true,
                    modifiers: 0,
                    flags: 1
                },
            ]
        );
    }

    #[test]
    fn a_quick_second_start_held_opens_the_overlay() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        let start = |action, at, down| key(16, code::BUTTON_START, 315, action, at, down);
        input.key(
            &start(KeyAction::Down, 1000, 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        input.key(
            &start(KeyAction::Up, 1050, 1000),
            1050,
            &mut devices,
            &mut sink,
        );
        // Down again within 200 ms of the up, then held for 800 ms.
        input.key(
            &start(KeyAction::Down, 1200, 1200),
            1200,
            &mut devices,
            &mut sink,
        );
        sink.take();
        assert_eq!(
            input.key(
                &start(KeyAction::Up, 2000, 1200),
                2000,
                &mut devices,
                &mut sink
            ),
            Outcome::Overlay
        );
        // The hold that follows lets go of Start; the up itself sends
        // nothing, and the pad does not become a mouse.
        assert!(sink.take().is_empty());
        input.motion(
            &joystick(16, &[(axis::X, 1.0)], 2010),
            2010,
            &mut devices,
            &mut sink,
        );
        input.tick(2100, &mut sink);
        assert!(
            sink.take()
                .iter()
                .all(|p| !matches!(p, Packet::MouseMove { .. }))
        );
    }

    #[test]
    fn a_slow_second_start_held_still_turns_the_pad_into_a_mouse() {
        let (mut input, mut sink) = (input(), Recorded::default());
        let mut devices = Known::with([pad(16, 0x045e)]);
        let start = |action, at, down| key(16, code::BUTTON_START, 315, action, at, down);
        input.key(
            &start(KeyAction::Down, 1000, 1000),
            1000,
            &mut devices,
            &mut sink,
        );
        input.key(
            &start(KeyAction::Up, 1050, 1000),
            1050,
            &mut devices,
            &mut sink,
        );
        input.key(
            &start(KeyAction::Down, 1300, 1300),
            1300,
            &mut devices,
            &mut sink,
        );
        assert_eq!(
            input.key(
                &start(KeyAction::Up, 2100, 1300),
                2100,
                &mut devices,
                &mut sink
            ),
            Outcome::Handled
        );
        sink.take();
        input.motion(
            &joystick(16, &[(axis::X, 1.0)], 2110),
            2110,
            &mut devices,
            &mut sink,
        );
        input.tick(2200, &mut sink);
        assert!(
            sink.take()
                .iter()
                .any(|p| matches!(p, Packet::MouseMove { .. }))
        );
    }

    /// The arrival of a controller that a test presses A on, and what the
    /// input told the feedback.
    fn arrival_of(device: Device, phone: Phone) -> (Packet, Rc<RefCell<SeatLog>>) {
        let (seats, log) = seats();
        let id = device.id;
        let mut input = Input::new(
            Settings {
                mouse_emulation: true,
            },
            0,
            VIDEO,
            seats,
        );
        let mut sink = Recorded::default();
        let mut devices = Known::with([device]);
        devices.phone = phone;
        press(
            &mut input,
            &mut devices,
            &mut sink,
            id,
            code::BUTTON_A,
            304,
            1000,
        );
        let arrival = sink
            .take()
            .into_iter()
            .find(|p| matches!(p, Packet::Arrival { .. }))
            .expect("an arrival");
        (arrival, log)
    }

    fn capabilities(packet: &Packet) -> u16 {
        match packet {
            Packet::Arrival { capabilities, .. } => *capabilities,
            _ => panic!("{packet:?}"),
        }
    }

    const ANDROID_12: Phone = Phone {
        api_level: 31,
        vibration: Vibration {
            manager: Vibrators {
                count: 0,
                amplitude: false,
            },
            vibrator: crate::feedback::Vibrator {
                present: false,
                amplitude: false,
            },
        },
    };

    fn two_vibrators() -> Vibration {
        Vibration {
            manager: Vibrators {
                count: 2,
                amplitude: true,
            },
            ..Default::default()
        }
    }

    #[test]
    fn a_dualshock_4_arrives_with_rumble_battery_and_light_and_takes_its_seat() {
        let mut ds4 = dualshock(20);
        ds4.vibration = two_vibrators();
        ds4.rgb_light = true;
        let (arrival, log) = arrival_of(ds4, ANDROID_12);
        assert_eq!(
            capabilities(&arrival),
            CAPABILITY_ANALOG_TRIGGERS
                | CAPABILITY_RUMBLE
                | CAPABILITY_BATTERY_STATE
                | CAPABILITY_RGB_LED
        );
        assert_eq!(
            log.borrow().seated,
            [Seat {
                device: 20,
                name: "Sony Interactive Entertainment Wireless Controller".into(),
                number: 0,
                rumble: Rumble::Dual(Owner::Device),
                light: true,
            }]
        );
    }

    #[test]
    fn an_rgb_light_is_claimed_for_others_than_playstation_pads_from_android_14() {
        // Before Android 14 every light said it had RGB control.
        let mut xbox = pad(16, 0x045e);
        xbox.rgb_light = true;
        let (arrival, log) = arrival_of(xbox.clone(), ANDROID_12);
        assert_eq!(capabilities(&arrival) & CAPABILITY_RGB_LED, 0);
        // The light still takes the host's colour, as in Artemis.
        assert!(log.borrow().seated[0].light);
        let android_14 = Phone {
            api_level: 34,
            ..ANDROID_12
        };
        let (arrival, _) = arrival_of(xbox, android_14);
        assert_ne!(capabilities(&arrival) & CAPABILITY_RGB_LED, 0);
    }

    #[test]
    fn four_vibrators_claim_trigger_rumble() {
        let mut pad = pad(16, 0x045e);
        pad.vibration.manager = Vibrators {
            count: 4,
            amplitude: true,
        };
        let (arrival, log) = arrival_of(pad, ANDROID_12);
        assert_eq!(
            capabilities(&arrival) & (CAPABILITY_RUMBLE | CAPABILITY_TRIGGER_RUMBLE),
            CAPABILITY_RUMBLE | CAPABILITY_TRIGGER_RUMBLE
        );
        assert_eq!(log.borrow().seated[0].rumble, Rumble::Quad(Owner::Device));
    }

    #[test]
    fn built_in_buttons_rumble_with_the_phone_and_claim_no_battery() {
        let mut buttons = pad(2, 0);
        buttons.external = false;
        let phone = Phone {
            api_level: 31,
            vibration: Vibration {
                vibrator: crate::feedback::Vibrator {
                    present: true,
                    amplitude: true,
                },
                ..Default::default()
            },
        };
        let (arrival, log) = arrival_of(buttons, phone);
        assert_eq!(
            capabilities(&arrival),
            CAPABILITY_ANALOG_TRIGGERS | CAPABILITY_RUMBLE
        );
        assert_eq!(
            log.borrow().seated[0].rumble,
            Rumble::Single {
                owner: Owner::Phone,
                amplitude: true
            }
        );
    }

    #[test]
    fn the_feedback_hears_of_a_seated_pad_that_goes() {
        let (seats, log) = seats();
        let mut input = Input::new(
            Settings {
                mouse_emulation: true,
            },
            0,
            VIDEO,
            seats,
        );
        let mut sink = Recorded::default();
        let mut devices = Known::with([pad(16, 0x045e), pad(17, 0x045e)]);
        press(
            &mut input,
            &mut devices,
            &mut sink,
            16,
            code::BUTTON_A,
            304,
            1000,
        );
        devices.devices.clear();
        input.check_devices(&mut devices, &mut sink);
        // Only the pad that had a seat: the other never sent.
        assert_eq!(log.borrow().left, [16]);
        assert_eq!(log.borrow().seated.len(), 1);
    }

    #[test]
    fn a_battery_report_goes_to_the_core_as_its_own_packet() {
        let mut sink = Recorded::default();
        let packet = Packet::Battery {
            number: 1,
            state: 2,
            percentage: 55,
        };
        assert_eq!(sink.send(packet), 0);
        assert_eq!(sink.packets, [packet]);
    }

    #[test]
    fn the_launch_mask_counts_the_joysticks() {
        let mut devices = Known::with([
            keyboard_device(-1),
            pad(16, 0x045e),
            touchscreen(3),
            pad(18, 0x054c),
        ]);
        assert_eq!(attached(&mut devices), 0b11);
    }
}
