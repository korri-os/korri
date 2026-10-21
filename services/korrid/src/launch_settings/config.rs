//! Owner-approved schema: host CPU governor; percent limits at all six layers.
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "min"
    )]
    pub min: Option<u8>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "max"
    )]
    pub max: Option<u8>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceCpu {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "min"
    )]
    pub min: Option<u8>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "max"
    )]
    pub max: Option<u8>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::config::optional_non_null"
    )]
    pub governor: Option<crate::config::NonEmptyString>,
}
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub title: crate::config::NonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::config::optional_non_null"
    )]
    pub cpu: Option<Limits>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::config::optional_non_null"
    )]
    pub gpu: Option<Limits>,
}
fn percent<'de, D: Deserializer<'de>>(d: D, lower: u8) -> Result<Option<u8>, D::Error> {
    let n = u8::deserialize(d)?;
    if !(lower..=100).contains(&n) {
        return Err(serde::de::Error::custom(format!(
            "percent must be an integer from {lower} to 100"
        )));
    }
    Ok(Some(n))
}
fn min<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u8>, D::Error> {
    percent(d, 0)
}
fn max<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u8>, D::Error> {
    percent(d, 1)
}

pub fn conflict(kind: &str, layer: &str, min: Option<u8>, max: Option<u8>) -> Option<String> {
    match (min,max) {
        (Some(min),Some(max)) if min > max => Some(format!("{kind} min {min} % is above {kind} max {max} % on {layer}; lower the min or raise the max")),
        _ => None,
    }
}
pub fn validate(
    snapshot: &crate::config::ConfigSnapshot,
) -> Result<(), crate::config::ConfigSchemaError> {
    fn check(
        layer: &str,
        cpu: Option<&Limits>,
        gpu: Option<&Limits>,
    ) -> Result<(), crate::config::ConfigSchemaError> {
        for (kind, limits) in [("CPU", cpu), ("GPU", gpu)] {
            if let Some(v) = limits {
                if let Some(message) = conflict(kind, layer, v.min, v.max) {
                    return Err(crate::config::ConfigSchemaError::Invalid {
                        path: layer.into(),
                        message,
                    });
                }
            }
        }
        Ok(())
    }
    if let Some(host) = &snapshot.host {
        let cpu = host.cpu.as_ref().map(|c| Limits {
            min: c.min,
            max: c.max,
        });
        check("Device", cpu.as_ref(), host.gpu.as_ref())?;
    }
    for (id, s) in &snapshot.systems {
        check(&format!("System {id}"), s.cpu.as_ref(), s.gpu.as_ref())?;
    }
    for (id, r) in &snapshot.runners {
        check(&format!("Runner {id}"), r.cpu.as_ref(), r.gpu.as_ref())?;
    }
    for (id, g) in &snapshot.library {
        check(&format!("Game {id}"), g.cpu.as_ref(), g.gpu.as_ref())?;
        for r in &g.releases {
            check(
                &format!("Release {}", r.id.0),
                r.inheritable.cpu.as_ref(),
                r.inheritable.gpu.as_ref(),
            )?;
        }
        for (id, p) in &g.profiles {
            if id.is_empty() {
                return Err(crate::config::ConfigSchemaError::Invalid {
                    path: "profiles".into(),
                    message: "profile ID must not be empty".into(),
                });
            }
            check(&format!("Profile {id}"), p.cpu.as_ref(), p.gpu.as_ref())?;
        }
        if g.profile
            .as_ref()
            .is_some_and(|p| !g.profiles.contains_key(&p.0))
        {
            return Err(crate::config::ConfigSchemaError::Invalid {
                path: format!("Game {id}"),
                message: "selected profile does not exist".into(),
            });
        }
    }
    Ok(())
}
