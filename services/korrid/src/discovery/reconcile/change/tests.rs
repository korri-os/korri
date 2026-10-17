use super::*;
use crate::discovery::identify::{
    AccountRoot, DeclaredFile, DeclaredFileStatus, EvidenceLevel, SourceScan,
};

const NOW: &str = "2026-10-16T00:00:00Z";
const LATER: &str = "2026-10-17T00:00:00Z";
fn hash(c: char) -> String {
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
        title: "Wario Land 4".into(),
        version: None,
        region: None,
        identity: ContentIdentity::Hash {
            sha256: hash(c),
            sha1: format!("sha1:{}", c.to_string().repeat(40)),
        },
        evidence: EvidenceLevel::Extension,
    }
}
fn snapshot() -> ConfigSnapshot {
    crate::config::decode_config_documents("storage: {gba: {root: /games/gba}, other: {root: /games/other}}\nsources: {gba: {kind: [files], storage: gba, system: gba}, other: {kind: [files], storage: other, system: gba}}", "{}").unwrap()
}
fn report(units: Vec<FoundUnit>) -> ScanReport {
    ScanReport {
        created_storages: BTreeSet::from(["gba".into(), "other".into()]),
        scanned: vec![SourceScan {
            source_id: "gba".into(),
            seen_paths: units.iter().flat_map(|u| u.parts.clone()).collect(),
            units,
            ..SourceScan::default()
        }],
        ..ScanReport::default()
    }
}
fn apply(snapshot: &ConfigSnapshot, change: &CatalogChange) -> ConfigSnapshot {
    super::super::apply_change(snapshot, change).unwrap()
}
fn after(units: Vec<FoundUnit>) -> (ConfigSnapshot, CatalogChange) {
    let snapshot = snapshot();
    let change = reconcile(&report(units), &snapshot, NOW);
    (apply(&snapshot, &change), change)
}
fn with_history(mut report: ScanReport, change: &CatalogChange) -> ScanReport {
    report.created_storages = change.created_storages.clone();
    report.origins = change.origins.clone();
    report.paths = change.paths.clone();
    report
}

