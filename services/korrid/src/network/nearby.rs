//! Nearby link lifecycle. NetworkManager keeps an independent 120 s home
//! rollback deadline. korrid renews it every 30 s, never after losing the link.
use super::{dbus, NetworkFailure};
use crate::{
    federation::{coordinator::FederationResources, PeerSnapshot, PeerState},
    identity::DeviceIdentity,
};
use dbus::nearby as nm;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, OwnedMutexGuard};
use zbus::{zvariant::OwnedObjectPath, Connection};
use zeroize::Zeroizing;

const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(75);
const SCAN_TIMEOUT: Duration = Duration::from_secs(25);
const CALL_TIMEOUT: Duration = Duration::from_secs(15);
const REFRESH_INTERVAL: Duration = Duration::from_secs(30);

struct PeerAddress {
    key: String,
    ip: IpAddr,
    valid_until: Instant,
}
static ACTIVE_PEER: Mutex<Option<PeerAddress>> = Mutex::new(None);
static RADIO_OPERATION: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

pub(super) fn radio_operation() -> Arc<tokio::sync::Mutex<()>> {
    RADIO_OPERATION
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

/// This function returns the peer's Wi-Fi address while its nearby link is up.
/// The caller uses this LAN candidate with the peer's existing korrid port.
pub fn nearby_peer_ip(device_public_key: &str) -> Option<IpAddr> {
    let active = ACTIVE_PEER.lock().ok()?;
    active
        .as_ref()
        .filter(|peer| peer.key == device_public_key && peer.valid_until > Instant::now())
        .map(|peer| peer.ip)
}

struct Credentials {
    ssid: String,
    passphrase: Zeroizing<String>,
    starter: bool,
}
impl Credentials {
    fn derive(identity: &DeviceIdentity, peer: &str) -> Result<Self, NetworkFailure> {
        let material = identity
            .nearby_key_material(peer)
            .map_err(|_| NetworkFailure::unavailable())?;
        let expand = |label: &[u8]| {
            let mut mac = Hmac::<Sha256>::new_from_slice(material.as_slice())
                .expect("HMAC accepts any key length");
            mac.update(label);
            Zeroizing::new(<[u8; 32]>::from(mac.finalize().into_bytes()))
        };
        Ok(Self {
            ssid: hex::encode(&expand(b"korri/nearby/ssid/v1")[..16]),
            passphrase: Zeroizing::new(hex::encode(&expand(b"korri/nearby/passphrase/v1")[..30])),
            starter: identity
                .device_public_key()
                .ok_or_else(NetworkFailure::unavailable)?
                < peer,
        })
    }
}

struct Link {
    checkpoint: OwnedObjectPath,
    active: Option<OwnedObjectPath>,
    contact_deadline: Option<Instant>,
    _radio: OwnedMutexGuard<()>,
}

struct NearbyLink {
    bus: Connection,
    resources: FederationResources,
    link: Option<Link>,
}
impl NearbyLink {
    async fn start(&mut self, peer: &str) -> Result<(), NetworkFailure> {
        self.activate_nearby(peer, false).await
    }

    async fn connect_nearby(&mut self, peer: &str) -> Result<(), NetworkFailure> {
        self.activate_nearby(peer, true).await
    }

    async fn activate_nearby(
        &mut self,
        peer: &str,
        client_only: bool,
    ) -> Result<(), NetworkFailure> {
        if self.link.is_some() {
            return Err(NetworkFailure::unavailable());
        }
        let result = tokio::time::timeout(ATTEMPT_TIMEOUT, self.attempt(peer, client_only))
            .await
            .unwrap_or_else(|_| Err(NetworkFailure::timeout()));
        if result.is_err() {
            self.stop().await?;
        }
        result
    }

    async fn attempt(&mut self, peer: &str, client_only: bool) -> Result<(), NetworkFailure> {
        let radio = radio_operation()
            .try_lock_owned()
            .map_err(|_| NetworkFailure::unavailable())?;
        if !self
            .resources
            .directory
            .snapshot()
            .map_err(|_| NetworkFailure::unavailable())?
            .iter()
            .any(|entry| entry.device_public_key == peer)
        {
            return Err(NetworkFailure::unavailable());
        }
        let identity = self
            .resources
            .credentials
            .identity_snapshot()
            .map_err(|_| NetworkFailure::unavailable())?;
        let credentials = Credentials::derive(&identity, peer)?;
        let point = self.scan(&credentials.ssid).await?;
        let starter = point.is_none() && credentials.starter && !client_only;
        if point.is_none() && !starter {
            // The trigger applies backoff before it requests another scan.
            return Ok(());
        }
        let checkpoint = tokio::time::timeout(CALL_TIMEOUT, nm::checkpoint(&self.bus))
            .await
            .map_err(|_| NetworkFailure::timeout())?
            .map_err(|_| NetworkFailure::unavailable())?;
        // korrid stores the checkpoint before activation.
        // Cancellation leaves a handle for stop(). NM keeps its deadline.
        self.link = Some(Link {
            checkpoint,
            active: None,
            contact_deadline: None,
            _radio: radio,
        });
        let active = nm::activate(
            &self.bus,
            &credentials.ssid,
            &credentials.passphrase,
            starter,
            point.as_ref(),
        )
        .await
        .map_err(|_| NetworkFailure::unavailable())?;
        let link = self.link.as_mut().expect("armed checkpoint");
        link.active = Some(active);
        link.contact_deadline = Some(Instant::now() + ATTEMPT_TIMEOUT);
        *ACTIVE_PEER
            .lock()
            .map_err(|_| NetworkFailure::unavailable())? = Some(PeerAddress {
            key: peer.into(),
            ip: IpAddr::V4(Ipv4Addr::new(10, 77, 77, if starter { 2 } else { 1 })),
            valid_until: Instant::now() + REFRESH_INTERVAL + CALL_TIMEOUT,
        });
        Ok(())
    }

    async fn scan(&self, ssid: &str) -> Result<Option<OwnedObjectPath>, NetworkFailure> {
        tokio::time::timeout(SCAN_TIMEOUT, nm::scan_for(&self.bus, ssid))
            .await
            .map_err(|_| NetworkFailure::timeout())?
            .map_err(|_| NetworkFailure::unavailable())
    }

    async fn refresh(&self) -> Result<(), NetworkFailure> {
        let Some(link) = &self.link else {
            return Ok(());
        };
        let active = link
            .active
            .clone()
            .ok_or_else(NetworkFailure::unavailable)?;
        let session = nm::Session {
            checkpoint: link.checkpoint.clone(),
            active,
        };
        tokio::time::timeout(CALL_TIMEOUT, nm::refresh(&self.bus, &session))
            .await
            .map_err(|_| NetworkFailure::timeout())?
            .map_err(|_| NetworkFailure::unavailable())?;
        if let Some(peer) = ACTIVE_PEER
            .lock()
            .map_err(|_| NetworkFailure::unavailable())?
            .as_mut()
        {
            peer.valid_until = Instant::now() + REFRESH_INTERVAL + CALL_TIMEOUT;
        }
        Ok(())
    }

    async fn tick(
        &mut self,
        backoff: &mut Backoff,
        refresh_at: &mut Instant,
        kernel_has_default_route: bool,
    ) -> Result<(), NetworkFailure> {
        let peers = self
            .resources
            .directory
            .snapshot()
            .map_err(|_| NetworkFailure::unavailable())?;
        backoff.0.retain(|key, _| {
            peers.iter().any(|peer| {
                &peer.device_public_key == key && matches!(peer.state, PeerState::Failed { .. })
            })
        });
        if self.link.is_some() {
            let peer_state = ACTIVE_PEER.lock().ok().and_then(|peer| {
                peer.as_ref().and_then(|p| {
                    peers
                        .iter()
                        .find(|entry| entry.device_public_key == p.key)
                        .map(|entry| (entry.device_public_key.clone(), entry.state.clone()))
                })
            });
            let Some((peer_key, peer_state)) = peer_state else {
                return self.stop().await;
            };
            let link = self.link.as_mut().expect("active nearby link");
            if matches!(peer_state, PeerState::Ready) {
                link.contact_deadline = None;
            } else if Instant::now()
                >= *link
                    .contact_deadline
                    .get_or_insert_with(|| Instant::now() + ATTEMPT_TIMEOUT)
            {
                // Signed LAN calls prove peer contact.
                // korrid restores home Wi-Fi if those calls fail.
                backoff.defer(&peer_key, Instant::now());
                return self.stop().await;
            }
            if Instant::now() >= *refresh_at {
                self.refresh().await?;
                *refresh_at = Instant::now() + REFRESH_INTERVAL;
            }
            return Ok(());
        }
        if let Some(peer) = backoff.peer(&peers, Instant::now()) {
            let local = self
                .resources
                .credentials
                .public_key()
                .map_err(|_| NetworkFailure::unavailable())?;
            // A default route blocks AP creation, not scans or connections.
            // A VPN can install a route outside NM.
            let can_start = local.as_str() < peer
                && !kernel_has_default_route
                && matches!(
                    tokio::time::timeout(CALL_TIMEOUT, nm::has_default_route(&self.bus)).await,
                    Ok(Ok(false))
                );
            let result = if can_start {
                self.start(peer).await
            } else {
                self.connect_nearby(peer).await
            };
            backoff.attempted(peer, Instant::now());
            result?;
            *refresh_at = Instant::now() + REFRESH_INTERVAL;
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), NetworkFailure> {
        *ACTIVE_PEER
            .lock()
            .map_err(|_| NetworkFailure::unavailable())? = None;
        if let Some(link) = &self.link {
            tokio::time::timeout(CALL_TIMEOUT, nm::restore(&self.bus, &link.checkpoint))
                .await
                .map_err(|_| NetworkFailure::timeout())?
                .map_err(|_| NetworkFailure::unavailable())?;
        }
        self.link = None;
        Ok(())
    }
}
impl Drop for NearbyLink {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE_PEER.lock() {
            *active = None;
        }
        // Drop does not perform async work. NM keeps the home rollback deadline.
    }
}

#[derive(Default)]
struct Backoff(BTreeMap<String, (u32, Instant)>);
impl Backoff {
    fn peer<'a>(&self, peers: &'a [PeerSnapshot], now: Instant) -> Option<&'a str> {
        peers
            .iter()
            .filter(|peer| matches!(peer.state, PeerState::Failed { .. }))
            .filter(|peer| {
                self.0
                    .get(&peer.device_public_key)
                    .is_none_or(|(_, next)| *next <= now)
            })
            .map(|peer| peer.device_public_key.as_str())
            .min()
    }
    fn attempted(&mut self, peer: &str, now: Instant) {
        let attempts = self
            .0
            .get(peer)
            .map_or(0, |(attempts, _)| *attempts)
            .saturating_add(1);
        self.0.insert(peer.into(), (attempts, now));
        self.defer(peer, now);
    }
    fn defer(&mut self, peer: &str, now: Instant) {
        let (attempts, next) = self.0.entry(peer.into()).or_insert((1, now));
        let seconds = (30u64 << (*attempts).min(5).saturating_sub(1)).min(300);
        *next = now + Duration::from_secs(seconds);
    }
}

