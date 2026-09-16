//! Catalog identity and links from the approved minimum-layer brief.
//! Release content retains the existing readable release fields. Targets are
//! device locations; launch opinions are not catalog content.
use serde::{de::Error, Deserialize, Deserializer};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    optional_non_null, ConfigSchemaError, ConfigSnapshot, InheritableLayer, NonEmptyString,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GamePayload {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub runner: Option<NonEmptyString>,
    #[serde(default)]
    pub families: super::cascade::RunnerConfigs,
    #[serde(default)]
    pub runners: super::cascade::RunnerConfigs,
    #[serde(deserialize_with = "required_non_null")]
    pub title: String,
    pub releases: Vec<ReleaseKey>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReleasePayload {
    pub game: GameId,
    #[serde(deserialize_with = "required_non_null")]
    pub system: NonEmptyString,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub identity: Option<ReleaseIdentity>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub display: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(flatten)]
    pub inheritable: InheritableLayer,
}

fn required_non_null<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    optional_non_null(deserializer)?.ok_or_else(|| D::Error::custom("required field is absent"))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseIdentity {
    File,
    Manifest,
    Chd,
    Provider,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameId(pub String);

impl<'de> Deserialize<'de> for GameId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if !valid_game_id(&value) {
            return Err(D::Error::custom("game ids must be canonical ULIDs"));
        }
        Ok(Self(value))
    }
}

fn valid_game_id(value: &str) -> bool {
    value.len() == 26
        && value.as_bytes()[0] <= b'7'
        && value
            .bytes()
            .all(|byte| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&byte))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseKey(pub String);

impl<'de> Deserialize<'de> for ReleaseKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        validate_release_key(&value).map_err(D::Error::custom)?;
        Ok(Self(value))
    }
}

fn validate_release_key(value: &str) -> Result<(), String> {
    if value.starts_with("sha256:") {
        serde_json::from_value::<super::ArtifactIdString>(value.into())
            .map_err(|error| error.to_string())?;
    } else if let Some((provider, reference)) = value.split_once('/') {
        super::validate_provider_id("release key", provider).map_err(|error| error.to_string())?;
        serde_json::from_value::<super::SafeRefValue>(reference.into())
            .map_err(|error| error.to_string())?;
    } else {
        return Err(
            "release keys must be sha256:<64 lowercase hex characters> or @ns:name/ref".into(),
        );
    }
    Ok(())
}

pub(super) fn validate(snapshot: &ConfigSnapshot) -> Result<(), ConfigSchemaError> {
    let invalid = |path: String, message: &str| ConfigSchemaError::Invalid {
        path,
        message: message.into(),
    };
    for (id, game) in &snapshot.games {
        let path = format!("catalog/games.yaml.games[{id}]");
        if !valid_game_id(id) {
            return Err(invalid(path, "game ids must be canonical ULIDs"));
        }
        let mut seen = BTreeSet::new();
        if game.releases.is_empty() {
            return Err(invalid(path, "game must declare at least one release"));
        }
        for key in &game.releases {
            if !seen.insert(&key.0) {
                return Err(invalid(path.clone(), "game release keys must be unique"));
            }
            if !snapshot
                .releases
                .get(&key.0)
                .is_some_and(|release| release.game.0 == *id)
            {
                return Err(invalid(
                    path.clone(),
                    "release must exist and link back to this game",
                ));
            }
        }
    }
    for (key, release) in &snapshot.releases {
        let path = format!("catalog/releases.yaml.releases[{key}]");
        validate_release_key(key).map_err(|message| invalid(path.clone(), &message))?;
        if release.inheritable.launch.is_some() {
            return Err(invalid(path, "launch opinions are not catalog content"));
        }
        if release.inheritable.hooks.as_ref().is_some_and(|hooks| {
            hooks.before.as_ref().is_some_and(|steps| !steps.is_empty())
                || hooks.after.as_ref().is_some_and(|steps| !steps.is_empty())
        }) {
            return Err(invalid(path, "only device.yaml may carry commands"));
        }
        if !snapshot
            .games
            .get(&release.game.0)
            .is_some_and(|game| game.releases.iter().any(|id| id.0 == *key))
        {
            return Err(invalid(path, "game must exist and list this release"));
        }
    }
    for (key, locations) in &snapshot.locations {
        let path = format!("device.yaml.locations[{key}]");
        validate_release_key(key).map_err(|message| invalid(path.clone(), &message))?;
        for location in locations {
            if let super::Location::ProviderRef {
                provider,
                provider_ref,
            } = location
            {
                if key != &format!("{}/{}", provider.0, provider_ref.0) {
                    return Err(invalid(
                        path.clone(),
                        "provider location must match its release key",
                    ));
                }
            }
        }
    }
    Ok(())
}
