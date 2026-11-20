//! `catalog/library.yaml` and the new `device.yaml` sections, in the legacy
//! shapes (legacy `records/library-item.ts`, `records/provider-link.ts`,
//! brainstorm `2026-06-08-004` `sources`). Deltas are listed in contract
//! section 3 of `docs/briefs/2026-10-16-discovery-v2-contract.md`. Owned by
//! the `catalog` builder, which wires these into `ConfigSnapshot`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{optional_non_null, InheritableLayer, NonEmptyString};

pub const LIBRARY_FILE_NAME: &str = "catalog/library.yaml";

/// Public ids (ADR 0090): an outside authority's key, such as `wikidata`
/// or `igdb`, to that authority's own ids for one system or game, as text.
/// Legacy's `externalIds` with a list per authority, not one value.
pub type ExternalIds = BTreeMap<String, Vec<String>>;

const MAX_AUTHORITIES: usize = 64;
const MAX_IDS_PER_AUTHORITY: usize = 64;
const MAX_ID_CHARS: usize = 256;

/// Authority keys and ids are non-empty text with no control character;
/// an authority has at least one id and no id twice.
pub fn validate_external_ids(ids: &ExternalIds) -> Result<(), &'static str> {
    let text = |value: &str| {
        !value.is_empty()
            && value.chars().count() <= MAX_ID_CHARS
            && !value.chars().any(char::is_control)
    };
    if ids.len() > MAX_AUTHORITIES {
        return Err("externalIds has too many authorities");
    }
    for (authority, values) in ids {
        if !text(authority) {
            return Err("an externalIds authority is 1 to 256 characters of text");
        }
        if values.is_empty() || values.len() > MAX_IDS_PER_AUTHORITY {
            return Err("an externalIds authority has 1 to 64 ids");
        }
        if !values.iter().all(|value| text(value)) {
            return Err("an externalIds id is 1 to 256 characters of text");
        }
        let unique: std::collections::BTreeSet<_> = values.iter().collect();
        if unique.len() != values.len() {
            return Err("an externalIds authority lists one id twice");
        }
    }
    Ok(())
}

/// Union per authority: `into`'s ids first, then each new one of `from` in
/// its order. Returns whether `into` changed.
pub fn merge_external_ids(into: &mut ExternalIds, from: &ExternalIds) -> bool {
    let mut changed = false;
    for (authority, values) in from {
        let merged = into.entry(authority.clone()).or_default();
        for value in values {
            if !merged.contains(value) {
                merged.push(value.clone());
                changed = true;
            }
        }
    }
    changed
}

/// `library.<id>`. Ids are readable legacy playable-id segments.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct LibraryItemPayload {
    #[serde(flatten)]
    pub place: super::place::PlaceBody,
    #[serde(default)]
    pub profiles: BTreeMap<String, crate::launch_settings::config::Profile>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub profile: Option<NonEmptyString>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub title: Option<String>,
    #[serde(
        default,
        rename = "version-of",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub version_of: Option<NonEmptyString>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub relation: Option<NonEmptyString>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub contains: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub display: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub metadata: Option<BTreeMap<String, serde_json::Value>>,
    /// The game's public ids that art plugins matched (ADR 0090). Not part
    /// of game identity.
    #[serde(
        default,
        rename = "externalIds",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub external_ids: Option<ExternalIds>,
    /// Ordered, oldest first, non-empty.
    pub releases: Vec<LibraryReleasePayload>,
}

impl std::ops::Deref for LibraryItemPayload {
    type Target = super::place::PlaceBody;
    fn deref(&self) -> &Self::Target {
        &self.place
    }
}

/// One entry of `library.<id>.releases`. No `target`: device paths stay in
/// `device.yaml` `locations`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct LibraryReleasePayload {
    pub id: NonEmptyString,
    /// Optional only for declared releases with no system.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub system: Option<NonEmptyString>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub identity: Option<LibraryIdentity>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub display: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(flatten)]
    pub inheritable: InheritableLayer,
}

/// Legacy content identities (ADR 0005), plus existing provider release keys.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum LibraryIdentity {
    Hash { value: String },
    Parts { value: String, parts: Vec<String> },
    Chd { value: String },
    Provider { value: String },
}

impl LibraryIdentity {
    pub fn value(&self) -> &str {
        match self {
            Self::Hash { value }
            | Self::Parts { value, .. }
            | Self::Chd { value }
            | Self::Provider { value } => value,
        }
    }
}

/// `device.yaml` `sources.<id>` (ADR 0001).
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourcePayload {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub title: Option<String>,
    pub kind: Vec<NonEmptyString>,
    pub storage: NonEmptyString,
    /// A system id or a group id.
    pub system: NonEmptyString,
}

/// `device.yaml` `provider-links.<key>`, legacy shape.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderLinkPayload {
    pub provider: NonEmptyString,
    pub playable: NonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub release: Option<NonEmptyString>,
    pub refs: Vec<ProviderLinkRef>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderLinkRef {
    pub kind: ProviderLinkRefKind,
    pub value: NonEmptyString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub scope: Option<NonEmptyString>,
    #[serde(
        default,
        rename = "target-part",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_non_null"
    )]
    pub target_part: Option<NonEmptyString>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderLinkRefKind {
    Url,
    ProviderItemId,
    ExternalId,
}

impl std::ops::DerefMut for LibraryItemPayload {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.place
    }
}
