use crate::input_seat::{GamepadState, SeatBackend, SeatSpec};
use evdev::{
    uinput::VirtualDevice, AbsInfo, AbsoluteAxisCode, AttributeSet, BusType, EventType, InputEvent,
    InputId, KeyCode, UinputAbsSetup,
};
use std::{
    collections::BTreeMap, ffi::CString, fs, os::unix::fs::MetadataExt, thread, time::Duration,
};

const BUTTONS: &[(u32, KeyCode)] = &[
    (0x0010, KeyCode::BTN_START),
    (0x0020, KeyCode::BTN_SELECT),
    (0x0040, KeyCode::BTN_THUMBL),
    (0x0080, KeyCode::BTN_THUMBR),
    (0x0100, KeyCode::BTN_TL),
    (0x0200, KeyCode::BTN_TR),
    (0x0400, KeyCode::BTN_MODE),
    (0x1000, KeyCode::BTN_SOUTH),
    (0x2000, KeyCode::BTN_EAST),
    (0x4000, KeyCode::BTN_NORTH),
    (0x8000, KeyCode::BTN_WEST),
];
const DPAD_UP: u32 = 0x0001;
const DPAD_DOWN: u32 = 0x0002;
const DPAD_LEFT: u32 = 0x0004;
const DPAD_RIGHT: u32 = 0x0008;

struct SeatDevice {
    device: VirtualDevice,
    output: SeatOutput,
}

struct SeatOutput {
    state: Option<GamepadState>,
}

pub struct UinputSeatBackend {
    devices: BTreeMap<u8, SeatDevice>,
    event_gid: u32,
    preflight_complete: bool,
}

impl UinputSeatBackend {
    pub fn new(event_gid: u32) -> Self {
        Self {
            devices: BTreeMap::new(),
            event_gid,
            preflight_complete: false,
        }
    }

    fn reject_existing_seats(&self) -> Result<(), String> {
        let input_root = std::path::Path::new("/sys/class/input");
        for entry in fs::read_dir(input_root).map_err(display)? {
            let entry = entry.map_err(display)?;
            let file_name = entry.file_name();
            if !file_name.to_string_lossy().starts_with("event") {
                continue;
            }
            let device = entry.path().join("device");
            let name = fs::read_to_string(device.join("name")).ok();
            let physical = fs::read_to_string(device.join("phys")).ok();
            if is_seat_identity(
                name.as_deref().map(str::trim_end),
                physical.as_deref().map(str::trim_end),
            ) {
                return Err("a stale Korri input seat already exists".into());
            }
        }
        Ok(())
    }

