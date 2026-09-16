//! Narrow, conflict-safe writes to Korri's fixed `device.yaml`.
//!
//! Settings never serialise a `ConfigSnapshot`: that would drop schema content
//! this slice can read but does not execute. Instead we edit the YAML value,
//! validate the complete candidate beside the current `catalog documents`, and only
//! then atomically replace the fixed file. The revision is the hash of the bytes
//! the user actually edited, so a file-manager change between read and save is
//! rejected rather than silently overwritten.

use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde_yaml::{Mapping, Value};

use super::{
    classify_snapshot_support, decode_config_documents,
    snapshot::{DEVICE_FILE_NAME, FILE_NAMES, GAMES_FILE_NAME, RELEASES_FILE_NAME},
};
use crate::plugin_policy;

pub const DEVICE_NAME_SETTING_ID: &str = "device-name";
const STEAMGRIDDB_CREDENTIAL_FILE_NAME: &str = "steamgriddb.credential";

#[typeshare::typeshare]
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum SecretSettingStatus {
    Configured,
    NotConfigured,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SensitiveSettings {
    pub steam_grid_db_credential: SecretSettingStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadableSettings {
    pub revision: String,
    pub device_name: Option<String>,
    pub plugins: Vec<ReadablePluginSetting>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadablePluginSetting {
    pub id: String,
    pub title: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingChange {
    DeviceName(String),
    PluginEnabled { id: String, enabled: bool },
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("settings changed outside Korri; reload and try again")]
    Conflict,
    #[error("invalid setting: {0}")]
    Invalid(String),
    #[error("settings storage: {0}")]
    Storage(String),
    #[error("settings candidate: {0}")]
    Candidate(String),
}

pub fn read(root: &Path) -> Result<ReadableSettings, SettingsError> {
    read_with_registry_source(root, &plugin_policy::RegistrySource::Installed)
}

pub fn read_with_registry_source(
    root: &Path,
    source: &plugin_policy::RegistrySource,
) -> Result<ReadableSettings, SettingsError> {
    ensure_fixed_files(root)?;
    let config = read_fixed(root, DEVICE_FILE_NAME)?;
    let games = read_fixed(root, GAMES_FILE_NAME)?;
    let snapshot = decode_config_documents(&config, &games, &read_fixed(root, RELEASES_FILE_NAME)?)
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    // Existing unsupported content is still reported rather than presenting a
    // settings page that would be unable to save it safely.
    classify_snapshot_support(&snapshot)
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;

    let registry = source
        .registry()
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    let enabled = registry.enabled_plugin_ids();
    let plugins = registry
        .registered_plugin_ids()
        .into_iter()
        .map(|id| ReadablePluginSetting {
            enabled: enabled.contains(&id),
            id: id.into(),
            title: registry.plugin_title(id).unwrap_or(id).into(),
        })
        .collect();

    Ok(ReadableSettings {
        revision: revision(&config),
        device_name: snapshot.host.and_then(|host| host.title),
        plugins,
    })
}

pub fn read_sensitive(private_root: &Path) -> Result<SensitiveSettings, SettingsError> {
    let path = secret_path(private_root);
    if !private_root.exists() {
        return Ok(SensitiveSettings {
            steam_grid_db_credential: SecretSettingStatus::NotConfigured,
        });
    }
    if !private_root.is_dir() {
        return Err(SettingsError::Storage(
            "private state root is unavailable".into(),
        ));
    }
    Ok(SensitiveSettings {
        steam_grid_db_credential: if path.exists() {
            SecretSettingStatus::Configured
        } else {
            SecretSettingStatus::NotConfigured
        },
    })
}

pub fn set_steamgriddb_credential(
    private_root: &Path,
    token: &str,
) -> Result<SecretSettingStatus, SettingsError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(SettingsError::Invalid(
            "SteamGridDB credential cannot be empty".into(),
        ));
    }
    fs::create_dir_all(private_root)
        .map_err(|_| SettingsError::Storage("private state root is unavailable".into()))?;
    if !private_root.is_dir() {
        return Err(SettingsError::Storage(
            "private state root is unavailable".into(),
        ));
    }
    write_secret_atomically(&secret_path(private_root), token.as_bytes())?;
    Ok(SecretSettingStatus::Configured)
}

pub(crate) fn read_steamgriddb_credential(
    private_root: &Path,
) -> Result<Option<String>, SettingsError> {
    let path = secret_path(private_root);
    if !private_root.exists() {
        return Ok(None);
    }
    if !private_root.is_dir() {
        return Err(SettingsError::Storage(
            "private state root is unavailable".into(),
        ));
    }
    match fs::File::open(path) {
        Ok(mut file) => {
            let mut token = String::new();
            file.read_to_string(&mut token)
                .map_err(|_| SettingsError::Storage("private state root is unavailable".into()))?;
            let token = token.trim().to_owned();
            Ok((!token.is_empty()).then_some(token))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(SettingsError::Storage(
            "private state root is unavailable".into(),
        )),
    }
}

pub fn clear_steamgriddb_credential(
    private_root: &Path,
) -> Result<SecretSettingStatus, SettingsError> {
    if !private_root.exists() {
        return Ok(SecretSettingStatus::NotConfigured);
    }
    if !private_root.is_dir() {
        return Err(SettingsError::Storage(
            "private state root is unavailable".into(),
        ));
    }
    match fs::remove_file(secret_path(private_root)) {
        Ok(()) => {
            sync_directory(private_root);
            Ok(SecretSettingStatus::NotConfigured)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(SecretSettingStatus::NotConfigured)
        }
        Err(_) => Err(SettingsError::Storage(
            "private state root is unavailable".into(),
        )),
    }
}

fn secret_path(private_root: &Path) -> PathBuf {
    private_root.join(STEAMGRIDDB_CREDENTIAL_FILE_NAME)
}

pub fn update(
    root: &Path,
    private_root: &Path,
    write_lock: &std::sync::Mutex<()>,
    expected_revision: &str,
    change: SettingChange,
) -> Result<ReadableSettings, SettingsError> {
    update_with_registry_source(
        root,
        private_root,
        write_lock,
        expected_revision,
        change,
        &plugin_policy::RegistrySource::Installed,
    )
}

pub fn update_with_registry_source(
    root: &Path,
    private_root: &Path,
    write_lock: &std::sync::Mutex<()>,
    expected_revision: &str,
    change: SettingChange,
    source: &plugin_policy::RegistrySource,
) -> Result<ReadableSettings, SettingsError> {
    if matches!(change, SettingChange::PluginEnabled { .. }) {
        return Err(SettingsError::Invalid(
            "installed plugin selections require local administrator approval".into(),
        ));
    }
    let _guard = write_lock.lock().expect("settings write lock poisoned");
    crate::discovery::reconcile::reject_pending_publication(private_root).map_err(|error| {
        match error {
            crate::discovery::DiscoveryError::Conflict => SettingsError::Conflict,
            other => SettingsError::Storage(other.to_string()),
        }
    })?;
    let config = read_fixed(root, DEVICE_FILE_NAME)?;
    if revision(&config) != expected_revision {
        return Err(SettingsError::Conflict);
    }
    let games = read_fixed(root, GAMES_FILE_NAME)?;
    let mut document = parse_mapping(&config)?;

    match change {
        SettingChange::DeviceName(value) => set_device_name(&mut document, value)?,
        SettingChange::PluginEnabled { id, enabled } => {
            set_plugin_enabled(&mut document, id, enabled)?
        }
    }

    let candidate = serde_yaml::to_string(&Value::Mapping(document))
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    let snapshot =
        decode_config_documents(&candidate, &games, &read_fixed(root, RELEASES_FILE_NAME)?)
            .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    classify_snapshot_support(&snapshot)
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    source
        .registry()
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;

    write_atomically(
        &root.join(DEVICE_FILE_NAME),
        candidate.as_bytes(),
        expected_revision,
    )?;
    read_with_registry_source(root, source)
}

/// Choices extend the approved system/game records, never a separate document.
#[typeshare::typeshare]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "_tag", content = "id")]
pub enum RunnerChoiceScope {
    System(String),
    Game(String),
}

#[typeshare::typeshare]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RunnerChoiceRevisions {
    pub device: String,
    pub games: String,
}

pub fn runner_choice_revisions(root: &Path) -> Result<RunnerChoiceRevisions, SettingsError> {
    runner_choice_snapshot(root).map(|(_, revisions)| revisions)
}

pub fn runner_choice_snapshot(
    root: &Path,
) -> Result<(super::ConfigSnapshot, RunnerChoiceRevisions), SettingsError> {
    ensure_fixed_files(root)?;
    let device = read_fixed(root, DEVICE_FILE_NAME)?;
    let games = read_fixed(root, GAMES_FILE_NAME)?;
    let snapshot = decode_config_documents(&device, &games, &read_fixed(root, RELEASES_FILE_NAME)?)
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    classify_snapshot_support(&snapshot)
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    Ok((
        snapshot,
        RunnerChoiceRevisions {
            device: revision(&device),
            games: revision(&games),
        },
    ))
}

pub fn set_runner_choice(
    root: &Path,
    private_root: &Path,
    write_lock: &std::sync::Mutex<()>,
    expected_revision: &str,
    scope: &RunnerChoiceScope,
    runner_id: Option<&str>,
) -> Result<RunnerChoiceRevisions, SettingsError> {
    if let Some(id) = runner_id {
        // The same fully-qualified contribution syntax as release provider refs.
        if !id.starts_with('@') || serde_json::from_value::<super::ReleaseKey>(id.into()).is_err() {
            return Err(SettingsError::Invalid(
                "runner must be a fully-qualified contribution ID".into(),
            ));
        }
    }
    let _guard = write_lock
        .lock()
        .expect("runner choice write lock poisoned");
    crate::discovery::reconcile::reject_pending_publication(private_root).map_err(|error| {
        match error {
            crate::discovery::DiscoveryError::Conflict => SettingsError::Conflict,
            other => SettingsError::Storage(other.to_string()),
        }
    })?;
    let mut device = read_fixed(root, DEVICE_FILE_NAME)?;
    let mut games = read_fixed(root, GAMES_FILE_NAME)?;
    let releases = read_fixed(root, RELEASES_FILE_NAME)?;
    let current = decode_config_documents(&device, &games, &releases)
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    let (file, section, id, text) = match scope {
        RunnerChoiceScope::System(id) => {
            // Installed systems need no local record until their first opinion.
            if !current
                .releases
                .values()
                .any(|release| &release.system.0 == id)
                && !current.systems.contains_key(id)
            {
                return Err(SettingsError::Invalid(format!("unknown system {id}")));
            }
            (DEVICE_FILE_NAME, "systems", id, &mut device)
        }
        RunnerChoiceScope::Game(id) => {
            if !current.games.contains_key(id) {
                return Err(SettingsError::Invalid(format!("unknown game {id}")));
            }
            (GAMES_FILE_NAME, "games", id, &mut games)
        }
    };
    if revision(text) != expected_revision {
        return Err(SettingsError::Conflict);
    }
    let mut document = parse_mapping(text)?;
    let record = mapping_at(mapping_at(&mut document, section)?, id)?;
    let key = Value::String("runner".into());
    if let Some(id) = runner_id {
        record.insert(key, Value::String(id.into()));
    } else {
        record.remove(&key);
    }
    *text = serde_yaml::to_string(&Value::Mapping(document))
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    let candidate = text.clone();
    let snapshot = decode_config_documents(&device, &games, &releases)
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    classify_snapshot_support(&snapshot)
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    write_atomically(&root.join(file), candidate.as_bytes(), expected_revision)?;
    // These are this commit's bytes, captured before another writer can enter.
    Ok(RunnerChoiceRevisions {
        device: revision(&device),
        games: revision(&games),
    })
}

fn set_device_name(document: &mut Mapping, value: String) -> Result<(), SettingsError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(SettingsError::Invalid("device name cannot be empty".into()));
    }
    if value.chars().count() > 64 {
        return Err(SettingsError::Invalid(
            "device name must be 64 characters or fewer".into(),
        ));
    }
    let host = mapping_at(document, "host")?;
    host.insert(Value::String("title".into()), Value::String(value.into()));
    Ok(())
}

