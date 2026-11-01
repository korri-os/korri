//! Surface preferences (native surfaces contract, section 3.6).
//!
//! A package declares each preference with a mark and an initial value; the
//! shell hands korrid that declaration with `app.surface.declare` after the
//! package passed verification. korrid keeps declarations in memory, refuses
//! undeclared keys and anything outside the caps, and stores each value with
//! its mark.
//!
//! Storage follows korrid's existing device-local state: one fixed-name file
//! in the private state root, written the way `onboarding.rs` writes its
//! marker (0700 directory, 0600 file, temporary file, fsync, rename, fsync of
//! the directory). Shared values stay on this device, marked shared, until
//! record sync exists. Values for keys a newer declaration dropped stay on
//! disk, unreadable and unwritable, so a package update that brings a key
//! back finds its value again.

use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Mutex,
};

use korri_surface_treaty::{
    check_preference_declarations, check_preference_value, SurfacePreferenceDeclarations,
    SurfacePreferenceMark, SurfacePreferenceValues,
};
use serde::{Deserialize, Serialize};

use crate::RpcFailure;

/// The file in the private state root, beside `onboarding-finished`.
pub const SURFACE_PREFERENCES_FILE_NAME: &str = "surface-preferences.json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredValue {
    mark: SurfacePreferenceMark,
    value: String,
}

/// surface id -> key -> stored value.
type Stored = BTreeMap<String, BTreeMap<String, StoredValue>>;

pub struct PreferenceStore {
    root: PathBuf,
    declarations: Mutex<HashMap<String, SurfacePreferenceDeclarations>>,
    write: Mutex<()>,
}

fn storage_failure() -> RpcFailure {
    RpcFailure {
        code: "PreferencesStorageUnavailable".into(),
        message: "Could not read or save this surface's preferences. Try again.".into(),
    }
}

fn undeclared_surface() -> RpcFailure {
    RpcFailure {
        code: "SurfaceNotDeclared".into(),
        message: "This surface has not declared its preferences.".into(),
    }
}

fn refused(refusal: korri_surface_treaty::PreferenceRefusal) -> RpcFailure {
    RpcFailure {
        code: "PreferenceRefused".into(),
        message: refusal.to_string(),
    }
}

impl PreferenceStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            declarations: Mutex::new(HashMap::new()),
            write: Mutex::new(()),
        }
    }

    fn path(&self) -> PathBuf {
        self.root.join(SURFACE_PREFERENCES_FILE_NAME)
    }

    fn read(&self) -> Result<Stored, RpcFailure> {
        match fs::read(self.path()) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| storage_failure()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Stored::new()),
            Err(_) => Err(storage_failure()),
        }
    }

    fn write(&self, stored: &Stored) -> Result<(), RpcFailure> {
        let bytes = serde_json::to_vec(stored).map_err(|_| storage_failure())?;
        write_atomically(&self.root, &self.path(), &bytes).map_err(|_| storage_failure())
    }

    /// Replace the declaration of one surface.
    pub fn declare(
        &self,
        surface_id: &str,
        declarations: SurfacePreferenceDeclarations,
    ) -> Result<(), RpcFailure> {
        check_preference_declarations(&declarations).map_err(refused)?;
        self.declarations
            .lock()
            .expect("preference declarations poisoned")
            .insert(surface_id.to_owned(), declarations);
        Ok(())
    }

    fn declared(&self, surface_id: &str) -> Result<SurfacePreferenceDeclarations, RpcFailure> {
        self.declarations
            .lock()
            .expect("preference declarations poisoned")
            .get(surface_id)
            .cloned()
            .ok_or_else(undeclared_surface)
    }

    /// Stored values of declared keys, merged over the declared initial values.
    pub fn values(&self, surface_id: &str) -> Result<SurfacePreferenceValues, RpcFailure> {
        let declarations = self.declared(surface_id)?;
        let stored = self.read()?;
        let mine = stored.get(surface_id);
        Ok(declarations
            .iter()
            .map(|(key, declaration)| {
                let value = mine
                    .and_then(|values| values.get(key))
                    .map(|stored| stored.value.clone())
                    .unwrap_or_else(|| declaration.initial.clone());
                (key.clone(), value)
            })
            .collect())
    }

    /// Store one declared value with its declared mark.
    pub fn remember(&self, surface_id: &str, key: &str, value: &str) -> Result<(), RpcFailure> {
        let declarations = self.declared(surface_id)?;
        let mark = check_preference_value(&declarations, key, value).map_err(refused)?;
        let _guard = self.write.lock().expect("preference writes poisoned");
        let mut stored = self.read()?;
        stored.entry(surface_id.to_owned()).or_default().insert(
            key.to_owned(),
            StoredValue {
                mark,
                value: value.to_owned(),
            },
        );
        self.write(&stored)
    }
}

