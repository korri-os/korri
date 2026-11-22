//! Endpoint lifetime and authenticated incoming connections, not path management.
use super::{tunnel, Binding, Routes, ALPN};
use crate::federation::{FederationDirectory, PeerState};
#[cfg(not(test))]
use iroh::endpoint::presets::N0;
use iroh::Endpoint;
use std::{path::Path, sync::Arc};
use tokio::task::JoinHandle;

#[derive(Clone)]
pub(crate) struct ReachLifetime(Arc<Lifetime>);
struct Lifetime {
    task: JoinHandle<()>,
    routes: Routes,
}
impl Drop for Lifetime {
    fn drop(&mut self) {
        self.task.abort();
        self.routes
            .starting
            .store(false, std::sync::atomic::Ordering::Release);
        self.routes.ready.notify_waiters();
        self.routes.clear();
        if let Some(endpoint) = self.routes.endpoint.get().cloned() {
            tokio::spawn(async move {
                endpoint.close().await;
            });
        }
    }
}
impl std::fmt::Debug for ReachLifetime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ReachLifetime")
            .field(&Arc::strong_count(&self.0))
            .finish()
    }
}
pub(crate) fn start(
    root: &Path,
    directory: FederationDirectory,
    peer_router: axum::Router,
) -> Option<ReachLifetime> {
    let runtime = tokio::runtime::Handle::try_current().ok()?;
    let key = crate::identity::iroh::load_or_create(root).ok()?;
    let routes = directory.reach.clone();
    if routes.endpoint.get().is_some()
        || routes
            .starting
            .swap(true, std::sync::atomic::Ordering::AcqRel)
    {
        return None;
    }
    // Reuse the daemon's configured port for UDP so the firewall can admit
    // offline LAN dials too. Android has no Nix firewall and uses an ephemeral port.
    let port = if cfg!(test) {
        0
    } else {
        std::env::var("KORRID_ADDRESS")
            .ok()
            .and_then(|address| address.parse::<std::net::SocketAddr>().ok())
            .map_or(0, |address| address.port())
    };
    let task = runtime.spawn(async move {
        #[cfg(not(test))]
        let builder = Endpoint::builder(N0);
        #[cfg(test)]
        let builder = Endpoint::builder(iroh::endpoint::presets::Minimal).relay_mode(iroh::RelayMode::Disabled);
        let endpoint = match builder
            .secret_key(key)
            .alpns(vec![ALPN.to_vec()])
            .bind_addr(std::net::SocketAddr::from(([0, 0, 0, 0], port))).expect("IPv4 wildcard")
            .bind_addr(std::net::SocketAddr::from(([0u16; 8], port))).expect("IPv6 wildcard")
            .address_lookup(iroh_mdns_address_lookup::MdnsAddressLookup::builder())
            .bind().await {
                Ok(endpoint) => endpoint,
                Err(_) => {
                    log::error!("peer endpoint could not start");
                    directory.reach.starting.store(false, std::sync::atomic::Ordering::Release);
                    directory.reach.ready.notify_waiters();
                    return;
                }
            };
        let _ = directory.reach.router.set(peer_router);
        let _ = directory.reach.endpoint.set(endpoint.clone());
        directory.reach.starting.store(false, std::sync::atomic::Ordering::Release);
        directory.reach.ready.notify_waiters();
        let mut tasks = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                incoming = endpoint.accept(), if tasks.len() < 64 => {
                    let Some(incoming) = incoming else { break; };
                    let directory = directory.clone();
                    tasks.spawn(async move {
                        let Ok(Ok(connection)) = tokio::time::timeout(std::time::Duration::from_secs(10), incoming).await else { return; };
                        let member = directory.snapshot().ok().and_then(|peers| peers.into_iter().find(|peer| {
                            peer.remembered_endpoint.as_ref().is_some_and(|record| record.endpoint_id == connection.remote_id().to_string())
                        }));
                        let alive = connection.clone();
                        admit(directory, member.map(|peer| peer.device_public_key), connection);
                        alive.closed().await;
                    });
                }
                _ = tasks.join_next(), if !tasks.is_empty() => {}
            }
        }
    });
    Some(ReachLifetime(Arc::new(Lifetime { task, routes })))
}

pub(super) fn admit(
    directory: FederationDirectory,
    device: Option<String>,
    connection: iroh::endpoint::Connection,
) {
    let responses = tokio::sync::broadcast::channel(64).0;
    let binding = Binding {
        directory: directory.clone(),
        connection: connection.clone(),
        responses: responses.clone(),
        device: Arc::default(),
        proof: Arc::default(),
    };
    if let Some(device) = device {
        if binding.register(device).is_err() {
            return;
        }
    }
    let router = directory
        .reach
        .router
        .get()
        .cloned()
        .expect("peer router")
        .layer(axum::Extension(binding.clone()));
    tokio::spawn(async move {
        log::info!("peer iroh connection established");
        let membership = async {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                if binding.device.get().is_some() && !binding.membership_is_current() {
                    connection.close(1u32.into(), b"federation membership changed");
                    break;
                }
            }
        };
        tokio::select! {
            _ = tunnel::serve(connection.clone(), router, responses, Some(binding.clone())) => {},
            _ = connection.closed() => {},
            _ = membership => {},
        }
        // One completion, no reconnect or viewer recovery. Keep the closed entry.
        if let Some(device) = binding.device.get() {
            let current = directory
                .reach
                .live
                .read()
                .expect("peer connections")
                .get(device)
                .is_some_and(|peer| peer.connection.stable_id() == connection.stable_id());
            if !current {
                return;
            }
            if let Ok(token) = directory.begin_peer_work(device) {
                let _ = directory.set_peer_state(
                    &token,
                    device,
                    PeerState::Failed {
                        error: "Peer connection is gone".into(),
                    },
                );
            }
        }
        log::info!("peer iroh connection gone");
    });
}
