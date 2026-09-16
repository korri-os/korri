use super::*;
use crate::config::{resolver, snapshot::ConfigSnapshotCoordinator};
use std::{
    thread,
    time::{Duration, Instant},
};

fn options() -> DiscoveryOptions {
    DiscoveryOptions {
        first_seen_at: "2026-08-05T00:00:00Z".into(),
        max_diagnostics: 100,
        max_candidates: 100,
        ..DiscoveryOptions::default()
    }
}

fn coordinator(readable: &Path, private: &Path) -> DiscoveryCoordinator {
    DiscoveryCoordinator::new(readable, private)
}

fn read_library(root: &Path) -> String {
    fs::read_to_string(root.join(GAMES_FILE_NAME)).unwrap()
}

#[test]
fn pending_ownership_repairs_crash_after_library_commit_before_final_private_write() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    let discovery = coordinator(readable.path(), private.path());
    let storage_id = discovery
        .add_location(root.path(), &options())
        .unwrap()
        .storage_id
        .unwrap();

    let mut private_state = PrivateState::read(private.path()).unwrap();
    private_state.repair.pending_ownership = private_state.ownership.releases.clone();
    private_state.ownership.releases.clear();
    private_state.write(private.path()).unwrap();

    discovery.remove_location(&storage_id, &options()).unwrap();

    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert!(state.snapshot.locations.is_empty());
    assert_eq!(state.snapshot.games.len(), 1);
    let private_state = PrivateState::read(private.path()).unwrap();
    assert!(private_state.repair.pending_ownership.is_empty());
}

#[test]
fn stale_config_revision_rejects_library_commit() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("one.gba"), b"one").unwrap();
    fs::write(root.path().join("two.gba"), b"two").unwrap();
    let discovery = coordinator(readable.path(), private.path());
    let added = discovery.add_location(root.path(), &options()).unwrap();
    let storage_id = added.storage_id.unwrap();
    let current = Documents::read(readable.path()).unwrap();
    let config_yaml = &current.device;
    fs::write(root.path().join("three.gba"), b"tri").unwrap();
    fs::write(
        readable.path().join(DEVICE_FILE_NAME),
        format!("{config_yaml}\n# external edit during scan\n"),
    )
    .unwrap();
    let candidate = ScanCandidate {
        storage_id,
        canonical_path: root.path().join("three.gba").canonicalize().unwrap(),
        relative_path: "three.gba".into(),
        title: "two".into(),
        hash: "sha256:cddd67830982a78cc83998c15c13e49e1cb6bea286c4507cb5510d9c6aba4ec3".into(),
        size: 3,
        system: "gba".into(),
    };
    let mut private_state = PrivateState::read(private.path()).unwrap();

    let result = reconcile_candidates(
        readable.path(),
        private.path(),
        &current,
        &mut private_state,
        &[candidate],
        &options(),
    );

    assert!(
        matches!(result, Err(DiscoveryError::Conflict)),
        "{result:?}"
    );
}

#[test]
fn scan_does_not_hold_yaml_write_lock_while_hashing() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let rom = root.path().join("big.gba");
    fs::write(&rom, vec![7u8; 32 * 1024 * 1024]).unwrap();
    let lock = Arc::new(Mutex::new(()));
    let discovery =
        DiscoveryCoordinator::with_write_lock(readable.path(), private.path(), lock.clone());
    discovery.add_location(root.path(), &options()).unwrap();
    fs::write(&rom, vec![8u8; 32 * 1024 * 1024]).unwrap();
    let worker = {
        let discovery = discovery.clone();
        thread::spawn(move || discovery.rescan(&options()))
    };

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut observed_unlocked = false;
    while Instant::now() < deadline {
        if let Ok(_guard) = lock.try_lock() {
            if !worker.is_finished() {
                observed_unlocked = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        observed_unlocked,
        "write lock stayed held for the whole scan"
    );
    let report = worker.join().unwrap().unwrap();
    assert!(report.scan.hashed_bytes > 0);
}

#[test]
fn adds_two_folders_as_launchable_schema_valid_games_and_reuses_hashes() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    fs::write(first.path().join("Wario_Land_4 (USA).GBA"), b"wario").unwrap();
    fs::write(second.path().join("Pokémon - Emerald.gba"), b"emerald").unwrap();
    let discovery = coordinator(readable.path(), private.path());

    let one = discovery.add_location(first.path(), &options()).unwrap();
    assert_eq!(one.scan.candidates.len(), 1);
    let two = discovery.add_location(second.path(), &options()).unwrap();
    assert_eq!(two.scan.candidates.len(), 2);
    assert!(read_library(readable.path()).contains("Wario Land 4"));
    assert!(read_library(readable.path()).contains("Pokémon Emerald"));

    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert!(state.diagnostic.is_none(), "{:?}", state.diagnostic);
    assert_eq!(state.snapshot.games.len(), 2);
    let registry = plugin_policy::installed_registry().unwrap();
    for id in state.snapshot.games.keys() {
        resolver::resolve_route(readable.path(), &state.snapshot, &registry, [], id).unwrap();
    }

    let repeated = discovery.rescan(&options()).unwrap();
    assert_eq!(repeated.scan.hashed_bytes, 0);
    assert_eq!(
        ConfigSnapshotCoordinator::new(readable.path())
            .reload()
            .snapshot
            .games
            .len(),
        2
    );
}