#[test]
fn new_identities_never_fold_by_title_and_clashes_get_readable_suffixes() {
    let (snapshot, _) = after(vec![
        unit("a.gba", 'a'),
        unit("b.gba", 'b'),
        unit("c.gba", 'c'),
    ]);
    assert_eq!(
        snapshot
            .library
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["wario-land-4", "wario-land-4-2", "wario-land-4-3"]
    );
    assert_eq!(slug("  WARIO: Land 4! "), "wario-land-4");
    assert_eq!(slug("🎮"), "game");
}
#[test]
fn same_bytes_add_locations_not_games_and_rescan_is_idempotent() {
    let units = vec![unit("a.gba", 'a'), unit("copy.gba", 'a')];
    let (snapshot, change) = after(units.clone());
    assert_eq!(snapshot.library.len(), 1);
    assert_eq!(snapshot.locations[&hash('a')].len(), 2);
    let again = reconcile(&with_history(report(units), &change), &snapshot, LATER);
    assert!(again.device_yaml.is_none());
    assert!(again.library_yaml.is_none());
    assert_eq!(again.origins, change.origins);
    assert_eq!(again.paths, change.paths);
}
#[test]
fn changed_bytes_append_release_and_keep_unavailable_history() {
    let (snapshot, first) = after(vec![unit("a.gba", 'a')]);
    let change = reconcile(
        &with_history(report(vec![unit("a.gba", 'b')]), &first),
        &snapshot,
        LATER,
    );
    let snapshot = apply(&snapshot, &change);
    let game = &snapshot.library["wario-land-4"];
    assert_eq!(
        game.releases
            .iter()
            .map(|r| r.id.0.as_str())
            .collect::<Vec<_>>(),
        ["gba", "gba-2"]
    );
    assert_eq!(
        game.releases[0].identity.as_ref().unwrap().value(),
        hash('a')
    );
    assert!(!snapshot.locations.contains_key(&hash('a')));
    assert_eq!(snapshot.locations[&hash('b')].len(), 1);
}
#[test]
fn missing_since_is_stable_and_clears_when_file_returns() {
    let (snapshot, first) = after(vec![unit("a.gba", 'a')]);
    let missing = reconcile(&with_history(report(vec![]), &first), &snapshot, LATER);
    let gone = apply(&snapshot, &missing);
    assert_eq!(
        discovery(&gone.locations[&hash('a')][0])
            .unwrap()
            .missing_since
            .as_ref()
            .unwrap()
            .0,
        LATER
    );
    assert_eq!(gone.library, snapshot.library);
    let again = reconcile(
        &with_history(report(vec![]), &missing),
        &gone,
        "2026-10-18T00:00:00Z",
    );
    assert!(again.device_yaml.is_none());
    let returned = reconcile(
        &with_history(report(vec![unit("a.gba", 'a')]), &missing),
        &gone,
        "2026-10-19T00:00:00Z",
    );
    let available = apply(&gone, &returned);
    let meta = discovery(&available.locations[&hash('a')][0]).unwrap();
    assert!(meta.missing_since.is_none());
    assert_eq!(meta.first_seen_at.0, NOW);
}
#[test]
fn skipped_sources_never_change_locations_or_availability() {
    let (snapshot, first) = after(vec![unit("a.gba", 'a')]);
    let scan = ScanReport {
        skipped: vec!["gba".into()],
        ..ScanReport::default()
    };
    let change = reconcile(&with_history(scan, &first), &snapshot, LATER);
    assert!(change.device_yaml.is_none());
    assert!(change.library_yaml.is_none());
}
#[test]
fn unpair_removes_exclusive_history_and_empty_games_but_keeps_shared_releases() {
    let (snapshot, first) = after(vec![unit("a.gba", 'a')]);
    let replace = reconcile(
        &with_history(report(vec![unit("a.gba", 'b')]), &first),
        &snapshot,
        LATER,
    );
    let replaced = apply(&snapshot, &replace);
    let unpair = ScanReport {
        unpaired: vec!["gba".into()],
        ..ScanReport::default()
    };
    let change = reconcile(&with_history(unpair, &replace), &replaced, LATER);
    let empty = apply(&replaced, &change);
    assert!(empty.library.is_empty());
    assert!(empty.locations.is_empty());
    assert!(!empty.storage.contains_key("gba"));
    assert!(!empty.sources.contains_key("gba"));
    let mut copy = unit("copy.gba", 'a');
    copy.source_id = "other".into();
    copy.storage_id = "other".into();
    let scan = ScanReport {
        scanned: vec![SourceScan {
            source_id: "other".into(),
            units: vec![copy],
            ..SourceScan::default()
        }],
        ..ScanReport::default()
    };
    let copied = reconcile(&with_history(scan, &first), &snapshot, LATER);
    let shared = apply(&snapshot, &copied);
    let change = reconcile(
        &with_history(
            ScanReport {
                unpaired: vec!["gba".into()],
                ..ScanReport::default()
            },
            &copied,
        ),
        &shared,
        LATER,
    );
    let shared = apply(&shared, &change);
    assert_eq!(shared.library.len(), 1);
    assert_eq!(shared.locations[&hash('a')].len(), 1);
    assert_eq!(storage(&shared.locations[&hash('a')][0]), Some("other"));
}
fn declaration() -> (String, crate::plugin::DeclaredGame) {
    (
        "@test:games/wario".into(),
        crate::plugin::DeclaredGame {
            provider: "@test:games".into(),
            key: "wario".into(),
            title: "Declared Wario".into(),
            releases: BTreeMap::from([
                (
                    "original".into(),
                    crate::plugin::DeclaredRelease {
                        system: Some("gba".into()),
                        hashes: vec![format!("sha1:{}", "a".repeat(40))],
                        file: None,
                    },
                ),
                (
                    "prepared".into(),
                    crate::plugin::DeclaredRelease {
                        system: None,
                        hashes: vec![hash('b')],
                        file: Some("built/game".into()),
                    },
                ),
            ]),
        },
    )
}
#[test]
fn declared_sha1_and_sha256_join_and_write_provider_links_with_account_storage() {
    let mut scan = report(vec![unit("a.gba", 'a')]);
    scan.declared_games = BTreeMap::from([declaration()]);
    scan.accounts = vec![AccountRoot {
        user: "default".into(),
        storage_id: "account-default".into(),
        root: "/korri/users/default".into(),
    }];
    scan.declared.files.push(DeclaredFile {
        game: declaration().0,
        release_key: "prepared".into(),
        storage_id: "account-default".into(),
        path: "built/game".into(),
        status: DeclaredFileStatus::Found(unit("unused", 'b').identity),
    });
    let initial = snapshot();
    let change = reconcile(&scan, &initial, NOW);
    let snapshot = apply(&initial, &change);
    assert_eq!(snapshot.library.len(), 1);
    let game = &snapshot.library["declared-wario"];
    assert_eq!(game.releases.len(), 2);
    assert_eq!(game.releases[0].id.0, "original");
    assert_eq!(game.releases[1].id.0, "prepared");
    assert!(game.releases[1].system.is_none());
    assert_eq!(
        snapshot.storage["account-default"].root.0,
        "/korri/users/default"
    );
    let link = &snapshot.provider_links[&declaration().0];
    assert_eq!(link.playable.0, "declared-wario");
    assert_eq!(link.provider.0, "@test:games");
    assert_eq!(link.refs[0].value.0, "wario");
    assert_eq!(
        snapshot.game_identity("declared-wario"),
        Some(crate::GameIdentity::Provider(crate::GameProviderIdentity {
            provider: "@test:games".into(),
            provider_ref: "wario".into()
        }))
    );
}
#[test]
fn declared_games_with_no_available_release_do_not_enter_catalog() {
    let scan = ScanReport {
        declared_games: BTreeMap::from([declaration()]),
        ..ScanReport::default()
    };
    let initial = snapshot();
    let change = reconcile(&scan, &initial, NOW);
    assert!(change.library_yaml.is_none());
    assert!(change.device_yaml.is_none());
}
#[test]
fn first_release_remains_game_identity_after_replacement() {
    let (snapshot, first) = after(vec![unit("a.gba", 'a')]);
    let change = reconcile(
        &with_history(report(vec![unit("a.gba", 'b')]), &first),
        &snapshot,
        LATER,
    );
    let snapshot = apply(&snapshot, &change);
    assert_eq!(
        snapshot.game_identity("wario-land-4"),
        Some(crate::GameIdentity::Hash(hash('a')))
    );
    assert_eq!(snapshot.game_identity("unknown"), None);
    let mut snapshot = snapshot;
    snapshot.library.get_mut("wario-land-4").unwrap().releases[0].identity = None;
    assert_eq!(snapshot.game_identity("wario-land-4"), None);
}
#[test]
fn multipart_locations_are_ordered_filesets_and_rescan_does_not_write() {
    use sha2::{Digest, Sha256};
    let mut game = unit("a.cue", 'a');
    let parts = vec![hash('a'), hash('b')];
    game.parts = vec!["a-1.bin".into(), "a-2.bin".into()];
    game.tracks = game.parts.clone();
    game.identity = ContentIdentity::Parts {
        value: format!("sha256:{}", hex::encode(Sha256::digest(parts.join("\n")))),
        parts,
    };
    let key = game.identity.value().to_owned();
    let (snapshot, first) = after(vec![game.clone()]);
    let Location::FileSet {
        files, discovery, ..
    } = &snapshot.locations[&key][0]
    else {
        panic!("expected FileSet");
    };
    assert_eq!(
        files
            .0
            .iter()
            .map(|p| p.path.0.as_str())
            .collect::<Vec<_>>(),
        ["a.cue", "a-1.bin", "a-2.bin"]
    );
    assert!(discovery.is_some());
    assert_eq!(files.0[0].role.as_ref().unwrap().0, "descriptor");
    assert_eq!(files.0[1].role.as_ref().unwrap().0, "track");
    let change = reconcile(&with_history(report(vec![game]), &first), &snapshot, LATER);
    assert!(change.device_yaml.is_none());
    assert!(change.library_yaml.is_none());
    assert_eq!(change.paths, first.paths);
}

