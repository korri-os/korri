//! Linux ACL proof without root or changing this process's uid.
use korrid::saves::{record::snapshot_id, store::NativeSaveStore, types::*, SaveRepository};
use std::{
    fs,
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::Path,
};

const PERSON: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";

fn shared_acl(path: &Path, first: u32, second: u32) {
    let mut bytes = 2u32.to_le_bytes().to_vec();
    for (tag, permission, id) in [
        (1u16, 7u16, u32::MAX),
        (2, 7, first),
        (2, 7, second),
        (4, 0, u32::MAX),
        (16, 7, u32::MAX),
        (32, 0, u32::MAX),
    ] {
        bytes.extend(tag.to_le_bytes());
        bytes.extend(permission.to_le_bytes());
        bytes.extend(id.to_le_bytes());
    }
    let file = fs::File::open(path).unwrap();
    for name in [c"system.posix_acl_access", c"system.posix_acl_default"] {
        assert_eq!(
            unsafe {
                libc::fsetxattr(
                    file.as_raw_fd(),
                    name.as_ptr(),
                    bytes.as_ptr().cast(),
                    bytes.len(),
                    0,
                )
            },
            0,
            "{}",
            std::io::Error::last_os_error()
        );
    }
}

fn named_access(path: &Path, uid: u32) -> u16 {
    let file = fs::File::open(path).unwrap();
    let mut bytes = [0u8; 256];
    let len = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            c"system.posix_acl_access".as_ptr(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
        )
    };
    assert!(len >= 4, "{}", std::io::Error::last_os_error());
    let mut named = None;
    let mut mask = None;
    for entry in bytes[4..len as usize].chunks_exact(8) {
        let tag = u16::from_le_bytes(entry[0..2].try_into().unwrap());
        let permissions = u16::from_le_bytes(entry[2..4].try_into().unwrap());
        let id = u32::from_le_bytes(entry[4..8].try_into().unwrap());
        if tag == 2 && id == uid {
            named = Some(permissions);
        }
        if tag == 16 {
            mask = Some(permissions);
        }
        if tag == 4 || tag == 32 {
            assert_eq!(permissions, 0, "owning group and others must stay denied");
        }
    }
    named.expect("inherited named user") & mask.expect("ACL mask")
}

fn snapshot() -> SnapshotRecord {
    SnapshotRecord {
        parents: vec![],
        game_id: "game".into(),
        release_id: None,
        runner_id: "runner".into(),
        device: PERSON.into(),
        ended_at: "2026-10-21T00:00:00.000Z".into(),
        duration_seconds: 1,
        interrupted: false,
        locations: vec![],
    }
}

#[test]
fn restore_uses_shared_acl_without_chown_and_capture_keeps_runtime_access() {
    // A subprocess supplies a different configured runtime uid without changing
    // global environment shared by parallel tests, or requiring setuid privilege.
    let current = unsafe { libc::geteuid() };
    let other = current + 1;
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "shared_acl_child", "--ignored", "--nocapture"])
        .env("KORRID_RUNTIME_UID", other.to_string())
        .env("KORRID_RUNTIME_GID", unsafe { libc::getegid() }.to_string())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "subprocess helper; invoked with a different runtime uid by the ACL test"]
fn shared_acl_child() {
    let current = unsafe { libc::geteuid() };
    let other = current + 1;
    let root = tempfile::tempdir().unwrap();
    let private = root.path().join("private");
    fs::DirBuilder::new().mode(0o700).create(&private).unwrap();
    let users = root.path().join("users");
    fs::create_dir(&users).unwrap();
    shared_acl(&users, current, other);
    let save_root = users.join(PERSON).join("saves/game");
    let source = save_root.join("saves");
    fs::create_dir_all(&source).unwrap();
    let runtime_file = source.join("game.srm");
    // Runtime programs normally create saves with 0666; the default ACL, not
    // umask, gives the other named uid read/write while group/other stay denied.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o666)
        .open(&runtime_file)
        .unwrap();
    std::io::Write::write_all(&mut file, b"runtime progress").unwrap();
    drop(file);
    assert_eq!(named_access(&runtime_file, other), 6);
    let store = NativeSaveStore::new(&private);
    let locations = vec![SaveLocation {
        key: "saves".into(),
        kind: SaveKind::Save,
        path: source.display().to_string(),
    }];
    let captured = store.capture(PERSON, &locations, snapshot()).unwrap();
    assert_eq!(
        captured.locations[0].files[0].sha256,
        korrid::saves::record::hash(b"runtime progress")
    );
    let id = store.put_record(PERSON, &captured).unwrap();
    assert_eq!(id, snapshot_id(&captured).unwrap());
    fs::remove_dir_all(users.join(PERSON)).unwrap();
    store
        .materialize(PERSON, "game", &id, save_root.to_str().unwrap(), &locations)
        .unwrap();
    assert_eq!(fs::read(&runtime_file).unwrap(), b"runtime progress");
    assert_eq!(
        fs::metadata(&runtime_file).unwrap().uid(),
        current,
        "restore must not chown"
    );
    assert_eq!(
        named_access(&runtime_file, other),
        6,
        "the different runtime uid can read and write by the kernel ACL model"
    );
    for directory in [
        users.join(PERSON),
        save_root.parent().unwrap().to_path_buf(),
        save_root.clone(),
        source,
    ] {
        assert_eq!(
            named_access(&directory, other),
            7,
            "runtime uid can traverse and create in {}",
            directory.display()
        );
    }
    let object = store
        .object(PERSON, &captured.locations[0].files[0].sha256)
        .unwrap();
    assert_eq!(
        object.metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(private.join("saves"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
fn restore_never_changes_acl_or_contents_outside_users_or_through_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let private = root.path().join("private");
    fs::create_dir(&private).unwrap();
    let store = NativeSaveStore::new(&private);
    let id = store.put_record(PERSON, &snapshot()).unwrap();
    let outside = root.path().join("outside");
    let outside_saves = outside.join("saves");
    fs::create_dir_all(&outside_saves).unwrap();
    fs::write(outside_saves.join("keep"), b"unchanged").unwrap();
    let mode = fs::metadata(&outside).unwrap().permissions().mode();
    let locations = vec![SaveLocation {
        key: "saves".into(),
        kind: SaveKind::Save,
        path: outside_saves.display().to_string(),
    }];
    assert!(store
        .materialize(PERSON, "game", &id, outside.to_str().unwrap(), &locations)
        .is_err());
    assert_eq!(fs::metadata(&outside).unwrap().permissions().mode(), mode);
    assert_eq!(fs::read(outside_saves.join("keep")).unwrap(), b"unchanged");
    let saves = root.path().join("users").join(PERSON).join("saves");
    fs::create_dir_all(&saves).unwrap();
    let game = saves.join("game");
    std::os::unix::fs::symlink(&outside, &game).unwrap();
    let locations = vec![SaveLocation {
        key: "saves".into(),
        kind: SaveKind::Save,
        path: game.join("saves").display().to_string(),
    }];
    assert!(store
        .materialize(PERSON, "game", &id, game.to_str().unwrap(), &locations)
        .is_err());
    assert_eq!(fs::read(outside_saves.join("keep")).unwrap(), b"unchanged");
}
