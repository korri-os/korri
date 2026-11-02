//! The treaty values the shell builds or reads (contract 3.2, 3.3), through
//! the treaty crate `korri-surface-treaty` (`contracts/surface/`), so the
//! shell and korrid share one serialisation.

use korri_surface_treaty::{SurfaceMessage, SurfaceRequest, CATALOG_MAX_COUNT};
use serde_json::Value;

pub use korri_surface_treaty::SurfaceButton as Button;

/// Every button, for the verifier's sweep.
pub const BUTTONS: [Button; 9] = [
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

fn json(message: SurfaceMessage) -> Value {
    serde_json::to_value(message).expect("a SurfaceMessage serialises")
}

pub fn pressed(button: Button) -> Value {
    json(SurfaceMessage::Pressed { button })
}

pub fn korri_published() -> Value {
    json(SurfaceMessage::KorriPublished)
}

/// Contract amendment A1 (section 7.1): once a second, the time since the
/// last `Pressed` delivered to the surface, or since it loaded.
pub fn tick(idle_ms: u32) -> Value {
    json(SurfaceMessage::Tick { idle_ms })
}

/// How often the shell sends `Tick`.
pub const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

pub fn preference_stored(key: &str) -> Value {
    json(SurfaceMessage::PreferenceStored {
        key: key.to_owned(),
    })
}

/// A request as the treaty reads it, or why it is not one. The reason never
/// includes the request's values (a Wi-Fi password may be among them).
pub fn request(value: &Value) -> Result<SurfaceRequest, String> {
    let tag = value
        .get("_tag")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .chars()
        .take(40)
        .collect::<String>();
    SurfaceRequest::deserialize(value)
        .map_err(|_| format!("update returned a request that is not a SurfaceRequest ({tag})"))
}

use serde::Deserialize as _;

/// The tag of a valid request, safe to log.
pub fn request_tag(value: &Value) -> Option<&'static str> {
    request(value).ok().map(|r| r.tag())
}

/// `RememberPreference { key, value }`.
pub fn remembered(value: &Value) -> Option<(String, String)> {
    match request(value).ok()? {
        SurfaceRequest::RememberPreference { key, value } => Some((key, value)),
        _ => None,
    }
}

/// Largest count a `MoveCatalog` may ask for (contract 3.4).
pub const MAX_CATALOG_COUNT: u64 = CATALOG_MAX_COUNT as u64;

/// Whether the presentation is the gameplay overlay (contract 3.7).
pub fn is_gameplay_overlay(model: &Value) -> bool {
    model.pointer("/presentation/kind").and_then(Value::as_str) == Some("gameplay-overlay")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_the_agreed_wire_shapes() {
        assert_eq!(
            pressed(Button::Left).to_string(),
            r#"{"_tag":"Pressed","button":"left"}"#
        );
        assert_eq!(
            korri_published().to_string(),
            r#"{"_tag":"KorriPublished"}"#
        );
        assert_eq!(
            tick(31_000).to_string(),
            r#"{"_tag":"Tick","idleMs":31000}"#
        );
        assert_eq!(
            preference_stored("zone.look").to_string(),
            r#"{"_tag":"PreferenceStored","key":"zone.look"}"#
        );
    }

    #[test]
    fn reads_requests_through_the_treaty() {
        let around = json!({"_tag": "MoveCatalog", "query": {"text": "", "section": null, "order": "korri"}, "from": {"_tag": "Around", "gameId": "g"}, "count": 50});
        assert_eq!(request_tag(&around), Some("MoveCatalog"));
        assert_eq!(request_tag(&json!({"_tag": "DismissProblem"})), None);
        assert_eq!(
            request_tag(&json!({"_tag": "LaunchGame", "locationId": "x"})),
            None,
            "fields are checked, not only tags"
        );
        assert_eq!(
            request_tag(&json!({"_tag": "LaunchGame", "gameId": "g", "launchLocationId": "x"})),
            Some("LaunchGame")
        );
        assert_eq!(
            remembered(
                &json!({"_tag": "RememberPreference", "key": "pico.font", "value": "tiny5"})
            ),
            Some(("pico.font".into(), "tiny5".into()))
        );
        let secret =
            request(&json!({"_tag": "ConnectWifi", "networkId": 3, "password": "hunter2"}))
                .err()
                .unwrap();
        assert!(!secret.contains("hunter2"));
        assert!(is_gameplay_overlay(
            &json!({"presentation": {"kind": "gameplay-overlay"}})
        ));
    }
}
