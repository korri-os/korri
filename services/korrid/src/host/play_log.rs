//! Play log: the sole stored representation of a game's play history.
//!
//! The completed record shape and the on-disk path codec are the legacy
//! producer baseline:
//!
//! - `legacy:product/platform/library/config/records/play-log.ts` defines
//!   `PlayLog { userId, gameId, entries: PlayEntry[] }` and
//!   `PlayEntry { occurredAt, durationSeconds, releaseId? }`.
//! - `legacy:product/platform/library/play-log-store.ts` stores one JSON
//!   document per `(user, game)` at
//!   `<root>/<encodeURIComponent(userId)>/<encodeURIComponent(gameId)>.json`.
//!   Before every write, it serializes `Date` values with `toISOString()`.
//!   Therefore producer-emitted timestamps are UTC RFC 3339 values. The
//!   recording gate admits durations at or above zero; actual elapsed
//!   durations are finite. Main preserves these producer-emitted records.
//!   It does not accept every permissive input that the legacy JavaScript
//!   decoder could temporarily turn into a `Date` or `Number`.
//! - `legacy:product/platform/library/play-stats.ts` derives `lastPlayed`,
//!   `playCount`, and `totalPlaytimeSeconds` from the entries and never
//!   stores them.
//!
//! On `main` the `userId` is the authenticated person public key. The root
//! is the `play-log` directory under korrid's private state root; legacy
//! used `$XDG_STATE_HOME/korri/play-log`, and the private state root is the
//! `main` equivalent of that state directory. Files follow the identity
//! module's private-state rules: `0700` directories, `0600` files,
//! create-new temporaries, file sync, rename, directory sync, and rejection
//! of symlinks, special files, and oversized documents.

use crate::PlayStats;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

pub(super) const PLAY_LOG_DIRECTORY: &str = "play-log";
/// One person's log for one game must fit comfortably in memory. Each
/// entry is under 100 bytes; this admits well over ten thousand plays.
const MAX_PLAY_LOG_BYTES: usize = 4 * 1024 * 1024;
const NAME_MAX: usize = 255;

static PLAY_LOG_STORAGE: OnceLock<Mutex<()>> = OnceLock::new();

/// One completed play. Preserves the legacy producer's stored shape.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayEntry {
    /// When the session ended, as an RFC 3339 UTC timestamp.
    pub occurred_at: String,
    #[serde(serialize_with = "serialize_legacy_number")]
    pub duration_seconds: f64,
    /// Provenance only. Aggregate stats ignore it. Absent in this slice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
}

