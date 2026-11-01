//! Native gamepad events to semantic presses: a port of
//! `clients/portal/src/input/native-gamepad-mapper.ts`, unchanged in its
//! codes, baselines, axes and timing (first repeat after 400 ms, then every
//! 100 ms). Two additions belong to the shell, not the portal: L1+R1
//! (`BTN_TL` 0x136 with `BTN_TR` 0x137) pressed together asks for the next
//! surface, and outputs are returned instead of emitted through a listener.

use std::collections::{BTreeMap, BTreeSet, HashMap};

const EV_KEY: u32 = 1;
const EV_ABS: u32 = 3;
const ABS_X: u16 = 0;
const ABS_Y: u16 = 1;
const ABS_HAT0X: u16 = 16;
const ABS_HAT0Y: u16 = 17;
const BTN_TL: u16 = 0x136;
const BTN_TR: u16 = 0x137;
pub const REPEAT_DELAY: f64 = 400.0;
pub const REPEAT_INTERVAL: f64 = 100.0;
/// `portal_input.rs`: contracts/input MAX_PHYSICAL_SOURCES plus 16.
pub const MAX_DEVICES: usize = 255 + 16;

/// BTN_MODE/Home and system actions belong to the host; they are not here.
const BUTTONS: [(u16, Action); 5] = [
    (0x130, Action::Confirm),
    (0x131, Action::Back),
    (0x13a, Action::Back),
    (0x134, Action::Options),
    (0x13b, Action::Menu),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Confirm,
    Back,
    Options,
    Menu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Direction {
        direction: Direction,
        gesture: u64,
        repeat: bool,
    },
    DirectionEnd {
        direction: Direction,
        gesture: u64,
    },
    Button(Action),
    /// L1+R1: switch to the next surface.
    Switch,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub code: u16,
    pub minimum: f64,
    pub maximum: f64,
    pub flat: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
struct Hold {
    direction: Direction,
    gesture: u64,
    next_repeat: f64,
}

#[derive(Default)]
struct Device {
    initialized: bool,
    ready: bool,
    keys: BTreeSet<u16>,
    values: BTreeMap<u16, i64>,
    axes: HashMap<u16, Axis>,
    pending: Option<(BTreeSet<u16>, BTreeMap<u16, i64>)>,
    hold: Option<Hold>,
}

#[derive(Default)]
pub struct Mapper {
    devices: HashMap<String, Device>,
    initialized: bool,
    next_gesture: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DeviceLimit;

impl Mapper {
    fn release(device: &mut Device, out: &mut Vec<Output>) {
        if let Some(hold) = device.hold.take() {
            out.push(Output::DirectionEnd {
                direction: hold.direction,
                gesture: hold.gesture,
            });
        }
    }

    pub fn configure_device(
        &mut self,
        id: &str,
        gamepad: bool,
        axes: &[Axis],
    ) -> Result<Vec<Output>, DeviceLimit> {
        let out = self.clear_device(id);
        if !gamepad {
            return Ok(out);
        }
        if self.devices.len() >= MAX_DEVICES {
            return Err(DeviceLimit);
        }
        self.devices.insert(
            id.to_owned(),
            Device {
                axes: axes.iter().map(|a| (a.code, *a)).collect(),
                ..Default::default()
            },
        );
        Ok(out)
    }

    /// `None`: the whole subscription completed initialization.
    pub fn initialize(&mut self, device_id: Option<&str>, active: bool) {
        if device_id.is_none() {
            self.initialized = true;
        }
        let initialized = self.initialized;
        for (id, device) in &mut self.devices {
            if device_id.is_some_and(|d| d != id) {
                continue;
            }
            commit(device); // initialization completion commits a silent whole baseline
            device.initialized = true;
            // Arm a neutral baseline immediately; a held baseline must reach
            // full neutral before any button or direction on it can fire.
            device.ready = initialized && active && neutral(device);
        }
    }

    /// One native `input` event. `Err` for an unknown device.
    pub fn handle(
        &mut self,
        device_id: &str,
        kind: u32,
        code: u16,
        value: i64,
        now: f64,
        active: bool,
    ) -> Result<Vec<Output>, UnknownDevice> {
        let mut out = Vec::new();
        if !self.devices.contains_key(device_id) {
            return Err(UnknownDevice);
        }
        if !active {
            self.reset_into(&mut out);
            return Ok(out);
        }
        let initialized = self.initialized;
        let device = self.devices.get_mut(device_id).expect("checked above");
        if kind == 0 && code == 0 && value == 0 {
            // EV_SYN/SYN_REPORT: one measured sample, never partial deltas.
            let previous = device.keys.clone();
            let ready = device.ready;
            commit(device);
            reconcile(device, &mut self.next_gesture, now, initialized, &mut out);
            if ready && initialized && device.initialized {
                for (code, action) in BUTTONS {
                    if device.keys.contains(&code) && !previous.contains(&code) {
                        out.push(Output::Button(action));
                    }
                }
                let both = |keys: &BTreeSet<u16>| keys.contains(&BTN_TL) && keys.contains(&BTN_TR);
                if both(&device.keys) && !both(&previous) {
                    out.push(Output::Switch);
                }
            }
        } else if kind == EV_KEY || kind == EV_ABS {
            let pending = device
                .pending
                .get_or_insert_with(|| (device.keys.clone(), device.values.clone()));
            if kind == EV_KEY {
                if value == 0 {
                    pending.0.remove(&code);
                } else {
                    pending.0.insert(code);
                }
            } else {
                pending.1.insert(code, value);
            }
        }
        Ok(out)
    }

    pub fn tick(&mut self, now: f64, active: bool) -> Vec<Output> {
        let mut out = Vec::new();
        if !active {
            self.reset_into(&mut out);
            return out;
        }
        let initialized = self.initialized;
        let mut ids: Vec<_> = self.devices.keys().cloned().collect();
        ids.sort();
        for id in ids {
            let device = self.devices.get_mut(&id).expect("listed");
            reconcile(device, &mut self.next_gesture, now, initialized, &mut out);
        }
        out
    }

    pub fn clear_device(&mut self, id: &str) -> Vec<Output> {
        let mut out = Vec::new();
        if let Some(mut device) = self.devices.remove(id) {
            Self::release(&mut device, &mut out);
        }
        out
    }

    pub fn reset(&mut self) -> Vec<Output> {
        let mut out = Vec::new();
        self.reset_into(&mut out);
        out
    }

    fn reset_into(&mut self, out: &mut Vec<Output>) {
        self.initialized = false;
        let mut ids: Vec<_> = self.devices.keys().cloned().collect();
        ids.sort();
        for id in ids {
            let mut device = self.devices.remove(&id).expect("listed");
            Self::release(&mut device, out);
        }
    }

    /// Whether a held direction could still repeat, so the caller keeps a timer.
    pub fn holding(&self) -> bool {
        self.devices.values().any(|d| d.hold.is_some())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct UnknownDevice;

fn commit(device: &mut Device) {
    if let Some((keys, values)) = device.pending.take() {
        device.keys = keys;
        device.values = values;
    }
}

fn reconcile(
    device: &mut Device,
    next_gesture: &mut u64,
    now: f64,
    initialized: bool,
    out: &mut Vec<Output>,
) {
    if !(initialized && device.initialized) {
        return;
    }
    if !device.ready {
        if neutral(device) {
            device.ready = true;
        }
        return; // the neutral transition only arms; it never fires
    }
    let direction = direction_for(device);
    if device
        .hold
        .is_some_and(|hold| Some(hold.direction) != direction)
    {
        Mapper::release(device, out);
    }
    let Some(direction) = direction else {
        return;
    };
    match &mut device.hold {
        None => {
            *next_gesture += 1;
            device.hold = Some(Hold {
                direction,
                gesture: *next_gesture,
                next_repeat: now + REPEAT_DELAY,
            });
            out.push(Output::Direction {
                direction,
                gesture: *next_gesture,
                repeat: false,
            });
        }
        Some(hold) if now >= hold.next_repeat => {
            hold.next_repeat = now + REPEAT_INTERVAL; // no catch-up burst after a busy frame
            out.push(Output::Direction {
                direction,
                gesture: hold.gesture,
                repeat: true,
            });
        }
        Some(_) => {}
    }
}

fn normalized(value: f64, axis: &Axis) -> f64 {
    let center = (axis.minimum + axis.maximum) / 2.0;
    let offset = value - center;
    if axis.flat.is_some_and(|flat| offset.abs() <= flat) {
        return 0.0;
    }
    let extent = (axis.maximum - center).max(center - axis.minimum);
    if extent <= 0.0 {
        0.0
    } else {
        (offset / extent).clamp(-1.0, 1.0)
    }
}

fn stick(device: &Device) -> (f64, f64, f64) {
    let value = |code| device.values.get(&code).map(|v| *v as f64);
    match (device.axes.get(&ABS_X), device.axes.get(&ABS_Y)) {
        // Legacy requires BOTH metadata ranges; no device-name heuristics.
        (Some(x), Some(y)) if x.maximum > x.minimum && y.maximum > y.minimum => (
            normalized(value(ABS_X).unwrap_or((x.minimum + x.maximum) / 2.0), x),
            normalized(value(ABS_Y).unwrap_or((y.minimum + y.maximum) / 2.0), y),
            0.6,
        ),
        _ => (
            value(ABS_X).unwrap_or(0.0),
            value(ABS_Y).unwrap_or(0.0),
            16_000.0,
        ),
    }
}

fn direction_for(device: &Device) -> Option<Direction> {
    let key = |code| i64::from(device.keys.contains(&code));
    let or_hat = |d: i64, code| {
        if d != 0 {
            d
        } else {
            *device.values.get(&code).unwrap_or(&0)
        }
    };
    let horizontal = or_hat(key(0x223) - key(0x222), ABS_HAT0X);
    let vertical = or_hat(key(0x221) - key(0x220), ABS_HAT0Y);
    // One gesture per source, D-pad priority.
    if vertical != 0 {
        return Some(if vertical > 0 {
            Direction::Down
        } else {
            Direction::Up
        });
    }
    if horizontal != 0 {
        return Some(if horizontal > 0 {
            Direction::Right
        } else {
            Direction::Left
        });
    }
    let (x, y, threshold) = stick(device);
    if x.abs() > y.abs() {
        if x > threshold {
            return Some(Direction::Right);
        }
        if x < -threshold {
            return Some(Direction::Left);
        }
    } else {
        if y > threshold {
            return Some(Direction::Down);
        }
        if y < -threshold {
            return Some(Direction::Up);
        }
    }
    None
}

fn neutral(device: &Device) -> bool {
    if !device.keys.is_empty() || direction_for(device).is_some() {
        return false;
    }
    // Triggers (ABS_Z/ABS_RZ) rest at their minimum.
    for code in [2, 5] {
        let axis = device.axes.get(&code);
        if let Some(value) = device.values.get(&code) {
            let minimum = axis.map_or(0.0, |a| a.minimum);
            let flat = axis.and_then(|a| a.flat).unwrap_or(0.0);
            if *value as f64 > minimum + flat {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    //! The cases of `native-gamepad-mapper.test.ts`, plus L1+R1.
    use super::*;

    struct Session {
        mapper: Mapper,
        out: Vec<Output>,
        now: f64,
        active: bool,
    }

    impl Session {
        fn new() -> Self {
            let mut mapper = Mapper::default();
            mapper.configure_device("pad", true, &[]).unwrap();
            Self {
                mapper,
                out: vec![],
                now: 0.0,
                active: true,
            }
        }
        fn initialize(&mut self) {
            self.mapper.initialize(None, self.active);
        }
        fn tick(&mut self, time: f64) {
            self.now = time;
            let out = self.mapper.tick(time, self.active);
            self.out.extend(out);
        }
        fn event(&mut self, device: &str, kind: u32, code: u16, value: i64) {
            let out = self
                .mapper
                .handle(device, kind, code, value, self.now, self.active)
                .unwrap();
            self.out.extend(out);
        }
        fn report(&mut self, device: &str) {
            self.event(device, 0, 0, 0);
        }
        fn key(&mut self, code: u16, value: i64) {
            self.key_on("pad", code, value, true);
        }
        fn key_on(&mut self, device: &str, code: u16, value: i64, report: bool) {
            self.event(device, 1, code, value);
            if report {
                self.report(device);
            }
        }
        fn axis(&mut self, code: u16, value: i64) {
            self.axis_on("pad", code, value, true);
        }
        fn axis_on(&mut self, device: &str, code: u16, value: i64, report: bool) {
            self.event(device, 3, code, value);
            if report {
                self.report(device);
            }
        }
        fn buttons(&self) -> Vec<Action> {
            self.out
                .iter()
                .filter_map(|o| match o {
                    Output::Button(a) => Some(*a),
                    _ => None,
                })
                .collect()
        }
    }

    #[test]
    fn accepts_all_sources_with_independent_baselines_and_a_cap() {
        let mut mapper = Mapper::default();
        let mut out = vec![];
        let count = MAX_DEVICES;
        let key = |mapper: &mut Mapper, id: &str, value: i64, out: &mut Vec<Output>| {
            out.extend(mapper.handle(id, 1, 0x130, value, 0.0, true).unwrap());
            out.extend(mapper.handle(id, 0, 0, 0, 0.0, true).unwrap());
        };
        for index in 0..count {
            let id = format!("source-{index}");
            mapper.configure_device(&id, true, &[]).unwrap();
            key(&mut mapper, &id, 1, &mut out);
        }
        mapper.initialize(None, true);
        assert!(out.is_empty());
        assert_eq!(
            mapper.configure_device("over-budget", true, &[]),
            Err(DeviceLimit)
        );
        for index in 0..count {
            let id = format!("source-{index}");
            key(&mut mapper, &id, 0, &mut out);
            key(&mut mapper, &id, 1, &mut out);
        }
        assert_eq!(out.len(), count);
        assert!(out.iter().all(|o| *o == Output::Button(Action::Confirm)));
    }

    #[test]
    fn never_arms_from_a_transient_neutral_inside_a_sample() {
        let mut s = Session::new();
        s.key_on("pad", 0x13b, 1, false);
        s.initialize();
        s.key_on("pad", 0x13b, 0, false);
        s.tick(1000.0);
        s.key_on("pad", 0x130, 1, false);
        assert!(s.out.is_empty());
        s.report("pad");
        s.tick(2000.0);
        assert!(s.out.is_empty());
        s.key(0x130, 0);
        s.key(0x130, 1);
        assert_eq!(s.out, vec![Output::Button(Action::Confirm)]);
    }

    #[test]
    fn reconciles_simultaneous_axes_once() {
        let mut s = Session::new();
        s.initialize();
        s.axis_on("pad", 0, 20_000, false);
        s.axis_on("pad", 1, -30_000, false);
        s.tick(0.0);
        assert!(s.out.is_empty());
        s.report("pad");
        assert!(matches!(
            s.out[..],
            [Output::Direction {
                direction: Direction::Up,
                ..
            }]
        ));
        s.axis_on("pad", 0, 0, false);
        s.axis_on("pad", 1, 0, false);
        s.report("pad");
        s.key(0x130, 1);
        s.key(0x130, 0);
        assert!(matches!(
            s.out[..],
            [
                Output::Direction { .. },
                Output::DirectionEnd { .. },
                Output::Button(Action::Confirm)
            ]
        ));
    }

    #[test]
    fn never_fires_from_a_held_baseline() {
        let mut s = Session::new();
        s.key(0x130, 1);
        s.axis(16, 1);
        s.initialize();
        s.tick(1000.0);
        s.key(0x130, 0);
        s.key(0x131, 1);
        s.key(0x131, 0);
        assert!(s.out.is_empty());
        s.axis(16, 0);
        s.key(0x130, 1);
        assert_eq!(s.out, vec![Output::Button(Action::Confirm)]);
    }

    #[test]
    fn keeps_legacy_buttons_and_leaves_home_to_the_host() {
        let mut s = Session::new();
        s.initialize();
        for code in [0x130, 0x131, 0x13a, 0x134, 0x13b, 0x13c] {
            s.key(code, 1);
            s.key(code, 0);
        }
        assert_eq!(
            s.buttons(),
            vec![
                Action::Confirm,
                Action::Back,
                Action::Back,
                Action::Options,
                Action::Menu
            ]
        );
    }

    #[test]
    fn repeats_after_400_then_every_100_and_ends_once() {
        let mut s = Session::new();
        s.initialize();
        s.key(0x223, 1);
        s.tick(399.0);
        assert_eq!(s.out.len(), 1);
        s.tick(400.0);
        s.tick(500.0);
        s.tick(20_000.0);
        s.key(0x223, 0);
        s.tick(21_000.0);
        s.mapper.reset();
        let right = |repeat| Output::Direction {
            direction: Direction::Right,
            gesture: 1,
            repeat,
        };
        assert_eq!(
            s.out,
            vec![
                right(false),
                right(true),
                right(true),
                right(true),
                Output::DirectionEnd {
                    direction: Direction::Right,
                    gesture: 1
                }
            ]
        );
    }

    #[test]
    fn one_gesture_for_agreeing_stick_and_dpad() {
        let mut s = Session::new();
        s.initialize();
        s.axis(0, 20_000);
        s.axis(16, 1);
        assert_eq!(s.out.len(), 1);
        s.axis(16, -1);
        assert!(matches!(
            s.out[..],
            [
                Output::Direction { .. },
                Output::DirectionEnd { .. },
                Output::Direction {
                    direction: Direction::Left,
                    gesture: 2,
                    ..
                }
            ]
        ));
        let end = s.mapper.clear_device("pad");
        assert_eq!(
            end,
            vec![Output::DirectionEnd {
                direction: Direction::Left,
                gesture: 2
            }]
        );
    }

    #[test]
    fn low_range_metadata_dominant_axes_and_flat_dead_zones() {
        let mut s = Session::new();
        let axes = [
            Axis {
                code: 0,
                minimum: -1408.0,
                maximum: 1408.0,
                flat: Some(200.0),
            },
            Axis {
                code: 1,
                minimum: -1408.0,
                maximum: 1408.0,
                flat: Some(200.0),
            },
        ];
        s.mapper.configure_device("pad", true, &axes).unwrap();
        s.initialize();
        s.axis(0, 150);
        s.axis(1, -150);
        assert!(s.out.is_empty());
        s.axis(0, 1000);
        assert!(matches!(
            s.out.last(),
            Some(Output::Direction {
                direction: Direction::Right,
                ..
            })
        ));
        s.axis(1, -1200);
        assert!(matches!(
            s.out.last(),
            Some(Output::Direction {
                direction: Direction::Up,
                ..
            })
        ));
    }

    #[test]
    fn does_not_infer_scaling_from_incomplete_metadata() {
        let cases: [Vec<Axis>; 3] = [
            vec![],
            vec![Axis {
                code: 0,
                minimum: -1408.0,
                maximum: 1408.0,
                flat: None,
            }],
            vec![
                Axis {
                    code: 0,
                    minimum: 0.0,
                    maximum: 0.0,
                    flat: None,
                },
                Axis {
                    code: 1,
                    minimum: -1.0,
                    maximum: 1.0,
                    flat: None,
                },
            ],
        ];
        for axes in cases {
            let mut s = Session::new();
            s.mapper.configure_device("pad", true, &axes).unwrap();
            s.initialize();
            s.axis(0, 1000);
            assert!(s.out.is_empty());
        }
    }

    #[test]
    fn unsigned_centers_and_trigger_minima_in_held_baselines() {
        let mut s = Session::new();
        let axes = [0, 1, 2].map(|code| Axis {
            code,
            minimum: 0.0,
            maximum: 255.0,
            flat: None,
        });
        s.mapper.configure_device("pad", true, &axes).unwrap();
        s.axis(0, 128);
        s.axis(1, 128);
        s.axis(2, 255);
        s.initialize();
        s.key(0x130, 1);
        s.key(0x130, 0);
        assert!(s.out.is_empty());
        s.axis(2, 0);
        s.key(0x130, 1);
        assert_eq!(s.out, vec![Output::Button(Action::Confirm)]);
    }

    #[test]
    fn separates_held_arrival_for_concurrent_sources() {
        let mut s = Session::new();
        s.initialize();
        s.mapper.configure_device("second", true, &[]).unwrap();
        s.key_on("second", 0x130, 1, true);
        s.mapper.initialize(Some("second"), true);
        s.key(0x130, 1);
        s.key_on("second", 0x131, 1, true);
        assert_eq!(s.out, vec![Output::Button(Action::Confirm)]);
        s.key_on("second", 0x131, 0, true);
        s.key_on("second", 0x130, 0, true);
        s.key_on("second", 0x130, 1, true);
        assert_eq!(s.out.len(), 2);
    }

    #[test]
    fn inactive_releases_and_a_returning_held_state_never_fires() {
        let mut s = Session::new();
        s.initialize();
        s.axis(16, 1);
        s.active = false;
        s.tick(0.0);
        assert!(matches!(s.out.last(), Some(Output::DirectionEnd { .. })));
        s.tick(1000.0);
        s.active = true;
        s.mapper.configure_device("pad", true, &[]).unwrap();
        s.axis(16, 1);
        s.initialize();
        s.tick(2000.0);
        assert_eq!(s.out.len(), 2);
        s.axis(16, 0);
        s.axis(16, 1);
        assert!(matches!(
            s.out.last(),
            Some(Output::Direction { gesture: 2, .. })
        ));
    }

    #[test]
    fn l1_and_r1_together_switch_once() {
        let mut s = Session::new();
        s.initialize();
        s.key(BTN_TL, 1);
        assert!(s.out.is_empty());
        s.key(BTN_TR, 1);
        assert_eq!(s.out, vec![Output::Switch]);
        s.key(BTN_TR, 0);
        s.key(BTN_TR, 1);
        assert_eq!(s.out, vec![Output::Switch, Output::Switch]);
    }

    #[test]
    fn unknown_devices_are_refused() {
        let mut mapper = Mapper::default();
        assert_eq!(
            mapper.handle("ghost", 1, 0x130, 1, 0.0, true),
            Err(UnknownDevice)
        );
    }
}
