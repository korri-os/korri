use super::journal::{IdentitySwitchJournal, JournalError, JournalPhase, MAX_JOURNAL_BYTES};
use crate::identity::{DeviceIdentity, IdentityState, VerifiedOwnerStatement};
use rand::random;
use std::{
    ffi::CString,
    fs::{self, DirBuilder, OpenOptions},
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::Arc,
};

const IDENTITY_SWITCH_DIRECTORY: &str = "identity-switch";
const JOURNAL_FILE: &str = "journal.json";
const NEXT_ROOT_DIRECTORY: &str = "next-root";
const IDENTITY_DIRECTORY: &str = "identity";
const DEVICE_KEY_FILE: &str = "device.key";
const OWNER_EVENT_FILE: &str = "owner.event.json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExchangePosition {
    Before,
    After,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityInspection {
    pub device_public_key: String,
    pub owner_public_key: String,
    pub owner_event_id: String,
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum IdentitySwitchStorageError {
    #[error("identity-switch path is unsafe: {0}")]
    UnsafePath(String),
    #[error("identity-switch storage operation failed: {0}")]
    Storage(&'static str),
    #[error("identity-switch journal error: {0}")]
    Journal(#[from] JournalError),
    #[error("identity-switch journal is already present")]
    JournalExists,
    #[error("identity-switch journal is absent")]
    JournalMissing,
    #[error("identity-switch journal phase is invalid for this operation")]
    WrongPhase,
    #[error("identity-switch commit details differ from the prepared transaction")]
    JournalChanged,
    #[error("identity-switch staging root is already present")]
    StagingExists,
    #[error("identity evidence does not match either complete exchange state")]
    EvidenceMismatch,
    #[error("identity directories are on different filesystems")]
    CrossFilesystem,
    #[error("atomic identity exchange is unsupported")]
    ExchangeUnsupported,
    #[error("atomic identity exchange failed")]
    ExchangeFailed,
    #[cfg(test)]
    #[error("injected identity-switch crash boundary")]
    InjectedFault,
}

#[derive(Clone)]
pub struct IdentitySwitchStorage {
    root: PathBuf,
    transaction: PathBuf,
    expected_uid: u32,
    exchange: Arc<dyn RenameExchange>,
}

impl IdentitySwitchStorage {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, IdentitySwitchStorageError> {
        Self::with_exchange(
            root.into(),
            unsafe { libc::geteuid() },
            Arc::new(LinuxRenameExchange),
        )
    }

    fn with_exchange(
        root: PathBuf,
        expected_uid: u32,
        exchange: Arc<dyn RenameExchange>,
    ) -> Result<Self, IdentitySwitchStorageError> {
        validate_directory(&root, expected_uid)?;
        let transaction = root.join(IDENTITY_SWITCH_DIRECTORY);
        create_or_validate_directory(&transaction, expected_uid)?;
        Ok(Self {
            root,
            transaction,
            expected_uid,
            exchange,
        })
    }

    pub fn transaction_directory(&self) -> &Path {
        &self.transaction
    }

    pub fn live_private_root(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn staged_private_root(&self) -> PathBuf {
        self.transaction.join(NEXT_ROOT_DIRECTORY)
    }

    pub fn prepare_staging_root(&self) -> Result<PathBuf, IdentitySwitchStorageError> {
        if path_exists_no_follow(&self.journal_path())? {
            return Err(IdentitySwitchStorageError::JournalExists);
        }
        let staged = self.staged_private_root();
        if path_exists_no_follow(&staged)? {
            return Err(IdentitySwitchStorageError::StagingExists);
        }
        create_directory(&staged)?;
        sync_directory(&self.transaction)?;
        Ok(staged)
    }

    pub fn load_journal(
        &self,
    ) -> Result<Option<IdentitySwitchJournal>, IdentitySwitchStorageError> {
        let path = self.journal_path();
        let Some(bytes) = read_private_optional(&path, self.expected_uid, MAX_JOURNAL_BYTES)?
        else {
            return Ok(None);
        };
        Ok(Some(IdentitySwitchJournal::decode(&bytes)?))
    }

    pub fn write_preparing(
        &self,
        journal: &IdentitySwitchJournal,
    ) -> Result<(), IdentitySwitchStorageError> {
        if journal.phase() != JournalPhase::Preparing {
            return Err(IdentitySwitchStorageError::WrongPhase);
        }
        if self.load_journal()?.is_some() {
            return Err(IdentitySwitchStorageError::JournalExists);
        }
        self.write_journal(journal, WriteFault::None)
    }

    pub fn write_commit(
        &self,
        journal: &IdentitySwitchJournal,
    ) -> Result<(), IdentitySwitchStorageError> {
        if journal.phase() != JournalPhase::Commit {
            return Err(IdentitySwitchStorageError::WrongPhase);
        }
        let current = self
            .load_journal()?
            .ok_or(IdentitySwitchStorageError::JournalMissing)?;
        if current.phase() != JournalPhase::Preparing {
            return Err(IdentitySwitchStorageError::WrongPhase);
        }
        if current.committing() != *journal {
            return Err(IdentitySwitchStorageError::JournalChanged);
        }
        self.write_journal(journal, WriteFault::None)
    }

    pub fn cleanup_abandoned_without_journal(&self) -> Result<bool, IdentitySwitchStorageError> {
        if self.load_journal()?.is_some() {
            return Ok(false);
        }
        self.remove_safe_staging_tree()
    }

    pub fn abort_preparing(&self) -> Result<(), IdentitySwitchStorageError> {
        let journal = self
            .load_journal()?
            .ok_or(IdentitySwitchStorageError::JournalMissing)?;
        if journal.phase() != JournalPhase::Preparing {
            return Err(IdentitySwitchStorageError::WrongPhase);
        }
        fs::remove_file(self.journal_path())
            .map_err(|_| IdentitySwitchStorageError::Storage("remove preparing journal"))?;
        sync_directory(&self.transaction)?;
        self.remove_safe_staging_tree()?;
        Ok(())
    }

    pub fn finish_commit(&self) -> Result<(), IdentitySwitchStorageError> {
        let journal = self
            .load_journal()?
            .ok_or(IdentitySwitchStorageError::JournalMissing)?;
        if journal.phase() != JournalPhase::Commit {
            return Err(IdentitySwitchStorageError::WrongPhase);
        }
        fs::remove_file(self.journal_path())
            .map_err(|_| IdentitySwitchStorageError::Storage("remove committed journal"))?;
        sync_directory(&self.transaction)?;
        self.remove_safe_staging_tree()?;
        Ok(())
    }

    pub fn inspect_live_identity(
        &self,
        expected_owner_event_json: &str,
    ) -> Result<IdentityInspection, IdentitySwitchStorageError> {
        self.inspect_identity(&self.root, expected_owner_event_json)
    }

    pub fn inspect_staged_identity(
        &self,
        expected_owner_event_json: &str,
    ) -> Result<IdentityInspection, IdentitySwitchStorageError> {
        self.inspect_identity(&self.staged_private_root(), expected_owner_event_json)
    }

    pub fn classify_exchange(
        &self,
        old_owner_event_json: &str,
        new_owner_event_json: &str,
    ) -> Result<ExchangePosition, IdentitySwitchStorageError> {
        let before = self.inspection_matches(&self.root, old_owner_event_json)?
            && self.inspection_matches(&self.staged_private_root(), new_owner_event_json)?;
        let after = self.inspection_matches(&self.root, new_owner_event_json)?
            && self.inspection_matches(&self.staged_private_root(), old_owner_event_json)?;
        match (before, after) {
            (true, false) => Ok(ExchangePosition::Before),
            (false, true) => Ok(ExchangePosition::After),
            _ => Err(IdentitySwitchStorageError::EvidenceMismatch),
        }
    }

    pub fn exchange_identity_directories(
        &self,
        old_owner_event_json: &str,
        new_owner_event_json: &str,
    ) -> Result<ExchangePosition, IdentitySwitchStorageError> {
        self.exchange_with_fault(
            old_owner_event_json,
            new_owner_event_json,
            ExchangeFault::None,
        )
    }

    fn inspect_identity(
        &self,
        private_root: &Path,
        expected_owner_event_json: &str,
    ) -> Result<IdentityInspection, IdentitySwitchStorageError> {
        let expected = DeviceIdentity::derive_owner_statement(expected_owner_event_json)
            .map_err(|_| IdentitySwitchStorageError::EvidenceMismatch)?;
        validate_directory(private_root, self.expected_uid)?;
        let identity_directory = private_root.join(IDENTITY_DIRECTORY);
        validate_directory(&identity_directory, self.expected_uid)?;
        validate_private_file(&identity_directory.join(DEVICE_KEY_FILE), self.expected_uid)?;
        validate_private_file(
            &identity_directory.join(OWNER_EVENT_FILE),
            self.expected_uid,
        )?;
        validate_safe_tree(&identity_directory, self.expected_uid)?;

        let identity = DeviceIdentity::load_or_create(private_root)
            .map_err(|_| IdentitySwitchStorageError::EvidenceMismatch)?;
        let actual_device = identity
            .device_public_key()
            .ok_or(IdentitySwitchStorageError::EvidenceMismatch)?;
        let actual_event = identity
            .owner_statement_json()
            .ok_or(IdentitySwitchStorageError::EvidenceMismatch)?;
        let actual = DeviceIdentity::derive_owner_statement(&actual_event)
            .map_err(|_| IdentitySwitchStorageError::EvidenceMismatch)?;
        if actual_device != expected.device_public_key || actual != expected {
            return Err(IdentitySwitchStorageError::EvidenceMismatch);
        }
        match identity.state() {
            IdentityState::Owned { event_id, .. } | IdentityState::Revoked { event_id, .. }
                if event_id == &expected.event_id => {}
            _ => return Err(IdentitySwitchStorageError::EvidenceMismatch),
        }
        Ok(inspection(expected))
    }

    fn inspection_matches(
        &self,
        private_root: &Path,
        expected_owner_event_json: &str,
    ) -> Result<bool, IdentitySwitchStorageError> {
        match self.inspect_identity(private_root, expected_owner_event_json) {
            Ok(_) => Ok(true),
            Err(IdentitySwitchStorageError::EvidenceMismatch) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn exchange_with_fault(
        &self,
        old_owner_event_json: &str,
        new_owner_event_json: &str,
        fault: ExchangeFault,
    ) -> Result<ExchangePosition, IdentitySwitchStorageError> {
        let journal = self
            .load_journal()?
            .ok_or(IdentitySwitchStorageError::JournalMissing)?;
        if journal.phase() != JournalPhase::Commit {
            return Err(IdentitySwitchStorageError::WrongPhase);
        }
        match self.classify_exchange(old_owner_event_json, new_owner_event_json)? {
            ExchangePosition::After => return Ok(ExchangePosition::After),
            ExchangePosition::Before => {}
        }
        let live = self.root.join(IDENTITY_DIRECTORY);
        let staged_parent = self.staged_private_root();
        let staged = staged_parent.join(IDENTITY_DIRECTORY);
        let live_parent_device = fs::metadata(&self.root)
            .map_err(|_| IdentitySwitchStorageError::Storage("inspect live parent"))?
            .dev();
        let staged_parent_device = fs::metadata(&staged_parent)
            .map_err(|_| IdentitySwitchStorageError::Storage("inspect staged parent"))?
            .dev();
        if live_parent_device != staged_parent_device {
            return Err(IdentitySwitchStorageError::CrossFilesystem);
        }
        self.exchange.exchange(&live, &staged)?;
        fault.fail_if(ExchangeFault::AfterExchange)?;
        sync_directory(&self.root)?;
        fault.fail_if(ExchangeFault::AfterLiveParentSync)?;
        sync_directory(&staged_parent)?;
        fault.fail_if(ExchangeFault::AfterStagedParentSync)?;
        if self.classify_exchange(old_owner_event_json, new_owner_event_json)?
            != ExchangePosition::After
        {
            return Err(IdentitySwitchStorageError::EvidenceMismatch);
        }
        Ok(ExchangePosition::After)
    }

    fn remove_safe_staging_tree(&self) -> Result<bool, IdentitySwitchStorageError> {
        let staged = self.staged_private_root();
        if !path_exists_no_follow(&staged)? {
            return Ok(false);
        }
        validate_safe_tree(&staged, self.expected_uid)?;
        fs::remove_dir_all(&staged)
            .map_err(|_| IdentitySwitchStorageError::Storage("remove staging tree"))?;
        sync_directory(&self.transaction)?;
        Ok(true)
    }

    fn journal_path(&self) -> PathBuf {
        self.transaction.join(JOURNAL_FILE)
    }

    fn write_journal(
        &self,
        journal: &IdentitySwitchJournal,
        fault: WriteFault,
    ) -> Result<(), IdentitySwitchStorageError> {
        let bytes = journal.encode()?;
        let temporary = self
            .transaction
            .join(format!(".{JOURNAL_FILE}.{}.tmp", random::<u64>()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&temporary)
                .map_err(|_| IdentitySwitchStorageError::Storage("create journal temporary"))?;
            file.write_all(&bytes)
                .map_err(|_| IdentitySwitchStorageError::Storage("write journal temporary"))?;
            file.sync_all()
                .map_err(|_| IdentitySwitchStorageError::Storage("sync journal temporary"))?;
            fault.fail_if(WriteFault::AfterTemporarySync)?;
            fs::rename(&temporary, self.journal_path())
                .map_err(|_| IdentitySwitchStorageError::Storage("publish journal"))?;
            fault.fail_if(WriteFault::AfterRename)?;
            sync_directory(&self.transaction)
        })();
        #[cfg(test)]
        let injected = matches!(&result, Err(IdentitySwitchStorageError::InjectedFault));
        #[cfg(not(test))]
        let injected = false;
        if result.is_err() && !injected && path_exists_no_follow(&temporary).unwrap_or(false) {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    #[cfg(test)]
    pub(crate) fn test_with_exchange(
        root: PathBuf,
        expected_uid: u32,
        exchange: Arc<dyn RenameExchange>,
    ) -> Result<Self, IdentitySwitchStorageError> {
        Self::with_exchange(root, expected_uid, exchange)
    }

    #[cfg(test)]
    pub(crate) fn test_write_preparing(
        &self,
        journal: &IdentitySwitchJournal,
        fault: WriteFault,
    ) -> Result<(), IdentitySwitchStorageError> {
        self.write_journal(journal, fault)
    }

    #[cfg(test)]
    pub(crate) fn test_write_commit(
        &self,
        journal: &IdentitySwitchJournal,
        fault: WriteFault,
    ) -> Result<(), IdentitySwitchStorageError> {
        self.write_journal(journal, fault)
    }

    #[cfg(test)]
    pub(crate) fn test_exchange(
        &self,
        old_owner_event_json: &str,
        new_owner_event_json: &str,
        fault: ExchangeFault,
    ) -> Result<ExchangePosition, IdentitySwitchStorageError> {
        self.exchange_with_fault(old_owner_event_json, new_owner_event_json, fault)
    }
}

fn inspection(statement: VerifiedOwnerStatement) -> IdentityInspection {
    IdentityInspection {
        device_public_key: statement.device_public_key,
        owner_public_key: statement.owner_public_key,
        owner_event_id: statement.event_id,
    }
}

fn create_or_validate_directory(
    path: &Path,
    expected_uid: u32,
) -> Result<(), IdentitySwitchStorageError> {
    match fs::symlink_metadata(path) {
        Ok(_) => validate_directory(path, expected_uid),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_directory(path)?;
            sync_directory(
                path.parent()
                    .ok_or(IdentitySwitchStorageError::Storage("find directory parent"))?,
            )
        }
        Err(_) => Err(IdentitySwitchStorageError::Storage("inspect directory")),
    }
}

fn create_directory(path: &Path) -> Result<(), IdentitySwitchStorageError> {
    DirBuilder::new()
        .mode(0o700)
        .create(path)
        .map_err(|_| IdentitySwitchStorageError::Storage("create directory"))
}

fn validate_directory(path: &Path, expected_uid: u32) -> Result<(), IdentitySwitchStorageError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| IdentitySwitchStorageError::UnsafePath(path.display().to_string()))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != expected_uid
        || metadata.permissions().mode() & 0o7777 != 0o700
    {
        return Err(IdentitySwitchStorageError::UnsafePath(
            path.display().to_string(),
        ));
    }
    Ok(())
}

fn validate_private_file(path: &Path, expected_uid: u32) -> Result<(), IdentitySwitchStorageError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| IdentitySwitchStorageError::UnsafePath(path.display().to_string()))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.uid() != expected_uid
        || metadata.nlink() != 1
        || metadata.permissions().mode() & 0o7777 != 0o600
    {
        return Err(IdentitySwitchStorageError::UnsafePath(
            path.display().to_string(),
        ));
    }
    Ok(())
}

fn validate_safe_tree(path: &Path, expected_uid: u32) -> Result<(), IdentitySwitchStorageError> {
    validate_directory(path, expected_uid)?;
    let entries = fs::read_dir(path)
        .map_err(|_| IdentitySwitchStorageError::Storage("read safe transaction tree"))?;
    for entry in entries {
        let entry = entry
            .map_err(|_| IdentitySwitchStorageError::Storage("read transaction tree entry"))?;
        let child = entry.path();
        let metadata = fs::symlink_metadata(&child)
            .map_err(|_| IdentitySwitchStorageError::UnsafePath(child.display().to_string()))?;
        if metadata.file_type().is_symlink() || metadata.uid() != expected_uid {
            return Err(IdentitySwitchStorageError::UnsafePath(
                child.display().to_string(),
            ));
        }
        if metadata.is_dir() {
            validate_safe_tree(&child, expected_uid)?;
        } else if metadata.is_file()
            && metadata.nlink() == 1
            && metadata.permissions().mode() & 0o7777 == 0o600
        {
        } else {
            return Err(IdentitySwitchStorageError::UnsafePath(
                child.display().to_string(),
            ));
        }
    }
    Ok(())
}

fn read_private_optional(
    path: &Path,
    expected_uid: u32,
    limit: usize,
) -> Result<Option<Vec<u8>>, IdentitySwitchStorageError> {
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(IdentitySwitchStorageError::UnsafePath(
                path.display().to_string(),
            ))
        }
    };
    let metadata = file
        .metadata()
        .map_err(|_| IdentitySwitchStorageError::UnsafePath(path.display().to_string()))?;
    if !metadata.is_file()
        || metadata.uid() != expected_uid
        || metadata.nlink() != 1
        || metadata.permissions().mode() & 0o7777 != 0o600
        || metadata.len() > limit as u64
    {
        return Err(IdentitySwitchStorageError::UnsafePath(
            path.display().to_string(),
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    Read::by_ref(&mut file)
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| IdentitySwitchStorageError::Storage("read journal"))?;
    if bytes.len() > limit {
        return Err(IdentitySwitchStorageError::Journal(JournalError::TooLarge));
    }
    Ok(Some(bytes))
}

fn sync_directory(path: &Path) -> Result<(), IdentitySwitchStorageError> {
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| IdentitySwitchStorageError::Storage("open directory for sync"))?;
    directory
        .sync_all()
        .map_err(|_| IdentitySwitchStorageError::Storage("sync directory"))
}

fn path_exists_no_follow(path: &Path) -> Result<bool, IdentitySwitchStorageError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(IdentitySwitchStorageError::Storage("inspect path")),
    }
}

