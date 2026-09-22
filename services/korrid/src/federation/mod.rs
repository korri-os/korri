//! Verified peer memory and live state. Clone this aggregate, never open a
//! second writer. Callers do network work between begin_work and commit methods.
pub mod coordinator;
mod store;

use crate::{
    authorization::{Authorization, AuthorizationContext, Principal},
    identity::{DeviceIdentity, IdentityState, OwnerStatementStatus, VerifiedOwnerStatement},
    peer_rpc::PeerCredentials,
    relay::{decode_endpoint, EndpointRecord},
};
use nostr::key::PublicKey;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use store::Store;

const MAX_MEMORY_BYTES: usize = 8 * 1024 * 1024;
const MAX_PEERS: usize = 1024;
const MAX_UPDATE_EVENTS: usize = 256;
const MAX_FUTURE_SECONDS: u64 = 300;

#[derive(Debug, thiserror::Error)]
pub enum FederationError {
    #[error("federation private storage is unavailable or changed")]
    Storage,
    #[error("federation evidence does not bind the current owned identity")]
    Identity,
    #[error("federation work is stale")]
    Stale,
    #[error("federation input exceeds its bound")]
    Bounds,
    #[error("endpoint publication progression exceeds the clock or counter bound")]
    Publication,
    #[error("peer endpoint must be a bounded HTTP(S) origin")]
    Endpoint,
}

/// One policy for static, relay and remembered peer endpoints. Check the raw
/// authority/path too: URL parsing alone silently repairs credentials, dot paths,
/// backslashes and whitespace. Relay WebSocket URLs use their separate policy.
pub fn peer_origin(value: &str) -> Result<String, FederationError> {
    if value.is_empty()
        || value.len() > 2048
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
        || value.contains('\\')
    {
        return Err(FederationError::Endpoint);
    }
    let (_, rest) = value.split_once("://").ok_or(FederationError::Endpoint)?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    if !path.is_empty() || authority.contains(['@', '?', '#']) || authority.ends_with(':') {
        return Err(FederationError::Endpoint);
    }
    let url = url::Url::parse(value).map_err(|_| FederationError::Endpoint)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.port_or_known_default().is_none_or(|port| port == 0)
    {
        return Err(FederationError::Endpoint);
    }
    Ok(url.origin().ascii_serialization())
}

/// In-memory only; this is not the future PeerList wire contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PeerState {
    Loading,
    Ready,
    Failed { error: String },
}

#[derive(Clone, Debug)]
pub struct PeerSnapshot {
    pub device_public_key: String,
    pub membership: VerifiedOwnerStatement,
    pub current_endpoint: Option<EndpointRecord>,
    pub remembered_endpoint: Option<EndpointRecord>,
    pub first_seen: u64,
    pub last_seen: u64,
    pub state: PeerState,
    pub updated_at: u64,
}

