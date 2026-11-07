use super::*;
use korrid::identity::{DeviceIdentity, OwnerStatementStatus};
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::Keys,
    types::Timestamp,
};
use std::io::Cursor;

#[test]
fn only_existing_public_identity_and_explicit_setup_tools_are_exposed() {
    let args = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
    assert_eq!(
        command(&args(&["identity", "status"])).unwrap(),
        Command::Identity(args(&["status"]))
    );
    assert!(command(&args(&["identity", "owner-binding-request"])).is_ok());
    assert_eq!(
        command(&args(&["setup", "finish"])).unwrap(),
        Command::SetupFinish
    );
    assert!(command(&args(&["identity", "import", "--stdin"])).is_ok());
    assert!(command(&args(&[
        "identity",
        "import",
        "--file",
        "/tmp/binding.json"
    ]))
    .is_ok());
    for words in [
        vec!["identity", "reset"],
        vec!["--webos"],
        vec!["identity", "import", "secret"],
    ] {
        assert!(command(&args(&words)).is_err());
    }
}

#[test]
fn roots_are_private_owned_and_exclusive_without_state_fallback() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("state");
    let state = State::open(&root).unwrap();
    for path in [&root, &state.private, &state.storage] {
        let metadata = fs::symlink_metadata(path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o7777, 0o700);
        assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
    }
    assert!(State::open(&root).is_err());
    drop(state);
    assert!(State::open(&root).is_ok());
    fs::set_permissions(root.join("private"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(State::open(&root).is_ok());
    assert_eq!(
        fs::metadata(root.join("private"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o700
    );
}

#[test]
fn refuses_symlinks_files_and_wrong_owner() {
    let parent = tempfile::tempdir().unwrap();
    let target = parent.path().join("target");
    private_directory(&target, true).unwrap();
    let link = parent.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(State::open(&link).is_err());
    assert!(State::open(&link.join("child")).is_err());
    assert!(!target.join("child").exists());
    let file = parent.path().join("file");
    fs::write(&file, b"preserve").unwrap();
    assert!(State::open(&file).is_err());
    assert_eq!(fs::read(file).unwrap(), b"preserve");
    // /root is 0700 but not owned by this unprivileged test process.
    if unsafe { libc::geteuid() } != 0 && fs::symlink_metadata("/root").is_ok() {
        assert!(private_directory(Path::new("/root"), false).is_err());
    }
}

#[test]
fn credential_requires_private_directory_and_never_replaces_a_file_or_link() {
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    write_credential(directory.path(), "first").unwrap();
    let credential = directory.path().join("KORRID_RPC_CAPABILITY");
    assert_eq!(
        fs::metadata(&credential).unwrap().permissions().mode() & 0o7777,
        0o600
    );
    assert!(write_credential(directory.path(), "second").is_err());
    assert_eq!(fs::read(&credential).unwrap(), b"first");
    fs::remove_file(&credential).unwrap();
    std::os::unix::fs::symlink(directory.path().join("victim"), &credential).unwrap();
    assert!(write_credential(directory.path(), "second").is_err());
    assert!(!directory.path().join("victim").exists());
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(write_credential(directory.path(), "second").is_err());
}

fn signed_binding(template: &str, owner: &Keys) -> String {
    let value: serde_json::Value = serde_json::from_str(template).unwrap();
    EventBuilder::new(Kind::Custom(30_078), "")
        .tags(value["tags"].as_array().unwrap().iter().map(|tag| {
            Tag::parse(
                tag.as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_str().unwrap()),
            )
            .unwrap()
        }))
        .custom_created_at(Timestamp::from(value["created_at"].as_u64().unwrap()))
        .finalize(owner)
        .unwrap()
        .as_json()
}

#[test]
fn exact_offline_request_import_survives_restart_and_real_brain_has_no_local_games() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("state");
    let owner = Keys::generate();
    let state = State::open(&root).unwrap();
    let template = korrid::identity_cli::run(
        &["owner-binding-request".into()],
        &state.private,
        42,
        &mut Cursor::new([]),
    )
    .unwrap();
    let signed = signed_binding(&template, &owner);
    let imported = korrid::identity_cli::run(
        &["import".into(), "--stdin".into()],
        &state.private,
        43,
        &mut Cursor::new(&signed),
    )
    .unwrap();
    assert!(imported.contains("\"_tag\":\"Owned\""));
    let key = DeviceIdentity::load_or_create(&state.private)
        .unwrap()
        .device_public_key()
        .unwrap()
        .to_owned();
    let other = tempfile::tempdir().unwrap();
    let wrong_template = DeviceIdentity::load_or_create(other.path())
        .unwrap()
        .owner_statement_template(OwnerStatementStatus::Owned, 42)
        .unwrap();
    assert!(korrid::identity_cli::run(
        &["import".into(), "--stdin".into()],
        &state.private,
        43,
        &mut Cursor::new(signed_binding(&wrong_template, &owner)),
    )
    .is_err());
    drop(state);
    let state = State::open(&root).unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let mut previous = String::new();
    for _ in 0..2 {
        let credentials = tempfile::tempdir().unwrap();
        fs::set_permissions(credentials.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let running = start(&state, credentials.path(), None).unwrap();
        let capability =
            fs::read_to_string(credentials.path().join("KORRID_RPC_CAPABILITY")).unwrap();
        assert_ne!(capability, previous);
        previous = capability.clone();
        let port = korrid::local_server_port().unwrap();
        let url = format!("http://127.0.0.1:{port}/rpc");
        let request = serde_json::json!({"_tag": "system.health", "payload": {}});
        assert_eq!(
            client.post(&url).json(&request).send().unwrap().status(),
            reqwest::StatusCode::UNAUTHORIZED
        );
        for tag in [
            "system.health",
            "app.local-games.list",
            "app.catalog.snapshot",
        ] {
            let response: serde_json::Value = client
                .post(&url)
                .bearer_auth(&capability)
                .json(&serde_json::json!({"_tag": tag, "payload": {}}))
                .send()
                .unwrap()
                .json()
                .unwrap();
            assert_eq!(response["outcome"]["_tag"], "Ok", "{response}");
            if tag != "system.health" {
                assert_eq!(
                    response["outcome"]["payload"]["games"]
                        .as_array()
                        .unwrap()
                        .len(),
                    0
                );
            }
        }
        assert_eq!(
            DeviceIdentity::load_or_create(&state.private)
                .unwrap()
                .device_public_key(),
            Some(key.as_str())
        );
        drop(running);
        assert!(korrid::local_server_port().is_none());
    }
}

#[test]
fn explicit_setup_finish_uses_existing_marker_and_keeps_the_identity() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("state");
    let state = State::open(&root).unwrap();
    let identity = DeviceIdentity::load_or_create(&state.private).unwrap();
    let before = identity.state().clone();
    assert!(matches!(
        korrid::onboarding::status(&state.private),
        korrid::onboarding::OnboardingStatusOutcome::Ok(korrid::onboarding::OnboardingStatus {
            needed: true
        })
    ));
    finish_setup(&state.private).unwrap();
    finish_setup(&state.private).unwrap();
    drop(state);
    let state = State::open(&root).unwrap();
    assert!(matches!(
        korrid::onboarding::status(&state.private),
        korrid::onboarding::OnboardingStatusOutcome::Ok(korrid::onboarding::OnboardingStatus {
            needed: false
        })
    ));
    assert_eq!(
        DeviceIdentity::load_or_create(&state.private)
            .unwrap()
            .state(),
        &before
    );
}

fn allocation_entries(root: &Path) -> Vec<PathBuf> {
    let mut entries = vec![root.to_owned()];
    if fs::symlink_metadata(root).unwrap().is_dir() {
        for child in fs::read_dir(root).unwrap() {
            entries.extend(allocation_entries(&child.unwrap().path()));
        }
    }
    entries
}

fn emulate_devmode_boot(root: &Path) {
    for entry in allocation_entries(root) {
        fs::set_permissions(entry, fs::Permissions::from_mode(0o777)).unwrap();
    }
}

#[test]
fn devmode_world_writable_tree_is_repaired_before_real_startup_and_offline_commands() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("state");
    let state = State::open(&root).unwrap();
    let template = korrid::identity_cli::run(
        &["owner-binding-request".into()],
        &state.private,
        42,
        &mut Cursor::new([]),
    )
    .unwrap();
    let owner = Keys::generate();
    korrid::identity_cli::run(
        &["import".into(), "--stdin".into()],
        &state.private,
        43,
        &mut Cursor::new(signed_binding(&template, &owner)),
    )
    .unwrap();
    finish_setup(&state.private).unwrap();
    fs::write(state.storage.join("device.yaml"), "{}\n").unwrap();
    let identity = DeviceIdentity::load_or_create(&state.private)
        .unwrap()
        .state()
        .clone();
    let key = fs::read(state.private.join("identity/device.key")).unwrap();
    drop(state);
    // A sibling and the shared parent are outside the app's allocation.
    let parent_mode = fs::metadata(parent.path()).unwrap().mode();
    let sibling = parent.path().join("not-our-allocation");
    fs::write(&sibling, "preserve").unwrap();
    fs::set_permissions(&sibling, fs::Permissions::from_mode(0o777)).unwrap();
    emulate_devmode_boot(&root);
    let state = State::open(&root).unwrap();
    for entry in allocation_entries(&root) {
        let metadata = fs::symlink_metadata(entry).unwrap();
        assert_eq!(
            metadata.mode() & 0o7777,
            if metadata.is_dir() { 0o700 } else { 0o600 }
        );
    }
    assert_eq!(fs::metadata(&sibling).unwrap().mode() & 0o7777, 0o777);
    assert_eq!(fs::metadata(parent.path()).unwrap().mode(), parent_mode);
    assert!(fs::read(state.private.join("identity/device.key")).unwrap() == key);
    assert_eq!(
        DeviceIdentity::load_or_create(&state.private)
            .unwrap()
            .state(),
        &identity
    );
    let credentials = tempfile::tempdir().unwrap();
    fs::set_permissions(credentials.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let running = start(&state, credentials.path(), None).unwrap();
    let capability = fs::read_to_string(credentials.path().join("KORRID_RPC_CAPABILITY")).unwrap();
    let response: serde_json::Value = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .post(format!(
            "http://127.0.0.1:{}/rpc",
            korrid::local_server_port().unwrap()
        ))
        .bearer_auth(capability)
        .json(&serde_json::json!({"_tag": "system.health", "payload": {}}))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(response["outcome"]["_tag"], "Ok");
    drop(running);
    drop(state);
    emulate_devmode_boot(&root);
    let state = State::open(&root).unwrap();
    let status =
        korrid::identity_cli::run(&["status".into()], &state.private, 44, &mut Cursor::new([]))
            .unwrap();
    assert!(status.contains("Owned"));
    assert_eq!(
        DeviceIdentity::load_or_create(&state.private)
            .unwrap()
            .state(),
        &identity
    );
}

#[test]
fn unsafe_nested_entries_refuse_the_whole_repair_without_changing_other_modes() {
    for unsafe_kind in ["symlink", "fifo", "hardlink"] {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("state");
        drop(State::open(&root).unwrap());
        let outside = parent.path().join("outside");
        fs::write(&outside, "preserve").unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o777)).unwrap();
        let ordinary = root.join("private/ordinary");
        fs::write(&ordinary, "preserve").unwrap();
        emulate_devmode_boot(&root);
        let unsafe_entry = root.join("storage/unsafe");
        match unsafe_kind {
            "symlink" => std::os::unix::fs::symlink(&outside, &unsafe_entry).unwrap(),
            "hardlink" => fs::hard_link(&outside, &unsafe_entry).unwrap(),
            "fifo" => {
                use std::os::unix::ffi::OsStrExt;
                let path = std::ffi::CString::new(unsafe_entry.as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            _ => unreachable!(),
        }
        assert!(State::open(&root).is_err(), "must reject {unsafe_kind}");
        for unchanged in [
            &root,
            &root.join("private"),
            &root.join("storage"),
            &ordinary,
            &outside,
        ] {
            assert_eq!(
                fs::symlink_metadata(unchanged).unwrap().mode() & 0o7777,
                0o777
            );
        }
        assert_eq!(fs::read(&outside).unwrap(), b"preserve");
    }
}
