use super::*;
use crate::discovery::identify::{ContentIdentity, EvidenceLevel, FoundUnit, SourceScan};

fn fixture() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    DiscoveryCoordinator,
    ScanReport,
) {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    ensure_fixed_files(root.path()).unwrap();
    fs::write(root.path().join(DEVICE_FILE_NAME), "# keep authored settings\nhost: {title: desk}\nstorage: {gba: {root: /games}}\nsources: {gba: {kind: [files], storage: gba, system: gba}}\n").unwrap();
    let writer = DiscoveryCoordinator::new(root.path(), private.path());
    let scan = ScanReport {
        scanned: vec![SourceScan {
            source_id: "gba".into(),
            units: vec![FoundUnit {
                source_id: "gba".into(),
                storage_id: "gba".into(),
                path: "Game.gba".into(),
                parts: vec!["Game.gba".into()],
                tracks: Vec::new(),
                system: "gba".into(),
                title: "Game".into(),
                version: None,
                region: None,
                identity: ContentIdentity::Hash {
                    sha256: format!("sha256:{}", "a".repeat(64)),
                    sha1: format!("sha1:{}", "a".repeat(40)),
                },
                evidence: EvidenceLevel::Extension,
            }],
            ..SourceScan::default()
        }],
        ..ScanReport::default()
    };
    (root, private, writer, scan)
}
#[test]
fn run_publishes_library_locations_and_ownership_and_unchanged_rescan_writes_nothing() {
    let (root, private, writer, report) = fixture();
    let options = DiscoveryOptions {
        first_seen_at: "2026-10-16T00:00:00Z".into(),
        ..DiscoveryOptions::default()
    };
    assert_eq!(writer.run(&report, &options).unwrap().added_games, 1);
    let before = Documents::read(root.path()).unwrap();
    let owned = fs::read(state_file(private.path(), OWNERSHIP_FILE)).unwrap();
    let modified = fs::metadata(root.path().join(LIBRARY_FILE_NAME))
        .unwrap()
        .modified()
        .unwrap();
    let mut later = options.clone();
    later.first_seen_at = "2026-10-17T00:00:00Z".into();
    assert_eq!(writer.run(&report, &later).unwrap().added_games, 0);
    assert_eq!(Documents::read(root.path()).unwrap(), before);
    assert_eq!(
        fs::read(state_file(private.path(), OWNERSHIP_FILE)).unwrap(),
        owned
    );
    assert_eq!(
        fs::metadata(root.path().join(LIBRARY_FILE_NAME))
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
    assert!(!root.path().join("catalog/games.yaml").exists());
    assert!(!root.path().join("catalog/releases.yaml").exists());
    assert_eq!(
        before.validate().unwrap().host.unwrap().title.as_deref(),
        Some("desk")
    );
}
#[test]
fn pairing_scans_and_publishes_once() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let games = tempfile::tempdir().unwrap();
    fs::write(games.path().join("Game.gba"), b"game").unwrap();
    let writer = DiscoveryCoordinator::for_tests(root.path(), private.path());
    writer
        .pair(games.path().into(), "gba", &DiscoveryOptions::default())
        .unwrap();
    assert_eq!(
        Documents::read(root.path())
            .unwrap()
            .validate()
            .unwrap()
            .library
            .len(),
        1
    );
}

#[test]
fn source_views_is_read_only_and_can_run_under_the_shared_write_lock() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let writer = DiscoveryCoordinator::new(root.path(), private.path());
    let lock = writer.write_lock();
    let _guard = lock.lock().unwrap();
    assert!(writer.source_views().unwrap().is_empty());
    assert!(!root.path().join(DEVICE_FILE_NAME).exists());
    fs::write(root.path().join(DEVICE_FILE_NAME), "storage: {gba: {root: /games}}\nsources: {gba: {kind: [files], storage: gba, system: gba}}\n").unwrap();
    assert_eq!(writer.source_views().unwrap()[0].source_id, "gba");
    assert!(!root.path().join(LIBRARY_FILE_NAME).exists());
}

