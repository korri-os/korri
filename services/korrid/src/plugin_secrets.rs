//! A plugin's secret: a value korrid uses for the plugin and the plugin's
//! code never sees (ADR 0034, section 2).
//!
//! One secret per plugin, in `<private>/<plugin id>/credential`. korrid
//! creates the folder with mode 0700 and the file with mode 0600 itself, so
//! the modes do not depend on the unit's `UMask`. The value never goes into a
//! log, a failure, readable config or an RPC answer: callers get a
//! `SecretSettingStatus`, and only the code that adds the header reads it.

use crate::config::settings::SecretSettingStatus;
use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

/// The file a plugin's secret is kept in: main's `steamgriddb.credential`
/// without the SteamGridDB part (ADR 0034, "Names").
const CREDENTIAL_FILE_NAME: &str = "credential";
/// The length the settings row allows (`sensitiveText`, 256 characters).
const MAX_SECRET_BYTES: usize = 256;

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum PluginSecretError {
    #[error("plugin ID must be @namespace:name with lowercase names")]
    InvalidPluginId,
    #[error("the secret cannot be empty")]
    Empty,
    #[error("the secret is too long")]
    TooLong,
    #[error("private state root is unavailable")]
    Storage,
}

/// The secrets of the plugins on this device.
#[derive(Clone, Debug)]
pub struct PluginSecrets {
    private_root: PathBuf,
}

impl PluginSecrets {
    pub fn new(private_root: impl AsRef<Path>) -> Self {
        Self {
            private_root: private_root.as_ref().to_owned(),
        }
    }

    /// Whether the plugin's secret is set. Never the value.
    pub fn status(&self, plugin_id: &str) -> Result<SecretSettingStatus, PluginSecretError> {
        Ok(match self.read(plugin_id)? {
            Some(_) => SecretSettingStatus::Configured,
            None => SecretSettingStatus::NotConfigured,
        })
    }

    /// Keep `secret` for the plugin, replacing any earlier one.
    pub fn set(
        &self,
        plugin_id: &str,
        secret: &str,
    ) -> Result<SecretSettingStatus, PluginSecretError> {
        let folder = self.folder(plugin_id)?;
        let secret = secret.trim();
        if secret.is_empty() {
            return Err(PluginSecretError::Empty);
        }
        if secret.len() > MAX_SECRET_BYTES {
            return Err(PluginSecretError::TooLong);
        }
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&folder)
            .map_err(|_| PluginSecretError::Storage)?;
        // A folder that already existed keeps its mode: tighten it.
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))
            .map_err(|_| PluginSecretError::Storage)?;
        write_private_file(&folder.join(CREDENTIAL_FILE_NAME), secret.as_bytes())?;
        Ok(SecretSettingStatus::Configured)
    }

    /// The secret, for the one caller that adds it to a request. `None` when
    /// it is not set.
    pub(crate) fn read(&self, plugin_id: &str) -> Result<Option<String>, PluginSecretError> {
        let path = self.folder(plugin_id)?.join(CREDENTIAL_FILE_NAME);
        let mut file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(PluginSecretError::Storage),
        };
        let mut secret = String::new();
        file.read_to_string(&mut secret)
            .map_err(|_| PluginSecretError::Storage)?;
        let secret = secret.trim();
        Ok((!secret.is_empty()).then(|| secret.to_owned()))
    }

    /// Delete the plugin's secret. Deleting one that is already gone succeeds,
    /// so a step that runs twice is safe.
    pub fn clear(&self, plugin_id: &str) -> Result<SecretSettingStatus, PluginSecretError> {
        let folder = self.folder(plugin_id)?;
        match fs::remove_file(folder.join(CREDENTIAL_FILE_NAME)) {
            Ok(()) => {
                sync_directory(&folder);
                Ok(SecretSettingStatus::NotConfigured)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(SecretSettingStatus::NotConfigured)
            }
            Err(_) => Err(PluginSecretError::Storage),
        }
    }

    /// The plugin's own folder in korrid's private state. The id is checked
    /// here, again, before it becomes a path part.
    fn folder(&self, plugin_id: &str) -> Result<PathBuf, PluginSecretError> {
        if !valid_plugin_id(plugin_id) {
            return Err(PluginSecretError::InvalidPluginId);
        }
        Ok(self.private_root.join(plugin_id))
    }
}

/// The plugin host's rule for an id (`plugin-host/src/declaration.rs`,
/// `validate_id`): `@namespace:name`, each of lowercase letters, digits,
/// `-`, `_` and `.`, starting with a letter or a digit. Such an id is always
/// one safe path part.
fn valid_plugin_id(plugin_id: &str) -> bool {
    fn valid_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 64
            && (name.as_bytes()[0].is_ascii_lowercase() || name.as_bytes()[0].is_ascii_digit())
            && name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-_.".contains(&c))
    }
    plugin_id.split_once(':').is_some_and(|(namespace, name)| {
        namespace.strip_prefix('@').is_some_and(valid_name) && valid_name(name)
    })
}

