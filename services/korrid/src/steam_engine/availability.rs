//! Why the engine runner cannot run here (contract "Fallback states").
//! Games keep working on the client runner in every case; only a refusal by
//! Steam offers to install and select the client runner for now.
use crate::acquisition::{ArtifactState, ArtifactStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Unavailable {
    /// The host was built with the public, empty engine map.
    MapMissing,
    /// The engine file is not the build the map was made for.
    EngineMismatch,
    /// The pinned engine package is not downloaded on this device.
    PackageNotDownloaded,
    /// Steam refused the pinned engine at sign-in.
    EngineRefused { eresult: i64 },
}

impl Unavailable {
    /// Finished, user-facing copy.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::MapMissing => {
                "This Korri build has no map of Steam's engine. Steam games use the Steam client."
            }
            Self::EngineMismatch => {
                "The Steam engine on this device does not match the one Korri knows. Steam games use the Steam client."
            }
            Self::PackageNotDownloaded => {
                "Korri has not downloaded Steam's engine yet. Steam games use the Steam client until it has."
            }
            Self::EngineRefused { .. } => {
                "Steam no longer accepts the engine version Korri uses. You can use the Steam client until Korri updates."
            }
        }
    }

    pub fn offers_client_runner(&self) -> bool {
        matches!(self, Self::EngineRefused { .. })
    }

    /// The launch refusal code the surface sees.
    pub const CODE: &'static str = "SteamEngineUnavailable";
}

/// The engine package's state, from korrid's artifact status.
pub fn package(status: Option<&ArtifactStatus>) -> Option<Unavailable> {
    match status.map(|status| &status.state) {
        Some(ArtifactState::Ready { .. }) => None,
        _ => Some(Unavailable::PackageNotDownloaded),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_state_has_a_reason_and_only_a_refusal_offers_the_client() {
        for state in [
            Unavailable::MapMissing,
            Unavailable::EngineMismatch,
            Unavailable::PackageNotDownloaded,
            Unavailable::EngineRefused { eresult: 5 },
        ] {
            assert!(state.reason().ends_with('.'));
            assert_eq!(
                state.offers_client_runner(),
                matches!(state, Unavailable::EngineRefused { .. })
            );
        }
        assert_eq!(package(None), Some(Unavailable::PackageNotDownloaded));
    }
}
