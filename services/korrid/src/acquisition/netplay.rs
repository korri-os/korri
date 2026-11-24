//! ADR 0040 and ADR 0092 (#554): the release that netplay needs arrives by
//! one install from devices, at the front of the line. The holding device is
//! its install source; its piece route admits the owner's and family
//! members' devices (ADR 0077). A file release and a release of several
//! files (a cue with its tracks, an m3u with its discs) are one collection.
use super::{
    game::GameContext,
    pieces::{DeviceInstallState, PeerHolder, Pieces},
    placement,
    space::Floor,
    AcquisitionCoordinator, Failure, Place,
};
use crate::{netplay::failure, RpcFailure};
use std::{path::Path, sync::Arc};

/// Room for an install of `bytes` on this device.
pub(crate) fn room(root: &Path, bytes: u64) -> bool {
    let cache = placement::install_cache(root);
    let directory = if cache.is_dir() {
        cache.as_path()
    } else {
        root
    };
    Floor::new(directory, None)
        .and_then(|floor| floor.room())
        .is_ok_and(|room| room >= bytes.saturating_add(8192))
}

/// The netplay answer for an install that did not end in the library. The
/// codes and the space message are netplay's wire (`surface/shared_play.rs`
/// reads the bytes from "The copy needs N bytes.").
fn netplay_failure(reason: &RpcFailure, bytes: u64) -> RpcFailure {
    if reason.message == Failure::NoSpace.rpc().message {
        failure("NotEnoughSpace", &format!("The copy needs {bytes} bytes."))
    } else {
        failure(
            "CopyFailed",
            "The release could not be installed from the holding device.",
        )
    }
}

/// A netplay that ends before its install does cancels the install it
/// started. An install that was already in the line, which the person or
/// play-here started, goes on.
struct Started {
    pieces: Pieces,
    job: String,
    armed: bool,
}
impl Drop for Started {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let (pieces, job) = (self.pieces.clone(), std::mem::take(&mut self.job));
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                pieces.cancel(&job).await;
            });
        }
    }
}

/// Install `key` from the holding device at the front of the line and wait
/// for it. `progress` hears `(bytes done, bytes total)` as pieces arrive.
/// Returns the library game id.
pub(crate) async fn install(
    coordinator: &AcquisitionCoordinator,
    context: &GameContext,
    key: &str,
    title: &str,
    bytes: u64,
    holder: PeerHolder,
    progress: &(dyn Fn(u64, u64) + Send + Sync),
) -> Result<String, RpcFailure> {
    let pieces = coordinator.pieces();
    let joined = pieces.live_job(key).is_some();
    let job = pieces
        .start(
            coordinator,
            context,
            key,
            title,
            vec![Arc::new(holder)],
            Place::Front,
        )
        .map_err(|reason| netplay_failure(&reason, bytes))?;
    let mut started = Started {
        pieces: pieces.clone(),
        job: job.clone(),
        armed: !joined,
    };
    let mut status = pieces
        .watch(&job)
        .ok_or_else(|| netplay_failure(&Failure::Storage.rpc(), bytes))?;
    loop {
        let state = status.borrow_and_update().state.clone();
        match state {
            DeviceInstallState::Waiting => (),
            DeviceInstallState::Installing {
                bytes_done,
                bytes_total,
            } => progress(bytes_done, bytes_total),
            DeviceInstallState::Imported { game_id } => {
                started.armed = false;
                return Ok(game_id);
            }
            // A failed install keeps its verified pieces for the next try,
            // and the person's list shows it until it is thrown away.
            DeviceInstallState::Failed { reason } => {
                started.armed = false;
                return Err(netplay_failure(&reason, bytes));
            }
            DeviceInstallState::Cancelled => {
                started.armed = false;
                return Err(failure("CopyFailed", "The install was cancelled."));
            }
        }
        if status.changed().await.is_err() {
            return Err(netplay_failure(&Failure::Storage.rpc(), bytes));
        }
    }
}
