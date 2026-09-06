//! Offline writer only. The caller owns the external lock, stopped consumers,
//! startup gates, and recovery decision. This module never starts a process.

use std::{
    fs::File,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Component, Path, PathBuf},
};

use korri_inputd::bundle::resolve_bundle;
use rustix::fs::{
    open, openat, readlinkat, renameat, statat, symlinkat, unlinkat, AtFlags, Mode, OFlags,
};

const DIRECTORY: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const LINK: OFlags = OFlags::PATH.union(OFlags::NOFOLLOW).union(OFlags::CLOEXEC);
const TEMPORARY_ATTEMPTS: usize = 16;

pub(super) fn select(expected: &Path, requested: &Path) -> Result<String, String> {
    if unsafe { libc::geteuid() } != 0 {
        return Err("root is required".into());
    }
    let store = Path::new(super::STORE_ROOT);
    exact_bundle(expected, store)?;
    exact_bundle(requested, store)?;
    if expected == requested {
        return Err("offline selection requires different expected-current and new bundles".into());
    }
    // Convert output before any write, so an encoding failure cannot hide success.
    let message = format!(
        "active={}",
        requested.to_str().ok_or("bundle path is not valid UTF-8")?
    );
    let root = Path::new(super::STATE_ROOT);
    let directory = open_directory(root)?;
    let current = selected(&directory, expected)?;
    let temporary = Temporary::create(&directory, requested)?;

    // Reopen the fixed path as well as active. A held fd alone could refer to a
    // directory another root writer has detached from the selector namespace.
    let reopened = open_directory(root)?;
    same_inode(&directory, &reopened)?;
    let checked = selected(&directory, expected)?;
    same_inode(&current, &checked).map_err(|_| "expected-current selector inode changed")?;
    renameat(&directory, &temporary.name, &directory, "active")
        .map_err(|error| format!("could not rename offline selector: {error}"))?;
    directory
        .sync_all()
        .map_err(|error| format!("could not sync offline selector directory: {error}"))?;

    let reopened = open_directory(root)?;
    same_inode(&directory, &reopened)?;
    let active = selected(&reopened, requested)?;
    same_inode(&temporary.link, &active)?;
    Ok(message)
}

fn exact_bundle(path: &Path, store: &Path) -> Result<(), String> {
    // The shared resolver owns component/data/profile validation. Unlike normal
    // launch resolution, this operation must not accept aliases or normalization.
    let resolved = resolve_bundle(path, store)?;
    if resolved.as_os_str() != path.as_os_str() {
        return Err("bundle argument must be the exact direct store-root path".into());
    }
    Ok(())
}

fn open_directory(path: &Path) -> Result<File, String> {
    let mut directory = File::from(open("/", DIRECTORY, Mode::empty()).map_err(io_error)?);
    for component in path.components() {
        match component {
            // Check the held / inode below before opening its first child.
            Component::RootDir => {}
            Component::Normal(name) => {
                directory = File::from(
                    openat(&directory, name, DIRECTORY, Mode::empty()).map_err(io_error)?,
                );
            }
            _ => return Err("selector directory path must be absolute and normalized".into()),
        }
        let metadata = directory.metadata().map_err(io_error)?;
        if metadata.uid() != 0 {
            return Err("selector directory and parents must be owned by root".into());
        }
        if metadata.mode() & 0o022 != 0 {
            return Err(
                "selector directory and parents must not allow group or other writes".into(),
            );
        }
    }
    if directory.metadata().map_err(io_error)?.mode() & 0o7777 != 0o711 {
        return Err("existing selector directory must have mode 0711".into());
    }
    Ok(directory)
}

fn selected(directory: &File, expected: &Path) -> Result<File, String> {
    let link = File::from(openat(directory, "active", LINK, Mode::empty()).map_err(io_error)?);
    let metadata = link.metadata().map_err(io_error)?;
    if !metadata.file_type().is_symlink() {
        return Err("active selector must be a symbolic link".into());
    }
    if metadata.uid() != 0 {
        return Err("active selector must be owned by root".into());
    }
    // Replacement must not change an aliased inode, including previous, which
    // this operation must never look up or modify.
    if metadata.nlink() != 1 {
        return Err("active selector must have exactly one hard link".into());
    }
    // Empty-path readlinkat reads the held symlink, not a second path lookup.
    let target = readlinkat(&link, "", Vec::new()).map_err(io_error)?;
    if target.as_bytes() != expected.as_os_str().as_bytes() {
        return Err("expected-current selector target does not match exactly".into());
    }
    Ok(link)
}

fn same_inode(left: &File, right: &File) -> Result<(), String> {
    let left = left.metadata().map_err(io_error)?;
    let right = right.metadata().map_err(io_error)?;
    if (left.dev(), left.ino()) != (right.dev(), right.ino()) {
        return Err("offline selector filesystem identity changed".into());
    }
    Ok(())
}

struct Temporary<'a> {
    directory: &'a File,
    name: PathBuf,
    link: File,
}

impl<'a> Temporary<'a> {
    fn create(directory: &'a File, requested: &Path) -> Result<Self, String> {
        for attempt in 0..TEMPORARY_ATTEMPTS {
            let name = PathBuf::from(format!(".active.offline.{}.{attempt}", std::process::id()));
            match symlinkat(requested, directory, &name) {
                Err(rustix::io::Errno::EXIST) => continue,
                Err(error) => return Err(format!("could not stage offline selector: {error}")),
                Ok(()) => {
                    let link = File::from(
                        openat(directory, &name, LINK, Mode::empty()).map_err(io_error)?,
                    );
                    return Ok(Self {
                        directory,
                        name,
                        link,
                    });
                }
            }
        }
        Err("offline selector temporary names are occupied; nothing was removed".into())
    }
}

impl Drop for Temporary<'_> {
    fn drop(&mut self) {
        // After rename this name is absent. On pre-rename failure, remove only
        // our exact staged inode, never a collided or replacement entry.
        if let (Ok(held), Ok(named)) = (
            self.link.metadata(),
            statat(self.directory, &self.name, AtFlags::SYMLINK_NOFOLLOW),
        ) {
            if (held.dev(), held.ino()) == (named.st_dev, named.st_ino) {
                let _ = unlinkat(self.directory, &self.name, AtFlags::empty());
            }
        }
    }
}

fn io_error(error: impl std::fmt::Display) -> String {
    format!("offline selector filesystem operation failed: {error}")
}
