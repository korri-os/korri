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
pub fn set(
    root: &Path,
    private: &Path,
    lock: &Mutex<()>,
    request: &LaunchSettingsSetRequest,
    client: &helper::Client,
) -> Result<RunnerChoiceRevisions> {
    let governor = matches!(request.setting, LaunchSetting::CpuGovernor);
    if governor && !matches!(request.scope, LaunchSettingsScope::Device) {
        return Err(invalid("CPU governor belongs only to device"));
    }
    let value = match &request.value {
        None => None,
        Some(LaunchLimitValue::Percent(n)) if !governor => {
            let lower = if matches!(
                request.setting,
                LaunchSetting::CpuMax | LaunchSetting::GpuMax
            ) {
                1
            } else {
                0
            };
            if !(lower..=100).contains(n) {
                return Err(invalid(format!("percent must be {lower}–100")));
            }
            Some(Value::from(*n))
        }
        Some(LaunchLimitValue::Governor(name)) if governor && !name.trim().is_empty() => {
            if let Ok(facts) = client.describe() {
                if facts
                    .cpu_policies
                    .iter()
                    .any(|p| !p.available_governors.contains(name))
                {
                    return Err(invalid("governor is not supported by every CPU policy"));
                }
            }
            Some(Value::String(name.clone()))
        }
        _ => {
            return Err(invalid(
                "expected an integer percent or a device CPU governor",
            ))
        }
    };
    let device = matches!(
        request.scope,
        LaunchSettingsScope::Device
            | LaunchSettingsScope::System(_)
            | LaunchSettingsScope::Runner(_)
    );
    let revisions = edit(
        root,
        private,
        lock,
        &request.expected_revision,
        device,
        |document, current| {
            let record = match &request.scope {
                LaunchSettingsScope::Device => settings::mapping_at(document, "host")?,
                LaunchSettingsScope::System(id) => {
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
                LaunchSettingsScope::Runner(id) => {
                    if !id.starts_with('@') || schema::catalog::validate_release_key(id).is_err() {
                        return Err(invalid("runner must be a fully qualified contribution ID"));
                    }
                    settings::mapping_at(settings::mapping_at(document, "runners")?, id)?
                }
                LaunchSettingsScope::Game(id) => game(document, id)?,
                LaunchSettingsScope::Profile {
                    game_id,
                    profile_id,
                } => profile(document, game_id, profile_id)?,
                LaunchSettingsScope::Release {
                    game_id,
                    release_id,
                } => game(document, game_id)?
                    .get_mut(key("releases"))
                    .and_then(Value::as_sequence_mut)
                    .and_then(|releases| {
                        releases
                            .iter_mut()
                            .find(|r| r.get("id").and_then(Value::as_str) == Some(release_id))
                    })
                    .and_then(Value::as_mapping_mut)
                    .ok_or_else(|| invalid(format!("unknown release {release_id}")))?,
            };
            let (kind, leaf) = match request.setting {
                LaunchSetting::CpuMin => ("cpu", "min"),
                LaunchSetting::CpuMax => ("cpu", "max"),
                LaunchSetting::GpuMin => ("gpu", "min"),
                LaunchSetting::GpuMax => ("gpu", "max"),
                LaunchSetting::CpuGovernor => ("cpu", "governor"),
            };
            let limits = settings::mapping_at(record, kind)?;
            if let Some(value) = value {
                limits.insert(key(leaf), value);
            } else {
                limits.remove(key(leaf));
            }
            Ok(())
        },
    )?;
    if matches!(request.scope, LaunchSettingsScope::Device) {
        let _guard = lock
            .lock()
            .expect("launch device limits write lock poisoned");
        // Do not roll back a valid persisted edit if the optional helper is down.
        if let Err(error) = super::resolution::apply_device(root, client) {
            eprintln!("korrid: launch device limits: {error}");
        }
    }
    Ok(revisions)
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
