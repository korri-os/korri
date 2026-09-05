//! Offline peer authorization and Sunshine trust reconciliation.
//!
//! This module owns principal resolution, the exhaustive RPC policy, signed
//! revocation state, exact certificate grants, and the pure reconciliation
//! decision. Peer transport verifies and decrypts an envelope before calling
//! this boundary. RPC handlers receive only an authorized dispatch context.

use crate::{
    identity::{DeviceIdentity, IdentityState, OwnerStatementStatus},
    RpcRequest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub const PERSON_PASS_EVENT_KIND: u16 = 30_079;
pub const STREAM_LAUNCH_SCOPE: &str = "stream.launch";
pub const CATALOG_READ_SCOPE: &str = "catalog.read";
pub const MAX_PERSON_PASS_LIFETIME_SECONDS: u64 = 24 * 60 * 60;

const NIP_09_DELETION_KIND: u16 = 5;
const OWNER_EVENT_KIND: u16 = 30_078;
const PASS_ADDRESS_PREFIX: &str = "org.korri.person-pass:";
const REVOCATION_DIRECTORY: &str = "identity/authorization-revocations";
const CERTIFICATE_DIRECTORY: &str = "identity/peer-certificates";
const MAX_EVENT_BYTES: usize = 64 * 1024;
const MAX_GRANT_BYTES: usize = 128 * 1024;
const CLOCK_SKEW_SECONDS: u64 = 120;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Scope {
    CatalogRead,
    StreamLaunch,
}

impl Scope {
    fn parse(value: &str) -> Option<Self> {
        match value {
            CATALOG_READ_SCOPE => Some(Self::CatalogRead),
            STREAM_LAUNCH_SCOPE => Some(Self::StreamLaunch),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonPass {
    pub event_id: String,
    pub event_json: String,
    pub expires_at: u64,
    pub scopes: Vec<Scope>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Principal {
    OwnerDevice {
        device_public_key: String,
        owner_statement: String,
    },
    Household {
        device_public_key: String,
        owner_statement: String,
        person_pass: PersonPass,
    },
    Guest {
        device_public_key: String,
        owner_statement: String,
        person_pass: PersonPass,
    },
    Unknown {
        device_public_key: String,
    },
}

impl Principal {
    pub fn device_public_key(&self) -> &str {
        match self {
            Self::OwnerDevice {
                device_public_key, ..
            }
            | Self::Household {
                device_public_key, ..
            }
            | Self::Guest {
                device_public_key, ..
            }
            | Self::Unknown { device_public_key } => device_public_key,
        }
    }

    fn has_scope(&self, expected: Scope) -> bool {
        match self {
            Self::Household { person_pass, .. } | Self::Guest { person_pass, .. } => {
                person_pass.scopes.contains(&expected)
            }
            Self::OwnerDevice { .. } | Self::Unknown { .. } => false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthorizationContext {
    LocalBrowser,
    LocalUnixControl,
    Peer(Principal),
    TrustReconciliation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerPolicy {
    OwnerDeviceOnly,
    ExplicitScope(Scope),
    CatalogOrStreamScope,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("RPC request is not authorized")]
pub struct AuthorizationDenied;

#[derive(Debug, thiserror::Error)]
pub enum AuthorizationError {
    #[error("authorization evidence is invalid")]
    InvalidEvidence,
    #[error("authorization storage is unavailable")]
    Storage,
}

/// The complete peer policy table. A new `RpcRequest` variant cannot compile
/// until this mapping receives an explicit policy.
pub fn policy_for(request: &RpcRequest) -> PeerPolicy {
    match request {
        RpcRequest::CatalogSnapshot(_) => PeerPolicy::CatalogOrStreamScope,
        RpcRequest::MoonlightResolve(_) => PeerPolicy::ExplicitScope(Scope::StreamLaunch),
        RpcRequest::MoonlightLaunchPrepare(_) => PeerPolicy::ExplicitScope(Scope::StreamLaunch),
        RpcRequest::MoonlightLaunchCancel(_) => PeerPolicy::OwnerDeviceOnly,
        RpcRequest::MoonlightCertificateAttest(_) => PeerPolicy::ExplicitScope(Scope::StreamLaunch),
        RpcRequest::MoonlightCertificateProvision(_) => {
            PeerPolicy::ExplicitScope(Scope::StreamLaunch)
        }
        RpcRequest::MoonlightCertificateRevoke(_) => PeerPolicy::OwnerDeviceOnly,
        RpcRequest::SessionPrepare(_) => PeerPolicy::ExplicitScope(Scope::StreamLaunch),
        RpcRequest::SessionStatus(_) => PeerPolicy::ExplicitScope(Scope::StreamLaunch),
        RpcRequest::SessionStop(request) if request.force.unwrap_or(false) => {
            PeerPolicy::OwnerDeviceOnly
        }
        RpcRequest::SessionStop(_) => PeerPolicy::ExplicitScope(Scope::StreamLaunch),
        RpcRequest::SessionFreeze(_) => PeerPolicy::ExplicitScope(Scope::StreamLaunch),
        RpcRequest::SessionThaw(_) => PeerPolicy::ExplicitScope(Scope::StreamLaunch),
        RpcRequest::SessionControls(_) => PeerPolicy::ExplicitScope(Scope::StreamLaunch),
        RpcRequest::SessionControlInvoke(_) => PeerPolicy::ExplicitScope(Scope::StreamLaunch),
        RpcRequest::LocalGamesList(_) => PeerPolicy::OwnerDeviceOnly,
        RpcRequest::LocalGameLaunch(_) => PeerPolicy::OwnerDeviceOnly,
        RpcRequest::Health(_) => PeerPolicy::OwnerDeviceOnly,
        RpcRequest::DiscoverySnapshot(_) => PeerPolicy::OwnerDeviceOnly,
        RpcRequest::DiscoveryRegisterReceipt(_) => PeerPolicy::OwnerDeviceOnly,
        RpcRequest::DiscoveryRemoveLocation(_) => PeerPolicy::OwnerDeviceOnly,
        RpcRequest::DiscoveryRescan(_) => PeerPolicy::OwnerDeviceOnly,
        RpcRequest::SettingsSnapshot(_) => PeerPolicy::OwnerDeviceOnly,
        RpcRequest::SettingsUpdate(_) => PeerPolicy::OwnerDeviceOnly,
        RpcRequest::SteamGridDbCredentialSet(_) => PeerPolicy::OwnerDeviceOnly,
        RpcRequest::SteamGridDbCredentialClear(_) => PeerPolicy::OwnerDeviceOnly,
    }
}

pub fn authorize(
    context: &AuthorizationContext,
    request: &RpcRequest,
) -> Result<(), AuthorizationDenied> {
    let allowed = match context {
        AuthorizationContext::LocalBrowser
        | AuthorizationContext::LocalUnixControl
        | AuthorizationContext::TrustReconciliation => true,
        AuthorizationContext::Peer(Principal::OwnerDevice { .. }) => true,
        AuthorizationContext::Peer(principal @ Principal::Household { .. })
        | AuthorizationContext::Peer(principal @ Principal::Guest { .. }) => {
            match policy_for(request) {
                PeerPolicy::OwnerDeviceOnly => false,
                PeerPolicy::ExplicitScope(scope) => principal.has_scope(scope),
                PeerPolicy::CatalogOrStreamScope => {
                    principal.has_scope(Scope::CatalogRead)
                        || principal.has_scope(Scope::StreamLaunch)
                }
            }
        }
        AuthorizationContext::Peer(Principal::Unknown { .. }) => false,
    };
    allowed.then_some(()).ok_or(AuthorizationDenied)
}

pub fn is_security_mutation(request: &RpcRequest) -> bool {
    matches!(
        request,
        RpcRequest::SessionStop(_)
            | RpcRequest::SessionFreeze(_)
            | RpcRequest::SessionThaw(_)
            | RpcRequest::MoonlightCertificateProvision(_)
            | RpcRequest::MoonlightCertificateRevoke(_)
            | RpcRequest::DiscoveryRegisterReceipt(_)
            | RpcRequest::DiscoveryRemoveLocation(_)
            | RpcRequest::DiscoveryRescan(_)
            | RpcRequest::SettingsUpdate(_)
            | RpcRequest::SteamGridDbCredentialSet(_)
            | RpcRequest::SteamGridDbCredentialClear(_)
    )
}

#[derive(Clone)]
pub struct Authorization {
    state: Arc<Mutex<AuthorizationState>>,
    revocation_directory: PathBuf,
    certificate_directory: PathBuf,
}

struct AuthorizationState {
    revocations: Vec<Revocation>,
}

#[derive(Clone)]
pub struct AuthorizationAttempt {
    context: AuthorizationContext,
    revocations: Vec<Revocation>,
}

impl AuthorizationAttempt {
    pub fn context(&self) -> &AuthorizationContext {
        &self.context
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CertificateRevocation {
    pub device_public_key: String,
    pub host_uuid: String,
    pub client_certificate: String,
}

#[derive(Clone, Debug)]
struct Revocation {
    event_id: String,
    event_json: String,
    author: String,
    target: RevocationTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RevocationTarget {
    OwnerDevice(String),
    PersonPass(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CertificateGrant {
    host_uuid: String,
    client_certificate: String,
    authorization: CertificateAuthorization,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum CertificateAuthorization {
    OwnerDevice {
        owner_statement: String,
    },
    PersonPass {
        owner_statement: String,
        person_pass: String,
    },
}

impl Authorization {
    pub fn load(private_state_root: &Path) -> Result<Self, AuthorizationError> {
        let revocation_directory = private_state_root.join(REVOCATION_DIRECTORY);
        let certificate_directory = private_state_root.join(CERTIFICATE_DIRECTORY);
        prepare_private_directory(&revocation_directory)?;
        prepare_private_directory(&certificate_directory)?;
        let revocations = load_revocations(&revocation_directory)?;
        Ok(Self {
            state: Arc::new(Mutex::new(AuthorizationState { revocations })),
            revocation_directory,
            certificate_directory,
        })
    }

    pub fn attempt(
        &self,
        local: &IdentityState,
        sender_device_public_key: &str,
        sender_owner_statement: Option<&str>,
        person_pass: Option<&str>,
        revocation_events: &[String],
        now: u64,
    ) -> Result<AuthorizationAttempt, AuthorizationError> {
        let candidate_revocations = revocation_events
            .iter()
            .map(|event| parse_revocation(event))
            .collect::<Result<Vec<_>, _>>()?;
        let mut revocations = self
            .state
            .lock()
            .map_err(|_| AuthorizationError::Storage)?
            .revocations
            .clone();
        revocations.extend(candidate_revocations.iter().cloned());
        let principal = principal_for(
            local,
            sender_device_public_key,
            sender_owner_statement,
            person_pass,
            now,
            &revocations,
        );
        Ok(AuthorizationAttempt {
            context: AuthorizationContext::Peer(principal),
            revocations: candidate_revocations,
        })
    }

    /// Persist already-verified revocations only after the carrying request is
    /// authorized and its replay nonce is accepted.
    pub fn commit_revocations(
        &self,
        attempt: &AuthorizationAttempt,
    ) -> Result<(), AuthorizationError> {
        if attempt.revocations.is_empty() {
            return Ok(());
        }
        let mut state = self.state.lock().map_err(|_| AuthorizationError::Storage)?;
        for revocation in &attempt.revocations {
            if state
                .revocations
                .iter()
                .any(|known| known.event_id == revocation.event_id)
            {
                continue;
            }
            write_private_atomically(
                &self
                    .revocation_directory
                    .join(format!("{}.json", revocation.event_id)),
                revocation.event_json.as_bytes(),
            )?;
            state.revocations.push(revocation.clone());
        }
        Ok(())
    }

    pub fn record_certificate(
        &self,
        principal: &Principal,
        host_uuid: &str,
        client_certificate: &str,
    ) -> Result<(), AuthorizationError> {
        let (device_public_key, authorization) = match principal {
            Principal::OwnerDevice {
                device_public_key,
                owner_statement,
            } => (
                device_public_key,
                CertificateAuthorization::OwnerDevice {
                    owner_statement: owner_statement.clone(),
                },
            ),
            Principal::Household {
                device_public_key,
                owner_statement,
                person_pass,
            }
            | Principal::Guest {
                device_public_key,
                owner_statement,
                person_pass,
            } => (
                device_public_key,
                CertificateAuthorization::PersonPass {
                    owner_statement: owner_statement.clone(),
                    person_pass: person_pass.event_json.clone(),
                },
            ),
            Principal::Unknown { .. } => return Err(AuthorizationError::InvalidEvidence),
        };
        let grant = CertificateGrant {
            host_uuid: host_uuid.into(),
            client_certificate: client_certificate.into(),
            authorization,
        };
        let bytes = serde_json::to_vec(&grant).map_err(|_| AuthorizationError::Storage)?;
        write_private_atomically(&self.certificate_path(device_public_key), &bytes)
    }

    pub fn certificate_revocations(
        &self,
        local: &IdentityState,
        now: u64,
    ) -> Result<Vec<CertificateRevocation>, AuthorizationError> {
        let revocations = self
            .state
            .lock()
            .map_err(|_| AuthorizationError::Storage)?
            .revocations
            .clone();
        let mut grants = Vec::new();
        for entry in
            fs::read_dir(&self.certificate_directory).map_err(|_| AuthorizationError::Storage)?
        {
            let entry = entry.map_err(|_| AuthorizationError::Storage)?;
            let metadata =
                fs::symlink_metadata(entry.path()).map_err(|_| AuthorizationError::Storage)?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.permissions().mode() & 0o077 != 0
                || metadata.len() > MAX_GRANT_BYTES as u64
            {
                return Err(AuthorizationError::Storage);
            }
            let file_name = entry
                .file_name()
                .into_string()
                .map_err(|_| AuthorizationError::Storage)?;
            if file_name.starts_with('.') {
                return Err(AuthorizationError::Storage);
            }
            validate_hex_32(&file_name)?;
            let bytes = read_bounded(&entry.path(), MAX_GRANT_BYTES)?;
            let grant: CertificateGrant =
                serde_json::from_slice(&bytes).map_err(|_| AuthorizationError::Storage)?;
            grants.push((file_name, grant));
        }
        grants.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(plan_certificate_revocations(
            local,
            now,
            &revocations,
            grants,
        ))
    }

    pub fn complete_certificate_revocation(
        &self,
        device_public_key: &str,
    ) -> Result<(), AuthorizationError> {
        match fs::remove_file(self.certificate_path(device_public_key)) {
            Ok(()) => sync_directory(&self.certificate_directory),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(AuthorizationError::Storage),
        }
    }

    pub fn remove_certificate_grant(
        &self,
        device_public_key: &str,
    ) -> Result<(), AuthorizationError> {
        self.complete_certificate_revocation(device_public_key)
    }

    fn certificate_path(&self, device_public_key: &str) -> PathBuf {
        self.certificate_directory.join(device_public_key)
    }
}

fn principal_for(
    local: &IdentityState,
    sender_device_public_key: &str,
    sender_owner_statement: Option<&str>,
    person_pass: Option<&str>,
    now: u64,
    revocations: &[Revocation],
) -> Principal {
    let unknown = || Principal::Unknown {
        device_public_key: sender_device_public_key.into(),
    };
    let IdentityState::Owned {
        owner_public_key: local_owner,
        ..
    } = local
    else {
        return unknown();
    };
    let Some(owner_statement_json) = sender_owner_statement else {
        return unknown();
    };
    let Ok(owner_statement) =
        DeviceIdentity::verify_owner_statement(owner_statement_json, sender_device_public_key)
    else {
        return unknown();
    };
    if owner_statement.status != OwnerStatementStatus::Owned
        || is_owner_device_revoked(
            revocations,
            &owner_statement.owner_public_key,
            sender_device_public_key,
        )
    {
        return unknown();
    }
    if owner_statement.owner_public_key == *local_owner {
        return Principal::OwnerDevice {
            device_public_key: sender_device_public_key.into(),
            owner_statement: owner_statement_json.into(),
        };
    }
    let Some(pass_json) = person_pass else {
        return unknown();
    };
    let Some((tier, person_pass)) = parse_person_pass(
        pass_json,
        local_owner,
        sender_device_public_key,
        now,
        revocations,
    ) else {
        return unknown();
    };
    match tier {
        PassTier::Household => Principal::Household {
            device_public_key: sender_device_public_key.into(),
            owner_statement: owner_statement_json.into(),
            person_pass,
        },
        PassTier::Guest => Principal::Guest {
            device_public_key: sender_device_public_key.into(),
            owner_statement: owner_statement_json.into(),
            person_pass,
        },
    }
}

#[derive(Clone, Copy)]
enum PassTier {
    Household,
    Guest,
}

fn parse_person_pass(
    event_json: &str,
    expected_owner: &str,
    expected_device: &str,
    now: u64,
    revocations: &[Revocation],
) -> Option<(PassTier, PersonPass)> {
    let event = DeviceIdentity::verify_event(event_json).ok()?;
    if event.kind != PERSON_PASS_EVENT_KIND
        || event.author != expected_owner
        || !event.content.is_empty()
        || event.created_at > now.saturating_add(CLOCK_SKEW_SECONDS)
        || is_person_pass_revoked(revocations, expected_owner, &event.id)
    {
        return None;
    }
    if event.tags.len() < 5
        || event.tags[0].len() != 2
        || event.tags[0][0] != "d"
        || !event.tags[0][1].starts_with(PASS_ADDRESS_PREFIX)
        || event.tags[1].as_slice() != ["device", expected_device]
        || event.tags[2].len() != 2
        || event.tags[2][0] != "tier"
        || event.tags[3].len() != 2
        || event.tags[3][0] != "expires"
        || event.tags[4..]
            .iter()
            .any(|tag| tag.len() != 2 || tag[0] != "scope")
    {
        return None;
    }
    let address = event.tags[0][1].strip_prefix(PASS_ADDRESS_PREFIX)?;
    validate_hex_32(address).ok()?;
    let tier = match event.tags[2][1].as_str() {
        "household" => PassTier::Household,
        "guest" => PassTier::Guest,
        _ => return None,
    };
    let expires_at = event.tags[3][1].parse::<u64>().ok()?;
    if expires_at <= now
        || expires_at <= event.created_at
        || expires_at.saturating_sub(event.created_at) > MAX_PERSON_PASS_LIFETIME_SECONDS
    {
        return None;
    }
    let scopes = event.tags[4..]
        .iter()
        .map(|tag| Scope::parse(&tag[1]))
        .collect::<Option<Vec<_>>>()?;
    let unique = scopes.iter().copied().collect::<HashSet<_>>();
    if scopes.is_empty() || unique.len() != scopes.len() {
        return None;
    }
    Some((
        tier,
        PersonPass {
            event_id: event.id,
            event_json: event_json.into(),
            expires_at,
            scopes,
        },
    ))
}

fn parse_revocation(event_json: &str) -> Result<Revocation, AuthorizationError> {
    let event = DeviceIdentity::verify_event(event_json)
        .map_err(|_| AuthorizationError::InvalidEvidence)?;
    let target = match event.kind {
        OWNER_EVENT_KIND => {
            if !event.content.is_empty()
                || event.tags.len() != 3
                || event.tags[0].len() != 2
                || event.tags[1].len() != 2
                || event.tags[1][0] != "device"
                || event.tags[2].as_slice() != ["status", "revoked"]
            {
                return Err(AuthorizationError::InvalidEvidence);
            }
            let device = event.tags[1][1].clone();
            let expected_d = format!("org.korri.device-owner:{device}");
            if event.tags[0].as_slice() != ["d", expected_d.as_str()] {
                return Err(AuthorizationError::InvalidEvidence);
            }
            validate_hex_32(&device)?;
            RevocationTarget::OwnerDevice(device)
        }
        NIP_09_DELETION_KIND => {
            if !event.content.is_empty()
                || event.tags.len() != 2
                || event.tags[0].len() != 2
                || event.tags[0][0] != "e"
                || event.tags[1].as_slice() != ["k", PERSON_PASS_EVENT_KIND.to_string().as_str()]
            {
                return Err(AuthorizationError::InvalidEvidence);
            }
            let pass_id = event.tags[0][1].clone();
            validate_hex_32(&pass_id)?;
            RevocationTarget::PersonPass(pass_id)
        }
        _ => return Err(AuthorizationError::InvalidEvidence),
    };
    Ok(Revocation {
        event_id: event.id,
        event_json: event_json.into(),
        author: event.author,
        target,
    })
}

fn is_owner_device_revoked(revocations: &[Revocation], owner: &str, device: &str) -> bool {
    revocations.iter().any(|revocation| {
        revocation.author == owner
            && revocation.target == RevocationTarget::OwnerDevice(device.into())
    })
}

fn is_person_pass_revoked(revocations: &[Revocation], owner: &str, pass_id: &str) -> bool {
    revocations.iter().any(|revocation| {
        revocation.author == owner
            && revocation.target == RevocationTarget::PersonPass(pass_id.into())
    })
}

fn plan_certificate_revocations(
    local: &IdentityState,
    now: u64,
    revocations: &[Revocation],
    grants: Vec<(String, CertificateGrant)>,
) -> Vec<CertificateRevocation> {
    grants
        .into_iter()
        .filter_map(|(device_public_key, grant)| {
            let principal = match &grant.authorization {
                CertificateAuthorization::OwnerDevice { owner_statement } => principal_for(
                    local,
                    &device_public_key,
                    Some(owner_statement),
                    None,
                    now,
                    revocations,
                ),
                CertificateAuthorization::PersonPass {
                    owner_statement,
                    person_pass,
                } => principal_for(
                    local,
                    &device_public_key,
                    Some(owner_statement),
                    Some(person_pass),
                    now,
                    revocations,
                ),
            };
            let context = AuthorizationContext::Peer(principal);
            let probe = crate::MoonlightCertificateProvisionRequest {
                host_uuid: grant.host_uuid.clone(),
                client_certificate: grant.client_certificate.clone(),
            };
            authorize(&context, &RpcRequest::MoonlightCertificateProvision(probe))
                .is_err()
                .then_some(CertificateRevocation {
                    device_public_key,
                    host_uuid: grant.host_uuid,
                    client_certificate: grant.client_certificate,
                })
        })
        .collect()
}

fn load_revocations(path: &Path) -> Result<Vec<Revocation>, AuthorizationError> {
    let mut revocations = Vec::new();
    for entry in fs::read_dir(path).map_err(|_| AuthorizationError::Storage)? {
        let entry = entry.map_err(|_| AuthorizationError::Storage)?;
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|_| AuthorizationError::Storage)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.permissions().mode() & 0o077 != 0
            || metadata.len() > MAX_EVENT_BYTES as u64
        {
            return Err(AuthorizationError::Storage);
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| AuthorizationError::Storage)?;
        let event_id = name
            .strip_suffix(".json")
            .ok_or(AuthorizationError::Storage)?;
        validate_hex_32(event_id)?;
        let bytes = read_bounded(&entry.path(), MAX_EVENT_BYTES)?;
        let event_json = std::str::from_utf8(&bytes).map_err(|_| AuthorizationError::Storage)?;
        let revocation = parse_revocation(event_json)?;
        if revocation.event_id != event_id {
            return Err(AuthorizationError::Storage);
        }
        revocations.push(revocation);
    }
    revocations.sort_by(|left, right| left.event_id.cmp(&right.event_id));
    Ok(revocations)
}

fn prepare_private_directory(path: &Path) -> Result<(), AuthorizationError> {
    if !path.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(|_| AuthorizationError::Storage)?;
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| AuthorizationError::Storage)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(AuthorizationError::Storage);
    }
    Ok(())
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, AuthorizationError> {
    let file = File::open(path).map_err(|_| AuthorizationError::Storage)?;
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| AuthorizationError::Storage)?;
    if bytes.len() > limit {
        return Err(AuthorizationError::Storage);
    }
    Ok(bytes)
}

fn write_private_atomically(path: &Path, content: &[u8]) -> Result<(), AuthorizationError> {
    let parent = path.parent().ok_or(AuthorizationError::Storage)?;
    let digest = Sha256::digest(content);
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        hex::encode(digest),
        rand::random::<u64>()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| AuthorizationError::Storage)?;
        file.write_all(content)
            .map_err(|_| AuthorizationError::Storage)?;
        file.sync_all().map_err(|_| AuthorizationError::Storage)?;
        fs::rename(&temporary, path).map_err(|_| AuthorizationError::Storage)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sync_directory(path: &Path) -> Result<(), AuthorizationError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| AuthorizationError::Storage)
}

fn validate_hex_32(value: &str) -> Result<(), AuthorizationError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(AuthorizationError::InvalidEvidence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use nostr::{
        event::{EventBuilder, FinalizeEvent, Kind, Tag},
        key::Keys,
        types::Timestamp,
    };

    const NOW: u64 = 1_700_000_000;
    const OWNER: &str = "0000000000000000000000000000000000000000000000000000000000000003";
    const OTHER_OWNER: &str = "0000000000000000000000000000000000000000000000000000000000000004";
    const DEVICE: &str = "0000000000000000000000000000000000000000000000000000000000000006";

    fn local() -> IdentityState {
        IdentityState::Owned {
            device_public_key: "host".into(),
            owner_public_key: Keys::parse(OWNER).unwrap().public_key().to_hex(),
            event_id: "event".into(),
            created_at: 1,
        }
    }

    fn owner_statement(owner: &Keys, device: &str, status: &str, created_at: u64) -> String {
        EventBuilder::new(Kind::Custom(OWNER_EVENT_KIND), "")
            .tags([
                Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
                Tag::parse(["device", device]).unwrap(),
                Tag::parse(["status", status]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(created_at))
            .finalize(owner)
            .unwrap()
            .as_json()
    }

    fn pass(owner: &Keys, tier: &str, scopes: &[&str], expires_at: u64) -> String {
        let mut tags = vec![
            Tag::parse(["d", &format!("{PASS_ADDRESS_PREFIX}{}", "11".repeat(32))]).unwrap(),
            Tag::parse(["device", DEVICE]).unwrap(),
            Tag::parse(["tier", tier]).unwrap(),
            Tag::parse(["expires", &expires_at.to_string()]).unwrap(),
        ];
        tags.extend(
            scopes
                .iter()
                .map(|scope| Tag::parse(["scope", *scope]).unwrap()),
        );
        EventBuilder::new(Kind::Custom(PERSON_PASS_EVENT_KIND), "")
            .tags(tags)
            .custom_created_at(Timestamp::from(NOW))
            .finalize(owner)
            .unwrap()
            .as_json()
    }

    fn pass_revocation(owner: &Keys, pass_event_id: &str) -> String {
        EventBuilder::new(Kind::Custom(NIP_09_DELETION_KIND), "")
            .tags([
                Tag::parse(["e", pass_event_id]).unwrap(),
                Tag::parse(["k", &PERSON_PASS_EVENT_KIND.to_string()]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(NOW + 1))
            .finalize(owner)
            .unwrap()
            .as_json()
    }

    fn request(json: serde_json::Value) -> RpcRequest {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn every_current_request_tag_has_one_explicit_policy() {
        use PeerPolicy::{
            CatalogOrStreamScope as Both, ExplicitScope as Scope, OwnerDeviceOnly as Owner,
        };
        let stream = Scope(super::Scope::StreamLaunch);
        let cases = vec![
            (
                request(serde_json::json!({"_tag":"app.catalog.snapshot","payload":{}})),
                Both,
            ),
            (
                request(serde_json::json!({"_tag":"app.moonlight.resolve","payload":{}})),
                stream,
            ),
            (
                request(
                    serde_json::json!({"_tag":"app.moonlight.launch.prepare","payload":{"hostUuid":"h","appId":1}}),
                ),
                stream,
            ),
            (
                request(
                    serde_json::json!({"_tag":"app.moonlight.launch.cancel","payload":{"launchId":"l"}}),
                ),
                Owner,
            ),
            (
                request(
                    serde_json::json!({"_tag":"app.moonlight.certificate.attest","payload":{"hostUuid":"h"}}),
                ),
                stream,
            ),
            (
                request(
                    serde_json::json!({"_tag":"app.moonlight.certificate.provision","payload":{"hostUuid":"h","clientCertificate":"c"}}),
                ),
                stream,
            ),
            (
                request(
                    serde_json::json!({"_tag":"app.moonlight.certificate.revoke","payload":{"hostUuid":"h","clientCertificate":"c"}}),
                ),
                Owner,
            ),
            (
                request(serde_json::json!({"_tag":"app.session.prepare","payload":{"gameId":"g"}})),
                stream,
            ),
            (
                request(serde_json::json!({"_tag":"app.session.status","payload":{}})),
                stream,
            ),
            (
                request(serde_json::json!({"_tag":"app.session.stop","payload":{}})),
                stream,
            ),
            (
                request(serde_json::json!({"_tag":"app.session.freeze","payload":{}})),
                stream,
            ),
            (
                request(serde_json::json!({"_tag":"app.session.thaw","payload":{}})),
                stream,
            ),
            (
                request(
                    serde_json::json!({"_tag":"app.session.controls","payload":{"launchId":"l"}}),
                ),
                stream,
            ),
            (
                request(
                    serde_json::json!({"_tag":"app.session.control.invoke","payload":{"launchId":"l","controlId":"c"}}),
                ),
                stream,
            ),
            (
                request(serde_json::json!({"_tag":"app.local-games.list","payload":{}})),
                Owner,
            ),
            (
                request(
                    serde_json::json!({"_tag":"app.local-games.launch","payload":{"gameId":"g"}}),
                ),
                Owner,
            ),
            (
                request(serde_json::json!({"_tag":"system.health","payload":{}})),
                Owner,
            ),
            (
                request(serde_json::json!({"_tag":"app.discovery.snapshot","payload":{}})),
                Owner,
            ),
            (
                request(
                    serde_json::json!({"_tag":"app.discovery.registerReceipt","payload":{"receipt":"r"}}),
                ),
                Owner,
            ),
            (
                request(
                    serde_json::json!({"_tag":"app.discovery.removeLocation","payload":{"locationId":"l"}}),
                ),
                Owner,
            ),
            (
                request(serde_json::json!({"_tag":"app.discovery.rescan","payload":{}})),
                Owner,
            ),
            (
                request(serde_json::json!({"_tag":"system.settings.snapshot","payload":{}})),
                Owner,
            ),
            (
                request(
                    serde_json::json!({"_tag":"system.settings.update","payload":{"expectedRevision":"r","settingId":"s","value":"v"}}),
                ),
                Owner,
            ),
            (
                request(
                    serde_json::json!({"_tag":"system.settings.steamgriddbCredential.set","payload":{"token":"t"}}),
                ),
                Owner,
            ),
            (
                request(
                    serde_json::json!({"_tag":"system.settings.steamgriddbCredential.clear","payload":{}}),
                ),
                Owner,
            ),
        ];
        assert_eq!(cases.len(), 25);
        for (request, expected) in cases {
            assert_eq!(policy_for(&request), expected);
        }
    }

    #[test]
    fn owner_receives_every_action_and_unknown_receives_none() {
        let owner = AuthorizationContext::Peer(Principal::OwnerDevice {
            device_public_key: DEVICE.into(),
            owner_statement: "event".into(),
        });
        let unknown = AuthorizationContext::Peer(Principal::Unknown {
            device_public_key: DEVICE.into(),
        });
        let requests = [
            request(serde_json::json!({"_tag":"app.catalog.snapshot","payload":{}})),
            request(serde_json::json!({"_tag":"system.settings.snapshot","payload":{}})),
            request(
                serde_json::json!({"_tag":"app.moonlight.certificate.provision","payload":{"hostUuid":"h","clientCertificate":"c"}}),
            ),
            request(serde_json::json!({"_tag":"app.session.stop","payload":{"force":true}})),
        ];
        for request in requests {
            assert!(authorize(&owner, &request).is_ok());
            assert!(authorize(&unknown, &request).is_err());
        }
    }

    #[test]
    fn household_and_guest_need_an_explicit_scope_and_stream_launch_is_the_guest_bundle() {
        let local_owner = Keys::parse(OWNER).unwrap();
        let device_owner = Keys::parse(OTHER_OWNER).unwrap();
        let statement = owner_statement(&device_owner, DEVICE, "owned", NOW);
        let root = tempfile::tempdir().unwrap();
        let authorization = Authorization::load(root.path()).unwrap();
        for tier in ["household", "guest"] {
            let pass = pass(
                &local_owner,
                tier,
                &[STREAM_LAUNCH_SCOPE],
                NOW + MAX_PERSON_PASS_LIFETIME_SECONDS,
            );
            let attempt = authorization
                .attempt(&local(), DEVICE, Some(&statement), Some(&pass), &[], NOW)
                .unwrap();
            let allowed = [
                request(serde_json::json!({"_tag":"app.catalog.snapshot","payload":{}})),
                request(serde_json::json!({"_tag":"app.moonlight.resolve","payload":{}})),
                request(
                    serde_json::json!({"_tag":"app.moonlight.launch.prepare","payload":{"hostUuid":"h","appId":1}}),
                ),
                request(serde_json::json!({"_tag":"app.session.prepare","payload":{"gameId":"g"}})),
                request(serde_json::json!({"_tag":"app.session.status","payload":{}})),
                request(serde_json::json!({"_tag":"app.session.stop","payload":{}})),
                request(
                    serde_json::json!({"_tag":"app.session.controls","payload":{"launchId":"l"}}),
                ),
                request(
                    serde_json::json!({"_tag":"app.session.control.invoke","payload":{"launchId":"l","controlId":"c"}}),
                ),
                request(
                    serde_json::json!({"_tag":"app.moonlight.certificate.attest","payload":{"hostUuid":"h"}}),
                ),
                request(
                    serde_json::json!({"_tag":"app.moonlight.certificate.provision","payload":{"hostUuid":"h","clientCertificate":"c"}}),
                ),
            ];
            for request in allowed {
                assert!(authorize(attempt.context(), &request).is_ok());
            }
            for request in [
                request(
                    serde_json::json!({"_tag":"app.moonlight.certificate.revoke","payload":{"hostUuid":"h","clientCertificate":"c"}}),
                ),
                request(serde_json::json!({"_tag":"system.health","payload":{}})),
                request(serde_json::json!({"_tag":"system.settings.snapshot","payload":{}})),
                request(serde_json::json!({"_tag":"app.session.stop","payload":{"force":true}})),
            ] {
                assert!(authorize(attempt.context(), &request).is_err());
            }
        }
    }

    #[test]
    fn unowned_revoked_invalid_expired_and_revoked_passes_are_unknown() {
        let local_owner = Keys::parse(OWNER).unwrap();
        let device_owner = Keys::parse(OTHER_OWNER).unwrap();
        let owned = owner_statement(&device_owner, DEVICE, "owned", NOW);
        let revoked = owner_statement(&device_owner, DEVICE, "revoked", NOW + 1);
        let root = tempfile::tempdir().unwrap();
        let authorization = Authorization::load(root.path()).unwrap();
        let valid_pass = pass(&local_owner, "guest", &[STREAM_LAUNCH_SCOPE], NOW + 60);
        let pass_id = DeviceIdentity::verify_event(&valid_pass).unwrap().id;
        let pass_revocation = pass_revocation(&local_owner, &pass_id);
        let states = [
            IdentityState::Unowned {
                device_public_key: "host".into(),
            },
            IdentityState::Revoked {
                device_public_key: "host".into(),
                owner_public_key: local_owner.public_key().to_hex(),
                event_id: "event".into(),
                created_at: NOW,
            },
            IdentityState::Invalid {
                reason: "invalid".into(),
            },
        ];
        for state in states {
            let attempt = authorization
                .attempt(&state, DEVICE, Some(&owned), Some(&valid_pass), &[], NOW)
                .unwrap();
            assert!(matches!(
                attempt.context(),
                AuthorizationContext::Peer(Principal::Unknown { .. })
            ));
        }
        let revoked_device = authorization
            .attempt(
                &local(),
                DEVICE,
                Some(&revoked),
                Some(&valid_pass),
                &[],
                NOW,
            )
            .unwrap();
        assert!(matches!(
            revoked_device.context(),
            AuthorizationContext::Peer(Principal::Unknown { .. })
        ));
        let expired = authorization
            .attempt(
                &local(),
                DEVICE,
                Some(&owned),
                Some(&valid_pass),
                &[],
                NOW + 60,
            )
            .unwrap();
        assert!(matches!(
            expired.context(),
            AuthorizationContext::Peer(Principal::Unknown { .. })
        ));
        let revoked_pass = authorization
            .attempt(
                &local(),
                DEVICE,
                Some(&owned),
                Some(&valid_pass),
                &[pass_revocation],
                NOW,
            )
            .unwrap();
        assert!(matches!(
            revoked_pass.context(),
            AuthorizationContext::Peer(Principal::Unknown { .. })
        ));
    }

    #[test]
    fn reconciliation_is_sorted_and_returns_the_exact_certificate() {
        let local_owner = Keys::parse(OWNER).unwrap();
        let device_owner = Keys::parse(OTHER_OWNER).unwrap();
        let statement = owner_statement(&device_owner, DEVICE, "owned", NOW);
        let root = tempfile::tempdir().unwrap();
        let authorization = Authorization::load(root.path()).unwrap();
        let pass = pass(&local_owner, "guest", &[STREAM_LAUNCH_SCOPE], NOW + 10);
        let attempt = authorization
            .attempt(&local(), DEVICE, Some(&statement), Some(&pass), &[], NOW)
            .unwrap();
        let AuthorizationContext::Peer(principal) = attempt.context() else {
            panic!("peer principal")
        };
        authorization
            .record_certificate(principal, "sunshine-host", "exact-certificate")
            .unwrap();
        let plan = authorization
            .certificate_revocations(&local(), NOW + 10)
            .unwrap();
        assert_eq!(
            plan,
            vec![CertificateRevocation {
                device_public_key: DEVICE.into(),
                host_uuid: "sunshine-host".into(),
                client_certificate: "exact-certificate".into(),
            }]
        );
    }
}
