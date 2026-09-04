//! Korri device identity and owner binding.
//!
//! This module keeps Nostr library types behind Korri-owned strings and state.
//! NIP-01 supplies event signatures. NIP-44 v2 supplies encrypted payloads.
//! NIP-78 supplies the addressable event shape for owner state.

use nostr::{
    event::{Event, EventBuilder, FinalizeEvent, FinalizeUnsignedEvent, Kind, Tag},
    key::{Keys, PublicKey},
    nips::{
        nip44::{self, Version as Nip44Version},
        nip59::{extract_rumor, GiftWrapBuilder},
    },
    types::Timestamp,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use zeroize::Zeroizing;

const IDENTITY_DIRECTORY: &str = "identity";
const DEVICE_KEY_FILE: &str = "device.key";
const OWNER_EVENT_FILE: &str = "owner.event.json";
const NIP46_CLIENT_KEY_FILE: &str = "nip46-client.key";
const NIP46_CONNECTION_FILE: &str = "nip46.connection.json";
const OWNER_EVENT_KIND: u16 = 30_078;
const OWNER_EVENT_PREFIX: &str = "org.korri.device-owner:";
const MAX_EVENT_BYTES: usize = 64 * 1024;
const MAX_ENCRYPTED_PLAINTEXT_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENCRYPTED_PAYLOAD_BYTES: usize = 6 * 1024 * 1024;
static IDENTITY_STORAGE: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum IdentityState {
    Unowned {
        device_public_key: String,
    },
    Owned {
        device_public_key: String,
        owner_public_key: String,
        event_id: String,
        created_at: u64,
    },
    Revoked {
        device_public_key: String,
        owner_public_key: String,
        event_id: String,
        created_at: u64,
    },
    Invalid {
        reason: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedEvent {
    pub id: String,
    pub author: String,
    pub json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedEvent {
    pub id: String,
    pub author: String,
    pub created_at: u64,
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
    pub content: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GiftRumor {
    pub id: String,
    pub author: String,
    pub created_at: u64,
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
    pub content: String,
}

/// A disposable NIP-46 client key. It is separate from the device identity.
pub struct Nip46ConnectionIdentity {
    directory: PathBuf,
    keys: Keys,
}

impl std::fmt::Debug for Nip46ConnectionIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Nip46ConnectionIdentity")
            .field("public_key", &self.public_key())
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("identity storage is unavailable")]
    Storage,
    #[error("device identity is invalid: {0}")]
    Invalid(String),
    #[error("identity event is invalid: {0}")]
    InvalidEvent(String),
    #[error("identity encryption failed")]
    Encryption,
    #[error("identity decryption failed")]
    Decryption,
    #[error("device has no usable key")]
    NoDeviceKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnerStatementStatus {
    Owned,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedOwnerStatement {
    pub event_id: String,
    pub owner_public_key: String,
    pub device_public_key: String,
    pub status: OwnerStatementStatus,
    pub created_at: u64,
}

impl OwnerStatementStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Owned => "owned",
            Self::Revoked => "revoked",
        }
    }

    fn parse(value: &str) -> Result<Self, IdentityError> {
        match value {
            "owned" => Ok(Self::Owned),
            "revoked" => Ok(Self::Revoked),
            _ => Err(IdentityError::InvalidEvent(
                "owner status is not supported".into(),
            )),
        }
    }
}

/// A device key and its current owner state.
///
/// `Keys` stays private. Callers receive only lowercase hexadecimal public keys
/// and signed JSON events.
#[derive(Clone)]
pub struct DeviceIdentity {
    directory: PathBuf,
    keys: Option<Keys>,
    state: IdentityState,
    owner_event: Option<Event>,
}

impl std::fmt::Debug for DeviceIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeviceIdentity")
            .field("directory", &self.directory)
            .field("state", &self.state)
            .finish()
    }
}

