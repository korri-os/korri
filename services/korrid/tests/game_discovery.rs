#[path = "fixtures/readable.rs"]
mod readable;
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use sha2::{Digest, Sha256};

use korrid::{
    config::{resolver, snapshot::ConfigSnapshotCoordinator, Location},
    discovery::{
        DiscoveryCoordinator, DiscoveryDiagnosticCode, DiscoveryLifecycleCoordinator,
        DiscoveryOptions, DiscoveryPhase, FolderSelectionGrantError, FolderSelectionGrantStore,
    },
    plugin_policy,
};

fn options() -> DiscoveryOptions {
    DiscoveryOptions {
        first_seen_at: "2026-08-05T00:00:00Z".into(),
        max_diagnostics: 100,
        max_candidates: 100,
        ..DiscoveryOptions::default()
    }
}

fn lifecycle(
    readable: &tempfile::TempDir,
    private: &tempfile::TempDir,
    grants: FolderSelectionGrantStore,
) -> DiscoveryLifecycleCoordinator {
    DiscoveryLifecycleCoordinator::new(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        grants,
    )
    .with_options(options())
}

fn wait_until_idle(
    discovery: &DiscoveryLifecycleCoordinator,
) -> korrid::discovery::DiscoverySnapshot {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = discovery.snapshot();
        if !matches!(
            snapshot.state,
            DiscoveryPhase::Scanning | DiscoveryPhase::Enriching
        ) {
            return snapshot;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "discovery did not settle"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn write_private_storage_state(
    private_root: &Path,
    storage_id: &str,
    root: &Path,
    pending_scan: bool,
    pending_removal: bool,
) {
    let discovery_root = private_root.join("game-discovery");
    fs::create_dir_all(&discovery_root).unwrap();
    let record_yaml = format!("root: {}\n", root.display());
    let fingerprint = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(record_yaml.as_bytes()))
    );
    fs::write(discovery_root.join("hash-cache.json"), "{}\n").unwrap();
    fs::write(discovery_root.join("ownership.json"), r#"{"releases":{}}"#).unwrap();
    fs::write(
        discovery_root.join("storage-order.json"),
        format!(r#"["{storage_id}"]"#),
    )
    .unwrap();
    fs::write(
        discovery_root.join("storage-ownership.json"),
        format!(
            r#"{{"storages":{{"{storage_id}":{{"root":"{}","fingerprint":"{fingerprint}"}}}}}}"#,
            root.display()
        ),
    )
    .unwrap();
    write_repair_state(private_root, storage_id, pending_scan, pending_removal);
}

fn write_repair_state(
    private_root: &Path,
    storage_id: &str,
    pending_scan: bool,
    pending_removal: bool,
) {
    let discovery_root = private_root.join("game-discovery");
    fs::create_dir_all(&discovery_root).unwrap();
    let pending_scans = if pending_scan {
        format!(r#"["{storage_id}"]"#)
    } else {
        "[]".into()
    };
    let pending_removals = if pending_removal {
        format!(r#"["{storage_id}"]"#)
    } else {
        "[]".into()
    };
    fs::write(
        discovery_root.join("repair.json"),
        format!(
            r#"{{"pending_removals":{pending_removals},"pending_scans":{pending_scans},"pending_ownership":{{}}}}"#
        ),
    )
    .unwrap();
}

#[test]
fn folder_selection_receipts_are_one_use_short_lived_and_pathless() {
    let folder = tempfile::tempdir().unwrap();
    let grants = FolderSelectionGrantStore::new(Duration::from_millis(20));
    let receipt = grants.issue_approved_path(folder.path()).unwrap().token;
    assert!(!receipt.contains(folder.path().to_string_lossy().as_ref()));
    assert_eq!(
        grants.consume(&receipt).unwrap(),
        folder.path().canonicalize().unwrap()
    );
    assert_eq!(
        grants.consume(&receipt).unwrap_err(),
        FolderSelectionGrantError::Unknown
    );

    let expired = grants.issue_approved_path(folder.path()).unwrap().token;
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        grants.consume(&expired).unwrap_err(),
        FolderSelectionGrantError::Expired
    );

    let restarted = FolderSelectionGrantStore::new(Duration::from_secs(60));
    assert_eq!(
        restarted.consume(&expired).unwrap_err(),
        FolderSelectionGrantError::Unknown
    );
}

#[test]
fn lifecycle_registers_receipt_and_publishes_scanning_then_idle_with_game_visible() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    fs::write(folder.path().join("game.gba"), b"rom").unwrap();
    let grants = FolderSelectionGrantStore::default();
    let discovery = lifecycle(&readable, &private, grants.clone());
    let receipt = grants.issue_approved_path(folder.path()).unwrap().token;

    let scanning = discovery.register_receipt(&receipt).unwrap();
    assert_eq!(scanning.state, DiscoveryPhase::Scanning);
    assert!(matches!(
        discovery.register_receipt(&receipt),
        Err(FolderSelectionGrantError::Unknown)
    ));

    let idle = wait_until_idle(&discovery);
    assert_eq!(idle.state, DiscoveryPhase::Idle);
    assert_eq!(idle.locations.len(), 1);
    assert!(ConfigSnapshotCoordinator::new(readable.path())
        .reload()
        .snapshot
        .games
        .values()
        .any(|game| game.title == "game"));
}

#[test]
fn lifecycle_coalesces_active_rescans() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    fs::write(folder.path().join("one.gba"), b"one").unwrap();
    let grants = FolderSelectionGrantStore::default();
    let discovery = lifecycle(&readable, &private, grants.clone());
    let receipt = grants.issue_approved_path(folder.path()).unwrap().token;

    discovery.register_receipt(&receipt).unwrap();
    let rescan = discovery.rescan();
    assert_eq!(rescan.state, DiscoveryPhase::Scanning);
    let idle = wait_until_idle(&discovery);

    assert_eq!(idle.state, DiscoveryPhase::Idle);
    assert_eq!(
        ConfigSnapshotCoordinator::new(readable.path())
            .reload()
            .snapshot
            .games
            .len(),
        1
    );
}

#[test]
fn lifecycle_preserves_active_add_location_payloads_while_scan_runs() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    fs::write(first.path().join("one.gba"), b"one").unwrap();
    fs::write(second.path().join("two.gba"), b"two").unwrap();
    let grants = FolderSelectionGrantStore::default();
    let discovery = lifecycle(&readable, &private, grants.clone());
    let first_receipt = grants.issue_approved_path(first.path()).unwrap().token;
    let second_receipt = grants.issue_approved_path(second.path()).unwrap().token;

    discovery.register_receipt(&first_receipt).unwrap();
    discovery.register_receipt(&second_receipt).unwrap();
    let idle = wait_until_idle(&discovery);

    assert_eq!(idle.state, DiscoveryPhase::Idle);
    assert_eq!(idle.locations.len(), 2);
    let snapshot = ConfigSnapshotCoordinator::new(readable.path())
        .reload()
        .snapshot;
    assert!(snapshot.games.values().any(|game| game.title == "one"));
    assert!(snapshot.games.values().any(|game| game.title == "two"));
}