/// Replace `path` with a new mode-0600 file holding `content`: a temporary
/// file, `fsync`, rename, folder `fsync`.
fn write_private_file(path: &Path, content: &[u8]) -> Result<(), PluginSecretError> {
    let parent = path.parent().ok_or(PluginSecretError::Storage)?;
    let temporary = parent.join(format!(
        ".{CREDENTIAL_FILE_NAME}.{}.tmp",
        hex::encode(rand::random::<[u8; 8]>())
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| PluginSecretError::Storage)?;
        // `mode` is masked by the process umask only downwards; set it
        // exactly in case the umask is unusual.
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| PluginSecretError::Storage)?;
        file.write_all(content)
            .map_err(|_| PluginSecretError::Storage)?;
        file.sync_all().map_err(|_| PluginSecretError::Storage)?;
        fs::rename(&temporary, path).map_err(|_| PluginSecretError::Storage)?;
        sync_directory(parent);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sync_directory(path: &Path) {
    if let Ok(directory) = fs::File::open(path) {
        let _ = directory.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLUGIN: &str = "@korri:steamgriddb";

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[test]
    fn a_set_secret_is_private_on_disk_whatever_the_umask() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        assert_eq!(
            secrets.status(PLUGIN),
            Ok(SecretSettingStatus::NotConfigured)
        );

        // A folder left wider by an earlier run is tightened too.
        let folder = private.path().join(PLUGIN);
        fs::create_dir(&folder).unwrap();
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(
            secrets.set(PLUGIN, "  key-one\n"),
            Ok(SecretSettingStatus::Configured)
        );
        assert_eq!(mode(&folder), 0o700);
        assert_eq!(mode(&folder.join("credential")), 0o600);
        assert_eq!(secrets.read(PLUGIN), Ok(Some("key-one".into())));
        assert_eq!(secrets.status(PLUGIN), Ok(SecretSettingStatus::Configured));

        // A new value replaces the old one, and no temporary file stays.
        secrets.set(PLUGIN, "key-two").unwrap();
        assert_eq!(secrets.read(PLUGIN), Ok(Some("key-two".into())));
        assert_eq!(mode(&folder.join("credential")), 0o600);
        let left: Vec<_> = fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from("credential")]);
    }

    #[test]
    fn each_plugin_has_its_own_secret() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        secrets.set(PLUGIN, "one").unwrap();
        secrets.set("@other:art", "two").unwrap();
        assert_eq!(secrets.read(PLUGIN), Ok(Some("one".into())));
        assert_eq!(secrets.read("@other:art"), Ok(Some("two".into())));
        secrets.clear(PLUGIN).unwrap();
        assert_eq!(secrets.read(PLUGIN), Ok(None));
        assert_eq!(secrets.read("@other:art"), Ok(Some("two".into())));
    }

    #[test]
    fn clear_deletes_the_secret_and_can_run_twice() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        // Nothing was ever set: no folder, and still a success.
        assert_eq!(
            secrets.clear(PLUGIN),
            Ok(SecretSettingStatus::NotConfigured)
        );
        secrets.set(PLUGIN, "key").unwrap();
        for _ in 0..2 {
            assert_eq!(
                secrets.clear(PLUGIN),
                Ok(SecretSettingStatus::NotConfigured)
            );
        }
        assert_eq!(secrets.read(PLUGIN), Ok(None));
        assert!(!private.path().join(PLUGIN).join("credential").exists());
    }

    #[test]
    fn an_empty_or_overlong_secret_is_refused_and_changes_nothing() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        secrets.set(PLUGIN, "kept").unwrap();
        assert_eq!(secrets.set(PLUGIN, "  \n"), Err(PluginSecretError::Empty));
        assert_eq!(
            secrets.set(PLUGIN, &"k".repeat(257)),
            Err(PluginSecretError::TooLong)
        );
        assert_eq!(
            secrets.set(PLUGIN, &"k".repeat(256)),
            Ok(SecretSettingStatus::Configured)
        );
        secrets.set(PLUGIN, "kept").unwrap();
        assert_eq!(secrets.read(PLUGIN), Ok(Some("kept".into())));
    }

    #[test]
    fn an_id_that_is_not_one_plugin_id_never_becomes_a_path() {
        let outside = tempfile::tempdir().unwrap();
        let private = outside.path().join("private");
        fs::create_dir(&private).unwrap();
        let secrets = PluginSecrets::new(&private);
        for id in [
            "",
            "steamgriddb",
            "@korri",
            "@korri:",
            "@:steamgriddb",
            "@korri:../identity",
            "@korri:a/b",
            "../@korri:steamgriddb",
            "@korri:.hidden",
            "@Korri:steamgriddb",
            "@korri:steam griddb",
            "identity",
        ] {
            assert_eq!(
                secrets.set(id, "key"),
                Err(PluginSecretError::InvalidPluginId),
                "{id}"
            );
            assert_eq!(
                secrets.read(id),
                Err(PluginSecretError::InvalidPluginId),
                "{id}"
            );
            assert_eq!(
                secrets.clear(id),
                Err(PluginSecretError::InvalidPluginId),
                "{id}"
            );
        }
        // Nothing was written anywhere.
        assert_eq!(fs::read_dir(&private).unwrap().count(), 0);
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
    }

    #[test]
    fn no_error_text_carries_the_secret() {
        let private = tempfile::tempdir().unwrap();
        // The private root is a file: every write fails.
        let blocked = private.path().join("blocked");
        fs::write(&blocked, b"").unwrap();
        let secrets = PluginSecrets::new(&blocked);
        let failure = secrets.set(PLUGIN, "very-secret-value").unwrap_err();
        assert_eq!(failure, PluginSecretError::Storage);
        assert!(!failure.to_string().contains("very-secret-value"));
        assert!(!format!("{failure:?}").contains("very-secret-value"));
    }
}
