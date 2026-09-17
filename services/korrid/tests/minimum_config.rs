use std::fs;

use korrid::{
    config::{resolver, snapshot::ConfigSnapshotCoordinator},
    discovery::{DiscoveryCoordinator, DiscoveryOptions},
    plugin_policy,
};
use serde_yaml::Value;
use sha2::{Digest, Sha256};

fn document(root: &std::path::Path, name: &str) -> Value {
    serde_yaml::from_str(&fs::read_to_string(root.join(name)).expect(name)).unwrap()
}

#[test]
fn discovery_separates_catalog_facts_from_locations_and_launches_by_system() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let roms = tempfile::tempdir().unwrap();
    let bytes = b"minimum config acceptance ROM";
    fs::write(roms.path().join("Wario Land 4.gba"), bytes).unwrap();
    let sha = format!("sha256:{}", hex::encode(Sha256::digest(bytes)));
    let discovery = DiscoveryCoordinator::for_tests(readable.path(), private.path());
    let options = DiscoveryOptions {
        first_seen_at: "2026-09-06T00:00:00Z".into(),
        ..DiscoveryOptions::default()
    };
    discovery.add_location(roms.path(), &options).unwrap();

    assert!(readable.path().join("device.yaml").is_file());
    assert!(!readable.path().join("config.yaml").exists());
    assert!(!readable.path().join("library.yaml").exists());
    let games = document(readable.path(), "catalog/games.yaml");
    let releases = document(readable.path(), "catalog/releases.yaml");
    let device = document(readable.path(), "device.yaml");
    let games_map = games["games"].as_mapping().unwrap();
    assert_eq!(games_map.len(), 1);
    let (game_id, game) = games_map.iter().next().unwrap();
    let game_id = game_id.as_str().unwrap();
    assert_eq!(game_id.len(), 26, "discovery must mint a ULID, not a slug");
    assert!(game_id
        .bytes()
        .all(|byte| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&byte)));
    assert!(game_id.as_bytes()[0] <= b'7');
    assert_eq!(game["title"].as_str(), Some("Wario Land 4"));
    assert_eq!(
        game["releases"].as_sequence().unwrap(),
        &[Value::String(sha.clone())]
    );
    assert_eq!(releases["releases"].as_mapping().unwrap().len(), 1);
    let release = &releases["releases"][&sha];
    assert_eq!(release["game"].as_str(), Some(game_id));
    assert_eq!(release["system"].as_str(), Some("gba"));
    assert!(release.get("target").is_none());
    assert!(release.get("launch").is_none());
    let locations = device["locations"][&sha].as_sequence().unwrap();
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0]["path"].as_str(), Some("Wario Land 4.gba"));
    assert_eq!(
        locations[0]["discovery"]["first-seen-at"].as_str(),
        Some(options.first_seen_at.as_str())
    );
    assert!(locations[0].get("kind").is_none());

    let loaded = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert!(loaded.diagnostic.is_none(), "{:?}", loaded.diagnostic);
    let registry = korrid::plugin_test_fixtures::installed(readable.path());
    let route =
        resolver::resolve_route(readable.path(), &loaded.snapshot, &registry, [], game_id).unwrap();
    assert_eq!(route.runner_id, "@korri:mgba/mgba");

    let before_games = fs::read(readable.path().join("catalog/games.yaml")).unwrap();
    let before_releases = fs::read(readable.path().join("catalog/releases.yaml")).unwrap();
    let before_device = fs::read(readable.path().join("device.yaml")).unwrap();
    discovery.rescan(&options).unwrap();
    assert_eq!(
        fs::read(readable.path().join("catalog/games.yaml")).unwrap(),
        before_games
    );
    assert_eq!(
        fs::read(readable.path().join("catalog/releases.yaml")).unwrap(),
        before_releases
    );
    assert_eq!(
        fs::read(readable.path().join("device.yaml")).unwrap(),
        before_device
    );
}
