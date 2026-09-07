use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use super::{ConfigSnapshot, StoragePayload};
use crate::config::resolver::ResolvedFileTarget;

pub const IMPLICIT_ROMS_STORAGE_ID: &str = "roms";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedStorageFile {
    pub authorized_root: PathBuf,
    pub path: PathBuf,
}

#[derive(Debug, Error)]
pub enum StorageResolutionError {
    #[error("storage {storage_id} is unavailable")]
    MissingStorage { storage_id: String },
    #[error("storage {storage_id} root is not absolute")]
    RootNotAbsolute { storage_id: String },
    #[error("storage {storage_id} root is unavailable: {source}")]
    RootUnavailable {
        storage_id: String,
        #[source]
        source: std::io::Error,
    },
    #[error("storage {storage_id} root is not a directory")]
    RootNotDirectory { storage_id: String },
    #[error("storage {storage_id} root escapes Korri storage")]
    ImplicitRootEscapes { storage_id: String },
    #[error("storage {storage_id} file target is unsafe")]
    UnsafeTarget { storage_id: String },
    #[error("storage {storage_id} target is missing: {path}")]
    MissingTarget { storage_id: String, path: String },
    #[error("storage {storage_id} target is unavailable: {source}")]
    TargetUnavailable {
        storage_id: String,
        #[source]
        source: std::io::Error,
    },
    #[error("storage {storage_id} target is not a file: {path}")]
    TargetNotFile { storage_id: String, path: String },
    #[error("storage {storage_id} target escapes storage {storage_id} root")]
    TargetEscapesRoot { storage_id: String },
}

impl StorageResolutionError {
    pub fn is_missing_target(&self) -> bool {
        matches!(self, Self::MissingTarget { .. })
    }

    pub fn is_storage_access(&self) -> bool {
        matches!(
            self,
            Self::RootUnavailable { .. } | Self::TargetUnavailable { .. }
        )
    }
}

pub fn validate_file_target_shape(
    target: &ResolvedFileTarget,
) -> Result<(), StorageResolutionError> {
    validate_relative_target(&target.storage_id, &target.path)
}

pub fn resolve_file_target(
    korri_root: &Path,
    snapshot: &ConfigSnapshot,
    target: &ResolvedFileTarget,
) -> Result<ResolvedStorageFile, StorageResolutionError> {
    validate_relative_target(&target.storage_id, &target.path)?;
    if target.storage_id == IMPLICIT_ROMS_STORAGE_ID {
        resolve_implicit_roms_file(korri_root, target)
    } else {
        resolve_configured_file_target(snapshot, target)
    }
}

fn resolve_implicit_roms_file(
    korri_root: &Path,
    target: &ResolvedFileTarget,
) -> Result<ResolvedStorageFile, StorageResolutionError> {
    let storage_id = target.storage_id.clone();
    let declared_root = korri_root.join(IMPLICIT_ROMS_STORAGE_ID);
    let root = declared_root
        .canonicalize()
        .map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => StorageResolutionError::MissingTarget {
                storage_id: storage_id.clone(),
                path: declared_root.join(&target.path).display().to_string(),
            },
            _ => StorageResolutionError::RootUnavailable {
                storage_id: storage_id.clone(),
                source,
            },
        })?;
    let korri_root =
        korri_root
            .canonicalize()
            .map_err(|source| StorageResolutionError::RootUnavailable {
                storage_id: storage_id.clone(),
                source,
            })?;
    if !is_strict_descendant(&root, &korri_root) {
        return Err(StorageResolutionError::ImplicitRootEscapes { storage_id });
    }
    resolve_file_beneath_root(root, target)
}

fn resolve_configured_file_target(
    snapshot: &ConfigSnapshot,
    target: &ResolvedFileTarget,
) -> Result<ResolvedStorageFile, StorageResolutionError> {
    validate_relative_target(&target.storage_id, &target.path)?;
    let storage = snapshot.storage.get(&target.storage_id).ok_or_else(|| {
        StorageResolutionError::MissingStorage {
            storage_id: target.storage_id.clone(),
        }
    })?;
    let root = canonical_storage_root(&target.storage_id, storage)?;
    resolve_file_beneath_root(root, target)
}

fn canonical_storage_root(
    storage_id: &str,
    storage: &StoragePayload,
) -> Result<PathBuf, StorageResolutionError> {
    let root = Path::new(&storage.root.0);
    if !root.is_absolute() {
        return Err(StorageResolutionError::RootNotAbsolute {
            storage_id: storage_id.to_owned(),
        });
    }
    let root = root
        .canonicalize()
        .map_err(|source| StorageResolutionError::RootUnavailable {
            storage_id: storage_id.to_owned(),
            source,
        })?;
    let metadata = root
        .metadata()
        .map_err(|source| StorageResolutionError::RootUnavailable {
            storage_id: storage_id.to_owned(),
            source,
        })?;
    if !metadata.is_dir() {
        return Err(StorageResolutionError::RootNotDirectory {
            storage_id: storage_id.to_owned(),
        });
    }
    Ok(root)
}

fn resolve_file_beneath_root(
    root: PathBuf,
    target: &ResolvedFileTarget,
) -> Result<ResolvedStorageFile, StorageResolutionError> {
    let storage_id = target.storage_id.clone();
    let candidate = root.join(&target.path);
    let path = candidate
        .canonicalize()
        .map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => StorageResolutionError::MissingTarget {
                storage_id: storage_id.clone(),
                path: candidate.display().to_string(),
            },
            _ => StorageResolutionError::TargetUnavailable {
                storage_id: storage_id.clone(),
                source,
            },
        })?;
    if !is_strict_descendant(&path, &root) {
        return Err(StorageResolutionError::TargetEscapesRoot { storage_id });
    }
    let metadata = path
        .metadata()
        .map_err(|source| StorageResolutionError::TargetUnavailable {
            storage_id: target.storage_id.clone(),
            source,
        })?;
    if !metadata.is_file() {
        return Err(StorageResolutionError::TargetNotFile {
            storage_id: target.storage_id.clone(),
            path: path.display().to_string(),
        });
    }
    Ok(ResolvedStorageFile {
        authorized_root: root,
        path,
    })
}

fn validate_relative_target(storage_id: &str, path: &str) -> Result<(), StorageResolutionError> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(StorageResolutionError::UnsafeTarget {
            storage_id: storage_id.to_owned(),
        });
    }
    Ok(())
}

fn is_strict_descendant(path: &Path, root: &Path) -> bool {
    path.parent()
        .is_some_and(|parent| parent == root || parent.starts_with(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_absolute_targets_before_filesystem_access() {
        let target = ResolvedFileTarget {
            storage_id: "selected".into(),
            path: "/storage/emulated/0/Games/wl4.gba".into(),
        };

        assert!(matches!(
            validate_file_target_shape(&target),
            Err(StorageResolutionError::UnsafeTarget { storage_id }) if storage_id == "selected"
        ));
    }
}