impl DeviceIdentity {
    /// Remove all device identity state, then create one new unowned identity.
    pub fn reset(private_state_root: &Path) -> Result<Self, IdentityError> {
        {
            let _storage = IDENTITY_STORAGE
                .get_or_init(|| Mutex::new(()))
                .lock()
                .expect("identity storage mutex poisoned");
            let directory = private_state_root.join(IDENTITY_DIRECTORY);
            match fs::symlink_metadata(private_state_root) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => return Err(IdentityError::Storage),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(IdentityError::Storage),
            }
            match fs::symlink_metadata(&directory) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                    fs::remove_dir_all(&directory).map_err(|_| IdentityError::Storage)?;
                    sync_directory(private_state_root)?;
                }
                Ok(_) => return Err(IdentityError::Storage),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(IdentityError::Storage),
            }
        }
        Self::load_or_create(private_state_root)
    }

    /// Load the fixed device identity or create it once.
    pub fn load_or_create(private_state_root: &Path) -> Result<Self, IdentityError> {
        let _storage = IDENTITY_STORAGE
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("identity storage mutex poisoned");
        let directory = prepare_identity_directory(private_state_root)?;
        let keys = match load_or_create_keys(&directory) {
            Ok(keys) => keys,
            Err(error) => {
                return Ok(Self {
                    directory,
                    keys: None,
                    state: IdentityState::Invalid {
                        reason: error.to_string(),
                    },
                    owner_event: None,
                })
            }
        };
        let device_public_key = keys.public_key().to_hex();
        let owner_path = directory.join(OWNER_EVENT_FILE);
        let owner_event = match read_bounded_optional(&owner_path, MAX_EVENT_BYTES) {
            Ok(Some(bytes)) => match parse_owner_statement(&bytes, &device_public_key) {
                Ok(statement) => Some(statement.event),
                Err(error) => {
                    return Ok(Self {
                        directory,
                        keys: Some(keys),
                        state: IdentityState::Invalid {
                            reason: error.to_string(),
                        },
                        owner_event: None,
                    })
                }
            },
            Ok(None) => None,
            Err(error) => {
                return Ok(Self {
                    directory,
                    keys: Some(keys),
                    state: IdentityState::Invalid {
                        reason: error.to_string(),
                    },
                    owner_event: None,
                })
            }
        };
        let state = owner_event
            .as_ref()
            .map(|event| state_from_owner_event(event, &device_public_key))
            .unwrap_or_else(|| IdentityState::Unowned {
                device_public_key: device_public_key.clone(),
            });
        Ok(Self {
            directory,
            keys: Some(keys),
            state,
            owner_event,
        })
    }

    pub fn state(&self) -> &IdentityState {
        &self.state
    }

    pub fn device_public_key(&self) -> Option<&str> {
        match &self.state {
            IdentityState::Unowned { device_public_key }
            | IdentityState::Owned {
                device_public_key, ..
            }
            | IdentityState::Revoked {
                device_public_key, ..
            } => Some(device_public_key),
            IdentityState::Invalid { .. } => None,
        }
    }

    /// Return the verified owner statement without exposing the device key.
    pub fn owner_statement_json(&self) -> Option<String> {
        self.owner_event.as_ref().map(Event::as_json)
    }

    /// Build the unsigned NIP-78 event that an external person signer signs.
    pub fn owner_statement_template(
        &self,
        status: OwnerStatementStatus,
        created_at: u64,
    ) -> Result<String, IdentityError> {
        let device = self.public_key()?;
        let template = OwnerStatementTemplate {
            kind: OWNER_EVENT_KIND,
            created_at,
            tags: owner_tags(&device.to_hex(), status),
            content: String::new(),
        };
        serde_json::to_string(&template)
            .map_err(|_| IdentityError::InvalidEvent("owner template cannot be serialized".into()))
    }

    /// Verify that an external signer signed this exact owner-binding template,
    /// then store it. No signer private material crosses this boundary.
    pub fn apply_signed_owner_binding(
        &mut self,
        unsigned_template_json: &str,
        expected_owner_public_key: &str,
        signed_event_json: &str,
    ) -> Result<(), IdentityError> {
        let expected_owner = parse_public_key(expected_owner_public_key)?;
        let supplied_template: OwnerStatementTemplate =
            serde_json::from_str(unsigned_template_json).map_err(|_| {
                IdentityError::InvalidEvent("owner template JSON is malformed".into())
            })?;
        let device_public_key = self.public_key()?.to_hex();
        let expected_template = OwnerStatementTemplate {
            kind: OWNER_EVENT_KIND,
            created_at: supplied_template.created_at,
            tags: owner_tags(&device_public_key, OwnerStatementStatus::Owned),
            content: String::new(),
        };
        if supplied_template != expected_template {
            return Err(IdentityError::InvalidEvent(
                "owner template does not request this device binding".into(),
            ));
        }
        if signed_event_json.len() > MAX_EVENT_BYTES {
            return Err(IdentityError::InvalidEvent(
                "owner statement is too large".into(),
            ));
        }
        let signed = Event::from_json(signed_event_json)
            .map_err(|_| IdentityError::InvalidEvent("owner statement JSON is malformed".into()))?;
        signed
            .verify()
            .map_err(|_| IdentityError::InvalidEvent("owner signature is invalid".into()))?;
        let signed_tags: Vec<Vec<String>> = signed
            .tags
            .iter()
            .map(|tag| tag.as_slice().to_vec())
            .collect();
        if signed.pubkey != expected_owner
            || signed.kind.as_u16() != supplied_template.kind
            || signed.created_at.as_secs() != supplied_template.created_at
            || signed_tags != supplied_template.tags
            || signed.content != supplied_template.content
        {
            return Err(IdentityError::InvalidEvent(
                "signer returned a different event".into(),
            ));
        }
        self.apply_owner_statement(signed_event_json)
    }

    /// Store a newer valid owner statement for this exact device.
    pub fn apply_owner_statement(&mut self, event_json: &str) -> Result<(), IdentityError> {
        if matches!(
            self.state,
            IdentityState::Invalid { .. } | IdentityState::Revoked { .. }
        ) {
            return Err(IdentityError::Invalid(
                "the current identity state does not accept owner changes".into(),
            ));
        }
        let device_public_key = self.public_key()?.to_hex();
        let statement = parse_owner_statement(event_json.as_bytes(), &device_public_key)?;
        match (&self.state, statement.status) {
            (IdentityState::Unowned { .. }, OwnerStatementStatus::Owned) => {}
            (IdentityState::Unowned { .. }, OwnerStatementStatus::Revoked) => {
                return Err(IdentityError::InvalidEvent(
                    "an unowned device cannot accept a revocation".into(),
                ))
            }
            (
                IdentityState::Owned {
                    owner_public_key, ..
                },
                _,
            ) if owner_public_key != &statement.event.pubkey.to_hex() => {
                return Err(IdentityError::InvalidEvent(
                    "owner statement uses a different owner".into(),
                ))
            }
            (IdentityState::Owned { .. }, _) => {
                let current = self.owner_event.as_ref().ok_or_else(|| {
                    IdentityError::Invalid("owned identity has no owner event".into())
                })?;
                if !event_is_newer(&statement.event, current) {
                    return Err(IdentityError::InvalidEvent(
                        "owner statement is not newer".into(),
                    ));
                }
            }
            _ => unreachable!("invalid and revoked states returned above"),
        }
        let bytes = statement.event.as_json().into_bytes();
        write_private_atomically(&self.directory.join(OWNER_EVENT_FILE), &bytes)?;
        self.state = state_from_owner_event(&statement.event, &device_public_key);
        self.owner_event = Some(statement.event);
        Ok(())
    }

    /// Sign a NIP-01 event with the device key.
    pub fn sign_event(
        &self,
        kind: u16,
        tags: Vec<Vec<String>>,
        content: String,
        created_at: u64,
    ) -> Result<SignedEvent, IdentityError> {
        let tags = parse_tags(tags)?;
        let event = EventBuilder::new(Kind::Custom(kind), content)
            .tags(tags)
            .custom_created_at(Timestamp::from(created_at))
            .finalize(self.keys.as_ref().ok_or(IdentityError::NoDeviceKey)?)
            .map_err(|_| IdentityError::InvalidEvent("event signing failed".into()))?;
        Ok(signed_event(event))
    }

    /// Parse and verify one bounded NIP-01 event.
    pub fn verify_event(event_json: &str) -> Result<VerifiedEvent, IdentityError> {
        verify_event_bounded(event_json, MAX_EVENT_BYTES)
    }

    /// Parse and verify one bounded event carrying NIP-44 content.
    pub fn verify_encrypted_event(event_json: &str) -> Result<VerifiedEvent, IdentityError> {
        verify_event_bounded(event_json, MAX_ENCRYPTED_PAYLOAD_BYTES)
    }

    /// Verify one owner statement for one exact device.
    pub fn verify_owner_statement(
        event_json: &str,
        expected_device_public_key: &str,
    ) -> Result<VerifiedOwnerStatement, IdentityError> {
        let statement = parse_owner_statement(event_json.as_bytes(), expected_device_public_key)?;
        Ok(VerifiedOwnerStatement {
            event_id: statement.event.id.to_hex(),
            owner_public_key: statement.event.pubkey.to_hex(),
            device_public_key: expected_device_public_key.into(),
            status: statement.status,
            created_at: statement.event.created_at.as_secs(),
        })
    }

    /// Verify an owned statement for one exact device and return its owner key.
    pub fn owner_public_key_from_statement(
        event_json: &str,
        expected_device_public_key: &str,
    ) -> Result<String, IdentityError> {
        let statement = Self::verify_owner_statement(event_json, expected_device_public_key)?;
        if statement.status != OwnerStatementStatus::Owned {
            return Err(IdentityError::InvalidEvent(
                "owner statement does not own the device".into(),
            ));
        }
        Ok(statement.owner_public_key)
    }

    /// Encrypt content with NIP-44 v2 and sign the containing NIP-01 event.
    pub fn encrypt_event(
        &self,
        recipient_public_key: &str,
        kind: u16,
        plaintext: &str,
        created_at: u64,
    ) -> Result<SignedEvent, IdentityError> {
        self.encrypt_tagged_event(
            recipient_public_key,
            kind,
            vec![vec!["p".into(), recipient_public_key.into()]],
            plaintext,
            created_at,
        )
    }

    /// Encrypt content while preserving a Korri-owned ordered tag list.
    pub fn encrypt_tagged_event(
        &self,
        recipient_public_key: &str,
        kind: u16,
        tags: Vec<Vec<String>>,
        plaintext: &str,
        created_at: u64,
    ) -> Result<SignedEvent, IdentityError> {
        if plaintext.is_empty() || plaintext.len() > MAX_ENCRYPTED_PLAINTEXT_BYTES {
            return Err(IdentityError::Encryption);
        }
        let recipient = parse_public_key(recipient_public_key)?;
        let keys = self.keys.as_ref().ok_or(IdentityError::NoDeviceKey)?;
        let encrypted = nip44::encrypt(
            keys.secret_key(),
            &recipient,
            plaintext.as_bytes(),
            Nip44Version::V2,
        )
        .map_err(|_| IdentityError::Encryption)?;
        let event = EventBuilder::new(Kind::Custom(kind), encrypted)
            .tags(parse_tags(tags)?)
            .custom_created_at(Timestamp::from(created_at))
            .finalize(keys)
            .map_err(|_| IdentityError::Encryption)?;
        Ok(signed_event(event))
    }

    /// Wrap a private queued coordination message using NIP-59.
    pub fn gift_wrap(
        &self,
        recipient_public_key: &str,
        rumor_kind: u16,
        content: &str,
        created_at: u64,
        expires_at: u64,
    ) -> Result<SignedEvent, IdentityError> {
        if content.is_empty() || content.len() > MAX_EVENT_BYTES || expires_at <= created_at {
            return Err(IdentityError::Encryption);
        }
        let recipient = parse_public_key(recipient_public_key)?;
        let keys = self.keys.as_ref().ok_or(IdentityError::NoDeviceKey)?;
        let rumor = EventBuilder::new(Kind::Custom(rumor_kind), content)
            .tag(Tag::expiration(Timestamp::from(expires_at)))
            .custom_created_at(Timestamp::from(created_at))
            .finalize_unsigned(keys.public_key());
        let event = GiftWrapBuilder::new(recipient, rumor)
            .extra_tags([Tag::expiration(Timestamp::from(expires_at))])
            .finalize(keys)
            .map_err(|_| IdentityError::Encryption)?;
        Ok(signed_event(event))
    }

    /// Verify and unwrap one bounded NIP-59 gift wrap for this device.
    pub fn unwrap_gift_wrap(&self, event_json: &str, now: u64) -> Result<GiftRumor, IdentityError> {
        if event_json.len() > MAX_ENCRYPTED_PAYLOAD_BYTES {
            return Err(IdentityError::Decryption);
        }
        let event = Event::from_json(event_json).map_err(|_| IdentityError::Decryption)?;
        event.verify().map_err(|_| IdentityError::Decryption)?;
        if event.kind != Kind::GiftWrap
            || !has_exact_recipient(&event, &self.public_key()?.to_hex())
        {
            return Err(IdentityError::Decryption);
        }
        if event
            .tags
            .expiration()
            .is_some_and(|expiry| expiry.as_secs() <= now)
        {
            return Err(IdentityError::Decryption);
        }
        let gift = extract_rumor(
            self.keys.as_ref().ok_or(IdentityError::NoDeviceKey)?,
            &event,
        )
        .map_err(|_| IdentityError::Decryption)?;
        gift.rumor
            .verify_id()
            .map_err(|_| IdentityError::Decryption)?;
        if gift
            .rumor
            .tags
            .expiration()
            .is_some_and(|expiry| expiry.as_secs() <= now)
        {
            return Err(IdentityError::Decryption);
        }
        Ok(GiftRumor {
            id: gift.rumor.id.ok_or(IdentityError::Decryption)?.to_hex(),
            author: gift.sender.to_hex(),
            created_at: gift.rumor.created_at.as_secs(),
            kind: gift.rumor.kind.as_u16(),
            tags: gift
                .rumor
                .tags
                .iter()
                .map(|tag| tag.as_slice().to_vec())
                .collect(),
            content: gift.rumor.content,
        })
    }

    /// Build one canonical NIP-42 authentication event.
    pub fn relay_auth_event(
        &self,
        relay_url: &str,
        challenge: &str,
        created_at: u64,
    ) -> Result<SignedEvent, IdentityError> {
        if relay_url.len() > 2048 || challenge.is_empty() || challenge.len() > 4096 {
            return Err(IdentityError::InvalidEvent(
                "relay challenge is invalid".into(),
            ));
        }
        self.sign_event(
            22_242,
            vec![
                vec!["relay".into(), relay_url.into()],
                vec!["challenge".into(), challenge.into()],
            ],
            String::new(),
            created_at,
        )
    }

    /// Verify the signed event before NIP-44 v2 decryption.
    pub fn decrypt_event(&self, event_json: &str) -> Result<String, IdentityError> {
        if event_json.len() > MAX_ENCRYPTED_PAYLOAD_BYTES {
            return Err(IdentityError::Decryption);
        }
        let event = Event::from_json(event_json).map_err(|_| IdentityError::Decryption)?;
        event.verify().map_err(|_| IdentityError::Decryption)?;
        let keys = self.keys.as_ref().ok_or(IdentityError::NoDeviceKey)?;
        let recipient = keys.public_key().to_hex();
        let addressed_to_device = event.tags.iter().any(|tag| {
            let values = tag.as_slice();
            values.len() == 2 && values[0] == "p" && values[1] == recipient
        });
        if !addressed_to_device {
            return Err(IdentityError::Decryption);
        }
        nip44::decrypt(keys.secret_key(), &event.pubkey, event.content.as_bytes())
            .map_err(|_| IdentityError::Decryption)
    }

    fn public_key(&self) -> Result<PublicKey, IdentityError> {
        self.keys
            .as_ref()
            .map(Keys::public_key)
            .ok_or(IdentityError::NoDeviceKey)
    }
}

