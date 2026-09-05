use crate::{
    CatalogSnapshot, CatalogSnapshotOutcome, MoonlightCertificateAttestOutcome,
    MoonlightCertificateAttestRequest, MoonlightCertificateAttested,
    MoonlightCertificateProvisionOutcome, MoonlightCertificateProvisionRequest,
    MoonlightCertificateProvisioned, MoonlightCertificateRevokeOutcome,
    MoonlightCertificateRevokeRequest, MoonlightCertificateRevoked, RpcFailure, RpcRequest,
    RpcResponse, SessionPrepareOutcome, SessionPrepareRequest, SessionPrepared,
    SessionStatusOutcome, SessionStatusRequest, SessionStopOutcome, SessionStopPhase,
    SessionStopRequest,
};
use std::time::Duration;

use crate::{
    peer_rpc::{unix_time, PeerCredentials},
    upstream::{UpstreamActiveSession, UpstreamSessionStatus, UpstreamSessionStop},
    upstreams::UpstreamError,
};

const MAX_NATIVE_RPC_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const NATIVE_RPC_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct NativeClient {
    rpc_url: String,
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
        let http = reqwest::Client::builder()
            .timeout(NATIVE_RPC_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("construct native upstream client");
        Self {
            rpc_url: format!("{}/peer-rpc", base_url.trim_end_matches('/')),
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
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("construct native upstream client");
        Self {
            rpc_url: format!("{}/rpc", base_url.trim_end_matches('/')),
            expected_peer_public_key: None,
            credentials: None,
            http,
            fixed_request: None,
        }
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
            UpstreamError::Tagged { code, .. } if code == "SunshineCertificateControlBusy" => {
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
            | UpstreamError::AmbiguousActiveSessions
            | UpstreamError::SelectedRemoteSessionReplaced
            | UpstreamError::NativeSessionRecoveryIncomplete
            | UpstreamError::ActiveRemoteSessionConflict
            | UpstreamError::ExpectedLaunchIdRequired
            | UpstreamError::StaleLaunchIdentity
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

    pub async fn prepare_stream(&self, game_id: &str) -> Result<SessionPrepared, UpstreamError> {
        match self
            .call(RpcRequest::SessionPrepare(SessionPrepareRequest {
                game_id: game_id.into(),
                host: None,
            }))
            .await?
        {
            RpcResponse::SessionPrepare(SessionPrepareOutcome::Ok(prepared)) => Ok(prepared),
            RpcResponse::SessionPrepare(SessionPrepareOutcome::Err(failure)) => {
                Err(Self::tagged_failure(failure))
            }
            response => Err(UpstreamError::Wire(format!(
                "native prepare returned {response:?}"
            ))),
        }
    }

    pub async fn session_status(&self) -> Result<UpstreamSessionStatus, UpstreamError> {
        match self
            .call(RpcRequest::SessionStatus(SessionStatusRequest {}))
            .await?
        {
            RpcResponse::SessionStatus(SessionStatusOutcome::Ok(status)) => {
                Ok(UpstreamSessionStatus::SessionStatus {
                    active: status.active.map(|active| UpstreamActiveSession {
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

    pub async fn session_stop(
        &self,
        expected_launch_id: &str,
        force: bool,
    ) -> Result<UpstreamSessionStop, UpstreamError> {
        match self
            .call(RpcRequest::SessionStop(SessionStopRequest {
                force: force.then_some(true),
                expected_launch_id: Some(expected_launch_id.into()),
            }))
            .await?
        {
            RpcResponse::SessionStop(SessionStopOutcome::Ok(result)) => match result.phase {
                SessionStopPhase::Stopped => Ok(UpstreamSessionStop::Stopped {
                    launch_id: Some(expected_launch_id.into()),
                }),
                SessionStopPhase::Pending => Ok(UpstreamSessionStop::StopPending {
                    launch_id: Some(expected_launch_id.into()),
                }),
            },
            RpcResponse::SessionStop(SessionStopOutcome::Err(failure)) => {
                Err(Self::tagged_failure(failure))
            }
            response => Err(UpstreamError::Wire(format!(
                "native session stop returned {response:?}"
            ))),
        }
    }

    async fn call(&self, request: RpcRequest) -> Result<RpcResponse, UpstreamError> {
        #[cfg(test)]
        if self.credentials.is_none() {
            let response = self
                .http
                .post(&self.rpc_url)
                .json(&request)
                .send()
                .await
                .map_err(|error| UpstreamError::Unreachable(error.to_string()))?;
            return read_plain_test_response(response).await;
        }
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
        let response = self
            .http
            .post(&self.rpc_url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(encoded.event_json.clone())
            .send()
            .await
            .map_err(|error| UpstreamError::Unreachable(error.to_string()))?;
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
        let mut response = response;
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
            .map_err(|error| UpstreamError::Wire(error.to_string()))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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

        let prepared = client.prepare_stream("neverball").await.unwrap();
        assert_eq!(prepared.game_id, "neverball");
        let UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = client.session_status().await.unwrap()
        else {
            panic!("native host must report the prepared session")
        };
        assert_eq!(active.launch_id, prepared.launch_id);
        assert_eq!(active.game_id.as_deref(), Some("neverball"));
        assert!(matches!(
            client
                .session_stop(&prepared.launch_id, false)
                .await
                .unwrap(),
            UpstreamSessionStop::Stopped { .. }
        ));
    }

    #[test]
    fn native_client_preserves_configured_ipv6_and_non_default_ports() {
        let client = NativeClient::new("http://[2001:db8::1]:49231/".into());
        assert_eq!(client.rpc_url, "http://[2001:db8::1]:49231/rpc");
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
