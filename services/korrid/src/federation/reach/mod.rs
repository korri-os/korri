//! One authenticated iroh connection per peer. iroh owns discovery and paths.
mod address_cache;
mod binding;
mod datagrams;
pub(crate) use binding::Binding;
mod endpoint;
pub(crate) mod http;
mod tunnel;
mod viewer;
use crate::federation::FederationDirectory;
pub(crate) use endpoint::start;
use iroh::{endpoint::Connection, Endpoint};
use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex, RwLock, Weak},
};
pub(crate) use tunnel::NetplayForward;
pub(crate) use viewer::{prepare_peer_viewer, SelectedViewer};
pub(super) const ALPN: &[u8] = b"korri/peer/1";
#[derive(Clone, Default)]
pub(crate) struct Routes {
    live: Arc<RwLock<BTreeMap<String, PeerConnection>>>,
    admission: Arc<endpoint::Admission>,
    address_cache: Arc<std::sync::OnceLock<Arc<address_cache::Cache>>>,
    endpoint: Arc<tokio::sync::OnceCell<Endpoint>>,
    starting: Arc<std::sync::atomic::AtomicBool>,
    ready: Arc<tokio::sync::Notify>,
    router: Arc<tokio::sync::OnceCell<axum::Router>>,
    dialing: Arc<Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    selected: Arc<tokio::sync::Mutex<Option<Weak<Lease>>>>,
    retired: Arc<Mutex<Vec<tunnel::Forwards>>>,
    #[cfg(test)]
    bindings: Arc<RwLock<Option<tunnel::Bindings>>>,
}
#[derive(Clone)]
struct PeerConnection {
    connection: Connection,
    responses: tunnel::Responses,
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
    pub(crate) fn connection_gone(&self) -> bool {
        self.0.connection.close_reason().is_some()
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
    fn connection(&self, device: &str) -> Option<Connection> {
        self.live
            .read()
            .ok()?
            .get(device)
            .filter(|c| c.connection.close_reason().is_none())
            .map(|c| c.connection.clone())
    }
    fn stream_address(&self, device: &str) -> Option<String> {
        let lease = self.selected.try_lock().ok()?.as_ref()?.upgrade()?;
        (lease.device == device)
            .then(|| StreamLease(lease).address())
            .flatten()
    }
    fn clear(&self) {
        if let Ok(mut live) = self.live.write() {
            for c in live.values() {
                c.connection.close(0u32.into(), b"transport stopped");
            }
            live.clear();
        }
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
        let connection = self
            .connection(device)
            .ok_or_else(|| io::Error::other("peer connection is gone"))?;
        #[cfg(test)]
        let bindings = self
            .bindings
            .read()
            .expect("test bindings")
            .clone()
            .unwrap_or_default();
        #[cfg(not(test))]
        let bindings = tunnel::Bindings::default();
        let responses = self
            .live
            .read()
            .expect("peer connections")
            .get(device)
            .ok_or_else(|| io::Error::other("peer connection is gone"))?
            .responses
            .subscribe();
        let forwards = tunnel::Forwards::open(connection.clone(), bindings, responses).await?;
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
        self.reach.stream_address(device)
    }
    pub(crate) fn has_peer_connection(&self, device: &str) -> bool {
        self.reach.connection(device).is_some()
    }
    pub(crate) async fn activate_stream(&self, device: &str) -> io::Result<Option<StreamLease>> {
        self.reach_connection(device).await?;
        self.reach.activate(device).await
    }
    pub(crate) async fn netplay_forward(&self, device: &str) -> io::Result<NetplayForward> {
        NetplayForward::open(self.reach_connection(device).await?).await
    }
    #[cfg(target_os = "android")]
    pub(crate) async fn network_change(&self) {
        if let Some(endpoint) = self.reach.endpoint.get() {
            endpoint.network_change().await;
        }
    }
    pub(crate) fn own_endpoint_event(&self, peer: &str) -> io::Result<String> {
        let endpoint = self
            .reach
            .endpoint
            .get()
            .ok_or_else(|| io::Error::other("peer transport is not active"))?;
        let record = self
            .own_endpoint_record(endpoint.id())
            .map_err(io::Error::other)?;
        let identity = self
            .credentials
            .identity_snapshot()
            .map_err(io::Error::other)?;
        crate::relay::encode_endpoint(&identity, peer, &record, record.issued_at)
            .map_err(io::Error::other)
    }
    pub(crate) async fn reach_connection(&self, device: &str) -> io::Result<Connection> {
        let peer = self
            .snapshot()
            .map_err(io::Error::other)?
            .into_iter()
            .find(|p| p.device_public_key == device)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "peer is not a federation member",
                )
            })?;
        let dialing = self
            .reach
            .dialing
            .lock()
            .expect("peer dial locks")
            .entry(device.into())
            .or_default()
            .clone();
        let _dialing = dialing.lock().await;
        {
            let mut live = self.reach.live.write().expect("peer connections");
            if let Some(peer) = live
                .get(device)
                .filter(|peer| peer.connection.close_reason().is_none())
            {
                return Ok(peer.connection.clone());
            }
            // The close observer may not have run yet. Evict here too, under
            // the per-peer dial lock, so concurrent demand shares one new dial.
            live.remove(device);
        }
        let ready = self.reach.ready.notified();
        tokio::pin!(ready);
        ready.as_mut().enable();
        if self.reach.endpoint.get().is_none()
            && self
                .reach
                .starting
                .load(std::sync::atomic::Ordering::Acquire)
        {
            ready.await;
        }
        let endpoint = self.reach.endpoint.get().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotConnected, "peer transport is not active")
        })?;
        let id = peer
            .remembered_endpoint
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "peer endpoint key is not known")
            })?
            .endpoint_id
            .parse::<iroh::EndpointId>()
            .map_err(io::Error::other)?;
        let connection = endpoint.connect(id, ALPN).await.map_err(io::Error::other)?;
        endpoint::admit(self.clone(), Some(device.to_owned()), connection.clone());
        self.reach
            .connection(device)
            .ok_or_else(|| io::Error::other("peer connection is gone"))
    }
    pub(crate) fn unreachable_sentence(&self, device: &str) -> String {
        format!("Can't reach {device} from this network right now")
    }
}
#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod path_change_tests;
