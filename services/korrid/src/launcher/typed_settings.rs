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
    let mut warnings = describe(snapshot, runner_id, build)?.diagnostics;
    let input = serde_json::to_string(&ValidateInput {
        runner_id,
        values: settings,
    })
    .map_err(|error| error.to_string())?;
    let result =
        match script::call_plugin_operation_snapshot(snapshot, script::SETTINGS_VALIDATE, &input) {
            Ok(result) => result,
            Err(failure) if failure.is_unimplemented() => return Ok(warnings),
            Err(failure) => return Err(failure.to_string()),
        };
    let validation: SettingsValidation = serde_json::from_str(&result)
        .map_err(|error| format!("invalid settings.validate result: {error}"))?;
    warnings.extend(
        validation
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.severity != DiagnosticSeverity::Info)
            .map(|diagnostic| LaunchWarning {
                setting: diagnostic.path.first().cloned().unwrap_or_default(),
                runner_id: runner_id.to_owned(),
                build: build.to_owned(),
                message: diagnostic.message,
            }),
    );
    Ok(warnings)
}

/// Scope belongs to a setting kind, never to an authored layer (ADR 0018).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingScope {
    Shared,
    Device,
}
#[derive(Debug)]
pub struct SettingsDescription {
    pub schema_available: bool,
    pub schema: serde_json::Value,
    pub revision: String,
    pub defaults: HashMap<String, LaunchSettingValue>,
    pub scopes: HashMap<String, SettingScope>,
    pub diagnostics: Vec<LaunchWarning>,
}
/// Retain the marks for future settings consumers; plugins own their schema.
pub fn describe(
    snapshot: &script::source::SourceSnapshot,
    runner_id: &str,
    build: &str,
) -> Result<SettingsDescription, String> {
    let input = serde_json::json!({"id":runner_id}).to_string();
    let result =
        match script::call_plugin_operation_snapshot(snapshot, script::SETTINGS_DESCRIBE, &input) {
            Ok(result) => result,
            Err(failure) if failure.is_unimplemented() => {
                return Ok(SettingsDescription {
                    schema_available: false,
                    schema: serde_json::json!({}),
                    revision: String::new(),
                    defaults: HashMap::new(),
                    scopes: HashMap::new(),
                    diagnostics: Vec::new(),
                })
            }
            Err(failure) => return Err(failure.to_string()),
        };
    let value: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| format!("invalid settings.describe result: {e}"))?;
    let mut defaults = HashMap::new();
    let schema = value
        .get("schema")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let revision = value
        .get("revision")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut scopes = HashMap::new();
    let mut diagnostics = Vec::new();
    if let Some(properties) = value
        .get("schema")
        .and_then(|s| s.get("properties"))
        .and_then(serde_json::Value::as_object)
    {
        for (key, property) in properties {
            if let Some(value) = property
                .get("default")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
            {
                defaults.insert(key.clone(), value);
            }
            match property.get("x-korri-scope") {
                None => {
                    scopes.insert(key.clone(), SettingScope::Shared);
                }
                Some(serde_json::Value::String(scope)) if scope == "device" => {
                    scopes.insert(key.clone(), SettingScope::Device);
                }
                Some(_) => diagnostics.push(LaunchWarning {
                    setting: key.clone(),
                    runner_id: runner_id.into(),
                    build: build.into(),
                    message: "x-korri-scope must be \"device\" or absent for shared settings"
                        .into(),
                }),
            }
        }
    }
    Ok(SettingsDescription {
        schema_available: value.get("schema").is_some(),
        schema,
        revision,
        defaults,
        scopes,
        diagnostics,
    })
}
