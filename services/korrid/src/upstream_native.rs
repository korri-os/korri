use crate::{
    game_routes::SelectedGameLaunchOutcome, CatalogSnapshot, CatalogSnapshotOutcome,
    MoonlightCertificateAttestOutcome, MoonlightCertificateAttestRequest,
    MoonlightCertificateAttested, MoonlightCertificateProvisionOutcome,
    MoonlightCertificateProvisionRequest, MoonlightCertificateProvisioned,
    MoonlightCertificateRevokeOutcome, MoonlightCertificateRevokeRequest,
    MoonlightCertificateRevoked, RpcFailure, RpcRequest, RpcResponse, SessionControlInvokeOutcome,
    SessionControlInvokeRequest, SessionControlsOutcome, SessionControlsRequest, SessionEndOutcome,
    SessionEndPhase, SessionEndRequest, SessionLeaveRequest, SessionPrepareRequest,
    SessionResumeRequest, SessionStatusOutcome, SessionStatusRequest, SessionTransitionOutcome,
    SessionTransitionResult, SourceStatus, SourceStatusOutcome, SourceStatusRequest,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use crate::{
    peer_rpc::{unix_time, PeerCredentials},
    upstream::{UpstreamActiveSession, UpstreamSessionStatus, UpstreamSessionStop},
    upstreams::UpstreamError,
};

const MAX_NATIVE_RPC_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const NATIVE_RPC_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const PEER_OPERATION_FAILED_MESSAGE: &str = "Authenticated peer operation failed";

/// One policy for static, relay and remembered peer endpoints. Check the raw
/// authority/path too: URL parsing alone silently repairs credentials, dot paths,
/// backslashes and whitespace. Relay WebSocket URLs use their separate policy.
pub(crate) fn configured_peer_origin(value: &str) -> Result<String, UpstreamError> {
    if value.is_empty()
        || value.len() > 2048
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
        || value.contains('\\')
    {
        return Err(UpstreamError::Wire("invalid configured peer origin".into()));
    }
    let (_, rest) = value
        .split_once("://")
        .ok_or(UpstreamError::Wire("invalid configured peer origin".into()))?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    if !path.is_empty() || authority.contains(['@', '?', '#']) || authority.ends_with(':') {
        return Err(UpstreamError::Wire("invalid configured peer origin".into()));
    }
    let url = url::Url::parse(value)
        .map_err(|_| UpstreamError::Wire("invalid configured peer origin".into()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.port_or_known_default().is_none_or(|port| port == 0)
    {
        return Err(UpstreamError::Wire("invalid configured peer origin".into()));
    }
    Ok(url.origin().ascii_serialization())
}

#[derive(Clone)]
pub struct NativeClient {
    // Secure constructors retain raw input until configured_peer_origin validates it.
    origins: Vec<String>,
    peer_label: Option<String>,
    directory: Option<Arc<crate::federation::FederationDirectory>>,
    operation_timeout: Duration,
    expected_peer_public_key: Option<String>,
    credentials: Option<PeerCredentials>,
    http: reqwest::Client,
    #[cfg(test)]
    fixed_request: Option<FixedPeerRequest>,
}

#[cfg(test)]
#[derive(Clone)]
struct FixedPeerRequest {
    now: u64,
    request_id: String,
    nonce: String,
}

impl NativeClient {
    pub fn new_secure(
        base_url: String,
        expected_peer_public_key: String,
        credentials: PeerCredentials,
    ) -> Self {
        Self::new_secure_candidates(vec![base_url], expected_peer_public_key, credentials)
    }

    pub(crate) fn new_secure_candidates(
        candidates: Vec<String>,
        expected_peer_public_key: String,
        credentials: PeerCredentials,
    ) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("construct native upstream client");
        Self {
            origins: candidates,
            peer_label: None,
            directory: None,
            operation_timeout: NATIVE_RPC_TIMEOUT,
            expected_peer_public_key: Some(expected_peer_public_key),
            credentials: Some(credentials),
            http,
            #[cfg(test)]
            fixed_request: None,
        }
    }

    #[cfg(test)]
    pub fn new_secure_at(
        base_url: String,
        expected_peer_public_key: String,
        credentials: PeerCredentials,
        now: u64,
        request_id: String,
        nonce: String,
    ) -> Self {
        let mut client = Self::new_secure(base_url, expected_peer_public_key, credentials);
        client.fixed_request = Some(FixedPeerRequest {
            now,
            request_id,
            nonce,
        });
        client
    }

    #[cfg(test)]
    pub fn new(base_url: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(NATIVE_RPC_TIMEOUT)
            .connect_timeout(Duration::from_secs(5))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("construct native upstream client");
        Self {
            origins: vec![base_url.trim_end_matches('/').to_owned()],
            peer_label: None,
            directory: None,
            operation_timeout: NATIVE_RPC_TIMEOUT,
            expected_peer_public_key: None,
            credentials: None,
            http,
            fixed_request: None,
        }
    }

    pub(crate) fn with_directory(
        mut self,
        directory: crate::federation::FederationDirectory,
    ) -> Self {
        self.directory = Some(Arc::new(directory));
        self
    }

    pub(crate) fn with_peer_label(mut self, label: String) -> Self {
        self.peer_label = Some(label);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_timeout(mut self, timeout: Duration) -> Self {
        self.operation_timeout = timeout;
        self
    }

    pub(crate) fn ensure_not_revoked(&self) -> Result<(), UpstreamError> {
        if let (Some(directory), Some(key)) = (&self.directory, &self.expected_peer_public_key) {
            if directory
                .peer_is_revoked(key)
                .map_err(|_| UpstreamError::SourcePeerNotFound)?
            {
                return Err(UpstreamError::SourcePeerNotFound);
            }
        }
        Ok(())
    }

    fn tagged_failure(failure: RpcFailure) -> UpstreamError {
        UpstreamError::Tagged {
            code: failure.code,
            message: failure.message,
        }
    }

    fn certificate_failure(error: UpstreamError) -> UpstreamError {
        match error {
            UpstreamError::Tagged { code, .. } if code == "HostMismatch" => {
                UpstreamError::MoonlightHostChanged
            }
            UpstreamError::Tagged { code, .. } if code == "StreamCertificateControlBusy" => {
                UpstreamError::MoonlightCertificateBusy
            }
            UpstreamError::Tagged { .. } => UpstreamError::MoonlightCertificateRejected,
            UpstreamError::Unreachable(_) | UpstreamError::Http(_) => {
                UpstreamError::MoonlightCertificatePeerUnavailable
            }
            UpstreamError::Wire(_) => UpstreamError::MoonlightCertificatePeerProtocol,
            UpstreamError::Failure(_)
            | UpstreamError::MoonlightCertificatePeerUnavailable
            | UpstreamError::MoonlightCertificatePeerProtocol
            | UpstreamError::MoonlightCertificateRejected
            | UpstreamError::MoonlightCertificateBusy
            | UpstreamError::MoonlightHostChanged
            | UpstreamError::MoonlightHostNotFound
            | UpstreamError::MoonlightHostAmbiguous
            | UpstreamError::SelectedRemoteSessionReplaced
            | UpstreamError::ActiveRemoteSessionConflict
            | UpstreamError::ExpectedLaunchIdRequired
            | UpstreamError::StaleLaunchIdentity
            | UpstreamError::NoActiveSession
            | UpstreamError::FreezerUnsupportedOnLegacyRoute
            | UpstreamError::SourcePeerNotFound
            | UpstreamError::MoonlightHostCandidatesUnavailable => error,
        }
    }

    pub async fn catalog_snapshot(&self) -> Result<CatalogSnapshot, UpstreamError> {
        match self
            .call(RpcRequest::CatalogSnapshot(
                crate::CatalogSnapshotRequest {},
            ))
            .await?
        {
            RpcResponse::CatalogSnapshot(CatalogSnapshotOutcome::Ok(catalog)) => Ok(catalog),
            RpcResponse::CatalogSnapshot(CatalogSnapshotOutcome::Err(failure)) => {
                Err(Self::tagged_failure(failure))
            }
            response => Err(UpstreamError::Wire(format!(
                "native catalog returned {response:?}"
            ))),
        }
    }

    pub async fn moonlight_certificate_attest(
        &self,
        host_uuid: &str,
    ) -> Result<MoonlightCertificateAttested, UpstreamError> {
        match self
            .call(RpcRequest::MoonlightCertificateAttest(
                MoonlightCertificateAttestRequest {
                    host_uuid: host_uuid.into(),
                },
            ))
            .await
            .map_err(Self::certificate_failure)?
        {
            RpcResponse::MoonlightCertificateAttest(MoonlightCertificateAttestOutcome::Ok(
                attested,
            )) => Ok(attested),
            RpcResponse::MoonlightCertificateAttest(MoonlightCertificateAttestOutcome::Err(
                failure,
            )) => Err(Self::certificate_failure(Self::tagged_failure(failure))),
            _ => Err(UpstreamError::MoonlightCertificatePeerProtocol),
        }
    }

    pub async fn moonlight_certificate_provision(
        &self,
        host_uuid: &str,
        client_certificate: &str,
    ) -> Result<MoonlightCertificateProvisioned, UpstreamError> {
        match self
            .call(RpcRequest::MoonlightCertificateProvision(
                MoonlightCertificateProvisionRequest {
                    host_uuid: host_uuid.into(),
                    client_certificate: client_certificate.into(),
                },
            ))
            .await
            .map_err(Self::certificate_failure)?
        {
            RpcResponse::MoonlightCertificateProvision(
                MoonlightCertificateProvisionOutcome::Ok(provisioned),
            ) => Ok(provisioned),
            RpcResponse::MoonlightCertificateProvision(
                MoonlightCertificateProvisionOutcome::Err(failure),
            ) => Err(Self::certificate_failure(Self::tagged_failure(failure))),
            _ => Err(UpstreamError::MoonlightCertificatePeerProtocol),
        }
    }

    pub async fn moonlight_certificate_revoke(
        &self,
        host_uuid: &str,
        client_certificate: &str,
    ) -> Result<MoonlightCertificateRevoked, UpstreamError> {
        match self
            .call(RpcRequest::MoonlightCertificateRevoke(
                MoonlightCertificateRevokeRequest {
                    host_uuid: host_uuid.into(),
                    client_certificate: client_certificate.into(),
                },
            ))
            .await
            .map_err(Self::certificate_failure)?
        {
            RpcResponse::MoonlightCertificateRevoke(MoonlightCertificateRevokeOutcome::Ok(
                revoked,
            )) => Ok(revoked),
            RpcResponse::MoonlightCertificateRevoke(MoonlightCertificateRevokeOutcome::Err(
                failure,
            )) => Err(Self::certificate_failure(Self::tagged_failure(failure))),
            _ => Err(UpstreamError::MoonlightCertificatePeerProtocol),
        }
    }

    pub(crate) async fn viewer_display_report(
        &self,
        request: crate::live_display::SessionViewerDisplayRequest,
    ) -> Result<crate::live_display::SessionViewerDisplay, UpstreamError> {
        match self.call(RpcRequest::SessionViewerDisplay(request)).await? {
            RpcResponse::SessionViewerDisplay(
                crate::live_display::SessionViewerDisplayOutcome::Ok(value),
            ) => Ok(value),
            RpcResponse::SessionViewerDisplay(
                crate::live_display::SessionViewerDisplayOutcome::Err(error),
            ) => Err(Self::tagged_failure(error)),
            _ => Err(UpstreamError::Wire(
                "unexpected viewer display response".into(),
            )),
        }
    }

    pub(crate) async fn viewer_resize(
        &self,
        request: crate::live_display::SessionViewerResizeRequest,
    ) -> Result<crate::live_display::ViewerResize, UpstreamError> {
        match self.call(RpcRequest::SessionViewerResize(request)).await? {
            RpcResponse::SessionViewerResize(
                crate::live_display::SessionViewerResizeOutcome::Ok(value),
            ) => Ok(value),
            RpcResponse::SessionViewerResize(
                crate::live_display::SessionViewerResizeOutcome::Err(error),
            ) => Err(Self::tagged_failure(error)),
            _ => Err(UpstreamError::Wire(
                "unexpected viewer resize response".into(),
            )),
        }
    }

    pub async fn prepare_stream(
        &self,
        game_id: &str,
        viewer_display: Option<crate::launcher::plugin_launch::PluginDisplay>,
    ) -> Result<crate::game_routes::SelectedGameLaunch, UpstreamError> {
        self.prepare_stream_for_launch(game_id, viewer_display, None)
            .await
    }

    /// Prepare with the values of this one launch. The holder takes them only
    /// from a device of its owner.
    pub async fn prepare_stream_for_launch(
        &self,
        game_id: &str,
        viewer_display: Option<crate::launcher::plugin_launch::PluginDisplay>,
        launch: Option<crate::launch_settings::LaunchOverride>,
    ) -> Result<crate::game_routes::SelectedGameLaunch, UpstreamError> {
        match self
            .call(RpcRequest::SessionPrepare(SessionPrepareRequest {
                viewer_display,
                game_id: game_id.into(),
                host: None,
                launch,
            }))
            .await?
        {
            RpcResponse::SessionPrepare(SelectedGameLaunchOutcome::Ok(prepared)) => Ok(prepared),
            RpcResponse::SessionPrepare(SelectedGameLaunchOutcome::Err(failure)) => {
                Err(Self::tagged_failure(failure))
            }
            response => Err(UpstreamError::Wire(format!(
                "native prepare returned {response:?}"
            ))),
        }
    }

    /// Launch with the runner the person chose on the holder (issue #401).
    pub async fn selected_game_launch(
        &self,
        game_id: &str,
        runner_id: &str,
        launch: Option<crate::launch_settings::LaunchOverride>,
    ) -> Result<crate::game_routes::SelectedGameLaunch, UpstreamError> {
        match self
            .call(RpcRequest::SelectedGameLaunch(
                crate::game_routes::SelectedGameLaunchRequest {
                    game_id: game_id.into(),
                    runner_id: runner_id.into(),
                    launch,
                    holder: None,
                },
            ))
            .await?
        {
            RpcResponse::SelectedGameLaunch(SelectedGameLaunchOutcome::Ok(prepared)) => {
                Ok(prepared)
            }
            RpcResponse::SelectedGameLaunch(SelectedGameLaunchOutcome::Err(failure)) => {
                Err(Self::tagged_failure(failure))
            }
            response => Err(UpstreamError::Wire(format!(
                "native selected launch returned {response:?}"
            ))),
        }
    }

    pub(crate) async fn stream_play(
        &self,
        launch_id: &str,
        device_label: &str,
    ) -> Result<crate::ActiveSession, UpstreamError> {
        match self
            .call(RpcRequest::SessionStreamPlay(
                crate::stream_play::SessionStreamPlayRequest {
                    launch_id: launch_id.into(),
                    device_label: device_label.into(),
                },
            ))
            .await?
        {
            RpcResponse::SessionStreamPlay(crate::stream_play::SessionStreamPlayOutcome::Ok(
                active,
            )) => Ok(active),
            RpcResponse::SessionStreamPlay(crate::stream_play::SessionStreamPlayOutcome::Err(
                failure,
            )) => Err(Self::tagged_failure(failure)),
            _ => Err(UpstreamError::Wire(
                "unexpected stream play response".into(),
            )),
        }
    }

    pub async fn session_status(&self) -> Result<UpstreamSessionStatus, UpstreamError> {
        Self::session_status_response(
            self.call(RpcRequest::SessionStatus(SessionStatusRequest {}))
                .await?,
        )
    }

    /// A session observes only its captured connection. This path cannot dial.
    pub(crate) async fn session_status_on(
        &self,
        connection: &iroh::endpoint::Connection,
    ) -> Result<UpstreamSessionStatus, UpstreamError> {
        self.ensure_not_revoked()?;
        if connection.close_reason().is_some() {
            return Err(UpstreamError::Tagged {
                code: "PeerConnectionGone".into(),
                message: "Peer connection is gone. Leave or end this session.".into(),
            });
        }
        let result = self
            .send_request(
                &RpcRequest::SessionStatus(SessionStatusRequest {}),
                &AtomicBool::new(false),
                Some(connection),
            )
            .await;
        self.ensure_not_revoked()?;
        Self::session_status_response(result?)
    }

    /// Idle invite discovery is not a federation health probe. Keep peer
    /// authentication and revocation checks, without starting or completing a
    /// directory observation (which would also supersede genuine peer work).
    pub(crate) async fn invite_session_status(
        &self,
    ) -> Result<UpstreamSessionStatus, UpstreamError> {
        self.ensure_not_revoked()?;
        let result = self
            .send_request(
                &RpcRequest::SessionStatus(SessionStatusRequest {}),
                &AtomicBool::new(false),
                None,
            )
            .await;
        self.ensure_not_revoked()?;
        Self::session_status_response(result?)
    }

    fn session_status_response(
        response: RpcResponse,
    ) -> Result<UpstreamSessionStatus, UpstreamError> {
        match response {
            RpcResponse::SessionStatus(SessionStatusOutcome::Ok(status)) => {
                Ok(UpstreamSessionStatus::SessionStatus {
                    active: status.active.map(|active| UpstreamActiveSession {
                        steps: active.steps,
                        initial_handoff: active.initial_handoff,
                        netplay: active.netplay,
                        person_public_key: active.person_public_key,
                        stream_players: active.stream_players,
                        launch_id: active.launch_id,
                        host: active.host,
                        game_id: active.game_id,
                        title: active.title,
                        phase: active.phase,
                    }),
                })
            }
            RpcResponse::SessionStatus(SessionStatusOutcome::Err(failure))
                if matches!(
                    failure.code.as_str(),
                    "SessionCompleted" | "NoActiveSession"
                ) =>
            {
                Ok(UpstreamSessionStatus::SessionStatus { active: None })
            }
            RpcResponse::SessionStatus(SessionStatusOutcome::Err(failure)) => {
                Err(Self::tagged_failure(failure))
            }
            response => Err(UpstreamError::Wire(format!(
                "native session status returned {response:?}"
            ))),
        }
    }

    pub async fn session_controls(
        &self,
        launch_id: &str,
    ) -> Result<SessionControlsOutcome, UpstreamError> {
        match self
            .call(RpcRequest::SessionControls(SessionControlsRequest {
                launch_id: launch_id.into(),
            }))
            .await?
        {
            RpcResponse::SessionControls(SessionControlsOutcome::Ok(controls))
                if controls.launch_id != launch_id =>
            {
                Err(UpstreamError::SelectedRemoteSessionReplaced)
            }
            RpcResponse::SessionControls(outcome) => Ok(outcome),
            _ => Err(UpstreamError::Wire(
                "invalid session controls response".into(),
            )),
        }
    }

    pub async fn invoke_session_control(
        &self,
        request: SessionControlInvokeRequest,
    ) -> Result<SessionControlInvokeOutcome, UpstreamError> {
        let launch_id = request.launch_id.clone();
        match self.call(RpcRequest::SessionControlInvoke(request)).await? {
            RpcResponse::SessionControlInvoke(SessionControlInvokeOutcome::Ok(completed))
                if completed.launch_id != launch_id =>
            {
                Err(UpstreamError::SelectedRemoteSessionReplaced)
            }
            RpcResponse::SessionControlInvoke(outcome) => Ok(outcome),
            _ => Err(UpstreamError::Wire(
                "invalid session control response".into(),
            )),
        }
    }

    pub async fn session_stop(
        &self,
        expected_launch_id: &str,
        force: bool,
    ) -> Result<UpstreamSessionStop, UpstreamError> {
        match self
            .call(RpcRequest::SessionEnd(SessionEndRequest {
                force: force.then_some(true),
                expected_launch_id: Some(expected_launch_id.into()),
            }))
            .await?
        {
            RpcResponse::SessionEnd(SessionEndOutcome::Ok(result)) => match result.phase {
                SessionEndPhase::Ended => Ok(UpstreamSessionStop::Stopped {
                    launch_id: Some(expected_launch_id.into()),
                }),
                SessionEndPhase::Pending => Ok(UpstreamSessionStop::StopPending {
                    launch_id: Some(expected_launch_id.into()),
                }),
            },
            RpcResponse::SessionEnd(SessionEndOutcome::Err(failure)) => {
                Err(Self::tagged_failure(failure))
            }
            response => Err(UpstreamError::Wire(format!(
                "native session stop returned {response:?}"
            ))),
        }
    }

    pub async fn session_freeze(
        &self,
        expected_launch_id: &str,
    ) -> Result<SessionTransitionResult, UpstreamError> {
        match self
            .call(RpcRequest::SessionLeave(SessionLeaveRequest {
                expected_launch_id: Some(expected_launch_id.into()),
            }))
            .await?
        {
            RpcResponse::SessionLeave(SessionTransitionOutcome::Ok(result)) => Ok(result),
            RpcResponse::SessionLeave(SessionTransitionOutcome::Err(failure)) => {
                Err(Self::tagged_failure(failure))
            }
            response => Err(UpstreamError::Wire(format!(
                "native session freeze returned {response:?}"
            ))),
        }
    }

    pub async fn session_thaw(
        &self,
        expected_launch_id: &str,
    ) -> Result<SessionTransitionResult, UpstreamError> {
        match self
            .call(RpcRequest::SessionResume(SessionResumeRequest {
                expected_launch_id: Some(expected_launch_id.into()),
            }))
            .await?
        {
            RpcResponse::SessionResume(SessionTransitionOutcome::Ok(result)) => Ok(result),
            RpcResponse::SessionResume(SessionTransitionOutcome::Err(failure)) => {
                Err(Self::tagged_failure(failure))
            }
            response => Err(UpstreamError::Wire(format!(
                "native session thaw returned {response:?}"
            ))),
        }
    }

    /// Ask the peer for its own readiness. The request must name the peer's
    /// own `devicePublicKey`. The peer never answers for another device.
    pub async fn source_status(
        &self,
        device_public_key: &str,
    ) -> Result<SourceStatus, UpstreamError> {
        match self
            .call(RpcRequest::SourceStatus(SourceStatusRequest {
                device_public_key: device_public_key.into(),
            }))
            .await?
        {
            RpcResponse::SourceStatus(SourceStatusOutcome::Ok(status)) => Ok(status),
            RpcResponse::SourceStatus(SourceStatusOutcome::Err(failure)) => {
                Err(Self::tagged_failure(failure))
            }
            response => Err(UpstreamError::Wire(format!(
                "native source status returned {response:?}"
            ))),
        }
    }

    pub(crate) async fn call(&self, request: RpcRequest) -> Result<RpcResponse, UpstreamError> {
        #[cfg(test)]
        if self.credentials.is_none() {
            return self
                .send_request(&request, &AtomicBool::new(false), None)
                .await;
        }
        let key = self
            .expected_peer_public_key
            .as_deref()
            .ok_or_else(|| UpstreamError::Wire("native peer public key is unavailable".into()))?;
        let token = if let Some(directory) = &self.directory {
            if directory
                .peer_is_revoked(key)
                .map_err(|_| UpstreamError::SourcePeerNotFound)?
            {
                return Err(UpstreamError::SourcePeerNotFound);
            }
            // Static-only peers have no live directory row. Do not manufacture
            // membership or unsigned remembered evidence for them.
            if directory
                .snapshot()
                .map_err(|_| UpstreamError::SourcePeerNotFound)?
                .iter()
                .any(|peer| peer.device_public_key == key)
            {
                Some(
                    directory
                        .begin_peer_work(key)
                        .map_err(|_| UpstreamError::SourcePeerNotFound)?,
                )
            } else {
                None
            }
        } else {
            None
        };
        let answered = AtomicBool::new(false);
        let result = self.send_request(&request, &answered, None).await;
        let result = match result {
            // Preserve the reply evidence for callers such as the background
            // liveness probe, not only for this directory update.
            Err(UpstreamError::Unreachable(_)) if answered.load(Ordering::Relaxed) => {
                Err(UpstreamError::Wire("peer response did not complete".into()))
            }
            result => result,
        };

        if let (Some(directory), Some(token)) = (&self.directory, token) {
            let state = match &result {
                Ok(_) => crate::federation::PeerState::Ready,
                Err(UpstreamError::Unreachable(_)) => crate::federation::PeerState::Failed {
                    error: self
                        .peer_label
                        .as_ref()
                        .map(|label| format!("Can't reach {label} from this network right now"))
                        .unwrap_or_else(|| directory.unreachable_sentence(key)),
                },
                Err(UpstreamError::Tagged { code, .. }) if code == "PeerConnectionGone" => {
                    crate::federation::PeerState::Failed {
                        error: "Peer connection is gone".into(),
                    }
                }
                Err(_) => crate::federation::PeerState::Failed {
                    error: PEER_OPERATION_FAILED_MESSAGE.into(),
                },
            };
            // A superseded/configuration-changed/revoked completion cannot
            // overwrite a newer state. Network results never restore membership.
            let _ = directory.set_peer_state(&token, key, state);
        }
        self.ensure_not_revoked()?;
        result
    }

    async fn send_request(
        &self,
        request: &RpcRequest,
        answered: &AtomicBool,
        captured: Option<&iroh::endpoint::Connection>,
    ) -> Result<RpcResponse, UpstreamError> {
        #[cfg(test)]
        if self.credentials.is_none() {
            let response = self
                .http
                .post(format!("{}/rpc", self.origins[0]))
                .json(request)
                .send()
                .await
                .map_err(|error| UpstreamError::Unreachable(error.to_string()))?;
            return read_plain_test_response(response).await;
        }
        self.call_candidates(request, answered, captured).await
    }

    async fn call_candidates(
        &self,
        request: &RpcRequest,
        answered: &AtomicBool,
        captured: Option<&iroh::endpoint::Connection>,
    ) -> Result<RpcResponse, UpstreamError> {
        self.ensure_not_revoked()?;
        let connection = if let Some(connection) = captured {
            Some(connection.clone())
        } else if let (Some(directory), Some(key)) =
            (&self.directory, &self.expected_peer_public_key)
        {
            Some(directory.reach_connection(key).await.map_err(|error| {
                if error.kind() == std::io::ErrorKind::ConnectionAborted {
                    UpstreamError::Tagged {
                        code: "PeerConnectionGone".into(),
                        message: "Peer connection is gone. Leave or end this session.".into(),
                    }
                } else {
                    UpstreamError::Unreachable("peer connection unavailable".into())
                }
            })?)
        } else {
            None
        };
        // Explicit configured native endpoints are still used by isolated CLI/test adapters.
        // Federation never selects, probes, or falls back to an IP endpoint.
        let url = if connection.is_some() {
            "/peer-rpc".to_owned()
        } else {
            let origin = self
                .origins
                .first()
                .ok_or_else(|| UpstreamError::Unreachable("peer endpoint unavailable".into()))?;
            format!(
                "{}/peer-rpc",
                configured_peer_origin(origin)
                    .map_err(|_| UpstreamError::Wire("invalid peer origin".into()))?
            )
        };
        match self
            .attempt_with_wait(
                &url,
                request.clone(),
                connection.as_ref(),
                answered,
                self.operation_timeout,
            )
            .await
        {
            Ok(Ok(response)) => checked_response(request, response),
            Ok(Err(failure)) => Err(failure.error),
            Err(_) => Err(UpstreamError::Unreachable(
                "peer operation timed out".into(),
            )),
        }
    }

    async fn attempt_with_wait(
        &self,
        rpc_url: &str,
        request: RpcRequest,
        connection: Option<&iroh::endpoint::Connection>,
        answered: &AtomicBool,
        timeout: Duration,
    ) -> Result<Result<RpcResponse, AttemptFailure>, tokio::time::error::Elapsed> {
        // PluginAdminSocket owns transfer/stall bounds and has no total
        // deadline. The transport sends each request once; mutations are never
        // replayed on another connection.
        if crate::plugin_repositories::is_admin_request(&request) {
            Ok(self.attempt(rpc_url, request, connection, answered).await)
        } else {
            tokio::time::timeout(
                timeout,
                self.attempt(rpc_url, request, connection, answered),
            )
            .await
        }
    }

    async fn attempt(
        &self,
        rpc_url: &str,
        request: RpcRequest,
        connection: Option<&iroh::endpoint::Connection>,
        answered: &AtomicBool,
    ) -> Result<RpcResponse, AttemptFailure> {
        let admin_wait = crate::plugin_repositories::is_admin_request(&request);
        let credentials = self
            .credentials
            .as_ref()
            .ok_or_else(|| UpstreamError::Wire("native peer credentials are unavailable".into()))?;
        let expected_peer_public_key = self
            .expected_peer_public_key
            .as_deref()
            .ok_or_else(|| UpstreamError::Wire("native peer public key is unavailable".into()))?;
        #[cfg(test)]
        let encoded = match &self.fixed_request {
            Some(fixed) => credentials.encode_request_with_tokens(
                expected_peer_public_key,
                request,
                fixed.now,
                fixed.request_id.clone(),
                fixed.nonce.clone(),
            ),
            None => credentials.encode_request(expected_peer_public_key, request, unix_time()),
        };
        #[cfg(not(test))]
        let encoded = credentials.encode_request(expected_peer_public_key, request, unix_time());
        let encoded = encoded.map_err(|error| UpstreamError::Wire(error.to_string()))?;
        let response = if let Some(connection) = connection {
            let proof = self
                .directory
                .as_ref()
                .ok_or_else(|| UpstreamError::Wire("peer directory unavailable".into()))?
                .own_endpoint_event(expected_peer_public_key)
                .map_err(|_| UpstreamError::Wire("endpoint proof unavailable".into()))?;
            crate::federation::reach::http::post(
                connection,
                rpc_url,
                encoded.event_json.clone(),
                None,
                Some(&proof),
            )
            .await
            .map_err(|_| AttemptFailure {
                error: UpstreamError::Unreachable("peer transport failed".into()),
            })?
        } else {
            let mut pending = self
                .http
                .post(rpc_url)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(encoded.event_json.clone());
            if !admin_wait {
                pending = pending.timeout(self.operation_timeout);
            }
            crate::federation::reach::http::Response::Tcp(pending.send().await.map_err(
                |_error| AttemptFailure {
                    error: UpstreamError::Unreachable("peer transport failed".into()),
                },
            )?)
        };
        // HTTP headers prove that a method answered, even if the body is
        // invalid, unauthenticated, or later times out. A subsequent route
        // failure must not turn that into an unreachable-peer sentence.
        answered.store(true, Ordering::Relaxed);
        let status = response.status();
        if !status.is_success() {
            return Err(UpstreamError::Http(status.as_u16()).into());
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_NATIVE_RPC_RESPONSE_BYTES as u64)
        {
            return Err(
                UpstreamError::Wire("native response exceeds the size limit".into()).into(),
            );
        }
        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| UpstreamError::Wire("native response body could not be read".into()))?
        {
            if body.len().saturating_add(chunk.len()) > MAX_NATIVE_RPC_RESPONSE_BYTES {
                return Err(
                    UpstreamError::Wire("native response exceeds the size limit".into()).into(),
                );
            }
            body.extend_from_slice(&chunk);
        }
        let event_json = std::str::from_utf8(&body)
            .map_err(|_| UpstreamError::Wire("native response is invalid UTF-8".into()))?;
        #[cfg(test)]
        let response_time = self
            .fixed_request
            .as_ref()
            .map(|fixed| fixed.now)
            .unwrap_or_else(unix_time);
        #[cfg(not(test))]
        let response_time = unix_time();
        credentials
            .decode_response(
                expected_peer_public_key,
                &encoded,
                event_json,
                response_time,
            )
            .map_err(|_| UpstreamError::Wire("peer response authentication failed".into()).into())
    }
}

struct AttemptFailure {
    error: UpstreamError,
}

impl From<UpstreamError> for AttemptFailure {
    fn from(error: UpstreamError) -> Self {
        Self { error }
    }
}

/// Validate the operation tag before marking a directory attempt ready. Typed
/// application failures end even a read. Never try another origin.
fn checked_response(
    request: &RpcRequest,
    response: RpcResponse,
) -> Result<RpcResponse, UpstreamError> {
    let failure = match (request, &response) {
        (RpcRequest::SettingsSnapshot(_), RpcResponse::SettingsSnapshot(outcome)) => {
            match outcome {
                crate::SettingsSnapshotOutcome::Ok(_) => None,
                crate::SettingsSnapshotOutcome::Err(e) => Some(e),
            }
        }
        (RpcRequest::SettingsUpdate(_), RpcResponse::SettingsUpdate(outcome)) => match outcome {
            crate::SettingsUpdateOutcome::Ok(_) => None,
            crate::SettingsUpdateOutcome::Err(e) => Some(e),
        },
        (RpcRequest::CatalogSnapshot(_), RpcResponse::CatalogSnapshot(outcome)) => match outcome {
            CatalogSnapshotOutcome::Ok(_) => None,
            CatalogSnapshotOutcome::Err(e) => Some(e),
        },
        (RpcRequest::SourceStatus(_), RpcResponse::SourceStatus(outcome)) => match outcome {
            SourceStatusOutcome::Ok(_) => None,
            SourceStatusOutcome::Err(e) => Some(e),
        },
        (RpcRequest::SessionStatus(_), RpcResponse::SessionStatus(outcome)) => match outcome {
            SessionStatusOutcome::Ok(_) => None,
            SessionStatusOutcome::Err(e)
                if matches!(e.code.as_str(), "SessionCompleted" | "NoActiveSession") =>
            {
                None
            }
            SessionStatusOutcome::Err(e) => Some(e),
        },
        // Gameplay refusals have SessionControlFailure, not RpcFailure. Keep
        // that typed outcome for the overlay (unknown, disabled, stale, etc.).
        (RpcRequest::SessionControls(_), RpcResponse::SessionControls(_))
        | (RpcRequest::SessionControlInvoke(_), RpcResponse::SessionControlInvoke(_)) => None,
        (RpcRequest::SessionPrepare(_), RpcResponse::SessionPrepare(outcome)) => match outcome {
            SelectedGameLaunchOutcome::Ok(_) => None,
            SelectedGameLaunchOutcome::Err(e) => Some(e),
        },
        (RpcRequest::SessionEnd(_), RpcResponse::SessionEnd(outcome)) => match outcome {
            SessionEndOutcome::Ok(_) => None,
            SessionEndOutcome::Err(e) => Some(e),
        },
        (RpcRequest::SessionLeave(_), RpcResponse::SessionLeave(outcome))
        | (RpcRequest::SessionResume(_), RpcResponse::SessionResume(outcome)) => match outcome {
            SessionTransitionOutcome::Ok(_) => None,
            SessionTransitionOutcome::Err(e) => Some(e),
        },
        (
            RpcRequest::MoonlightCertificateAttest(_),
            RpcResponse::MoonlightCertificateAttest(outcome),
        ) => match outcome {
            MoonlightCertificateAttestOutcome::Ok(_) => None,
            MoonlightCertificateAttestOutcome::Err(e) => Some(e),
        },
        (
            RpcRequest::MoonlightCertificateProvision(_),
            RpcResponse::MoonlightCertificateProvision(outcome),
        ) => match outcome {
            MoonlightCertificateProvisionOutcome::Ok(_) => None,
            MoonlightCertificateProvisionOutcome::Err(e) => Some(e),
        },
        (
            RpcRequest::MoonlightCertificateRevoke(_),
            RpcResponse::MoonlightCertificateRevoke(outcome),
        ) => match outcome {
            MoonlightCertificateRevokeOutcome::Ok(_) => None,
            MoonlightCertificateRevokeOutcome::Err(e) => Some(e),
        },
        _ => {
            // The local CLI can forward any request already allowed by the
            // peer policy. Validate the shared wire envelope, not a copied
            // table of every area's domain response.
            let expected = serde_json::to_value(request)
                .map_err(|_| UpstreamError::Wire("request encoding failed".into()))?;
            let actual = serde_json::to_value(&response)
                .map_err(|_| UpstreamError::Wire("response encoding failed".into()))?;
            if expected["_tag"] != actual["_tag"] {
                return Err(UpstreamError::Wire(
                    "peer returned a different operation tag".into(),
                ));
            }
            if actual["outcome"]["_tag"] == "Err" {
                let failure = serde_json::from_value(actual["outcome"]["payload"].clone())
                    .map_err(|_| UpstreamError::Wire("invalid peer failure".into()))?;
                return Err(NativeClient::tagged_failure(failure));
            }
            return Ok(response);
        }
    };
    if let Some(failure) = failure {
        return Err(NativeClient::tagged_failure(failure.clone()));
    }
    Ok(response)
}

#[cfg(test)]
async fn read_plain_test_response(
    mut response: reqwest::Response,
) -> Result<RpcResponse, UpstreamError> {
    let status = response.status();
    if !status.is_success() {
        return Err(UpstreamError::Http(status.as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_NATIVE_RPC_RESPONSE_BYTES as u64)
    {
        return Err(UpstreamError::Wire(
            "native response exceeds the size limit".into(),
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| UpstreamError::Wire("native response body could not be read".into()))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_NATIVE_RPC_RESPONSE_BYTES {
            return Err(UpstreamError::Wire(
                "native response exceeds the size limit".into(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body)
        .map_err(|_| UpstreamError::Wire("native response is invalid JSON".into()))
}

impl NativeClient {
    /// ADR 0092 (#554): this peer as the install source of the release that
    /// netplay needs, over its iroh route with this device's endpoint proof.
    pub(crate) fn install_holder(
        &self,
    ) -> Result<crate::acquisition::pieces::PeerHolder, UpstreamError> {
        self.ensure_not_revoked()?;
        let device = self
            .expected_peer_public_key
            .clone()
            .ok_or_else(|| UpstreamError::Wire("netplay requires an authenticated peer".into()))?;
        let directory = self
            .directory
            .as_deref()
            .cloned()
            .ok_or_else(|| UpstreamError::Wire("netplay requires federation reach".into()))?;
        let credentials = self
            .credentials
            .clone()
            .ok_or_else(|| UpstreamError::Wire("netplay requires device credentials".into()))?;
        Ok(crate::acquisition::pieces::PeerHolder {
            route: crate::acquisition::pieces::PeerRoute::Federation(directory),
            credentials,
            device,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_request_omits_absent_viewer_and_preserves_present_size() {
        let mut request = SessionPrepareRequest {
            game_id: "game".into(),
            host: None,
            viewer_display: None,
            launch: None,
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            serde_json::json!({"gameId":"game"})
        );
        request.viewer_display = Some(crate::launcher::plugin_launch::PluginDisplay {
            width: 2176,
            height: 1812,
        });
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(
            json["viewerDisplay"],
            serde_json::json!({"width":2176,"height":1812})
        );
        assert_eq!(
            serde_json::from_value::<SessionPrepareRequest>(json)
                .unwrap()
                .viewer_display,
            request.viewer_display
        );
    }

    async fn serve(app: axum::Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        serve_listener(listener, app).await
    }

    async fn serve_listener(listener: tokio::net::TcpListener, app: axum::Router) -> String {
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    async fn serve_response(status: axum::http::StatusCode, body: &'static str) -> String {
        let app = axum::Router::new().route(
            "/rpc",
            axum::routing::post(move || async move {
                (
                    status,
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    body,
                )
            }),
        );
        serve(app).await
    }

    #[tokio::test]
    async fn native_client_maps_host_http_and_wire_failures() {
        let root = tempfile::tempdir().unwrap();
        let tagged =
            NativeClient::new(serve(crate::host_router(root.path().join("missing"))).await)
                .catalog_snapshot()
                .await
                .unwrap_err();
        assert!(matches!(
            tagged,
            UpstreamError::Tagged { code, .. } if code == "HostConfigInvalid"
        ));

        let http = NativeClient::new(
            serve_response(axum::http::StatusCode::SERVICE_UNAVAILABLE, "unavailable").await,
        )
        .catalog_snapshot()
        .await
        .unwrap_err();
        assert!(matches!(http, UpstreamError::Http(503)));

        for body in [
            "not json",
            r#"{"_tag":"system.health","outcome":{"_tag":"Ok","payload":{"version":"test"}}}"#,
        ] {
            let error = NativeClient::new(serve_response(axum::http::StatusCode::OK, body).await)
                .catalog_snapshot()
                .await
                .unwrap_err();
            assert!(matches!(error, UpstreamError::Wire(_)));
        }
    }

    #[tokio::test]
    async fn native_client_round_trips_catalog_and_prepare() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(
            &config,
            r#"
label = "zao"

[[games]]
id = "neverball"
title = "Neverball"
command = ["neverball"]
"#,
        )
        .unwrap();
        let base_url = serve(crate::host_router_with_in_memory_units(&config)).await;
        let client = NativeClient::new(base_url);

        let catalog = client.catalog_snapshot().await.unwrap();
        assert_eq!(catalog.games[0].id, "neverball");
        assert_eq!(catalog.games[0].host.as_deref(), Some("zao"));

        let prepared = client.prepare_stream("neverball", None).await.unwrap();
        assert_eq!(prepared.session.game_id, "neverball");
        let UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = client.session_status().await.unwrap()
        else {
            panic!("native host must report the prepared session")
        };
        assert_eq!(active.launch_id, prepared.session.launch_id);
        assert_eq!(active.game_id.as_deref(), Some("neverball"));
        assert!(matches!(
            client
                .session_stop(&prepared.session.launch_id, false)
                .await
                .unwrap(),
            UpstreamSessionStop::Stopped { .. }
        ));
    }

    #[tokio::test]
    async fn native_client_freeze_requires_exact_tags_and_thaw_needs_focus_authority() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(
            &config,
            "label = \"zao\"\n[[games]]\nid = \"neverball\"\ntitle = \"Neverball\"\ncommand = [\"neverball\"]\n",
        )
        .unwrap();
        let base_url = serve(crate::host_router_with_in_memory_units(&config)).await;
        let client = NativeClient::new(base_url);
        let prepared = client.prepare_stream("neverball", None).await.unwrap();

        let frozen = client
            .session_freeze(&prepared.session.launch_id)
            .await
            .unwrap();
        assert_eq!(frozen.state, crate::SessionFreezerState::Frozen);
        assert!(frozen.changed);
        let again = client
            .session_freeze(&prepared.session.launch_id)
            .await
            .unwrap();
        assert!(!again.changed);
        assert!(matches!(
            client.session_thaw(&prepared.session.launch_id).await,
            Err(UpstreamError::Tagged { code, message })
                if code == "HostFocusFailed"
                    && message == "compositor focus authority is not configured"
        ));
        assert!(matches!(
            client.session_freeze("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").await,
            Err(UpstreamError::Tagged { code, .. }) if code == "StaleLaunchIdentity"
        ));

        // A response carrying a different tag is a wire error, never a success.
        let wrong = NativeClient::new(
            serve_response(
                axum::http::StatusCode::OK,
                r#"{"_tag":"app.session.resume","outcome":{"_tag":"Ok","payload":{"launchId":"x","state":"running","changed":true}}}"#,
            )
            .await,
        );
        assert!(matches!(
            wrong.session_freeze(&prepared.session.launch_id).await,
            Err(UpstreamError::Wire(_))
        ));
        let wrong = NativeClient::new(
            serve_response(
                axum::http::StatusCode::OK,
                r#"{"_tag":"app.session.leave","outcome":{"_tag":"Ok","payload":{"launchId":"x","state":"frozen","changed":true}}}"#,
            )
            .await,
        );
        assert!(matches!(
            wrong.session_thaw(&prepared.session.launch_id).await,
            Err(UpstreamError::Wire(_))
        ));

        let replaced = NativeClient::new(
            serve_response(
                axum::http::StatusCode::OK,
                r#"{"_tag":"app.session.resume","outcome":{"_tag":"Err","payload":{"code":"SelectedRemoteSessionReplaced","message":"replacement"}}}"#,
            )
            .await,
        );
        assert!(matches!(
            replaced.session_thaw(&prepared.session.launch_id).await,
            Err(UpstreamError::Tagged { code, .. })
                if code == "SelectedRemoteSessionReplaced"
        ));
    }

    #[tokio::test]
    async fn native_client_source_status_requires_exact_tag_and_forwards_the_key() {
        let ok = NativeClient::new(
            serve_response(
                axum::http::StatusCode::OK,
                r#"{"_tag":"app.source.status","outcome":{"_tag":"Ok","payload":{"catalog":"available","streamControl":"disabled"}}}"#,
            )
            .await,
        );
        let status = ok.source_status(&"ab".repeat(32)).await.unwrap();
        assert_eq!(status.catalog, crate::SourceCatalogState::Available);
        assert_eq!(
            status.stream_control,
            crate::SourceStreamControlState::Disabled
        );

        let tagged = NativeClient::new(
            serve_response(
                axum::http::StatusCode::OK,
                r#"{"_tag":"app.source.status","outcome":{"_tag":"Err","payload":{"code":"SourceDeviceMismatch","message":"m"}}}"#,
            )
            .await,
        );
        assert!(matches!(
            tagged.source_status(&"ab".repeat(32)).await,
            Err(UpstreamError::Tagged { code, .. }) if code == "SourceDeviceMismatch"
        ));

        // A response carrying a different tag is a wire error, never a status.
        let wrong = NativeClient::new(
            serve_response(
                axum::http::StatusCode::OK,
                r#"{"_tag":"system.health","outcome":{"_tag":"Ok","payload":{"version":"test"}}}"#,
            )
            .await,
        );
        assert!(matches!(
            wrong.source_status(&"ab".repeat(32)).await,
            Err(UpstreamError::Wire(_))
        ));
    }

    #[test]
    fn native_client_preserves_configured_ipv6_and_non_default_ports() {
        let client = NativeClient::new("http://[2001:db8::1]:49231/".into());
        assert_eq!(client.origins, vec!["http://[2001:db8::1]:49231"]);
    }

    #[tokio::test]
    async fn native_certificate_client_requires_exact_tags_and_redacts_certificates() {
        let server_pem = "-----BEGIN CERTIFICATE-----\nserver-secret\n-----END CERTIFICATE-----\n";
        let body = serde_json::json!({
            "_tag": "app.moonlight.certificate.provision",
            "outcome": {"_tag":"Ok", "payload":{"serverCertificate":server_pem}}
        })
        .to_string();
        let leaked: &'static str = Box::leak(body.into_boxed_str());
        let client = NativeClient::new(serve_response(axum::http::StatusCode::OK, leaked).await);
        let provisioned = client
            .moonlight_certificate_provision(
                "sunshine-host",
                "-----BEGIN CERTIFICATE-----\nclient-secret\n-----END CERTIFICATE-----\n",
            )
            .await
            .unwrap();
        assert_eq!(provisioned.server_certificate, server_pem);
        assert!(!format!("{provisioned:?}").contains("server-secret"));

        let wrong = NativeClient::new(
            serve_response(
                axum::http::StatusCode::OK,
                r#"{"_tag":"system.health","outcome":{"_tag":"Ok","payload":{"version":"test"}}}"#,
            )
            .await,
        )
        .moonlight_certificate_provision(
            "sunshine-host",
            "-----BEGIN CERTIFICATE-----\nclient-secret\n-----END CERTIFICATE-----\n",
        )
        .await
        .unwrap_err();
        assert!(matches!(
            wrong,
            UpstreamError::MoonlightCertificatePeerProtocol
        ));
        assert!(!wrong.to_string().contains("client-secret"));
    }

    #[tokio::test]
    async fn native_client_refuses_temporary_and_permanent_post_redirects() {
        for status in [
            axum::http::StatusCode::TEMPORARY_REDIRECT,
            axum::http::StatusCode::PERMANENT_REDIRECT,
        ] {
            let contacted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let target_contacts = contacted.clone();
            let target = serve(axum::Router::new().route(
                "/rpc",
                axum::routing::post(move || {
                    let target_contacts = target_contacts.clone();
                    async move {
                        target_contacts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        r#"{"_tag":"app.moonlight.certificate.attest","outcome":{"_tag":"Ok","payload":{"matched":true}}}"#
                    }
                }),
            ))
            .await;
            let redirect_target = format!("{target}/rpc");
            let redirect = serve(axum::Router::new().route(
                "/rpc",
                axum::routing::post(move || {
                    let redirect_target = redirect_target.clone();
                    async move {
                        axum::http::Response::builder()
                            .status(status)
                            .header(axum::http::header::LOCATION, redirect_target)
                            .body(axum::body::Body::empty())
                            .unwrap()
                    }
                }),
            ))
            .await;

            let error = NativeClient::new(redirect)
                .moonlight_certificate_attest("sunshine-host")
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                UpstreamError::MoonlightCertificatePeerUnavailable
            ));
            assert_eq!(contacted.load(std::sync::atomic::Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn native_client_bounds_declared_and_chunked_response_bodies() {
        let declared = serve(axum::Router::new().route(
            "/rpc",
            axum::routing::post(|| async {
                axum::http::Response::builder()
                    .status(axum::http::StatusCode::OK)
                    .header(
                        axum::http::header::CONTENT_LENGTH,
                        (MAX_NATIVE_RPC_RESPONSE_BYTES + 1).to_string(),
                    )
                    .body(axum::body::Body::empty())
                    .unwrap()
            }),
        ))
        .await;
        let declared_error = NativeClient::new(declared)
            .moonlight_certificate_attest("sunshine-host")
            .await
            .unwrap_err();
        assert!(matches!(
            declared_error,
            UpstreamError::MoonlightCertificatePeerProtocol
        ));

        let chunks = vec![
            axum::body::Bytes::from(vec![b'x'; MAX_NATIVE_RPC_RESPONSE_BYTES / 2 + 1]),
            axum::body::Bytes::from(vec![b'y'; MAX_NATIVE_RPC_RESPONSE_BYTES / 2 + 1]),
        ];
        let chunked = serve(axum::Router::new().route(
            "/rpc",
            axum::routing::post(move || {
                let stream = futures::stream::iter(
                    chunks
                        .clone()
                        .into_iter()
                        .map(Ok::<_, std::convert::Infallible>),
                );
                async move {
                    axum::http::Response::builder()
                        .status(axum::http::StatusCode::OK)
                        .body(axum::body::Body::from_stream(stream))
                        .unwrap()
                }
            }),
        ))
        .await;
        let chunked_error = NativeClient::new(chunked)
            .moonlight_certificate_attest("sunshine-host")
            .await
            .unwrap_err();
        assert!(matches!(
            chunked_error,
            UpstreamError::MoonlightCertificatePeerProtocol
        ));
    }

    #[tokio::test]
    async fn native_certificate_failures_are_stable_and_redacted() {
        let certificate = "-----BEGIN CERTIFICATE-----\nclient-secret\n-----END CERTIFICATE-----\n";
        for body in [
            "not json".to_owned(),
            serde_json::json!({
                "_tag": "app.moonlight.certificate.provision",
                "outcome": {
                    "_tag": "Err",
                    "payload": {
                        "code": "AnythingThePeerChooses",
                        "message": format!("echo {certificate} from http://secret-peer.invalid")
                    }
                }
            })
            .to_string(),
        ] {
            let body: &'static str = Box::leak(body.into_boxed_str());
            let error = NativeClient::new(serve_response(axum::http::StatusCode::OK, body).await)
                .moonlight_certificate_provision("sunshine-host", certificate)
                .await
                .unwrap_err();
            let text = error.to_string();
            assert!(!text.contains("client-secret"));
            assert!(!text.contains("secret-peer"));
            assert!(matches!(
                error,
                UpstreamError::MoonlightCertificatePeerProtocol
                    | UpstreamError::MoonlightCertificateRejected
            ));
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let unavailable_url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let error = NativeClient::new(unavailable_url.clone())
            .moonlight_certificate_provision("sunshine-host", certificate)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            UpstreamError::MoonlightCertificatePeerUnavailable
        ));
        assert!(!error.to_string().contains(&unavailable_url));
    }

    #[tokio::test]
    async fn native_client_executes_a_real_ipv6_request_when_loopback_is_available() {
        let listener = match tokio::net::TcpListener::bind("[::1]:0").await {
            Ok(listener) => listener,
            Err(_) => return,
        };
        let app = axum::Router::new().route(
            "/rpc",
            axum::routing::post(|| async {
                r#"{"_tag":"app.moonlight.certificate.attest","outcome":{"_tag":"Ok","payload":{"matched":true}}}"#
            }),
        );
        let client = NativeClient::new(serve_listener(listener, app).await);
        assert!(
            client
                .moonlight_certificate_attest("sunshine-host")
                .await
                .unwrap()
                .matched
        );
    }
}
