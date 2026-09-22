//! Local person signer and its Unix-socket `PersonSigner` adapter.

use crate::{
    identity::validate_owned_statement_template,
    remote_signer::{PersonSigner, PersonSignerRequest, PersonSignerState},
};
use futures::future::BoxFuture;
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::{Keys, PublicKey},
    nips::{
        nip19::{FromBech32, ToBech32},
        nip49::EncryptedSecretKey,
    },
    types::Timestamp,
};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zeroize::Zeroizing;

const IDENTITY_DIRECTORY: &str = "identity";
const PERSON_KEY_FILE: &str = "person.key";
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
static SIGNER_STORAGE: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
pub enum LocalSignerError {
    #[error("local signer storage is unavailable")]
    Storage,
    #[error("local signer key is invalid")]
    InvalidKey,
    #[error("password must not be blank")]
    InvalidPassword,
    #[error("NIP-49 encrypted secret is malformed")]
    MalformedEncryptedSecret,
    #[error("NIP-49 encrypted secret could not be decrypted with this password")]
    IncorrectPassword,
    #[error("local signer could not encrypt the person key")]
    Encryption,
}

pub struct LocalPersonSigner {
    keys: Keys,
    expected_device_public_key: String,
    state: Mutex<PersonSignerState>,
}

impl LocalPersonSigner {
    /// Load the fixed automatic person key or create it once.
    ///
    /// The private file follows the established identity-key representation:
    /// one lowercase hexadecimal secp256k1 key plus a newline in a `0600` file
    /// under a `0700` fixed identity directory.
    pub fn load_or_create(
        private_state_root: &Path,
        expected_device_public_key: &str,
    ) -> Result<Self, LocalSignerError> {
        let expected_device_public_key = parse_public_key_text(expected_device_public_key)?;
        let _storage = SIGNER_STORAGE
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("local signer storage mutex poisoned");
        let directory = prepare_identity_directory(private_state_root)?;
        let _process_lock = lock_directory(&directory)?;
        cleanup_private_temporaries(&directory)?;
        let path = directory.join(PERSON_KEY_FILE);
        let keys = match read_private_optional(&path)? {
            Some(bytes) => parse_keys(bytes)?,
            None => {
                let keys = Keys::generate();
                let mut encoded = Zeroizing::new(keys.secret_key().to_secret_hex().into_bytes());
                encoded.push(b'\n');
                if write_new_private(&path, &encoded)? {
                    keys
                } else {
                    parse_keys(read_private_optional(&path)?.ok_or(LocalSignerError::Storage)?)?
                }
            }
        };
        Ok(Self::from_keys(keys, expected_device_public_key))
    }

    /// Export this signer's person key as a NIP-49 encrypted secret.
    ///
    /// The password is passed unchanged to the NIP-49 implementation. A
    /// whitespace-only password is rejected before encryption.
    pub fn export_nip49(&self, password: &str) -> Result<String, LocalSignerError> {
        validate_password(password)?;
        self.keys
            .secret_key()
            .encrypt(password)
            .and_then(|encrypted| encrypted.to_bech32())
            .map_err(|_| LocalSignerError::Encryption)
    }

    /// Import a NIP-49 encrypted secret as a non-persisted signer candidate.
    ///
    /// This does not read or write signer storage. The returned candidate can
    /// report its public key and implement the existing [`PersonSigner`]
    /// owner-binding contract for a later identity-switch prepare operation.
    pub fn import_nip49(
        encrypted_secret: &str,
        password: &str,
        expected_device_public_key: &str,
    ) -> Result<Self, LocalSignerError> {
        validate_password(password)?;
        let expected_device_public_key = parse_public_key_text(expected_device_public_key)?;
        let encrypted = EncryptedSecretKey::from_bech32(encrypted_secret)
            .map_err(|_| LocalSignerError::MalformedEncryptedSecret)?;
        let secret = encrypted
            .decrypt(password)
            .map_err(|_| LocalSignerError::IncorrectPassword)?;
        Ok(Self::from_keys(
            Keys::new(secret),
            expected_device_public_key,
        ))
    }

