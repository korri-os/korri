//! The active host session record.
//!
//! The record is a two-phase write-ahead journal. `running` names the live
//! unit. After systemd proves completion, korrid atomically replaces it with
//! `completionPending`, which contains the exact legacy-shaped play entry.
//! The play log append is idempotent for that exact entry. Only then does
//! korrid remove the active record. A crash at any boundary therefore keeps
//! either a recoverable running record or a replayable completion record.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

use serde::{Deserialize, Serialize};

use super::{play_log::PlayEntry, systemd_unit::validate_launch_id};

pub(super) const ACTIVE_FILE: &str = "active.json";
pub(super) const TEMP_ACTIVE_PREFIX: &str = ".active.json-";
const MAX_ACTIVE_BYTES: usize = 4 * 1024;
const MAX_GAME_ID_BYTES: usize = 1_024;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum ActiveSession {
    Running {
        #[serde(rename = "launchId")]
        launch_id: String,
        #[serde(rename = "gameId")]
        game_id: String,
        #[serde(
            rename = "personPublicKey",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        person_public_key: Option<String>,
        #[serde(rename = "startedAt")]
        started_at: u64,
    },
    CompletionPending {
        #[serde(rename = "launchId")]
        launch_id: String,
        #[serde(rename = "gameId")]
        game_id: String,
        #[serde(rename = "personPublicKey")]
        person_public_key: String,
        #[serde(rename = "startedAt")]
        started_at: u64,
        entry: PlayEntry,
    },
}

impl ActiveSession {
    pub(super) fn running(
        launch_id: String,
        game_id: String,
        person_public_key: Option<String>,
        started_at: u64,
    ) -> Self {
        Self::Running {
            launch_id,
            game_id,
            person_public_key,
            started_at,
        }
    }

    pub(super) fn launch_id(&self) -> &str {
        match self {
            Self::Running { launch_id, .. } | Self::CompletionPending { launch_id, .. } => {
                launch_id
            }
        }
    }

    pub(super) fn game_id(&self) -> &str {
        match self {
            Self::Running { game_id, .. } | Self::CompletionPending { game_id, .. } => game_id,
        }
    }

    fn validate(&self) -> Result<(), String> {
        validate_launch_id(self.launch_id()).map_err(|error| error.message)?;
        if self.game_id().is_empty() || self.game_id().len() > MAX_GAME_ID_BYTES {
            return Err("host recovery game identity is invalid".into());
        }
        let person = match self {
            Self::Running {
                person_public_key, ..
            } => person_public_key.as_deref(),
            Self::CompletionPending {
                person_public_key,
                entry,
                ..
            } => {
                if super::play_log::validate_entry(entry).is_err() {
                    return Err("host recovery completion entry is invalid".into());
                }
                Some(person_public_key.as_str())
            }
        };
        if let Some(person) = person {
            if person.len() != 64
                || !person
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err("host recovery person identity is invalid".into());
            }
        }
        Ok(())
    }
}