#[test]
fn skipped_source_does_not_prune_paths_even_if_report_contains_partial_units() {
    let (snapshot, first) = after(vec![unit("a.gba", 'a')]);
    let mut partial = with_history(report(vec![unit("b.gba", 'b')]), &first);
    partial.skipped = vec!["gba".into()];
    let change = reconcile(&partial, &snapshot, LATER);
    assert!(change.device_yaml.is_none());
    assert!(change.library_yaml.is_none());
    assert_eq!(change.paths, first.paths);
    assert_eq!(change.origins, first.origins);
}
#[test]
fn changed_cue_replaces_only_its_own_location_when_all_track_names_change() {
    use sha2::{Digest, Sha256};
    let mut old = unit("one.cue", 'a');
    let parts = vec![hash('a'), hash('b')];
    old.identity = ContentIdentity::Parts {
        value: format!("sha256:{}", hex::encode(Sha256::digest(parts.join("\n")))),
        parts,
    };
    old.parts = vec!["one-track1.bin".into(), "one-track2.bin".into()];
    let mut copy = old.clone();
    copy.path = "two.cue".into();
    copy.parts = vec!["two-track1.bin".into(), "two-track2.bin".into()];
    let old_key = old.identity.value().to_owned();
    let (snapshot, first) = after(vec![old.clone(), copy.clone()]);
    let mut replacement = old;
    replacement.parts = vec!["new-track1.bin".into(), "new-track2.bin".into()];
    let parts = vec![hash('c'), hash('d')];
    replacement.identity = ContentIdentity::Parts {
        value: format!("sha256:{}", hex::encode(Sha256::digest(parts.join("\n")))),
        parts,
    };
    let change = reconcile(
        &with_history(report(vec![replacement, copy]), &first),
        &snapshot,
        LATER,
    );
    let snapshot = apply(&snapshot, &change);
    assert_eq!(snapshot.library.len(), 1);
    assert_eq!(snapshot.library["wario-land-4"].releases.len(), 2);
    assert_eq!(snapshot.locations[&old_key].len(), 1);
    assert_eq!(
        location_paths(&snapshot.locations[&old_key][0]),
        ["two.cue", "two-track1.bin", "two-track2.bin"]
    );
}