#[test]
fn lifecycle_hides_authored_storage_from_discovery_locations() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let authored = tempfile::tempdir().unwrap();
    let selected = tempfile::tempdir().unwrap();
    fs::write(selected.path().join("game.gba"), b"rom").unwrap();
    fs::write(
        readable.path().join("device.yaml"),
        format!(
            "storage:\n  authored:\n    root: {}\n",
            authored.path().canonicalize().unwrap().display()
        ),
    )
    .unwrap();
    let grants = FolderSelectionGrantStore::default();
    let discovery = lifecycle(&readable, &private, grants.clone());

    assert!(discovery.snapshot().locations.is_empty());
    let receipt = grants.issue_approved_path(selected.path()).unwrap().token;
    discovery.register_receipt(&receipt).unwrap();
    let idle = wait_until_idle(&discovery);

    assert_eq!(idle.locations.len(), 1);
    assert_ne!(idle.locations[0].id, "authored");
}

#[test]
fn lifecycle_recovers_pending_scan_after_config_first_add_restart() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("game.gba"), b"rom").unwrap();
    let storage_id = "game-folder-recovery";
    let canonical = root.path().canonicalize().unwrap();
    fs::write(
        readable.path().join("device.yaml"),
        format!(
            "storage:\n  {storage_id}:\n    root: {}\n",
            canonical.display()
        ),
    )
    .unwrap();
    readable::write(readable.path().join("catalog/games.yaml"), "{}\n").unwrap();
    write_private_storage_state(private.path(), storage_id, &canonical, true, false);

    let discovery = lifecycle(&readable, &private, FolderSelectionGrantStore::default());
    assert_eq!(discovery.snapshot().state, DiscoveryPhase::Scanning);
    let idle = wait_until_idle(&discovery);

    assert_eq!(idle.state, DiscoveryPhase::Idle);
    assert!(ConfigSnapshotCoordinator::new(readable.path())
        .reload()
        .snapshot
        .games
        .values()
        .any(|game| game.title == "game"));
}

