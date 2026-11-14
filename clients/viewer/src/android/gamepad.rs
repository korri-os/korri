// SPDX-License-Identifier: GPL-3.0-only
//! Game controllers: Android's key and axis events of each device become
//! the stream's controller packets. Pure; `input.rs` routes the events and
//! sends the packets.
//!
//! Ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`, a fork of
//! `ClassicOldSong/moonlight-android`, itself a fork of
//! `moonlight-stream/moonlight-android`): `ControllerHandler.java` (the
//! device contexts, the axes, deadzones and buttons, the remapping, the
//! controller numbers, the arrival and the mouse emulation) and
//! `ControllerHeartbeat.java` (Korri's own addition to that copy). Artemis
//! ships under the GNU GPL version 3 with no "or later" grant, so this file
//! does too. The Rust adaptation is Korri work dated 2026-11-14.
//!
//! Left out, each named in `clients/viewer/README.md`: rumble, battery,
//! the RGB light, motion sensors, a controller's own touchpad, Artemis's USB
//! driver (ADR 0064), the overlay's Guide and Start menus (slice D5), and
//! the settings that Korri's launch does not set (flip face buttons, back
//! as Guide, the Joy-Con fix).
use super::{
    controller_list,
    input::{
        ControllerState as State, Device, Devices, KeyAction, KeyEvent, MotionEvent, Packet, Sink,
        axis, everything, source,
    },
    keyboard::code,
};
use crate::log;
use std::collections::BTreeMap;

/// The stream's button bits (`Limelight.h`).
pub(crate) mod button {
    pub const UP: u32 = 0x0001;
    pub const DOWN: u32 = 0x0002;
    pub const LEFT: u32 = 0x0004;
    pub const RIGHT: u32 = 0x0008;
    pub const PLAY: u32 = 0x0010;
    pub const BACK: u32 = 0x0020;
    pub const LS_CLK: u32 = 0x0040;
    pub const RS_CLK: u32 = 0x0080;
    pub const LB: u32 = 0x0100;
    pub const RB: u32 = 0x0200;
    pub const SPECIAL: u32 = 0x0400;
    pub const A: u32 = 0x1000;
    pub const B: u32 = 0x2000;
    pub const X: u32 = 0x4000;
    pub const Y: u32 = 0x8000;
    pub const PADDLE1: u32 = 0x010000;
    pub const PADDLE2: u32 = 0x020000;
    pub const PADDLE3: u32 = 0x040000;
    pub const PADDLE4: u32 = 0x080000;
    pub const TOUCHPAD: u32 = 0x100000;
    pub const MISC: u32 = 0x200000;
}

/// The stream's controller capabilities (`Limelight.h`, `LI_CCAP_*`).
pub(crate) const CAPABILITY_ANALOG_TRIGGERS: u16 = 0x01;

/// The mouse buttons (`Limelight.h`).
pub(crate) const MOUSE_LEFT: u8 = 0x01;
pub(crate) const MOUSE_RIGHT: u8 = 0x03;

/// The protocol's limit: the bits of the active controller mask.
const MAX_GAMEPADS: u16 = 16;
/// Artemis's default stick deadzone: 5 %.
pub(crate) const STICK_DEADZONE: f32 = 0.05;
const MAXIMUM_BUMPER_UP_DELAY_MS: i64 = 100;
const START_DOWN_TIME_MOUSE_MODE_MS: i64 = 750;
pub(crate) const MINIMUM_BUTTON_DOWN_TIME_MS: i64 = 25;
const MOUSE_EMULATION_PERIOD_MS: i64 = 50;
/// `ControllerHeartbeat`: the last state again every 250 ms, and a gone
/// controller's last state four more times. A Korri sender's input seat
/// (`services/inputd`) drops a source that is quiet for 1.25 s.
pub(crate) const HEARTBEAT_MS: i64 = 250;
const DISCONNECT_REPEATS: u8 = 4;

const EMULATING_SPECIAL: u8 = 0x1;
const EMULATING_SELECT: u8 = 0x2;

/// `ANDROID_TO_LI_BUTTON_MAP`: the buttons a controller says it has, for
/// its arrival.
const BUTTON_MAP: [(u32, u32); 23] = [
    (code::BUTTON_A, button::A),
    (code::BUTTON_B, button::B),
    (code::BUTTON_X, button::X),
    (code::BUTTON_Y, button::Y),
    (code::DPAD_UP, button::UP),
    (code::DPAD_DOWN, button::DOWN),
    (code::DPAD_LEFT, button::LEFT),
    (code::DPAD_RIGHT, button::RIGHT),
    (code::DPAD_UP_LEFT, button::UP | button::LEFT),
    (code::DPAD_UP_RIGHT, button::UP | button::RIGHT),
    (code::DPAD_DOWN_LEFT, button::DOWN | button::LEFT),
    (code::DPAD_DOWN_RIGHT, button::DOWN | button::RIGHT),
    (code::BUTTON_L1, button::LB),
    (code::BUTTON_R1, button::RB),
    (code::BUTTON_THUMBL, button::LS_CLK),
    (code::BUTTON_THUMBR, button::RS_CLK),
    (code::BUTTON_START, button::PLAY),
    (code::MENU, button::PLAY),
    (code::BUTTON_SELECT, button::BACK),
    (code::BACK, button::BACK),
    (code::BUTTON_MODE, button::SPECIAL),
    // The Xbox Series Share button.
    (code::MEDIA_RECORD, button::MISC),
    // The DualShock and DualSense touchpad button before Linux 4.10.
    (code::BUTTON_1, button::TOUCHPAD),
];

/// Every key code whose presence the controllers ask a device for.
pub(crate) fn probed_keys() -> Vec<u32> {
    let mut keys: Vec<u32> = BUTTON_MAP.iter().map(|&(key, _)| key).collect();
    keys.push(code::BUTTON_C);
    keys
}

/// A device with a joystick's X and Y axes (`hasJoystickAxes`).
pub(crate) fn has_joystick_axes(device: &Device) -> bool {
    device.sources & source::JOYSTICK == source::JOYSTICK
        && device.range(axis::X).is_some()
        && device.range(axis::Y).is_some()
}

