#[path = "fixtures/readable.rs"]
mod readable;
use korrid::{
    config::{
        decode_config_documents,
        resolver::{self, RoutePlatform},
        snapshot::{ConfigSnapshotCoordinator, FILE_NAMES},
    },
    discovery::{DiscoveryCoordinator, DiscoveryOptions},
    plugin,
    plugin::{load_plugin_source, PluginRegistry},
    plugin_installation, plugin_policy,
};
#[path = "../src/plugin_test_fixtures.rs"]
mod native_packages;
use serde_yaml::Value;
use std::{fs, path::Path};

fn documents(root: &Path) -> Vec<Vec<u8>> {
    FILE_NAMES
        .iter()
        .map(|name| fs::read(root.join(name)).unwrap())
        .collect()
}
fn snapshot(root: &Path) -> std::sync::Arc<korrid::config::ConfigSnapshot> {
    let state = ConfigSnapshotCoordinator::new(root).reload();
    assert!(state.diagnostic.is_none(), "{:?}", state.diagnostic);
    state.snapshot
}

#[test]
fn all_copies_and_overlapping_roots_survive_restart_without_minting_new_games() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let roms = tempfile::tempdir().unwrap();
    let child = roms.path().join("child");
    fs::create_dir(&child).unwrap();
    fs::write(child.join("one.gba"), b"same").unwrap();
    fs::write(child.join("two.gba"), b"same").unwrap();
    let discovery = DiscoveryCoordinator::new(root.path(), private.path());
    let options = DiscoveryOptions::default();
    let first = discovery
        .add_location(&child, &options)
        .unwrap()
        .storage_id
        .unwrap();
    let second = discovery
        .add_location(roms.path(), &options)
        .unwrap()
        .storage_id
        .unwrap();
    let state = snapshot(root.path());
    assert_eq!(state.games.len(), 1);
    assert_eq!(state.releases.len(), 1);
    assert_eq!(state.locations.values().next().unwrap().len(), 4);
    let before = documents(root.path());
    let restarted = DiscoveryCoordinator::new(root.path(), private.path());
    restarted.rescan(&options).unwrap();
    assert_eq!(documents(root.path()), before);
    restarted.remove_location(&first, &options).unwrap();
    assert_eq!(
        snapshot(root.path())
            .locations
            .values()
            .next()
            .unwrap()
            .len(),
        2
    );
    restarted.remove_location(&second, &options).unwrap();
    let final_state = snapshot(root.path());
    assert!(final_state.locations.is_empty());
    assert!(final_state.storage.is_empty());
    assert_eq!(final_state.games, state.games);
    assert_eq!(final_state.releases, state.releases);
    assert_eq!(&documents(root.path())[1..], &before[1..]);
    restarted.add_location(&child, &options).unwrap();
    assert_eq!(snapshot(root.path()).games, state.games);
}

#[test]
fn same_release_copy_ownership_survives_rescan_and_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let roms = tempfile::tempdir().unwrap();
    for index in 0..256 {
        fs::write(roms.path().join(format!("copy-{index}.gba")), b"same ROM").unwrap();
    }
    let discovery = DiscoveryCoordinator::new(root.path(), private.path());
    let options = DiscoveryOptions::default();
    let storage = discovery
        .add_location(roms.path(), &options)
        .unwrap()
        .storage_id
        .unwrap();
    let initial = snapshot(root.path());
    assert_eq!(initial.games.len(), 1);
    assert_eq!(initial.locations.values().next().unwrap().len(), 256);
    discovery.rescan(&options).unwrap();
    assert_eq!(*snapshot(root.path()), *initial);
    discovery.remove_location(&storage, &options).unwrap();
    let removed = snapshot(root.path());
    assert!(removed.locations.is_empty());
    assert!(removed.storage.is_empty());
    assert_eq!(removed.games, initial.games);
}

