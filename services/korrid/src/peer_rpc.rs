use crate::{
    authorization::{self, Authorization, AuthorizationContext},
    dispatch,
    identity::{DeviceIdentity, IdentityState},
    AppState, MoonlightCertificateProvisionOutcome, MoonlightCertificateRevokeOutcome,
    MoonlightCertificateRevokeRequest, RpcRequest, RpcResponse,
};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::io::Write;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

pub const PEER_RPC_VERSION: u8 = 1;
pub const PEER_REQUEST_KIND: u16 = 21_100;
pub const PEER_RESPONSE_KIND: u16 = 21_101;
pub const MAX_PEER_EVENT_BYTES: usize = 8 * 1024 * 1024;
const CLOCK_WINDOW_SECONDS: u64 = 120;
const TOKEN_BYTES: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum PeerRpcError {
    #[error("peer RPC body is invalid")]
    Invalid,
    #[error("peer RPC identity is unavailable")]
    Identity,
    #[error("peer RPC event has the wrong kind")]
    WrongKind,
    #[error("peer RPC sender is not authorized")]
    Unauthorized,
    #[error("peer RPC event time is outside the accepted window")]
    Time,
    #[error("peer RPC nonce was already used")]
    Replay,
    #[error("peer RPC response does not match the request")]
    Binding,
    #[error("peer RPC storage is unavailable")]
    Storage,
    #[error("peer certificate trust could not be reconciled")]
    Trust,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PeerRequestEnvelope {
    version: u8,
    recipient: String,
    request_id: String,
    nonce: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner_statement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    person_pass: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    revocations: Vec<String>,
    request: RpcRequest,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PeerResponseEnvelope {
    version: u8,
    recipient: String,
    request_id: String,
    request_event_id: String,
    request_nonce: String,
    response: RpcResponse,
}

struct ProvisionedCertificate {
    host_uuid: String,
    client_certificate: String,
}

#[derive(Clone)]
pub struct PeerCredentials {
    identity: Arc<Mutex<DeviceIdentity>>,
    person_pass: Option<String>,
    revocations: Vec<String>,
}

impl PeerCredentials {
    pub fn load(private_state_root: &Path) -> Result<Self, PeerRpcError> {
        let identity = DeviceIdentity::load_or_create(private_state_root)
            .map_err(|_| PeerRpcError::Identity)?;
        Ok(Self {
            identity: Arc::new(Mutex::new(identity)),
            person_pass: None,
            revocations: Vec::new(),
        })
    }

    #[cfg(test)]
    pub fn from_identity(identity: DeviceIdentity) -> Self {
        Self {
            identity: Arc::new(Mutex::new(identity)),
            person_pass: None,
            revocations: Vec::new(),
        }
    }

    pub fn with_person_pass(&self, person_pass: Option<String>) -> Self {
        Self {
            identity: Arc::clone(&self.identity),
            person_pass,
            revocations: self.revocations.clone(),
        }
    }

    pub fn with_revocations(&self, revocations: Vec<String>) -> Self {
        Self {
            identity: Arc::clone(&self.identity),
            person_pass: self.person_pass.clone(),
            revocations,
        }
    }

    pub fn public_key(&self) -> Result<String, PeerRpcError> {
        self.identity
            .lock()
            .map_err(|_| PeerRpcError::Identity)?
            .device_public_key()
            .map(str::to_owned)
            .ok_or(PeerRpcError::Identity)
    }

    pub fn encode_request(
        &self,
        recipient: &str,
        request: RpcRequest,
        now: u64,
    ) -> Result<EncodedPeerRequest, PeerRpcError> {
        self.encode_request_inner(recipient, request, now, random_token(), random_token())
    }

    #[cfg(test)]
    pub fn encode_request_with_tokens(
        &self,
        recipient: &str,
        request: RpcRequest,
        now: u64,
        request_id: String,
        nonce: String,
    ) -> Result<EncodedPeerRequest, PeerRpcError> {
        self.encode_request_inner(recipient, request, now, request_id, nonce)
    }

    fn encode_request_inner(
        &self,
        recipient: &str,
        request: RpcRequest,
        now: u64,
        request_id: String,
        nonce: String,
    ) -> Result<EncodedPeerRequest, PeerRpcError> {
        validate_token(&request_id)?;
        validate_token(&nonce)?;
        let identity = self.identity.lock().map_err(|_| PeerRpcError::Identity)?;
        if !matches!(identity.state(), IdentityState::Owned { .. }) {
            return Err(PeerRpcError::Identity);
        }
        let envelope = PeerRequestEnvelope {
            version: PEER_RPC_VERSION,
            recipient: recipient.into(),
            request_id: request_id.clone(),
            nonce: nonce.clone(),
            owner_statement: identity.owner_statement_json(),
            person_pass: self.person_pass.clone(),
            revocations: self.revocations.clone(),
            request,
        };
        let plaintext = serde_json::to_string(&envelope).map_err(|_| PeerRpcError::Invalid)?;
        let event = identity
            .encrypt_event(recipient, PEER_REQUEST_KIND, &plaintext, now)
            .map_err(|_| PeerRpcError::Identity)?;
        Ok(EncodedPeerRequest {
            event_json: event.json,
            event_id: event.id,
            request_id,
            nonce,
            recipient: recipient.into(),
        })
    }

    pub fn decode_response(
        &self,
        expected_peer: &str,
        request: &EncodedPeerRequest,
        event_json: &str,
        now: u64,
    ) -> Result<RpcResponse, PeerRpcError> {
        let verified = DeviceIdentity::verify_encrypted_event(event_json)
            .map_err(|_| PeerRpcError::Invalid)?;
        if verified.kind != PEER_RESPONSE_KIND || verified.author != expected_peer {
            return Err(PeerRpcError::WrongKind);
        }
        check_time(verified.created_at, now)?;
        let identity = self.identity.lock().map_err(|_| PeerRpcError::Identity)?;
        let own_key = identity.device_public_key().ok_or(PeerRpcError::Identity)?;
        if !has_exact_recipient(&verified.tags, own_key) {
            return Err(PeerRpcError::Binding);
        }
        let plaintext = identity
            .decrypt_event(event_json)
            .map_err(|_| PeerRpcError::Invalid)?;
        let response: PeerResponseEnvelope =
            serde_json::from_str(&plaintext).map_err(|_| PeerRpcError::Invalid)?;
        if response.version != PEER_RPC_VERSION
            || response.recipient != own_key
            || response.request_id != request.request_id
            || response.request_event_id != request.event_id
            || response.request_nonce != request.nonce
        {
            return Err(PeerRpcError::Binding);
        }
        Ok(response.response)
    }
}

pub struct EncodedPeerRequest {
    pub event_json: String,
    pub event_id: String,
    pub request_id: String,
    pub nonce: String,
    pub recipient: String,
}

type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

#[derive(Clone)]
pub struct PeerRpcServer {
    app: AppState,
    identity: Arc<Mutex<DeviceIdentity>>,
    replay: Arc<ReplayGuard>,
    authorization: Authorization,
    clock: Clock,
}

impl PeerRpcServer {
    pub fn new(app: AppState, private_state_root: &Path) -> Result<Self, PeerRpcError> {
        Self::new_with_clock(app, private_state_root, Arc::new(unix_time))
    }

    fn new_with_clock(
        app: AppState,
        private_state_root: &Path,
        clock: Clock,
    ) -> Result<Self, PeerRpcError> {
        let identity = DeviceIdentity::load_or_create(private_state_root)
            .map_err(|_| PeerRpcError::Identity)?;
        Ok(Self {
            app,
            identity: Arc::new(Mutex::new(identity)),
            replay: Arc::new(ReplayGuard::new(private_state_root)?),
            authorization: Authorization::load(private_state_root)
                .map_err(|_| PeerRpcError::Storage)?,
            clock,
        })
    }

    #[cfg(test)]
    pub fn new_at(
        app: AppState,
        private_state_root: &Path,
        now: u64,
    ) -> Result<Self, PeerRpcError> {
        Self::new_with_clock(app, private_state_root, Arc::new(move || now))
    }

    pub fn router(self) -> Router {
        Router::new()
            .route("/peer-rpc", post(peer_rpc))
            .route("/rpc", post(reject_plaintext))
            .layer(DefaultBodyLimit::max(MAX_PEER_EVENT_BYTES))
            .with_state(self)
    }

    async fn handle(&self, event_json: &str, now: u64) -> Result<String, PeerRpcError> {
        let verified = DeviceIdentity::verify_encrypted_event(event_json)
            .map_err(|_| PeerRpcError::Invalid)?;
        if verified.kind != PEER_REQUEST_KIND {
            return Err(PeerRpcError::WrongKind);
        }
        check_time(verified.created_at, now)?;
        let local_state;
        let local_key;
        let plaintext = {
            let identity = self.identity.lock().map_err(|_| PeerRpcError::Identity)?;
            local_state = identity.state().clone();
            local_key = identity
                .device_public_key()
                .map(str::to_owned)
                .ok_or(PeerRpcError::Identity)?;
            if !has_exact_recipient(&verified.tags, &local_key) {
                return Err(PeerRpcError::Binding);
            }
            identity
                .decrypt_event(event_json)
                .map_err(|_| PeerRpcError::Invalid)?
        };
        let envelope: PeerRequestEnvelope =
            serde_json::from_str(&plaintext).map_err(|_| PeerRpcError::Invalid)?;
        if envelope.version != PEER_RPC_VERSION || envelope.recipient != local_key {
            return Err(PeerRpcError::Binding);
        }
        validate_token(&envelope.request_id)?;
        validate_token(&envelope.nonce)?;
        let attempt = self
            .authorization
            .attempt(
                &local_state,
                &verified.author,
                envelope.owner_statement.as_deref(),
                envelope.person_pass.as_deref(),
                &envelope.revocations,
                now,
            )
            .map_err(|_| PeerRpcError::Invalid)?;
        authorization::authorize(attempt.context(), &envelope.request)
            .map_err(|_| PeerRpcError::Unauthorized)?;
        self.replay.consume(
            &verified.author,
            &envelope.nonce,
            verified.created_at,
            authorization::is_security_mutation(&envelope.request),
        )?;
        self.authorization
            .commit_revocations(&attempt)
            .map_err(|_| PeerRpcError::Storage)?;
        self.reconcile_certificate_trust(&local_state, now).await?;

        let provision = provisioned_certificate(&envelope.request);
        let revocation = revoked_certificate(&envelope.request);
        let context = attempt.context().clone();
        let response = dispatch(&self.app, &context, envelope.request)
            .await
            .map_err(|_| PeerRpcError::Unauthorized)?;
        if let Some(certificate) = provision {
            if matches!(
                response,
                RpcResponse::MoonlightCertificateProvision(
                    MoonlightCertificateProvisionOutcome::Ok(_)
                )
            ) {
                let AuthorizationContext::Peer(principal) = &context else {
                    unreachable!("a peer attempt always has a peer context")
                };
                self.authorization
                    .record_certificate(
                        principal,
                        &certificate.host_uuid,
                        &certificate.client_certificate,
                    )
                    .map_err(|_| PeerRpcError::Storage)?;
            }
        }
        if revocation
            && matches!(
                response,
                RpcResponse::MoonlightCertificateRevoke(MoonlightCertificateRevokeOutcome::Ok(_))
            )
        {
            self.authorization
                .remove_certificate_grant(&verified.author)
                .map_err(|_| PeerRpcError::Storage)?;
        }
        let response_envelope = PeerResponseEnvelope {
            version: PEER_RPC_VERSION,
            recipient: verified.author.clone(),
            request_id: envelope.request_id,
            request_event_id: verified.id,
            request_nonce: envelope.nonce,
            response,
        };
        let plaintext =
            serde_json::to_string(&response_envelope).map_err(|_| PeerRpcError::Invalid)?;
        self.identity
            .lock()
            .map_err(|_| PeerRpcError::Identity)?
            .encrypt_event(&verified.author, PEER_RESPONSE_KIND, &plaintext, now)
            .map(|event| event.json)
            .map_err(|_| PeerRpcError::Identity)
    }

    async fn reconcile_certificate_trust(
        &self,
        local_state: &IdentityState,
        now: u64,
    ) -> Result<(), PeerRpcError> {
        let revocations = self
            .authorization
            .certificate_revocations(local_state, now)
            .map_err(|_| PeerRpcError::Storage)?;
        for revocation in revocations {
            let response = dispatch(
                &self.app,
                &AuthorizationContext::TrustReconciliation,
                RpcRequest::MoonlightCertificateRevoke(MoonlightCertificateRevokeRequest {
                    host_uuid: revocation.host_uuid,
                    client_certificate: revocation.client_certificate,
                }),
            )
            .await
            .map_err(|_| PeerRpcError::Trust)?;
            if !matches!(
                response,
                RpcResponse::MoonlightCertificateRevoke(MoonlightCertificateRevokeOutcome::Ok(_))
            ) {
                return Err(PeerRpcError::Trust);
            }
            self.authorization
                .complete_certificate_revocation(&revocation.device_public_key)
                .map_err(|_| PeerRpcError::Storage)?;
        }
        Ok(())
    }
}

async fn peer_rpc(State(state): State<PeerRpcServer>, body: Bytes) -> Response {
    let event_json = match std::str::from_utf8(&body) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let now = (state.clock)();
    match state.handle(event_json, now).await {
        Ok(response) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            response,
        )
            .into_response(),
        Err(PeerRpcError::Unauthorized) => StatusCode::FORBIDDEN.into_response(),
        Err(PeerRpcError::Replay) => StatusCode::CONFLICT.into_response(),
        Err(PeerRpcError::Time) => StatusCode::UNAUTHORIZED.into_response(),
        Err(PeerRpcError::Trust | PeerRpcError::Storage) => {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

async fn reject_plaintext() -> StatusCode {
    StatusCode::UPGRADE_REQUIRED
}

struct ReplayGuard {
    seen: Mutex<HashMap<(String, String), u64>>,
    persistent_directory: PathBuf,
}

impl ReplayGuard {
    fn new(private_state_root: &Path) -> Result<Self, PeerRpcError> {
        let persistent_directory = private_state_root.join("identity/security-replay");
        prepare_private_directory(&persistent_directory)?;
        Ok(Self {
            seen: Mutex::new(HashMap::new()),
            persistent_directory,
        })
    }

    fn consume(
        &self,
        sender: &str,
        nonce: &str,
        created_at: u64,
        persist: bool,
    ) -> Result<(), PeerRpcError> {
        if persist {
            let digest = Sha256::digest(format!("{sender}:{nonce}").as_bytes());
            let path = self.persistent_directory.join(hex::encode(digest));
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(path)
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        PeerRpcError::Replay
                    } else {
                        PeerRpcError::Storage
                    }
                })?;
        }
        let mut seen = self.seen.lock().map_err(|_| PeerRpcError::Storage)?;
        seen.retain(|_, time| created_at.saturating_sub(*time) <= CLOCK_WINDOW_SECONDS);
        if seen
            .insert((sender.into(), nonce.into()), created_at)
            .is_some()
        {
            return Err(PeerRpcError::Replay);
        }
        Ok(())
    }
}

fn has_exact_recipient(tags: &[Vec<String>], expected: &str) -> bool {
    tags.len() == 1 && tags[0].as_slice() == ["p", expected]
}

fn provisioned_certificate(request: &RpcRequest) -> Option<ProvisionedCertificate> {
    match request {
        RpcRequest::MoonlightCertificateProvision(request) => Some(ProvisionedCertificate {
            host_uuid: request.host_uuid.clone(),
            client_certificate: request.client_certificate.clone(),
        }),
        _ => None,
    }
}

fn revoked_certificate(request: &RpcRequest) -> bool {
    matches!(request, RpcRequest::MoonlightCertificateRevoke(_))
}

fn prepare_private_directory(path: &Path) -> Result<(), PeerRpcError> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path).map_err(|_| PeerRpcError::Storage)?;
        if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
            return Err(PeerRpcError::Storage);
        }
        return Ok(());
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(path).map_err(|_| PeerRpcError::Storage)
}

