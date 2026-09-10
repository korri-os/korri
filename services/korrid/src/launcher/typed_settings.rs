//! Source-check output belongs to the launching instance, never its kind.
use crate::{config::cascade::LaunchSettingValue, plugin_installation::EnabledPackage};
use std::{collections::HashMap, fs::File, io::Read};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
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

/// Derived artifact registered in the existing manifest files map as
/// `<program file key>-settings`. `program` is PluginLaunchInput.program;
/// version/keys are the existing SourceCheckedSettings evidence. Binding the
/// enclosing installed package here avoids a self-referential Nix output.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackagedSettings {
    program: String,
    version: String,
    keys: HashMap<String, SettingType>,
}

impl PackagedSettings {
    pub fn read(package: &EnabledPackage, program_key: &str) -> Result<Option<Self>, String> {
        let Some(path) = package.files.get(&format!("{program_key}-settings")) else {
            return Ok(None);
        };
        let mut bytes = Vec::new();
        File::open(path)
            .and_then(|file| file.take(1024 * 1024 + 1).read_to_end(&mut bytes))
            .map_err(|error| format!("read source-checked settings {}: {error}", path.display()))?;
        if bytes.len() > 1024 * 1024 {
            return Err("source-checked settings exceed 1 MiB".into());
        }
        let evidence: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid source-checked settings: {error}"))?;
        if evidence.version.is_empty() {
            return Err("source-checked settings have no program version".into());
        }
        Ok(Some(evidence))
    }

    pub fn validate(
        &self,
        settings: HashMap<String, LaunchSettingValue>,
        launcher_id: &str,
        build: &str,
        program: &str,
    ) -> (HashMap<String, LaunchSettingValue>, Vec<LaunchWarning>) {
        let unverified = HashMap::new();
        validate(
            settings,
            launcher_id,
            build,
            Some(SourceCheckedSettings {
                version: &self.version,
                build,
                // Preserve the display version in warnings, but accept no
                // evidence for a different executable in this instance.
                keys: if self.program == program {
                    &self.keys
                } else {
                    &unverified
                },
            }),
        )
    }
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
