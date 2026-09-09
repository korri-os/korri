use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{symlink, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

pub const STATE_ROOT: &str = "/var/lib/korri-plugin-host";
pub const ROOTS: &str = "/nix/var/nix/gcroots/korri-plugin-host";
pub const OFFICIAL_CATALOG: &str = "/etc/korri-plugin-host/official-catalog-url";
pub const SOURCES_DIR: &str = "sources";
pub const STAGING_DIR: &str = "staging";
pub const MAX_JSON_BYTES: usize = 128 * 1024;

/// Every state mutation, including source writes and stale staging cleanup,
/// requires the same exclusive lock for this root.
pub struct State {
    root: PathBuf,
    _lock: File,
}

impl State {
    pub fn open(root: &Path) -> Result<Self, String> {
        directory(root)?;
        let lock = lock(&root.join("lock"))?;
        Ok(Self {
            root: root.into(),
            _lock: lock,
        })
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub fn staging(&self) -> Result<tempfile::TempDir, String> {
        let root = self.root.join(STAGING_DIR);
        directory(&root)?;
        tempfile::Builder::new()
            .prefix("download-")
            .rand_bytes(8)
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(root)
            .map_err(|e| e.to_string())
    }
    pub fn cleanup_staging(&self) -> Result<(), String> {
        let root = self.root.join(STAGING_DIR);
        if fs::symlink_metadata(&root).is_err() {
            return Ok(());
        }
        directory(&root)?;
        for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            if !name
                .to_str()
                .and_then(|s| s.strip_prefix("download-"))
                .is_some_and(|s| s.len() == 8 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
            {
                return Err("unexpected entry in plugin staging directory".into());
            }
            directory(&entry.path())?;
            fs::remove_dir_all(entry.path()).map_err(|e| e.to_string())?;
        }
        File::open(root)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    }
}

pub fn directory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| e.to_string())?;
        sync_parent(path)?;
    }
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.permissions().mode() & 0o077 != 0 {
        return Err(format!(
            "{} must be a private directory owned by the caller",
            path.display()
        ));
    }
    Ok(())
}

pub fn lock(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;
    protect_file(&file)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("another plugin operation is in progress".into());
    }
    Ok(file)
}

fn protect_file(file: &File) -> Result<(), String> {
    let m = file.metadata().map_err(|e| e.to_string())?;
    if !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.permissions().mode() & 0o077 != 0
    {
        return Err("plugin state must be a private regular file owned by the caller".into());
    }
    Ok(())
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    protect_file(&file)?;
    let mut bytes = Vec::new();
    file.take(MAX_JSON_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err("plugin state JSON is too large".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| format!("invalid plugin receipt: {e}"))
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err("plugin state JSON is too large".into());
    }
    write_atomic(path, &bytes)
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension("new");
    // This path is inside the locked, root-owned host directory. Leftovers are
    // never followed, even after an interrupted write.
    remove(&temporary)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(&temporary, path).map_err(|e| e.to_string())?;
    sync_parent(path)
}

pub fn root_link(path: &Path, target: &Path) -> Result<(), String> {
    let temporary = path.with_extension("new");
    remove(&temporary)?;
    symlink(target, &temporary).map_err(|e| e.to_string())?;
    fs::rename(&temporary, path).map_err(|e| e.to_string())?;
    sync_parent(path)
}

pub fn remove(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => sync_parent(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot remove {}: {e}", path.display())),
    }
}

pub fn sync_parent(path: &Path) -> Result<(), String> {
    File::open(path.parent().ok_or("path has no parent")?)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
}
