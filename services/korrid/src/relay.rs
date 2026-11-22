//! Bounded relay coordination.
//!
//! This module converts Nostr relay events into owner roster evidence, endpoint
//! candidates, or queued commands. It never dispatches product RPC and never transports
//! catalog, artwork, saves, controller input, interactive calls, or stream data.

use crate::identity::{
    event_is_newer, DeviceIdentity, IdentityState, OwnerStatementStatus, VerifiedOwnerStatement,
    OWNER_EVENT_KIND,
};
use futures::{future::BoxFuture, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};
#[cfg(test)]
use tokio::time::sleep;
use tokio::time::{timeout, Instant};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

pub const ENDPOINT_EVENT_KIND: u16 = 30_078;
pub const GIFT_WRAP_EVENT_KIND: u16 = 1_059;
pub const COORDINATION_RUMOR_KIND: u16 = 29_100;
const ENDPOINT_EVENT_PREFIX: &str = "org.korri.endpoint:";
const MAX_RELAYS: usize = 8;
const MAX_RELAY_URL_BYTES: usize = 2_048;
const MAX_EVENT_BYTES: usize = 64 * 1024;
const MAX_STORED_EVENTS_PER_RELAY: usize = 256;
pub(crate) const MAX_READ_EVENTS: usize = 128;
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_CLOCK_FUTURE_SECONDS: u64 = 5 * 60;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(8);
const INITIAL_RECONNECT_DELAY: Duration = Duration::from_secs(5);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(300);
const REFUSAL_BACKOFF: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayList(Vec<String>);

impl RelayList {
    pub fn configured(values: Vec<String>) -> Result<Self, RelayError> {
        if values.is_empty() || values.len() > MAX_RELAYS {
            return Err(RelayError::Configuration(
                "relay list must contain between 1 and 8 relays".into(),
            ));
        }
        let mut unique = BTreeSet::new();
        for value in &values {
            validate_relay_url(value)?;
            if !unique.insert(value.as_str()) {
                return Err(RelayError::Configuration(
                    "relay list contains a duplicate".into(),
                ));
            }
        }
        Ok(Self(values))
    }

    /// Linux uses one JSON array in `KORRID_RELAYS`; no public relay is built in.
    pub fn from_linux_environment(value: Option<&str>) -> Result<Option<Self>, RelayError> {
        value
            .map(|json| {
                serde_json::from_str::<Vec<String>>(json)
                    .map_err(|_| {
                        RelayError::Configuration("KORRID_RELAYS must be a JSON array".into())
                    })
                    .and_then(Self::configured)
            })
            .transpose()
    }

    /// Device settings use `host.relays` in the established config snapshot.
    pub fn from_device_settings(
        snapshot: &crate::config::ConfigSnapshot,
    ) -> Result<Option<Self>, RelayError> {
        match snapshot.host.as_ref().and_then(|host| host.relays.as_ref()) {
            Some(values) => Self::configured(values.clone()).map(Some),
            None => Ok(None),
        }
    }

    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

fn validate_relay_url(value: &str) -> Result<(), RelayError> {
    if value.len() > MAX_RELAY_URL_BYTES {
        return Err(RelayError::Configuration("relay URL is too long".into()));
    }
    let url =
        Url::parse(value).map_err(|_| RelayError::Configuration("relay URL is invalid".into()))?;
    match url.scheme() {
        "wss" => {}
        "ws" if is_loopback_host(url.host_str()) => {}
        "ws" => {
            return Err(RelayError::Configuration(
                "ws:// is allowed only for loopback tests".into(),
            ))
        }
        _ => {
            return Err(RelayError::Configuration(
                "production relays must use wss://".into(),
            ))
        }
    }
    if url.host_str().is_none() || url.fragment().is_some() {
        return Err(RelayError::Configuration("relay URL is invalid".into()));
    }
    Ok(())
}

fn is_loopback_host(host: Option<&str>) -> bool {
    matches!(host, Some("localhost" | "127.0.0.1" | "::1"))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointRecord {
    pub device_public_key: String,
    pub owner_public_key: String,
    pub generation: u64,
    /// Ed25519 EndpointId, authenticated by this device's Nostr signature.
    /// Name and encoding come from the former reach Offer.
    pub endpoint_id: String,
    pub issued_at: u64,
}

impl EndpointRecord {
    pub fn validate(&self, now: u64) -> Result<(), RelayError> {
        validate_public_key(&self.device_public_key)?;
        validate_public_key(&self.owner_public_key)?;
        if self.generation == 0 {
            return Err(RelayError::InvalidEvent(
                "endpoint generation must be positive".into(),
            ));
        }
        self.endpoint_id
            .parse::<iroh::EndpointId>()
            .map_err(|_| RelayError::InvalidEvent("endpoint key is invalid".into()))?;
        if self.issued_at > now.saturating_add(MAX_CLOCK_FUTURE_SECONDS) {
            return Err(RelayError::Expired);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoordinationCommand {
    RunIdleKorrid {
        sender_device_public_key: String,
        requested_at: u64,
        expires_at: u64,
    },
    OwnerBindingRequested {
        sender_device_public_key: String,
        unsigned_event_template: String,
        requested_at: u64,
        expires_at: u64,
    },
    OwnerBindingResponse {
        sender_device_public_key: String,
        owner_public_key: String,
        unsigned_event_template: String,
        signed_event_json: String,
        requested_at: u64,
        expires_at: u64,
    },
}

impl CoordinationCommand {
    fn expires_at(&self) -> u64 {
        match self {
            Self::RunIdleKorrid { expires_at, .. }
            | Self::OwnerBindingRequested { expires_at, .. }
            | Self::OwnerBindingResponse { expires_at, .. } => *expires_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase", deny_unknown_fields)]
enum QueuedMessage {
    RunIdleKorrid {
        requested_at: u64,
        expires_at: u64,
    },
    OwnerBindingRequested {
        unsigned_event_template: String,
        requested_at: u64,
        expires_at: u64,
    },
    OwnerBindingResponse {
        owner_public_key: String,
        unsigned_event_template: String,
        signed_event_json: String,
        requested_at: u64,
        expires_at: u64,
    },
}

impl QueuedMessage {
    fn expires_at(&self) -> u64 {
        match self {
            Self::RunIdleKorrid { expires_at, .. }
            | Self::OwnerBindingRequested { expires_at, .. }
            | Self::OwnerBindingResponse { expires_at, .. } => *expires_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinationSnapshot {
    /// Transport outcome, independent of whether a successful read found events.
    pub all_relay_reads_failed: bool,
    pub endpoints: Vec<EndpointEvidence>,
    pub commands: Vec<CoordinationCommand>,
    pub rejected_events: usize,
}

/// Original signed ciphertext is required for the directory's membership join
/// and durable re-verification. A decoded record is not sufficient evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointEvidence {
    pub record: EndpointRecord,
    pub event_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerStatementEvidence {
    pub statement: VerifiedOwnerStatement,
    pub event_json: String,
}

/// Newest evidence and a signed terminal revocation, if observed, for one device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerRosterEntry {
    pub latest: OwnerStatementEvidence,
    pub revocation: Option<OwnerStatementEvidence>,
}

impl OwnerRosterEntry {
    pub fn is_owned(&self) -> bool {
        self.latest.statement.status == OwnerStatementStatus::Owned && self.revocation.is_none()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayFailure {
    pub relay: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublishState {
    Published {
        accepted_relays: Vec<String>,
    },
    Partial {
        accepted_relays: Vec<String>,
        failed_relays: Vec<RelayFailure>,
    },
    Failed {
        failed_relays: Vec<RelayFailure>,
    },
}

impl PublishState {
    pub fn accepted(&self) -> bool {
        !matches!(self, Self::Failed { .. })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayFilter {
    pub kinds: Vec<u16>,
    pub recipient_public_key: Option<String>,
    pub author_public_key: Option<String>,
    pub since: Option<u64>,
    pub limit: usize,
    /// Addressable event identifiers (`#d`); empty asks for any.
    pub identifiers: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayEvent {
    pub relay: String,
    pub event_json: String,
}

pub trait RelayAuthSigner: Send + Sync {
    fn sign_auth(
        &self,
        relay: &str,
        challenge: &str,
        created_at: u64,
    ) -> Result<String, RelayError>;
}

pub trait RelayTransport: Send + Sync {
    fn publish<'a>(
        &'a self,
        relay: &'a str,
        event_json: &'a str,
        auth: Arc<dyn RelayAuthSigner>,
    ) -> BoxFuture<'a, Result<(), RelayError>>;

    fn read<'a>(
        &'a self,
        relay: &'a str,
        filter: &'a RelayFilter,
        auth: Arc<dyn RelayAuthSigner>,
    ) -> BoxFuture<'a, Result<Vec<RelayEvent>, RelayError>>;

    /// Reach consumes incremental batches when supported. Snapshot adapters may
    /// return repeats; the controller already deduplicates signed event IDs.
    /// A transport owns one device's coordination subscription per relay.
    fn read_coordination<'a>(
        &'a self,
        relay: &'a str,
        filter: &'a RelayFilter,
        auth: Arc<dyn RelayAuthSigner>,
    ) -> BoxFuture<'a, Result<Vec<RelayEvent>, RelayError>> {
        self.read(relay, filter, auth)
    }
}

pub trait RelayCoordinator: Send + Sync {
    fn publish_owner_statement(&self) -> BoxFuture<'_, Result<PublishState, RelayError>>;

    /// Observe whether the exact current owned statement exists on at least one
    /// configured relay. This is prepare-time evidence, not persisted relay state.
    fn owner_statement_was_published<'a>(
        &'a self,
        event_json: &'a str,
    ) -> BoxFuture<'a, Result<bool, RelayError>>;

    /// A bounded observation, never an authoritative list of absent/revoked devices.
    fn read_owner_roster(&self) -> BoxFuture<'_, Result<Vec<OwnerRosterEntry>, RelayError>>;

    fn publish_endpoint<'a>(
        &'a self,
        recipient_device_public_key: &'a str,
        endpoint: EndpointRecord,
        now: u64,
    ) -> BoxFuture<'a, Result<PublishState, RelayError>>;

    fn queue_command<'a>(
        &'a self,
        recipient_device_public_key: &'a str,
        command: CoordinationCommand,
        now: u64,
    ) -> BoxFuture<'a, Result<PublishState, RelayError>>;

    fn receive<'a>(&'a self, now: u64) -> BoxFuture<'a, Result<CoordinationSnapshot, RelayError>>;
}

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("relay configuration: {0}")]
    Configuration(String),
    #[error("relay event is invalid: {0}")]
    InvalidEvent(String),
    #[error("relay event has expired")]
    Expired,
    #[error("relay is unavailable: {0}")]
    Unavailable(String),
    #[error("relay protocol: {0}")]
    Protocol(String),
    #[error("relay operation timed out")]
    Timeout,
    #[error("relay HTTP {code}")]
    Http {
        code: u16,
        retry_after: Option<Duration>,
    },
    #[error("relay DNS lookup failed")]
    Dns,
    #[error("relay backoff: {reason}")]
    Backoff { reason: String },
    #[error("device identity cannot coordinate")]
    Identity,
}

#[derive(Clone)]
pub(crate) struct DeviceAuthSigner(pub(crate) Arc<DeviceIdentity>);

impl RelayAuthSigner for DeviceAuthSigner {
    fn sign_auth(
        &self,
        relay: &str,
        challenge: &str,
        created_at: u64,
    ) -> Result<String, RelayError> {
        self.0
            .relay_auth_event(relay, challenge, created_at)
            .map(|event| event.json)
            .map_err(|_| RelayError::Identity)
    }
}

pub struct CoordinatedRelays<T> {
    relays: RelayList,
    identity: Arc<DeviceIdentity>,
    transport: Arc<T>,
}

impl<T> CoordinatedRelays<T>
where
    T: RelayTransport + 'static,
{
    pub fn new(
        relays: RelayList,
        identity: DeviceIdentity,
        transport: Arc<T>,
    ) -> Result<Self, RelayError> {
        if identity.device_public_key().is_none() {
            return Err(RelayError::Identity);
        }
        Ok(Self {
            relays,
            identity: Arc::new(identity),
            transport,
        })
    }

    // Both publication APIs encode and authorize the exact same command once.
    // The aggregate API below retains its complete per-relay result contract.
    fn command_event(
        &self,
        recipient_device_public_key: &str,
        command: CoordinationCommand,
        now: u64,
    ) -> Result<String, RelayError> {
        validate_public_key(recipient_device_public_key)?;
        if command.expires_at() <= now {
            return Err(RelayError::Expired);
        }
        let own_key = self
            .identity
            .device_public_key()
            .ok_or(RelayError::Identity)?;
        let queued = match command {
            CoordinationCommand::RunIdleKorrid {
                sender_device_public_key,
                requested_at,
                expires_at,
            } => {
                if sender_device_public_key != own_key {
                    return Err(RelayError::InvalidEvent(
                        "command sender is not this device".into(),
                    ));
                }
                QueuedMessage::RunIdleKorrid {
                    requested_at,
                    expires_at,
                }
            }
            CoordinationCommand::OwnerBindingRequested {
                sender_device_public_key,
                unsigned_event_template,
                requested_at,
                expires_at,
            } => {
                if sender_device_public_key != own_key
                    || unsigned_event_template.len() > MAX_EVENT_BYTES
                {
                    return Err(RelayError::InvalidEvent(
                        "owner binding request is invalid".into(),
                    ));
                }
                QueuedMessage::OwnerBindingRequested {
                    unsigned_event_template,
                    requested_at,
                    expires_at,
                }
            }
            CoordinationCommand::OwnerBindingResponse {
                sender_device_public_key,
                owner_public_key,
                unsigned_event_template,
                signed_event_json,
                requested_at,
                expires_at,
            } => {
                if sender_device_public_key != own_key
                    || unsigned_event_template.len() > MAX_EVENT_BYTES
                    || signed_event_json.len() > MAX_EVENT_BYTES
                {
                    return Err(RelayError::InvalidEvent(
                        "owner binding response is invalid".into(),
                    ));
                }
                validate_public_key(&owner_public_key)?;
                DeviceIdentity::verify_event(&signed_event_json).map_err(|_| {
                    RelayError::InvalidEvent("owner binding response is not signed".into())
                })?;
                QueuedMessage::OwnerBindingResponse {
                    owner_public_key,
                    unsigned_event_template,
                    signed_event_json,
                    requested_at,
                    expires_at,
                }
            }
        };
        let content = serde_json::to_string(&queued)
            .map_err(|_| RelayError::InvalidEvent("command cannot be encoded".into()))?;
        let event = self
            .identity
            .gift_wrap(
                recipient_device_public_key,
                COORDINATION_RUMOR_KIND,
                &content,
                now,
                queued.expires_at(),
            )
            .map_err(|_| RelayError::Identity)?;
        Ok(event.json)
    }

    pub fn relay_list(&self) -> &RelayList {
        &self.relays
    }

    async fn publish_json(&self, event_json: &str) -> PublishState {
        let auth: Arc<dyn RelayAuthSigner> = Arc::new(DeviceAuthSigner(self.identity.clone()));
        let results = futures::future::join_all(self.relays.as_slice().iter().map(|relay| {
            let auth = auth.clone();
            async move {
                self.transport
                    .publish(relay, event_json, auth)
                    .await
                    .map(|()| relay.clone())
                    .map_err(|error| RelayFailure {
                        relay: relay.clone(),
                        message: error.to_string(),
                    })
            }
        }))
        .await;
        publish_state(results)
    }

    pub async fn publish_signer_packet(&self, event_json: &str) -> PublishState {
        self.publish_json(event_json).await
    }

    /// NIP-46 packets to `recipient_public_key`; from `author_public_key`
    /// only when it is given.
    pub async fn read_signer_packets(
        &self,
        author_public_key: Option<&str>,
        recipient_public_key: &str,
        since: u64,
    ) -> Result<Vec<String>, RelayError> {
        if let Some(author) = author_public_key {
            validate_public_key(author)?;
        }
        validate_public_key(recipient_public_key)?;
        let filter = RelayFilter {
            kinds: vec![24_133],
            recipient_public_key: Some(recipient_public_key.into()),
            author_public_key: author_public_key.map(str::to_owned),
            since: Some(since),
            limit: MAX_READ_EVENTS,
            identifiers: Vec::new(),
        };
        let mut seen = BTreeSet::new();
        let mut packets = Vec::new();
        for event in self.read_all(&filter).await.0 {
            if let Ok(verified) = DeviceIdentity::verify_encrypted_event(&event.event_json) {
                if seen.insert(verified.id) {
                    packets.push(event.event_json);
                }
            }
        }
        Ok(packets)
    }

    pub(crate) async fn read_person_records(
        &self,
        person: &str,
    ) -> Result<Vec<String>, RelayError> {
        validate_public_key(person)?;
        let filter = RelayFilter {
            kinds: vec![30_078, 0],
            recipient_public_key: None,
            author_public_key: Some(person.into()),
            since: None,
            limit: MAX_READ_EVENTS,
            identifiers: Vec::new(),
        };
        Ok(self
            .read_all(&filter)
            .await
            .0
            .into_iter()
            .map(|event| event.event_json)
            .collect())
    }

    pub(crate) async fn publish_person_name(
        &self,
        event_json: &str,
        person: &str,
    ) -> Result<PublishState, RelayError> {
        if crate::stream_play::person_name::verified_name(event_json, person).is_none() {
            return Err(RelayError::Identity);
        }
        Ok(self.publish_json(event_json).await)
    }

    async fn read_all(&self, filter: &RelayFilter) -> (Vec<RelayEvent>, usize) {
        let auth: Arc<dyn RelayAuthSigner> = Arc::new(DeviceAuthSigner(self.identity.clone()));
        let results = futures::future::join_all(
            self.relays
                .as_slice()
                .iter()
                .map(|relay| self.transport.read(relay, filter, auth.clone())),
        )
        .await;
        let mut events = Vec::new();
        let mut failures = 0;
        for result in results {
            match result {
                Ok(mut delivered) => events.append(&mut delivered),
                Err(_) => failures += 1,
            }
        }
        (events, failures)
    }

    fn owner_public_key(&self) -> Result<&str, RelayError> {
        match self.identity.state() {
            IdentityState::Owned {
                owner_public_key, ..
            } => Ok(owner_public_key),
            _ => Err(RelayError::Identity),
        }
    }
}

impl<T> RelayCoordinator for CoordinatedRelays<T>
where
    T: RelayTransport + 'static,
{
    fn publish_owner_statement(&self) -> BoxFuture<'_, Result<PublishState, RelayError>> {
        Box::pin(async move {
            let events = self.identity.owner_statement_events();
            let (own, chain) = events.split_first().ok_or(RelayError::Identity)?;
            // EVENT keeps its signer; publish_json supplies device NIP-42 auth.
            let state = self.publish_json(own).await;
            // A vouched device also keeps its voucher's chain on the relays,
            // so peers can follow it. Its result does not change the state.
            for event in chain {
                self.publish_json(event).await;
            }
            Ok(state)
        })
    }

    fn owner_statement_was_published<'a>(
        &'a self,
        event_json: &'a str,
    ) -> BoxFuture<'a, Result<bool, RelayError>> {
        Box::pin(async move {
            let (device, owner, event_id, created_at) = match self.identity.state() {
                IdentityState::Owned {
                    device_public_key,
                    owner_public_key,
                    event_id,
                    created_at,
                } => (
                    device_public_key.clone(),
                    owner_public_key.clone(),
                    event_id.clone(),
                    *created_at,
                ),
                _ => return Err(RelayError::Identity),
            };
            let current_json = self
                .identity
                .owner_statement_json()
                .ok_or(RelayError::Identity)?;
            let current_statement = DeviceIdentity::verify_owner_statement(&current_json, &device)
                .map_err(|_| RelayError::Identity)?;
            let current_event =
                DeviceIdentity::verify_event(&current_json).map_err(|_| RelayError::Identity)?;
            let current_signed: serde_json::Value =
                serde_json::from_str(&current_json).map_err(|_| RelayError::Identity)?;
            if current_statement.status != OwnerStatementStatus::Owned
                || current_statement.owner_public_key != owner
                || current_statement.event_id != event_id
                || current_statement.created_at != created_at
            {
                return Err(RelayError::Identity);
            }

            let supplied_statement = DeviceIdentity::verify_owner_statement(event_json, &device)
                .map_err(|_| {
                    RelayError::InvalidEvent(
                        "owner publication evidence is not a valid current statement".into(),
                    )
                })?;
            let supplied_event = DeviceIdentity::verify_event(event_json).map_err(|_| {
                RelayError::InvalidEvent(
                    "owner publication evidence is not a valid current statement".into(),
                )
            })?;
            let supplied_signed: serde_json::Value =
                serde_json::from_str(event_json).map_err(|_| {
                    RelayError::InvalidEvent(
                        "owner publication evidence is not a valid current statement".into(),
                    )
                })?;
            if supplied_statement != current_statement
                || supplied_event != current_event
                || supplied_signed != current_signed
            {
                return Err(RelayError::InvalidEvent(
                    "owner publication evidence is not the current owned statement".into(),
                ));
            }

            let filter = RelayFilter {
                kinds: vec![OWNER_EVENT_KIND],
                recipient_public_key: None,
                author_public_key: Some(owner),
                since: Some(created_at),
                limit: MAX_READ_EVENTS,
                identifiers: Vec::new(),
            };
            let (events, failures) = self.read_all(&filter).await;
            if failures == self.relays.as_slice().len() {
                return Err(RelayError::Unavailable(
                    "all owner publication evidence reads failed".into(),
                ));
            }
            for delivered in events {
                let Ok(statement) =
                    DeviceIdentity::verify_owner_statement(&delivered.event_json, &device)
                else {
                    continue;
                };
                let Ok(event) = DeviceIdentity::verify_event(&delivered.event_json) else {
                    continue;
                };
                let Ok(signed) = serde_json::from_str::<serde_json::Value>(&delivered.event_json)
                else {
                    continue;
                };
                if statement == supplied_statement
                    && event == supplied_event
                    && signed == supplied_signed
                {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }

    fn read_owner_roster(&self) -> BoxFuture<'_, Result<Vec<OwnerRosterEntry>, RelayError>> {
        Box::pin(async move {
            let owner = self.owner_public_key()?;
            let own_key = self
                .identity
                .device_public_key()
                .ok_or(RelayError::Identity)?;
            let filter = RelayFilter {
                kinds: vec![OWNER_EVENT_KIND],
                recipient_public_key: None,
                author_public_key: Some(owner.into()),
                since: None,
                limit: MAX_READ_EVENTS,
                identifiers: Vec::new(),
            };
            // Vouches are signed by devices and name the person key in `p`
            // (ADR 0069), so they need their own read.
            let vouch_filter = RelayFilter {
                author_public_key: None,
                recipient_public_key: Some(owner.into()),
                ..filter.clone()
            };
            // One read at a time: discovery cancels a blocked read as a whole.
            let (events, failures) = self.read_all(&filter).await;
            if failures == self.relays.as_slice().len() {
                return Err(RelayError::Unavailable(
                    "all owner roster reads failed".into(),
                ));
            }
            let (vouches, _) = self.read_all(&vouch_filter).await;
            let known: BTreeMap<String, (String, String)> = events
                .iter()
                .chain(&vouches)
                .filter_map(|delivered| {
                    let event = DeviceIdentity::verify_event(&delivered.event_json).ok()?;
                    Some((event.id, (event.author, delivered.event_json.clone())))
                })
                .collect();
            let chains = vouches
                .iter()
                .filter_map(|delivered| owner_chain(&delivered.event_json, &known, owner))
                .map(|event_json| RelayEvent {
                    relay: String::new(),
                    event_json,
                });
            let mut seen = BTreeSet::new();
            let mut devices: BTreeMap<String, OwnerRosterEntry> = BTreeMap::new();
            for delivered in events.into_iter().chain(chains) {
                let statement = match DeviceIdentity::derive_owner_statement(&delivered.event_json)
                {
                    Ok(statement)
                        if statement.owner_public_key == owner
                            && statement.device_public_key != own_key =>
                    {
                        statement
                    }
                    _ => continue,
                };
                if !seen.insert(statement.event_id.clone()) {
                    continue;
                }
                let evidence = OwnerStatementEvidence {
                    statement,
                    event_json: delivered.event_json,
                };
                let entry = devices
                    .entry(evidence.statement.device_public_key.clone())
                    .or_insert_with(|| OwnerRosterEntry {
                        latest: evidence.clone(),
                        revocation: None,
                    });
                if evidence.statement.supersedes(&entry.latest.statement) {
                    entry.latest = evidence.clone();
                }
                // Authorization treats any signed same-owner revocation as terminal.
                // Retain its full signature even if newer owned evidence exists elsewhere.
                if evidence.statement.status == OwnerStatementStatus::Revoked
                    && entry
                        .revocation
                        .as_ref()
                        .map(|current| evidence.statement.is_newer_than(&current.statement))
                        .unwrap_or(true)
                {
                    entry.revocation = Some(evidence);
                }
            }
            // Aggregate all bounded relay reads before capping devices: duplicate events
            // or newer owned events must not hide a selected device's revocation.
            Ok(devices.into_values().take(MAX_READ_EVENTS).collect())
        })
    }

    fn publish_endpoint<'a>(
        &'a self,
        recipient_device_public_key: &'a str,
        endpoint: EndpointRecord,
        now: u64,
    ) -> BoxFuture<'a, Result<PublishState, RelayError>> {
        Box::pin(async move {
            endpoint.validate(now)?;
            validate_public_key(recipient_device_public_key)?;
            if endpoint.device_public_key
                != self
                    .identity
                    .device_public_key()
                    .ok_or(RelayError::Identity)?
                || endpoint.owner_public_key != self.owner_public_key()?
            {
                return Err(RelayError::InvalidEvent(
                    "endpoint record does not bind the publishing identity".into(),
                ));
            }
            let plaintext = serde_json::to_string(&endpoint)
                .map_err(|_| RelayError::InvalidEvent("endpoint cannot be encoded".into()))?;
            let event = self
                .identity
                .encrypt_tagged_event(
                    recipient_device_public_key,
                    ENDPOINT_EVENT_KIND,
                    vec![
                        vec![
                            "d".into(),
                            format!("{ENDPOINT_EVENT_PREFIX}{recipient_device_public_key}"),
                        ],
                        vec!["p".into(), recipient_device_public_key.into()],
                    ],
                    &plaintext,
                    now,
                )
                .map_err(|_| RelayError::Identity)?;
            Ok(self.publish_json(&event.json).await)
        })
    }

    fn queue_command<'a>(
        &'a self,
        recipient_device_public_key: &'a str,
        command: CoordinationCommand,
        now: u64,
    ) -> BoxFuture<'a, Result<PublishState, RelayError>> {
        Box::pin(async move {
            let event = self.command_event(recipient_device_public_key, command, now)?;
            Ok(self.publish_json(&event).await)
        })
    }

    fn receive<'a>(&'a self, now: u64) -> BoxFuture<'a, Result<CoordinationSnapshot, RelayError>> {
        Box::pin(async move {
            let mut decoder = CoordinationDecoder::new(&self.identity, now)?;
            let (events, failed_reads) = self.read_all(&decoder.filter()).await;
            Ok(decoder.decode(
                events,
                failed_reads,
                failed_reads == self.relays.as_slice().len(),
            ))
        })
    }
}

