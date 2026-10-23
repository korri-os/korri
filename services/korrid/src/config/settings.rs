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
    num::NonZeroU8,
    path::{Path, PathBuf},
};

use serde_yaml::{Mapping, Value};

use super::{
    classify_snapshot_support, decode_config_documents,
    snapshot::{DEVICE_FILE_NAME, FILE_NAMES, LIBRARY_FILE_NAME},
};
use crate::plugin_policy;

pub const DEVICE_NAME_SETTING_ID: &str = "device-name";
pub const PLAYER_COUNT_SETTING_ID: &str = "host.preferences.playerCount";
pub(crate) const ONBOARDING_FINISHED_FILE_NAME: &str = "onboarding-finished";
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
    pub player_count: u8,
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
    PlayerCount(NonZeroU8),
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

pub fn parse_change(setting_id: &str, value: String) -> Result<SettingChange, SettingsError> {
    match setting_id {
        DEVICE_NAME_SETTING_ID => Ok(SettingChange::DeviceName(value)),
        PLAYER_COUNT_SETTING_ID => value
            .parse::<NonZeroU8>()
            .map(SettingChange::PlayerCount)
            .map_err(|_| {
                SettingsError::Invalid("player count must be an integer from 1 to 255".into())
            }),
        _ => value
            .parse::<bool>()
            .map(|enabled| SettingChange::PluginEnabled {
                id: setting_id.into(),
                enabled,
            })
            .map_err(|_| SettingsError::Invalid("plugin value must be true or false".into())),
    }
}

pub fn read(root: &Path) -> Result<ReadableSettings, SettingsError> {
    read_with_registry_source(root, &plugin_policy::RegistrySource::Installed)
}

/// The seat count from `device.yaml` alone. Startup sizes the shared seat pool
/// with it, so it must not depend on plugin state: a slow or failed plugin host
/// would otherwise fence every controller.
pub fn read_player_count(root: &Path) -> Result<u8, SettingsError> {
    read_device_documents(root).map(|(_, snapshot)| snapshot.player_count())
}

fn read_device_documents(root: &Path) -> Result<(String, super::ConfigSnapshot), SettingsError> {
    ensure_fixed_files(root)?;
    let config = read_fixed(root, DEVICE_FILE_NAME)?;
    let games = read_fixed(root, LIBRARY_FILE_NAME)?;
    let snapshot = decode_config_documents(&config, &games)
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    // Existing unsupported content is still reported rather than presenting a
    // settings page that would be unable to save it safely.
    classify_snapshot_support(&snapshot)
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    Ok((config, snapshot))
}

pub fn read_with_registry_source(
    root: &Path,
    source: &plugin_policy::RegistrySource,
) -> Result<ReadableSettings, SettingsError> {
    let (config, snapshot) = read_device_documents(root)?;

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
        player_count: snapshot.player_count(),
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
    let games = read_fixed(root, LIBRARY_FILE_NAME)?;
    let mut document = parse_mapping(&config)?;

    match change {
        SettingChange::DeviceName(value) => set_device_name(&mut document, value)?,
        SettingChange::PlayerCount(value) => {
            let preferences = mapping_at(mapping_at(&mut document, "host")?, "preferences")?;
            preferences.insert(
                Value::String("player-count".into()),
                Value::from(value.get()),
            );
        }
        SettingChange::PluginEnabled { id, enabled } => {
            set_plugin_enabled(&mut document, id, enabled, source)?
        }
    }

    let candidate = serde_yaml::to_string(&Value::Mapping(document))
        .map_err(|error| SettingsError::Candidate(error.to_string()))?;
    let snapshot = decode_config_documents(&candidate, &games)
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
    let games = read_fixed(root, LIBRARY_FILE_NAME)?;
    let snapshot = decode_config_documents(&device, &games)
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
    registry_source: &plugin_policy::RegistrySource,
) -> Result<(), SettingsError> {
    let known = registry_source
        .registry()
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

pub(crate) fn mapping_at<'a>(
    parent: &'a mut Mapping,
    key: &str,
) -> Result<&'a mut Mapping, SettingsError> {
    let key = Value::String(key.into());
    if !parent.contains_key(&key) {
        parent.insert(key.clone(), Value::Mapping(Mapping::new()));
    }
    parent
        .get_mut(&key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| SettingsError::Invalid(format!("{key:?} must be a record")))
}

