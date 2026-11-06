use super::*;
use korrid::identity::{DeviceIdentity, OwnerStatementStatus};
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::Keys,
    types::Timestamp,
};
use std::io::Cursor;

#[test]
fn only_existing_public_identity_tools_are_exposed() {
    let args = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
    assert_eq!(
        command(&args(&["identity", "status"])).unwrap(),
        Command::Identity(args(&["status"]))
    );
    assert!(command(&args(&["identity", "owner-binding-request"])).is_ok());
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
fn roots_are_private_owned_and_exclusive_without_repair_or_fallback() {
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
    assert!(State::open(&root).is_err());
    assert_eq!(
        fs::metadata(root.join("private"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o755
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
