//! Disposable iroh hints, never federation authority. The document is a list
//! of iroh's own EndpointAddr values; transport addresses stay opaque here.
use super::*;
use crate::federation::{
    endpoint_from_event, is_authorized, store, DirectoryState, FederationError,
};
use iroh::{address_lookup::memory::MemoryLookup, EndpointAddr, EndpointId};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const FILE: &str = "federation/iroh-addresses.json";
const MAX_ADDRESSES: usize = 16;
const MAX_BYTES: usize = 8 * 1024 * 1024;

pub(super) struct Cache {
    path: PathBuf,
    pub(super) lookup: MemoryLookup,
    entries: Mutex<Entries>,
}
struct Entries {
    peers: BTreeMap<EndpointId, EndpointAddr>,
    dirty: bool,
}
impl Cache {
    fn load(root: &Path, allowed: &BTreeSet<EndpointId>) -> Self {
        let path = root.join(FILE);
        let loaded = read(&path);
        let dirty = loaded.is_err() && path.exists();
        let raw = loaded.unwrap_or_default();
        let mut peers = BTreeMap::new();
        let mut pruned = false;
        for addr in raw {
            if allowed.contains(&addr.id) {
                peers.insert(addr.id, addr);
            } else {
                pruned = true;
            }
        }
        let lookup = MemoryLookup::from_endpoint_info(peers.values().cloned());
        Self {
            path,
            lookup,
            entries: Mutex::new(Entries {
                peers,
                dirty: dirty || pruned,
            }),
        }
    }
    fn update(&self, allowed: &BTreeSet<EndpointId>, addr: Option<EndpointAddr>) -> io::Result<()> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| io::Error::other("address cache lock"))?;
        let removed = entries
            .peers
            .keys()
            .copied()
            .filter(|id| !allowed.contains(id))
            .collect::<Vec<_>>();
        for id in removed {
            entries.peers.remove(&id);
            self.lookup.remove_endpoint_info(id);
            entries.dirty = true;
        }
        if let Some(addr) = addr.filter(|addr| allowed.contains(&addr.id)) {
            if entries.peers.get(&addr.id) != Some(&addr) {
                self.lookup.set_endpoint_info(addr.clone());
                entries.peers.insert(addr.id, addr);
                entries.dirty = true;
            }
        }
        if entries.dirty {
            write(&self.path, &entries.peers.values().collect::<Vec<_>>())?;
            entries.dirty = false;
        }
        Ok(())
    }
}
fn read(path: &Path) -> io::Result<Vec<EndpointAddr>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.nlink() != 1
        || metadata.permissions().mode() & 0o7777 != 0o600
        || metadata.len() > MAX_BYTES as u64
    {
        return Err(io::Error::other("invalid address cache file"));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(io::Error::other("address cache too large"));
    }
    let peers: Vec<EndpointAddr> = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if peers.len() > crate::federation::MAX_PEERS
        || peers.iter().any(|peer| peer.addrs.len() > MAX_ADDRESSES)
    {
        return Err(io::Error::other("address cache exceeds bounds"));
    }
    Ok(peers)
}
fn write(path: &Path, peers: &[&EndpointAddr]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("address cache directory"))?;
    store::validate_ancestors(parent).map_err(io::Error::other)?;
    store::private_directory(parent).map_err(io::Error::other)?;
    let bytes = serde_json::to_vec(peers).map_err(io::Error::other)?;
    if bytes.len() > MAX_BYTES {
        return Err(io::Error::other("address cache too large"));
    }
    let temporary = parent.join(format!(".iroh-addresses.{}.tmp", rand::random::<u64>()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
fn allowed(
    state: &DirectoryState,
    identity: &crate::identity::DeviceIdentity,
    authorization: &crate::authorization::Authorization,
) -> Result<BTreeSet<EndpointId>, FederationError> {
    let mut allowed = BTreeSet::new();
    for peer in state.memory.peers.values() {
        if is_authorized(identity, authorization, &peer.owner_statement)? {
            if let Some(event) = &peer.endpoint_event {
                let record = endpoint_from_event(identity, event, None)?;
                allowed.insert(
                    record
                        .endpoint_id
                        .parse()
                        .map_err(|_| FederationError::Identity)?,
                );
            }
        }
    }
    Ok(allowed)
}
impl FederationDirectory {
    pub(in crate::federation) fn start_address_cache(&self) -> MemoryLookup {
        // This is a current-state read, not asynchronous peer work: a concurrent
        // roster update must not make startup permanently lose its provider.
        let result = (|| -> Result<MemoryLookup, FederationError> {
            let state = self.state.lock().map_err(|_| FederationError::Storage)?;
            self.credentials
                .with_identity(|identity| {
                    crate::federation::check_binding(&state.memory, identity)?;
                    state.store.validate()?;
                    crate::federation::validate_authorization_paths(&self.root)?;
                    let allowed = allowed(&state, identity, &self.authorization)?;
                    let cache = self
                        .reach
                        .address_cache
                        .get_or_init(|| Arc::new(Cache::load(&self.root, &allowed)));
                    if cache.update(&allowed, None).is_err() {
                        log::warn!("iroh address cache could not be written");
                    }
                    Ok(cache.lookup.clone())
                })
                .map_err(|_| FederationError::Identity)?
        })();
        result.unwrap_or_else(|_| {
            log::warn!("iroh address cache unavailable");
            self.reach
                .address_cache
                .get_or_init(|| Arc::new(Cache::load(&self.root, &BTreeSet::new())))
                .lookup
                .clone()
        })
    }
    pub(in crate::federation) fn clear_address_cache(&self) {
        if let Some(cache) = self.reach.address_cache.get() {
            if cache.update(&BTreeSet::new(), None).is_err() {
                log::warn!("iroh address cache could not be cleared");
            }
        }
    }
    pub(in crate::federation) fn retain_address_cache(
        &self,
        peers: &[crate::federation::PeerSnapshot],
    ) {
        if let Some(cache) = self.reach.address_cache.get() {
            // Snapshot already checked signatures and live authorization. Reuse
            // its derived records rather than decrypting every event again.
            let allowed = peers
                .iter()
                .filter_map(|peer| peer.remembered_endpoint.as_ref())
                .filter_map(|record| record.endpoint_id.parse().ok())
                .collect();
            if cache.update(&allowed, None).is_err() {
                log::warn!("iroh address cache could not be pruned");
            }
        }
    }
    // Called while the directory mutation holds its authority lock. An old
    // observation cannot reinsert a peer after a concurrent roster removal.
    pub(in crate::federation) fn prune_address_cache(
        &self,
        state: &DirectoryState,
        identity: &crate::identity::DeviceIdentity,
        authorization: &crate::authorization::Authorization,
    ) {
        if let Some(cache) = self.reach.address_cache.get() {
            let result = allowed(state, identity, authorization)
                .map_err(io::Error::other)
                .and_then(|allowed| cache.update(&allowed, None));
            if result.is_err() {
                log::warn!("iroh address cache could not be pruned");
            }
        }
    }
    pub(super) async fn capture_addresses(&self, connection: &Connection) {
        if connection.close_reason().is_some() {
            return;
        }
        let Some(endpoint) = self.reach.endpoint.get() else {
            return;
        };
        let Some(info) = endpoint.remote_info(connection.remote_id()).await else {
            return;
        };
        // Keep iroh's reporting order and opaque values. The sole truncation is
        // the owner-approved count bound; no transport is interpreted here.
        let addr = EndpointAddr::from_parts(
            info.id(),
            info.into_addrs()
                .take(MAX_ADDRESSES)
                .map(|addr| addr.into_addr()),
        );
        let result = self.begin_work().and_then(|token| {
            self.mutate(&token, |state, identity, authorization, _| {
                if let Some(cache) = self.reach.address_cache.get() {
                    cache
                        .update(&allowed(state, identity, authorization)?, Some(addr))
                        .map_err(|_| FederationError::Storage)?;
                }
                Ok(())
            })
        });
        if result.is_err() {
            log::warn!("iroh address cache observation could not be saved");
        }
    }
}
pub(super) async fn observe(binding: &Binding) {
    use futures::StreamExt;
    let mut paths = binding.connection.paths_stream();
    loop {
        tokio::select! {
            snapshot = paths.next() => { if snapshot.is_none() { return; } },
            _ = binding.bound.notified() => {},
        }
        if binding.device.get().is_some() {
            binding
                .directory
                .capture_addresses(&binding.connection)
                .await;
        }
    }
}

#[cfg(test)]
#[path = "address_cache_tests.rs"]
mod tests;
