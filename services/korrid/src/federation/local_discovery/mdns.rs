//! Multicast DNS (DNS-SD) announce and browse on every network the device
//! joins. The instance and host names are random and change with each tag
//! window, so neither follows the device from place to place. A device's own
//! announcement matches nothing, because its tags are for other devices.
use super::{announcement_tags, matching_device, origins, record_found, window, Tag};
use crate::{
    federation::{interfaces, FederationDirectory},
    identity::DeviceIdentity,
    peer_rpc::{unix_time, PeerCredentials},
};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo, TxtProperty};
use std::{
    collections::BTreeSet,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

const SERVICE: &str = "_korri._tcp.local.";
/// How often korrid checks for a new window, address set or peer set.
const POLL: Duration = Duration::from_secs(5);

struct Daemon(ServiceDaemon);
impl Drop for Daemon {
    fn drop(&mut self) {
        // Sends goodbyes for the current announcement and stops the thread.
        let _ = self.0.shutdown();
    }
}

/// What the current announcement was made from. A change re-announces.
#[derive(PartialEq)]
struct Basis {
    window: u64,
    addresses: Vec<IpAddr>,
    peers: Vec<String>,
}

struct Announcer {
    basis: Option<Basis>,
    current: Option<String>,
}

pub async fn run(directory: FederationDirectory, credentials: PeerCredentials, bind: SocketAddr) {
    let daemon = match ServiceDaemon::new() {
        Ok(daemon) => Daemon(daemon),
        Err(_) => {
            log::warn!("local discovery: multicast DNS is unavailable");
            return;
        }
    };
    let Ok(events) = daemon.0.browse(SERVICE) else {
        log::warn!("local discovery: multicast DNS browse failed");
        return;
    };
    let mut announcer = Announcer {
        basis: None,
        current: None,
    };
    let mut poll = tokio::time::interval(POLL);
    loop {
        tokio::select! {
            _ = poll.tick() => announcer.update(&daemon.0, &directory, &credentials, bind),
            event = events.recv_async() => match event {
                Ok(ServiceEvent::ServiceResolved(service)) => {
                    let Some((identity, peers)) = known(&directory, &credentials) else {
                        continue;
                    };
                    let tags = service
                        .txt_properties
                        .iter()
                        .filter_map(|property| parse_tag(property.key()))
                        .collect::<BTreeSet<_>>();
                    if let Some(device) = matching_device(&identity, &peers, &tags, unix_time()) {
                        let addresses = service.addresses.iter().map(|ip| ip.to_ip_addr());
                        let found = origins(addresses, service.port);
                        if !found.is_empty() && super::found(&device).is_empty() {
                            log::info!(
                                "local discovery: found peer {} on a local network",
                                device.get(..8).unwrap_or_default()
                            );
                        }
                        record_found(&device, found);
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    log::warn!("local discovery: multicast DNS stopped");
                    return;
                }
            },
        }
    }
}

/// This device's identity and its known owner devices, in directory order.
fn known(
    directory: &FederationDirectory,
    credentials: &PeerCredentials,
) -> Option<(DeviceIdentity, Vec<String>)> {
    let identity = credentials.identity_snapshot().ok()?;
    let peers: Vec<_> = directory
        .snapshot()
        .ok()?
        .into_iter()
        .map(|peer| peer.device_public_key)
        .collect();
    (!peers.is_empty()).then_some((identity, peers))
}

fn parse_tag(key: &str) -> Option<Tag> {
    let mut tag = [0u8; 8];
    hex::decode_to_slice(key, &mut tag).ok()?;
    Some(tag)
}

impl Announcer {
    fn update(
        &mut self,
        daemon: &ServiceDaemon,
        directory: &FederationDirectory,
        credentials: &PeerCredentials,
        bind: SocketAddr,
    ) {
        let now = unix_time();
        let known = known(directory, credentials);
        let addresses: Vec<IpAddr> = interfaces::endpoints(bind)
            .unwrap_or_default()
            .iter()
            .filter_map(|origin| url::Url::parse(origin).ok())
            .filter_map(|url| match url.host()? {
                url::Host::Ipv4(ip) => Some(IpAddr::V4(ip)),
                url::Host::Ipv6(ip) => Some(IpAddr::V6(ip)),
                url::Host::Domain(_) => None,
            })
            .collect();
        let basis = known.as_ref().map(|(_, peers)| Basis {
            window: window(now),
            addresses: addresses.clone(),
            peers: peers.clone(),
        });
        if basis == self.basis {
            return;
        }
        self.basis = basis;
        if let Some(fullname) = self.current.take() {
            let _ = daemon.unregister(&fullname);
        }
        // Nobody to be found by, or no address a peer could use.
        let Some((identity, peers)) = known.filter(|_| !addresses.is_empty()) else {
            return;
        };
        let tags = announcement_tags(&identity, &peers, now);
        if tags.is_empty() {
            return;
        }
        let instance = hex::encode(rand::random::<[u8; 8]>());
        let properties: Vec<TxtProperty> = tags
            .iter()
            .map(|tag| TxtProperty::from(hex::encode(tag).as_str()))
            .collect();
        let Ok(service) = ServiceInfo::new(
            SERVICE,
            &instance,
            &format!("{instance}.local."),
            &addresses[..],
            bind.port(),
            properties,
        ) else {
            return;
        };
        let fullname = service.get_fullname().to_owned();
        if daemon.register(service).is_err() {
            log::warn!("local discovery: announcement failed");
            return;
        }
        self.current = Some(fullname);
    }
}
