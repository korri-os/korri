use crate::identity::{DeviceIdentity, IdentityState, OwnerStatementStatus};
use std::{
    ffi::OsString,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

const MAX_OWNER_BINDING_BYTES: usize = 64 * 1024;

pub fn run(
    arguments: &[OsString],
    private_state_root: &Path,
    now: u64,
    stdin: &mut dyn Read,
) -> Result<String, String> {
    match arguments {
        [command] if command == "status" => status(private_state_root),
        [command] if command == "owner-binding-request" => {
            owner_binding_request(private_state_root, now)
        }
        [command, source] if command == "import" && source == "--stdin" => {
            let event = read_bounded(stdin)?;
            import(private_state_root, &event)
        }
        [command, flag, path] if command == "import" && flag == "--file" => {
            let path = PathBuf::from(path);
            let mut file = File::open(&path).map_err(|error| {
                format!("could not open owner-binding file {}: {error}", path.display())
            })?;
            let event = read_bounded(&mut file)?;
            import(private_state_root, &event)
        }
        [command] if command == "reset" => {
            let identity = DeviceIdentity::reset(private_state_root)
                .map_err(|error| error.to_string())?;
            serialize_state(identity.state()).map_err(|error| error.to_string())
        }
        _ => Err(
            "usage: korrid identity {status|owner-binding-request|import --file PATH|import --stdin|reset}"
                .into(),
        ),
    }
}

pub fn run_from_environment(
    arguments: &[OsString],
    private_state_root: &Path,
) -> Result<String, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system clock is before the Unix epoch".to_owned())?
        .as_secs();
    run(arguments, private_state_root, now, &mut io::stdin().lock())
}

fn status(private_state_root: &Path) -> Result<String, String> {
    let identity =
        DeviceIdentity::load_or_create(private_state_root).map_err(|error| error.to_string())?;
    serialize_state(identity.state()).map_err(|error| error.to_string())
}

fn owner_binding_request(private_state_root: &Path, now: u64) -> Result<String, String> {
    DeviceIdentity::load_or_create(private_state_root)
        .and_then(|identity| identity.owner_statement_template(OwnerStatementStatus::Owned, now))
        .map_err(|error| error.to_string())
}

fn import(private_state_root: &Path, event_json: &str) -> Result<String, String> {
    let mut identity =
        DeviceIdentity::load_or_create(private_state_root).map_err(|error| error.to_string())?;
    let device_public_key = identity
        .device_public_key()
        .ok_or_else(|| "device identity has no usable public key".to_owned())?;
    let candidate = DeviceIdentity::verify_owner_statement(event_json, device_public_key)
        .map_err(|error| error.to_string())?;
    if candidate.status != OwnerStatementStatus::Owned {
        return Err("owner-binding import requires an owned statement".into());
    }
    let already_imported = identity
        .owner_statement_json()
        .and_then(|current| DeviceIdentity::verify_event(&current).ok())
        .is_some_and(|current| current.id == candidate.event_id);
    if !already_imported {
        identity
            .apply_owner_statement(event_json)
            .map_err(|error| error.to_string())?;
    }
    serialize_state(identity.state()).map_err(|error| error.to_string())
}

fn read_bounded(reader: &mut dyn Read) -> Result<String, String> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_OWNER_BINDING_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("could not read owner binding: {error}"))?;
    if bytes.len() > MAX_OWNER_BINDING_BYTES {
        return Err("owner binding exceeds 65536 bytes".into());
    }
    String::from_utf8(bytes).map_err(|_| "owner binding is not UTF-8".into())
}

fn serialize_state(state: &IdentityState) -> Result<String, serde_json::Error> {
    serde_json::to_string(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{
        event::{EventBuilder, FinalizeEvent, Kind, Tag},
        key::{Keys, SecretKey},
        types::Timestamp,
    };
    use std::{fs, io::Cursor};

    fn signed_binding(root: &Path, created_at: u64) -> String {
        let identity = DeviceIdentity::load_or_create(root).unwrap();
        let template: serde_json::Value = serde_json::from_str(
            &identity
                .owner_statement_template(OwnerStatementStatus::Owned, created_at)
                .unwrap(),
        )
        .unwrap();
        let owner = Keys::new(
            SecretKey::from_hex("0000000000000000000000000000000000000000000000000000000000000003")
                .unwrap(),
        );
        EventBuilder::new(Kind::Custom(30_078), "")
            .tags(template["tags"].as_array().unwrap().iter().map(|tag| {
                Tag::parse(
                    tag.as_array()
                        .unwrap()
                        .iter()
                        .map(|value| value.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>(),
                )
                .unwrap()
            }))
            .custom_created_at(Timestamp::from(created_at))
            .finalize(&owner)
            .unwrap()
            .as_json()
    }

    #[test]
    fn status_and_request_print_only_public_identity_data() {
        let root = tempfile::tempdir().unwrap();
        let status = run(&["status".into()], root.path(), 42, &mut Cursor::new([])).unwrap();
        let request = run(
            &["owner-binding-request".into()],
            root.path(),
            42,
            &mut Cursor::new([]),
        )
        .unwrap();

        assert!(status.contains("devicePublicKey"));
        assert!(!status.contains("private"));
        assert!(!status.contains("secret"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&request).unwrap()["created_at"],
            42
        );
        assert!(!request.contains("pubkey"));
        assert!(!request.contains("sig"));
        assert!(!request.contains("private"));
        assert!(!request.contains("secret"));
    }

    #[test]
    fn imports_the_same_public_binding_from_a_file_or_stdin_idempotently() {
        let file_root = tempfile::tempdir().unwrap();
        let file_event = signed_binding(file_root.path(), 42);
        let file_path = file_root.path().join("binding.json");
        fs::write(&file_path, &file_event).unwrap();
        let first = run(
            &["import".into(), "--file".into(), file_path.into_os_string()],
            file_root.path(),
            50,
            &mut Cursor::new([]),
        )
        .unwrap();
        let second = run(
            &["import".into(), "--stdin".into()],
            file_root.path(),
            50,
            &mut Cursor::new(file_event),
        )
        .unwrap();

        assert_eq!(first, second);
        assert!(first.contains("\"_tag\":\"Owned\""));
    }

    #[test]
    fn rejects_revocations_secrets_and_implicit_import_sources() {
        let root = tempfile::tempdir().unwrap();
        for arguments in [
            vec!["import".into(), "nsec1notaccepted".into()],
            vec!["import".into(), "--person-private-key".into(), "00".into()],
            vec!["owner-binding-request".into(), "--private-key".into()],
        ] {
            assert!(run(&arguments, root.path(), 42, &mut Cursor::new([]))
                .unwrap_err()
                .starts_with("usage:"));
        }
        assert!(run(
            &["import".into(), "--stdin".into()],
            root.path(),
            42,
            &mut Cursor::new("nsec1notaccepted"),
        )
        .is_err());
    }

    #[test]
    fn reset_returns_a_new_unowned_public_identity() {
        let root = tempfile::tempdir().unwrap();
        let before: serde_json::Value = serde_json::from_str(
            &run(&["status".into()], root.path(), 42, &mut Cursor::new([])).unwrap(),
        )
        .unwrap();
        let after: serde_json::Value = serde_json::from_str(
            &run(&["reset".into()], root.path(), 42, &mut Cursor::new([])).unwrap(),
        )
        .unwrap();

        assert_eq!(after["_tag"], "Unowned");
        assert_ne!(after["devicePublicKey"], before["devicePublicKey"]);
    }
}