#[test]
fn lifecycle_recovers_pending_removal_after_config_first_remove_restart() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("game.gba"), b"rom").unwrap();
    let discovery = DiscoveryCoordinator::new(readable.path(), private.path());
    let storage_id = discovery
        .add_location(root.path(), &options())
        .unwrap()
        .storage_id
        .unwrap();
    write_repair_state(private.path(), &storage_id, false, true);

    let lifecycle = lifecycle(&readable, &private, FolderSelectionGrantStore::default());
    assert_eq!(lifecycle.snapshot().state, DiscoveryPhase::Scanning);
    let idle = wait_until_idle(&lifecycle);

    assert_eq!(idle.state, DiscoveryPhase::Idle);
    assert!(ConfigSnapshotCoordinator::new(readable.path())
        .reload()
        .snapshot
        .locations
        .is_empty());
}

#[test]
fn lifecycle_reports_invalid_location_without_hiding_existing_catalog() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let good = tempfile::tempdir().unwrap();
    let bad = tempfile::tempdir().unwrap();
    fs::write(good.path().join("good.gba"), b"good").unwrap();
    let grants = FolderSelectionGrantStore::default();
    let discovery = lifecycle(&readable, &private, grants.clone());
    let good_receipt = grants.issue_approved_path(good.path()).unwrap().token;
    discovery.register_receipt(&good_receipt).unwrap();
    wait_until_idle(&discovery);
    let bad_receipt = grants.issue_approved_path(bad.path()).unwrap().token;
    drop(bad);

    discovery.register_receipt(&bad_receipt).unwrap();
    let problem = wait_until_idle(&discovery);

    assert_eq!(problem.state, DiscoveryPhase::Problem);
    assert!(!problem.diagnostics.is_empty());
    assert!(ConfigSnapshotCoordinator::new(readable.path())
        .reload()
        .snapshot
        .games
        .values()
        .any(|game| game.title == "good"));
}

#[test]
fn selected_locations_scan_into_launch_resolvable_library_records() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    fs::write(first.path().join("Wario_Land_4 (USA).GBA"), b"wario").unwrap();
    fs::write(second.path().join("Pokémon - Emerald.gba"), b"emerald").unwrap();
    let discovery = DiscoveryCoordinator::new(readable.path(), private.path());

    discovery.add_location(first.path(), &options()).unwrap();
    let report = discovery.add_location(second.path(), &options()).unwrap();
    assert_eq!(report.scan.candidates.len(), 2);

    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert!(state.diagnostic.is_none(), "{:?}", state.diagnostic);
    assert_eq!(state.snapshot.games.len(), 2);
    let registry = plugin_policy::registry_for_snapshot(&state.snapshot).unwrap();
    for id in state.snapshot.games.keys() {
        resolver::resolve_route(readable.path(), &state.snapshot, &registry, [], id).unwrap();
    }
}

