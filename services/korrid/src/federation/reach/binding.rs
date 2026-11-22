//! In-band proof binds iroh's authenticated key to the existing peer signature.
use super::{tunnel, PeerConnection};
use crate::{
    authorization::{AuthorizationContext, Principal},
    federation::FederationDirectory,
};
use iroh::endpoint::Connection;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone)]
pub(crate) struct Binding {
    pub(super) directory: FederationDirectory,
    pub(super) connection: Connection,
    pub(super) responses: tunnel::Responses,
    pub(super) device: Arc<OnceLock<String>>,
    pub(super) proof: Arc<Mutex<()>>,
}
impl Binding {
    pub(crate) fn prove(
        &self,
        context: &AuthorizationContext,
        statement: Option<&str>,
    ) -> Result<(), crate::peer_rpc::PeerRpcError> {
        let AuthorizationContext::Peer(Principal::OwnerDevice {
            device_public_key, ..
        }) = context
        else {
            return Ok(());
        };
        let _proof = self
            .proof
            .lock()
            .map_err(|_| crate::peer_rpc::PeerRpcError::Storage)?;
        if self.device.get() == Some(device_public_key) && self.membership_is_current() {
            return Ok(());
        }
        if self
            .device
            .get()
            .is_some_and(|device| device != device_public_key)
        {
            return Err(crate::peer_rpc::PeerRpcError::Binding);
        }
        if let Some(statement) = statement {
            let token = self
                .directory
                .begin_work()
                .map_err(|_| crate::peer_rpc::PeerRpcError::Storage)?;
            self.directory
                .apply_membership(&token, &[statement.into()])
                .map_err(|_| crate::peer_rpc::PeerRpcError::Unauthorized)?;
        }
        if !self.directory.snapshot().is_ok_and(|peers| {
            peers
                .iter()
                .any(|peer| peer.device_public_key == *device_public_key)
        }) {
            return Err(crate::peer_rpc::PeerRpcError::Unauthorized);
        }
        self.register(device_public_key.clone())
    }
    pub(super) fn register(&self, device: String) -> Result<(), crate::peer_rpc::PeerRpcError> {
        if self.device.get_or_init(|| device.clone()) != &device {
            return Err(crate::peer_rpc::PeerRpcError::Binding);
        }
        let mut live = self.directory.reach.live.write().expect("peer connections");
        if let Some(old) = live.get(&device) {
            if old.connection.stable_id() == self.connection.stable_id() {
                return Ok(());
            }
            if old.connection.close_reason().is_none() {
                // Simultaneous dials converge on the connection initiated by the
                // smaller EndpointId. No paths or addresses participate.
                let preferred = self.directory.reach.endpoint.get().map(|endpoint| {
                    if endpoint.id() < self.connection.remote_id() {
                        iroh::endpoint::Side::Client
                    } else {
                        iroh::endpoint::Side::Server
                    }
                });
                if preferred.is_none_or(|side| {
                    old.connection.side() == side || self.connection.side() != side
                }) {
                    self.connection
                        .close(0u32.into(), b"duplicate peer connection");
                    return Err(crate::peer_rpc::PeerRpcError::Binding);
                }
                old.connection
                    .close(0u32.into(), b"duplicate peer connection");
            }
        }
        live.insert(
            device,
            PeerConnection {
                connection: self.connection.clone(),
                responses: self.responses.clone(),
            },
        );
        Ok(())
    }
    pub(super) fn membership_is_current(&self) -> bool {
        self.device.get().is_some_and(|device| {
            self.directory
                .snapshot()
                .is_ok_and(|peers| peers.iter().any(|peer| peer.device_public_key == *device))
        })
    }
}

impl Binding {
    pub(super) fn allows_streams(&self) -> bool {
        // The connection's one monitor checks revocations. No filesystem I/O per packet.
        self.device.get().is_some() && self.connection.close_reason().is_none()
    }
}
