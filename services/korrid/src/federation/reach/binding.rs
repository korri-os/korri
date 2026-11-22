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
    // Serializes proof and holds the unbound budget until it succeeds.
    pub(super) proof: Arc<Mutex<Option<tokio::sync::OwnedSemaphorePermit>>>,
    pub(super) member: Arc<Mutex<Option<tokio::sync::OwnedSemaphorePermit>>>,
}
impl Binding {
    pub(crate) fn prove(
        &self,
        context: &AuthorizationContext,
        statement: Option<&str>,
        endpoint_event: Option<&str>,
    ) -> Result<(), crate::peer_rpc::PeerRpcError> {
        let AuthorizationContext::Peer(Principal::OwnerDevice {
            device_public_key,
            owner_public_key,
            ..
        }) = context
        else {
            return Ok(());
        };
        let mut proof = self
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
        let event = endpoint_event.ok_or(crate::peer_rpc::PeerRpcError::Binding)?;
        let record = self
            .directory
            .credentials
            .with_identity(|identity| {
                crate::relay::decode_endpoint(
                    identity,
                    event,
                    device_public_key,
                    identity
                        .device_public_key()
                        .ok_or(crate::relay::RelayError::Identity)?,
                    (self.directory.clock)(),
                )
            })
            .map_err(|_| crate::peer_rpc::PeerRpcError::Storage)?
            .map_err(|_| crate::peer_rpc::PeerRpcError::Binding)?;
        if record.endpoint_id != self.connection.remote_id().to_string()
            || record.owner_public_key != *owner_public_key
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
        self.directory
            .apply_endpoint_event(
                &self
                    .directory
                    .begin_work()
                    .map_err(|_| crate::peer_rpc::PeerRpcError::Storage)?,
                event,
            )
            .map_err(|_| crate::peer_rpc::PeerRpcError::Binding)?;
        if !self.directory.snapshot().is_ok_and(|peers| {
            peers.iter().any(|peer| {
                peer.device_public_key == *device_public_key
                    && peer.remembered_endpoint.as_ref().is_some_and(|record| {
                        record.endpoint_id == self.connection.remote_id().to_string()
                    })
            })
        }) {
            return Err(crate::peer_rpc::PeerRpcError::Binding);
        }
        self.register(device_public_key.clone())?;
        proof.take();
        Ok(())
    }
    pub(super) fn register(&self, device: String) -> Result<(), crate::peer_rpc::PeerRpcError> {
        let mut member = self
            .member
            .lock()
            .map_err(|_| crate::peer_rpc::PeerRpcError::Storage)?;
        if member.is_none() {
            *member = Some(
                self.directory
                    .reach
                    .admission
                    .members
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| crate::peer_rpc::PeerRpcError::Storage)?,
            );
        }
        drop(member);
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
