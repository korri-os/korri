use crate::RpcFailure;
use serde::{Deserialize, Serialize};
use typeshare::typeshare;

// Typeshare cannot infer internally tagged enums; the exact wire union is
// mapped in typeshare.toml, as for NativeInputEvent. Serde owns the decoder.
#[typeshare(serialized_as = "ArtifactStateWire")]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag")]
pub enum ArtifactState {
    Missing,
    /// Waiting for its turn: one job runs at a time (ADR 0013).
    Queued,
    Downloading {
        file: u32,
        #[serde(rename = "fileCount")]
        file_count: u32,
        #[serde(rename = "bytesDone")]
        #[typeshare(serialized_as = "number")]
        bytes_done: u64,
        #[serde(rename = "bytesTotal", skip_serializing_if = "Option::is_none")]
        #[typeshare(serialized_as = "Option<number>")]
        bytes_total: Option<u64>,
        #[serde(rename = "bytesPerSecond", skip_serializing_if = "Option::is_none")]
        bytes_per_second: Option<f64>,
    },
    Unpacking,
    Ready {
        path: String,
    },
    Failed {
        reason: RpcFailure,
    },
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactStatus {
    pub id: String,
    pub title: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[typeshare(serialized_as = "Option<number>")]
    pub bytes_total: Option<u64>,
    pub state: ArtifactState,
    /// Present, and `true`, only in a game-scoped result, on an artifact that
    /// one of the game's declared releases names. Runner artifacts omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<bool>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcquisitionStatusRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_id: Option<String>,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcquisitionStartRequest {
    pub artifact_ids: Vec<String>,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcquisitionCancelRequest {
    pub artifact_id: String,
}
/// Queue every download of the game's default route, behind earlier ones.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcquisitionDownloadRequest {
    pub game_id: String,
}
/// Remove the ready directories of the game's own release artifacts.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcquisitionRemoveRequest {
    pub game_id: String,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AcquisitionSnapshot {
    pub artifacts: Vec<ArtifactStatus>,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum AcquisitionOutcome {
    Ok(AcquisitionSnapshot),
    Err(RpcFailure),
}

// Game acquisition RPC (ADR 0013). Legacy portal acquisition RPC names and
// fields, with korrid holding the claim the portal names.

/// Legacy `AcquisitionPluginsPayload`.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionPluginsPayload {}

/// An enabled plugin's explicit `providers` record (`ProviderRecord`) and its
/// owner. Replaces legacy `PluginMetadata`.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquisitionProvider {
    pub id: String,
    pub title: String,
    pub plugin_id: String,
}

/// Legacy `PluginListResponse`.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PluginListResponse {
    pub providers: Vec<AcquisitionProvider>,
}

/// Legacy `SearchRequest` without `platforms`. Exactly one provider id.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchRequest {
    pub query: String,
    pub provider_ids: Vec<String>,
}

/// Legacy `SearchResponse`.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SearchResponse {
    pub claims: Vec<super::claims::ProviderClaim>,
}

/// Legacy `DetailsRequest`.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DetailsRequest {
    pub provider_id: String,
    pub id: String,
}

/// Legacy `AcquireArtifactRequest` minus `url`, `size`, `artifactFormat`,
/// `system`, `title` and `artUrl`: korrid takes those from the claim it holds.
/// `sourceId` chooses among several eligible paired sources.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcquireArtifactRequest {
    pub provider_id: String,
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::plugin::deserialize_optional_non_null"
    )]
    pub file_name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::plugin::deserialize_optional_non_null"
    )]
    pub source_id: Option<String>,
}

/// Legacy `AcquireStatusRequest`.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcquireStatusRequest {
    pub job_id: String,
}

/// Follows `AcquireStatusRequest`; legacy had no cancel.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcquireCancelRequest {
    pub job_id: String,
}

/// One eligible paired source, with `DiscoverySource`'s own `id` and `label`.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AcquireSourceChoice {
    pub id: String,
    pub label: String,
}

