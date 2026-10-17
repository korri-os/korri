//! Library and device reference validation. No superseded catalog schema is read.
use std::collections::BTreeSet;

use super::{invalid, library::LibraryIdentity, ConfigSchemaError, ConfigSnapshot, Location};

pub(crate) fn valid_local_id(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

pub(crate) fn validate_release_key(value: &str) -> Result<(), String> {
    if let Some(digest) = value
        .strip_prefix("sha256:")
        .filter(|v| v.len() == 64)
        .or_else(|| value.strip_prefix("sha1:").filter(|v| v.len() == 40))
    {
        if digest
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Ok(());
        }
    } else if let Some((provider, reference)) = value.split_once('/') {
        super::validate_provider_id("release key", provider).map_err(|e| e.to_string())?;
        serde_json::from_value::<super::SafeRefValue>(reference.into())
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    Err(
        "release keys must be sha256:<64 lowercase hex>, sha1:<40 lowercase hex> or @ns:name/ref"
            .into(),
    )
}

pub(super) fn validate(snapshot: &ConfigSnapshot) -> Result<(), ConfigSchemaError> {
    let mut identities = BTreeSet::new();
    for (id, item) in &snapshot.library {
        let path = format!("catalog/library.yaml.library[{id}]");
        if !valid_local_id(id) {
            return Err(invalid(&path, "game ids must be readable local ids"));
        }
        if item.releases.is_empty() {
            return Err(invalid(&path, "game must declare at least one release"));
        }
        let mut ids = BTreeSet::new();
        for release in &item.releases {
            if !valid_local_id(&release.id.0) || !ids.insert(&release.id.0) {
                return Err(invalid(
                    &path,
                    "release ids must be unique readable local ids",
                ));
            }
            if let Some(identity) = &release.identity {
                validate_release_key(identity.value())
                    .map_err(|message| invalid(&path, &message))?;
                if !identities.insert(identity.value()) {
                    return Err(invalid(
                        &path,
                        "release identity must be unique in the library",
                    ));
                }
                match identity {
                    LibraryIdentity::Hash { value }
                        if !value.starts_with("sha256:") && !value.starts_with("sha1:") =>
                    {
                        return Err(invalid(&path, "hash identity must be SHA-256 or SHA-1"))
                    }
                    LibraryIdentity::Chd { value } if !value.starts_with("sha1:") => {
                        return Err(invalid(&path, "CHD identity must be SHA-1"))
                    }
                    LibraryIdentity::Parts { value, parts } => {
                        if !value.starts_with("sha256:") || parts.is_empty() {
                            return Err(invalid(
                                &path,
                                "parts identity needs SHA-256 and non-empty parts",
                            ));
                        }
                        for part in parts {
                            validate_release_key(part)
                                .map_err(|message| invalid(&path, &message))?;
                        }
                        use sha2::{Digest, Sha256};
                        if *value
                            != format!(
                                "sha256:{}",
                                hex::encode(Sha256::digest(parts.join("\n").as_bytes()))
                            )
                        {
                            return Err(invalid(
                                &path,
                                "parts identity must hash the ordered part values",
                            ));
                        }
                    }
                    _ => {}
                }
            }
            if release.inheritable.launch.is_some() {
                return Err(invalid(&path, "launch opinions are not catalog content"));
            }
            if release.inheritable.hooks.as_ref().is_some_and(|hooks| {
                hooks.before.as_ref().is_some_and(|v| !v.is_empty())
                    || hooks.after.as_ref().is_some_and(|v| !v.is_empty())
            }) {
                return Err(invalid(&path, "only device.yaml may carry commands"));
            }
        }
    }
    for (id, source) in &snapshot.sources {
        if !valid_local_id(id) {
            return Err(invalid(
                "device.yaml.sources",
                "source ids must be readable local ids",
            ));
        }
        if !snapshot.storage.contains_key(&source.storage.0) {
            return Err(invalid(
                &format!("device.yaml.sources[{id}]"),
                "source storage must exist",
            ));
        }
    }
    for (id, link) in &snapshot.provider_links {
        super::validate_provider_id(
            &format!("device.yaml.provider-links[{id}]"),
            &link.provider.0,
        )?;
        if !snapshot.library.contains_key(&link.playable.0) {
            return Err(invalid(
                &format!("device.yaml.provider-links[{id}]"),
                "provider link game must exist",
            ));
        }
        if link.refs.is_empty() {
            return Err(invalid(
                &format!("device.yaml.provider-links[{id}]"),
                "provider link refs must be non-empty",
            ));
        }
    }
    for (key, locations) in &snapshot.locations {
        let path = format!("device.yaml.locations[{key}]");
        validate_release_key(key).map_err(|message| invalid(&path, &message))?;
        for location in locations {
            if let Location::ProviderRef {
                provider,
                provider_ref,
            } = location
            {
                if key != &format!("{}/{}", provider.0, provider_ref.0) {
                    return Err(invalid(
                        &path,
                        "provider location must match its release key",
                    ));
                }
            }
            let discovery = match location {
                Location::File { discovery, .. } | Location::FileSet { discovery, .. } => {
                    discovery.as_ref()
                }
                _ => None,
            };
            if let Some(discovery) = discovery {
                for time in
                    std::iter::once(&discovery.first_seen_at).chain(discovery.missing_since.iter())
                {
                    // time is already in the dependency graph and accepts RFC 3339 only.
                    chrono::DateTime::parse_from_rfc3339(&time.0)
                        .map_err(|_| invalid(&path, "discovery times must be RFC 3339"))?;
                }
            }
        }
    }
    Ok(())
}

/// Content identity values and provider references used as device location keys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseKey(pub String);
impl<'de> serde::Deserialize<'de> for ReleaseKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        validate_release_key(&value).map_err(serde::de::Error::custom)?;
        Ok(Self(value))
    }
}
