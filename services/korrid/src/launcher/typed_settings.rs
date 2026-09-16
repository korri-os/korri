//! Settings validation belongs to the runner, not to korrid.
//!
//! korrid used to read a `<program key>-settings` JSON file out of the
//! installed package and compare every authored key and type against it. That
//! made the host the owner of one emulator's option table: a runner could not
//! change what it accepts without changing korrid, and a runner korrid had
//! never heard of could say nothing about its own settings at all.
//!
//! Now korrid asks. `settings.validate` reports what this build cannot apply,
//! and the runner keeps the schema that answers it. The host still owns the
//! result: it surfaces the diagnostics as launch warnings and never rewrites
//! or deletes what a person authored.
use crate::{config::cascade::LaunchSettingValue, script};
use std::collections::HashMap;

#[typeshare::typeshare]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchWarning {
    pub setting: String,
    pub runner_id: String,
    pub build: String,
    pub message: String,
}

/// One reported problem. The runner names the setting through `path`; korrid
/// adds the identity facts, which are its own and never the plugin's to claim.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsDiagnostic {
    #[allow(dead_code)]
    code: String,
    severity: DiagnosticSeverity,
    message: String,
    #[serde(default)]
    path: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsValidation {
    #[allow(dead_code)]
    valid: bool,
    #[serde(default)]
    diagnostics: Vec<SettingsDiagnostic>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidateInput<'a> {
    runner_id: &'a str,
    values: &'a HashMap<String, LaunchSettingValue>,
}

/// Ask the runner about the authored values. A runner that implements no
/// `settings.validate` handler reports nothing, which is not an error: the
/// values still reach `launch.prepare`, which is the operation that decides
/// what it can render.
pub fn validate(
    snapshot: &script::source::SourceSnapshot,
    settings: &HashMap<String, LaunchSettingValue>,
    runner_id: &str,
    build: &str,
) -> Result<Vec<LaunchWarning>, String> {
    let input = serde_json::to_string(&ValidateInput {
        runner_id,
        values: settings,
    })
    .map_err(|error| error.to_string())?;
    let result =
        match script::call_plugin_operation_snapshot(snapshot, script::SETTINGS_VALIDATE, &input) {
            Ok(result) => result,
            Err(failure) if failure.is_unimplemented() => return Ok(Vec::new()),
            Err(failure) => return Err(failure.to_string()),
        };
    let validation: SettingsValidation = serde_json::from_str(&result)
        .map_err(|error| format!("invalid settings.validate result: {error}"))?;
    Ok(validation
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.severity != DiagnosticSeverity::Info)
        .map(|diagnostic| LaunchWarning {
            setting: diagnostic.path.first().cloned().unwrap_or_default(),
            runner_id: runner_id.to_owned(),
            build: build.to_owned(),
            message: diagnostic.message,
        })
        .collect())
}
