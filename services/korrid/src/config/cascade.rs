//! Authored runner settings and native config; resolution lives in launch_settings.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::launcher::plugin_launch::LaunchConfigOverrides;

#[typeshare(serialized_as = "LaunchSettingNested")]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum LaunchSettingValue {
    Boolean(bool),
    Number(serde_json::Number),
    String(String),
    Section(HashMap<String, LaunchSettingValue>),
    Cleared,
}

#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
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

/// Walk authored nested settings. Dots are path separators, never file names.
pub fn leaves(
    settings: &HashMap<String, LaunchSettingValue>,
) -> (Vec<(String, LaunchSettingValue)>, Vec<String>) {
    fn walk(
        tree: &HashMap<String, LaunchSettingValue>,
        prefix: &str,
        leaves: &mut Vec<(String, LaunchSettingValue)>,
        skipped: &mut Vec<String>,
    ) {
        for (name, value) in tree {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            if name.contains('.') || name.is_empty() {
                skipped.push(path);
                continue;
            }
            match value {
                LaunchSettingValue::Section(tree) => walk(tree, &path, leaves, skipped),
                value => leaves.push((path, value.clone())),
            }
        }
    }
    let (mut values, mut skipped) = (Vec::new(), Vec::new());
    walk(settings, "", &mut values, &mut skipped);
    (values, skipped)
}

pub fn set_leaf(
    tree: &mut HashMap<String, LaunchSettingValue>,
    path: &str,
    value: Option<LaunchSettingValue>,
) {
    match path.split_once('.') {
        None => {
            if let Some(value) = value {
                tree.insert(path.into(), value);
            } else {
                tree.remove(path);
            }
        }
        Some((head, tail)) => {
            if value.is_none() && !tree.contains_key(head) {
                return;
            }
            let section = tree
                .entry(head.into())
                .or_insert_with(|| LaunchSettingValue::Section(HashMap::new()));
            if !matches!(section, LaunchSettingValue::Section(_)) {
                *section = LaunchSettingValue::Section(HashMap::new());
            }
            let LaunchSettingValue::Section(section) = section else {
                unreachable!()
            };
            set_leaf(section, tail, value);
            if section.is_empty() {
                tree.remove(head);
            }
        }
    }
}

pub fn schema_leaf<'a>(schema: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut property = schema;
    for segment in path.split('.') {
        property = property.get("properties")?.get(segment)?;
    }
    (property.get("type").and_then(serde_json::Value::as_str) != Some("object")).then_some(property)
}

pub fn leaf<'a>(
    tree: &'a HashMap<String, LaunchSettingValue>,
    path: &str,
) -> Option<&'a LaunchSettingValue> {
    match path.split_once('.') {
        None => tree.get(path),
        Some((head, tail)) => match tree.get(head)? {
            LaunchSettingValue::Section(section) => leaf(section, tail),
            _ => None,
        },
    }
}
