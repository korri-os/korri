// SPDX-License-Identifier: GPL-3.0-only
//! What goes back to the controllers (#391 step D, slice D4b): the host's
//! rumble, trigger rumble and RGB light, and each controller's battery to
//! the host, as Artemis's `ControllerHandler` does them at API 31.
//!
//! The core calls `rumble`, `rumble_triggers` and `set_controller_led` on
//! its own thread (`CtrlAsyncCb`). These callbacks only leave the
//! controller's latest value for the feedback thread (`Worker`), which keeps
//! no more than that: no JNI and no wait on the core's thread, no backlog
//! behind a call into Android that stalls, and no panic into C. The feedback thread is attached to the VM for its
//! whole life; it alone calls Android (`Outputs`, `feedback_jni.rs` in the
//! app, a scripted one in the tests), and it alone polls the batteries,
//! which Artemis also keeps off its main thread: `getBatteryState` is a
//! Binder call. A battery report goes to the core through the input's gate
//! (`input::CORE_GATE`), as every other packet does.
//!
//! The app's thread tells the feedback thread which device holds which
//! controller number (`Seats`), when the controller's arrival goes to the
//! host. `stop` ends the feedback before the core stops: the core's
//! callbacks reach nothing more, no battery report of the stream goes out
//! any more, and every vibration and light that the stream started is
//! stopped, before any work that waited.
//!
//! Ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`, a fork of
//! `ClassicOldSong/moonlight-android`, itself a fork of
//! `moonlight-stream/moonlight-android`): `ControllerHandler.java`
//! (`createInputDeviceContextForDevice`'s vibrators and light,
//! `handleRumble`, `handleRumbleTriggers`, the three `rumble*Vibrators`,
//! `handleSetControllerLED`, `sendControllerBatteryPacket`,
//! `InputDeviceContext.destroy`, `stop`) and `Game.java`'s callbacks.
//! Artemis ships under the GNU GPL version 3 with no "or later" grant, so
//! this file does too. The Rust adaptation is Korri work dated 2026-11-14.
use super::input::{Device, Gate, Packet, Sink};
use crate::log;
use anyhow::Result;
use std::{
    collections::{BTreeMap, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Condvar, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
    },
    thread::JoinHandle,
    time::Duration,
};

/// `BatteryState`'s statuses.
pub mod status {
    pub const UNKNOWN: i32 = 1;
    pub const CHARGING: i32 = 2;
    pub const DISCHARGING: i32 = 3;
    pub const NOT_CHARGING: i32 = 4;
    pub const FULL: i32 = 5;
}

/// The stream's battery states (`Limelight.h`, `LI_BATTERY_STATE_*`).
pub(crate) mod battery_state {
    pub const UNKNOWN: u8 = 0x00;
    pub const DISCHARGING: u8 = 0x02;
    pub const CHARGING: u8 = 0x03;
    pub const NOT_CHARGING: u8 = 0x04;
    pub const FULL: u8 = 0x05;
}

/// `LI_BATTERY_PERCENTAGE_UNKNOWN`.
pub(crate) const PERCENTAGE_UNKNOWN: u8 = 0xff;

/// `BATTERY_RECHECK_INTERVAL_MS`.
pub(crate) const BATTERY_RECHECK_MS: i64 = 120_000;

/// The longest that a stop waits for the feedback thread. A Binder call to
/// a vibrator or a light that hangs must not hold the stream's stop.
pub const STOP_WAIT: Duration = Duration::from_secs(2);

/// A `VibratorManager`'s vibrators: how many, and whether every one has
/// amplitude control.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Vibrators {
    pub count: usize,
    pub amplitude: bool,
}

/// One `Vibrator`: whether it is there (`hasVibrator`) and has amplitude
/// control.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Vibrator {
    pub present: bool,
    pub amplitude: bool,
}

/// What can vibrate: a `VibratorManager`, and the single `Vibrator`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Vibration {
    pub manager: Vibrators,
    pub vibrator: Vibrator,
}

/// The phone itself: its Android version, and its own vibrators, for its
/// built-in controller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Phone {
    /// `Build.VERSION.SDK_INT`.
    pub api_level: i32,
    pub vibration: Vibration,
}

/// Whose vibrators rumble a controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// The controller's own: `InputDevice.getVibratorManager()`, `getVibrator()`.
    Device,
    /// The phone's: the `VibratorManager` and `Vibrator` services.
    Phone,
}

/// How a controller rumbles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rumble {
    #[default]
    None,
    /// Four vibrators with amplitude control: both motors and both triggers.
    Quad(Owner),
    /// Two with amplitude control: the motors.
    Dual(Owner),
    /// One vibrator, with or without amplitude control.
    Single { owner: Owner, amplitude: bool },
}

impl Rumble {
    /// `createInputDeviceContextForDevice`: the controller's own vibrators
    /// first, four, then two, then one; a built-in controller without any
    /// takes the phone's the same way.
    pub fn of(device: &Device, phone: &Phone) -> Self {
        match Self::from(&device.vibration, Owner::Device) {
            Self::None if !super::gamepad::is_external(device) => {
                Self::from(&phone.vibration, Owner::Phone)
            }
            rumble => rumble,
        }
    }

    fn from(vibration: &Vibration, owner: Owner) -> Self {
        let manager = vibration.manager;
        if manager.count == 4 && manager.amplitude {
            Self::Quad(owner)
        } else if manager.count == 2 && manager.amplitude {
            Self::Dual(owner)
        } else if vibration.vibrator.present {
            Self::Single {
                owner,
                amplitude: vibration.vibrator.amplitude,
            }
        } else {
            Self::None
        }
    }
}

/// One controller's place in the stream, for its feedback: Android's
/// device, its controller number, how it rumbles and whether it has an RGB
/// light.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    pub device: i32,
    pub name: String,
    pub number: u16,
    pub rumble: Rumble,
    /// Some light reports RGB control (`Light.hasRgbControl`).
    pub light: bool,
}

/// Where the app's thread says which controllers are seated.
pub trait Seats {
    /// Its arrival went to the host.
    fn seated(&mut self, seat: Seat);
    /// The device went.
    fn left(&mut self, device: i32);
}

/// Whose vibrator a call goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Device(i32),
    Phone,
}

impl Target {
    fn of(owner: Owner, device: i32) -> Self {
        match owner {
            Owner::Device => Self::Device(device),
            Owner::Phone => Self::Phone,
        }
    }
}

/// One call into Android.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    /// `VibratorManager.vibrate`: each vibrator, in `getVibratorIds()`
    /// order, a one-shot of 60 s at its amplitude. A vibrator whose
    /// amplitude is 0 is left out, so it stops.
    Combined { target: Target, amplitudes: Vec<u8> },
    /// `VibratorManager.cancel()`.
    CancelManager { target: Target },
    /// `Vibrator.vibrate`: a one-shot of 60 s at this amplitude.
    OneShot { target: Target, amplitude: u8 },
    /// `Vibrator.vibrate` without amplitude control: on, then off, repeated.
    Pulse {
        target: Target,
        on_ms: i64,
        off_ms: i64,
    },
    /// `Vibrator.cancel()`.
    CancelVibrator { target: Target },
    /// The device's `LightsManager`: its session, opened at the first call,
    /// asks every light with RGB control for this colour.
    Light { device: i32, argb: u32 },
    /// The light session closes; Android gives the light back.
    CloseLight { device: i32 },
}

/// `InputDevice.getBatteryState()` of a present battery.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Battery {
    pub status: i32,
    /// 0 to 1, or NaN when unknown.
    pub capacity: f32,
}

