//! An exact launch's session status, pushed over its existing peer connection.
use serde::{Deserialize, Serialize};
use typeshare::typeshare;

/// The launch returned by prepare. A subscription never follows a replacement.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionEventsRequest {
    pub launch_id: String,
}

/// Stream records use the existing authenticated PeerResponseEnvelope with a
/// SessionStatus response. Each newline is one independently bounded record.
/// Subscribe before reading the initial status, so a concurrent change is not lost.
pub(crate) fn exact_status(
    launch: &str,
    outcome: crate::SessionStatusOutcome,
) -> crate::SessionStatusOutcome {
    match outcome {
        crate::SessionStatusOutcome::Ok(status)
            if status
                .active
                .as_ref()
                .is_some_and(|active| active.launch_id != launch) =>
        {
            crate::SessionStatusOutcome::Err(crate::RpcFailure {
                code: "StaleLaunchIdentity".into(),
                message: "The subscribed launch was replaced".into(),
            })
        }
        other => other,
    }
}

pub(crate) struct Client {
    response: crate::federation::reach::http::Response,
    credentials: crate::peer_rpc::PeerCredentials,
    peer: String,
    request: crate::peer_rpc::EncodedPeerRequest,
    buffered: Vec<u8>,
    pending: axum::body::Bytes,
}
impl Client {
    pub(crate) fn new(
        response: crate::federation::reach::http::Response,
        credentials: crate::peer_rpc::PeerCredentials,
        peer: String,
        request: crate::peer_rpc::EncodedPeerRequest,
    ) -> Self {
        Self {
            response,
            credentials,
            peer,
            request,
            buffered: Vec::new(),
            pending: axum::body::Bytes::new(),
        }
    }

    pub(crate) async fn next(
        &mut self,
    ) -> Result<crate::upstream::UpstreamSessionStatus, crate::upstreams::UpstreamError> {
        use crate::upstreams::UpstreamError;
        loop {
            if self.pending.is_empty() {
                self.pending = self
                    .response
                    .chunk()
                    .await
                    .map_err(|e| UpstreamError::Unreachable(e.to_string()))?
                    .ok_or_else(|| {
                        UpstreamError::Unreachable("session events stream closed".into())
                    })?;
            }
            let count = self
                .pending
                .iter()
                .position(|b| *b == b'\n')
                .map_or(self.pending.len(), |end| end + 1);
            // Keep only one record in the allocation, even when HTTP coalesces
            // several records. Reject an oversized record before copying it.
            if self.buffered.len().saturating_add(count) > crate::peer_rpc::MAX_PEER_EVENT_BYTES + 1
            {
                return Err(UpstreamError::Wire("oversized session event".into()));
            }
            self.buffered
                .extend_from_slice(&self.pending.split_to(count));
            if self.buffered.last() == Some(&b'\n') {
                let bytes = std::mem::take(&mut self.buffered);
                let event =
                    std::str::from_utf8(&bytes).map_err(|e| UpstreamError::Wire(e.to_string()))?;
                let response = self
                    .credentials
                    .decode_response(
                        &self.peer,
                        &self.request,
                        event,
                        crate::peer_rpc::unix_time(),
                    )
                    .map_err(|e| UpstreamError::Wire(e.to_string()))?;
                return crate::upstream_native::NativeClient::session_status_response(response);
            }
        }
    }
}
