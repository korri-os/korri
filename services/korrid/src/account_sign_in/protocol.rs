//! The account host's line protocol: the account subset of the Steam engine
//! host's (`steam_engine/protocol.rs`), with the same command words, event
//! names and fields.
//!
//! korrid writes one command per line; the program writes one JSON object
//! per line with an `event` name. korrid reads only the fields below and
//! ignores every other event and field. The program ends when its input
//! ends: korrid closes the connection to end a job.
use serde::Deserialize;

/// One command line korrid writes to the account host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    /// Report who is signed in: `signed_in` or `signed_out`.
    Status,
    /// Start a QR sign-in: `qr_code`, any `qr_refreshed`, then `signed_in`
    /// or `signin_failed`.
    SignInQr,
    /// End the sign-in: `signed_out`.
    SignOut,
}

impl Command {
    /// The line, with its newline. No command carries a value.
    pub fn line(self) -> &'static str {
        match self {
            Self::Status => "status\n",
            Self::SignInQr => "signin-qr\n",
            Self::SignOut => "signout\n",
        }
    }
}

/// `signed_out` with this reason: the sign-in ran out and the service needs
/// a new one.
pub const EXPIRED: &str = "expired";

/// The events korrid acts on. Everything else is `Other`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    QrCode {
        challenge_url: String,
    },
    QrRefreshed {
        challenge_url: String,
    },
    SignedIn {
        account_name: String,
    },
    SigninFailed {
        reason: String,
    },
    SignedOut {
        #[serde(default)]
        reason: Option<String>,
    },
    #[serde(other)]
    Other,
}

/// A reason as korrid logs it: no control characters, at most 200 bytes.
/// A reason is the program's word; it is never shown to the person.
pub fn loggable(reason: &str) -> String {
    let mut out = String::new();
    for c in reason.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > 200 {
            break;
        }
        out.push(c);
    }
    out
}

/// One output line, or `None` when it is not a JSON event. A line is never
/// logged: it may carry a login link.
pub fn parse(line: &str) -> Option<Event> {
    serde_json::from_str(line.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_the_steam_hosts_words() {
        use crate::steam_engine::protocol::Command as Steam;
        assert_eq!(Command::SignInQr.line(), Steam::SignInQr.line());
        assert_eq!(Command::SignOut.line(), Steam::SignOut.line());
        assert_eq!(Command::Status.line(), "status\n");
    }

    #[test]
    fn a_logged_reason_is_bounded_and_has_no_control_characters() {
        assert_eq!(loggable("time\nout\u{1b}[31m"), "timeout[31m");
        let long = "é".repeat(300);
        let logged = loggable(&long);
        assert!(logged.len() <= 200 && logged.len() >= 198);
        assert!(logged.chars().all(|c| c == 'é'));
    }

    #[test]
    fn account_events_parse_and_others_are_ignored() {
        assert_eq!(
            parse(r#"{"event":"qr_code","challenge_url":"https://login.example/a/1"}"#),
            Some(Event::QrCode {
                challenge_url: "https://login.example/a/1".into()
            })
        );
        assert_eq!(
            parse(r#"{"event":"qr_refreshed","challenge_url":"u","extra":1}"#),
            Some(Event::QrRefreshed {
                challenge_url: "u".into()
            })
        );
        assert_eq!(
            parse(r#"{"event":"signed_in","account_name":"p@example.com"}"#),
            Some(Event::SignedIn {
                account_name: "p@example.com".into()
            })
        );
        assert_eq!(
            parse(r#"{"event":"signin_failed","reason":"timeout"}"#),
            Some(Event::SigninFailed {
                reason: "timeout".into()
            })
        );
        assert_eq!(
            parse(r#"{"event":"signed_out","reason":"expired"}"#),
            Some(Event::SignedOut {
                reason: Some(EXPIRED.into())
            })
        );
        assert_eq!(
            parse(r#"{"event":"signed_out"}"#),
            Some(Event::SignedOut { reason: None })
        );
        assert_eq!(
            parse(r#"{"event":"phase","phase":"x"}"#),
            Some(Event::Other)
        );
        assert_eq!(parse("not json"), None);
        assert_eq!(parse(r#"{"event":"signed_in"}"#), None);
    }
}
