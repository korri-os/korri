//! Idle observation at the already authenticated/normalized input boundary.
//! This never routes input or synthesizes releases, keys, or pointer events.
use std::collections::BTreeSet;

use korri_input_core::controls::{Control, ControlTransition, DpadAxis};

use crate::dbus::SemanticInput;

pub const ACTIVITY_INTERVAL_MS: u64 = 1000;

#[derive(Default)]
pub struct ControllerActivity {
    keys: BTreeSet<u16>,
    controls: BTreeSet<Control>,
    axes: [bool; 6],
    hats: [i32; 2],
    semantic_hats: [i32; 2],
    pending: bool,
    last_sent: Option<u64>,
}

impl ControllerActivity {
    /// Topology loss cancels holds, but must not bypass the process rate limit.
    pub fn reset(&mut self) {
        *self = Self {
            last_sent: self.last_sent,
            ..Self::default()
        };
    }

    pub fn evdev(&mut self, event_type: u16, code: u16, value: i32) {
        match (event_type, code, value) {
            // The validated InputPlumber Xbox target, including ABXY (which
            // deliberately have no shortcut policy), Back and volume keys.
            (
                1,
                0x130 | 0x131 | 0x133 | 0x134 | 0x136..=0x13e | 0x2c0..=0x2c3 | 0x116 | 0x72 | 0x73,
                0..=1,
            ) => {
                self.pending |= if value == 1 {
                    self.keys.insert(code)
                } else {
                    self.keys.remove(&code)
                };
            }
            (3, 0..=5, _) => {
                // InputPlumber 0.75.2 target/xb360.rs supplies signed 16-bit
                // sticks and 0..255 triggers. Idle detection is not portal
                // navigation: partial travel during gameplay must count too.
                // Use a 1/8-travel stick deadzone with hysteresis; triggers
                // enter above 8/255 and leave at 4/255. Verify these noise
                // margins on hardware; no calibration data is persisted.
                let trigger = matches!(code, 2 | 5);
                if !(if trigger { 0..=255 } else { -32768..=32767 }).contains(&value) {
                    return;
                }
                let magnitude = if trigger {
                    value.max(0) as u32
                } else {
                    value.unsigned_abs()
                };
                let previous = self.axes[code as usize];
                let (enter, leave) = if trigger { (8, 4) } else { (4096, 3072) };
                let active = magnitude > if previous { leave } else { enter };
                self.pending |= previous != active;
                self.axes[code as usize] = active;
            }
            (3, 16..=17, -1..=1) => {
                let slot = &mut self.hats[(code - 16) as usize];
                self.pending |= *slot != value;
                *slot = value;
            }
            _ => {}
        }
    }

    pub fn semantic(&mut self, input: SemanticInput) {
        match input {
            SemanticInput::Control(control, transition) => {
                self.pending |= match transition {
                    ControlTransition::Pressed => self.controls.insert(control),
                    ControlTransition::Released => self.controls.remove(&control),
                };
            }
            SemanticInput::Axis(axis, value) => {
                let slot = &mut self.semantic_hats[match axis {
                    DpadAxis::Horizontal => 0,
                    DpadAxis::Vertical => 1,
                }];
                self.pending |= *slot != value;
                *slot = value;
            }
        }
    }

    pub fn take_due(&mut self, now_ms: u64) -> bool {
        let held = !self.keys.is_empty()
            || !self.controls.is_empty()
            || self.axes.contains(&true)
            || self.hats.iter().any(|v| *v != 0)
            || self.semantic_hats.iter().any(|v| *v != 0);
        if !(self.pending || held)
            || self
                .last_sent
                .is_some_and(|last| now_ms.saturating_sub(last) < ACTIVITY_INTERVAL_MS)
        {
            return false;
        }
        self.pending = false;
        self.last_sent = Some(now_ms);
        true
    }
}
