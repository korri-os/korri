//! Local person signer and its Unix-socket `PersonSigner` adapter.

use crate::{
    identity::validate_signable_owner_statement_template,
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
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zeroize::Zeroizing;

const IDENTITY_DIRECTORY: &str = "identity";
const ACTIVE_DIRECTORY: &str = "active";
const RETIRED_DIRECTORY: &str = "retired";
const PERSON_KEY_FILE: &str = "person.key";
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
static SIGNER_STORAGE: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
pub enum LocalSignerError {
    #[error("local signer storage is unavailable")]
    Storage,
    #[error("local signer key is invalid")]
    InvalidKey,
    #[error("local signer has no active local key")]
    NoActiveKey,
    #[error("legacy local signer storage requires the explicit migration tool")]
    LegacyKey,
    #[error("local signer key conflicts with the requested keyring operation")]
    KeyConflict,
    #[error("local signer retired key was not found")]
    RetiredKeyNotFound,
    #[error("password must not be blank")]
    InvalidPassword,
    #[error("NIP-49 encrypted secret is malformed")]
    MalformedEncryptedSecret,
    #[error("NIP-49 encrypted secret could not be decrypted with this password")]
    IncorrectPassword,
    #[error("local signer could not encrypt the person key")]
    Encryption,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalSignerKeyringStatus {
    Active { public_key: String },
    Remote,
}

#[derive(Clone, Debug)]
pub struct LocalSignerKeyring {
    private_state_root: PathBuf,
    identity_directory: PathBuf,
    active_directory: PathBuf,
    retired_directory: PathBuf,
}

pub struct LocalPersonSigner {
    keys: Keys,
    expected_device_public_key: String,
    state: Mutex<PersonSignerState>,
}

impl LocalSignerKeyring {
    pub fn open_or_initialize(private_state_root: &Path) -> Result<Self, LocalSignerError> {
        let _storage = storage_lock();
        prepare_private_state_root(private_state_root)?;
        let _process_lock = lock_directory(private_state_root)?;
        cleanup_identity_temporaries(private_state_root)?;
        let identity_directory = private_state_root.join(IDENTITY_DIRECTORY);
        if path_kind(&identity_directory)?.is_none() {
            initialize_keyring(private_state_root, &identity_directory)?;
        }
        let keyring = Self {
            private_state_root: private_state_root.to_path_buf(),
            active_directory: identity_directory.join(ACTIVE_DIRECTORY),
            retired_directory: identity_directory.join(RETIRED_DIRECTORY),
            identity_directory,
        };
        keyring.validate_top_level()?;
        Ok(keyring)
    }

    pub fn status(&self) -> Result<LocalSignerKeyringStatus, LocalSignerError> {
        self.with_lock(|keyring| {
            keyring.validate_complete_layout()?;
            match keyring.read_active_keys()? {
                Some(keys) => Ok(LocalSignerKeyringStatus::Active {
                    public_key: keys.public_key().to_hex(),
                }),
                None => Ok(LocalSignerKeyringStatus::Remote),
            }
        })
    }

    pub fn retired_public_keys(&self) -> Result<Vec<String>, LocalSignerError> {
        self.with_lock(|keyring| {
            keyring.validate_complete_layout()?;
            let mut keys = keyring.retired_entries()?;
            keys.sort();
            Ok(keys)
        })
    }

    pub fn persist_inactive(&self, signer: &LocalPersonSigner) -> Result<String, LocalSignerError> {
        let public_key = signer.public_key();
        self.with_lock(|keyring| {
            keyring.validate_layout_except(Some(&public_key))?;
            if keyring
                .read_active_keys()?
                .is_some_and(|active| active.public_key().to_hex() == public_key)
            {
                return Err(LocalSignerError::KeyConflict);
            }
            let directory = keyring.prepare_retired_key_directory(&public_key)?;
            let path = directory.join(PERSON_KEY_FILE);
            let encoded = encode_keys(&signer.keys);
            match read_private_optional(&path, &keyring.private_state_root)? {
                Some(bytes) => {
                    let existing = parse_private_key_record(bytes)?;
                    if existing.public_key().to_hex() != public_key {
                        return Err(LocalSignerError::KeyConflict);
                    }
                }
                None => {
                    write_new_private(&path, &encoded, &keyring.private_state_root)?;
                }
            }
            keyring.verify_retired_key(&public_key)?;
            Ok(public_key)
        })
    }

    pub fn inspect_inactive(&self, public_key: &str) -> Result<String, LocalSignerError> {
        let public_key = parse_public_key_text(public_key)?;
        self.with_lock(|keyring| {
            keyring.validate_complete_layout()?;
            keyring.verify_retired_key(&public_key)?;
            Ok(public_key)
        })
    }

    pub fn inactive_signer(
        &self,
        public_key: &str,
        expected_device_public_key: &str,
    ) -> Result<LocalPersonSigner, LocalSignerError> {
        let public_key = parse_public_key_text(public_key)?;
        let expected_device_public_key = parse_public_key_text(expected_device_public_key)?;
        self.with_lock(|keyring| {
            keyring.validate_complete_layout()?;
            let keys = keyring.read_retired_keys(&public_key)?;
            Ok(LocalPersonSigner {
                keys,
                expected_device_public_key,
                state: Mutex::new(PersonSignerState::Unavailable {
                    message: "Local signer is ready".into(),
                }),
            })
        })
    }

    pub fn export_retired_nip49(
        &self,
        public_key: &str,
        password: &str,
    ) -> Result<String, LocalSignerError> {
        validate_password(password)?;
        let public_key = parse_public_key_text(public_key)?;
        self.with_lock(|keyring| {
            keyring.validate_complete_layout()?;
            keyring
                .read_retired_keys(&public_key)?
                .secret_key()
                .encrypt(password)
                .and_then(|encrypted| encrypted.to_bech32())
                .map_err(|_| LocalSignerError::Encryption)
        })
    }

    pub fn promote_retired(&self, public_key: &str) -> Result<(), LocalSignerError> {
        let public_key = parse_public_key_text(public_key)?;
        self.with_lock(|keyring| {
            keyring.validate_layout_except(Some(&public_key))?;
            keyring.promote_retired_locked(&public_key)
        })
    }

    pub fn retire_active(
        &self,
        expected_active_public_key: &str,
        replacement_public_key: Option<&str>,
    ) -> Result<(), LocalSignerError> {
        let expected = parse_public_key_text(expected_active_public_key)?;
        let replacement = replacement_public_key
            .map(parse_public_key_text)
            .transpose()?;
        if replacement.as_deref() == Some(expected.as_str()) {
            return Err(LocalSignerError::KeyConflict);
        }
        self.with_lock(|keyring| {
            keyring.validate_layout_except(Some(&expected))?;
            match keyring.read_active_keys()? {
                Some(active) if active.public_key().to_hex() == expected => {
                    if let Some(replacement) = replacement.as_deref() {
                        keyring.validate_retired_target(replacement)?;
                    }
                    let directory = keyring.prepare_retired_key_directory(&expected)?;
                    let retired_path = directory.join(PERSON_KEY_FILE);
                    let encoded = encode_keys(&active);
                    match read_private_optional(&retired_path, &keyring.private_state_root)? {
                        Some(bytes) => {
                            if parse_private_key_record(bytes)?.public_key().to_hex() != expected {
                                return Err(LocalSignerError::KeyConflict);
                            }
                        }
                        None => {
                            write_new_private(
                                &retired_path,
                                &encoded,
                                &keyring.private_state_root,
                            )?;
                        }
                    }
                    keyring.verify_retired_key(&expected)?;
                    fs::remove_file(keyring.active_key_path())
                        .map_err(|_| LocalSignerError::Storage)?;
                    sync_directory(&keyring.active_directory)?;
                }
                Some(active) => {
                    let active_public_key = active.public_key().to_hex();
                    if replacement.as_deref() != Some(active_public_key.as_str()) {
                        return Err(LocalSignerError::KeyConflict);
                    }
                    keyring.verify_retired_key(&expected)?;
                    keyring.promote_retired_locked(&active_public_key)?;
                    return Ok(());
                }
                None => {
                    keyring.verify_retired_key(&expected)?;
                    if let Some(replacement) = replacement.as_deref() {
                        keyring.validate_retired_target(replacement)?;
                    }
                }
            }
            if let Some(replacement) = replacement.as_deref() {
                keyring.promote_retired_locked(replacement)?;
            }
            Ok(())
        })
    }

    pub fn rollback_inactive(&self, public_key: &str) -> Result<(), LocalSignerError> {
        self.remove_retired(public_key, true)
    }

    pub fn delete_retired(&self, public_key: &str) -> Result<(), LocalSignerError> {
        self.remove_retired(public_key, false)
    }

    fn active_keys(&self) -> Result<Option<Keys>, LocalSignerError> {
        self.with_lock(|keyring| {
            keyring.validate_complete_layout()?;
            keyring.read_active_keys()
        })
    }

    fn remove_retired(
        &self,
        public_key: &str,
        allow_empty_staging_directory: bool,
    ) -> Result<(), LocalSignerError> {
        let public_key = parse_public_key_text(public_key)?;
        self.with_lock(|keyring| {
            keyring.validate_layout_except(Some(&public_key))?;
            if keyring
                .read_active_keys()?
                .is_some_and(|active| active.public_key().to_hex() == public_key)
            {
                return Err(LocalSignerError::KeyConflict);
            }
            let directory = keyring.retired_directory.join(&public_key);
            if path_kind(&directory)?.is_none() {
                return Ok(());
            }
            validate_private_directory(&directory, &keyring.private_state_root)?;
            cleanup_private_temporaries(&directory)?;
            if fs::read_dir(&directory)
                .map_err(|_| LocalSignerError::Storage)?
                .next()
                .is_none()
            {
                if !allow_empty_staging_directory {
                    return Err(LocalSignerError::RetiredKeyNotFound);
                }
            } else {
                keyring.verify_retired_key(&public_key)?;
                fs::remove_file(directory.join(PERSON_KEY_FILE))
                    .map_err(|_| LocalSignerError::Storage)?;
                sync_directory(&directory)?;
            }
            fs::remove_dir(&directory).map_err(|_| LocalSignerError::Storage)?;
            sync_directory(&keyring.retired_directory)
        })
    }

    fn with_lock<T>(
        &self,
        operation: impl FnOnce(&Self) -> Result<T, LocalSignerError>,
    ) -> Result<T, LocalSignerError> {
        let _storage = storage_lock();
        let _process_lock = lock_directory(&self.private_state_root)?;
        self.validate_top_level()?;
        operation(self)
    }

    fn validate_top_level(&self) -> Result<(), LocalSignerError> {
        validate_private_directory(&self.identity_directory, &self.private_state_root)?;
        if path_kind(&self.identity_directory.join(PERSON_KEY_FILE))?.is_some() {
            return Err(LocalSignerError::LegacyKey);
        }
        let mut seen_active = false;
        let mut seen_retired = false;
        for entry in
            fs::read_dir(&self.identity_directory).map_err(|_| LocalSignerError::Storage)?
        {
            let entry = entry.map_err(|_| LocalSignerError::Storage)?;
            match entry.file_name().to_str() {
                Some(ACTIVE_DIRECTORY) => seen_active = true,
                Some(RETIRED_DIRECTORY) => seen_retired = true,
                Some(PERSON_KEY_FILE) => return Err(LocalSignerError::LegacyKey),
                _ => return Err(LocalSignerError::Storage),
            }
        }
        if !seen_active || !seen_retired {
            return Err(LocalSignerError::Storage);
        }
        validate_private_directory(&self.active_directory, &self.private_state_root)?;
        validate_private_directory(&self.retired_directory, &self.private_state_root)
    }

    fn validate_complete_layout(&self) -> Result<(), LocalSignerError> {
        self.validate_layout_except(None)?;
        if let Some(active) = self.read_active_keys()? {
            let active_public_key = active.public_key().to_hex();
            if path_kind(&self.retired_directory.join(active_public_key))?.is_some() {
                return Err(LocalSignerError::Storage);
            }
        }
        Ok(())
    }

    fn validate_layout_except(&self, target: Option<&str>) -> Result<(), LocalSignerError> {
        cleanup_private_temporaries(&self.active_directory)?;
        validate_active_directory(self)?;
        for entry in fs::read_dir(&self.retired_directory).map_err(|_| LocalSignerError::Storage)? {
            let entry = entry.map_err(|_| LocalSignerError::Storage)?;
            let name = entry
                .file_name()
                .to_str()
                .ok_or(LocalSignerError::Storage)?
                .to_owned();
            parse_public_key_text(&name)?;
            if target == Some(name.as_str()) {
                let metadata =
                    fs::symlink_metadata(entry.path()).map_err(|_| LocalSignerError::Storage)?;
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(LocalSignerError::Storage);
                }
                validate_private_directory(&entry.path(), &self.private_state_root)?;
                cleanup_private_temporaries(&entry.path())?;
                continue;
            }
            self.verify_retired_key(&name)?;
        }
        Ok(())
    }

    fn retired_entries(&self) -> Result<Vec<String>, LocalSignerError> {
        fs::read_dir(&self.retired_directory)
            .map_err(|_| LocalSignerError::Storage)?
            .map(|entry| {
                entry
                    .map_err(|_| LocalSignerError::Storage)?
                    .file_name()
                    .into_string()
                    .map_err(|_| LocalSignerError::Storage)
            })
            .collect()
    }

    fn active_key_path(&self) -> PathBuf {
        self.active_directory.join(PERSON_KEY_FILE)
    }

    fn read_active_keys(&self) -> Result<Option<Keys>, LocalSignerError> {
        read_private_optional(&self.active_key_path(), &self.private_state_root)?
            .map(parse_private_key_record)
            .transpose()
    }

    fn read_retired_keys(&self, public_key: &str) -> Result<Keys, LocalSignerError> {
        let path = self
            .retired_directory
            .join(public_key)
            .join(PERSON_KEY_FILE);
        let keys = read_private_optional(&path, &self.private_state_root)?
            .ok_or(LocalSignerError::RetiredKeyNotFound)
            .and_then(parse_private_key_record)?;
        if keys.public_key().to_hex() != public_key {
            return Err(LocalSignerError::KeyConflict);
        }
        Ok(keys)
    }

    fn verify_retired_key(&self, public_key: &str) -> Result<(), LocalSignerError> {
        let directory = self.retired_directory.join(public_key);
        validate_private_directory(&directory, &self.private_state_root)?;
        cleanup_private_temporaries(&directory)?;
        let mut entries = fs::read_dir(&directory).map_err(|_| LocalSignerError::Storage)?;
        let entry = entries
            .next()
            .transpose()
            .map_err(|_| LocalSignerError::Storage)?
            .ok_or(LocalSignerError::RetiredKeyNotFound)?;
        if entry.file_name() != PERSON_KEY_FILE || entries.next().is_some() {
            return Err(LocalSignerError::Storage);
        }
        self.read_retired_keys(public_key).map(|_| ())
    }

    fn validate_retired_target(&self, public_key: &str) -> Result<(), LocalSignerError> {
        self.verify_retired_key(public_key)
    }

    fn prepare_retired_key_directory(&self, public_key: &str) -> Result<PathBuf, LocalSignerError> {
        let directory = self.retired_directory.join(public_key);
        match path_kind(&directory)? {
            None => {
                DirBuilder::new()
                    .mode(0o700)
                    .create(&directory)
                    .map_err(|_| LocalSignerError::Storage)?;
                sync_directory(&self.retired_directory)?;
            }
            Some(PathKind::Directory) => {
                validate_private_directory(&directory, &self.private_state_root)?;
                cleanup_private_temporaries(&directory)?;
            }
            Some(_) => return Err(LocalSignerError::Storage),
        }
        Ok(directory)
    }

    fn promote_retired_locked(&self, public_key: &str) -> Result<(), LocalSignerError> {
        let retired_directory = self.retired_directory.join(public_key);
        let retired_path = retired_directory.join(PERSON_KEY_FILE);
        match self.read_active_keys()? {
            Some(active) if active.public_key().to_hex() == public_key => {
                if path_kind(&retired_directory)?.is_none() {
                    return Ok(());
                }
                self.verify_retired_key(public_key)?;
            }
            Some(_) => return Err(LocalSignerError::KeyConflict),
            None => {
                self.verify_retired_key(public_key)?;
                match fs::hard_link(&retired_path, self.active_key_path()) {
                    Ok(()) => sync_directory(&self.active_directory)?,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        let active = self.read_active_keys()?.ok_or(LocalSignerError::Storage)?;
                        if active.public_key().to_hex() != public_key {
                            return Err(LocalSignerError::KeyConflict);
                        }
                    }
                    Err(_) => return Err(LocalSignerError::Storage),
                }
            }
        }
        if path_kind(&retired_path)?.is_some() {
            fs::remove_file(&retired_path).map_err(|_| LocalSignerError::Storage)?;
            sync_directory(&retired_directory)?;
        }
        match fs::remove_dir(&retired_directory) {
            Ok(()) => sync_directory(&self.retired_directory),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(LocalSignerError::Storage),
        }
    }
}

