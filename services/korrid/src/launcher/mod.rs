pub mod linux_plugin;
mod native_config_transaction;
mod native_toml;
pub mod plugin_launch;
pub(crate) mod redaction;
pub mod runtime;
#[cfg(test)]
mod shared_settings_tests;
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

pub use types::{
    FileProvisionMode, LaunchContributorKind, LaunchExecutor, LaunchPublicationReservationFailure,
    LaunchRouteContributor, LocalGame, LocalGameMedia, ProvisionedFile,
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

/// Static local playable ids owned outside configuration. There are none.
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
    let art = private_root
        .map(|private_root| stored_art(readable_root, private_root, registry))
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
                games.push(local_game_from_route(route, &art, &play_stats));
            }
        }
    }

    LocalGameCatalog { games, diagnostics }
}

fn local_game_from_route(
    route: ResolvedRoute,
    art: &StoredArt,
    play_stats: &BTreeMap<String, crate::play_log::PlayStats>,
) -> LocalGame {
    let cover_asset_id = art.covers.get(&route.playable_id).cloned();
    let media = art.media.get(&route.playable_id).cloned();
    let stats = play_stats
        .get(&route.playable_id)
        .cloned()
        .filter(|stats| stats.play_count > 0 || stats.last_played.is_some());
    LocalGame {
        id: route.playable_id,
        title: route.title.unwrap_or(route.release_id),
        system_id: (!route.system_id.is_empty()).then(|| route.system_id.clone()),
        system: route.system_title.unwrap_or(route.system_id),
        identity: route.identity,
        cover_asset_id,
        media,
        play_stats: stats,
    }
}

/// The images korrid stores for local catalog games, by playable id: the
/// largest square-ish picture as the cover, and every image as media.
#[derive(Default)]
struct StoredArt {
    covers: BTreeMap<String, String>,
    media: BTreeMap<String, Vec<LocalGameMedia>>,
}

