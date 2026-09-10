use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
    pub daemons: Vec<Daemon>,
}

// These field names are systemd's service contract, not a new permission
// vocabulary. Only properties required by the first real daemon are accepted.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Daemon {
    #[serde(rename = "Type")]
    pub service_type: ServiceType,
    #[serde(rename = "ExecStart")]
    pub start: Vec<String>,
    #[serde(
        rename = "ExecStopPost",
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub cleanup: Option<Vec<String>>,
    #[serde(rename = "CapabilityBoundingSet")]
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceType {
    Notify,
    Exec,
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
        let json = crate::script::eval_plugin_ts(source)?;
        let mut declaration: Self =
            serde_json::from_str(&json).map_err(|e| format!("invalid daemon declaration: {e}"))?;
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
        if self.daemons.len() != 1 {
            return Err("this host supports exactly one daemon per plugin".into());
        }
        let daemon = &self.daemons[0];
        validate_command(&daemon.start)?;
        if let Some(cleanup) = &daemon.cleanup {
            validate_command(cleanup)?;
        }
        let mut seen = BTreeSet::new();
        for capability in &daemon.capabilities {
            if !matches!(capability.as_str(), "CAP_NET_ADMIN" | "CAP_NET_RAW")
                || !seen.insert(capability)
            {
                return Err(format!(
                    "unsupported or repeated Linux capability: {capability}"
                ));
            }
        }
        Ok(())
    }

    pub fn id(&self) -> String {
        format!("{}:{}", self.namespace, self.name)
    }
    pub fn host_network_admin(&self) -> bool {
        self.daemons[0]
            .capabilities
            .iter()
            .any(|c| c == "CAP_NET_ADMIN")
    }
}

fn optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

fn validate_command(command: &[String]) -> Result<(), String> {
    if command.is_empty() || command.len() > 32 {
        return Err("daemon command must contain 1 to 32 arguments".into());
    }
    let program = command[0]
        .strip_prefix("bin/")
        .ok_or("executable must be bin/<program>")?;
    if program.is_empty()
        || !program
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        || program == "."
        || program == ".."
    {
        return Err("executable must be bin/<program> without path traversal".into());
    }
    for arg in command {
        if arg.len() > 4096 || arg.chars().any(char::is_control) || arg.contains('%') {
            return Err(
                "daemon arguments cannot contain control characters or systemd specifiers".into(),
            );
        }
        let rest = arg
            .replace("${STATE_DIRECTORY}", "")
            .replace("${RUNTIME_DIRECTORY}", "");
        if rest.contains('$') {
            return Err(
                "only STATE_DIRECTORY and RUNTIME_DIRECTORY substitution is supported".into(),
            );
        }
    }
    Ok(())
}
