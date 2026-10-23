//! The approved short-lived signal. The gift wrap authenticates its author;
//! the directory checks membership before binding this endpoint to that device.
use crate::relay::RelayError;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

pub(crate) const SIGNAL_LIFETIME: u64 = 60;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Offer {
    pub endpoint_id: String,
    pub direct_addresses: Vec<SocketAddr>,
    pub expires_at: u64,
    pub request_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Answer {
    pub endpoint_id: String,
    pub direct_addresses: Vec<SocketAddr>,
    pub expires_at: u64,
    pub request_id: String,
    pub can_relay_now: bool,
}

pub(crate) fn validate(
    endpoint: &str,
    addresses: &[SocketAddr],
    expires: u64,
    request: &str,
    now: u64,
) -> Result<iroh::EndpointId, RelayError> {
    if expires <= now || expires > now.saturating_add(SIGNAL_LIFETIME) {
        return Err(RelayError::Expired);
    }
    if request.len() != 64
        || !request.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || addresses.len() > 16
        || addresses.iter().any(|a| a.port() == 0 || a.ip().is_unspecified() || a.ip().is_multicast())
        || endpoint.len() != 64
    {
        return Err(RelayError::InvalidEvent("reach signal exceeds its bounds".into()));
    }
    endpoint.parse().map_err(|_| RelayError::InvalidEvent("reach endpoint id is invalid".into()))
}

impl Offer {
    pub(crate) fn validate(&self, now: u64) -> Result<iroh::EndpointId, RelayError> {
        validate(&self.endpoint_id, &self.direct_addresses, self.expires_at, &self.request_id, now)
    }
}
impl Answer {
    pub(crate) fn validate(&self, now: u64) -> Result<iroh::EndpointId, RelayError> {
        validate(&self.endpoint_id, &self.direct_addresses, self.expires_at, &self.request_id, now)
    }
}
