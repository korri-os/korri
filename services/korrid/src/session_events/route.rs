use super::*;
use crate::{session_events::exact_status, SessionStatusOutcome, SessionStatusRequest};

pub(super) async fn route(
    State(state): State<PeerRpcServer>,
    admission: Option<axum::Extension<AdmissionPermit>>,
    body: Bytes,
) -> Response {
    let result = async {
        let event = std::str::from_utf8(&body).map_err(|_| PeerRpcError::Invalid)?;
        let proof = admission
            .as_ref()
            .and_then(|a| a.binding.as_ref().map(|b| (b, a.endpoint_event.as_deref())));
        let authenticated = state
            .authenticate(event, (state.clock)(), false, proof)
            .await?;
        if let Some(admission) = &admission {
            admission.release();
        }
        let RpcRequest::SessionEvents(request) = &authenticated.envelope.request else {
            return Err(PeerRpcError::WrongKind);
        };
        if request.launch_id.len() != 32
            || !request
                .launch_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(PeerRpcError::Invalid);
        }
        let changes = match &state.app.mode {
            crate::ServerMode::Host(host) => host.session_changes(),
            _ => None,
        }
        .ok_or(PeerRpcError::Invalid)?;
        let launch = request.launch_id.clone();
        // One pending record per consumer. A slow/disconnected reader cannot
        // accumulate statuses or keep a producer task alive after body drop.
        let stream = futures::stream::try_unfold(
            Some((state, authenticated, launch, changes, false)),
            |next| async move {
                let Some((state, authenticated, launch, mut changes, emitted)) = next else {
                    return Ok::<_, std::io::Error>(None);
                };
                if emitted && changes.changed().await.is_err() {
                    return Ok::<_, std::io::Error>(None);
                }
                changes.borrow_and_update();
                let local_state = state
                    .identity
                    .lock()
                    .map_err(|_| std::io::Error::other("identity unavailable"))?
                    .state()
                    .clone();
                // A stream grants no longer-lived authority than status reads:
                // check current ownership, pass expiry and revocations per fact.
                let attempt = state
                    .authorization
                    .attempt(
                        &local_state,
                        &authenticated.author,
                        authenticated.envelope.owner_statement.as_deref(),
                        authenticated.envelope.person_pass.as_deref(),
                        &authenticated.envelope.revocations,
                        (state.clock)(),
                    )
                    .map_err(std::io::Error::other)?;
                authorization::authorize(attempt.context(), &authenticated.envelope.request)
                    .map_err(std::io::Error::other)?;
                let response = dispatch(
                    &state.app,
                    attempt.context(),
                    RpcRequest::SessionStatus(SessionStatusRequest {}),
                )
                .await
                .map_err(std::io::Error::other)?;
                let RpcResponse::SessionStatus(status) = response else {
                    return Err(std::io::Error::other("unexpected session status"));
                };
                let status = exact_status(&launch, status);
                let terminal =
                    !matches!(&status, SessionStatusOutcome::Ok(s) if s.active.is_some());
                let envelope = PeerResponseEnvelope {
                    version: PEER_RPC_VERSION,
                    recipient: authenticated.author.clone(),
                    request_id: authenticated.envelope.request_id.clone(),
                    request_event_id: authenticated.event_id.clone(),
                    request_nonce: authenticated.envelope.nonce.clone(),
                    response: RpcResponse::SessionStatus(status),
                };
                let plaintext = serde_json::to_string(&envelope).map_err(std::io::Error::other)?;
                let mut record = state
                    .identity
                    .lock()
                    .map_err(|_| std::io::Error::other("identity unavailable"))?
                    .encrypt_event(
                        &authenticated.author,
                        PEER_RESPONSE_KIND,
                        &plaintext,
                        (state.clock)(),
                    )
                    .map_err(std::io::Error::other)?
                    .json;
                record.push('\n');
                // Stop after the terminal record without waiting for another change.
                let next = (!terminal).then_some((state, authenticated, launch, changes, true));
                Ok(Some((Bytes::from(record), next)))
            },
        );
        Ok(axum::body::Body::from_stream(stream).into_response())
    }
    .await;
    match result {
        Ok(response) => response,
        Err(PeerRpcError::Unauthorized) => StatusCode::FORBIDDEN.into_response(),
        Err(PeerRpcError::Replay) => StatusCode::CONFLICT.into_response(),
        Err(PeerRpcError::Time) => StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}