pub(crate) trait RenameExchange: Send + Sync {
    fn exchange(&self, left: &Path, right: &Path) -> Result<(), IdentitySwitchStorageError>;
}

struct LinuxRenameExchange;

impl RenameExchange for LinuxRenameExchange {
    fn exchange(&self, left: &Path, right: &Path) -> Result<(), IdentitySwitchStorageError> {
        let left = CString::new(left.as_os_str().as_bytes())
            .map_err(|_| IdentitySwitchStorageError::ExchangeFailed)?;
        let right = CString::new(right.as_os_str().as_bytes())
            .map_err(|_| IdentitySwitchStorageError::ExchangeFailed)?;
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                libc::AT_FDCWD,
                left.as_ptr(),
                libc::AT_FDCWD,
                right.as_ptr(),
                libc::RENAME_EXCHANGE,
            )
        };
        if result == 0 {
            return Ok(());
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EXDEV) => Err(IdentitySwitchStorageError::CrossFilesystem),
            Some(libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP) => {
                Err(IdentitySwitchStorageError::ExchangeUnsupported)
            }
            _ => Err(IdentitySwitchStorageError::ExchangeFailed),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WriteFault {
    None,
    AfterTemporarySync,
    AfterRename,
}

impl WriteFault {
    fn fail_if(self, boundary: Self) -> Result<(), IdentitySwitchStorageError> {
        #[cfg(test)]
        if self == boundary {
            return Err(IdentitySwitchStorageError::InjectedFault);
        }
        let _ = boundary;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExchangeFault {
    None,
    AfterExchange,
    AfterLiveParentSync,
    AfterStagedParentSync,
}

impl ExchangeFault {
    fn fail_if(self, boundary: Self) -> Result<(), IdentitySwitchStorageError> {
        #[cfg(test)]
        if self == boundary {
            return Err(IdentitySwitchStorageError::InjectedFault);
        }
        let _ = boundary;
        Ok(())
    }
}
