//! Account scaffolding at a fresh launch boundary. No runner directory is visited.
use super::SaveError;
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

fn io(error: std::io::Error) -> SaveError {
    SaveError::Storage(error.to_string())
}
fn invalid(reason: &str) -> SaveError {
    SaveError::Invalid(reason.into())
}

fn acl(users: &[u32], group_access: u16, groups: &[u32]) -> Vec<u8> {
    let mut bytes = 2u32.to_le_bytes().to_vec();
    let mut entry = |tag: u16, permissions: u16, id: u32| {
        bytes.extend(tag.to_le_bytes());
        bytes.extend(permissions.to_le_bytes());
        bytes.extend(id.to_le_bytes());
    };
    entry(1, 7, u32::MAX);
    for user in users.iter().copied().collect::<BTreeSet<_>>() {
        entry(2, 7, user);
    }
    entry(4, group_access, u32::MAX);
    for group in groups {
        entry(8, 7, *group);
    }
    entry(16, 7, u32::MAX);
    entry(32, 0, u32::MAX);
    bytes
}
fn set_acl(file: &File, name: &std::ffi::CStr, bytes: &[u8]) -> Result<(), SaveError> {
    if unsafe {
        libc::fsetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            bytes.as_ptr().cast(),
            bytes.len(),
            0,
        )
    } != 0
    {
        return Err(io(std::io::Error::last_os_error()));
    }
    Ok(())
}
fn no_default(file: &File) -> Result<(), SaveError> {
    if unsafe { libc::fremovexattr(file.as_raw_fd(), c"system.posix_acl_default".as_ptr()) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ENODATA) {
            return Err(io(error));
        }
    }
    Ok(())
}
fn directory(path: &Path, gid: u32, mode: u32) -> Result<File, SaveError> {
    super::store::runtime_save_directory(path)?;
    match fs::DirBuilder::new().mode(mode).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(io(error)),
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(path)
        .map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(invalid("account scaffolding must be owned by korrid"));
    }
    // This changes only the group of the three scaffold directories, never an
    // owner or restored file. Production users/ already has this setgid group.
    if metadata.gid() != gid && unsafe { libc::fchown(file.as_raw_fd(), u32::MAX, gid) } != 0 {
        return Err(io(std::io::Error::last_os_error()));
    }
    file.set_permissions(fs::Permissions::from_mode(mode))
        .map_err(io)?;
    Ok(file)
}

/// Preserve the old account's runtime-user/group inheritance. Only saves/
/// gains named korrid sharing. Runtime ids use the game backend's configuration.
/// Existing runner siblings (including Steam) are never opened or changed.
pub fn prepare_account(root: &Path, person: &str) -> Result<PathBuf, SaveError> {
    let account = super::account_root(root, person).map_err(SaveError::Invalid)?;
    super::store::runtime_save_directory(&account.join("saves"))?;
    let (runtime_uid, runtime_gid) = crate::host::configured_runtime_identity();
    let daemon_uid = unsafe { libc::geteuid() };
    let users = directory(&root.join("users"), runtime_gid, 0o2710)?;
    no_default(&users)?;
    set_acl(
        &users,
        c"system.posix_acl_access",
        &acl(&[daemon_uid], 1, &[]),
    )?;
    users.sync_all().map_err(io)?;
    let person_dir = directory(&account, runtime_gid, 0o2770)?;
    set_acl(
        &person_dir,
        c"system.posix_acl_access",
        &acl(&[runtime_uid], 7, &[]),
    )?;
    set_acl(
        &person_dir,
        c"system.posix_acl_default",
        &acl(&[runtime_uid], 7, &[runtime_gid]),
    )?;
    person_dir.sync_all().map_err(io)?;
    let saves = directory(&account.join("saves"), runtime_gid, 0o2770)?;
    let shared = acl(&[daemon_uid, runtime_uid], 0, &[]);
    set_acl(&saves, c"system.posix_acl_access", &shared)?;
    set_acl(&saves, c"system.posix_acl_default", &shared)?;
    saves.sync_all().map_err(io)?;
    person_dir.sync_all().map_err(io)?;
    users.sync_all().map_err(io)?;
    File::open(root)
        .and_then(|file| file.sync_all())
        .map_err(io)?;
    Ok(account)
}
