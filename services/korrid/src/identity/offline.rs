//! One local, offline exception for retiring the publicly known test owner.
//! The caller must already hold exclusive authority over every identity user.
//! The directory lock coordinates this command only; it cannot stop a daemon.
use super::{
    parse_named_keys, parse_owner_statement, state_from_owner_event, DeviceIdentity, IdentityError,
    IdentityState, OwnerStatementStatus, DEVICE_KEY_FILE, IDENTITY_DIRECTORY, MAX_EVENT_BYTES,
    OWNER_EVENT_FILE,
};
use std::{
    ffi::{CStr, CString, OsStr},
    fs::{File, Metadata, OpenOptions},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::{Component, Path},
};
use zeroize::Zeroizing;

#[cfg(test)]
pub(crate) mod checkpoints;

// BIP-340 vector-zero / repository test signer (secret 3). This is deliberately
// not a general cross-owner transfer: the production cut retires this key only.
const TEST_OWNER: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";

pub(crate) struct Replacement<'a> {
    pub expected_device: &'a str,
    pub expected_owner: &'a str,
    pub expected_event: &'a str,
    pub new_owner: &'a str,
    pub template: &'a Path,
    pub signed_event: &'a Path,
}

pub(crate) fn replace_test_owner(
    root: &Path,
    replacement: Replacement<'_>,
) -> Result<IdentityState, String> {
    replace(root, replacement).map_err(|error| {
        format!("offline owner replacement failed: {error}; keep all authority stopped and inspect the current binding before recovery; never restore the test owner")
    })
}

fn replace(root: &Path, replacement: Replacement<'_>) -> Result<IdentityState, IdentityError> {
    if replacement.expected_owner != TEST_OWNER || replacement.new_owner == TEST_OWNER {
        return Err(IdentityError::InvalidEvent(
            "only test-to-private-owner replacement is allowed".into(),
        ));
    }
    let root_directory = Directory::open(root)?;
    let directory = root_directory.child(OsStr::new(IDENTITY_DIRECTORY))?;
    // No lock file or new persisted schema. Closing this descriptor releases it.
    syscall(unsafe { libc::flock(directory.file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) })?;
    let key = directory.read(DEVICE_KEY_FILE, 128)?;
    let keys = parse_named_keys(key.bytes.to_vec(), "device")?;
    let device = keys.public_key().to_hex();
    if device != replacement.expected_device {
        return Err(IdentityError::InvalidEvent(
            "expected device does not match the existing key".into(),
        ));
    }
    let current = directory.read(OWNER_EVENT_FILE, MAX_EVENT_BYTES)?;
    let old = DeviceIdentity::verify_owner_statement(text(&current)?, &device)?;
    if old.status != OwnerStatementStatus::Owned
        || old.owner_public_key != replacement.expected_owner
        || old.event_id != replacement.expected_event
    {
        return Err(IdentityError::InvalidEvent(
            "expected current owned binding does not match".into(),
        ));
    }
    let template = read_source(replacement.template)?;
    let signed = read_source(replacement.signed_event)?;
    DeviceIdentity::verify_signed_owner_binding(
        &device,
        text(&template)?,
        replacement.new_owner,
        text(&signed)?,
    )?;
    let new = parse_owner_statement(&signed.bytes, &device)?;

    let temporary_name = CString::new(format!(".{OWNER_EVENT_FILE}.{}.tmp", rand::random::<u64>()))
        .map_err(|_| IdentityError::Storage)?;
    let mut temporary = Temporary::create(&directory, temporary_name)?;
    // A root-controlled invocation may write for a different service UID/GID.
    // A service-UID invocation must also retain the exact existing file owner.
    let staged_metadata = temporary
        .file
        .metadata()
        .map_err(|_| IdentityError::Storage)?;
    if staged_metadata.uid() != current.metadata.uid()
        || staged_metadata.gid() != current.metadata.gid()
    {
        syscall(unsafe {
            libc::fchown(
                temporary.file.as_raw_fd(),
                current.metadata.uid(),
                current.metadata.gid(),
            )
        })?;
    }
    syscall(unsafe { libc::fchmod(temporary.file.as_raw_fd(), 0o600) })?;
    temporary
        .file
        .write_all(&signed.bytes)
        .map_err(|_| IdentityError::Storage)?;
    temporary
        .file
        .sync_all()
        .map_err(|_| IdentityError::Storage)?;
    let staged = directory.read_name(&temporary.name, MAX_EVENT_BYTES)?;
    if staged.bytes != signed.bytes
        || !same_inode(
            &staged.metadata,
            &temporary
                .file
                .metadata()
                .map_err(|_| IdentityError::Storage)?,
        )
    {
        return Err(IdentityError::Storage);
    }

    #[cfg(test)]
    checkpoints::observe(checkpoints::Checkpoint::Staged, &directory.file);

    // Recheck through the original public paths immediately before the rename.
    // Offline exclusivity, not this check or the advisory lock, excludes a live
    // importer or privileged writer in the remaining check/rename interval.
    recheck_directory(root, &root_directory, &directory)?;
    key.require_unchanged(&directory.read(DEVICE_KEY_FILE, 128)?)?;
    current.require_unchanged(&directory.read(OWNER_EVENT_FILE, MAX_EVENT_BYTES)?)?;
    let destination = name(OsStr::new(OWNER_EVENT_FILE))?;
    #[cfg(test)]
    checkpoints::observe(checkpoints::Checkpoint::BeforeRename, &directory.file);
    syscall(unsafe {
        libc::renameat(
            directory.file.as_raw_fd(),
            temporary.name.as_ptr(),
            directory.file.as_raw_fd(),
            destination.as_ptr(),
        )
    })?;
    temporary.renamed = true;
    #[cfg(test)]
    checkpoints::observe(checkpoints::Checkpoint::Renamed, &directory.file);
    directory
        .file
        .sync_all()
        .map_err(|_| IdentityError::Storage)?;
    // Any error after rename is an ambiguous outcome, never an old-owner retry.
    recheck_directory(root, &root_directory, &directory)?;
    key.require_unchanged(&directory.read(DEVICE_KEY_FILE, 128)?)?;
    staged.require_unchanged(&directory.read(OWNER_EVENT_FILE, MAX_EVENT_BYTES)?)?;
    Ok(state_from_owner_event(&new.event, &device))
}