fn set_plugin_enabled(
    document: &mut Mapping,
    id: String,
    enabled: bool,
) -> Result<(), SettingsError> {
    let known = plugin_policy::installed_registry()
        .map_err(|error| SettingsError::Candidate(error.to_string()))?
        .registered_plugin_ids()
        .iter()
        .any(|plugin_id| plugin_id == &id);
    if !known {
        return Err(SettingsError::Invalid(format!("unknown plugin {id}")));
    }
    let host = mapping_at(document, "host")?;
    let plugin = mapping_at(host, "plugin")?;
    plugin.insert(Value::String(id), Value::Bool(enabled));
    Ok(())
}

fn mapping_at<'a>(parent: &'a mut Mapping, key: &str) -> Result<&'a mut Mapping, SettingsError> {
    let key = Value::String(key.into());
    if !parent.contains_key(&key) {
        parent.insert(key.clone(), Value::Mapping(Mapping::new()));
    }
    parent
        .get_mut(&key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| SettingsError::Invalid(format!("{key:?} must be a record")))
}

fn parse_mapping(config: &str) -> Result<Mapping, SettingsError> {
    let value: Value = serde_yaml::from_str(config)
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    match value {
        Value::Null => Ok(Mapping::new()),
        Value::Mapping(mapping) => Ok(mapping),
        _ => Err(SettingsError::Invalid(
            "device.yaml must contain a record".into(),
        )),
    }
}