struct CoordinationDecoder<'a> {
    identity: &'a DeviceIdentity,
    own_key: String,
    now: u64,
    seen: BTreeSet<String>,
    remaining: usize,
}
impl<'a> CoordinationDecoder<'a> {
    fn new(identity: &'a DeviceIdentity, now: u64) -> Result<Self, RelayError> {
        Ok(Self {
            identity,
            own_key: identity
                .device_public_key()
                .ok_or(RelayError::Identity)?
                .to_owned(),
            now,
            seen: BTreeSet::new(),
            remaining: MAX_READ_EVENTS,
        })
    }
    fn filter(&self) -> RelayFilter {
        RelayFilter {
            kinds: vec![ENDPOINT_EVENT_KIND, GIFT_WRAP_EVENT_KIND],
            recipient_public_key: Some(self.own_key.clone()),
            author_public_key: None,
            since: None,
            limit: MAX_READ_EVENTS,
            identifiers: Vec::new(),
        }
    }
    fn decode(
        &mut self,
        events: Vec<RelayEvent>,
        failed_reads: usize,
        all_relay_reads_failed: bool,
    ) -> CoordinationSnapshot {
        let now = self.now;
        let mut endpoints: BTreeMap<String, EndpointEvidence> = BTreeMap::new();
        let mut commands = Vec::new();
        let mut rejected = failed_reads;
        for delivered in events.into_iter().take(self.remaining) {
            self.remaining -= 1;
            let verified = match DeviceIdentity::verify_encrypted_event(&delivered.event_json) {
                Ok(event) if self.seen.insert(event.id.clone()) => event,
                Ok(_) => continue,
                Err(_) => {
                    rejected += 1;
                    continue;
                }
            };
            if event_expired(&verified.tags, now) {
                rejected += 1;
                continue;
            }
            match verified.kind {
                ENDPOINT_EVENT_KIND => {
                    match decode_endpoint(
                        self.identity,
                        &delivered.event_json,
                        &verified.author,
                        &self.own_key,
                        now,
                    ) {
                        Ok(endpoint) => {
                            let replace = endpoints
                                .get(&endpoint.device_public_key)
                                .map(|current| endpoint_is_newer(&endpoint, &current.record))
                                .unwrap_or(true);
                            if replace {
                                endpoints.insert(
                                    endpoint.device_public_key.clone(),
                                    EndpointEvidence {
                                        record: endpoint,
                                        event_json: delivered.event_json.clone(),
                                    },
                                );
                            }
                        }
                        Err(_) => rejected += 1,
                    }
                }
                GIFT_WRAP_EVENT_KIND => {
                    match decode_command(self.identity, &delivered.event_json, now) {
                        Ok(command) => commands.push(command),
                        Err(_) => rejected += 1,
                    }
                }
                _ => rejected += 1,
            }
        }
        commands.sort_by_key(command_order);
        CoordinationSnapshot {
            all_relay_reads_failed,
            endpoints: endpoints.into_values().collect(),
            commands,
            rejected_events: rejected,
        }
    }
}

/// Follow one vouch through the roster's own events to a statement that the
/// person key signed. The result is a vouched device's owner statement; the
/// caller still verifies it in full.
fn owner_chain(
    vouch_json: &str,
    known: &BTreeMap<String, (String, String)>,
    owner: &str,
) -> Option<String> {
    let mut chain = vec![vouch_json.to_owned()];
    let mut current = DeviceIdentity::verify_event(vouch_json).ok()?;
    while chain.len() < crate::identity::MAX_OWNER_CHAIN {
        let next_id = current
            .tags
            .iter()
            .find(|tag| tag.len() == 2 && tag[0] == "e")
            .map(|tag| tag[1].clone())?;
        let (author, json) = known.get(&next_id)?;
        chain.push(json.clone());
        if author == owner {
            return Some(format!("[{}]", chain.join(",")));
        }
        current = DeviceIdentity::verify_event(json).ok()?;
    }
    None
}

