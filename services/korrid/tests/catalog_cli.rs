use korrid::{catalog_cli, config::snapshot::ConfigSnapshotCoordinator};
use std::{ffi::OsString, fs, path::Path};

fn run(root: &Path, private: &Path, args: Vec<OsString>) -> Result<String, String> {
    catalog_cli::run_with_registry_source(
        &args,
        Some(root.as_os_str()),
        Some(private.as_os_str()),
        korrid::plugin_policy::RegistrySource::Selected(
            korrid::plugin_test_fixtures::installed(root).into(),
        ),
    )
}
#[test]
fn pair_rescan_and_unpair_use_sources_and_one_library_document() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let selected = tempfile::tempdir().unwrap();
    fs::write(selected.path().join("Wario Land 4.gba"), b"game").unwrap();
    run(
        root.path(),
        private.path(),
        vec![
            "pair".into(),
            selected.path().as_os_str().into(),
            "gba".into(),
        ],
    )
    .unwrap();
    let state = ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(state.diagnostic.is_none());
    assert!(state.snapshot.sources.contains_key("gba"));
    assert!(state.snapshot.library.contains_key("wario-land-4"));
    assert!(!root.path().join("catalog/games.yaml").exists());
    assert!(!root.path().join("catalog/releases.yaml").exists());
    let before = fs::read(root.path().join("catalog/library.yaml")).unwrap();
    run(root.path(), private.path(), vec!["rescan".into()]).unwrap();
    assert_eq!(
        fs::read(root.path().join("catalog/library.yaml")).unwrap(),
        before
    );
    run(
        root.path(),
        private.path(),
        vec!["unpair".into(), "gba".into()],
    )
    .unwrap();
    let state = ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(state.snapshot.library.is_empty());
    assert!(state.snapshot.sources.is_empty());
}
#[test]
fn superseded_import_and_remove_are_not_accepted() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    for command in ["import", "remove"] {
        assert!(run(
            root.path(),
            private.path(),
            vec![command.into(), "/tmp".into()]
        )
        .unwrap_err()
        .contains("usage:"));
    }
    assert!(!root.path().join("device.yaml").exists());
}
#[test]
fn pairing_refuses_unknown_systems_without_making_a_source() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let selected = tempfile::tempdir().unwrap();
    let error = run(
        root.path(),
        private.path(),
        vec![
            "pair".into(),
            selected.path().as_os_str().into(),
            "unknown".into(),
        ],
    )
    .unwrap_err();
    assert!(error.contains("unknown") || error.contains("declares"));
    assert!(ConfigSnapshotCoordinator::new(root.path())
        .reload()
        .snapshot
        .sources
        .is_empty());
}
#[test]
fn malformed_library_fails_without_echoing_private_document_content() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("catalog")).unwrap();
    fs::write(root.path().join("device.yaml"), "{}\n").unwrap();
    fs::write(
        root.path().join("catalog/library.yaml"),
        "private-secret-token: [broken\n",
    )
    .unwrap();
    let error = run(root.path(), private.path(), vec!["rescan".into()]).unwrap_err();
    assert!(!error.contains("private-secret-token"));
    assert!(error.contains("validated"));
}

#[test]
fn only_paired_games_enter_catalog_not_build_trees_or_unpaired_folders() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    for directory in [
        "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/build",
        "unpaired",
        "games/gba",
    ] {
        fs::create_dir_all(root.path().join(directory)).unwrap();
        for (file, bytes) in [
            ("compiler.o", b"object".as_slice()),
            ("README.md", b"readme".as_slice()),
            ("Game.gba", b"real game".as_slice()),
        ] {
            fs::write(root.path().join(directory).join(file), bytes).unwrap();
        }
    }
    run(root.path(), private.path(), vec!["rescan".into()]).unwrap();
    assert!(ConfigSnapshotCoordinator::new(root.path())
        .reload()
        .snapshot
        .library
        .is_empty());
    run(
        root.path(),
        private.path(),
        vec![
            "pair".into(),
            root.path().join("games/gba").into_os_string(),
            "gba".into(),
        ],
    )
    .unwrap();
    let state = ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(state.diagnostic.is_none());
    assert_eq!(state.snapshot.library.len(), 1);
    assert_eq!(
        state.snapshot.library["game"].title.as_deref(),
        Some("Game")
    );
    let locations: Vec<_> = state.snapshot.locations.values().flatten().collect();
    assert_eq!(locations.len(), 1);
    assert!(
        matches!(locations[0], korrid::config::Location::File { storage, path, .. }
        if storage.0 == "gba" && path.0 == "Game.gba")
    );
}
