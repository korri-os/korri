//! Deterministic subprocess interruption at the real offline writer's boundaries.
//! All environment controls and descriptor faults exist only in test builds.
use super::{keys, metadata, run, sign, FileMetadata, Fixture};
use crate::identity::{
    offline::checkpoints::{self, Checkpoint},
    DeviceIdentity, IdentityState,
};
use std::{
    fs::{self, File, OpenOptions},
    io::Cursor,
    os::{
        fd::AsRawFd,
        unix::{fs::OpenOptionsExt, process::ExitStatusExt},
    },
    path::PathBuf,
    process::{Child, Command, ExitStatus},
    time::{Duration, Instant},
};

const STORAGE_ERROR: &str = "offline owner replacement failed: identity storage is unavailable; keep all authority stopped and inspect the current binding before recovery; never restore the test owner";

#[derive(Debug)]
enum ChildState {
    Stopped(i32),
    Exited(ExitStatus),
}

#[derive(PartialEq, Eq)]
enum ChildLifecycle {
    Running,
    Stopped,
    Reaped,
}

struct OfflineChild {
    child: Child,
    root: PathBuf,
    lifecycle: ChildLifecycle,
}

impl OfflineChild {
    fn start(fixture: &Fixture, case: &str) -> Self {
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(fixture.path("child-output"))
            .unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "identity_cli::offline_tests::interruption::offline_checkpoint_child",
                "--ignored",
                "--nocapture",
            ])
            .env("KORRI_OFFLINE_TEST_ROOT", fixture.root.path())
            .env("KORRI_OFFLINE_TEST_CASE", case)
            .stdout(output.try_clone().unwrap())
            .stderr(output)
            .spawn()
            .unwrap();
        Self {
            child,
            root: fixture.root.path().to_owned(),
            lifecycle: ChildLifecycle::Running,
        }
    }

    fn diagnostic(&self, message: impl std::fmt::Display) -> String {
        format!(
            "{message}\n{}",
            fs::read_to_string(self.root.join("child-output")).unwrap()
        )
    }

    fn wait(&mut self) -> Result<ChildState, String> {
        assert!(
            self.lifecycle != ChildLifecycle::Reaped,
            "child status must be consumed exactly once"
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut status = 0;
            let result = unsafe {
                libc::waitpid(
                    self.child.id() as libc::pid_t,
                    &mut status,
                    libc::WUNTRACED | libc::WNOHANG,
                )
            };
            if result < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(self.diagnostic(error));
            }
            if result > 0 {
                if libc::WIFSTOPPED(status) {
                    self.lifecycle = ChildLifecycle::Stopped;
                    return Ok(ChildState::Stopped(libc::WSTOPSIG(status)));
                }
                self.lifecycle = ChildLifecycle::Reaped;
                return Ok(ChildState::Exited(ExitStatus::from_raw(status)));
            }
            if Instant::now() >= deadline {
                return Err(self.diagnostic("child did not stop or exit within 10 seconds"));
            }
            // Polling only bounds a failed test. Time never selects an interruption.
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn checkpoint(&mut self, expected: Checkpoint) -> Result<(), String> {
        match self.wait()? {
            ChildState::Stopped(libc::SIGSTOP) => {
                let observed = fs::read_to_string(self.root.join("checkpoint"))
                    .map_err(|error| self.diagnostic(error))?;
                if observed == format!("{expected:?}") {
                    Ok(())
                } else {
                    Err(self.diagnostic(format!("expected {expected:?}, observed {observed}")))
                }
            }
            ChildState::Exited(status) => {
                Err(self.diagnostic(format!("child exited before {expected:?}: {status}")))
            }
            other => Err(self.diagnostic(format!("expected {expected:?}, observed {other:?}"))),
        }
    }

    fn resume(&mut self) {
        assert!(self.lifecycle == ChildLifecycle::Stopped);
        assert_eq!(
            unsafe { libc::kill(self.child.id() as libc::pid_t, libc::SIGCONT) },
            0
        );
        self.lifecycle = ChildLifecycle::Running;
    }

    fn interrupt(&mut self) {
        assert!(self.lifecycle == ChildLifecycle::Stopped);
        self.child.kill().unwrap();
        match self.wait().unwrap() {
            ChildState::Exited(status) => assert_eq!(status.signal(), Some(libc::SIGKILL)),
            other => panic!("expected our SIGKILL, observed {other:?}"),
        }
    }

    fn finish(&mut self) -> Result<(), String> {
        match self.wait()? {
            ChildState::Exited(status) if status.success() => Ok(()),
            ChildState::Exited(status) => Err(self.diagnostic(format!("child failed: {status}"))),
            other => Err(self.diagnostic(format!("expected successful exit, observed {other:?}"))),
        }
    }
}

