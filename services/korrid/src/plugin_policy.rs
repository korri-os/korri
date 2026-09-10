//! Layered enablement policy for device-local plugins.
//!
//! Registration and enablement stay separate: bundled source registers the
//! plugins available to this korrid build, and semantic policy layers decide
//! which registered plugin ids are enabled. Later layers override earlier ones,
//! and the result gates every contribution category, including session controls.

use std::collections::BTreeMap;

use crate::{
    config::ConfigSnapshot,
    plugin::{load_plugin_source, Plugin, PluginError, PluginRegistry},
};

pub const ANDROID_APP_PLUGIN_ID: &str = "@korri:android-app";
pub const ANDROID_APP_PLUGIN_SOURCE: &str = include_str!("../plugins/android-app.plugin.ts");
pub const MGBA_PLUGIN_ID: &str = "@korri:mgba";
pub const MGBA_PLUGIN_SOURCE: &str = include_str!("../plugins/mgba.plugin.ts");
pub const MOONLIGHT_PLUGIN_ID: &str = "@korri:moonlight";
pub const MOONLIGHT_PLUGIN_SOURCE: &str = include_str!("../plugins/moonlight.plugin.ts");
pub const RETROARCH_PLUGIN_ID: &str = "@korri:retroarch";
pub const RETROARCH_PLUGIN_SOURCE: &str = include_str!("../plugins/retroarch.plugin.ts");

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PluginPolicyLayer {
    entries: BTreeMap<String, bool>,
}

impl PluginPolicyLayer {
    pub fn from_enabled<'a>(entries: impl IntoIterator<Item = (&'a str, bool)>) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|(plugin_id, enabled)| (plugin_id.to_owned(), enabled))
                .collect(),
        }
    }
}

pub fn bundled_plugins() -> Result<Vec<Plugin>, PluginError> {
    // Publisher authority is compiled composition, not a claim in source or
    // an inference from the bundled filename.
    Ok(vec![
        load_plugin_source("@korri", ANDROID_APP_PLUGIN_SOURCE)?,
        load_plugin_source("@korri", MGBA_PLUGIN_SOURCE)?,
        load_plugin_source("@korri", MOONLIGHT_PLUGIN_SOURCE)?,
        load_plugin_source("@korri", RETROARCH_PLUGIN_SOURCE)?,
    ])
}

pub fn bundled_plugin_policy_layer() -> PluginPolicyLayer {
    PluginPolicyLayer::from_enabled([
        (ANDROID_APP_PLUGIN_ID, true),
        (MGBA_PLUGIN_ID, true),
        (MOONLIGHT_PLUGIN_ID, true),
        (RETROARCH_PLUGIN_ID, true),
    ])
}

pub fn empty_user_plugin_policy_layer() -> PluginPolicyLayer {
    PluginPolicyLayer::default()
}

/** The legacy-readable `host.plugin` record is now the user policy layer when
 * its values are booleans. Schema/support validation rejects every other shape
 * before this point. */
pub fn user_plugin_policy_layer(snapshot: &ConfigSnapshot) -> PluginPolicyLayer {
    let entries = snapshot
        .host
        .as_ref()
        .and_then(|host| host.plugin.as_ref())
        .into_iter()
        .flat_map(|plugins| plugins.iter())
        .filter_map(|(id, value)| value.as_bool().map(|enabled| (id.0.as_str(), enabled)));
    PluginPolicyLayer::from_enabled(entries)
}

pub fn enabled_plugin_ids_for_snapshot(
    snapshot: &ConfigSnapshot,
) -> Result<Vec<String>, PluginError> {
    let plugins = bundled_plugins()?;
    let enabled = resolve_enabled_plugin_ids([
        bundled_plugin_policy_layer(),
        user_plugin_policy_layer(snapshot),
    ]);
    // Validation belongs at the policy edge: disabled unknown ids are invalid
    // too, even though PluginRegistry only observes enabled ids.
    let known: std::collections::BTreeSet<&str> =
        plugins.iter().map(|plugin| plugin.id()).collect();
    if let Some(unknown) = snapshot
        .host
        .as_ref()
        .and_then(|host| host.plugin.as_ref())
        .into_iter()
        .flat_map(|values| values.keys())
        .find(|id| !known.contains(id.0.as_str()))
    {
        return Err(PluginError::UnknownEnabledPlugin(unknown.0.clone()));
    }
    Ok(enabled)
}

pub fn registry_for_snapshot(snapshot: &ConfigSnapshot) -> Result<PluginRegistry, PluginError> {
    PluginRegistry::new(
        bundled_plugins()?,
        enabled_plugin_ids_for_snapshot(snapshot)?,
    )
}

pub fn resolve_enabled_plugin_ids(
    layers: impl IntoIterator<Item = PluginPolicyLayer>,
) -> Vec<String> {
    let mut resolved = BTreeMap::new();
    for layer in layers {
        for (plugin_id, enabled) in layer.entries {
            resolved.insert(plugin_id, enabled);
        }
    }

    resolved
        .into_iter()
        .filter_map(|(plugin_id, enabled)| enabled.then_some(plugin_id))
        .collect()
}
