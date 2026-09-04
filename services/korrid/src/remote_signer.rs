//! NIP-46 remote person signer behind a Korri-owned signer contract.

use crate::{
    identity::{DeviceIdentity, Nip46ConnectionIdentity},
    relay::{CoordinatedRelays, PublishState, RelayError, RelayTransport},
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
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_DELAY: Duration = Duration::from_millis(50);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_CONNECTION_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonSignerRequest {
    pub unsigned_event_template: String,
}

pub trait PersonSigner: Send + Sync {
    fn state(&self) -> PersonSignerState;
    fn request<'a>(&'a self, request: PersonSignerRequest) -> BoxFuture<'a, PersonSignerState>;
}

pub trait SignerRelayCoordinator: Send + Sync {
    fn publish<'a>(&'a self, event_json: &'a str) -> BoxFuture<'a, PublishState>;
    fn responses<'a>(
        &'a self,
        remote_signer_public_key: &'a str,
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
        remote_signer_public_key: &'a str,
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
        let url = Url::parse(uri).map_err(|_| RemoteSignerError::InvalidConnection)?;
        if url.scheme() != "bunker" {
            return Err(RemoteSignerError::InvalidConnection);
        }
        let remote_signer_public_key = url
            .host_str()
            .ok_or(RemoteSignerError::InvalidConnection)?
            .to_owned();
        validate_public_key(&client_public_key)?;
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
        Ok(Self {
            client_public_key,
            remote_signer_public_key,
            user_public_key: None,
            relays,
            secret,
        })
    }
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
    #[error("remote signer denied the request")]
    Denied,
    #[error("remote signer response is invalid")]
    InvalidResponse,
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

    pub async fn establish(&self, now: u64) -> Result<String, RemoteSignerError> {
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
                serde_json::json!({"name": "Korri"}).to_string(),
            ],
        };
        let result = self.send_request(connect, now).await?;
        let expected = connection.secret.as_deref().unwrap_or("ack");
        if result != expected && !(connection.secret.is_some() && result == "ack") {
            return Err(RemoteSignerError::InvalidResponse);
        }
        let user = self
            .send_request(
                Nip46Request {
                    id: self.request_ids.next(),
                    method: "get_public_key".into(),
                    params: Vec::new(),
                },
                now,
            )
            .await?;
        validate_public_key(&user)?;
        self.connection
            .lock()
            .expect("remote signer lock poisoned")
            .user_public_key = Some(user.clone());
        self.persist()?;
        Ok(user)
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
        let user = self
            .connection
            .lock()
            .expect("remote signer lock poisoned")
            .user_public_key
            .clone()
            .ok_or(RemoteSignerError::InvalidConnection)?;
        let result = self
            .send_request(
                Nip46Request {
                    id: self.request_ids.next(),
                    method: "sign_event".into(),
                    params: vec![template.into()],
                },
                now,
            )
            .await?;
        verify_exact_signed_event(template_value, &user, &result)?;
        Ok((user, result))
    }

    async fn send_request(
        &self,
        request: Nip46Request,
        now: u64,
    ) -> Result<String, RemoteSignerError> {
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
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        timeout(REQUEST_TIMEOUT, async {
            loop {
                let packets = self
                    .coordinator
                    .responses(
                        &connection.remote_signer_public_key,
                        &connection.client_public_key,
                        now.saturating_sub(300),
                    )
                    .await
                    .map_err(|_| RemoteSignerError::Request)?;
                for packet in packets {
                    if packet.len() > MAX_RESPONSE_BYTES {
                        continue;
                    }
                    let response = match self.decode_response(&packet, now) {
                        Ok(response) if response.id == request.id => response,
                        _ => continue,
                    };
                    if response.result == "auth_url" {
                        continue;
                    }
                    if response.error.is_some() {
                        return Err(RemoteSignerError::Denied);
                    }
                    return Ok(response.result);
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

    fn decode_response(
        &self,
        event_json: &str,
        now: u64,
    ) -> Result<Nip46Response, RemoteSignerError> {
        let connection = self
            .connection
            .lock()
            .expect("remote signer lock poisoned")
            .clone();
        let verified = self
            .connection_identity
            .decrypt_event_from(event_json, &connection.remote_signer_public_key)
            .map_err(|_| RemoteSignerError::InvalidResponse)?;
        if verified.kind != NIP46_EVENT_KIND
            || verified.created_at > now.saturating_add(300)
            || verified.content.len() > MAX_RESPONSE_BYTES
        {
            return Err(RemoteSignerError::InvalidResponse);
        }
        serde_json::from_str(&verified.content).map_err(|_| RemoteSignerError::InvalidResponse)
    }
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
                Err(RemoteSignerError::Denied) => PersonSignerState::Denied {
                    message: "The remote signer denied the request".into(),
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

fn verify_exact_signed_event(
    template: UnsignedTemplate,
    expected_user_public_key: &str,
    signed_event_json: &str,
) -> Result<(), RemoteSignerError> {
    if signed_event_json.len() > MAX_RESPONSE_BYTES {
        return Err(RemoteSignerError::InvalidResponse);
    }
    let signed = DeviceIdentity::verify_event(signed_event_json)
        .map_err(|_| RemoteSignerError::InvalidResponse)?;
    if signed.author != expected_user_public_key
        || signed.kind != template.kind
        || signed.created_at != template.created_at
        || signed.tags != template.tags
        || signed.content != template.content
    {
        return Err(RemoteSignerError::InvalidResponse);
    }
    Ok(())
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
            user_public_key: Some(owner.public_key().to_hex()),
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
                        recipient_public_key: remote.public_key().to_hex(),
                        author_public_key: None,
                        since: None,
                        limit: 10,
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
}