    pub fn public_key(&self) -> String {
        self.keys.public_key().to_hex()
    }

    fn from_keys(keys: Keys, expected_device_public_key: String) -> Self {
        Self {
            keys,
            expected_device_public_key,
            state: Mutex::new(PersonSignerState::Unavailable {
                message: "Local signer is ready".into(),
            }),
        }
    }

    fn sign(&self, template: &str) -> Result<(String, String), ()> {
        if template.len() > MAX_MESSAGE_BYTES {
            return Err(());
        }
        let template = validate_owned_statement_template(template).map_err(|_| ())?;
        if template.device_public_key != self.expected_device_public_key {
            return Err(());
        }
        let tags = template
            .tags
            .into_iter()
            .map(|tag| Tag::parse(tag).map_err(|_| ()))
            .collect::<Result<Vec<_>, _>>()?;
        let event = EventBuilder::new(
            Kind::Custom(crate::identity::OWNER_EVENT_KIND),
            template.content,
        )
        .tags(tags)
        .custom_created_at(Timestamp::from(template.created_at))
        .finalize(&self.keys)
        .map_err(|_| ())?;
        Ok((self.keys.public_key().to_hex(), event.as_json()))
    }
}

impl PersonSigner for LocalPersonSigner {
    fn state(&self) -> PersonSignerState {
        self.state
            .lock()
            .expect("local signer state poisoned")
            .clone()
    }

    fn request<'a>(&'a self, request: PersonSignerRequest) -> BoxFuture<'a, PersonSignerState> {
        Box::pin(async move {
            *self.state.lock().expect("local signer state poisoned") = PersonSignerState::Pending {
                message: "Signing the local owner binding".into(),
            };
            let next = match self.sign(&request.unsigned_event_template) {
                Ok((owner_public_key, signed_event_json)) => PersonSignerState::Approved {
                    owner_public_key,
                    unsigned_event_template: request.unsigned_event_template,
                    signed_event_json,
                },
                Err(()) => PersonSignerState::InvalidResponse {
                    message: "Local signer accepts only a valid owner-binding template".into(),
                },
            };
            *self.state.lock().expect("local signer state poisoned") = next.clone();
            next
        })
    }
}

pub struct UnixPersonSigner {
    socket: PathBuf,
    state: Mutex<PersonSignerState>,
}

impl UnixPersonSigner {
    pub fn new(socket: PathBuf) -> Self {
        Self {
            socket,
            state: Mutex::new(PersonSignerState::Unavailable {
                message: "Local signer has not been contacted".into(),
            }),
        }
    }
}

impl PersonSigner for UnixPersonSigner {
    fn state(&self) -> PersonSignerState {
        self.state
            .lock()
            .expect("local signer state poisoned")
            .clone()
    }

    fn request<'a>(&'a self, request: PersonSignerRequest) -> BoxFuture<'a, PersonSignerState> {
        Box::pin(async move {
            *self.state.lock().expect("local signer state poisoned") = PersonSignerState::Pending {
                message: "Waiting for the local signer".into(),
            };
            let next = request_over_unix(&self.socket, &request)
                .await
                .unwrap_or_else(|message| PersonSignerState::Defect { message });
            *self.state.lock().expect("local signer state poisoned") = next.clone();
            next
        })
    }
}

