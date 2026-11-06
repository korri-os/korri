//! Android's stable KeyEvent/axis codes, translated at the host edge into
//! the Linux mapper's measured samples. No surface or focus knowledge here.
use super::{mapper::Axis, native::NativeEvent};
use std::collections::HashSet;

#[derive(Default)]
pub struct AndroidInput {
    devices: HashSet<i32>,
    inactive: bool,
}

/// Android KeyEvent constants -> Linux input codes. X is BTN_WEST (Options),
/// A is BTN_SOUTH (Confirm), independent of the labels printed on a controller.
fn key_code(code: u32) -> Option<u16> {
    Some(match code {
        19 => 0x220, // DPAD_UP
        20 => 0x221,
        21 => 0x222,
        22 => 0x223,
        23 | 66 | 96 => 0x130, // DPAD_CENTER, ENTER, BUTTON_A
        4 | 97 => 0x131,       // BACK, BUTTON_B
        99 => 0x134,           // BUTTON_X
        100 => 0x133,          // BUTTON_Y: no Linux surface action
        102 => 0x136,          // L1
        103 => 0x137,          // R1
        108 | 82 => 0x13b,     // START, MENU
        109 => 0x13a,          // SELECT
        110 => 0x13c,          // MODE: reserved by host, never sent to surface
        _ => return None,
    })
}

impl AndroidInput {
    pub fn set_active(&mut self, active: bool) {
        self.inactive = !active;
        if !active {
            self.devices.clear();
        }
    }

    pub fn removed(&mut self, device: i32) -> NativeEvent {
        self.devices.remove(&device);
        NativeEvent::DeviceRemoved(device.to_string())
    }

    fn attach(&mut self, device: i32, out: &mut Vec<NativeEvent>) {
        if self.devices.insert(device) {
            let id = device.to_string();
            out.push(NativeEvent::DeviceAdded {
                id: id.clone(),
                gamepad: true,
                axes: [0, 1]
                    .map(|code| Axis {
                        code,
                        minimum: -1000.0,
                        maximum: 1000.0,
                        flat: Some(200.0),
                    })
                    .to_vec(),
            });
            // Android sends deltas, not an evdev baseline. Begin neutral; the
            // mapper owns all subsequent edges, chords and direction repeats.
            out.push(NativeEvent::DeviceInitialized(id));
            if self.devices.len() == 1 {
                out.push(NativeEvent::Initialized);
            }
        }
    }

    pub fn key(&mut self, device: i32, code: u32, pressed: bool) -> Option<Vec<NativeEvent>> {
        let code = key_code(code)?;
        if self.inactive {
            return Some(Vec::new());
        }
        let mut out = Vec::new();
        self.attach(device, &mut out);
        out.push(input(device, 1, code, i64::from(pressed)));
        out.push(input(device, 0, 0, 0));
        Some(out)
    }

