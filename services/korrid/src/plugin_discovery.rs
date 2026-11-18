//! The closed first-party discovery vocabulary shared by runtime and admission.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// ADR 0083: the Steam plugin is found by its short name, in any publisher
/// namespace (`@<namespace>:steam`), and every id it owns is derived from
/// its own plugin id `P` by ADR 0045's `<pluginId>/<key>`.
pub const STEAM_NAME: &str = "steam";
/// `P/installed-apps`: discovery of the apps Steam installed.
pub const INSTALLED_APPS_KEY: &str = "installed-apps";

/// True for a plugin id `@<namespace>:steam`.
pub fn is_steam_plugin(id: &str) -> bool {
    id.strip_prefix('@')
        .and_then(|rest| rest.split_once(':'))
        .is_some_and(|(namespace, name)| !namespace.is_empty() && name == STEAM_NAME)
}

/// The Steam plugin `P` of a discovery provider id `P/installed-apps`.
pub fn steam_installed_apps_plugin(id: &str) -> Option<&str> {
    let (plugin, key) = id.split_once('/')?;
    (key == INSTALLED_APPS_KEY && is_steam_plugin(plugin)).then_some(plugin)
}

/// The test fixtures' Steam discovery provider.
#[cfg(test)]
pub const STEAM_INSTALLED_APPS: &str = "@korri:steam/installed-apps";
/// ADR 0054: apps installed on the Android device. The input is the app list
/// PackageManager produces, not files, so it has no location glob.
pub const ANDROID_INSTALLED_APPS: &str = "@korri:android-app/installed-apps";

/// A Steam plugin's discovery key -> glob relative to the existing account
/// root (ADR 0011). One provider may have several folders; ADR 0083 adds
/// the engine runner's. This is not plugin configuration or a general
/// filesystem permission.
pub const LOCATIONS: &[(&str, &str)] = &[
    (INSTALLED_APPS_KEY, "steam/steamapps/appmanifest_*.acf"),
    (
        INSTALLED_APPS_KEY,
        "steam-engine/steam/steamapps/appmanifest_*.acf",
    ),
];

/// Legacy ReleaseDiscoveryProvider data; executable code lives in handlers.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseDiscoveryProvider {
    pub id: String,
    pub title: String,
}

/// ADR 0045: a discovery entry may name its local key; its id is then
/// `<owner>/<key>`. A full id is kept as it is. Both the korrid loader and the
/// plugin host validate the id this gives.
pub fn resolve_id(owner: &str, id: &str) -> String {
    if !id.is_empty() && !id.contains([':', '/', '@']) {
        format!("{owner}/{id}")
    } else {
        id.to_owned()
    }
}

pub fn validate(owner: &str, providers: &[ReleaseDiscoveryProvider]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for provider in providers {
        let id = resolve_id(owner, &provider.id);
        let known = (is_steam_plugin(owner) && steam_installed_apps_plugin(&id) == Some(owner))
            || (owner == "@korri:android-app" && id == ANDROID_INSTALLED_APPS);
        if !known || provider.title.trim().is_empty() || !seen.insert(id.clone()) {
            return Err(format!("invalid discovery provider {}: unknown first-party provider, empty title or duplicate id", provider.id));
        }
    }
    Ok(())
}
