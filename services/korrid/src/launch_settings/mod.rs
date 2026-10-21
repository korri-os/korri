//! Device-only launch limits and game profiles (ADRs 0018–0020).
use crate::{config::settings::RunnerChoiceRevisions, RpcFailure};
use serde::{Deserialize, Serialize};
use typeshare::typeshare;

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "_tag", content = "id")]
pub enum LaunchSettingsScope {
    Device,
    System(String),
    Runner(String),
    Game(String),
    Release {
        #[serde(rename = "gameId")]
        game_id: String,
        #[serde(rename = "releaseId")]
        release_id: String,
    },
    Profile {
        #[serde(rename = "gameId")]
        game_id: String,
        #[serde(rename = "profileId")]
        profile_id: String,
    },
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPercentRange {
    pub min: u8,
    pub max: u8,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSettingsValues {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u8>,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSettingsLayer {
    pub scope: LaunchSettingsScope,
    pub title: String,
    pub cpu: LaunchSettingsValues,
    pub gpu: LaunchSettingsValues,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governor: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResolvedValue {
    pub value: u8,
    pub source: LaunchSettingsScope,
    pub adjusted: bool,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResolvedRange {
    pub min: LaunchResolvedValue,
    pub max: LaunchResolvedValue,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSettingsResolved {
    pub cpu: LaunchResolvedRange,
    pub gpu: LaunchResolvedRange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governor: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSettingsProfile {
    pub id: String,
    pub title: String,
    pub cpu: LaunchSettingsValues,
    pub gpu: LaunchSettingsValues,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchDeviceDefaults {
    pub cpu: LaunchPercentRange,
    pub gpu: LaunchPercentRange,
    pub governor: String,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchCpuPolicy {
    pub name: String,
    pub cpus: Vec<u32>,
    pub cpuinfo_max_freq: u32,
    pub available_frequencies: Vec<u32>,
    pub available_governors: Vec<String>,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchGpuDevfreq {
    pub name: String,
    pub available_frequencies: Vec<u32>,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchDeviceFacts {
    pub defaults: LaunchDeviceDefaults,
    pub cpu_policies: Vec<LaunchCpuPolicy>,
    pub gpu_devfreq: Vec<LaunchGpuDevfreq>,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSettingsSnapshot {
    pub layers: Vec<LaunchSettingsLayer>,
    pub resolved: LaunchSettingsResolved,
    pub profiles: Vec<LaunchSettingsProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_facts: Option<LaunchDeviceFacts>,
    pub revisions: RunnerChoiceRevisions,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSettingsSnapshotRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]

pub enum LaunchSetting {
    #[serde(rename = "cpu.min")]
    CpuMin,
    #[serde(rename = "cpu.max")]
    CpuMax,
    #[serde(rename = "gpu.min")]
    GpuMin,
    #[serde(rename = "gpu.max")]
    GpuMax,
    #[serde(rename = "cpu.governor")]
    CpuGovernor,
}

#[typeshare(serialized_as = "LaunchLimitValueWire")]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LaunchSettingValue {
    Percent(u8),
    Governor(String),
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSettingsSetRequest {
    pub scope: LaunchSettingsScope,
    pub setting: LaunchSetting,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<LaunchSettingValue>,
    pub expected_revision: String,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProfileSaveRequest {
    pub game_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub title: String,
    pub expected_revision: String,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProfileRemoveRequest {
    pub game_id: String,
    pub profile_id: String,
    pub expected_revision: String,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProfileSelectRequest {
    pub game_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub expected_revision: String,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum LaunchSettingsSnapshotOutcome {
    Ok(LaunchSettingsSnapshot),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum LaunchSettingsWriteOutcome {
    Ok(RunnerChoiceRevisions),
    Err(RpcFailure),
}
