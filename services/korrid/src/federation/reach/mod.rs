//! Internet reachability sits below signed peer HTTP. The directory owns live
//! routes. The existing selected remote session owns the one local forward set.
mod controller;
mod datagrams;
pub(crate) mod http;
pub(crate) mod nearby_seam;
mod policy;
mod recovery;
mod relay;
mod settings;
pub mod signal;
mod stun;
mod tunnel;
pub(crate) use tunnel::NetplayForward;
mod viewer;

// The registry uses this boundary to prepare a viewer and retain its lease.
pub(crate) use policy::RelayEnvironment;
#[cfg(target_os = "android")]
pub(crate) use policy::RelayFacts;
pub(crate) use viewer::{prepare_peer_viewer, SelectedViewer};

pub(crate) type InterfaceReader =
    Arc<dyn Fn() -> io::Result<Vec<super::interfaces::InterfaceAddress>> + Send + Sync>;

/// Live platform observations and public product policy, never persisted config.
#[derive(Clone)]
pub(crate) struct Environment {
    pub interfaces: InterfaceReader,
    pub relay: Arc<dyn RelayEnvironment>,
    pub relays: Option<crate::relay::RelayList>,
}
impl Environment {
    pub(crate) fn linux() -> Self {
        Self {
            interfaces: Arc::new(super::interfaces::interface_addresses),
            relay: Arc::new(policy::LinuxEnvironment::new()),
            relays: None,
        }
    }
}

use crate::federation::FederationDirectory;
use iroh::endpoint::Connection;
use std::{
    collections::BTreeMap,
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock, Weak,
    },
    time::Duration,
};
use tokio::sync::Notify;
pub(super) const ALPN: &[u8] = b"korri/reach/1";
pub(super) struct LiveRoute {
    connection: Connection,
}
#[derive(Clone, Default)]
pub(crate) struct Routes {
    live: Arc<RwLock<BTreeMap<String, LiveRoute>>>,
    recovery: Arc<Mutex<recovery::Recovery>>,
    changed: Arc<Notify>,
    active: Arc<AtomicBool>,
    selected: Arc<tokio::sync::Mutex<Option<Weak<Lease>>>>,
    retired: Arc<Mutex<Vec<tunnel::Forwards>>>,
    #[cfg(test)]
    bindings: Arc<RwLock<Option<tunnel::Bindings>>>,
}
struct Lease {
    device: String,
    connection: Connection,
    forwards: Mutex<Option<tunnel::Forwards>>,
    retired: Arc<Mutex<Vec<tunnel::Forwards>>>,
}
impl Lease {
    fn release(&self) {
        if let Some(mut forwards) = self.forwards.lock().expect("stream forwards").take() {
            forwards.abort();
            self.retired
                .lock()
                .expect("retired forwards")
                .push(forwards);
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.release();
    }
}
/// Clones share one lease. Explicit release closes it for every clone. Dropping
/// the last clone also frees it, including a failed or cancelled preparation.
#[derive(Clone)]
pub(crate) struct StreamLease(Arc<Lease>);
impl std::fmt::Debug for StreamLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StreamForwardLease")
    }
}
impl PartialEq for StreamLease {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for StreamLease {}
impl StreamLease {
    pub(crate) fn release(&self) {
        self.0.release();
    }
    pub(crate) fn address(&self) -> Option<String> {
        if self.0.connection.close_reason().is_some() {
            return None;
        }
        self.0
            .forwards
            .lock()
            .ok()?
            .as_ref()
            .map(|f| f.address.clone())
    }
}
impl Routes {
    fn owns_forward_origin(&self, origin: &str) -> bool {
        let owns = |forward: &tunnel::Forwards| {
            nearby_seam::moonlight_control_origin(&forward.address).as_deref() == Some(origin)
        };
        // Do not probe an address while another task changes forward ownership.
        let Ok(selected) = self.selected.try_lock() else {
            return true;
        };
        if let Some(lease) = selected.as_ref().and_then(Weak::upgrade) {
            let forwards = lease.forwards.lock().expect("stream forwards");
            if forwards.as_ref().is_some_and(owns) {
                return true;
            }
        }
        self.retired
            .lock()
            .expect("retired forwards")
            .iter()
            .any(owns)
    }