#[test]
fn enrichment_uses_available_replacement_without_discarding_release_history() {
    let (root, private, writer, mut report) = fixture();
    let registry = crate::plugin_test_fixtures::claims_only_registry();
    writer.run(&report, &DiscoveryOptions::default()).unwrap();
    report.scanned[0].units[0].identity = ContentIdentity::Hash {
        sha256: format!("sha256:{}", "b".repeat(64)),
        sha1: format!("sha1:{}", "b".repeat(40)),
    };
    writer.run(&report, &DiscoveryOptions::default()).unwrap();
    let games = artwork_games(root.path(), private.path(), &registry).unwrap();
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].release_id, format!("sha256:{}", "b".repeat(64)));
    assert_eq!(
        Documents::read(root.path())
            .unwrap()
            .validate()
            .unwrap()
            .library["game"]
            .releases
            .len(),
        2
    );
    report.scanned[0].units.clear();
    writer.run(&report, &DiscoveryOptions::default()).unwrap();
    assert!(artwork_games(root.path(), private.path(), &registry)
        .unwrap()
        .is_empty());
}

#[test]
fn unpair_removes_only_storage_created_by_pair() {
    for authored in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let games = tempfile::tempdir().unwrap();
        fs::write(games.path().join("Game.gba"), b"game").unwrap();
        let writer = DiscoveryCoordinator::for_tests(root.path(), private.path());
        if authored {
            fs::write(root.path().join(DEVICE_FILE_NAME), format!("storage: {{gba: {{root: '{}', path: {{authored: keep}}}}}}\nsources: {{gba: {{kind: [files], storage: gba, system: gba}}}}\n", games.path().display())).unwrap();
            writer.rescan(&Default::default()).unwrap();
        } else {
            writer
                .pair(games.path().into(), "gba", &Default::default())
                .unwrap();
        }
        assert_eq!(
            PrivateState::read(private.path())
                .unwrap()
                .ownership
                .created_storages
                .contains("gba"),
            !authored
        );
        writer.unpair("gba", &Default::default()).unwrap();
        assert!(!PrivateState::read(private.path())
            .unwrap()
            .ownership
            .created_storages
            .contains("gba"));
        let snapshot = Documents::read(root.path()).unwrap().validate().unwrap();
        assert!(snapshot.sources.is_empty());
        assert_eq!(snapshot.storage.contains_key("gba"), authored);
        if authored {
            assert_eq!(
                snapshot.storage["gba"].path.as_ref().unwrap()["authored"].0,
                "keep"
            );
        }
    }
}

#[test]
fn installed_steam_manifest_needs_no_authored_library_or_location() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    let registry = crate::plugin_test_fixtures::steam(package.path());
    let manifests = root.path().join(
        "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/steam/steamapps",
    );
    fs::create_dir_all(&manifests).unwrap();
    let manifest = manifests.join("appmanifest_401710.acf");
    let installed =
        "\"AppState\" { \"appid\" \"401710\" \"name\" \"Flinthook\" \"StateFlags\" \"4\" }";
    fs::write(&manifest, installed).unwrap();
    let writer = DiscoveryCoordinator::new(root.path(), private.path())
        .with_registry_source(plugin_policy::RegistrySource::Selected(Arc::new(registry)));
    let options = DiscoveryOptions::default();
    assert_eq!(writer.rescan(&options).unwrap().added_games, 1);
    let first = Documents::read(root.path()).unwrap();
    let snapshot = first.validate().unwrap();
    let game = &snapshot.library["flinthook"];
    assert_eq!(game.title.as_deref(), Some("Flinthook"));
    assert_eq!(game.releases[0].id.0, "steam");
    assert_eq!(
        game.releases[0].identity.as_ref().unwrap().value(),
        "@korri:steam/401710"
    );
    assert!(matches!(
        snapshot.locations["@korri:steam/401710"][0],
        config::Location::ProviderRef { .. }
    ));
    assert!(snapshot.sources.is_empty());
    writer.rescan(&options).unwrap();
    assert_eq!(Documents::read(root.path()).unwrap(), first);
    fs::remove_file(&manifest).unwrap();
    writer.rescan(&options).unwrap();
    let absent = Documents::read(root.path()).unwrap().validate().unwrap();
    assert!(absent.library.contains_key("flinthook"));
    assert!(!absent.locations.contains_key("@korri:steam/401710"));
    fs::write(&manifest, installed).unwrap();
    writer.rescan(&options).unwrap();
    assert_eq!(Documents::read(root.path()).unwrap(), first);
    fs::write(&manifest, installed.replace("\"4\"", "\"1026\"")).unwrap();
    writer.rescan(&options).unwrap();
    assert!(!Documents::read(root.path())
        .unwrap()
        .validate()
        .unwrap()
        .locations
        .contains_key("@korri:steam/401710"));
}