fn random_token() -> String {
    let bytes: [u8; TOKEN_BYTES] = rand::random();
    hex::encode(bytes)
}

fn validate_token(token: &str) -> Result<(), PeerRpcError> {
    if token.len() != TOKEN_BYTES * 2
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(PeerRpcError::Invalid);
    }
    Ok(())
}

fn check_time(created_at: u64, now: u64) -> Result<(), PeerRpcError> {
    if created_at.abs_diff(now) > CLOCK_WINDOW_SECONDS {
        return Err(PeerRpcError::Time);
    }
    Ok(())
}

pub fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
pub(crate) fn test_owned_identity(
    private_state_root: &Path,
    device_secret: &str,
    owner_secret: &str,
) -> DeviceIdentity {
    use nostr::{
        event::{EventBuilder, FinalizeEvent, Kind, Tag},
        key::Keys,
        types::Timestamp,
    };
    let directory = private_state_root.join("identity");
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(&directory).unwrap();
    let mut key = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(directory.join("device.key"))
        .unwrap();
    writeln!(key, "{device_secret}").unwrap();
    key.sync_all().unwrap();
    let mut identity = DeviceIdentity::load_or_create(private_state_root).unwrap();
    let device = identity.device_public_key().unwrap().to_owned();
    let owner = Keys::parse(owner_secret).unwrap();
    let event = EventBuilder::new(Kind::Custom(30_078), "")
        .tags([
            Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
            Tag::parse(["device", &device]).unwrap(),
            Tag::parse(["status", "owned"]).unwrap(),
        ])
        .custom_created_at(Timestamp::from(100))
        .finalize(&owner)
        .unwrap();
    identity.apply_owner_statement(&event.as_json()).unwrap();
    identity
}

