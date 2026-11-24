//! ADR 0092 (#552): `/peer-installs-piece`. Loaded as a child of `peer_rpc`
//! so it uses that one authentication and replay boundary, as the save and
//! release object routes do.
use super::*;

pub(super) async fn piece_route(
    State(state): State<PeerRpcServer>,
    binding: Option<axum::Extension<crate::federation::reach::Binding>>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    let Ok(event) = std::str::from_utf8(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    // On the iroh route the caller proves its endpoint, as for the other
    // object routes.
    let proof = binding.as_ref().map(|binding| {
        (
            &binding.0,
            headers
                .get(crate::federation::reach::http::ENDPOINT_HEADER)
                .and_then(|value| value.to_str().ok()),
        )
    });
    let authenticated = match state
        .authenticate(event, (state.clock)(), true, proof)
        .await
    {
        Ok(authenticated) => authenticated,
        Err(PeerRpcError::Replay) => return StatusCode::CONFLICT.into_response(),
        Err(PeerRpcError::Unauthorized) => return StatusCode::FORBIDDEN.into_response(),
        Err(PeerRpcError::Time) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    match &authenticated.envelope.request {
        RpcRequest::InstallPiece(request) => {
            crate::acquisition::pieces::piece_response(&state.app, &authenticated.context, request)
                .await
        }
        _ => StatusCode::BAD_REQUEST.into_response(),
    }
}