#[test]
fn byte_replacement_removes_only_the_owned_old_location_not_the_catalog() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let roms = tempfile::tempdir().unwrap();
    let path = roms.path().join("game.gba");
    fs::write(&path, b"old").unwrap();
    let discovery = DiscoveryCoordinator::new(root.path(), private.path());
    discovery
        .add_location(roms.path(), &DiscoveryOptions::default())
        .unwrap();
    let before = snapshot(root.path());
    let old_sha = before.releases.keys().next().unwrap();
    fs::write(&path, b"new longer bytes").unwrap();
    discovery.rescan(&DiscoveryOptions::default()).unwrap();
    let after = snapshot(root.path());
    assert_eq!(after.games.len(), 2);
    assert_eq!(after.releases[old_sha], before.releases[old_sha]);
    assert!(!after.locations.contains_key(old_sha));
    assert_eq!(after.locations.len(), 1);
}

#[test]
fn catalog_title_edits_do_not_transfer_location_ownership() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let roms = tempfile::tempdir().unwrap();
    fs::write(roms.path().join("game.gba"), b"rom").unwrap();
    let discovery = DiscoveryCoordinator::new(root.path(), private.path());
    let storage = discovery
        .add_location(roms.path(), &DiscoveryOptions::default())
        .unwrap()
        .storage_id
        .unwrap();
    let path = root.path().join("catalog/games.yaml");
    let authored = fs::read_to_string(&path)
        .unwrap()
        .replace("title: game", "title: Authored");
    fs::write(&path, &authored).unwrap();
    discovery
        .remove_location(&storage, &DiscoveryOptions::default())
        .unwrap();
    let after = snapshot(root.path());
    assert!(after.locations.is_empty());
    assert!(after.storage.is_empty());
    assert_eq!(fs::read_to_string(path).unwrap(), authored);
}

#[test]
fn authored_release_content_and_location_survive_rescan_and_storage_removal() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let roms = tempfile::tempdir().unwrap();
    fs::write(roms.path().join("game.gba"), b"rom").unwrap();
    let discovery = DiscoveryCoordinator::new(root.path(), private.path());
    let storage = discovery
        .add_location(roms.path(), &DiscoveryOptions::default())
        .unwrap()
        .storage_id
        .unwrap();
    let release_path = root.path().join("catalog/releases.yaml");
    let authored_release = format!(
        "{}    display:\n      title: Authored release\n",
        fs::read_to_string(&release_path).unwrap()
    );
    fs::write(&release_path, &authored_release).unwrap();
    let device_path = root.path().join("device.yaml");
    let mut device: Value =
        serde_yaml::from_str(&fs::read_to_string(&device_path).unwrap()).unwrap();
    let location = device["locations"]
        .as_mapping_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    location[0]["discovery"]["first-seen-at"] = "authored-time".into();
    fs::write(&device_path, serde_yaml::to_string(&device).unwrap()).unwrap();
    discovery.rescan(&DiscoveryOptions::default()).unwrap();
    discovery
        .remove_location(&storage, &DiscoveryOptions::default())
        .unwrap();
    let after = snapshot(root.path());
    assert!(after.storage.contains_key(&storage));
    assert_eq!(after.locations.values().next().unwrap().len(), 1);
    assert_eq!(fs::read_to_string(release_path).unwrap(), authored_release);
}