pub async fn serve_connection(
    mut stream: tokio::net::UnixStream,
    signer: Arc<LocalPersonSigner>,
) -> Result<(), std::io::Error> {
    let mut bytes = Vec::new();
    (&mut stream)
        .take((MAX_MESSAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    let state = if bytes.len() > MAX_MESSAGE_BYTES {
        PersonSignerState::InvalidResponse {
            message: "Local signer request is too large".into(),
        }
    } else {
        match serde_json::from_slice::<PersonSignerRequest>(&bytes) {
            Ok(request) => signer.request(request).await,
            Err(_) => PersonSignerState::InvalidResponse {
                message: "Local signer request is invalid".into(),
            },
        }
    };
    let response = serde_json::to_vec(&state).map_err(std::io::Error::other)?;
    stream.write_all(&response).await?;
    stream.shutdown().await
}

async fn request_over_unix(
    socket: &Path,
    request: &PersonSignerRequest,
) -> Result<PersonSignerState, String> {
    let bytes =
        serde_json::to_vec(request).map_err(|_| "local signer request failed".to_owned())?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("local signer request is too large".into());
    }
    let mut stream = tokio::net::UnixStream::connect(socket)
        .await
        .map_err(|_| "local signer is unavailable".to_owned())?;
    stream
        .write_all(&bytes)
        .await
        .map_err(|_| "local signer request failed".to_owned())?;
    stream
        .shutdown()
        .await
        .map_err(|_| "local signer request failed".to_owned())?;
    let mut response = Vec::new();
    stream
        .take((MAX_MESSAGE_BYTES + 1) as u64)
        .read_to_end(&mut response)
        .await
        .map_err(|_| "local signer response failed".to_owned())?;
    if response.len() > MAX_MESSAGE_BYTES {
        return Err("local signer response is too large".into());
    }
    serde_json::from_slice(&response).map_err(|_| "local signer response is invalid".into())
}

fn validate_password(password: &str) -> Result<(), LocalSignerError> {
    if password.trim().is_empty() {
        Err(LocalSignerError::InvalidPassword)
    } else {
        Ok(())
    }
}

fn parse_public_key_text(value: &str) -> Result<String, LocalSignerError> {
    let value = value.trim();
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(LocalSignerError::InvalidKey);
    }
    let key = PublicKey::from_hex(value).map_err(|_| LocalSignerError::InvalidKey)?;
    key.xonly().map_err(|_| LocalSignerError::InvalidKey)?;
    Ok(value.to_owned())
}

pub fn publish_public_key(path: &Path, public_key: &str) -> Result<(), LocalSignerError> {
    let public_key = parse_public_key_text(public_key)?;
    let parent = path.parent().ok_or(LocalSignerError::Storage)?;
    validate_directory(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("person-public-key"),
        rand::random::<u64>()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o640)
            .open(&temporary)
            .map_err(|_| LocalSignerError::Storage)?;
        file.write_all(public_key.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|_| LocalSignerError::Storage)?;
        file.set_permissions(fs::Permissions::from_mode(0o640))
            .map_err(|_| LocalSignerError::Storage)?;
        file.sync_all().map_err(|_| LocalSignerError::Storage)?;
        fs::rename(&temporary, path).map_err(|_| LocalSignerError::Storage)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn read_published_public_key(path: &Path) -> Result<String, LocalSignerError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| LocalSignerError::Storage)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o027 != 0
        || metadata.len() != 65
    {
        return Err(LocalSignerError::Storage);
    }
    let mut bytes = Vec::with_capacity(65);
    File::open(path)
        .map_err(|_| LocalSignerError::Storage)?
        .take(66)
        .read_to_end(&mut bytes)
        .map_err(|_| LocalSignerError::Storage)?;
    if bytes.len() != 65 || bytes[64] != b'\n' {
        return Err(LocalSignerError::InvalidKey);
    }
    parse_public_key_text(
        std::str::from_utf8(&bytes[..64]).map_err(|_| LocalSignerError::InvalidKey)?,
    )
}

fn prepare_identity_directory(private_state_root: &Path) -> Result<PathBuf, LocalSignerError> {
    if !private_state_root.exists() {
        match DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(private_state_root)
        {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(LocalSignerError::Storage),
        }
    }
    validate_directory(private_state_root)?;
    let directory = private_state_root.join(IDENTITY_DIRECTORY);
    if !directory.exists() {
        match DirBuilder::new().mode(0o700).create(&directory) {
            Ok(()) => sync_directory(private_state_root)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(LocalSignerError::Storage),
        }
    }
    validate_private_directory(&directory)?;
    Ok(directory)
}