/// Android's vibrators, lights and batteries, on the feedback thread.
pub trait Outputs {
    fn call(&mut self, call: &Call) -> Result<()>;
    /// The battery; none when the device has none, or is gone.
    fn battery(&mut self, device: i32) -> Result<Option<Battery>>;
    /// The device went: drop what is kept for it.
    fn forget(&mut self, device: i32);
}

/// What a value from the host sets.
#[derive(Clone, Copy)]
enum Kind {
    Rumble,
    Triggers,
    Light,
}

/// One seated controller, as Artemis's `InputDeviceContext` keeps it.
#[derive(Debug)]
struct Held {
    seat: Seat,
    low: u16,
    high: u16,
    left_trigger: u16,
    right_trigger: u16,
    light_open: bool,
    /// `lastReportedBatteryStatus` and `lastReportedBatteryCapacity`.
    reported: (i32, f32),
    next_battery_ms: i64,
}

#[derive(Debug, Default)]
struct Counts {
    rumbles: u32,
    trigger_rumbles: u32,
    lights: u32,
    calls: u32,
    battery_reads: u32,
    battery_sent: u32,
    refused: u32,
    failed: u32,
}

/// Every seated controller's feedback: pure but for `Outputs` and `Sink`.
#[derive(Debug, Default)]
pub struct Feedback {
    held: BTreeMap<i32, Held>,
    counts: Counts,
}

impl Feedback {
    /// A controller arrived: its battery is read now, then every 120 s.
    pub fn seat(&mut self, seat: Seat, now_ms: i64) {
        log::line(&format!(
            "feedback: {} ({}) is controller {}: rumble {:?}, RGB light {}",
            seat.name, seat.device, seat.number, seat.rumble, seat.light
        ));
        self.held.insert(
            seat.device,
            Held {
                seat,
                low: 0,
                high: 0,
                left_trigger: 0,
                right_trigger: 0,
                light_open: false,
                reported: (0, 0.0),
                next_battery_ms: now_ms,
            },
        );
    }

    /// `onInputDeviceRemoved`: its vibration and light stop.
    pub fn leave(&mut self, device: i32, outputs: &mut dyn Outputs) {
        if let Some(held) = self.held.remove(&device) {
            self.destroy(&held, outputs);
        }
        outputs.forget(device);
    }

    /// `handleRumble`: every controller with this number. The log line.
    pub fn rumble(
        &mut self,
        number: u16,
        low: u16,
        high: u16,
        outputs: &mut dyn Outputs,
    ) -> String {
        self.counts.rumbles += 1;
        let (mut calls, mut matched) = (Vec::new(), 0);
        for held in self.held.values_mut().filter(|h| h.seat.number == number) {
            matched += 1;
            held.low = low;
            held.high = high;
            calls.extend(motors(held));
        }
        let values = format!("low {low:04x} high {high:04x}");
        self.run(Kind::Rumble, number, &values, matched, calls, outputs)
    }

    /// `handleRumbleTriggers`: only four vibrators rumble the triggers.
    pub fn rumble_triggers(
        &mut self,
        number: u16,
        left: u16,
        right: u16,
        outputs: &mut dyn Outputs,
    ) -> String {
        self.counts.trigger_rumbles += 1;
        let (mut calls, mut matched) = (Vec::new(), 0);
        for held in self.held.values_mut().filter(|h| h.seat.number == number) {
            matched += 1;
            held.left_trigger = left;
            held.right_trigger = right;
            if matches!(held.seat.rumble, Rumble::Quad(_)) {
                calls.extend(motors(held));
            }
        }
        let values = format!("left {left:04x} right {right:04x}");
        self.run(Kind::Triggers, number, &values, matched, calls, outputs)
    }

    /// `handleSetControllerLED`: every controller with this number and an
    /// RGB light.
    pub fn light(&mut self, number: u16, rgb: (u8, u8, u8), outputs: &mut dyn Outputs) -> String {
        self.counts.lights += 1;
        let (r, g, b) = rgb;
        let argb = 0xff00_0000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
        let (mut calls, mut matched) = (Vec::new(), 0);
        for held in self.held.values_mut().filter(|h| h.seat.number == number) {
            matched += 1;
            if held.seat.light {
                held.light_open = true;
                calls.push(Call::Light {
                    device: held.seat.device,
                    argb,
                });
            }
        }
        let values = format!("{argb:08x}");
        self.run(Kind::Light, number, &values, matched, calls, outputs)
    }