pub(crate) fn decode_endpoint(
    identity: &DeviceIdentity,
    event_json: &str,
    author: &str,
    recipient: &str,
    now: u64,
) -> Result<EndpointRecord, RelayError> {
    let verified = DeviceIdentity::verify_encrypted_event(event_json)
        .map_err(|_| RelayError::InvalidEvent("endpoint signature is invalid".into()))?;
    let expected_tags = vec![
        vec!["d".into(), format!("{ENDPOINT_EVENT_PREFIX}{recipient}")],
        vec!["p".into(), recipient.into()],
    ];
    if verified.kind != ENDPOINT_EVENT_KIND || verified.tags != expected_tags {
        return Err(RelayError::InvalidEvent(
            "endpoint event shape is invalid".into(),
        ));
    }
    let plaintext = identity
        .decrypt_event(event_json)
        .map_err(|_| RelayError::InvalidEvent("endpoint cannot be decrypted".into()))?;
    let endpoint: EndpointRecord = serde_json::from_str(&plaintext)
        .map_err(|_| RelayError::InvalidEvent("endpoint content is malformed".into()))?;
    endpoint.validate(now)?;
    if endpoint.device_public_key != author || verified.author != author {
        return Err(RelayError::InvalidEvent(
            "endpoint binding is invalid".into(),
        ));
    }
    Ok(endpoint)
}

fn decode_command(
    identity: &DeviceIdentity,
    event_json: &str,
    now: u64,
) -> Result<CoordinationCommand, RelayError> {
    let rumor = identity
        .unwrap_gift_wrap(event_json, now)
        .map_err(|_| RelayError::InvalidEvent("gift wrap is invalid".into()))?;
    if rumor.kind != COORDINATION_RUMOR_KIND || rumor.content.len() > MAX_EVENT_BYTES {
        return Err(RelayError::InvalidEvent(
            "coordination rumor is invalid".into(),
        ));
    }
    let queued: QueuedMessage = serde_json::from_str(&rumor.content)
        .map_err(|_| RelayError::InvalidEvent("coordination command is malformed".into()))?;
    let command = match queued {
        QueuedMessage::RunIdleKorrid {
            requested_at,
            expires_at,
        } => CoordinationCommand::RunIdleKorrid {
            sender_device_public_key: rumor.author,
            requested_at,
            expires_at,
        },
        QueuedMessage::OwnerBindingRequested {
            unsigned_event_template,
            requested_at,
            expires_at,
        } => CoordinationCommand::OwnerBindingRequested {
            sender_device_public_key: rumor.author,
            unsigned_event_template,
            requested_at,
            expires_at,
        },
        QueuedMessage::OwnerBindingResponse {
            owner_public_key,
            unsigned_event_template,
            signed_event_json,
            requested_at,
            expires_at,
        } => {
            validate_public_key(&owner_public_key)?;
            let signed = DeviceIdentity::verify_event(&signed_event_json).map_err(|_| {
                RelayError::InvalidEvent("owner response signature is invalid".into())
            })?;
            if signed.author != owner_public_key {
                return Err(RelayError::InvalidEvent(
                    "owner response uses a different owner".into(),
                ));
            }
            CoordinationCommand::OwnerBindingResponse {
                sender_device_public_key: rumor.author,
                owner_public_key,
                unsigned_event_template,
                signed_event_json,
                requested_at,
                expires_at,
            }
        }
    };
    if command.expires_at() <= now {
        return Err(RelayError::Expired);
    }
    Ok(command)
}

fn command_order(command: &CoordinationCommand) -> (u64, String) {
    match command {
        CoordinationCommand::RunIdleKorrid {
            sender_device_public_key,
            requested_at,
            ..
        }
        | CoordinationCommand::OwnerBindingRequested {
            sender_device_public_key,
            requested_at,
            ..
        }
        | CoordinationCommand::OwnerBindingResponse {
            sender_device_public_key,
            requested_at,
            ..
        } => (*requested_at, sender_device_public_key.clone()),
    }
}

fn endpoint_is_newer(candidate: &EndpointRecord, current: &EndpointRecord) -> bool {
    candidate.generation > current.generation
        || (candidate.generation == current.generation && candidate.issued_at > current.issued_at)
}

fn event_expired(tags: &[Vec<String>], now: u64) -> bool {
    tags.iter().any(|tag| {
        tag.len() == 2
            && tag[0] == "expiration"
            && tag[1]
                .parse::<u64>()
                .map(|expiry| expiry <= now)
                .unwrap_or(true)
    })
}

fn validate_public_key(value: &str) -> Result<(), RelayError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(RelayError::InvalidEvent("public key is invalid".into()))
    }
}

pub(crate) fn publish_state(results: Vec<Result<String, RelayFailure>>) -> PublishState {
    let mut accepted_relays = Vec::new();
    let mut failed_relays = Vec::new();
    for result in results {
        match result {
            Ok(relay) => accepted_relays.push(relay),
            Err(failure) => failed_relays.push(failure),
        }
    }
    match (accepted_relays.is_empty(), failed_relays.is_empty()) {
        (false, true) => PublishState::Published { accepted_relays },
        (false, false) => PublishState::Partial {
            accepted_relays,
            failed_relays,
        },
        (true, _) => PublishState::Failed { failed_relays },
    }
}

#[derive(Clone)]
pub struct InProcessRelayNetwork {
    inner: Arc<Mutex<BTreeMap<String, InProcessRelay>>>,
}

#[derive(Clone, Debug)]
struct StoredRelayEvent {
    id: String,
    author: String,
    kind: u16,
    created_at: u64,
    tags: Vec<Vec<String>>,
    json: String,
    visible_after_read: u64,
}

#[derive(Clone, Debug)]
struct InProcessRelay {
    available: bool,
    read_count: u64,
    next_delivery_delay: u64,
    events: Vec<StoredRelayEvent>,
}

impl InProcessRelayNetwork {
    pub fn new(relays: &RelayList) -> Self {
        Self {
            inner: Arc::new(Mutex::new(
                relays
                    .as_slice()
                    .iter()
                    .cloned()
                    .map(|relay| {
                        (
                            relay,
                            InProcessRelay {
                                available: true,
                                read_count: 0,
                                next_delivery_delay: 0,
                                events: Vec::new(),
                            },
                        )
                    })
                    .collect(),
            )),
        }
    }

    pub fn set_available(&self, relay: &str, available: bool) {
        if let Some(node) = self
            .inner
            .lock()
            .expect("relay lock poisoned")
            .get_mut(relay)
        {
            node.available = available;
        }
    }

    pub fn delay_next_delivery(&self, relay: &str, reads: u64) {
        if let Some(node) = self
            .inner
            .lock()
            .expect("relay lock poisoned")
            .get_mut(relay)
        {
            node.next_delivery_delay = reads;
        }
    }

    pub fn stored_event_count(&self, relay: &str) -> usize {
        self.inner
            .lock()
            .expect("relay lock poisoned")
            .get(relay)
            .map(|node| node.events.len())
            .unwrap_or(0)
    }

    pub fn inject(&self, relay: &str, event_json: &str) -> Result<(), RelayError> {
        self.store(relay, event_json)
    }

    fn store(&self, relay: &str, event_json: &str) -> Result<(), RelayError> {
        if event_json.len() > MAX_EVENT_BYTES {
            return Err(RelayError::InvalidEvent("event exceeds relay bound".into()));
        }
        let verified = DeviceIdentity::verify_encrypted_event(event_json)
            .map_err(|_| RelayError::InvalidEvent("relay rejected malformed event".into()))?;
        let mut relays = self.inner.lock().expect("relay lock poisoned");
        let node = relays
            .get_mut(relay)
            .ok_or_else(|| RelayError::Unavailable(relay.into()))?;
        if !node.available {
            return Err(RelayError::Unavailable(relay.into()));
        }
        if node.events.iter().any(|event| event.id == verified.id) {
            return Ok(());
        }
        let address = addressable_key(verified.kind, &verified.author, &verified.tags);
        if let Some(address) = address {
            if let Some(current) = node.events.iter().position(|event| {
                addressable_key(event.kind, &event.author, &event.tags).as_ref() == Some(&address)
            }) {
                let existing = &node.events[current];
                let newer = event_is_newer(
                    verified.created_at,
                    &verified.id,
                    existing.created_at,
                    &existing.id,
                );
                if !newer {
                    return Ok(());
                }
                node.events.remove(current);
            }
        }
        if node.events.len() >= MAX_STORED_EVENTS_PER_RELAY {
            node.events.remove(0);
        }
        let visible_after_read = node
            .read_count
            .saturating_add(node.next_delivery_delay)
            .saturating_add(1);
        node.next_delivery_delay = 0;
        node.events.push(StoredRelayEvent {
            id: verified.id,
            author: verified.author,
            kind: verified.kind,
            created_at: verified.created_at,
            tags: verified.tags,
            json: event_json.into(),
            visible_after_read,
        });
        Ok(())
    }
}

impl RelayTransport for InProcessRelayNetwork {
    fn publish<'a>(
        &'a self,
        relay: &'a str,
        event_json: &'a str,
        _auth: Arc<dyn RelayAuthSigner>,
    ) -> BoxFuture<'a, Result<(), RelayError>> {
        Box::pin(async move { self.store(relay, event_json) })
    }

    fn read<'a>(
        &'a self,
        relay: &'a str,
        filter: &'a RelayFilter,
        _auth: Arc<dyn RelayAuthSigner>,
    ) -> BoxFuture<'a, Result<Vec<RelayEvent>, RelayError>> {
        Box::pin(async move {
            if filter.limit == 0 || filter.limit > MAX_READ_EVENTS {
                return Err(RelayError::Protocol("subscription limit is invalid".into()));
            }
            let mut relays = self.inner.lock().expect("relay lock poisoned");
            let node = relays
                .get_mut(relay)
                .ok_or_else(|| RelayError::Unavailable(relay.into()))?;
            if !node.available {
                return Err(RelayError::Unavailable(relay.into()));
            }
            node.read_count = node.read_count.saturating_add(1);
            let read_count = node.read_count;
            let mut matched: Vec<_> = node
                .events
                .iter()
                .filter(|event| {
                    event.visible_after_read <= read_count
                        && filter.kinds.contains(&event.kind)
                        && filter
                            .author_public_key
                            .as_ref()
                            .map(|author| author == &event.author)
                            .unwrap_or(true)
                        && filter
                            .since
                            .map(|since| event.created_at >= since)
                            .unwrap_or(true)
                        && filter
                            .recipient_public_key
                            .as_ref()
                            .map(|recipient| has_tag(&event.tags, "p", recipient))
                            .unwrap_or(true)
                        && (filter.identifiers.is_empty()
                            || filter
                                .identifiers
                                .iter()
                                .any(|identifier| has_tag(&event.tags, "d", identifier)))
                })
                .collect();
            matched.sort_by(|left, right| {
                right
                    .created_at
                    .cmp(&left.created_at)
                    .then_with(|| left.id.cmp(&right.id))
            });
            Ok(matched
                .into_iter()
                .take(filter.limit)
                .map(|event| RelayEvent {
                    relay: relay.into(),
                    event_json: event.json.clone(),
                })
                .collect())
        })
    }
}

fn addressable_key(kind: u16, author: &str, tags: &[Vec<String>]) -> Option<String> {
    if !(30_000..40_000).contains(&kind) {
        return None;
    }
    tags.iter()
        .find(|tag| tag.len() >= 2 && tag[0] == "d")
        .map(|tag| format!("{kind}:{author}:{}", tag[1]))
}

fn has_tag(tags: &[Vec<String>], name: &str, value: &str) -> bool {
    tags.iter()
        .any(|tag| tag.len() >= 2 && tag[0] == name && tag[1] == value)
}

/// Production NIP-01 transport. Every caller shares the relay's refusal window.
pub struct WebSocketRelayTransport {
    relays: Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<RelayConnection>>>>,
}

#[derive(Default)]
struct RelayConnection {
    retry_at: Option<Instant>,
    failures: u32,
    reason: String,
    read_failures: u64,
    read_report: Option<(Instant, String)>,
    socket: Option<RelaySocket>,
    authentication: Authentication,
    coordination: Option<CoordinationSubscription>,
    maintenance_auth: Option<Arc<dyn RelayAuthSigner>>,
    maintenance: Option<tokio::task::JoinHandle<()>>,
}
impl Drop for RelayConnection {
    fn drop(&mut self) {
        if let Some(task) = &self.maintenance {
            task.abort();
        }
    }
}
impl RelayConnection {
    fn begin(&mut self) -> Result<(), RelayError> {
        if self.retry_at.is_some_and(|until| until > Instant::now()) {
            return Err(RelayError::Backoff {
                reason: self.reason.clone(),
            });
        }
        self.failures = self.failures.saturating_add(1);
        // Reserve the retry window before I/O. Cancellation must not let the next
        // reach pass open another connection to the same unresponsive relay.
        self.reason = "timeout".into();
        self.retry_at = Some(Instant::now() + self.reconnect_delay());
        Ok(())
    }

    fn read_failed(&mut self, _relay: &str, error: &RelayError) {
        self.read_failures = self.read_failures.saturating_add(1);
        let class = relay_failure_class(&error.to_string());
        let now = Instant::now();
        // Count every failed read, including backoff suppression. Report a new
        // reason immediately, then totals once a minute instead of every poll.
        if self.read_report.as_ref().is_none_or(|(at, previous)| {
            previous != &class || now.duration_since(*at) >= Duration::from_secs(60)
        }) {
            log::warn!(
                "Nostr record relay read failed: {}",
                relay_failure_class(&error.to_string())
            );
            self.read_report = Some((now, class));
        }
    }

    fn reconnect_delay(&self) -> Duration {
        INITIAL_RECONNECT_DELAY
            .saturating_mul(
                1u32.checked_shl(self.failures.saturating_sub(1))
                    .unwrap_or(u32::MAX),
            )
            .min(MAX_RECONNECT_DELAY)
    }

    fn complete<T>(&mut self, result: &Result<T, RelayError>) {
        match result {
            Ok(_) => {
                self.retry_at = None;
                self.failures = 0;
            }
            Err(error) => {
                self.reason = relay_failure_class(&error.to_string());
                let delay = match error {
                    RelayError::Http { code, retry_after } => {
                        let base = if (400..500).contains(code) {
                            REFUSAL_BACKOFF
                        } else {
                            self.reconnect_delay()
                        };
                        base.max(retry_after.unwrap_or_default())
                    }
                    RelayError::Protocol(_) => REFUSAL_BACKOFF,
                    _ => self.reconnect_delay(),
                };
                self.retry_at = Some(Instant::now() + delay);
            }
        }
    }
}

impl WebSocketRelayTransport {
    pub fn new() -> Self {
        Self {
            relays: Mutex::new(BTreeMap::new()),
        }
    }

    fn connection(
        &self,
        relay: &str,
    ) -> Result<Arc<tokio::sync::Mutex<RelayConnection>>, RelayError> {
        validate_relay_url(relay)?;
        let mut relays = self.relays.lock().expect("relay connections");
        if !relays.contains_key(relay) && relays.len() >= MAX_RELAYS {
            let unused = relays
                .iter()
                .find(|(_, state)| Arc::strong_count(state) == 1)
                .map(|(url, _)| url.clone());
            if let Some(unused) = unused {
                relays.remove(&unused);
            } else {
                return Err(RelayError::Unavailable(
                    "relay connection bound reached".into(),
                ));
            }
        }
        Ok(relays
            .entry(relay.into())
            .or_insert_with(|| {
                let state = Arc::new(tokio::sync::Mutex::new(RelayConnection::default()));
                let task = tokio::spawn(maintain_connection(
                    relay.to_owned(),
                    Arc::downgrade(&state),
                ));
                state.try_lock().expect("new relay connection").maintenance = Some(task);
                state
            })
            .clone())
    }

