use std::{ffi::OsString, fs, path::Path, process::Command};

use korrid::{
    catalog_cli,
    config::{
        resolver::{resolve_launchable_routes_for_platform, RoutePlatform},
        snapshot::{ConfigSnapshotCoordinator, DEVICE_FILE_NAME},
        storage::resolve_file_target,
    },
    discovery::{DiscoveryCoordinator, DiscoveryOptions},
    plugin_policy::registry_for_snapshot,
    GameIdentity,
};
use sha2::{Digest, Sha256};

fn import(storage: &Path, private: &Path, selected: &Path) -> Result<String, String> {
    catalog_cli::run(
        &["import".into(), selected.as_os_str().to_owned()],
        Some(storage.as_os_str()),
        Some(private.as_os_str()),
    )
}

#[test]
fn imports_actual_bytes_and_repeated_import_and_rescan_preserve_identity() {
    let storage = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let selected = tempfile::tempdir().unwrap();
    let bytes = b"GBA catalog import test content\0\x01\xff";
    let file = selected.path().join("Wario Land 4.gba");
    fs::write(&file, bytes).unwrap();

    let first = import(storage.path(), private.path(), selected.path()).unwrap();
    assert!(first.contains("Candidates: 1"), "{first}");
    assert!(first.contains("Added games: 1"), "{first}");
    assert!(first.contains(&format!("Hashed bytes: {}", bytes.len())));
    let snapshot = ConfigSnapshotCoordinator::new(storage.path()).reload();
    assert!(snapshot.diagnostic.is_none(), "{:?}", snapshot.diagnostic);
    assert_eq!(snapshot.snapshot.storage.len(), 1);
    assert_eq!(snapshot.snapshot.games.len(), 1);
    assert_eq!(snapshot.snapshot.releases.len(), 1);
    assert_eq!(snapshot.snapshot.locations.len(), 1);
    let registry = registry_for_snapshot(&snapshot.snapshot).unwrap();
    let routes = resolve_launchable_routes_for_platform(
        storage.path(),
        &snapshot.snapshot,
        &registry,
        [],
        RoutePlatform::Linux,
    );
    assert!(routes.diagnostics.is_empty(), "{:?}", routes.diagnostics);
    assert_eq!(routes.routes.len(), 1);
    let route = &routes.routes[0];
    assert_eq!(route.title.as_deref(), Some("Wario Land 4"));
    assert_eq!(
        route.identity,
        Some(GameIdentity::Hash(format!(
            "sha256:{}",
            hex::encode(Sha256::digest(bytes))
        )))
    );
    let resolved = resolve_file_target(
        storage.path(),
        &snapshot.snapshot,
        route.file_target.as_ref().unwrap(),
    )
    .unwrap();
    assert_eq!(resolved.path, file.canonicalize().unwrap());
    assert_eq!(fs::read(&resolved.path).unwrap(), bytes);

    let second = import(storage.path(), private.path(), selected.path()).unwrap();
    assert!(second.contains("Added games: 0"), "{second}");
    assert!(second.contains("Hashed bytes: 0"), "{second}");
    let rescan = DiscoveryCoordinator::new(storage.path(), private.path())
        .rescan(&DiscoveryOptions::default())
        .unwrap();
    assert_eq!(rescan.added_games, 0);
    assert_eq!(rescan.removed_locations, 0);
    assert_eq!(rescan.scan.hashed_bytes, 0);
    let after = ConfigSnapshotCoordinator::new(storage.path()).reload();
    assert!(after.diagnostic.is_none());
    assert_eq!(after.snapshot.games, snapshot.snapshot.games);
    assert_eq!(after.snapshot.releases, snapshot.snapshot.releases);
    assert_eq!(after.snapshot.locations, snapshot.snapshot.locations);
    assert_eq!(after.snapshot.storage, snapshot.snapshot.storage);
    assert_eq!(fs::read(&file).unwrap(), bytes);
}

#[test]
fn rejects_missing_unknown_and_extra_arguments_before_writes() {
    let storage = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    for arguments in [
        vec![],
        vec!["import"],
        vec!["rescan"],
        vec!["unknown", "/tmp"],
        vec!["import", ""],
        vec!["import", "/tmp", "extra"],
    ] {
        let arguments: Vec<OsString> = arguments.into_iter().map(Into::into).collect();
        let error = catalog_cli::run(
            &arguments,
            Some(storage.path().as_os_str()),
            Some(private.path().as_os_str()),
        )
        .unwrap_err();
        assert!(error.contains("usage: korrid catalog import <directory>"));
    }
    assert_eq!(fs::read_dir(storage.path()).unwrap().count(), 0);
    assert_eq!(fs::read_dir(private.path()).unwrap().count(), 0);
}

