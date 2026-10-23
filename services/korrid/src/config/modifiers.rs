//! Authored modifier values; the launch-settings resolver owns their order.
use super::{cascade::LaunchSettingValue, optional_non_null};
use indexmap::IndexMap;
use serde::{
    de::{Error, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{
    collections::HashMap,
    fmt,
    ops::{Deref, DerefMut},
};
use typeshare::typeshare;

#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModifierConfigAppend {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub append: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModifierConfig {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub settings: HashMap<String, LaunchSettingValue>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub config: Option<ModifierConfigAppend>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct ModifierConfigs(IndexMap<String, ModifierConfig>);
impl Deref for ModifierConfigs {
    type Target = IndexMap<String, ModifierConfig>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for ModifierConfigs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl<'de> Deserialize<'de> for ModifierConfigs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Records;
        impl<'de> Visitor<'de> for Records {
            type Value = ModifierConfigs;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a modifier contribution-ID map")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut records = IndexMap::new();
                while let Some((id, record)) = map.next_entry::<String, ModifierConfig>()? {
                    let valid = id.split_once('/').is_some_and(|(owner, local)| {
                        super::validate_provider_id("modifier", owner).is_ok()
                            && super::catalog::valid_local_id(local)
                    });
                    if !valid {
                        return Err(A::Error::custom(format!("modifier key {id:?} must be a contribution ID like '@korri:example/main'")));
                    }
                    if records.insert(id.clone(), record).is_some() {
                        return Err(A::Error::custom(format!("duplicate modifier key {id}")));
                    }
                }
                Ok(ModifierConfigs(records))
            }
        }
        deserializer.deserialize_map(Records)
    }
}
