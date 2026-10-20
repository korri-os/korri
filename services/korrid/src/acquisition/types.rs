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
