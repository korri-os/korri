use crate::process;
use std::{path::PathBuf, time::Duration};

/// Reclaims only Nix-store paths that are unreachable from every GC root.
/// System generations, other plugin selections, and shared dependencies stay
/// reachable and therefore cannot be deleted by this operation.
pub struct SoftwareCleanup {
    nix: PathBuf,
}

impl SoftwareCleanup {
    pub fn new(nix: PathBuf) -> Self {
        Self { nix }
    }

    pub fn reclaim(&self) -> Result<(), String> {
        process::checked(
            &self.nix,
            [
                "--extra-experimental-features",
                "nix-command",
                "store",
                "gc",
            ],
            Duration::from_secs(300),
        )
        .map(|_| ())
        .map_err(|error| format!("storage cleanup failed; uninstall remains incomplete: {error}"))
    }
}