    async fn connect(
        &self,
        relay: &str,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        RelayError,
    > {
        validate_relay_url(relay)?;
        timeout(CONNECT_TIMEOUT, connect_async(relay))
            .await
            .map_err(|_| RelayError::Timeout)?
            .map(|(socket, _)| socket)
            .map_err(websocket_error)
    }
}

fn websocket_error(error: tokio_tungstenite::tungstenite::Error) -> RelayError {
    use tokio_tungstenite::tungstenite::Error;
    match error {
        Error::Http(response) => RelayError::Http {
            code: response.status().as_u16(),
            retry_after: response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(retry_after),
        },
        Error::Protocol(
            tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
        ) => RelayError::Unavailable("relay closed".into()),
        Error::Io(ref error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::UnexpectedEof
            ) =>
        {
            RelayError::Unavailable("relay closed".into())
        }
        Error::ConnectionClosed | Error::AlreadyClosed => {
            RelayError::Unavailable("relay closed".into())
        }
        Error::Io(ref error)
            if [
                "failed to lookup address",
                "Name or service not known",
                "dns error",
            ]
            .iter()
            .any(|part| error.to_string().contains(part)) =>
        {
            RelayError::Dns
        }
        error => RelayError::Unavailable(error.to_string()),
    }
}

fn retry_after(value: &str) -> Option<Duration> {
    if let Ok(seconds) = value.trim().parse::<u64>() {
        let delay = Duration::from_secs(seconds);
        Instant::now().checked_add(delay)?;
        return Some(delay);
    }
    let date = chrono::DateTime::parse_from_rfc2822(value)
        .ok()?
        .timestamp();
    Some(Duration::from_secs(
        u64::try_from(date).ok()?.saturating_sub(unix_now()),
    ))
}

/// Only fixed protocol labels leave this seam. Never return an arbitrary prefix
/// or a substring of a relay's response, even when it resembles a diagnostic.
const NIP_REASON_CLASSES: &[&str] = &[
    "blocked",
    "rate-limited",
    "invalid",
    "restricted",
    "auth-required",
    "pow",
    "duplicate",
    "error",
];

fn nip_reason_class(message: &str) -> Option<&'static str> {
    NIP_REASON_CLASSES.iter().copied().find(|prefix| {
        message
            .strip_prefix(prefix)
            .is_some_and(|tail| tail.starts_with(':'))
    })
}

pub(crate) fn relay_failure_class(message: &str) -> String {
    if let Some(reason) = message.strip_prefix("relay protocol: ") {
        // Raw protocol text cannot impersonate a typed transport error.
        return nip_reason_class(reason).unwrap_or("unprefixed").into();
    }
    if let Some(class) = message.strip_prefix("relay backoff: ") {
        if NIP_REASON_CLASSES.contains(&class)
            || ["timeout", "closed", "dns", "unprefixed"].contains(&class)
        {
            return class.into();
        }
        return http_reason_class(class, "http_").unwrap_or_else(|| "unprefixed".into());
    }
    if let Some(class) = nip_reason_class(message) {
        return class.into();
    }
    if let Some(class) = http_reason_class(message, "relay HTTP ") {
        return class;
    }
    match message {
        "relay operation timed out" => "timeout".into(),
        "relay DNS lookup failed" => "dns".into(),
        value if value.starts_with("relay is unavailable: relay closed") => "closed".into(),
        _ => "unprefixed".into(),
    }
}

fn http_reason_class(message: &str, prefix: &str) -> Option<String> {
    message
        .strip_prefix(prefix)
        .and_then(|code| code.parse::<u16>().ok())
        .filter(|code| (100..600).contains(code))
        .map(|code| format!("http_{code}"))
}

impl Default for WebSocketRelayTransport {
    fn default() -> Self {
        Self::new()
    }
}

type RelaySocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
const COORDINATION_SUBSCRIPTION: &str = "korri-reach";
const SNAPSHOT_SUBSCRIPTION: &str = "korri-snapshot";
const LIVE_READ_WINDOW: Duration = Duration::from_millis(20);

#[derive(Default)]
enum Authentication {
    #[default]
    Unchallenged,
    Pending(String),
    Accepted,
}
#[derive(Default)]
enum AuthRetry {
    #[default]
    Initial,
    Required,
    Retried,
}
struct CoordinationSubscription {
    filter: RelayFilter,
    ready: bool,
    authentication: AuthRetry,
    events: std::collections::VecDeque<RelayEvent>,
}

impl WebSocketRelayTransport {
    async fn exchange<T: Send>(
        &self,
        relay: &str,
        read: bool,
        auth: Arc<dyn RelayAuthSigner>,
        operation: impl for<'a> FnOnce(
                &'a mut RelayConnection,
                &'a mut RelaySocket,
            ) -> BoxFuture<'a, Result<T, RelayError>>
            + Send,
    ) -> Result<T, RelayError> {
        let deadline = Instant::now() + OPERATION_TIMEOUT;
        let state = self.connection(relay)?;
        let mut state = tokio::time::timeout_at(deadline, state.lock())
            .await
            .map_err(|_| RelayError::Timeout)?;
        if let Err(error) = state.begin() {
            if read {
                state.read_failed(relay, &error);
            }
            return Err(error);
        }
        state.maintenance_auth = Some(auth);
        // The socket lives outside the table during I/O. Cancellation drops it,
        // but retains the reserved backoff and any already received live events.
        let mut socket = state.socket.take();
        let result = tokio::time::timeout_at(deadline, async {
            if socket.is_none() {
                state.authentication = Authentication::Unchallenged;
                socket = Some(self.connect(relay).await?);
                if let Some(subscription) = &mut state.coordination {
                    subscription.ready = false;
                    subscription.authentication = AuthRetry::Initial;
                    send_frame(
                        socket.as_mut().expect("connected relay"),
                        relay_request(COORDINATION_SUBSCRIPTION, &subscription.filter),
                    )
                    .await?;
                }
            }
            operation(&mut state, socket.as_mut().expect("connected relay")).await
        })
        .await
        .unwrap_or(Err(RelayError::Timeout));
        if result.is_ok() {
            state.socket = socket;
        }
        state.complete(&result);
        if let Err(error) = &result {
            if read {
                state.read_failed(relay, error);
            }
        }
        result
    }
}

/// Keep snapshot-only callers' sockets alive too. The table owns this task;
/// its weak reference never keeps a discarded transport or relay entry alive.
async fn maintain_connection(
    relay: String,
    weak: std::sync::Weak<tokio::sync::Mutex<RelayConnection>>,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let Some(connection) = weak.upgrade() else {
            return;
        };
        let Ok(mut state) = connection.try_lock() else {
            continue;
        };
        let Some(auth) = state.maintenance_auth.clone() else {
            continue;
        };
        let Some(mut socket) = state.socket.take() else {
            continue;
        };
        let result = timeout(OPERATION_TIMEOUT, async {
            let deadline = Instant::now() + LIVE_READ_WINDOW;
            let mut bytes = 0;
            loop {
                let frame =
                    match tokio::time::timeout_at(deadline, next_frame(&mut socket, &mut bytes))
                        .await
                    {
                        Ok(result) => result?,
                        Err(_) => return Ok::<_, RelayError>(()),
                    };
                connection_frame(&relay, &mut socket, &mut state, auth.as_ref(), &frame).await?;
            }
        })
        .await
        .unwrap_or(Err(RelayError::Timeout));
        match &result {
            Ok(()) => state.socket = Some(socket),
            Err(error) => {
                state.failures = state.failures.saturating_add(1);
                state.complete(&result);
                state.read_failed(&relay, error);
            }
        }
    }
}

async fn send_frame(socket: &mut RelaySocket, frame: String) -> Result<(), RelayError> {
    socket
        .send(Message::Text(frame.into()))
        .await
        .map_err(websocket_error)
}

async fn next_frame(
    socket: &mut RelaySocket,
    bytes: &mut usize,
) -> Result<serde_json::Value, RelayError> {
    while let Some(message) = socket.next().await {
        let message = message.map_err(websocket_error)?;
        if let Some(text) = bounded_text(message, bytes)? {
            return serde_json::from_str(&text)
                .map_err(|_| RelayError::Protocol("relay response is malformed".into()));
        }
        // Tungstenite queues the PONG when it reads PING. Flush it even when
        // this operation has no application message to send back.
        socket.flush().await.map_err(websocket_error)?;
    }
    Err(RelayError::Unavailable("relay closed".into()))
}

fn frame_reason(frame: &serde_json::Value, index: usize) -> &str {
    frame
        .get(index)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("relay rejected operation")
}

fn received_event(
    relay: &str,
    value: &serde_json::Value,
) -> Result<Option<(String, RelayEvent)>, RelayError> {
    let event = value
        .get(2)
        .ok_or_else(|| RelayError::Protocol("EVENT payload is missing".into()))?;
    let json = serde_json::to_string(event)
        .map_err(|_| RelayError::Protocol("EVENT payload is invalid".into()))?;
    if json.len() > MAX_EVENT_BYTES {
        return Ok(None);
    }
    Ok(DeviceIdentity::verify_encrypted_event(&json)
        .ok()
        .map(|verified| {
            (
                verified.id,
                RelayEvent {
                    relay: relay.into(),
                    event_json: json,
                },
            )
        }))
}

async fn retry_authenticated_subscription(
    socket: &mut RelaySocket,
    retry: &mut AuthRetry,
    authenticated: bool,
    request: &str,
) -> Result<(), RelayError> {
    if authenticated && matches!(retry, AuthRetry::Required) {
        send_frame(socket, request.into()).await?;
        *retry = AuthRetry::Retried;
    }
    Ok(())
}

/// Demultiplex connection authentication and the retained reach subscription.
/// Other read contracts keep their complete snapshot and EOSE semantics.
async fn connection_frame(
    relay: &str,
    socket: &mut RelaySocket,
    state: &mut RelayConnection,
    auth: &dyn RelayAuthSigner,
    frame: &serde_json::Value,
) -> Result<bool, RelayError> {
    match frame.get(0).and_then(serde_json::Value::as_str) {
        Some("AUTH") => {
            if !matches!(state.authentication, Authentication::Unchallenged) {
                return Err(RelayError::Protocol("AUTH challenge limit exceeded".into()));
            }
            let challenge = frame
                .get(1)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| RelayError::Protocol("AUTH challenge is malformed".into()))?;
            let event: serde_json::Value =
                serde_json::from_str(&auth.sign_auth(relay, challenge, unix_now())?)
                    .map_err(|_| RelayError::Protocol("AUTH event is malformed".into()))?;
            let id = event
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| RelayError::Protocol("AUTH event id is missing".into()))?
                .to_owned();
            send_frame(socket, serde_json::json!(["AUTH", event]).to_string()).await?;
            state.authentication = Authentication::Pending(id);
            return Ok(true);
        }
        Some("OK") if matches!(&state.authentication, Authentication::Pending(id) if frame.get(1).and_then(serde_json::Value::as_str) == Some(id.as_str())) =>
        {
            if frame.get(2).and_then(serde_json::Value::as_bool) != Some(true) {
                return Err(RelayError::Protocol(frame_reason(frame, 3).into()));
            }
            state.authentication = Authentication::Accepted;
            if let Some(subscription) = &mut state.coordination {
                retry_authenticated_subscription(
                    socket,
                    &mut subscription.authentication,
                    true,
                    &relay_request(COORDINATION_SUBSCRIPTION, &subscription.filter),
                )
                .await?;
            }
            return Ok(true);
        }
        _ => {}
    }
    let Some(subscription) = &mut state.coordination else {
        return Ok(false);
    };
    if frame.get(1).and_then(serde_json::Value::as_str) != Some(COORDINATION_SUBSCRIPTION) {
        return Ok(false);
    }
    match frame.get(0).and_then(serde_json::Value::as_str) {
        Some("EVENT") => {
            if let Some((_, event)) = received_event(relay, frame)? {
                if subscription.events.len() >= MAX_READ_EVENTS {
                    subscription.events.pop_front();
                }
                subscription.events.push_back(event);
            }
        }
        Some("EOSE") => subscription.ready = true,
        Some("CLOSED") => {
            let reason = frame_reason(frame, 2);
            if !reason.starts_with("auth-required:")
                || !matches!(subscription.authentication, AuthRetry::Initial)
            {
                return Err(RelayError::Protocol(reason.into()));
            }
            subscription.authentication = AuthRetry::Required;
            retry_authenticated_subscription(
                socket,
                &mut subscription.authentication,
                matches!(state.authentication, Authentication::Accepted),
                &relay_request(COORDINATION_SUBSCRIPTION, &subscription.filter),
            )
            .await?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

async fn publish_operation(
    relay: String,
    event: serde_json::Value,
    id: String,
    auth: Arc<dyn RelayAuthSigner>,
    state: &mut RelayConnection,
    socket: &mut RelaySocket,
) -> Result<(), RelayError> {
    let request = serde_json::json!(["EVENT", event]).to_string();
    send_frame(socket, request.clone()).await?;
    let mut retry = AuthRetry::Initial;
    let mut bytes = 0;
    loop {
        let frame = next_frame(socket, &mut bytes).await?;
        let handled = connection_frame(&relay, socket, state, auth.as_ref(), &frame).await?;
        if !handled
            && frame.get(0).and_then(serde_json::Value::as_str) == Some("OK")
            && frame.get(1).and_then(serde_json::Value::as_str) == Some(&id)
        {
            if frame.get(2).and_then(serde_json::Value::as_bool) == Some(true) {
                return Ok(());
            }
            let reason = frame_reason(&frame, 3);
            if frame.get(2).and_then(serde_json::Value::as_bool) != Some(false)
                || !reason.starts_with("auth-required:")
                || !matches!(retry, AuthRetry::Initial)
            {
                return Err(RelayError::Protocol(reason.into()));
            }
            retry = AuthRetry::Required;
        }
        // A matching AUTH OK and the initial EVENT's auth-required rejection
        // may arrive in either order. Only both permit the one EVENT retry.
        retry_authenticated_subscription(
            socket,
            &mut retry,
            matches!(state.authentication, Authentication::Accepted),
            &request,
        )
        .await?;
    }
}

async fn snapshot_read(
    relay: String,
    filter: RelayFilter,
    auth: Arc<dyn RelayAuthSigner>,
    state: &mut RelayConnection,
    socket: &mut RelaySocket,
) -> Result<Vec<RelayEvent>, RelayError> {
    let request = relay_request(SNAPSHOT_SUBSCRIPTION, &filter);
    send_frame(socket, request.clone()).await?;
    let mut bytes = 0;
    let mut events = Vec::new();
    let mut seen = BTreeSet::new();
    let mut retry = AuthRetry::Initial;
    loop {
        let frame = next_frame(socket, &mut bytes).await?;
        let handled = connection_frame(&relay, socket, state, auth.as_ref(), &frame).await?;
        retry_authenticated_subscription(
            socket,
            &mut retry,
            matches!(state.authentication, Authentication::Accepted),
            &request,
        )
        .await?;
        if handled
            || frame.get(1).and_then(serde_json::Value::as_str) != Some(SNAPSHOT_SUBSCRIPTION)
        {
            continue;
        }
        match frame.get(0).and_then(serde_json::Value::as_str) {
            Some("EVENT") if events.len() < filter.limit => {
                if let Some((id, event)) = received_event(&relay, &frame)? {
                    if seen.insert(id) {
                        events.push(event);
                    }
                }
            }
            Some("EOSE") => {
                send_frame(
                    socket,
                    serde_json::json!(["CLOSE", SNAPSHOT_SUBSCRIPTION]).to_string(),
                )
                .await?;
                return Ok(events);
            }
            Some("CLOSED") => {
                let reason = frame_reason(&frame, 2);
                if !reason.starts_with("auth-required:") || !matches!(retry, AuthRetry::Initial) {
                    return Err(RelayError::Protocol(reason.into()));
                }
                retry = AuthRetry::Required;
                retry_authenticated_subscription(
                    socket,
                    &mut retry,
                    matches!(state.authentication, Authentication::Accepted),
                    &request,
                )
                .await?;
            }
            _ => {}
        }
    }
}

async fn coordination_read(
    relay: String,
    filter: RelayFilter,
    auth: Arc<dyn RelayAuthSigner>,
    state: &mut RelayConnection,
    socket: &mut RelaySocket,
) -> Result<Vec<RelayEvent>, RelayError> {
    let replace = state.coordination.as_ref().is_none_or(|subscription| {
        subscription.filter.kinds != filter.kinds
            || subscription.filter.recipient_public_key != filter.recipient_public_key
            || subscription.filter.author_public_key != filter.author_public_key
            || subscription.filter.identifiers != filter.identifiers
            || subscription.filter.limit != filter.limit
    });
    if replace {
        if state.coordination.is_some() {
            send_frame(
                socket,
                serde_json::json!(["CLOSE", COORDINATION_SUBSCRIPTION]).to_string(),
            )
            .await?;
        }
        send_frame(socket, relay_request(COORDINATION_SUBSCRIPTION, &filter)).await?;
        state.coordination = Some(CoordinationSubscription {
            filter,
            ready: false,
            authentication: AuthRetry::Initial,
            events: Default::default(),
        });
    } else {
        // The live filter stays installed. Only the next reconnect uses the
        // caller's newer overlap; don't issue another REQ on every reach pass.
        state
            .coordination
            .as_mut()
            .expect("coordination subscription")
            .filter
            .since = filter.since;
    }
    let mut bytes = 0;
    let mut drain_until = None;
    loop {
        if state
            .coordination
            .as_ref()
            .expect("coordination subscription")
            .ready
        {
            let deadline = *drain_until.get_or_insert_with(|| Instant::now() + LIVE_READ_WINDOW);
            match tokio::time::timeout_at(deadline, next_frame(socket, &mut bytes)).await {
                Ok(frame) => {
                    connection_frame(&relay, socket, state, auth.as_ref(), &frame?).await?;
                }
                Err(_) => {
                    return Ok(state
                        .coordination
                        .as_mut()
                        .expect("coordination subscription")
                        .events
                        .drain(..)
                        .collect())
                }
            }
        } else {
            let frame = next_frame(socket, &mut bytes).await?;
            connection_frame(&relay, socket, state, auth.as_ref(), &frame).await?;
        }
    }
}

fn validate_filter(filter: &RelayFilter) -> Result<(), RelayError> {
    if filter.limit == 0 || filter.limit > MAX_READ_EVENTS {
        Err(RelayError::Protocol("subscription bound is invalid".into()))
    } else {
        Ok(())
    }
}

impl RelayTransport for WebSocketRelayTransport {
    fn publish<'a>(
        &'a self,
        relay: &'a str,
        event_json: &'a str,
        auth: Arc<dyn RelayAuthSigner>,
    ) -> BoxFuture<'a, Result<(), RelayError>> {
        Box::pin(async move {
            if event_json.len() > MAX_EVENT_BYTES {
                return Err(RelayError::InvalidEvent("event exceeds relay bound".into()));
            }
            let event: serde_json::Value = serde_json::from_str(event_json)
                .map_err(|_| RelayError::InvalidEvent("event JSON is malformed".into()))?;
            let id = event
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| RelayError::InvalidEvent("event id is missing".into()))?
                .to_owned();
            let url = relay.to_owned();
            self.exchange(relay, false, auth.clone(), move |state, socket| {
                Box::pin(publish_operation(url, event, id, auth, state, socket))
            })
            .await
        })
    }
    fn read<'a>(
        &'a self,
        relay: &'a str,
        filter: &'a RelayFilter,
        auth: Arc<dyn RelayAuthSigner>,
    ) -> BoxFuture<'a, Result<Vec<RelayEvent>, RelayError>> {
        Box::pin(async move {
            validate_filter(filter)?;
            let (url, filter) = (relay.to_owned(), filter.clone());
            self.exchange(relay, true, auth.clone(), move |state, socket| {
                Box::pin(snapshot_read(url, filter, auth, state, socket))
            })
            .await
        })
    }
    fn read_coordination<'a>(
        &'a self,
        relay: &'a str,
        filter: &'a RelayFilter,
        auth: Arc<dyn RelayAuthSigner>,
    ) -> BoxFuture<'a, Result<Vec<RelayEvent>, RelayError>> {
        Box::pin(async move {
            validate_filter(filter)?;
            let (url, filter) = (relay.to_owned(), filter.clone());
            self.exchange(relay, true, auth.clone(), move |state, socket| {
                Box::pin(coordination_read(url, filter, auth, state, socket))
            })
            .await
        })
    }
}

