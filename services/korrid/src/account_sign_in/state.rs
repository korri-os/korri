//! One plugin's account sign-in as a pure state machine fed by its account
//! host's events, and its projection onto the treaty's `SurfaceAccountState`.
//!
//! korrid never reads the program's sign-in state. It learns it only from
//! the host: a `status` job, the QR flow and sign-out. The copy names the
//! plugin's own title; korrid names no service.
use super::{
    protocol::{Event, EXPIRED},
    qr,
};
use korri_surface_treaty::{SurfaceAccountState, SurfaceAction};

/// Action ids are `account-sign-in:<verb>:<plugin id>`.
pub const PREFIX: &str = "account-sign-in:";
pub const SIGN_IN: &str = "sign-in";
pub const CANCEL: &str = "cancel";
pub const SIGN_OUT: &str = "sign-out";
pub const RETRY: &str = "retry";

pub fn action_id(verb: &str, plugin: &str) -> String {
    format!("{PREFIX}{verb}:{plugin}")
}

/// `(verb, plugin id)` of an action id this module published.
pub fn parse_action(id: &str) -> Option<(&str, &str)> {
    let rest = id.strip_prefix(PREFIX)?;
    let (verb, plugin) = rest.split_once(':')?;
    [SIGN_IN, CANCEL, SIGN_OUT, RETRY]
        .contains(&verb)
        .then_some((verb, plugin))
}

/// `signin_failed` reasons that are not a failed sign-in, as Steam's: a QR
/// session is already running, or the account is signed in (its
/// `signed_in` follows).
const NOT_FAILED: [&str; 2] = ["in_progress", "already_signed_in"];

#[derive(Clone, Debug, PartialEq)]
pub enum Account {
    /// The plugin is off: nobody is signed in, and Sign in turns it on first.
    Idle,
    /// The status job has not answered yet.
    Checking,
    SignedOut {
        /// The service ended an earlier sign-in (`signed_out{reason:
        /// "expired"}`): it needs a new one.
        expired: bool,
    },
    /// Turning the plugin on, or waiting for its code.
    Starting,
    Code {
        challenge_url: String,
    },
    SignedIn {
        account_name: String,
    },
    Problem {
        problem: Problem,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The sign-in did not finish, or the host ended before it answered.
    SignIn,
    /// Korri could not turn the plugin on.
    NotEnabled,
    /// The plugin is on, but its service is not running yet.
    NotRunning,
}

impl Account {
    /// The next state after one host event. Events that do not concern the
    /// sign-in leave it as it is.
    pub fn apply(&self, event: &Event) -> Self {
        match event {
            Event::QrCode { challenge_url } | Event::QrRefreshed { challenge_url } => Self::Code {
                challenge_url: challenge_url.clone(),
            },
            Event::SignedIn { account_name } => Self::SignedIn {
                account_name: account_name.clone(),
            },
            Event::SigninFailed { reason } if NOT_FAILED.contains(&reason.as_str()) => self.clone(),
            Event::SigninFailed { .. } => Self::Problem {
                problem: Problem::SignIn,
            },
            // A QR session under way is not undone by a status line.
            Event::SignedOut { reason } => match self {
                Self::Code { .. } | Self::Starting => self.clone(),
                _ => Self::SignedOut {
                    expired: reason.as_deref() == Some(EXPIRED),
                },
            },
            Event::Other => self.clone(),
        }
    }

    pub fn signed_in(&self) -> bool {
        matches!(self, Self::SignedIn { .. })
    }

