//! Shared setting definitions from ADR 0057. No runner-native keys belong here.
use serde_json::{json, Value};

#[typeshare::typeshare]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SharedSetting {
    #[serde(rename = "video.render-scale")]
    RenderScale,
    #[serde(rename = "video.aspect-ratio")]
    AspectRatio,
    #[serde(rename = "video.window-mode")]
    WindowMode,
    #[serde(rename = "video.vsync")]
    Vsync,
    #[serde(rename = "video.fps-limit")]
    FpsLimit,
    #[serde(rename = "video.renderer")]
    Renderer,
    #[serde(rename = "video.resolution.width")]
    Width,
    #[serde(rename = "video.resolution.height")]
    Height,
    #[serde(rename = "audio.volume-percent")]
    Volume,
    #[serde(rename = "audio.music.volume-percent")]
    MusicVolume,
    #[serde(rename = "audio.effects.volume-percent")]
    EffectsVolume,
    #[serde(rename = "audio.voice.volume-percent")]
    VoiceVolume,
    #[serde(rename = "input.rumble")]
    Rumble,
    #[serde(rename = "input.rumble-strength-percent")]
    RumbleStrength,
    #[serde(rename = "input.deadzone-percent")]
    Deadzone,
    #[serde(rename = "locale.language")]
    Language,
    #[serde(rename = "locale.region")]
    Region,
    #[serde(rename = "video.texture-filter")]
    TextureFilter,
    #[serde(rename = "video.scaling.filter")]
    ScalingFilter,
    #[serde(rename = "video.scaling.scaler")]
    Scaler,
    #[serde(rename = "video.anisotropic-filtering")]
    AnisotropicFiltering,
    #[serde(rename = "video.msaa")]
    Msaa,
    #[serde(rename = "video.rotation-degrees")]
    Rotation,
    #[serde(rename = "rewind.enable")]
    Rewind,
    #[serde(rename = "speed.fast-forward-multiplier")]
    FastForward,
    #[serde(rename = "saves.auto-save-state")]
    AutoSaveState,
    #[serde(rename = "saves.auto-load-state")]
    AutoLoadState,
    #[serde(rename = "cheats.enable")]
    Cheats,
    #[serde(rename = "achievements.enable")]
    Achievements,
    #[serde(rename = "achievements.hardcore")]
    Hardcore,
}

pub const ALL: &[SharedSetting] = &[
    SharedSetting::RenderScale,
    SharedSetting::AspectRatio,
    SharedSetting::WindowMode,
    SharedSetting::Vsync,
    SharedSetting::FpsLimit,
    SharedSetting::Renderer,
    SharedSetting::Width,
    SharedSetting::Height,
    SharedSetting::Volume,
    SharedSetting::MusicVolume,
    SharedSetting::EffectsVolume,
    SharedSetting::VoiceVolume,
    SharedSetting::Rumble,
    SharedSetting::RumbleStrength,
    SharedSetting::Deadzone,
    SharedSetting::Language,
    SharedSetting::Region,
    SharedSetting::TextureFilter,
    SharedSetting::ScalingFilter,
    SharedSetting::Scaler,
    SharedSetting::AnisotropicFiltering,
    SharedSetting::Msaa,
    SharedSetting::Rotation,
    SharedSetting::Rewind,
    SharedSetting::FastForward,
    SharedSetting::AutoSaveState,
    SharedSetting::AutoLoadState,
    SharedSetting::Cheats,
    SharedSetting::Achievements,
    SharedSetting::Hardcore,
];

