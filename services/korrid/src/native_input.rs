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
