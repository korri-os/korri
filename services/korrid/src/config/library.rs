//! `catalog/library.yaml` and the new `device.yaml` sections, in the legacy
//! shapes (legacy `records/library-item.ts`, `records/provider-link.ts`,
//! brainstorm `2026-06-08-004` `sources`). Deltas are listed in contract
//! section 3 of `docs/briefs/2026-10-16-discovery-v2-contract.md`. Owned by
//! the `catalog` builder, which wires these into `ConfigSnapshot`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{optional_non_null, InheritableLayer, NonEmptyString};

pub const LIBRARY_FILE_NAME: &str = "catalog/library.yaml";

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