/// korrid owns this task for the server lifetime.
/// The task only reads federation snapshots.
/// Peer transport uses the existing LAN path.
pub async fn run(resources: FederationResources, mut stopped: oneshot::Receiver<()>) {
    let Ok(Ok(bus)) = tokio::time::timeout(CALL_TIMEOUT, Connection::system()).await else {
        log::warn!("nearby link: system bus is unavailable");
        return;
    };
    let mut link = NearbyLink {
        bus,
        resources,
        link: None,
    };
    let radio = radio_operation().lock_owned().await;
    let recover = tokio::time::timeout(CALL_TIMEOUT, nm::recover_stale(&link.bus)).await;
    drop(radio);
    if !matches!(recover, Ok(Ok(()))) {
        log::warn!("nearby link: startup restoration failed. NM keeps its rollback deadline");
        return;
    }
    let mut backoff = Backoff::default();
    let mut refresh_at = Instant::now();
    loop {
        let kernel_has_default_route = crate::federation::interfaces::interface_addresses()
            .map(|addresses| addresses.iter().any(|address| address.has_default_route))
            .unwrap_or(true);
        tokio::select! {
            _ = &mut stopped => break,
            result = link.tick(&mut backoff, &mut refresh_at, kernel_has_default_route) => {
                if result.is_err() {
                    log::warn!("nearby link: attempt failed. korrid will restore the home network");
                    if link.stop().await.is_err() {
                        // Clearing the candidate prevents further renewal.
                        // korrid retries cleanup. NM also keeps a deadline.
                        log::warn!("nearby link: restoration needs more time. NM keeps its rollback deadline");
                    }
                }
            }
        }
        tokio::select! {
            _ = &mut stopped => break,
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
        }
    }
    if link.stop().await.is_err() {
        log::warn!(
            "nearby link: shutdown restoration needs more time. NM keeps its rollback deadline"
        );
    }
}

#[cfg(test)]
#[path = "nearby_tests.rs"]
mod tests;