#[test]
fn missing_implicit_copy_does_not_hide_an_existing_explicit_copy_from_launch_or_either_platform() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("roms")).unwrap();
    let private = tempfile::tempdir().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    fs::write(first.path().join("game.gba"), b"rom").unwrap();
    fs::write(second.path().join("game.gba"), b"rom").unwrap();
    let discovery = DiscoveryCoordinator::new(root.path(), private.path());
    discovery
        .add_location(first.path(), &DiscoveryOptions::default())
        .unwrap();
    let second_id = discovery
        .add_location(second.path(), &DiscoveryOptions::default())
        .unwrap()
        .storage_id
        .unwrap();
    fs::remove_file(first.path().join("game.gba")).unwrap();
    let device_path = root.path().join("device.yaml");
    let mut device: Value =
        serde_yaml::from_str(&fs::read_to_string(&device_path).unwrap()).unwrap();
    device["locations"]
        .as_mapping_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()[0]["storage"] = "roms".into();
    fs::write(&device_path, serde_yaml::to_string(&device).unwrap()).unwrap();
    let loaded = ConfigSnapshotCoordinator::new(root.path()).reload();
    let state = &loaded.snapshot;
    let registry = plugin_policy::registry_for_snapshot(state).unwrap();
    let spec = korrid::launcher::launch_game(
        root.path(),
        state.games.keys().next().unwrap(),
        korrid::launcher::FileProvisionMode::Deferred,
        &loaded,
        &registry,
        50000,
    )
    .unwrap();
    assert_eq!(
        spec.extras["ROM"],
        second.path().join("game.gba").display().to_string()
    );
    let native_registry = native_packages::installed(private.path());
    for platform in [RoutePlatform::Android, RoutePlatform::Linux] {
        let route = resolver::resolve_route_for_platform(
            root.path(),
            state,
            if platform == RoutePlatform::Linux {
                &native_registry
            } else {
                &registry
            },
            [],
            state.games.keys().next().unwrap(),
            platform,
        )
        .unwrap();
        assert_eq!(route.file_target.unwrap().storage_id, second_id);
    }
    fs::remove_file(second.path().join("game.gba")).unwrap();
    let error = korrid::launcher::launch_game(
        root.path(),
        state.games.keys().next().unwrap(),
        korrid::launcher::FileProvisionMode::Deferred,
        &loaded,
        &registry,
        50000,
    )
    .unwrap_err();
    assert!(
        matches!(error, korrid::launcher::LaunchError::RomMissing(_)),
        "{error:?}"
    );
    assert!(
        korrid::launcher::local_games(root.path(), &loaded, &registry)
            .games
            .is_empty()
    );
}

#[test]
fn runtime_ambiguity_counts_only_candidates_capable_of_the_requested_platform() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/game.gba"), b"rom").unwrap();
    let source = plugin_policy::MGBA_PLUGIN_SOURCE.replace(
        "mgba: {",
        r#"other: {
          id: "@korri:mgba/other", kind: "libretro-core", app: "@korri:retroarch/retroarch",
          path: "/cores/other.so", supports: { systems: ["gba"] },
        }, mgba: {"#,
    );
    let plugins = plugin_policy::bundled_plugins()
        .unwrap()
        .into_iter()
        .map(|plugin| {
            if plugin.id() == "@korri:mgba" {
                load_plugin_source("@korri", &source).unwrap()
            } else {
                plugin
            }
        })
        .collect();
    let registry = PluginRegistry::new(
        plugins,
        plugin_policy::resolve_enabled_plugin_ids([plugin_policy::bundled_plugin_policy_layer()]),
    )
    .unwrap();
    let state = decode_config_documents(
        &readable::gba_locations("roms", "game.gba", false),
        &readable::gba_games(),
        &readable::gba_releases(),
    )
    .unwrap();
    let error =
        resolver::resolve_route(root.path(), &state, &registry, [], readable::GBA_ID).unwrap_err();
    assert!(
        error.message.contains("ambiguous runtimes"),
        "{}",
        error.message
    );
    let route = resolver::resolve_route_for_platform(
        root.path(),
        &state,
        &native_packages::installed(root.path()),
        [],
        readable::GBA_ID,
        RoutePlatform::Linux,
    )
    .unwrap();
    assert_eq!(route.runtime.unwrap().id, "@korri:mgba/mgba");
}