impl LocalPersonSigner {
    /// Load the fixed automatic person key or initialize the complete keyring once.
    ///
    /// An existing keyring without `active/person.key` represents remote
    /// ownership. Runtime never generates a replacement key in that state.
    pub fn load_or_create(
        private_state_root: &Path,
        expected_device_public_key: &str,
    ) -> Result<Self, LocalSignerError> {
        let expected_device_public_key = parse_public_key_text(expected_device_public_key)?;
        let keyring = LocalSignerKeyring::open_or_initialize(private_state_root)?;
        let keys = keyring
            .active_keys()?
            .ok_or(LocalSignerError::NoActiveKey)?;
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
        let template = validate_signable_owner_statement_template(template).map_err(|_| ())?;
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

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(tag = "_tag", content = "payload", deny_unknown_fields)]
pub enum LocalSignerWireRequest {
    Sign(PersonSignerRequest),
    ExportActive {
        password: String,
    },
    StageImport {
        encrypted_secret: String,
        password: String,
        expected_device_public_key: String,
    },
    SignInactive {
        public_key: String,
        expected_device_public_key: String,
        request: PersonSignerRequest,
    },
    Activate {
        expected_active_public_key: Option<String>,
        replacement_public_key: Option<String>,
        new_device_public_key: String,
        new_owner_event_json: String,
    },
    ExportRetired {
        public_key: String,
        password: String,
    },
    DeleteRetired {
        public_key: String,
    },
    RollbackInactive {
        public_key: String,
    },
    Status,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(tag = "_tag", content = "payload", deny_unknown_fields)]
pub enum LocalSignerWireResponse {
    Sign(PersonSignerState),
    Exported {
        encrypted_secret: String,
    },
    Staged {
        public_key: String,
    },
    Status {
        active_public_key: Option<String>,
        retired_public_keys: Vec<String>,
    },
    Ok,
    Error {
        message: String,
    },
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
            let next = match request_over_unix(&self.socket, &LocalSignerWireRequest::Sign(request))
                .await
            {
                Ok(LocalSignerWireResponse::Sign(state)) => state,
                Ok(LocalSignerWireResponse::Error { message }) | Err(message) => {
                    PersonSignerState::Defect { message }
                }
                Ok(_) => PersonSignerState::InvalidResponse {
                    message: "Local signer returned the wrong response".into(),
                },
            };
            *self.state.lock().expect("local signer state poisoned") = next.clone();
            next
        })
    }
}

