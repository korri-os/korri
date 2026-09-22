use super::{
    journal::{DataDisposition, IdentitySwitchJournal, JournalPhase, ReplacementSigner},
    storage::{
        ExchangeFault, ExchangePosition, IdentitySwitchStorage, IdentitySwitchStorageError,
        RenameExchange, WriteFault,
    },
};
use crate::identity::{DeviceIdentity, OwnerStatementStatus};
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::Keys,
    types::Timestamp,
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::TempDir;

struct Fixture {
    _temporary: TempDir,
    root: PathBuf,
    storage: IdentitySwitchStorage,
    old_event: String,
    new_event: String,
    revocation: String,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().to_path_buf();
        mode(&root, 0o700);
        let old_owner = Keys::generate();
        let (old_identity, old_event) = owned_identity(&root, &old_owner, 100);
        let storage = IdentitySwitchStorage::open(&root).unwrap();
        let staged = storage.prepare_staging_root().unwrap();
        let new_owner = Keys::generate();
        let (_new_identity, new_event) = owned_identity(&staged, &new_owner, 200);
        let revocation = sign_template(
            &old_identity
                .owner_statement_template(OwnerStatementStatus::Revoked, 101)
                .unwrap(),
            &old_owner,
        );
        Self {
            _temporary: temporary,
            root,
            storage,
            old_event,
            new_event,
            revocation,
        }
    }

    fn preparing(&self) -> IdentitySwitchJournal {
        IdentitySwitchJournal::new(
            JournalPhase::Preparing,
            DataDisposition::Transfer,
            ReplacementSigner::Local,
            self.revocation.clone(),
            true,
        )
        .unwrap()
    }

    fn commit(&self) -> IdentitySwitchJournal {
        self.preparing().committing()
    }

    fn write_commit(&self) {
        self.storage.write_preparing(&self.preparing()).unwrap();
        self.storage.write_commit(&self.commit()).unwrap();
    }

    fn assert_complete_directories(&self) {
        for path in [
            self.root.join("identity"),
            self.root.join("identity-switch/next-root/identity"),
        ] {
            assert!(path.is_dir(), "{} is absent", path.display());
            assert!(path.join("device.key").is_file());
            assert!(path.join("owner.event.json").is_file());
        }
    }
}

#[test]
fn journal_is_strict_bounded_private_and_verifies_the_revoked_event() {
    let fixture = Fixture::new();
    let journal = fixture.preparing();
    fixture.storage.write_preparing(&journal).unwrap();
    assert_eq!(fixture.storage.load_journal().unwrap(), Some(journal));
    let metadata = fs::metadata(fixture.root.join("identity-switch/journal.json")).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o7777, 0o600);
    assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
    assert_eq!(
        fs::metadata(fixture.root.join("identity-switch"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o700
    );

    let cases = [
        serde_json::json!({
            "phase":"preparing",
            "dataDisposition":"transfer",
            "replacementSigner":"local",
            "oldOwnerRevocation":fixture.revocation,
            "oldOwnerWasPublished":true,
            "extra":false
        }),
        serde_json::json!({
            "phase":"unknown",
            "dataDisposition":"transfer",
            "replacementSigner":"local",
            "oldOwnerRevocation":fixture.revocation,
            "oldOwnerWasPublished":true
        }),
        serde_json::json!({
            "phase":"preparing",
            "dataDisposition":"keep",
            "replacementSigner":"local",
            "oldOwnerRevocation":fixture.revocation,
            "oldOwnerWasPublished":true
        }),
        serde_json::json!({
            "phase":"preparing",
            "dataDisposition":"transfer",
            "replacementSigner":"device",
            "oldOwnerRevocation":fixture.revocation,
            "oldOwnerWasPublished":true
        }),
    ];
    for value in cases {
        replace_journal(&fixture.root, serde_json::to_vec(&value).unwrap());
        assert!(fixture.storage.load_journal().is_err());
    }

    replace_journal(&fixture.root, b"{not-json".to_vec());
    assert!(fixture.storage.load_journal().is_err());
    replace_journal(&fixture.root, vec![b' '; 192 * 1024 + 1]);
    assert!(fixture.storage.load_journal().is_err());

    let owned = fixture.old_event.clone();
    let invalid = IdentitySwitchJournal::new(
        JournalPhase::Preparing,
        DataDisposition::Delete,
        ReplacementSigner::Nip46,
        owned,
        false,
    );
    assert!(invalid.is_err());
}

#[test]
fn journal_crash_boundaries_preserve_the_last_durable_phase() {
    let fixture = Fixture::new();
    let preparing = fixture.preparing();
    assert_eq!(
        fixture
            .storage
            .test_write_preparing(&preparing, WriteFault::AfterTemporarySync),
        Err(IdentitySwitchStorageError::InjectedFault)
    );
    assert_eq!(fixture.storage.load_journal().unwrap(), None);
    assert!(fs::read_dir(fixture.root.join("identity-switch"))
        .unwrap()
        .flatten()
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with(".journal.json.")));

    fixture.storage.write_preparing(&preparing).unwrap();
    assert_eq!(
        fixture.storage.load_journal().unwrap(),
        Some(preparing.clone())
    );
    assert_eq!(
        fixture
            .storage
            .test_write_commit(&preparing.committing(), WriteFault::AfterTemporarySync),
        Err(IdentitySwitchStorageError::InjectedFault)
    );
    assert_eq!(
        fixture.storage.load_journal().unwrap(),
        Some(preparing.clone())
    );

    assert_eq!(
        fixture
            .storage
            .test_write_commit(&preparing.committing(), WriteFault::AfterRename),
        Err(IdentitySwitchStorageError::InjectedFault)
    );
    assert_eq!(
        fixture.storage.load_journal().unwrap().unwrap().phase(),
        JournalPhase::Commit
    );
    fixture.assert_complete_directories();
}

