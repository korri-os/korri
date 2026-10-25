use super::*;
use crate::config::{
    self as schema,
    settings::{self, SettingsError},
    snapshot::{DEVICE_FILE_NAME, LIBRARY_FILE_NAME},
};
use serde_yaml::{Mapping, Value};
use std::{path::Path, sync::Mutex};

type Result<T> = std::result::Result<T, SettingsError>;
fn invalid(message: impl Into<String>) -> SettingsError {
    SettingsError::Invalid(message.into())
}
fn key(s: &str) -> Value {
    Value::String(s.into())
}
fn game<'a>(document: &'a mut Mapping, id: &str) -> Result<&'a mut Mapping> {
    settings::mapping_at(document, "library")?
        .get_mut(key(id))
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| invalid(format!("unknown game {id}")))
}
fn profile<'a>(document: &'a mut Mapping, game_id: &str, id: &str) -> Result<&'a mut Mapping> {
    settings::mapping_at(game(document, game_id)?, "profiles")?
        .get_mut(key(id))
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| invalid(format!("unknown profile {id}")))
}
fn edit(
    root: &Path,
    private: &Path,
    lock: &Mutex<()>,
    expected: &str,
    device: bool,
    change: impl FnOnce(&mut Mapping, &schema::ConfigSnapshot) -> Result<()>,
) -> Result<RunnerChoiceRevisions> {
    let _guard = lock.lock().expect("launch settings write lock poisoned");
    crate::discovery::reconcile::reject_pending_publication(private).map_err(|e| match e {
        crate::discovery::DiscoveryError::Conflict => SettingsError::Conflict,
        other => SettingsError::Storage(other.to_string()),
    })?;
    let mut device_text = settings::read_fixed(root, DEVICE_FILE_NAME)?;
    let mut games = settings::read_fixed(root, LIBRARY_FILE_NAME)?;
    let current = schema::decode_config_documents(&device_text, &games)
        .map_err(|e| SettingsError::Candidate(e.to_string()))?;
    let text = if device { &mut device_text } else { &mut games };
    if settings::revision(text) != expected {
        return Err(SettingsError::Conflict);
    }
    let mut document = settings::parse_mapping(text)?;
    change(&mut document, &current)?;
    *text = serde_yaml::to_string(&Value::Mapping(document))
        .map_err(|e| SettingsError::Candidate(e.to_string()))?;
    let candidate = text.clone();
    let snapshot = schema::decode_config_documents(&device_text, &games)
        .map_err(|e| SettingsError::Invalid(e.to_string()))?;
    schema::classify_snapshot_support(&snapshot)
        .map_err(|e| SettingsError::Candidate(e.to_string()))?;
    settings::write_atomically(
        &root.join(if device {
            DEVICE_FILE_NAME
        } else {
            LIBRARY_FILE_NAME
        }),
        candidate.as_bytes(),
        expected,
    )?;
    Ok(RunnerChoiceRevisions {
        device: settings::revision(&device_text),
        games: settings::revision(&games),
    })
}
pub(crate) fn requires_registry(request: &LaunchSetRequest) -> bool {
    matches!(
        request.path,
        LaunchPath::Runner
            | LaunchPath::Setting { .. }
            | LaunchPath::ModifierSetting { .. }
            | LaunchPath::ServiceSetting { .. }
    ) && (request.value.is_some() || request.clear)
}

