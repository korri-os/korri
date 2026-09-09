//! The two administrator-selected input routes. No receipt migration or source
//! inference is permitted; provenance is committed with the package selection.
use crate::{catalog, declaration::validate_id, package, repository::SourceUrl};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Provenance {
    RawCache {
        cache_url: String,
    },
    Repository {
        source_url: String,
        plugin_id: String,
        release_version: String,
        platform: String,
        archive_sha256: String,
    },
}

impl Provenance {
    pub fn validate(&self, id: &str) -> Result<(), String> {
        match self {
            Self::RawCache { cache_url } => package::validate_cache_source(cache_url),
            Self::Repository {
                source_url,
                plugin_id,
                release_version,
                platform,
                archive_sha256,
            } => {
                if SourceUrl::parse(source_url)?.as_str() != source_url {
                    return Err("repository provenance URL must be canonical".into());
                }
                validate_id(plugin_id)?;
                if plugin_id != id {
                    return Err("catalog plugin identity differs from the declaration".into());
                }
                catalog::validate_release_version(release_version)?;
                catalog::validate_platform(platform)?;
                if platform != current_platform() {
                    return Err("repository platform does not match this host".into());
                }
                catalog::validate_sha256_hex(archive_sha256, "archive_sha256")
            }
        }
    }
}

pub fn current_platform() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64-linux"
    }
    #[cfg(target_arch = "aarch64")]
    {
        "aarch64-linux"
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    compile_error!(
        "plugin host supports the published x86_64-linux and aarch64-linux targets only"
    );
}

#[derive(Clone, Copy, Debug)]
pub enum SelectionIntent<'a> {
    Install,
    Update { id: &'a str },
    SwitchSource { id: &'a str },
}

impl SelectionIntent<'_> {
    pub fn validate(
        self,
        id: &str,
        old: Option<&Provenance>,
        selected: &Provenance,
    ) -> Result<(), String> {
        match (self, old) {
            (Self::Install, None) => Ok(()),
            (Self::Install, Some(_)) => {
                Err("plugin is installed; use update or explicit source switch".into())
            }
            (_, None) => Err("plugin is not installed".into()),
            (Self::Update { id: expected } | Self::SwitchSource { id: expected }, _)
                if expected != id =>
            {
                Err("selection cannot change the plugin identity".into())
            }
            (Self::Update { .. }, Some(old)) => match (old, selected) {
                (Provenance::RawCache { .. }, Provenance::RawCache { .. }) => Ok(()),
                (
                    Provenance::Repository { source_url: a, .. },
                    Provenance::Repository { source_url: b, .. },
                ) if a == b => Ok(()),
                _ => Err(
                    "update must retain the installed source; inspect and switch explicitly".into(),
                ),
            },
            (Self::SwitchSource { .. }, Some(old)) => match (old, selected) {
                (
                    Provenance::Repository { source_url: a, .. },
                    Provenance::Repository { source_url: b, .. },
                ) if a == b => Err("source is unchanged; use repository update".into()),
                (_, Provenance::Repository { .. }) => Ok(()),
                _ => Err("source switch requires an explicit repository selection".into()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn repository(source: &str) -> Provenance {
        Provenance::Repository {
            source_url: source.into(),
            plugin_id: "@test:plugin".into(),
            release_version: "1".into(),
            platform: current_platform().into(),
            archive_sha256: "a".repeat(64),
        }
    }
    #[test]
    fn updates_never_jump_sources_and_switch_is_explicit() {
        let a = repository("https://a.example/catalog");
        let b = repository("https://b.example/catalog");
        let id = "@test:plugin";
        assert!(SelectionIntent::Install.validate(id, None, &a).is_ok());
        assert!(SelectionIntent::Install.validate(id, Some(&a), &b).is_err());
        assert!(SelectionIntent::Update { id }
            .validate(id, Some(&a), &a)
            .is_ok());
        assert!(SelectionIntent::Update { id }
            .validate(id, Some(&a), &b)
            .is_err());
        assert!(SelectionIntent::SwitchSource { id }
            .validate(id, Some(&a), &b)
            .is_ok());
        assert!(SelectionIntent::SwitchSource { id }
            .validate(id, Some(&a), &a)
            .is_err());
        assert!(SelectionIntent::SwitchSource { id: "@test:other" }
            .validate(id, Some(&a), &b)
            .is_err());
        let raw = Provenance::RawCache {
            cache_url: "file:///cache".into(),
        };
        assert!(SelectionIntent::Update { id }
            .validate(id, Some(&a), &raw)
            .is_err());
        assert!(SelectionIntent::Update { id }
            .validate(id, Some(&raw), &a)
            .is_err());
        assert!(SelectionIntent::SwitchSource { id }
            .validate(id, Some(&raw), &a)
            .is_ok());
    }
}