fn lock_directory(path: &Path) -> Result<File, LocalSignerError> {
    let file = File::open(path).map_err(|_| LocalSignerError::Storage)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(LocalSignerError::Storage);
    }
    Ok(file)
}

fn cleanup_private_temporaries(directory: &Path) -> Result<(), LocalSignerError> {
    let prefix = format!(".{PERSON_KEY_FILE}.");
    for entry in fs::read_dir(directory).map_err(|_| LocalSignerError::Storage)? {
        let entry = entry.map_err(|_| LocalSignerError::Storage)?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(LocalSignerError::Storage)?;
        if name.starts_with(&prefix) && name.ends_with(".tmp") {
            let metadata = entry.metadata().map_err(|_| LocalSignerError::Storage)?;
            if !metadata.is_file()
                || entry
                    .file_type()
                    .map_err(|_| LocalSignerError::Storage)?
                    .is_symlink()
            {
                return Err(LocalSignerError::Storage);
            }
            fs::remove_file(entry.path()).map_err(|_| LocalSignerError::Storage)?;
        }
    }
    sync_directory(directory)
}

fn validate_directory(path: &Path) -> Result<(), LocalSignerError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| LocalSignerError::Storage)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(LocalSignerError::Storage);
    }
    Ok(())
}

fn validate_private_directory(path: &Path) -> Result<(), LocalSignerError> {
    validate_directory(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| LocalSignerError::Storage)?;
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(LocalSignerError::Storage);
    }
    Ok(())
}

fn read_private_optional(path: &Path) -> Result<Option<Vec<u8>>, LocalSignerError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(LocalSignerError::Storage),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > 128
    {
        return Err(LocalSignerError::Storage);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .map_err(|_| LocalSignerError::Storage)?
        .take(129)
        .read_to_end(&mut bytes)
        .map_err(|_| LocalSignerError::Storage)?;
    if bytes.len() > 128 {
        return Err(LocalSignerError::Storage);
    }
    Ok(Some(bytes))
}

fn parse_keys(bytes: Vec<u8>) -> Result<Keys, LocalSignerError> {
    let bytes = Zeroizing::new(bytes);
    let value = std::str::from_utf8(&bytes)
        .map_err(|_| LocalSignerError::InvalidKey)?
        .trim();
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(LocalSignerError::InvalidKey);
    }
    Keys::parse(value).map_err(|_| LocalSignerError::InvalidKey)
}

