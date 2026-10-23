//! Pure launch resolution shared by preparation, limits and hooks.
use super::{wire::*, LaunchSettingsResolved};
use crate::config::{
    cascade::RunnerConfig, modifiers::ModifierConfigs, resolver::ResolvedRoute, ConfigSnapshot,
    HookAfterStep, HookBeforeStep,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResolvedHooks {
    /// Merge order. The host executes after steps in reverse order.
    pub before: Vec<HookBeforeStep>,
    pub after: Vec<HookAfterStep>,
}
#[derive(Clone, Debug)]
pub struct ResolvedLaunch {
    /// Only authored settings are passed to the runner, not schema defaults.
    pub runner: RunnerConfig,
    pub modifiers: ModifierConfigs,
    pub env: BTreeMap<String, Option<String>>,
    pub args: LaunchArgs,
    pub cwd: Option<String>,
    pub hooks: ResolvedHooks,
    /// Device-any CPU is excluded; these limits apply to the session.
    pub limits: LaunchSettingsResolved,
    pub values: LaunchResolvedSnapshot,
}

pub fn resolve_launch(
    snapshot: &ConfigSnapshot,
    route: &ResolvedRoute,
    launch: Option<&LaunchOverride>,
    facts: Option<&super::LaunchDeviceFacts>,
) -> Result<ResolvedLaunch, crate::config::settings::SettingsError> {
    let _ = (snapshot, route, launch, facts);
    todo!("the next engine commit implements the single cascade")
}
