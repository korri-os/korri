//! Offline operator entry point. Discovery owns all catalog and private-state writes.

use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use crate::discovery::{DiscoveryCoordinator, DiscoveryError, DiscoveryOptions};

const USAGE: &str = "usage: korrid catalog import <directory> | remove <storage-id>\nSet KORRID_STORAGE_ROOT and KORRID_PRIVATE_STATE_ROOT to existing directories.\nThe korrid daemon must be stopped during catalog changes.";

pub fn run_from_environment(arguments: &[OsString]) -> Result<String, String> {
    eprintln!("Offline catalog change: the korrid daemon must be stopped.");
    run(
        arguments,
        std::env::var_os("KORRID_STORAGE_ROOT").as_deref(),
        std::env::var_os("KORRID_PRIVATE_STATE_ROOT").as_deref(),
    )
}

pub fn run(
    arguments: &[OsString],
    storage_root: Option<&OsStr>,
    private_state_root: Option<&OsStr>,
) -> Result<String, String> {
    run_with_registry_source(
        arguments,
        storage_root,
        private_state_root,
        crate::plugin_policy::RegistrySource::Installed,
    )
}

pub fn run_with_registry_source(
    arguments: &[OsString],
    storage_root: Option<&OsStr>,
    private_state_root: Option<&OsStr>,
    registry_source: crate::plugin_policy::RegistrySource,
) -> Result<String, String> {
    let [command, value] = arguments else {
        return Err(USAGE.into());
    };
    if (command != "import" && command != "remove") || value.is_empty() {
        return Err(USAGE.into());
    }
    let storage_root = required_root("KORRID_STORAGE_ROOT", storage_root)?;
    let private_state_root = required_root("KORRID_PRIVATE_STATE_ROOT", private_state_root)?;
    let coordinator = DiscoveryCoordinator::new(storage_root, private_state_root)
        .with_registry_source(registry_source);
    let report = if command == "import" {
        coordinator.add_location(PathBuf::from(value), &DiscoveryOptions::default())
    } else {
        let storage_id = value.to_str().ok_or("storage id must be UTF-8")?;
        coordinator.remove_location(storage_id, &DiscoveryOptions::default())
    }
        .map_err(|error| match error {
            // Schema errors can quote user configuration, including credentials.
            DiscoveryError::Candidate(_) =>
                "discovery candidate: catalog configuration or discovery state could not be validated; inspect the local files privately".into(),
            other => other.to_string(),
        })?;

    let mut output = format!(
        "Catalog change complete (rescan of all configured locations).\nCandidates: {}\nHashed bytes: {}\nAdded games: {}\nRemoved locations: {}\nRepaired: {}\nDiagnostics: {}",
        report.scan.candidates.len(),
        report.scan.hashed_bytes,
        report.added_games,
        report.removed_locations,
        report.repaired,
        report.scan.diagnostics.len(),
    );
    // Do not print paths, configuration, or identity material in operator reports.
    for diagnostic in report.scan.diagnostics {
        output.push_str(&format!("\n{:?}: {}", diagnostic.code, diagnostic.message));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::snapshot::ConfigSnapshotCoordinator;
    use crate::plugin_policy::RegistrySource;
    use std::sync::Arc;

    #[test]
    fn native_cli_removes_scan_ownership_without_deleting_original_files() {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let carts = tempfile::tempdir().unwrap();
        std::fs::write(carts.path().join("game.gba"), b"original bytes").unwrap();
        let registry =
            RegistrySource::Selected(Arc::new(crate::plugin_test_fixtures::claims_only_registry()));
        let execute = |arguments: &[OsString]| {
            run_with_registry_source(
                arguments,
                Some(readable.path().as_os_str()),
                Some(private.path().as_os_str()),
                registry.clone(),
            )
        };
        execute(&["import".into(), carts.path().as_os_str().to_owned()]).unwrap();
        let before = ConfigSnapshotCoordinator::new(readable.path()).reload();
        let id = before.snapshot.storage.keys().next().unwrap();
        execute(&["remove".into(), id.into()]).unwrap();
        let removed = ConfigSnapshotCoordinator::new(readable.path()).reload();
        assert!(removed.snapshot.storage.is_empty());
        assert!(removed.snapshot.locations.is_empty());
        assert_eq!(
            std::fs::read(carts.path().join("game.gba")).unwrap(),
            b"original bytes"
        );
        // A later rescan must not re-register the removed directory.
        DiscoveryCoordinator::new(readable.path(), private.path())
            .with_registry_source(registry)
            .rescan(&DiscoveryOptions::default())
            .unwrap();
        let rescanned = ConfigSnapshotCoordinator::new(readable.path()).reload();
        assert!(rescanned.snapshot.locations.is_empty());
    }
}

fn required_root(name: &str, value: Option<&OsStr>) -> Result<PathBuf, String> {
    let value = value.filter(|value| !value.is_empty()).ok_or_else(|| {
        format!("{name} must be set to an existing directory; no default is used")
    })?;
    let path = PathBuf::from(value)
        .canonicalize()
        .map_err(|_| format!("{name} must identify an existing, accessible directory"))?;
    if !path.is_dir() {
        return Err(format!("{name} must identify a directory"));
    }
    Ok(path)
}
