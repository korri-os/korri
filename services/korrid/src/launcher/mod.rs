pub mod linux_plugin;
pub mod plugin_launch;
pub mod typed_settings;
mod types;

use crate::{
    config::{
        resolver::{self, ResolvedRoute, RouteDiagnostic},
        snapshot::{ConfigSnapshotState, SnapshotAuthorization},
    },
    discovery,
    game_assets::{AssetOwnerIdentity, GameAssetRepository},
    plugin::PluginRegistry,
};
use std::{collections::BTreeMap, path::Path};

pub(crate) use types::derive_retroarch_control_port;

pub use types::{
    AndroidActiveLaunch, AndroidComponent, AndroidMoonlightEffect, FileProvisionMode,
    LaunchContext, LaunchContributorKind, LaunchDisposition, LaunchExecutor, LaunchForegroundKind,
    LaunchForegroundRule, LaunchPublicationReservationFailure, LaunchPublicationReservations,
    LaunchRouteContributor, LaunchSpec, LocalGame, MoonlightLaunchAuthority, MoonlightLaunchSpec,
    MoonlightLaunchVerificationFailure, PlatformEffect, PlatformInstruction,
    PlatformInstructionVerificationFailure, PlatformInstructionVerifier, ProvisionedFile,
};

#[derive(Clone, Debug, PartialEq)]
pub struct LocalGameCatalog {
    pub games: Vec<LocalGame>,
    pub diagnostics: Vec<RouteDiagnostic>,
}

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("unknown local game: {0}")]
    UnknownGame(String),
    #[error("local ROM is missing: {0}")]
    RomMissing(String),
    #[error("local storage is unavailable: {0}")]
    StorageAccess(String),
    #[error("failed to provision RetroArch config: {0}")]
    Config(String),
    #[error("local configuration is unauthorized: {0}")]
    ConfigUnauthorized(String),
    #[error("local route is unavailable: {0}")]
    RouteUnavailable(String),
    #[error("local route collides with another route: {0}")]
    RouteCollision(String),
}

/// Static local playable ids owned outside configuration. First-party Android
/// integrations are plugin routes, so this is intentionally empty.
pub fn static_playable_ids() -> Vec<&'static str> {
    Vec::new()
}

/// Everything playable on this device, from the immutable configuration state
/// returned by the caller's reload plus every built-in launcher it knows.
pub fn local_games(
    root: &Path,
    config_state: &ConfigSnapshotState,
    registry: &PluginRegistry,
) -> LocalGameCatalog {
    local_games_with_cover_assets(root, None, config_state, registry)
}

pub fn local_games_with_cover_assets(
    readable_root: &Path,
    private_root: Option<&Path>,
    config_state: &ConfigSnapshotState,
    registry: &PluginRegistry,
) -> LocalGameCatalog {
    let mut diagnostics = Vec::new();
    let mut games = Vec::new();
    let cover_asset_ids = private_root
        .map(|private_root| cover_asset_ids(readable_root, private_root))
        .unwrap_or_default();
    let play_stats = private_root
        .map(|root| crate::play_log::PlayLogRepository::new(root).load_all_stats())
        .unwrap_or_default();

    if config_state.authorization == SnapshotAuthorization::Authorized {
        let catalog = resolver::resolve_launchable_routes(
            readable_root,
            &config_state.snapshot,
            registry,
            static_playable_ids(),
        );
        diagnostics.extend(catalog.diagnostics);
        // A route reaching here already resolved against the enabled registry
        // and its installed files, so listing it needs no second opinion from
        // korrid about which family produced it. The catalogue yields one route
        // per admitting runner; the library shows one entry per game and the
        // runner choice belongs to app.local-games.routes.
        let mut listed = std::collections::BTreeSet::new();
        for route in catalog.routes {
            if listed.insert(route.playable_id.clone()) {
                games.push(local_game_from_route(route, &cover_asset_ids, &play_stats));
            }
        }
    }

    LocalGameCatalog { games, diagnostics }
}

fn local_game_from_route(
    route: ResolvedRoute,
    cover_asset_ids: &BTreeMap<String, String>,
    play_stats: &BTreeMap<String, crate::play_log::PlayStats>,
) -> LocalGame {
    let cover_asset_id = cover_asset_ids.get(&route.playable_id).cloned();
    let stats = play_stats
        .get(&route.playable_id)
        .cloned()
        .filter(|stats| stats.play_count > 0 || stats.last_played.is_some());
    LocalGame {
        id: route.playable_id,
        title: route.title.unwrap_or(route.release_id),
        system: route.system_title.unwrap_or(route.system_id),
        identity: route.identity,
        cover_asset_id,
        play_stats: stats,
    }
}

fn cover_asset_ids(readable_root: &Path, private_root: &Path) -> BTreeMap<String, String> {
    let Ok(games) = discovery::reconcile::owned_discovery_games(readable_root, private_root) else {
        return BTreeMap::new();
    };
    let repo = GameAssetRepository::new(private_root);
    let owners: Vec<_> = games
        .into_iter()
        .map(|game| AssetOwnerIdentity {
            playable_id: game.playable_id,
            release_id: game.release_id,
            release_fingerprint: game.release_fingerprint,
            rom_identity: game.rom_identity,
        })
        .collect();
    repo.matching_tile_asset_ids(&owners).unwrap_or_default()
}

fn launch_error_from_route_diagnostic(diagnostic: RouteDiagnostic) -> LaunchError {
    match diagnostic.code {
        resolver::RouteDiagnosticCode::LocalRomMissing => {
            LaunchError::RomMissing(diagnostic.message)
        }
        resolver::RouteDiagnosticCode::LocalRouteUnavailable => {
            LaunchError::RouteUnavailable(diagnostic.message)
        }
        resolver::RouteDiagnosticCode::LocalRouteCollision => {
            LaunchError::RouteCollision(diagnostic.message)
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::snapshot::ConfigSnapshotCoordinator;
    use tempfile::tempdir;

    /// One library entry per game, even when several installed runners admit
    /// it. The catalogue yields a route per runner; choosing between them is
    /// app.local-games.routes, not the library.
    #[test]
    fn a_game_two_runners_admit_is_listed_once() {
        let root = tempdir().unwrap();
        let registry = crate::plugin_test_fixtures::installed_pair(root.path());
        std::fs::create_dir_all(root.path().join("roms")).unwrap();
        std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        crate::plugin_test_fixtures::write_gba_library(root.path());
        let state = ConfigSnapshotCoordinator::new(root.path()).reload();

        let catalog = local_games(root.path(), &state, &registry);

        assert_eq!(
            catalog
                .games
                .iter()
                .map(|game| game.id.as_str())
                .collect::<Vec<_>>(),
            vec![crate::plugin_test_fixtures::GBA_ID],
            "diagnostics: {:?}",
            catalog.diagnostics
        );
    }
}