/// Holds the same offline-writer lock as owner replacement. It never creates
/// state, and verifies operator-supplied public evidence before exposing state.
pub(crate) struct VerifiedPrivateOwner<'a> {
    root: &'a Path,
    root_directory: Directory,
    directory: Directory,
    key: PrivateFile,
    owner: PrivateFile,
    pub state: IdentityState,
}

impl<'a> VerifiedPrivateOwner<'a> {
    pub(crate) fn open(
        root: &'a Path,
        expected_device: &str,
        expected_owner: &str,
        expected_event: &str,
    ) -> Result<Self, IdentityError> {
        if unsafe { libc::geteuid() } == 0 || expected_owner == TEST_OWNER {
            return Err(IdentityError::InvalidEvent(
                "grant reconciliation requires the private-state UID and a private owner".into(),
            ));
        }
        let root_directory = Directory::open(root)?;
        let directory = root_directory.child(OsStr::new(IDENTITY_DIRECTORY))?;
        syscall(unsafe { libc::flock(directory.file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) })?;
        let key = directory.read(DEVICE_KEY_FILE, 128)?;
        let keys = parse_named_keys(key.bytes.to_vec(), "device")?;
        let device = keys.public_key().to_hex();
        let owner = directory.read(OWNER_EVENT_FILE, MAX_EVENT_BYTES)?;
        let statement = parse_owner_statement(&owner.bytes, &device)?;
        let state = state_from_owner_event(&statement.event, &device);
        if !matches!(&state, IdentityState::Owned { device_public_key, owner_public_key, event_id, .. }
            if device_public_key == expected_device && owner_public_key == expected_owner && event_id == expected_event)
        {
            return Err(IdentityError::InvalidEvent(
                "expected new owned binding does not match the existing identity".into(),
            ));
        }
        Ok(Self {
            root,
            root_directory,
            directory,
            key,
            owner,
            state,
        })
    }

    pub(crate) fn recheck(&self) -> Result<(), IdentityError> {
        recheck_directory(self.root, &self.root_directory, &self.directory)?;
        self.key
            .require_unchanged(&self.directory.read(DEVICE_KEY_FILE, 128)?)?;
        self.owner
            .require_unchanged(&self.directory.read(OWNER_EVENT_FILE, MAX_EVENT_BYTES)?)
    }
}

fn read_source(path: &Path) -> Result<PrivateFile, IdentityError> {
    let parent = Directory::open(path.parent().ok_or(IdentityError::Storage)?)?;
    parent.read_name(
        &name(path.file_name().ok_or(IdentityError::Storage)?)?,
        MAX_EVENT_BYTES,
    )
}

fn text(file: &PrivateFile) -> Result<&str, IdentityError> {
    std::str::from_utf8(&file.bytes)
        .map_err(|_| IdentityError::InvalidEvent("owner input is not UTF-8".into()))
}

pub(crate) struct Directory {
    file: File,
    metadata: Metadata,
}

