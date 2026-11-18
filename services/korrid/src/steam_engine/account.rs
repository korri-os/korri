//! The current person's Steam sign-in on this device, as a pure state
//! machine fed by host events, and its projection onto the treaty's
//! `SurfaceSteamAccount` (ADR 0083, contract "Surface treaty").
//!
//! korrid never reads the engine's sign-in stores. It learns the state only
//! from the host: a status job at start (the host signs in from the engine's
//! own login, or says it has none), the QR flow, and sign-out.
use super::{availability::Unavailable, protocol::Event, qr};
use korri_surface_treaty::{SurfaceAction, SurfaceSteamAccount};

pub const SIGN_IN: &str = "steam-account:sign-in";
pub const CANCEL: &str = "steam-account:cancel";
pub const SIGN_OUT: &str = "steam-account:sign-out";
pub const RETRY: &str = "steam-account:retry";
/// Only when Steam refused the pinned engine: install and select the client
/// runner as a temporary fallback.
pub const USE_CLIENT: &str = "steam-account:use-client-runner";

/// `signin_failed` reasons that are not a failed sign-in: a QR session is
/// already running, or the engine is signed in (its `signed_in` follows).
const NOT_FAILED: [&str; 2] = ["in_progress", "already_signed_in"];

#[derive(Clone, Debug, PartialEq)]
pub enum Account {
    /// The status job has not answered yet.
    Checking,
    SignedOut,
    /// A QR session is starting; no code yet.
    Starting {
        for_game: Option<String>,
    },
    Code {
        challenge_url: String,
        for_game: Option<String>,
    },
    /// Approved; the engine is signing in.
    Working {
        for_game: Option<String>,
    },
    SignedIn {
        account_name: String,
    },
    Problem {
        problem: Problem,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Problem {
    /// The sign-in did not finish; Retry starts it again.
    SignIn { for_game: Option<String> },
    /// The engine runner cannot run on this device.
    Unavailable(Unavailable),
}

impl Account {
    pub fn for_game(&self) -> Option<&str> {
        match self {
            Self::Starting { for_game }
            | Self::Code { for_game, .. }
            | Self::Working { for_game }
            | Self::Problem {
                problem: Problem::SignIn { for_game },
            } => for_game.as_deref(),
            _ => None,
        }
    }

    pub fn signed_in(&self) -> bool {
        matches!(self, Self::SignedIn { .. })
    }

    /// The next state after one host event. Events that do not concern the
    /// sign-in leave it as it is.
    pub fn apply(&self, event: &Event) -> Self {
        let for_game = self.for_game().map(str::to_owned);
        match event {
            Event::EngineMapMissing {} => Self::unavailable(Unavailable::MapMissing),
            Event::EngineMismatch {} => Self::unavailable(Unavailable::EngineMismatch),
            Event::EngineRefused { eresult } => {
                Self::unavailable(Unavailable::EngineRefused { eresult: *eresult })
            }
            Event::QrCode { challenge_url } | Event::QrRefreshed { challenge_url } => Self::Code {
                challenge_url: challenge_url.clone(),
                for_game,
            },
            Event::SignedIn { account_name } => Self::SignedIn {
                account_name: account_name.clone(),
            },
            Event::SigninFailed { reason } if NOT_FAILED.contains(&reason.as_str()) => self.clone(),
            Event::SigninFailed { .. } => Self::Problem {
                problem: Problem::SignIn { for_game },
            },
            // At start a host with no saved login says `signed_out`: sign-in
            // needed. A QR session already under way is not undone by it.
            Event::SignedOut {} => match self {
                Self::Code { .. } | Self::Working { .. } | Self::Starting { .. } => self.clone(),
                _ => Self::SignedOut,
            },
            Event::LogonState {
                logged_on: true, ..
            } => match self {
                Self::Code { .. } | Self::Starting { .. } => Self::Working { for_game },
                other => other.clone(),
            },
            _ => self.clone(),
        }
    }

    fn unavailable(reason: Unavailable) -> Self {
        Self::Problem {
            problem: Problem::Unavailable(reason),
        }
    }

    /// The treaty value. `None` only while the status job runs and nothing
    /// is known: a surface then shows no Steam entry rather than a guess.
    pub fn surface(&self) -> Option<SurfaceSteamAccount> {
        let action = |id: &str, label: &str| SurfaceAction::new(id, label, true);
        Some(match self {
            Self::Checking => SurfaceSteamAccount::Working {
                kicker: "Checking your Steam sign-in".into(),
                for_game: None,
                actions: vec![],
            },
            Self::SignedOut => SurfaceSteamAccount::SignedOut {
                actions: vec![action(SIGN_IN, "Sign in")],
            },
            Self::Starting { for_game } => SurfaceSteamAccount::Working {
                kicker: "Getting a sign-in code from Steam".into(),
                for_game: for_game.clone(),
                actions: vec![action(CANCEL, "Cancel")],
            },
            Self::Code {
                challenge_url,
                for_game,
            } => match qr::grid(challenge_url) {
                Some(qr) => SurfaceSteamAccount::Code {
                    qr,
                    kicker: "Sign in to Steam".into(),
                    detail: Some(
                        "Open the Steam app on your phone, scan this code, and approve the sign-in."
                            .into(),
                    ),
                    for_game: for_game.clone(),
                    actions: vec![action(CANCEL, "Cancel")],
                },
                None => SurfaceSteamAccount::Problem {
                    kicker: "Steam sign-in did not finish".into(),
                    reason: "Steam sent a sign-in code Korri cannot show.".into(),
                    actions: vec![action(RETRY, "Try again")],
                },
            },
            Self::Working { for_game } => SurfaceSteamAccount::Working {
                kicker: "Signing in to Steam".into(),
                for_game: for_game.clone(),
                actions: vec![],
            },
            Self::SignedIn { account_name } => SurfaceSteamAccount::SignedIn {
                account_name: account_name.clone(),
                actions: vec![action(SIGN_OUT, "Sign out")],
            },
            Self::Problem {
                problem: Problem::SignIn { .. },
            } => SurfaceSteamAccount::Problem {
                kicker: "Steam sign-in did not finish".into(),
                reason: "Steam did not answer, or the code ran out. Check the network and try again."
                    .into(),
                actions: vec![action(RETRY, "Try again")],
            },
            Self::Problem {
                problem: Problem::Unavailable(reason),
            } => SurfaceSteamAccount::Problem {
                kicker: "Steam without the client is unavailable".into(),
                reason: reason.reason().into(),
                actions: if reason.offers_client_runner() {
                    vec![action(USE_CLIENT, "Use the Steam client for now")]
                } else if *reason == Unavailable::PackageNotDownloaded {
                    // Sign in downloads the pinned engine first.
                    vec![action(SIGN_IN, "Download and sign in")]
                } else {
                    vec![]
                },
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(account: &SurfaceSteamAccount) -> Vec<String> {
        let actions = match account {
            SurfaceSteamAccount::SignedOut { actions }
            | SurfaceSteamAccount::Code { actions, .. }
            | SurfaceSteamAccount::Working { actions, .. }
            | SurfaceSteamAccount::SignedIn { actions, .. }
            | SurfaceSteamAccount::Problem { actions, .. } => actions,
        };
        actions.iter().map(|a| a.id.clone()).collect()
    }

    #[test]
    fn the_qr_flow_runs_from_signed_out_to_signed_in_with_its_actions() {
        let mut account = Account::Checking;
        account = account.apply(&Event::SignedOut {});
        assert_eq!(account, Account::SignedOut);
        assert_eq!(ids(&account.surface().unwrap()), [SIGN_IN]);

        account = Account::Starting {
            for_game: Some("Celeste".into()),
        };
        assert_eq!(ids(&account.surface().unwrap()), [CANCEL]);
        account = account.apply(&Event::QrCode {
            challenge_url: "https://s.team/q/1/111".into(),
        });
        let SurfaceSteamAccount::Code {
            qr,
            for_game,
            actions,
            ..
        } = account.surface().unwrap()
        else {
            panic!("{account:?}")
        };
        assert_eq!(for_game.as_deref(), Some("Celeste"));
        assert_eq!(actions[0].id, CANCEL);
        assert_eq!(qr.modules.len(), (qr.size * qr.size) as usize);

        // Steam rotates the challenge about every 20 s: a new grid.
        let rotated = account.apply(&Event::QrRefreshed {
            challenge_url: "https://s.team/q/1/222".into(),
        });
        let SurfaceSteamAccount::Code { qr: next, .. } = rotated.surface().unwrap() else {
            panic!()
        };
        assert_ne!(next, qr);

        let working = rotated.apply(&Event::LogonState {
            logged_on: true,
            offline: false,
        });
        assert_eq!(
            working,
            Account::Working {
                for_game: Some("Celeste".into())
            }
        );
        let signed_in = working.apply(&Event::SignedIn {
            account_name: "korri_player".into(),
        });
        assert_eq!(
            signed_in.surface(),
            Some(SurfaceSteamAccount::SignedIn {
                account_name: "korri_player".into(),
                actions: vec![SurfaceAction::new(SIGN_OUT, "Sign out", true)],
            })
        );
        assert_eq!(signed_in.apply(&Event::SignedOut {}), Account::SignedOut);
    }

    #[test]
    fn a_failed_sign_in_offers_retry_and_keeps_the_game_that_asked() {
        let account = Account::Code {
            challenge_url: "https://s.team/q/1/1".into(),
            for_game: Some("Celeste".into()),
        }
        .apply(&Event::SigninFailed {
            reason: "steam_unavailable".into(),
        });
        assert_eq!(account.for_game(), Some("Celeste"));
        assert_eq!(ids(&account.surface().unwrap()), [RETRY]);
        // The start notice and a second request do not undo a QR session.
        let code = Account::Code {
            challenge_url: "u".into(),
            for_game: None,
        };
        assert_eq!(code.apply(&Event::SignedOut {}), code);
        assert_eq!(
            code.apply(&Event::SigninFailed {
                reason: "in_progress".into()
            }),
            code
        );
    }

    #[test]
    fn engine_states_are_problems_and_only_a_refusal_offers_the_client_runner() {
        let missing = Account::Checking.apply(&Event::EngineMapMissing {});
        let mismatch = Account::Checking.apply(&Event::EngineMismatch {});
        let refused = Account::SignedOut.apply(&Event::EngineRefused { eresult: 5 });
        for account in [&missing, &mismatch] {
            assert!(ids(&account.surface().unwrap()).is_empty());
        }
        assert_eq!(ids(&refused.surface().unwrap()), [USE_CLIENT]);
        let SurfaceSteamAccount::Problem { reason, .. } = mismatch.surface().unwrap() else {
            panic!()
        };
        assert!(reason.contains("does not match"), "{reason}");
    }
}