impl Nip46ConnectionIdentity {
    pub fn load_or_create(private_state_root: &Path) -> Result<Self, IdentityError> {
        let _storage = IDENTITY_STORAGE
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("identity storage mutex poisoned");
        let directory = prepare_identity_directory(private_state_root)?;
        let keys = load_or_create_named_keys(&directory, NIP46_CLIENT_KEY_FILE, "NIP-46 client")?;
        Ok(Self { directory, keys })
    }

    pub fn public_key(&self) -> String {
        self.keys.public_key().to_hex()
    }

    pub fn sign_encrypted_event(
        &self,
        recipient_public_key: &str,
        kind: u16,
        plaintext: &str,
        created_at: u64,
    ) -> Result<SignedEvent, IdentityError> {
        if plaintext.is_empty() || plaintext.len() > MAX_EVENT_BYTES {
            return Err(IdentityError::Encryption);
        }
        let recipient = parse_public_key(recipient_public_key)?;
        let content = nip44::encrypt(
            self.keys.secret_key(),
            &recipient,
            plaintext.as_bytes(),
            Nip44Version::V2,
        )
        .map_err(|_| IdentityError::Encryption)?;
        let event = EventBuilder::new(Kind::Custom(kind), content)
            .tag(Tag::public_key(recipient))
            .custom_created_at(Timestamp::from(created_at))
            .finalize(&self.keys)
            .map_err(|_| IdentityError::Encryption)?;
        Ok(signed_event(event))
    }

