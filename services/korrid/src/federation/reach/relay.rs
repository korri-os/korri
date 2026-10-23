//! An embedded relay, never a third-party traffic service. Its authenticated
//! Ed25519 clients must have a current, device-signed membership binding.
use super::policy::{RelayEnvironment, RelayFacts};
use crate::federation::{interfaces::InterfaceAddress, FederationDirectory};
use iroh::EndpointId;
use iroh_relay::server::{Access, AccessControl, ClientRequest, RelayConfig, Server, ServerConfig};
use std::{
    collections::BTreeMap,
    fmt,
    net::{IpAddr, SocketAddr},
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{task::JoinSet, time::timeout};

pub(super) const FACT_POLL: Duration = Duration::from_secs(5);
pub(super) type Bindings = Arc<RwLock<BTreeMap<EndpointId, String>>>;
#[derive(Clone)]
pub(super) struct Membership {
    pub directory: FederationDirectory,
    pub bindings: Bindings,
    pub local: EndpointId,
}
impl fmt::Debug for Membership {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FederationRelayMembership")
    }
}
impl Membership {
    pub fn allows(&self, endpoint: EndpointId) -> bool {
        let Ok(identity) = self.directory.credentials.identity_snapshot() else {
            return false;
        };
        if !matches!(
            identity.state(),
            crate::identity::IdentityState::Owned { .. }
        ) {
            return false;
        }
        let Some(local_device) = identity.device_public_key() else {
            return false;
        };
        if self.directory.peer_is_revoked(local_device).unwrap_or(true) {
            return false;
        }
        if endpoint == self.local {
            return true;
        }
        let device = self
            .bindings
            .read()
            .ok()
            .and_then(|b| b.get(&endpoint).cloned());
        let Some(device) = device else { return false };
        self.directory
            .snapshot()
            .is_ok_and(|peers| peers.iter().any(|p| p.device_public_key == device))
    }
}
impl AccessControl for Membership {
    async fn on_connect(&self, request: &ClientRequest) -> Access {
        if self.allows(request.endpoint_id()) {
            Access::Allow
        } else {
            Access::Deny { reason: None }
        }
    }
}

pub(super) fn is_tailscale(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => u32::from(ip) & 0xffc00000 == 0x64400000,
        IpAddr::V6(ip) => {
            ip.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0]
                || ip.to_ipv4_mapped().is_some_and(|v| is_tailscale(v.into()))
        }
    }
}
pub(super) fn allowed_relay_ip(ip: IpAddr) -> bool {
    if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() || is_tailscale(ip) {
        return false;
    }
    match ip {
        IpAddr::V4(ip) => !ip.is_broadcast() && !ip.is_link_local() && ip.octets()[0] != 0,
        IpAddr::V6(ip) => !ip.is_unicast_link_local() && ip.to_ipv4_mapped().is_none(),
    }
}
pub(super) fn listener_ips(addresses: &[InterfaceAddress]) -> Vec<IpAddr> {
    addresses
        .iter()
        .filter(|a| a.interface != "tailscale0" && allowed_relay_ip(a.address))
        .filter(|a| match a.address {
            IpAddr::V4(ip) => a.has_default_route && ip.is_private(),
            // ULA is not reachable from other networks. Do not advertise it
            // as the globally reachable IPv6 relay case.
            IpAddr::V6(ip) => !ip.is_unique_local(),
        })
        .map(|a| a.address)
        .take(8)
        .collect()
}

pub(super) struct Fleet {
    servers: Vec<Server>,
    pub addresses: Vec<SocketAddr>,
}
impl Fleet {
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
            addresses: Vec::new(),
        }
    }
    pub async fn update(
        &mut self,
        facts: RelayFacts,
        ips: &[IpAddr],
        access: &Membership,
        preferred_port: u16,
    ) {
        if !facts.eligible()
            || self
                .addresses
                .iter()
                .map(SocketAddr::ip)
                .collect::<Vec<_>>()
                != ips
        {
            self.stop().await;
        }
        if !facts.eligible() || !self.servers.is_empty() {
            return;
        }
        for ip in ips {
            let spawn = |port| {
                let mut relay = RelayConfig::new(SocketAddr::new(*ip, port));
                relay.access = Arc::new(access.clone());
                // The SDK default caches one million keys. This service has
                // the directory's existing peer bound, not a public audience.
                relay.key_cache_capacity = Some(crate::federation::MAX_PEERS);
                let mut config = ServerConfig::default();
                config.relay = Some(relay);
                Server::spawn(config)
            };
            let first = spawn(preferred_port).await;
            let fallback = first
                .as_ref()
                .is_err_and(|error| preferred_port != 0 && port_unavailable(error));
            let result = if fallback { spawn(0).await } else { first };
            if let Ok(server) = result {
                if let Some(address) = server.http_addr() {
                    if fallback {
                        log::info!("federation relay uses an ephemeral TCP port at {address}; requested TCP port {preferred_port} is occupied or not permitted");
                    } else {
                        log::info!("federation relay bound TCP listener at {address}");
                    }
                    self.addresses.push(address);
                    self.servers.push(server);
                }
            }
        }
    }
    pub async fn stop(&mut self) {
        self.addresses.clear();
        let mut stopping = JoinSet::new();
        for server in self.servers.drain(..) {
            stopping.spawn(async move {
                let _ = timeout(Duration::from_secs(5), server.shutdown()).await;
            });
        }
        while stopping.join_next().await.is_some() {}
    }
}
impl Drop for Fleet {
    fn drop(&mut self) {
        self.servers.clear();
    }
}

fn port_unavailable(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    for _ in 0..16 {
        let Some(error) = current else { break };
        if error.downcast_ref::<std::io::Error>().is_some_and(|e| {
            matches!(
                e.kind(),
                std::io::ErrorKind::AddrInUse | std::io::ErrorKind::PermissionDenied
            )
        }) {
            return true;
        }
        current = error.source();
    }
    false
}

pub(super) async fn supervise(
    environment: Arc<dyn RelayEnvironment>,
    access: Membership,
    ips: Vec<IpAddr>,
    advertised: Arc<RwLock<Vec<SocketAddr>>>,
    preferred_port: u16,
) {
    let mut fleet = Fleet::new();
    let mut members = Vec::new();
    loop {
        let current = access
            .directory
            .snapshot()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.device_public_key)
            .collect::<Vec<_>>();
        if current != members {
            advertised.write().expect("relay addresses").clear();
            fleet.stop().await;
            members = current;
        }
        let mut facts = timeout(Duration::from_secs(2), environment.facts())
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        if !access.allows(access.local) {
            facts = RelayFacts::default();
        }
        // Do not leave a stale eligible answer visible while shutdown waits.
        if !facts.eligible() {
            advertised.write().expect("relay addresses").clear();
        }
        // Revocation changes close existing sessions as well as denying new
        // clients. A full restart costs connectivity but avoids stale authority.
        fleet.update(facts, &ips, &access, preferred_port).await;
        *advertised.write().expect("relay addresses") = fleet.addresses.clone();
        tokio::time::sleep(FACT_POLL).await;
    }
}