    /// Read the batteries that are due, and report each change.
    pub fn tick(&mut self, now_ms: i64, outputs: &mut dyn Outputs, sink: &mut dyn Sink) {
        let due: Vec<i32> = self
            .held
            .values()
            .filter(|held| held.next_battery_ms <= now_ms)
            .map(|held| held.seat.device)
            .collect();
        for device in due {
            self.counts.battery_reads += 1;
            let read = outputs.battery(device);
            let Some(held) = self.held.get_mut(&device) else {
                continue;
            };
            held.next_battery_ms = now_ms + BATTERY_RECHECK_MS;
            match read {
                Ok(Some(battery)) => {
                    let sent = report_battery(held, battery, sink);
                    match sent {
                        None | Some(0) => {}
                        Some(super::input::CLOSED) => self.counts.refused += 1,
                        Some(_) => self.counts.failed += 1,
                    }
                    if sent == Some(0) {
                        self.counts.battery_sent += 1;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    self.counts.failed += 1;
                    log::line(&format!("feedback: battery of device {device}: {error:#}"));
                }
            }
        }
    }

    /// When `tick` next has work.
    pub fn next_deadline(&self) -> Option<i64> {
        self.held.values().map(|held| held.next_battery_ms).min()
    }

    /// `stop`: every controller's vibration stops, then the phone's own
    /// vibrator, then the lights are given back.
    pub fn stop(&mut self, outputs: &mut dyn Outputs) {
        let held = std::mem::take(&mut self.held);
        for held in held.values() {
            self.cancel(held, outputs);
        }
        self.call(
            &Call::CancelVibrator {
                target: Target::Phone,
            },
            outputs,
        );
        for (device, held) in held {
            self.close_light(&held, outputs);
            outputs.forget(device);
        }
    }

    /// What the feedback did, for the log.
    pub fn report(&self) -> String {
        let c = &self.counts;
        format!(
            "{} rumbles, {} trigger rumbles, {} lights, {} calls to Android; {} battery reads, {} battery reports sent, {} outside the stream; {} failed",
            c.rumbles,
            c.trigger_rumbles,
            c.lights,
            c.calls,
            c.battery_reads,
            c.battery_sent,
            c.refused,
            c.failed
        )
    }

    /// `InputDeviceContext.destroy`.
    fn destroy(&mut self, held: &Held, outputs: &mut dyn Outputs) {
        self.cancel(held, outputs);
        self.close_light(held, outputs);
    }

    fn cancel(&mut self, held: &Held, outputs: &mut dyn Outputs) {
        let device = held.seat.device;
        match held.seat.rumble {
            Rumble::Quad(owner) | Rumble::Dual(owner) => self.call(
                &Call::CancelManager {
                    target: Target::of(owner, device),
                },
                outputs,
            ),
            Rumble::Single { owner, .. } => self.call(
                &Call::CancelVibrator {
                    target: Target::of(owner, device),
                },
                outputs,
            ),
            Rumble::None => {}
        }
    }

    fn close_light(&mut self, held: &Held, outputs: &mut dyn Outputs) {
        let device = held.seat.device;
        if held.light_open {
            self.call(&Call::CloseLight { device }, outputs);
        }
        log::line(&format!("feedback: {} ({device}) stopped", held.seat.name));
    }

    /// Log what a value does, make its calls, and give the log line.
    fn run(
        &mut self,
        kind: Kind,
        number: u16,
        values: &str,
        matched: usize,
        calls: Vec<Call>,
        outputs: &mut dyn Outputs,
    ) -> String {
        let what = match kind {
            Kind::Rumble => "rumble",
            Kind::Triggers => "trigger rumble",
            Kind::Light => "light",
        };
        let done = if matched == 0 {
            format!("no call: no controller {number}")
        } else if calls.is_empty() {
            match kind {
                Kind::Rumble => "no call: it has no vibrator",
                Kind::Triggers => {
                    "no call: it has no trigger vibrators (four vibrators rumble the triggers)"
                }
                Kind::Light => "no call: it has no RGB light",
            }
            .to_owned()
        } else {
            format!("{calls:?}")
        };
        let line = format!("feedback: {what} controller {number} {values}: {done}");
        log::line(&line);
        for call in &calls {
            self.call(call, outputs);
        }
        line
    }

    fn call(&mut self, call: &Call, outputs: &mut dyn Outputs) {
        self.counts.calls += 1;
        if let Err(error) = outputs.call(call) {
            self.counts.failed += 1;
            log::line(&format!("feedback: {call:?} failed: {error:#}"));
        }
    }
}

/// The calls that set a controller's vibrators to its motors' values.
fn motors(held: &Held) -> Option<Call> {
    let device = held.seat.device;
    let msb = |value: u16| (value >> 8) as u8;
    match held.seat.rumble {
        Rumble::None => None,
        Rumble::Quad(owner) => Some(combined(
            Target::of(owner, device),
            vec![
                msb(held.high),
                msb(held.low),
                msb(held.left_trigger),
                msb(held.right_trigger),
            ],
        )),
        // Artemis found the vibrators of an FF_RUMBLE device in this order
        // on Xbox Series X, PS3 and PS4 pads, with no document that says so.
        Rumble::Dual(owner) => Some(combined(
            Target::of(owner, device),
            vec![msb(held.high), msb(held.low)],
        )),
        Rumble::Single { owner, amplitude } => Some(single(
            Target::of(owner, device),
            amplitude,
            msb(held.low),
            msb(held.high),
        )),
    }
}

/// `rumbleDualVibrators` and `rumbleQuadVibrators`: all at 0 cancels.
fn combined(target: Target, amplitudes: Vec<u8>) -> Call {
    if amplitudes.iter().all(|&amplitude| amplitude == 0) {
        Call::CancelManager { target }
    } else {
        Call::Combined { target, amplitudes }
    }
}

/// `rumbleSingleVibrator`: 80 % of the big motor and 33 % of the small one
/// in one amplitude; without amplitude control, a pulse in a 20 ms period.
fn single(target: Target, amplitude_control: bool, low: u8, high: u8) -> Call {
    let amplitude = ((f64::from(low) * 0.80) + (f64::from(high) * 0.33)) as i32;
    let amplitude = amplitude.min(255);
    if amplitude == 0 {
        // Not "both motors at 0": a small value can round to 0 as well.
        return Call::CancelVibrator { target };
    }
    if amplitude_control {
        return Call::OneShot {
            target,
            amplitude: amplitude as u8,
        };
    }
    const PERIOD_MS: i64 = 20;
    let on_ms = ((f64::from(amplitude) / 255.0) * PERIOD_MS as f64) as i64;
    Call::Pulse {
        target,
        on_ms,
        off_ms: PERIOD_MS - on_ms,
    }
}

/// `sendControllerBatteryPacket`: a change goes to the host. The result of
/// the send; none when nothing went.
fn report_battery(held: &mut Held, battery: Battery, sink: &mut dyn Sink) -> Option<i32> {
    let (last_status, last_capacity) = held.reported;
    let same_capacity = if battery.capacity.is_nan() || last_capacity.is_nan() {
        battery.capacity.is_nan() == last_capacity.is_nan()
    } else {
        battery.capacity == last_capacity
    };
    if battery.status == last_status && same_capacity {
        return None;
    }
    let state = match battery.status {
        status::UNKNOWN => battery_state::UNKNOWN,
        status::CHARGING => battery_state::CHARGING,
        status::DISCHARGING => battery_state::DISCHARGING,
        status::NOT_CHARGING => battery_state::NOT_CHARGING,
        status::FULL => battery_state::FULL,
        _ => return None,
    };
    // Java's (byte)(capacity * 100).
    let percentage = if battery.capacity.is_nan() {
        PERCENTAGE_UNKNOWN
    } else {
        (battery.capacity * 100.0) as i32 as u8
    };
    let number = held.seat.number as u8;
    let sent = sink.send(Packet::Battery {
        number,
        state,
        percentage,
    });
    log::line(&format!(
        "feedback: battery of {} ({}): status {} capacity {} sent to controller {number} as state {state} percentage {percentage}: {sent}",
        held.seat.name, held.seat.device, battery.status, battery.capacity
    ));
    held.reported = (battery.status, battery.capacity);
    Some(sent)
}

/// One change of the seats, in the order the input made them.
#[derive(Debug)]
enum SeatChange {
    Seated(Seat),
    Left(i32),
}

/// What waits for the feedback thread. The seats change in order; of
/// rumble, trigger rumble and light, only each controller's latest value
/// waits, as the core merges back-to-back rumble packets for a controller
/// while Artemis's callback holds the core's thread. A value that is
/// replaced is never sent, and the last one always is: a rumble of 0 is
/// never lost. So however long a call into Android stalls, the work that
/// waits stays three values per controller.
#[derive(Debug, Default)]
struct Inbox {
    seats: VecDeque<SeatChange>,
    rumble: BTreeMap<u16, (u16, u16)>,
    triggers: BTreeMap<u16, (u16, u16)>,
    lights: BTreeMap<u16, (u8, u8, u8)>,
    /// The stop, with its answer. It goes before all the rest.
    stop: Option<Sender<()>>,
    /// Stopped: nothing more is taken.
    closed: bool,
}

impl Inbox {
    fn has_work(&self) -> bool {
        !(self.seats.is_empty()
            && self.rumble.is_empty()
            && self.triggers.is_empty()
            && self.lights.is_empty())
    }

    fn take_work(&mut self) -> Work {
        Work {
            seats: std::mem::take(&mut self.seats).into(),
            rumble: std::mem::take(&mut self.rumble),
            triggers: std::mem::take(&mut self.triggers),
            lights: std::mem::take(&mut self.lights),
        }
    }
}

#[derive(Debug, Default)]
struct Work {
    seats: Vec<SeatChange>,
    rumble: BTreeMap<u16, (u16, u16)>,
    triggers: BTreeMap<u16, (u16, u16)>,
    lights: BTreeMap<u16, (u8, u8, u8)>,
}

/// The inbox and the signal that wakes the feedback thread.
#[derive(Debug, Default)]
struct Mailbox {
    inbox: Mutex<Inbox>,
    wake: Condvar,
    /// The stop was asked for: work in hand is left undone.
    stopping: AtomicBool,
}

impl Mailbox {
    fn post(&self, put: impl FnOnce(&mut Inbox)) {
        let mut inbox = self.inbox.lock().unwrap_or_else(PoisonError::into_inner);
        if inbox.closed {
            return;
        }
        put(&mut inbox);
        drop(inbox);
        self.wake.notify_one();
    }