/// A device with a gamepad's buttons (`hasGamepadButtons`).
pub(crate) fn has_gamepad_buttons(device: &Device) -> bool {
    device.sources & source::GAMEPAD == source::GAMEPAD
}

/// `isGameControllerDevice`: no device, a joystick, a gamepad, or anything
/// that is not a full keyboard.
pub(crate) fn is_game_controller(device: Option<&Device>) -> bool {
    device.is_none_or(|device| {
        has_joystick_axes(device)
            || has_gamepad_buttons(device)
            || device.keyboard_type != KEYBOARD_TYPE_ALPHABETIC
    })
}

pub(crate) const KEYBOARD_TYPE_ALPHABETIC: i32 = 2;

/// `getAttachedControllerMask`: a bit for each device with joystick axes,
/// in the order of the device list.
pub(crate) fn attached_mask<'a>(devices: impl IntoIterator<Item = &'a Device>) -> u16 {
    let count = devices
        .into_iter()
        .filter(|device| has_joystick_axes(device))
        .count()
        .min(usize::from(MAX_GAMEPADS));
    ((1u32 << count) - 1) as u16
}

/// One input device, as Artemis's `InputDeviceContext` keeps it.
#[derive(Debug, Default)]
struct Context {
    name: String,
    vendor: u16,
    product: u16,
    external: bool,
    has_joystick_axes: bool,

    left_deadzone: f32,
    right_deadzone: f32,
    trigger_deadzone: f32,

    assigned: bool,
    reserved: bool,
    number: u16,

    input_map: u32,
    left_trigger: u8,
    right_trigger: u8,
    left_stick_x: i16,
    left_stick_y: i16,
    right_stick_x: i16,
    right_stick_y: i16,

    left_stick: Option<(u32, u32)>,
    right_stick: Option<(u32, u32)>,
    triggers: Option<(u32, u32)>,
    triggers_idle_negative: bool,
    left_trigger_used: bool,
    right_trigger_used: bool,
    hat: bool,
    hat_x_used: bool,
    hat_y_used: bool,

    non_standard_dualshock4: bool,
    linux_face_buttons: bool,
    non_standard_xbox_bluetooth: bool,
    serval: bool,
    back_is_start: bool,
    mode_is_select: bool,
    search_is_mode: bool,
    ignore_back: bool,
    dualshock_standalone_touchpad: bool,

    emulating: u8,
    has_select: bool,
    has_mode: bool,
    has_paddles: bool,
    has_share: bool,

    last_lb_up_ms: i64,
    last_rb_up_ms: i64,
    start_down_ms: i64,
    start_up_ms: i64,

    mouse: MouseEmulation,
    /// The arrival: the device's kind, buttons and capabilities.
    arrival: Option<(u8, u32, u16)>,
}

#[derive(Debug, Default)]
struct MouseEmulation {
    active: bool,
    x_down: bool,
    multiplier: i32,
    last_input_map: u32,
    /// When the sticks move the mouse next.
    next_ms: i64,
}

/// What a controller did with a key.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Key {
    Handled,
    /// Not a controller key: the keyboard may take it.
    Unhandled,
}

/// Every controller of the stream: Artemis's `ControllerHandler`.
pub(crate) struct Controllers {
    contexts: BTreeMap<i32, Context>,
    /// Events with no device (ID 0).
    default: Context,
    current: u16,
    initial: u16,
    mouse_emulation: bool,
    heartbeat: Heartbeat,
}