#[test]
fn repeated_rescan_is_additive_and_uses_private_hash_cache() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let rom = root.path().join("wl4.gba");
    fs::write(&rom, b"rom").unwrap();
    let discovery = DiscoveryCoordinator::new(readable.path(), private.path());

    discovery.add_location(root.path(), &options()).unwrap();
    let repeated = discovery.rescan(&options()).unwrap();
    assert_eq!(repeated.scan.hashed_bytes, 0);
    fs::remove_file(rom).unwrap();
    discovery.rescan(&options()).unwrap();

    assert_eq!(
        ConfigSnapshotCoordinator::new(readable.path())
            .reload()
            .snapshot
            .games
            .len(),
        1
    );
}

#[test]
fn removing_location_deletes_only_owned_matching_payload_and_sweeps_remaining_roots() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    fs::write(first.path().join("same.gba"), b"same").unwrap();
    fs::write(second.path().join("same.gba"), b"same").unwrap();
    let discovery = DiscoveryCoordinator::new(readable.path(), private.path());

    let first_report = discovery.add_location(first.path(), &options()).unwrap();
    let duplicate_report = discovery.add_location(second.path(), &options()).unwrap();
    assert!(duplicate_report
        .scan
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == DiscoveryDiagnosticCode::ClaimConflict));

    discovery
        .remove_location(&first_report.storage_id.unwrap(), &options())
        .unwrap();
    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert_eq!(state.snapshot.games.len(), 1);
    assert!(matches!(
        &state.snapshot.locations.values().next().unwrap()[0],
        Location::File { storage, .. } if !storage.0.is_empty()
    ));
}

#[test]
fn edited_generated_record_survives_location_removal() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    let discovery = DiscoveryCoordinator::new(readable.path(), private.path());
    let added = discovery.add_location(root.path(), &options()).unwrap();
    let library = fs::read_to_string(readable.path().join("catalog/games.yaml"))
        .unwrap()
        .replace("title: wl4", "title: Hand Edited");
    readable::write(readable.path().join("catalog/games.yaml"), library).unwrap();

    discovery
        .remove_location(&added.storage_id.unwrap(), &options())
        .unwrap();

    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert_eq!(state.snapshot.games.len(), 1);
    assert!(
        fs::read_to_string(readable.path().join("catalog/games.yaml"))
            .unwrap()
            .contains("Hand Edited")
    );
}

#[test]
fn selecting_root_with_authored_storage_uses_separate_scanner_storage_and_preserves_authored_route()
{
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    authored(readable.path(), root.path(), "wl4.gba", b"rom");
    let discovery = DiscoveryCoordinator::new(readable.path(), private.path());

    let added = discovery.add_location(root.path(), &options()).unwrap();
    assert_ne!(added.storage_id.as_deref(), Some("authored"));
    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert!(state.diagnostic.is_none(), "{:?}", state.diagnostic);
    assert_eq!(state.snapshot.games.len(), 1);
    assert!(state.snapshot.storage.contains_key("authored"));

    discovery
        .remove_location(&added.storage_id.unwrap(), &options())
        .unwrap();
    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert!(state.snapshot.storage.contains_key("authored"));
    assert_eq!(state.snapshot.games.len(), 1);
    let registry = plugin_policy::registry_for_snapshot(&state.snapshot).unwrap();
    resolver::resolve_route(
        readable.path(),
        &state.snapshot,
        &registry,
        [],
        readable::GBA_ID,
    )
    .unwrap();
}