#[test]
fn exchange_is_atomic_and_every_crash_boundary_has_two_complete_identities() {
    for fault in [
        ExchangeFault::AfterExchange,
        ExchangeFault::AfterLiveParentSync,
        ExchangeFault::AfterStagedParentSync,
    ] {
        let fixture = Fixture::new();
        fixture.write_commit();
        assert_eq!(
            fixture
                .storage
                .test_exchange(&fixture.old_event, &fixture.new_event, fault),
            Err(IdentitySwitchStorageError::InjectedFault)
        );
        fixture.assert_complete_directories();
        assert_eq!(
            fixture
                .storage
                .classify_exchange(&fixture.old_event, &fixture.new_event)
                .unwrap(),
            ExchangePosition::After
        );
        assert_eq!(
            fixture
                .storage
                .exchange_identity_directories(&fixture.old_event, &fixture.new_event)
                .unwrap(),
            ExchangePosition::After
        );
    }
}

#[test]
fn commit_before_exchange_restarts_from_the_complete_before_state() {
    let fixture = Fixture::new();
    fixture.write_commit();
    assert_eq!(
        fixture
            .storage
            .classify_exchange(&fixture.old_event, &fixture.new_event)
            .unwrap(),
        ExchangePosition::Before
    );
    fixture.assert_complete_directories();
    assert_eq!(
        fixture
            .storage
            .exchange_identity_directories(&fixture.old_event, &fixture.new_event)
            .unwrap(),
        ExchangePosition::After
    );
    fixture.assert_complete_directories();
}

#[test]
fn unsupported_exchange_refuses_without_changing_live_identity() {
    let fixture = Fixture::new();
    let storage = IdentitySwitchStorage::test_with_exchange(
        fixture.root.clone(),
        unsafe { libc::geteuid() },
        Arc::new(UnsupportedExchange),
    )
    .unwrap();
    fixture.write_commit();
    assert_eq!(
        storage.exchange_identity_directories(&fixture.old_event, &fixture.new_event),
        Err(IdentitySwitchStorageError::ExchangeUnsupported)
    );
    assert_eq!(
        storage
            .classify_exchange(&fixture.old_event, &fixture.new_event)
            .unwrap(),
        ExchangePosition::Before
    );
    fixture.assert_complete_directories();
}

#[test]
fn inspection_uses_exact_signed_evidence_and_rejects_mixed_or_unknown_state() {
    let fixture = Fixture::new();
    assert!(fixture
        .storage
        .inspect_live_identity(&fixture.old_event)
        .is_ok());
    assert!(fixture
        .storage
        .inspect_staged_identity(&fixture.new_event)
        .is_ok());
    assert!(matches!(
        fixture.storage.inspect_live_identity(&fixture.new_event),
        Err(IdentitySwitchStorageError::EvidenceMismatch)
    ));

    fs::copy(
        fixture
            .root
            .join("identity-switch/next-root/identity/owner.event.json"),
        fixture.root.join("identity/owner.event.json"),
    )
    .unwrap();
    mode(&fixture.root.join("identity/owner.event.json"), 0o600);
    assert!(matches!(
        fixture
            .storage
            .classify_exchange(&fixture.old_event, &fixture.new_event),
        Err(IdentitySwitchStorageError::EvidenceMismatch)
    ));
    fixture.assert_complete_directories();
}

#[test]
fn no_journal_cleanup_and_preparing_abort_remove_only_safe_staging() {
    let fixture = Fixture::new();
    assert!(fixture.storage.cleanup_abandoned_without_journal().unwrap());
    assert!(!fixture.storage.staged_private_root().exists());
    assert!(fixture.root.join("identity").is_dir());

    let fixture = Fixture::new();
    fixture
        .storage
        .write_preparing(&fixture.preparing())
        .unwrap();
    assert!(!fixture.storage.cleanup_abandoned_without_journal().unwrap());
    fixture.storage.abort_preparing().unwrap();
    assert!(!fixture.storage.staged_private_root().exists());
    assert!(fixture.storage.load_journal().unwrap().is_none());

    let fixture = Fixture::new();
    fixture.write_commit();
    assert_eq!(
        fixture.storage.abort_preparing(),
        Err(IdentitySwitchStorageError::WrongPhase)
    );
    assert!(fixture.storage.staged_private_root().exists());
    assert!(fixture.storage.load_journal().unwrap().is_some());
}

