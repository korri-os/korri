//! The closed first-party discovery vocabulary shared by runtime and admission.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const STEAM_INSTALLED_APPS: &str = "@korri:steam/installed-apps";

/// Provider ID -> glob relative to the existing account root (ADR 0011).
/// This is not plugin configuration or a general filesystem permission.
pub const LOCATIONS: &[(&str, &str)] =
    &[(STEAM_INSTALLED_APPS, "steam/steamapps/appmanifest_*.acf")];

/// Legacy ReleaseDiscoveryProvider data; executable code lives in handlers.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseDiscoveryProvider {
    pub id: String,
    pub title: String,
}

pub fn validate(owner: &str, providers: &[ReleaseDiscoveryProvider]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for provider in providers {
        if owner != "@korri:steam"
            || !LOCATIONS.iter().any(|(id, _)| *id == provider.id)
            || provider.title.trim().is_empty()
            || !seen.insert(&provider.id)
        {
            return Err(format!("invalid discovery provider {}: unknown first-party provider, empty title or duplicate id", provider.id));
        }
    }
    Ok(())
}
