//! Read-only admission projection. The administrator publishes the existing
//! enabled-packages report fields after committing a selection. No RPC and no
//! administrator helper are reachable through this reader.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

pub const REGISTRY_DIRECTORY: &str = "/run/korri-plugin-host";
pub const REGISTRY_PATH: &str = "/run/korri-plugin-host/enabled-packages.json";
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Field names are the existing host Report, not a new plugin manifest.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnabledPackage {
    pub id: String,
    pub package: PathBuf,
    pub files: BTreeMap<String, PathBuf>,
    pub requires: Vec<PathBuf>,
}

pub fn encode(selections: &[EnabledPackage]) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(selections).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("enabled package registry exceeds its read budget".into());
    }
    Ok(bytes)
}

pub fn read() -> Result<Vec<EnabledPackage>, String> {
    read_owned(Path::new(REGISTRY_PATH), 0, Path::new("/"))
}

fn read_owned(path: &Path, owner: u32, anchor: &Path) -> Result<Vec<EnabledPackage>, String> {
    path.strip_prefix(anchor)
        .map_err(|_| "registry is outside its trusted directory")?;
    for parent in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
        if !metadata.is_dir()
            || (metadata.uid() != owner && metadata.uid() != 0)
            || metadata.mode() & 0o022 != 0
        {
            return Err(format!(
                "registry parent {} is not protected by its owner",
                parent.display()
            ));
        }
        if parent == anchor {
            break;
        }
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.uid() != owner || metadata.mode() & 0o022 != 0 {
        return Err("enabled package registry must be an owner-protected regular file".into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("enabled package registry exceeds its read budget".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    #[test]
    fn reader_accepts_an_owner_snapshot_but_never_symlinks_mutable_parents_or_mutable_files() {
        let root = tempfile::Builder::new()
            .prefix("registry-reader-")
            .tempdir_in(std::env::current_dir().unwrap())
            .unwrap();
        // The fixture is the trusted root. Nix's build sandbox may have an
        // unmapped owner for /; production always checks ancestors through /.
        let read_owned = |path: &Path, owner| super::read_owned(path, owner, root.path());
        let path = root.path().join("enabled-packages.json");
        fs::write(&path, "[]").unwrap();
        let owner = unsafe { libc::geteuid() };
        assert!(read_owned(&path, owner).unwrap().is_empty());
        let link = root.path().join("link");
        symlink(&path, &link).unwrap();
        assert!(read_owned(&link, owner).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(read_owned(&path, owner).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(read_owned(&path, owner).is_err());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(
            read_owned(&path, owner).is_err(),
            "missing authority is not an empty fallback registry"
        );
        fs::create_dir(&path).unwrap();
        assert!(read_owned(&path, owner).is_err());
    }
}