#[test]
fn requires_both_explicit_nonempty_existing_directory_roots() {
    let root = tempfile::tempdir().unwrap();
    let arguments = ["import".into(), root.path().as_os_str().to_owned()];
    let good = Some(root.path().as_os_str());
    let missing = root.path().join("missing");
    let file = root.path().join("file");
    fs::write(&file, b"unchanged").unwrap();
    for bad in [
        None,
        Some(std::ffi::OsStr::new("")),
        Some(missing.as_os_str()),
        Some(file.as_os_str()),
    ] {
        assert!(catalog_cli::run(&arguments, bad, good)
            .unwrap_err()
            .contains("KORRID_STORAGE_ROOT"));
        assert!(catalog_cli::run(&arguments, good, bad)
            .unwrap_err()
            .contains("KORRID_PRIVATE_STATE_ROOT"));
    }
    assert!(!missing.exists());
    assert_eq!(fs::read(&file).unwrap(), b"unchanged");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn coordinator_rejects_missing_directory_and_regular_file() {
    let storage = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let selected = tempfile::tempdir().unwrap();
    let missing = selected.path().join("missing");
    let file = selected.path().join("game.gba");
    fs::write(&file, b"keep").unwrap();
    assert!(import(storage.path(), private.path(), &missing)
        .unwrap_err()
        .contains("selected folder is unavailable"));
    assert!(import(storage.path(), private.path(), &file)
        .unwrap_err()
        .contains("selected folder is not a directory"));
    let snapshot = ConfigSnapshotCoordinator::new(storage.path()).reload();
    assert!(snapshot.diagnostic.is_none());
    assert!(snapshot.snapshot.storage.is_empty());
    assert!(snapshot.snapshot.games.is_empty());
    assert!(snapshot.snapshot.releases.is_empty());
    assert!(snapshot.snapshot.locations.is_empty());
    assert_eq!(fs::read(&file).unwrap(), b"keep");
}

#[test]
fn reports_unclaimed_files_without_echoing_paths_or_file_contents() {
    let storage = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let selected = tempfile::tempdir().unwrap();
    let file = selected.path().join("secret-credential.txt");
    fs::write(&file, b"secret-value").unwrap();
    let output = import(storage.path(), private.path(), selected.path()).unwrap();
    assert!(output.contains("Candidates: 0"), "{output}");
    assert!(output.contains("Diagnostics: 1"), "{output}");
    assert!(output.contains("EntryUnclaimed"), "{output}");
    assert!(!output.contains("secret"));
    assert_eq!(fs::read(&file).unwrap(), b"secret-value");
}

#[test]
fn malformed_config_fails_without_echoing_its_contents() {
    let storage = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let selected = tempfile::tempdir().unwrap();
    // This is deliberately invalid syntax, not a hand-authored catalog schema.
    let invalid = b"[secret-credential: [";
    fs::write(storage.path().join(DEVICE_FILE_NAME), invalid).unwrap();
    let error = import(storage.path(), private.path(), selected.path()).unwrap_err();
    assert!(error.contains("discovery candidate"), "{error}");
    assert!(!error.contains("secret-credential"));
    assert_eq!(
        fs::read(storage.path().join(DEVICE_FILE_NAME)).unwrap(),
        invalid
    );
}

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_korrid"));
    command
        .env_remove("KORRID_STORAGE_ROOT")
        .env_remove("KORRID_PRIVATE_STATE_ROOT")
        // Server startup would fail if the CLI accidentally fell through.
        .env("KORRID_MODE", "not-a-server-mode")
        .env("KORRID_ADDRESS", "not-a-listen-address");
    command
}

#[test]
fn binary_imports_offline_then_exits_before_server_configuration() {
    let storage = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let selected = tempfile::tempdir().unwrap();
    fs::write(selected.path().join("game.gba"), b"game bytes").unwrap();
    let output = command()
        .args(["catalog", "import"])
        .arg(selected.path())
        .env("KORRID_STORAGE_ROOT", storage.path())
        .env("KORRID_PRIVATE_STATE_ROOT", private.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("Added games: 1"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("daemon must be stopped"));
    assert_eq!(
        ConfigSnapshotCoordinator::new(storage.path())
            .reload()
            .snapshot
            .games
            .len(),
        1
    );
}

#[test]
fn binary_rejects_missing_arguments_and_environment_without_server_startup() {
    let output = command().arg("catalog").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage: korrid catalog import"));
    let output = command().args(["catalog", "import", "."]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("KORRID_STORAGE_ROOT must be set"));
}
