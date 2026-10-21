//! Canonical immutable records: all devices hash the same bytes.
use super::{types::*, SaveError};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Component, Path},
};

pub fn hash(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}
pub fn hash_hex(value: &str) -> Result<&str, SaveError> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| invalid("hash prefix"))?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("hash"));
    }
    Ok(hex)
}
pub(crate) fn invalid(message: &str) -> SaveError {
    SaveError::Invalid(message.into())
}
pub(crate) fn relative_file(path: &str) -> Result<(), SaveError> {
    if path.is_empty()
        || path.contains('\0')
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(invalid("file path must be relative without traversal"));
    }
    Ok(())
}
pub fn canonical_record(record: &SnapshotRecord) -> Result<Vec<u8>, SaveError> {
    let mut record = record.clone();
    if record.game_id.is_empty() || record.runner_id.is_empty() {
        return Err(invalid("game or runner identity"));
    }
    super::account_root(Path::new("/"), &record.device).map_err(SaveError::Invalid)?;
    let time = chrono::DateTime::parse_from_rfc3339(&record.ended_at)
        .map_err(|_| invalid("snapshot end time"))?;
    if time
        .to_utc()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        != record.ended_at
    {
        return Err(invalid("end time must be canonical UTC milliseconds"));
    }
    for parent in &record.parents {
        hash_hex(parent)?;
    }
    record.parents.sort();
    record.parents.dedup();
    record.locations.sort_by(|a, b| a.key.cmp(&b.key));
    let mut keys = BTreeSet::new();
    for location in &mut record.locations {
        if location.key.is_empty() || !keys.insert(location.key.clone()) {
            return Err(invalid("location key must be nonempty and unique"));
        }
        location.files.sort_by(|a, b| a.path.cmp(&b.path));
        let mut paths = BTreeSet::new();
        for file in &location.files {
            relative_file(&file.path)?;
            hash_hex(&file.sha256)?;
            if !paths.insert(&file.path) {
                return Err(invalid("duplicate location file"));
            }
        }
    }
    fn sorted(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let ordered: std::collections::BTreeMap<_, _> = map.into_iter().collect();
                serde_json::Value::Object(
                    ordered.into_iter().map(|(k, v)| (k, sorted(v))).collect(),
                )
            }
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(sorted).collect())
            }
            other => other,
        }
    }
    serde_json::to_vec(&sorted(
        serde_json::to_value(record).map_err(|e| invalid(&e.to_string()))?,
    ))
    .map_err(|e| invalid(&e.to_string()))
}
pub fn snapshot_id(record: &SnapshotRecord) -> Result<String, SaveError> {
    Ok(hash(&canonical_record(record)?))
}