#[test]
fn launcher_ambiguity_counts_only_candidates_capable_of_the_requested_platform() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/game.gba"), b"rom").unwrap();
    let source = plugin_policy::RETROARCH_PLUGIN_SOURCE.replace(
        "retroarch: {",
        r#""android-only": {
        id: "@korri:retroarch/android-only", plugin: "@korri:retroarch", command: "retroarch",
        systems: ["gba"], android: { packageName: "com.korri.retroarch", className: "com.retroarch.browser.retroactivity.RetroActivityFuture" },
      }, retroarch: {"#,
    );
    let plugins = plugin_policy::bundled_plugins()
        .unwrap()
        .into_iter()
        .map(|plugin| {
            if plugin.id() == "@korri:retroarch" {
                load_plugin_source("@korri", &source).unwrap()
            } else {
                plugin
            }
        })
        .collect();
    let registry = PluginRegistry::new(
        plugins,
        plugin_policy::resolve_enabled_plugin_ids([plugin_policy::bundled_plugin_policy_layer()]),
    )
    .unwrap();
    let state = decode_config_documents(
        &readable::gba_locations("roms", "game.gba", false),
        &readable::gba_games(),
        &readable::gba_releases(),
    )
    .unwrap();
    let error =
        resolver::resolve_route(root.path(), &state, &registry, [], readable::GBA_ID).unwrap_err();
    assert!(error.message.contains("ambiguous launchers"), "{error:?}");
    let route = resolver::resolve_route_for_platform(
        root.path(),
        &state,
        &native_packages::installed(root.path()),
        [],
        readable::GBA_ID,
        RoutePlatform::Linux,
    )
    .unwrap();
    assert_eq!(route.launcher_id, "@korri:retroarch/retroarch");
}

#[test]
fn ten_thousand_existing_locations_rescan_without_rewriting_or_minting_games() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let roms = tempfile::tempdir().unwrap();
    for index in 0..10_000u32 {
        fs::write(
            roms.path().join(format!("{index:05}.gba")),
            index.to_le_bytes(),
        )
        .unwrap();
    }
    let discovery = DiscoveryCoordinator::new(root.path(), private.path());
    let options = DiscoveryOptions::default();
    let first = discovery.add_location(roms.path(), &options).unwrap();
    assert_eq!(first.added_games, 10_000);
    let before = documents(root.path());
    let again = discovery.rescan(&options).unwrap();
    assert_eq!(again.scan.candidates.len(), 10_000);
    assert_eq!(again.added_games, 0);
    assert_eq!(again.scan.hashed_bytes, 0);
    assert_eq!(documents(root.path()), before);
}

#[test]
fn any_complete_release_launches_in_catalog_order_without_a_preference_fold() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/game.gba"), b"rom").unwrap();
    fs::write(root.path().join("roms/other.gba"), b"other").unwrap();
    let other_sha = format!("sha256:{}", "a".repeat(64));
    let games = readable::gba_games().replace(
        &format!("['{}']", readable::GBA_RELEASE),
        &format!("['{}', '{other_sha}']", readable::GBA_RELEASE),
    );
    let releases = format!(
        "{}  '{other_sha}':\n    game: {}\n    system: gba\n",
        readable::gba_releases(),
        readable::GBA_ID
    );
    let device = format!(
        "{}  '{other_sha}':\n    - storage: roms\n      path: other.gba\n",
        readable::gba_locations("roms", "game.gba", false)
    );
    let state = decode_config_documents(&device, &games, &releases).unwrap();
    let registry = plugin_policy::registry_for_snapshot(&state).unwrap();
    let route =
        resolver::resolve_route(root.path(), &state, &registry, [], readable::GBA_ID).unwrap();
    assert_eq!(route.release_id, readable::GBA_RELEASE);

    let reordered = games.replace(
        &format!("['{}', '{other_sha}']", readable::GBA_RELEASE),
        &format!("['{other_sha}', '{}']", readable::GBA_RELEASE),
    );
    let reordered = decode_config_documents(&device, &reordered, &releases).unwrap();
    let route =
        resolver::resolve_route(root.path(), &reordered, &registry, [], readable::GBA_ID).unwrap();
    assert_eq!(route.release_id, other_sha);

    fs::remove_file(root.path().join("roms/game.gba")).unwrap();
    let route =
        resolver::resolve_route(root.path(), &state, &registry, [], readable::GBA_ID).unwrap();
    assert_eq!(route.release_id, other_sha);
}

