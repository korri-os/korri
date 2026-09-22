use serde::Deserialize;
use std::collections::BTreeMap;

pub const MAX_SEATS: u8 = 4;
pub const MAX_MIRROR_FRAME_BYTES: usize = 2048;
pub const MAX_EVENTS_PER_SECOND: u16 = 240;
pub const STALE_SOURCE_TIMEOUT_MS: u64 = 1_250;
const SUPPORTED_BUTTON_MASK: u32 = 0x0000_f7ff;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GamepadState {
    pub buttons: u32,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub left_stick_x: i16,
    pub left_stick_y: i16,
    pub right_stick_x: i16,
    pub right_stick_y: i16,
}

impl GamepadState {
    pub const fn neutral() -> Self {
        Self {
            buttons: 0,
            left_trigger: 0,
            right_trigger: 0,
            left_stick_x: 0,
            left_stick_y: 0,
            right_stick_x: 0,
            right_stick_y: 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeatSpec {
    pub slot: u8,
    pub name: String,
    pub physical_path: String,
}

impl SeatSpec {
    pub fn for_slot(slot: u8) -> Self {
        Self {
            slot,
            name: format!("Korri Seat P{slot}"),
            physical_path: format!("korri/input-seat/p{slot}"),
        }
    }
}

pub trait SeatBackend: Send {
    fn create(&mut self, spec: &SeatSpec) -> Result<(), String>;
    fn write_state(&mut self, slot: u8, state: GamepadState) -> Result<(), String>;
    fn destroy(&mut self, slot: u8) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MirrorOutcome {
    Accepted { slot: u8 },
    Unauthorized,
    Invalid,
    StaleLaunch,
    UnknownSource,
    NoSeat,
    RateLimited,
    BackendFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeatResetOutcome {
    Accepted,
    StaleLaunch,
    BackendFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceState {
    Connected,
    Reserved,
}

#[derive(Clone, Copy, Debug)]
struct SourceBinding {
    slot: u8,
    state: SourceState,
    window_start_ms: u64,
    events_in_window: u16,
    last_event_ms: u64,
    requires_neutral: bool,
}

pub struct SeatRuntime<B: SeatBackend> {
    launch_id: String,
    mirror_token: String,
    backend: Option<B>,
    sources: BTreeMap<u8, SourceBinding>,
    last_state: [GamepadState; MAX_SEATS as usize],
    has_reset: bool,
}

impl<B: SeatBackend> SeatRuntime<B> {
    pub fn start(launch_id: &str, mirror_token: &str, mut backend: B) -> Result<Self, String> {
        validate_launch_id(launch_id)?;
        validate_token(mirror_token)?;
        let mut created = Vec::new();
        for slot in 1..=MAX_SEATS {
            if let Err(error) = backend.create(&SeatSpec::for_slot(slot)) {
                for created_slot in created.into_iter().rev() {
                    let _ = backend.destroy(created_slot);
                }
                return Err(error);
            }
            created.push(slot);
        }
        Ok(Self {
            launch_id: launch_id.to_owned(),
            mirror_token: mirror_token.to_owned(),
            backend: Some(backend),
            sources: BTreeMap::new(),
            last_state: [GamepadState::neutral(); MAX_SEATS as usize],
            has_reset: false,
        })
    }

    pub fn accept(&mut self, packet: &[u8], now_ms: u64) -> MirrorOutcome {
        let Some(envelope) = decode_envelope(packet) else {
            return MirrorOutcome::Invalid;
        };
        if !constant_time_equal(
            envelope.mirror_token.as_bytes(),
            self.mirror_token.as_bytes(),
        ) {
            return MirrorOutcome::Unauthorized;
        }
        let frame = envelope.frame;
        if frame.launch_id() != self.launch_id {
            return MirrorOutcome::StaleLaunch;
        }
        match frame {
            SunshineFrame::Connected(frame) if frame.kind == "source-connected" => {
                self.connect(frame.controller_number)
            }
            SunshineFrame::Disconnected(frame) if frame.kind == "source-disconnected" => {
                if frame.reason.as_ref().is_some_and(|value| {
                    value.len() > 128 || value.as_bytes().contains(&b'\n') || value.contains('\0')
                }) {
                    return MirrorOutcome::Invalid;
                }
                self.disconnect(frame.controller_number)
            }
            SunshineFrame::State(frame) if frame.kind == "source-state" => {
                if !self.sources.contains_key(&frame.controller_number)
                    && !matches!(
                        self.connect(frame.controller_number),
                        MirrorOutcome::Accepted { .. }
                    )
                {
                    return MirrorOutcome::NoSeat;
                }
                let Some(binding) = self.sources.get_mut(&frame.controller_number) else {
                    return MirrorOutcome::UnknownSource;
                };
                if binding.state == SourceState::Reserved {
                    binding.state = SourceState::Connected;
                    binding.window_start_ms = now_ms;
                    binding.events_in_window = 0;
                }
                if now_ms.saturating_sub(binding.window_start_ms) >= 1000 {
                    binding.window_start_ms = now_ms;
                    binding.events_in_window = 0;
                }
                if binding.events_in_window >= MAX_EVENTS_PER_SECOND {
                    return MirrorOutcome::RateLimited;
                }
                binding.events_in_window += 1;
                binding.last_event_ms = now_ms;
                let slot = binding.slot;
                let next = GamepadState {
                    buttons: frame.buttons & SUPPORTED_BUTTON_MASK,
                    left_trigger: frame.left_trigger,
                    right_trigger: frame.right_trigger,
                    left_stick_x: frame.left_stick_x,
                    left_stick_y: invert_sunshine_axis(frame.left_stick_y),
                    right_stick_x: frame.right_stick_x,
                    right_stick_y: invert_sunshine_axis(frame.right_stick_y),
                };
                if binding.requires_neutral {
                    if next == GamepadState::neutral() {
                        binding.requires_neutral = false;
                    }
                    return MirrorOutcome::Accepted { slot };
                }
                if self.last_state[(slot - 1) as usize] != next {
                    let Some(backend) = self.backend.as_mut() else {
                        return MirrorOutcome::BackendFailed;
                    };
                    if backend.write_state(slot, next).is_err() {
                        return MirrorOutcome::BackendFailed;
                    }
                    self.last_state[(slot - 1) as usize] = next;
                }
                MirrorOutcome::Accepted { slot }
            }
            _ => MirrorOutcome::Invalid,
        }
    }

    pub fn reset(&mut self, launch_id: &str) -> SeatResetOutcome {
        if launch_id != self.launch_id {
            return SeatResetOutcome::StaleLaunch;
        }
        self.has_reset = true;
        for binding in self.sources.values_mut() {
            binding.requires_neutral = true;
        }
        let active: Vec<_> = self
            .sources
            .values()
            .filter_map(|binding| (binding.state == SourceState::Connected).then_some(binding.slot))
            .collect();
        let Some(backend) = self.backend.as_mut() else {
            return SeatResetOutcome::BackendFailed;
        };
        for slot in active {
            if backend.write_state(slot, GamepadState::neutral()).is_err() {
                return SeatResetOutcome::BackendFailed;
            }
            self.last_state[(slot - 1) as usize] = GamepadState::neutral();
        }
        SeatResetOutcome::Accepted
    }

    pub fn expire_stale(&mut self, now_ms: u64) -> Result<usize, String> {
        let stale: Vec<_> = self
            .sources
            .iter()
            .filter_map(|(controller, binding)| {
                (binding.state == SourceState::Connected
                    && binding.last_event_ms != 0
                    && now_ms.saturating_sub(binding.last_event_ms) >= STALE_SOURCE_TIMEOUT_MS)
                    .then_some((*controller, binding.slot))
            })
            .collect();
        let mut expired = 0;
        for (controller, slot) in stale {
            if self.last_state[(slot - 1) as usize] != GamepadState::neutral() {
                self.backend
                    .as_mut()
                    .ok_or("input-seat backend is absent")?
                    .write_state(slot, GamepadState::neutral())?;
                self.last_state[(slot - 1) as usize] = GamepadState::neutral();
            }
            if let Some(binding) = self.sources.get_mut(&controller) {
                binding.state = SourceState::Reserved;
                binding.last_event_ms = 0;
            }
            expired += 1;
        }
        Ok(expired)
    }

    pub fn stop(mut self) -> Result<(), String> {
        self.cleanup()
    }

    fn connect(&mut self, controller_number: u8) -> MirrorOutcome {
        if controller_number > 15 {
            return MirrorOutcome::Invalid;
        }
        if let Some(binding) = self.sources.get_mut(&controller_number) {
            if binding.state == SourceState::Reserved {
                binding.state = SourceState::Connected;
                binding.window_start_ms = 0;
                binding.events_in_window = 0;
                binding.last_event_ms = 0;
            }
            return MirrorOutcome::Accepted { slot: binding.slot };
        }
        let slot = (1..=MAX_SEATS)
            .find(|slot| !self.sources.values().any(|binding| binding.slot == *slot));
        let Some(slot) = slot else {
            return MirrorOutcome::NoSeat;
        };
        self.sources.insert(
            controller_number,
            SourceBinding {
                slot,
                state: SourceState::Connected,
                window_start_ms: 0,
                events_in_window: 0,
                last_event_ms: 0,
                requires_neutral: self.has_reset,
            },
        );
        MirrorOutcome::Accepted { slot }
    }

    fn disconnect(&mut self, controller_number: u8) -> MirrorOutcome {
        if controller_number > 15 {
            return MirrorOutcome::Invalid;
        }
        let Some(binding) = self.sources.get(&controller_number) else {
            return MirrorOutcome::UnknownSource;
        };
        let slot = binding.slot;
        let neutral = GamepadState::neutral();
        if self.last_state[(slot - 1) as usize] != neutral {
            let Some(backend) = self.backend.as_mut() else {
                return MirrorOutcome::BackendFailed;
            };
            if backend.write_state(slot, neutral).is_err() {
                return MirrorOutcome::BackendFailed;
            }
            self.last_state[(slot - 1) as usize] = neutral;
        }
        if let Some(binding) = self.sources.get_mut(&controller_number) {
            binding.state = SourceState::Reserved;
            binding.last_event_ms = 0;
        }
        MirrorOutcome::Accepted { slot }
    }

    fn cleanup(&mut self) -> Result<(), String> {
        let Some(mut backend) = self.backend.take() else {
            return Ok(());
        };
        let mut first_error = None;
        for slot in 1..=MAX_SEATS {
            if self.last_state[(slot - 1) as usize] != GamepadState::neutral() {
                if let Err(error) = backend.write_state(slot, GamepadState::neutral()) {
                    first_error.get_or_insert(error);
                }
            }
        }
        for slot in (1..=MAX_SEATS).rev() {
            if let Err(error) = backend.destroy(slot) {
                first_error.get_or_insert(error);
            }
        }
        self.sources.clear();
        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(())
        }
    }
}

impl<B: SeatBackend> Drop for SeatRuntime<B> {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

pub const fn invert_sunshine_axis(value: i16) -> i16 {
    if value == i16::MIN {
        i16::MAX
    } else {
        -value
    }
}

pub fn validate_launch_id(value: &str) -> Result<(), String> {
    if value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err("launch ID must be 32 lower-case hexadecimal bytes".into())
    }
}

fn validate_token(value: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err("mirror token must be 64 lower-case hexadecimal bytes".into())
    }
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let width = left.len().max(right.len());
    for index in 0..width {
        difference |= usize::from(
            left.get(index).copied().unwrap_or(0) ^ right.get(index).copied().unwrap_or(0),
        );
    }
    difference == 0
}

fn decode_envelope(packet: &[u8]) -> Option<MirrorEnvelope> {
    if packet.is_empty()
        || packet.len() > MAX_MIRROR_FRAME_BYTES
        || packet.last() != Some(&b'\n')
        || packet[..packet.len() - 1].contains(&b'\n')
        || packet[..packet.len() - 1].contains(&b'\r')
    {
        return None;
    }
    serde_json::from_slice(&packet[..packet.len() - 1]).ok()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MirrorEnvelope {
    #[serde(rename = "mirrorToken")]
    mirror_token: String,
    frame: SunshineFrame,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SunshineFrame {
    Connected(SourceConnectedFrame),
    Disconnected(SourceDisconnectedFrame),
    State(SourceStateFrame),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceConnectedFrame {
    kind: String,
    #[serde(rename = "launchId")]
    launch_id: String,
    #[serde(rename = "controllerNumber")]
    controller_number: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceDisconnectedFrame {
    kind: String,
    #[serde(rename = "launchId")]
    launch_id: String,
    #[serde(rename = "controllerNumber")]
    controller_number: u8,
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceStateFrame {
    kind: String,
    #[serde(rename = "launchId")]
    launch_id: String,
    #[serde(rename = "controllerNumber")]
    controller_number: u8,
    buttons: u32,
    #[serde(rename = "leftTrigger")]
    left_trigger: u8,
    #[serde(rename = "rightTrigger")]
    right_trigger: u8,
    #[serde(rename = "leftStickX")]
    left_stick_x: i16,
    #[serde(rename = "leftStickY")]
    left_stick_y: i16,
    #[serde(rename = "rightStickX")]
    right_stick_x: i16,
    #[serde(rename = "rightStickY")]
    right_stick_y: i16,
}

impl SunshineFrame {
    fn launch_id(&self) -> &str {
        match self {
            Self::Connected(frame) => &frame.launch_id,
            Self::Disconnected(frame) => &frame.launch_id,
            Self::State(frame) => &frame.launch_id,
        }
    }
}

impl<T: SeatBackend + ?Sized> SeatBackend for Box<T> {
    fn create(&mut self, spec: &SeatSpec) -> Result<(), String> {
        (**self).create(spec)
    }

    fn write_state(&mut self, slot: u8, state: GamepadState) -> Result<(), String> {
        (**self).write_state(slot, state)
    }

    fn destroy(&mut self, slot: u8) -> Result<(), String> {
        (**self).destroy(slot)
    }
}
