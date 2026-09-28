//! Native input wire from legacy `product/platform/input/native/wire-schema.ts`.
//! Authentication is separate: these events are server-to-browser only.

use serde::{Deserialize, Serialize};
use typeshare::typeshare;

#[typeshare(serialized_as = "NativeInputDeviceClassWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeInputDeviceClass {
    Gamepad,
    Keyboard,
    Mouse,
    Touch,
    System,
    Unknown,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NativeInputAxisInfo {
    pub code: f64,
    pub minimum: f64,
    pub maximum: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flat: Option<f64>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeInputDeviceInfo {
    pub device_id: String,
    pub class: NativeInputDeviceClass,
    pub name: String,
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<Vec<NativeInputAxisInfo>>,
}

// Single-variant enums enforce the original literal fields in Serde. Typeshare
// needs the literal mappings in typeshare.toml; it cannot infer untagged unions.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum NativeInputInputKind {
    #[serde(rename = "input")]
    Input,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeInputInput {
    #[typeshare(serialized_as = "NativeInputInputKindWire")]
    pub kind: NativeInputInputKind,
    pub device_id: String,
    pub class: NativeInputDeviceClass,
    #[serde(rename = "type")]
    pub input_type: f64,
    pub code: f64,
    pub value: f64,
    pub timestamp: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum NativeInputDeviceAddedKind {
    #[serde(rename = "device-added")]
    DeviceAdded,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NativeInputDeviceAdded {
    #[typeshare(serialized_as = "NativeInputDeviceAddedKindWire")]
    pub kind: NativeInputDeviceAddedKind,
    pub device: NativeInputDeviceInfo,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum NativeInputDeviceRemovedKind {
    #[serde(rename = "device-removed")]
    DeviceRemoved,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeInputDeviceRemoved {
    #[typeshare(serialized_as = "NativeInputDeviceRemovedKindWire")]
    pub kind: NativeInputDeviceRemovedKind,
    pub device_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum NativeInputActionKind {
    #[serde(rename = "action")]
    Action,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum NativeInputSystemAction {
    #[serde(rename = "system")]
    System,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NativeInputAction {
    #[typeshare(serialized_as = "NativeInputActionKindWire")]
    pub kind: NativeInputActionKind,
    pub class: NativeInputDeviceClass,
    #[typeshare(serialized_as = "NativeInputSystemActionWire")]
    pub action: NativeInputSystemAction,
    pub timestamp: f64,
}

#[typeshare(serialized_as = "NativeInputEventWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum NativeInputEvent {
    Input(NativeInputInput),
    DeviceAdded(NativeInputDeviceAdded),
    DeviceRemoved(NativeInputDeviceRemoved),
    Action(NativeInputAction),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeInputSubscription {
    pub classes: Vec<NativeInputDeviceClass>,
}

/// Transient delivery barriers grounded in the host's freezer transitions.
/// Use the same literal/struct union as legacy events: Typeshare does not support
/// internally tagged algebraic enums. Rust still enforces every exact wire kind.
#[typeshare(serialized_as = "NativeInputControlWire")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum NativeInputControl {
    InitializationComplete(NativeInputInitializationComplete),
    DeviceStateComplete(NativeInputDeviceStateComplete),
    Suspend(NativeInputSuspend),
    Resume(NativeInputResume),
    Retired(NativeInputRetired),
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum NativeInputInitializationCompleteKind {
    #[serde(rename = "initialization-complete")]
    InitializationComplete,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeInputInitializationComplete {
    #[typeshare(serialized_as = "NativeInputInitializationCompleteKindWire")]
    pub kind: NativeInputInitializationCompleteKind,
    pub generation: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum NativeInputDeviceStateCompleteKind {
    #[serde(rename = "device-state-complete")]
    DeviceStateComplete,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeInputDeviceStateComplete {
    #[typeshare(serialized_as = "NativeInputDeviceStateCompleteKindWire")]
    pub kind: NativeInputDeviceStateCompleteKind,
    pub device_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum NativeInputSuspendKind {
    #[serde(rename = "suspend")]
    Suspend,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeInputSuspend {
    #[typeshare(serialized_as = "NativeInputSuspendKindWire")]
    pub kind: NativeInputSuspendKind,
    pub generation: String,
    pub request_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum NativeInputResumeKind {
    #[serde(rename = "resume")]
    Resume,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeInputResume {
    #[typeshare(serialized_as = "NativeInputResumeKindWire")]
    pub kind: NativeInputResumeKind,
    pub generation: String,
    pub request_id: String,
}

/// Local blur retirement requests release only this authenticated attachment.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum NativeInputRetireKind {
    #[serde(rename = "retire")]
    Retire,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeInputRetirement {
    #[typeshare(serialized_as = "NativeInputRetireKindWire")]
    pub kind: NativeInputRetireKind,
    pub generation: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum NativeInputRetiredKind {
    #[serde(rename = "retired")]
    Retired,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeInputRetired {
    #[typeshare(serialized_as = "NativeInputRetiredKindWire")]
    pub kind: NativeInputRetiredKind,
    pub generation: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum NativeInputSuspendedKind {
    #[serde(rename = "suspended")]
    Suspended,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeInputAcknowledgement {
    #[typeshare(serialized_as = "NativeInputSuspendedKindWire")]
    pub kind: NativeInputSuspendedKind,
    pub generation: String,
    pub request_id: String,
}