/// The stored document. Preserves the legacy producer's stored shape.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayLog {
    pub user_id: String,
    pub game_id: String,
    pub entries: Vec<PlayEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayHistoryKey {
    pub user_id: String,
    pub game_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PlayLogError {
    #[error("play log storage is unavailable")]
    Storage,
    #[error("play log aggregate exceeds the finite wire-number range")]
    AggregateOverflow,
}

#[derive(Clone, Debug)]
pub struct PlayLogStore {
    root: PathBuf,
}

impl PlayLogStore {
    pub fn new(private_state_root: &Path) -> Self {
        Self {
            root: private_state_root.join(PLAY_LOG_DIRECTORY),
        }
    }

    pub(crate) fn validate_key(&self, key: &PlayHistoryKey) -> Result<(), PlayLogError> {
        validate_history_key(key)
    }

    fn user_directory(&self, key: &PlayHistoryKey) -> PathBuf {
        self.root.join(encode_uri_component(&key.user_id))
    }

    fn path_for(&self, key: &PlayHistoryKey) -> PathBuf {
        self.user_directory(key)
            .join(format!("{}.json", encode_uri_component(&key.game_id)))
    }

    /// Loads the log for one `(person, game)`. A missing or malformed
    /// document reads as empty, as in legacy. A document that is a symlink,
    /// a special file, world-readable, or oversized is a storage error.
    pub fn load(&self, key: &PlayHistoryKey) -> Result<PlayLog, PlayLogError> {
        let _storage = storage_lock();
        self.load_unlocked(key)
    }

    fn load_unlocked(&self, key: &PlayHistoryKey) -> Result<PlayLog, PlayLogError> {
        self.load_decoded_unlocked(key, false)
    }

    fn load_for_write_unlocked(&self, key: &PlayHistoryKey) -> Result<PlayLog, PlayLogError> {
        self.load_decoded_unlocked(key, true)
    }

    fn load_decoded_unlocked(
        &self,
        key: &PlayHistoryKey,
        reject_malformed: bool,
    ) -> Result<PlayLog, PlayLogError> {
        validate_history_key(key)?;
        if !existing_private_directory(&self.root)?
            || !existing_private_directory(&self.user_directory(key))?
        {
            return Ok(empty_log(key));
        }
        match read_bounded_private(&self.path_for(key), MAX_PLAY_LOG_BYTES)? {
            None => Ok(empty_log(key)),
            Some(bytes) => {
                let decoded = serde_json::from_slice::<PlayLog>(&bytes)
                    .ok()
                    .filter(|log| valid_log(log, key));
                match decoded {
                    Some(log) => Ok(log),
                    None if reject_malformed => Err(PlayLogError::Storage),
                    None => Ok(empty_log(key)),
                }
            }
        }
    }

    /// Returns a legacy-shaped completion entry whose timestamp is unique
    /// in this person's log for this game. Milliseconds are the smallest
    /// precision that legacy Date serialization preserves.
    pub fn unique_completion_entry(
        &self,
        key: &PlayHistoryKey,
        actual_epoch_millis: u64,
        duration_seconds: f64,
    ) -> Result<PlayEntry, PlayLogError> {
        if !duration_seconds.is_finite() || duration_seconds < 0.0 {
            return Err(PlayLogError::Storage);
        }
        let _storage = storage_lock();
        let log = self.load_for_write_unlocked(key)?;
        let occupied: std::collections::BTreeSet<i64> = log
            .entries
            .iter()
            .filter_map(|entry| parse_rfc3339(&entry.occurred_at))
            .collect();
        let mut instant = i64::try_from(actual_epoch_millis).map_err(|_| PlayLogError::Storage)?;
        while occupied.contains(&instant) {
            instant = instant.checked_add(1).ok_or(PlayLogError::Storage)?;
        }
        Ok(PlayEntry {
            occurred_at: format_rfc3339_millis(instant)?,
            duration_seconds,
            release_id: None,
        })
    }

    /// Appends one completed play. An exact existing entry is an idempotent
    /// success for completion recovery. Retention can remove only entries
    /// that existed before this call. The completion candidate always stays
    /// in the final document, or the write fails and its journal stays live.
    pub fn record(&self, key: &PlayHistoryKey, entry: PlayEntry) -> Result<(), PlayLogError> {
        validate_entry(&entry)?;
        let _storage = storage_lock();
        let mut log = self.load_for_write_unlocked(key)?;
        if log.entries.contains(&entry) {
            return Ok(());
        }
        log.entries.push(entry);
        let protected_index = log.entries.len() - 1;
        let directory = self.user_directory(key);
        ensure_private_directory(&self.root)?;
        ensure_private_directory(&directory)?;
        let serialized = serialize_with_retention(&mut log, protected_index)?;
        write_private_atomically(&self.path_for(key), &serialized)
    }

    /// Derived stats for one `(person, game)`. An empty log has no
    /// `lastPlayed`, a zero count, and zero total playtime, as in legacy.
    pub fn stats(&self, key: &PlayHistoryKey) -> Result<PlayStats, PlayLogError> {
        derive_play_stats(&self.load(key)?.entries)
    }

    /// Moves every local play-log record from one person to another. The
    /// complete merged destination is staged before one directory-level
    /// publication. Existing destination games are preserved; a game-name
    /// collision fails without changing either owner.
    pub fn transfer_person_records(
        &self,
        old_user_id: &str,
        new_user_id: &str,
    ) -> Result<(), PlayLogError> {
        validate_person_id(old_user_id)?;
        validate_person_id(new_user_id)?;
        if old_user_id == new_user_id {
            return Err(PlayLogError::Storage);
        }

        let _storage = storage_lock();
        if !existing_private_directory(&self.root)? {
            return Ok(());
        }
        clean_staged_directories(&self.root)?;
        let old_directory = self.person_directory(old_user_id);
        let new_directory = self.person_directory(new_user_id);
        let old_records = read_person_records(&old_directory, old_user_id)?;
        let new_records = read_person_records(&new_directory, new_user_id)?;
        let Some(mut old_records) = old_records else {
            return Ok(());
        };
        for record in &mut old_records {
            record.log.user_id = new_user_id.to_owned();
            record.bytes = serialize_log(&record.log)?;
        }

        let existing_by_name: std::collections::BTreeMap<_, _> = new_records
            .iter()
            .flatten()
            .map(|record| (record.file_name.as_str(), record.bytes.as_slice()))
            .collect();
        let mut records_to_publish = Vec::new();
        for record in &old_records {
            match existing_by_name.get(record.file_name.as_str()) {
                Some(bytes) if *bytes == record.bytes => {}
                Some(_) => return Err(PlayLogError::Storage),
                None => records_to_publish.push(record),
            }
        }
        if records_to_publish.is_empty() {
            fs::remove_dir_all(&old_directory).map_err(|_| PlayLogError::Storage)?;
            return sync_directory(&self.root);
        }

        let staged = create_staged_directory(&self.root)?;
        let result = (|| {
            if let Some(records) = &new_records {
                for record in records {
                    write_private_new(&staged.join(&record.file_name), &record.bytes)?;
                }
            }
            for record in records_to_publish {
                write_private_new(&staged.join(&record.file_name), &record.bytes)?;
            }
            sync_directory(&staged)?;

            if new_records.is_some() {
                exchange_paths(&staged, &new_directory)?;
            } else {
                fs::rename(&staged, &new_directory).map_err(|_| PlayLogError::Storage)?;
            }
            sync_directory(&self.root)?;

            fs::remove_dir_all(&old_directory).map_err(|_| PlayLogError::Storage)?;
            sync_directory(&self.root)?;
            if staged.exists() {
                fs::remove_dir_all(&staged).map_err(|_| PlayLogError::Storage)?;
                sync_directory(&self.root)?;
            }
            Ok(())
        })();
        if result.is_err() && staged.exists() {
            let _ = fs::remove_dir_all(&staged);
            let _ = sync_directory(&self.root);
        }
        result
    }

    /// Deletes every local play-log record for one person. Missing records are
    /// an idempotent success. The directory is validated in full before any
    /// record is removed.
    pub fn delete_person_records(&self, user_id: &str) -> Result<(), PlayLogError> {
        validate_person_id(user_id)?;
        let _storage = storage_lock();
        if !existing_private_directory(&self.root)? {
            return Ok(());
        }
        let directory = self.person_directory(user_id);
        if read_person_records(&directory, user_id)?.is_none() {
            return Ok(());
        }
        fs::remove_dir_all(&directory).map_err(|_| PlayLogError::Storage)?;
        sync_directory(&self.root)
    }

    fn person_directory(&self, user_id: &str) -> PathBuf {
        self.root.join(encode_uri_component(user_id))
    }
}

#[derive(Debug)]
struct StoredPlayLog {
    file_name: String,
    bytes: Vec<u8>,
    log: PlayLog,
}

fn read_person_records(
    directory: &Path,
    user_id: &str,
) -> Result<Option<Vec<StoredPlayLog>>, PlayLogError> {
    if !existing_private_directory(directory)? {
        return Ok(None);
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(directory).map_err(|_| PlayLogError::Storage)? {
        let entry = entry.map_err(|_| PlayLogError::Storage)?;
        let file_name = entry
            .file_name()
            .into_string()
            .map_err(|_| PlayLogError::Storage)?;
        let bytes = read_bounded_private(&entry.path(), MAX_PLAY_LOG_BYTES)?
            .ok_or(PlayLogError::Storage)?;
        let log: PlayLog = serde_json::from_slice(&bytes).map_err(|_| PlayLogError::Storage)?;
        let key = PlayHistoryKey {
            user_id: user_id.to_owned(),
            game_id: log.game_id.clone(),
        };
        validate_history_key(&key)?;
        if !valid_log(&log, &key)
            || file_name != format!("{}.json", encode_uri_component(&log.game_id))
        {
            return Err(PlayLogError::Storage);
        }
        records.push(StoredPlayLog {
            file_name,
            bytes,
            log,
        });
    }
    records.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    Ok(Some(records))
}

fn serialize_log(log: &PlayLog) -> Result<Vec<u8>, PlayLogError> {
    let mut bytes = serde_json::to_vec_pretty(log).map_err(|_| PlayLogError::Storage)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_PLAY_LOG_BYTES {
        return Err(PlayLogError::Storage);
    }
    Ok(bytes)
}

fn clean_staged_directories(root: &Path) -> Result<(), PlayLogError> {
    let mut staged = Vec::new();
    for entry in fs::read_dir(root).map_err(|_| PlayLogError::Storage)? {
        let entry = entry.map_err(|_| PlayLogError::Storage)?;
        let Some(file_name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_staged_directory_name(&file_name) {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|_| PlayLogError::Storage)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(PlayLogError::Storage);
        }
        for child in fs::read_dir(&path).map_err(|_| PlayLogError::Storage)? {
            let child = child.map_err(|_| PlayLogError::Storage)?;
            let metadata = fs::symlink_metadata(child.path()).map_err(|_| PlayLogError::Storage)?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.permissions().mode() & 0o077 != 0
                || metadata.len() > MAX_PLAY_LOG_BYTES as u64
            {
                return Err(PlayLogError::Storage);
            }
        }
        staged.push(path);
    }
    if staged.is_empty() {
        return Ok(());
    }
    for path in staged {
        fs::remove_dir_all(path).map_err(|_| PlayLogError::Storage)?;
    }
    sync_directory(root)
}

fn is_staged_directory_name(file_name: &str) -> bool {
    file_name
        .strip_prefix(".transfer.")
        .and_then(|name| name.strip_suffix(".tmp"))
        .is_some_and(|nonce| {
            nonce.len() == 16
                && nonce
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

fn create_staged_directory(root: &Path) -> Result<PathBuf, PlayLogError> {
    for _ in 0..16 {
        let path = root.join(format!(".transfer.{:016x}.tmp", rand::random::<u64>()));
        match DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(PlayLogError::Storage),
        }
    }
    Err(PlayLogError::Storage)
}

fn write_private_new(path: &Path, content: &[u8]) -> Result<(), PlayLogError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| PlayLogError::Storage)?;
    file.write_all(content).map_err(|_| PlayLogError::Storage)?;
    file.sync_all().map_err(|_| PlayLogError::Storage)
}

fn exchange_paths(left: &Path, right: &Path) -> Result<(), PlayLogError> {
    let left =
        std::ffi::CString::new(left.as_os_str().as_bytes()).map_err(|_| PlayLogError::Storage)?;
    let right =
        std::ffi::CString::new(right.as_os_str().as_bytes()).map_err(|_| PlayLogError::Storage)?;
    // SAFETY: both C strings live through the call and contain no interior
    // NUL. renameat2 does not retain either pointer.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            left.as_ptr(),
            libc::AT_FDCWD,
            right.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(PlayLogError::Storage)
    }
}

fn storage_lock() -> std::sync::MutexGuard<'static, ()> {
    PLAY_LOG_STORAGE
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("play log storage mutex poisoned")
}