#[derive(Clone)]
pub struct UnixLocalSignerAdmin {
    socket: PathBuf,
}

impl UnixLocalSignerAdmin {
    pub fn new(socket: PathBuf) -> Self {
        Self { socket }
    }

    pub async fn export_active(&self, password: String) -> Result<String, String> {
        match request_over_unix(
            &self.socket,
            &LocalSignerWireRequest::ExportActive { password },
        )
        .await?
        {
            LocalSignerWireResponse::Exported { encrypted_secret } => Ok(encrypted_secret),
            LocalSignerWireResponse::Error { message } => Err(message),
            _ => Err("Local signer returned the wrong response".into()),
        }
    }

    pub async fn stage_import(
        &self,
        encrypted_secret: String,
        password: String,
        expected_device_public_key: String,
    ) -> Result<String, String> {
        match request_over_unix(
            &self.socket,
            &LocalSignerWireRequest::StageImport {
                encrypted_secret,
                password,
                expected_device_public_key,
            },
        )
        .await?
        {
            LocalSignerWireResponse::Staged { public_key } => Ok(public_key),
            LocalSignerWireResponse::Error { message } => Err(message),
            _ => Err("Local signer returned the wrong response".into()),
        }
    }

    pub async fn sign_inactive(
        &self,
        public_key: String,
        expected_device_public_key: String,
        request: PersonSignerRequest,
    ) -> Result<PersonSignerState, String> {
        match request_over_unix(
            &self.socket,
            &LocalSignerWireRequest::SignInactive {
                public_key,
                expected_device_public_key,
                request,
            },
        )
        .await?
        {
            LocalSignerWireResponse::Sign(state) => Ok(state),
            LocalSignerWireResponse::Error { message } => Err(message),
            _ => Err("Local signer returned the wrong response".into()),
        }
    }

