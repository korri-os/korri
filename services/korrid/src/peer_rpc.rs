use crate::{
    authorization::{self, DomainAction},
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
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
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
const NONCE_BYTES: usize = 32;

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
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PeerRequestEnvelope {
    version: u8,
    recipient: String,
    nonce: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner_statement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pass: Option<String>,
    request: RpcRequest,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PeerResponseEnvelope {
    version: u8,
    recipient: String,
    request_event_id: String,
    request_nonce: String,
    response: RpcResponse,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProvisionedCertificate {
    host_uuid: String,
    client_certificate: String,
}

#[derive(Clone)]
pub struct PeerCredentials {
    identity: Arc<Mutex<DeviceIdentity>>,
    pass: Option<String>,
}

impl PeerCredentials {
    pub fn load(private_state_root: &Path, pass: Option<String>) -> Result<Self, PeerRpcError> {
        let identity = DeviceIdentity::load_or_create(private_state_root)
            .map_err(|_| PeerRpcError::Identity)?;
        Ok(Self {
            identity: Arc::new(Mutex::new(identity)),
            pass,
        })
    }

    #[cfg(test)]
    pub fn from_identity(identity: DeviceIdentity, pass: Option<String>) -> Self {
        Self {
            identity: Arc::new(Mutex::new(identity)),
            pass,
        }
    }

    pub fn with_pass(&self, pass: Option<String>) -> Self {
        Self {
            identity: self.identity.clone(),
            pass,
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
        let nonce = random_nonce();
        self.encode_request_with_nonce(recipient, request, now, nonce)
    }

    #[cfg(test)]
    pub fn encode_request_with_nonce(
        &self,
        recipient: &str,
        request: RpcRequest,
        now: u64,
        nonce: String,
    ) -> Result<EncodedPeerRequest, PeerRpcError> {
        self.encode_request_inner(recipient, request, now, nonce)
    }

    #[cfg(not(test))]
    fn encode_request_with_nonce(
        &self,
        recipient: &str,
        request: RpcRequest,
        now: u64,
        nonce: String,
    ) -> Result<EncodedPeerRequest, PeerRpcError> {
        self.encode_request_inner(recipient, request, now, nonce)
    }

    fn encode_request_inner(
        &self,
        recipient: &str,
        request: RpcRequest,
        now: u64,
        nonce: String,
    ) -> Result<EncodedPeerRequest, PeerRpcError> {
        validate_nonce(&nonce)?;
        let identity = self.identity.lock().map_err(|_| PeerRpcError::Identity)?;
        if !matches!(identity.state(), IdentityState::Owned { .. }) {
            return Err(PeerRpcError::Identity);
        }
        let envelope = PeerRequestEnvelope {
            version: PEER_RPC_VERSION,
            recipient: recipient.into(),
            nonce: nonce.clone(),
            owner_statement: identity.owner_statement_json(),
            pass: self.pass.clone(),
            request,
        };
        let plaintext = serde_json::to_string(&envelope).map_err(|_| PeerRpcError::Invalid)?;
        let event = identity
            .encrypt_event(recipient, PEER_REQUEST_KIND, &plaintext, now)
            .map_err(|_| PeerRpcError::Identity)?;
        Ok(EncodedPeerRequest {
            event_json: event.json,
            event_id: event.id,
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
        let verified =
            DeviceIdentity::verify_event(event_json).map_err(|_| PeerRpcError::Invalid)?;
        if verified.kind != PEER_RESPONSE_KIND || verified.author != expected_peer {
            return Err(PeerRpcError::WrongKind);
        }
        check_time(verified.created_at, now)?;
        let identity = self.identity.lock().map_err(|_| PeerRpcError::Identity)?;
        let plaintext = identity
            .decrypt_event(event_json)
            .map_err(|_| PeerRpcError::Invalid)?;
        let response: PeerResponseEnvelope =
            serde_json::from_str(&plaintext).map_err(|_| PeerRpcError::Invalid)?;
        let own_key = identity.device_public_key().ok_or(PeerRpcError::Identity)?;
        if response.version != PEER_RPC_VERSION
            || response.recipient != own_key
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
    pub nonce: String,
    pub recipient: String,
}

#[derive(Clone)]
pub struct PeerRpcServer {
    app: AppState,
    identity: Arc<Mutex<DeviceIdentity>>,
    replay: Arc<ReplayGuard>,
    certificate_directory: PathBuf,
}

impl PeerRpcServer {
    pub fn new(app: AppState, private_state_root: &Path) -> Result<Self, PeerRpcError> {
        let identity = DeviceIdentity::load_or_create(private_state_root)
            .map_err(|_| PeerRpcError::Identity)?;
        Ok(Self {
            app,
            identity: Arc::new(Mutex::new(identity)),
            replay: Arc::new(ReplayGuard::new(private_state_root)?),
            certificate_directory: private_state_root.join("identity/peer-certificates"),
        })
    }

    pub fn router(self) -> Router {
        Router::new()
            .route("/peer-rpc", post(peer_rpc))
            .route("/rpc", post(reject_plaintext))
            .layer(DefaultBodyLimit::max(MAX_PEER_EVENT_BYTES))
            .with_state(self)
    }

    async fn handle(&self, event_json: &str, now: u64) -> Result<String, PeerRpcError> {
        let verified =
            DeviceIdentity::verify_event(event_json).map_err(|_| PeerRpcError::Invalid)?;
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
            identity
                .decrypt_event(event_json)
                .map_err(|_| PeerRpcError::Invalid)?
        };
        let envelope: PeerRequestEnvelope =
            serde_json::from_str(&plaintext).map_err(|_| PeerRpcError::Invalid)?;
        if envelope.version != PEER_RPC_VERSION || envelope.recipient != local_key {
            return Err(PeerRpcError::Binding);
        }
        validate_nonce(&envelope.nonce)?;
        let action = authorization::action_for(&envelope.request);
        let principal = authorization::principal_for(
            &local_state,
            &verified.author,
            envelope.owner_statement.as_deref(),
            envelope.pass.as_deref(),
            now,
        );
        if !authorization::authorize(&principal, action) {
            self.revoke_recorded_certificate(&verified.author).await;
            return Err(PeerRpcError::Unauthorized);
        }
        self.replay.consume(
            &verified.author,
            &envelope.nonce,
            verified.created_at,
            action,
        )?;

        let provision = provisioned_certificate(&envelope.request);
        let revocation = revoked_certificate(&envelope.request);
        let response = dispatch(&self.app, envelope.request).await;
        if let Some(certificate) = provision {
            if matches!(
                response,
                RpcResponse::MoonlightCertificateProvision(
                    MoonlightCertificateProvisionOutcome::Ok(_)
                )
            ) {
                self.record_certificate(&verified.author, &certificate)?;
            }
        }
        if revocation
            && matches!(
                response,
                RpcResponse::MoonlightCertificateRevoke(MoonlightCertificateRevokeOutcome::Ok(_))
            )
        {
            self.remove_recorded_certificate(&verified.author)?;
        }
        let response_envelope = PeerResponseEnvelope {
            version: PEER_RPC_VERSION,
            recipient: verified.author.clone(),
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

    fn certificate_path(&self, device_public_key: &str) -> PathBuf {
        self.certificate_directory.join(device_public_key)
    }

    fn record_certificate(
        &self,
        device_public_key: &str,
        certificate: &ProvisionedCertificate,
    ) -> Result<(), PeerRpcError> {
        prepare_private_directory(&self.certificate_directory)?;
        let path = self.certificate_path(device_public_key);
        let temporary = self
            .certificate_directory
            .join(format!(".{device_public_key}.tmp"));
        let bytes = serde_json::to_vec(certificate).map_err(|_| PeerRpcError::Storage)?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| PeerRpcError::Storage)?;
        file.write_all(&bytes).map_err(|_| PeerRpcError::Storage)?;
        file.sync_all().map_err(|_| PeerRpcError::Storage)?;
        fs::rename(temporary, path).map_err(|_| PeerRpcError::Storage)
    }

    fn remove_recorded_certificate(&self, device_public_key: &str) -> Result<(), PeerRpcError> {
        match fs::remove_file(self.certificate_path(device_public_key)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(PeerRpcError::Storage),
        }
    }

    async fn revoke_recorded_certificate(&self, device_public_key: &str) {
        let path = self.certificate_path(device_public_key);
        let Ok(bytes) = fs::read(&path) else {
            return;
        };
        let Ok(certificate) = serde_json::from_slice::<ProvisionedCertificate>(&bytes) else {
            return;
        };
        let response = dispatch(
            &self.app,
            RpcRequest::MoonlightCertificateRevoke(MoonlightCertificateRevokeRequest {
                host_uuid: certificate.host_uuid,
                client_certificate: certificate.client_certificate,
            }),
        )
        .await;
        if matches!(
            response,
            RpcResponse::MoonlightCertificateRevoke(MoonlightCertificateRevokeOutcome::Ok(_))
        ) {
            let _ = fs::remove_file(path);
        }
    }
}

async fn peer_rpc(State(state): State<PeerRpcServer>, body: Bytes) -> Response {
    let event_json = match std::str::from_utf8(&body) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    match state.handle(event_json, unix_time()).await {
        Ok(response) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            response,
        )
            .into_response(),
        Err(PeerRpcError::Unauthorized) => StatusCode::FORBIDDEN.into_response(),
        Err(PeerRpcError::Replay) => StatusCode::CONFLICT.into_response(),
        Err(PeerRpcError::Time) => StatusCode::UNAUTHORIZED.into_response(),
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
        action: DomainAction,
    ) -> Result<(), PeerRpcError> {
        if action.is_security_mutation() {
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

fn random_nonce() -> String {
    let bytes: [u8; NONCE_BYTES] = rand::random();
    hex::encode(bytes)
}

fn validate_nonce(nonce: &str) -> Result<(), PeerRpcError> {
    if nonce.len() != NONCE_BYTES * 2
        || !nonce
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
    PeerCredentials::from_identity(
        test_owned_identity(private_state_root, device_secret, owner_secret),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_is_exact_lowercase_hex() {
        let nonce = random_nonce();
        assert_eq!(nonce.len(), NONCE_BYTES * 2);
        validate_nonce(&nonce).unwrap();
        assert!(validate_nonce(&nonce.to_uppercase()).is_err());
        assert!(validate_nonce("short").is_err());
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
        let app = crate::secure_host_router_with_in_memory_units(&config, host_root.path());
        let base = serve(app).await;
        let client =
            crate::upstream_native::NativeClient::new_secure(base.clone(), host_key, credentials);
        let catalog = client.catalog_snapshot().await.unwrap();
        assert_eq!(catalog.games[0].id, "neverball");

        let plaintext =
            serde_json::to_string(&RpcRequest::Health(crate::HealthRequest {})).unwrap();
        assert_eq!(
            post(&base, "/rpc", plaintext).await.status(),
            StatusCode::UPGRADE_REQUIRED
        );
    }

    #[tokio::test]
    async fn rejects_tamper_wrong_recipient_stale_future_reflection_and_unknown_owner() {
        let (host_root, _client_root, credentials, host_key, config) = setup();
        let base = serve(crate::secure_host_router_with_in_memory_units(
            &config,
            host_root.path(),
        ))
        .await;
        let now = unix_time();
        let health = || RpcRequest::Health(crate::HealthRequest {});

        for timestamp in [
            now - CLOCK_WINDOW_SECONDS - 1,
            now + CLOCK_WINDOW_SECONDS + 1,
        ] {
            let encoded = credentials
                .encode_request_with_nonce(&host_key, health(), timestamp, random_nonce())
                .unwrap();
            assert_eq!(
                post(&base, "/peer-rpc", encoded.event_json).await.status(),
                StatusCode::UNAUTHORIZED
            );
        }

        let other_root = tempfile::tempdir().unwrap();
        let other = test_owned_identity(other_root.path(), OTHER, OWNER);
        let wrong_recipient = credentials
            .encode_request_with_nonce(
                other.device_public_key().unwrap(),
                health(),
                now,
                random_nonce(),
            )
            .unwrap();
        assert_eq!(
            post(&base, "/peer-rpc", wrong_recipient.event_json)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );

        let encoded = credentials
            .encode_request_with_nonce(&host_key, health(), now, random_nonce())
            .unwrap();
        let mut tampered: serde_json::Value = serde_json::from_str(&encoded.event_json).unwrap();
        tampered["content"] = serde_json::Value::String("changed".into());
        assert_eq!(
            post(&base, "/peer-rpc", tampered.to_string())
                .await
                .status(),
            StatusCode::BAD_REQUEST
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
            .encode_request_with_nonce(&host_key, health(), now, random_nonce())
            .unwrap();
        assert_eq!(
            post(&base, "/peer-rpc", unknown.event_json).await.status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn security_mutation_nonce_survives_server_restart() {
        let (host_root, _client_root, credentials, host_key, config) = setup();
        let now = unix_time();
        let encoded = credentials
            .encode_request_with_nonce(
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
                random_nonce(),
            )
            .unwrap();
        let first = serve(crate::secure_host_router_with_in_memory_units(
            &config,
            host_root.path(),
        ))
        .await;
        assert_eq!(
            post(&first, "/peer-rpc", encoded.event_json.clone())
                .await
                .status(),
            StatusCode::OK
        );
        let restarted = serve(crate::secure_host_router_with_in_memory_units(
            &config,
            host_root.path(),
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
        let now = unix_time();
        let request = credentials
            .encode_request_with_nonce(
                &host_key,
                RpcRequest::Health(crate::HealthRequest {}),
                now,
                random_nonce(),
            )
            .unwrap();
        let host = DeviceIdentity::load_or_create(host_root.path()).unwrap();
        let response = PeerResponseEnvelope {
            version: PEER_RPC_VERSION,
            recipient: credentials.public_key().unwrap(),
            request_event_id: "00".repeat(32),
            request_nonce: request.nonce.clone(),
            response: RpcResponse::Health(crate::HealthOutcome::Ok(crate::Health {
                version: "test".into(),
            })),
        };
        let event = host
            .encrypt_event(
                &response.recipient,
                PEER_RESPONSE_KIND,
                &serde_json::to_string(&response).unwrap(),
                now,
            )
            .unwrap();
        assert!(matches!(
            credentials.decode_response(&host_key, &request, &event.json, now),
            Err(PeerRpcError::Binding)
        ));
    }
}