pub(super) fn read_active(root: &Path) -> Result<Option<ActiveSession>, String> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err("host recovery state directory is not private".into());
    }
    let entries: Vec<_> = fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|error| error.to_string())?;
    if entries.is_empty() {
        return Ok(None);
    }
    if entries.len() != 1 || entries[0].file_name() != ACTIVE_FILE {
        return Err("host recovery state is ambiguous".into());
    }
    let path = entries[0].path();
    let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > MAX_ACTIVE_BYTES as u64
    {
        return Err("host recovery record is not a private regular file".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(&path)
        .map_err(|error| error.to_string())?
        .take((MAX_ACTIVE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_ACTIVE_BYTES {
        return Err("host recovery record is oversized".into());
    }
    let record: ActiveSession = serde_json::from_slice(&bytes)
        .map_err(|_| String::from("host recovery record is malformed"))?;
    record.validate()?;
    Ok(Some(record))
}

pub(super) fn persist_active(root: &Path, record: &ActiveSession) -> Result<(), String> {
    ensure_private_root(root)?;
    if fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .next()
        .is_some()
    {
        return Err("host recovery state is not empty".into());
    }
    write_active(root, record)
}

pub(super) fn replace_active(root: &Path, record: &ActiveSession) -> Result<(), String> {
    let current = read_active(root)?;
    if current.is_none() {
        return Err("host recovery state has no active record to replace".into());
    }
    write_active(root, record)
}

fn ensure_private_root(root: &Path) -> Result<(), String> {
    match fs::symlink_metadata(root) {
        Ok(metadata)
            if metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && metadata.permissions().mode() & 0o077 == 0 =>
        {
            Ok(())
        }
        Ok(_) => Err("host recovery state directory is not private".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root)
                .map_err(|error| error.to_string())?;
            if let Some(parent) = root.parent() {
                File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn write_active(root: &Path, record: &ActiveSession) -> Result<(), String> {
    record.validate()?;
    let temporary = root.join(format!(
        "{TEMP_ACTIVE_PREFIX}{}",
        crate::generate_launch_id()
    ));
    let mut serialized = serde_json::to_vec(record).map_err(|error| error.to_string())?;
    serialized.push(b'\n');
    if serialized.len() > MAX_ACTIVE_BYTES {
        return Err("host recovery record is oversized".into());
    }
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&serialized)
            .and_then(|_| file.sync_all())
            .map_err(|error| error.to_string())?;
        fs::rename(&temporary, root.join(ACTIVE_FILE)).map_err(|error| error.to_string())?;
        File::open(root)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(super) fn clear_crash_temporary_active(root: &Path) -> Result<bool, String> {
    if !root.exists() {
        return Ok(false);
    }
    let metadata = fs::symlink_metadata(root).map_err(|error| error.to_string())?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Ok(false);
    }
    let entries: Vec<_> = fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|error| error.to_string())?;
    let temporaries: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(TEMP_ACTIVE_PREFIX)
        })
        .collect();
    if temporaries.is_empty() {
        return Ok(false);
    }
    let non_temporaries: Vec<_> = entries
        .iter()
        .filter(|entry| {
            !entry
                .file_name()
                .to_string_lossy()
                .starts_with(TEMP_ACTIVE_PREFIX)
        })
        .collect();
    if non_temporaries.len() > 1
        || non_temporaries
            .first()
            .is_some_and(|entry| entry.file_name() != ACTIVE_FILE)
    {
        return Ok(false);
    }
    if entries.iter().any(|entry| {
        fs::symlink_metadata(entry.path())
            .map(|metadata| {
                !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata.permissions().mode() & 0o077 != 0
            })
            .unwrap_or(true)
    }) {
        return Ok(false);
    }
    for entry in temporaries {
        fs::remove_file(entry.path()).map_err(|error| error.to_string())?;
    }
    File::open(root)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())?;
    Ok(true)
}

pub(super) fn clear_active(root: &Path) -> Result<(), String> {
    match fs::remove_file(root.join(ACTIVE_FILE)) {
        Ok(()) => File::open(root)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// Removes a record for a launch that never reached the player. An unreadable
/// record is still unlinked, but a symlink target is never followed.
pub(super) fn consume_active(root: &Path) -> Result<Option<ActiveSession>, String> {
    let record = read_active(root).unwrap_or_default();
    clear_active(root)?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    const LAUNCH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const PERSON: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";

    fn record() -> ActiveSession {
        ActiveSession::running(
            LAUNCH.into(),
            "wario".into(),
            Some(PERSON.into()),
            1_700_000_000,
        )
    }

    #[test]
    fn persists_reads_and_consumes_one_private_record() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("host-session");
        persist_active(&state, &record()).unwrap();
        let mode = fs::symlink_metadata(&state).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        let path = state.join(ACTIVE_FILE);
        let mode = fs::symlink_metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!(
                "{{\"phase\":\"running\",\"launchId\":\"{LAUNCH}\",\"gameId\":\"wario\",\"personPublicKey\":\"{PERSON}\",\"startedAt\":1700000000}}\n"
            )
        );
        assert_eq!(read_active(&state).unwrap(), Some(record()));
        assert_eq!(
            persist_active(&state, &record()).unwrap_err(),
            "host recovery state is not empty"
        );
        assert_eq!(consume_active(&state).unwrap(), Some(record()));
        assert_eq!(consume_active(&state).unwrap(), None);
        assert_eq!(read_active(&state).unwrap(), None);
    }

    #[test]
    fn obsolete_launch_id_atom_is_not_a_supported_format() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("host-session");
        fs::create_dir(&state).unwrap();
        fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
        let obsolete = state.join("launch-id");
        fs::write(&obsolete, LAUNCH).unwrap();
        fs::set_permissions(&obsolete, fs::Permissions::from_mode(0o600)).unwrap();

        assert_eq!(
            read_active(&state).unwrap_err(),
            "host recovery state is ambiguous"
        );
    }

    #[test]
    fn unowned_launches_persist_without_a_person() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("host-session");
        let record = ActiveSession::running(LAUNCH.into(), "wario".into(), None, 1_700_000_000);
        persist_active(&state, &record).unwrap();
        assert!(!fs::read_to_string(state.join(ACTIVE_FILE))
            .unwrap()
            .contains("personPublicKey"));
        assert_eq!(read_active(&state).unwrap(), Some(record));
    }

    #[test]
    fn rejects_malformed_oversized_linked_and_loose_records() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("host-session");
        fs::create_dir(&state).unwrap();
        fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
        let path = state.join(ACTIVE_FILE);

        let write = |content: &[u8]| {
            fs::write(&path, content).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        };
        write(b"bad");
        assert_eq!(
            read_active(&state).unwrap_err(),
            "host recovery record is malformed"
        );
        write(
            b"{\"phase\":\"running\",\"launchId\":\"short\",\"gameId\":\"wario\",\"startedAt\":1}",
        );
        assert!(read_active(&state).is_err());
        write(format!("{{\"phase\":\"running\",\"launchId\":\"{LAUNCH}\",\"gameId\":\"\",\"startedAt\":1}}").as_bytes());
        assert_eq!(
            read_active(&state).unwrap_err(),
            "host recovery game identity is invalid"
        );
        write(
            format!("{{\"phase\":\"running\",\"launchId\":\"{LAUNCH}\",\"gameId\":\"wario\",\"personPublicKey\":\"XYZ\",\"startedAt\":1}}")
                .as_bytes(),
        );
        assert_eq!(
            read_active(&state).unwrap_err(),
            "host recovery person identity is invalid"
        );
        write(
            format!(
                "{{\"phase\":\"running\",\"launchId\":\"{LAUNCH}\",\"gameId\":\"wario\",\"startedAt\":1,\"extra\":true}}"
            )
            .as_bytes(),
        );
        assert_eq!(
            read_active(&state).unwrap_err(),
            "host recovery record is malformed"
        );
        write(&vec![b' '; MAX_ACTIVE_BYTES + 1]);
        assert_eq!(
            read_active(&state).unwrap_err(),
            "host recovery record is not a private regular file"
        );
        write(serde_json::to_string(&record()).unwrap().as_bytes());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            read_active(&state).unwrap_err(),
            "host recovery record is not a private regular file"
        );

        let real = root.path().join("real.json");
        fs::rename(&path, &real).unwrap();
        symlink(&real, &path).unwrap();
        assert_eq!(
            read_active(&state).unwrap_err(),
            "host recovery record is not a private regular file"
        );
        // Consuming an unreadable record still removes the link, never the
        // target, and reports nothing to log.
        assert_eq!(consume_active(&state).unwrap(), None);
        assert!(real.exists());
        assert!(!path.exists());

        let linked_root = root.path().join("linked");
        symlink(&state, &linked_root).unwrap();
        assert_eq!(
            read_active(&linked_root).unwrap_err(),
            "host recovery state directory is not private"
        );
        assert!(persist_active(&linked_root, &record()).is_err());
    }

    #[test]
    fn crash_temporaries_are_cleared_only_when_nothing_else_exists() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("host-session");
        fs::create_dir(&state).unwrap();
        fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
        let temporary = state.join(format!("{TEMP_ACTIVE_PREFIX}partial"));
        fs::write(&temporary, "partial").unwrap();
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(clear_crash_temporary_active(&state).unwrap());
        assert!(!temporary.exists());
        assert!(!clear_crash_temporary_active(&state).unwrap());

        fs::write(&temporary, "partial").unwrap();
        persist_active(&state, &record()).unwrap_err();
        fs::remove_file(&temporary).unwrap();
        persist_active(&state, &record()).unwrap();
        fs::write(&temporary, "partial").unwrap();
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(clear_crash_temporary_active(&state).unwrap());
        assert!(!temporary.exists());
        assert_eq!(read_active(&state).unwrap(), Some(record()));
    }

    #[test]
    fn atomically_replaces_running_with_completion_pending() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("host-session");
        persist_active(&state, &record()).unwrap();
        let pending = ActiveSession::CompletionPending {
            launch_id: LAUNCH.into(),
            game_id: "wario".into(),
            person_public_key: PERSON.into(),
            started_at: 1_700_000_000,
            entry: PlayEntry {
                occurred_at: "2026-09-04T10:00:00.000Z".into(),
                duration_seconds: 42.5,
                release_id: None,
            },
        };
        replace_active(&state, &pending).unwrap();
        assert_eq!(read_active(&state).unwrap(), Some(pending));
        let names: Vec<_> = fs::read_dir(&state)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [ACTIVE_FILE]);
    }
}