impl Directory {
    pub(crate) fn open(path: &Path) -> Result<Self, IdentityError> {
        if !path.is_absolute() {
            return Err(IdentityError::Storage);
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")
            .map_err(|_| IdentityError::Storage)?;
        let mut ancestors = vec![];
        for component in path.components() {
            match component {
                Component::RootDir => {}
                Component::Normal(component) => {
                    ancestors.push(file.metadata().map_err(|_| IdentityError::Storage)?);
                    file = open_at(&file, &name(component)?, libc::O_RDONLY | libc::O_DIRECTORY)?;
                }
                _ => return Err(IdentityError::Storage),
            }
        }
        let metadata = file.metadata().map_err(|_| IdentityError::Storage)?;
        let uid = unsafe { libc::geteuid() };
        if metadata.mode() & 0o7777 != 0o700 || (uid != 0 && metadata.uid() != uid) {
            return Err(IdentityError::Storage);
        }
        for ancestor in ancestors {
            // System ancestors may be traversable, but not replaceable by an
            // unrelated UID. Root-owned sticky directories protect /tmp entries.
            let sticky_root = ancestor.uid() == 0 && ancestor.mode() & libc::S_ISVTX != 0;
            if (ancestor.uid() != 0 && ancestor.uid() != metadata.uid())
                || (ancestor.mode() & 0o022 != 0 && !sticky_root)
            {
                return Err(IdentityError::Storage);
            }
        }
        Ok(Self { file, metadata })
    }

    pub(crate) fn child(&self, child: &OsStr) -> Result<Self, IdentityError> {
        let file = open_at(
            &self.file,
            &name(child)?,
            libc::O_RDONLY | libc::O_DIRECTORY,
        )?;
        let metadata = file.metadata().map_err(|_| IdentityError::Storage)?;
        if metadata.mode() & 0o7777 != 0o700
            || metadata.uid() != self.metadata.uid()
            || metadata.gid() != self.metadata.gid()
        {
            return Err(IdentityError::Storage);
        }
        Ok(Self { file, metadata })
    }

    pub(crate) fn read(&self, file_name: &str, limit: usize) -> Result<PrivateFile, IdentityError> {
        self.read_name(&name(OsStr::new(file_name))?, limit)
    }

    fn read_name(&self, file_name: &CStr, limit: usize) -> Result<PrivateFile, IdentityError> {
        // NONBLOCK makes FIFOs reject promptly instead of waiting for a writer.
        let file = open_at(&self.file, file_name, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let metadata = file.metadata().map_err(|_| IdentityError::Storage)?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.mode() & 0o7777 != 0o600
            || metadata.uid() != self.metadata.uid()
            || metadata.gid() != self.metadata.gid()
            || metadata.len() > limit as u64
        {
            return Err(IdentityError::Storage);
        }
        let mut bytes = Zeroizing::new(Vec::new());
        (&file)
            .take((limit + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| IdentityError::Storage)?;
        if bytes.len() > limit
            || !same_file(
                &metadata,
                &file.metadata().map_err(|_| IdentityError::Storage)?,
            )
        {
            return Err(IdentityError::Storage);
        }
        Ok(PrivateFile { bytes, metadata })
    }
}

pub(crate) struct PrivateFile {
    bytes: Zeroizing<Vec<u8>>,
    metadata: Metadata,
}

impl PrivateFile {
    fn require_unchanged(&self, other: &Self) -> Result<(), IdentityError> {
        if self.bytes != other.bytes || !same_file(&self.metadata, &other.metadata) {
            return Err(IdentityError::Storage);
        }
        Ok(())
    }
}

fn same_inode(left: &Metadata, right: &Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

fn same_file(left: &Metadata, right: &Metadata) -> bool {
    same_inode(left, right)
        && left.mode() == right.mode()
        && left.uid() == right.uid()
        && left.gid() == right.gid()
        && left.nlink() == right.nlink()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
}

fn recheck_directory(
    root: &Path,
    expected_root: &Directory,
    expected_identity: &Directory,
) -> Result<(), IdentityError> {
    let reopened = Directory::open(root)?;
    let identity = reopened.child(OsStr::new(IDENTITY_DIRECTORY))?;
    let unchanged = |left: &Metadata, right: &Metadata| {
        same_inode(left, right)
            && left.mode() == right.mode()
            && left.uid() == right.uid()
            && left.gid() == right.gid()
    };
    if !unchanged(&reopened.metadata, &expected_root.metadata)
        || !unchanged(&identity.metadata, &expected_identity.metadata)
    {
        return Err(IdentityError::Storage);
    }
    Ok(())
}

struct Temporary<'a> {
    directory: &'a Directory,
    name: CString,
    file: File,
    renamed: bool,
}

impl<'a> Temporary<'a> {
    fn create(directory: &'a Directory, name: CString) -> Result<Self, IdentityError> {
        let file = open_at(
            &directory.file,
            &name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        )?;
        Ok(Self {
            directory,
            name,
            file,
            renamed: false,
        })
    }
}

impl Drop for Temporary<'_> {
    fn drop(&mut self) {
        if !self.renamed {
            // Only remove a name this invocation created. Failed create_new must
            // never remove another writer's file, including a pre-existing link.
            unsafe {
                libc::unlinkat(self.directory.file.as_raw_fd(), self.name.as_ptr(), 0);
            }
        }
    }
}

fn name(value: &OsStr) -> Result<CString, IdentityError> {
    if value.as_bytes().contains(&b'/') || value == "." || value == ".." {
        return Err(IdentityError::Storage);
    }
    CString::new(value.as_bytes()).map_err(|_| IdentityError::Storage)
}

fn open_at(directory: &File, name: &CStr, flags: libc::c_int) -> Result<File, IdentityError> {
    // The descriptor is owned exactly once after a successful openat. All names
    // are single components; every ancestor is opened separately without links.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    syscall(fd)?;
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn syscall(result: libc::c_int) -> Result<(), IdentityError> {
    if result < 0 {
        Err(IdentityError::Storage)
    } else {
        Ok(())
    }
}