/// Legacy job states as a tagged union with ADR 0012 progress fields.
/// `ChooseSource` is terminal: no download started; the host asks the person
/// and calls `acquire` again with `sourceId`, which starts a new job.
#[typeshare(serialized_as = "AcquireJobStateWire")]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag")]
pub enum AcquireJobState {
    ChooseSource {
        sources: Vec<AcquireSourceChoice>,
    },
    Resolving,
    Downloading {
        #[serde(rename = "bytesDone")]
        #[typeshare(serialized_as = "number")]
        bytes_done: u64,
        #[serde(rename = "bytesTotal", skip_serializing_if = "Option::is_none")]
        #[typeshare(serialized_as = "Option<number>")]
        bytes_total: Option<u64>,
        #[serde(rename = "bytesPerSecond", skip_serializing_if = "Option::is_none")]
        bytes_per_second: Option<f64>,
    },
    Placing,
    Imported {
        #[serde(rename = "gameId")]
        game_id: String,
    },
    Staged {
        message: String,
    },
    Failed {
        reason: RpcFailure,
    },
    Cancelled,
}

/// Legacy `AcquireJobStatus` without `stagedPath`/`placedPath`: paths never
/// leave korrid, and the game id is the handle for a placed game.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquireJobStatus {
    pub job_id: String,
    pub provider_id: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub state: AcquireJobState,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum PluginListOutcome {
    Ok(PluginListResponse),
    Err(RpcFailure),
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SearchOutcome {
    Ok(SearchResponse),
    Err(RpcFailure),
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum DetailsOutcome {
    Ok(super::claims::ProviderClaimDetails),
    Err(RpcFailure),
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum AcquireJobOutcome {
    Ok(AcquireJobStatus),
    Err(RpcFailure),
}

/// Never send transport, filesystem or unpack stderr strings to the portal.
/// A closed vocabulary is stronger than trying to redact arbitrary diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Failure {
    UnsafeUrl,
    UnsafePath,
    Redirect,
    Network,
    Stalled,
    Hash,
    Storage,
    NoSpace,
    Unpack,
    Cancelled,
    /// A request or redirect hop outside the plugin's declared hosts.
    UndeclaredHost,
    TooLarge,
    WebPage,
    Empty,
    /// The file does not match the target system's declared content rules.
    NotSystemFile,
    /// Legacy placement: 20 different files already hold the name.
    NameTaken,
}
impl Failure {
    pub(super) fn rpc(self) -> RpcFailure {
        let message = match self {
            Self::UnsafeUrl => "The download address is not allowed.",
            Self::UnsafePath => "The download file name is not safe.",
            Self::Redirect => "The download redirected too many times.",
            Self::Network => "The download could not finish. Check the connection and retry.",
            Self::Stalled => "The download stopped making progress. Please retry.",
            Self::Hash => "The downloaded file did not pass verification. Please retry.",
            Self::Storage => "The download storage is unavailable.",
            Self::NoSpace => "There is not enough space for this download.",
            Self::Unpack => "The downloaded files could not be unpacked.",
            Self::Cancelled => "The download was cancelled.",
            Self::UndeclaredHost => "The download address is not allowed.",
            Self::TooLarge => "The download is larger than 2 GiB.",
            Self::WebPage => "The source sent a web page instead of the game file.",
            Self::Empty => "The source sent an empty file.",
            Self::NotSystemFile => "The downloaded file is not a game for this system.",
            Self::NameTaken => "The library folder already has different files with this name.",
        };
        RpcFailure {
            code: "AcquisitionFailed".into(),
            message: crate::launcher::redaction::status_text(
                message,
                crate::launcher::redaction::MAX_REASON_CHARS,
            ),
        }
    }
}
impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        if error.raw_os_error() == Some(libc::ENOSPC) {
            Self::NoSpace
        } else {
            Self::Storage
        }
    }
}
