//! NIP-46 remote person signer behind a Korri-owned signer contract.

use crate::{
    identity::{DeviceIdentity, Nip46ConnectionIdentity},
    relay::{CoordinatedRelays, PublishState, RelayError, RelayList, RelayTransport},
};
use futures::future::BoxFuture;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::{sleep, timeout, Instant};
use url::Url;

const NIP46_EVENT_KIND: u16 = 24_133;
const OWNER_EVENT_KIND: u16 = 30_078;
const REQUESTED_PERMISSIONS: &str = "sign_event:30078";
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// A new connection asks the person more than one signature does.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(300);
const POLL_DELAY: Duration = Duration::from_millis(50);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_CONNECTION_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PersonSignerState {
    Unavailable {
        message: String,
    },
    Pending {
        message: String,
    },
    Approved {
        owner_public_key: String,
        unsigned_event_template: String,
        signed_event_json: String,
    },
    Denied {
        message: String,
    },
    InvalidResponse {
        message: String,
    },
    Defect {
        message: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonSignerRequest {
    pub unsigned_event_template: String,
}

pub trait PersonSigner: Send + Sync {
    fn state(&self) -> PersonSignerState;
    fn request<'a>(&'a self, request: PersonSignerRequest) -> BoxFuture<'a, PersonSignerState>;
}

pub trait SignerRelayCoordinator: Send + Sync {
    fn publish<'a>(&'a self, event_json: &'a str) -> BoxFuture<'a, PublishState>;
    /// Packets to the client, from `remote_signer_public_key` when given.
    fn responses<'a>(
        &'a self,
        remote_signer_public_key: Option<&'a str>,
        client_public_key: &'a str,
        since: u64,
    ) -> BoxFuture<'a, Result<Vec<String>, RelayError>>;
}

