//! THROWAWAY: semantic input, no grabs and no changes to input permissions.
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::Sender,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Menu,
    Options,
    Switch,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::Confirm => "confirm",
            Self::Back => "back",
            Self::Menu => "menu",
            Self::Options => "options",
            Self::Switch => "switch",
        }
    }
}
#[derive(Debug)]
pub enum Input {
    Direction(String, Option<Action>),
    Press(String, Action),
    Clear(String),
}
struct Hold {
    action: Action,
    next: u64,
}
#[derive(Default)]
pub struct Mapper {
    held: HashMap<String, Hold>,
    last: HashMap<Action, (String, u64)>,
}
impl Mapper {
    fn emit(&mut self, source: &str, action: Action, now: u64, out: &mut Vec<Action>) {
        if self
            .last
            .get(&action)
            .is_some_and(|(other, when)| other != source && now.saturating_sub(*when) < 40)
        {
            return;
        }
        self.last.insert(action, (source.to_owned(), now));
        out.push(action);
    }
    pub fn handle(&mut self, input: Input, now: u64) -> Vec<Action> {
        let mut out = Vec::new();
        match input {
            Input::Press(source, action) => self.emit(&source, action, now, &mut out),
            Input::Clear(source) => {
                self.held.remove(&source);
            }
            Input::Direction(source, action) => {
                if self.held.get(&source).map(|h| h.action) == action {
                    return out;
                }
                self.held.remove(&source);
                if let Some(action) = action {
                    self.held.insert(
                        source.clone(),
                        Hold {
                            action,
                            next: now + 400,
                        },
                    );
                    self.emit(&source, action, now, &mut out);
                }
            }
        }
        out
    }
    pub fn tick(&mut self, now: u64) -> Vec<Action> {
        let due: Vec<_> = self
            .held
            .iter_mut()
            .filter_map(|(source, hold)| {
                if now < hold.next {
                    return None;
                }
                hold.next = now + 100; // Do not replay missed repeats after a slow compile.
                Some((source.clone(), hold.action))
            })
            .collect();
        let mut out = Vec::new();
        for (source, action) in due {
            self.emit(&source, action, now, &mut out);
        }
        out
    }
}

#[derive(Default)]
struct Pad {
    keys: HashSet<u16>,
    axes: HashMap<u16, i32>,
    ranges: HashMap<u16, (i32, i32, i32)>,
    direction: Option<Action>,
    combo: bool,
}
impl Pad {
    fn normalized(&self, code: u16) -> f64 {
        let value = *self.axes.get(&code).unwrap_or(&0) as f64;
        if let Some(&(min, max, flat)) = self.ranges.get(&code) {
            let center = (min as f64 + max as f64) / 2.;
            let extent = (max as f64 - center).max(center - min as f64);
            let delta = value - center;
            if delta.abs() <= flat as f64 || extent <= 0. {
                return 0.;
            }
            return delta / extent;
        }
        value / (16000. / 0.6)
    }
    fn direction(&self) -> Option<Action> {
        let x = i32::from(self.keys.contains(&0x223)) - i32::from(self.keys.contains(&0x222));
        let y = i32::from(self.keys.contains(&0x221)) - i32::from(self.keys.contains(&0x220));
        let x = if x != 0 {
            x
        } else {
            *self.axes.get(&16).unwrap_or(&0)
        };
        let y = if y != 0 {
            y
        } else {
            *self.axes.get(&17).unwrap_or(&0)
        };
        if y != 0 {
            return Some(if y > 0 { Action::Down } else { Action::Up });
        }
        if x != 0 {
            return Some(if x > 0 { Action::Right } else { Action::Left });
        }
        let (x, y) = (self.normalized(0), self.normalized(1));
        if x.abs() > y.abs() {
            if x.abs() > 0.6 {
                return Some(if x > 0. { Action::Right } else { Action::Left });
            }
        } else if y.abs() > 0.6 {
            return Some(if y > 0. { Action::Down } else { Action::Up });
        }
        None
    }
}

