//! Prepare one viewer from current peer facts. The session owns its lease.
use super::StreamLease;
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
    let reach = match directory {
        Some(directory) => {
            directory
                .activate_stream(device)
                .await
                .map_err(|error| UpstreamError::Tagged {
                    code: "StreamForwardUnavailable".into(),
                    message: error.to_string(),
                })?
        }
        None => None,
    };
    let address = reach
        .as_ref()
        .and_then(StreamLease::address)
        .or_else(|| current.map(str::to_owned))
        .filter(|address| !address.trim().is_empty())
        .ok_or(UpstreamError::MoonlightHostCandidatesUnavailable)?;
    Ok(SelectedViewer {
        reach,
        moonlight_address: address,
        confirmed_live: false,
    })
}
