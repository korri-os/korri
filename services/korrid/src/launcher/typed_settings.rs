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
use super::plugin_launch::{
    PluginSettingsDescribeInput, PluginSettingsDescribeOutput, PluginSettingsDiagnosticSeverity,
    PluginSettingsRenderInput, PluginSettingsRenderOutput, PluginSettingsValidateInput,
    PluginSettingsValidateOutput,
};
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
    let mut description = describe(snapshot, id, build)?;
    let paths = crate::config::cascade::leaves(settings)
        .0
        .into_iter()
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    description.include_paths(&paths, id, build);
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
    let input = serde_json::to_string(&PluginSettingsValidateInput {
        values: settings.clone(),
        revision: description.revision,
    })
    .map_err(|error| error.to_string())?;
    let result =
        match script::call_plugin_operation_snapshot(snapshot, script::SETTINGS_VALIDATE, &input) {
            Ok(result) => result,
            Err(failure) if failure.is_unimplemented() => return Ok(check),
            Err(failure) => return Err(failure.to_string()),
        };
    let validation: PluginSettingsValidateOutput = serde_json::from_str(&result)
        .map_err(|error| format!("invalid settings.validate result: {error}"))?;
    for diagnostic in validation.diagnostics {
        let path = diagnostic.path.join(".");
        if diagnostic.severity == PluginSettingsDiagnosticSeverity::Error {
            check
                .skipped
                .push((path.clone(), diagnostic.message.clone()));
            if path.is_empty() {
                settings.clear();
            } else {
                crate::config::cascade::set_leaf(settings, &path, None);
            }
        }
        if diagnostic.severity != PluginSettingsDiagnosticSeverity::Info {
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
    let input = serde_json::to_string(&PluginSettingsRenderInput {
        id: id.into(),
        settings,
        display,
    })
    .map_err(|e| e.to_string())?;
    let result = script::call_plugin_operation_snapshot(snapshot, script::SETTINGS_RENDER, &input)
        .map_err(|e| e.to_string())?;
    let rendered: PluginSettingsRenderOutput = serde_json::from_str(&result)
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
    let input = serde_json::to_string(&PluginSettingsDescribeInput {
        id: runner_id.into(),
    })
    .map_err(|error| error.to_string())?;
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
    let value: PluginSettingsDescribeOutput = serde_json::from_str(&result)
        .map_err(|e| format!("invalid settings.describe result: {e}"))?;
    let defaults = HashMap::new();
    let schema = value.schema;
    let revision = value.revision;
    let scopes = HashMap::new();
    let diagnostics = Vec::new();
    let mut description = SettingsDescription {
        schema_available: true,
        schema,
        revision,
        defaults,
        scopes,
        diagnostics,
    };
    description.include_paths(&[], runner_id, build);
    Ok(description)
}

impl SettingsDescription {
    /// Pattern objects have no enumerable keys. Expand only concrete paths
    /// present in authored settings, including cleared leaves; never invent keys.
    pub fn include_paths(&mut self, paths: &[String], id: &str, build: &str) {
        struct Metadata<'a> {
            defaults: &'a mut HashMap<String, LaunchSettingValue>,
            scopes: &'a mut HashMap<String, SettingScope>,
            diagnostics: &'a mut Vec<LaunchWarning>,
            id: &'a str,
            build: &'a str,
        }
        impl Metadata<'_> {
            fn collect(&mut self, schema: &serde_json::Value, prefix: &str, paths: &[String]) {
                let mut names: std::collections::BTreeSet<String> = schema
                    .get("properties")
                    .and_then(serde_json::Value::as_object)
                    .into_iter()
                    .flat_map(|properties| properties.keys().cloned())
                    .collect();
                let dotted_prefix = if prefix.is_empty() {
                    String::new()
                } else {
                    format!("{prefix}.")
                };
                names.extend(paths.iter().filter_map(|path| {
                    path.strip_prefix(&dotted_prefix)
                        .and_then(|tail| tail.split('.').next())
                        .filter(|name| !name.is_empty())
                        .map(str::to_owned)
                }));
                for name in names {
                    let Some(property) = crate::config::cascade::schema_child(schema, &name) else {
                        continue;
                    };
                    let path = format!("{dotted_prefix}{name}");
                    if property.get("type").and_then(serde_json::Value::as_str) == Some("object") {
                        self.collect(property, &path, paths);
                        continue;
                    }
                    if let Some(value) = property
                        .get("default")
                        .filter(|v| v.is_boolean() || v.is_number() || v.is_string())
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                    {
                        self.defaults.insert(path.clone(), value);
                    }
                    match property.get("x-korri-scope") {
                        None => {
                            self.scopes.insert(path, SettingScope::Shared);
                        }
                        Some(serde_json::Value::String(scope)) if scope == "device" => {
                            self.scopes.insert(path, SettingScope::Device);
                        }
                        Some(_) => self.diagnostics.push(LaunchWarning {
                            setting: path,
                            runner_id: self.id.into(),
                            build: self.build.into(),
                            message:
                                "x-korri-scope must be \"device\" or absent for shared settings"
                                    .into(),
                        }),
                    }
                }
            }
        }
        self.defaults.clear();
        self.scopes.clear();
        self.diagnostics.clear();
        Metadata {
            defaults: &mut self.defaults,
            scopes: &mut self.scopes,
            diagnostics: &mut self.diagnostics,
            id,
            build,
        }
        .collect(&self.schema, "", paths);
    }
}
