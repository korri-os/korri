//! The approved per-ID launcher → system → runtime → game → override cascade.
//! Settings are legacy LaunchSettings (shallow, per-key last wins). Raw config
//! retains legacy LaunchOverrides.config prepend/append; kinds never participate.
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
pub struct LauncherConfig {
    #[serde(default)]
    pub settings: HashMap<String, LaunchSettingValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<LaunchConfigOverrides>,
}

pub type LauncherConfigs = std::collections::BTreeMap<String, LauncherConfig>;

pub fn resolve(
    snapshot: &ConfigSnapshot,
    route: &ResolvedRoute,
    overrides: Option<&LauncherConfig>,
) -> LauncherConfig {
    let id = &route.launcher_id;
    let mut result = LauncherConfig::default();
    for layer in [
        snapshot.launchers.get(id).and_then(|v| v.launchers.get(id)),
        snapshot
            .systems
            .get(&route.system_id)
            .and_then(|v| v.launchers.get(id)),
        route
            .runtime
            .as_ref()
            .and_then(|runtime| snapshot.runtimes.get(&runtime.id))
            .and_then(|v| v.launchers.get(id)),
        snapshot
            .games
            .get(&route.playable_id)
            .and_then(|v| v.launchers.get(id)),
        overrides,
    ]
    .into_iter()
    .flatten()
    {
        result.settings.extend(layer.settings.clone());
        if let Some(config) = &layer.config {
            let target = result.config.get_or_insert_with(Default::default);
            // Legacy raw blocks are scalar overrides, not text concatenation.
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