#[test]
fn preserves_authored_content_identity_and_adds_the_located_copy() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    fs::write(
        readable.path().join(DEVICE_FILE_NAME),
        format!(
            "storage:\n  selected:\n    root: {}\n",
            root.path().display()
        ),
    )
    .unwrap();
    authored(
        readable.path(),
        root.path(),
        "selected",
        "wl4.gba",
        b"rom",
        "Curated Title",
    );

    coordinator(readable.path(), private.path())
        .add_location(root.path(), &options())
        .unwrap();
    let library = read_library(readable.path());
    assert!(library.contains("Curated Title"));
    assert!(fs::read_to_string(readable.path().join(RELEASES_FILE_NAME))
        .unwrap()
        .contains("identity: file"));
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
fn removes_only_fingerprint_matching_generated_records() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    let discovery = coordinator(readable.path(), private.path());
    let add = discovery.add_location(root.path(), &options()).unwrap();
    let storage_id = add.storage_id.unwrap();
    let edited = read_library(readable.path()).replace("title: wl4", "title: Hand Edited");
    crate::config::test_fixtures::write(readable.path().join(GAMES_FILE_NAME), edited).unwrap();

    discovery.remove_location(&storage_id, &options()).unwrap();
    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert_eq!(
        state.snapshot.games.len(),
        1,
        "edited generated record survives as user-owned"
    );
    assert!(read_library(readable.path()).contains("Hand Edited"));
}

#[test]
fn edited_generated_record_is_not_current_for_enrichment_assignment() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    let lock = Arc::new(Mutex::new(()));
    let discovery =
        DiscoveryCoordinator::with_write_lock(readable.path(), private.path(), lock.clone());
    discovery.add_location(root.path(), &options()).unwrap();
    let game = owned_discovery_games(readable.path(), private.path())
        .unwrap()
        .pop()
        .unwrap();
    let edited = read_library(readable.path()).replace("title: wl4", "title: Hand Edited");
    crate::config::test_fixtures::write(readable.path().join(GAMES_FILE_NAME), edited).unwrap();

    assert_eq!(
        current_owned_discovery_game(readable.path(), private.path(), &game).unwrap(),
        None
    );
    assert_eq!(
        update_owned_discovery_title(
            readable.path(),
            private.path(),
            &lock,
            &game,
            "Wario Land 4"
        )
        .unwrap(),
        None
    );
}

#[test]
fn enriched_owned_title_updates_fingerprint_and_survives_rescan_until_removal() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    let lock = Arc::new(Mutex::new(()));
    let discovery =
        DiscoveryCoordinator::with_write_lock(readable.path(), private.path(), lock.clone());
    let add = discovery.add_location(root.path(), &options()).unwrap();

    let game = owned_discovery_games(readable.path(), private.path())
        .unwrap()
        .into_iter()
        .find(|game| game.title == "wl4")
        .unwrap();
    assert!(update_owned_discovery_title(
        readable.path(),
        private.path(),
        &lock,
        &game,
        "Wario Land 4"
    )
    .unwrap()
    .is_some());
    discovery.rescan(&options()).unwrap();
    assert!(read_library(readable.path()).contains("title: Wario Land 4"));

    discovery
        .remove_location(&add.storage_id.unwrap(), &options())
        .unwrap();
    assert_eq!(
        ConfigSnapshotCoordinator::new(readable.path())
            .reload()
            .snapshot
            .locations
            .len(),
        0
    );
}

#[test]
fn removes_unedited_generated_records_and_sweeps_remaining_roots() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    fs::write(first.path().join("same.gba"), b"same-rom").unwrap();
    fs::write(second.path().join("same.gba"), b"same-rom").unwrap();
    let discovery = coordinator(readable.path(), private.path());
    let first_report = discovery.add_location(first.path(), &options()).unwrap();
    discovery.add_location(second.path(), &options()).unwrap();
    assert_eq!(
        ConfigSnapshotCoordinator::new(readable.path())
            .reload()
            .snapshot
            .games
            .len(),
        1
    );

    discovery
        .remove_location(&first_report.storage_id.unwrap(), &options())
        .unwrap();
    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert_eq!(state.snapshot.games.len(), 1);
    let route = &state.snapshot.locations.values().next().unwrap()[0];
    assert!(matches!(route, config::Location::File { storage, .. } if !storage.0.is_empty()));
}