fn empty_log(key: &PlayHistoryKey) -> PlayLog {
    PlayLog {
        user_id: key.user_id.clone(),
        game_id: key.game_id.clone(),
        entries: Vec::new(),
    }
}

/// Mirrors legacy `derivePlayStats`: the newest `occurredAt` wins, counts
/// are entry counts, and playtime is the plain sum. A log with no entries
/// reads as never played.
pub fn derive_play_stats(entries: &[PlayEntry]) -> Result<PlayStats, PlayLogError> {
    let mut last: Option<(i64, &str)> = None;
    let mut total_playtime_seconds = 0.0;
    for entry in entries {
        if let Some(instant) = parse_rfc3339(&entry.occurred_at) {
            if last.is_none_or(|(current, _)| instant > current) {
                last = Some((instant, &entry.occurred_at));
            }
        }
        total_playtime_seconds += entry.duration_seconds;
        if !total_playtime_seconds.is_finite() {
            return Err(PlayLogError::AggregateOverflow);
        }
    }
    Ok(PlayStats {
        last_played: last.map(|(_, value)| value.to_owned()),
        play_count: u32::try_from(entries.len()).unwrap_or(u32::MAX),
        total_playtime_seconds,
    })
}

#[cfg(test)]
pub fn format_rfc3339_utc(epoch_seconds: u64) -> String {
    format_rfc3339_millis(
        i64::try_from(epoch_seconds)
            .ok()
            .and_then(|seconds| seconds.checked_mul(1_000))
            .unwrap_or(i64::MAX),
    )
    .unwrap_or_else(|_| "9999-12-31T23:59:59.999Z".into())
}

fn format_rfc3339_millis(epoch_millis: i64) -> Result<String, PlayLogError> {
    let instant =
        DateTime::<Utc>::from_timestamp_millis(epoch_millis).ok_or(PlayLogError::Storage)?;
    Ok(instant.to_rfc3339_opts(SecondsFormat::Millis, true))
}

fn parse_rfc3339(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|instant| instant.timestamp_millis())
}

fn serialize_legacy_number<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if value.fract() == 0.0 && *value >= i64::MIN as f64 && *value <= i64::MAX as f64 {
        serializer.serialize_i64(*value as i64)
    } else {
        serializer.serialize_f64(*value)
    }
}

fn valid_log(log: &PlayLog, key: &PlayHistoryKey) -> bool {
    log.user_id == key.user_id
        && log.game_id == key.game_id
        && log
            .entries
            .iter()
            .all(|entry| validate_entry(entry).is_ok())
}

pub(super) fn validate_entry(entry: &PlayEntry) -> Result<(), PlayLogError> {
    if parse_rfc3339(&entry.occurred_at).is_none()
        || !entry.duration_seconds.is_finite()
        || entry.duration_seconds < 0.0
    {
        return Err(PlayLogError::Storage);
    }
    Ok(())
}

fn validate_person_id(user_id: &str) -> Result<(), PlayLogError> {
    let encoded = encode_uri_component(user_id);
    if user_id.is_empty() || encoded == "." || encoded == ".." || encoded.len() > NAME_MAX {
        return Err(PlayLogError::Storage);
    }
    Ok(())
}