#[test]
fn edited_generated_record_keeps_storage_so_route_stays_resolvable_after_removal() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    let discovery = DiscoveryCoordinator::new(readable.path(), private.path());
    let added = discovery.add_location(root.path(), &options()).unwrap();
    let storage_id = added.storage_id.unwrap();
    let library = fs::read_to_string(readable.path().join("device.yaml"))
        .unwrap()
        .replace("2026-08-05T00:00:00Z", "authored-time");
    fs::write(readable.path().join("device.yaml"), library).unwrap();

    discovery.remove_location(&storage_id, &options()).unwrap();

    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert!(state.snapshot.storage.contains_key(&storage_id));
    let registry = plugin_policy::registry_for_snapshot(&state.snapshot).unwrap();
    resolver::resolve_route(
        readable.path(),
        &state.snapshot,
        &registry,
        [],
        state.snapshot.games.keys().next().unwrap(),
    )
    .unwrap();
}

#[test]
fn authored_same_content_later_path_beats_generated_candidate_before_hash_dedupe() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let generated_root = tempfile::tempdir().unwrap();
    let authored_root = tempfile::tempdir().unwrap();
    fs::write(generated_root.path().join("aaa.gba"), b"same").unwrap();
    fs::write(authored_root.path().join("zzz.gba"), b"same").unwrap();
    authored(readable.path(), authored_root.path(), "zzz.gba", b"same");
    let discovery = DiscoveryCoordinator::new(readable.path(), private.path());

    discovery
        .add_location(generated_root.path(), &options())
        .unwrap();
    discovery
        .add_location(authored_root.path(), &options())
        .unwrap();

    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert_eq!(state.snapshot.games.len(), 1);
    assert!(state
        .snapshot
        .releases
        .values()
        .next()
        .unwrap()
        .identity
        .is_some());
    assert_eq!(state.snapshot.locations.values().next().unwrap().len(), 3);
}

#[test]
fn overlapping_roots_use_registration_order_not_storage_id_sort_order() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let child = parent.path().join("child");
    fs::create_dir(&child).unwrap();
    fs::write(child.join("same.gba"), b"same").unwrap();
    let discovery = DiscoveryCoordinator::new(readable.path(), private.path());

    let first = discovery
        .add_location(&child, &options())
        .unwrap()
        .storage_id
        .unwrap();
    let second = discovery
        .add_location(parent.path(), &options())
        .unwrap()
        .storage_id
        .unwrap();

    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    let storage = match &state.snapshot.locations.values().next().unwrap()[0] {
        Location::File { storage, .. } => storage.0.clone(),
        _ => panic!("expected file target"),
    };
    assert_eq!(storage, first);
    assert_ne!(storage, second);
}

#[test]
fn traversal_budget_exhaustion_reports_one_bounded_diagnostic() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("one.gba"), b"one").unwrap();
    fs::write(root.path().join("two.gba"), b"two").unwrap();
    let discovery = DiscoveryCoordinator::new(readable.path(), private.path());
    let mut limited = options();
    limited.max_entries = 1;
    limited.max_sortable_entries = 10;

    let report = discovery.add_location(root.path(), &limited).unwrap();

    assert_eq!(
        report
            .scan
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == DiscoveryDiagnosticCode::TraversalLimitReached)
            .count(),
        1
    );
    assert!(report.scan.candidates.len() <= 1);
}

fn authored(root: &std::path::Path, folder: &std::path::Path, path: &str, bytes: &[u8]) {
    let sha = format!("sha256:{}", hex::encode(Sha256::digest(bytes)));
    readable::install(
        root,
        &format!(
            "storage:\n  authored:\n    root: {}\n{}",
            folder.display(),
            readable::gba_locations("authored", path, false).replace(readable::GBA_RELEASE, &sha)
        ),
        &readable::gba_games()
            .replace(readable::GBA_RELEASE, &sha)
            .replace("Wario Land 4", "Curated"),
        &readable::gba_releases().replace(readable::GBA_RELEASE, &sha),
    );
}
