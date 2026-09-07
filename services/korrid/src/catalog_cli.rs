//! Offline operator entry point. Discovery owns all catalog and private-state writes.

use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use crate::discovery::{DiscoveryCoordinator, DiscoveryError, DiscoveryOptions};

const USAGE: &str = "usage: korrid catalog import <directory>\nSet KORRID_STORAGE_ROOT and KORRID_PRIVATE_STATE_ROOT to existing directories.\nThe korrid daemon must be stopped during import.";

pub fn run_from_environment(arguments: &[OsString]) -> Result<String, String> {
    eprintln!("Offline catalog import: the korrid daemon must be stopped during import.");
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
    let [command, directory] = arguments else {
        return Err(USAGE.into());
    };
    if command != "import" || directory.is_empty() {
        return Err(USAGE.into());
    }
    let storage_root = required_root("KORRID_STORAGE_ROOT", storage_root)?;
    let private_state_root = required_root("KORRID_PRIVATE_STATE_ROOT", private_state_root)?;
    let report = DiscoveryCoordinator::new(storage_root, private_state_root)
        .add_location(PathBuf::from(directory), &DiscoveryOptions::default())
        .map_err(|error| match error {
            // Schema errors can quote user configuration, including credentials.
            DiscoveryError::Candidate(_) =>
                "discovery candidate: catalog configuration or discovery state could not be validated; inspect the local files privately".into(),
            other => other.to_string(),
        })?;

    let mut output = format!(
        "Catalog import complete (rescan of all configured locations).\nCandidates: {}\nHashed bytes: {}\nAdded games: {}\nRemoved locations: {}\nRepaired: {}\nDiagnostics: {}",
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
