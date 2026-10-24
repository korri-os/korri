//! The person socket has no browser capability and no private input routes.
//! Its forwarding selector is the same exact key used by SourceStatusRequest.
use crate::{
    authorization::{self, AuthorizationContext, PeerPolicy},
    federation::coordinator::FederationResources,
    AppState, RpcFailure, RpcRequest, RpcResponse,
};
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::{Deserialize, Serialize};

pub const DEFAULT_SOCKET: &str = "/run/korrid-cli/cli.sock";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForwardRequest {
    pub device_public_key: String,
    pub request: RpcRequest,
}

#[derive(Clone)]
struct CliState {
    local: AppState,
    resources: FederationResources,
}

pub(crate) fn router(local: AppState, resources: FederationResources) -> Router {
    Router::new()
        .route("/rpc", post(rpc))
        .route("/forward", post(forward))
        .route("/folder-selection", post(select_folder))
        .with_state(CliState { local, resources })
}

/// Only the peer-credential-protected person socket installs this route. The
/// receipt comes from discovery's actual grant store and is consumed by Pair.
async fn select_folder(
    State(state): State<CliState>,
    Json(path): Json<std::path::PathBuf>,
) -> Result<Json<String>, (StatusCode, Json<RpcFailure>)> {
    let library = state.local.local_library().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(crate::library_unavailable("discovery")),
        )
    })?;
    crate::require_discovery_owner(crate::caller_is_owner(
        library.owner_public_key.as_deref(),
        &AuthorizationContext::LocalCli,
    ))
    .map_err(|failure| (StatusCode::FORBIDDEN, Json(failure)))?;
    library.discovery.grants().issue_approved_path(path)
        .map(|grant| Json(grant.token))
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(RpcFailure {
            code: "FolderUnavailable".into(),
            message: "The selected library folder is not an available directory. Check its path and access permissions.".into(),
        })))
}

async fn rpc(
    State(state): State<CliState>,
    Json(request): Json<RpcRequest>,
) -> Result<Json<RpcResponse>, StatusCode> {
    crate::dispatch(&state.local, &AuthorizationContext::LocalCli, request)
        .await
        .map(Json)
        .map_err(|_| StatusCode::FORBIDDEN)
}

async fn forward(
    State(state): State<CliState>,
    Json(request): Json<ForwardRequest>,
) -> Result<Json<RpcResponse>, StatusCode> {
    // A local-only operation must never be sent off-device, even to its owner.
    // The receiving peer still checks its unchanged policy independently.
    if matches!(
        authorization::policy_for(&request.request),
        PeerPolicy::LocalOnly | PeerPolicy::LocalPortalOnly
    ) {
        return Err(StatusCode::FORBIDDEN);
    }
    let result = forward_to_owner(&state, &request).await;
    match result {
        Ok(response) => Ok(Json(response)),
        Err(failure) => failure_response(&request.request, failure)
            .map(Json)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn forward_to_owner(
    state: &CliState,
    request: &ForwardRequest,
) -> Result<RpcResponse, RpcFailure> {
    // snapshot verifies signed membership, current owner, and revocations.
    // Static configuration and labels alone do not establish membership.
    let peer = state.resources.directory.snapshot()
        .map_err(|_| unavailable())?
        .into_iter()
        .find(|peer| peer.device_public_key == request.device_public_key)
        .ok_or_else(|| RpcFailure {
            code: "OwnerDeviceRequired".into(),
            message: "The device has no verified membership for this owner. Use SSH or local access on that device.".into(),
        })?;
    let candidates = peer
        .current_endpoint
        .as_ref()
        .or(peer.remembered_endpoint.as_ref())
        .map(|endpoint| endpoint.candidates.clone())
        .unwrap_or_default();
    let client = crate::upstream_native::NativeClient::new_secure_candidates(
        candidates,
        peer.device_public_key,
        state.resources.credentials.clone(),
    )
    .with_directory(state.resources.directory.clone());
    client
        .call(request.request.clone())
        .await
        .map_err(|error| match error {
            crate::upstreams::UpstreamError::Tagged { code, message } => {
                RpcFailure { code, message }
            }
            crate::upstreams::UpstreamError::Http(403) => RpcFailure {
                code: "PeerAuthorizationDenied".into(),
                message:
                    "The device refused this operation. Use SSH or local access on that device."
                        .into(),
            },
            _ => unavailable(),
        })
}

fn unavailable() -> RpcFailure {
    RpcFailure {
        code: "DeviceUnavailable".into(),
        message: "The selected device could not complete the operation. Check its connection and try again.".into(),
    }
}

/// Keep an operation's existing Err outcome. This is wire-envelope plumbing,
/// not a second set of domain responses.
pub fn failure_response(
    request: &RpcRequest,
    failure: RpcFailure,
) -> Result<RpcResponse, serde_json::Error> {
    let value = serde_json::to_value(request)?;
    serde_json::from_value(serde_json::json!({
        "_tag": value["_tag"],
        "outcome": { "_tag": "Err", "payload": failure }
    }))
}

pub(crate) fn settings_caller_allowed(state: &AppState, context: &AuthorizationContext) -> bool {
    matches!(context, AuthorizationContext::LocalCli)
        || (state.portal_access.is_some() && matches!(context, AuthorizationContext::LocalBrowser))
        || match &state.mode {
            crate::ServerMode::Host(host) => {
                matches!(context, AuthorizationContext::Peer(_))
                    && crate::caller_is_owner(host.owner_public_key().as_deref(), context)
            }
            crate::ServerMode::Brain(brain) => {
                matches!(context, AuthorizationContext::Peer(_))
                    && crate::caller_is_owner(brain.local_owner_public_key(), context)
            }
        }
}

pub(crate) fn settings_write_allowed(
    state: &AppState,
    context: &AuthorizationContext,
    setting_id: &str,
) -> bool {
    settings_caller_allowed(state, context)
        && (!matches!(context, AuthorizationContext::Peer(_))
            || setting_id == crate::config::settings::DEVICE_NAME_SETTING_ID)
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