#[cfg(test)]
pub(crate) fn test_owned_credentials(
    private_state_root: &Path,
    device_secret: &str,
    owner_secret: &str,
) -> PeerCredentials {
    PeerCredentials::from_identity(test_owned_identity(
        private_state_root,
        device_secret,
        owner_secret,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{
        event::{EventBuilder, FinalizeEvent, Kind, Tag},
        key::Keys,
        nips::nip44::{self, Version as Nip44Version},
        types::Timestamp,
    };

    const NOW: u64 = 1_700_000_000;

    fn token(byte: u8) -> String {
        hex::encode([byte; TOKEN_BYTES])
    }

    fn encrypted_event_with_recipients(
        sender_secret: &str,
        recipient_secret: &str,
        additional_recipient_secret: &str,
        kind: u16,
        plaintext: &str,
    ) -> String {
        let sender = Keys::parse(sender_secret).unwrap();
        let recipient = Keys::parse(recipient_secret).unwrap().public_key();
        let additional_recipient = Keys::parse(additional_recipient_secret)
            .unwrap()
            .public_key();
        let content = nip44::encrypt(
            sender.secret_key(),
            &recipient,
            plaintext.as_bytes(),
            Nip44Version::V2,
        )
        .unwrap();
        EventBuilder::new(Kind::Custom(kind), content)
            .tags([
                Tag::public_key(recipient),
                Tag::public_key(additional_recipient),
            ])
            .custom_created_at(Timestamp::from(NOW))
            .finalize(&sender)
            .unwrap()
            .as_json()
    }

    #[test]
    fn envelope_tokens_are_exact_lowercase_hex() {
        let value = random_token();
        assert_eq!(value.len(), TOKEN_BYTES * 2);
        validate_token(&value).unwrap();
        assert!(validate_token(&value.to_uppercase()).is_err());
        assert!(validate_token("short").is_err());
    }

    #[test]
    fn accepted_time_window_is_bounded_in_both_directions() {
        check_time(880, 1_000).unwrap();
        check_time(1_120, 1_000).unwrap();
        assert!(matches!(check_time(879, 1_000), Err(PeerRpcError::Time)));
        assert!(matches!(check_time(1_121, 1_000), Err(PeerRpcError::Time)));
    }

    const OWNER: &str = "0000000000000000000000000000000000000000000000000000000000000003";
    const OTHER_OWNER: &str = "0000000000000000000000000000000000000000000000000000000000000004";
    const HOST: &str = "0000000000000000000000000000000000000000000000000000000000000005";
    const CLIENT: &str = "0000000000000000000000000000000000000000000000000000000000000006";
    const OTHER: &str = "0000000000000000000000000000000000000000000000000000000000000007";

    fn host_config(root: &Path) -> PathBuf {
        let path = root.join("host.toml");
        fs::write(
            &path,
            "label = \"zao\"\n[[games]]\nid = \"neverball\"\ntitle = \"Neverball\"\ncommand = [\"neverball\"]\n",
        )
        .unwrap();
        path
    }

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    async fn post(base: &str, path: &str, body: String) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{base}{path}"))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .unwrap()
    }

    fn setup() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        PeerCredentials,
        String,
        PathBuf,
    ) {
        let host_root = tempfile::tempdir().unwrap();
        let client_root = tempfile::tempdir().unwrap();
        let host = test_owned_identity(host_root.path(), HOST, OWNER);
        let host_key = host.device_public_key().unwrap().to_owned();
        let client = test_owned_credentials(client_root.path(), CLIENT, OWNER);
        let config = host_config(host_root.path());
        (host_root, client_root, client, host_key, config)
    }

    #[tokio::test]
    async fn same_owner_round_trips_and_plaintext_has_no_fallback() {
        let (host_root, _client_root, credentials, host_key, config) = setup();
        let app = crate::secure_host_router_with_in_memory_units_at(&config, host_root.path(), NOW);
        let base = serve(app).await;
        let catalog_client = crate::upstream_native::NativeClient::new_secure_at(
            base.clone(),
            host_key.clone(),
            credentials.clone(),
            NOW,
            token(1),
            token(2),
        );
        let catalog = catalog_client.catalog_snapshot().await.unwrap();
        assert_eq!(catalog.games[0].id, "neverball");

        let prepare_client = crate::upstream_native::NativeClient::new_secure_at(
            base.clone(),
            host_key.clone(),
            credentials.clone(),
            NOW,
            token(3),
            token(4),
        );
        let prepared = prepare_client.prepare_stream("neverball").await.unwrap();
        let status_client = crate::upstream_native::NativeClient::new_secure_at(
            base.clone(),
            host_key.clone(),
            credentials.clone(),
            NOW,
            token(5),
            token(6),
        );
        let crate::upstream::UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = status_client.session_status().await.unwrap()
        else {
            panic!("secure peer status must report the prepared session")
        };
        assert_eq!(active.launch_id, prepared.launch_id);
        assert_eq!(active.game_id.as_deref(), Some("neverball"));
        let freeze_client = crate::upstream_native::NativeClient::new_secure_at(
            base.clone(),
            host_key.clone(),
            credentials.clone(),
            NOW,
            token(11),
            token(12),
        );
        let frozen = freeze_client
            .session_freeze(&prepared.launch_id)
            .await
            .unwrap();
        assert_eq!(frozen.launch_id, prepared.launch_id);
        assert_eq!(frozen.state, crate::SessionFreezerState::Frozen);
        assert!(frozen.changed);
        let stale_thaw_client = crate::upstream_native::NativeClient::new_secure_at(
            base.clone(),
            host_key.clone(),
            credentials.clone(),
            NOW,
            token(13),
            token(14),
        );
        assert!(matches!(
            stale_thaw_client
                .session_thaw("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                .await,
            Err(crate::upstreams::UpstreamError::Tagged { code, .. })
                if code == "StaleLaunchIdentity"
        ));
        let thaw_client = crate::upstream_native::NativeClient::new_secure_at(
            base.clone(),
            host_key.clone(),
            credentials.clone(),
            NOW,
            token(15),
            token(16),
        );
        let thawed = thaw_client.session_thaw(&prepared.launch_id).await.unwrap();
        assert_eq!(thawed.state, crate::SessionFreezerState::Running);
        assert!(thawed.changed);
        let stale_stop_client = crate::upstream_native::NativeClient::new_secure_at(
            base.clone(),
            host_key.clone(),
            credentials.clone(),
            NOW,
            token(7),
            token(8),
        );
        assert!(matches!(
            stale_stop_client
                .session_stop("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", false)
                .await,
            Err(crate::upstreams::UpstreamError::Tagged { code, .. })
                if code == "StaleLaunchIdentity"
        ));
        let stop_client = crate::upstream_native::NativeClient::new_secure_at(
            base.clone(),
            host_key,
            credentials,
            NOW,
            token(9),
            token(10),
        );
        assert!(matches!(
            stop_client
                .session_stop(&prepared.launch_id, false)
                .await
                .unwrap(),
            crate::upstream::UpstreamSessionStop::Stopped { .. }
        ));

        let plaintext =
            serde_json::to_string(&RpcRequest::Health(crate::HealthRequest {})).unwrap();
        assert_eq!(
            post(&base, "/rpc", plaintext).await.status(),
            StatusCode::UPGRADE_REQUIRED
        );
    }

    #[tokio::test]
    async fn peer_router_rejects_oversized_events_before_parsing() {
        let (host_root, _client_root, _credentials, _host_key, config) = setup();
        let base = serve(crate::secure_host_router_with_in_memory_units_at(
            &config,
            host_root.path(),
            NOW,
        ))
        .await;
        assert_eq!(
            post(&base, "/peer-rpc", "x".repeat(MAX_PEER_EVENT_BYTES + 1))
                .await
                .status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[tokio::test]
    async fn rejects_tamper_wrong_recipient_stale_future_reflection_and_unknown_owner() {
        let (host_root, _client_root, credentials, host_key, config) = setup();
        let base = serve(crate::secure_host_router_with_in_memory_units_at(
            &config,
            host_root.path(),
            NOW,
        ))
        .await;
        let now = NOW;
        let health = || RpcRequest::Health(crate::HealthRequest {});

        for (index, timestamp) in [
            now - CLOCK_WINDOW_SECONDS - 10,
            now + CLOCK_WINDOW_SECONDS + 10,
        ]
        .into_iter()
        .enumerate()
        {
            let encoded = credentials
                .encode_request_with_tokens(
                    &host_key,
                    health(),
                    timestamp,
                    token(10 + index as u8),
                    token(20 + index as u8),
                )
                .unwrap();
            assert_eq!(
                post(&base, "/peer-rpc", encoded.event_json).await.status(),
                StatusCode::UNAUTHORIZED
            );
        }

        let other_root = tempfile::tempdir().unwrap();
        let other = test_owned_identity(other_root.path(), OTHER, OWNER);
        let wrong_recipient = credentials
            .encode_request_with_tokens(
                other.device_public_key().unwrap(),
                health(),
                now,
                token(30),
                token(31),
            )
            .unwrap();
        assert_eq!(
            post(&base, "/peer-rpc", wrong_recipient.event_json)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );

        let owner_statement = credentials.identity.lock().unwrap().owner_statement_json();
        let multiple_recipients = encrypted_event_with_recipients(
            CLIENT,
            HOST,
            OTHER,
            PEER_REQUEST_KIND,
            &serde_json::to_string(&PeerRequestEnvelope {
                version: PEER_RPC_VERSION,
                recipient: host_key.clone(),
                request_id: token(35),
                nonce: token(36),
                owner_statement,
                person_pass: None,
                revocations: Vec::new(),
                request: health(),
            })
            .unwrap(),
        );
        assert_eq!(
            post(&base, "/peer-rpc", multiple_recipients).await.status(),
            StatusCode::BAD_REQUEST
        );

        let encoded = credentials
            .encode_request_with_tokens(&host_key, health(), now, token(40), token(41))
            .unwrap();
        let mut tampered: serde_json::Value = serde_json::from_str(&encoded.event_json).unwrap();
        tampered["content"] = serde_json::Value::String("changed".into());
        assert_eq!(
            post(&base, "/peer-rpc", tampered.to_string())
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );

        let replay = credentials
            .encode_request_with_tokens(&host_key, health(), now, token(42), token(43))
            .unwrap();
        assert_eq!(
            post(&base, "/peer-rpc", replay.event_json.clone())
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            post(&base, "/peer-rpc", replay.event_json).await.status(),
            StatusCode::CONFLICT
        );

        let reflection = {
            let host = DeviceIdentity::load_or_create(host_root.path()).unwrap();
            host.encrypt_event(
                credentials.public_key().unwrap().as_str(),
                PEER_RESPONSE_KIND,
                "{}",
                now,
            )
            .unwrap()
            .json
        };
        assert_eq!(
            post(&base, "/peer-rpc", reflection).await.status(),
            StatusCode::BAD_REQUEST
        );

        let stranger_root = tempfile::tempdir().unwrap();
        let stranger = test_owned_credentials(stranger_root.path(), OTHER, OTHER_OWNER);
        let unknown = stranger
            .encode_request_with_tokens(&host_key, health(), now, token(50), token(51))
            .unwrap();
        assert_eq!(
            post(&base, "/peer-rpc", unknown.event_json).await.status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn security_mutation_nonce_survives_server_restart() {
        let (host_root, _client_root, credentials, host_key, config) = setup();
        let now = NOW;
        let encoded = credentials
            .encode_request_with_tokens(
                &host_key,
                RpcRequest::MoonlightCertificateProvision(
                    crate::MoonlightCertificateProvisionRequest {
                        host_uuid: "sunshine-host".into(),
                        client_certificate:
                            "-----BEGIN CERTIFICATE-----\nclient\n-----END CERTIFICATE-----\n"
                                .into(),
                    },
                ),
                now,
                token(60),
                token(61),
            )
            .unwrap();
        let first = serve(crate::secure_host_router_with_in_memory_units_at(
            &config,
            host_root.path(),
            NOW,
        ))
        .await;
        assert_eq!(
            post(&first, "/peer-rpc", encoded.event_json.clone())
                .await
                .status(),
            StatusCode::OK
        );
        let restarted = serve(crate::secure_host_router_with_in_memory_units_at(
            &config,
            host_root.path(),
            NOW,
        ))
        .await;
        assert_eq!(
            post(&restarted, "/peer-rpc", encoded.event_json)
                .await
                .status(),
            StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn session_stop_nonce_survives_server_restart() {
        let (host_root, _client_root, credentials, host_key, config) = setup();
        let encoded = credentials
            .encode_request_with_tokens(
                &host_key,
                RpcRequest::SessionStop(crate::SessionStopRequest {
                    force: None,
                    expected_launch_id: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
                }),
                NOW,
                token(62),
                token(63),
            )
            .unwrap();
        let first = serve(crate::secure_host_router_with_in_memory_units_at(
            &config,
            host_root.path(),
            NOW,
        ))
        .await;
        assert_eq!(
            post(&first, "/peer-rpc", encoded.event_json.clone())
                .await
                .status(),
            StatusCode::OK
        );
        let restarted = serve(crate::secure_host_router_with_in_memory_units_at(
            &config,
            host_root.path(),
            NOW,
        ))
        .await;
        assert_eq!(
            post(&restarted, "/peer-rpc", encoded.event_json)
                .await
                .status(),
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn response_must_bind_request_id_nonce_sender_and_recipient() {
        let (host_root, _client_root, credentials, host_key, _config) = setup();
        let now = NOW;
        let request = credentials
            .encode_request_with_tokens(
                &host_key,
                RpcRequest::Health(crate::HealthRequest {}),
                now,
                token(70),
                token(71),
            )
            .unwrap();
        let host = DeviceIdentity::load_or_create(host_root.path()).unwrap();
        let response = PeerResponseEnvelope {
            version: PEER_RPC_VERSION,
            recipient: credentials.public_key().unwrap(),
            request_id: request.request_id.clone(),
            request_event_id: request.event_id.clone(),
            request_nonce: request.nonce.clone(),
            response: RpcResponse::Health(crate::HealthOutcome::Ok(crate::Health {
                version: "test".into(),
            })),
        };
        let mut wrong_request_id = response.clone();
        wrong_request_id.request_id = token(72);
        let mut wrong_event_id = response.clone();
        wrong_event_id.request_event_id = token(73);
        let mut wrong_nonce = response.clone();
        wrong_nonce.request_nonce = token(74);
        let mut wrong_recipient = response.clone();
        wrong_recipient.recipient = host_key.clone();
        for invalid in [
            wrong_request_id,
            wrong_event_id,
            wrong_nonce,
            wrong_recipient,
        ] {
            let event = host
                .encrypt_event(
                    credentials.public_key().unwrap().as_str(),
                    PEER_RESPONSE_KIND,
                    &serde_json::to_string(&invalid).unwrap(),
                    now,
                )
                .unwrap();
            assert!(matches!(
                credentials.decode_response(&host_key, &request, &event.json, now),
                Err(PeerRpcError::Binding)
            ));
        }

        let multiple_recipients = encrypted_event_with_recipients(
            HOST,
            CLIENT,
            OTHER,
            PEER_RESPONSE_KIND,
            &serde_json::to_string(&response).unwrap(),
        );
        assert!(matches!(
            credentials.decode_response(&host_key, &request, &multiple_recipients, now),
            Err(PeerRpcError::Binding)
        ));

        let stranger = Keys::parse(OTHER).unwrap();
        let wrong_sender = EventBuilder::new(
            Kind::Custom(PEER_RESPONSE_KIND),
            nip44::encrypt(
                stranger.secret_key(),
                &Keys::parse(CLIENT).unwrap().public_key(),
                serde_json::to_string(&response).unwrap().as_bytes(),
                Nip44Version::V2,
            )
            .unwrap(),
        )
        .tag(Tag::public_key(Keys::parse(CLIENT).unwrap().public_key()))
        .custom_created_at(Timestamp::from(now))
        .finalize(&stranger)
        .unwrap()
        .as_json();
        assert!(matches!(
            credentials.decode_response(&host_key, &request, &wrong_sender, now),
            Err(PeerRpcError::WrongKind)
        ));
    }
}