    pub fn decrypt_event_from(
        &self,
        event_json: &str,
        expected_author: &str,
    ) -> Result<VerifiedEvent, IdentityError> {
        let expected_author = parse_public_key(expected_author)?;
        let event = Event::from_json(event_json).map_err(|_| IdentityError::Decryption)?;
        event.verify().map_err(|_| IdentityError::Decryption)?;
        if event.pubkey != expected_author
            || !has_exact_recipient(&event, &self.keys.public_key().to_hex())
        {
            return Err(IdentityError::Decryption);
        }
        let mut verified = verified_event(event.clone());
        verified.content = nip44::decrypt(
            self.keys.secret_key(),
            &event.pubkey,
            event.content.as_bytes(),
        )
        .map_err(|_| IdentityError::Decryption)?;
        Ok(verified)
    }

    pub fn save_connection_data(&self, json: &str) -> Result<(), IdentityError> {
        if self.directory.as_os_str().is_empty() || json.len() > MAX_EVENT_BYTES {
            return Err(IdentityError::Storage);
        }
        write_private_atomically(&self.directory.join(NIP46_CONNECTION_FILE), json.as_bytes())
    }

    pub fn load_connection_data(&self) -> Result<Option<String>, IdentityError> {
        read_bounded_optional(&self.directory.join(NIP46_CONNECTION_FILE), MAX_EVENT_BYTES)?
            .map(String::from_utf8)
            .transpose()
            .map_err(|_| IdentityError::Storage)
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OwnerStatementTemplate {
    kind: u16,
    created_at: u64,
    tags: Vec<Vec<String>>,
    content: String,
}

struct ParsedOwnerStatement {
    event: Event,
    status: OwnerStatementStatus,
}

fn owner_tags(device_public_key: &str, status: OwnerStatementStatus) -> Vec<Vec<String>> {
    vec![
        vec![
            "d".into(),
            format!("{OWNER_EVENT_PREFIX}{device_public_key}"),
        ],
        vec!["device".into(), device_public_key.into()],
        vec!["status".into(), status.as_str().into()],
    ]
}

fn parse_owner_statement(
    bytes: &[u8],
    expected_device_public_key: &str,
) -> Result<ParsedOwnerStatement, IdentityError> {
    if bytes.len() > MAX_EVENT_BYTES {
        return Err(IdentityError::InvalidEvent(
            "owner statement is too large".into(),
        ));
    }
    let event = Event::from_json(bytes)
        .map_err(|_| IdentityError::InvalidEvent("owner statement JSON is malformed".into()))?;
    event
        .verify()
        .map_err(|_| IdentityError::InvalidEvent("owner signature is invalid".into()))?;
    if event.kind != Kind::Custom(OWNER_EVENT_KIND) || !event.content.is_empty() {
        return Err(IdentityError::InvalidEvent(
            "owner statement has the wrong event shape".into(),
        ));
    }
    let tags: Vec<Vec<String>> = event
        .tags
        .iter()
        .map(|tag| tag.as_slice().to_vec())
        .collect();
    if tags.len() != 3 {
        return Err(IdentityError::InvalidEvent(
            "owner statement has unexpected tags".into(),
        ));
    }
    let expected_d = format!("{OWNER_EVENT_PREFIX}{expected_device_public_key}");
    if tags[0] != ["d", expected_d.as_str()]
        || tags[1] != ["device", expected_device_public_key]
        || tags[2].len() != 2
        || tags[2][0] != "status"
    {
        return Err(IdentityError::InvalidEvent(
            "owner statement names a different device".into(),
        ));
    }
    let status = OwnerStatementStatus::parse(&tags[2][1])?;
    Ok(ParsedOwnerStatement { event, status })
}

fn state_from_owner_event(event: &Event, device_public_key: &str) -> IdentityState {
    let status = event
        .tags
        .iter()
        .find_map(|tag| {
            let values = tag.as_slice();
            (values.len() == 2 && values[0] == "status").then(|| values[1].as_str())
        })
        .expect("validated owner statement has a status");
    let common = (
        device_public_key.to_owned(),
        event.pubkey.to_hex(),
        event.id.to_hex(),
        event.created_at.as_secs(),
    );
    match status {
        "owned" => IdentityState::Owned {
            device_public_key: common.0,
            owner_public_key: common.1,
            event_id: common.2,
            created_at: common.3,
        },
        "revoked" => IdentityState::Revoked {
            device_public_key: common.0,
            owner_public_key: common.1,
            event_id: common.2,
            created_at: common.3,
        },
        _ => unreachable!("validated owner statement has a supported status"),
    }
}

fn event_is_newer(candidate: &Event, current: &Event) -> bool {
    candidate.created_at > current.created_at
        || (candidate.created_at == current.created_at && candidate.id < current.id)
}

fn signed_event(event: Event) -> SignedEvent {
    SignedEvent {
        id: event.id.to_hex(),
        author: event.pubkey.to_hex(),
        json: event.as_json(),
    }
}

fn verify_event_bounded(
    event_json: &str,
    maximum_bytes: usize,
) -> Result<VerifiedEvent, IdentityError> {
    if event_json.len() > maximum_bytes {
        return Err(IdentityError::InvalidEvent("event is too large".into()));
    }
    let event = Event::from_json(event_json)
        .map_err(|_| IdentityError::InvalidEvent("event JSON is malformed".into()))?;
    event
        .verify()
        .map_err(|_| IdentityError::InvalidEvent("event signature is invalid".into()))?;
    Ok(verified_event(event))
}

fn verified_event(event: Event) -> VerifiedEvent {
    VerifiedEvent {
        id: event.id.to_hex(),
        author: event.pubkey.to_hex(),
        created_at: event.created_at.as_secs(),
        kind: event.kind.as_u16(),
        tags: event
            .tags
            .iter()
            .map(|tag| tag.as_slice().to_vec())
            .collect(),
        content: event.content,
    }
}

fn parse_tags(tags: Vec<Vec<String>>) -> Result<Vec<Tag>, IdentityError> {
    tags.into_iter()
        .map(|tag| {
            Tag::parse(tag)
                .map_err(|_| IdentityError::InvalidEvent("event has an invalid tag".into()))
        })
        .collect()
}

fn has_exact_recipient(event: &Event, recipient: &str) -> bool {
    event
        .tags
        .iter()
        .filter(|tag| {
            let values = tag.as_slice();
            values.len() == 2 && values[0] == "p" && values[1] == recipient
        })
        .count()
        == 1
}

fn parse_public_key(value: &str) -> Result<PublicKey, IdentityError> {
    if value.len() != 64 || !value.bytes().all(is_lower_hex) {
        return Err(IdentityError::InvalidEvent(
            "public key must be 32-byte hexadecimal".into(),
        ));
    }
    PublicKey::from_hex(value)
        .map_err(|_| IdentityError::InvalidEvent("public key is invalid".into()))
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn prepare_identity_directory(private_state_root: &Path) -> Result<PathBuf, IdentityError> {
    if !private_state_root.exists() {
        match DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(private_state_root)
        {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(IdentityError::Storage),
        }
    }
    let root_metadata =
        fs::symlink_metadata(private_state_root).map_err(|_| IdentityError::Storage)?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(IdentityError::Storage);
    }
    let directory = private_state_root.join(IDENTITY_DIRECTORY);
    if !directory.exists() {
        match DirBuilder::new().mode(0o700).create(&directory) {
            Ok(()) => sync_directory(private_state_root)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(IdentityError::Storage),
        }
    }
    let metadata = fs::symlink_metadata(&directory).map_err(|_| IdentityError::Storage)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(IdentityError::Storage);
    }
    Ok(directory)
}

fn load_or_create_keys(directory: &Path) -> Result<Keys, IdentityError> {
    load_or_create_named_keys(directory, DEVICE_KEY_FILE, "device")
}

fn load_or_create_named_keys(
    directory: &Path,
    file_name: &str,
    label: &str,
) -> Result<Keys, IdentityError> {
    let path = directory.join(file_name);
    match read_bounded_optional(&path, 128)? {
        Some(bytes) => parse_named_keys(bytes, label),
        None => {
            let keys = Keys::generate();
            let mut encoded = Zeroizing::new(keys.secret_key().to_secret_hex().into_bytes());
            encoded.push(b'\n');
            if write_new_private(&path, &encoded)? {
                Ok(keys)
            } else {
                load_existing_keys(&path, label)
            }
        }
    }
}

fn load_existing_keys(path: &Path, label: &str) -> Result<Keys, IdentityError> {
    parse_named_keys(
        read_bounded_optional(path, 128)?.ok_or(IdentityError::Storage)?,
        label,
    )
}

fn parse_named_keys(bytes: Vec<u8>, label: &str) -> Result<Keys, IdentityError> {
    let bytes = Zeroizing::new(bytes);
    let value = std::str::from_utf8(&bytes)
        .map_err(|_| IdentityError::Invalid(format!("{label} key is not UTF-8")))?
        .trim();
    if value.len() != 64 || !value.bytes().all(is_lower_hex) {
        return Err(IdentityError::Invalid(format!(
            "{label} key is not 32-byte lowercase hexadecimal"
        )));
    }
    Keys::parse(value)
        .map_err(|_| IdentityError::Invalid(format!("{label} key is not valid secp256k1")))
}

fn read_bounded_optional(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, IdentityError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(IdentityError::Storage),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > limit as u64
    {
        return Err(IdentityError::Storage);
    }
    let file = File::open(path).map_err(|_| IdentityError::Storage)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| IdentityError::Storage)?;
    if bytes.len() > limit {
        return Err(IdentityError::Storage);
    }
    Ok(Some(bytes))
}