#[test]
fn steam_disable_withdraws_owned_locations_but_not_authored_locations() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    let manifests = root.path().join(
        "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/steam/steamapps",
    );
    fs::create_dir_all(&manifests).unwrap();
    fs::write(
        manifests.join("appmanifest_401710.acf"),
        "\"AppState\" { \"appid\" \"401710\" \"name\" \"Flinthook\" \"StateFlags\" \"4\" }",
    )
    .unwrap();
    let writer = DiscoveryCoordinator::new(root.path(), private.path()).with_registry_source(
        plugin_policy::RegistrySource::Selected(Arc::new(crate::plugin_test_fixtures::steam(
            package.path(),
        ))),
    );
    writer.rescan(&DiscoveryOptions::default()).unwrap();
    // An unrelated authored provider location is never claimed by discovery.
    let mut device =
        parse_mapping(&fs::read_to_string(root.path().join(DEVICE_FILE_NAME)).unwrap()).unwrap();
    device
        .get_mut("locations")
        .unwrap()
        .as_mapping_mut()
        .unwrap()
        .insert(
            Value::String("@korri:steam/1".into()),
            serde_yaml::to_value(vec![
                serde_json::json!({"provider":"@korri:steam", "ref":"1"}),
            ])
            .unwrap(),
        );
    fs::write(
        root.path().join(DEVICE_FILE_NAME),
        serde_yaml::to_string(&device).unwrap(),
    )
    .unwrap();
    let disabled = DiscoveryCoordinator::new(root.path(), private.path()).with_registry_source(
        plugin_policy::RegistrySource::Selected(Arc::new(
            crate::plugin::PluginRegistry::from_installed(vec![]).unwrap(),
        )),
    );
    disabled.rescan(&DiscoveryOptions::default()).unwrap();
    let snapshot = Documents::read(root.path()).unwrap().validate().unwrap();
    assert!(snapshot.library.contains_key("flinthook"));
    assert!(!snapshot.locations.contains_key("@korri:steam/401710"));
    assert!(snapshot.locations.contains_key("@korri:steam/1"));
}

