//! Legacy acquisition results, admitted from provider plugin output.
//!
//! Field names and unions are legacy `product/platform/protocol/acquisition/
//! claim.ts` and `download-resolution.ts` verbatim, minus the ADR 0013 deltas:
//! no `ref`, `playable`, `facets`, or `FinalDownload.requestHeaders`.
use serde::{Deserialize, Serialize};
use typeshare::typeshare;
use url::Url;

use crate::plugin::deserialize_optional_non_null as non_null;

/// Legacy `SafeText`.
const SAFE_TEXT_CHARS: usize = 8192;
/// Legacy `ProviderRef.value` bound, applied to the claim id korrid holds.
const CLAIM_ID_CHARS: usize = 2048;

#[typeshare(serialized_as = "ProviderClaimTagWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProviderClaimTag {
    ProviderClaim,
}

#[typeshare(serialized_as = "ProviderClaimDetailsTagWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProviderClaimDetailsTag {
    ProviderClaimDetails,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderClaim {
    #[serde(rename = "_tag")]
    pub tag: ProviderClaimTag,
    pub provider_id: String,
    pub id: String,
    pub title: String,
    pub url: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub platform: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub thumbnail_url: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub artifact: Option<ArtifactAcquisitionHint>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub fetched_at: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderClaimDetails {
    #[serde(rename = "_tag")]
    pub tag: ProviderClaimDetailsTag,
    pub provider_id: String,
    pub id: String,
    pub title: String,
    pub url: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub download_page_url: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub artifact: Option<ArtifactAcquisitionHint>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub fetched_at: Option<String>,
}

/// Legacy `ArtifactAcquisitionHint`. `system` is the Korri system id that
/// decides where the file goes; without a known one the claim cannot be acquired.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactAcquisitionHint {
    pub kind: ArtifactKind,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub system: Option<String>,
    pub format: SemanticFormat,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub file: Option<ArtifactFile>,
}

#[typeshare(serialized_as = "ArtifactKindWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ArtifactKind {
    #[serde(rename = "content")]
    Content,
    #[serde(rename = "patch")]
    Patch,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticFormat {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub version: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactFile {
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub extension: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub media_type: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    #[typeshare(serialized_as = "Option<number>")]
    pub size_bytes: Option<u64>,
}

#[typeshare(serialized_as = "FinalDownloadTagWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FinalDownloadTag {
    FinalDownload,
}
#[typeshare(serialized_as = "NonFinalDownloadTagWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum NonFinalDownloadTag {
    NonFinalDownload,
}
#[typeshare(serialized_as = "FailedDownloadTagWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FailedDownloadTag {
    FailedDownload,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinalDownload {
    #[serde(rename = "_tag")]
    pub tag: FinalDownloadTag,
    pub provider_id: String,
    pub url: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub filename: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub content_type: Option<String>,
}

#[typeshare(serialized_as = "NonFinalDownloadReasonWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum NonFinalDownloadReason {
    #[serde(rename = "interstitial")]
    Interstitial,
    #[serde(rename = "requires-user-action")]
    RequiresUserAction,
    #[serde(rename = "unsupported")]
    Unsupported,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NonFinalDownload {
    #[serde(rename = "_tag")]
    pub tag: NonFinalDownloadTag,
    pub provider_id: String,
    pub reason: NonFinalDownloadReason,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub url: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub choices: Option<Vec<DownloadChoice>>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadChoice {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub file_name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub size: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub platforms: Option<Vec<String>>,
}

#[typeshare(serialized_as = "FailedDownloadReasonWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FailedDownloadReason {
    #[serde(rename = "provider-error")]
    ProviderError,
    #[serde(rename = "configuration")]
    Configuration,
    #[serde(rename = "not-found")]
    NotFound,
    #[serde(rename = "defective-provider")]
    DefectiveProvider,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FailedDownload {
    #[serde(rename = "_tag")]
    pub tag: FailedDownloadTag,
    pub provider_id: String,
    pub reason: FailedDownloadReason,
    pub message: String,
}

#[typeshare(serialized_as = "DownloadResolutionWire")]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DownloadResolution {
    Final(FinalDownload),
    NonFinal(NonFinalDownload),
    Failed(FailedDownload),
}

/// Why plugin output was refused. Diagnostics only; never sent to the portal.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Inadmissible(pub(super) &'static str);