fn relay_request(subscription: &str, filter: &RelayFilter) -> String {
    let mut wire = serde_json::Map::new();
    wire.insert("kinds".into(), serde_json::json!(filter.kinds));
    if let Some(recipient) = &filter.recipient_public_key {
        wire.insert("#p".into(), serde_json::json!([recipient]));
    }
    if !filter.identifiers.is_empty() {
        wire.insert("#d".into(), serde_json::json!(filter.identifiers));
    }
    wire.insert("limit".into(), serde_json::json!(filter.limit));
    if let Some(author) = &filter.author_public_key {
        wire.insert("authors".into(), serde_json::json!([author]));
    }
    if let Some(since) = filter.since {
        wire.insert("since".into(), serde_json::json!(since));
    }
    serde_json::json!(["REQ", subscription, wire]).to_string()
}

fn bounded_text(message: Message, bytes: &mut usize) -> Result<Option<String>, RelayError> {
    let text = match message {
        Message::Text(text) => text.to_string(),
        Message::Binary(binary) => String::from_utf8(binary.to_vec())
            .map_err(|_| RelayError::Protocol("relay response is not UTF-8".into()))?,
        Message::Ping(_) | Message::Pong(_) => return Ok(None),
        Message::Close(_) => return Err(RelayError::Unavailable("relay closed".into())),
        _ => return Ok(None),
    };
    *bytes = bytes.saturating_add(text.len());
    if text.len() > MAX_EVENT_BYTES || *bytes > MAX_RESPONSE_BYTES {
        return Err(RelayError::Protocol("relay response exceeds bounds".into()));
    }
    Ok(Some(text))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RelayInformation {
    #[serde(default)]
    pub supported_nips: Vec<u16>,
    #[serde(default)]
    pub limitation: RelayLimitations,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RelayLimitations {
    pub max_message_length: Option<usize>,
    pub max_subscriptions: Option<usize>,
    pub max_limit: Option<usize>,
    pub max_content_length: Option<usize>,
    pub auth_required: Option<bool>,
}

impl RelayInformation {
    pub fn parse_bounded(bytes: &[u8]) -> Result<Self, RelayError> {
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(RelayError::Protocol(
                "NIP-11 metadata exceeds bounds".into(),
            ));
        }
        let information: Self = serde_json::from_slice(bytes)
            .map_err(|_| RelayError::Protocol("NIP-11 metadata is malformed".into()))?;
        if information.supported_nips.len() > 128 {
            return Err(RelayError::Protocol(
                "NIP-11 metadata exceeds bounds".into(),
            ));
        }
        Ok(information)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{Nip46ConnectionIdentity, OwnerStatementStatus};
    use nostr::{
        event::{EventBuilder, FinalizeEvent, Kind, Tag},
        key::Keys,
        types::Timestamp,
    };
    use std::fs;
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    pub(super) fn owned_identity(
        root: &std::path::Path,
        owner: &Keys,
        created_at: u64,
    ) -> DeviceIdentity {
        let mut identity = DeviceIdentity::load_or_create(root).unwrap();
        let device = identity.device_public_key().unwrap().to_owned();
        let statement = EventBuilder::new(Kind::Custom(30_078), "")
            .tags([
                Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
                Tag::parse(["device", &device]).unwrap(),
                Tag::parse(["status", "owned"]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(created_at))
            .finalize(owner)
            .unwrap()
            .as_json();
        identity.apply_owner_statement(&statement).unwrap();
        identity
    }

    fn setup() -> (
        RelayList,
        Arc<InProcessRelayNetwork>,
        DeviceIdentity,
        DeviceIdentity,
        Keys,
    ) {
        let relays = RelayList::configured(vec![
            "ws://127.0.0.1:17001".into(),
            "ws://127.0.0.1:17002".into(),
        ])
        .unwrap();
        let network = Arc::new(InProcessRelayNetwork::new(&relays));
        let owner = Keys::generate();
        let first_root = tempfile::tempdir().unwrap().keep();
        let second_root = tempfile::tempdir().unwrap().keep();
        let first = owned_identity(&first_root, &owner, 10);
        let second = owned_identity(&second_root, &owner, 10);
        (relays, network, first, second, owner)
    }

    #[derive(Clone)]
    enum ProbeResponse {
        Events(Vec<String>),
        Failed,
    }

    #[derive(Clone, Default)]
    struct ProbeTransport {
        responses: Arc<Mutex<BTreeMap<String, ProbeResponse>>>,
        filters: Arc<Mutex<Vec<(String, RelayFilter)>>>,
    }

    impl ProbeTransport {
        fn respond(&self, relay: &str, response: ProbeResponse) {
            self.responses
                .lock()
                .unwrap()
                .insert(relay.into(), response);
        }

        fn filters(&self) -> Vec<(String, RelayFilter)> {
            self.filters.lock().unwrap().clone()
        }
    }

    impl RelayTransport for ProbeTransport {
        fn publish<'a>(
            &'a self,
            _relay: &'a str,
            _event_json: &'a str,
            _auth: Arc<dyn RelayAuthSigner>,
        ) -> BoxFuture<'a, Result<(), RelayError>> {
            Box::pin(async { Ok(()) })
        }

        fn read<'a>(
            &'a self,
            relay: &'a str,
            filter: &'a RelayFilter,
            _auth: Arc<dyn RelayAuthSigner>,
        ) -> BoxFuture<'a, Result<Vec<RelayEvent>, RelayError>> {
            Box::pin(async move {
                self.filters
                    .lock()
                    .unwrap()
                    .push((relay.into(), filter.clone()));
                match self.responses.lock().unwrap().get(relay).cloned() {
                    Some(ProbeResponse::Events(events)) => Ok(events
                        .into_iter()
                        .map(|event_json| RelayEvent {
                            relay: relay.into(),
                            event_json,
                        })
                        .collect()),
                    Some(ProbeResponse::Failed) | None => {
                        Err(RelayError::Unavailable(relay.into()))
                    }
                }
            })
        }
    }

    fn probe(
        relays: &RelayList,
        identity: DeviceIdentity,
        transport: ProbeTransport,
    ) -> CoordinatedRelays<ProbeTransport> {
        CoordinatedRelays::new(relays.clone(), identity, Arc::new(transport)).unwrap()
    }

    fn endpoint(
        identity: &DeviceIdentity,
        owner: &Keys,
        generation: u64,
        now: u64,
    ) -> EndpointRecord {
        EndpointRecord {
            device_public_key: identity.device_public_key().unwrap().into(),
            owner_public_key: owner.public_key().to_hex(),
            generation,
            endpoint_id: iroh::SecretKey::generate().public().to_string(),
            issued_at: now,
        }
    }

    fn roster_statement(owner: &Keys, device: &str, status: &str, at: u64) -> String {
        EventBuilder::new(Kind::Custom(30_078), "")
            .tags([
                Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
                Tag::parse(["device", device]).unwrap(),
                Tag::parse(["status", status]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(at))
            .finalize(owner)
            .unwrap()
            .as_json()
    }

    #[tokio::test]
    async fn publication_probe_finds_the_exact_current_owner_statement_with_a_bounded_filter() {
        let (relays, _, identity, _, owner) = setup();
        let current = identity.owner_statement_json().unwrap();
        let transport = ProbeTransport::default();
        transport.respond(
            &relays.as_slice()[0],
            ProbeResponse::Events(vec![current.clone()]),
        );
        transport.respond(&relays.as_slice()[1], ProbeResponse::Events(Vec::new()));
        let reader = probe(&relays, identity, transport.clone());

        assert!(reader
            .owner_statement_was_published(&current)
            .await
            .unwrap());
        let filters = transport.filters();
        assert_eq!(filters.len(), 2);
        assert!(filters.iter().all(|(_, filter)| {
            filter.kinds == [OWNER_EVENT_KIND]
                && filter.recipient_public_key.is_none()
                && filter.author_public_key.as_deref() == Some(&owner.public_key().to_hex())
                && filter.since == Some(10)
                && filter.limit == MAX_READ_EVENTS
        }));
    }

    #[tokio::test]
    async fn publication_probe_distinguishes_successful_absence_and_total_outage() {
        let (relays, _, identity, _, _) = setup();
        let current = identity.owner_statement_json().unwrap();
        let transport = ProbeTransport::default();
        transport.respond(&relays.as_slice()[0], ProbeResponse::Events(Vec::new()));
        transport.respond(&relays.as_slice()[1], ProbeResponse::Events(Vec::new()));
        let reader = probe(&relays, identity.clone(), transport.clone());
        assert!(!reader
            .owner_statement_was_published(&current)
            .await
            .unwrap());

        transport.respond(&relays.as_slice()[0], ProbeResponse::Failed);
        transport.respond(&relays.as_slice()[1], ProbeResponse::Failed);
        let reader = probe(&relays, identity, transport);
        assert!(matches!(
            reader.owner_statement_was_published(&current).await,
            Err(RelayError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn publication_probe_uses_successful_partial_observations() {
        let (relays, _, identity, _, _) = setup();
        let current = identity.owner_statement_json().unwrap();
        let present = ProbeTransport::default();
        present.respond(&relays.as_slice()[0], ProbeResponse::Failed);
        present.respond(
            &relays.as_slice()[1],
            ProbeResponse::Events(vec![current.clone()]),
        );
        assert!(probe(&relays, identity.clone(), present)
            .owner_statement_was_published(&current)
            .await
            .unwrap());

        let absent = ProbeTransport::default();
        absent.respond(&relays.as_slice()[0], ProbeResponse::Failed);
        absent.respond(&relays.as_slice()[1], ProbeResponse::Events(Vec::new()));
        assert!(!probe(&relays, identity, absent)
            .owner_statement_was_published(&current)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn publication_probe_refuses_non_current_or_invalid_input_without_reading_relays() {
        let (relays, _, identity, _, owner) = setup();
        let device = identity.device_public_key().unwrap();
        let different_device = Keys::generate().public_key().to_hex();
        let different_owner = Keys::generate();
        let invalid = [
            "not-json".into(),
            roster_statement(&owner, device, "owned", 9),
            roster_statement(&owner, device, "revoked", 20),
            roster_statement(&owner, &different_device, "owned", 10),
            roster_statement(&different_owner, device, "owned", 10),
        ];
        let transport = ProbeTransport::default();
        let reader = probe(&relays, identity, transport.clone());

        for statement in invalid {
            assert!(matches!(
                reader.owner_statement_was_published(&statement).await,
                Err(RelayError::InvalidEvent(_))
            ));
        }
        assert!(transport.filters().is_empty());
    }

    #[tokio::test]
    async fn publication_probe_ignores_forged_and_unrelated_relay_events() {
        let (relays, _, identity, second, owner) = setup();
        let current = identity.owner_statement_json().unwrap();
        let mut forged: serde_json::Value = serde_json::from_str(&current).unwrap();
        forged["content"] = "forged".into();
        let unrelated_device = second.owner_statement_json().unwrap();
        let unrelated_owner = roster_statement(
            &Keys::generate(),
            identity.device_public_key().unwrap(),
            "owned",
            20,
        );
        let revoked =
            roster_statement(&owner, identity.device_public_key().unwrap(), "revoked", 20);
        let transport = ProbeTransport::default();
        transport.respond(
            &relays.as_slice()[0],
            ProbeResponse::Events(vec![
                serde_json::to_string(&forged).unwrap(),
                unrelated_device,
                unrelated_owner,
                revoked,
            ]),
        );
        transport.respond(&relays.as_slice()[1], ProbeResponse::Events(Vec::new()));

        assert!(!probe(&relays, identity, transport)
            .owner_statement_was_published(&current)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn roster_keeps_latest_evidence_and_terminal_revocation_across_relays() {
        let (relays, network, first, second, owner) = setup();
        let device = second.device_public_key().unwrap();
        let reader =
            CoordinatedRelays::new(relays.clone(), first.clone(), network.clone()).unwrap();
        let owned = second.owner_statement_json().unwrap();
        network.inject(&relays.as_slice()[0], &owned).unwrap();
        network.inject(&relays.as_slice()[1], &owned).unwrap();
        network
            .inject(
                &relays.as_slice()[0],
                &first.owner_statement_json().unwrap(),
            )
            .unwrap();
        let foreign = roster_statement(&Keys::generate(), device, "revoked", 500);
        network.inject(&relays.as_slice()[0], &foreign).unwrap();
        let initial = reader.read_owner_roster().await.unwrap();
        assert_eq!(initial.len(), 1);
        assert!(initial[0].is_owned());
        assert_eq!(initial[0].latest.event_json, owned);
        let revoked = roster_statement(&owner, device, "revoked", 20);
        network.inject(&relays.as_slice()[1], &revoked).unwrap();
        // A stale replica still has the old owned statement.
        let roster = reader.read_owner_roster().await.unwrap();
        assert_eq!(roster.len(), 1);
        assert!(!roster[0].is_owned());
        assert_eq!(roster[0].latest.event_json, revoked);
        assert_eq!(roster[0].revocation.as_ref().unwrap().event_json, revoked);
        // Newer owned evidence cannot erase an observed terminal revocation.
        let newer_owned = roster_statement(&owner, device, "owned", 30);
        network.inject(&relays.as_slice()[0], &newer_owned).unwrap();
        let roster = reader.read_owner_roster().await.unwrap();
        assert_eq!(roster[0].latest.event_json, newer_owned);
        assert!(!roster[0].is_owned());
        assert_eq!(roster[0].revocation.as_ref().unwrap().event_json, revoked);
    }

    #[tokio::test]
    async fn roster_uses_lower_event_id_for_equal_timestamp_and_preserves_revocation() {
        let (relays, network, first, second, owner) = setup();
        let owned = roster_statement(&owner, second.device_public_key().unwrap(), "owned", 20);
        let revoked = roster_statement(&owner, second.device_public_key().unwrap(), "revoked", 20);
        let owned_id = DeviceIdentity::verify_event(&owned).unwrap().id;
        let revoked_id = DeviceIdentity::verify_event(&revoked).unwrap().id;
        let expected = if owned_id < revoked_id {
            &owned
        } else {
            &revoked
        };
        network.inject(&relays.as_slice()[0], &owned).unwrap();
        network.inject(&relays.as_slice()[1], &revoked).unwrap();
        let reader = CoordinatedRelays::new(relays.clone(), first, network.clone()).unwrap();
        let roster = reader.read_owner_roster().await.unwrap();
        assert_eq!(&roster[0].latest.event_json, expected);
        assert!(!roster[0].is_owned());
        assert_eq!(roster[0].revocation.as_ref().unwrap().event_json, revoked);
        // The in-process relay follows the same replacement rule in either order.
        for relay in relays.as_slice() {
            network.inject(relay, &owned).unwrap();
            network.inject(relay, &revoked).unwrap();
        }
        assert_eq!(
            &reader.read_owner_roster().await.unwrap()[0]
                .latest
                .event_json,
            expected
        );
    }

    #[tokio::test]
    async fn roster_deduplicates_before_cap_and_retains_late_revocation_evidence() {
        let (relays, network, first, _, owner) = setup();
        let mut devices: Vec<_> = (0..MAX_READ_EVENTS)
            .map(|_| Keys::generate().public_key().to_hex())
            .collect();
        devices.sort();
        for device in &devices {
            let owned = roster_statement(&owner, device, "owned", 100);
            for relay in relays.as_slice() {
                network.inject(relay, &owned).unwrap();
            }
        }
        let revoked = roster_statement(&owner, &devices[0], "revoked", 200);
        network.inject(&relays.as_slice()[1], &revoked).unwrap();
        let reader = CoordinatedRelays::new(relays.clone(), first, network.clone()).unwrap();
        let roster = reader.read_owner_roster().await.unwrap();
        assert_eq!(roster.len(), MAX_READ_EVENTS);
        assert_eq!(roster[0].latest.statement.device_public_key, devices[0]);
        assert_eq!(roster[0].revocation.as_ref().unwrap().event_json, revoked);
        assert_eq!(
            roster.iter().filter(|entry| entry.is_owned()).count(),
            MAX_READ_EVENTS - 1
        );
        // More distinct devices across bounded relays cannot grow the global roster.
        for _ in 0..20 {
            let device = Keys::generate().public_key().to_hex();
            network
                .inject(
                    &relays.as_slice()[1],
                    &roster_statement(&owner, &device, "owned", 300),
                )
                .unwrap();
        }
        let roster = reader.read_owner_roster().await.unwrap();
        assert_eq!(roster.len(), MAX_READ_EVENTS);
        assert!(roster
            .windows(2)
            .all(|pair| pair[0].latest.statement.device_public_key
                < pair[1].latest.statement.device_public_key));
    }

    #[tokio::test]
    async fn roster_empty_success_partial_outage_and_total_outage_are_distinct() {
        let (relays, network, first, second, _) = setup();
        let reader = CoordinatedRelays::new(relays.clone(), first, network.clone()).unwrap();
        let publisher =
            CoordinatedRelays::new(relays.clone(), second.clone(), network.clone()).unwrap();
        assert!(reader.read_owner_roster().await.unwrap().is_empty());
        network.set_available(&relays.as_slice()[1], false);
        assert!(reader.read_owner_roster().await.unwrap().is_empty());
        assert!(matches!(
            publisher.publish_owner_statement().await.unwrap(),
            PublishState::Partial { .. }
        ));
        assert_eq!(
            reader.read_owner_roster().await.unwrap()[0]
                .latest
                .event_json,
            second.owner_statement_json().unwrap()
        );
        network.set_available(&relays.as_slice()[0], false);
        assert!(matches!(
            reader.read_owner_roster().await,
            Err(RelayError::Unavailable(_))
        ));
        assert!(matches!(
            publisher.publish_owner_statement().await.unwrap(),
            PublishState::Failed { .. }
        ));
        // Roster-specific outage reporting must not change signer or endpoint reads.
        assert!(reader
            .read_signer_packets(
                second.device_public_key(),
                second.device_public_key().unwrap(),
                0
            )
            .await
            .unwrap()
            .is_empty());
        assert!(reader.receive(100).await.unwrap().endpoints.is_empty());
        network.set_available(&relays.as_slice()[0], true);
        assert!(reader.read_owner_roster().await.unwrap()[0].is_owned());
    }

    #[tokio::test]
    async fn public_roster_filters_omit_recipient_but_private_filters_retain_it() {
        let (relays, network, first, second, owner) = setup();
        let mut filter = RelayFilter {
            kinds: vec![30_078],
            recipient_public_key: None,
            author_public_key: Some(owner.public_key().to_hex()),
            since: None,
            limit: MAX_READ_EVENTS,
            identifiers: Vec::new(),
        };
        let public: serde_json::Value =
            serde_json::from_str(&relay_request("roster", &filter)).unwrap();
        assert!(public[2].get("#p").is_none());
        assert_eq!(
            public[2]["authors"],
            serde_json::json!([owner.public_key().to_hex()])
        );
        network
            .inject(
                &relays.as_slice()[0],
                &second.owner_statement_json().unwrap(),
            )
            .unwrap();
        network
            .inject(
                &relays.as_slice()[0],
                &roster_statement(
                    &Keys::generate(),
                    second.device_public_key().unwrap(),
                    "owned",
                    30,
                ),
            )
            .unwrap();
        let auth: Arc<dyn RelayAuthSigner> = Arc::new(DeviceAuthSigner(Arc::new(first)));
        assert_eq!(
            network
                .read(&relays.as_slice()[0], &filter, auth.clone())
                .await
                .unwrap()
                .len(),
            1
        );
        filter.recipient_public_key = Some(second.device_public_key().unwrap().into());
        let private: serde_json::Value =
            serde_json::from_str(&relay_request("private", &filter)).unwrap();
        assert_eq!(
            private[2]["#p"],
            serde_json::json!([second.device_public_key().unwrap()])
        );
        assert!(network
            .read(&relays.as_slice()[0], &filter, auth)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn websocket_owner_publication_retries_auth_required_after_device_auth_ack() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_url = format!("ws://{}", listener.local_addr().unwrap());
        let owner = Keys::generate();
        let root = tempfile::tempdir().unwrap();
        let identity = owned_identity(root.path(), &owner, 10);
        let expected: serde_json::Value =
            serde_json::from_str(&identity.owner_statement_json().unwrap()).unwrap();
        let device = identity.device_public_key().unwrap().to_owned();
        let auth_relay = relay_url.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let first = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let first: serde_json::Value = serde_json::from_str(&first).unwrap();
            assert_eq!(first, serde_json::json!(["EVENT", expected]));
            let event = DeviceIdentity::verify_event(&first[1].to_string()).unwrap();
            assert_eq!(event.author, owner.public_key().to_hex());
            assert_ne!(event.author, device);
            socket
                .send(Message::Text(
                    serde_json::json!(["AUTH", "delegated-challenge"])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            socket
                .send(Message::Text(
                    serde_json::json!(["OK", event.id, false, "auth-required: authenticate first"])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            let auth = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let auth: serde_json::Value = serde_json::from_str(&auth).unwrap();
            assert_eq!(auth[0], "AUTH");
            let verified = DeviceIdentity::verify_event(&auth[1].to_string()).unwrap();
            assert_eq!(verified.author, device);
            assert_eq!(verified.kind, 22_242);
            assert_eq!(
                verified.tags,
                vec![
                    vec!["relay".to_owned(), auth_relay],
                    vec!["challenge".to_owned(), "delegated-challenge".to_owned()]
                ]
            );
            assert!(timeout(Duration::from_millis(25), socket.next())
                .await
                .is_err());
            socket
                .send(Message::Text(
                    serde_json::json!(["OK", verified.id, true, ""])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            let repeated = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&repeated).unwrap(),
                first
            );
            socket
                .send(Message::Text(
                    serde_json::json!(["OK", event.id, true, ""])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        });
        let publisher = CoordinatedRelays::new(
            RelayList::configured(vec![relay_url]).unwrap(),
            identity,
            Arc::new(WebSocketRelayTransport::new()),
        )
        .unwrap();
        assert!(matches!(
            publisher.publish_owner_statement().await.unwrap(),
            PublishState::Published { .. }
        ));
        server.await.unwrap();
    }

    type RelaySocket = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

    async fn send_relay(socket: &mut RelaySocket, value: serde_json::Value) {
        socket
            .send(Message::Text(value.to_string().into()))
            .await
            .unwrap();
    }

    async fn receive_relay(socket: &mut RelaySocket) -> serde_json::Value {
        let text = socket.next().await.unwrap().unwrap().into_text().unwrap();
        serde_json::from_str(&text).unwrap()
    }

    async fn challenge_publication(socket: &mut RelaySocket) -> String {
        send_relay(socket, serde_json::json!(["AUTH", "publish-challenge"])).await;
        let auth = receive_relay(socket).await;
        assert_eq!(auth[0], "AUTH");
        let verified = DeviceIdentity::verify_event(&auth[1].to_string()).unwrap();
        assert_eq!(verified.kind, 22_242);
        assert!(has_tag(&verified.tags, "challenge", "publish-challenge"));
        verified.id
    }

    async fn publication_connection_closed(socket: &mut RelaySocket) {
        let next = timeout(OPERATION_TIMEOUT + Duration::from_secs(1), socket.next())
            .await
            .expect("publisher must close within its operation deadline");
        assert!(!matches!(next, Some(Ok(Message::Text(_)))));
    }

    async fn websocket_publication_exchange<F, Fut>(exchange: F) -> Result<(), RelayError>
    where
        F: FnOnce(RelaySocket, serde_json::Value) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_url = format!("ws://{}", listener.local_addr().unwrap());
        let root = tempfile::tempdir().unwrap();
        let identity = owned_identity(root.path(), &Keys::generate(), 10);
        let event = identity.owner_statement_json().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let first = receive_relay(&mut socket).await;
            assert_eq!(first[0], "EVENT");
            DeviceIdentity::verify_event(&first[1].to_string()).unwrap();
            exchange(socket, first).await;
        });
        let result = timeout(
            OPERATION_TIMEOUT + Duration::from_secs(2),
            WebSocketRelayTransport::new().publish(
                &relay_url,
                &event,
                Arc::new(DeviceAuthSigner(Arc::new(identity))),
            ),
        )
        .await
        .expect("authentication must not reset the publication deadline");
        server.await.unwrap();
        result
    }

    #[tokio::test]
    async fn websocket_http_429_does_not_retry_and_shares_backoff_with_reads() {
        use tokio::io::AsyncWriteExt;
        let date = chrono::DateTime::from_timestamp((unix_now() + 600) as i64, 0)
            .unwrap()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        for response in [
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 600\r\nContent-Length: 0\r\n\r\n"
                .to_owned(),
            format!("HTTP/1.1 403 Forbidden\r\nRetry-After: {date}\r\nContent-Length: 0\r\n\r\n"),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("ws://{}", listener.local_addr().unwrap());
            let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let connections = count.clone();
            let server = tokio::spawn(async move {
                loop {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    connections.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let response = response.clone();
                    tokio::spawn(async move {
                        use tokio::io::AsyncReadExt;
                        let mut request = [0; 4096];
                        let _ = stream.read(&mut request).await;
                        stream.write_all(response.as_bytes()).await.unwrap();
                    });
                }
            });
            let root = tempfile::tempdir().unwrap();
            let identity = owned_identity(root.path(), &Keys::generate(), 10);
            let auth: Arc<dyn RelayAuthSigner> =
                Arc::new(DeviceAuthSigner(Arc::new(identity.clone())));
            let transport = WebSocketRelayTransport::new();
            let event = identity.owner_statement_json().unwrap();
            assert!(transport.publish(&url, &event, auth.clone()).await.is_err());
            let filter = CoordinationDecoder::new(&identity, unix_now())
                .unwrap()
                .filter();
            assert!(transport.read(&url, &filter, auth).await.is_err());
            assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(301)).await;
            let auth: Arc<dyn RelayAuthSigner> = Arc::new(DeviceAuthSigner(Arc::new(identity)));
            assert!(transport.read(&url, &filter, auth.clone()).await.is_err());
            assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
            tokio::time::advance(Duration::from_secs(300)).await;
            tokio::time::resume();
            assert!(transport.read(&url, &filter, auth).await.is_err());
            assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
            server.abort();
        }
    }

    async fn counted_publication_relay(
        reason: Option<&'static str>,
    ) -> (
        String,
        Arc<std::sync::atomic::AtomicUsize>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let connections = count.clone();
        let server = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                connections.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut socket = accept_async(stream).await.unwrap();
                    let event = receive_relay(&mut socket).await;
                    send_relay(
                        &mut socket,
                        serde_json::json!([
                            "OK",
                            event[1]["id"],
                            reason.is_none(),
                            reason.unwrap_or("")
                        ]),
                    )
                    .await;
                });
            }
        });
        (url, count, server)
    }

    #[tokio::test]
    async fn websocket_ok_false_backs_off_only_the_refusing_relay() {
        use std::sync::atomic::Ordering;
        for reason in [
            "blocked: private response",
            "Policy violated and private response",
        ] {
            let (refusing, count, server) = counted_publication_relay(Some(reason)).await;
            let (accepting, other_count, other_server) = counted_publication_relay(None).await;
            let root = tempfile::tempdir().unwrap();
            let identity = owned_identity(root.path(), &Keys::generate(), 10);
            let event = identity.owner_statement_json().unwrap();
            let filter = CoordinationDecoder::new(&identity, unix_now())
                .unwrap()
                .filter();
            let auth: Arc<dyn RelayAuthSigner> = Arc::new(DeviceAuthSigner(Arc::new(identity)));
            let transport = WebSocketRelayTransport::new();
            assert!(transport
                .publish(&refusing, &event, auth.clone())
                .await
                .is_err());
            assert!(transport
                .publish(&refusing, &event, auth.clone())
                .await
                .is_err());
            assert!(transport
                .read(&refusing, &filter, auth.clone())
                .await
                .is_err());
            assert!(transport
                .publish(&accepting, &event, auth.clone())
                .await
                .is_ok());
            assert_eq!(count.load(Ordering::SeqCst), 1);
            assert_eq!(other_count.load(Ordering::SeqCst), 1);
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(301)).await;
            tokio::time::resume();
            assert!(transport.publish(&refusing, &event, auth).await.is_err());
            assert_eq!(count.load(Ordering::SeqCst), 2);
            server.abort();
            other_server.abort();
        }
    }

    #[tokio::test]
    async fn websocket_coordination_minute_reuses_one_connection_and_subscription_per_relay() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = tempfile::tempdir().unwrap();
        let owner = Keys::generate();
        let identity = owned_identity(root.path(), &owner, 10);
        let now = unix_now();
        let key = identity.device_public_key().unwrap();
        let event = identity
            .encrypt_tagged_event(
                key,
                ENDPOINT_EVENT_KIND,
                vec![vec!["p".into(), key.into()]],
                "encrypted endpoint fixture",
                now,
            )
            .unwrap()
            .json;
        let expected: serde_json::Value = serde_json::from_str(&event).unwrap();
        let mut urls = Vec::new();
        let mut servers = Vec::new();
        let mut counts = Vec::new();
        for _ in 0..2 {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            urls.push(format!("ws://{}", listener.local_addr().unwrap()));
            let connections = Arc::new(AtomicUsize::new(0));
            let subscriptions = Arc::new(AtomicUsize::new(0));
            let publications = Arc::new(AtomicUsize::new(0));
            counts.push((
                connections.clone(),
                subscriptions.clone(),
                publications.clone(),
            ));
            let expected = expected.clone();
            let recipient = key.to_owned();
            servers.push(tokio::spawn(async move {
                loop {
                    let (stream, _) = listener.accept().await.unwrap();
                    connections.fetch_add(1, Ordering::SeqCst);
                    let (subscriptions, publications, expected, recipient) = (
                        subscriptions.clone(),
                        publications.clone(),
                        expected.clone(),
                        recipient.clone(),
                    );
                    tokio::spawn(async move {
                        let mut socket = accept_async(stream).await.unwrap();
                        let mut subscription = None;
                        while let Some(Ok(Message::Text(text))) = socket.next().await {
                            let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
                            match frame[0].as_str() {
                                Some("REQ") => {
                                    if frame[2]["kinds"] == serde_json::json!([30078, 1059]) {
                                        assert!(frame[2]["since"].is_u64());
                                        assert_eq!(frame[2]["#p"], serde_json::json!([recipient]));
                                        subscriptions.fetch_add(1, Ordering::SeqCst);
                                        subscription = Some(frame[1].clone());
                                    } else {
                                        assert_eq!(frame[2]["kinds"], serde_json::json!([30078]));
                                    }
                                    send_relay(
                                        &mut socket,
                                        serde_json::json!(["EVENT", frame[1], expected]),
                                    )
                                    .await;
                                    send_relay(&mut socket, serde_json::json!(["EOSE", frame[1]]))
                                        .await;
                                }
                                Some("EVENT") => {
                                    publications.fetch_add(1, Ordering::SeqCst);
                                    // Push while the caller waits for its publication ACK.
                                    send_relay(
                                        &mut socket,
                                        serde_json::json!(["EVENT", subscription, expected]),
                                    )
                                    .await;
                                    send_relay(
                                        &mut socket,
                                        serde_json::json!(["OK", frame[1]["id"], true, ""]),
                                    )
                                    .await;
                                }
                                Some("CLOSE") if subscription.as_ref() == Some(&frame[1]) => {
                                    subscription = None
                                }
                                _ => {}
                            }
                        }
                    });
                }
            }));
        }
        let transport = WebSocketRelayTransport::new();
        let auth: Arc<dyn RelayAuthSigner> = Arc::new(DeviceAuthSigner(Arc::new(identity.clone())));
        let mut filter = CoordinationDecoder::new(&identity, now).unwrap().filter();
        filter.since = Some(now - (7 * 24 * 60 * 60));
        for second in 0..60 {
            for url in &urls {
                let events = transport
                    .read_coordination(url, &filter, auth.clone())
                    .await
                    .unwrap();
                if second == 0 {
                    assert_eq!(events.len(), 1);
                    assert_eq!(events[0].event_json, event);
                }
                if second % 5 == 1 {
                    assert!(
                        events.iter().any(|received| received.event_json == event),
                        "publication discarded a pushed coordination event"
                    );
                }
                if second == 0 || second == 30 {
                    let mut snapshot = filter.clone();
                    snapshot.kinds = vec![ENDPOINT_EVENT_KIND];
                    snapshot.recipient_public_key = None;
                    snapshot.author_public_key = Some(key.into());
                    snapshot.since = None;
                    assert_eq!(
                        transport
                            .read(url, &snapshot, auth.clone())
                            .await
                            .unwrap()
                            .len(),
                        1
                    );
                }
                if second % 5 == 0 || second == 59 {
                    transport.publish(url, &event, auth.clone()).await.unwrap();
                }
            }
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(1)).await;
            tokio::time::resume();
        }
        for server in servers {
            server.abort();
        }
        for (connections, subscriptions, publications) in counts {
            assert_eq!(connections.load(Ordering::SeqCst), 1);
            assert_eq!(subscriptions.load(Ordering::SeqCst), 1);
            assert_eq!(publications.load(Ordering::SeqCst), 13);
        }
    }

    #[tokio::test]
    async fn websocket_cancelled_subscription_keeps_buffered_events_and_reconnects_with_since() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let root = tempfile::tempdir().unwrap();
        let identity = owned_identity(root.path(), &Keys::generate(), 10);
        let key = identity.device_public_key().unwrap();
        let event = identity
            .encrypt_tagged_event(
                key,
                ENDPOINT_EVENT_KIND,
                vec![vec!["p".into(), key.into()]],
                "buffered ciphertext",
                unix_now(),
            )
            .unwrap()
            .json;
        let expected = event.clone();
        let mut filter = CoordinationDecoder::new(&identity, unix_now())
            .unwrap()
            .filter();
        filter.since = Some(unix_now() - (7 * 24 * 60 * 60));
        let since = filter.since;
        let connections = Arc::new(AtomicUsize::new(0));
        let count = connections.clone();
        let (received_event, first_event) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            count.fetch_add(1, Ordering::SeqCst);
            let mut socket = accept_async(stream).await.unwrap();
            let request = receive_relay(&mut socket).await;
            assert_eq!(request[2]["since"], serde_json::json!(since.unwrap()));
            send_relay(
                &mut socket,
                serde_json::json!([
                    "EVENT",
                    request[1],
                    serde_json::from_str::<serde_json::Value>(&event).unwrap()
                ]),
            )
            .await;
            received_event.send(()).unwrap();
            // No EOSE: the caller cancels this pass after buffering the event.
            let _ = socket.next().await;
            let (stream, _) = listener.accept().await.unwrap();
            count.fetch_add(1, Ordering::SeqCst);
            let mut socket = accept_async(stream).await.unwrap();
            let request = receive_relay(&mut socket).await;
            assert_eq!(request[2]["since"], serde_json::json!(since.unwrap()));
            send_relay(&mut socket, serde_json::json!(["EOSE", request[1]])).await;
            let _ = socket.next().await;
        });
        let auth: Arc<dyn RelayAuthSigner> = Arc::new(DeviceAuthSigner(Arc::new(identity)));
        let transport = WebSocketRelayTransport::new();
        {
            let read = transport.read_coordination(&url, &filter, auth.clone());
            tokio::pin!(read);
            tokio::select! {
                result = &mut read => panic!("subscription completed without EOSE: {result:?}"),
                _ = first_event => {}
            }
            assert!(timeout(Duration::from_millis(100), read).await.is_err());
        }
        assert!(transport
            .read_coordination(&url, &filter, auth.clone())
            .await
            .is_err());
        assert_eq!(connections.load(Ordering::SeqCst), 1);
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::time::resume();
        let events = transport
            .read_coordination(&url, &filter, auth)
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_json, expected);
        assert_eq!(connections.load(Ordering::SeqCst), 2);
        drop(transport);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn websocket_idle_snapshot_connection_answers_relay_ping() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let request = receive_relay(&mut socket).await;
            assert_eq!(request[0], "REQ");
            send_relay(&mut socket, serde_json::json!(["EOSE", request[1]])).await;
            assert_eq!(receive_relay(&mut socket).await[0], "CLOSE");
            socket
                .send(Message::Ping(b"alive".to_vec().into()))
                .await
                .unwrap();
            let pong = timeout(Duration::from_secs(3), socket.next())
                .await
                .expect("an idle relay connection must keep answering PING")
                .unwrap()
                .unwrap();
            assert_eq!(pong, Message::Pong(b"alive".to_vec().into()));
        });
        let root = tempfile::tempdir().unwrap();
        let identity = owned_identity(root.path(), &Keys::generate(), 10);
        let filter = CoordinationDecoder::new(&identity, unix_now())
            .unwrap()
            .filter();
        let transport = WebSocketRelayTransport::new();
        assert!(transport
            .read(
                &url,
                &filter,
                Arc::new(DeviceAuthSigner(Arc::new(identity)))
            )
            .await
            .unwrap()
            .is_empty());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn websocket_publication_accepts_initial_event_without_auth_or_retry() {
        let result = websocket_publication_exchange(|mut socket, first| async move {
            send_relay(
                &mut socket,
                serde_json::json!(["OK", first[1]["id"], true, ""]),
            )
            .await;
            publication_connection_closed(&mut socket).await;
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn websocket_publication_ignores_unrelated_ok_while_waiting_for_auth() {
        let result = websocket_publication_exchange(|mut socket, first| async move {
            let auth_id = challenge_publication(&mut socket).await;
            send_relay(
                &mut socket,
                serde_json::json!(["OK", first[1]["id"], false, "auth-required: login"]),
            )
            .await;
            for accepted in [false, true] {
                send_relay(
                    &mut socket,
                    serde_json::json!(["OK", "00".repeat(32), accepted, "unrelated event"]),
                )
                .await;
                assert!(timeout(Duration::from_millis(25), socket.next())
                    .await
                    .is_err());
            }
            send_relay(&mut socket, serde_json::json!(["OK", auth_id, true, ""])).await;
            assert_eq!(receive_relay(&mut socket).await, first);
            send_relay(
                &mut socket,
                serde_json::json!(["OK", first[1]["id"], true, ""]),
            )
            .await;
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn websocket_publication_rejects_auth_denial_without_retry() {
        let result = websocket_publication_exchange(|mut socket, first| async move {
            let auth_id = challenge_publication(&mut socket).await;
            send_relay(
                &mut socket,
                serde_json::json!(["OK", first[1]["id"], false, "auth-required: login"]),
            )
            .await;
            send_relay(
                &mut socket,
                serde_json::json!(["OK", auth_id, false, "restricted: device denied"]),
            )
            .await;
            publication_connection_closed(&mut socket).await;
        })
        .await;
        assert!(
            matches!(result, Err(RelayError::Protocol(reason)) if reason == "restricted: device denied")
        );
    }

    #[tokio::test]
    async fn websocket_publication_bounds_repeated_challenges_before_and_after_auth() {
        for after_ack in [false, true] {
            for challenge in ["publish-challenge", "different-challenge"] {
                let result = websocket_publication_exchange(move |mut socket, first| async move {
                    let auth_id = challenge_publication(&mut socket).await;
                    if after_ack {
                        send_relay(
                            &mut socket,
                            serde_json::json!([
                                "OK",
                                first[1]["id"],
                                false,
                                "auth-required: login"
                            ]),
                        )
                        .await;
                        send_relay(&mut socket, serde_json::json!(["OK", auth_id, true, ""])).await;
                        assert_eq!(receive_relay(&mut socket).await, first);
                    }
                    send_relay(&mut socket, serde_json::json!(["AUTH", challenge])).await;
                    publication_connection_closed(&mut socket).await;
                })
                .await;
                assert!(
                    matches!(result, Err(RelayError::Protocol(reason)) if reason == "AUTH challenge limit exceeded")
                );
            }
        }
    }

    #[tokio::test]
    async fn websocket_publication_rejects_event_policy_and_retry_failures() {
        for after_retry in [false, true] {
            for reason in [
                "blocked: owner publication denied",
                "restricted: author is not authenticated device",
                "invalid: event rejected",
                "rate-limited: slow down",
                "auth-required: login",
            ] {
                // Only the first auth-required rejection is recoverable.
                if !after_retry && reason.starts_with("auth-required:") {
                    continue;
                }
                let result = websocket_publication_exchange(move |mut socket, first| async move {
                    let auth_id = challenge_publication(&mut socket).await;
                    if after_retry {
                        send_relay(
                            &mut socket,
                            serde_json::json!([
                                "OK",
                                first[1]["id"],
                                false,
                                "auth-required: login"
                            ]),
                        )
                        .await;
                        send_relay(&mut socket, serde_json::json!(["OK", auth_id, true, ""])).await;
                        assert_eq!(receive_relay(&mut socket).await, first);
                    }
                    send_relay(
                        &mut socket,
                        serde_json::json!(["OK", first[1]["id"], false, reason]),
                    )
                    .await;
                    publication_connection_closed(&mut socket).await;
                })
                .await;
                assert!(matches!(result, Err(RelayError::Protocol(message)) if message == reason));
            }
        }
    }

    #[tokio::test]
    async fn websocket_publication_auth_ack_alone_does_not_retry_or_report_success() {
        let result = websocket_publication_exchange(|mut socket, _| async move {
            let auth_id = challenge_publication(&mut socket).await;
            send_relay(&mut socket, serde_json::json!(["OK", auth_id, true, ""])).await;
            assert!(timeout(Duration::from_millis(25), socket.next())
                .await
                .is_err());
            socket.close(None).await.unwrap();
        })
        .await;
        assert!(matches!(result, Err(RelayError::Unavailable(_))));
    }

    #[tokio::test]
    async fn websocket_publication_auth_retry_keeps_total_deadline() {
        let result = websocket_publication_exchange(|mut socket, first| async move {
            sleep(OPERATION_TIMEOUT / 2).await;
            let auth_id = challenge_publication(&mut socket).await;
            send_relay(
                &mut socket,
                serde_json::json!(["OK", first[1]["id"], false, "auth-required: login"]),
            )
            .await;
            send_relay(&mut socket, serde_json::json!(["OK", auth_id, true, ""])).await;
            assert_eq!(receive_relay(&mut socket).await, first);
            publication_connection_closed(&mut socket).await;
        })
        .await;
        assert!(matches!(result, Err(RelayError::Timeout)));
    }

    #[tokio::test]
    async fn websocket_publication_auth_keeps_message_and_aggregate_response_bounds() {
        for aggregate in [false, true] {
            let result = websocket_publication_exchange(move |mut socket, _| async move {
                let auth_id = challenge_publication(&mut socket).await;
                // Keep the EVENT pending while exercising bounds after AUTH succeeds.
                send_relay(&mut socket, serde_json::json!(["OK", auth_id, true, ""])).await;
                let size = if aggregate {
                    MAX_EVENT_BYTES / 2
                } else {
                    MAX_EVENT_BYTES
                };
                let count = if aggregate {
                    MAX_RESPONSE_BYTES / size
                } else {
                    1
                };
                for _ in 0..count {
                    send_relay(&mut socket, serde_json::json!(["NOTICE", "x".repeat(size)])).await;
                }
                publication_connection_closed(&mut socket).await;
            })
            .await;
            assert!(
                matches!(result, Err(RelayError::Protocol(reason)) if reason == "relay response exceeds bounds")
            );
        }
    }

    #[tokio::test]
    async fn websocket_signer_publication_waits_for_matching_auth_ack_and_initial_rejection() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_url = format!("ws://{}", listener.local_addr().unwrap());
        let root = tempfile::tempdir().unwrap();
        let identity = owned_identity(root.path(), &Keys::generate(), 10);
        let signer = Nip46ConnectionIdentity::load_or_create(root.path()).unwrap();
        let device = identity.device_public_key().unwrap().to_owned();
        let packet = signer
            .sign_encrypted_event(
                &Keys::generate().public_key().to_hex(),
                24_133,
                "signer request",
                unix_now(),
            )
            .unwrap();
        let expected: serde_json::Value = serde_json::from_str(&packet.json).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let first = receive_relay(&mut socket).await;
            assert_eq!(first, serde_json::json!(["EVENT", expected]));
            let event = DeviceIdentity::verify_event(&first[1].to_string()).unwrap();
            assert_eq!(event.kind, 24_133);
            assert_eq!(event.author, signer.public_key());
            assert_ne!(event.author, device);
            send_relay(&mut socket, serde_json::json!(["AUTH", "signer-challenge"])).await;
            let auth = receive_relay(&mut socket).await;
            assert_eq!(auth[0], "AUTH");
            let auth = DeviceIdentity::verify_event(&auth[1].to_string()).unwrap();
            assert_eq!(auth.author, device);
            assert_eq!(auth.kind, 22_242);
            assert!(has_tag(&auth.tags, "challenge", "signer-challenge"));
            send_relay(
                &mut socket,
                serde_json::json!(["OK", "00".repeat(32), true, ""]),
            )
            .await;
            assert!(timeout(Duration::from_millis(25), socket.next())
                .await
                .is_err());
            // AUTH acknowledgement may precede the initial EVENT rejection.
            send_relay(&mut socket, serde_json::json!(["OK", auth.id, true, ""])).await;
            assert!(timeout(Duration::from_millis(25), socket.next())
                .await
                .is_err());
            send_relay(
                &mut socket,
                serde_json::json!(["OK", event.id, false, "auth-required: login"]),
            )
            .await;
            assert_eq!(receive_relay(&mut socket).await, first);
            send_relay(&mut socket, serde_json::json!(["OK", event.id, true, ""])).await;
        });
        let publisher = CoordinatedRelays::new(
            RelayList::configured(vec![relay_url]).unwrap(),
            identity,
            Arc::new(WebSocketRelayTransport::new()),
        )
        .unwrap();
        assert!(matches!(
            publisher.publish_signer_packet(&packet.json).await,
            PublishState::Published { .. }
        ));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn websocket_roster_rechecks_owner_and_shape_even_when_relay_ignores_filter() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_url = format!("ws://{}", listener.local_addr().unwrap());
        let owner = Keys::generate();
        let root = tempfile::tempdir().unwrap();
        let identity = owned_identity(root.path(), &owner, 10);
        let device = Keys::generate().public_key().to_hex();
        let valid = roster_statement(&owner, &device, "owned", 100);
        let wrong_owner = roster_statement(&Keys::generate(), &device, "revoked", 200);
        let malformed = roster_statement(&owner, "not-a-key", "owned", 300);
        let mut tampered: serde_json::Value = serde_json::from_str(&valid).unwrap();
        tampered["sig"] = serde_json::json!("00".repeat(64));
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let request: serde_json::Value = serde_json::from_str(&request).unwrap();
            assert_eq!(request[0], "REQ");
            assert!(request[2].get("#p").is_none());
            assert_eq!(
                request[2]["authors"],
                serde_json::json!([owner.public_key().to_hex()])
            );
            assert_eq!(request[2]["kinds"], serde_json::json!([30_078]));
            assert_eq!(request[2]["limit"], MAX_READ_EVENTS);
            for json in [
                wrong_owner,
                malformed,
                tampered.to_string(),
                valid.clone(),
                valid,
            ] {
                let event: serde_json::Value = serde_json::from_str(&json).unwrap();
                socket
                    .send(Message::Text(
                        serde_json::json!(["EVENT", request[1], event])
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
            socket
                .send(Message::Text(
                    serde_json::json!(["EOSE", request[1]]).to_string().into(),
                ))
                .await
                .unwrap();
            let close = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&close).unwrap()[0],
                "CLOSE"
            );
        });
        let reader = CoordinatedRelays::new(
            RelayList::configured(vec![relay_url]).unwrap(),
            identity,
            Arc::new(WebSocketRelayTransport::new()),
        )
        .unwrap();
        let roster = reader.read_owner_roster().await.unwrap();
        assert_eq!(roster.len(), 1);
        assert_eq!(roster[0].latest.statement.device_public_key, device);
        assert!(roster[0].is_owned());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn publishes_to_two_relays_and_deduplicates_delivery_by_event_id() {
        let (relays, network, first, second, owner) = setup();
        let publisher =
            CoordinatedRelays::new(relays.clone(), first.clone(), network.clone()).unwrap();
        let reader = CoordinatedRelays::new(relays, second.clone(), network).unwrap();
        let state = publisher
            .publish_endpoint(
                second.device_public_key().unwrap(),
                endpoint(&first, &owner, 1, 100),
                100,
            )
            .await
            .unwrap();
        assert!(
            matches!(state, PublishState::Published { accepted_relays } if accepted_relays.len() == 2)
        );
        let snapshot = reader.receive(101).await.unwrap();
        assert_eq!(snapshot.endpoints.len(), 1);
        assert_eq!(snapshot.rejected_events, 0);
    }

    #[tokio::test]
    async fn production_websocket_handles_nip42_before_accepting_a_publish() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_url = format!("ws://{}", listener.local_addr().unwrap());
        let relay_server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let first = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let first: serde_json::Value = serde_json::from_str(&first).unwrap();
            let event_id = first[1]["id"].as_str().unwrap().to_owned();
            let endpoint_event = DeviceIdentity::verify_event(&first[1].to_string()).unwrap();
            assert_eq!(endpoint_event.kind, ENDPOINT_EVENT_KIND);
            // This ordering also covers rejection before the challenge arrives.
            socket
                .send(Message::Text(
                    serde_json::json!(["OK", event_id, false, "auth-required: authenticate first"])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            socket
                .send(Message::Text(
                    serde_json::json!(["AUTH", "relay-challenge"])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            let auth = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let auth: serde_json::Value = serde_json::from_str(&auth).unwrap();
            let verified = DeviceIdentity::verify_event(&auth[1].to_string()).unwrap();
            assert_eq!(verified.kind, 22_242);
            assert!(has_tag(&verified.tags, "challenge", "relay-challenge"));
            assert_eq!(verified.author, endpoint_event.author);
            socket
                .send(Message::Text(
                    serde_json::json!(["OK", verified.id, true, ""])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            let repeated = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let repeated: serde_json::Value = serde_json::from_str(&repeated).unwrap();
            assert_eq!(repeated, first);
            socket
                .send(Message::Text(
                    serde_json::json!(["OK", event_id, true, ""])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        });

        let relays = RelayList::configured(vec![relay_url]).unwrap();
        let owner = Keys::generate();
        let first_root = tempfile::tempdir().unwrap();
        let second_root = tempfile::tempdir().unwrap();
        let first = owned_identity(first_root.path(), &owner, 10);
        let second = owned_identity(second_root.path(), &owner, 10);
        let publisher = CoordinatedRelays::new(
            relays,
            first.clone(),
            Arc::new(WebSocketRelayTransport::new()),
        )
        .unwrap();
        let state = publisher
            .publish_endpoint(
                second.device_public_key().unwrap(),
                endpoint(&first, &owner, 1, unix_now()),
                unix_now(),
            )
            .await
            .unwrap();
        assert!(matches!(state, PublishState::Published { .. }));
        relay_server.await.unwrap();
    }

    #[tokio::test]
    async fn one_relay_loss_is_structured_partial_success() {
        let (relays, network, first, second, owner) = setup();
        network.set_available(&relays.as_slice()[1], false);
        let publisher = CoordinatedRelays::new(relays, first.clone(), network).unwrap();
        let state = publisher
            .publish_endpoint(
                second.device_public_key().unwrap(),
                endpoint(&first, &owner, 1, 100),
                100,
            )
            .await
            .unwrap();
        assert!(
            matches!(state, PublishState::Partial { accepted_relays, failed_relays }
            if accepted_relays.len() == 1 && failed_relays.len() == 1)
        );
    }

    #[tokio::test]
    async fn newer_generation_replaces_current_endpoint() {
        let (relays, network, first, second, owner) = setup();
        let publisher =
            CoordinatedRelays::new(relays.clone(), first.clone(), network.clone()).unwrap();
        let reader = CoordinatedRelays::new(relays, second.clone(), network.clone()).unwrap();
        publisher
            .publish_endpoint(
                second.device_public_key().unwrap(),
                endpoint(&first, &owner, 1, 100),
                100,
            )
            .await
            .unwrap();
        let mut next = endpoint(&first, &owner, 2, 110);
        next.endpoint_id = iroh::SecretKey::generate().public().to_string();
        let expected = next.endpoint_id.clone();
        publisher
            .publish_endpoint(second.device_public_key().unwrap(), next, 110)
            .await
            .unwrap();
        let snapshot = reader.receive(111).await.unwrap();
        assert_eq!(snapshot.endpoints[0].record.generation, 2);
        assert_eq!(snapshot.endpoints[0].record.endpoint_id, expected);
        assert!(network.stored_event_count("ws://127.0.0.1:17001") <= 2);
    }

    #[tokio::test]
    async fn rejects_stale_wrong_recipient_and_malformed_events() {
        let (relays, network, first, second, owner) = setup();
        let publisher =
            CoordinatedRelays::new(relays.clone(), first.clone(), network.clone()).unwrap();
        let reader =
            CoordinatedRelays::new(relays.clone(), second.clone(), network.clone()).unwrap();
        let stale = endpoint(&first, &owner, 1, 100);
        publisher
            .publish_endpoint(second.device_public_key().unwrap(), stale, 100)
            .await
            .unwrap();
        let other_root = tempfile::tempdir().unwrap();
        let other = owned_identity(other_root.path(), &owner, 10);
        publisher
            .publish_endpoint(
                other.device_public_key().unwrap(),
                endpoint(&first, &owner, 2, 100),
                100,
            )
            .await
            .unwrap();
        assert!(network.inject(&relays.as_slice()[0], "{bad").is_err());
        let snapshot = reader.receive(701).await.unwrap();
        assert_eq!(snapshot.endpoints.len(), 1);
        assert!(snapshot.rejected_events >= 1);
    }

    #[test]
    fn metadata_and_relay_configuration_are_bounded() {
        assert!(RelayInformation::parse_bounded(&vec![b'x'; MAX_RESPONSE_BYTES + 1]).is_err());
        assert!(RelayList::configured(vec!["ws://example.com".into()]).is_err());
        assert!(RelayList::configured(vec!["wss://relay.example.com".into()]).is_ok());
        assert_eq!(
            RelayList::from_linux_environment(Some("[\"wss://relay.example.com\"]"))
                .unwrap()
                .unwrap()
                .as_slice()
                .len(),
            1
        );
        let snapshot = crate::config::decode_config_documents(
            "host:\n  relays:\n    - wss://relay.example.com\n",
            "{}\n",
        )
        .unwrap();
        assert_eq!(
            RelayList::from_device_settings(&snapshot)
                .unwrap()
                .unwrap()
                .as_slice(),
            &["wss://relay.example.com"]
        );
    }

    #[tokio::test]
    async fn queued_command_waits_until_a_running_korrid_reconnects() {
        let (relays, network, first, second, _) = setup();
        network.delay_next_delivery(&relays.as_slice()[0], 1);
        network.delay_next_delivery(&relays.as_slice()[1], 1);
        let publisher =
            CoordinatedRelays::new(relays.clone(), first.clone(), network.clone()).unwrap();
        let reader = CoordinatedRelays::new(relays, second.clone(), network).unwrap();
        let command = CoordinationCommand::RunIdleKorrid {
            sender_device_public_key: first.device_public_key().unwrap().into(),
            requested_at: 100,
            expires_at: 1_000,
        };
        publisher
            .queue_command(second.device_public_key().unwrap(), command, 100)
            .await
            .unwrap();
        assert!(reader.receive(101).await.unwrap().commands.is_empty());
        assert_eq!(reader.receive(102).await.unwrap().commands.len(), 1);
    }

    #[tokio::test]
    async fn remote_owner_binding_response_uses_the_same_private_queue() {
        let (relays, network, first, second, owner) = setup();
        let publisher =
            CoordinatedRelays::new(relays.clone(), first.clone(), network.clone()).unwrap();
        let reader = CoordinatedRelays::new(relays, second.clone(), network).unwrap();
        let template = second
            .owner_statement_template(OwnerStatementStatus::Owned, 120)
            .unwrap();
        let signed = EventBuilder::new(Kind::Custom(30_078), "")
            .tags([
                Tag::parse([
                    "d",
                    &format!(
                        "org.korri.device-owner:{}",
                        second.device_public_key().unwrap()
                    ),
                ])
                .unwrap(),
                Tag::parse(["device", second.device_public_key().unwrap()]).unwrap(),
                Tag::parse(["status", "owned"]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(120))
            .finalize(&owner)
            .unwrap()
            .as_json();
        publisher
            .queue_command(
                second.device_public_key().unwrap(),
                CoordinationCommand::OwnerBindingResponse {
                    sender_device_public_key: first.device_public_key().unwrap().into(),
                    owner_public_key: owner.public_key().to_hex(),
                    unsigned_event_template: template.clone(),
                    signed_event_json: signed.clone(),
                    requested_at: 120,
                    expires_at: 1_000,
                },
                120,
            )
            .await
            .unwrap();
        let snapshot = reader.receive(121).await.unwrap();
        assert!(
            matches!(&snapshot.commands[0], CoordinationCommand::OwnerBindingResponse {
            unsigned_event_template, signed_event_json, ..
        } if unsigned_event_template == &template && signed_event_json == &signed)
        );
    }

    #[test]
    fn nip46_key_is_distinct_and_persists_protected_connection_data() {
        let root = tempfile::tempdir().unwrap();
        let device = DeviceIdentity::load_or_create(root.path()).unwrap();
        let connection = Nip46ConnectionIdentity::load_or_create(root.path()).unwrap();
        assert_ne!(device.device_public_key().unwrap(), connection.public_key());
        connection
            .save_connection_data(
                "{\"relay\":\"wss://relay.example.com\",\"secret\":\"protected\"}",
            )
            .unwrap();
        assert!(connection
            .load_connection_data()
            .unwrap()
            .unwrap()
            .contains("protected"));
        let path = root.path().join("identity/nip46.connection.json");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn relay_adapter_has_no_product_rpc_type_boundary() {
        let source = include_str!("relay.rs");
        assert!(!source.contains(&["Rpc", "Request"].concat()));
        assert!(!source.contains(&["Catalog", "Snapshot"].concat()));
        assert!(!source.contains(&["Session", "Prepare"].concat()));
    }
}