pub fn gamepads(tx: Sender<Input>) {
    std::thread::spawn(move || {
        let mut devices: HashMap<String, (evdev::Device, Pad)> = HashMap::new();
        let mut scan = Instant::now() - Duration::from_secs(2);
        loop {
            if scan.elapsed() >= Duration::from_secs(1) {
                scan = Instant::now();
                for (path, device) in evdev::enumerate() {
                    let id = path.to_string_lossy().into_owned();
                    if devices.contains_key(&id)
                        || !device
                            .supported_keys()
                            .is_some_and(|k| k.contains(evdev::KeyCode::BTN_SOUTH))
                    {
                        continue;
                    }
                    if let Err(error) = device.set_nonblocking(true) {
                        eprintln!("shell: input {id}: {error}");
                        continue;
                    }
                    let mut pad = Pad::default();
                    if let Ok(axes) = device.get_absinfo() {
                        for (code, info) in axes {
                            pad.ranges
                                .insert(code.0, (info.minimum(), info.maximum(), info.flat()));
                            pad.axes.insert(code.0, info.value());
                        }
                    }
                    if let Ok(keys) = device.get_key_state() {
                        pad.keys.extend(keys.iter().map(|k| k.0));
                    }
                    eprintln!(
                        "shell: input {id}: {} (not grabbed)",
                        device.name().unwrap_or("unnamed")
                    );
                    devices.insert(id, (device, pad));
                }
            }
            let mut removed = Vec::new();
            for (source, (device, pad)) in &mut devices {
                match device.fetch_events() {
                    Ok(events) => {
                        let old_keys = pad.keys.clone();
                        let mut previous_keys = old_keys;
                        for event in events {
                            match event.event_type() {
                                evdev::EventType::KEY => {
                                    if event.value() == 0 {
                                        pad.keys.remove(&event.code());
                                    } else {
                                        pad.keys.insert(event.code());
                                    }
                                }
                                evdev::EventType::ABSOLUTE => {
                                    pad.axes.insert(event.code(), event.value());
                                }
                                evdev::EventType::SYNCHRONIZATION if event.code() == 0 => {
                                    let direction = pad.direction();
                                    if direction != pad.direction {
                                        pad.direction = direction;
                                        let _ =
                                            tx.send(Input::Direction(source.clone(), direction));
                                    }
                                    let combo =
                                        pad.keys.contains(&0x136) && pad.keys.contains(&0x137);
                                    if combo && !pad.combo {
                                        let _ =
                                            tx.send(Input::Press(source.clone(), Action::Switch));
                                    }
                                    pad.combo = combo;
                                    for (code, action) in [
                                        (0x130, Action::Confirm),
                                        (0x131, Action::Back),
                                        (0x13a, Action::Back),
                                        (0x134, Action::Options),
                                        (0x13b, Action::Menu),
                                    ] {
                                        if pad.keys.contains(&code)
                                            && !previous_keys.contains(&code)
                                        {
                                            let _ = tx.send(Input::Press(source.clone(), action));
                                        }
                                    }
                                    previous_keys = pad.keys.clone();
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => {
                        eprintln!("shell: input disconnected {source}: {e}");
                        removed.push(source.clone());
                    }
                }
            }
            for id in removed {
                devices.remove(&id);
                let _ = tx.send(Input::Clear(id));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dedupes_only_other_sources_inside_40ms() {
        let mut mapper = Mapper::default();
        let press = |s: &str| Input::Press(s.into(), Action::Confirm);
        assert_eq!(mapper.handle(press("physical"), 0), [Action::Confirm]);
        assert!(mapper.handle(press("seat"), 39).is_empty());
        assert_eq!(mapper.handle(press("seat"), 40), [Action::Confirm]);
        assert_eq!(mapper.handle(press("seat"), 41), [Action::Confirm]);
    }
    #[test]
    fn repeats_at_400_then_100_and_release_cancels() {
        let mut mapper = Mapper::default();
        assert_eq!(
            mapper.handle(Input::Direction("pad".into(), Some(Action::Right)), 0),
            [Action::Right]
        );
        assert!(mapper.tick(399).is_empty());
        assert_eq!(mapper.tick(400), [Action::Right]);
        assert!(mapper.tick(499).is_empty());
        assert_eq!(mapper.tick(500), [Action::Right]);
        assert_eq!(mapper.tick(900), [Action::Right]);
        assert!(mapper.tick(901).is_empty());
        mapper.handle(Input::Direction("pad".into(), None), 902);
        assert!(mapper.tick(2000).is_empty());
    }
    #[test]
    fn mirrors_do_not_double_repeat() {
        let mut mapper = Mapper::default();
        assert_eq!(
            mapper.handle(Input::Direction("physical".into(), Some(Action::Left)), 0),
            [Action::Left]
        );
        assert!(mapper
            .handle(Input::Direction("seat".into(), Some(Action::Left)), 1)
            .is_empty());
        assert_eq!(mapper.tick(401), [Action::Left]);
        assert_eq!(mapper.tick(501), [Action::Left]);
    }
    #[test]
    fn normalizes_stick_and_prioritizes_dpad() {
        let mut pad = Pad::default();
        pad.ranges.insert(0, (0, 255, 10));
        pad.ranges.insert(1, (0, 255, 10));
        pad.axes.insert(0, 255);
        pad.axes.insert(1, 128);
        assert_eq!(pad.direction(), Some(Action::Right));
        pad.axes.insert(17, -1);
        assert_eq!(pad.direction(), Some(Action::Up));
        pad.axes.insert(17, 0);
        pad.axes.insert(0, 128);
        assert_eq!(pad.direction(), None);
    }
}