fn ensure_fixed_files(root: &Path) -> Result<(), SettingsError> {
    fs::create_dir_all(root.join("catalog"))
        .map_err(|error| SettingsError::Storage(error.to_string()))?;
    for name in FILE_NAMES {
        let path = root.join(name);
        if path.exists() {
            continue;
        }
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut file) => file
                .write_all(b"{}\n")
                .map_err(|error| SettingsError::Storage(error.to_string()))?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(SettingsError::Storage(error.to_string())),
        }
    }
    Ok(())
}

fn read_fixed(root: &Path, name: &str) -> Result<String, SettingsError> {
    fs::read_to_string(root.join(name)).map_err(|error| SettingsError::Storage(error.to_string()))
}

fn revision(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

fn write_secret_atomically(path: &Path, content: &[u8]) -> Result<(), SettingsError> {
    let parent = path
        .parent()
        .ok_or_else(|| SettingsError::Storage("private state file has no parent".into()))?;
    let temporary: PathBuf = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("secret"),
        hex::encode(rand::random::<[u8; 8]>())
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| SettingsError::Storage("private state root is unavailable".into()))?;
        file.write_all(content)
            .map_err(|_| SettingsError::Storage("private state root is unavailable".into()))?;
        file.sync_all()
            .map_err(|_| SettingsError::Storage("private state root is unavailable".into()))?;
        fs::rename(&temporary, path)
            .map_err(|_| SettingsError::Storage("private state root is unavailable".into()))?;
        sync_directory(parent);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sync_directory(path: &Path) {
    if let Ok(directory) = OpenOptions::new().read(true).open(path) {
        let _ = directory.sync_all();
    }
}

fn write_atomically(
    path: &Path,
    content: &[u8],
    expected_revision: &str,
) -> Result<(), SettingsError> {
    let parent = path
        .parent()
        .ok_or_else(|| SettingsError::Storage("config file has no parent".into()))?;
    let temporary: PathBuf = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config"),
        hex::encode(rand::random::<[u8; 8]>())
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| SettingsError::Storage(error.to_string()))?;
        file.write_all(content)
            .map_err(|error| SettingsError::Storage(error.to_string()))?;
        file.sync_all()
            .map_err(|error| SettingsError::Storage(error.to_string()))?;

        // Validation may take long enough for a file manager or sync tool to
        // replace device.yaml. Gate the rename on the bytes that are present
        // immediately before replacement, not only those read at update start.
        let current =
            fs::read_to_string(path).map_err(|error| SettingsError::Storage(error.to_string()))?;
        if revision(&current) != expected_revision {
            return Err(SettingsError::Conflict);
        }

        fs::rename(&temporary, path).map_err(|error| SettingsError::Storage(error.to_string()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Production reads the root-owned installation record. A test root has no
    /// such record, so these tests supply one sample plugin instead.
    fn read(root: &std::path::Path) -> Result<ReadableSettings, SettingsError> {
        read_with_registry_source(
            root,
            &plugin_policy::RegistrySource::Selected(std::sync::Arc::new(
                crate::plugin_test_fixtures::claims_only_registry(),
            )),
        )
    }

    fn root(config: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(DEVICE_FILE_NAME), config).unwrap();
        crate::config::test_fixtures::write(root.path().join(GAMES_FILE_NAME), "{}\n").unwrap();
        crate::config::test_fixtures::write(root.path().join(RELEASES_FILE_NAME), "{}\n").unwrap();
        root
    }

    #[test]
    fn first_read_creates_only_the_three_fixed_empty_documents() {
        let root = tempfile::tempdir().unwrap();

        let settings = read(root.path()).unwrap();

        assert_eq!(settings.device_name, None);
        assert_eq!(
            fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap(),
            b"{}\n"
        );
        assert_eq!(
            fs::read(root.path().join(GAMES_FILE_NAME)).unwrap(),
            b"{}\n"
        );
        assert_eq!(
            fs::read(root.path().join(RELEASES_FILE_NAME)).unwrap(),
            b"{}\n"
        );
    }

    #[test]
    fn changes_the_name_without_dropping_other_sections() {
        let root = root("host:\n  title: old\nproviders: {}\n");
        let before = read(root.path()).unwrap();

        let after = update(
            root.path(),
            tempfile::tempdir().unwrap().path(),
            &std::sync::Mutex::new(()),
            &before.revision,
            SettingChange::DeviceName("  usu  ".into()),
        )
        .unwrap();

        assert_eq!(after.device_name.as_deref(), Some("usu"));
        let saved = fs::read_to_string(root.path().join(DEVICE_FILE_NAME)).unwrap();
        assert!(saved.contains("providers: {}"));
    }
    #[test]
    fn rejects_an_external_edit_instead_of_overwriting_it() {
        let root = root("host:\n  title: first\n");
        let before = read(root.path()).unwrap();
        fs::write(
            root.path().join(DEVICE_FILE_NAME),
            "host:\n  title: outside\n",
        )
        .unwrap();

        let error = update(
            root.path(),
            tempfile::tempdir().unwrap().path(),
            &std::sync::Mutex::new(()),
            &before.revision,
            SettingChange::DeviceName("inside".into()),
        )
        .unwrap_err();

        assert!(matches!(error, SettingsError::Conflict));
        assert!(fs::read_to_string(root.path().join(DEVICE_FILE_NAME))
            .unwrap()
            .contains("outside"));
    }

    #[test]
    fn rechecks_external_edits_at_the_final_rename_gate() {
        let root = root("host:\n  title: first\n");
        let path = root.path().join(DEVICE_FILE_NAME);
        let expected_revision = revision(&fs::read_to_string(&path).unwrap());
        fs::write(&path, "host:\n  title: changed-during-validation\n").unwrap();

        let error = write_atomically(
            &path,
            b"host:\n  title: settings-write\n",
            &expected_revision,
        )
        .unwrap_err();

        assert!(matches!(error, SettingsError::Conflict));
        assert!(fs::read_to_string(path)
            .unwrap()
            .contains("changed-during-validation"));
    }

    #[test]
    fn rejects_unknown_plugins_without_touching_the_file() {
        let root = root("{}\n");
        let before = read(root.path()).unwrap();
        let original = fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap();

        let error = update(
            root.path(),
            tempfile::tempdir().unwrap().path(),
            &std::sync::Mutex::new(()),
            &before.revision,
            SettingChange::PluginEnabled {
                id: "@someone:surprise".into(),
                enabled: false,
            },
        )
        .unwrap_err();

        assert!(matches!(error, SettingsError::Invalid(_)));
        assert_eq!(
            fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap(),
            original
        );
    }
}