    fn build_device(&self, spec: &SeatSpec) -> Result<VirtualDevice, String> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for (_, key) in BUTTONS {
            keys.insert(*key);
        }
        let physical = CString::new(spec.physical_path.as_str()).map_err(display)?;
        let mut builder = VirtualDevice::builder()
            .map_err(display)?
            .name(&spec.name)
            .input_id(InputId::new(BusType::BUS_USB, 0x045e, 0x028e, 1))
            .with_phys(&physical)
            .map_err(display)?
            .with_keys(&keys)
            .map_err(display)?;
        for (axis, minimum, maximum, flat) in [
            (AbsoluteAxisCode::ABS_X, -32768, 32767, 4096),
            (AbsoluteAxisCode::ABS_Y, -32768, 32767, 4096),
            (AbsoluteAxisCode::ABS_Z, 0, 255, 0),
            (AbsoluteAxisCode::ABS_RX, -32768, 32767, 4096),
            (AbsoluteAxisCode::ABS_RY, -32768, 32767, 4096),
            (AbsoluteAxisCode::ABS_RZ, 0, 255, 0),
            (AbsoluteAxisCode::ABS_HAT0X, -1, 1, 0),
            (AbsoluteAxisCode::ABS_HAT0Y, -1, 1, 0),
        ] {
            builder = builder
                .with_absolute_axis(&UinputAbsSetup::new(
                    axis,
                    AbsInfo::new(0, minimum, maximum, 0, flat, 0),
                ))
                .map_err(display)?;
        }
        builder.build().map_err(display)
    }

    fn wait_for_event_node(&self, device: &mut VirtualDevice) -> Result<(), String> {
        for _ in 0..100 {
            if let Ok(nodes) = device.enumerate_dev_nodes_blocking() {
                for path in nodes.flatten() {
                    if let Ok(metadata) = fs::metadata(path) {
                        if metadata.gid() == self.event_gid && metadata.mode() & 0o777 == 0o660 {
                            return Ok(());
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        Err("Korri seat event node did not reach the required group and mode".into())
    }
}

impl SeatBackend for UinputSeatBackend {
    fn create(&mut self, spec: &SeatSpec) -> Result<(), String> {
        if spec.slot == 0 || self.devices.contains_key(&spec.slot) {
            return Err("invalid or duplicate Korri seat slot".into());
        }
        if !self.preflight_complete {
            self.reject_existing_seats()?;
            self.preflight_complete = true;
        }
        let mut device = self.build_device(spec)?;
        self.wait_for_event_node(&mut device)?;
        self.devices.insert(
            spec.slot,
            SeatDevice {
                device,
                output: SeatOutput {
                    state: Some(GamepadState::neutral()),
                },
            },
        );
        Ok(())
    }

    fn write_state(&mut self, slot: u8, next: GamepadState) -> Result<(), String> {
        let seat = self
            .devices
            .get_mut(&slot)
            .ok_or_else(|| "Korri seat is not active".to_string())?;
        seat.output
            .write(next, |events| seat.device.emit(events).map_err(display))
    }

    fn destroy(&mut self, slot: u8) -> Result<(), String> {
        self.devices.remove(&slot);
        Ok(())
    }
}

impl SeatOutput {
    fn write(
        &mut self,
        next: GamepadState,
        emit: impl FnOnce(&[InputEvent]) -> Result<(), String>,
    ) -> Result<(), String> {
        let current = self.state;
        let mut events = Vec::new();
        for (mask, key) in BUTTONS {
            let before = current.map(|state| state.buttons & mask != 0);
            let after = next.buttons & mask != 0;
            if before != Some(after) {
                events.push(InputEvent::new(EventType::KEY.0, key.0, i32::from(after)));
            }
        }
        push_axis(
            &mut events,
            AbsoluteAxisCode::ABS_HAT0X,
            current.map(|state| hat(state.buttons, DPAD_LEFT, DPAD_RIGHT)),
            hat(next.buttons, DPAD_LEFT, DPAD_RIGHT),
        );
        push_axis(
            &mut events,
            AbsoluteAxisCode::ABS_HAT0Y,
            current.map(|state| hat(state.buttons, DPAD_UP, DPAD_DOWN)),
            hat(next.buttons, DPAD_UP, DPAD_DOWN),
        );
        push_axis(
            &mut events,
            AbsoluteAxisCode::ABS_Z,
            current.map(|state| state.left_trigger.into()),
            next.left_trigger.into(),
        );
        push_axis(
            &mut events,
            AbsoluteAxisCode::ABS_RZ,
            current.map(|state| state.right_trigger.into()),
            next.right_trigger.into(),
        );
        push_axis(
            &mut events,
            AbsoluteAxisCode::ABS_X,
            current.map(|state| state.left_stick_x.into()),
            next.left_stick_x.into(),
        );
        push_axis(
            &mut events,
            AbsoluteAxisCode::ABS_Y,
            current.map(|state| state.left_stick_y.into()),
            next.left_stick_y.into(),
        );
        push_axis(
            &mut events,
            AbsoluteAxisCode::ABS_RX,
            current.map(|state| state.right_stick_x.into()),
            next.right_stick_x.into(),
        );
        push_axis(
            &mut events,
            AbsoluteAxisCode::ABS_RY,
            current.map(|state| state.right_stick_y.into()),
            next.right_stick_y.into(),
        );
        if !events.is_empty() {
            // emit can apply part of a frame before failing. The next write must
            // establish every control, not diff against a state we no longer know.
            self.state = None;
            emit(&events)?;
        }
        self.state = Some(next);
        Ok(())
    }
}

fn push_axis(
    events: &mut Vec<InputEvent>,
    axis: AbsoluteAxisCode,
    before: Option<i32>,
    after: i32,
) {
    if before != Some(after) {
        events.push(InputEvent::new(EventType::ABSOLUTE.0, axis.0, after));
    }
}

fn hat(buttons: u32, negative: u32, positive: u32) -> i32 {
    match (buttons & negative != 0, buttons & positive != 0) {
        (true, false) => -1,
        (false, true) => 1,
        _ => 0,
    }
}

fn is_seat_identity(name: Option<&str>, physical_path: Option<&str>) -> bool {
    fn slot(value: &str, prefix: &str) -> bool {
        value.strip_prefix(prefix).is_some_and(|slot| {
            slot.parse::<std::num::NonZeroU8>()
                .is_ok_and(|number| number.to_string() == slot)
        })
    }
    name.is_some_and(|value| slot(value, "Korri Seat P"))
        || physical_path.is_some_and(|value| slot(value, "korri/input-seat/p"))
}

fn display(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::{is_seat_identity, GamepadState, InputEvent, SeatOutput, BUTTONS};

    #[test]
    fn failed_emit_requires_a_full_neutral_write_instead_of_trusting_the_old_cache() {
        let mut output = SeatOutput {
            state: Some(GamepadState::neutral()),
        };
        let mut observed = Vec::<InputEvent>::new();
        let held = GamepadState {
            buttons: 0x1000,
            left_stick_x: 20_000,
            ..GamepadState::neutral()
        };
        assert!(output
            .write(held, |events| {
                // The real emit call can fail after some events reached uinput.
                observed.push(events[0]);
                Err("partial write".into())
            })
            .is_err());
        assert_eq!(observed[0].value(), 1);

        let mut retry = Vec::new();
        output
            .write(GamepadState::neutral(), |events| {
                retry.extend_from_slice(events);
                Ok(())
            })
            .unwrap();
        assert_eq!(retry.len(), BUTTONS.len() + 8);
        assert!(retry.iter().all(|event| event.value() == 0));
        assert!(retry.iter().any(|event| event.code() == observed[0].code()));
    }

    #[test]
    fn successful_emit_keeps_delta_writes_and_deduplication() {
        let mut output = SeatOutput {
            state: Some(GamepadState::neutral()),
        };
        let held = GamepadState {
            buttons: 0x1000,
            ..GamepadState::neutral()
        };
        output
            .write(held, |events| {
                assert_eq!(events.len(), 1);
                assert_eq!(events[0].value(), 1);
                Ok(())
            })
            .unwrap();
        output
            .write(held, |_| panic!("unchanged state must not be emitted"))
            .unwrap();
        output
            .write(GamepadState::neutral(), |events| {
                assert_eq!(events.len(), 1);
                assert_eq!(events[0].value(), 0);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn only_exact_korri_seat_identities_are_reserved() {
        assert!(is_seat_identity(Some("Korri Seat P1"), None));
        assert!(is_seat_identity(None, Some("korri/input-seat/p4")));
        assert!(is_seat_identity(Some("Korri Seat P6"), None));
        assert!(is_seat_identity(None, Some("korri/input-seat/p255")));
        assert!(!is_seat_identity(Some("Korri Seat P0"), None));
        assert!(!is_seat_identity(Some("Korri Seat P01"), None));
        assert!(!is_seat_identity(Some("Korri Seat P256"), None));
        assert!(!is_seat_identity(Some("Korri Seat P+1"), None));
        assert!(!is_seat_identity(Some("Korri Seat P1 extra"), None));
        assert!(!is_seat_identity(None, Some("korri/input-seat/p1/extra")));
    }
}