#[test]
fn declared_unreadable_and_mismatch_keep_locations_but_absent_marks_missing() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("built/game");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"prepared").unwrap();
    let identity = crate::discovery::identify::identity_of(
        &path,
        &crate::plugin::ContentRules::default(),
        &mut Default::default(),
    )
    .unwrap();
    let (key, mut game) = declaration();
    game.releases.get_mut("prepared").unwrap().hashes = vec![identity.value().into()];
    let mut scan = ScanReport {
        accounts: vec![AccountRoot {
            user: "default".into(),
            storage_id: "account-default".into(),
            root: root.path().into(),
        }],
        declared_games: BTreeMap::from([(key, game)]),
        ..ScanReport::default()
    };
    let resolve = |scan: &ScanReport| {
        crate::discovery::identify::resolve_declared_games(
            &scan.accounts,
            &scan.declared_games,
            &mut Default::default(),
        )
    };
    scan.declared = resolve(&scan);
    let first = reconcile(&scan, &snapshot(), NOW);
    let current = apply(&snapshot(), &first);
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    scan.declared = resolve(&scan);
    assert_eq!(
        scan.declared.attention[0].kind,
        crate::discovery::attention::AttentionKind::Unreadable
    );
    let unreadable = reconcile(&with_history(scan.clone(), &first), &current, LATER);
    assert!(unreadable.device_yaml.is_none(), "unreadable is not absent");
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, b"wrong bytes").unwrap();
    scan.declared = resolve(&scan);
    assert_eq!(
        scan.declared.attention[0].kind,
        crate::discovery::attention::AttentionKind::DeclaredMismatch
    );
    assert!(
        reconcile(&with_history(scan.clone(), &first), &current, LATER)
            .device_yaml
            .is_none()
    );
    std::fs::remove_file(&path).unwrap();
    scan.declared = resolve(&scan);
    let absent = reconcile(&with_history(scan, &first), &current, LATER);
    assert_eq!(absent.missing_locations, 1);
}
