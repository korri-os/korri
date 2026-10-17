#[path = "fixtures/readable.rs"]
mod readable;
use korrid::{
    config::{
        decode_config_documents, resolver,
        snapshot::{ConfigSnapshotCoordinator, FILE_NAMES},
    },
    discovery::{
        identify::{ContentIdentity, EvidenceLevel, FoundUnit, ScanReport, SourceScan},
        DiscoveryCoordinator, DiscoveryOptions,
    },
};
use serde_yaml::Value;
use std::{fs, path::Path};

fn documents(root: &Path) -> Vec<Vec<u8>> {
    FILE_NAMES
        .iter()
        .map(|f| fs::read(root.join(f)).unwrap())
        .collect()
}
fn snapshot(root: &Path) -> std::sync::Arc<korrid::config::ConfigSnapshot> {
    let state = ConfigSnapshotCoordinator::new(root).reload();
    assert!(state.diagnostic.is_none(), "{:?}", state.diagnostic);
    state.snapshot
}
fn setup() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    DiscoveryCoordinator,
    ScanReport,
) {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    fs::write(root.path().join("device.yaml"), format!("storage: {{gba: {{root: '{}'}}}}\nsources: {{gba: {{kind: [files], storage: gba, system: gba}}}}\n", root.path().display())).unwrap();
    let scan = ScanReport {
        // This fixture models the storage that pair created for its source.
        created_storages: std::collections::BTreeSet::from(["gba".into()]),
        scanned: vec![SourceScan {
            source_id: "gba".into(),
            units: vec![unit("game.gba", 'a')],
            ..SourceScan::default()
        }],
        ..ScanReport::default()
    };
    let writer = DiscoveryCoordinator::new(root.path(), private.path());
    (root, private, writer, scan)
}
fn key(c: char) -> String {
    format!("sha256:{}", c.to_string().repeat(64))
}
fn unit(path: &str, c: char) -> FoundUnit {
    FoundUnit {
        source_id: "gba".into(),
        storage_id: "gba".into(),
        path: path.into(),
        parts: vec![path.into()],
        tracks: Vec::new(),
        system: "gba".into(),
        title: "Game".into(),
        version: None,
        region: None,
        identity: ContentIdentity::Hash {
            sha256: key(c),
            sha1: format!("sha1:{}", c.to_string().repeat(40)),
        },
        evidence: EvidenceLevel::Extension,
    }
}

