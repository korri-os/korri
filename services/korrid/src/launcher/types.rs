use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::GameIdentity;

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LocalGame {
    pub id: String,
    pub title: String,
    pub system: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<GameIdentity>,
    #[serde(
        default,
        rename = "coverAssetId",
        skip_serializing_if = "Option::is_none"
    )]
    pub cover_asset_id: Option<String>,
    #[serde(default, rename = "playStats", skip_serializing_if = "Option::is_none")]
    pub play_stats: Option<crate::play_log::PlayStats>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionedFile {
    pub path: String,
    pub content: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchContributorKind {
    Runner,
    Transport,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchRouteContributor {
    pub kind: LaunchContributorKind,
    pub id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchExecutor {
    pub id: String,
    pub available: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchPublicationReservationFailure {
    Stale,
    Replay,
    NotStarted,
    AlreadyPublished,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileProvisionMode {
    Direct,
    Deferred,
}
