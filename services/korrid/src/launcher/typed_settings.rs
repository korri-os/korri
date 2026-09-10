//! Source-check output belongs to the launching build, not a version table.
//! The package publisher does not yet supply this evidence. Until it does,
//! omit typed settings with explicit warnings; never claim a key is supported.
use crate::config::cascade::LaunchSettingValue;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingType {
    Boolean,
    Number,
    String,
}

/// Ephemeral renderer-output evidence, not a persisted metadata format.
/// Keys/types come from legacy policy.ts and launch-spec.ts's rendered cfg
/// pairs, checked by the kind against this exact instance's pinned source.
pub struct SourceCheckedSettings<'a> {
    pub version: &'a str,
    pub build: &'a str,
    pub keys: &'a HashMap<String, SettingType>,
}

#[typeshare::typeshare]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchWarning {
    pub setting: String,
    pub launcher_id: String,
    pub build: String,
    pub message: String,
}

pub fn validate(
    settings: HashMap<String, LaunchSettingValue>,
    launcher_id: &str,
    build: &str,
    source: Option<SourceCheckedSettings<'_>>,
) -> (HashMap<String, LaunchSettingValue>, Vec<LaunchWarning>) {
    let mut accepted = HashMap::new();
    let mut warnings = Vec::new();
    let mut settings: Vec<_> = settings.into_iter().collect();
    settings.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in settings {
        let actual = match &value {
            LaunchSettingValue::Boolean(_) => SettingType::Boolean,
            LaunchSettingValue::Number(_) => SettingType::Number,
            LaunchSettingValue::String(_) => SettingType::String,
        };
        if source
            .as_ref()
            .filter(|s| s.build == build)
            .and_then(|s| s.keys.get(&key))
            == Some(&actual)
        {
            accepted.insert(key, value);
        } else {
            let version = source
                .as_ref()
                .map(|s| s.version)
                .unwrap_or("unavailable (source metadata absent)");
            warnings.push(LaunchWarning {
                message: format!("omitted setting {key} for launcher {launcher_id}, version {version}, build {build}: key/type not verified against this build's source"),
                setting: key, launcher_id: launcher_id.into(), build: build.into(),
            });
        }
    }
    (accepted, warnings)
}
