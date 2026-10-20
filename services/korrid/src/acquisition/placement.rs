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
    create_base(root, contribution(root, artifact))
}

fn create_base(root: &Path, base: PathBuf) -> Result<PathBuf, Failure> {
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

/// The queue marker (ADR 0013): `<hex of file 1>.part`, zero bytes when no
/// byte has arrived yet. Never truncates partial bytes that already exist.
pub(super) fn mark_queued(root: &Path, artifact: &ArtifactRecord) -> Result<(), Failure> {
    let downloads = downloads(&prepare(root, artifact)?)?;
    let first = artifact.files.first().ok_or(Failure::Storage)?;
    let hex = first
        .sha256
        .strip_prefix("sha256:")
        .ok_or(Failure::UnsafePath)?;
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(downloads.join(format!("{hex}.part")))?;
    Ok(())
}

/// When this contribution's interrupted download was queued, or `None` when
/// nothing was interrupted. Any entry in `.downloads` counts: a restart
/// between two files or during unpacking leaves verified files and no `.part`.
/// The earliest creation time is the queue marker for file 1; renaming it to
/// its verified name keeps that time.
pub(super) fn interrupted_since(base: &Path) -> Option<std::time::SystemTime> {
    fs::read_dir(base.join(".downloads"))
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            // A file's mtime moves while bytes arrive. Use it only where the
            // filesystem has no creation time; that can reorder a running
            // download behind a later one.
            metadata.created().or_else(|_| metadata.modified()).ok()
        })
        .min()
}

/// Cancel deletes everything downloaded so far (ADR 0013 amends ADR 0012):
/// partial and verified files alike, so a cancelled download is never taken
/// for an interrupted one and starts again from zero. The caller holds
/// the contribution's `.lock`.
pub(super) fn remove_downloads(base: &Path) -> Result<(), Failure> {
    match fs::remove_dir_all(base.join(".downloads")) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
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

pub(super) fn discard_work(path: &Path) -> Result<(), Failure> {
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

/// ADR 0012 grammar for a provider's reconstructible cache:
/// `<root>/cache/<plugin short name>/<provider name>`.
pub(super) fn provider_cache(root: &Path, plugin_id: &str, provider_id: &str) -> PathBuf {
    let (_, plugin) = plugin_id.split_once(':').expect("validated plugin id");
    let (_, provider) = provider_id.split_once(':').expect("validated provider id");
    root.join("cache").join(plugin).join(provider)
}

/// The private (0700) staging directory of a provider's game downloads.
pub(super) fn game_staging(
    root: &Path,
    plugin_id: &str,
    provider_id: &str,
) -> Result<PathBuf, Failure> {
    downloads(&create_base(
        root,
        provider_cache(root, plugin_id, provider_id),
    )?)
}

/// Startup: a game download never resumes, so every staging file goes.
/// Pinned artifact partials in the same grammar have other names and stay.
pub(super) fn remove_game_staging(directory: &Path) -> Result<(), Failure> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(super::download::STAGING_PREFIX))
            && entry.file_type()?.is_file()
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// ADR 0014: publish a downloaded game into its ready directory,
/// `<provider cache>/<sha256 hex>/<URL basename>` (ADR 0012 grammar: the
/// version directory holds the file named by the URL's last segment). The
/// file moves into a `.tmp-*` work directory, is frozen and synced, and the
/// directory is renamed atomically. Identical bytes already published are
/// reused. Returns the ready directory.
pub(super) fn publish_acquired_game(
    provider_cache: &Path,
    staged: &Path,
    name: &str,
    sha256_hex: &str,
) -> Result<PathBuf, Failure> {
    if !super::claims::safe_file_name(name) || name.starts_with('.') {
        return Err(Failure::UnsafePath);
    }
    let ready = provider_cache.join(sha256_hex);
    if is_ready(&ready) {
        return Ok(ready);
    }
    let work = provider_cache.join(format!(".tmp-{:032x}", rand::random::<u128>()));
    fs::create_dir(&work)?;
    fs::set_permissions(&work, fs::Permissions::from_mode(0o755))?;
    let outcome = (|| {
        fs::rename(staged, work.join(name))?;
        fs::set_permissions(work.join(name), fs::Permissions::from_mode(0o644))?;
        freeze_and_sync(&work)?;
        match fs::rename(&work, &ready) {
            Ok(()) => (),
            // A concurrent job published the same bytes first.
            Err(_) if is_ready(&ready) => (),
            Err(error) => return Err(error.into()),
        }
        File::open(provider_cache)?.sync_all()?;
        Ok(())
    })();
    if work.exists() {
        let _ = discard_work(&work);
    }
    outcome.map(|()| ready)
}

/// The one file in an acquired game's ready directory, when the directory is ready.
pub(crate) fn ready_game_file(ready: &Path) -> Option<PathBuf> {
    if !is_ready(ready) {
        return None;
    }
    let mut files = fs::read_dir(ready)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && !entry.file_name().to_string_lossy().starts_with('.')
        });
    let file = files.next()?;
    files.next().is_none().then(|| file.path())
}
