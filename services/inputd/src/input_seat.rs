use crate::seat_pool::{PoolError, SeatPool, DEFAULT_SEAT_COUNT};
pub use korri_input_contract::*;
use serde::Deserialize;
use std::{collections::BTreeMap, num::NonZeroU8};

pub const MAX_SEATS: u8 = 4;
pub const MAX_MIRROR_FRAME_BYTES: usize = 2048;
pub const MAX_EVENTS_PER_SECOND: u16 = 240;
pub const STALE_SOURCE_TIMEOUT_MS: u64 = 1_250;
const SUPPORTED_BUTTON_MASK: u32 = 0x0000_f7ff;

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
    FeedbackFailed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeatResetOutcome {
    Accepted,
    StaleLaunch,
    BackendFailed,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum SourceKey {
    Physical(String),
    Remote(String, u8),
}
struct Source {
    state: GamepadState,
    has_state: bool,
    requires_neutral: bool,
    last_event_ms: u64,
    window_start_ms: u64,
    events_in_window: u16,
}

/// The only allocator in the production receiver. Mirror authority and session
/// reservation lifetime are deliberately independent.
pub struct SeatRuntime<B: SeatBackend> {
    pool: SeatPool<SourceKey, B>,
    sources: BTreeMap<SourceKey, Source>,
    mirror: Option<(String, String)>,
    game_route: bool,
    recovery_required: bool,
    faulted: bool,
    feedback_failed: bool,
    feedback: Vec<RemoteEvent>,
}
impl<B: SeatBackend> SeatRuntime<B> {
    pub fn boot(backend: B) -> Result<Self, String> {
        Ok(Self {
            pool: SeatPool::new(DEFAULT_SEAT_COUNT, backend).map_err(|e| format!("{e:?}"))?,
            sources: BTreeMap::new(),
            mirror: None,
            game_route: false,
            recovery_required: true,
            faulted: false,
            feedback_failed: false,
            feedback: Vec::new(),
        })
    }
    pub fn faulted(&self) -> bool {
        self.faulted
    }
    pub fn feedback_failed(&self) -> bool {
        self.feedback_failed
    }
    pub fn mirror_launch(&self) -> Option<&str> {
        self.mirror.as_ref().map(|(id, _)| id.as_str())
    }

    /// One authenticated coordinator request. Errors are acknowledged, never
    /// represented as successful stored-only count changes.
    pub fn coordinate(&mut self, request: SeatRequest, now_ms: u64) -> SeatReply {
        let result = self.apply(request, now_ms);
        if result == Err(SeatFailure::Backend) {
            self.faulted = true;
            self.recovery_required = true;
        }
        SeatReply {
            failure: result.as_ref().err().cloned(),
            count: self.pool.count(),
            session: self.pool.session().map(str::to_owned),
            recovery_required: self.recovery_required,
            slot: result.ok().flatten(),
            remote_sources: self.remote_sources(),
            remote_events: std::mem::take(&mut self.feedback),
        }
    }
    fn apply(&mut self, request: SeatRequest, now_ms: u64) -> Result<Option<u8>, SeatFailure> {
        if self.faulted {
            return Err(SeatFailure::Backend);
        }
        match request {
            SeatRequest::Hello | SeatRequest::Poll => Ok(None),
            SeatRequest::ApplyCount { count } => {
                let count = NonZeroU8::new(count).ok_or(SeatFailure::Invalid)?;
                self.pool.resize(count).map_err(pool_failure)?;
                self.rebalance()?;
                self.recovery_required = false;
                Ok(None)
            }
            SeatRequest::BeginSession { launch_id } => {
                validate_launch_id(&launch_id).map_err(|_| SeatFailure::Invalid)?;
                if self.recovery_required && self.pool.session() == Some(launch_id.as_str()) {
                    self.recovery_required = false;
                    return Ok(None);
                }
                self.require_ready()?;
                self.pool.begin_session(&launch_id).map_err(pool_failure)?;
                Ok(None)
            }
            SeatRequest::EndSession { launch_id } => {
                if self.pool.session() != Some(launch_id.as_str()) {
                    return Err(SeatFailure::Stale);
                }
                self.unbind().map_err(|_| SeatFailure::Backend)?;
                self.set_route(false)?;
                self.pool.end_session(&launch_id).map_err(pool_failure)?;
                self.rebalance()?;
                Ok(None)
            }
            SeatRequest::Route { launch_id } => {
                self.require_ready()?;
                if let Some(id) = launch_id.as_deref() {
                    if self.pool.session() != Some(id) {
                        return Err(SeatFailure::Stale);
                    }
                }
                self.set_route(launch_id.is_some())?;
                Ok(None)
            }
            SeatRequest::PhysicalConnected {
                device_id,
                name,
                state,
            } => {
                self.require_ready()?;
                if !valid_native_text(&device_id, 512)
                    || !valid_native_text(&name, 256)
                    || !valid_state(state)
                {
                    return Err(SeatFailure::Invalid);
                }
                let key = SourceKey::Physical(device_id);
                if self.sources.contains_key(&key) {
                    return Ok(self.pool.slot(&key));
                }
                if self
                    .sources
                    .keys()
                    .filter(|key| matches!(key, SourceKey::Physical(_)))
                    .count()
                    >= MAX_PHYSICAL_SOURCES
                {
                    return Err(SeatFailure::Invalid);
                }
                let slot = self.connect_source(key.clone(), state, now_ms, true)?;
                // PhysicalConnected carries an actual held-state snapshot. A
                // neutral snapshot can arm without synthesizing any action.
                self.sources.get_mut(&key).unwrap().requires_neutral =
                    !state.is_neutral_for_rearm();
                Ok(slot)
            }
            SeatRequest::PhysicalState { device_id, state } => {
                self.require_ready()?;
                if !valid_state(state) {
                    return Err(SeatFailure::Invalid);
                }
                self.update_source(&SourceKey::Physical(device_id), state, now_ms)
            }
            SeatRequest::PhysicalDisconnected { device_id } => {
                self.require_ready()?;
                self.disconnect_source(&SourceKey::Physical(device_id))?;
                Ok(None)
            }
        }
    }
    fn require_ready(&self) -> Result<(), SeatFailure> {
        if self.recovery_required || self.faulted || self.feedback_failed {
            Err(SeatFailure::NotReady)
        } else {
            Ok(())
        }
    }
    fn set_route(&mut self, game: bool) -> Result<(), SeatFailure> {
        // Even repeated route barriers retire held state.
        self.game_route = false;
        for source in self.sources.values_mut() {
            source.requires_neutral = true;
        }
        self.pool.neutralize().map_err(pool_failure)?;
        self.game_route = game;
        Ok(())
    }
    fn connect_source(
        &mut self,
        key: SourceKey,
        state: GamepadState,
        now_ms: u64,
        has_state: bool,
    ) -> Result<Option<u8>, SeatFailure> {
        let slot = match self.pool.connect(key.clone()) {
            Ok(slot) => Some(slot),
            Err(PoolError::NoSeat) => None,
            Err(error) => return Err(pool_failure(error)),
        };
        self.sources.insert(
            key,
            Source {
                state,
                has_state,
                requires_neutral: true,
                last_event_ms: now_ms,
                window_start_ms: now_ms,
                events_in_window: 0,
            },
        );
        Ok(slot)
    }
    fn update_source(
        &mut self,
        key: &SourceKey,
        state: GamepadState,
        now_ms: u64,
    ) -> Result<Option<u8>, SeatFailure> {
        let source = self
            .sources
            .get_mut(key)
            .ok_or(SeatFailure::UnknownSource)?;
        // Sunshine's existing admission budget applies only to remote frames.
        // Trusted physical capture can emit several normalized SYN reports per
        // hardware sample. Its transport queues/timeouts bound overload; there
        // is no physical reports-per-second contract to enforce here.
        if matches!(key, SourceKey::Remote(..)) {
            if now_ms.saturating_sub(source.window_start_ms) >= 1000 {
                source.window_start_ms = now_ms;
                source.events_in_window = 0;
            }
            if source.events_in_window >= MAX_EVENTS_PER_SECOND {
                return Err(SeatFailure::RateLimited);
            }
            source.events_in_window += 1;
        }
        source.last_event_ms = now_ms;
        source.state = state;
        source.has_state = true;
        let slot = self.pool.slot(key);
        if !self.game_route {
            return Ok(slot);
        }
        if source.requires_neutral {
            if state.is_neutral_for_rearm() {
                source.requires_neutral = false;
            }
            return Ok(slot);
        }
        if slot.is_some() {
            self.pool.write_state(key, state).map_err(pool_failure)?;
        }
        Ok(slot)
    }
    fn disconnect_source(&mut self, key: &SourceKey) -> Result<(), SeatFailure> {
        let source = self.sources.remove(key).ok_or(SeatFailure::UnknownSource)?;
        let result = match self.pool.disconnect(key) {
            Ok(_) | Err(PoolError::UnknownSource) => Ok(()),
            Err(error) => Err(pool_failure(error)),
        };
        if source.has_state {
            if let SourceKey::Remote(launch_id, controller_number) = key {
                self.push_feedback(RemoteEvent::Disconnected {
                    launch_id: launch_id.clone(),
                    controller_number: *controller_number,
                });
            }
        }
        result?;
        self.rebalance()
    }
    fn rebalance(&mut self) -> Result<(), SeatFailure> {
        for (key, source) in &mut self.sources {
            if self.pool.slot(key).is_none() {
                match self.pool.connect(key.clone()) {
                    Ok(_) => source.requires_neutral = true,
                    Err(PoolError::NoSeat) => {}
                    Err(error) => return Err(pool_failure(error)),
                }
            }
        }
        Ok(())
    }
    pub fn bind(&mut self, launch_id: &str, mirror_token: &str) -> Result<(), String> {
        self.require_ready()
            .map_err(|_| "seat coordinator is not ready")?;
        validate_launch_id(launch_id)?;
        validate_token(mirror_token)?;
        if self.pool.session() != Some(launch_id) {
            return Err("mirror has no matching authoritative session".into());
        }
        if self.mirror.is_some() {
            return Err("input-seat launch is already bound".into());
        }
        self.mirror = Some((launch_id.to_owned(), mirror_token.to_owned()));
        Ok(())
    }
    /// Revoke only mirror authority; a game may still be running after focus fails.
    pub fn unbind(&mut self) -> Result<(), String> {
        self.mirror = None;
        let keys: Vec<_> = self
            .sources
            .keys()
            .filter(|key| matches!(key, SourceKey::Remote(..)))
            .cloned()
            .collect();
        let mut failed = false;
        for key in keys {
            failed |= self.disconnect_source(&key).is_err();
        }
        if failed {
            self.faulted = true;
            Err("mirror neutralization failed".into())
        } else {
            Ok(())
        }
    }
    /// Coordinator loss does not assert that the authoritative game ended.
    pub fn coordinator_lost(&mut self) -> Result<(), String> {
        self.recovery_required = true;
        self.game_route = false;
        self.mirror = None;
        let keys: Vec<_> = self.sources.keys().cloned().collect();
        let mut failed = false;
        for key in keys {
            failed |= self.disconnect_source(&key).is_err();
        }
        failed |= self.pool.neutralize().is_err();
        self.feedback.clear();
        self.feedback_failed = false;
        if failed {
            self.faulted = true;
            Err("coordinator loss neutralization failed".into())
        } else {
            Ok(())
        }
    }
    pub fn reset(&mut self, launch_id: &str) -> SeatResetOutcome {
        if self.faulted {
            return SeatResetOutcome::BackendFailed;
        }
        if self.mirror_launch() != Some(launch_id) {
            return SeatResetOutcome::StaleLaunch;
        }
        // Preserve physical controls and routing. RESET retires this mirror only.
        let keys: Vec<_> = self
            .sources
            .keys()
            .filter(|key| matches!(key, SourceKey::Remote(..)))
            .cloned()
            .collect();
        let mut failed = false;
        for key in keys {
            self.sources.get_mut(&key).unwrap().requires_neutral = true;
            if self.pool.slot(&key).is_some() {
                failed |= self
                    .pool
                    .write_state(&key, GamepadState::neutral())
                    .is_err();
            }
        }
        if failed {
            self.faulted = true;
            SeatResetOutcome::BackendFailed
        } else {
            SeatResetOutcome::Accepted
        }
    }
    pub fn accept(&mut self, packet: &[u8], now_ms: u64) -> MirrorOutcome {
        if self.faulted {
            return MirrorOutcome::BackendFailed;
        }
        if self.feedback_failed {
            return MirrorOutcome::FeedbackFailed;
        }
        let Some((launch, token)) = &self.mirror else {
            return MirrorOutcome::StaleLaunch;
        };
        let Some(envelope) = decode_envelope(packet) else {
            return MirrorOutcome::Invalid;
        };
        if !constant_time_equal(envelope.mirror_token.as_bytes(), token.as_bytes()) {
            return MirrorOutcome::Unauthorized;
        }
        if envelope.frame.launch_id() != launch {
            return MirrorOutcome::StaleLaunch;
        }
        let launch = launch.clone();
        let (controller, state, disconnect) = match envelope.frame {
            SunshineFrame::Connected(frame) => (frame.controller_number, None, false),
            SunshineFrame::Disconnected(frame) => {
                if frame
                    .reason
                    .as_ref()
                    .is_some_and(|reason| reason.len() > 128 || reason.contains(['\n', '\0']))
                {
                    return MirrorOutcome::Invalid;
                }
                (frame.controller_number, None, true)
            }
            SunshineFrame::State(frame) => (
                frame.controller_number,
                Some(GamepadState {
                    buttons: frame.buttons & SUPPORTED_BUTTON_MASK,
                    left_trigger: frame.left_trigger,
                    right_trigger: frame.right_trigger,
                    left_stick_x: frame.left_stick_x,
                    left_stick_y: invert_sunshine_axis(frame.left_stick_y),
                    right_stick_x: frame.right_stick_x,
                    right_stick_y: invert_sunshine_axis(frame.right_stick_y),
                }),
                false,
            ),
        };
        if controller > 15 {
            return MirrorOutcome::Invalid;
        }
        let key = SourceKey::Remote(launch, controller);
        let result = if disconnect {
            let slot = self.pool.slot(&key);
            self.disconnect_source(&key).map(|_| slot)
        } else {
            let mut result = Ok(self.pool.slot(&key));
            if !self.sources.contains_key(&key) {
                result = self.connect_source(key.clone(), GamepadState::neutral(), now_ms, false);
            }
            if result.is_ok() {
                if let Some(state) = state {
                    let initialized = self
                        .sources
                        .get(&key)
                        .is_some_and(|source| source.has_state);
                    let changed = self
                        .sources
                        .get(&key)
                        .is_some_and(|source| source.state != state);
                    result = self.update_source(&key, state, now_ms);
                    if result.is_ok() && (!initialized || changed) {
                        let source = self.remote_source(&key).unwrap();
                        self.push_feedback(if initialized {
                            RemoteEvent::State { source }
                        } else {
                            RemoteEvent::Connected { source }
                        });
                    }
                }
            }
            result
        };
        if self.feedback_failed {
            return MirrorOutcome::FeedbackFailed;
        }
        match result {
            Ok(Some(slot)) => MirrorOutcome::Accepted { slot },
            Ok(None) => MirrorOutcome::NoSeat,
            Err(SeatFailure::UnknownSource) => MirrorOutcome::UnknownSource,
            Err(SeatFailure::RateLimited) => MirrorOutcome::RateLimited,
            Err(_) => {
                self.faulted = true;
                MirrorOutcome::BackendFailed
            }
        }
    }
    fn remote_source(&self, key: &SourceKey) -> Option<RemoteSource> {
        let SourceKey::Remote(launch_id, controller_number) = key else {
            return None;
        };
        let source = self.sources.get(key)?;
        if !source.has_state {
            return None;
        }
        Some(RemoteSource {
            launch_id: launch_id.clone(),
            controller_number: *controller_number,
            state: source.state,
            slot: self.pool.slot(key),
        })
    }
    fn remote_sources(&self) -> Vec<RemoteSource> {
        self.sources
            .keys()
            .filter_map(|key| self.remote_source(key))
            .collect()
    }
    fn push_feedback(&mut self, event: RemoteEvent) {
        if self.feedback.len() >= MAX_REMOTE_EVENTS {
            self.feedback_failed = true;
        } else {
            self.feedback.push(event);
        }
    }
    pub fn expire_stale(&mut self, now_ms: u64) -> Result<usize, String> {
        let keys: Vec<_> = self
            .sources
            .iter()
            .filter_map(|(key, source)| {
                (now_ms.saturating_sub(source.last_event_ms) >= STALE_SOURCE_TIMEOUT_MS)
                    .then_some(key.clone())
            })
            .collect();
        let count = keys.len();
        for key in keys {
            if let Err(error) = self.disconnect_source(&key) {
                self.faulted = true;
                return Err(format!("source expiry failed: {error:?}"));
            }
        }
        Ok(count)
    }
    pub fn stop(self) -> Result<(), String> {
        self.pool.stop().map_err(|error| format!("{error:?}"))
    }
}
fn pool_failure(error: PoolError) -> SeatFailure {
    match error {
        PoolError::SessionAlreadyActive => SeatFailure::Active,
        PoolError::StaleSession => SeatFailure::Stale,
        PoolError::InvalidSession(_) => SeatFailure::Invalid,
        PoolError::UnknownSource | PoolError::DisconnectedSource => SeatFailure::UnknownSource,
        PoolError::NoSeat => SeatFailure::Active,
        PoolError::Backend(_) => SeatFailure::Backend,
    }
}
fn valid_native_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn valid_state(state: GamepadState) -> bool {
    state.buttons & !SUPPORTED_BUTTON_MASK == 0
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
    for index in 0..left.len().max(right.len()) {
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
#[serde(tag = "kind")]
enum SunshineFrame {
    #[serde(rename = "source-connected")]
    Connected(SourceConnectedFrame),
    #[serde(rename = "source-disconnected")]
    Disconnected(SourceDisconnectedFrame),
    #[serde(rename = "source-state")]
    State(SourceStateFrame),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceConnectedFrame {
    #[serde(rename = "launchId")]
    launch_id: String,
    #[serde(rename = "controllerNumber")]
    controller_number: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceDisconnectedFrame {
    #[serde(rename = "launchId")]
    launch_id: String,
    #[serde(rename = "controllerNumber")]
    controller_number: u8,
    reason: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceStateFrame {
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
