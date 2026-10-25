//! Authored runner settings and native config; resolution lives in launch_settings.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::launcher::plugin_launch::LaunchConfigOverrides;

#[typeshare(serialized_as = "LaunchSettingNested")]
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum LaunchSettingValue {
    Boolean(bool),
    Number(serde_json::Number),
    String(String),
    Section(HashMap<String, LaunchSettingValue>),
    Cleared,
    /// Invalid authored data is retained for place inspection, but never folded
    /// into plugin input. This is not a setting kind on the treaty.
    Invalid(#[serde(serialize_with = "serialize_invalid")] serde_yaml::Value),
}

// Custom YAML tags remain file syntax errors in the outer flattened-record
// decoder. Tolerant settings decoding covers JSON-Schema values, not YAML tags.
impl<'de> Deserialize<'de> for LaunchSettingValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        fn decode(value: serde_yaml::Value) -> LaunchSettingValue {
            use serde_yaml::Value as Y;
            match value {
                Y::Null => LaunchSettingValue::Cleared,
                Y::Bool(value) => LaunchSettingValue::Boolean(value),
                Y::String(value) => LaunchSettingValue::String(value),
                Y::Number(value) => {
                    let number = value
                        .as_i64()
                        .map(serde_json::Number::from)
                        .or_else(|| value.as_u64().map(serde_json::Number::from))
                        .or_else(|| value.as_f64().and_then(serde_json::Number::from_f64));
                    number
                        .map(LaunchSettingValue::Number)
                        .unwrap_or_else(|| LaunchSettingValue::Invalid(Y::Number(value)))
                }
                Y::Mapping(map) if map.keys().all(|key| key.as_str().is_some()) => {
                    LaunchSettingValue::Section(
                        map.into_iter()
                            .map(|(key, value)| (key.as_str().unwrap().to_owned(), decode(value)))
                            .collect(),
                    )
                }
                value => LaunchSettingValue::Invalid(value),
            }
        }
        Ok(decode(serde_yaml::Value::deserialize(deserializer)?))
    }
}

fn serialize_invalid<S: serde::Serializer>(
    value: &serde_yaml::Value,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    // Place inspection is JSON. Non-JSON YAML stays inspectable as authored text;
    // the original document remains untouched, and no invalid leaf reaches a plugin.
    let json = serde_json::to_value(value).ok().filter(|v| !v.is_null());
    match json {
        Some(value) => value.serialize(serializer),
        None => serde_yaml::to_string(value)
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer),
    }
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

/// A literal property wins over the first matching pattern, as RetroArch's
/// validator does. JSON Schema patterns match substrings unless anchored.
pub fn schema_child<'a>(
    schema: &'a serde_json::Value,
    name: &str,
) -> Option<&'a serde_json::Value> {
    schema
        .get("properties")
        .and_then(|properties| properties.get(name))
        .or_else(|| {
            schema
                .get("patternProperties")?
                .as_object()?
                .iter()
                .find_map(|(pattern, property)| {
                    regex::Regex::new(pattern)
                        .ok()
                        .filter(|pattern| pattern.is_match(name))
                        .map(|_| property)
                })
        })
}

pub fn schema_leaf<'a>(schema: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut property = schema;
    for segment in path.split('.') {
        property = schema_child(property, segment)?;
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
