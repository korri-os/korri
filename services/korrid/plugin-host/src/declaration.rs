use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    #[serde(skip_deserializing)]
    pub namespace: String,
    pub name: String,
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub title: Option<String>,
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<String>,
    #[serde(default)]
    pub services: Vec<String>,
    // These are the existing named data exports, not a second plugin schema.
    // Admission retains their values; the registry owns their meaning. Explicit
    // empty records remain present. No data contribution becomes permission.
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub providers: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub systems: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub families: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub runners: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub transports: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(
        default,
        rename = "sessionControls",
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub session_controls: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub discovery: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub android: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub config: Option<BTreeMap<String, serde_json::Value>>,
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && (name.as_bytes()[0].is_ascii_lowercase() || name.as_bytes()[0].is_ascii_digit())
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-_.".contains(&c))
}

pub fn validate_id(id: &str) -> Result<(), String> {
    let (namespace, name) = id
        .split_once(':')
        .ok_or("plugin ID must be @namespace:name")?;
    if !namespace.strip_prefix('@').is_some_and(valid_name) || !valid_name(name) {
        return Err("plugin ID must be @namespace:name with lowercase names".into());
    }
    Ok(())
}

impl Declaration {
    pub fn evaluate(namespace: &str, source: &str) -> Result<Self, String> {
        Self::evaluate_snapshot(
            namespace,
            &crate::script::source::SourceSnapshot::plugin(source)?,
        )
    }

    pub fn evaluate_snapshot(
        namespace: &str,
        source: &crate::script::source::SourceSnapshot,
    ) -> Result<Self, String> {
        let json = crate::script::eval_plugin_snapshot(source)?;
        let mut declaration: Self =
            serde_json::from_str(&json).map_err(|e| format!("invalid plugin declaration: {e}"))?;
        declaration.namespace = namespace.to_owned();
        declaration.validate()?;
        Ok(declaration)
    }

    fn validate(&self) -> Result<(), String> {
        validate_id(&self.id())?;
        if self
            .title
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 128 || s.chars().any(char::is_control))
        {
            return Err("plugin title must be 1 to 128 bytes without control characters".into());
        }
        if self
            .description
            .as_ref()
            .is_some_and(|s| s.len() > 1024 || s.chars().any(char::is_control))
        {
            return Err(
                "plugin description must be at most 1024 bytes without control characters".into(),
            );
        }
        crate::native_unit::validate_names(&self.services)?;
        if self.services.len() > 3 {
            return Err("a plugin may request at most three native units".into());
        }
        Ok(())
    }

    pub fn id(&self) -> String {
        format!("{}:{}", self.namespace, self.name)
    }
}

fn optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