#[test]
fn unsafe_modes_links_special_files_and_owner_expectations_fail_closed() {
    let temporary = tempfile::tempdir().unwrap();
    mode(temporary.path(), 0o755);
    assert!(matches!(
        IdentitySwitchStorage::open(temporary.path()),
        Err(IdentitySwitchStorageError::UnsafePath(_))
    ));

    mode(temporary.path(), 0o700);
    let transaction_target = temporary.path().join("elsewhere");
    fs::create_dir(&transaction_target).unwrap();
    symlink(
        &transaction_target,
        temporary.path().join("identity-switch"),
    )
    .unwrap();
    assert!(matches!(
        IdentitySwitchStorage::open(temporary.path()),
        Err(IdentitySwitchStorageError::UnsafePath(_))
    ));

    let fixture = Fixture::new();
    fixture
        .storage
        .write_preparing(&fixture.preparing())
        .unwrap();
    mode(&fixture.root.join("identity-switch/journal.json"), 0o644);
    assert!(matches!(
        fixture.storage.load_journal(),
        Err(IdentitySwitchStorageError::UnsafePath(_))
    ));

    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("identity-switch/journal.json")).ok();
    symlink(
        fixture.root.join("identity/device.key"),
        fixture.root.join("identity-switch/journal.json"),
    )
    .unwrap();
    assert!(matches!(
        fixture.storage.load_journal(),
        Err(IdentitySwitchStorageError::UnsafePath(_))
    ));

    let fixture = Fixture::new();
    assert!(matches!(
        IdentitySwitchStorage::test_with_exchange(
            fixture.root.clone(),
            unsafe { libc::geteuid() }.wrapping_add(1),
            Arc::new(UnsupportedExchange)
        ),
        Err(IdentitySwitchStorageError::UnsafePath(_))
    ));

    let fixture = Fixture::new();
    let unsafe_link = fixture
        .root
        .join("identity-switch/next-root/identity/unsafe-link");
    symlink("/tmp", &unsafe_link).unwrap();
    assert!(matches!(
        fixture.storage.cleanup_abandoned_without_journal(),
        Err(IdentitySwitchStorageError::UnsafePath(_))
    ));
    assert!(fixture.storage.staged_private_root().exists());

    let fixture = Fixture::new();
    let fifo = fixture
        .root
        .join("identity-switch/next-root/identity/unsafe-fifo");
    let fifo_name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
    assert!(matches!(
        fixture.storage.cleanup_abandoned_without_journal(),
        Err(IdentitySwitchStorageError::UnsafePath(_))
    ));
}

#[test]
fn commit_cannot_change_prepared_choices_or_run_from_preparing() {
    let fixture = Fixture::new();
    let preparing = fixture.preparing();
    fixture.storage.write_preparing(&preparing).unwrap();
    let changed = IdentitySwitchJournal::new(
        JournalPhase::Commit,
        DataDisposition::Delete,
        ReplacementSigner::Local,
        fixture.revocation.clone(),
        true,
    )
    .unwrap();
    assert_eq!(
        fixture.storage.write_commit(&changed),
        Err(IdentitySwitchStorageError::JournalChanged)
    );
    assert_eq!(
        fixture
            .storage
            .exchange_identity_directories(&fixture.old_event, &fixture.new_event),
        Err(IdentitySwitchStorageError::WrongPhase)
    );
}

struct UnsupportedExchange;

impl RenameExchange for UnsupportedExchange {
    fn exchange(&self, _left: &Path, _right: &Path) -> Result<(), IdentitySwitchStorageError> {
        Err(IdentitySwitchStorageError::ExchangeUnsupported)
    }
}

fn owned_identity(root: &Path, owner: &Keys, created_at: u64) -> (DeviceIdentity, String) {
    let mut identity = DeviceIdentity::load_or_create(root).unwrap();
    let template = identity
        .owner_statement_template(OwnerStatementStatus::Owned, created_at)
        .unwrap();
    let event = sign_template(&template, owner);
    identity
        .apply_signed_owner_binding(&template, &owner.public_key().to_hex(), &event)
        .unwrap();
    (identity, event)
}

fn sign_template(template: &str, owner: &Keys) -> String {
    let value: serde_json::Value = serde_json::from_str(template).unwrap();
    let tags: Vec<Vec<String>> = serde_json::from_value(value["tags"].clone()).unwrap();
    EventBuilder::new(
        Kind::Custom(value["kind"].as_u64().unwrap() as u16),
        value["content"].as_str().unwrap(),
    )
    .tags(
        tags.into_iter()
            .map(|tag| Tag::parse(tag).unwrap())
            .collect::<Vec<_>>(),
    )
    .custom_created_at(Timestamp::from(value["created_at"].as_u64().unwrap()))
    .finalize(owner)
    .unwrap()
    .as_json()
}

fn replace_journal(root: &Path, bytes: Vec<u8>) {
    let path = root.join("identity-switch/journal.json");
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    file.write_all(&bytes).unwrap();
    file.sync_all().unwrap();
}

fn mode(path: &Path, value: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(value)).unwrap();
}