fn safe_text(value: &str) -> Result<(), Inadmissible> {
    (value.chars().count() <= SAFE_TEXT_CHARS)
        .then_some(())
        .ok_or(Inadmissible("text exceeds 8192 characters"))
}

/// Legacy `SafeUrl`: absolute http(s), no embedded credentials.
pub(super) fn safe_url(value: &str) -> Result<(), Inadmissible> {
    let url = Url::parse(value).map_err(|_| Inadmissible("invalid URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host().is_none()
    {
        return Err(Inadmissible("URL must be http(s) without credentials"));
    }
    safe_text(value)
}

fn claim_id(value: &str) -> Result<(), Inadmissible> {
    let count = value.chars().count();
    (count > 0 && count <= CLAIM_ID_CHARS)
        .then_some(())
        .ok_or(Inadmissible("claim id must be 1 to 2048 characters"))
}

fn provider(asked: &str, value: &str) -> Result<(), Inadmissible> {
    (asked == value)
        .then_some(())
        .ok_or(Inadmissible("result names another provider"))
}

fn optional<T>(
    value: &Option<T>,
    check: impl Fn(&T) -> Result<(), Inadmissible>,
) -> Result<(), Inadmissible> {
    value.as_ref().map_or(Ok(()), check)
}

/// Legacy `SEMANTIC_ID`.
fn semantic_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b".-_".contains(b))
}

/// Legacy `SafeFileName`.
pub(super) fn safe_file_name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains(['/', '\\', '\0'])
        && value.chars().count() <= SAFE_TEXT_CHARS
}

/// Legacy `SAFE_EXTENSION`.
fn safe_extension(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 32
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn hint(value: &ArtifactAcquisitionHint) -> Result<(), Inadmissible> {
    if value.system.as_deref() == Some("") {
        return Err(Inadmissible("artifact system must not be empty"));
    }
    if !semantic_id(&value.format.id) || value.format.version.as_deref() == Some("") {
        return Err(Inadmissible("artifact format is invalid"));
    }
    if let Some(file) = &value.file {
        if !safe_file_name(&file.name)
            || file
                .extension
                .as_deref()
                .is_some_and(|e| !safe_extension(e))
            || file.media_type.as_deref() == Some("")
            || file.size_bytes == Some(0)
        {
            return Err(Inadmissible("artifact file is invalid"));
        }
    }
    Ok(())
}

impl ProviderClaim {
    pub(super) fn admit(&self, asked: &str) -> Result<(), Inadmissible> {
        provider(asked, &self.provider_id)?;
        claim_id(&self.id)?;
        safe_text(&self.title)?;
        safe_url(&self.url)?;
        optional(&self.platform, |v| safe_text(v))?;
        optional(&self.thumbnail_url, |v| safe_url(v))?;
        optional(&self.fetched_at, |v| safe_text(v))?;
        optional(&self.artifact, hint)
    }
}

impl ProviderClaimDetails {
    pub(super) fn admit(&self, asked: &str, id: &str) -> Result<(), Inadmissible> {
        provider(asked, &self.provider_id)?;
        if self.id != id {
            return Err(Inadmissible("details name another claim"));
        }
        safe_text(&self.title)?;
        safe_url(&self.url)?;
        optional(&self.description, |v| safe_text(v))?;
        optional(&self.download_page_url, |v| safe_url(v))?;
        optional(&self.fetched_at, |v| safe_text(v))?;
        optional(&self.artifact, hint)
    }
}

impl DownloadResolution {
    pub(super) fn admit(&self, asked: &str) -> Result<(), Inadmissible> {
        match self {
            Self::Final(value) => {
                provider(asked, &value.provider_id)?;
                safe_url(&value.url)?;
                optional(&value.filename, |v| {
                    safe_file_name(v)
                        .then_some(())
                        .ok_or(Inadmissible("download file name is unsafe"))
                })?;
                optional(&value.content_type, |v| safe_text(v))
            }
            Self::NonFinal(value) => {
                provider(asked, &value.provider_id)?;
                optional(&value.url, |v| safe_url(v))?;
                for choice in value.choices.iter().flatten() {
                    safe_text(&choice.id)?;
                    optional(&choice.file_name, |v| safe_text(v))?;
                    optional(&choice.size, |v| safe_text(v))?;
                }
                Ok(())
            }
            Self::Failed(value) => {
                provider(asked, &value.provider_id)?;
                safe_text(&value.message)
            }
        }
    }
}
