//! The engine-only Steam folder of one Korri person (ADR 0083, contract
//! "Steam folder per person").
//!
//! Grounding: the spike's minimal home (FINDINGS section 11, `min-home.sh`):
//! `steam/` is the Steam root, `home/` is HOME for the host and the game,
//! `home/.steam/steam` and `home/.steam/sdkarm64` are links, the engine is
//! `steam/linuxarm64/steamclient.so` and the compat tool is a link in
//! `steam/compatibilitytools.d`. The contract's changes are kept: nothing is
//! bound read-only and nothing is borrowed from the client runner's folder;
//! the engine writes its own `config/config.vdf` and `local.vdf`. The spike's
//! `state/` (harness files) has no product use and is not made.
//!
//! The folder sits beside the client runner's `steam/` under the account
//! root and is created by the game user, inside the host unit, as the client
//! runner's folder is created inside the game unit. korrid itself only reads
//! `steam/steamapps/appmanifest_*.acf` for discovery.
use std::{
    fs, io,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
};

/// The folder's name under the account root. Chosen beside the client
/// runner's `steam` (`@korri:steam` `prepareLaunch`); no other record names it.
pub const NAME: &str = "steam-engine";

/// The compat-tool link's name. Steam reads each tool's own
/// `compatibilitytool.vdf`, so the directory name is not the tool's name.
pub const PROTON_LINK: &str = "korri-proton";

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

    /// The Steam root.
    pub fn steam(&self) -> PathBuf {
        self.root.join("steam")
    }

    /// HOME for the host and the game.
    pub fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    /// The engine the host loads: a link to the pinned artifact's file.
    pub fn engine(&self) -> PathBuf {
        self.steam().join("linuxarm64").join("steamclient.so")
    }

    /// The only part of the folder korrid reads: installed app manifests.
    pub fn manifests(&self) -> PathBuf {
        self.steam().join("steamapps")
    }

    /// Make or repair the layout. `engine` is the pinned artifact's
    /// `linuxarm64/steamclient.so`; `proton` is korrid's Proton artifact.
    /// Links that point elsewhere are replaced, so a new pin takes effect;
    /// anything that is not a link where a link belongs is refused, never
    /// removed, because it may be the person's data.
    pub fn prepare(&self, engine: &Path, proton: Option<&Path>) -> io::Result<()> {
        for directory in [
            self.root.clone(),
            self.steam(),
            self.steam().join("linuxarm64"),
            self.steam().join("compatibilitytools.d"),
            self.home(),
            self.home().join(".steam"),
        ] {
            private_directory(&directory)?;
        }
        link(engine, &self.engine())?;
        link(&self.steam(), &self.home().join(".steam").join("steam"))?;
        link(
            &self.steam().join("linuxarm64"),
            &self.home().join(".steam").join("sdkarm64"),
        )?;
        let tool = self.steam().join("compatibilitytools.d").join(PROTON_LINK);
        match proton {
            Some(proton) => link(proton, &tool)?,
            None => remove_link(&tool)?,
        }
        Ok(())
    }
}

fn private_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(io::Error::other(format!(
            "{} is not a directory",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path)?;
            // The engine's sign-in stores live here: only the game user.
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        }
        Err(error) => Err(error),
    }
}

fn link(target: &Path, path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            if fs::read_link(path)? == target {
                return Ok(());
            }
            fs::remove_file(path)?;
        }
        Ok(_) => {
            return Err(io::Error::other(format!(
                "{} is not a link; it is left as it is",
                path.display()
            )))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    symlink(target, path)
}

fn remove_link(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::remove_file(path),
        Ok(_) => Err(io::Error::other(format!(
            "{} is not a link; it is left as it is",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_is_the_spike_minimal_home_with_korrids_links() {
        let temp = tempfile::tempdir().unwrap();
        let account = temp.path().join("person");
        fs::create_dir(&account).unwrap();
        let engine = temp.path().join("cache/linuxarm64/steamclient.so");
        let proton = temp.path().join("cache/proton");
        let folder = EngineFolder::for_account(&account);
        folder.prepare(&engine, Some(&proton)).unwrap();
        let root = account.join("steam-engine");
        assert_eq!(folder.root(), root);
        for directory in [
            "steam",
            "steam/linuxarm64",
            "steam/compatibilitytools.d",
            "home",
            "home/.steam",
        ] {
            let metadata = fs::symlink_metadata(root.join(directory)).unwrap();
            assert!(metadata.is_dir(), "{directory}");
            assert_eq!(metadata.permissions().mode() & 0o777, 0o700, "{directory}");
        }
        assert_eq!(
            fs::read_link(root.join("steam/linuxarm64/steamclient.so")).unwrap(),
            engine
        );
        assert_eq!(
            fs::read_link(root.join("home/.steam/steam")).unwrap(),
            root.join("steam")
        );
        assert_eq!(
            fs::read_link(root.join("home/.steam/sdkarm64")).unwrap(),
            root.join("steam/linuxarm64")
        );
        assert_eq!(
            fs::read_link(root.join("steam/compatibilitytools.d/korri-proton")).unwrap(),
            proton
        );
        // No sign-in store is made or borrowed: the engine writes its own.
        assert!(!root.join("steam/config").exists());
        assert!(!root.join("steam/local.vdf").exists());
        assert!(!account.join("steam").exists());
    }

    #[test]
    fn a_new_pin_moves_the_links_and_the_persons_files_are_never_removed() {
        let temp = tempfile::tempdir().unwrap();
        let folder = EngineFolder::for_account(temp.path());
        folder
            .prepare(
                Path::new("/old/steamclient.so"),
                Some(Path::new("/old/proton")),
            )
            .unwrap();
        fs::create_dir_all(folder.steam().join("config")).unwrap();
        fs::write(folder.steam().join("config/config.vdf"), "engine-written").unwrap();
        folder
            .prepare(Path::new("/new/steamclient.so"), None)
            .unwrap();
        assert_eq!(
            fs::read_link(folder.engine()).unwrap(),
            Path::new("/new/steamclient.so")
        );
        assert!(
            fs::symlink_metadata(folder.steam().join("compatibilitytools.d/korri-proton")).is_err()
        );
        assert_eq!(
            fs::read_to_string(folder.steam().join("config/config.vdf")).unwrap(),
            "engine-written"
        );
        // A real file where the engine link belongs is refused, not replaced.
        fs::remove_file(folder.engine()).unwrap();
        fs::write(folder.engine(), "a real engine copy").unwrap();
        assert!(folder
            .prepare(Path::new("/new/steamclient.so"), None)
            .is_err());
        assert_eq!(
            fs::read_to_string(folder.engine()).unwrap(),
            "a real engine copy"
        );
    }
}