    pub async fn activate(
        &self,
        expected_active_public_key: Option<String>,
        replacement_public_key: Option<String>,
        new_device_public_key: String,
        new_owner_event_json: String,
    ) -> Result<(), String> {
        self.expect_ok(LocalSignerWireRequest::Activate {
            expected_active_public_key,
            replacement_public_key,
            new_device_public_key,
            new_owner_event_json,
        })
        .await
    }

    pub async fn export_retired(
        &self,
        public_key: String,
        password: String,
    ) -> Result<String, String> {
        match request_over_unix(
            &self.socket,
            &LocalSignerWireRequest::ExportRetired {
                public_key,
                password,
            },
        )
        .await?
        {
            LocalSignerWireResponse::Exported { encrypted_secret } => Ok(encrypted_secret),
            LocalSignerWireResponse::Error { message } => Err(message),
            _ => Err("Local signer returned the wrong response".into()),
        }
    }

    pub async fn delete_retired(&self, public_key: String) -> Result<(), String> {
        self.expect_ok(LocalSignerWireRequest::DeleteRetired { public_key })
            .await
    }

    pub async fn rollback_inactive(&self, public_key: String) -> Result<(), String> {
        self.expect_ok(LocalSignerWireRequest::RollbackInactive { public_key })
            .await
    }

    pub async fn status(&self) -> Result<(Option<String>, Vec<String>), String> {
        match request_over_unix(&self.socket, &LocalSignerWireRequest::Status).await? {
            LocalSignerWireResponse::Status {
                active_public_key,
                retired_public_keys,
            } => Ok((active_public_key, retired_public_keys)),
            LocalSignerWireResponse::Error { message } => Err(message),
            _ => Err("Local signer returned the wrong response".into()),
        }
    }

    async fn expect_ok(&self, request: LocalSignerWireRequest) -> Result<(), String> {
        match request_over_unix(&self.socket, &request).await? {
            LocalSignerWireResponse::Ok => Ok(()),
            LocalSignerWireResponse::Error { message } => Err(message),
            _ => Err("Local signer returned the wrong response".into()),
        }
    }
}

