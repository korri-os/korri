//! The approved per-ID family → system → runner → game → override cascade.
//! Settings are legacy LaunchSettings (shallow, per-key last wins). Raw config
//! retains legacy LaunchOverrides.config prepend/append; family defaults participate before runner settings.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use super::{resolver::ResolvedRoute, ConfigSnapshot};
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
#[serde(deny_unknown_fields)]
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

pub fn resolve(
    snapshot: &ConfigSnapshot,
    route: &ResolvedRoute,
    overrides: Option<&RunnerConfig>,
) -> RunnerConfig {
    let launch = overrides.map(|v| crate::launch_settings::LaunchOverride {
        settings: Some(crate::launch_settings::LaunchScopedSettings {
            scope: crate::launch_settings::LaunchRunnerScope::Runner(route.runner_id.clone()),
            values: v.settings.clone(),
        }),
        ..Default::default()
    });
    crate::launch_settings::resolve_launch(snapshot, route, launch.as_ref(), None)
        .expect("validated launch settings")
        .runner
}