fn validate_history_key(key: &PlayHistoryKey) -> Result<(), PlayLogError> {
    validate_person_id(&key.user_id)?;
    if key.game_id.is_empty() || encode_uri_component(&key.game_id).len() + ".json".len() > NAME_MAX
    {
        return Err(PlayLogError::Storage);
    }
    Ok(())
}

fn serialize_with_retention(
    log: &mut PlayLog,
    mut protected_index: usize,
) -> Result<Vec<u8>, PlayLogError> {
    if protected_index >= log.entries.len() {
        return Err(PlayLogError::Storage);
    }
    let protected = log.entries[protected_index].clone();
    loop {
        let mut serialized = serde_json::to_vec_pretty(log).map_err(|_| PlayLogError::Storage)?;
        serialized.push(b'\n');
        if serialized.len() <= MAX_PLAY_LOG_BYTES {
            if log.entries.get(protected_index) != Some(&protected) {
                return Err(PlayLogError::Storage);
            }
            return Ok(serialized);
        }
        let oldest = log
            .entries
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != protected_index)
            .min_by_key(|(_, entry)| parse_rfc3339(&entry.occurred_at).unwrap_or(i64::MIN))
            .map(|(index, _)| index)
            .ok_or(PlayLogError::Storage)?;
        log.entries.remove(oldest);
        if oldest < protected_index {
            protected_index -= 1;
        }
    }
}

/// Exact `encodeURIComponent` semantics: `A-Z a-z 0-9 - _ . ! ~ * ' ( )`
/// pass through; every other byte of the UTF-8 encoding is `%XX` with
/// uppercase hexadecimal digits.
pub fn encode_uri_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => encoded.push(byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn existing_private_directory(path: &Path) -> Result<bool, PlayLogError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(PlayLogError::Storage),
    };
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(PlayLogError::Storage);
    }
    Ok(true)
}

fn ensure_private_directory(path: &Path) -> Result<(), PlayLogError> {
    if !path.exists() {
        match DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {
                if let Some(parent) = path.parent() {
                    sync_directory(parent)?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(PlayLogError::Storage),
        }
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| PlayLogError::Storage)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(PlayLogError::Storage);
    }
    Ok(())
}

fn read_bounded_private(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, PlayLogError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(PlayLogError::Storage),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > limit as u64
    {
        return Err(PlayLogError::Storage);
    }
    let file = File::open(path).map_err(|_| PlayLogError::Storage)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| PlayLogError::Storage)?;
    if bytes.len() > limit {
        return Err(PlayLogError::Storage);
    }
    Ok(Some(bytes))
}