impl Drop for OfflineChild {
    fn drop(&mut self) {
        // Only failure cleanup ignores status. Assertions above own every result.
        if self.lifecycle != ChildLifecycle::Reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn staged_file(fixture: &Fixture) -> (PathBuf, FileMetadata) {
    let staged: Vec<_> = fs::read_dir(fixture.path("identity"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            let name = path.file_name().unwrap().to_str().unwrap();
            name.starts_with(".owner.event.json.") && name.ends_with(".tmp")
        })
        .collect();
    assert_eq!(
        staged.len(),
        1,
        "must observe exactly one actual staged file"
    );
    let path = staged.into_iter().next().unwrap();
    assert!(fs::read(&path).unwrap() == fixture.new);
    let staged = metadata(&path);
    let current = metadata(&fixture.path("identity/owner.event.json"));
    assert_eq!((staged.uid, staged.gid), (current.uid, current.gid));
    assert_eq!(staged.mode & 0o7777, 0o600);
    assert_eq!(staged.links, 1);
    (path, staged)
}

fn assert_binding(fixture: &Fixture, expected: &[u8]) {
    assert!(fs::read(fixture.path("identity/owner.event.json")).unwrap() == expected);
    let event =
        DeviceIdentity::derive_owner_statement(std::str::from_utf8(expected).unwrap()).unwrap();
    let reopened = DeviceIdentity::load_or_create(fixture.root.path()).unwrap();
    assert_eq!(
        reopened.state(),
        &IdentityState::Owned {
            device_public_key: event.device_public_key,
            owner_public_key: event.owner_public_key,
            event_id: event.event_id,
            created_at: event.created_at,
        }
    );
    fixture.preserves_key_and_evidence();
}

fn assert_renamed(fixture: &Fixture, staged: &FileMetadata) {
    assert_binding(fixture, &fixture.new);
    let current = metadata(&fixture.path("identity/owner.event.json"));
    assert_eq!((current.dev, current.ino), (staged.dev, staged.ino));
    assert_eq!(
        (current.uid, current.gid, current.mode),
        (staged.uid, staged.gid, staged.mode)
    );
    assert_eq!(fs::read_dir(fixture.path("identity")).unwrap().count(), 3);
}

#[test]
fn offline_replacement_interrupted_pre_rename_keeps_exact_old_binding() {
    for checkpoint in [Checkpoint::Staged, Checkpoint::BeforeRename] {
        let fixture = Fixture::new();
        let old_metadata = metadata(&fixture.path("identity/owner.event.json"));
        let mut child = OfflineChild::start(&fixture, "replace");
        child.checkpoint(Checkpoint::Staged).unwrap();
        let (staged_path, staged_metadata) = staged_file(&fixture);
        if checkpoint == Checkpoint::BeforeRename {
            child.resume();
            child.checkpoint(checkpoint).unwrap();
        }
        assert_binding(&fixture, &fixture.old);
        child.interrupt();
        assert_binding(&fixture, &fixture.old);
        assert_eq!(
            metadata(&fixture.path("identity/owner.event.json")),
            old_metadata
        );
        assert_eq!(metadata(&staged_path), staged_metadata);
        assert!(fs::read(staged_path).unwrap() == fixture.new);
    }
}

#[test]
fn offline_replacement_interrupted_post_rename_keeps_exact_new_binding() {
    let fixture = Fixture::new();
    let mut child = OfflineChild::start(&fixture, "replace");
    child.checkpoint(Checkpoint::Staged).unwrap();
    let (_, staged) = staged_file(&fixture);
    child.resume();
    child.checkpoint(Checkpoint::BeforeRename).unwrap();
    assert_binding(&fixture, &fixture.old);
    child.resume();
    child.checkpoint(Checkpoint::Renamed).unwrap();
    assert_renamed(&fixture, &staged);
    child.interrupt();
    assert_renamed(&fixture, &staged);
}

#[test]
fn offline_replacement_checkpoint_child_must_finish_successfully() {
    let fixture = Fixture::new();
    let mut child = OfflineChild::start(&fixture, "replace");
    child.checkpoint(Checkpoint::Staged).unwrap();
    let (_, staged) = staged_file(&fixture);
    child.resume();
    child.checkpoint(Checkpoint::BeforeRename).unwrap();
    child.resume();
    child.checkpoint(Checkpoint::Renamed).unwrap();
    child.resume();
    child.finish().unwrap();
    assert_renamed(&fixture, &staged);
}

#[test]
fn offline_replacement_final_recheck_refuses_changed_current_binding() {
    let fixture = Fixture::new();
    let mut child = OfflineChild::start(&fixture, "storage-error");
    child.checkpoint(Checkpoint::Staged).unwrap();
    let (staged_path, _) = staged_file(&fixture);
    let mut template: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.path("template.json")).unwrap()).unwrap();
    template["created_at"] = 150.into();
    let changed = sign(&template.to_string(), 3).into_bytes();
    assert!(changed != fixture.old && changed != fixture.new);
    // Real concurrent replacement evidence, introduced after the initial read.
    fs::write(fixture.path("identity/owner.event.json"), &changed).unwrap();
    let changed_metadata = metadata(&fixture.path("identity/owner.event.json"));
    child.resume();
    child.finish().unwrap();
    assert_binding(&fixture, &changed);
    assert_eq!(
        metadata(&fixture.path("identity/owner.event.json")),
        changed_metadata
    );
    assert!(!staged_path.exists());
    assert_eq!(fs::read_dir(fixture.path("identity")).unwrap().count(), 3);
}

