//! Relay effects and their joined lifetime. The directory remains the only
//! persistence owner; each cycle signs with the shared credentials' snapshot.
use super::{peer_origin, FederationDirectory, FederationError};
use crate::{
    authorization::Authorization,
    config::snapshot::{ConfigSnapshotCoordinator, SnapshotAuthorization},
    identity::IdentityState,
    peer_rpc::{unix_time, PeerCredentials},
    relay::{
        CoordinatedRelays, EndpointRecord, PublishState, RelayCoordinator, RelayList,
        RelayTransport,
    },
};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};
use tokio::{sync::watch, task::JoinHandle};

#[derive(Clone)]
pub struct FederationResources {
    pub credentials: PeerCredentials,
    pub authorization: Authorization,
    pub directory: FederationDirectory,
}
impl FederationResources {
    pub fn open(root: &Path) -> Result<Self, FederationError> {
        let credentials = PeerCredentials::load(root).map_err(|_| FederationError::Identity)?;
        let authorization = Authorization::load(root).map_err(|_| FederationError::Storage)?;
        let directory = FederationDirectory::open(
            root,
            credentials.clone(),
            authorization.clone(),
            Arc::new(unix_time),
        )?;
        Ok(Self {
            credentials,
            authorization,
            directory,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryInputs {
    pub relays: Option<RelayList>,
    pub advertised_endpoints: Vec<String>,
    pub label: Option<String>,
    pub moonlight_address: Option<String>,
}
impl DiscoveryInputs {
    /// Never consume the coordinator's initially empty or retained unauthorized
    /// snapshot. Android has no reachable peer listener, so it is query-only.
    pub fn android(config: &ConfigSnapshotCoordinator) -> Result<Self, FederationError> {
        let state = config.reload();
        if state.authorization != SnapshotAuthorization::Authorized {
            return Err(FederationError::Identity);
        }
        Ok(Self {
            relays: RelayList::from_device_settings(&state.snapshot)
                .map_err(|_| FederationError::Bounds)?,
            advertised_endpoints: Vec::new(),
            // Query-only discovery has no endpoint metadata to validate/publish.
            label: None,
            moonlight_address: None,
        })
    }
    pub fn linux(
        config: &ConfigSnapshotCoordinator,
        initial: &Self,
    ) -> Result<Self, FederationError> {
        let state = config.reload();
        if state.authorization != SnapshotAuthorization::Authorized {
            return Err(FederationError::Identity);
        }
        let mut inputs = initial.clone();
        if inputs.advertised_endpoints.is_empty() {
            inputs.label = None;
            inputs.moonlight_address = None;
        } else {
            inputs.label = state
                .snapshot
                .host
                .as_ref()
                .and_then(|host| host.title.clone())
                .or(inputs.label);
        }
        Ok(inputs)
    }
    pub fn validate(&self) -> Result<(), FederationError> {
        // All adapters share this rule, including Linux's startup validation:
        // query-only discovery has no endpoint metadata to validate or publish.
        if self.advertised_endpoints.is_empty() {
            return Ok(());
        }
        if self.advertised_endpoints.len() > 8 {
            return Err(FederationError::Bounds);
        }
        for endpoint in &self.advertised_endpoints {
            peer_origin(endpoint)?;
        }
        for value in [&self.label, &self.moonlight_address].into_iter().flatten() {
            if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
                return Err(FederationError::Bounds);
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct DiscoveryTiming {
    pub poll: Duration,
    pub failure_initial: Duration,
    pub failure_max: Duration,
    pub lifetime: u64,
    pub renew: u64,
}
impl Default for DiscoveryTiming {
    fn default() -> Self {
        Self {
            poll: Duration::from_secs(60),
            failure_initial: Duration::from_secs(5),
            failure_max: Duration::from_secs(300),
            lifetime: 24 * 3600,
            renew: 6 * 3600,
        }
    }
}
impl DiscoveryTiming {
    pub fn retry_delay(&self, failures: u32) -> Duration {
        self.failure_initial
            .saturating_mul(1u32.checked_shl(failures).unwrap_or(u32::MAX))
            .min(self.failure_max)
    }
}

#[derive(Clone, Default)]
struct Signal {
    cancelled: bool,
    revision: u64,
}
#[derive(Clone)]
pub struct DiscoveryControl {
    signal: watch::Sender<Signal>,
    directory: FederationDirectory,
}
impl DiscoveryControl {
    pub fn wake(&self) -> Result<(), FederationError> {
        self.directory.invalidate_work()?;
        self.signal
            .send_modify(|signal| signal.revision = signal.revision.wrapping_add(1));
        Ok(())
    }
    pub fn cancel(&self) {
        self.signal.send_modify(|signal| signal.cancelled = true);
    }
}

#[must_use = "discovery must be cancelled and joined with shutdown"]
pub struct DiscoveryTask {
    control: DiscoveryControl,
    task: JoinHandle<()>,
}
impl DiscoveryTask {
    pub fn control(&self) -> DiscoveryControl {
        self.control.clone()
    }
    pub async fn shutdown(mut self) -> Result<(), tokio::task::JoinError> {
        self.control.cancel();
        (&mut self.task).await
    }
}
impl Drop for DiscoveryTask {
    fn drop(&mut self) {
        // Explicit shutdown joins. Accidental handle loss must not detach work.
        self.control.cancel();
        self.task.abort();
    }
}

pub type InputsProvider = Arc<dyn Fn() -> Result<DiscoveryInputs, FederationError> + Send + Sync>;
pub struct Discovery<T> {
    directory: FederationDirectory,
    credentials: PeerCredentials,
    inputs: InputsProvider,
    transport: Arc<T>,
    timing: DiscoveryTiming,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    previous_inputs: Option<DiscoveryInputs>,
    publication: Option<EndpointRecord>,
    delivered: BTreeMap<String, u64>,
}
impl<T: RelayTransport + 'static> Discovery<T> {
    pub fn new(
        directory: FederationDirectory,
        credentials: PeerCredentials,
        inputs: InputsProvider,
        transport: Arc<T>,
        timing: DiscoveryTiming,
        clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Self {
        Self {
            directory,
            credentials,
            inputs,
            transport,
            timing,
            clock,
            previous_inputs: None,
            publication: None,
            delivered: BTreeMap::new(),
        }
    }
    /// Construct outside a runtime when necessary; run/spawn only inside the
    /// owning runtime. A wake drops in-flight I/O before taking fresh evidence.
    pub fn start(
        self,
    ) -> (
        DiscoveryControl,
        impl std::future::Future<Output = ()> + Send,
    ) {
        let (signal, receiver) = watch::channel(Signal::default());
        let control = DiscoveryControl {
            signal,
            directory: self.directory.clone(),
        };
        (control, self.run(receiver))
    }
    pub fn spawn(self) -> DiscoveryTask {
        let (control, future) = self.start();
        DiscoveryTask {
            control,
            task: tokio::spawn(future),
        }
    }
    async fn run(mut self, mut signal: watch::Receiver<Signal>) {
        let mut failures = 0u32;
        loop {
            if signal.borrow_and_update().cancelled {
                break;
            }
            let result = tokio::select! {
                biased;
                changed = signal.changed() => { if changed.is_err() { break; } continue; }
                result = self.cycle() => result,
            };
            let delay = if result.is_ok() {
                failures = 0;
                self.timing.poll
            } else {
                let delay = self.timing.retry_delay(failures);
                failures = failures.saturating_add(1);
                delay
            };
            tokio::select! {
                biased;
                changed = signal.changed() => { if changed.is_err() { break; } }
                _ = tokio::time::sleep(delay) => {}
            }
        }
    }
    async fn cycle(&mut self) -> Result<(), FederationError> {
        let inputs = (self.inputs)()?;
        inputs.validate()?;
        if self.previous_inputs.as_ref() != Some(&inputs) {
            self.directory.invalidate_work()?;
            self.publication = None;
            self.delivered.clear();
            self.previous_inputs = Some(inputs.clone());
        }
        let Some(relays) = inputs.relays else {
            return Ok(());
        };
        let identity = self
            .credentials
            .identity_snapshot()
            .map_err(|_| FederationError::Identity)?;
        let (device, owner) = match identity.state() {
            IdentityState::Owned {
                device_public_key,
                owner_public_key,
                ..
            } => (device_public_key.clone(), owner_public_key.clone()),
            _ => return Ok(()),
        };
        let relay = CoordinatedRelays::new(relays, identity, self.transport.clone())
            .map_err(|_| FederationError::Identity)?;
        let token = self.directory.begin_work()?;
        let owner_publication = relay.publish_owner_statement().await;
        let roster = relay.read_owner_roster().await;
        let roster_ok = roster.is_ok();
        if let Ok(entries) = roster {
            let sources = entries
                .into_iter()
                .flat_map(|entry| {
                    std::iter::once(entry.latest.event_json)
                        .chain(entry.revocation.map(|e| e.event_json))
                })
                .collect::<Vec<_>>();
            self.directory.apply_membership(&token, &sources)?;
        }
        let token = self.directory.begin_work()?;
        let now = (self.clock)();
        let received = relay.receive(now).await;
        let endpoint_ok = received
            .as_ref()
            .is_ok_and(|result| !result.all_relay_reads_failed);
        if let Ok(received) = received {
            for evidence in received.endpoints {
                // Invalid untrusted announcements do not poison other valid peers.
                match self
                    .directory
                    .apply_endpoint_event(&token, &evidence.event_json)
                {
                    Err(
                        FederationError::Identity
                        | FederationError::Endpoint
                        | FederationError::Bounds,
                    ) => {}
                    result => {
                        result?;
                    }
                }
            }
        }
        let mut publication_ok = owner_publication.is_ok_and(|state| state.accepted());
        if !inputs.advertised_endpoints.is_empty() {
            let renew = self.publication.as_ref().is_none_or(|record| {
                record.owner_public_key != owner
                    || now >= record.issued_at.saturating_add(self.timing.renew)
            });
            if renew {
                let reservation = self.directory.reserve_publication(&token)?;
                self.publication = Some(EndpointRecord {
                    device_public_key: device,
                    owner_public_key: owner,
                    generation: reservation.generation,
                    candidates: inputs.advertised_endpoints,
                    issued_at: reservation.created_at,
                    expires_at: reservation
                        .created_at
                        .checked_add(self.timing.lifetime)
                        .ok_or(FederationError::Publication)?,
                    label: inputs.label,
                    moonlight_address: inputs.moonlight_address,
                });
                self.delivered.clear();
            }
            let record = self
                .publication
                .as_ref()
                .expect("reserved nonempty endpoints");
            // The outer NIP timestamp is reserved, not a substitute for today's
            // expiry/future-skew check when retrying a prior reservation.
            record
                .validate(now)
                .map_err(|_| FederationError::Publication)?;
            for peer in self.directory.snapshot()? {
                if self.directory.peer_is_revoked(&peer.device_public_key)? {
                    continue;
                }
                if self.delivered.get(&peer.device_public_key) == Some(&record.generation) {
                    continue;
                }
                let result = relay
                    .publish_endpoint(&peer.device_public_key, record.clone(), record.issued_at)
                    .await;
                match result {
                    Ok(PublishState::Published { .. }) => {
                        self.delivered
                            .insert(peer.device_public_key, record.generation);
                    }
                    Ok(PublishState::Partial { .. }) => {}
                    _ => publication_ok = false,
                }
            }
        }
        // Empty and partial successful reads are healthy. Partial writes remain
        // pending for the next poll; only total operation failure uses backoff.
        if roster_ok && endpoint_ok && publication_ok {
            Ok(())
        } else {
            Err(FederationError::Stale)
        }
    }
}
