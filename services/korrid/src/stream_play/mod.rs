//! Shared play wire contracts. The holder remains the session authority.
pub(crate) mod device;
pub(crate) mod person_name;
pub(crate) mod registration;
pub(crate) mod rights;
mod viewer_wiring;
use crate::{ActiveSession, RpcFailure};
use serde::{Deserialize, Serialize};
use typeshare::typeshare;

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StreamPlayer {
    pub device_public_key: String,
    pub label: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStreamPlayRequest {
    pub launch_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SessionStreamPlayOutcome {
    Ok(ActiveSession),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamPlayEndCause {
    SessionEnded,
    DeviceUnreachable,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamPlayEnded {
    pub launch_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_title: Option<String>,
    pub device_label: String,
    pub cause: StreamPlayEndCause,
}

#[cfg(test)]
mod tests {
    use crate::{
        authorization::{policy_for, PeerPolicy},
        RpcRequest,
    };

    #[test]
    fn stream_play_wire_has_owner_only_registration_and_local_invite_actions() {
        for (tag, payload, expected) in [
            (
                "app.session.streamPlay",
                serde_json::json!({"launchId":"exact"}),
                PeerPolicy::OwnerDeviceOnly,
            ),
            (
                "app.invites.list",
                serde_json::json!({}),
                PeerPolicy::LocalOnly,
            ),
            (
                "app.invites.streamPlay",
                serde_json::json!({"inviteId":"opaque"}),
                PeerPolicy::LocalOnly,
            ),
            (
                "app.invites.dismiss",
                serde_json::json!({"inviteId":"opaque"}),
                PeerPolicy::LocalOnly,
            ),
        ] {
            let request: RpcRequest =
                serde_json::from_value(serde_json::json!({"_tag":tag,"payload":payload})).unwrap();
            assert_eq!(policy_for(&request), expected, "{tag}");
            assert_eq!(
                serde_json::to_value(request).unwrap(),
                serde_json::json!({"_tag":tag,"payload":payload})
            );
        }
    }
}