    /// Joystick X/Y are normalized by Android to [-1, 1]. The existing mapper
    /// applies its 0.6 activation threshold and dominant-axis/D-pad priority.
    pub fn axes(
        &mut self,
        device: i32,
        x: f32,
        y: f32,
        hat_x: f32,
        hat_y: f32,
    ) -> Vec<NativeEvent> {
        if self.inactive {
            return Vec::new();
        }
        let mut out = Vec::new();
        self.attach(device, &mut out);
        let normalized = |v: f32| {
            if v.is_finite() {
                v.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        };
        for (code, value) in [
            (0, (normalized(x) * 1000.0).round() as i64),
            (1, (normalized(y) * 1000.0).round() as i64),
            (16, normalized(hat_x).round() as i64),
            (17, normalized(hat_y).round() as i64),
        ] {
            out.push(input(device, 3, code, value));
        }
        out.push(input(device, 0, 0, 0));
        out
    }
}

fn input(device: i32, kind: u32, code: u16, value: i64) -> NativeEvent {
    NativeEvent::Input {
        device: device.to_string(),
        gamepad: true,
        kind,
        code,
        value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        input::{Press, Router},
        messages::Button,
    };

    fn route(router: &mut Router, events: Vec<NativeEvent>, now: f64) -> Vec<Press> {
        events
            .into_iter()
            .flat_map(|event| router.native(event, now))
            .collect()
    }

    #[test]
    fn android_keys_match_linux_semantics_and_suppress_os_repeat() {
        for (code, button) in [
            (19, Button::Up),
            (20, Button::Down),
            (21, Button::Left),
            (22, Button::Right),
            (23, Button::Confirm),
            (66, Button::Confirm),
            (96, Button::Confirm),
            (4, Button::Back),
            (97, Button::Back),
            (109, Button::Back),
            (99, Button::Options),
            (108, Button::Menu),
            (82, Button::Menu),
        ] {
            let mut source = AndroidInput::default();
            let mut router = Router::new(true);
            assert_eq!(
                route(&mut router, source.key(7, code, true).unwrap(), 0.0),
                vec![Press::Button(button)]
            );
            assert!(route(&mut router, source.key(7, code, true).unwrap(), 1.0).is_empty());
            route(&mut router, source.key(7, code, false).unwrap(), 2.0);
            assert!(!router.holding());
        }
        let mut source = AndroidInput::default();
        let mut router = Router::new(true);
        for code in [100, 110] {
            assert!(route(&mut router, source.key(7, code, true).unwrap(), 0.0).is_empty());
            route(&mut router, source.key(7, code, false).unwrap(), 0.0);
        }
        assert!(source.key(7, 24, true).is_none()); // volume belongs to Android
    }

    #[test]
    fn android_shoulders_switch_once_only_on_the_same_device() {
        let mut source = AndroidInput::default();
        let mut router = Router::new(true);
        assert!(route(&mut router, source.key(7, 102, true).unwrap(), 0.0).is_empty());
        assert!(route(&mut router, source.key(8, 103, true).unwrap(), 0.0).is_empty());
        assert_eq!(
            route(&mut router, source.key(7, 103, true).unwrap(), 0.0),
            vec![Press::Switch]
        );
        assert!(route(&mut router, source.key(7, 103, true).unwrap(), 1.0).is_empty());
    }

    #[test]
    fn android_disconnect_releases_repeat_and_allows_reconnection() {
        let mut source = AndroidInput::default();
        let mut router = Router::new(true);
        route(&mut router, source.key(7, 22, true).unwrap(), 0.0);
        assert!(router.holding());
        assert!(router.native(source.removed(7), 1.0).is_empty());
        assert!(!router.holding());
        assert!(router.tick(1000.0).is_empty());
        assert_eq!(
            route(&mut router, source.key(7, 96, true).unwrap(), 1001.0),
            vec![Press::Button(Button::Confirm)]
        );
    }

    #[test]
    fn android_axes_deadzone_dominance_hat_priority_repeat_and_release() {
        let mut source = AndroidInput::default();
        let mut router = Router::new(true);
        assert!(route(&mut router, source.axes(7, 0.6, -0.2, 0.0, 0.0), 0.0).is_empty());
        assert_eq!(
            route(&mut router, source.axes(7, 0.8, -0.9, 0.0, 0.0), 0.0),
            vec![Press::Button(Button::Up)]
        );
        assert!(router.tick(399.0).is_empty());
        assert_eq!(router.tick(400.0), vec![Press::Button(Button::Up)]);
        assert_eq!(router.tick(500.0), vec![Press::Button(Button::Up)]);
        assert_eq!(
            route(&mut router, source.axes(7, 0.8, -0.9, 1.0, 0.0), 501.0),
            vec![Press::Button(Button::Right)]
        );
        assert_eq!(
            route(&mut router, source.axes(7, 0.0, 0.0, -1.0, 1.0), 502.0),
            vec![Press::Button(Button::Down)]
        );
        assert!(route(&mut router, source.axes(7, f32::NAN, 0.0, 0.0, 0.0), 503.0).is_empty());
        assert!(!router.holding());
        route(&mut router, source.key(7, 19, true).unwrap(), 504.0);
        router.set_active(false, 505.0);
        source.set_active(false);
        assert!(!router.holding());
        // Android can drain a queued release/motion after LostFocus. It must
        // not register a source while Router is inactive and then leave it
        // known only at this edge (unknown to the mapper) on return.
        assert!(source.key(7, 19, false).unwrap().is_empty());
        assert!(source.axes(7, 1.0, 0.0, 0.0, 0.0).is_empty());
        router.set_active(true, 506.0);
        source.set_active(true);
        assert_eq!(
            route(&mut router, source.key(7, 19, true).unwrap(), 507.0),
            vec![Press::Button(Button::Up)]
        );
    }
}