impl SharedSetting {
    pub fn path(self) -> String {
        serde_json::to_value(self)
            .expect("setting serializes")
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub fn from_path(path: &str) -> Option<Self> {
        serde_json::from_value(Value::String(path.into())).ok()
    }

    pub fn device_only(self) -> bool {
        matches!(
            self,
            Self::RenderScale | Self::Renderer | Self::WindowMode | Self::Vsync | Self::FpsLimit
        )
    }

    pub fn property(self) -> Value {
        use SharedSetting::*;
        let choices = |values: &[&str]| json!({"type":"string", "enum":values});
        let mut property = match self {
            RenderScale => json!({"type":"number", "exclusiveMinimum":0}),
            FastForward => json!({"type":"number", "minimum":0}),
            Width | Height => json!({"type":"integer", "minimum":1, "maximum":u32::MAX}),
            FpsLimit => json!({"type":"integer", "minimum":0, "maximum":u32::MAX}),
            Volume | MusicVolume | EffectsVolume | VoiceVolume | RumbleStrength | Deadzone => {
                json!({"type":"number", "minimum":0, "maximum":100})
            }
            AspectRatio => choices(&["original", "fill-view", "stretch", "4:3", "16:9"]),
            WindowMode => choices(&["windowed", "borderless", "exclusive"]),
            Region => choices(&[
                "japan",
                "north-america",
                "europe",
                "australia",
                "korea",
                "china",
                "taiwan",
                "asia",
            ]),
            TextureFilter => choices(&["nearest", "linear"]),
            ScalingFilter => choices(&["nearest", "linear", "sharp-linear"]),
            Scaler => choices(&["fit", "integer"]),
            AnisotropicFiltering => choices(&["off", "2x", "4x", "8x", "16x"]),
            Msaa => choices(&["off", "2x", "4x", "8x"]),
            Rotation => json!({"type":"integer", "enum":[0,90,180,270]}),
            // A runner can have its own API identifier. Language tags are not
            // enumerated: each runner reports the languages it can represent.
            Renderer | Language => json!({"type":"string", "minLength":1}),
            _ => json!({"type":"boolean"}),
        };
        if self.device_only() {
            property["x-korri-scope"] = json!("device");
        }
        property
    }
}

/// All shared paths remain visible in an editor, including unsupported ones.
/// An unsupported value can still be authored at a broad place for other runners.
pub fn schema(supported: &[SharedSetting], plugin: Value) -> Value {
    fn insert(node: &mut Value, parts: &[&str], property: Value) {
        if parts.len() == 1 {
            node["properties"][parts[0]] = property;
        } else {
            if node["properties"].get(parts[0]).is_none() {
                node["properties"][parts[0]] =
                    json!({"type":"object", "properties":{}, "additionalProperties":false});
            }
            insert(&mut node["properties"][parts[0]], &parts[1..], property);
        }
    }
    let mut root = json!({"type":"object", "properties":{}, "additionalProperties":false});
    for &setting in ALL {
        let mut property = setting.property();
        property["x-korri-supported"] = json!(supported.contains(&setting));
        insert(
            &mut root,
            &setting.path().split('.').collect::<Vec<_>>(),
            property,
        );
    }
    root["properties"]["plugin"] = plugin;
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_paths_are_unique_and_round_trip() {
        let mut paths = std::collections::BTreeSet::new();
        for &setting in ALL {
            assert!(paths.insert(setting.path()));
            assert_eq!(SharedSetting::from_path(&setting.path()), Some(setting));
            assert!(setting.property().get("default").is_none());
        }
        assert!(SharedSetting::from_path("video.resolution_multiplier").is_none());
    }

    #[test]
    fn only_the_five_chosen_settings_are_device_only() {
        let paths: Vec<_> = ALL
            .iter()
            .filter(|s| s.device_only())
            .map(|s| s.path())
            .collect();
        assert_eq!(
            paths,
            [
                "video.render-scale",
                "video.window-mode",
                "video.vsync",
                "video.fps-limit",
                "video.renderer"
            ]
        );
    }

    #[test]
    fn unsupported_paths_remain_visible_without_changing_their_meaning() {
        let tree = schema(
            &[SharedSetting::RenderScale],
            json!({"type":"object", "properties":{}}),
        );
        let video = &tree["properties"]["video"]["properties"];
        assert_eq!(video["render-scale"]["x-korri-supported"], true);
        assert_eq!(video["fps-limit"]["x-korri-supported"], false);
        assert_eq!(video["render-scale"]["x-korri-scope"], "device");
        assert_eq!(video["render-scale"]["exclusiveMinimum"], 0);
        assert!(tree["properties"].get("plugin").is_some());
    }
}