    /// The treaty value, in copy that names the plugin's `title`.
    pub fn surface(&self, plugin: &str, title: &str) -> SurfaceAccountState {
        let action =
            |verb: &str, label: &str| SurfaceAction::new(action_id(verb, plugin), label, true);
        match self {
            Self::Idle | Self::SignedOut { expired: false } => SurfaceAccountState::SignedOut {
                kicker: None,
                actions: vec![action(SIGN_IN, "Sign in")],
            },
            Self::SignedOut { expired: true } => SurfaceAccountState::SignedOut {
                kicker: Some(format!("{title} needs you to sign in again")),
                actions: vec![action(SIGN_IN, "Sign in")],
            },
            Self::Checking => SurfaceAccountState::Working {
                kicker: format!("Checking your {title} sign-in"),
                for_game: None,
                actions: vec![],
            },
            Self::Starting => SurfaceAccountState::Working {
                kicker: format!("Getting a sign-in code from {title}"),
                for_game: None,
                actions: vec![action(CANCEL, "Cancel")],
            },
            Self::Code { challenge_url } => match qr::grid(challenge_url) {
                Some(qr) => SurfaceAccountState::Code {
                    qr,
                    kicker: format!("Sign in to {title}"),
                    detail: Some(
                        "Scan this code with your phone, then approve the sign-in there.".into(),
                    ),
                    for_game: None,
                    actions: vec![action(CANCEL, "Cancel")],
                },
                None => SurfaceAccountState::Problem {
                    kicker: format!("{title} sign-in did not finish"),
                    reason: format!("{title} sent a sign-in code Korri cannot show."),
                    actions: vec![action(RETRY, "Try again")],
                },
            },
            Self::SignedIn { account_name } => SurfaceAccountState::SignedIn {
                account_name: account_name.clone(),
                actions: vec![action(SIGN_OUT, "Sign out")],
            },
            Self::Problem {
                problem: Problem::SignIn,
            } => SurfaceAccountState::Problem {
                kicker: format!("{title} sign-in did not finish"),
                reason: format!(
                    "{title} did not answer, or the code ran out. Check the network and try again."
                ),
                actions: vec![action(RETRY, "Try again")],
            },
            Self::Problem {
                problem: Problem::NotEnabled,
            } => SurfaceAccountState::Problem {
                kicker: format!("{title} could not be turned on"),
                reason: format!("Korri could not start {title}. Try again."),
                actions: vec![action(RETRY, "Try again")],
            },
            Self::Problem {
                problem: Problem::NotRunning,
            } => SurfaceAccountState::Problem {
                kicker: format!("{title} is starting"),
                reason: format!("{title} is not running yet. Try again in a moment."),
                actions: vec![action(RETRY, "Try again")],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(state: &SurfaceAccountState) -> Vec<String> {
        let actions = match state {
            SurfaceAccountState::SignedOut { actions, .. }
            | SurfaceAccountState::Code { actions, .. }
            | SurfaceAccountState::Working { actions, .. }
            | SurfaceAccountState::SignedIn { actions, .. }
            | SurfaceAccountState::Problem { actions, .. } => actions,
        };
        actions.iter().map(|a| a.id.clone()).collect()
    }

    const ID: &str = "@korri:tailscale";

    #[test]
    fn the_qr_flow_runs_from_signed_out_to_signed_in_and_back() {
        let mut account = Account::Checking.apply(&Event::SignedOut { reason: None });
        assert_eq!(account, Account::SignedOut { expired: false });
        assert_eq!(
            ids(&account.surface(ID, "Tailscale")),
            ["account-sign-in:sign-in:@korri:tailscale"]
        );
        account = Account::Starting.apply(&Event::SignedOut { reason: None });
        assert_eq!(account, Account::Starting, "a QR session is not undone");
        account = account.apply(&Event::QrCode {
            challenge_url: "https://login.example/a/1".into(),
        });
        let SurfaceAccountState::Code {
            qr,
            kicker,
            actions,
            ..
        } = account.surface(ID, "Tailscale")
        else {
            panic!("{account:?}")
        };
        assert_eq!(kicker, "Sign in to Tailscale");
        assert_eq!(actions[0].id, action_id(CANCEL, ID));
        assert_eq!(qr, qr::grid("https://login.example/a/1").unwrap());
        let refreshed = account.apply(&Event::QrRefreshed {
            challenge_url: "https://login.example/a/2".into(),
        });
        assert_ne!(refreshed, account);
        let signed_in = refreshed.apply(&Event::SignedIn {
            account_name: "p@example.com".into(),
        });
        assert_eq!(
            signed_in.surface(ID, "Tailscale"),
            SurfaceAccountState::SignedIn {
                account_name: "p@example.com".into(),
                actions: vec![SurfaceAction::new(
                    action_id(SIGN_OUT, ID),
                    "Sign out",
                    true
                )],
            }
        );
        assert_eq!(
            signed_in.apply(&Event::SignedOut { reason: None }),
            Account::SignedOut { expired: false }
        );
    }

    #[test]
    fn an_expired_sign_in_is_signed_out_with_a_kicker_that_says_so() {
        let expired = Account::SignedIn {
            account_name: "p".into(),
        }
        .apply(&Event::SignedOut {
            reason: Some("expired".into()),
        });
        assert_eq!(expired, Account::SignedOut { expired: true });
        let SurfaceAccountState::SignedOut { kicker, actions } = expired.surface(ID, "Tailscale")
        else {
            panic!()
        };
        assert_eq!(
            kicker.as_deref(),
            Some("Tailscale needs you to sign in again")
        );
        assert_eq!(actions[0].id, action_id(SIGN_IN, ID));
    }

    #[test]
    fn failures_offer_retry_and_in_progress_is_not_a_failure() {
        let code = Account::Code {
            challenge_url: "u".into(),
        };
        assert_eq!(
            code.apply(&Event::SigninFailed {
                reason: "in_progress".into()
            }),
            code
        );
        let failed = code.apply(&Event::SigninFailed {
            reason: "timeout".into(),
        });
        assert_eq!(ids(&failed.surface(ID, "T")), [action_id(RETRY, ID)]);
        for problem in [Problem::NotEnabled, Problem::NotRunning] {
            assert_eq!(
                ids(&Account::Problem { problem }.surface(ID, "T")),
                [action_id(RETRY, ID)]
            );
        }
        assert_eq!(code.apply(&Event::Other), code);
    }

    #[test]
    fn action_ids_round_trip_and_others_are_refused() {
        assert_eq!(parse_action(&action_id(SIGN_OUT, ID)), Some((SIGN_OUT, ID)));
        assert_eq!(parse_action("account-sign-in:delete:@korri:x"), None);
        assert_eq!(parse_action("steam-account:sign-in"), None);
    }
}