#[test]
fn non_ascii_and_colliding_titles_produce_stable_schema_safe_ids() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Pokémon.gba"), b"one").unwrap();
    fs::write(root.path().join("Pok mon.gba"), b"two").unwrap();

    coordinator(readable.path(), private.path())
        .add_location(root.path(), &options())
        .unwrap();
    let state = ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert_eq!(state.snapshot.games.len(), 2);
    assert!(state.snapshot.games.keys().all(|id| id.len() == 26));
    let before = read_library(readable.path());
    coordinator(readable.path(), private.path())
        .rescan(&options())
        .unwrap();
    assert_eq!(before, read_library(readable.path()));
    assert!(read_library(readable.path()).contains("Pokémon"));
}

#[test]
fn ordinary_rescan_reports_missing_files_but_does_not_delete_generated_records() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let rom = root.path().join("wl4.gba");
    fs::write(&rom, b"rom").unwrap();
    let discovery = coordinator(readable.path(), private.path());
    discovery.add_location(root.path(), &options()).unwrap();
    fs::remove_file(rom).unwrap();

    let report = discovery.rescan(&options()).unwrap();
    assert!(report.scan.candidates.is_empty());
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
fn duplicate_content_reports_a_bounded_diagnostic() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("one.gba"), b"same").unwrap();
    fs::write(root.path().join("two.gba"), b"same").unwrap();

    let report = coordinator(readable.path(), private.path())
        .add_location(root.path(), &options())
        .unwrap();
    assert_eq!(
        ConfigSnapshotCoordinator::new(readable.path())
            .reload()
            .snapshot
            .games
            .len(),
        1
    );
    assert!(report
        .scan
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("duplicates an earlier")));
}

#[test]
fn preserves_raw_decodable_fields_on_scanner_mutation() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    fs::write(readable.path().join(DEVICE_FILE_NAME), format!("providers:\n  \"@local:source\":\n    title: Source\nstorage:\n  selected:\n    root: {}\n", root.path().display())).unwrap();
    crate::config::test_fixtures::write(
        readable.path().join(GAMES_FILE_NAME),
        "games: {}\n# authored comment\n",
    )
    .unwrap();
    crate::config::test_fixtures::write(readable.path().join(RELEASES_FILE_NAME), "releases: {}\n")
        .unwrap();

    coordinator(readable.path(), private.path())
        .rescan(&options())
        .unwrap();
    assert!(fs::read_to_string(readable.path().join(DEVICE_FILE_NAME))
        .unwrap()
        .contains("@local:source"));
    assert!(read_library(readable.path()).contains("authored comment"));
}

#[test]
fn cleanup_repairs_pending_location_removal_after_config_commit() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    let discovery = coordinator(readable.path(), private.path());
    let add = discovery.add_location(root.path(), &options()).unwrap();
    let storage_id = add.storage_id.unwrap();
    let mut private_state = PrivateState::read(private.path()).unwrap();
    private_state
        .repair
        .pending_removals
        .insert(storage_id.clone());
    private_state.write(private.path()).unwrap();
    let config = fs::read_to_string(readable.path().join(DEVICE_FILE_NAME))
        .unwrap()
        .replace(
            &format!(
                "  {storage_id}:\n    root: {}\n",
                root.path().canonicalize().unwrap().display()
            ),
            "",
        );
    fs::write(
        readable.path().join(DEVICE_FILE_NAME),
        config.replace("storage:\nlocations:", "storage: {}\nlocations:"),
    )
    .unwrap();

    let report = discovery.rescan(&options()).unwrap();
    assert!(report.repaired);
    assert!(ConfigSnapshotCoordinator::new(readable.path())
        .reload()
        .snapshot
        .locations
        .is_empty());
}

#[test]
fn final_rename_gate_rejects_external_library_edit() {
    let readable = tempfile::tempdir().unwrap();
    let path = readable.path().join(GAMES_FILE_NAME);
    ensure_fixed_files(readable.path()).unwrap();
    fs::write(&path, "games: {}\n").unwrap();
    let expected = revision("games: {}\n");
    fs::write(&path, "games: {}\n# outside edit\n").unwrap();

    let error = write_atomically(&path, b"games: {}\n", &expected).unwrap_err();
    assert!(matches!(error, DiscoveryError::Conflict));
    assert!(fs::read_to_string(path).unwrap().contains("outside"));
}

fn authored(readable: &Path, roms: &Path, storage: &str, path: &str, bytes: &[u8], title: &str) {
    let fixture = &crate::config::test_fixtures::gba_games();
    let sha = format!("sha256:{}", hex::encode(Sha256::digest(bytes)));
    let device = format!(
        "storage:\n  {storage}:\n    root: {}\n{}",
        roms.display(),
        crate::config::test_fixtures::gba_locations(storage, path, false)
            .replace(crate::config::test_fixtures::GBA_RELEASE, &sha)
    );
    crate::config::test_fixtures::install(
        readable,
        &device,
        &fixture
            .replace(crate::config::test_fixtures::GBA_RELEASE, &sha)
            .replace("Wario Land 4", title),
        &crate::config::test_fixtures::gba_releases()
            .replace(crate::config::test_fixtures::GBA_RELEASE, &sha),
    );
}