impl Controllers {
    /// `initial`: the controllers attached when the stream started. Each
    /// takes its number out of it when it first sends.
    pub(crate) fn new(initial: u16, mouse_emulation: bool) -> Self {
        let default = Context {
            left_stick: Some((axis::X, axis::Y)),
            right_stick: Some((axis::Z, axis::RZ)),
            triggers: Some((axis::BRAKE, axis::GAS)),
            hat: true,
            left_deadzone: STICK_DEADZONE,
            right_deadzone: STICK_DEADZONE,
            assigned: true,
            // Some devices send a Back button with device ID 0: let it
            // through.
            ignore_back: true,
            mouse: MouseEmulation {
                multiplier: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        Self {
            contexts: BTreeMap::new(),
            default,
            current: 0,
            initial,
            mouse_emulation,
            heartbeat: Heartbeat::default(),
        }
    }

    fn active_mask(&self) -> u16 {
        self.current | self.initial
    }

    /// The context of an event's device; none while the device is gone.
    fn context_id(
        &mut self,
        device_id: i32,
        device: Option<&Device>,
        devices: &mut dyn Devices,
    ) -> Option<i32> {
        if device_id == 0 {
            return Some(0);
        }
        let device = device?;
        if let std::collections::btree_map::Entry::Vacant(entry) = self.contexts.entry(device_id) {
            // Every device, for the checks that look at the others.
            entry.insert(new_context(device, &everything(devices)));
        }
        Some(device_id)
    }

    fn context(&mut self, id: i32) -> &mut Context {
        if id == 0 {
            &mut self.default
        } else {
            self.contexts
                .get_mut(&id)
                .expect("a context that context_id made")
        }
    }

    /// `handleButtonDown` and `handleButtonUp`. `device` is the event's
    /// device; `all` every device, for the checks that look at the others.
    pub(crate) fn key(
        &mut self,
        event: &KeyEvent,
        device: Option<&Device>,
        devices: &mut dyn Devices,
        now_ms: i64,
        sink: &mut dyn Sink,
    ) -> Key {
        let Some(id) = self.context_id(event.device, device, devices) else {
            // A device that is gone: eat its events.
            return Key::Handled;
        };
        let mouse_emulation = self.mouse_emulation;
        let context = self.context(id);
        let key_code = match remap(context, event) {
            Remap::Ignore => return Key::Unhandled,
            Remap::Consume => return Key::Handled,
            Remap::Key(key_code) => key_code,
        };
        let press = match event.action {
            KeyAction::Down => button_down(context, key_code, event),
            KeyAction::Up => {
                // A press shorter than this is held a little: an instant
                // press and release would be lost by a host that polls.
                let held = event.event_time_ms - event.down_time_ms;
                if held < MINIMUM_BUTTON_DOWN_TIME_MS {
                    std::thread::sleep(std::time::Duration::from_millis(
                        (MINIMUM_BUTTON_DOWN_TIME_MS - held) as u64,
                    ));
                }
                button_up(context, key_code, event, mouse_emulation)
            }
            KeyAction::Multiple => Press::NotMine,
        };
        match press {
            Press::NotMine => return Key::Unhandled,
            Press::Eaten => return Key::Handled,
            Press::ToggleMouse => toggle_mouse(self.context(id), now_ms),
            Press::Send => {}
        }
        self.send(id, devices, sink);
        Key::Handled
    }

    /// `handleMotionEvent`: a joystick event's axes.
    pub(crate) fn motion(
        &mut self,
        event: &MotionEvent,
        device: Option<&Device>,
        devices: &mut dyn Devices,
        sink: &mut dyn Sink,
    ) {
        let Some(id) = self.context_id(event.device, device, devices) else {
            return;
        };
        let context = self.context(id);
        let value = |axis: u32| event.axis(axis);
        let pair =
            |axes: Option<(u32, u32)>| axes.map_or((0.0, 0.0), |(x, y)| (value(x), value(y)));
        let (lsx, lsy) = pair(context.left_stick);
        let (rsx, rsy) = pair(context.right_stick);
        let (lt, rt) = pair(context.triggers);
        let (hat_x, hat_y) = if context.hat {
            (value(axis::HAT_X), value(axis::HAT_Y))
        } else {
            (0.0, 0.0)
        };
        axis_set(context, [lsx, lsy, rsx, rsy, lt, rt, hat_x, hat_y]);
        self.send(id, devices, sink);
    }

    /// `onInputDeviceRemoved`: its number is free, and the host sees the
    /// controller let go of everything.
    pub(crate) fn removed(&mut self, device_id: i32, sink: &mut dyn Sink) {
        let Some(context) = self.contexts.remove(&device_id) else {
            return;
        };
        log::line(&format!(
            "input: controller {} ({device_id}) removed",
            context.name
        ));
        if context.reserved {
            log::line(&format!(
                "input: controller number {} is free",
                context.number
            ));
            self.current &= !(1 << context.number);
        }
        if context.assigned {
            let state = State {
                number: context.number,
                active_mask: self.active_mask(),
                ..Default::default()
            };
            self.send_state(state, sink);
        }
    }

    /// The device IDs that have a context.
    pub(crate) fn devices(&self) -> impl Iterator<Item = i32> + '_ {
        self.contexts.keys().copied()
    }

    /// `assignControllerNumberIfNeeded`.
    fn assign(&mut self, id: i32, devices: &mut dyn Devices, sink: &mut dyn Sink) {
        if self.context(id).assigned {
            return;
        }
        let (external, has_axes) = {
            let context = self.context(id);
            (context.external, context.has_joystick_axes)
        };
        let number = if !external {
            log::line("input: built-in buttons are controller 0");
            0
        } else if has_axes {
            match (0..MAX_GAMEPADS).find(|number| self.current & (1 << number) == 0) {
                Some(number) => {
                    self.current |= 1 << number;
                    self.initial &= !(1 << number);
                    self.context(id).reserved = true;
                    number
                }
                None => 0,
            }
        } else {
            // A device with no joystick axes may belong to one that has
            // them, as a DualShock 4's touchpad does: take its number.
            self.associated_number(id, devices, sink).unwrap_or(0)
        };
        let context = self.context(id);
        context.number = number;
        context.assigned = true;
        log::line(&format!(
            "input: {} ({id}) is controller {number}",
            context.name
        ));
        if let Some((kind, buttons, capabilities)) = context.arrival {
            let arrival = Packet::Arrival {
                number: number as u8,
                active_mask: self.active_mask(),
                kind,
                buttons,
                capabilities,
            };
            log::line(&format!("input: arrival {arrival:?}"));
            sink.send(arrival);
        }
    }

    /// The number of the joystick next to this device, `isAssociatedJoystick`:
    /// another name, the same descriptor.
    fn associated_number(
        &mut self,
        id: i32,
        devices: &mut dyn Devices,
        sink: &mut dyn Sink,
    ) -> Option<u16> {
        let all = everything(devices);
        let this = all.iter().find(|device| device.id == id)?;
        let other = [id + 1, id - 1].into_iter().find_map(|other| {
            all.iter().find(|device| {
                device.id == other
                    && device.sources & source::JOYSTICK == source::JOYSTICK
                    && device.name != this.name
                    && device.descriptor == this.descriptor
            })
        })?;
        let other_id = other.id;
        self.contexts
            .entry(other_id)
            .or_insert_with(|| new_context(other, &all));
        self.assign(other_id, devices, sink);
        Some(self.contexts[&other_id].number)
    }

    /// `sendControllerInputPacket`: every device with this number makes one
    /// controller, unless one of them emulates a mouse.
    fn send(&mut self, id: i32, devices: &mut dyn Devices, sink: &mut dyn Sink) {
        self.assign(id, devices, sink);
        let (number, mouse_active) = {
            let context = self.context(id);
            (context.number, context.mouse.active)
        };
        let mut state = State {
            number,
            active_mask: self.active_mask(),
            ..Default::default()
        };
        let fused = self
            .contexts
            .values()
            .filter(|c| c.assigned && c.number == number && c.mouse.active == mouse_active)
            .chain((self.default.number == number).then_some(&self.default));
        for c in fused {
            state.buttons |= c.input_map;
            // As Artemis does it: each value or-ed with the larger by
            // magnitude, triggers as signed bytes.
            state.left_trigger |= larger_byte(state.left_trigger, c.left_trigger);
            state.right_trigger |= larger_byte(state.right_trigger, c.right_trigger);
            state.left_stick_x |= larger(state.left_stick_x, c.left_stick_x);
            state.left_stick_y |= larger(state.left_stick_y, c.left_stick_y);
            state.right_stick_x |= larger(state.right_stick_x, c.right_stick_x);
            state.right_stick_y |= larger(state.right_stick_y, c.right_stick_y);
        }
        if mouse_active {
            let context = self.context(id);
            mouse_buttons(context, state.buttons, sink);
            state = State {
                number,
                active_mask: state.active_mask,
                ..Default::default()
            };
        }
        self.send_state(state, sink);
    }

    fn send_state(&mut self, state: State, sink: &mut dyn Sink) {
        self.heartbeat.record(state);
        sink.send(Packet::Controller(state));
    }

    /// The timers: each emulated mouse moves, and the heartbeat repeats.
    pub(crate) fn tick(&mut self, now_ms: i64, sink: &mut dyn Sink) {
        for context in self
            .contexts
            .values_mut()
            .chain(std::iter::once(&mut self.default))
        {
            if context.mouse.active && now_ms >= context.mouse.next_ms {
                context.mouse.next_ms = now_ms + MOUSE_EMULATION_PERIOD_MS;
                mouse_move(context, sink);
            }
        }
        self.heartbeat.tick(now_ms, sink);
    }

    /// The next moment that `tick` has work.
    pub(crate) fn next_deadline(&self) -> Option<i64> {
        let mouse = self
            .contexts
            .values()
            .chain(std::iter::once(&self.default))
            .filter(|context| context.mouse.active)
            .map(|context| context.mouse.next_ms)
            .min();
        [mouse, self.heartbeat.next_deadline()]
            .into_iter()
            .flatten()
            .min()
    }
}

fn larger(a: i16, b: i16) -> i16 {
    if (a as i32).abs() > (b as i32).abs() {
        a
    } else {
        b
    }
}

fn larger_byte(a: u8, b: u8) -> u8 {
    larger(i16::from(a as i8), i16::from(b as i8)) as u8
}

/// `createInputDeviceContextForDevice`.
fn new_context(device: &Device, all: &[Device]) -> Context {
    let name = device.name.clone();
    log::line(&format!(
        "input: controller {name} ({}) vendor {:04x} product {:04x} external {}",
        device.id, device.vendor, device.product, device.external
    ));
    let mut c = Context {
        name: name.clone(),
        vendor: device.vendor,
        product: device.product,
        external: is_external(device),
        has_paddles: controller_list::has_paddles(device.vendor, device.product),
        has_share: controller_list::has_share(device.vendor, device.product),
        has_mode: device.has_key(code::BUTTON_MODE),
        has_select: device.has_key(code::BUTTON_SELECT) || device.has_key(code::BACK),
        mouse: MouseEmulation {
            multiplier: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let range = |axis: u32| device.range(axis);
    if range(axis::X).is_some() && range(axis::Y).is_some() {
        c.has_joystick_axes = true;
    }
    c.left_stick = Some((axis::X, axis::Y));
    // The Nvidia Shield's DualShock 4 clickpad, a duplicate Select.
    c.dualshock_standalone_touchpad = device.vendor == 0x054c
        && name.ends_with(" Touchpad")
        && device.sources == source::KEYBOARD | source::MOUSE;
    let has = |a: u32, b: u32| range(a).is_some() && range(b).is_some();
    if has(axis::LTRIGGER, axis::RTRIGGER) {
        c.triggers = Some((axis::LTRIGGER, axis::RTRIGGER));
    } else if has(axis::BRAKE, axis::GAS) {
        c.triggers = Some((axis::BRAKE, axis::GAS));
    } else if has(axis::BRAKE, axis::THROTTLE) {
        c.triggers = Some((axis::BRAKE, axis::THROTTLE));
    } else if has(axis::RX, axis::RY) {
        if device.vendor == 0x054c {
            if device.has_key(code::BUTTON_C) {
                log::line("input: a DualShock 4 with the old mapping");
                c.non_standard_dualshock4 = true;
            } else {
                log::line("input: a DualShock 4 with Linux's mapping");
                c.linux_face_buttons = true;
            }
        }
        if c.non_standard_dualshock4 {
            // The old driver puts the triggers on RX and RY.
            c.triggers = Some((axis::RX, axis::RY));
            c.has_select = true;
            c.has_mode = true;
        } else {
            c.right_stick = Some((axis::RX, axis::RY));
            if has(axis::Z, axis::RZ) {
                c.triggers = Some((axis::Z, axis::RZ));
            }
        }
        c.triggers_idle_negative = true;
    }
    if c.right_stick.is_none() {
        if has(axis::Z, axis::RZ) {
            c.right_stick = Some((axis::Z, axis::RZ));
        } else if has(axis::RX, axis::RY) {
            c.right_stick = Some((axis::RX, axis::RY));
        }
    }
    c.hat = has(axis::HAT_X, axis::HAT_Y);
    c.left_deadzone = STICK_DEADZONE;
    if c.right_stick.is_some() {
        c.right_deadzone = STICK_DEADZONE;
    }
    if let Some((left, right)) = c.triggers {
        let flat = |axis: u32| range(axis).map_or(0.0, |range| range.flat.abs());
        // A valid deadzone keeps the packets from flapping; 13 % is about
        // XInput's own.
        c.trigger_deadzone = flat(left).max(flat(right));
        if !(0.13..=0.30).contains(&c.trigger_deadzone) {
            c.trigger_deadzone = 0.13;
        }
    }
    // The ADT-1 controller.
    if device.vendor == 0x18d1 && device.product == 0x2c40 {
        c.back_is_start = true;
        c.mode_is_select = true;
        c.trigger_deadzone = 0.30;
        c.has_select = true;
        c.has_mode = false;
    }
    c.ignore_back = ignore_back(device, all);
    if name.contains("ASUS Gamepad") {
        if !device.has_key(code::BUTTON_START) && !device.has_key(code::MENU) {
            c.back_is_start = true;
            c.mode_is_select = true;
            c.has_select = true;
            c.has_mode = false;
        }
        // Its triggers sit far forward.
        c.trigger_deadzone = 0.30;
    } else if name.contains("SHIELD") || name.contains("NVIDIA Controller") {
        if name.contains("NVIDIA Controller v01.03") || name.contains("NVIDIA Controller v01.04") {
            c.search_is_mode = true;
            c.has_mode = true;
        }
    } else if name.contains("Razer Serval") {
        c.serval = true;
        c.has_mode = true;
        c.has_select = true;
    } else if name == "Xbox Wireless Controller" && range(axis::GAS).is_none() {
        // The Xbox One S over Bluetooth before its firmware fix.
        c.non_standard_xbox_bluetooth = true;
        c.has_mode = true;
        c.has_select = true;
    }
    // The Thrustmaster Score A's home button comes on another device.
    if device.vendor == 0x044f && device.product == 0xb328 {
        c.has_mode = false;
    }
    c.arrival = Some(arrival(device, &c));
    log::line(&format!(
        "input: {name} sticks {:?} {:?} triggers {:?} hat {} deadzones {} {} {}",
        c.left_stick,
        c.right_stick,
        c.triggers,
        c.hat,
        c.left_deadzone,
        c.right_deadzone,
        c.trigger_deadzone
    ));
    c
}

/// `sendControllerArrival`: the device's kind, the buttons it has and its
/// capabilities. Rumble, battery, light, sensors and touchpad are not
/// claimed: the viewer does not carry them.
fn arrival(device: &Device, context: &Context) -> (u8, u32, u16) {
    let kind = match device.vendor {
        0x045e => controller_list::XBOX,
        0x054c => controller_list::PS,
        0x057e => controller_list::NINTENDO,
        _ => controller_list::kind(device.vendor, device.product),
    };
    let mut buttons = BUTTON_MAP
        .iter()
        .filter(|&&(key, _)| device.has_key(key))
        .fold(0, |buttons, &(_, flag)| buttons | flag);
    if context.has_paddles {
        buttons |= button::PADDLE1 | button::PADDLE2 | button::PADDLE3 | button::PADDLE4;
    }
    if context.has_share {
        buttons |= button::MISC;
    }
    if device.range(axis::HAT_X).is_some() {
        buttons |= button::LEFT | button::RIGHT;
    }
    if device.range(axis::HAT_Y).is_some() {
        buttons |= button::UP | button::DOWN;
    }
    let capabilities = if context.triggers.is_some() {
        CAPABILITY_ANALOG_TRIGGERS
    } else {
        0
    };
    (kind, buttons, capabilities)
}

/// `isExternal`, with Artemis's list of built-in controllers that Android
/// calls external.
pub(crate) fn is_external(device: &Device) -> bool {
    let name = &device.name;
    let built_in = name.contains("gpio")
        || name.contains("joy_key")
        || name.contains("keypad")
        || name.eq_ignore_ascii_case("NVIDIA Corporation NVIDIA Controller v01.01")
        || name.eq_ignore_ascii_case("NVIDIA Corporation NVIDIA Controller v01.02")
        || name.eq_ignore_ascii_case("GR0006");
    !built_in && device.external
}

/// `shouldIgnoreBack`: whether Back stays Android's own (it leaves the
/// stream) instead of the controller's Select.
fn ignore_back(device: &Device, all: &[Device]) -> bool {
    let name = &device.name;
    if name.contains("Razer Serval") {
        return true;
    }
    if !has_joystick_axes(device) && name.to_lowercase().contains("remote") {
        return true;
    }
    if !is_external(device) {
        // A phone's own Back, unless it has built-in gamepad buttons and
        // no built-in Select (a Shield Portable).
        let internal = all.iter().filter(|other| !is_external(other));
        let (mut gamepad, mut select) = (false, false);
        for other in internal {
            select |= other.has_key(code::BUTTON_SELECT);
            gamepad |= has_gamepad_buttons(other);
        }
        return !gamepad || select;
    }
    !has_joystick_axes(device) && !has_gamepad_buttons(device)
}

enum Remap {
    Ignore,
    Consume,
    Key(u32),
}

/// `handleRemapping`: each device's corrections, by its scan codes.
fn remap(c: &mut Context, event: &KeyEvent) -> Remap {
    let key = event.key_code;
    let scan = event.scan_code;
    if c.ignore_back && key == code::BACK {
        return Remap::Ignore;
    }
    if c.has_share && key == code::UNKNOWN && scan == 167 {
        return Remap::Key(code::MEDIA_RECORD);
    }
    // The Shield maps the DualShock 4 clickpad to Select.
    if c.vendor == 0x054c
        && key == code::BUTTON_SELECT
        && (scan == 317 || c.dualshock_standalone_touchpad)
    {
        return Remap::Key(code::BUTTON_1);
    }
    // 8BitDo's Mode button.
    if c.vendor == 0x2dc8 && scan == 306 {
        return Remap::Key(code::BUTTON_MODE);
    }
    // The HORIPAD for Switch.
    if c.vendor == 0x0f0d && c.product == 0x00c1 {
        let mapped = match scan {
            0x130 => Some(code::BUTTON_A),
            0x131 => Some(code::BUTTON_B),
            0x132 => Some(code::BUTTON_X),
            0x133 => Some(code::BUTTON_Y),
            0x134 => Some(code::BUTTON_L1),
            0x135 => Some(code::BUTTON_R1),
            0x136 => Some(code::BUTTON_L2),
            0x137 => Some(code::BUTTON_R2),
            0x138 => Some(code::BUTTON_SELECT),
            0x139 => Some(code::BUTTON_START),
            0x13a => Some(code::BUTTON_THUMBL),
            0x13b => Some(code::BUTTON_THUMBR),
            0x13d => Some(code::BUTTON_MODE),
            _ => None,
        };
        if let Some(mapped) = mapped {
            return Remap::Key(mapped);
        }
    }
    if c.linux_face_buttons {
        // Android's Generic.kl swaps BTN_NORTH and BTN_WEST.
        match scan {
            304 => return Remap::Key(code::BUTTON_A),
            305 => return Remap::Key(code::BUTTON_B),
            307 => return Remap::Key(code::BUTTON_Y),
            308 => return Remap::Key(code::BUTTON_X),
            _ => {}
        }
    }
    if c.non_standard_dualshock4 {
        return match scan {
            304 => Remap::Key(code::BUTTON_X),
            305 => Remap::Key(code::BUTTON_A),
            306 => Remap::Key(code::BUTTON_B),
            307 => Remap::Key(code::BUTTON_Y),
            308 => Remap::Key(code::BUTTON_L1),
            309 => Remap::Key(code::BUTTON_R1),
            312 => Remap::Key(code::BUTTON_SELECT),
            313 => Remap::Key(code::BUTTON_START),
            314 => Remap::Key(code::BUTTON_THUMBL),
            315 => Remap::Key(code::BUTTON_THUMBR),
            316 => Remap::Key(code::BUTTON_MODE),
            _ => Remap::Consume,
        };
    } else if c.serval && key == code::UNKNOWN {
        match scan {
            314 => return Remap::Key(code::BUTTON_SELECT),
            315 => return Remap::Key(code::BUTTON_START),
            _ => {}
        }
    } else if c.non_standard_xbox_bluetooth {
        let mapped = match scan {
            306 => Some(code::BUTTON_X),
            307 => Some(code::BUTTON_Y),
            308 => Some(code::BUTTON_L1),
            309 => Some(code::BUTTON_R1),
            310 => Some(code::BUTTON_SELECT),
            311 => Some(code::BUTTON_START),
            312 => Some(code::BUTTON_THUMBL),
            313 => Some(code::BUTTON_THUMBR),
            139 => Some(code::BUTTON_MODE),
            _ => None,
        };
        if let Some(mapped) = mapped {
            return Remap::Key(mapped);
        }
        if key == code::MENU {
            return Remap::Key(code::BUTTON_MODE);
        }
    } else if c.vendor == 0x0b05 && matches!(c.product, 0x7900 | 0x7902) {
        // The ROG Kunai's M1 to M4.
        match scan {
            264 | 266 => return Remap::Key(code::BUTTON_START),
            265 | 267 => return Remap::Key(code::BUTTON_SELECT),
            _ => {}
        }
    }
    if !c.hat && key == code::UNKNOWN {
        // An Xbox pad without a proper mapping sends its d-pad as raw scan
        // codes.
        match scan {
            704 => return Remap::Key(code::DPAD_LEFT),
            705 => return Remap::Key(code::DPAD_RIGHT),
            706 => return Remap::Key(code::DPAD_UP),
            707 => return Remap::Key(code::DPAD_DOWN),
            _ => {}
        }
    }
    let mut key = key;
    // The "Tablet Remote" app sends B as Back with Alt.
    if key == code::BACK && event.meta_state != 0 && event.flags & SOFT_KEYBOARD != 0 {
        key = code::BUTTON_B;
    }
    if key == code::BUTTON_START || key == code::MENU {
        c.back_is_start = false;
    } else if key == code::BUTTON_SELECT {
        c.mode_is_select = false;
    } else if c.back_is_start && key == code::BACK {
        return Remap::Key(code::BUTTON_START);
    } else if c.mode_is_select && key == code::BUTTON_MODE {
        return Remap::Key(code::BUTTON_SELECT);
    } else if c.search_is_mode && key == code::SEARCH {
        return Remap::Key(code::BUTTON_MODE);
    }
    Remap::Key(key)
}

const SOFT_KEYBOARD: u32 = 0x2;

/// The button bits of a key, and whether a hat or an analog trigger
/// already gives it.
enum Bits {
    Flags(u32),
    LeftTrigger,
    RightTrigger,
    Paddle(u32),
    None,
}

fn bits(c: &Context, key: u32, scan: i32) -> Option<Bits> {
    let hat_x = c.hat_x_used;
    let hat_y = c.hat_y_used;
    let hat = hat_x && hat_y;
    let bits = match key {
        code::BUTTON_MODE => Bits::Flags(button::SPECIAL),
        code::BUTTON_START | code::MENU => Bits::Flags(button::PLAY),
        code::BACK | code::BUTTON_SELECT => Bits::Flags(button::BACK),
        // A d-pad that a hat already reports sends nothing more.
        code::DPAD_LEFT if hat_x => Bits::None,
        code::DPAD_LEFT => Bits::Flags(button::LEFT),
        code::DPAD_RIGHT if hat_x => Bits::None,
        code::DPAD_RIGHT => Bits::Flags(button::RIGHT),
        code::DPAD_UP if hat_y => Bits::None,
        code::DPAD_UP => Bits::Flags(button::UP),
        code::DPAD_DOWN if hat_y => Bits::None,
        code::DPAD_DOWN => Bits::Flags(button::DOWN),
        code::DPAD_UP_LEFT if hat => Bits::None,
        code::DPAD_UP_LEFT => Bits::Flags(button::UP | button::LEFT),
        code::DPAD_UP_RIGHT if hat => Bits::None,
        code::DPAD_UP_RIGHT => Bits::Flags(button::UP | button::RIGHT),
        code::DPAD_DOWN_LEFT if hat => Bits::None,
        code::DPAD_DOWN_LEFT => Bits::Flags(button::DOWN | button::LEFT),
        code::DPAD_DOWN_RIGHT if hat => Bits::None,
        code::DPAD_DOWN_RIGHT => Bits::Flags(button::DOWN | button::RIGHT),
        code::BUTTON_B => Bits::Flags(button::B),
        code::DPAD_CENTER | code::BUTTON_A => Bits::Flags(button::A),
        code::BUTTON_X => Bits::Flags(button::X),
        code::BUTTON_Y => Bits::Flags(button::Y),
        code::BUTTON_L1 => Bits::Flags(button::LB),
        code::BUTTON_R1 => Bits::Flags(button::RB),
        code::BUTTON_THUMBL => Bits::Flags(button::LS_CLK),
        code::BUTTON_THUMBR => Bits::Flags(button::RS_CLK),
        code::MEDIA_RECORD => Bits::Flags(button::MISC),
        code::BUTTON_1 => Bits::Flags(button::TOUCHPAD),
        // A digital trigger that an analog one already reports sends nothing.
        code::BUTTON_L2 if c.left_trigger_used => Bits::None,
        code::BUTTON_L2 => Bits::LeftTrigger,
        code::BUTTON_R2 if c.right_trigger_used => Bits::None,
        code::BUTTON_R2 => Bits::RightTrigger,
        // Paddles are in no key layout file: their Linux codes.
        code::UNKNOWN if c.has_paddles => match scan {
            0x2c4 => Bits::Paddle(button::PADDLE1),
            0x2c5 => Bits::Paddle(button::PADDLE2),
            0x2c6 => Bits::Paddle(button::PADDLE3),
            0x2c7 => Bits::Paddle(button::PADDLE4),
            _ => return None,
        },
        _ => return None,
    };
    Some(bits)
}

/// What a button did: send the controller's state, nothing, or not a
/// controller's key at all.
enum Press {
    Send,
    Eaten,
    NotMine,
    /// Start was held: the mouse emulation goes on or off, then send.
    ToggleMouse,
}

/// `handleButtonDown`, after the remapping.
fn button_down(c: &mut Context, key: u32, event: &KeyEvent) -> Press {
    let Some(bits) = bits(c, key, event.scan_code) else {
        return Press::NotMine;
    };
    match key {
        code::BUTTON_MODE => c.has_mode = true,
        code::BUTTON_START | code::MENU if event.repeat_count == 0 => {
            c.start_down_ms = event.event_time_ms;
        }
        code::BACK | code::BUTTON_SELECT => c.has_select = true,
        _ => {}
    }
    match bits {
        Bits::None => return Press::Eaten,
        Bits::Flags(flags) | Bits::Paddle(flags) => c.input_map |= flags,
        Bits::LeftTrigger => c.left_trigger = 0xff,
        Bits::RightTrigger => c.right_trigger = 0xff,
    }
    combinations_down(c, event.event_time_ms);
    Press::Send
}

/// The emulated buttons of `handleButtonDown`: Start+LB is Select on a pad
/// with none; Start+Select (or Start+RB with no Select) is Guide on a pad
/// with none.
fn combinations_down(c: &mut Context, now_ms: i64) {
    use button::*;
    if !c.has_select
        && (c.input_map == PLAY | LB
            || (c.input_map == PLAY && now_ms - c.last_lb_up_ms <= MAXIMUM_BUMPER_UP_DELAY_MS))
    {
        c.input_map &= !(PLAY | LB);
        c.input_map |= BACK;
        c.emulating |= EMULATING_SELECT;
    }
    if !c.has_mode {
        if c.has_select {
            if c.input_map == PLAY | BACK {
                c.input_map &= !(PLAY | BACK);
                c.input_map |= SPECIAL;
                c.emulating |= EMULATING_SPECIAL;
            }
        } else if c.input_map == PLAY | RB
            || (c.input_map == PLAY && now_ms - c.last_rb_up_ms <= MAXIMUM_BUMPER_UP_DELAY_MS)
        {
            c.input_map &= !(PLAY | RB);
            c.input_map |= SPECIAL;
            c.emulating |= EMULATING_SPECIAL;
        }
    }
}

/// `handleButtonUp`, after the remapping.
fn button_up(c: &mut Context, key: u32, event: &KeyEvent, mouse_emulation: bool) -> Press {
    let Some(bits) = bits(c, key, event.scan_code) else {
        return Press::NotMine;
    };
    let mut toggle = false;
    match key {
        code::BUTTON_START | code::MENU => {
            c.start_up_ms = event.event_time_ms;
            // A spurious up comes on disconnect: Start must be down.
            if c.input_map & button::PLAY != 0
                && c.start_up_ms - c.start_down_ms > START_DOWN_TIME_MOUSE_MODE_MS
                && (mouse_emulation || c.mouse.active)
            {
                toggle = true;
            }
        }
        code::BUTTON_L1 => c.last_lb_up_ms = event.event_time_ms,
        code::BUTTON_R1 => c.last_rb_up_ms = event.event_time_ms,
        _ => {}
    }
    match bits {
        Bits::None => return Press::Eaten,
        Bits::Flags(flags) | Bits::Paddle(flags) => c.input_map &= !flags,
        Bits::LeftTrigger => c.left_trigger = 0,
        Bits::RightTrigger => c.right_trigger = 0,
    }
    use button::*;
    if c.emulating & EMULATING_SELECT != 0 && (c.input_map & PLAY == 0 || c.input_map & LB == 0) {
        c.input_map &= !BACK;
        c.emulating &= !EMULATING_SELECT;
    }
    if c.emulating & EMULATING_SPECIAL != 0
        && (c.input_map & PLAY == 0 || (c.input_map & BACK == 0 && c.input_map & RB == 0))
    {
        c.input_map &= !SPECIAL;
        c.emulating &= !EMULATING_SPECIAL;
    }
    if toggle {
        Press::ToggleMouse
    } else {
        Press::Send
    }
}

/// `handleAxisSet` with `handleDeadZone`: values in Android's units, in
/// the order left stick X and Y, right stick X and Y, left and right
/// trigger, hat X and Y.
fn axis_set(c: &mut Context, values: [f32; 8]) {
    let [lsx, lsy, rsx, rsy, mut lt, mut rt, hat_x, hat_y] = values;
    if c.left_stick.is_some() {
        let (x, y) = dead_zone(lsx, lsy, c.left_deadzone);
        c.left_stick_x = (x * 32766.0) as i16;
        c.left_stick_y = (-y * 32766.0) as i16;
    }
    if c.right_stick.is_some() {
        let (x, y) = dead_zone(rsx, rsy, c.right_deadzone);
        c.right_stick_x = (x * 32766.0) as i16;
        c.right_stick_y = (-y * 32766.0) as i16;
    }
    if c.triggers.is_some() {
        // Android sends 0 for a trigger that idles at -1 until it is first
        // touched.
        c.left_trigger_used |= lt != 0.0;
        c.right_trigger_used |= rt != 0.0;
        if c.triggers_idle_negative {
            if c.left_trigger_used {
                lt = (lt + 1.0) / 2.0;
            }
            if c.right_trigger_used {
                rt = (rt + 1.0) / 2.0;
            }
        }
        if lt <= c.trigger_deadzone {
            lt = 0.0;
        }
        if rt <= c.trigger_deadzone {
            rt = 0.0;
        }
        c.left_trigger = trigger_byte(lt);
        c.right_trigger = trigger_byte(rt);
    }
    if c.hat {
        c.input_map &= !(button::LEFT | button::RIGHT);
        if hat_x < -0.5 {
            c.input_map |= button::LEFT;
            c.hat_x_used = true;
        } else if hat_x > 0.5 {
            c.input_map |= button::RIGHT;
            c.hat_x_used = true;
        }
        c.input_map &= !(button::UP | button::DOWN);
        if hat_y < -0.5 {
            c.input_map |= button::UP;
            c.hat_y_used = true;
        } else if hat_y > 0.5 {
            c.input_map |= button::DOWN;
            c.hat_y_used = true;
        }
    }
}

/// Java's `(byte)(value * 0xFF)`: truncate, then keep the low byte.
fn trigger_byte(value: f32) -> u8 {
    (value * 255.0) as i32 as u8
}

/// `handleDeadZone` with a positive radius: a stick inside it is centred,
/// one outside it is sent as it is. The host applies its own deadzone too,
/// so the values are not rescaled.
pub(crate) fn dead_zone(x: f32, y: f32, radius: f32) -> (f32, f32) {
    let magnitude = (f64::from(x).powi(2) + f64::from(y).powi(2)).sqrt();
    if radius > 0.0 && magnitude <= f64::from(radius) {
        (0.0, 0.0)
    } else {
        (x, y)
    }
}

/// `toggleMouseEmulation`.
fn toggle_mouse(c: &mut Context, now_ms: i64) {
    c.mouse.active = !c.mouse.active;
    c.mouse.next_ms = now_ms + MOUSE_EMULATION_PERIOD_MS;
    log::line(&format!(
        "input: mouse emulation is {}",
        if c.mouse.active { "on" } else { "off" }
    ));
}

/// The emulated mouse's buttons: A and B click, X moves by fixed steps, Y
/// doubles the step, the d-pad scrolls.
fn mouse_buttons(c: &mut Context, input_map: u32, sink: &mut dyn Sink) {
    use button::*;
    let changed = input_map ^ c.mouse.last_input_map;
    c.mouse.last_input_map = input_map;
    let pressed = |flag: u32| input_map & flag != 0;
    if changed & X != 0 {
        c.mouse.x_down = pressed(X);
    }
    if changed & Y != 0 && pressed(Y) {
        c.mouse.multiplier *= 2;
        if c.mouse.multiplier > 255 {
            c.mouse.multiplier = 1;
        }
    }
    for (flag, mouse_button) in [(A, MOUSE_LEFT), (B, MOUSE_RIGHT)] {
        if changed & flag != 0 {
            sink.send(Packet::MouseButton {
                pressed: pressed(flag),
                button: mouse_button,
            });
        }
    }
    for (flag, vertical, clicks) in [
        (UP, true, 1),
        (DOWN, true, -1),
        (RIGHT, false, 1),
        (LEFT, false, -1),
    ] {
        if changed & flag != 0 && pressed(flag) {
            let amount = clicks * 120;
            sink.send(if vertical {
                Packet::Scroll { amount }
            } else {
                Packet::HScroll { amount }
            });
        }
    }
}

/// `convertRawStickAxisToPixelMovement`: faster as the stick goes further.
fn stick_movement(x: i16, y: i16) -> (f32, f32) {
    let scale = 4.0 / 32766.0;
    let (mut vx, mut vy) = (f32::from(x) * scale, f32::from(y) * scale);
    let magnitude = (f64::from(vx).powi(2) + f64::from(vy).powi(2)).sqrt();
    if magnitude > 0.0 {
        let factor = magnitude.powi(2);
        vx = (f64::from(vx) * factor) as f32;
        vy = (f64::from(vy) * factor) as f32;
    }
    (vx, vy)
}

fn magnitude((x, y): (f32, f32)) -> f64 {
    (f64::from(x).powi(2) + f64::from(y).powi(2)).sqrt()
}

/// The emulated mouse's movement, every 50 ms: the left stick moves it,
/// the right stick scrolls (Artemis's default, "right").
fn mouse_move(c: &Context, sink: &mut dyn Sink) {
    let moved = stick_movement(c.left_stick_x, c.left_stick_y);
    if magnitude(moved) >= 1.0 {
        let (dx, dy) = if c.mouse.x_down {
            let step = |value: f32| ((value as i32).signum() * c.mouse.multiplier) as i16;
            (step(moved.0), step(-moved.1))
        } else {
            (moved.0 as i16, (-moved.1) as i16)
        };
        sink.send(Packet::MouseMove { dx, dy });
    }
    let scrolled = stick_movement(c.right_stick_x, c.right_stick_y);
    if magnitude(scrolled) >= 1.0 {
        sink.send(Packet::Scroll {
            amount: scrolled.1 as i16,
        });
        sink.send(Packet::HScroll {
            amount: scrolled.0 as i16,
        });
    }
}

/// `ControllerHeartbeat`.
#[derive(Debug, Default)]
struct Heartbeat {
    states: [Option<(State, u8)>; MAX_GAMEPADS as usize],
    next_ms: Option<i64>,
}

impl Heartbeat {
    fn record(&mut self, state: State) {
        let Some(slot) = self.states.get_mut(usize::from(state.number)) else {
            return;
        };
        let repeats = if active(&state) {
            0
        } else {
            DISCONNECT_REPEATS
        };
        *slot = Some((state, repeats));
    }

    fn tick(&mut self, now_ms: i64, sink: &mut dyn Sink) {
        let next = *self.next_ms.get_or_insert(now_ms + HEARTBEAT_MS);
        if now_ms < next {
            return;
        }
        self.next_ms = Some(now_ms + HEARTBEAT_MS);
        for slot in &mut self.states {
            let Some((state, repeats)) = slot else {
                continue;
            };
            if active(state) {
                sink.send(Packet::Controller(*state));
            } else if *repeats > 0 {
                sink.send(Packet::Controller(*state));
                *repeats -= 1;
                if *repeats == 0 {
                    *slot = None;
                }
            }
        }
    }

    /// None until the first tick sets the clock: the app ticks on every
    /// turn of its loop anyway.
    fn next_deadline(&self) -> Option<i64> {
        self.next_ms
            .filter(|_| self.states.iter().any(Option::is_some))
    }
}

fn active(state: &State) -> bool {
    state.active_mask & (1 << state.number) != 0
}
