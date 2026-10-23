//! Authored runner settings and native config; resolution lives in launch_settings.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::launcher::plugin_launch::LaunchConfigOverrides;

#[typeshare(serialized_as = "LaunchSettingScalar")]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum LaunchSettingValue {
    Boolean(bool),
    Number(serde_json::Number),
    String(String),
}

#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunnerConfig {
    #[serde(flatten)]
    #[typeshare(skip)]
    pub values: super::place::PlaceValues,
    #[serde(default)]
    pub settings: HashMap<String, LaunchSettingValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<LaunchConfigOverrides>,
}

pub type RunnerConfigs = std::collections::BTreeMap<String, RunnerConfig>;
