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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