#[test]
fn copies_add_locations_and_restart_preserves_identity_and_bytes() {
    let (root, private, writer, mut scan) = setup();
    scan.scanned[0].units.push(unit("copy.gba", 'a'));
    writer.run(&scan, &DiscoveryOptions::default()).unwrap();
    let before = documents(root.path());
    let state = snapshot(root.path());
    assert_eq!(state.library.len(), 1);
    assert_eq!(state.locations[&key('a')].len(), 2);
    DiscoveryCoordinator::new(root.path(), private.path())
        .run(&scan, &DiscoveryOptions::default())
        .unwrap();
    assert_eq!(before, documents(root.path()));
}
#[test]
fn byte_replacement_keeps_history_in_one_game_and_unpair_cleans_exclusive_history() {
    let (root, _, writer, mut scan) = setup();
    writer.run(&scan, &DiscoveryOptions::default()).unwrap();
    scan.scanned[0].units[0] = unit("game.gba", 'b');
    writer.run(&scan, &DiscoveryOptions::default()).unwrap();
    let state = snapshot(root.path());
    assert_eq!(state.library.len(), 1);
    assert_eq!(state.library["game"].releases.len(), 2);
    assert!(!state.locations.contains_key(&key('a')));
    assert!(state.locations.contains_key(&key('b')));
    writer
        .run(
            &ScanReport {
                unpaired: vec!["gba".into()],
                ..ScanReport::default()
            },
            &DiscoveryOptions::default(),
        )
        .unwrap();
    let state = snapshot(root.path());
    assert!(state.library.is_empty());
    assert!(state.locations.is_empty());
    assert!(state.sources.is_empty());
    assert!(state.storage.is_empty());
}
#[test]
fn ten_thousand_locations_rescan_without_rewriting_or_minting_games() {
    let (root, _, writer, mut scan) = setup();
    scan.scanned[0].units = (0..10_000)
        .map(|i| unit(&format!("copy-{i}.gba"), 'a'))
        .collect();
    assert_eq!(
        writer
            .run(&scan, &DiscoveryOptions::default())
            .unwrap()
            .added_games,
        1
    );
    let before = documents(root.path());
    assert_eq!(snapshot(root.path()).locations[&key('a')].len(), 10_000);
    assert_eq!(
        writer
            .run(&scan, &DiscoveryOptions::default())
            .unwrap()
            .added_games,
        0
    );
    assert_eq!(documents(root.path()), before);
}
#[test]
fn available_releases_route_in_catalog_order_and_keep_first_release_identity() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/game.gba"), b"rom").unwrap();
    fs::write(root.path().join("roms/other.gba"), b"other").unwrap();
    let mut library: Value = serde_yaml::from_str(&readable::gba_library()).unwrap();
    let mut second = library["library"][readable::GBA_ID]["releases"][0].clone();
    second["id"] = "gba-2".into();
    second["identity"]["value"] = key('a').into();
    library["library"][readable::GBA_ID]["releases"]
        .as_sequence_mut()
        .unwrap()
        .push(second);
    let device = format!(
        "{}  '{}':\n    - storage: roms\n      path: other.gba\n",
        readable::gba_locations("roms", "game.gba", false),
        key('a')
    );
    let state =
        decode_config_documents(&device, &serde_yaml::to_string(&library).unwrap()).unwrap();
    let registry = korrid::plugin_test_fixtures::installed(root.path());
    let route =
        resolver::resolve_route(root.path(), &state, &registry, [], readable::GBA_ID).unwrap();
    assert_eq!(route.release_id, readable::GBA_RELEASE);
    fs::remove_file(root.path().join("roms/game.gba")).unwrap();
    let route =
        resolver::resolve_route(root.path(), &state, &registry, [], readable::GBA_ID).unwrap();
    assert_eq!(route.release_id, key('a'));
    assert_eq!(
        route.identity,
        Some(korrid::GameIdentity::Hash(readable::GBA_RELEASE.into()))
    );
}
#[test]
fn malformed_library_retains_last_known_good_until_restored() {
    let root = tempfile::tempdir().unwrap();
    readable::android(root.path());
    let coordinator = ConfigSnapshotCoordinator::new(root.path());
    let good = coordinator.reload();
    fs::write(
        root.path().join("catalog/library.yaml"),
        "library: {game: {releases: []}}\n",
    )
    .unwrap();
    let rejected = coordinator.reload();
    assert!(rejected.diagnostic.is_some());
    assert_eq!(rejected.generation, good.generation);
    assert_eq!(rejected.snapshot.library, good.snapshot.library);
    fs::write(
        root.path().join("catalog/library.yaml"),
        readable::ANDROID_LIBRARY,
    )
    .unwrap();
    assert!(coordinator.reload().diagnostic.is_none());
}
#[test]
fn snapshot_probe_exercises_the_two_document_loader_and_retained_snapshot() {
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
    for text in [
        "tmnt present: yes",
        "retained tmnt: yes",
        "release records: 1",
        "LocalConfigReloadFailed",
    ] {
        assert!(stdout.contains(text), "{stdout}");
    }
}
#[test]
fn superseded_documents_are_not_read_or_rewritten() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("catalog")).unwrap();
    for name in [
        "config.yaml",
        "library.yaml",
        "catalog/games.yaml",
        "catalog/releases.yaml",
    ] {
        fs::write(root.path().join(name), "not: [valid yaml").unwrap();
    }
    assert!(snapshot(root.path()).library.is_empty());
    for name in [
        "config.yaml",
        "library.yaml",
        "catalog/games.yaml",
        "catalog/releases.yaml",
    ] {
        assert_eq!(
            fs::read_to_string(root.path().join(name)).unwrap(),
            "not: [valid yaml"
        );
    }
}
