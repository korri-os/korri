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
pub struct Profile {
    pub title: crate::config::NonEmptyString,
    #[serde(flatten)]
    pub place: crate::config::place::PlaceBody,
}
impl std::ops::Deref for Profile {
    type Target = crate::config::place::PlaceBody;
    fn deref(&self) -> &Self::Target {
        &self.place
    }
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
    use crate::config::{
        place::{PlaceBody, PlaceValues},
        ConfigSchemaError,
    };
    fn check(v: &PlaceValues, path: &str, host: bool) -> Result<(), ConfigSchemaError> {
        for (kind, min, max) in [
            (
                "CPU",
                v.cpu.as_ref().and_then(|v| v.min),
                v.cpu.as_ref().and_then(|v| v.max),
            ),
            (
                "GPU",
                v.gpu.as_ref().and_then(|v| v.min),
                v.gpu.as_ref().and_then(|v| v.max),
            ),
        ] {
            if let Some(message) = conflict(kind, path, min, max) {
                return Err(ConfigSchemaError::Invalid {
                    path: path.into(),
                    message,
                });
            }
        }
        if !host && v.cpu.as_ref().is_some_and(|v| v.governor.is_some()) {
            return Err(ConfigSchemaError::Invalid {
                path: path.into(),
                message: "CPU governor belongs only to device-any".into(),
            });
        }
        if v.env.iter().any(|(k, v)| {
            k.is_empty() || k.contains(['=', '\0']) || v.as_ref().is_some_and(|v| v.contains('\0'))
        }) || v.cwd.as_ref().is_some_and(|v| v.contains('\0'))
            || v.args
                .prepend
                .iter()
                .chain(&v.args.append)
                .any(|v| v.contains('\0'))
        {
            return Err(ConfigSchemaError::Invalid {
                path: path.into(),
                message: "invalid environment, argument or working directory".into(),
            });
        }
        if let Some(hooks) = &v.hooks {
            for name in hooks.use_profiles.iter().flatten() {
                if name.0.is_empty() {
                    return Err(ConfigSchemaError::Invalid {
                        path: path.into(),
                        message: "routine name is empty".into(),
                    });
                }
            }
        }
        Ok(())
    }
    fn place(
        v: &PlaceBody,
        path: &str,
        host: bool,
        snapshot: &crate::config::ConfigSnapshot,
    ) -> Result<(), ConfigSchemaError> {
        check(v, path, host)?;
        for (kind, records) in [("families", &v.families), ("runners", &v.runners)] {
            for (id, v) in records {
                crate::config::place::validate_scope_id(kind, id).map_err(|message| {
                    ConfigSchemaError::Invalid {
                        path: path.into(),
                        message,
                    }
                })?;
                check(v, &format!("{path}.{kind}.{id}"), false)?;
            }
        }
        for v in std::iter::once(&v.values)
            .chain(v.families.values().map(|v| &v.values))
            .chain(v.runners.values().map(|v| &v.values))
        {
            for name in v
                .hooks
                .as_ref()
                .and_then(|h| h.use_profiles.as_ref())
                .into_iter()
                .flatten()
            {
                if !snapshot.routines.contains_key(&name.0) {
                    return Err(ConfigSchemaError::Invalid {
                        path: path.into(),
                        message: format!("unknown routine {}", name.0),
                    });
                }
            }
        }
        Ok(())
    }
    if let Some(host) = &snapshot.host {
        place(&host.place, "host", true, snapshot)?;
    }
    for (id, s) in &snapshot.systems {
        place(&s.place, &format!("systems.{id}"), false, snapshot)?;
    }
    for (id, g) in &snapshot.library {
        place(&g.place, &format!("library.{id}"), false, snapshot)?;
        for r in &g.releases {
            place(
                &r.inheritable,
                &format!("release.{}", r.id.0),
                false,
                snapshot,
            )?;
        }
        for (id, p) in &g.profiles {
            if id.is_empty() {
                return Err(ConfigSchemaError::Invalid {
                    path: "profiles".into(),
                    message: "profile ID must not be empty".into(),
                });
            }
            place(&p.place, &format!("profile.{id}"), false, snapshot)?;
        }
        if g.profile
            .as_ref()
            .is_some_and(|p| !g.profiles.contains_key(&p.0))
        {
            return Err(ConfigSchemaError::Invalid {
                path: format!("library.{id}"),
                message: "selected profile does not exist".into(),
            });
        }
    }
    Ok(())
}

impl std::ops::DerefMut for Profile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.place
    }
}