pub(crate) fn parse_mapping(config: &str) -> Result<Mapping, SettingsError> {
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

pub(crate) fn read_fixed(root: &Path, name: &str) -> Result<String, SettingsError> {
    fs::read_to_string(root.join(name)).map_err(|error| SettingsError::Storage(error.to_string()))
}

pub(crate) fn revision(content: &str) -> String {
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

pub(crate) fn write_atomically(
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

    fn update(
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
            &plugin_policy::RegistrySource::Selected(std::sync::Arc::new(
                crate::plugin_test_fixtures::claims_only_registry(),
            )),
        )
    }

    fn root(config: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(DEVICE_FILE_NAME), config).unwrap();
        crate::config::test_fixtures::write(root.path().join(LIBRARY_FILE_NAME), "{}\n").unwrap();
        root
    }

    #[test]
    fn first_read_creates_only_the_two_fixed_empty_documents() {
        let root = tempfile::tempdir().unwrap();

        let settings = read(root.path()).unwrap();

        assert_eq!(settings.device_name, None);
        assert_eq!(settings.player_count, super::super::DEFAULT_PLAYER_COUNT);
        assert_eq!(
            fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap(),
            b"{}\n"
        );
        assert_eq!(
            fs::read(root.path().join(LIBRARY_FILE_NAME)).unwrap(),
            b"{}\n"
        );
    }

    // Startup sizes the seat pool from device.yaml alone. A missing or failed
    // plugin registry must not make the count unreadable, because an unreadable
    // count fences all controller input.
    #[test]
    fn player_count_is_read_without_the_plugin_registry() {
        let root = root("host: {preferences: {player-count: 6}}\n");
        assert!(
            plugin_policy::RegistrySource::Installed.registry().is_err(),
            "this test needs the installed plugin registry to be absent"
        );

        assert_eq!(read_player_count(root.path()).unwrap(), 6);
    }

    #[test]
    fn player_count_defaults_when_device_yaml_is_empty() {
        let root = root("{}\n");
        assert_eq!(
            read_player_count(root.path()).unwrap(),
            super::super::DEFAULT_PLAYER_COUNT
        );
    }

    #[test]
    fn removing_only_the_count_leaf_restores_the_default_without_rewriting_the_file() {
        let root = root("host: {title: handheld, preferences: {player-count: 6}}\n");
        let before = read(root.path()).unwrap();
        assert_eq!(before.player_count, 6);
        let reset = "host: {title: handheld, preferences: {}}\n";
        fs::write(root.path().join(DEVICE_FILE_NAME), reset).unwrap();
        let after = read(root.path()).unwrap();
        assert_eq!(after.player_count, 4);
        assert_eq!(after.device_name, before.device_name);
        assert_ne!(after.revision, before.revision);
        assert_eq!(
            fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap(),
            reset.as_bytes()
        );
    }

    #[test]
    fn parses_count_without_changing_name_or_plugin_parsing() {
        assert_eq!(PLAYER_COUNT_SETTING_ID, "host.preferences.playerCount");
        for count in [1, 6, 255] {
            assert_eq!(
                parse_change(PLAYER_COUNT_SETTING_ID, count.to_string()).unwrap(),
                SettingChange::PlayerCount(NonZeroU8::new(count).unwrap())
            );
        }
        for value in [
            "0", "-1", "256", "1.5", "null", "\"6\"", "true", "false", "", "six",
        ] {
            assert!(
                matches!(
                    parse_change(PLAYER_COUNT_SETTING_ID, value.into()),
                    Err(SettingsError::Invalid(_))
                ),
                "{value}"
            );
        }
        assert_eq!(
            parse_change(DEVICE_NAME_SETTING_ID, "  handheld  ".into()).unwrap(),
            SettingChange::DeviceName("  handheld  ".into())
        );
        for enabled in [true, false] {
            assert_eq!(
                parse_change("@korri:sample", enabled.to_string()).unwrap(),
                SettingChange::PluginEnabled {
                    id: "@korri:sample".into(),
                    enabled
                }
            );
        }
        assert!(matches!(
            parse_change("@korri:sample", "1".into()),
            Err(SettingsError::Invalid(message)) if message == "plugin value must be true or false"
        ));
    }

    #[test]
    fn count_updates_only_the_approved_leaf_and_preserves_catalog_bytes() {
        let root = root("{}");
        crate::config::test_fixtures::combined(root.path());
        let device_path = root.path().join(DEVICE_FILE_NAME);
        let original = fs::read_to_string(&device_path).unwrap();
        let mut expected: Value = serde_yaml::from_str(&original).unwrap();
        let catalog_before: Vec<_> = [LIBRARY_FILE_NAME]
            .map(|name| fs::read(root.path().join(name)).unwrap())
            .into();
        let private = tempfile::tempdir().unwrap();
        let lock = std::sync::Mutex::new(());
        let mut settings = read(root.path()).unwrap();
        for count in [1, 6, 255] {
            let before = settings;
            settings = update(
                root.path(),
                private.path(),
                &lock,
                &before.revision,
                SettingChange::PlayerCount(NonZeroU8::new(count).unwrap()),
            )
            .unwrap();
            assert_eq!(settings.player_count, count);
            assert_eq!(settings.device_name, before.device_name);
            assert_eq!(settings.plugins, before.plugins);
            assert_ne!(settings.revision, before.revision);
            let saved = fs::read_to_string(&device_path).unwrap();
            let actual: Value = serde_yaml::from_str(&saved).unwrap();
            let preferences = mapping_at(
                mapping_at(expected.as_mapping_mut().unwrap(), "host").unwrap(),
                "preferences",
            )
            .unwrap();
            preferences.insert(Value::String("player-count".into()), Value::from(count));
            assert_eq!(actual, expected);
            for (name, bytes) in [LIBRARY_FILE_NAME].iter().zip(&catalog_before) {
                assert_eq!(&fs::read(root.path().join(name)).unwrap(), bytes);
            }
            assert_eq!(read(root.path()).unwrap(), settings);
        }
        // Other settings also preserve the count once written.
        let after = update(
            root.path(),
            private.path(),
            &lock,
            &settings.revision,
            SettingChange::DeviceName("renamed".into()),
        )
        .unwrap();
        assert_eq!(after.player_count, 255);
    }

    #[test]
    fn count_update_creates_only_missing_host_and_preferences_maps() {
        for device in ["{}", "host: {}", "host: {title: handheld}"] {
            let root = root(device);
            let before = read(root.path()).unwrap();
            let after = update(
                root.path(),
                tempfile::tempdir().unwrap().path(),
                &std::sync::Mutex::new(()),
                &before.revision,
                SettingChange::PlayerCount(NonZeroU8::new(6).unwrap()),
            )
            .unwrap();
            assert_eq!(after.player_count, 6);
            assert_eq!(after.device_name, before.device_name);
            let saved: Value = serde_yaml::from_str(
                &fs::read_to_string(root.path().join(DEVICE_FILE_NAME)).unwrap(),
            )
            .unwrap();
            assert_eq!(saved["host"]["preferences"]["player-count"], Value::from(6));
        }
    }

    #[test]
    fn count_update_rejects_stale_revision_without_mutation() {
        let root = root("host: {preferences: {player-count: 1}}\n");
        let private = tempfile::tempdir().unwrap();
        let lock = std::sync::Mutex::new(());
        let before = read(root.path()).unwrap();
        let after = update(
            root.path(),
            private.path(),
            &lock,
            &before.revision,
            SettingChange::PlayerCount(NonZeroU8::new(6).unwrap()),
        )
        .unwrap();
        let original = fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap();
        assert!(matches!(
            update(
                root.path(),
                private.path(),
                &lock,
                &before.revision,
                SettingChange::PlayerCount(NonZeroU8::new(255).unwrap())
            ),
            Err(SettingsError::Conflict)
        ));
        assert_eq!(
            fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap(),
            original
        );
        assert_eq!(read(root.path()).unwrap(), after);

        fs::write(
            root.path().join(DEVICE_FILE_NAME),
            b"host: {title: external}\n",
        )
        .unwrap();
        assert!(matches!(
            update(
                root.path(),
                private.path(),
                &lock,
                &after.revision,
                SettingChange::PlayerCount(NonZeroU8::new(1).unwrap())
            ),
            Err(SettingsError::Conflict)
        ));
        assert_eq!(
            fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap(),
            b"host: {title: external}\n"
        );
    }

    #[test]
    fn count_update_validates_all_candidate_documents_before_writing() {
        for (file, content) in [
            (
                DEVICE_FILE_NAME,
                "host: {preferences: {player-count: 6, launch: {audio: {volume: 75}}}}",
            ),
            (
                DEVICE_FILE_NAME,
                "host: {preferences: {player-count: 6, typo: true}}",
            ),
            (DEVICE_FILE_NAME, "host: {env: {EXAMPLE: []}}"),
            (
                LIBRARY_FILE_NAME,
                "games: {bad: {title: broken, releases: []}}",
            ),
        ] {
            let root = root("host: {preferences: {player-count: 1}}");
            fs::write(root.path().join(file), content).unwrap();
            let original = fs::read_to_string(root.path().join(DEVICE_FILE_NAME)).unwrap();
            let error = update(
                root.path(),
                tempfile::tempdir().unwrap().path(),
                &std::sync::Mutex::new(()),
                &revision(&original),
                SettingChange::PlayerCount(NonZeroU8::new(255).unwrap()),
            )
            .unwrap_err();
            assert!(
                matches!(error, SettingsError::Candidate(_)),
                "{file}: {error}"
            );
            assert_eq!(
                fs::read_to_string(root.path().join(DEVICE_FILE_NAME)).unwrap(),
                original
            );
            assert_eq!(fs::read_to_string(root.path().join(file)).unwrap(), content);
        }
    }

    #[cfg(unix)]
    #[test]
    fn count_write_failure_preserves_file_and_readable_state() {
        use std::os::unix::fs::PermissionsExt;
        // Root bypasses directory permissions. The normal unprivileged check
        // exercises a real failed temporary-file write, not a simulated error.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let root = root("host: {preferences: {player-count: 6}}\n");
        let before = read(root.path()).unwrap();
        let original = fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let result = update(
            root.path(),
            tempfile::tempdir().unwrap().path(),
            &std::sync::Mutex::new(()),
            &before.revision,
            SettingChange::PlayerCount(NonZeroU8::new(255).unwrap()),
        );
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            matches!(result, Err(SettingsError::Storage(_))),
            "{result:?}"
        );
        assert_eq!(
            fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap(),
            original
        );
        assert_eq!(read(root.path()).unwrap(), before);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
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
    #[test]
    fn settings_preserves_boolean_plugin_states_and_keeps_administrator_approval() {
        for enabled in [false, true] {
            let root = root(&format!("host: {{plugin: {{'@korri:core': {enabled}}}}}\n"));
            let private = tempfile::tempdir().unwrap();
            let lock = std::sync::Mutex::new(());
            let before = read(root.path()).unwrap();
            let snapshot = decode_config_documents(
                &fs::read_to_string(root.path().join(DEVICE_FILE_NAME)).unwrap(),
                "{}",
            )
            .unwrap();
            classify_snapshot_support(&snapshot).unwrap();
            assert_eq!(
                snapshot.host.as_ref().unwrap().plugin.as_ref().unwrap()
                    [&super::super::ProviderIdString("@korri:core".into())],
                serde_json::json!(enabled)
            );
            assert_eq!(before.plugins[0].id, "@korri:core");
            let after = update(
                root.path(),
                private.path(),
                &lock,
                &before.revision,
                SettingChange::DeviceName("Preserved".into()),
            )
            .unwrap();
            let saved = fs::read_to_string(root.path().join(DEVICE_FILE_NAME)).unwrap();
            let snapshot = decode_config_documents(&saved, "{}").unwrap();
            assert_eq!(
                snapshot.host.as_ref().unwrap().plugin.as_ref().unwrap()
                    [&super::super::ProviderIdString("@korri:core".into())],
                serde_json::json!(enabled)
            );
            let error = update(
                root.path(),
                private.path(),
                &lock,
                &after.revision,
                SettingChange::PluginEnabled {
                    id: "@korri:core".into(),
                    enabled: !enabled,
                },
            )
            .unwrap_err();
            assert!(error
                .to_string()
                .contains("installed plugin selections require local administrator approval"));
            assert_eq!(
                fs::read_to_string(root.path().join(DEVICE_FILE_NAME)).unwrap(),
                saved
            );
        }
        let snapshot =
            decode_config_documents("host: {plugin: {'@korri:core': {policy: true}}}", "{}")
                .unwrap();
        assert!(classify_snapshot_support(&snapshot).is_err());
    }
}
