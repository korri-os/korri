//! Idle-device invites. Session facts come from same-owner peers.
use crate::{ActiveSession, RpcFailure};
use serde::{Deserialize, Serialize};
use typeshare::typeshare;

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InvitesListRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteRequest {
    pub invite_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Invite {
    pub invite_id: String,
    pub device_public_key: String,
    pub device_label: String,
    pub person_name: String,
    pub session: ActiveSession,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InvitesList {
    pub invites: Vec<Invite>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum InvitesListOutcome {
    Ok(InvitesList),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InviteDismissed {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum InviteDismissOutcome {
    Ok(InviteDismissed),
    Err(RpcFailure),
}
