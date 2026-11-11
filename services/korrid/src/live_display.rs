//! Exact-session viewer reports. Peer identity comes from RPC authentication,
//! never from a field in these requests. Sizes reuse the launch display treaty.
use crate::{launcher::plugin_launch::PluginDisplay, RpcFailure};
use serde::{Deserialize, Serialize};
use typeshare::typeshare;

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    content = "payload",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum ViewerDisplayEvent {
    Report {
        connection: String,
        display: PluginDisplay,
    },
    Leave {
        connection: String,
    },
    Portrait {
        connection: String,
        enabled: bool,
    },
    Applied {
        connection: String,
        revision: String,
    },
    Refused {
        connection: String,
        revision: String,
    },
}
impl ViewerDisplayEvent {
    pub(crate) fn connection(&self) -> &str {
        match self {
            Self::Report { connection, .. }
            | Self::Leave { connection }
            | Self::Portrait { connection, .. }
            | Self::Applied { connection, .. }
            | Self::Refused { connection, .. } => connection,
        }
    }
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionViewerDisplayRequest {
    pub expected_launch_id: String,
    pub event: ViewerDisplayEvent,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionViewerDisplay {
    pub display: PluginDisplay,
    pub owner: bool,
    pub portrait: bool,
    /// Present only for the exact owner that must apply this pending encode size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SessionViewerDisplayOutcome {
    Ok(SessionViewerDisplay),
    Err(RpcFailure),
}
