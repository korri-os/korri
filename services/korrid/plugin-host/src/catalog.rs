// Catalog contract shared by the build-side publisher and the device-side reader.
//
// Field provenance:
// - `plugin_id`: Declaration::id() = "@namespace:name" per declaration.rs
// - `title`: Declaration.title (optional, same validation as declaration)
// - `description`: Declaration.description (optional, same validation)
// - `release_version`: explicit string supplied by the publisher, distinct from
//   the upstream binary version embedded in the Nix store path name. A plugin
//   author may cut a new release with the same upstream binary (e.g. to fix
//   plugin.ts). A different upstream binary does not automatically become a new
//   release. This is a label, not a semver; ordering is always explicit.
// - `platform`: Nix system string (e.g. "x86_64-linux", "aarch64-linux")
//   sourced from the build environment at publish time.
// - `store_path`: exact content-addressed /nix/store output produced by
//   `nix store make-content-addressed`. Identity comes from the bytes, not a
//   publisher key.
// - `archive_url`: HTTPS URL where the complete Nix file-cache tar is hosted.
//   The device downloads and verifies this before import.
// - `archive_sha256`: lowercase hex SHA-256 of the archive bytes, measured by
//   the publisher over the file it produced. The device re-measures before use.
//
// The catalog carries no approval, unit policy, or device-specific data.
// Those are the host's responsibility (package.rs, host.rs).

use crate::declaration::validate_id;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// One release entry inside a catalog file.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogRecord {
    /// Plugin identity in "@namespace:name" form.
    pub plugin_id: String,
    /// Human-readable title sourced from the declaration (optional).
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub title: Option<String>,
    /// Human-readable description sourced from the declaration (optional).
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<String>,
    /// Publisher-assigned release label, independent of upstream binary version.
    /// ASCII letters, digits and -._+ are accepted; no ordering is inferred.
    pub release_version: String,
    /// Nix system string for this record's binary (e.g. "x86_64-linux").
    pub platform: String,
    /// Content-addressed /nix/store path produced by make-content-addressed.
    pub store_path: String,
    /// HTTPS URL of the published archive containing the complete Nix file cache.
    pub archive_url: String,
    /// Lowercase hex SHA-256 of the archive file bytes.
    pub archive_sha256: String,
}

/// A parsed and validated catalog file.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub records: Vec<CatalogRecord>,
}

impl CatalogRecord {
    pub fn validate(&self) -> Result<(), String> {
        validate_id(&self.plugin_id)?;

        validate_release_version(&self.release_version)?;

        validate_platform(&self.platform)?;

        crate::package::validate_store_path(std::path::Path::new(&self.store_path))?;

        validate_archive_url(&self.archive_url)?;

        validate_sha256_hex(&self.archive_sha256, "archive_sha256")?;

        if let Some(t) = &self.title {
            if t.is_empty() || t.len() > 128 || t.chars().any(char::is_control) {
                return Err("title must be 1–128 bytes without control characters".into());
            }
        }
        if let Some(d) = &self.description {
            if d.len() > 1024 || d.chars().any(char::is_control) {
                return Err(
                    "description must be at most 1024 bytes without control characters".into(),
                );
            }
        }

        Ok(())
    }
}

impl Catalog {
    /// Parse and validate a catalog from JSON bytes.
    ///
    /// Rejects unknown fields, invalid records, and ambiguous duplicate
    /// (plugin_id, release_version, platform) triples within one catalog.
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("catalog exceeds 4 MiB limit".into());
        }
        let catalog: Catalog =
            serde_json::from_slice(bytes).map_err(|e| format!("invalid catalog JSON: {e}"))?;
        catalog.validate()?;
        Ok(catalog)
    }

    fn validate(&self) -> Result<(), String> {
        if self.records.len() > 10_000 {
            return Err("catalog contains more than 10,000 records".into());
        }
        let mut seen: HashSet<(String, String, String)> = HashSet::new();
        for record in &self.records {
            record.validate()?;
            let key = (
                record.plugin_id.clone(),
                record.release_version.clone(),
                record.platform.clone(),
            );
            if !seen.insert(key) {
                return Err(format!(
                    "ambiguous duplicate record: plugin_id={} release_version={} platform={}",
                    record.plugin_id, record.release_version, record.platform
                ));
            }
        }
        Ok(())
    }

    /// Serialize to compact JSON bytes.
    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|e| e.to_string())
    }

    /// Serialize to pretty-printed JSON bytes.
    pub fn to_json_pretty(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        serde_json::to_vec_pretty(self).map_err(|e| e.to_string())
    }

    /// Return all records for a given plugin identity.
    pub fn records_for(&self, plugin_id: &str) -> Vec<&CatalogRecord> {
        self.records
            .iter()
            .filter(|r| r.plugin_id == plugin_id)
            .collect()
    }

    /// Return the record for a specific plugin, version, and platform.
    /// Returns an error if there is no matching record.
    pub fn record(
        &self,
        plugin_id: &str,
        release_version: &str,
        platform: &str,
    ) -> Result<&CatalogRecord, String> {
        self.records
            .iter()
            .find(|r| {
                r.plugin_id == plugin_id
                    && r.release_version == release_version
                    && r.platform == platform
            })
            .ok_or_else(|| {
                format!("no catalog record for {plugin_id} version {release_version} on {platform}")
            })
    }
}

pub fn validate_platform(platform: &str) -> Result<(), String> {
    // Nix system strings are "{cpu}-{os}" with alphanumeric and "-".
    if platform.is_empty()
        || platform.len() > 64
        || !platform
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || !platform.contains('-')
    {
        return Err(format!(
            "platform {:?} must be a Nix system string like \"x86_64-linux\"",
            platform
        ));
    }
    Ok(())
}

pub fn validate_release_version(version: &str) -> Result<(), String> {
    if version.is_empty()
        || version.len() > 64
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._+".contains(&b))
        || version == "."
        || version == ".."
    {
        return Err(
            "release_version must be a bounded release label without path separators".into(),
        );
    }
    Ok(())
}

pub fn validate_sha256_hex(value: &str, field: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{field} must be 64 lowercase hex digits"));
    }
    if value.bytes().any(|b| b.is_ascii_uppercase()) {
        return Err(format!("{field} must be lowercase hex"));
    }
    Ok(())
}

/// Validate archive_url using a real URL parser.
///
/// Accepted: `https://` scheme, non-empty host, no userinfo (credentials),
/// no fragment, length <= 4096, no control characters.
/// Rejected: bare `https://`, credentials (`user:pass@`), fragments (`#...`),
/// and any non-HTTPS scheme.
pub fn validate_archive_url(url_str: &str) -> Result<(), String> {
    crate::https_url::parse(url_str)
        .map(|_| ())
        .map_err(|error| format!("archive_url: {error}"))
}

fn optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
