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

/// Legacy `artifact-placement.ts`: at most 20 names are tried.
const PLACEMENT_ATTEMPTS: usize = 20;
const PLACEMENT_TEMP_PREFIX: &str = ".korri-acquire-";

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Placed {
    /// Path relative to the source root, `/`-separated.
    pub relative: String,
    pub path: PathBuf,
    /// Identical bytes were already there (legacy `alreadyPresent`).
    pub reused: bool,
}

/// Place a staged game inside a paired source without clobbering anything.
/// Identical SHA-256 present: reuse. Different file: `name (n).ext`, where
/// `ext` is the system extension the name matched. Written to a hidden temp
/// in the target directory, synced, then linked into place so a concurrent
/// writer's file is never replaced. `directory` is relative to the source root.
pub(super) fn place_game(
    staged: &Path,
    sha256: &str,
    source_root: &Path,
    directory: &str,
    file_name: &str,
    extension_len: usize,
) -> Result<Placed, Failure> {
    if !super::claims::safe_file_name(file_name)
        || file_name.starts_with('.')
        || extension_len + 1 >= file_name.len()
        || !file_name.is_char_boundary(file_name.len() - extension_len - 1)
    {
        return Err(Failure::UnsafePath);
    }
    let root = fs::canonicalize(source_root).map_err(|_| Failure::Storage)?;
    if !root.is_dir() {
        return Err(Failure::Storage);
    }
    let segments: Vec<_> = directory
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments
        .iter()
        .any(|segment| matches!(*segment, "." | "..") || segment.contains('\0'))
    {
        return Err(Failure::UnsafePath);
    }
    let mut target = root.clone();
    for segment in segments {
        target.push(segment);
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => return Err(Failure::UnsafePath),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&target)?,
            Err(error) => return Err(error.into()),
        }
    }
    if fs::canonicalize(&target)? != target || !target.starts_with(&root) {
        return Err(Failure::UnsafePath);
    }
    let (stem, dot_extension) = file_name.split_at(file_name.len() - extension_len - 1);
    for attempt in 0..PLACEMENT_ATTEMPTS {
        let candidate = if attempt == 0 {
            file_name.to_owned()
        } else {
            format!("{stem} ({attempt}){dot_extension}")
        };
        let path = target.join(&candidate);
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| Failure::UnsafePath)?
            .to_str()
            .ok_or(Failure::UnsafePath)?
            .to_owned();
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match link_new(staged, &target, &path) {
                    Ok(()) => {
                        return Ok(Placed {
                            relative,
                            path,
                            reused: false,
                        })
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(metadata) if metadata.is_file() => {
                if file_sha256(&path)? == sha256 {
                    return Ok(Placed {
                        relative,
                        path,
                        reused: true,
                    });
                }
            }
            // A directory, symlink or special file occupies the name.
            Ok(_) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(Failure::NameTaken)
}

fn link_new(staged: &Path, directory: &Path, path: &Path) -> std::io::Result<()> {
    let temp = directory.join(format!(
        "{PLACEMENT_TEMP_PREFIX}{:032x}",
        rand::random::<u128>()
    ));
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temp)?;
        let mut input = File::options()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(staged)?;
        std::io::copy(&mut input, &mut output)?;
        output.sync_all()?;
        // link(2) never replaces an existing name, unlike rename(2).
        fs::hard_link(&temp, path)?;
        File::open(directory)?.sync_all()
    })();
    let _ = fs::remove_file(&temp);
    result
}

fn file_sha256(path: &Path) -> Result<String, Failure> {
    use sha2::{Digest, Sha256};
    let mut file = File::options()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash)?;
    Ok(hex::encode(hash.finalize()))
}
