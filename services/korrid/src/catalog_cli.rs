//! Offline operator entry point. Discovery owns all catalog and private-state writes.

use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use crate::discovery::{DiscoveryCoordinator, DiscoveryError, DiscoveryOptions};

const USAGE: &str = "usage: korrid catalog pair <directory> <system> | unpair <source> | rescan | migrate [--apply] [--plan <file>]\nSet KORRID_STORAGE_ROOT and KORRID_PRIVATE_STATE_ROOT to existing directories.\nThe korrid daemon must be stopped during catalog changes.";

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
    if arguments
        .first()
        .is_some_and(|command| command == "migrate")
    {
        return crate::catalog_migrate::run(
            &arguments[1..],
            storage_root,
            private_state_root,
            registry_source,
        );
    }
    match arguments {
        [command, directory, system]
            if command == "pair" && !directory.is_empty() && !system.is_empty() => {}
        [command, source] if command == "unpair" && !source.is_empty() => {}
        [command] if command == "rescan" => {}
        _ => return Err(USAGE.into()),
    }
    let storage_root = required_root("KORRID_STORAGE_ROOT", storage_root)?;
    let private_state_root = required_root("KORRID_PRIVATE_STATE_ROOT", private_state_root)?;
    let coordinator = DiscoveryCoordinator::new(storage_root, private_state_root)
        .with_registry_source(registry_source);
    let report = match arguments {
        [command, directory, system] if command == "pair" => coordinator.pair(
            PathBuf::from(directory),
            system.to_str().ok_or("system must be UTF-8")?,
            &DiscoveryOptions::default(),
        ),
        [command, source] if command == "unpair" => coordinator.unpair(
            source.to_str().ok_or("source must be UTF-8")?,
            &DiscoveryOptions::default(),
        ),
        [_] => coordinator.rescan(&DiscoveryOptions::default()),
        _ => unreachable!("validated command"),
    }
    .map_err(safe_error)?;

    let units = report
        .scan
        .scanned
        .iter()
        .map(|source| source.units.len())
        .sum::<usize>();
    let hashed_bytes = report
        .scan
        .scanned
        .iter()
        .map(|source| source.hashed_bytes)
        .sum::<u64>()
        + report.scan.declared.hashed_bytes;
    let diagnostics = report.scan.diagnostics.len()
        + report
            .scan
            .scanned
            .iter()
            .map(|source| source.diagnostics.len())
            .sum::<usize>();
    let output = format!("Catalog change complete (rescan of paired sources).\nUnits: {units}\nHashed bytes: {hashed_bytes}\nAdded games: {}\nRemoved locations: {}\nRepaired: {}\nDiagnostics: {diagnostics}", report.added_games, report.removed_locations, report.repaired);
    // Never print paths, configuration, attention messages, or identity material.
    Ok(output)
}

fn safe_error(error: DiscoveryError) -> String {
    match error {
        DiscoveryError::Conflict => "discovery configuration changed; reload and try again".into(),
        DiscoveryError::Pairing(error) => match error {
            crate::discovery::sources::PairingError::SourceOverlap(_) => {
                "SourceOverlap: folder overlaps a paired source"
            }
            crate::discovery::sources::PairingError::PluginData => {
                "PluginData: folder holds plugin data"
            }
            crate::discovery::sources::PairingError::UnknownSystem(_) => {
                "UnknownSystem: no enabled plugin declares this system or group"
            }
            crate::discovery::sources::PairingError::UnknownSource(_) => {
                "UnknownSource: source is not paired"
            }
            crate::discovery::sources::PairingError::InvalidPath(_) => {
                "InvalidPath: folder is unavailable"
            }
        }
        .into(),
        _ => "discovery state could not be read or validated; inspect local files privately".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_removed_commands_and_wrong_argument_counts_before_reading_roots() {
        for args in [
            vec!["import", "/secret"],
            vec!["remove", "id"],
            vec!["pair", "/games"],
            vec!["rescan", "extra"],
            vec![],
        ] {
            assert_eq!(
                run(
                    &args.iter().map(OsString::from).collect::<Vec<_>>(),
                    None,
                    None
                ),
                Err(USAGE.into())
            );
        }
        for args in [
            vec!["pair", "/games", "gba"],
            vec!["unpair", "gba"],
            vec!["rescan"],
        ] {
            assert!(run(
                &args.iter().map(OsString::from).collect::<Vec<_>>(),
                None,
                None
            )
            .unwrap_err()
            .starts_with("KORRID_STORAGE_ROOT"));
        }
    }
    #[test]
    fn errors_never_echo_configuration_or_paths() {
        for error in [
            DiscoveryError::Candidate("Bearer secret".into()),
            DiscoveryError::Storage("/private/key".into()),
            DiscoveryError::Pairing(crate::discovery::sources::PairingError::SourceOverlap(
                "secret".into(),
            )),
        ] {
            let output = safe_error(error);
            assert!(!output.contains("secret"));
            assert!(!output.contains("/private"));
        }
    }
    #[test]
    fn offline_pair_unpair_rescan_preserves_original_files() {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let games = tempfile::tempdir().unwrap();
        std::fs::write(games.path().join("game.gba"), b"original bytes").unwrap();
        let registry = crate::plugin_policy::RegistrySource::Selected(std::sync::Arc::new(
            crate::plugin_test_fixtures::installed(readable.path()),
        ));
        let execute = |args: &[OsString]| {
            run_with_registry_source(
                args,
                Some(readable.path().as_os_str()),
                Some(private.path().as_os_str()),
                registry.clone(),
            )
        };
        execute(&["pair".into(), games.path().as_os_str().into(), "gba".into()]).unwrap();
        execute(&["unpair".into(), "gba".into()]).unwrap();
        execute(&["rescan".into()]).unwrap();
        assert_eq!(
            std::fs::read(games.path().join("game.gba")).unwrap(),
            b"original bytes"
        );
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
