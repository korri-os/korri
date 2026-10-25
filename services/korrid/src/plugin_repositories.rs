//! korrid's client for the root plugin host's admin socket.
//!
//! korrid holds no plugin authority. It forwards one typed request to
//! `korri-plugin serve-admin`, which runs the host's own checks, and returns
//! the host's answer or its refusal text unchanged.
use crate::plugin_admin::{
    PluginAdminFailure, PluginAdminRequest, PluginAdminResponse, CODE_REFUSED, CODE_UNAVAILABLE,
    MAX_MESSAGE_BYTES,
};
use crate::RpcFailure;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// These seven requests await the root admin socket. Its transfer/stall
/// bounds, not an RPC total deadline, determine when the operation finishes.
/// Shared by the person client and native peer transport.
pub fn is_admin_request(request: &crate::RpcRequest) -> bool {
    matches!(
        request,
        crate::RpcRequest::PluginRepositories(_)
            | crate::RpcRequest::PluginRepositoryAdd(_)
            | crate::RpcRequest::PluginRepositoryRemove(_)
            | crate::RpcRequest::PluginCatalog(_)
            | crate::RpcRequest::PluginInspect(_)
            | crate::RpcRequest::PluginInstall(_)
            | crate::RpcRequest::PluginSetEnabled(_)
    )
}

/// Set by korrid's NixOS module only when the device runs the plugin host.
pub const SOCKET_ENV: &str = "KORRID_PLUGIN_ADMIN_SOCKET";

#[derive(Clone, Debug)]
pub struct PluginAdminSocket {
    path: PathBuf,
}

impl PluginAdminSocket {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn from_env() -> Option<Self> {
        std::env::var_os(SOCKET_ENV).map(|path| Self::new(PathBuf::from(path)))
    }

    /// No deadline here: the host bounds catalog transfers (30 s, 4 MiB)
    /// and fails a stalled archive transfer, so it always answers.
    pub async fn request(
        &self,
        request: &PluginAdminRequest,
    ) -> Result<PluginAdminResponse, RpcFailure> {
        let unavailable = |message: &str| RpcFailure {
            code: CODE_UNAVAILABLE.into(),
            message: message.into(),
        };
        let bytes = serde_json::to_vec(request)
            .map_err(|_| unavailable("the plugin host request could not be encoded"))?;
        let mut stream = tokio::net::UnixStream::connect(&self.path)
            .await
            .map_err(|_| unavailable("the plugin host is unavailable"))?;
        stream
            .write_all(&bytes)
            .await
            .map_err(|_| unavailable("the plugin host request failed"))?;
        stream
            .shutdown()
            .await
            .map_err(|_| unavailable("the plugin host request failed"))?;
        let mut response = Vec::new();
        (&mut stream)
            .take(MAX_MESSAGE_BYTES as u64 + 1)
            .read_to_end(&mut response)
            .await
            .map_err(|_| unavailable("the plugin host did not answer"))?;
        if response.is_empty() {
            return Err(unavailable("the plugin host refused this connection"));
        }
        if response.len() > MAX_MESSAGE_BYTES {
            return Err(unavailable("the plugin host answer is too large"));
        }
        let response = serde_json::from_slice::<PluginAdminResponse>(&response)
            .map_err(|_| unavailable("the plugin host answer is invalid"))?;
        match response {
            PluginAdminResponse::Refused(PluginAdminFailure { code, message }) => {
                Err(RpcFailure { code, message })
            }
            other => Ok(other),
        }
    }
}

/// The plugin host is not part of this device.
pub fn not_configured() -> RpcFailure {
    RpcFailure {
        code: CODE_UNAVAILABLE.into(),
        message: "plugins are not managed on this device".into(),
    }
}

/// The host answered with the wrong response kind for the request.
pub fn wrong_response() -> RpcFailure {
    RpcFailure {
        code: CODE_REFUSED.into(),
        message: "the plugin host returned the wrong response".into(),
    }
}
