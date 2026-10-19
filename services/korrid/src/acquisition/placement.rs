//! Cache layout and atomic publication. No sidecar journal is authoritative.
use super::{policy, Failure};
use crate::plugin::ArtifactRecord;
use std::{
    fs::{self, File, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

pub(crate) fn contribution(root: &Path, artifact: &ArtifactRecord) -> PathBuf {
    let (plugin, key) = artifact.id.split_once('/').expect("validated artifact id");
    let (_, name) = plugin.split_once(':').expect("validated plugin id");
    root.join("cache").join(name).join(key)
}
pub(crate) fn ready_path(root: &Path, artifact: &ArtifactRecord) -> PathBuf {
    contribution(root, artifact).join(&artifact.version)
}
pub(super) fn is_ready(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

pub(super) fn prepare(root: &Path, artifact: &ArtifactRecord) -> Result<PathBuf, Failure> {
    let base = contribution(root, artifact);
    // The daemon runs with umask 0077. Published runtime paths must still be
    // traversable by the game user. Never chmod an existing parent or symlink.
    let relative = base.strip_prefix(root).map_err(|_| Failure::UnsafePath)?;
    let mut path = root.to_path_buf();
    for segment in relative.components() {
        path.push(segment);
        match fs::create_dir(&path) {
            Ok(()) => fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&path)?.is_dir() {
                    return Err(Failure::UnsafePath);
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(base)
}

pub(super) fn lock(base: &Path) -> Result<Option<File>, Failure> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(base.join(".lock"))?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(Some(file));
    }
    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::WouldBlock {
        Ok(None)
    } else {
        Err(error.into())
    }
}

pub(super) fn downloads(base: &Path) -> Result<PathBuf, Failure> {
    let path = base.join(".downloads");
    match fs::create_dir(&path) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if !fs::symlink_metadata(&path)?.is_dir() {
                return Err(Failure::UnsafePath);
            }
        }
        Err(error) => return Err(error.into()),
    }
    // The cache inherits read/traverse ACLs for games. Staged bytes belong
    // only to korrid, including when reusing a partial download directory.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

pub(super) fn publish(
    base: &Path,
    private_root: &Path,
    artifact: &ArtifactRecord,
    unpack: Option<&Path>,
    files: &[PathBuf],
) -> Result<(), Failure> {
    let work = base.join(format!(".tmp-{:032x}", rand::random::<u128>()));
    fs::create_dir(&work)?;
    fs::set_permissions(&work, fs::Permissions::from_mode(0o755))?;
    let outcome = (|| {
        if let Some(program) = unpack {
            super::unit::unpack(base, private_root, artifact, program, &work, files)?;
        } else {
            let mut names = std::collections::BTreeSet::new();
            for (declaration, file) in artifact.files.iter().zip(files) {
                let name = policy::basename(&declaration.url)?;
                if !names.insert(name.clone()) {
                    return Err(Failure::UnsafePath);
                }
                fs::copy(file, work.join(&name))?;
                fs::set_permissions(work.join(name), fs::Permissions::from_mode(0o644))?;
            }
        }
        freeze_and_sync(&work)?;
        fs::rename(&work, base.join(&artifact.version))?;
        File::open(base)?.sync_all()?;
        fs::remove_dir_all(base.join(".downloads"))?;
        Ok(())
    })();
    if work.exists() {
        let _ = discard_work(&work);
    }
    outcome
}

/// Walk without following symlinks: rootfs images contain absolute symlinks.
fn freeze_and_sync(path: &Path) -> Result<(), Failure> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            freeze_and_sync(&entry?.path())?;
        }
    } else if !metadata.is_file() {
        return Err(Failure::UnsafePath);
    }
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(metadata.permissions().mode() & !0o222),
    )?;
    File::options()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?
        .sync_all()?;
    Ok(())
}

fn discard_work(path: &Path) -> Result<(), Failure> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(metadata.permissions().mode() | 0o700),
        )?;
        for entry in fs::read_dir(path)? {
            discard_work(&entry?.path())?;
        }
        fs::remove_dir(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub(super) fn clean_work(base: &Path) -> Result<(), Failure> {
    for entry in fs::read_dir(base)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(".tmp-"))
        {
            discard_work(&entry.path())?;
        }
    }
    Ok(())
}
