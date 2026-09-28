//! Private korrid ↔ root seat receiver coordination. Not a browser/peer API.
//! Each SOCK_SEQPACKET packet is [COORDINATION_VERSION] followed by JSON.
//! Binary version-1 START/STOP/RESET remains a separate lease connection.
use serde::{Deserialize, Serialize};

pub const COORDINATION_VERSION: u8 = 2;
pub const MAX_COORDINATION_BYTES: usize = 65_536;
pub const MAX_REMOTE_EVENTS: usize = 128;
pub const COORDINATOR_TIMEOUT_MS: u64 = 1_250;
pub const MAX_PHYSICAL_SOURCES: usize = 255;

/// Existing ABS_X/Y/RX/RY flat value of the shared Linux uinput seats.
/// This is a rearm boundary, not a transformation of gameplay axis values.
pub const GAMEPAD_STICK_FLAT: i16 = 4096;

/// Existing input-seat full state; axes use Linux orientation (positive Y down).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Buttons (including D-pad) and triggers must be released. All four stick
    /// axes must lie within the seats' existing inclusive kernel flat interval.
    /// Portal navigation's 0.6 directional threshold is not a release deadzone.
    pub const fn is_neutral_for_rearm(self) -> bool {
        self.buttons == 0
            && self.left_trigger == 0
            && self.right_trigger == 0
            && self.left_stick_x >= -GAMEPAD_STICK_FLAT
            && self.left_stick_x <= GAMEPAD_STICK_FLAT
            && self.left_stick_y >= -GAMEPAD_STICK_FLAT
            && self.left_stick_y <= GAMEPAD_STICK_FLAT
            && self.right_stick_x >= -GAMEPAD_STICK_FLAT
            && self.right_stick_x <= GAMEPAD_STICK_FLAT
            && self.right_stick_y >= -GAMEPAD_STICK_FLAT
            && self.right_stick_y <= GAMEPAD_STICK_FLAT
    }

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase", deny_unknown_fields)]
pub enum SeatRequest {
    /// First packet on the single persistent coordinator connection.
    Hello,
    /// Heartbeat and remote feedback. Poll every 20 ms when otherwise idle.
    Poll,
    /// Idle-only; also acknowledges startup reconciliation with device.yaml.
    ApplyCount { count: u8 },
    BeginSession {
        #[serde(rename = "launchId")]
        launch_id: String,
    },
    EndSession {
        #[serde(rename = "launchId")]
        launch_id: String,
    },
    /// None routes to portal (all seats neutral); Some must match active session.
    Route {
        #[serde(rename = "launchId")]
        launch_id: Option<String>,
    },
    /// deviceId/name are the existing native producer identity and display name.
    /// Initial state is a baseline, never a synthetic button press.
    PhysicalConnected {
        #[serde(rename = "deviceId")]
        device_id: String,
        name: String,
        state: GamepadState,
    },
    PhysicalState {
        #[serde(rename = "deviceId")]
        device_id: String,
        state: GamepadState,
    },
    PhysicalDisconnected {
        #[serde(rename = "deviceId")]
        device_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SeatFailure {
    Invalid,
    NotReady,
    Active,
    Stale,
    UnknownSource,
    RateLimited,
    Backend,
}

/// Full current connected remote source set, including sources without seats.
/// Identity is (launchId, controllerNumber), never a seat number. Absence means
/// source loss. Only the current authenticated mirror lease can populate it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteSource {
    pub launch_id: String,
    pub controller_number: u8,
    pub state: GamepadState,
    pub slot: Option<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum RemoteEvent {
    Connected {
        source: RemoteSource,
    },
    State {
        source: RemoteSource,
    },
    Disconnected {
        #[serde(rename = "launchId")]
        launch_id: String,
        #[serde(rename = "controllerNumber")]
        controller_number: u8,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SeatReply {
    pub failure: Option<SeatFailure>,
    pub count: u8,
    pub session: Option<String>,
    pub recovery_required: bool,
    /// Result of physical connect/state, not controller identity.
    pub slot: Option<u8>,
    pub remote_sources: Vec<RemoteSource>,
    /// Ordered transitions since the previous reply; bounded, never silently lost.
    pub remote_events: Vec<RemoteEvent>,
}
