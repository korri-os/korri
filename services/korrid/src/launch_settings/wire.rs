//! The place and source treaties for ADR 0028. File-only values have no write path.
use crate::config::settings::RunnerChoiceRevisions;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use typeshare::typeshare;

#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "_tag", content = "id")]
pub enum LaunchLayer {
    Device,
    System(String),
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
    Launch,
}
#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "_tag", content = "id")]
pub enum LaunchRunnerScope {
    Any,
    Family(String),
    Runner(String),
}
#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchPlace {
    pub layer: LaunchLayer,
    pub scope: LaunchRunnerScope,
}
#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum LaunchSource {
    Place(LaunchPlace),
    PluginDefault {
        #[serde(rename = "pluginId")]
        plugin_id: String,
    },
    DeviceDefault,
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "_tag", content = "payload", deny_unknown_fields)]
pub enum LaunchPath {
    Runner,
    CpuMin,
    CpuMax,
    CpuGovernor,
    GpuMin,
    GpuMax,
    ModifierEnabled { id: String },
    ModifierSetting { id: String, key: String },
    Setting { key: String },
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchSetRequest {
    pub place: LaunchPlace,
    pub path: LaunchPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[typeshare(serialized_as = "Option<LaunchJsonValue>")]
    pub value: Option<serde_json::Value>,
    pub expected_revision: String,
}
#[typeshare]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchArgs {
    #[serde(default)]
    pub prepend: Vec<String>,
    #[serde(default)]
    pub append: Vec<String>,
}
#[typeshare]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u8>,
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchScopedSettings {
    pub scope: LaunchRunnerScope,
    pub values: HashMap<String, crate::config::cascade::LaunchSettingValue>,
}
#[typeshare]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<LaunchScopedSettings>,
    #[serde(default)]
    pub modifiers: HashMap<String, LaunchModifierValues>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu: Option<LaunchLimits>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu: Option<LaunchLimits>,
}
#[typeshare]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchModifierValues {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub settings: HashMap<String, crate::config::cascade::LaunchSettingValue>,
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum LaunchNotice {
    RunnerUnavailable {
        #[serde(rename = "runnerId")]
        runner_id: String,
        place: LaunchPlace,
    },
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPlaceSnapshot {
    pub place: LaunchPlace,
    pub title: String,
    #[typeshare(serialized_as = "LaunchJsonValue")]
    pub body: serde_json::Value,
    pub editable_kinds: Vec<String>,
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSourcedValue {
    pub path: LaunchValuePath,
    #[typeshare(serialized_as = "LaunchJsonValue")]
    pub value: serde_json::Value,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub source: Option<LaunchSource>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub contributions: Option<Vec<LaunchContribution>>,
    pub adjusted: bool,
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResolvedSnapshot {
    /// Keys are the authored value paths, including file-only kinds.
    pub values: HashMap<String, LaunchSourcedValue>,
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPrepareFacts {
    pub env: HashMap<String, String>,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum LaunchRunnerFacts {
    Ready(LaunchPrepareFacts),
    Unavailable(String),
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRunnerSnapshot {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    pub title: String,
    pub selected: bool,
    #[typeshare(serialized_as = "LaunchJsonValue")]
    pub schema: serde_json::Value,
    pub revision: String,
    pub prepare: LaunchRunnerFacts,
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaunchProfileSnapshot {
    pub id: String,
    pub title: String,
    #[typeshare(serialized_as = "LaunchJsonValue")]
    pub body: serde_json::Value,
}
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSnapshot {
    pub places: Vec<LaunchPlaceSnapshot>,
    pub resolved: LaunchResolvedSnapshot,
    pub runners: Vec<LaunchRunnerSnapshot>,
    pub modifiers: Vec<LaunchModifierSnapshot>,
    pub profiles: Vec<LaunchProfileSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_facts: Option<super::LaunchDeviceFacts>,
    pub notices: Vec<LaunchNotice>,
    pub route_layers: super::LaunchSettingsRouteLayers,
    pub revisions: RunnerChoiceRevisions,
}

/// Opaque JSON is decoded against the runner schema or the authored place schema.
#[typeshare(serialized_as = "unknown")]
pub type LaunchJsonValue = serde_json::Value;

/// Structured keys remove ambiguity when a plugin ID or setting contains dots.
#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum LaunchValuePath {
    Editable(LaunchPath),
    Env { key: String },
    ArgsPrepend,
    ArgsAppend,
    Cwd,
    HooksBefore,
    HooksAfter,
    Config,
    ModifierConfig { id: String },
}

#[typeshare]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchModifierSnapshot {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[typeshare(serialized_as = "Option<LaunchJsonValue>")]
    pub schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

#[typeshare]
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct LaunchContribution {
    pub source: LaunchSource,
    #[typeshare(serialized_as="LaunchJsonValue")]
    pub value: serde_json::Value,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub routine: Option<String>,
}