pub async fn serve_connection(
    mut stream: tokio::net::UnixStream,
    private_state_root: PathBuf,
    expected_device_public_key: Arc<Mutex<String>>,
    public_key_path: PathBuf,
) -> Result<(), std::io::Error> {
    let mut bytes = Vec::new();
    (&mut stream)
        .take((MAX_MESSAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    let response = if bytes.len() > MAX_MESSAGE_BYTES {
        LocalSignerWireResponse::Error {
            message: "Local signer request is too large".into(),
        }
    } else {
        match serde_json::from_slice::<LocalSignerWireRequest>(&bytes) {
            Ok(request) => {
                handle_wire_request(
                    &private_state_root,
                    &expected_device_public_key,
                    &public_key_path,
                    request,
                )
                .await
            }
            Err(_) => LocalSignerWireResponse::Error {
                message: "Local signer request is invalid".into(),
            },
        }
    };
    let response = serde_json::to_vec(&response).map_err(std::io::Error::other)?;
    stream.write_all(&response).await?;
    stream.shutdown().await
}

async fn handle_wire_request(
    private_state_root: &Path,
    expected_device_public_key: &Arc<Mutex<String>>,
    public_key_path: &Path,
    request: LocalSignerWireRequest,
) -> LocalSignerWireResponse {
    let result: Result<LocalSignerWireResponse, String> = async {
        let keyring = LocalSignerKeyring::open_or_initialize(private_state_root)
            .map_err(|error| error.to_string())?;
        match request {
            LocalSignerWireRequest::Sign(request) => {
                let expected = expected_device_public_key
                    .lock()
                    .map_err(|_| "Local signer state is unavailable".to_owned())?
                    .clone();
                let signer = LocalPersonSigner::load_or_create(private_state_root, &expected)
                    .map_err(|error| error.to_string())?;
                Ok(LocalSignerWireResponse::Sign(signer.request(request).await))
            }
            LocalSignerWireRequest::ExportActive { password } => {
                let expected = expected_device_public_key
                    .lock()
                    .map_err(|_| "Local signer state is unavailable".to_owned())?
                    .clone();
                let signer = LocalPersonSigner::load_or_create(private_state_root, &expected)
                    .map_err(|error| error.to_string())?;
                let encrypted_secret = signer
                    .export_nip49(&password)
                    .map_err(|error| error.to_string())?;
                Ok(LocalSignerWireResponse::Exported { encrypted_secret })
            }
            LocalSignerWireRequest::StageImport {
                encrypted_secret,
                password,
                expected_device_public_key,
            } => {
                let signer = LocalPersonSigner::import_nip49(
                    &encrypted_secret,
                    &password,
                    &expected_device_public_key,
                )
                .map_err(|error| error.to_string())?;
                let public_key = keyring
                    .persist_inactive(&signer)
                    .map_err(|error| error.to_string())?;
                Ok(LocalSignerWireResponse::Staged { public_key })
            }
            LocalSignerWireRequest::SignInactive {
                public_key,
                expected_device_public_key,
                request,
            } => {
                let signer = keyring
                    .inactive_signer(&public_key, &expected_device_public_key)
                    .map_err(|error| error.to_string())?;
                Ok(LocalSignerWireResponse::Sign(signer.request(request).await))
            }
            LocalSignerWireRequest::Activate {
                expected_active_public_key,
                replacement_public_key,
                new_device_public_key,
                new_owner_event_json,
            } => {
                let statement = crate::identity::DeviceIdentity::verify_owner_statement(
                    &new_owner_event_json,
                    &new_device_public_key,
                )
                .map_err(|error| error.to_string())?;
                if statement.status != crate::identity::OwnerStatementStatus::Owned
                    || replacement_public_key
                        .as_deref()
                        .is_some_and(|key| key != statement.owner_public_key)
                {
                    return Err("Replacement owner evidence does not match the signer".into());
                }
                if let Some(expected_active_public_key) = expected_active_public_key {
                    keyring
                        .retire_active(
                            &expected_active_public_key,
                            replacement_public_key.as_deref(),
                        )
                        .map_err(|error| error.to_string())?;
                } else {
                    match (
                        keyring.status().map_err(|error| error.to_string())?,
                        replacement_public_key.as_deref(),
                    ) {
                        (LocalSignerKeyringStatus::Remote, Some(replacement)) => keyring
                            .promote_retired(replacement)
                            .map_err(|error| error.to_string())?,
                        (LocalSignerKeyringStatus::Remote, None) => {}
                        (LocalSignerKeyringStatus::Active { public_key }, Some(replacement))
                            if public_key == replacement => {}
                        _ => {
                            return Err(
                                "Local signer state does not match the identity switch".into()
                            )
                        }
                    }
                }
                *expected_device_public_key
                    .lock()
                    .map_err(|_| "Local signer state is unavailable".to_owned())? =
                    new_device_public_key;
                if let Some(public_key) = replacement_public_key {
                    publish_public_key(public_key_path, &public_key)
                        .map_err(|error| error.to_string())?;
                }
                Ok(LocalSignerWireResponse::Ok)
            }
            LocalSignerWireRequest::ExportRetired {
                public_key,
                password,
            } => {
                let encrypted_secret = keyring
                    .export_retired_nip49(&public_key, &password)
                    .map_err(|error| error.to_string())?;
                Ok(LocalSignerWireResponse::Exported { encrypted_secret })
            }
            LocalSignerWireRequest::DeleteRetired { public_key } => {
                keyring
                    .delete_retired(&public_key)
                    .map_err(|error| error.to_string())?;
                Ok(LocalSignerWireResponse::Ok)
            }
            LocalSignerWireRequest::RollbackInactive { public_key } => {
                keyring
                    .rollback_inactive(&public_key)
                    .map_err(|error| error.to_string())?;
                Ok(LocalSignerWireResponse::Ok)
            }
            LocalSignerWireRequest::Status => {
                let active_public_key = match keyring.status().map_err(|error| error.to_string())? {
                    LocalSignerKeyringStatus::Active { public_key } => Some(public_key),
                    LocalSignerKeyringStatus::Remote => None,
                };
                let retired_public_keys = keyring
                    .retired_public_keys()
                    .map_err(|error| error.to_string())?;
                Ok(LocalSignerWireResponse::Status {
                    active_public_key,
                    retired_public_keys,
                })
            }
        }
    }
    .await;
    result.unwrap_or_else(|message| LocalSignerWireResponse::Error { message })
}

async fn request_over_unix(
    socket: &Path,
    request: &LocalSignerWireRequest,
) -> Result<LocalSignerWireResponse, String> {
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

pub fn read_published_public_key_if_present(
    path: &Path,
) -> Result<Option<String>, LocalSignerError> {
    match fs::symlink_metadata(path) {
        Ok(_) => read_published_public_key(path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(LocalSignerError::Storage),
    }
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

fn storage_lock() -> std::sync::MutexGuard<'static, ()> {
    SIGNER_STORAGE
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("local signer storage mutex poisoned")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PathKind {
    File,
    Directory,
    Symlink,
    Other,
}

fn path_kind(path: &Path) -> Result<Option<PathKind>, LocalSignerError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(LocalSignerError::Storage),
    };
    Ok(Some(if metadata.file_type().is_symlink() {
        PathKind::Symlink
    } else if metadata.is_file() {
        PathKind::File
    } else if metadata.is_dir() {
        PathKind::Directory
    } else {
        PathKind::Other
    }))
}

fn prepare_private_state_root(private_state_root: &Path) -> Result<(), LocalSignerError> {
    let mut created = false;
    if path_kind(private_state_root)?.is_none() {
        match DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(private_state_root)
        {
            Ok(()) => created = true,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(LocalSignerError::Storage),
        }
    }
    validate_directory(private_state_root)?;
    if created {
        sync_directory(private_state_root)?;
        if let Some(parent) = private_state_root.parent().filter(|parent| parent.exists()) {
            sync_directory(parent)?;
        }
    }
    Ok(())
}

fn initialize_keyring(
    private_state_root: &Path,
    identity_directory: &Path,
) -> Result<(), LocalSignerError> {
    let temporary = private_state_root.join(format!(
        ".{IDENTITY_DIRECTORY}.{}.tmp",
        rand::random::<u64>()
    ));
    let result = (|| {
        DirBuilder::new()
            .mode(0o700)
            .create(&temporary)
            .map_err(|_| LocalSignerError::Storage)?;
        let active = temporary.join(ACTIVE_DIRECTORY);
        let retired = temporary.join(RETIRED_DIRECTORY);
        DirBuilder::new()
            .mode(0o700)
            .create(&active)
            .map_err(|_| LocalSignerError::Storage)?;
        DirBuilder::new()
            .mode(0o700)
            .create(&retired)
            .map_err(|_| LocalSignerError::Storage)?;
        let keys = Keys::generate();
        let encoded = encode_keys(&keys);
        write_unpublished_private(&active.join(PERSON_KEY_FILE), &encoded)?;
        sync_directory(&active)?;
        sync_directory(&retired)?;
        sync_directory(&temporary)?;
        fs::rename(&temporary, identity_directory).map_err(|_| LocalSignerError::Storage)?;
        sync_directory(private_state_root)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

fn cleanup_identity_temporaries(private_state_root: &Path) -> Result<(), LocalSignerError> {
    let prefix = format!(".{IDENTITY_DIRECTORY}.");
    let owner = fs::symlink_metadata(private_state_root).map_err(|_| LocalSignerError::Storage)?;
    let mut changed = false;
    for entry in fs::read_dir(private_state_root).map_err(|_| LocalSignerError::Storage)? {
        let entry = entry.map_err(|_| LocalSignerError::Storage)?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(LocalSignerError::Storage)?;
        if name.starts_with(&prefix) && name.ends_with(".tmp") {
            let metadata =
                fs::symlink_metadata(entry.path()).map_err(|_| LocalSignerError::Storage)?;
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || metadata.permissions().mode() & 0o077 != 0
                || metadata.uid() != owner.uid()
                || metadata.gid() != owner.gid()
            {
                return Err(LocalSignerError::Storage);
            }
            fs::remove_dir_all(entry.path()).map_err(|_| LocalSignerError::Storage)?;
            changed = true;
        }
    }
    if changed {
        sync_directory(private_state_root)?;
    }
    Ok(())
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
    let owner = fs::symlink_metadata(directory).map_err(|_| LocalSignerError::Storage)?;
    let mut changed = false;
    for entry in fs::read_dir(directory).map_err(|_| LocalSignerError::Storage)? {
        let entry = entry.map_err(|_| LocalSignerError::Storage)?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(LocalSignerError::Storage)?;
        if name.starts_with(&prefix) && name.ends_with(".tmp") {
            let metadata =
                fs::symlink_metadata(entry.path()).map_err(|_| LocalSignerError::Storage)?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.permissions().mode() & 0o077 != 0
                || metadata.uid() != owner.uid()
                || metadata.gid() != owner.gid()
            {
                return Err(LocalSignerError::Storage);
            }
            fs::remove_file(entry.path()).map_err(|_| LocalSignerError::Storage)?;
            changed = true;
        }
    }
    if changed {
        sync_directory(directory)?;
    }
    Ok(())
}

fn validate_active_directory(keyring: &LocalSignerKeyring) -> Result<(), LocalSignerError> {
    let mut count = 0;
    for entry in fs::read_dir(&keyring.active_directory).map_err(|_| LocalSignerError::Storage)? {
        let entry = entry.map_err(|_| LocalSignerError::Storage)?;
        if entry.file_name() != PERSON_KEY_FILE {
            return Err(LocalSignerError::Storage);
        }
        count += 1;
    }
    if count > 1 {
        return Err(LocalSignerError::Storage);
    }
    if count == 1 {
        keyring
            .read_active_keys()?
            .ok_or(LocalSignerError::Storage)?;
    }
    Ok(())
}

fn validate_directory(path: &Path) -> Result<(), LocalSignerError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| LocalSignerError::Storage)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(LocalSignerError::Storage);
    }
    Ok(())
}

fn validate_private_directory(path: &Path, ownership_root: &Path) -> Result<(), LocalSignerError> {
    validate_directory(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| LocalSignerError::Storage)?;
    let owner = fs::symlink_metadata(ownership_root).map_err(|_| LocalSignerError::Storage)?;
    if metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != owner.uid()
        || metadata.gid() != owner.gid()
    {
        return Err(LocalSignerError::Storage);
    }
    Ok(())
}

fn read_private_optional(
    path: &Path,
    ownership_root: &Path,
) -> Result<Option<Vec<u8>>, LocalSignerError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(LocalSignerError::Storage),
    };
    let owner = fs::symlink_metadata(ownership_root).map_err(|_| LocalSignerError::Storage)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != owner.uid()
        || metadata.gid() != owner.gid()
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
    Ok(Some(bytes))
}

fn parse_private_key_record(bytes: Vec<u8>) -> Result<Keys, LocalSignerError> {
    let bytes = Zeroizing::new(bytes);
    if bytes.len() != 65 || bytes[64] != b'\n' {
        return Err(LocalSignerError::InvalidKey);
    }
    let value = std::str::from_utf8(&bytes[..64]).map_err(|_| LocalSignerError::InvalidKey)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(LocalSignerError::InvalidKey);
    }
    Keys::parse(value).map_err(|_| LocalSignerError::InvalidKey)
}

