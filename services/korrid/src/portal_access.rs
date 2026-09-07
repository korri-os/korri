//! Browser authority for the existing `/rpc` contract on either runtime.

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};

use crate::RpcRequest;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortalPermission {
    Full,
    ReadOnly,
}

impl PortalPermission {
    fn permits(self, request: &RpcRequest) -> bool {
        let initial_read = match request {
            RpcRequest::CatalogSnapshot(_)
            | RpcRequest::Health(_)
            | RpcRequest::LocalGamesList(_)
            | RpcRequest::SettingsSnapshot(_)
            | RpcRequest::DiscoverySnapshot(_)
            | RpcRequest::MoonlightResolve(_)
            | RpcRequest::SessionStatus(_) => true,
            RpcRequest::MoonlightLaunchPrepare(_)
            | RpcRequest::MoonlightLaunchCancel(_)
            | RpcRequest::MoonlightCertificateAttest(_)
            | RpcRequest::MoonlightCertificateProvision(_)
            | RpcRequest::MoonlightCertificateRevoke(_)
            | RpcRequest::SessionPrepare(_)
            | RpcRequest::SessionStop(_)
            | RpcRequest::SessionFreeze(_)
            | RpcRequest::SessionThaw(_)
            | RpcRequest::SourceStatus(_)
            | RpcRequest::SessionControls(_)
            | RpcRequest::SessionControlInvoke(_)
            | RpcRequest::LocalGameLaunch(_)
            | RpcRequest::DiscoveryRegisterReceipt(_)
            | RpcRequest::DiscoveryRemoveLocation(_)
            | RpcRequest::DiscoveryRescan(_)
            | RpcRequest::SettingsUpdate(_)
            | RpcRequest::SteamGridDbCredentialSet(_)
            | RpcRequest::SteamGridDbCredentialClear(_) => false,
        };
        self == Self::Full || initial_read
    }
}

/// Configuration from the existing token and portal-origin producers. The
/// capability is intentionally absent from Debug output and wire serialization.
#[derive(Clone)]
pub struct PortalAccess {
    capability: String,
    allowed_origins: Vec<HeaderValue>,
    permission: PortalPermission,
}

impl PortalAccess {
    pub fn new(capability: &str, allowed_origin: &str, permission: PortalPermission) -> Self {
        assert!(
            !capability.is_empty(),
            "portal capability must not be empty"
        );
        let origin: HeaderValue = allowed_origin
            .parse()
            .expect("allowed portal origin must be a valid header value");
        Self {
            capability: capability.into(),
            allowed_origins: vec![origin],
            permission,
        }
    }

    pub(crate) fn allow_bundled_android_origin(&mut self) {
        let origin = HeaderValue::from_static(crate::ANDROID_BUNDLED_PORTAL_ORIGIN);
        if !self.allowed_origins.contains(&origin) {
            self.allowed_origins.push(origin);
        }
    }

    pub(crate) fn allowed_origins(&self) -> &[HeaderValue] {
        &self.allowed_origins
    }

    pub(crate) fn authorize(
        &self,
        headers: &HeaderMap,
        request: &RpcRequest,
    ) -> Result<(), StatusCode> {
        let expected = format!("Bearer {}", self.capability);
        let mut authorizations = headers.get_all(header::AUTHORIZATION).iter();
        if authorizations.next().and_then(|value| value.to_str().ok()) != Some(expected.as_str())
            || authorizations.next().is_some()
        {
            return Err(StatusCode::UNAUTHORIZED);
        }
        // CORS alone hides a response; it does not prevent a request's effects.
        // A native caller can omit Origin, but must still possess the token.
        let mut origins = headers.get_all(header::ORIGIN).iter();
        if let Some(origin) = origins.next() {
            if !self.allowed_origins.contains(origin) || origins.next().is_some() {
                return Err(StatusCode::FORBIDDEN);
            }
        }
        if !self.permission.permits(request) {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(())
    }
}
