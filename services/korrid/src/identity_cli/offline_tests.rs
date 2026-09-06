//! Public CLI contract tests. All keys and files here are disposable fixtures.
use super::run;
use crate::identity::{DeviceIdentity, IdentityState, OwnerStatementStatus};
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::{Keys, SecretKey},
    types::Timestamp,
};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{symlink, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

#[path = "offline_tests/interruption.rs"]
mod interruption;

const TEST_OWNER: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";

fn keys(number: u8) -> Keys {
    Keys::new(SecretKey::from_hex(&format!("{number:064x}")).unwrap())
}

fn write_private(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
}

fn sign(template: &str, owner: u8) -> String {
    let template: serde_json::Value = serde_json::from_str(template).unwrap();
    EventBuilder::new(
        Kind::Custom(template["kind"].as_u64().unwrap() as u16),
        template["content"].as_str().unwrap(),
    )
    .tags(template["tags"].as_array().unwrap().iter().map(|tag| {
        Tag::parse(serde_json::from_value::<Vec<String>>(tag.clone()).unwrap()).unwrap()
    }))
    .custom_created_at(Timestamp::from(template["created_at"].as_u64().unwrap()))
    .finalize(&keys(owner))
    .unwrap()
    .as_json()
}

struct Fixture {
    root: tempfile::TempDir,
    args: Vec<OsString>,
    old: Vec<u8>,
    new: Vec<u8>,
    key: Vec<u8>,
    key_metadata: FileMetadata,
    evidence_metadata: FileMetadata,
}

#[derive(Debug, PartialEq, Eq)]
struct FileMetadata {
    dev: u64,
    ino: u64,
    uid: u32,
    gid: u32,
    mode: u32,
    links: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

fn metadata(path: &Path) -> FileMetadata {
    let m = fs::symlink_metadata(path).unwrap();
    // Reading may update atime. All other file identity/content metadata must stay.
    FileMetadata {
        dev: m.dev(),
        ino: m.ino(),
        uid: m.uid(),
        gid: m.gid(),
        mode: m.mode(),
        links: m.nlink(),
        size: m.len(),
        modified: (m.mtime(), m.mtime_nsec()),
        changed: (m.ctime(), m.ctime_nsec()),
    }
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let directory = root.path().join("identity");
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        write_private(
            &directory.join("device.key"),
            format!("{:064x}\n", 5).as_bytes(),
        );
        let mut identity = DeviceIdentity::load_or_create(root.path()).unwrap();
        let old_template = identity
            .owner_statement_template(OwnerStatementStatus::Owned, 100)
            .unwrap();
        let old = sign(&old_template, 3);
        identity.apply_owner_statement(&old).unwrap();
        let template = identity
            .owner_statement_template(OwnerStatementStatus::Owned, 200)
            .unwrap();
        let new = sign(&template, 4);
        write_private(&root.path().join("template.json"), template.as_bytes());
        write_private(&root.path().join("signed.json"), new.as_bytes());
        write_private(
            &directory.join("unrelated-evidence"),
            b"keep recovery and revocation evidence",
        );
        let old_id = DeviceIdentity::verify_event(&old).unwrap().id;
        let args = vec![
            "replace-test-owner-offline".into(),
            "--expected-device".into(),
            identity.device_public_key().unwrap().into(),
            "--expected-owner".into(),
            TEST_OWNER.into(),
            "--expected-event".into(),
            old_id.into(),
            "--new-owner".into(),
            keys(4).public_key().to_hex().into(),
            "--template".into(),
            root.path().join("template.json").into_os_string(),
            "--file".into(),
            root.path().join("signed.json").into_os_string(),
        ];
        let key_path = directory.join("device.key");
        Self {
            args,
            old: fs::read(directory.join("owner.event.json")).unwrap(),
            new: new.into_bytes(),
            key: fs::read(&key_path).unwrap(),
            key_metadata: metadata(&key_path),
            evidence_metadata: metadata(&directory.join("unrelated-evidence")),
            root,
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    fn run(&self) -> Result<String, String> {
        run(&self.args, self.root.path(), 300, &mut Cursor::new([]))
    }

    fn preserves_key_and_evidence(&self) {
        assert!(fs::read(self.path("identity/device.key")).unwrap() == self.key);
        assert_eq!(
            metadata(&self.path("identity/device.key")),
            self.key_metadata
        );
        assert_eq!(
            fs::read(self.path("identity/unrelated-evidence")).unwrap(),
            b"keep recovery and revocation evidence"
        );
        assert_eq!(
            metadata(&self.path("identity/unrelated-evidence")),
            self.evidence_metadata
        );
    }

    fn rejects(&self) {
        let error = self.run().expect_err("unsafe replacement must fail");
        assert!(
            !error.starts_with("usage:"),
            "command must recognize the guarded contract"
        );
        assert!(!error.contains("sig\""));
        assert!(!error.contains(std::str::from_utf8(&self.key).unwrap().trim()));
        assert!(fs::read(self.path("identity/owner.event.json")).unwrap() == self.old);
    }
}

#[test]
fn offline_replacement_preserves_key_and_reopens_new_owner_without_enabling_import_transfer() {
    let fixture = Fixture::new();
    let old_owner_metadata = metadata(&fixture.path("identity/owner.event.json"));
    let mut ordinary = DeviceIdentity::load_or_create(fixture.root.path()).unwrap();
    assert!(ordinary
        .apply_owner_statement(std::str::from_utf8(&fixture.new).unwrap())
        .is_err());
    assert!(ordinary
        .apply_signed_owner_binding(
            &fs::read_to_string(fixture.path("template.json")).unwrap(),
            &keys(4).public_key().to_hex(),
            std::str::from_utf8(&fixture.new).unwrap(),
        )
        .is_err());
    assert!(run(
        &["import".into(), "--stdin".into()],
        fixture.root.path(),
        300,
        &mut Cursor::new(&fixture.new)
    )
    .is_err());

    let output = fixture
        .run()
        .expect("approved offline replacement must succeed");
    assert!(!output.contains("sig\""));
    assert!(!output.contains("secret"));
    let reopened = DeviceIdentity::load_or_create(fixture.root.path()).unwrap();
    assert!(
        matches!(reopened.state(), IdentityState::Owned { owner_public_key, .. } if owner_public_key == &keys(4).public_key().to_hex())
    );
    assert!(fs::read(fixture.path("identity/owner.event.json")).unwrap() == fixture.new);
    let new_owner_metadata = metadata(&fixture.path("identity/owner.event.json"));
    assert_eq!(
        (
            new_owner_metadata.uid,
            new_owner_metadata.gid,
            new_owner_metadata.mode
        ),
        (
            old_owner_metadata.uid,
            old_owner_metadata.gid,
            old_owner_metadata.mode
        )
    );
    fixture.preserves_key_and_evidence();
    assert!(
        fixture.run().is_err(),
        "stale expected-current evidence must not be idempotent"
    );
    assert!(
        run(
            &["import".into(), "--stdin".into()],
            fixture.root.path(),
            300,
            &mut Cursor::new(&fixture.old)
        )
        .is_err(),
        "old configured owner import must fail after the cut"
    );
}

#[test]
fn offline_replacement_requires_every_exact_expected_current_and_selected_owner_value() {
    for index in [2, 4, 6, 8] {
        let mut fixture = Fixture::new();
        fixture.args[index] = if index == 6 {
            "00".repeat(32).into()
        } else {
            keys(6).public_key().to_hex().into()
        };
        fixture.rejects();
        fixture.preserves_key_and_evidence();
    }
    let mut fixture = Fixture::new();
    fixture.args[8] = TEST_OWNER.into();
    fs::write(
        fixture.path("signed.json"),
        sign(
            &fs::read_to_string(fixture.path("template.json")).unwrap(),
            3,
        ),
    )
    .unwrap();
    fixture.rejects();
}

#[test]
fn offline_replacement_is_only_from_the_known_test_owner_and_never_from_revoked_state() {
    for (owner, status) in [
        (4, OwnerStatementStatus::Owned),
        (3, OwnerStatementStatus::Revoked),
    ] {
        let mut fixture = Fixture::new();
        let identity = DeviceIdentity::load_or_create(fixture.root.path()).unwrap();
        let event = sign(
            &identity.owner_statement_template(status, 101).unwrap(),
            owner,
        );
        fixture.old = event.into_bytes();
        fs::write(fixture.path("identity/owner.event.json"), &fixture.old).unwrap();
        fixture.args[4] = keys(owner).public_key().to_hex().into();
        fixture.args[6] = DeviceIdentity::verify_event(std::str::from_utf8(&fixture.old).unwrap())
            .unwrap()
            .id
            .into();
        fixture.rejects();
    }
}

#[test]
fn offline_replacement_rejects_bad_signatures_and_every_template_mismatch() {
    for change in [
        "signature",
        "device",
        "time",
        "kind",
        "content",
        "status",
        "extra-template-field",
        "malformed",
        "oversized",
    ] {
        let fixture = Fixture::new();
        let mut template: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.path("template.json")).unwrap()).unwrap();
        let mut event: serde_json::Value = serde_json::from_slice(&fixture.new).unwrap();
        match change {
            "signature" => {
                event["sig"] = "00".repeat(64).into();
                fs::write(fixture.path("signed.json"), event.to_string()).unwrap();
            }
            "device" => {
                let device = keys(6).public_key().to_hex();
                template["tags"][0][1] = format!("org.korri.device-owner:{device}").into();
                template["tags"][1][1] = device.into();
                fs::write(fixture.path("signed.json"), sign(&template.to_string(), 4)).unwrap();
            }
            "time" => {
                template["created_at"] = 201.into();
            }
            "kind" => {
                template["kind"] = 1.into();
            }
            "content" => {
                template["content"] = "changed".into();
            }
            "status" => {
                template["tags"][2][1] = "revoked".into();
            }
            "extra-template-field" => {
                template["extra"] = true.into();
            }
            "malformed" => {
                fs::write(fixture.path("signed.json"), b"{").unwrap();
            }
            "oversized" => {
                fs::write(fixture.path("signed.json"), vec![b' '; 65537]).unwrap();
            }
            _ => unreachable!(),
        }
        fs::write(fixture.path("template.json"), template.to_string()).unwrap();
        fixture.rejects();
        fixture.preserves_key_and_evidence();
    }
}

#[test]
fn offline_replacement_never_creates_a_missing_key_owner_file_or_root() {
    for relative in ["identity/device.key", "identity/owner.event.json"] {
        let fixture = Fixture::new();
        fs::remove_file(fixture.path(relative)).unwrap();
        assert!(fixture.run().is_err());
        assert!(!fixture.path(relative).exists());
    }
    let fixture = Fixture::new();
    let missing = fixture.path("missing");
    assert!(run(&fixture.args, &missing, 300, &mut Cursor::new([])).is_err());
    assert!(!missing.exists());
}

#[test]
fn offline_replacement_rejects_unsafe_files_and_private_parents() {
    for relative in [
        "identity/device.key",
        "identity/owner.event.json",
        "template.json",
        "signed.json",
    ] {
        for unsafe_kind in [
            "symlink",
            "hardlink",
            "public",
            "directory",
            "fifo",
            "oversized",
            "invalid",
        ] {
            let fixture = Fixture::new();
            let path = fixture.path(relative);
            match unsafe_kind {
                "symlink" => {
                    let target = fixture.path("target");
                    fs::rename(&path, &target).unwrap();
                    symlink(target, &path).unwrap();
                }
                "hardlink" => {
                    fs::hard_link(&path, fixture.path("second-link")).unwrap();
                }
                "public" => {
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
                }
                "directory" => {
                    fs::remove_file(&path).unwrap();
                    fs::create_dir(&path).unwrap();
                }
                "fifo" => {
                    use std::os::unix::ffi::OsStrExt;
                    fs::remove_file(&path).unwrap();
                    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
                    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
                }
                "oversized" => {
                    fs::write(&path, vec![b' '; 65537]).unwrap();
                }
                "invalid" => {
                    fs::write(&path, b"not valid input").unwrap();
                }
                _ => unreachable!(),
            }
            let error = fixture.run().unwrap_err();
            assert!(!error.starts_with("usage:"));
            if relative != "identity/owner.event.json" {
                assert!(
                    fs::read(fixture.path("identity/owner.event.json")).unwrap() == fixture.old
                );
            }
        }
    }
    for relative in ["", "identity"] {
        let fixture = Fixture::new();
        fs::set_permissions(fixture.path(relative), fs::Permissions::from_mode(0o755)).unwrap();
        fixture.rejects();
    }
    let fixture = Fixture::new();
    let linked = fixture.path("linked-root");
    symlink(fixture.root.path(), &linked).unwrap();
    assert!(run(&fixture.args, &linked, 300, &mut Cursor::new([])).is_err());
}

#[test]
fn offline_replacement_rejects_source_parent_links_and_nonprivate_source_parents() {
    for unsafe_kind in ["symlink", "public", "writable-ancestor"] {
        let mut fixture = Fixture::new();
        let parent = fixture.path("source-parent");
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        let source = parent.join("signed.json");
        write_private(&source, &fixture.new);
        fixture.args[12] = source.into_os_string();
        match unsafe_kind {
            "symlink" => {
                let alias = fixture.path("source-link");
                symlink(&parent, &alias).unwrap();
                fixture.args[12] = alias.join("signed.json").into_os_string();
            }
            "public" => {
                fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
            }
            "writable-ancestor" => {
                let nested = parent.join("nested");
                fs::create_dir(&nested).unwrap();
                fs::set_permissions(&nested, fs::Permissions::from_mode(0o700)).unwrap();
                write_private(&nested.join("signed.json"), &fixture.new);
                fs::set_permissions(&parent, fs::Permissions::from_mode(0o777)).unwrap();
                fixture.args[12] = nested.join("signed.json").into_os_string();
            }
            _ => unreachable!(),
        }
        fixture.rejects();
    }
}

#[test]
fn offline_replacement_readers_observe_only_complete_old_or_new_events() {
    let fixture = Fixture::new();
    let mut old_descriptor = File::open(fixture.path("identity/owner.event.json")).unwrap();
    let finished = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            let mut reads = 0;
            while !finished.load(Ordering::Acquire) || reads < 100 {
                let bytes = fs::read(fixture.path("identity/owner.event.json")).unwrap();
                assert!(
                    bytes == fixture.old || bytes == fixture.new,
                    "no absent or partial owner event is allowed"
                );
                reads += 1;
            }
        });
        let result = fixture.run();
        finished.store(true, Ordering::Release);
        reader.join().unwrap();
        result.unwrap();
    });
    let mut bytes = vec![];
    old_descriptor.read_to_end(&mut bytes).unwrap();
    assert!(
        bytes == fixture.old,
        "replacement must not truncate the existing inode"
    );
    fixture.preserves_key_and_evidence();
}