fn assert_authored_steam_location_after_uninstall_survives(enabled: bool) {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    let manifests = root.path().join(
        "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/steam/steamapps",
    );
    fs::create_dir_all(&manifests).unwrap();
    let manifest = manifests.join("appmanifest_401710.acf");
    fs::write(
        &manifest,
        "\"AppState\" { \"appid\" \"401710\" \"name\" \"Flinthook\" \"StateFlags\" \"4\" }",
    )
    .unwrap();
    let writer = DiscoveryCoordinator::new(root.path(), private.path()).with_registry_source(
        plugin_policy::RegistrySource::Selected(Arc::new(crate::plugin_test_fixtures::steam(
            package.path(),
        ))),
    );
    let options = DiscoveryOptions::default();
    let key = "@korri:steam/401710";
    let scanner = crate::plugin_discovery::STEAM_INSTALLED_APPS;
    writer.rescan(&options).unwrap();
    assert!(PrivateState::read(private.path())
        .unwrap()
        .ownership
        .origins[key]
        .contains(scanner));

    fs::remove_file(&manifest).unwrap();
    let writer = if enabled {
        writer
    } else {
        writer.with_registry_source(plugin_policy::RegistrySource::Selected(Arc::new(
            crate::plugin::PluginRegistry::from_installed(vec![]).unwrap(),
        )))
    };
    writer.rescan(&options).unwrap();
    let absent = Documents::read(root.path()).unwrap().validate().unwrap();
    assert!(!absent.locations.contains_key(key));
    assert!(absent.library.contains_key("flinthook"));
    let stale_claim = PrivateState::read(private.path())
        .unwrap()
        .ownership
        .origins
        .get(key)
        .is_some_and(|origins| origins.contains(scanner));

    // The person adds the same location after discovery has withdrawn it.
    let mut device =
        parse_mapping(&fs::read_to_string(root.path().join(DEVICE_FILE_NAME)).unwrap()).unwrap();
    device
        .get_mut("locations")
        .unwrap()
        .as_mapping_mut()
        .unwrap()
        .insert(
            Value::String(key.into()),
            serde_yaml::to_value(vec![
                serde_json::json!({"provider":"@korri:steam", "ref":"401710"}),
            ])
            .unwrap(),
        );
    fs::write(
        root.path().join(DEVICE_FILE_NAME),
        serde_yaml::to_string(&device).unwrap(),
    )
    .unwrap();
    for _ in 0..2 {
        writer.rescan(&options).unwrap();
        let snapshot = Documents::read(root.path()).unwrap().validate().unwrap();
        assert!(
            snapshot.locations.contains_key(key),
            "authored location was removed: enabled={enabled}, stale ownership after uninstall={stale_claim}"
        );
    }
    assert!(
        !stale_claim,
        "withdrawal must release this scanner's ownership"
    );
    assert!(!PrivateState::read(private.path())
        .unwrap()
        .ownership
        .origins
        .get(key)
        .is_some_and(|origins| origins.contains(scanner)));
}

#[test]
fn authored_steam_location_after_uninstall_survives_enabled_rescan() {
    assert_authored_steam_location_after_uninstall_survives(true);
}

#[test]
fn authored_steam_location_after_uninstall_survives_disabled_rescan() {
    assert_authored_steam_location_after_uninstall_survives(false);
}

#[test]
fn artwork_uses_acquired_content_identities_without_discovery_ownership() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let writer = DiscoveryCoordinator::for_tests(root.path(), private.path());
    let registry = crate::plugin_test_fixtures::claims_only_registry();
    let parts = vec![format!("sha256:{}", "c".repeat(64))];
    let identities = [
        ContentIdentity::Hash {
            sha256: format!("sha256:{}", "a".repeat(64)),
            sha1: format!("sha1:{}", "b".repeat(40)),
        },
        ContentIdentity::Parts {
            value: format!(
                "sha256:{}",
                hex::encode(Sha256::digest(parts.join("\n").as_bytes()))
            ),
            parts,
        },
        ContentIdentity::Chd {
            sha1: format!("sha1:{}", "d".repeat(40)),
        },
    ];
    for (index, identity) in identities.iter().enumerate() {
        let title = format!("Acquired {index}");
        let id = writer
            .record_acquired(&AcquiredGame {
                provider: "@test:shop",
                claim_id: &index.to_string(),
                title: &title,
                system: "gba",
                identity,
            })
            .unwrap();
        let ready = crate::acquisition::placement::copied_release_cache(root.path())
            .join(crate::acquisition::placement::ready_name(identity.value()).unwrap());
        fs::create_dir_all(&ready).unwrap();
        fs::write(ready.join("game.gba"), b"verified acquisition content").unwrap();
        let game = artwork_games(root.path(), private.path(), &registry)
            .unwrap()
            .into_iter()
            .find(|game| game.playable_id == id)
            .unwrap();
        assert_eq!(game.title, title);
        assert_eq!(game.release_id, identity.value());
        assert_eq!(game.rom_identity, identity.value());
        assert!(game.release_fingerprint.starts_with("sha256:"));
        assert!(writer.published().unwrap().locations.is_empty());
    }
    assert_eq!(
        artwork_games(root.path(), private.path(), &registry)
            .unwrap()
            .len(),
        3
    );
}