    fn stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    /// The next thing to do: the stop's answer before anything else, else
    /// the work that waits, else nothing when a battery read is due.
    /// Broken, it waits for the stop alone, with no deadline: no spin.
    fn next(&self, feedback: &Feedback, broken: bool, clock: fn() -> i64) -> Next {
        let mut inbox = self.inbox.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(answer) = inbox.stop.take() {
                return Next::Stop(answer);
            }
            if broken {
                inbox.take_work();
                inbox = self
                    .wake
                    .wait(inbox)
                    .unwrap_or_else(PoisonError::into_inner);
                continue;
            }
            if inbox.has_work() {
                return Next::Work(inbox.take_work());
            }
            match feedback.next_deadline() {
                None => {
                    inbox = self
                        .wake
                        .wait(inbox)
                        .unwrap_or_else(PoisonError::into_inner)
                }
                Some(at) => {
                    let left = at.saturating_sub(clock());
                    if left <= 0 {
                        return Next::Work(Work::default());
                    }
                    let wait = Duration::from_millis(left.min(60_000) as u64);
                    inbox = self
                        .wake
                        .wait_timeout(inbox, wait)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
            }
        }
    }
}

enum Next {
    Work(Work),
    Stop(Sender<()>),
}

/// The battery sink of one stream: behind the core's gate, and behind the
/// feedback's own, which the stream's stop closes. A battery read that ends
/// after a stop that did not wait for it sends into no later stream.
struct Tied<S> {
    gate: Arc<Gate>,
    sink: S,
}

impl<S: Sink> Sink for Tied<S> {
    fn send(&mut self, packet: Packet) -> i32 {
        let sink = &mut self.sink;
        self.gate.pass(|| sink.send(packet))
    }
}

/// The feedback thread of one stream.
pub struct Worker {
    /// Which worker serves the core, if any.
    id: u64,
    mailbox: Arc<Mailbox>,
    /// The stream's own gate for its battery reports.
    gate: Arc<Gate>,
    thread: Option<JoinHandle<()>>,
}

/// The app's thread's way to the feedback thread.
pub struct SeatSender(Arc<Mailbox>);

impl Seats for SeatSender {
    fn seated(&mut self, seat: Seat) {
        self.0
            .post(|inbox| inbox.seats.push_back(SeatChange::Seated(seat)));
    }
    fn left(&mut self, device: i32) {
        self.0
            .post(|inbox| inbox.seats.push_back(SeatChange::Left(device)));
    }
}

impl Worker {
    /// Start the feedback thread. `outputs` makes Android's side on that
    /// thread, which it attaches to the VM; `sink` takes the battery
    /// reports, until the stop; `clock` gives milliseconds of
    /// `CLOCK_MONOTONIC`.
    pub fn start<O, S>(
        outputs: impl FnOnce() -> Result<O> + Send + 'static,
        sink: S,
        clock: fn() -> i64,
    ) -> Result<Self>
    where
        O: Outputs + 'static,
        S: Sink + Send + 'static,
    {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let mailbox = Arc::new(Mailbox::default());
        let gate = Arc::new(Gate::closed());
        gate.open();
        let sink = Tied {
            gate: gate.clone(),
            sink,
        };
        let served = mailbox.clone();
        let thread = std::thread::Builder::new()
            .name("controller-feedback".into())
            .spawn(move || serve(&served, outputs, sink, clock))?;
        Ok(Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            mailbox,
            gate,
            thread: Some(thread),
        })
    }

    /// For the input: where it says which controllers are seated.
    pub fn seats(&self) -> SeatSender {
        SeatSender(self.mailbox.clone())
    }

    /// From now on the core's callbacks reach this thread.
    pub fn serve_core(&self) {
        *CORE.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((self.id, self.mailbox.clone()));
    }