fn stored_art(readable_root: &Path, private_root: &Path, registry: &PluginRegistry) -> StoredArt {
    let Ok(games) = discovery::reconcile::artwork_games(readable_root, private_root, registry)
    else {
        return StoredArt::default();
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
    let media = repo
        .matching_media(&owners)
        .unwrap_or_default()
        .into_iter()
        .map(|(playable_id, stored)| {
            let media = stored
                .into_iter()
                .map(|stored| LocalGameMedia {
                    kind: stored.kind,
                    asset_id: stored.asset_id,
                    width: stored.width,
                    height: stored.height,
                    color: stored.color,
                })
                .collect();
            (playable_id, media)
        })
        .collect();
    StoredArt {
        covers: repo.matching_cover_asset_ids(&owners).unwrap_or_default(),
        media,
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

    /// A discovered game with a logo and two pictures from one plugin and an
    /// icon from another: the square-ish picture is the cover, and media
    /// lists every image with its kind, size and colour.
    #[test]
    fn a_listed_game_carries_every_stored_image_beside_its_cover() {
        use crate::game_assets::{test_image, AssetCandidate};
        use image::ImageFormat;
        use korri_surface_treaty::SurfaceMediaKind;
        let root = tempdir().unwrap();
        let private = tempdir().unwrap();
        let roms = tempdir().unwrap();
        std::fs::write(roms.path().join("Wario Land 4.gba"), b"rom").unwrap();
        let registry = crate::plugin_test_fixtures::installed(root.path());
        discovery::reconcile::DiscoveryCoordinator::for_tests(root.path(), private.path())
            .pair(
                roms.path().to_path_buf(),
                "gba",
                &discovery::DiscoveryOptions::default(),
            )
            .unwrap();
        let state = ConfigSnapshotCoordinator::new(root.path()).reload();
        let listed = |private_root| {
            local_games_with_cover_assets(root.path(), private_root, &state, &registry)
                .games
                .into_iter()
                .next()
                .expect("the discovered game is listed")
        };
        // No stored art: neither a cover nor a media list.
        let bare = listed(Some(private.path()));
        assert_eq!((bare.cover_asset_id, bare.media), (None, None));

        let game = discovery::reconcile::artwork_games(root.path(), private.path(), &registry)
            .unwrap()
            .remove(0);
        let owner = AssetOwnerIdentity {
            playable_id: game.playable_id.clone(),
            release_id: game.release_id.clone(),
            release_fingerprint: game.release_fingerprint.clone(),
            rom_identity: game.rom_identity.clone(),
        };
        let repository = GameAssetRepository::new(private.path());
        let store = |provider: &str, kind, images: Vec<Vec<u8>>| {
            repository
                .replace(
                    owner.clone(),
                    provider,
                    kind,
                    images
                        .into_iter()
                        .map(|bytes| AssetCandidate {
                            bytes,
                            declared_width: None,
                            declared_height: None,
                            source_id: None,
                            source_url: None,
                        })
                        .collect(),
                )
                .unwrap()
                .stored
                .into_iter()
                .map(|stored| stored.asset_id)
                .collect::<Vec<_>>()
        };
        let art = crate::plugin_test_fixtures::ART_PLUGIN_ID;
        let logo = store(
            art,
            SurfaceMediaKind::Logo,
            vec![test_image(40, 13, ImageFormat::Png, true, [240, 200, 40])],
        );
        let pictures = store(
            art,
            SurfaceMediaKind::Picture,
            vec![
                test_image(512, 512, ImageFormat::Png, true, [200, 60, 40]),
                test_image(600, 900, ImageFormat::Png, false, [20, 80, 220]),
            ],
        );
        let icon = store(
            "@korri:other",
            SurfaceMediaKind::Icon,
            vec![test_image(32, 32, ImageFormat::Png, false, [128, 128, 128])],
        );

        let game = listed(Some(private.path()));
        assert_eq!(game.cover_asset_id.as_ref(), Some(&pictures[0]));
        // On the wire: camelCase, every image, the cover among them.
        let wire = serde_json::to_value(&game).unwrap();
        assert_eq!(
            wire["media"],
            serde_json::json!([
                { "kind": "icon", "assetId": icon[0], "width": 32, "height": 32 },
                { "kind": "logo", "assetId": logo[0], "width": 40, "height": 13, "color": "#f0c828" },
                { "kind": "picture", "assetId": pictures[0], "width": 512, "height": 512, "color": "#c83c28" },
                { "kind": "picture", "assetId": pictures[1], "width": 600, "height": 900, "color": "#1450dc" },
            ])
        );
        // The fixed roles are gone: no `role`, and no `tile`.
        for item in [
            serde_json::json!({ "role": "poster", "assetId": pictures[1], "width": 600, "height": 900 }),
            serde_json::json!({ "kind": "tile", "assetId": pictures[0], "width": 512, "height": 512 }),
        ] {
            assert!(
                serde_json::from_value::<LocalGameMedia>(item.clone()).is_err(),
                "{item}"
            );
        }
        // Without the private root there is no stored art to speak of.
        let unknown = listed(None);
        assert_eq!((unknown.cover_asset_id, unknown.media), (None, None));
    }

    /// The listed game carries the route's system: its id as the route has
    /// it, and its title in words.
    #[test]
    fn a_listed_game_names_its_system_by_id_and_by_title() {
        let root = tempdir().unwrap();
        let registry = crate::plugin_test_fixtures::installed_pair(root.path());
        std::fs::create_dir_all(root.path().join("roms")).unwrap();
        std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        crate::plugin_test_fixtures::write_gba_library(root.path());
        let state = ConfigSnapshotCoordinator::new(root.path()).reload();

        let catalog = local_games(root.path(), &state, &registry);

        let game = &catalog.games[0];
        assert_eq!(game.system_id.as_deref(), Some("gba"));
        assert_eq!(game.system, "Game Boy Advance");
        assert_eq!(
            serde_json::to_value(game).unwrap()["systemId"],
            serde_json::json!("gba")
        );
    }
}
