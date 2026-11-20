use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::GameIdentity;

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LocalGame {
    pub id: String,
    pub title: String,
    /// The system's title, else its id.
    pub system: String,
    /// The route's system id (`ResolvedRoute.system_id`). Absent for a
    /// release that names no system.
    #[serde(default, rename = "systemId", skip_serializing_if = "Option::is_none")]
    pub system_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<GameIdentity>,
    #[serde(
        default,
        rename = "coverAssetId",
        skip_serializing_if = "Option::is_none"
    )]
    pub cover_asset_id: Option<String>,
    /// Every image korrid stores for the game, of every kind and from every
    /// art plugin (ADR 0091). Absent when it has none. `coverAssetId` is one
    /// of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Vec<LocalGameMedia>>,
    #[serde(default, rename = "playStats", skip_serializing_if = "Option::is_none")]
    pub play_stats: Option<crate::play_log::PlayStats>,
}

/// One stored image of a local game.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalGameMedia {
    /// `logo`, `picture` or `icon`.
    pub kind: korri_surface_treaty::SurfaceMediaKind,
    /// The asset id, as `coverAssetId`: `<sha256>.<png|jpg|webp>`.
    pub asset_id: String,
    /// The image's own size in pixels, as korrid measured it.
    pub width: u32,
    pub height: u32,
    /// One colour of the image as lowercase `#rrggbb`. Absent when the image
    /// has no clear colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
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