fn encode_keys(keys: &Keys) -> Zeroizing<Vec<u8>> {
    let mut encoded = Zeroizing::new(keys.secret_key().to_secret_hex().into_bytes());
    encoded.push(b'\n');
    encoded
}

fn write_unpublished_private(path: &Path, content: &[u8]) -> Result<(), LocalSignerError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| LocalSignerError::Storage)?;
    file.write_all(content)
        .map_err(|_| LocalSignerError::Storage)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| LocalSignerError::Storage)?;
    file.sync_all().map_err(|_| LocalSignerError::Storage)
}

fn write_new_private(
    path: &Path,
    content: &[u8],
    ownership_root: &Path,
) -> Result<bool, LocalSignerError> {
    let parent = path.parent().ok_or(LocalSignerError::Storage)?;
    validate_private_directory(parent, ownership_root)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("person-key"),
        rand::random::<u64>()
    ));
    let result = (|| {
        write_unpublished_private(&temporary, content)?;
        match fs::hard_link(&temporary, path) {
            Ok(()) => {
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
        let active = directory.join("active");
        let retired = directory.join("retired");
        let key = active.join("person.key");
        for path in [&directory, &active, &retired] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        assert_eq!(
            fs::metadata(key).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::read_dir(retired).unwrap().count(), 0);
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
    fn interrupted_private_temporary_does_not_turn_remote_ownership_into_a_local_key() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("identity");
        let active = directory.join("active");
        let retired = directory.join("retired");
        fs::create_dir(&directory).unwrap();
        fs::create_dir(&active).unwrap();
        fs::create_dir(&retired).unwrap();
        for path in [&directory, &active, &retired] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let interrupted = active.join(".person.key.interrupted.tmp");
        fs::write(&interrupted, b"incomplete").unwrap();
        fs::set_permissions(&interrupted, fs::Permissions::from_mode(0o600)).unwrap();

        let device = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device.path());
        assert!(matches!(
            LocalPersonSigner::load_or_create(root.path(), &expected_device),
            Err(LocalSignerError::NoActiveKey)
        ));
        assert!(!interrupted.exists());
        assert!(!active.join("person.key").exists());
    }

    #[test]
    fn abandoned_unpublished_identity_tree_is_removed_before_atomic_initialization() {
        let root = tempfile::tempdir().unwrap();
        let abandoned = root.path().join(".identity.interrupted.tmp");
        fs::create_dir(&abandoned).unwrap();
        fs::set_permissions(&abandoned, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(abandoned.join("partial"), b"not published").unwrap();
        let device = tempfile::tempdir().unwrap();

        let signer =
            LocalPersonSigner::load_or_create(root.path(), &device_public_key(device.path()))
                .unwrap();

        assert!(!abandoned.exists());
        assert!(root.path().join("identity/active/person.key").is_file());
        assert!(root.path().join("identity/retired").is_dir());
        assert_eq!(signer.public_key().len(), 64);
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
        let final_record =
            fs::read_to_string(root.path().join("identity/active/person.key")).unwrap();
        assert_eq!(final_record.len(), 65);
        assert!(final_record.ends_with('\n'));
        assert_eq!(
            fs::read_dir(root.path().join("identity/active"))
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

        let mut identity_entries = fs::read_dir(signer_root.path().join("identity"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        identity_entries.sort();
        assert_eq!(
            identity_entries,
            vec![
                std::ffi::OsString::from("active"),
                std::ffi::OsString::from("retired"),
            ]
        );
    }

    #[test]
    fn invalid_nip49_inputs_return_explicit_errors_without_changing_private_state() {
        let signer_root = tempfile::tempdir().unwrap();
        let device_root = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device_root.path());
        let active =
            LocalPersonSigner::load_or_create(signer_root.path(), &expected_device).unwrap();
        let before = fs::read(signer_root.path().join("identity/active/person.key")).unwrap();
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
            fs::read(signer_root.path().join("identity/active/person.key"),).unwrap(),
            before
        );
        assert_eq!(
            fs::read_dir(signer_root.path().join("identity/retired"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn legacy_person_key_is_refused_even_when_the_active_key_exists() {
        let root = tempfile::tempdir().unwrap();
        let device = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device.path());
        LocalPersonSigner::load_or_create(root.path(), &expected_device).unwrap();
        let legacy = root.path().join("identity/person.key");
        fs::write(&legacy, "0".repeat(64) + "\n").unwrap();
        fs::set_permissions(&legacy, fs::Permissions::from_mode(0o600)).unwrap();

        assert!(matches!(
            LocalPersonSigner::load_or_create(root.path(), &expected_device),
            Err(LocalSignerError::LegacyKey)
        ));
    }

    #[test]
    fn valid_keyring_without_an_active_key_reports_remote_ownership() {
        let root = tempfile::tempdir().unwrap();
        let device = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device.path());
        let keyring = LocalSignerKeyring::open_or_initialize(root.path()).unwrap();
        fs::remove_file(root.path().join("identity/active/person.key")).unwrap();

        assert_eq!(keyring.status().unwrap(), LocalSignerKeyringStatus::Remote);
        assert!(matches!(
            LocalPersonSigner::load_or_create(root.path(), &expected_device),
            Err(LocalSignerError::NoActiveKey)
        ));
        assert!(!root.path().join("identity/active/person.key").exists());
    }

    #[test]
    fn malformed_retired_names_and_key_mismatches_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let keyring = LocalSignerKeyring::open_or_initialize(root.path()).unwrap();
        let malformed = root.path().join("identity/retired/NOT-A-PUBLIC-KEY");
        fs::create_dir(&malformed).unwrap();
        fs::set_permissions(&malformed, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(keyring.status().is_err());
        fs::remove_dir(&malformed).unwrap();

        let device = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device.path());
        let first = LocalPersonSigner::import_nip49(
            &LocalPersonSigner::load_or_create(root.path(), &expected_device)
                .unwrap()
                .export_nip49("first password")
                .unwrap(),
            "first password",
            &expected_device,
        )
        .unwrap();
        let other = Keys::generate();
        let wrong_name = first.public_key();
        let directory = root.path().join("identity/retired").join(&wrong_name);
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(directory.join("person.key"), encode_keys(&other).as_slice()).unwrap();
        fs::set_permissions(
            directory.join("person.key"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        assert!(matches!(
            keyring.status(),
            Err(LocalSignerError::KeyConflict)
        ));
    }

    #[test]
    fn malformed_active_key_record_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let keyring = LocalSignerKeyring::open_or_initialize(root.path()).unwrap();
        let active = root.path().join("identity/active/person.key");
        fs::write(&active, "z".repeat(64) + "\n").unwrap();
        fs::set_permissions(&active, fs::Permissions::from_mode(0o600)).unwrap();

        assert!(matches!(
            keyring.status(),
            Err(LocalSignerError::InvalidKey)
        ));
    }

    #[test]
    fn loose_directory_or_key_modes_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let keyring = LocalSignerKeyring::open_or_initialize(root.path()).unwrap();
        let retired = root.path().join("identity/retired");
        fs::set_permissions(&retired, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(keyring.status(), Err(LocalSignerError::Storage)));

        fs::set_permissions(&retired, fs::Permissions::from_mode(0o700)).unwrap();
        let active = root.path().join("identity/active/person.key");
        fs::set_permissions(&active, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(matches!(keyring.status(), Err(LocalSignerError::Storage)));
    }

    #[test]
    fn symlinks_in_active_or_retired_storage_fail_closed() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let keyring = LocalSignerKeyring::open_or_initialize(root.path()).unwrap();
        let active = root.path().join("identity/active/person.key");
        let target = root.path().join("outside.key");
        fs::rename(&active, &target).unwrap();
        symlink(&target, &active).unwrap();
        assert!(keyring.status().is_err());

        fs::remove_file(&active).unwrap();
        fs::rename(&target, &active).unwrap();
        let retired_link = root.path().join("identity/retired").join("0".repeat(64));
        symlink(root.path(), retired_link).unwrap();
        assert!(keyring.status().is_err());
    }

    #[test]
    fn inactive_persist_promote_and_retire_are_idempotent_at_retry_boundaries() {
        let root = tempfile::tempdir().unwrap();
        let device = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device.path());
        let active = LocalPersonSigner::load_or_create(root.path(), &expected_device).unwrap();
        let old_public_key = active.public_key();
        let replacement_encrypted = Keys::generate()
            .secret_key()
            .encrypt("replacement password")
            .unwrap()
            .to_bech32()
            .unwrap();
        let replacement = LocalPersonSigner::import_nip49(
            &replacement_encrypted,
            "replacement password",
            &expected_device,
        )
        .unwrap();
        let replacement_public_key = replacement.public_key();
        let keyring = LocalSignerKeyring::open_or_initialize(root.path()).unwrap();

        assert_eq!(
            keyring.persist_inactive(&replacement).unwrap(),
            replacement_public_key
        );
        assert_eq!(
            keyring.persist_inactive(&replacement).unwrap(),
            replacement_public_key
        );
        assert_eq!(
            keyring.inspect_inactive(&replacement_public_key).unwrap(),
            replacement_public_key
        );
        let exported = keyring
            .export_retired_nip49(&replacement_public_key, "second backup")
            .unwrap();
        assert_eq!(
            LocalPersonSigner::import_nip49(&exported, "second backup", &expected_device)
                .unwrap()
                .public_key(),
            replacement_public_key
        );

        keyring
            .retire_active(&old_public_key, Some(&replacement_public_key))
            .unwrap();
        keyring
            .retire_active(&old_public_key, Some(&replacement_public_key))
            .unwrap();
        assert_eq!(
            keyring.status().unwrap(),
            LocalSignerKeyringStatus::Active {
                public_key: replacement_public_key.clone()
            }
        );
        assert_eq!(
            keyring.retired_public_keys().unwrap(),
            vec![old_public_key.clone()]
        );
        let old_export = keyring
            .export_retired_nip49(&old_public_key, "retired backup")
            .unwrap();
        assert_eq!(
            LocalPersonSigner::import_nip49(&old_export, "retired backup", &expected_device)
                .unwrap()
                .public_key(),
            old_public_key
        );
        keyring.promote_retired(&replacement_public_key).unwrap();
    }

    #[test]
    fn retry_after_replacement_link_cleans_the_duplicate_retired_key() {
        let root = tempfile::tempdir().unwrap();
        let device = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device.path());
        let active = LocalPersonSigner::load_or_create(root.path(), &expected_device).unwrap();
        let old_public_key = active.public_key();
        let imported = LocalPersonSigner::import_nip49(
            &Keys::generate()
                .secret_key()
                .encrypt("replacement password")
                .unwrap()
                .to_bech32()
                .unwrap(),
            "replacement password",
            &expected_device,
        )
        .unwrap();
        let replacement_public_key = imported.public_key();
        let keyring = LocalSignerKeyring::open_or_initialize(root.path()).unwrap();
        keyring.persist_inactive(&imported).unwrap();
        keyring.retire_active(&old_public_key, None).unwrap();
        fs::hard_link(
            root.path()
                .join("identity/retired")
                .join(&replacement_public_key)
                .join("person.key"),
            root.path().join("identity/active/person.key"),
        )
        .unwrap();
        assert!(matches!(keyring.status(), Err(LocalSignerError::Storage)));

        keyring
            .retire_active(&old_public_key, Some(&replacement_public_key))
            .unwrap();

        assert_eq!(
            keyring.status().unwrap(),
            LocalSignerKeyringStatus::Active {
                public_key: replacement_public_key.clone()
            }
        );
        assert!(!root
            .path()
            .join("identity/retired")
            .join(replacement_public_key)
            .exists());
    }

    #[test]
    fn malformed_retired_directory_blocks_promotion_before_active_changes() {
        let root = tempfile::tempdir().unwrap();
        let device = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device.path());
        let active = LocalPersonSigner::load_or_create(root.path(), &expected_device).unwrap();
        let active_public_key = active.public_key();
        let imported = LocalPersonSigner::import_nip49(
            &Keys::generate()
                .secret_key()
                .encrypt("replacement password")
                .unwrap()
                .to_bech32()
                .unwrap(),
            "replacement password",
            &expected_device,
        )
        .unwrap();
        let imported_public_key = imported.public_key();
        let keyring = LocalSignerKeyring::open_or_initialize(root.path()).unwrap();
        keyring.persist_inactive(&imported).unwrap();
        fs::write(
            root.path()
                .join("identity/retired")
                .join(&imported_public_key)
                .join("unexpected"),
            b"unsafe partial state",
        )
        .unwrap();

        assert!(keyring.promote_retired(&imported_public_key).is_err());
        assert!(matches!(keyring.status(), Err(LocalSignerError::Storage)));
        assert_eq!(
            parse_private_key_record(
                fs::read(root.path().join("identity/active/person.key")).unwrap()
            )
            .unwrap()
            .public_key()
            .to_hex(),
            active_public_key
        );
    }

    #[test]
    fn inactive_rollback_and_explicit_delete_are_exact_and_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let device = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device.path());
        let active = LocalPersonSigner::load_or_create(root.path(), &expected_device).unwrap();
        let active_public_key = active.public_key();
        let imported = LocalPersonSigner::import_nip49(
            &Keys::generate()
                .secret_key()
                .encrypt("staged password")
                .unwrap()
                .to_bech32()
                .unwrap(),
            "staged password",
            &expected_device,
        )
        .unwrap();
        let imported_public_key = imported.public_key();
        let keyring = LocalSignerKeyring::open_or_initialize(root.path()).unwrap();

        keyring.persist_inactive(&imported).unwrap();
        keyring.rollback_inactive(&imported_public_key).unwrap();
        keyring.rollback_inactive(&imported_public_key).unwrap();
        assert!(keyring.retired_public_keys().unwrap().is_empty());

        keyring.persist_inactive(&imported).unwrap();
        keyring.delete_retired(&imported_public_key).unwrap();
        keyring.delete_retired(&imported_public_key).unwrap();
        let interrupted = root
            .path()
            .join("identity/retired")
            .join(&imported_public_key);
        fs::create_dir(&interrupted).unwrap();
        fs::set_permissions(&interrupted, fs::Permissions::from_mode(0o700)).unwrap();
        keyring.rollback_inactive(&imported_public_key).unwrap();
        assert!(!interrupted.exists());
        assert!(matches!(
            keyring.delete_retired(&active_public_key),
            Err(LocalSignerError::KeyConflict)
        ));
    }

    #[test]
    fn active_key_can_be_retired_without_replacement_for_remote_ownership() {
        let root = tempfile::tempdir().unwrap();
        let device = tempfile::tempdir().unwrap();
        let expected_device = device_public_key(device.path());
        let active = LocalPersonSigner::load_or_create(root.path(), &expected_device).unwrap();
        let public_key = active.public_key();
        let keyring = LocalSignerKeyring::open_or_initialize(root.path()).unwrap();

        keyring.retire_active(&public_key, None).unwrap();
        keyring.retire_active(&public_key, None).unwrap();
        assert_eq!(keyring.status().unwrap(), LocalSignerKeyringStatus::Remote);
        assert_eq!(keyring.retired_public_keys().unwrap(), vec![public_key]);
    }

    #[tokio::test]
    async fn unix_adapter_carries_only_the_existing_person_signer_request_and_state() {
        let signer_root = tempfile::tempdir().unwrap();
        let device_root = tempfile::tempdir().unwrap();
        let socket_root = tempfile::tempdir().unwrap();
        let socket = socket_root.path().join("signer.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        LocalPersonSigner::load_or_create(
            signer_root.path(),
            &device_public_key(device_root.path()),
        )
        .unwrap();
        let server = {
            let root = signer_root.path().to_owned();
            let expected = Arc::new(Mutex::new(device_public_key(device_root.path())));
            let public_key_path = socket_root.path().join("person.pub");
            tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                super::serve_connection(stream, root, expected, public_key_path)
                    .await
                    .unwrap();
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