fn write_new_private(path: &Path, content: &[u8]) -> Result<bool, LocalSignerError> {
    let parent = path.parent().ok_or(LocalSignerError::Storage)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("person-key"),
        rand::random::<u64>()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| LocalSignerError::Storage)?;
        file.write_all(content)
            .map_err(|_| LocalSignerError::Storage)?;
        file.sync_all().map_err(|_| LocalSignerError::Storage)?;
        match fs::hard_link(&temporary, path) {
            Ok(()) => {
                // The final name now references the already-complete private
                // inode. Persist the install before removing the staging name.
                sync_directory(parent)?;
                fs::remove_file(&temporary).map_err(|_| LocalSignerError::Storage)?;
                sync_directory(parent)?;
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                fs::remove_file(&temporary).map_err(|_| LocalSignerError::Storage)?;
                Ok(false)
            }
            Err(_) => Err(LocalSignerError::Storage),
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sync_directory(path: &Path) -> Result<(), LocalSignerError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| LocalSignerError::Storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{DeviceIdentity, OwnerStatementStatus};
    use std::{fs, os::unix::fs::PermissionsExt, sync::Arc};

    fn device_public_key(root: &Path) -> String {
        match DeviceIdentity::load_or_create(root).unwrap().state() {
            crate::identity::IdentityState::Unowned { device_public_key }
            | crate::identity::IdentityState::Owned {
                device_public_key, ..
            } => device_public_key.clone(),
            state => panic!("unexpected identity state: {state:?}"),
        }
    }

    fn template(root: &Path, created_at: u64) -> String {
        DeviceIdentity::load_or_create(root)
            .unwrap()
            .owner_statement_template(OwnerStatementStatus::Owned, created_at)
            .unwrap()
    }

    #[tokio::test]
    async fn local_signer_creates_one_private_person_key_and_reuses_it() {
        let signer_root = tempfile::tempdir().unwrap();
        let device_root = tempfile::tempdir().unwrap();
        let request = PersonSignerRequest {
            unsigned_event_template: template(device_root.path(), 100),
        };

        let expected_device = device_public_key(device_root.path());
        let first =
            LocalPersonSigner::load_or_create(signer_root.path(), &expected_device).unwrap();
        let first_state = first.request(request.clone()).await;
        let first_owner = match first_state {
            PersonSignerState::Approved {
                owner_public_key, ..
            } => owner_public_key,
            state => panic!("unexpected signer state: {state:?}"),
        };
        let second =
            LocalPersonSigner::load_or_create(signer_root.path(), &expected_device).unwrap();
        let second_owner = match second.request(request).await {
            PersonSignerState::Approved {
                owner_public_key, ..
            } => owner_public_key,
            state => panic!("unexpected signer state: {state:?}"),
        };

        assert_eq!(second_owner, first_owner);
        let directory = signer_root.path().join("identity");
        let key = directory.join("person.key");
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(key).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[tokio::test]
    async fn local_signer_returns_an_exact_public_owner_binding_without_exposing_its_key() {
        let signer_root = tempfile::tempdir().unwrap();
        let device_root = tempfile::tempdir().unwrap();
        let mut identity = DeviceIdentity::load_or_create(device_root.path()).unwrap();
        let unsigned_event_template = identity
            .owner_statement_template(OwnerStatementStatus::Owned, 100)
            .unwrap();
        let signer = LocalPersonSigner::load_or_create(
            signer_root.path(),
            &device_public_key(device_root.path()),
        )
        .unwrap();

        let state = signer
            .request(PersonSignerRequest {
                unsigned_event_template: unsigned_event_template.clone(),
            })
            .await;
        let PersonSignerState::Approved {
            owner_public_key,
            unsigned_event_template: returned_template,
            signed_event_json,
        } = state
        else {
            panic!("local signer did not approve the owner binding")
        };

        assert_eq!(returned_template, unsigned_event_template);
        identity
            .apply_signed_owner_binding(&returned_template, &owner_public_key, &signed_event_json)
            .unwrap();
        assert!(!signed_event_json.contains("nsec"));
        assert!(!signed_event_json.contains("private"));
    }

    #[tokio::test]
    async fn local_signer_rejects_a_request_outside_the_owner_binding_contract() {
        let root = tempfile::tempdir().unwrap();
        let device = tempfile::tempdir().unwrap();
        let signer =
            LocalPersonSigner::load_or_create(root.path(), &device_public_key(device.path()))
                .unwrap();
        let canonical = template(device.path(), 100);
        let canonical_json: serde_json::Value = serde_json::from_str(&canonical).unwrap();
        let mut invalid = Vec::new();

        let mut value = canonical_json.clone();
        value["kind"] = serde_json::json!(1);
        invalid.push(value);
        let mut value = canonical_json.clone();
        value["content"] = serde_json::json!("arbitrary owner data");
        invalid.push(value);
        let mut value = canonical_json.clone();
        value["tags"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!(["extra", "authority"]));
        invalid.push(value);
        let mut value = canonical_json.clone();
        value["tags"].as_array_mut().unwrap().remove(0);
        invalid.push(value);
        let mut value = canonical_json.clone();
        value["tags"].as_array_mut().unwrap().swap(0, 1);
        invalid.push(value);
        let mut value = canonical_json.clone();
        value["tags"][2][1] = serde_json::json!("revoked");
        invalid.push(value);
        let mut value = canonical_json;
        value["tags"][0][1] = serde_json::json!("org.korri.device-owner:not-a-key");
        value["tags"][1][1] = serde_json::json!("not-a-key");
        invalid.push(value);

        for unsigned_event_template in invalid {
            let state = signer
                .request(PersonSignerRequest {
                    unsigned_event_template: serde_json::to_string(&unsigned_event_template)
                        .unwrap(),
                })
                .await;
            assert!(matches!(state, PersonSignerState::InvalidResponse { .. }));
        }
    }

    #[tokio::test]
    async fn local_signer_rejects_a_canonical_binding_for_another_device() {
        let root = tempfile::tempdir().unwrap();
        let expected = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let signer =
            LocalPersonSigner::load_or_create(root.path(), &device_public_key(expected.path()))
                .unwrap();

        let state = signer
            .request(PersonSignerRequest {
                unsigned_event_template: template(other.path(), 100),
            })
            .await;

        assert!(matches!(state, PersonSignerState::InvalidResponse { .. }));
    }

    #[test]
    fn interrupted_private_temporary_record_never_becomes_the_person_key() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("identity");
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let interrupted = directory.join(".person.key.interrupted.tmp");
        fs::write(&interrupted, b"incomplete").unwrap();
        fs::set_permissions(&interrupted, fs::Permissions::from_mode(0o600)).unwrap();

        let device = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device.path());
        let first = LocalPersonSigner::load_or_create(root.path(), &expected_device).unwrap();
        let owner = first.keys.public_key().to_hex();
        let complete = fs::read_to_string(directory.join("person.key")).unwrap();
        assert_eq!(complete.len(), 65);
        assert!(complete.ends_with('\n'));
        assert!(!interrupted.exists());
        assert_eq!(
            LocalPersonSigner::load_or_create(root.path(), &expected_device)
                .unwrap()
                .keys
                .public_key()
                .to_hex(),
            owner
        );
    }

    #[test]
    fn concurrent_key_creation_process_helper() {
        let Some(root) = std::env::var_os("KORRI_SIGNER_RACE_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        while !root.join("go").exists() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let expected_device = std::env::var("KORRI_SIGNER_RACE_DEVICE").unwrap();
        let signer = LocalPersonSigner::load_or_create(&root, &expected_device).unwrap();
        println!(
            "automatic-owner-public-key={}",
            signer.keys.public_key().to_hex()
        );
    }

    #[test]
    fn first_key_creation_is_atomic_across_processes() {
        use std::process::{Command, Stdio};

        let root = tempfile::tempdir().unwrap();
        let executable = std::env::current_exe().unwrap();
        let device = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device.path());
        let mut children = (0..8)
            .map(|_| {
                Command::new(&executable)
                    .args([
                        "--exact",
                        "local_signer::tests::concurrent_key_creation_process_helper",
                        "--nocapture",
                    ])
                    .env("KORRI_SIGNER_RACE_ROOT", root.path())
                    .env("KORRI_SIGNER_RACE_DEVICE", &expected_device)
                    .stdout(Stdio::piped())
                    .spawn()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        fs::write(root.path().join("go"), b"").unwrap();

        let mut owners = Vec::new();
        for child in children.drain(..) {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            owners.push(
                stdout
                    .lines()
                    .find_map(|line| line.strip_prefix("automatic-owner-public-key="))
                    .unwrap()
                    .to_owned(),
            );
        }
        assert!(owners.iter().all(|owner| owner == &owners[0]));
        let final_record = fs::read_to_string(root.path().join("identity/person.key")).unwrap();
        assert_eq!(final_record.len(), 65);
        assert!(final_record.ends_with('\n'));
        assert_eq!(
            fs::read_dir(root.path().join("identity"))
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn active_local_key_exports_and_imports_as_a_non_persisted_nip49_signer() {
        let signer_root = tempfile::tempdir().unwrap();
        let device_root = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device_root.path());
        let active =
            LocalPersonSigner::load_or_create(signer_root.path(), &expected_device).unwrap();

        let encrypted = active.export_nip49("backup password").unwrap();
        assert!(encrypted.starts_with("ncryptsec1"));
        let imported =
            LocalPersonSigner::import_nip49(&encrypted, "backup password", &expected_device)
                .unwrap();
        assert_eq!(imported.public_key(), active.public_key());

        let unsigned_event_template = template(device_root.path(), 100);
        let state = imported
            .request(PersonSignerRequest {
                unsigned_event_template: unsigned_event_template.clone(),
            })
            .await;
        let PersonSignerState::Approved {
            owner_public_key,
            unsigned_event_template: returned_template,
            signed_event_json,
        } = state
        else {
            panic!("imported signer did not approve the owner binding")
        };
        assert_eq!(owner_public_key, active.public_key());
        assert_eq!(returned_template, unsigned_event_template);
        DeviceIdentity::load_or_create(device_root.path())
            .unwrap()
            .apply_signed_owner_binding(&returned_template, &owner_public_key, &signed_event_json)
            .unwrap();

        let identity_entries = fs::read_dir(signer_root.path().join("identity"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            identity_entries,
            vec![std::ffi::OsString::from("person.key")]
        );
    }

    #[test]
    fn invalid_nip49_inputs_return_explicit_errors_without_changing_private_state() {
        let signer_root = tempfile::tempdir().unwrap();
        let device_root = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device_root.path());
        let active =
            LocalPersonSigner::load_or_create(signer_root.path(), &expected_device).unwrap();
        let before = fs::read(signer_root.path().join("identity/person.key")).unwrap();
        let encrypted = active.export_nip49("correct password").unwrap();

        assert!(matches!(
            active.export_nip49(" \t\n"),
            Err(LocalSignerError::InvalidPassword)
        ));
        assert!(matches!(
            LocalPersonSigner::import_nip49(&encrypted, "", &expected_device),
            Err(LocalSignerError::InvalidPassword)
        ));
        assert!(matches!(
            LocalPersonSigner::import_nip49(
                "not-a-nip49-encrypted-secret",
                "correct password",
                &expected_device,
            ),
            Err(LocalSignerError::MalformedEncryptedSecret)
        ));
        assert!(matches!(
            LocalPersonSigner::import_nip49(&encrypted, "wrong password", &expected_device),
            Err(LocalSignerError::IncorrectPassword)
        ));

        assert_eq!(
            fs::read(signer_root.path().join("identity/person.key")).unwrap(),
            before
        );
        assert_eq!(
            fs::read_dir(signer_root.path().join("identity"))
                .unwrap()
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn unix_adapter_carries_only_the_existing_person_signer_request_and_state() {
        let signer_root = tempfile::tempdir().unwrap();
        let device_root = tempfile::tempdir().unwrap();
        let socket_root = tempfile::tempdir().unwrap();
        let socket = socket_root.path().join("signer.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let signer = Arc::new(
            LocalPersonSigner::load_or_create(
                signer_root.path(),
                &device_public_key(device_root.path()),
            )
            .unwrap(),
        );
        let server = {
            let signer = signer.clone();
            tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                super::serve_connection(stream, signer).await.unwrap();
            })
        };
        let client = UnixPersonSigner::new(socket);

        let state = client
            .request(PersonSignerRequest {
                unsigned_event_template: template(device_root.path(), 100),
            })
            .await;
        server.await.unwrap();

        assert!(matches!(state, PersonSignerState::Approved { .. }));
        assert_eq!(client.state(), state);
    }
}