    /// The core's callbacks reach nothing more, no battery report of this
    /// stream goes out any more, and every vibration and light stops before
    /// any work that waited. True when the feedback thread confirmed it
    /// within `within`; a call into Android that is under way cannot be cut
    /// short, and the stop then runs when it returns.
    pub fn stop(&mut self, within: Duration) -> bool {
        {
            let mut core = CORE.lock().unwrap_or_else(PoisonError::into_inner);
            if core.as_ref().is_some_and(|(id, _)| *id == self.id) {
                core.take();
            }
        }
        // Waits only for a send under way, which queues and returns.
        self.gate.close();
        let Some(thread) = self.thread.take() else {
            return true;
        };
        let (answer, answered) = mpsc::channel();
        self.mailbox.stopping.store(true, Ordering::SeqCst);
        {
            let mut inbox = self
                .mailbox
                .inbox
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            inbox.closed = true;
            inbox.stop = Some(answer);
        }
        self.mailbox.wake.notify_one();
        match answered.recv_timeout(within) {
            Ok(()) => {
                let _ = thread.join();
                true
            }
            Err(RecvTimeoutError::Disconnected) => {
                // It ended without an answer, and said why.
                let _ = thread.join();
                false
            }
            Err(RecvTimeoutError::Timeout) => {
                log::line(&format!(
                    "feedback: no answer from the feedback thread in {} ms; the stream stops without it, and it stops the vibration when its call returns",
                    within.as_millis()
                ));
                false
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop(STOP_WAIT);
    }
}

/// The feedback thread that the core's callbacks reach, while it serves.
static CORE: Mutex<Option<(u64, Arc<Mailbox>)>> = Mutex::new(None);

fn to_core_worker(put: impl FnOnce(&mut Inbox)) {
    let mailbox = CORE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map(|(_, mailbox)| mailbox.clone());
    if let Some(mailbox) = mailbox {
        mailbox.post(put);
    }
}

/// `ConnListenerRumble`, on the core's thread: the controller's latest
/// value waits for the feedback thread; this never waits for it.
///
/// # Safety
/// None needed: the arguments are plain values. It is `unsafe` only as the
/// core's callback type is.
pub unsafe extern "C" fn rumble(number: u16, low: u16, high: u16) {
    crate::callback::guarded("rumble", (), || {
        to_core_worker(|inbox| {
            inbox.rumble.insert(number, (low, high));
        })
    });
}

/// `ConnListenerRumbleTriggers`, on the core's thread.
///
/// # Safety
/// As `rumble`.
pub unsafe extern "C" fn rumble_triggers(number: u16, left: u16, right: u16) {
    crate::callback::guarded("rumble triggers", (), || {
        to_core_worker(|inbox| {
            inbox.triggers.insert(number, (left, right));
        })
    });
}

/// `ConnListenerSetControllerLED`, on the core's thread.
///
/// # Safety
/// As `rumble`.
pub unsafe extern "C" fn set_controller_led(number: u16, r: u8, g: u8, b: u8) {
    crate::callback::guarded("set controller LED", (), || {
        to_core_worker(|inbox| {
            inbox.lights.insert(number, (r, g, b));
        })
    });
}

/// The feedback thread: the stop first, then the work that waits, and each
/// battery when it is due.
fn serve<O: Outputs, S: Sink>(
    mailbox: &Mailbox,
    outputs: impl FnOnce() -> Result<O>,
    mut sink: S,
    clock: fn() -> i64,
) {
    let mut outputs = match outputs() {
        Ok(outputs) => Some(outputs),
        Err(error) => {
            log::line(&format!(
                "feedback: no rumble, light or battery report: {error:#}"
            ));
            None
        }
    };
    let mut feedback = Feedback::default();
    // A panic ends the feedback, not the stream; the stop still runs.
    let mut broken = outputs.is_none();
    loop {
        let work = match mailbox.next(&feedback, broken, clock) {
            Next::Stop(answer) => {
                if let Some(outputs) = outputs.as_mut() {
                    // Also after a panic: the vibration must stop.
                    if catch_unwind(AssertUnwindSafe(|| feedback.stop(outputs))).is_err() {
                        log::line("feedback: the stop panicked");
                    }
                    log::line(&format!("feedback: ended: {}", feedback.report()));
                }
                let _ = answer.send(());
                return;
            }
            Next::Work(work) => work,
        };
        let Some(outputs) = outputs.as_mut() else {
            continue;
        };
        let done = catch_unwind(AssertUnwindSafe(|| {
            run_work(mailbox, &mut feedback, work, outputs, &mut sink, clock)
        }));
        if done.is_err() {
            log::line("feedback: a call panicked; the controllers get no more feedback");
            broken = true;
        }
    }
}

/// The seats' changes in order, then each controller's latest values, then
/// the batteries that are due. A stop asked for meanwhile leaves the rest.
fn run_work(
    mailbox: &Mailbox,
    feedback: &mut Feedback,
    work: Work,
    outputs: &mut dyn Outputs,
    sink: &mut dyn Sink,
    clock: fn() -> i64,
) {
    for change in work.seats {
        if mailbox.stopping() {
            return;
        }
        match change {
            SeatChange::Seated(seat) => feedback.seat(seat, clock()),
            SeatChange::Left(device) => feedback.leave(device, outputs),
        }
    }
    for (number, (low, high)) in work.rumble {
        if mailbox.stopping() {
            return;
        }
        feedback.rumble(number, low, high, outputs);
    }
    for (number, (left, right)) in work.triggers {
        if mailbox.stopping() {
            return;
        }
        feedback.rumble_triggers(number, left, right, outputs);
    }
    for (number, rgb) in work.lights {
        if mailbox.stopping() {
            return;
        }
        feedback.light(number, rgb, outputs);
    }
    if !mailbox.stopping() {
        feedback.tick(clock(), outputs, sink);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::input::{
        CLOSED, Gate, Gated,
        tests::{Recorded, pad},
    };
    use std::{
        collections::HashMap,
        sync::{
            Arc, Mutex as StdMutex,
            atomic::{AtomicUsize, Ordering as AtomicOrdering},
        },
        time::Instant,
    };

    /// Android's side, as a test scripts it: it records each call, gives
    /// each device the battery that the test set, fails the calls of the
    /// devices in `failing`, and takes `delay` for each call.
    #[derive(Clone, Default)]
    pub(crate) struct Scripted {
        pub calls: Arc<StdMutex<Vec<Call>>>,
        pub batteries: Arc<StdMutex<HashMap<i32, Option<Battery>>>>,
        pub failing: Vec<Target>,
        pub delay: Duration,
        pub panics: bool,
        /// A battery read takes this long, or panics.
        pub battery_delay: Duration,
        pub battery_panics: bool,
    }

    impl Scripted {
        pub(crate) fn take(&self) -> Vec<Call> {
            std::mem::take(&mut *self.calls.lock().unwrap())
        }
        fn set_battery(&self, device: i32, battery: Option<Battery>) {
            self.batteries.lock().unwrap().insert(device, battery);
        }
    }

    impl Outputs for Scripted {
        fn call(&mut self, call: &Call) -> Result<()> {
            std::thread::sleep(self.delay);
            if self.panics {
                panic!("a JNI fault");
            }
            self.calls.lock().unwrap().push(call.clone());
            let target = match call {
                Call::Combined { target, .. }
                | Call::CancelManager { target }
                | Call::OneShot { target, .. }
                | Call::Pulse { target, .. }
                | Call::CancelVibrator { target } => *target,
                Call::Light { device, .. } | Call::CloseLight { device } => Target::Device(*device),
            };
            anyhow::ensure!(!self.failing.contains(&target), "IllegalStateException");
            Ok(())
        }
        fn battery(&mut self, device: i32) -> Result<Option<Battery>> {
            std::thread::sleep(self.battery_delay);
            if self.battery_panics {
                panic!("a Binder fault");
            }
            Ok(self
                .batteries
                .lock()
                .unwrap()
                .get(&device)
                .copied()
                .flatten())
        }
        fn forget(&mut self, _: i32) {}
    }

    fn seat(device: i32, number: u16, rumble: Rumble, light: bool) -> Seat {
        Seat {
            device,
            name: format!("pad {device}"),
            number,
            rumble,
            light,
        }
    }

    fn vibration(count: usize, amplitude: bool, single: Option<bool>) -> Vibration {
        Vibration {
            manager: Vibrators { count, amplitude },
            vibrator: Vibrator {
                present: single.is_some(),
                amplitude: single.unwrap_or(false),
            },
        }
    }

    #[test]
    fn a_pad_rumbles_with_its_own_vibrators_four_then_two_then_one() {
        let phone = Phone {
            api_level: 31,
            vibration: vibration(1, true, Some(true)),
        };
        let with = |v: Vibration| {
            let mut device = pad(16, 0x054c);
            device.vibration = v;
            Rumble::of(&device, &phone)
        };
        assert_eq!(
            with(vibration(4, true, Some(true))),
            Rumble::Quad(Owner::Device)
        );
        assert_eq!(
            with(vibration(2, true, Some(true))),
            Rumble::Dual(Owner::Device)
        );
        // Two without amplitude control: its single vibrator.
        assert_eq!(
            with(vibration(2, false, Some(false))),
            Rumble::Single {
                owner: Owner::Device,
                amplitude: false
            }
        );
        // Three: neither four nor two.
        assert_eq!(
            with(vibration(3, true, Some(true))),
            Rumble::Single {
                owner: Owner::Device,
                amplitude: true
            }
        );
        // An external pad with none never takes the phone's.
        assert_eq!(with(vibration(0, false, None)), Rumble::None);
    }

    #[test]
    fn built_in_buttons_without_vibrators_rumble_with_the_phones() {
        let mut buttons = pad(2, 0);
        buttons.external = false;
        let phone = |v: Vibration| Phone {
            api_level: 31,
            vibration: v,
        };
        assert_eq!(
            Rumble::of(&buttons, &phone(vibration(2, true, Some(true)))),
            Rumble::Dual(Owner::Phone)
        );
        assert_eq!(
            Rumble::of(&buttons, &phone(vibration(1, true, Some(true)))),
            Rumble::Single {
                owner: Owner::Phone,
                amplitude: true
            }
        );
        assert_eq!(
            Rumble::of(&buttons, &phone(Vibration::default())),
            Rumble::None
        );
    }

    #[test]
    fn two_vibrators_take_the_motors_high_first_and_both_at_zero_cancel() {
        let mut outputs = Scripted::default();
        let mut feedback = Feedback::default();
        feedback.seat(seat(18, 0, Rumble::Dual(Owner::Device), true), 0);
        feedback.rumble(0, 0xc000, 0x40ff, &mut outputs);
        // Trigger rumble needs four vibrators.
        feedback.rumble_triggers(0, 0xffff, 0xffff, &mut outputs);
        feedback.rumble(0, 0x00ff, 0x0000, &mut outputs);
        let target = Target::Device(18);
        assert_eq!(
            outputs.take(),
            [
                Call::Combined {
                    target,
                    amplitudes: vec![0x40, 0xc0]
                },
                // Below 0x100 is 0: the amplitude is the high byte.
                Call::CancelManager { target },
            ]
        );
    }

    #[test]
    fn four_vibrators_rumble_the_triggers_with_the_motors() {
        let mut outputs = Scripted::default();
        let mut feedback = Feedback::default();
        feedback.seat(seat(18, 1, Rumble::Quad(Owner::Device), false), 0);
        feedback.rumble(1, 0x8000, 0x0000, &mut outputs);
        feedback.rumble_triggers(1, 0x1000, 0xff00, &mut outputs);
        feedback.rumble(1, 0, 0, &mut outputs);
        feedback.rumble_triggers(1, 0, 0, &mut outputs);
        let target = Target::Device(18);
        assert_eq!(
            outputs.take(),
            [
                Call::Combined {
                    target,
                    amplitudes: vec![0, 0x80, 0, 0]
                },
                Call::Combined {
                    target,
                    amplitudes: vec![0, 0x80, 0x10, 0xff]
                },
                Call::Combined {
                    target,
                    amplitudes: vec![0, 0, 0x10, 0xff]
                },
                Call::CancelManager { target },
            ]
        );
    }

    #[test]
    fn one_vibrator_mixes_both_motors_into_one_amplitude() {
        let mut outputs = Scripted::default();
        let mut feedback = Feedback::default();
        let one = |amplitude| Rumble::Single {
            owner: Owner::Phone,
            amplitude,
        };
        feedback.seat(seat(2, 0, one(true), false), 0);
        feedback.seat(seat(3, 1, one(false), false), 0);
        // 0xff * 0.8 + 0xff * 0.33 is above 255.
        feedback.rumble(0, 0xffff, 0xffff, &mut outputs);
        feedback.rumble(0, 0x6400, 0x0a00, &mut outputs);
        // 2 * 0.33 is 0: cancel, though a motor runs.
        feedback.rumble(0, 0x0000, 0x0200, &mut outputs);
        feedback.rumble(1, 0x8000, 0x0000, &mut outputs);
        let phone = Target::Phone;
        assert_eq!(
            outputs.take(),
            [
                Call::OneShot {
                    target: phone,
                    amplitude: 255
                },
                Call::OneShot {
                    target: phone,
                    amplitude: 83
                },
                Call::CancelVibrator { target: phone },
                // 0x80 * 0.8 = 102 of 255: 8 of 20 ms on.
                Call::Pulse {
                    target: phone,
                    on_ms: 8,
                    off_ms: 12
                },
            ]
        );
    }

    #[test]
    fn rumble_goes_to_every_device_of_that_number_and_to_no_other() {
        let mut outputs = Scripted::default();
        let mut feedback = Feedback::default();
        feedback.seat(seat(18, 0, Rumble::Dual(Owner::Device), true), 0);
        // A DualShock 4's touchpad shares its number and has no vibrator.
        feedback.seat(seat(16, 0, Rumble::None, true), 0);
        feedback.seat(seat(20, 1, Rumble::Dual(Owner::Device), false), 0);
        feedback.rumble(1, 0x0100, 0x0200, &mut outputs);
        // The host may rumble a controller that is not there.
        feedback.rumble(5, 0xffff, 0xffff, &mut outputs);
        assert_eq!(
            outputs.take(),
            [Call::Combined {
                target: Target::Device(20),
                amplitudes: vec![2, 1]
            }]
        );
    }

    #[test]
    fn a_value_that_makes_no_call_says_why() {
        let mut outputs = Scripted::default();
        let mut feedback = Feedback::default();
        feedback.seat(seat(18, 0, Rumble::Dual(Owner::Device), false), 0);
        feedback.seat(seat(20, 1, Rumble::None, false), 0);
        assert_eq!(
            feedback.rumble_triggers(0, 0xffff, 0xffff, &mut outputs),
            "feedback: trigger rumble controller 0 left ffff right ffff: no call: it has no trigger vibrators (four vibrators rumble the triggers)"
        );
        assert_eq!(
            feedback.rumble(1, 0x0100, 0x0100, &mut outputs),
            "feedback: rumble controller 1 low 0100 high 0100: no call: it has no vibrator"
        );
        assert_eq!(
            feedback.light(0, (1, 2, 3), &mut outputs),
            "feedback: light controller 0 ff010203: no call: it has no RGB light"
        );
        assert_eq!(
            feedback.rumble(5, 0xffff, 0xffff, &mut outputs),
            "feedback: rumble controller 5 low ffff high ffff: no call: no controller 5"
        );
        assert_eq!(outputs.take(), []);
    }

    #[test]
    fn the_light_goes_to_the_rgb_lights_of_that_number() {
        let mut outputs = Scripted::default();
        let mut feedback = Feedback::default();
        feedback.seat(seat(18, 0, Rumble::Dual(Owner::Device), true), 0);
        feedback.seat(seat(20, 0, Rumble::None, false), 0);
        feedback.light(0, (0x44, 0x66, 0x88), &mut outputs);
        feedback.light(1, (1, 2, 3), &mut outputs);
        assert_eq!(
            outputs.take(),
            [Call::Light {
                device: 18,
                argb: 0xff44_6688
            }]
        );
    }

    #[test]
    fn a_device_that_goes_stops_its_vibration_and_gives_its_light_back() {
        let mut outputs = Scripted::default();
        let mut feedback = Feedback::default();
        feedback.seat(seat(18, 0, Rumble::Dual(Owner::Device), true), 0);
        feedback.seat(
            seat(
                2,
                1,
                Rumble::Single {
                    owner: Owner::Phone,
                    amplitude: true,
                },
                false,
            ),
            0,
        );
        feedback.light(0, (1, 2, 3), &mut outputs);
        outputs.take();
        feedback.leave(18, &mut outputs);
        feedback.leave(2, &mut outputs);
        assert_eq!(
            outputs.take(),
            [
                Call::CancelManager {
                    target: Target::Device(18)
                },
                Call::CloseLight { device: 18 },
                Call::CancelVibrator {
                    target: Target::Phone
                },
            ]
        );
        // Nothing of it is left: no rumble, no battery read.
        feedback.rumble(0, 0xffff, 0xffff, &mut outputs);
        assert_eq!(outputs.take(), []);
        assert_eq!(feedback.next_deadline(), None);
    }

    #[test]
    fn the_stop_cancels_every_vibration_before_it_gives_the_lights_back() {
        let mut outputs = Scripted::default();
        let mut feedback = Feedback::default();
        feedback.seat(seat(18, 0, Rumble::Dual(Owner::Device), true), 0);
        feedback.seat(seat(20, 1, Rumble::None, false), 0);
        feedback.rumble(0, 0xffff, 0, &mut outputs);
        feedback.light(0, (9, 9, 9), &mut outputs);
        outputs.take();
        feedback.stop(&mut outputs);
        assert_eq!(
            outputs.take(),
            [
                Call::CancelManager {
                    target: Target::Device(18)
                },
                Call::CancelVibrator {
                    target: Target::Phone
                },
                Call::CloseLight { device: 18 },
            ]
        );
    }

    #[test]
    fn a_failed_call_is_counted_and_the_others_still_go() {
        let mut outputs = Scripted {
            failing: vec![Target::Device(18)],
            ..Default::default()
        };
        let mut feedback = Feedback::default();
        feedback.seat(seat(18, 0, Rumble::Dual(Owner::Device), false), 0);
        feedback.seat(seat(20, 0, Rumble::Dual(Owner::Device), false), 0);
        feedback.rumble(0, 0x0100, 0x0100, &mut outputs);
        assert_eq!(outputs.take().len(), 2);
        assert!(
            feedback.report().ends_with("; 1 failed"),
            "{}",
            feedback.report()
        );
    }

    fn battery(status: i32, capacity: f32) -> Option<Battery> {
        Some(Battery { status, capacity })
    }

    #[test]
    fn the_battery_goes_to_the_host_at_the_arrival_and_on_each_change() {
        let mut outputs = Scripted::default();
        let mut sink = Recorded::default();
        let mut feedback = Feedback::default();
        outputs.set_battery(18, battery(status::DISCHARGING, 0.55));
        feedback.seat(seat(18, 2, Rumble::Dual(Owner::Device), true), 1000);
        assert_eq!(feedback.next_deadline(), Some(1000));
        feedback.tick(1000, &mut outputs, &mut sink);
        assert_eq!(
            sink.packets,
            [Packet::Battery {
                number: 2,
                state: battery_state::DISCHARGING,
                percentage: 55
            }]
        );
        // Every 120 s; the same state is not sent again.
        assert_eq!(feedback.next_deadline(), Some(1000 + BATTERY_RECHECK_MS));
        feedback.tick(1000 + BATTERY_RECHECK_MS, &mut outputs, &mut sink);
        assert_eq!(sink.packets.len(), 1);
        outputs.set_battery(18, battery(status::CHARGING, f32::NAN));
        feedback.tick(1000 + BATTERY_RECHECK_MS - 1, &mut outputs, &mut sink);
        assert_eq!(sink.packets.len(), 1, "read before its time");
        feedback.tick(1000 + 2 * BATTERY_RECHECK_MS, &mut outputs, &mut sink);
        outputs.set_battery(18, battery(status::FULL, f32::NAN));
        feedback.tick(1000 + 3 * BATTERY_RECHECK_MS, &mut outputs, &mut sink);
        assert_eq!(
            sink.packets[1..],
            [
                Packet::Battery {
                    number: 2,
                    state: battery_state::CHARGING,
                    percentage: PERCENTAGE_UNKNOWN
                },
                Packet::Battery {
                    number: 2,
                    state: battery_state::FULL,
                    percentage: PERCENTAGE_UNKNOWN
                },
            ]
        );
        assert!(
            feedback
                .report()
                .contains("4 battery reads, 3 battery reports sent"),
            "{}",
            feedback.report()
        );
    }

    #[test]
    fn no_battery_or_a_status_android_does_not_name_sends_nothing() {
        let mut outputs = Scripted::default();
        let mut sink = Recorded::default();
        let mut feedback = Feedback::default();
        outputs.set_battery(18, None);
        outputs.set_battery(20, battery(9, 0.5));
        outputs.set_battery(21, battery(status::NOT_CHARGING, 1.0));
        feedback.seat(seat(18, 0, Rumble::None, false), 0);
        feedback.seat(seat(20, 1, Rumble::None, false), 0);
        feedback.seat(seat(21, 2, Rumble::None, false), 0);
        feedback.tick(0, &mut outputs, &mut sink);
        assert_eq!(
            sink.packets,
            [Packet::Battery {
                number: 2,
                state: battery_state::NOT_CHARGING,
                percentage: 100
            }]
        );
        // The unnamed status is read again, and still not sent.
        outputs.set_battery(20, battery(status::UNKNOWN, 0.0));
        feedback.tick(BATTERY_RECHECK_MS, &mut outputs, &mut sink);
        assert_eq!(
            sink.packets[1],
            Packet::Battery {
                number: 1,
                state: battery_state::UNKNOWN,
                percentage: 0
            }
        );
    }

    #[test]
    fn a_battery_report_outside_the_stream_is_refused_by_the_gate() {
        let gate = Gate::closed();
        let mut sink = Gated {
            gate: &gate,
            sink: Recorded::default(),
        };
        let mut outputs = Scripted::default();
        let mut feedback = Feedback::default();
        outputs.set_battery(18, battery(status::FULL, 1.0));
        feedback.seat(seat(18, 0, Rumble::None, false), 0);
        feedback.tick(0, &mut outputs, &mut sink);
        assert!(sink.sink.packets.is_empty());
        assert!(
            feedback
                .report()
                .contains("0 battery reports sent, 1 outside the stream"),
            "{}",
            feedback.report()
        );
        assert_eq!(
            sink.send(Packet::Battery {
                number: 0,
                state: 0,
                percentage: 0
            }),
            CLOSED
        );
    }

    /// The core's callbacks share one process-wide way to the feedback
    /// thread: the tests that use it take turns.
    static CORE_TESTS: StdMutex<()> = StdMutex::new(());

    fn clock() -> i64 {
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        START.get_or_init(Instant::now).elapsed().as_millis() as i64
    }

    /// Wait until the feedback thread has made `count` calls.
    fn wait_for(outputs: &Scripted, count: usize) -> Vec<Call> {
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until && outputs.calls.lock().unwrap().len() < count {
            std::thread::sleep(Duration::from_millis(5));
        }
        outputs.calls.lock().unwrap().clone()
    }

    #[test]
    fn the_cores_callbacks_reach_the_feedback_thread_only_while_it_serves() {
        let _turn = CORE_TESTS.lock().unwrap_or_else(PoisonError::into_inner);
        let outputs = Scripted::default();
        let made = outputs.clone();
        let mut worker = Worker::start(move || Ok(made), Recorded::default(), clock).unwrap();
        let mut seats = worker.seats();
        seats.seated(seat(18, 0, Rumble::Dual(Owner::Device), true));
        // Before it serves the core: nothing.
        unsafe { rumble(0, 0xffff, 0xffff) };
        worker.serve_core();
        // The core's own thread, which is not attached to the VM.
        std::thread::spawn(|| unsafe {
            rumble(0, 0x8000, 0x4000);
            set_controller_led(0, 1, 2, 3);
            rumble_triggers(0, 0xffff, 0xffff);
        })
        .join()
        .unwrap();
        let calls = wait_for(&outputs, 2);
        assert_eq!(
            calls,
            [
                Call::Combined {
                    target: Target::Device(18),
                    amplitudes: vec![0x40, 0x80]
                },
                Call::Light {
                    device: 18,
                    argb: 0xff01_0203
                },
            ]
        );
        assert!(worker.stop(STOP_WAIT));
        // The stop stopped the rumble and the light before it answered.
        assert_eq!(
            outputs.calls.lock().unwrap()[2..],
            [
                Call::CancelManager {
                    target: Target::Device(18)
                },
                Call::CancelVibrator {
                    target: Target::Phone
                },
                Call::CloseLight { device: 18 },
            ]
        );
        // After the stop the core's callbacks reach nothing.
        unsafe { rumble(0, 0xffff, 0xffff) };
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(outputs.calls.lock().unwrap().len(), 5);
    }

    #[test]
    fn a_stop_does_not_wait_for_a_call_that_hangs() {
        let _turn = CORE_TESTS.lock().unwrap_or_else(PoisonError::into_inner);
        let outputs = Scripted {
            delay: Duration::from_secs(2),
            ..Default::default()
        };
        let mut worker = Worker::start(move || Ok(outputs), Recorded::default(), clock).unwrap();
        worker.serve_core();
        worker
            .seats()
            .seated(seat(18, 0, Rumble::Dual(Owner::Device), false));
        unsafe { rumble(0, 0xffff, 0xffff) };
        let began = Instant::now();
        assert!(!worker.stop(Duration::from_millis(200)));
        assert!(
            began.elapsed() < Duration::from_secs(1),
            "{:?}",
            began.elapsed()
        );
    }

    #[test]
    fn a_panic_on_the_feedback_thread_ends_the_feedback_and_the_stop_still_runs() {
        let _turn = CORE_TESTS.lock().unwrap_or_else(PoisonError::into_inner);
        let outputs = Scripted {
            panics: true,
            ..Default::default()
        };
        let mut worker = Worker::start(move || Ok(outputs), Recorded::default(), clock).unwrap();
        worker.serve_core();
        worker
            .seats()
            .seated(seat(18, 0, Rumble::Dual(Owner::Device), false));
        unsafe { rumble(0, 0xffff, 0xffff) };
        unsafe { rumble(0, 0xffff, 0xffff) };
        // The thread is alive and answers, though its calls panicked.
        assert!(worker.stop(STOP_WAIT));
    }

    #[test]
    fn without_androids_side_the_feedback_thread_still_answers_the_stop() {
        let mut worker = Worker::start(
            || -> Result<Scripted> { anyhow::bail!("no VM") },
            Recorded::default(),
            clock,
        )
        .unwrap();
        worker.seats().seated(seat(18, 0, Rumble::None, false));
        assert!(worker.stop(STOP_WAIT));
    }

    /// The core's thread: rumble after rumble for controller 0, as a host
    /// may send them, and at the end both motors at 0.
    fn rumbles_then_zero(count: u32) {
        std::thread::spawn(move || unsafe {
            for at in 0..count {
                let value = (((at % 254) + 1) << 8) as u16;
                rumble(0, value, value);
            }
            rumble(0, 0, 0);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn rumble_behind_a_slow_call_collapses_and_the_stop_is_not_kept_waiting() {
        let _turn = CORE_TESTS.lock().unwrap_or_else(PoisonError::into_inner);
        // Each call into Android takes 100 ms: a Binder call that stalls.
        let outputs = Scripted {
            delay: Duration::from_millis(100),
            ..Default::default()
        };
        let made = outputs.clone();
        let mut worker = Worker::start(move || Ok(made), Recorded::default(), clock).unwrap();
        worker
            .seats()
            .seated(seat(18, 0, Rumble::Dual(Owner::Device), false));
        worker.serve_core();
        rumbles_then_zero(2000);
        let began = Instant::now();
        assert!(worker.stop(STOP_WAIT), "the stop waited behind the rumble");
        assert!(
            began.elapsed() < Duration::from_secs(1),
            "{:?}",
            began.elapsed()
        );
        let calls = outputs.calls.lock().unwrap().clone();
        // A few rumbles at most, then the stop's cancels.
        assert!(calls.len() <= 5, "{} calls: {calls:?}", calls.len());
        assert_eq!(
            calls[calls.len() - 2..],
            [
                Call::CancelManager {
                    target: Target::Device(18)
                },
                Call::CancelVibrator {
                    target: Target::Phone
                },
            ]
        );
    }

    #[test]
    fn the_latest_rumble_wins_so_a_rumble_of_zero_is_never_lost() {
        let _turn = CORE_TESTS.lock().unwrap_or_else(PoisonError::into_inner);
        let outputs = Scripted {
            delay: Duration::from_millis(50),
            ..Default::default()
        };
        let made = outputs.clone();
        let mut worker = Worker::start(move || Ok(made), Recorded::default(), clock).unwrap();
        worker
            .seats()
            .seated(seat(18, 0, Rumble::Dual(Owner::Device), false));
        worker.serve_core();
        rumbles_then_zero(500);
        // The motors stop without a stop: the last value was 0.
        let until = Instant::now() + Duration::from_secs(5);
        let last = || outputs.calls.lock().unwrap().last().cloned();
        while Instant::now() < until
            && last()
                != Some(Call::CancelManager {
                    target: Target::Device(18),
                })
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        let calls = outputs.calls.lock().unwrap().clone();
        assert_eq!(
            calls.last(),
            Some(&Call::CancelManager {
                target: Target::Device(18)
            }),
            "{} calls",
            calls.len()
        );
        assert!(calls.len() <= 4, "{} calls: {calls:?}", calls.len());
        assert!(worker.stop(STOP_WAIT));
    }

    #[test]
    fn the_stop_goes_before_pending_work_and_cancels_the_vibration_first() {
        let _turn = CORE_TESTS.lock().unwrap_or_else(PoisonError::into_inner);
        let outputs = Scripted {
            delay: Duration::from_millis(300),
            ..Default::default()
        };
        let made = outputs.clone();
        let mut worker = Worker::start(move || Ok(made), Recorded::default(), clock).unwrap();
        let mut seats = worker.seats();
        seats.seated(seat(18, 0, Rumble::Dual(Owner::Device), true));
        seats.seated(seat(20, 1, Rumble::Dual(Owner::Device), true));
        worker.serve_core();
        unsafe { rumble(0, 0xff00, 0xff00) };
        // The first rumble's call is under way; more waits behind it.
        std::thread::sleep(Duration::from_millis(100));
        unsafe {
            rumble(1, 0xff00, 0xff00);
            set_controller_led(0, 1, 2, 3);
            set_controller_led(1, 4, 5, 6);
        }
        assert!(worker.stop(STOP_WAIT));
        assert_eq!(
            *outputs.calls.lock().unwrap(),
            [
                Call::Combined {
                    target: Target::Device(18),
                    amplitudes: vec![0xff, 0xff]
                },
                // Nothing that waited: the stop, its cancels first.
                Call::CancelManager {
                    target: Target::Device(18)
                },
                Call::CancelManager {
                    target: Target::Device(20)
                },
                Call::CancelVibrator {
                    target: Target::Phone
                },
            ]
        );
    }

    /// The feedback thread's clock, which counts how often it is read.
    static CLOCK_READS: AtomicUsize = AtomicUsize::new(0);

    fn counted_clock() -> i64 {
        CLOCK_READS.fetch_add(1, AtomicOrdering::SeqCst);
        clock()
    }

    #[test]
    fn a_feedback_thread_that_panicked_waits_without_spinning() {
        // The first battery read panics, before it sets the next one.
        let outputs = Scripted {
            battery_panics: true,
            ..Default::default()
        };
        let mut worker =
            Worker::start(move || Ok(outputs), Recorded::default(), counted_clock).unwrap();
        worker.seats().seated(seat(18, 0, Rumble::None, false));
        std::thread::sleep(Duration::from_millis(300));
        let before = CLOCK_READS.load(AtomicOrdering::SeqCst);
        std::thread::sleep(Duration::from_millis(300));
        let reads = CLOCK_READS.load(AtomicOrdering::SeqCst) - before;
        assert!(reads < 10, "the clock was read {reads} times in 300 ms");
        assert!(worker.stop(STOP_WAIT));
    }

    #[test]
    fn a_battery_read_that_ends_after_a_stop_that_timed_out_reaches_no_stream() {
        let outputs = Scripted {
            battery_delay: Duration::from_millis(600),
            ..Default::default()
        };
        outputs.set_battery(18, battery(status::FULL, 1.0));
        let sent = Arc::new(StdMutex::new(Vec::new()));
        struct Shared(Arc<StdMutex<Vec<Packet>>>);
        impl Sink for Shared {
            fn send(&mut self, packet: Packet) -> i32 {
                self.0.lock().unwrap().push(packet);
                0
            }
        }
        let mut worker = Worker::start(move || Ok(outputs), Shared(sent.clone()), clock).unwrap();
        worker.seats().seated(seat(18, 0, Rumble::None, false));
        // The battery read is under way when the stream stops.
        std::thread::sleep(Duration::from_millis(100));
        assert!(!worker.stop(Duration::from_millis(100)));
        // It ends after the stop: a new stream may be running by then.
        std::thread::sleep(Duration::from_millis(1000));
        assert_eq!(
            *sent.lock().unwrap(),
            [],
            "a stopped stream's battery went on"
        );
    }

    #[test]
    fn the_feedback_thread_reads_a_battery_when_it_is_due() {
        let outputs = Scripted::default();
        outputs.set_battery(18, battery(status::DISCHARGING, 0.6));
        let sink = Arc::new(StdMutex::new(Vec::new()));
        struct Shared(Arc<StdMutex<Vec<Packet>>>);
        impl Sink for Shared {
            fn send(&mut self, packet: Packet) -> i32 {
                self.0.lock().unwrap().push(packet);
                0
            }
        }
        let made = outputs.clone();
        let mut worker = Worker::start(move || Ok(made), Shared(sink.clone()), clock).unwrap();
        worker.seats().seated(seat(18, 3, Rumble::None, false));
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until && sink.lock().unwrap().is_empty() {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(worker.stop(STOP_WAIT));
        assert_eq!(
            *sink.lock().unwrap(),
            [Packet::Battery {
                number: 3,
                state: battery_state::DISCHARGING,
                percentage: 60
            }]
        );
    }
}
