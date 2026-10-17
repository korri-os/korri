use korrid::{
    config::{resolver, snapshot::ConfigSnapshotCoordinator},
    discovery::{
        identify::{ContentIdentity, EvidenceLevel, FoundUnit, ScanReport, SourceScan},
        DiscoveryCoordinator, DiscoveryOptions,
    },
};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn completed_scan_publishes_one_library_and_launches_by_system_without_rewriting_on_rescan() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let games = tempfile::tempdir().unwrap();
    let bytes = b"minimum config acceptance ROM";
    fs::write(games.path().join("Wario Land 4.gba"), bytes).unwrap();
    fs::write(readable.path().join("device.yaml"), format!("storage: {{gba: {{root: '{}'}}}}\nsources: {{gba: {{kind: [files], storage: gba, system: gba}}}}\n", games.path().display())).unwrap();
    let sha = format!("sha256:{}", hex::encode(Sha256::digest(bytes)));
    let scan = ScanReport {
        scanned: vec![SourceScan {
            source_id: "gba".into(),
            units: vec![FoundUnit {
                source_id: "gba".into(),
                storage_id: "gba".into(),
                path: "Wario Land 4.gba".into(),
                parts: vec!["Wario Land 4.gba".into()],
                tracks: Vec::new(),
                system: "gba".into(),
                title: "Wario Land 4".into(),
                version: None,
                region: None,
                identity: ContentIdentity::Hash {
                    sha256: sha.clone(),
                    sha1: format!("sha1:{}", "a".repeat(40)),
                },
                evidence: EvidenceLevel::Extension,
            }],
            ..SourceScan::default()
        }],
        ..ScanReport::default()
    };
    let discovery = DiscoveryCoordinator::new(readable.path(), private.path());
    let options = DiscoveryOptions {
        first_seen_at: "2026-10-16T00:00:00Z".into(),
        ..DiscoveryOptions::default()
    };
    assert_eq!(discovery.run(&scan, &options).unwrap().added_games, 1);
    let loaded = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert!(loaded.diagnostic.is_none(), "{:?}", loaded.diagnostic);
    let game = &loaded.snapshot.library["wario-land-4"];
    assert_eq!(game.releases[0].id.0, "gba");
    assert_eq!(game.releases[0].identity.as_ref().unwrap().value(), sha);
    assert!(!readable.path().join("catalog/games.yaml").exists());
    assert!(!readable.path().join("catalog/releases.yaml").exists());
    let registry = korrid::plugin_test_fixtures::installed(readable.path());
    let route = resolver::resolve_route(
        readable.path(),
        &loaded.snapshot,
        &registry,
        [],
        "wario-land-4",
    )
    .unwrap();
    assert_eq!(route.runner_id, "@korri:mgba/mgba");
    assert_eq!(route.release_id, sha);
    let before: Vec<_> = korrid::config::snapshot::FILE_NAMES
        .iter()
        .map(|f| fs::read(readable.path().join(f)).unwrap())
        .collect();
    assert_eq!(discovery.run(&scan, &options).unwrap().added_games, 0);
    let after: Vec<_> = korrid::config::snapshot::FILE_NAMES
        .iter()
        .map(|f| fs::read(readable.path().join(f)).unwrap())
        .collect();
    assert_eq!(before, after);
}
