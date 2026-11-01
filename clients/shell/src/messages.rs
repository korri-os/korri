//! The treaty values the shell itself builds or reads (contract 3.2, 3.3).
//!
//! Wire shape, shared with korrid: tagged unions carry a PascalCase `_tag`,
//! fields are camelCase, and plain string enums are camelCase. The shell
//! builds `SurfaceMessage`s and reads only the tags of requests; korrid
//! validates and performs every request.

use serde_json::{json, Value};

/// `SurfaceButton`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Options,
    Menu,
    System,
}

impl Button {
    pub const ALL: [Button; 9] = [
        Button::Up,
        Button::Down,
        Button::Left,
        Button::Right,
        Button::Confirm,
        Button::Back,
        Button::Options,
        Button::Menu,
        Button::System,
    ];

    pub fn wire(self) -> &'static str {
        match self {
            Button::Up => "up",
            Button::Down => "down",
            Button::Left => "left",
            Button::Right => "right",
            Button::Confirm => "confirm",
            Button::Back => "back",
            Button::Options => "options",
            Button::Menu => "menu",
            Button::System => "system",
        }
    }
}

pub fn pressed(button: Button) -> Value {
    json!({"_tag": "Pressed", "button": button.wire()})
}

pub fn korri_published() -> Value {
    json!({"_tag": "KorriPublished"})
}

pub fn preference_stored(key: &str) -> Value {
    json!({"_tag": "PreferenceStored", "key": key})
}

/// Every `SurfaceRequest` tag (contract 3.3).
pub const REQUEST_TAGS: [&str; 25] = [
    "LaunchGame",
    "RunAction",
    "ChangeSetting",
    "DismissSettingsProblem",
    "ExportIdentityBackup",
    "SwitchIdentityFromBackup",
    "SwitchIdentityToNip46",
    "DeleteRetiredIdentity",
    "DismissIdentityStatus",
    "RunGameAction",
    "InvokeGameplayControl",
    "DismissGameplayOverlay",
    "Retry",
    "Dismiss",
    "Reload",
    "ScanWifi",
    "ConnectWifi",
    "ConnectHiddenWifi",
    "FinishOnboarding",
    "OpenStore",
    "SearchStore",
    "OpenStoreEntry",
    "FocusGame",
    "RememberPreference",
    "MoveCatalog",
];

/// The tag of a request the treaty knows, or `None`.
pub fn request_tag(request: &Value) -> Option<&str> {
    let tag = request.get("_tag")?.as_str()?;
    REQUEST_TAGS.contains(&tag).then_some(tag)
}

/// `RememberPreference { key, value }`.
pub fn remembered(request: &Value) -> Option<(&str, &str)> {
    if request.get("_tag")?.as_str()? != "RememberPreference" {
        return None;
    }
    Some((request.get("key")?.as_str()?, request.get("value")?.as_str()?))
}

/// Largest count a `MoveCatalog` may ask for (contract 3.4).
pub const MAX_CATALOG_COUNT: u64 = 200;

/// Whether the presentation is the gameplay overlay (contract 3.7).
pub fn is_gameplay_overlay(model: &Value) -> bool {
    model.pointer("/presentation/kind").and_then(Value::as_str) == Some("gameplay-overlay")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_agreed_wire_shapes() {
        assert_eq!(pressed(Button::Left).to_string(), r#"{"_tag":"Pressed","button":"left"}"#);
        assert_eq!(korri_published().to_string(), r#"{"_tag":"KorriPublished"}"#);
        assert_eq!(preference_stored("zone.look").to_string(), r#"{"_tag":"PreferenceStored","key":"zone.look"}"#);
    }

    #[test]
    fn reads_request_tags_and_preferences() {
        assert_eq!(request_tag(&json!({"_tag": "MoveCatalog"})), Some("MoveCatalog"));
        assert_eq!(request_tag(&json!({"_tag": "DismissProblem"})), None);
        assert_eq!(request_tag(&json!({"tag": "Retry"})), None);
        assert_eq!(remembered(&json!({"_tag": "RememberPreference", "key": "pico.font", "value": "tiny5"})), Some(("pico.font", "tiny5")));
        assert!(is_gameplay_overlay(&json!({"presentation": {"kind": "gameplay-overlay"}})));
        assert!(!is_gameplay_overlay(&json!({"presentation": {"kind": "catalog"}})));
    }
}