fn ibuki_lobby_fixture() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    crate::plugin::PluginRegistry,
) {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    ensure_fixed_files(root.path()).unwrap();
    fs::write(
        root.path().join(LIBRARY_FILE_NAME),
        include_str!("../../../tests/fixtures/artwork-library-lobby.yaml"),
    )
    .unwrap();
    let package = root.path().join("starter-pack");
    fs::create_dir(&package).unwrap();
    fs::write(
        package.join("plugin.ts"),
        include_str!("../../../tests/fixtures/artwork-starter-pack.plugin.ts"),
    )
    .unwrap();
    let registry = crate::plugin::PluginRegistry::from_installed(vec![
        crate::plugin_installation::EnabledPackage {
            id: "@korri:starter-pack".into(),
            package,
            files: BTreeMap::from([("cartridges".into(), root.path().join("cartridges"))]),
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            requires: Vec::new(),
            hosts: Vec::new(),
            credential_host: None,
        },
    ])
    .unwrap();
    (root, private, registry)
}

#[test]
fn artwork_includes_ibuki_library_only_and_starter_pack_games_before_download() {
    let (root, private, registry) = ibuki_lobby_fixture();
    let games = artwork_games(root.path(), private.path(), &registry).unwrap();
    assert_eq!(
        games
            .iter()
            .map(|g| g.playable_id.as_str())
            .collect::<Vec<_>>(),
        ["2048gb", "air-delivery"]
    );
    let snapshot = Documents::read(root.path()).unwrap().validate().unwrap();
    assert!(snapshot.locations.is_empty());
    assert!(!root.path().join("cache").exists());
    for game in games {
        let key = snapshot.library[&game.playable_id].releases[0]
            .identity
            .as_ref()
            .unwrap()
            .value();
        assert_eq!(game.release_id, key);
        assert_eq!(game.rom_identity, key);
    }
    let empty = crate::plugin::PluginRegistry::new(Vec::new(), Vec::<String>::new()).unwrap();
    assert_eq!(
        artwork_games(root.path(), private.path(), &empty)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn a_source_backed_release_precedes_an_earlier_library_only_release_for_artwork() {
    let (root, private, registry) = ibuki_lobby_fixture();
    let mut snapshot = Documents::read(root.path()).unwrap().validate().unwrap();
    let packaged = snapshot
        .library
        .remove("air-delivery")
        .unwrap()
        .releases
        .remove(0);
    let key = packaged.identity.as_ref().unwrap().value().to_owned();
    snapshot
        .library
        .get_mut("2048gb")
        .unwrap()
        .releases
        .push(packaged);
    fs::write(
        root.path().join(LIBRARY_FILE_NAME),
        serde_yaml::to_string(&BTreeMap::from([("library", snapshot.library)])).unwrap(),
    )
    .unwrap();
    let games = artwork_games(root.path(), private.path(), &registry).unwrap();
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].release_id, key);
    // With no source-backed release, the same oldest-first order applies.
    let empty = crate::plugin::PluginRegistry::new(Vec::new(), Vec::<String>::new()).unwrap();
    let games = artwork_games(root.path(), private.path(), &empty).unwrap();
    assert_eq!(
        games[0].release_id,
        "sha256:3ea2376b15b34bd26b10e6b31d2753bf8b086098dbdb5ee84d9c0f03d66d3d7f"
    );
}
