//! Private atomic document I/O. One open store per root in this process.
//! Like identity storage, checks assume a trusted private directory owner;
//! they do not make pathname operations safe against a hostile concurrent owner.
use super::{FederationError, MAX_MEMORY_BYTES};
use std::{
    collections::HashSet,
    fs::{self, DirBuilder, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

static OPEN_ROOTS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

pub(super) struct Store {
    root: PathBuf,
    path: PathBuf,
    bytes: Option<Vec<u8>>,
}

impl Store {
    pub(super) fn open(root: &Path) -> Result<Self, FederationError> {
        validate_ancestors(root)?;
        private_directory(root)?;
        let root = root.canonicalize().map_err(storage)?;
        let mut roots = OPEN_ROOTS
            .get_or_init(Default::default)
            .lock()
            .map_err(storage)?;
        if !roots.insert(root.clone()) {
            return Err(FederationError::Storage);
        }
        drop(roots);
        let mut store = Self {
            path: root.join("federation/peers.json"),
            root,
            bytes: None,
        };
        let directory = store.path.parent().ok_or(FederationError::Storage)?;
        match fs::symlink_metadata(directory) {
            Ok(_) => {
                private_directory(directory)?;
                store.bytes = Some(read(&store.path)?);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                DirBuilder::new()
                    .mode(0o700)
                    .create(directory)
                    .map_err(storage)?;
                sync(&store.root)?;
            }
            Err(_) => return Err(FederationError::Storage),
        }
        Ok(store)
    }

    pub(super) fn bytes(&self) -> Option<&[u8]> {
        self.bytes.as_deref()
    }

    pub(super) fn validate(&self) -> Result<(), FederationError> {
        validate_ancestors(&self.root)?;
        private_directory(&self.root)?;
        private_directory(self.path.parent().ok_or(FederationError::Storage)?)?;
        if let Some(bytes) = &self.bytes {
            if &read(&self.path)? != bytes {
                return Err(FederationError::Storage);
            }
        } else if fs::symlink_metadata(&self.path).is_ok() {
            return Err(FederationError::Storage);
        }
        Ok(())
    }

    pub(super) fn save(&mut self, bytes: Vec<u8>) -> Result<(), FederationError> {
        if bytes.len() > MAX_MEMORY_BYTES {
            return Err(FederationError::Bounds);
        }
        self.validate()?;
        let parent = self.path.parent().ok_or(FederationError::Storage)?;
        let temporary = parent.join(format!(".peers.{}.tmp", rand::random::<u64>()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)
                .map_err(storage)?;
            file.write_all(&bytes).map_err(storage)?;
            file.sync_all().map_err(storage)?;
            fs::rename(&temporary, &self.path).map_err(storage)?;
            sync(parent)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        self.bytes = Some(bytes);
        Ok(())
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        if let Ok(mut roots) = OPEN_ROOTS.get_or_init(Default::default).lock() {
            roots.remove(&self.root);
        }
    }
}

// Validate every existing component, including ancestors of the private root.
pub(super) fn validate_ancestors(path: &Path) -> Result<(), FederationError> {
    for ancestor in path.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        let metadata = fs::symlink_metadata(ancestor).map_err(storage)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(FederationError::Storage);
        }
    }
    Ok(())
}

pub(super) fn private_directory(path: &Path) -> Result<(), FederationError> {
    let metadata = fs::symlink_metadata(path).map_err(storage)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o7777 != 0o700 {
        return Err(FederationError::Storage);
    }
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>, FederationError> {
    let metadata = fs::symlink_metadata(path).map_err(storage)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.nlink() != 1
        || metadata.permissions().mode() & 0o7777 != 0o600
        || metadata.len() > MAX_MEMORY_BYTES as u64
    {
        return Err(FederationError::Storage);
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(storage)?
        .take(MAX_MEMORY_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(storage)?;
    if bytes.len() > MAX_MEMORY_BYTES {
        return Err(FederationError::Bounds);
    }
    Ok(bytes)
}

fn sync(path: &Path) -> Result<(), FederationError> {
    File::open(path)
        .map_err(storage)?
        .sync_all()
        .map_err(storage)
}
fn storage<T>(_: T) -> FederationError {
    FederationError::Storage
}