/// Must accompany results of asynchronous work. Identity and configuration
/// changes and membership changes invalidate old work. Peer attempts also carry
/// a per-peer sequence, so unrelated fan-out completions do not conflict.
#[derive(Clone, Debug)]
pub struct WorkToken {
    epoch: u64,
    identity: IdentityState,
    peer_attempt: Option<(String, u64)>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Publication {
    pub generation: u64,
    pub created_at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Memory {
    local_device_public_key: String,
    #[serde(deserialize_with = "required_option")]
    owner_public_key: Option<String>,
    #[serde(deserialize_with = "required_option")]
    publication: Option<Publication>,
    peers: BTreeMap<String, RememberedPeer>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RememberedPeer {
    // Signed sources avoid a second unverifiable copy of binding/generation fields.
    owner_statement: String,
    #[serde(deserialize_with = "required_option")]
    endpoint_event: Option<String>,
    first_seen: u64,
    last_seen: u64,
}

struct LivePeer {
    state: PeerState,
    updated_at: u64,
    attempt: u64,
}

struct DirectoryState {
    store: Store,
    memory: Memory,
    live: BTreeMap<String, LivePeer>,
    epoch: u64,
}

#[derive(Clone)]
pub struct FederationDirectory {
    state: Arc<Mutex<DirectoryState>>,
    credentials: PeerCredentials,
    authorization: Authorization,
    root: PathBuf,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl FederationDirectory {
    pub fn reset_persisted_after_identity_switch(root: &Path) -> Result<(), FederationError> {
        let identity =
            DeviceIdentity::load_or_create(root).map_err(|_| FederationError::Identity)?;
        let device = identity
            .device_public_key()
            .ok_or(FederationError::Identity)?;
        if matches!(
            identity.state(),
            IdentityState::Invalid { .. } | IdentityState::Revoked { .. }
        ) {
            return Err(FederationError::Identity);
        }
        store::validate_ancestors(root)?;
        store::private_directory(root)?;
        let mut store = Store::open(root)?;
        store.save(encode(&Memory {
            local_device_public_key: device.into(),
            owner_public_key: owner_key(&identity).map(str::to_owned),
            publication: None,
            peers: BTreeMap::new(),
        })?)
    }

    pub fn open(
        root: &Path,
        credentials: PeerCredentials,
        authorization: Authorization,
        clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Result<Self, FederationError> {
        let state = credentials
            .with_identity(|identity| {
                store::validate_ancestors(root)?;
                store::private_directory(root)?;
                let stored_identity =
                    DeviceIdentity::load_or_create(root).map_err(|_| FederationError::Identity)?;
                if stored_identity.state() != identity.state() {
                    return Err(FederationError::Identity);
                }
                let device = identity
                    .device_public_key()
                    .ok_or(FederationError::Identity)?;
                let owner = owner_key(identity);
                if matches!(
                    identity.state(),
                    IdentityState::Invalid { .. } | IdentityState::Revoked { .. }
                ) {
                    return Err(FederationError::Identity);
                }
                validate_authorization_paths(root)?;
                let mut store = Store::open(root)?;
                let mut memory: Memory = match store.bytes() {
                    Some(bytes) => {
                        serde_json::from_slice(bytes).map_err(|_| FederationError::Storage)?
                    }
                    None => Memory {
                        local_device_public_key: device.into(),
                        owner_public_key: owner.map(str::to_owned),
                        publication: None,
                        peers: BTreeMap::new(),
                    },
                };
                check_binding(&memory, identity)?;
                if memory.peers.len() > MAX_PEERS
                    || memory
                        .publication
                        .as_ref()
                        .is_some_and(|p| p.generation == 0)
                {
                    return Err(FederationError::Bounds);
                }
                for (key, peer) in &memory.peers {
                    let statement = verify_membership(identity, &peer.owner_statement)?;
                    if statement.device_public_key != *key
                        || statement.status != OwnerStatementStatus::Owned
                        || peer.first_seen > peer.last_seen
                    {
                        return Err(FederationError::Identity);
                    }
                    if let Some(event) = &peer.endpoint_event {
                        let endpoint = endpoint_from_event(identity, event, None)?;
                        check_endpoint_membership(&endpoint, &statement)?;
                    }
                }
                prune_revoked(&mut memory, identity, &authorization)?;
                store.save(encode(&memory)?)?;
                let now = clock();
                let live = memory
                    .peers
                    .keys()
                    .map(|key| {
                        (
                            key.clone(),
                            LivePeer {
                                state: PeerState::Loading,
                                updated_at: now,
                                attempt: 0,
                            },
                        )
                    })
                    .collect();
                Ok(DirectoryState {
                    store,
                    memory,
                    live,
                    epoch: 0,
                })
            })
            .map_err(|_| FederationError::Identity)??;
        Ok(Self {
            state: Arc::new(Mutex::new(state)),
            credentials,
            authorization,
            root: root.into(),
            clock,
        })
    }

    /// Static configuration supplies no owner statement. It still must not
    /// override terminal revocations held by the shared secure-RPC authority.
    pub fn peer_is_revoked(&self, device: &str) -> Result<bool, FederationError> {
        self.credentials
            .with_identity(|identity| {
                let Some(owner) = owner_key(identity) else {
                    return Ok(true);
                };
                self.authorization
                    .is_device_revoked(owner, device)
                    .map_err(|_| FederationError::Identity)
            })
            .map_err(|_| FederationError::Identity)?
    }

    pub fn begin_work(&self) -> Result<WorkToken, FederationError> {
        let state = self.state.lock().map_err(|_| FederationError::Storage)?;
        let identity = self
            .credentials
            .identity_snapshot()
            .map_err(|_| FederationError::Identity)?;
        check_binding(&state.memory, &identity)?;
        Ok(WorkToken {
            epoch: state.epoch,
            identity: identity.state().clone(),
            peer_attempt: None,
        })
    }

    /// Start a peer operation before network I/O. A newer attempt supersedes
    /// older completions for this peer only, not parallel fan-out to other peers.
    pub fn begin_peer_work(&self, device: &str) -> Result<WorkToken, FederationError> {
        let token = self.begin_work()?;
        self.mutate(&token, |state, identity, authorization, _| {
            let peer = state
                .memory
                .peers
                .get(device)
                .ok_or(FederationError::Identity)?;
            if !is_authorized(identity, authorization, &peer.owner_statement)? {
                return Err(FederationError::Identity);
            }
            let live = state
                .live
                .get_mut(device)
                .ok_or(FederationError::Identity)?;
            live.attempt = next_epoch(live.attempt)?;
            Ok(WorkToken {
                peer_attempt: Some((device.into(), live.attempt)),
                ..token.clone()
            })
        })
    }

    /// Call before replacing a relay/config snapshot. No network is performed here.
    pub fn invalidate_work(&self) -> Result<(), FederationError> {
        let mut state = self.state.lock().map_err(|_| FederationError::Storage)?;
        state.epoch = next_epoch(state.epoch)?;
        Ok(())
    }

    /// Replace all remembered federation state after an atomic identity switch.
    ///
    /// This operation deliberately does not accept the directory's current
    /// credential identity: after the identity-directory exchange, this live
    /// object still holds the old credential snapshot until korrid restarts.
    /// Holding the directory state lock across validation and the durable write
    /// excludes every ordinary peer/publication mutation. Ordinary operations
    /// remain bound to the credential identity and fail after this reset.
    pub fn reset_after_identity_switch(
        &self,
        new_device_public_key: &str,
        new_owner_public_key: &str,
    ) -> Result<(), FederationError> {
        validate_reset_public_key(new_device_public_key)?;
        validate_reset_public_key(new_owner_public_key)?;
        if new_device_public_key == new_owner_public_key {
            return Err(FederationError::Identity);
        }
        let memory = Memory {
            local_device_public_key: new_device_public_key.into(),
            owner_public_key: Some(new_owner_public_key.into()),
            publication: None,
            peers: BTreeMap::new(),
        };
        let bytes = encode(&memory)?;
        let mut state = self.state.lock().map_err(|_| FederationError::Storage)?;
        let epoch = next_epoch(state.epoch)?;
        state.store.validate()?;
        state.store.save(bytes)?;
        state.memory = memory;
        state.live.clear();
        state.epoch = epoch;
        Ok(())
    }

    fn mutate<R>(
        &self,
        token: &WorkToken,
        update: impl FnOnce(
            &mut DirectoryState,
            &DeviceIdentity,
            &Authorization,
            u64,
        ) -> Result<R, FederationError>,
    ) -> Result<R, FederationError> {
        let mut state = self.state.lock().map_err(|_| FederationError::Storage)?;
        // Hold the EXISTING credential identity lock until commit. Reload cannot
        // race the final binding check; no network runs under either lock.
        self.credentials
            .with_identity(|identity| {
                if token.epoch != state.epoch || &token.identity != identity.state() {
                    return Err(FederationError::Stale);
                }
                check_binding(&state.memory, identity)?;
                state.store.validate()?;
                validate_authorization_paths(&self.root)?;
                update(&mut state, identity, &self.authorization, (self.clock)())
            })
            .map_err(|_| FederationError::Identity)?
    }

    /// Pass B1's latest.event_json AND each revocation.event_json. Every source
    /// is verified again here; bounded/empty snapshots never remove membership.
    pub fn apply_membership(
        &self,
        token: &WorkToken,
        events: &[String],
    ) -> Result<(), FederationError> {
        if events.len() > MAX_UPDATE_EVENTS {
            return Err(FederationError::Bounds);
        }
        self.mutate(token, |state, identity, authorization, now| {
            let statements = events
                .iter()
                .map(|event| verify_membership(identity, event))
                .collect::<Result<Vec<_>, _>>()?;
            let revocations: Vec<_> = events
                .iter()
                .zip(&statements)
                .filter(|(_, s)| s.status == OwnerStatementStatus::Revoked)
                .map(|(e, _)| e.clone())
                .collect();
            if !revocations.is_empty() {
                // The owner signatures themselves authorize these terminal facts.
                // Reuse Authorization's verifier and exact signed-event storage;
                // this is relay evidence ingestion, not an RPC/replay-nonce path.
                let local_statement = identity
                    .owner_statement_json()
                    .ok_or(FederationError::Identity)?;
                let attempt = authorization
                    .attempt(
                        identity.state(),
                        identity
                            .device_public_key()
                            .ok_or(FederationError::Identity)?,
                        Some(&local_statement),
                        None,
                        &revocations,
                        now,
                    )
                    .map_err(|_| FederationError::Identity)?;
                if !matches!(
                    attempt.context(),
                    AuthorizationContext::Peer(Principal::OwnerDevice { .. })
                ) {
                    return Err(FederationError::Identity);
                }
                authorization
                    .commit_revocations(&attempt)
                    .map_err(|_| FederationError::Storage)?;
            }
            let mut memory = state.memory.clone();
            memory.owner_public_key = owner_key(identity).map(str::to_owned);
            let mut changed = prune_revoked(&mut memory, identity, authorization)?;
            for (event, statement) in events.iter().zip(statements) {
                if statement.status != OwnerStatementStatus::Owned
                    || !is_authorized(identity, authorization, event)?
                {
                    continue;
                }
                match memory.peers.get_mut(&statement.device_public_key) {
                    Some(peer) => {
                        let current = verify_membership(identity, &peer.owner_statement)?;
                        if statement.is_newer_than(&current) {
                            peer.owner_statement = event.clone();
                            changed = true;
                        }
                        // Stale binding snapshots must not advance observation metadata.
                        if statement.event_id == current.event_id
                            || statement.is_newer_than(&current)
                        {
                            peer.last_seen = peer.last_seen.max(now);
                        }
                    }
                    None => {
                        memory.peers.insert(
                            statement.device_public_key.clone(),
                            RememberedPeer {
                                owner_statement: event.clone(),
                                endpoint_event: None,
                                first_seen: now,
                                last_seen: now,
                            },
                        );
                        changed = true;
                    }
                }
            }
            if memory.peers.len() > MAX_PEERS {
                return Err(FederationError::Bounds);
            }
            let epoch = if changed || !revocations.is_empty() {
                next_epoch(state.epoch)?
            } else {
                state.epoch
            };
            state.store.save(encode(&memory)?)?;
            state.live.retain(|key, _| memory.peers.contains_key(key));
            for key in memory.peers.keys() {
                state.live.entry(key.clone()).or_insert(LivePeer {
                    state: PeerState::Loading,
                    updated_at: now,
                    attempt: 0,
                });
            }
            state.memory = memory;
            state.epoch = epoch;
            Ok(())
        })
    }

    /// Accept the original encrypted signed event, never a self-asserted record.
    /// Expired receive events are rejected; already remembered evidence is retained.
    pub fn apply_endpoint_event(
        &self,
        token: &WorkToken,
        event: &str,
    ) -> Result<bool, FederationError> {
        self.mutate(token, |state, identity, authorization, now| {
            let endpoint = endpoint_from_event(identity, event, Some(now))?;
            let peer = state
                .memory
                .peers
                .get(&endpoint.device_public_key)
                .ok_or(FederationError::Identity)?;
            let membership = verify_membership(identity, &peer.owner_statement)?;
            if !is_authorized(identity, authorization, &peer.owner_statement)? {
                return Err(FederationError::Identity);
            }
            check_endpoint_membership(&endpoint, &membership)?;
            if let Some(current) = &peer.endpoint_event {
                let current = endpoint_from_event(identity, current, None)?;
                // Exact source-protocol order: generation, then issued_at; first
                // accepted value wins an equal pair, including across restart.
                if endpoint.generation < current.generation
                    || (endpoint.generation == current.generation
                        && endpoint.issued_at <= current.issued_at)
                {
                    return Ok(false);
                }
            }
            let mut memory = state.memory.clone();
            let peer = memory
                .peers
                .get_mut(&endpoint.device_public_key)
                .ok_or(FederationError::Identity)?;
            peer.endpoint_event = Some(event.into());
            peer.last_seen = peer.last_seen.max(now);
            state.store.save(encode(&memory)?)?;
            state.memory = memory;
            Ok(true)
        })
    }

    /// Reserve before constructing or publishing any event. B4 must use created_at
    /// as BOTH EndpointRecord.issued_at and publish_endpoint's NIP timestamp.
    /// Failed/partial publication burns the reservation, never rolls it back.
    pub fn reserve_publication(&self, token: &WorkToken) -> Result<Publication, FederationError> {
        self.mutate(token, |state, identity, _, now| {
            if owner_key(identity).is_none() {
                return Err(FederationError::Identity);
            }
            let publication = match &state.memory.publication {
                None => Publication {
                    generation: 1,
                    created_at: now,
                },
                Some(previous) => Publication {
                    generation: previous
                        .generation
                        .checked_add(1)
                        .ok_or(FederationError::Publication)?,
                    created_at: now.max(
                        previous
                            .created_at
                            .checked_add(1)
                            .ok_or(FederationError::Publication)?,
                    ),
                },
            };
            if publication.created_at > now.saturating_add(MAX_FUTURE_SECONDS) {
                return Err(FederationError::Publication);
            }
            let mut memory = state.memory.clone();
            memory.owner_public_key = owner_key(identity).map(str::to_owned);
            memory.publication = Some(publication.clone());
            state.store.save(encode(&memory)?)?;
            state.memory = memory;
            Ok(publication)
        })
    }

    pub fn set_peer_state(
        &self,
        token: &WorkToken,
        device: &str,
        value: PeerState,
    ) -> Result<(), FederationError> {
        if let PeerState::Failed { error } = &value {
            if error.is_empty() || error.len() > 512 || error.chars().any(char::is_control) {
                return Err(FederationError::Bounds);
            }
        }
        self.mutate(token, |state, identity, authorization, now| {
            let peer = state
                .memory
                .peers
                .get(device)
                .ok_or(FederationError::Identity)?;
            if !is_authorized(identity, authorization, &peer.owner_statement)? {
                return Err(FederationError::Identity);
            }
            let live = state
                .live
                .get_mut(device)
                .ok_or(FederationError::Identity)?;
            if token.peer_attempt.as_ref() != Some(&(device.to_owned(), live.attempt)) {
                return Err(FederationError::Stale);
            }
            let attempt = next_epoch(live.attempt)?;
            live.state = value;
            live.updated_at = live.updated_at.max(now);
            live.attempt = attempt;
            Ok(())
        })
    }

    pub fn snapshot(&self) -> Result<Vec<PeerSnapshot>, FederationError> {
        let state = self.state.lock().map_err(|_| FederationError::Storage)?;
        self.credentials
            .with_identity(|identity| {
                check_binding(&state.memory, identity)?;
                state.store.validate()?;
                validate_authorization_paths(&self.root)?;
                let authorization = &self.authorization;
                let now = (self.clock)();
                let mut peers = Vec::new();
                for (key, peer) in &state.memory.peers {
                    if !is_authorized(identity, authorization, &peer.owner_statement)? {
                        continue;
                    }
                    let membership = verify_membership(identity, &peer.owner_statement)?;
                    let endpoint = peer
                        .endpoint_event
                        .as_ref()
                        .map(|event| endpoint_from_event(identity, event, None))
                        .transpose()?;
                    let live = state.live.get(key).ok_or(FederationError::Identity)?;
                    peers.push(PeerSnapshot {
                        device_public_key: key.clone(),
                        membership,
                        current_endpoint: endpoint.clone().filter(|e| e.validate(now).is_ok()),
                        remembered_endpoint: endpoint,
                        first_seen: peer.first_seen,
                        last_seen: peer.last_seen,
                        state: live.state.clone(),
                        updated_at: live.updated_at,
                    });
                }
                Ok(peers)
            })
            .map_err(|_| FederationError::Identity)?
    }
}

fn owner_key(identity: &DeviceIdentity) -> Option<&str> {
    match identity.state() {
        IdentityState::Owned {
            owner_public_key, ..
        } => Some(owner_public_key),
        _ => None,
    }
}

fn check_binding(memory: &Memory, identity: &DeviceIdentity) -> Result<(), FederationError> {
    if identity.device_public_key() != Some(memory.local_device_public_key.as_str())
        || memory
            .owner_public_key
            .as_deref()
            .is_some_and(|owner| Some(owner) != owner_key(identity))
        || (memory.owner_public_key.is_none()
            && (!memory.peers.is_empty() || memory.publication.is_some()))
        || (owner_key(identity).is_none() && !memory.peers.is_empty())
        || matches!(
            identity.state(),
            IdentityState::Invalid { .. } | IdentityState::Revoked { .. }
        )
    {
        return Err(FederationError::Identity);
    }
    Ok(())
}

fn verify_membership(
    identity: &DeviceIdentity,
    event: &str,
) -> Result<VerifiedOwnerStatement, FederationError> {
    let statement =
        DeviceIdentity::derive_owner_statement(event).map_err(|_| FederationError::Identity)?;
    if Some(statement.owner_public_key.as_str()) != owner_key(identity)
        || Some(statement.device_public_key.as_str()) == identity.device_public_key()
    {
        return Err(FederationError::Identity);
    }
    Ok(statement)
}

fn endpoint_from_event(
    identity: &DeviceIdentity,
    event: &str,
    now: Option<u64>,
) -> Result<EndpointRecord, FederationError> {
    let verified =
        DeviceIdentity::verify_encrypted_event(event).map_err(|_| FederationError::Identity)?;
    let at = match now {
        Some(now) => now,
        None => {
            let plaintext = identity
                .decrypt_event(event)
                .map_err(|_| FederationError::Identity)?;
            let endpoint: EndpointRecord =
                serde_json::from_str(&plaintext).map_err(|_| FederationError::Identity)?;
            endpoint.issued_at
        }
    };
    decode_endpoint(
        identity,
        event,
        &verified.author,
        identity
            .device_public_key()
            .ok_or(FederationError::Identity)?,
        at,
    )
    .map_err(|_| FederationError::Identity)
}

fn check_endpoint_membership(
    endpoint: &EndpointRecord,
    membership: &VerifiedOwnerStatement,
) -> Result<(), FederationError> {
    if endpoint.device_public_key != membership.device_public_key
        || endpoint.owner_public_key != membership.owner_public_key
        || membership.status != OwnerStatementStatus::Owned
    {
        return Err(FederationError::Identity);
    }
    Ok(())
}

fn validate_authorization_paths(root: &Path) -> Result<(), FederationError> {
    // The caller passes the SAME Authorization loaded for this private root as
    // secure RPC. Directory commits update all its clones, not a second cache.
    for path in [
        root.join("identity"),
        root.join("identity/authorization-revocations"),
        root.join("identity/peer-certificates"),
    ] {
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {
                store::validate_ancestors(&path)?;
                store::private_directory(&path)?;
            }
            Err(_) => return Err(FederationError::Storage),
        }
    }
    Ok(())
}

fn is_authorized(
    identity: &DeviceIdentity,
    authorization: &Authorization,
    statement: &str,
) -> Result<bool, FederationError> {
    let peer = verify_membership(identity, statement)?;
    let attempt = authorization
        .attempt(
            identity.state(),
            &peer.device_public_key,
            Some(statement),
            None,
            &[],
            0,
        )
        .map_err(|_| FederationError::Identity)?;
    Ok(matches!(
        attempt.context(),
        AuthorizationContext::Peer(Principal::OwnerDevice { .. })
    ))
}

fn prune_revoked(
    memory: &mut Memory,
    identity: &DeviceIdentity,
    authorization: &Authorization,
) -> Result<bool, FederationError> {
    let mut removed = Vec::new();
    for (key, peer) in &memory.peers {
        if !is_authorized(identity, authorization, &peer.owner_statement)? {
            removed.push(key.clone());
        }
    }
    for key in &removed {
        memory.peers.remove(key);
    }
    Ok(!removed.is_empty())
}

// Option fields are nullable, not optional: deleting security state is corruption.
fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

fn encode(memory: &Memory) -> Result<Vec<u8>, FederationError> {
    serde_json::to_vec(memory).map_err(|_| FederationError::Storage)
}
fn validate_reset_public_key(value: &str) -> Result<(), FederationError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || PublicKey::from_hex(value).is_err()
    {
        return Err(FederationError::Identity);
    }
    Ok(())
}

fn next_epoch(epoch: u64) -> Result<u64, FederationError> {
    epoch.checked_add(1).ok_or(FederationError::Bounds)
}

#[cfg(test)]
mod tests;