    pub(super) fn awaiting_route(&self, device: &str) -> bool {
        self.active.load(Ordering::Acquire) && self.connection(device).is_none()
    }
    fn connection(&self, device: &str) -> Option<Connection> {
        self.live
            .read()
            .ok()?
            .get(device)
            .filter(|r| r.connection.close_reason().is_none())
            .map(|r| r.connection.clone())
    }
    fn stream_address(&self, device: &str) -> Option<String> {
        let lease = self.selected.try_lock().ok()?.as_ref()?.upgrade()?;
        if lease.device != device {
            return None;
        }
        StreamLease(lease).address()
    }
    fn clear(&self) {
        self.clear_connections(true);
    }
    fn clear_connections(&self, forget_losses: bool) {
        self.recovery
            .lock()
            .expect("reach recovery")
            .stop(forget_losses);
        if let Ok(selected) = self.selected.try_lock() {
            if let Some(lease) = selected.as_ref().and_then(Weak::upgrade) {
                StreamLease(lease).release();
            }
        }
        if let Ok(mut routes) = self.live.write() {
            for route in routes.values() {
                route
                    .connection
                    .close(0u32.into(), b"network or authority changed");
            }
            routes.clear();
        }
        self.changed.notify_waiters();
    }
    async fn wait(&self, device: &str) -> io::Result<Connection> {
        if let Some(c) = self.connection(device) {
            return Ok(c);
        }
        if !self.active.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "reach transport is not active",
            ));
        }
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let notified = self.changed.notified();
                if let Some(c) = self.connection(device) {
                    return c;
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::NotConnected, "no reachable federation route"))
    }
    async fn activate(&self, device: &str) -> io::Result<Option<StreamLease>> {
        let mut selected = self.selected.lock().await;
        if let Some(old) = selected.take().and_then(|old| old.upgrade()) {
            old.release();
        }
        // Wait for cancelled tasks before reusing ports, including an error-path Drop.
        let retired = std::mem::take(&mut *self.retired.lock().expect("retired forwards"));
        for forwards in retired {
            forwards.shutdown().await;
        }
        let Some(connection) = self.connection(device) else {
            return Ok(None);
        };
        #[cfg(test)]
        let bindings = self
            .bindings
            .read()
            .expect("test bindings")
            .clone()
            .unwrap_or_default();
        #[cfg(not(test))]
        let bindings = tunnel::Bindings::default();
        let forwards = tunnel::Forwards::open(connection.clone(), bindings).await?;
        let lease = Arc::new(Lease {
            device: device.into(),
            connection,
            forwards: Mutex::new(Some(forwards)),
            retired: self.retired.clone(),
        });
        *selected = Some(Arc::downgrade(&lease));
        Ok(Some(StreamLease(lease)))
    }
}
impl FederationDirectory {
    pub fn stream_address(&self, device: &str) -> Option<String> {
        if self
            .snapshot()
            .ok()?
            .iter()
            .any(|p| p.device_public_key == device)
        {
            self.reach.stream_address(device)
        } else {
            None
        }
    }
    pub(crate) fn has_iroh_route(&self, device: &str) -> bool {
        self.reach.connection(device).is_some()
    }
    pub(crate) async fn activate_stream(&self, device: &str) -> io::Result<Option<StreamLease>> {
        // Static and native-only peers need no federation transport authority.
        // Retire any previous forward selection, then preserve their address.
        if !self.has_iroh_route(device) {
            return self.reach.activate(device).await;
        }
        if !self
            .snapshot()
            .map_err(io::Error::other)?
            .iter()
            .any(|p| p.device_public_key == device)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "peer is not a federation member",
            ));
        }
        self.reach.activate(device).await
    }
    pub(crate) async fn netplay_forward(&self, device: &str) -> io::Result<NetplayForward> {
        NetplayForward::open(self.reach_connection(device).await?).await
    }
    pub(crate) async fn reach_connection(&self, device: &str) -> io::Result<Connection> {
        if !self
            .snapshot()
            .map_err(io::Error::other)?
            .iter()
            .any(|p| p.device_public_key == device)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "peer is not a federation member",
            ));
        }
        self.reach.wait(device).await
    }
    pub(crate) fn unreachable_sentence(&self, device: &str) -> String {
        let label = self
            .snapshot()
            .ok()
            .and_then(|peers| peers.into_iter().find(|p| p.device_public_key == device))
            .and_then(|p| p.current_endpoint.or(p.remembered_endpoint))
            .and_then(|e| e.label)
            .unwrap_or_else(|| device.into());
        format!("Can't reach {label} from this network right now")
    }
}
pub(crate) use controller::start;
#[cfg(test)]
pub(crate) mod tests;