pub fn set(
    root: &Path,
    private: &Path,
    lock: &Mutex<()>,
    request: &LaunchSetRequest,
    client: &helper::Client,
) -> Result<RunnerChoiceRevisions> {
    let registry = if requires_registry(request) {
        Some(
            crate::plugin_policy::RegistrySource::Installed
                .registry()
                .map_err(|e| invalid(e.to_string()))?,
        )
    } else {
        None
    };
    set_with_registry(root, private, lock, request, client, registry.as_ref())
}
pub fn set_with_registry(
    root: &Path,
    private: &Path,
    lock: &Mutex<()>,
    request: &LaunchSetRequest,
    client: &helper::Client,
    registry: Option<&crate::plugin::PluginRegistry>,
) -> Result<RunnerChoiceRevisions> {
    if request.clear && request.value.is_some() {
        return Err(invalid("clear cannot be combined with a value"));
    }
    if request.clear
        && !matches!(
            request.path,
            LaunchPath::Setting { .. }
                | LaunchPath::ModifierSetting { .. }
                | LaunchPath::ServiceSetting { .. }
        )
    {
        return Err(invalid("only a setting leaf can be cleared"));
    }
    if let LaunchPath::Setting { key }
    | LaunchPath::ModifierSetting { key, .. }
    | LaunchPath::ServiceSetting { key, .. } = &request.path
    {
        if key.split('.').any(|v| v.is_empty()) {
            return Err(invalid("setting path segments must not be empty"));
        }
    }
    let any = matches!(request.place.scope, LaunchRunnerScope::Any);
    let host = matches!(request.place.layer, LaunchLayer::Device) && any;
    if matches!(request.path, LaunchPath::Runner) && !any {
        return Err(invalid("runner choice belongs to an any-runner place"));
    }
    if matches!(request.path, LaunchPath::Setting { .. }) && any {
        return Err(invalid("settings need a runner or family scope"));
    }
    if matches!(request.path, LaunchPath::CpuGovernor) && !host {
        return Err(invalid("CPU governor belongs only to device-any"));
    }
    if matches!(request.place.layer, LaunchLayer::Launch) {
        return Err(invalid("one launch is not a persisted place"));
    }
    if let Some(value) = request
        .value
        .as_ref()
        .or(request.clear.then_some(&serde_json::Value::Null))
    {
        match &request.path {
            LaunchPath::Runner => {
                let id = value
                    .as_str()
                    .ok_or_else(|| invalid("runner must be a string"))?;
                if !id.starts_with('@') || schema::catalog::validate_release_key(id).is_err() {
                    return Err(invalid("runner must be a fully qualified contribution ID"));
                }
                if let Some(runner) = registry.and_then(|r| r.runners().get(id)) {
                    if !super::resolution::choosable(runner) {
                        return Err(invalid("this runner cannot be chosen for a game"));
                    }
                }
            }
            LaunchPath::CpuMin | LaunchPath::CpuMax | LaunchPath::GpuMin | LaunchPath::GpuMax => {
                let n = value
                    .as_u64()
                    .ok_or_else(|| invalid("percent must be an integer"))?;
                let lower = if matches!(request.path, LaunchPath::CpuMax | LaunchPath::GpuMax) {
                    1
                } else {
                    0
                };
                if n < lower || n > 100 {
                    return Err(invalid("percent is outside its allowed range"));
                }
            }
            LaunchPath::CpuGovernor => {
                let name = value
                    .as_str()
                    .filter(|v| !v.trim().is_empty())
                    .ok_or_else(|| invalid("governor must be a nonempty string"))?;
                if let Ok(facts) = client.describe() {
                    if facts
                        .cpu_policies
                        .iter()
                        .any(|v| !v.available_governors.iter().any(|v| v == name))
                    {
                        return Err(invalid("governor is not supported by every CPU policy"));
                    }
                }
            }
            LaunchPath::ModifierEnabled { .. } => {
                if !value.is_boolean() {
                    return Err(invalid("modifier enabled must be boolean"));
                }
            }
            LaunchPath::ModifierSetting { id, key } | LaunchPath::ServiceSetting { id, key } => {
                if !request.clear && !(value.is_boolean() || value.is_number() || value.is_string())
                {
                    return Err(invalid("setting must be a scalar"));
                }
                let registry =
                    registry.ok_or_else(|| invalid("installed plugin registry is unavailable"))?;
                if matches!(request.path, LaunchPath::ModifierSetting { .. }) {
                    registry
                        .enabled_modifier(id)
                        .map_err(|e| invalid(e.to_string()))?;
                } else if id != super::resolution::SUNSHINE {
                    return Err(invalid("unknown launch service"));
                }
                let package = registry
                    .installed_package(id)
                    .map_err(|e| invalid(e.to_string()))?;
                let source = crate::script::source::SourceSnapshot::package_plugin(
                    &package.package,
                    &package.entry,
                    &package.sources,
                )
                .map_err(invalid)?;
                let description = crate::launcher::typed_settings::describe(
                    &source,
                    id,
                    &package.package.display().to_string(),
                )
                .map_err(invalid)?;
                if description.schema_available {
                    let property = schema::cascade::schema_leaf(&description.schema, key)
                        .ok_or_else(|| invalid(format!("unknown setting {key}")))?;
                    if !request.clear {
                        validate_property(property, value).map_err(invalid)?;
                    }
                }
            }
            LaunchPath::Setting { key } => {
                if !request.clear && !(value.is_boolean() || value.is_number() || value.is_string())
                {
                    return Err(invalid("setting must be a scalar"));
                }
                validate_setting(
                    registry.ok_or_else(|| invalid("installed runner registry is unavailable"))?,
                    &request.place.scope,
                    key,
                    value,
                )?;
            }
        }
    }
    let device = matches!(
        request.place.layer,
        LaunchLayer::Device | LaunchLayer::System(_)
    );
    let revisions = edit(
        root,
        private,
        lock,
        &request.expected_revision,
        device,
        |document, current| {
            let record = match &request.place.layer {
                LaunchLayer::Device => settings::mapping_at(document, "host")?,
                LaunchLayer::System(id) => {
                    if !current.systems.contains_key(id)
                        && !current
                            .library
                            .values()
                            .flat_map(|g| &g.releases)
                            .any(|r| r.system.as_ref().is_some_and(|s| &s.0 == id))
                    {
                        return Err(invalid(format!("unknown system {id}")));
                    }
                    settings::mapping_at(settings::mapping_at(document, "systems")?, id)?
                }
                LaunchLayer::Game(id) => game(document, id)?,
                LaunchLayer::Profile {
                    game_id,
                    profile_id,
                } => profile(document, game_id, profile_id)?,
                LaunchLayer::Release {
                    game_id,
                    release_id,
                } => game(document, game_id)?
                    .get_mut(key("releases"))
                    .and_then(Value::as_sequence_mut)
                    .and_then(|v| {
                        v.iter_mut()
                            .find(|r| r.get("id").and_then(Value::as_str) == Some(release_id))
                    })
                    .and_then(Value::as_mapping_mut)
                    .ok_or_else(|| invalid(format!("unknown release {release_id}")))?,
                LaunchLayer::Launch => unreachable!(),
            };
            let record = match &request.place.scope {
                LaunchRunnerScope::Any => record,
                LaunchRunnerScope::Family(id) | LaunchRunnerScope::Runner(id) => {
                    if id.is_empty() {
                        return Err(invalid("runner scope ID must not be empty"));
                    }
                    let section = if matches!(request.place.scope, LaunchRunnerScope::Family(_)) {
                        "families"
                    } else {
                        "runners"
                    };
                    settings::mapping_at(settings::mapping_at(record, section)?, id)?
                }
            };
            let (parent, leaf) = match &request.path {
                LaunchPath::Runner => (record, "runner".to_string()),
                LaunchPath::CpuMin => (settings::mapping_at(record, "cpu")?, "min".into()),
                LaunchPath::CpuMax => (settings::mapping_at(record, "cpu")?, "max".into()),
                LaunchPath::CpuGovernor => {
                    (settings::mapping_at(record, "cpu")?, "governor".into())
                }
                LaunchPath::GpuMin => (settings::mapping_at(record, "gpu")?, "min".into()),
                LaunchPath::GpuMax => (settings::mapping_at(record, "gpu")?, "max".into()),
                LaunchPath::ModifierEnabled { id } => (
                    settings::mapping_at(settings::mapping_at(record, "modifiers")?, id)?,
                    "enabled".into(),
                ),
                LaunchPath::ModifierSetting { id, key }
                | LaunchPath::ServiceSetting { id, key } => (
                    settings::mapping_at(
                        settings::mapping_at(
                            settings::mapping_at(
                                record,
                                if matches!(request.path, LaunchPath::ServiceSetting { .. }) {
                                    "services"
                                } else {
                                    "modifiers"
                                },
                            )?,
                            id,
                        )?,
                        "settings",
                    )?,
                    key.clone(),
                ),
                LaunchPath::Setting { key } => {
                    (settings::mapping_at(record, "settings")?, key.clone())
                }
            };
            let setting = matches!(
                request.path,
                LaunchPath::Setting { .. }
                    | LaunchPath::ModifierSetting { .. }
                    | LaunchPath::ServiceSetting { .. }
            );
            let parts: Vec<_> = if setting {
                leaf.split('.').collect()
            } else {
                vec![leaf.as_str()]
            };
            let value = if request.clear {
                Some(Value::Null)
            } else {
                request
                    .value
                    .as_ref()
                    .map(serde_yaml::to_value)
                    .transpose()
                    .map_err(|e| invalid(e.to_string()))?
            };
            write_leaf(parent, &parts, value)?;
            Ok(())
        },
    )?;
    if host
        && matches!(
            request.path,
            LaunchPath::CpuMin
                | LaunchPath::CpuMax
                | LaunchPath::CpuGovernor
                | LaunchPath::GpuMin
                | LaunchPath::GpuMax
        )
    {
        let _guard = lock.lock().expect("launch limits write lock poisoned");
        if let Err(error) = super::resolution::apply_device(root, client) {
            eprintln!("korrid: launch device limits: {error}");
        }
    }
    Ok(revisions)
}
fn write_leaf(parent: &mut Mapping, path: &[&str], value: Option<Value>) -> Result<()> {
    if path.len() == 1 {
        if let Some(value) = value {
            parent.insert(key(path[0]), value);
        } else {
            parent.remove(key(path[0]));
        }
    } else {
        if value.is_none() && !parent.contains_key(key(path[0])) {
            return Ok(());
        }
        let section = settings::mapping_at(parent, path[0])?;
        write_leaf(section, &path[1..], value)?;
        if section.is_empty() {
            parent.remove(key(path[0]));
        }
    }
    Ok(())
}

