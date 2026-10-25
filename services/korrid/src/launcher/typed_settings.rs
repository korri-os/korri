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

#[derive(Default)]
pub struct SettingsCheck {
    pub warnings: Vec<LaunchWarning>,
    pub skipped: Vec<(String, String)>,
}

/// Validate the resolved nested tree and remove only error paths. Warnings do
/// not discard values. The authored file remains unchanged.
pub fn validate(
    snapshot: &script::source::SourceSnapshot,
    settings: &mut HashMap<String, LaunchSettingValue>,
    id: &str,
    build: &str,
) -> Result<SettingsCheck, String> {
    let description = describe(snapshot, id, build)?;
    let mut check = SettingsCheck {
        warnings: description.diagnostics,
        ..Default::default()
    };
    // Private handoffs can also be inspected directly, without a place fold.
    // Keep the no-invalid-leaves/no-null guarantee at the plugin boundary.
    let (leaves, invalid_names) = crate::config::cascade::leaves(settings);
    let mut nested = HashMap::new();
    for (path, value) in leaves {
        match value {
            LaunchSettingValue::Invalid(_) => {
                let message = "setting values must be scalars, nested objects or null".to_owned();
                check.skipped.push((path.clone(), message.clone()));
                check.warnings.push(LaunchWarning {
                    setting: path,
                    runner_id: id.into(),
                    build: build.into(),
                    message,
                });
            }
            LaunchSettingValue::Cleared => {}
            value => crate::config::cascade::set_leaf(&mut nested, &path, Some(value)),
        }
    }
    for path in invalid_names {
        let message = "setting names must not contain dots or be empty".to_owned();
        check.skipped.push((path.clone(), message.clone()));
        check.warnings.push(LaunchWarning {
            setting: path,
            runner_id: id.into(),
            build: build.into(),
            message,
        });
    }
    *settings = nested;
    let input = serde_json::json!({"values":settings,"revision":description.revision}).to_string();
    let result =
        match script::call_plugin_operation_snapshot(snapshot, script::SETTINGS_VALIDATE, &input) {
            Ok(result) => result,
            Err(failure) if failure.is_unimplemented() => return Ok(check),
            Err(failure) => return Err(failure.to_string()),
        };
    let validation: SettingsValidation = serde_json::from_str(&result)
        .map_err(|error| format!("invalid settings.validate result: {error}"))?;
    for diagnostic in validation.diagnostics {
        let path = diagnostic.path.join(".");
        if diagnostic.severity == DiagnosticSeverity::Error {
            check
                .skipped
                .push((path.clone(), diagnostic.message.clone()));
            if path.is_empty() {
                settings.clear();
            } else {
                crate::config::cascade::set_leaf(settings, &path, None);
            }
        }
        if diagnostic.severity != DiagnosticSeverity::Info {
            check.warnings.push(LaunchWarning {
                setting: path,
                runner_id: id.into(),
                build: build.into(),
                message: diagnostic.message,
            });
        }
    }
    Ok(check)
}

/// Routes without a launch plan use the same validation boundary as prepare.
pub fn render(
    snapshot: &script::source::SourceSnapshot,
    id: &str,
    settings: &HashMap<String, LaunchSettingValue>,
    display: Option<super::plugin_launch::PluginDisplay>,
    build: &str,
) -> Result<(Vec<String>, Vec<LaunchWarning>), String> {
    let mut settings = settings.clone();
    let warnings = validate(snapshot, &mut settings, id, build)?;
    #[derive(serde::Serialize)]
    struct RenderInput<'a> {
        id: &'a str,
        settings: &'a HashMap<String, LaunchSettingValue>,
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<super::plugin_launch::PluginDisplay>,
    }
    let input = serde_json::to_string(&RenderInput {
        id,
        settings: &settings,
        display,
    })
    .map_err(|e| e.to_string())?;
    let result = script::call_plugin_operation_snapshot(snapshot, script::SETTINGS_RENDER, &input)
        .map_err(|e| e.to_string())?;
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Rendered {
        args: Vec<String>,
    }
    let rendered: Rendered = serde_json::from_str(&result)
        .map_err(|e| format!("invalid settings.render result: {e}"))?;
    if rendered.args.iter().any(|v| v.contains('\0')) {
        return Err("settings.render arguments contain NUL".into());
    }
    Ok((rendered.args, warnings.warnings))
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
    fn collect(
        schema: &serde_json::Value,
        prefix: &str,
        defaults: &mut HashMap<String, LaunchSettingValue>,
        scopes: &mut HashMap<String, SettingScope>,
        diagnostics: &mut Vec<LaunchWarning>,
        id: &str,
        build: &str,
    ) {
        if let Some(properties) = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
        {
            for (name, property) in properties {
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}.{name}")
                };
                if property.get("type").and_then(serde_json::Value::as_str) == Some("object") {
                    collect(property, &path, defaults, scopes, diagnostics, id, build);
                    continue;
                }
                if let Some(value) = property
                    .get("default")
                    .filter(|v| v.is_boolean() || v.is_number() || v.is_string())
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                {
                    defaults.insert(path.clone(), value);
                }
                match property.get("x-korri-scope") {
                    None => {
                        scopes.insert(path, SettingScope::Shared);
                    }
                    Some(serde_json::Value::String(scope)) if scope == "device" => {
                        scopes.insert(path, SettingScope::Device);
                    }
                    Some(_) => diagnostics.push(LaunchWarning {
                        setting: path,
                        runner_id: id.into(),
                        build: build.into(),
                        message: "x-korri-scope must be \"device\" or absent for shared settings"
                            .into(),
                    }),
                }
            }
        }
    }
    collect(
        &schema,
        "",
        &mut defaults,
        &mut scopes,
        &mut diagnostics,
        runner_id,
        build,
    );
    Ok(SettingsDescription {
        schema_available: value.get("schema").is_some(),
        schema,
        revision,
        defaults,
        scopes,
        diagnostics,
    })
}
