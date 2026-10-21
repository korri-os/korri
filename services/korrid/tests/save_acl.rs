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
        if tag == 32 {
            assert_eq!(permissions, 0, "others must stay denied");
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
    activate_users(root.path());
    let account = korrid::saves::access::prepare_account(root.path(), PERSON).unwrap();
    assert!(acl(&users, c"system.posix_acl_default").is_none());
    // The per-person ACL must match the old users/default rule exactly,
    // including the runtime group default used by prepared AccountRoot games.
    let legacy = root.path().join("old-account-rule");
    let config = root.path().join("old-account.conf");
    fs::write(
        &config,
        format!(
            "d {} 2770 {} {} -\na+ {} - - - - u:{}:rwx,d:u:{}:rwx,d:g:{}:rwx\n",
            legacy.display(),
            current,
            unsafe { libc::getegid() },
            legacy.display(),
            other,
            other,
            unsafe { libc::getegid() }
        ),
    )
    .unwrap();
    let status = std::process::Command::new("systemd-tmpfiles")
        .arg("--create")
        .arg(&config)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(
        acl(&account, c"system.posix_acl_access"),
        acl(&legacy, c"system.posix_acl_access")
    );
    assert_eq!(
        acl(&account, c"system.posix_acl_default"),
        acl(&legacy, c"system.posix_acl_default")
    );
    assert_eq!(
        fs::metadata(&account).unwrap().permissions().mode() & 0o7777,
        0o2770
    );
    let steam = account.join("steam");
    fs::create_dir(&steam).unwrap();
    let credential = steam.join("credentials");
    fs::write(&credential, b"private").unwrap();
    for path in [&account, &steam, &credential] {
        for name in [c"system.posix_acl_access", c"system.posix_acl_default"] {
            if let Some(bytes) = acl(path, name) {
                assert!(
                    !bytes[4..].chunks_exact(8).any(|entry| u16::from_le_bytes(
                        entry[0..2].try_into().unwrap()
                    ) == 2
                        && u32::from_le_bytes(entry[4..8].try_into().unwrap()) == current),
                    "new named korrid grant outside saves: {}",
                    path.display()
                );
            }
        }
    }
    let steam_access = acl(&steam, c"system.posix_acl_access");
    let steam_default = acl(&steam, c"system.posix_acl_default");
    let credential_access = acl(&credential, c"system.posix_acl_access");
    activate_users(root.path());
    korrid::saves::access::prepare_account(root.path(), PERSON).unwrap();
    assert_eq!(acl(&steam, c"system.posix_acl_access"), steam_access);
    assert_eq!(acl(&steam, c"system.posix_acl_default"), steam_default);
    assert_eq!(
        acl(&credential, c"system.posix_acl_access"),
        credential_access
    );
    prepared_account_file_resolves_and_hashes(root.path(), &account);

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
    assert_eq!(named_access(&runtime_file, current), 6);
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

fn acl(path: &Path, name: &std::ffi::CStr) -> Option<Vec<u8>> {
    let file = fs::File::open(path).unwrap();
    let mut bytes = vec![0u8; 512];
    let len = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
        )
    };
    if len < 0 {
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ENODATA)
        );
        return None;
    }
    bytes.truncate(len as usize);
    Some(bytes)
}

fn activate_users(root: &Path) {
    let rules = include_str!("../../../nix/product/game-library-access.nix")
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('"') && line.contains("${root}/users"))
        .map(|line| {
            line.trim_matches('"')
                .replace("${root}", root.to_str().unwrap())
                .replace("${user}", &(unsafe { libc::geteuid() } + 1).to_string())
                .replace("${group}", &unsafe { libc::getegid() }.to_string())
                .replace("korrid", &unsafe { libc::geteuid() }.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!rules.is_empty());
    let config = root.join("users-tmpfiles.conf");
    fs::write(&config, rules).unwrap();
    let result = std::process::Command::new("systemd-tmpfiles")
        .arg("--create")
        .arg(&config)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn activation_leaves_existing_steam_acl_and_credentials_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let steam = root.path().join("users").join(PERSON).join("steam");
    fs::create_dir_all(&steam).unwrap();
    let credentials = steam.join("credentials");
    fs::write(&credentials, b"private credentials").unwrap();
    let before = [
        acl(&steam, c"system.posix_acl_access"),
        acl(&steam, c"system.posix_acl_default"),
        acl(&credentials, c"system.posix_acl_access"),
    ];
    activate_users(root.path());
    assert_eq!(
        [
            acl(&steam, c"system.posix_acl_access"),
            acl(&steam, c"system.posix_acl_default"),
            acl(&credentials, c"system.posix_acl_access")
        ],
        before,
        "activation must not add named korrid access under Steam"
    );
    assert_eq!(fs::read(credentials).unwrap(), b"private credentials");
}

fn prepared_account_file_resolves_and_hashes(root: &Path, account: &Path) {
    use korrid::{
        discovery::identify::{resolve_declared_games, HashCache},
        plugin::{DeclaredGame, DeclaredRelease, DeclaredReleaseFile},
    };
    // The real AccountRoot form from catalog_migrate's Fallout CE fixture.
    // No per-file permission patch: the existing group default supplies access.
    let folder = account.join("fallout-ce");
    fs::create_dir(&folder).unwrap();
    let bytes = b"prepared Fallout data";
    let path = folder.join("MASTER.DAT");
    fs::write(&path, bytes).unwrap();
    let access = acl(&path, c"system.posix_acl_access").unwrap();
    let gid = unsafe { libc::getegid() };
    assert!(access[4..].chunks_exact(8).any(|entry| u16::from_le_bytes(
        entry[0..2].try_into().unwrap()
    ) == 8
        && u32::from_le_bytes(entry[4..8].try_into().unwrap()) == gid
        && u16::from_le_bytes(entry[2..4].try_into().unwrap()) == 7));
    let digest = korrid::saves::record::hash(bytes);
    let games = std::collections::BTreeMap::from([(
        "@test:fallout/fallout".into(),
        DeclaredGame {
            provider: "@test:fallout".into(),
            key: "fallout".into(),
            title: "Fallout".into(),
            releases: indexmap::IndexMap::from([(
                "prepared".into(),
                DeclaredRelease {
                    system: None,
                    hashes: vec![digest],
                    artifacts: vec![],
                    file: Some(DeclaredReleaseFile::AccountRoot(
                        "fallout-ce/MASTER.DAT".into(),
                    )),
                },
            )]),
        },
    )]);
    let accounts = korrid::discovery::sources::account_roots(root).unwrap();
    let scan = resolve_declared_games(&accounts, &games, &mut HashCache::default());
    assert_eq!(scan.found().count(), 1, "{scan:?}");
    assert_eq!(scan.hashed_bytes, bytes.len() as u64);
}

#[test]
fn account_scaffolding_refuses_a_save_symlink_before_changing_any_acl() {
    let root = tempfile::tempdir().unwrap();
    let account = root.path().join("users").join(PERSON);
    let steam = account.join("steam");
    fs::create_dir_all(&steam).unwrap();
    fs::write(steam.join("credentials"), b"private").unwrap();
    std::os::unix::fs::symlink(&steam, account.join("saves")).unwrap();
    let before = acl(&account, c"system.posix_acl_access");
    assert!(korrid::saves::access::prepare_account(root.path(), PERSON).is_err());
    assert_eq!(acl(&account, c"system.posix_acl_access"), before);
    assert!(acl(&steam, c"system.posix_acl_access").is_none());
    assert_eq!(fs::read(steam.join("credentials")).unwrap(), b"private");
}
