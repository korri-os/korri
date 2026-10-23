//! Prepare one viewer from current peer facts. The session owns its lease.
use super::{nearby_seam, StreamLease};
use crate::{federation::FederationDirectory, upstreams::UpstreamError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SelectedViewer {
    pub(crate) reach: Option<StreamLease>,
    pub(crate) moonlight_address: String,
    pub(crate) confirmed_live: bool,
}

fn is_forward_address(directory: Option<&FederationDirectory>, address: &str) -> bool {
    let Some(origin) = nearby_seam::moonlight_control_origin(address) else {
        return false;
    };
    // The selected-session forwards reserve these IPv6 loopback ports.
    let reserved = url::Url::parse(&origin).is_ok_and(|url| {
        matches!(url.host(), Some(url::Host::Ipv6(ip)) if ip.is_loopback())
            && url
                .port_or_known_default()
                .is_some_and(|port| super::tunnel::TCP_PORTS.contains(&port))
    });
    reserved || directory.is_some_and(|directory| directory.reach.owns_forward_origin(&origin))
}

pub(crate) async fn prepare_peer_viewer(
    directory: Option<&FederationDirectory>,
    device: &str,
    current: Option<&str>,
) -> Result<SelectedViewer, UpstreamError> {
    let native = nearby_seam::moonlight_address(device, current)
        .filter(|address| !is_forward_address(directory, address));
    let mut reach = None;
    if let Some(directory) = directory {
        if directory.has_iroh_route(device) {
            // An RPC reply does not prove that Sunshine can accept a connection.
            let direct = match native
                .as_deref()
                .and_then(nearby_seam::moonlight_control_origin)
            {
                Some(origin) => crate::federation::connect::select_origin(&[origin])
                    .await
                    .is_ok(),
                None => false,
            };
            if !direct {
                reach = directory.activate_stream(device).await.map_err(|error| {
                    UpstreamError::Tagged {
                        code: "StreamForwardUnavailable".into(),
                        message: error.to_string(),
                    }
                })?;
            }
        }
    }
    let forwarded = reach.as_ref().and_then(StreamLease::address);
    let address = forwarded
        .as_deref()
        .or(native.as_deref())
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .ok_or(UpstreamError::MoonlightHostCandidatesUnavailable)?
        .to_owned();
    Ok(SelectedViewer {
        reach,
        moonlight_address: address,
        confirmed_live: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_ipv6_forward_ports_are_not_native_addresses() {
        for port in super::super::tunnel::TCP_PORTS {
            assert!(is_forward_address(None, &format!("[::1]:{port}")));
            assert!(!is_forward_address(None, &format!("192.0.2.1:{port}")));
        }
        assert!(is_forward_address(None, "[::1]"));
        assert!(is_forward_address(None, "[0:0:0:0:0:0:0:1]:47989"));
    }
}
