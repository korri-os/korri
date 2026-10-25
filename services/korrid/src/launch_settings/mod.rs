//! One launch-settings cascade (ADRs 0028 and 0029).
use crate::{config::settings::RunnerChoiceRevisions, RpcFailure};
use serde::{Deserialize, Serialize};
use typeshare::typeshare;
pub mod config;
pub mod helper;
pub(crate) mod resolution;
pub mod session;
mod store;
pub mod wire;
pub use wire::*;
pub(crate) mod launch;
pub use launch::{resolve_launch, ResolvedHooks, ResolvedLaunch};
pub use resolution::snapshot;
pub(crate) use store::requires_registry;
pub use store::{remove_profile, save_profile, select_profile, set, set_with_registry};

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaunchPercentRange {
    pub min: u8,
    pub max: u8,
}
#[derive(Clone, Debug)]
pub struct LaunchResolvedValue {
    pub value: u8,
    pub source: LaunchSource,
    pub adjusted: bool,
}
#[derive(Clone, Debug)]
pub struct LaunchResolvedRange {
    pub min: LaunchResolvedValue,
    pub max: LaunchResolvedValue,
}
#[derive(Clone, Debug)]
pub struct LaunchSettingsResolved {
    pub cpu: LaunchResolvedRange,
    pub gpu: LaunchResolvedRange,
    pub governor: Option<String>,
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchSettingsSnapshotRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchProfileSaveRequest {
    pub game_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub title: String,
    pub expected_revision: String,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchProfileRemoveRequest {
    pub game_id: String,
    pub profile_id: String,
    pub expected_revision: String,
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    Ok(Box<LaunchSnapshot>),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum LaunchSettingsWriteOutcome {
    Ok(RunnerChoiceRevisions),
    Err(RpcFailure),
}
/// Resolved values use only the layers present when a route is not selected.
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "_tag", content = "message")]
pub enum LaunchSettingsRouteLayers {
    Present,
    NeedsRunnerChoice,
    Unavailable(String),
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod snapshot_defaults_tests;
