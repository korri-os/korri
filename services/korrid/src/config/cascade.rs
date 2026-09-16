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
    let id = &route.runner_id;
    let family = route.family_id.as_deref();
    let mut result = RunnerConfig::default();
    let layers = [
        family.and_then(|family| snapshot.families.get(family)),
        family.and_then(|family| {
            snapshot
                .systems
                .get(&route.system_id)
                .and_then(|v| v.families.get(family))
        }),
        snapshot.runners.get(id),
        snapshot
            .systems
            .get(&route.system_id)
            .and_then(|v| v.runners.get(id)),
        snapshot
            .games
            .get(&route.playable_id)
            .and_then(|v| v.families.get(family.unwrap_or(""))),
        snapshot
            .games
            .get(&route.playable_id)
            .and_then(|v| v.runners.get(id)),
        overrides,
    ];
    for layer in layers.into_iter().flatten() {
        result.settings.extend(layer.settings.clone());
        if let Some(config) = &layer.config {
            let target = result.config.get_or_insert_with(Default::default);
            if config.prepend.is_some() {
                target.prepend = config.prepend.clone();
            }
            if config.append.is_some() {
                target.append = config.append.clone();
            }
            if config.replace.is_some() {
                target.replace = config.replace.clone();
            }
        }
    }
    result
}
