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

/// A viewer's request for its own stream's encode size (#476 Route C). The
/// holder finds that stream through the caller's certificate grant, never
/// through a field here. The size is the viewer treaty's Resize display.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionViewerResizeRequest {
    pub expected_launch_id: String,
    pub display: PluginDisplay,
}
/// The stream's answer, as the viewer treaty's Resize answers: applied,
/// refused, or "Live resolution is unavailable".
#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ViewerResize {
    Applied,
    Refused,
    Unavailable,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SessionViewerResizeOutcome {
    Ok(ViewerResize),
    Err(RpcFailure),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_request_and_answer_have_the_wire_shapes_of_their_producers() {
        let request: SessionViewerResizeRequest = serde_json::from_value(serde_json::json!({
            "expectedLaunchId": "11111111111111111111111111111111",
            "display": {"width": 720, "height": 1280}
        }))
        .unwrap();
        assert_eq!(
            request.display,
            PluginDisplay {
                width: 720,
                height: 1280
            }
        );
        assert!(
            serde_json::from_value::<SessionViewerResizeRequest>(serde_json::json!({
                "expectedLaunchId": "11111111111111111111111111111111",
                "display": {"width": 720, "height": 1280},
                "devicePublicKey": "another"
            }))
            .is_err()
        );
        for (answer, wire) in [
            (ViewerResize::Applied, "Applied"),
            (ViewerResize::Refused, "Refused"),
            (ViewerResize::Unavailable, "Unavailable"),
        ] {
            assert_eq!(
                serde_json::to_value(SessionViewerResizeOutcome::Ok(answer)).unwrap(),
                serde_json::json!({"_tag": "Ok", "payload": wire})
            );
        }
    }
}