#[test]
fn offline_replacement_failed_rename_preserves_exact_old_binding() {
    let fixture = Fixture::new();
    let old_metadata = metadata(&fixture.path("identity/owner.event.json"));
    let mut child = OfflineChild::start(&fixture, "storage-error");
    child.checkpoint(Checkpoint::Staged).unwrap();
    let (staged_path, _) = staged_file(&fixture);
    child.resume();
    child.checkpoint(Checkpoint::BeforeRename).unwrap();
    // The next real renameat must fail with ENOENT. No syscall is replaced.
    fs::remove_file(staged_path).unwrap();
    child.resume();
    child.finish().unwrap();
    assert_binding(&fixture, &fixture.old);
    assert_eq!(
        metadata(&fixture.path("identity/owner.event.json")),
        old_metadata
    );
    assert_eq!(fs::read_dir(fixture.path("identity")).unwrap().count(), 3);
}

#[test]
fn offline_replacement_failed_directory_sync_keeps_exact_new_binding_and_warning() {
    let fixture = Fixture::new();
    let mut child = OfflineChild::start(&fixture, "directory-sync-error");
    child.checkpoint(Checkpoint::Staged).unwrap();
    let (_, staged) = staged_file(&fixture);
    child.resume();
    child.checkpoint(Checkpoint::BeforeRename).unwrap();
    child.resume();
    child.checkpoint(Checkpoint::Renamed).unwrap();
    assert_renamed(&fixture, &staged);
    child.resume();
    child.finish().unwrap();
    assert_renamed(&fixture, &staged);
}

#[test]
fn offline_replacement_checkpoint_harness_rejects_panics_before_persistence_and_after_rename() {
    for case in ["panic-before-staging", "panic-after-rename"] {
        let fixture = Fixture::new();
        let mut child = OfflineChild::start(&fixture, case);
        let error = if case == "panic-before-staging" {
            let error = child.checkpoint(Checkpoint::Staged).unwrap_err();
            assert!(!fixture.path("checkpoint").exists());
            assert_binding(&fixture, &fixture.old);
            assert_eq!(fs::read_dir(fixture.path("identity")).unwrap().count(), 3);
            error
        } else {
            child.checkpoint(Checkpoint::Staged).unwrap();
            let (_, staged) = staged_file(&fixture);
            child.resume();
            child.checkpoint(Checkpoint::BeforeRename).unwrap();
            child.resume();
            child.checkpoint(Checkpoint::Renamed).unwrap();
            child.resume();
            let error = child.finish().unwrap_err();
            assert_renamed(&fixture, &staged);
            error
        };
        assert!(error.contains("exit status: 101"), "{error}");
        assert!(
            error.contains("deliberate checkpoint child panic"),
            "{error}"
        );
    }
}

fn stop_at_checkpoint(checkpoint: Checkpoint, directory: &File) {
    let root = PathBuf::from(std::env::var_os("KORRI_OFFLINE_TEST_ROOT").unwrap());
    fs::write(root.join("checkpoint"), format!("{checkpoint:?}")).unwrap();
    assert_eq!(unsafe { libc::raise(libc::SIGSTOP) }, 0);
    if checkpoint == Checkpoint::Renamed {
        match std::env::var("KORRI_OFFLINE_TEST_CASE").unwrap().as_str() {
            "directory-sync-error" => {
                // Keep the owned descriptor valid, but make its real fsync fail
                // with EINVAL. Only this disposable child loses its directory fd.
                let null = File::open("/dev/null").unwrap();
                assert_eq!(
                    unsafe { libc::dup2(null.as_raw_fd(), directory.as_raw_fd()) },
                    directory.as_raw_fd()
                );
            }
            "panic-after-rename" => panic!("deliberate checkpoint child panic"),
            _ => {}
        }
    }
}

#[test]
#[ignore = "subprocess helper; requires a disposable fixture and parent checkpoint control"]
fn offline_checkpoint_child() {
    let root = PathBuf::from(std::env::var_os("KORRI_OFFLINE_TEST_ROOT").unwrap());
    let case = std::env::var("KORRI_OFFLINE_TEST_CASE").unwrap();
    if case == "panic-before-staging" {
        panic!("deliberate checkpoint child panic");
    }
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
    checkpoints::install(stop_at_checkpoint);
    let result = run(&args, &root, 300, &mut Cursor::new([]));
    match case.as_str() {
        "replace" => {
            let output: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
            assert_eq!(output["_tag"], "Owned");
            assert_eq!(output["ownerPublicKey"], keys(4).public_key().to_hex());
        }
        "storage-error" | "directory-sync-error" => {
            assert_eq!(result.unwrap_err(), STORAGE_ERROR);
        }
        other => panic!("unexpected child case/completion: {other}"),
    }
}
