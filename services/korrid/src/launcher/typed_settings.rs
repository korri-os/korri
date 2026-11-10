//! Korri owns shared definitions; runners own native settings and translation.
//!
//! A runner declares supported shared paths and its plugin-only schema.
//! `settings.validate` reports translation limits and rounding. The host skips
//! invalid or unsupported values without rewriting authored records. Modifiers
//! and launch services keep distinct values inside the same plugin bucket.
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
    validate_with_description(
        snapshot,
        settings,
        id,
        build,
        describe(snapshot, id, build)?,
        false,
    )
}

fn validate_with_description(
    snapshot: &script::source::SourceSnapshot,
    settings: &mut HashMap<String, LaunchSettingValue>,
    id: &str,
    build: &str,
    mut description: SettingsDescription,
    runner: bool,
) -> Result<SettingsCheck, String> {
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
    for (path, value) in crate::config::cascade::leaves(settings).0 {
        let reason = if !runner {
            (!path.starts_with("plugin."))
                .then(|| "plugin-only settings belong under plugin".to_owned())
        } else {
            match korri_settings_treaty::SharedSetting::from_path(&path) {
                Some(setting) if !description.shared.contains(&setting) => {
                    Some(format!("not supported by {id}"))
                }
                Some(setting) => crate::launch_settings::store::validate_property(
                    &setting.property(),
                    &serde_json::to_value(&value).map_err(|e| e.to_string())?,
                )
                .err(),
                None if path.starts_with("plugin.") => None,
                None => {
                    Some("unknown shared setting; runner-only settings belong under plugin".into())
                }
            }
        };
        if let Some(message) = reason {
            crate::config::cascade::set_leaf(settings, &path, None);
            check.skipped.push((path.clone(), message.clone()));
            check.warnings.push(LaunchWarning {
                setting: path,
                runner_id: id.into(),
                build: build.into(),
                message,
            });
        }
    }
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
    pub shared: Vec<korri_settings_treaty::SharedSetting>,
    pub schema_available: bool,
    pub schema: serde_json::Value,
    pub revision: String,
    pub defaults: HashMap<String, LaunchSettingValue>,
    pub scopes: HashMap<String, SettingScope>,
    pub diagnostics: Vec<LaunchWarning>,
}
/// A modifier or launch service keeps its own vocabulary under plugin.
pub fn describe(
    snapshot: &script::source::SourceSnapshot,
    id: &str,
    build: &str,
) -> Result<SettingsDescription, String> {
    let mut description = describe_native(snapshot, id, build)?;
    description.schema = serde_json::json!({"type":"object", "properties":{"plugin":korri_settings_treaty::plugin_object(description.schema)}});
    description.include_paths(&[], id, build);
    Ok(description)
}

/// Read the native schema before the host adds its namespace and shared fields.
fn describe_native(
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
                    shared: Vec::new(),
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
        shared: value.shared,
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

/// Runner settings have authoritative shared definitions and a plugin bucket.
/// Modifiers and services receive only their plugin bucket, without shared paths.
pub fn describe_runner(
    snapshot: &script::source::SourceSnapshot,
    id: &str,
    build: &str,
) -> Result<SettingsDescription, String> {
    let mut description = describe_native(snapshot, id, build)?;
    description.schema = korri_settings_treaty::schema(&description.shared, description.schema);
    description.schema_available = true;
    fn annotate(node: &mut serde_json::Value, id: &str) {
        if node
            .get("x-korri-supported")
            .and_then(serde_json::Value::as_bool)
            == Some(false)
        {
            node["description"] = serde_json::json!(format!(
                "Not supported by {id}. This value remains available for other runners."
            ));
        }
        if let Some(properties) = node
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
        {
            for property in properties.values_mut() {
                annotate(property, id);
            }
        }
    }
    annotate(&mut description.schema, id);
    description.include_paths(&[], id, build);
    Ok(description)
}

pub fn validate_runner(
    snapshot: &script::source::SourceSnapshot,
    settings: &mut HashMap<String, LaunchSettingValue>,
    id: &str,
    build: &str,
) -> Result<SettingsCheck, String> {
    validate_with_description(
        snapshot,
        settings,
        id,
        build,
        describe_runner(snapshot, id, build)?,
        true,
    )
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