impl<T> SignerRelayCoordinator for CoordinatedRelays<T>
where
    T: RelayTransport + 'static,
{
    fn publish<'a>(&'a self, event_json: &'a str) -> BoxFuture<'a, PublishState> {
        Box::pin(async move { self.publish_signer_packet(event_json).await })
    }

    fn responses<'a>(
        &'a self,
        remote_signer_public_key: Option<&'a str>,
        client_public_key: &'a str,
        since: u64,
    ) -> BoxFuture<'a, Result<Vec<String>, RelayError>> {
        Box::pin(async move {
            self.read_signer_packets(remote_signer_public_key, client_public_key, since)
                .await
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteSignerConnection {
    pub client_public_key: String,
    pub remote_signer_public_key: String,
    pub user_public_key: Option<String>,
    pub relays: Vec<String>,
    pub secret: Option<String>,
}

impl RemoteSignerConnection {
    pub fn from_bunker_uri(
        uri: &str,
        client_public_key: String,
    ) -> Result<Self, RemoteSignerError> {
        validate_public_key(&client_public_key)?;
        let (remote_signer_public_key, relays, secret) = parse_bunker_uri(uri)?;
        Ok(Self {
            client_public_key,
            remote_signer_public_key,
            user_public_key: None,
            relays,
            secret,
        })
    }

    /// NIP-46: the client reaches the signer on the relays the signer named,
    /// not on the device's own relay configuration.
    pub fn relay_list(&self) -> Result<RelayList, RemoteSignerError> {
        RelayList::configured(self.relays.clone()).map_err(|_| RemoteSignerError::InvalidConnection)
    }
}

/// The relays named by a `bunker://` URI, validated as a relay list.
pub fn bunker_relay_list(uri: &str) -> Result<RelayList, RemoteSignerError> {
    let (_, relays, _) = parse_bunker_uri(uri)?;
    RelayList::configured(relays).map_err(|_| RemoteSignerError::InvalidConnection)
}

/// The relays of the persisted signer connection, if one exists.
pub fn persisted_relay_list(
    private_state_root: &std::path::Path,
) -> Result<Option<RelayList>, RemoteSignerError> {
    let identity = Nip46ConnectionIdentity::load_or_create(private_state_root)
        .map_err(|_| RemoteSignerError::Storage)?;
    let Some(json) = identity
        .load_connection_data()
        .map_err(|_| RemoteSignerError::Storage)?
    else {
        return Ok(None);
    };
    if json.len() > MAX_CONNECTION_BYTES {
        return Err(RemoteSignerError::Storage);
    }
    let connection: RemoteSignerConnection =
        serde_json::from_str(&json).map_err(|_| RemoteSignerError::Storage)?;
    connection.relay_list().map(Some)
}

type BunkerParts = (String, Vec<String>, Option<String>);

fn parse_bunker_uri(uri: &str) -> Result<BunkerParts, RemoteSignerError> {
    let url = Url::parse(uri).map_err(|_| RemoteSignerError::InvalidConnection)?;
    if url.scheme() != "bunker" {
        return Err(RemoteSignerError::InvalidConnection);
    }
    let remote_signer_public_key = url
        .host_str()
        .ok_or(RemoteSignerError::InvalidConnection)?
        .to_owned();
    validate_public_key(&remote_signer_public_key)?;
    let mut relays = Vec::new();
    let mut secret = None;
    for (name, value) in url.query_pairs() {
        match name.as_ref() {
            "relay" => relays.push(value.into_owned()),
            "secret" if secret.is_none() => secret = Some(value.into_owned()),
            _ => {}
        }
    }
    if relays.is_empty() || relays.len() > 8 || relays.iter().any(|relay| relay.len() > 2_048) {
        return Err(RemoteSignerError::InvalidConnection);
    }
    if secret
        .as_ref()
        .is_some_and(|value| value.is_empty() || value.len() > 4_096)
    {
        return Err(RemoteSignerError::InvalidConnection);
    }
    Ok((remote_signer_public_key, relays, secret))
}

#[derive(Debug, thiserror::Error)]
pub enum RemoteSignerError {
    #[error("remote signer connection is invalid")]
    InvalidConnection,
    #[error("remote signer storage is unavailable")]
    Storage,
    #[error("remote signer request failed")]
    Request,
    #[error("remote signer request timed out")]
    Timeout,
    /// The signer's own error text, bounded and stripped of control
    /// characters, says which step it refused and why.
    #[error("remote signer denied {method}: {reason}")]
    Denied { method: String, reason: String },
    #[error("remote signer response is invalid")]
    InvalidResponse,
}

const MAX_SIGNER_ERROR_CHARS: usize = 200;

fn already_connected(reason: &str) -> bool {
    reason.trim().eq_ignore_ascii_case("already connected")
}

fn signer_error_text(error: &str) -> String {
    let text: String = error
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_SIGNER_ERROR_CHARS)
        .collect();
    if text.trim().is_empty() {
        "no reason given".into()
    } else {
        text
    }
}

trait RequestIdSource: Send + Sync {
    fn next(&self) -> String;
}

struct RandomRequestIds;

impl RequestIdSource for RandomRequestIds {
    fn next(&self) -> String {
        let mut bytes = [0_u8; 16];
        rand::rng().fill_bytes(&mut bytes);
        hex::encode(bytes)
    }
}

pub struct Nip46PersonSigner<C> {
    coordinator: Arc<C>,
    connection_identity: Arc<Nip46ConnectionIdentity>,
    connection: Mutex<RemoteSignerConnection>,
    state: Mutex<PersonSignerState>,
    request_ids: Arc<dyn RequestIdSource>,
}

impl<C> Nip46PersonSigner<C>
where
    C: SignerRelayCoordinator + 'static,
{
    pub fn load(
        private_state_root: &std::path::Path,
        coordinator: Arc<C>,
    ) -> Result<Option<Self>, RemoteSignerError> {
        let identity = Nip46ConnectionIdentity::load_or_create(private_state_root)
            .map_err(|_| RemoteSignerError::Storage)?;
        let Some(json) = identity
            .load_connection_data()
            .map_err(|_| RemoteSignerError::Storage)?
        else {
            return Ok(None);
        };
        if json.len() > MAX_CONNECTION_BYTES {
            return Err(RemoteSignerError::Storage);
        }
        let connection: RemoteSignerConnection =
            serde_json::from_str(&json).map_err(|_| RemoteSignerError::Storage)?;
        if connection.client_public_key != identity.public_key() {
            return Err(RemoteSignerError::Storage);
        }
        Ok(Some(Self::new(coordinator, identity, connection)?))
    }

    pub fn connect_from_bunker(
        private_state_root: &std::path::Path,
        coordinator: Arc<C>,
        bunker_uri: &str,
    ) -> Result<Self, RemoteSignerError> {
        let identity = Nip46ConnectionIdentity::load_or_create(private_state_root)
            .map_err(|_| RemoteSignerError::Storage)?;
        let connection =
            RemoteSignerConnection::from_bunker_uri(bunker_uri, identity.public_key())?;
        let signer = Self::new(coordinator, identity, connection)?;
        signer.persist()?;
        Ok(signer)
    }

    fn new(
        coordinator: Arc<C>,
        identity: Nip46ConnectionIdentity,
        connection: RemoteSignerConnection,
    ) -> Result<Self, RemoteSignerError> {
        validate_public_key(&connection.client_public_key)?;
        validate_public_key(&connection.remote_signer_public_key)?;
        if let Some(user) = &connection.user_public_key {
            validate_public_key(user)?;
        }
        let available = connection.user_public_key.is_some();
        Ok(Self {
            coordinator,
            connection_identity: Arc::new(identity),
            connection: Mutex::new(connection),
            state: Mutex::new(if available {
                PersonSignerState::Unavailable {
                    message: "Remote signer is connected".into(),
                }
            } else {
                PersonSignerState::Unavailable {
                    message: "Remote signer approval is required".into(),
                }
            }),
            request_ids: Arc::new(RandomRequestIds),
        })
    }

    #[cfg(test)]
    fn with_request_ids(mut self, ids: Arc<dyn RequestIdSource>) -> Self {
        self.request_ids = ids;
        self
    }

    pub(crate) fn person_public_key(&self) -> Option<String> {
        self.connection
            .lock()
            .expect("remote signer lock poisoned")
            .user_public_key
            .clone()
    }

    /// The key this client sends its requests to and takes replies from.
    pub fn remote_signer_public_key(&self) -> String {
        self.connection
            .lock()
            .expect("remote signer lock poisoned")
            .remote_signer_public_key
            .clone()
    }

    /// NIP-46 `connect` only. The person key is learned from the first signed
    /// owner statement, so the signer needs no `get_public_key` permission.
    ///
    /// The reply may come from another key than the `bunker://` one. NIP-46
    /// does not allow that in the bunker flow, but Amber 6.6.9 answers from a
    /// new key when the person picks "Create new connection". korrid finds
    /// the signer key as NIP-46's `nostrconnect://` flow does, from the reply
    /// author, with the random request id in place of the secret. That key
    /// becomes the remote signer of every later request.
    pub async fn establish(
        &self,
        now: u64,
        device_public_key: &str,
    ) -> Result<(), RemoteSignerError> {
        validate_public_key(device_public_key)?;
        let connection = self
            .connection
            .lock()
            .expect("remote signer lock poisoned")
            .clone();
        let connect = Nip46Request {
            id: self.request_ids.next(),
            method: "connect".into(),
            params: vec![
                connection.remote_signer_public_key.clone(),
                connection.secret.clone().unwrap_or_default(),
                REQUESTED_PERMISSIONS.into(),
                serde_json::json!({"name": connection_name(device_public_key)}).to_string(),
            ],
        };
        // Any sender: only the holder of the bunker key could read this
        // request, so only it knows the random request id that a reply must
        // carry.
        match self.send_request(connect, now, None, CONNECT_TIMEOUT).await {
            Ok((sender, result)) => {
                let expected = connection.secret.as_deref().unwrap_or("ack");
                if result != expected && !(connection.secret.is_some() && result == "ack") {
                    return Err(RemoteSignerError::InvalidResponse);
                }
                self.connection
                    .lock()
                    .expect("remote signer lock poisoned")
                    .remote_signer_public_key = sender;
            }
            // Every attempt connects with a new client key, so this means the
            // URI's one-time secret was already used by an earlier attempt.
            Err(RemoteSignerError::Denied { reason, .. }) if already_connected(&reason) => {
                return Err(RemoteSignerError::Denied {
                    method: "connect".into(),
                    reason:
                        "this signer address was already used; create a new one in the signer app"
                            .into(),
                });
            }
            Err(error) => return Err(error),
        }
        self.persist()
    }

    fn persist(&self) -> Result<(), RemoteSignerError> {
        let json =
            serde_json::to_string(&*self.connection.lock().expect("remote signer lock poisoned"))
                .map_err(|_| RemoteSignerError::Storage)?;
        self.connection_identity
            .save_connection_data(&json)
            .map_err(|_| RemoteSignerError::Storage)
    }

    async fn sign_owner_template(
        &self,
        template: &str,
        now: u64,
    ) -> Result<(String, String), RemoteSignerError> {
        let template_value: UnsignedTemplate =
            serde_json::from_str(template).map_err(|_| RemoteSignerError::InvalidResponse)?;
        if template.len() > MAX_RESPONSE_BYTES || template_value.kind != OWNER_EVENT_KIND {
            return Err(RemoteSignerError::InvalidResponse);
        }
        let known = self
            .connection
            .lock()
            .expect("remote signer lock poisoned")
            .user_public_key
            .clone();
        let remote = self.remote_signer_public_key();
        let (_, result) = self
            .send_request(
                Nip46Request {
                    id: self.request_ids.next(),
                    method: "sign_event".into(),
                    params: vec![template.into()],
                },
                now,
                Some(&remote),
                REQUEST_TIMEOUT,
            )
            .await?;
        let user = verify_exact_signed_event(template_value, known.as_deref(), &result)?;
        if known.is_none() {
            self.connection
                .lock()
                .expect("remote signer lock poisoned")
                .user_public_key = Some(user.clone());
            self.persist()?;
        }
        Ok((user, result))
    }

    /// Sends `request` to the remote signer and waits up to `wait` for its
    /// reply from `sender`, or from any key when `sender` is `None`. Returns
    /// the key that answered and the result.
    async fn send_request(
        &self,
        request: Nip46Request,
        now: u64,
        sender: Option<&str>,
        wait: Duration,
    ) -> Result<(String, String), RemoteSignerError> {
        let connection = self
            .connection
            .lock()
            .expect("remote signer lock poisoned")
            .clone();
        let payload = serde_json::to_string(&request).map_err(|_| RemoteSignerError::Request)?;
        let event = self
            .connection_identity
            .sign_encrypted_event(
                &connection.remote_signer_public_key,
                NIP46_EVENT_KIND,
                &payload,
                now,
            )
            .map_err(|_| RemoteSignerError::Request)?;
        if !self.coordinator.publish(&event.json).await.accepted() {
            return Err(RemoteSignerError::Request);
        }
        let started = Instant::now();
        let deadline = started + wait;
        timeout(wait, async {
            loop {
                let packets = self
                    .coordinator
                    .responses(
                        sender,
                        &connection.client_public_key,
                        now.saturating_sub(300),
                    )
                    .await
                    .map_err(|_| RemoteSignerError::Request)?;
                for packet in packets {
                    if packet.len() > MAX_RESPONSE_BYTES {
                        continue;
                    }
                    // The future-time bound moves with the wait, so a long
                    // connect keeps its clock margin to the end.
                    let read_at = now.saturating_add(started.elapsed().as_secs());
                    let (answered_by, response) =
                        match self.decode_response(&packet, read_at, sender) {
                            Ok((answered_by, response)) if response.id == request.id => {
                                (answered_by, response)
                            }
                            _ => continue,
                        };
                    if response.result == "auth_url" {
                        continue;
                    }
                    if let Some(error) = response.error {
                        return Err(RemoteSignerError::Denied {
                            method: request.method.clone(),
                            reason: signer_error_text(&error),
                        });
                    }
                    return Ok((answered_by, response.result));
                }
                if Instant::now() >= deadline {
                    return Err(RemoteSignerError::Timeout);
                }
                sleep(POLL_DELAY).await;
            }
        })
        .await
        .map_err(|_| RemoteSignerError::Timeout)?
    }

    /// A reply to this client from `sender`, or from any key when `sender`
    /// is `None`, decrypted with its author's key. Returns that author.
    fn decode_response(
        &self,
        event_json: &str,
        now: u64,
        sender: Option<&str>,
    ) -> Result<(String, Nip46Response), RemoteSignerError> {
        let verified = match sender {
            Some(sender) => self
                .connection_identity
                .decrypt_event_from(event_json, sender),
            None => self
                .connection_identity
                .decrypt_event_from_any_author(event_json),
        }
        .map_err(|_| RemoteSignerError::InvalidResponse)?;
        if verified.kind != NIP46_EVENT_KIND
            || verified.created_at > now.saturating_add(300)
            || verified.content.len() > MAX_RESPONSE_BYTES
        {
            return Err(RemoteSignerError::InvalidResponse);
        }
        let response = serde_json::from_str(&verified.content)
            .map_err(|_| RemoteSignerError::InvalidResponse)?;
        Ok((verified.author, response))
    }
}

/// The name in `connect`. It names the device, so a signer that already has a
/// "Korri" connection has no reason to offer to replace it (Amber 6.6.9 does,
/// and its "Create new connection" then answers from a new key).
pub(crate) fn connection_name(device_public_key: &str) -> String {
    format!("Korri {}", &device_public_key[..8])
}

impl<C> PersonSigner for Nip46PersonSigner<C>
where
    C: SignerRelayCoordinator + 'static,
{
    fn state(&self) -> PersonSignerState {
        self.state
            .lock()
            .expect("remote signer state poisoned")
            .clone()
    }

    fn request<'a>(&'a self, request: PersonSignerRequest) -> BoxFuture<'a, PersonSignerState> {
        Box::pin(async move {
            *self.state.lock().expect("remote signer state poisoned") =
                PersonSignerState::Pending {
                    message: "Waiting for the remote signer".into(),
                };
            let now = unix_now();
            let next = match self
                .sign_owner_template(&request.unsigned_event_template, now)
                .await
            {
                Ok((owner_public_key, signed_event_json)) => PersonSignerState::Approved {
                    owner_public_key,
                    unsigned_event_template: request.unsigned_event_template,
                    signed_event_json,
                },
                Err(error @ RemoteSignerError::Denied { .. }) => PersonSignerState::Denied {
                    message: error.to_string(),
                },
                Err(RemoteSignerError::InvalidResponse) => PersonSignerState::InvalidResponse {
                    message: "The remote signer returned a different or invalid event".into(),
                },
                Err(error) => PersonSignerState::Defect {
                    message: error.to_string(),
                },
            };
            *self.state.lock().expect("remote signer state poisoned") = next.clone();
            next
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Nip46Request {
    id: String,
    method: String,
    params: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Nip46Response {
    id: String,
    // A refusal may carry only the error: nak's bunker sends
    // {"id", "error": "unauthorized"}. Without this default korrid dropped
    // that reply and waited out REQUEST_TIMEOUT.
    #[serde(default)]
    result: String,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UnsignedTemplate {
    kind: u16,
    created_at: u64,
    tags: Vec<Vec<String>>,
    content: String,
}

/// The signed event must be exactly the template. With no known person key the
/// verified author becomes it; with one, the author must match it.
fn verify_exact_signed_event(
    template: UnsignedTemplate,
    expected_user_public_key: Option<&str>,
    signed_event_json: &str,
) -> Result<String, RemoteSignerError> {
    if signed_event_json.len() > MAX_RESPONSE_BYTES {
        return Err(RemoteSignerError::InvalidResponse);
    }
    let signed = DeviceIdentity::verify_event(signed_event_json)
        .map_err(|_| RemoteSignerError::InvalidResponse)?;
    if expected_user_public_key.is_some_and(|expected| signed.author != expected)
        || signed.kind != template.kind
        || signed.created_at != template.created_at
        || signed.tags != template.tags
        || signed.content != template.content
    {
        return Err(RemoteSignerError::InvalidResponse);
    }
    validate_public_key(&signed.author).map_err(|_| RemoteSignerError::InvalidResponse)?;
    Ok(signed.author)
}

fn validate_public_key(value: &str) -> Result<(), RemoteSignerError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(RemoteSignerError::InvalidConnection)
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        identity::{DeviceIdentity, OwnerStatementStatus},
        relay::{InProcessRelayNetwork, RelayList},
    };
    use nostr::{
        event::{Event, EventBuilder, FinalizeEvent, Kind, Tag},
        key::Keys,
        nips::nip44,
        types::Timestamp,
    };
    use std::{collections::VecDeque, sync::Mutex};

    struct SequenceRequestIds(Mutex<VecDeque<String>>);

    impl SequenceRequestIds {
        fn new(ids: &[&str]) -> Self {
            Self(Mutex::new(ids.iter().map(|id| (*id).into()).collect()))
        }
    }

    impl RequestIdSource for SequenceRequestIds {
        fn next(&self) -> String {
            self.0.lock().unwrap().pop_front().unwrap()
        }
    }

    fn owned_identity(root: &std::path::Path, owner: &Keys) -> DeviceIdentity {
        let mut identity = DeviceIdentity::load_or_create(root).unwrap();
        let device = identity.device_public_key().unwrap().to_owned();
        let event = EventBuilder::new(Kind::Custom(30_078), "")
            .tags([
                Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
                Tag::parse(["device", &device]).unwrap(),
                Tag::parse(["status", "owned"]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(1))
            .finalize(owner)
            .unwrap()
            .as_json();
        identity.apply_owner_statement(&event).unwrap();
        identity
    }

    #[tokio::test]
    async fn accepts_a_strict_nip46_signed_event_response() {
        sign_owner_template_roundtrip(true).await;
    }

    #[tokio::test]
    async fn learns_the_person_key_from_the_signed_owner_statement() {
        sign_owner_template_roundtrip(false).await;
    }

    async fn sign_owner_template_roundtrip(known_user: bool) {
        let relays = RelayList::configured(vec![
            "ws://127.0.0.1:18001".into(),
            "ws://127.0.0.1:18002".into(),
        ])
        .unwrap();
        let network = Arc::new(InProcessRelayNetwork::new(&relays));
        let owner = Keys::generate();
        let device_root = tempfile::tempdir().unwrap();
        let device = owned_identity(device_root.path(), &owner);
        let coordinator =
            Arc::new(CoordinatedRelays::new(relays.clone(), device, network.clone()).unwrap());
        let client_root = tempfile::tempdir().unwrap();
        let client_identity = Nip46ConnectionIdentity::load_or_create(client_root.path()).unwrap();
        let remote = Keys::generate();
        let connection = RemoteSignerConnection {
            client_public_key: client_identity.public_key(),
            remote_signer_public_key: remote.public_key().to_hex(),
            user_public_key: known_user.then(|| owner.public_key().to_hex()),
            relays: relays.as_slice().to_vec(),
            secret: Some("one-use-secret".into()),
        };
        let signer = Arc::new(
            Nip46PersonSigner::new(coordinator.clone(), client_identity, connection)
                .unwrap()
                .with_request_ids(Arc::new(SequenceRequestIds::new(&["sign-1"]))),
        );
        let target_root = tempfile::tempdir().unwrap();
        let target = DeviceIdentity::load_or_create(target_root.path()).unwrap();
        let template = target
            .owner_statement_template(OwnerStatementStatus::Owned, unix_now())
            .unwrap();

        let relay_task = {
            let network = network.clone();
            let relays = relays.clone();
            let remote = remote.clone();
            let owner = owner.clone();
            tokio::spawn(async move {
                loop {
                    let filter = crate::relay::RelayFilter {
                        kinds: vec![NIP46_EVENT_KIND],
                        recipient_public_key: Some(remote.public_key().to_hex()),
                        author_public_key: None,
                        since: None,
                        limit: 10,
                        identifiers: Vec::new(),
                    };
                    struct NoAuth;
                    impl crate::relay::RelayAuthSigner for NoAuth {
                        fn sign_auth(
                            &self,
                            _: &str,
                            _: &str,
                            _: u64,
                        ) -> Result<String, RelayError> {
                            unreachable!()
                        }
                    }
                    let events = network
                        .read(&relays.as_slice()[0], &filter, Arc::new(NoAuth))
                        .await
                        .unwrap();
                    if let Some(delivery) = events.first() {
                        let request_event = Event::from_json(&delivery.event_json).unwrap();
                        let plaintext = nip44::decrypt(
                            remote.secret_key(),
                            &request_event.pubkey,
                            request_event.content.as_bytes(),
                        )
                        .unwrap();
                        let request: Nip46Request = serde_json::from_str(&plaintext).unwrap();
                        let unsigned: UnsignedTemplate =
                            serde_json::from_str(&request.params[0]).unwrap();
                        let signed =
                            EventBuilder::new(Kind::Custom(unsigned.kind), unsigned.content)
                                .tags(
                                    unsigned
                                        .tags
                                        .into_iter()
                                        .map(|tag| Tag::parse(tag).unwrap()),
                                )
                                .custom_created_at(Timestamp::from(unsigned.created_at))
                                .finalize(&owner)
                                .unwrap()
                                .as_json();
                        let response = serde_json::to_string(&Nip46Response {
                            id: request.id,
                            result: signed,
                            error: None,
                        })
                        .unwrap();
                        let content = nip44::encrypt(
                            remote.secret_key(),
                            &request_event.pubkey,
                            response.as_bytes(),
                            nip44::Version::V2,
                        )
                        .unwrap();
                        let response_event =
                            EventBuilder::new(Kind::Custom(NIP46_EVENT_KIND), content)
                                .tag(Tag::public_key(request_event.pubkey))
                                .custom_created_at(Timestamp::now())
                                .finalize(&remote)
                                .unwrap()
                                .as_json();
                        network
                            .inject(&relays.as_slice()[0], &response_event)
                            .unwrap();
                        break;
                    }
                    sleep(Duration::from_millis(5)).await;
                }
            })
        };

        let state = signer
            .request(PersonSignerRequest {
                unsigned_event_template: template.clone(),
            })
            .await;
        relay_task.await.unwrap();
        assert!(matches!(state, PersonSignerState::Approved {
            owner_public_key, unsigned_event_template, ..
        } if owner_public_key == owner.public_key().to_hex() && unsigned_event_template == template));
        let persisted = Nip46ConnectionIdentity::load_or_create(client_root.path())
            .unwrap()
            .load_connection_data()
            .unwrap();
        if known_user {
            assert!(persisted.is_none());
        } else {
            let learned: RemoteSignerConnection =
                serde_json::from_str(&persisted.unwrap()).unwrap();
            assert_eq!(learned.user_public_key, Some(owner.public_key().to_hex()));
        }
    }

    #[test]
    fn connection_uses_exact_permission_and_rejects_spoofable_shapes() {
        assert_eq!(REQUESTED_PERMISSIONS, "sign_event:30078");
        let client = "11".repeat(32);
        let uri = format!(
            "bunker://{}?relay=wss%3A%2F%2Frelay.example.com&secret=single-use",
            "22".repeat(32)
        );
        let connection = RemoteSignerConnection::from_bunker_uri(&uri, client).unwrap();
        assert_eq!(connection.relays, vec!["wss://relay.example.com"]);
        assert_eq!(connection.secret.as_deref(), Some("single-use"));
        assert!(
            RemoteSignerConnection::from_bunker_uri("https://example.com", "11".repeat(32))
                .is_err()
        );
    }

    #[test]
    fn a_denial_names_the_step_and_the_signers_bounded_reason() {
        let error = RemoteSignerError::Denied {
            method: "connect".into(),
            reason: signer_error_text("invalid secret\n\u{1b}[31m"),
        };
        assert_eq!(
            error.to_string(),
            "remote signer denied connect: invalid secret[31m"
        );
        assert_eq!(signer_error_text(" \n"), "no reason given");
        assert!(already_connected("Already connected"));
        assert!(!already_connected("invalid secret"));
        assert_eq!(signer_error_text(&"x".repeat(500)).len(), 200);
    }

    #[test]
    fn a_refusal_without_a_result_is_read_as_a_refusal() {
        let response: Nip46Response =
            serde_json::from_str(r#"{"id":"connect-1","error":"unauthorized"}"#).unwrap();
        assert_eq!(response.error.as_deref(), Some("unauthorized"));
        assert_eq!(response.result, "");
        assert!(serde_json::from_str::<Nip46Response>(r#"{"result":"ack"}"#).is_err());
    }

    #[test]
    fn signer_relays_come_from_the_bunker_uri() {
        let uri = format!(
            "bunker://{}?relay=wss://relay.nip46.com/&relay=wss://nrs.primal.net/&secret=s",
            "22".repeat(32)
        );
        let expected = vec![
            "wss://relay.nip46.com/".to_owned(),
            "wss://nrs.primal.net/".to_owned(),
        ];
        assert_eq!(bunker_relay_list(&uri).unwrap().as_slice(), expected);
        let connection = RemoteSignerConnection::from_bunker_uri(&uri, "11".repeat(32)).unwrap();
        assert_eq!(connection.relay_list().unwrap().as_slice(), expected);
        let plaintext = format!(
            "bunker://{}?relay=http://relay.test&secret=s",
            "22".repeat(32)
        );
        assert!(bunker_relay_list(&plaintext).is_err());
    }

    struct NoRelayAuth;

    impl crate::relay::RelayAuthSigner for NoRelayAuth {
        fn sign_auth(&self, _: &str, _: &str, _: u64) -> Result<String, RelayError> {
            unreachable!()
        }
    }

    /// One NIP-46 reply: authored by `from`, encrypted with the conversation
    /// key of `encrypted_by` and the client.
    fn nip46_reply(
        from: &Keys,
        encrypted_by: &Keys,
        client: &nostr::key::PublicKey,
        id: &str,
        result: &str,
    ) -> String {
        let response = serde_json::to_string(&Nip46Response {
            id: id.into(),
            result: result.into(),
            error: None,
        })
        .unwrap();
        let content = nip44::encrypt(
            encrypted_by.secret_key(),
            client,
            response.as_bytes(),
            nip44::Version::V2,
        )
        .unwrap();
        EventBuilder::new(Kind::Custom(NIP46_EVENT_KIND), content)
            .tag(Tag::public_key(*client))
            .custom_created_at(Timestamp::now())
            .finalize(from)
            .unwrap()
            .as_json()
    }

    /// What the test's signer does with `connect`.
    #[derive(Clone, Copy)]
    enum SignerBehavior {
        Answers,
        /// First two replies from an intruder key: one claims the request
        /// but is encrypted under the answering key's conversation, one
        /// decrypts but answers another request. korrid sees them alone
        /// for two reads before the true reply.
        ForgesThenAnswers,
        OnlyForges,
    }

    /// A NIP-46 signer on the in-process relays. It reads the requests sent
    /// to `bunker` or to `answering` on the first relay and answers each
    /// from `answering` on the second, as Amber 6.6.9 does after "Create new
    /// connection". It returns the requests it read, after `sign_event`.
    fn spawn_signer(
        network: Arc<InProcessRelayNetwork>,
        relays: RelayList,
        keys: (Keys, Keys, Keys),
        behavior: SignerBehavior,
    ) -> tokio::task::JoinHandle<Vec<Nip46Request>> {
        let (bunker, answering, owner) = keys;
        tokio::spawn(async move {
            let inbox = relays.as_slice()[0].clone();
            let outbox = relays.as_slice()[1].clone();
            let mut seen = std::collections::BTreeSet::new();
            let mut requests = Vec::new();
            loop {
                for addressee in [&bunker, &answering] {
                    let filter = crate::relay::RelayFilter {
                        kinds: vec![NIP46_EVENT_KIND],
                        recipient_public_key: Some(addressee.public_key().to_hex()),
                        author_public_key: None,
                        since: None,
                        limit: 10,
                        identifiers: Vec::new(),
                    };
                    let deliveries = network
                        .read(&inbox, &filter, Arc::new(NoRelayAuth))
                        .await
                        .unwrap();
                    for delivery in deliveries {
                        let event = Event::from_json(&delivery.event_json).unwrap();
                        if !seen.insert(event.id) {
                            continue;
                        }
                        let plaintext = nip44::decrypt(
                            addressee.secret_key(),
                            &event.pubkey,
                            event.content.as_bytes(),
                        )
                        .unwrap();
                        let request: Nip46Request = serde_json::from_str(&plaintext).unwrap();
                        let result = match request.method.as_str() {
                            "connect" => {
                                if !matches!(behavior, SignerBehavior::Answers) {
                                    let intruder = Keys::generate();
                                    for reply in [
                                        nip46_reply(
                                            &intruder,
                                            &answering,
                                            &event.pubkey,
                                            &request.id,
                                            "ack",
                                        ),
                                        nip46_reply(
                                            &intruder,
                                            &intruder,
                                            &event.pubkey,
                                            "connect-0",
                                            "ack",
                                        ),
                                    ] {
                                        network.inject(&outbox, &reply).unwrap();
                                    }
                                    if matches!(behavior, SignerBehavior::OnlyForges) {
                                        requests.push(request);
                                        return requests;
                                    }
                                    network.delay_next_delivery(&outbox, 2);
                                }
                                "ack".to_owned()
                            }
                            "sign_event" => {
                                let unsigned: UnsignedTemplate =
                                    serde_json::from_str(&request.params[0]).unwrap();
                                EventBuilder::new(Kind::Custom(unsigned.kind), unsigned.content)
                                    .tags(
                                        unsigned
                                            .tags
                                            .into_iter()
                                            .map(|tag| Tag::parse(tag).unwrap()),
                                    )
                                    .custom_created_at(Timestamp::from(unsigned.created_at))
                                    .finalize(&owner)
                                    .unwrap()
                                    .as_json()
                            }
                            other => panic!("unexpected NIP-46 request {other}"),
                        };
                        let reply = nip46_reply(
                            &answering,
                            &answering,
                            &event.pubkey,
                            &request.id,
                            &result,
                        );
                        network.inject(&outbox, &reply).unwrap();
                        let signed = request.method == "sign_event";
                        requests.push(request);
                        if signed {
                            return requests;
                        }
                    }
                }
                sleep(Duration::from_millis(5)).await;
            }
        })
    }

    struct Nip46Setup {
        network: Arc<InProcessRelayNetwork>,
        relays: RelayList,
        signer: Nip46PersonSigner<CoordinatedRelays<InProcessRelayNetwork>>,
        client_root: tempfile::TempDir,
        _device_root: tempfile::TempDir,
        /// The device whose owner statement is signed, and its template.
        device: String,
        template: String,
    }

    fn nip46_setup(bunker: &Keys, owner: &Keys) -> Nip46Setup {
        let relays = RelayList::configured(vec![
            "ws://127.0.0.1:18011".into(),
            "ws://127.0.0.1:18012".into(),
        ])
        .unwrap();
        let network = Arc::new(InProcessRelayNetwork::new(&relays));
        let device_root = tempfile::tempdir().unwrap();
        let coordinator = Arc::new(
            CoordinatedRelays::new(
                relays.clone(),
                owned_identity(device_root.path(), owner),
                network.clone(),
            )
            .unwrap(),
        );
        let client_root = tempfile::tempdir().unwrap();
        let client_identity = Nip46ConnectionIdentity::load_or_create(client_root.path()).unwrap();
        let connection = RemoteSignerConnection {
            client_public_key: client_identity.public_key(),
            remote_signer_public_key: bunker.public_key().to_hex(),
            user_public_key: None,
            relays: relays.as_slice().to_vec(),
            secret: Some("one-use-secret".into()),
        };
        let signer = Nip46PersonSigner::new(coordinator, client_identity, connection)
            .unwrap()
            .with_request_ids(Arc::new(SequenceRequestIds::new(&["connect-1", "sign-1"])));
        let target_root = tempfile::tempdir().unwrap();
        let target = DeviceIdentity::load_or_create(target_root.path()).unwrap();
        Nip46Setup {
            network,
            relays,
            signer,
            client_root,
            _device_root: device_root,
            device: target.device_public_key().unwrap().to_owned(),
            template: target
                .owner_statement_template(OwnerStatementStatus::Owned, unix_now())
                .unwrap(),
        }
    }

    fn persisted_connection(setup: &Nip46Setup) -> RemoteSignerConnection {
        let json = Nip46ConnectionIdentity::load_or_create(setup.client_root.path())
            .unwrap()
            .load_connection_data()
            .unwrap()
            .unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[tokio::test(start_paused = true)]
    async fn a_signer_that_answers_connect_from_another_key_is_asked_from_that_key() {
        let (bunker, answering, owner) = (Keys::generate(), Keys::generate(), Keys::generate());
        let setup = nip46_setup(&bunker, &owner);
        let signer_task = spawn_signer(
            setup.network.clone(),
            setup.relays.clone(),
            (bunker.clone(), answering.clone(), owner.clone()),
            SignerBehavior::Answers,
        );
        setup
            .signer
            .establish(unix_now(), &setup.device)
            .await
            .unwrap();
        assert_eq!(
            setup.signer.remote_signer_public_key(),
            answering.public_key().to_hex()
        );
        let state = setup
            .signer
            .request(PersonSignerRequest {
                unsigned_event_template: setup.template.clone(),
            })
            .await;
        let requests = signer_task.await.unwrap();
        assert!(
            matches!(&state, PersonSignerState::Approved { owner_public_key, .. }
                if owner_public_key == &owner.public_key().to_hex()),
            "{state:?}"
        );
        // Its connect names the device, so a signer that already has a
        // "Korri" connection has no reason to offer to replace it.
        assert_eq!(requests[0].method, "connect");
        assert_eq!(requests[0].params[2], "sign_event:30078");
        assert_eq!(
            requests[0].params[3],
            serde_json::json!({"name": format!("Korri {}", &setup.device[..8])}).to_string()
        );
        let persisted = persisted_connection(&setup);
        assert_eq!(
            persisted.remote_signer_public_key,
            answering.public_key().to_hex()
        );
        assert_eq!(persisted.user_public_key, Some(owner.public_key().to_hex()));
    }

    #[tokio::test(start_paused = true)]
    async fn a_reply_from_another_key_that_does_not_decrypt_is_ignored() {
        let (bunker, answering, owner) = (Keys::generate(), Keys::generate(), Keys::generate());
        let setup = nip46_setup(&bunker, &owner);
        let _signer_task = spawn_signer(
            setup.network.clone(),
            setup.relays.clone(),
            (bunker.clone(), answering.clone(), owner.clone()),
            SignerBehavior::ForgesThenAnswers,
        );
        setup
            .signer
            .establish(unix_now(), &setup.device)
            .await
            .unwrap();
        assert_eq!(
            setup.signer.remote_signer_public_key(),
            answering.public_key().to_hex()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn connect_ignores_forged_replies_and_waits_300_s() {
        let (bunker, answering, owner) = (Keys::generate(), Keys::generate(), Keys::generate());
        let setup = nip46_setup(&bunker, &owner);
        let _signer_task = spawn_signer(
            setup.network.clone(),
            setup.relays.clone(),
            (bunker.clone(), answering, owner),
            SignerBehavior::OnlyForges,
        );
        let started = Instant::now();
        let result = setup.signer.establish(unix_now(), &setup.device).await;
        assert!(
            matches!(result, Err(RemoteSignerError::Timeout)),
            "{result:?}"
        );
        assert!(started.elapsed() >= Duration::from_secs(300));
        assert_eq!(
            setup.signer.remote_signer_public_key(),
            bunker.public_key().to_hex()
        );
    }
}
