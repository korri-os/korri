//! Input to surface messages (contract 3.2).
//!
//! Gamepads arrive through korrid's native input subscription ([`native`])
//! and the portal's mapper ([`mapper`]). The window keyboard stays for
//! development: arrows, Return, Escape/Backspace, M, O, and F2 to switch.
//! Both repeat a held direction after 400 ms, then every 100 ms.

pub mod mapper;
pub mod native;

use crate::messages::Button;
use mapper::{Direction, Output, REPEAT_DELAY, REPEAT_INTERVAL};
use native::NativeEvent;

/// What input asks of the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Button(Button),
    /// L1+R1 or F2: the next surface.
    Switch,
}

/// Window keys the shell understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Return,
    Escape,
    Backspace,
    M,
    O,
    F2,
}

fn direction_button(direction: Direction) -> Button {
    match direction {
        Direction::Up => Button::Up,
        Direction::Down => Button::Down,
        Direction::Left => Button::Left,
        Direction::Right => Button::Right,
    }
}

#[derive(Default)]
pub struct Router {
    mapper: mapper::Mapper,
    /// Held keyboard directions, most recent last.
    held: Vec<Key>,
    next_key_repeat: f64,
    active: bool,
}

impl Router {
    pub fn new(active: bool) -> Self {
        Self {
            active,
            ..Default::default()
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }

    /// Window focus. Losing it releases everything held, as the portal does.
    pub fn set_active(&mut self, active: bool, now: f64) -> Vec<Press> {
        self.active = active;
        if !active {
            self.held.clear();
            let outputs = self.mapper.tick(now, false);
            return Self::convert(outputs);
        }
        Vec::new()
    }

    pub fn native(&mut self, event: NativeEvent, now: f64) -> Vec<Press> {
        let outputs = match event {
            NativeEvent::Disconnected => self.mapper.reset(),
            NativeEvent::Initialized => {
                if self.active {
                    self.mapper.initialize(None, true);
                    Vec::new()
                } else {
                    self.mapper.reset()
                }
            }
            NativeEvent::DeviceInitialized(id) => {
                self.mapper.initialize(Some(&id), self.active);
                Vec::new()
            }
            NativeEvent::DeviceAdded { id, gamepad, axes } => self
                .mapper
                .configure_device(&id, gamepad, &axes)
                .unwrap_or_default(),
            NativeEvent::DeviceRemoved(id) => self.mapper.clear_device(&id),
            NativeEvent::Input {
                device,
                gamepad: true,
                kind,
                code,
                value,
            } => self
                .mapper
                .handle(&device, kind, code, value, now, self.active)
                .unwrap_or_default(),
            NativeEvent::Input { .. } => Vec::new(),
        };
        Self::convert(outputs)
    }

    pub fn key(&mut self, key: Key, pressed: bool, now: f64) -> Vec<Press> {
        let direction = matches!(key, Key::Up | Key::Down | Key::Left | Key::Right);
        if direction {
            let before = self.held.last().copied();
            self.held.retain(|k| *k != key);
            if pressed {
                self.held.push(key);
            }
            let after = self.held.last().copied();
            if after != before {
                if let Some(key) = after {
                    self.next_key_repeat = now + REPEAT_DELAY;
                    return vec![Press::Button(key_button(key))];
                }
            }
            return Vec::new();
        }
        if !pressed {
            return Vec::new();
        }
        vec![match key {
            Key::Return => Press::Button(Button::Confirm),
            Key::Escape | Key::Backspace => Press::Button(Button::Back),
            Key::M => Press::Button(Button::Menu),
            Key::O => Press::Button(Button::Options),
            Key::F2 => Press::Switch,
            Key::Up | Key::Down | Key::Left | Key::Right => unreachable!("handled above"),
        }]
    }

    /// Repeats for held directions. Call while [`Router::holding`].
    pub fn tick(&mut self, now: f64) -> Vec<Press> {
        let outputs = self.mapper.tick(now, self.active);
        let mut presses = Self::convert(outputs);
        if let Some(key) = self.held.last().copied() {
            if now >= self.next_key_repeat {
                self.next_key_repeat = now + REPEAT_INTERVAL;
                presses.push(Press::Button(key_button(key)));
            }
        }
        presses
    }

    pub fn holding(&self) -> bool {
        !self.held.is_empty() || self.mapper.holding()
    }

    fn convert(outputs: Vec<Output>) -> Vec<Press> {
        outputs
            .into_iter()
            .filter_map(|output| match output {
                Output::Direction { direction, .. } => {
                    Some(Press::Button(direction_button(direction)))
                }
                Output::DirectionEnd { .. } => None,
                Output::Button(action) => Some(Press::Button(match action {
                    mapper::Action::Confirm => Button::Confirm,
                    mapper::Action::Back => Button::Back,
                    mapper::Action::Options => Button::Options,
                    mapper::Action::Menu => Button::Menu,
                })),
                Output::Switch => Some(Press::Switch),
            })
            .collect()
    }
}

fn key_button(key: Key) -> Button {
    match key {
        Key::Up => Button::Up,
        Key::Down => Button::Down,
        Key::Left => Button::Left,
        _ => Button::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_repeats_like_the_gamepad() {
        let mut router = Router::new(true);
        assert_eq!(
            router.key(Key::Right, true, 0.0),
            vec![Press::Button(Button::Right)]
        );
        assert!(router.tick(399.0).is_empty());
        assert_eq!(router.tick(400.0), vec![Press::Button(Button::Right)]);
        assert!(router.tick(450.0).is_empty());
        assert_eq!(router.tick(500.0), vec![Press::Button(Button::Right)]);
        assert!(router.key(Key::Right, false, 510.0).is_empty());
        assert!(!router.holding());
        assert_eq!(router.key(Key::F2, true, 600.0), vec![Press::Switch]);
        assert_eq!(
            router.key(Key::Escape, true, 600.0),
            vec![Press::Button(Button::Back)]
        );
    }

    #[test]
    fn native_gamepad_presses_become_buttons() {
        let mut router = Router::new(true);
        router.native(
            NativeEvent::DeviceAdded {
                id: "pad".into(),
                gamepad: true,
                axes: vec![],
            },
            0.0,
        );
        router.native(NativeEvent::Initialized, 0.0);
        let input = |code, value| NativeEvent::Input {
            device: "pad".into(),
            gamepad: true,
            kind: 1,
            code,
            value,
        };
        let report = NativeEvent::Input {
            device: "pad".into(),
            gamepad: true,
            kind: 0,
            code: 0,
            value: 0,
        };
        router.native(input(0x130, 1), 0.0);
        assert_eq!(
            router.native(report.clone(), 0.0),
            vec![Press::Button(Button::Confirm)]
        );
        router.native(input(0x130, 0), 0.0);
        router.native(report.clone(), 0.0);
        router.native(input(0x223, 1), 0.0);
        assert_eq!(
            router.native(report, 0.0),
            vec![Press::Button(Button::Right)]
        );
        assert!(router.holding());
        assert_eq!(router.tick(400.0), vec![Press::Button(Button::Right)]);
        assert!(router.set_active(false, 401.0).is_empty());
        assert!(!router.holding());
    }
}
