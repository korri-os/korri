//! The engine-only Steam folder of one Korri person (ADR 0083, contract
//! "Steam folder per person"): `<account root>/steam-engine`, with `steam/`
//! as the Steam root and `home/` as HOME for the host and the game.
//!
//! Grounding: the spike's minimal home (FINDINGS section 11) and the Steam
//! plugin's `korri-steam-host` start wrapper, which makes the folder, its
//! links and the engine file's link as the game user, and grants korrid
//! only the listing and reading of `steam/steamapps/appmanifest_*.acf`.
//! korrid needs only the account root to exist; it never opens anything
//! else in this folder.
use std::path::{Path, PathBuf};

/// The folder's name under the account root.
pub const NAME: &str = "steam-engine";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineFolder {
    root: PathBuf,
}

impl EngineFolder {
    pub fn for_account(account_root: &Path) -> Self {
        Self {
            root: account_root.join(NAME),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// HOME for the host and the game.
    pub fn home(&self) -> PathBuf {
        self.root.join("home")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_is_beside_the_client_runners_steam() {
        let folder = EngineFolder::for_account(Path::new("/var/lib/korri/accounts/p1"));
        assert_eq!(
            folder.root(),
            Path::new("/var/lib/korri/accounts/p1/steam-engine")
        );
        assert_eq!(
            folder.home(),
            Path::new("/var/lib/korri/accounts/p1/steam-engine/home")
        );
        // The discovery location is the same folder's Steam root.
        assert!(crate::plugin_discovery::LOCATIONS
            .iter()
            .any(|(_, glob)| *glob == "steam-engine/steam/steamapps/appmanifest_*.acf"));
    }
}