fn write_new_private(path: &Path, content: &[u8]) -> Result<bool, IdentityError> {
    let parent = path.parent().ok_or(IdentityError::Storage)?;
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(_) => return Err(IdentityError::Storage),
    };
    file.write_all(content)
        .map_err(|_| IdentityError::Storage)?;
    file.sync_all().map_err(|_| IdentityError::Storage)?;
    sync_directory(parent)?;
    Ok(true)
}

fn write_private_atomically(path: &Path, content: &[u8]) -> Result<(), IdentityError> {
    let parent = path.parent().ok_or(IdentityError::Storage)?;
    let temporary = private_temporary_path(path);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| IdentityError::Storage)?;
        file.write_all(content)
            .map_err(|_| IdentityError::Storage)?;
        file.sync_all().map_err(|_| IdentityError::Storage)?;
        fs::rename(&temporary, path).map_err(|_| IdentityError::Storage)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn private_temporary_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("identity"),
        rand::random::<u64>()
    ))
}

fn sync_directory(path: &Path) -> Result<(), IdentityError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| IdentityError::Storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{key::SecretKey, nips::nip44::Nonce};
    use secp256k1::{schnorr::Signature, Secp256k1, XOnlyPublicKey};
    use std::{os::unix::fs::symlink, str::FromStr};

    fn keys(secret: &str) -> Keys {
        Keys::new(SecretKey::from_hex(secret).unwrap())
    }

    fn owner_statement(
        owner: &Keys,
        device: &str,
        status: OwnerStatementStatus,
        created_at: u64,
    ) -> String {
        EventBuilder::new(Kind::Custom(OWNER_EVENT_KIND), "")
            .tags(
                owner_tags(device, status)
                    .into_iter()
                    .map(|tag| Tag::parse(tag).unwrap()),
            )
            .custom_created_at(Timestamp::from(created_at))
            .finalize(owner)
            .unwrap()
            .as_json()
    }

    #[test]
    fn creates_one_private_device_key_and_reuses_it() {
        let root = tempfile::tempdir().unwrap();
        let first = DeviceIdentity::load_or_create(root.path()).unwrap();
        let public_key = first.device_public_key().unwrap().to_owned();
        assert!(matches!(first.state(), IdentityState::Unowned { .. }));

        let path = root.path().join(IDENTITY_DIRECTORY).join(DEVICE_KEY_FILE);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(root.path().join(IDENTITY_DIRECTORY))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );

        let second = DeviceIdentity::load_or_create(root.path()).unwrap();
        assert_eq!(second.device_public_key(), Some(public_key.as_str()));
    }

    #[test]
    fn concurrent_first_loads_share_one_valid_identity_directory() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("new-private-root");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    DeviceIdentity::load_or_create(&path)
                        .unwrap()
                        .device_public_key()
                        .unwrap()
                        .to_owned()
                })
            })
            .collect();
        let keys: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        assert!(keys.iter().all(|key| key == &keys[0]));
    }

    #[test]
    fn reset_replaces_the_complete_identity_without_following_links() {
        let root = tempfile::tempdir().unwrap();
        let mut identity = DeviceIdentity::load_or_create(root.path()).unwrap();
        let old_device = identity.device_public_key().unwrap().to_owned();
        let owner = keys("0000000000000000000000000000000000000000000000000000000000000003");
        identity
            .apply_owner_statement(&owner_statement(
                &owner,
                &old_device,
                OwnerStatementStatus::Owned,
                100,
            ))
            .unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("keep");
        fs::write(&outside_file, "keep").unwrap();
        symlink(
            outside.path(),
            root.path().join(IDENTITY_DIRECTORY).join("outside"),
        )
        .unwrap();

        let replacement = DeviceIdentity::reset(root.path()).unwrap();

        assert_ne!(replacement.device_public_key(), Some(old_device.as_str()));
        assert!(matches!(replacement.state(), IdentityState::Unowned { .. }));
        assert_eq!(fs::read_to_string(outside_file).unwrap(), "keep");
        assert_eq!(
            fs::metadata(root.path().join(IDENTITY_DIRECTORY))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    #[test]
    fn malformed_or_linked_private_state_becomes_invalid_or_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(IDENTITY_DIRECTORY);
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(directory.join(DEVICE_KEY_FILE), "not-a-key\n").unwrap();
        fs::set_permissions(
            directory.join(DEVICE_KEY_FILE),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let invalid = DeviceIdentity::load_or_create(root.path()).unwrap();
        assert!(matches!(invalid.state(), IdentityState::Invalid { .. }));

        let other = tempfile::tempdir().unwrap();
        let linked_root = root.path().join("linked");
        symlink(other.path(), &linked_root).unwrap();
        assert!(matches!(
            DeviceIdentity::load_or_create(&linked_root),
            Err(IdentityError::Storage)
        ));
    }

    #[test]
    fn owner_template_is_ready_for_an_external_nip55_or_nip46_signer() {
        let root = tempfile::tempdir().unwrap();
        let identity = DeviceIdentity::load_or_create(root.path()).unwrap();
        let device = identity.device_public_key().unwrap();
        let value: serde_json::Value = serde_json::from_str(
            &identity
                .owner_statement_template(OwnerStatementStatus::Owned, 42)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["kind"], OWNER_EVENT_KIND);
        assert_eq!(value["created_at"], 42);
        assert_eq!(value["content"], "");
        assert_eq!(
            value["tags"],
            serde_json::json!([
                ["d", format!("{OWNER_EVENT_PREFIX}{device}")],
                ["device", device],
                ["status", "owned"]
            ])
        );
        assert!(value.get("pubkey").is_none());
        assert!(value.get("sig").is_none());
    }

    #[test]
    fn strictly_accepts_only_the_exact_externally_signed_owner_template() {
        let root = tempfile::tempdir().unwrap();
        let mut identity = DeviceIdentity::load_or_create(root.path()).unwrap();
        let device = identity.device_public_key().unwrap().to_owned();
        let owner = keys("0000000000000000000000000000000000000000000000000000000000000003");
        let other = keys("0000000000000000000000000000000000000000000000000000000000000004");
        let template = identity
            .owner_statement_template(OwnerStatementStatus::Owned, 100)
            .unwrap();
        let signed = owner_statement(&owner, &device, OwnerStatementStatus::Owned, 100);

        identity
            .apply_signed_owner_binding(&template, &owner.public_key().to_hex(), &signed)
            .unwrap();
        assert!(matches!(identity.state(), IdentityState::Owned { .. }));

        let second_root = tempfile::tempdir().unwrap();
        let mut second = DeviceIdentity::load_or_create(second_root.path()).unwrap();
        let second_device = second.device_public_key().unwrap().to_owned();
        let second_template = second
            .owner_statement_template(OwnerStatementStatus::Owned, 100)
            .unwrap();
        let wrong_author =
            owner_statement(&other, &second_device, OwnerStatementStatus::Owned, 100);
        assert!(second
            .apply_signed_owner_binding(
                &second_template,
                &owner.public_key().to_hex(),
                &wrong_author,
            )
            .is_err());
        let wrong_time = owner_statement(&owner, &second_device, OwnerStatementStatus::Owned, 101);
        assert!(
            second
                .apply_signed_owner_binding(
                    &second_template,
                    &owner.public_key().to_hex(),
                    &wrong_time,
                )
                .is_err()
        );
        let mut changed_template: serde_json::Value =
            serde_json::from_str(&second_template).unwrap();
        changed_template["content"] = serde_json::Value::String("changed".into());
        assert!(second
            .apply_signed_owner_binding(
                &changed_template.to_string(),
                &owner.public_key().to_hex(),
                &owner_statement(&owner, &second_device, OwnerStatementStatus::Owned, 100),
            )
            .is_err());
    }

    #[test]
    fn applies_owner_binding_and_only_newer_same_owner_revocation() {
        let root = tempfile::tempdir().unwrap();
        let mut identity = DeviceIdentity::load_or_create(root.path()).unwrap();
        let device = identity.device_public_key().unwrap().to_owned();
        let owner = keys("0000000000000000000000000000000000000000000000000000000000000003");
        let other = keys("0000000000000000000000000000000000000000000000000000000000000004");

        let binding = owner_statement(&owner, &device, OwnerStatementStatus::Owned, 100);
        identity.apply_owner_statement(&binding).unwrap();
        assert!(matches!(
            identity.state(),
            IdentityState::Owned { owner_public_key, .. }
                if owner_public_key == &owner.public_key().to_hex()
        ));

        let stale = owner_statement(&owner, &device, OwnerStatementStatus::Revoked, 99);
        assert!(identity.apply_owner_statement(&stale).is_err());
        let wrong_owner = owner_statement(&other, &device, OwnerStatementStatus::Revoked, 101);
        assert!(identity.apply_owner_statement(&wrong_owner).is_err());

        let revocation = owner_statement(&owner, &device, OwnerStatementStatus::Revoked, 101);
        identity.apply_owner_statement(&revocation).unwrap();
        assert!(matches!(identity.state(), IdentityState::Revoked { .. }));

        let loaded = DeviceIdentity::load_or_create(root.path()).unwrap();
        assert_eq!(loaded.state(), identity.state());
        assert!(identity.apply_owner_statement(&binding).is_err());
    }

    #[test]
    fn rejects_tampered_binding_and_wrong_device() {
        let first_root = tempfile::tempdir().unwrap();
        let second_root = tempfile::tempdir().unwrap();
        let mut first = DeviceIdentity::load_or_create(first_root.path()).unwrap();
        let second = DeviceIdentity::load_or_create(second_root.path()).unwrap();
        let owner = keys("0000000000000000000000000000000000000000000000000000000000000003");
        let statement = owner_statement(
            &owner,
            second.device_public_key().unwrap(),
            OwnerStatementStatus::Owned,
            100,
        );
        assert!(first.apply_owner_statement(&statement).is_err());

        let own = owner_statement(
            &owner,
            first.device_public_key().unwrap(),
            OwnerStatementStatus::Owned,
            100,
        );
        let mut value: serde_json::Value = serde_json::from_str(&own).unwrap();
        value["content"] = serde_json::Value::String("tampered".into());
        assert!(first.apply_owner_statement(&value.to_string()).is_err());
    }

    #[test]
    fn signs_and_verifies_nip01_events_without_exposing_nostr_types() {
        let root = tempfile::tempdir().unwrap();
        let identity = DeviceIdentity::load_or_create(root.path()).unwrap();
        let signed = identity
            .sign_event(
                21_000,
                vec![vec!["alt".into(), "korri probe".into()]],
                "probe".into(),
                42,
            )
            .unwrap();
        let verified = DeviceIdentity::verify_event(&signed.json).unwrap();
        assert_eq!(verified.id, signed.id);
        assert_eq!(verified.author, signed.author);
        assert_eq!(verified.kind, 21_000);
        assert_eq!(verified.created_at, 42);
        assert_eq!(verified.content, "probe");

        let mut value: serde_json::Value = serde_json::from_str(&signed.json).unwrap();
        value["content"] = serde_json::Value::String("changed".into());
        assert!(DeviceIdentity::verify_event(&value.to_string()).is_err());
    }

    #[test]
    fn verifies_the_official_bip340_signature_vector_zero() {
        let public_key = XOnlyPublicKey::from_str(
            "F9308A019258C31049344F85F89D5229B531C845836F99B08601F113BCE036F9",
        )
        .unwrap();
        let message = [0u8; 32];
        let signature = Signature::from_str(
            "E907831F80848D1069A5371B402410364BDF1C5F8307B0084C55F1CE2DCA821525F66A4A85EA8B71E482A74F382D2CE5EBEEE8FDB2172F477DF4900D310536C0",
        )
        .unwrap();
        Secp256k1::verification_only()
            .verify_schnorr(&signature, &message, &public_key)
            .unwrap();
    }

    #[test]
    fn matches_the_official_nip44_v2_vector_and_requires_a_signed_container() {
        let sender = keys("0000000000000000000000000000000000000000000000000000000000000001");
        let receiver = keys("0000000000000000000000000000000000000000000000000000000000000002");
        let payload = nip44::encrypt_with_nonce(
            sender.secret_key(),
            &receiver.public_key(),
            "a",
            Nonce::V2([
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 1,
            ]),
        )
        .unwrap();
        assert_eq!(payload, "AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABee0G5VSK0/9YypIObAtDKfYEAjD35uVkHyB0F4DwrcNaCXlCWZKaArsGrY6M9wnuTMxWfp1RTN9Xga8no+kF5Vsb");

        let first_root = tempfile::tempdir().unwrap();
        let second_root = tempfile::tempdir().unwrap();
        let first = DeviceIdentity::load_or_create(first_root.path()).unwrap();
        let second = DeviceIdentity::load_or_create(second_root.path()).unwrap();
        let encrypted = first
            .encrypt_event(second.device_public_key().unwrap(), 21_001, "secret", 50)
            .unwrap();
        assert_eq!(second.decrypt_event(&encrypted.json).unwrap(), "secret");

        let mut tampered: serde_json::Value = serde_json::from_str(&encrypted.json).unwrap();
        tampered["sig"] = serde_json::Value::String("00".repeat(64));
        assert!(second.decrypt_event(&tampered.to_string()).is_err());
        assert!(first
            .encrypt_event(
                &second.device_public_key().unwrap().to_uppercase(),
                21_001,
                "secret",
                50,
            )
            .is_err());
    }
}
