//! Save-sync wire and record shapes approved for ADR 0014 slice 1.
use serde::{Deserialize, Serialize};
use typeshare::typeshare;

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum SaveKind {
    Save,
    Savestate,
    Screenshot,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SaveLocation {
    pub key: String,
    pub kind: SaveKind,
    pub path: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SaveFile {
    /// Relative to this location, never an absolute filesystem path.
    pub path: String,
    pub sha256: String,
    #[typeshare(serialized_as = "number")]
    pub bytes: u64,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SnapshotLocation {
    pub key: String,
    pub kind: SaveKind,
    pub files: Vec<SaveFile>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotRecord {
    pub parents: Vec<String>,
    pub game_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    pub runner_id: String,
    /// Public key of the device that made this snapshot.
    pub device: String,
    /// UTC RFC 3339 with milliseconds. Never used to decide a split.
    pub ended_at: String,
    #[typeshare(serialized_as = "number")]
    pub duration_seconds: u64,
    pub interrupted: bool,
    pub locations: Vec<SnapshotLocation>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveHeadsRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameSaveHeads {
    pub game_id: String,
    pub heads: Vec<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SaveHeads {
    pub games: Vec<GameSaveHeads>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveRecordsRequest {
    pub game_id: String,
    pub ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records: Option<Vec<SnapshotRecord>>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SaveRecords {
    pub records: Vec<SnapshotRecord>,
    /// Only snapshots whose objects have been verified locally.
    pub confirmed: Vec<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveGameRequest {
    pub game_id: String,
}

/// Untagged on the wire: since exists exactly when inUse is true.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SaveInUse {
    Active(SaveInUseActive),
    Idle(SaveInUseIdle),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveInUseActive {
    pub device: String,
    pub in_use: bool,
    #[typeshare(serialized_as = "number")]
    pub since: u64,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveInUseIdle {
    pub device: String,
    pub in_use: bool,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SaveObjectRequest {
    pub sha256: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum SaveMark {
    Synced,
    Split,
    NotChecked,
    NotSynced,
    SyncedBySteam,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SaveStatusRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameSaveStatus {
    pub game_id: String,
    pub status: SaveMark,
    pub heads: Vec<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SaveStatus {
    pub games: Vec<GameSaveStatus>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveHistoryEntry {
    pub snapshot_id: String,
    pub record: SnapshotRecord,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SaveHistory {
    pub records: Vec<SaveHistoryEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavePickRequest {
    pub game_id: String,
    pub snapshot_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveCheckRequest {
    pub game_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_id: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SaveInUseWarning {
    pub device: String,
    #[typeshare(serialized_as = "number")]
    pub since: u64,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveCheck {
    pub status: SaveMark,
    pub in_use: Vec<SaveInUseWarning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_id: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SaveStatusOutcome {
    Ok(SaveStatus),
    Err(crate::RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SaveHistoryOutcome {
    Ok(SaveHistory),
    Err(crate::RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SaveCheckOutcome {
    Ok(SaveCheck),
    Err(crate::RpcFailure),
}
