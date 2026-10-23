//! One authored place body, grounded in ADR 0028 and the approved slice contract.
use super::{cascade::RunnerConfigs, modifiers::ModifierConfigs, HooksPolicy, NonEmptyString};
use crate::launch_settings::config::{DeviceCpu, Limits};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::Deref};

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Args {
    #[serde(default)]
    pub prepend: Vec<String>,
    #[serde(default)]
    pub append: Vec<String>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct PlaceValues {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::optional_non_null"
    )]
    pub cpu: Option<DeviceCpu>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::optional_non_null"
    )]
    pub gpu: Option<Limits>,
    #[serde(default, skip_serializing_if = "modifiers_empty")]
    pub modifiers: ModifierConfigs,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::optional_non_null"
    )]
    pub cwd: Option<String>,
    #[serde(default)]
    pub args: Args,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::optional_non_null"
    )]
    pub hooks: Option<HooksPolicy>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct PlaceBody {
    #[serde(flatten)]
    pub values: PlaceValues,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::optional_non_null"
    )]
    pub runner: Option<NonEmptyString>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub families: RunnerConfigs,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub runners: RunnerConfigs,
}
impl Deref for PlaceBody {
    type Target = PlaceValues;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl Deref for super::cascade::RunnerConfig {
    type Target = PlaceValues;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

fn modifiers_empty(v: &ModifierConfigs) -> bool {
    v.is_empty()
}

// Serde cannot combine nested flatten with deny_unknown_fields. Validate the
// flattened record boundary explicitly; leaf records retain Serde's validation.
pub(crate) fn validate_document_shape(yaml: &str) -> Result<(), String> {
    use serde_yaml::Value;
    fn known(value: &Value, extra: &[&str], scoped: bool) -> Result<(), String> {
        let Some(map) = value.as_mapping() else {
            return Ok(());
        };
        const VALUES: &[&str] = &["cpu", "gpu", "modifiers", "env", "cwd", "args", "hooks"];
        const ANY: &[&str] = &["runner", "families", "runners"];
        for key in map.keys() {
            let key = key.as_str().ok_or("record keys must be strings")?;
            if !VALUES.contains(&key)
                && !extra.contains(&key)
                && !(if scoped {
                    ["settings", "config"].contains(&key)
                } else {
                    ANY.contains(&key)
                })
            {
                return Err(format!("unknown field `{key}`"));
            }
        }
        if !scoped {
            for kind in ["families", "runners"] {
                if let Some(records) = value.get(kind).and_then(Value::as_mapping) {
                    for v in records.values() {
                        known(v, &[], true)?;
                    }
                }
            }
        }
        Ok(())
    }
    let value: Value = serde_yaml::from_str(yaml).map_err(|e| e.to_string())?;
    if let Some(host) = value.get("host") {
        known(
            host,
            &[
                "title",
                "preferences",
                "relays",
                "moonlight",
                "plugin",
                "patches",
            ],
            false,
        )?;
    }
    if let Some(systems) = value.get("systems").and_then(Value::as_mapping) {
        for system in systems.values() {
            known(
                system,
                &["name", "title", "manufacturer", "aliases", "metadata"],
                false,
            )?;
        }
    }
    if let Some(games) = value.get("library").and_then(Value::as_mapping) {
        for game in games.values() {
            known(
                game,
                &[
                    "title",
                    "version-of",
                    "relation",
                    "contains",
                    "display",
                    "metadata",
                    "releases",
                    "profiles",
                    "profile",
                ],
                false,
            )?;
            if let Some(releases) = game.get("releases").and_then(Value::as_sequence) {
                for release in releases {
                    known(release, &["id", "system", "identity", "display"], false)?;
                }
            }
            if let Some(profiles) = game.get("profiles").and_then(Value::as_mapping) {
                for profile in profiles.values() {
                    known(profile, &["title"], false)?;
                }
            }
        }
    }
    Ok(())
}
