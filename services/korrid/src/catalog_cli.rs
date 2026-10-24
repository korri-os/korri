//! Hidden, one-off operator migration. Ordinary library changes use live korri.

use std::ffi::{OsStr, OsString};

const USAGE: &str = "usage: korrid catalog migrate [--apply] [--plan <file>]\nHidden offline operator tool. Set KORRID_STORAGE_ROOT and KORRID_PRIVATE_STATE_ROOT to existing directories.\nThe korrid daemon must be stopped. Use korri library folders for live changes.";

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
    match arguments.split_first() {
        Some((command, arguments)) if command == "migrate" => crate::catalog_migrate::run(
            arguments,
            storage_root,
            private_state_root,
            registry_source,
        ),
        _ => Err(USAGE.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_commands_refuse_before_reading_roots() {
        for args in [
            vec!["pair", "/games", "gba"],
            vec!["unpair", "gba"],
            vec!["rescan"],
            vec!["import", "/secret"],
            vec!["remove", "id"],
            vec!["--help"],
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
    }
}