#[test]
fn offline_replacement_refuses_another_offline_writer_lock() {
    let fixture = Fixture::new();
    let directory = File::open(fixture.path("identity")).unwrap();
    // A real advisory lock coordinates administrative writers, not live daemons.
    assert_eq!(
        unsafe { libc::flock(directory.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    fixture.rejects();
    fixture.preserves_key_and_evidence();
}

#[test]
fn offline_replacement_staging_write_failure_preserves_old_event() {
    let fixture = Fixture::new();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "identity_cli::offline_tests::offline_write_failure_child",
            "--ignored",
        ])
        .env("KORRI_OFFLINE_TEST_ROOT", fixture.root.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "write-failure child failed");
    assert!(fs::read(fixture.path("identity/owner.event.json")).unwrap() == fixture.old);
    fixture.preserves_key_and_evidence();
    assert_eq!(fs::read_dir(fixture.path("identity")).unwrap().count(), 3);
}

#[test]
#[ignore = "subprocess helper; invoked by the write-failure contract test"]
fn offline_write_failure_child() {
    let root = PathBuf::from(std::env::var_os("KORRI_OFFLINE_TEST_ROOT").unwrap());
    let old = fs::read_to_string(root.join("identity/owner.event.json")).unwrap();
    let verified = DeviceIdentity::derive_owner_statement(&old).unwrap();
    let args = vec![
        "replace-test-owner-offline".into(),
        "--expected-device".into(),
        verified.device_public_key.into(),
        "--expected-owner".into(),
        verified.owner_public_key.into(),
        "--expected-event".into(),
        verified.event_id.into(),
        "--new-owner".into(),
        keys(4).public_key().to_hex().into(),
        "--template".into(),
        root.join("template.json").into_os_string(),
        "--file".into(),
        root.join("signed.json").into_os_string(),
    ];
    // Only this disposable test process gets EFBIG for all writes.
    unsafe {
        libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
        assert_eq!(
            libc::setrlimit(
                libc::RLIMIT_FSIZE,
                &libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0
                }
            ),
            0
        );
    }
    let error = run(&args, &root, 300, &mut Cursor::new([])).unwrap_err();
    assert!(!error.starts_with("usage:"));
}