fn validate_setting(
    registry: &crate::plugin::PluginRegistry,
    scope: &LaunchRunnerScope,
    key: &str,
    value: &serde_json::Value,
) -> Result<()> {
    let mut errors = Vec::new();
    for runner in registry.runners().values().filter(|v| match scope {
        LaunchRunnerScope::Runner(id) => &v.id == id,
        LaunchRunnerScope::Family(id) => v.family.as_ref() == Some(id),
        LaunchRunnerScope::Any => false,
    }) {
        let accepted = (|| -> std::result::Result<(), String> {
            let package = registry
                .installed_package(&runner.id)
                .map_err(|e| e.to_string())?;
            let source = crate::script::source::SourceSnapshot::package_plugin(
                &package.package,
                &package.entry,
                &package.sources,
            )?;
            let description = crate::launcher::typed_settings::describe(
                &source,
                &runner.id,
                &package.package.display().to_string(),
            )?;
            let property = schema::cascade::schema_leaf(&description.schema, key)
                .ok_or_else(|| format!("unknown setting {key}"))?;
            if value.is_null() {
                Ok(())
            } else {
                validate_property(property, value)
            }
        })();
        match accepted {
            Ok(()) => return Ok(()),
            Err(e) => errors.push(e),
        }
    }
    Err(invalid(if errors.is_empty() {
        "no installed runner matches the scope".into()
    } else {
        errors.join("; ")
    }))
}
pub(crate) fn validate_property(
    property: &serde_json::Value,
    value: &serde_json::Value,
) -> std::result::Result<(), String> {
    use serde_json::Value as J;
    let valid_type = match property.get("type").and_then(J::as_str) {
        Some("boolean") => value.is_boolean(),
        Some("string") => value.is_string(),
        Some("number") => value.is_number(),
        Some("integer") => value.as_i64().is_some() || value.as_u64().is_some(),
        None => value.is_boolean() || value.is_string() || value.is_number(),
        _ => false,
    };
    if !valid_type {
        return Err("setting has the wrong type".into());
    }
    if property
        .get("enum")
        .and_then(J::as_array)
        .is_some_and(|v| !v.contains(value))
    {
        return Err("setting is not an allowed choice".into());
    }
    if let Some(n) = value.as_f64() {
        if property
            .get("minimum")
            .and_then(J::as_f64)
            .is_some_and(|v| n < v)
            || property
                .get("maximum")
                .and_then(J::as_f64)
                .is_some_and(|v| n > v)
        {
            return Err("setting is outside its allowed range".into());
        }
    }
    Ok(())
}
pub fn save_profile(
    root: &Path,
    private: &Path,
    lock: &Mutex<()>,
    request: &LaunchProfileSaveRequest,
) -> Result<RunnerChoiceRevisions> {
    let title = request.title.trim();
    if title.is_empty() {
        return Err(invalid("profile title must not be empty"));
    }
    edit(
        root,
        private,
        lock,
        &request.expected_revision,
        false,
        |document, _| {
            if let Some(id) = &request.profile_id {
                profile(document, &request.game_id, id)?.insert(key("title"), key(title));
            } else {
                let profiles = settings::mapping_at(game(document, &request.game_id)?, "profiles")?;
                let slug = title
                    .to_lowercase()
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("-");
                let base = if slug.is_empty() { "profile" } else { &slug };
                let mut id = base.to_string();
                let mut index = 2;
                while profiles.contains_key(key(&id)) {
                    id = format!("{base}-{index}");
                    index += 1;
                }
                profiles.insert(
                    key(&id),
                    Value::Mapping(Mapping::from_iter([(key("title"), key(title))])),
                );
            }
            Ok(())
        },
    )
}
pub fn remove_profile(
    root: &Path,
    private: &Path,
    lock: &Mutex<()>,
    request: &LaunchProfileRemoveRequest,
) -> Result<RunnerChoiceRevisions> {
    edit(
        root,
        private,
        lock,
        &request.expected_revision,
        false,
        |document, _| {
            let game = game(document, &request.game_id)?;
            let profiles = settings::mapping_at(game, "profiles")?;
            if profiles.remove(key(&request.profile_id)).is_none() {
                return Err(invalid("unknown profile"));
            }
            if game.get(key("profile")).and_then(Value::as_str) == Some(&request.profile_id) {
                game.remove(key("profile"));
            }
            Ok(())
        },
    )
}
pub fn select_profile(
    root: &Path,
    private: &Path,
    lock: &Mutex<()>,
    request: &LaunchProfileSelectRequest,
) -> Result<RunnerChoiceRevisions> {
    edit(
        root,
        private,
        lock,
        &request.expected_revision,
        false,
        |document, _| {
            if let Some(id) = &request.profile_id {
                profile(document, &request.game_id, id)?;
            }
            let game = game(document, &request.game_id)?;
            if let Some(id) = &request.profile_id {
                game.insert(key("profile"), key(id));
            } else {
                game.remove(key("profile"));
            }
            Ok(())
        },
    )
}
