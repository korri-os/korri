use super::*;
use crate::{
    config::{
        resolver::{resolve_launchable_routes_for_platform, RoutePlatform},
        snapshot::ConfigSnapshotCoordinator,
        ConfigSnapshot,
    },
    discovery::{DiscoveryCoordinator, DiscoveryOptions},
    plugin_policy,
};
use std::{collections::HashMap, path::PathBuf};

struct DiscoveredGame {
    root: tempfile::TempDir,
    folder: tempfile::TempDir,
    snapshot: ConfigSnapshot,
    route: ResolvedRoute,
    environment: HashMap<&'static str, OsString>,
}

impl DiscoveredGame {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        // Use the same GBA input as the existing Linux RetroArch route tests.
        fs::write(folder.path().join("wl4.gba"), b"rom").unwrap();
        let report = DiscoveryCoordinator::new(root.path(), private.path())
            .add_location(
                folder.path(),
                &DiscoveryOptions {
                    first_seen_at: "2026-08-05T00:00:00Z".into(),
                    ..DiscoveryOptions::default()
                },
            )
            .unwrap();
        assert_eq!(report.added_games, 1);
        let state = ConfigSnapshotCoordinator::new(root.path()).reload();
        assert!(state.diagnostic.is_none(), "{:?}", state.diagnostic);
        let snapshot = (*state.snapshot).clone();
        let registry = plugin_policy::registry_for_snapshot(&snapshot).unwrap();
        let mut catalog = resolve_launchable_routes_for_platform(
            root.path(),
            &snapshot,
            &registry,
            std::iter::empty(),
            RoutePlatform::Linux,
        );
        assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
        assert_eq!(catalog.routes.len(), 1);
        let route = catalog.routes.pop().unwrap();
        assert_eq!(
            route.file_target.as_ref().unwrap().storage_id,
            report.storage_id.unwrap()
        );
        assert_ne!(route.file_target.as_ref().unwrap().storage_id, "roms");

        let executable = root.path().join("retroarch");
        let core = root.path().join("mgba.so");
        let autoconfig = root.path().join("autoconfig");
        fs::write(&executable, b"binary").unwrap();
        fs::write(&core, b"core").unwrap();
        fs::create_dir(&autoconfig).unwrap();
        Self {
            root,
            folder,
            snapshot,
            route,
            environment: HashMap::from([
                ("KORRI_RETROARCH_EXECUTABLE", executable.into_os_string()),
                ("KORRI_MGBA_CORE", core.into_os_string()),
                ("KORRI_RETROARCH_AUTOCONFIG", autoconfig.into_os_string()),
            ]),
        }
    }

    fn rom(&self) -> PathBuf {
        self.folder
            .path()
            .join(&self.route.file_target.as_ref().unwrap().path)
    }

    fn launch(&self) -> Result<LinuxLaunchSpec, LaunchError> {
        launch_route_with_env(self.root.path(), &self.snapshot, &self.route, |key| {
            self.environment.get(key).cloned()
        })
    }

    fn assert_unavailable(&self) {
        assert!(matches!(
            self.launch(),
            Err(LaunchError::RouteUnavailable(_))
        ));
        assert!(!self.root.path().join("users").exists());
    }
}

#[test]
fn discovery_registered_gba_launches_from_its_snapshot_storage_root() {
    let game = DiscoveredGame::new();
    let launch = game.launch().unwrap();
    assert_eq!(
        launch.command[5],
        game.rom().canonicalize().unwrap().display().to_string()
    );
    assert!(game
        .root
        .path()
        .join("users/default/retroarch.cfg")
        .is_file());
}

#[test]
fn configured_target_missing_after_snapshot_is_rom_missing() {
    let game = DiscoveredGame::new();
    fs::remove_file(game.rom()).unwrap();
    assert!(matches!(game.launch(), Err(LaunchError::RomMissing(_))));
    assert!(!game.root.path().join("users").exists());
}

#[test]
fn configured_target_replaced_by_directory_is_unavailable() {
    let game = DiscoveredGame::new();
    fs::remove_file(game.rom()).unwrap();
    fs::create_dir(game.rom()).unwrap();
    game.assert_unavailable();
}

#[test]
fn configured_targets_reject_traversal_absolute_and_empty_paths() {
    let mut game = DiscoveredGame::new();
    let absolute = game.rom().display().to_string();
    for path in ["../wl4.gba", absolute.as_str(), ""] {
        game.route.file_target.as_mut().unwrap().path = path.into();
        game.assert_unavailable();
    }
}

#[cfg(unix)]
#[test]
fn configured_target_replaced_by_escaping_symlink_is_unavailable() {
    let game = DiscoveredGame::new();
    let outside = game.root.path().join("outside.gba");
    fs::write(&outside, b"outside").unwrap();
    fs::remove_file(game.rom()).unwrap();
    std::os::unix::fs::symlink(outside, game.rom()).unwrap();
    game.assert_unavailable();
}

#[cfg(unix)]
#[test]
fn configured_root_and_internal_target_symlinks_use_the_canonical_file() {
    let mut game = DiscoveredGame::new();
    let canonical = game.folder.path().join("canonical.gba");
    fs::rename(game.rom(), &canonical).unwrap();
    std::os::unix::fs::symlink(&canonical, game.rom()).unwrap();
    let root_alias = game.root.path().join("selected-folder");
    std::os::unix::fs::symlink(game.folder.path(), &root_alias).unwrap();
    let storage_id = &game.route.file_target.as_ref().unwrap().storage_id;
    game.snapshot.storage.get_mut(storage_id).unwrap().root.0 = root_alias.display().to_string();
    assert_eq!(
        game.launch().unwrap().command[5],
        canonical.canonicalize().unwrap().display().to_string()
    );
}

#[cfg(unix)]
#[test]
fn implicit_roms_root_cannot_escape_korri_through_a_symlink() {
    let mut game = DiscoveredGame::new();
    std::os::unix::fs::symlink(game.folder.path(), game.root.path().join("roms")).unwrap();
    game.route.file_target.as_mut().unwrap().storage_id = "roms".into();
    game.assert_unavailable();
}