fn write_atomically(root: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(root)?;
    let temporary = root.join(format!(
        ".{SURFACE_PREFERENCES_FILE_NAME}.{:016x}.tmp",
        rand::random::<u64>()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(root)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use korri_surface_treaty::SurfacePreferenceDeclaration;
    use std::os::unix::fs::PermissionsExt;

    fn declaration(mark: SurfacePreferenceMark, initial: &str) -> SurfacePreferenceDeclaration {
        SurfacePreferenceDeclaration {
            mark,
            initial: initial.into(),
        }
    }

    #[test]
    fn values_need_a_declaration_and_merge_over_initial_values() {
        let directory = tempfile::tempdir().unwrap();
        let store = PreferenceStore::new(directory.path().join("private"));
        assert_eq!(store.values("zone").unwrap_err().code, "SurfaceNotDeclared");
        assert_eq!(
            store
                .remember("zone", "zone.look", "retro")
                .unwrap_err()
                .code,
            "SurfaceNotDeclared"
        );
        store
            .declare(
                "zone",
                [(
                    "zone.look".to_owned(),
                    declaration(SurfacePreferenceMark::Shared, "modern"),
                )]
                .into(),
            )
            .unwrap();
        assert_eq!(store.values("zone").unwrap()["zone.look"], "modern");
        store.remember("zone", "zone.look", "retro").unwrap();
        assert_eq!(store.values("zone").unwrap()["zone.look"], "retro");
        // A fresh store reads the same file: the value survives restart.
        let reopened = PreferenceStore::new(directory.path().join("private"));
        reopened
            .declare(
                "zone",
                [(
                    "zone.look".to_owned(),
                    declaration(SurfacePreferenceMark::Shared, "modern"),
                )]
                .into(),
            )
            .unwrap();
        assert_eq!(reopened.values("zone").unwrap()["zone.look"], "retro");
        let path = directory
            .path()
            .join("private")
            .join(SURFACE_PREFERENCES_FILE_NAME);
        let stored: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            stored,
            serde_json::json!({"zone": {"zone.look": {"mark": "shared", "value": "retro"}}})
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn undeclared_keys_and_oversized_values_are_refused() {
        let directory = tempfile::tempdir().unwrap();
        let store = PreferenceStore::new(directory.path());
        store
            .declare(
                "pico",
                [(
                    "pico.font".to_owned(),
                    declaration(SurfacePreferenceMark::Device, "pico"),
                )]
                .into(),
            )
            .unwrap();
        assert_eq!(
            store.remember("pico", "pico.other", "x").unwrap_err().code,
            "PreferenceRefused"
        );
        assert_eq!(
            store
                .remember("pico", "pico.font", &"x".repeat(257))
                .unwrap_err()
                .code,
            "PreferenceRefused"
        );
        store
            .remember("pico", "pico.font", &"x".repeat(256))
            .unwrap();
        let too_many: SurfacePreferenceDeclarations = (0..17)
            .map(|index| {
                (
                    format!("k{index}"),
                    declaration(SurfacePreferenceMark::Device, ""),
                )
            })
            .collect();
        assert_eq!(
            store.declare("pico", too_many).unwrap_err().code,
            "PreferenceRefused"
        );
    }

    #[test]
    fn dropped_keys_stay_on_disk_and_return_with_the_key() {
        let directory = tempfile::tempdir().unwrap();
        let store = PreferenceStore::new(directory.path());
        let both: SurfacePreferenceDeclarations = [
            (
                "a".to_owned(),
                declaration(SurfacePreferenceMark::Device, "1"),
            ),
            (
                "b".to_owned(),
                declaration(SurfacePreferenceMark::Shared, "2"),
            ),
        ]
        .into();
        store.declare("s", both.clone()).unwrap();
        store.remember("s", "b", "stored").unwrap();
        store
            .declare(
                "s",
                [(
                    "a".to_owned(),
                    declaration(SurfacePreferenceMark::Device, "1"),
                )]
                .into(),
            )
            .unwrap();
        let values = store.values("s").unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(
            store.remember("s", "b", "x").unwrap_err().code,
            "PreferenceRefused"
        );
        store.declare("s", both).unwrap();
        assert_eq!(store.values("s").unwrap()["b"], "stored");
    }
}
