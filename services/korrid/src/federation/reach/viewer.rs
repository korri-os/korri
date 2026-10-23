//! Prepare one viewer from current peer facts. The session owns its lease.
use super::{nearby_seam, StreamLease};
use crate::{federation::FederationDirectory, upstreams::UpstreamError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SelectedViewer {
    pub(crate) reach: Option<StreamLease>,
    pub(crate) moonlight_address: String,
    pub(crate) confirmed_live: bool,
}

pub(crate) async fn prepare_peer_viewer(
    directory: Option<&FederationDirectory>,
    device: &str,
    current: Option<&str>,
) -> Result<SelectedViewer, UpstreamError> {
    let native = nearby_seam::moonlight_address(device, current);
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