#[test]
fn a_broken_catalog_link_retains_the_last_known_good_snapshot_until_restored() {
    let root = tempfile::tempdir().unwrap();
    readable::android(root.path());
    let coordinator = ConfigSnapshotCoordinator::new(root.path());
    let good = coordinator.reload();
    let releases = root.path().join("catalog/releases.yaml");
    fs::write(
        &releases,
        readable::ANDROID_RELEASES.replace(readable::ANDROID_ID, readable::OTHER_ID),
    )
    .unwrap();
    let broken = coordinator.reload();
    assert!(broken.diagnostic.is_some());
    assert_eq!(broken.generation, good.generation);
    assert_eq!(broken.snapshot, good.snapshot);
    fs::write(releases, readable::ANDROID_RELEASES).unwrap();
    let restored = coordinator.reload();
    assert!(restored.diagnostic.is_none());
    assert!(restored.generation > good.generation);
}

#[test]
fn snapshot_probe_exercises_the_three_document_loader_and_retained_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_config_snapshot_probe"))
        .arg(root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("tmnt present: yes"), "{stdout}");
    assert!(stdout.contains("retained tmnt: yes"), "{stdout}");
    assert!(stdout.contains("release records: 1"), "{stdout}");
    assert!(stdout.contains("LocalConfigReloadFailed"), "{stdout}");
}

#[test]
fn route_probe_uses_the_catalog_game_id_in_enabled_and_disabled_reports() {
    let root = tempfile::tempdir().unwrap();
    readable::android(root.path());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_plugin_route_probe"))
        .arg(root.path())
        .arg("--review")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(readable::ANDROID_ID), "{stdout}");
    assert!(
        stdout.contains("@korri:android-app:com.playdigious.tmnt"),
        "{stdout}"
    );
    assert!(
        stdout.contains("no launcher supports system android"),
        "{stdout}"
    );
}

#[test]
fn authored_physical_file_is_protected_across_storage_aliases_after_byte_change() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let roms = tempfile::tempdir().unwrap();
    let path = roms.path().join("game.gba");
    fs::write(&path, b"old").unwrap();
    let discovery = DiscoveryCoordinator::new(root.path(), private.path());
    discovery
        .add_location(roms.path(), &DiscoveryOptions::default())
        .unwrap();
    let device_path = root.path().join("device.yaml");
    let mut device: Value =
        serde_yaml::from_str(&fs::read_to_string(&device_path).unwrap()).unwrap();
    device["storage"]["authored"]["root"] = roms.path().display().to_string().into();
    let locations = device["locations"]
        .as_mapping_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()
        .as_sequence_mut()
        .unwrap();
    let mut authored = locations[0].clone();
    authored["storage"] = "authored".into();
    authored.as_mapping_mut().unwrap().remove("discovery");
    locations.push(authored);
    fs::write(&device_path, serde_yaml::to_string(&device).unwrap()).unwrap();
    let before = documents(root.path());
    fs::write(&path, b"changed bytes, new hash").unwrap();
    discovery.rescan(&DiscoveryOptions::default()).unwrap();
    assert_eq!(documents(root.path()), before);
}

#[test]
fn old_document_paths_are_not_read_or_rewritten() {
    let root = tempfile::tempdir().unwrap();
    for name in ["config.yaml", "library.yaml"] {
        fs::write(root.path().join(name), "not: [valid yaml").unwrap();
    }
    let state = snapshot(root.path());
    assert!(state.games.is_empty());
    for name in ["config.yaml", "library.yaml"] {
        assert_eq!(
            fs::read_to_string(root.path().join(name)).unwrap(),
            "not: [valid yaml"
        );
    }
}