fn write_private_atomically(path: &Path, content: &[u8]) -> Result<(), PlayLogError> {
    let parent = path.parent().ok_or(PlayLogError::Storage)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("play-log"),
        rand::random::<u64>()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| PlayLogError::Storage)?;
        file.write_all(content).map_err(|_| PlayLogError::Storage)?;
        file.sync_all().map_err(|_| PlayLogError::Storage)?;
        fs::rename(&temporary, path).map_err(|_| PlayLogError::Storage)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sync_directory(path: &Path) -> Result<(), PlayLogError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| PlayLogError::Storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    const PERSON: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
    const NEW_PERSON: &str = "42b4505d5a2643d8fbb74649efc53412fe401429b8b53c73943b7a2211c5c35a";

    fn key(game: &str) -> PlayHistoryKey {
        PlayHistoryKey {
            user_id: PERSON.into(),
            game_id: game.into(),
        }
    }

    fn entry(occurred_at: &str, duration: f64) -> PlayEntry {
        PlayEntry {
            occurred_at: occurred_at.into(),
            duration_seconds: duration,
            release_id: None,
        }
    }

    fn person_key(person: &str, game: &str) -> PlayHistoryKey {
        PlayHistoryKey {
            user_id: person.into(),
            game_id: game.into(),
        }
    }

    #[test]
    fn transfer_moves_all_records_rekeys_only_the_owner_and_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let old_wario = person_key(PERSON, "wario");
        let old_steam = person_key(PERSON, "steam/1029210");
        let new_existing = person_key(NEW_PERSON, "existing");
        store
            .record(
                &old_wario,
                PlayEntry {
                    occurred_at: "2026-09-04T10:00:00.000Z".into(),
                    duration_seconds: 10.25,
                    release_id: Some("release-wario".into()),
                },
            )
            .unwrap();
        store
            .record(&old_steam, entry("2026-09-05T11:00:00.000Z", 20.0))
            .unwrap();
        store
            .record(&new_existing, entry("2026-09-06T12:00:00.000Z", 30.0))
            .unwrap();
        let old_wario_log = store.load(&old_wario).unwrap();
        let old_steam_log = store.load(&old_steam).unwrap();
        let existing_log = store.load(&new_existing).unwrap();
        let old_directory = store.user_directory(&old_wario);
        let new_directory = store.user_directory(&new_existing);

        store.transfer_person_records(PERSON, NEW_PERSON).unwrap();

        assert!(!old_directory.exists());
        assert!(new_directory.is_dir());
        let mut expected_wario = old_wario_log;
        expected_wario.user_id = NEW_PERSON.into();
        let mut expected_steam = old_steam_log;
        expected_steam.user_id = NEW_PERSON.into();
        assert_eq!(
            store.load(&person_key(NEW_PERSON, "wario")).unwrap(),
            expected_wario
        );
        assert_eq!(
            store
                .load(&person_key(NEW_PERSON, "steam/1029210"))
                .unwrap(),
            expected_steam
        );
        assert_eq!(store.load(&new_existing).unwrap(), existing_log);
        let names: Vec<_> = fs::read_dir(&store.root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [encode_uri_component(NEW_PERSON)]);

        store.transfer_person_records(PERSON, NEW_PERSON).unwrap();
        assert!(!old_directory.exists());
        assert_eq!(
            store.load(&person_key(NEW_PERSON, "wario")).unwrap(),
            expected_wario
        );
    }

    #[test]
    fn decline_deletes_all_person_records_and_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        for game in ["wario", "steam/1029210"] {
            store
                .record(
                    &person_key(PERSON, game),
                    entry("2026-09-04T10:00:00.000Z", 10.0),
                )
                .unwrap();
        }
        let old_directory = store.user_directory(&key("wario"));

        store.delete_person_records(PERSON).unwrap();
        assert!(!old_directory.exists());
        store.delete_person_records(PERSON).unwrap();
        assert!(!old_directory.exists());
    }

    #[test]
    fn transfer_retry_after_destination_publication_finishes_without_orphans() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let old_wario = person_key(PERSON, "wario");
        let old_steam = person_key(PERSON, "steam/1029210");
        let new_existing = person_key(NEW_PERSON, "existing");
        store
            .record(&old_wario, entry("2026-09-04T10:00:00.000Z", 10.0))
            .unwrap();
        store
            .record(&old_steam, entry("2026-09-05T10:00:00.000Z", 20.0))
            .unwrap();
        store
            .record(&new_existing, entry("2026-09-06T10:00:00.000Z", 30.0))
            .unwrap();

        let old_directory = store.user_directory(&old_wario);
        let new_directory = store.user_directory(&new_existing);
        let stale = store.root.join(".transfer.0123456789abcdef.tmp");
        fs::create_dir(&stale).unwrap();
        fs::set_permissions(&stale, fs::Permissions::from_mode(0o700)).unwrap();
        fs::copy(store.path_for(&new_existing), stale.join("existing.json")).unwrap();
        fs::set_permissions(
            stale.join("existing.json"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();

        let mut expected_wario = store.load(&old_wario).unwrap();
        expected_wario.user_id = NEW_PERSON.into();
        let expected_wario_bytes = serialize_log(&expected_wario).unwrap();
        let mut expected_steam = store.load(&old_steam).unwrap();
        expected_steam.user_id = NEW_PERSON.into();
        let expected_steam_bytes = serialize_log(&expected_steam).unwrap();
        write_private_new(&new_directory.join("wario.json"), &expected_wario_bytes).unwrap();
        write_private_new(
            &new_directory.join("steam%2F1029210.json"),
            &expected_steam_bytes,
        )
        .unwrap();

        store.transfer_person_records(PERSON, NEW_PERSON).unwrap();

        assert!(!old_directory.exists());
        assert!(!stale.exists());
        assert_eq!(
            fs::read(new_directory.join("wario.json")).unwrap(),
            expected_wario_bytes
        );
        assert_eq!(
            fs::read(new_directory.join("steam%2F1029210.json")).unwrap(),
            expected_steam_bytes
        );
        assert_eq!(store.load(&new_existing).unwrap().entries.len(), 1);
        let names: Vec<_> = fs::read_dir(&store.root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [encode_uri_component(NEW_PERSON)]);
    }

    #[test]
    fn byte_different_rekeyed_destination_is_still_a_collision() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let old_key = person_key(PERSON, "wario");
        let new_key = person_key(NEW_PERSON, "wario");
        store
            .record(&old_key, entry("2026-09-04T10:00:00.000Z", 10.0))
            .unwrap();
        let old_before = fs::read(store.path_for(&old_key)).unwrap();
        let mut rekeyed = store.load(&old_key).unwrap();
        rekeyed.user_id = NEW_PERSON.into();
        let mut compact = serde_json::to_vec(&rekeyed).unwrap();
        compact.push(b'\n');
        ensure_private_directory(&store.user_directory(&new_key)).unwrap();
        write_private_new(&store.path_for(&new_key), &compact).unwrap();

        assert_eq!(
            store.transfer_person_records(PERSON, NEW_PERSON),
            Err(PlayLogError::Storage)
        );
        assert_eq!(fs::read(store.path_for(&old_key)).unwrap(), old_before);
        assert_eq!(fs::read(store.path_for(&new_key)).unwrap(), compact);
    }

    #[test]
    fn unsafe_stale_transfer_directory_is_rejected_without_being_followed() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let old_key = person_key(PERSON, "wario");
        store
            .record(&old_key, entry("2026-09-04T10:00:00.000Z", 10.0))
            .unwrap();
        let old_before = fs::read(store.path_for(&old_key)).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let marker = outside.path().join("keep");
        fs::write(&marker, b"keep").unwrap();
        let stale = store.root.join(".transfer.fedcba9876543210.tmp");
        symlink(outside.path(), &stale).unwrap();

        assert_eq!(
            store.transfer_person_records(PERSON, NEW_PERSON),
            Err(PlayLogError::Storage)
        );
        assert_eq!(fs::read(store.path_for(&old_key)).unwrap(), old_before);
        assert!(fs::symlink_metadata(stale)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(marker).unwrap(), b"keep");
    }

    #[test]
    fn transfer_collision_fails_without_changing_either_owner() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let old_key = person_key(PERSON, "wario");
        let new_key = person_key(NEW_PERSON, "wario");
        store
            .record(&old_key, entry("2026-09-04T10:00:00.000Z", 10.0))
            .unwrap();
        store
            .record(&new_key, entry("2026-09-05T10:00:00.000Z", 20.0))
            .unwrap();
        let old_before = fs::read(store.path_for(&old_key)).unwrap();
        let new_before = fs::read(store.path_for(&new_key)).unwrap();

        assert_eq!(
            store.transfer_person_records(PERSON, NEW_PERSON),
            Err(PlayLogError::Storage)
        );
        assert_eq!(fs::read(store.path_for(&old_key)).unwrap(), old_before);
        assert_eq!(fs::read(store.path_for(&new_key)).unwrap(), new_before);
        assert_eq!(fs::read_dir(&store.root).unwrap().count(), 2);
    }

    #[test]
    fn malformed_or_unsafe_records_fail_closed_for_transfer_and_delete() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let old_key = person_key(PERSON, "wario");
        let new_key = person_key(NEW_PERSON, "existing");
        store
            .record(&old_key, entry("2026-09-04T10:00:00.000Z", 10.0))
            .unwrap();
        store
            .record(&new_key, entry("2026-09-05T10:00:00.000Z", 20.0))
            .unwrap();
        let malformed = b"{not json";
        fs::write(store.path_for(&old_key), malformed).unwrap();

        assert_eq!(
            store.transfer_person_records(PERSON, NEW_PERSON),
            Err(PlayLogError::Storage)
        );
        assert_eq!(fs::read(store.path_for(&old_key)).unwrap(), malformed);
        assert_eq!(store.load(&new_key).unwrap().entries.len(), 1);
        assert_eq!(
            store.delete_person_records(PERSON),
            Err(PlayLogError::Storage)
        );
        assert!(store.user_directory(&old_key).is_dir());

        fs::remove_dir_all(store.user_directory(&old_key)).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_record = outside.path().join("outside.json");
        fs::write(&outside_record, b"keep").unwrap();
        symlink(outside.path(), store.user_directory(&old_key)).unwrap();
        assert_eq!(
            store.transfer_person_records(PERSON, NEW_PERSON),
            Err(PlayLogError::Storage)
        );
        assert_eq!(
            store.delete_person_records(PERSON),
            Err(PlayLogError::Storage)
        );
        assert_eq!(fs::read(outside_record).unwrap(), b"keep");
    }

    #[test]
    fn malformed_destination_and_unsafe_person_paths_fail_before_publication() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let old_key = person_key(PERSON, "wario");
        let new_key = person_key(NEW_PERSON, "existing");
        store
            .record(&old_key, entry("2026-09-04T10:00:00.000Z", 10.0))
            .unwrap();
        store
            .record(&new_key, entry("2026-09-05T10:00:00.000Z", 20.0))
            .unwrap();
        let old_before = fs::read(store.path_for(&old_key)).unwrap();
        fs::write(store.path_for(&new_key), b"{not json").unwrap();

        assert_eq!(
            store.transfer_person_records(PERSON, NEW_PERSON),
            Err(PlayLogError::Storage)
        );
        assert_eq!(fs::read(store.path_for(&old_key)).unwrap(), old_before);
        assert_eq!(fs::read(store.path_for(&new_key)).unwrap(), b"{not json");
        assert_eq!(fs::read_dir(&store.root).unwrap().count(), 2);

        assert_eq!(
            store.transfer_person_records("..", NEW_PERSON),
            Err(PlayLogError::Storage)
        );
        assert_eq!(
            store.delete_person_records(".."),
            Err(PlayLogError::Storage)
        );
        assert_eq!(fs::read(store.path_for(&old_key)).unwrap(), old_before);
    }

    #[test]
    fn missing_and_malformed_logs_read_as_empty_like_legacy() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        assert_eq!(store.load(&key("wario")).unwrap(), empty_log(&key("wario")));
        assert_eq!(
            store.stats(&key("wario")).unwrap(),
            PlayStats {
                last_played: None,
                play_count: 0,
                total_playtime_seconds: 0.0,
            }
        );

        store
            .record(&key("wario"), entry("2026-09-04T10:00:00.000Z", 60.0))
            .unwrap();
        let path = store.path_for(&key("wario"));
        fs::write(&path, "{not json").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(store.load(&key("wario")).unwrap(), empty_log(&key("wario")));

        // Unknown fields are strict in legacy (`onExcessProperty: error`).
        fs::write(
            &path,
            format!(r#"{{"userId":"{PERSON}","gameId":"wario","entries":[],"extra":1}}"#),
        )
        .unwrap();
        assert_eq!(store.load(&key("wario")).unwrap(), empty_log(&key("wario")));

        // A document whose identity does not match its path is malformed.
        fs::write(
            &path,
            r#"{"userId":"someone-else","gameId":"wario","entries":[{"occurredAt":"2026-09-04T10:00:00.000Z","durationSeconds":5}]}"#,
        )
        .unwrap();
        assert_eq!(store.load(&key("wario")).unwrap(), empty_log(&key("wario")));

        // Legacy's DateFromString rejects a non-date occurrence.
        fs::write(
            &path,
            format!(
                r#"{{"userId":"{PERSON}","gameId":"wario","entries":[{{"occurredAt":"not-a-date","durationSeconds":5}}]}}"#
            ),
        )
        .unwrap();
        assert_eq!(store.load(&key("wario")).unwrap(), empty_log(&key("wario")));
    }

    #[test]
    fn decodes_and_encodes_the_exact_legacy_document_shape() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let legacy_document = format!(
            "{{\n  \"userId\": \"{PERSON}\",\n  \"gameId\": \"steam/1029210\",\n  \"entries\": [\n    {{\n      \"occurredAt\": \"2026-07-07T04:42:08.376Z\",\n      \"durationSeconds\": 258,\n      \"releaseId\": \"release-1\"\n    }}\n  ]\n}}\n"
        );
        let key = key("steam/1029210");
        let path = store.path_for(&key);
        assert_eq!(
            path,
            root.path()
                .join(PLAY_LOG_DIRECTORY)
                .join(PERSON)
                .join("steam%2F1029210.json")
        );
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for directory in [&store.root, path.parent().unwrap()] {
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::write(&path, &legacy_document).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let loaded = store.load(&key).unwrap();
        assert_eq!(
            loaded.entries,
            [PlayEntry {
                occurred_at: "2026-07-07T04:42:08.376Z".into(),
                duration_seconds: 258.0,
                release_id: Some("release-1".into()),
            }]
        );

        store
            .record(&key, entry("2026-09-04T10:00:00.000Z", 60.0))
            .unwrap();
        let written = fs::read_to_string(&path).unwrap();
        let expected = format!(
            "{{\n  \"userId\": \"{PERSON}\",\n  \"gameId\": \"steam/1029210\",\n  \"entries\": [\n    {{\n      \"occurredAt\": \"2026-07-07T04:42:08.376Z\",\n      \"durationSeconds\": 258,\n      \"releaseId\": \"release-1\"\n    }},\n    {{\n      \"occurredAt\": \"2026-09-04T10:00:00.000Z\",\n      \"durationSeconds\": 60\n    }}\n  ]\n}}\n"
        );
        assert_eq!(written, expected);
        assert_eq!(
            store.stats(&key).unwrap(),
            PlayStats {
                last_played: Some("2026-09-04T10:00:00.000Z".into()),
                play_count: 2,
                total_playtime_seconds: 318.0,
            }
        );
    }

    #[test]
    fn path_codec_matches_encode_uri_component() {
        assert_eq!(encode_uri_component("abc-_.!~*'()"), "abc-_.!~*'()");
        assert_eq!(encode_uri_component("steam/1029210"), "steam%2F1029210");
        assert_eq!(encode_uri_component("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(encode_uri_component("../x"), "..%2Fx");
        assert_eq!(encode_uri_component("é"), "%C3%A9");
        assert_eq!(encode_uri_component("@korri:steam"), "%40korri%3Asteam");
    }

    #[test]
    fn append_is_atomic_private_and_leaves_no_temporary() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let key = key("wario");
        store
            .record(&key, entry("2026-09-04T10:00:00.000Z", 10.0))
            .unwrap();
        store
            .record(&key, entry("2026-09-04T11:00:00.000Z", 20.0))
            .unwrap();
        let path = store.path_for(&key);
        for directory in [&store.root, &store.user_directory(&key)] {
            let mode = fs::symlink_metadata(directory)
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700, "{directory:?}");
        }
        let mode = fs::symlink_metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["wario.json"]);
        assert_eq!(store.load(&key).unwrap().entries.len(), 2);
    }

    #[test]
    fn rejects_symlinks_special_files_loose_modes_and_oversized_documents() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let key = key("wario");
        store
            .record(&key, entry("2026-09-04T10:00:00.000Z", 10.0))
            .unwrap();
        let path = store.path_for(&key);

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(store.load(&key).unwrap_err(), PlayLogError::Storage);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let real = root.path().join("real.json");
        fs::rename(&path, &real).unwrap();
        symlink(&real, &path).unwrap();
        assert_eq!(store.load(&key).unwrap_err(), PlayLogError::Storage);
        assert_eq!(
            store
                .record(&key, entry("2026-09-04T12:00:00.000Z", 5.0))
                .unwrap_err(),
            PlayLogError::Storage
        );
        fs::remove_file(&path).unwrap();

        fs::create_dir(&path).unwrap();
        assert_eq!(store.load(&key).unwrap_err(), PlayLogError::Storage);
        fs::remove_dir(&path).unwrap();

        let oversized = vec![b' '; MAX_PLAY_LOG_BYTES + 1];
        fs::write(&path, oversized).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(store.load(&key).unwrap_err(), PlayLogError::Storage);

        // A linked user directory is refused before any write.
        let other = tempfile::tempdir().unwrap();
        let linked_key = PlayHistoryKey {
            user_id: "linked".into(),
            game_id: "wario".into(),
        };
        symlink(other.path(), store.user_directory(&linked_key)).unwrap();
        assert_eq!(store.load(&linked_key).unwrap_err(), PlayLogError::Storage);
        assert_eq!(
            store
                .record(&linked_key, entry("2026-09-04T12:00:00.000Z", 5.0))
                .unwrap_err(),
            PlayLogError::Storage
        );
        assert!(fs::read_dir(other.path()).unwrap().next().is_none());
    }

    #[test]
    fn concurrent_appends_are_serialized_and_none_are_lost() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let key = key("wario");
        let handles: Vec<_> = (0..16)
            .map(|index| {
                let store = store.clone();
                let key = key.clone();
                std::thread::spawn(move || {
                    store
                        .record(&key, entry(&format_rfc3339_utc(1_700_000_000 + index), 1.0))
                        .unwrap()
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let stats = store.stats(&key).unwrap();
        assert_eq!(stats.play_count, 16);
        assert_eq!(stats.total_playtime_seconds, 16.0);
        assert_eq!(stats.last_played, Some(format_rfc3339_utc(1_700_000_015)));
    }

    #[test]
    fn stats_follow_legacy_derivation_and_newest_last_played_wins() {
        assert_eq!(
            derive_play_stats(&[]).unwrap(),
            PlayStats {
                last_played: None,
                play_count: 0,
                total_playtime_seconds: 0.0,
            }
        );
        let stats = derive_play_stats(&[
            entry("2026-09-04T10:00:00.000Z", 30.0),
            entry("2026-09-06T08:00:00.000Z", 90.0),
            entry("2026-09-05T23:59:59.999Z", 15.0),
        ])
        .unwrap();
        assert_eq!(
            stats,
            PlayStats {
                last_played: Some("2026-09-06T08:00:00.000Z".into()),
                play_count: 3,
                total_playtime_seconds: 135.0,
            }
        );
        // The provenance field never changes the aggregate.
        let with_release = derive_play_stats(&[PlayEntry {
            occurred_at: "2026-09-04T10:00:00.000Z".into(),
            duration_seconds: 7.0,
            release_id: Some("r".into()),
        }])
        .unwrap();
        assert_eq!(with_release.play_count, 1);
        assert_eq!(with_release.total_playtime_seconds, 7.0);
    }

    #[test]
    fn rfc3339_codec_round_trips_and_orders() {
        assert_eq!(format_rfc3339_utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            format_rfc3339_utc(1_700_000_000),
            "2023-11-14T22:13:20.000Z"
        );
        assert_eq!(
            format_rfc3339_utc(1_782_213_728),
            "2026-06-23T11:22:08.000Z"
        );
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            parse_rfc3339("2023-11-14T22:13:20.000Z"),
            Some(1_700_000_000_000)
        );
        assert_eq!(
            parse_rfc3339("2023-11-14T22:13:20Z"),
            Some(1_700_000_000_000)
        );
        assert_eq!(
            parse_rfc3339("2026-07-07T04:42:08.376Z"),
            Some(1_783_399_328_376)
        );
        assert_eq!(
            parse_rfc3339("2026-07-07 04:42:08Z"),
            Some(1_783_399_328_000)
        );
        assert_eq!(
            parse_rfc3339("2026-07-07T05:42:08+01:00"),
            Some(1_783_399_328_000)
        );
        assert_eq!(parse_rfc3339("2026-07-07T04:42:08"), None);
        assert_eq!(parse_rfc3339("garbage"), None);
        assert!(
            parse_rfc3339("2026-09-05T00:00:00.000Z") > parse_rfc3339("2026-09-04T23:59:59.999Z")
        );
        assert_eq!(parse_rfc3339("garbage"), None);
    }

    #[test]
    fn accepts_offset_timestamps_and_fractional_nonnegative_durations() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let key = key("wario");
        store
            .record(&key, entry("2026-07-07T05:42:08+01:00", 1.25))
            .unwrap();
        store
            .record(&key, entry("2026-07-07T04:42:09Z", 2.5))
            .unwrap();
        assert_eq!(
            store.stats(&key).unwrap(),
            PlayStats {
                last_played: Some("2026-07-07T04:42:09Z".into()),
                play_count: 2,
                total_playtime_seconds: 3.75,
            }
        );
        let written = fs::read_to_string(store.path_for(&key)).unwrap();
        assert!(written.contains("2026-07-07T05:42:08+01:00"));
        assert!(written.contains("\"durationSeconds\": 1.25"));
    }

    #[test]
    fn producer_emitted_legacy_records_load_but_permissive_only_inputs_block_writes() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let key = key("wario");
        store
            .record(&key, entry("2026-09-04T10:00:00.000Z", 10.0))
            .unwrap();
        let path = store.path_for(&key);
        let before = fs::read(&path).unwrap();
        assert_eq!(
            store
                .record(&key, entry("2026-09-04T11:00:00.000Z", -1.0))
                .unwrap_err(),
            PlayLogError::Storage
        );
        assert_eq!(fs::read(&path).unwrap(), before);

        fs::write(&path, format!(
            r#"{{"userId":"{PERSON}","gameId":"wario","entries":[{{"occurredAt":"2026-07-07T04:42:08.376Z","durationSeconds":5}}]}}"#
        )).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(store.load(&key).unwrap().entries.len(), 1);

        // Legacy's decoder accepted this date-only string, but its writer
        // never emitted it: Date.toISOString() always wrote UTC RFC 3339.
        for unsupported in [
            format!(
                r#"{{"userId":"{PERSON}","gameId":"wario","entries":[{{"occurredAt":"2026-07-07","durationSeconds":5}}]}}"#
            ),
            format!(
                r#"{{"userId":"{PERSON}","gameId":"wario","entries":[{{"occurredAt":"2026-07-07T04:42:08.376Z","durationSeconds":-1}}]}}"#
            ),
        ] {
            fs::write(&path, unsupported).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            let malformed = fs::read(&path).unwrap();
            assert_eq!(store.load(&key).unwrap(), empty_log(&key));
            assert_eq!(
                store
                    .record(&key, entry("2026-09-04T12:00:00.000Z", 5.0))
                    .unwrap_err(),
                PlayLogError::Storage
            );
            assert_eq!(fs::read(&path).unwrap(), malformed);
        }
    }

    #[test]
    fn retention_keeps_newest_chronological_entries_within_the_bound() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let key = key("wario");
        let empty_old = PlayEntry {
            occurred_at: "2026-01-01T00:00:00.000Z".into(),
            duration_seconds: 1.0,
            release_id: Some(String::new()),
        };
        let base_size = serde_json::to_vec_pretty(&PlayLog {
            user_id: key.user_id.clone(),
            game_id: key.game_id.clone(),
            entries: vec![empty_old.clone()],
        })
        .unwrap()
        .len()
            + 1;
        let old = PlayEntry {
            release_id: Some("x".repeat(MAX_PLAY_LOG_BYTES - base_size)),
            ..empty_old
        };
        store.record(&key, old).unwrap();
        let newest = entry("2026-09-04T10:00:00.000Z", 2.0);
        store.record(&key, newest.clone()).unwrap();
        let loaded = store.load(&key).unwrap();
        assert_eq!(loaded.entries, [newest]);
        assert!(fs::metadata(store.path_for(&key)).unwrap().len() <= MAX_PLAY_LOG_BYTES as u64);
    }

    #[test]
    fn retention_never_discards_an_older_pending_completion() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let key = key("wario");
        let large_future = PlayEntry {
            occurred_at: "2099-01-01T00:00:00.000Z".into(),
            duration_seconds: 1.0,
            release_id: Some("x".repeat(MAX_PLAY_LOG_BYTES - 256)),
        };
        store.record(&key, large_future).unwrap();
        let pending = entry("2026-09-04T10:00:00.000Z", 2.0);
        store.record(&key, pending.clone()).unwrap();
        let loaded = store.load(&key).unwrap();
        assert_eq!(loaded.entries, [pending]);
    }

    #[test]
    fn candidate_that_cannot_fit_alone_fails_without_overwriting_history() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let key = key("wario");
        let existing = entry("2026-09-04T10:00:00.000Z", 2.0);
        store.record(&key, existing.clone()).unwrap();
        let path = store.path_for(&key);
        let before = fs::read(&path).unwrap();
        let oversized_candidate = PlayEntry {
            occurred_at: "2026-09-05T10:00:00.000Z".into(),
            duration_seconds: 3.0,
            release_id: Some("x".repeat(MAX_PLAY_LOG_BYTES)),
        };

        assert_eq!(
            store.record(&key, oversized_candidate),
            Err(PlayLogError::Storage)
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(store.load(&key).unwrap().entries, [existing]);
    }

    #[test]
    fn aggregate_overflow_is_typed() {
        assert_eq!(
            derive_play_stats(&[
                entry("2026-09-04T10:00:00.000Z", f64::MAX),
                entry("2026-09-05T10:00:00.000Z", f64::MAX),
            ]),
            Err(PlayLogError::AggregateOverflow)
        );
    }

    #[test]
    fn completion_timestamp_is_unique_and_exact_replay_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        let key = key("wario");
        store
            .record(&key, entry("2026-09-04T10:00:00.000Z", 1.0))
            .unwrap();
        let actual = parse_rfc3339("2026-09-04T10:00:00.000Z").unwrap() as u64;
        let pending = store.unique_completion_entry(&key, actual, 2.0).unwrap();
        assert_eq!(pending.occurred_at, "2026-09-04T10:00:00.001Z");
        store.record(&key, pending.clone()).unwrap();
        store.record(&key, pending).unwrap();
        assert_eq!(store.load(&key).unwrap().entries.len(), 2);
    }

    #[test]
    fn legacy_path_components_obey_linux_name_max_after_encoding() {
        let root = tempfile::tempdir().unwrap();
        let store = PlayLogStore::new(root.path());
        assert!(store.validate_key(&key(&"a".repeat(250))).is_ok());
        assert!(store.validate_key(&key(&"a".repeat(251))).is_err());
        assert!(store.validate_key(&key(&"é".repeat(41))).is_ok());
        assert!(store.validate_key(&key(&"é".repeat(42))).is_err());
        assert!(store
            .validate_key(&PlayHistoryKey {
                user_id: "é".repeat(43),
                game_id: "wario".into(),
            })
            .is_err());
    }
}
