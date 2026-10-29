//! Pure mapping: session state to what the window says, and container size
//! to layout. No korrid calls, no Slint types, so both are unit-tested.

use std::time::Duration;

use korrid::identity::IdentityState;

use crate::brain::{Inspection, Problem};

/// The one state store. The window receives a `Report` derived from it.
pub struct Session {
    pub build_id: &'static str,
    pub refreshing: bool,
    /// Completed refreshes.
    pub checks: u32,
    pub last: Option<Inspection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Checking,
    Healthy,
    Partial,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    pub label: String,
    pub value: String,
    /// One long token (a key): wrap between characters, not words.
    pub unbroken: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub tone: Tone,
    pub headline: String,
    pub detail: String,
    pub connection: Vec<Fact>,
    pub identity: Vec<Fact>,
    pub catalog_summary: String,
    pub games: Vec<String>,
    pub peers_summary: String,
    pub peers: Vec<String>,
    pub build: Vec<Fact>,
    pub refreshing: bool,
    pub checks: u32,
}

fn fact(label: &str, value: impl Into<String>) -> Fact {
    Fact {
        label: label.into(),
        value: value.into(),
        unbroken: false,
    }
}

fn key(label: &str, value: &str) -> Fact {
    Fact {
        unbroken: true,
        ..fact(label, value)
    }
}

pub fn describe(problem: &Problem) -> String {
    match problem {
        Problem::Unauthorized(status) => {
            format!("korrid refused the credential (HTTP {status})")
        }
        Problem::Unreachable(reason) => format!("no answer from korrid: {reason}"),
        Problem::Rejected { code, message } => format!("{code}: {message}"),
        Problem::Unexpected(reason) => format!("unexpected answer: {reason}"),
    }
}

fn milliseconds(elapsed: Duration) -> String {
    format!("{} ms", elapsed.as_millis())
}

pub fn present(session: &Session) -> Report {
    let build = |version: &str| {
        vec![
            fact(
                "This window",
                format!(
                    "korri-slint-spike {} ({})",
                    env!("CARGO_PKG_VERSION"),
                    session.build_id
                ),
            ),
            fact("korrid", version.to_owned()),
            fact("Storage", "Disposable scratch, removed on exit"),
        ]
    };
    let Some(last) = &session.last else {
        return Report {
            tone: Tone::Checking,
            headline: "Checking korrid".into(),
            detail: "Asking the embedded korrid for health, identity, catalog and peers.".into(),
            connection: vec![fact("Transport", "Loopback HTTP, waiting for first answer")],
            identity: Vec::new(),
            catalog_summary: "Not checked yet.".into(),
            games: Vec::new(),
            peers_summary: "Not checked yet.".into(),
            peers: Vec::new(),
            build: build("not checked yet"),
            refreshing: session.refreshing,
            checks: session.checks,
        };
    };

    let requests = [
        ("system.health", last.health.as_ref().err()),
        ("system.identity.status", last.identity.as_ref().err()),
        ("app.catalog.snapshot", last.catalog.as_ref().err()),
        ("app.peer.list", last.peers.as_ref().err()),
    ];
    let failures: Vec<String> = requests
        .iter()
        .filter_map(|(tag, problem)| problem.map(|problem| format!("{tag}: {}", describe(problem))))
        .collect();
    let answered = requests.len() - failures.len();

    // Health decides whether korrid is healthy at all. A rejected credential
    // or a closed port is never shown as healthy, whatever else answered.
    let (mut tone, mut headline, mut detail) = match &last.health {
        Ok(_) if failures.is_empty() => (
            Tone::Healthy,
            "korrid is answering".to_owned(),
            format!(
                "{answered} of {} requests answered in {}.",
                requests.len(),
                milliseconds(last.elapsed)
            ),
        ),
        Ok(_) => (
            Tone::Partial,
            "korrid answered with problems".to_owned(),
            format!(
                "{answered} of {} requests answered. {}",
                requests.len(),
                failures.join(". ")
            ),
        ),
        Err(Problem::Unauthorized(status)) => (
            Tone::Failed,
            "korrid refused this window".to_owned(),
            format!(
                "HTTP {status} on every request: the bearer capability was not accepted. \
                 Nothing below is current."
            ),
        ),
        Err(Problem::Unreachable(reason)) => (
            Tone::Failed,
            "korrid is not reachable".to_owned(),
            format!("No answer on the loopback port ({reason}). Nothing below is current."),
        ),
        Err(problem) => (
            Tone::Failed,
            "korrid health failed".to_owned(),
            format!("{}. Nothing below is current.", describe(problem)),
        ),
    };
    if session.refreshing {
        tone = Tone::Checking;
        headline = "Checking korrid again".into();
        detail = format!("Showing check {} until korrid answers.", session.checks);
    }

    let unavailable = |problem: &Problem| format!("Not available: {}", describe(problem));

    let identity = match &last.identity {
        Ok(status) => {
            let mut facts = match &status.identity {
                IdentityState::Unowned { device_public_key } => vec![
                    fact("State", "Unowned: no owner has claimed this device"),
                    key("Device key", device_public_key),
                ],
                IdentityState::Owned {
                    device_public_key,
                    owner_public_key,
                    ..
                } => vec![
                    fact("State", "Owned"),
                    key("Device key", device_public_key),
                    key("Owner key", owner_public_key),
                ],
                IdentityState::Revoked {
                    device_public_key,
                    owner_public_key,
                    ..
                } => vec![
                    fact("State", "Revoked by its owner"),
                    key("Device key", device_public_key),
                    key("Former owner key", owner_public_key),
                ],
                IdentityState::Invalid { reason } => {
                    vec![fact("State", format!("Invalid: {reason}"))]
                }
            };
            facts.push(fact(
                "Identity switching",
                if status.can_switch_identity {
                    "Available"
                } else {
                    "Not available in this host"
                },
            ));
            facts
        }
        Err(problem) => vec![fact("State", unavailable(problem))],
    };

    let (catalog_summary, games) = match &last.catalog {
        Ok(snapshot) => {
            let mut summary = match snapshot.games.len() {
                0 => "No games in this device's catalog.".to_owned(),
                1 => "1 game.".to_owned(),
                count => format!("{count} games."),
            };
            if let Some(failures) = snapshot.failures.as_ref().filter(|f| !f.is_empty()) {
                summary.push_str(&format!(
                    " {} host(s) did not answer: {}",
                    failures.len(),
                    failures
                        .iter()
                        .map(|failure| format!(
                            "{} ({}): {}",
                            failure.host, failure.code, failure.message
                        ))
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
            let games = snapshot
                .games
                .iter()
                .map(|game| format!("{} ({})", game.title, game.source.label))
                .collect();
            (summary, games)
        }
        Err(problem) => (unavailable(problem), Vec::new()),
    };

    let (peers_summary, peers) = match &last.peers {
        Ok(list) => (
            match list.peers.len() {
                0 => "No verified peers. This scratch device has no relays configured.".to_owned(),
                1 => "1 verified peer.".to_owned(),
                count => format!("{count} verified peers."),
            },
            list.peers
                .iter()
                .map(|peer| format!("{}: {:?}", peer.label, peer.state).to_lowercase())
                .collect(),
        ),
        Err(problem) => (unavailable(problem), Vec::new()),
    };

    let connection = vec![
        fact("Transport", "Loopback HTTP on 127.0.0.1 only"),
        fact("Credential", "Per-run bearer capability, held in memory"),
        fact(
            "Last check",
            format!(
                "Check {}: {answered} of {} answered in {}",
                session.checks,
                requests.len(),
                milliseconds(last.elapsed)
            ),
        ),
    ];

    let version = match &last.health {
        Ok(health) => format!("{} (reported by system.health)", health.version),
        Err(problem) => unavailable(problem),
    };

    Report {
        tone,
        headline,
        detail,
        connection,
        identity,
        catalog_summary,
        games,
        peers_summary,
        peers,
        build: build(&version),
        refreshing: session.refreshing,
        checks: session.checks,
    }
}

/// Layout as a pure function of the window's logical size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub columns: i32,
    pub header_inline: bool,
    pub compact: bool,
}

/// Guesses, checked by eye in the screenshot ladder, not measured on a TV.
pub const COLUMN_MIN_WIDTH: f32 = 340.0;
pub const HEADER_INLINE_MIN_WIDTH: f32 = 560.0;
pub const COMPACT_BELOW_HEIGHT: f32 = 480.0;
pub const PADDING: f32 = 24.0;
pub const COMPACT_PADDING: f32 = 12.0;
pub const GAP: f32 = 24.0;

pub fn plan(width: f32, height: f32) -> Plan {
    let compact = height < COMPACT_BELOW_HEIGHT;
    let padding = if compact { COMPACT_PADDING } else { PADDING };
    let usable = (width - 2.0 * padding + GAP).max(0.0);
    let columns = ((usable / (COLUMN_MIN_WIDTH + GAP)).floor() as i32).clamp(1, 3);
    Plan {
        columns,
        header_inline: width >= HEADER_INLINE_MIN_WIDTH,
        compact,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use korrid::{CatalogSnapshot, Health, IdentityStatus, PeerList};

    fn session(last: Option<Inspection>) -> Session {
        Session {
            build_id: "test",
            refreshing: false,
            checks: 1,
            last,
        }
    }

    fn healthy() -> Inspection {
        Inspection {
            health: Ok(Health {
                version: korrid::VERSION.into(),
            }),
            identity: Ok(IdentityStatus {
                identity: IdentityState::Unowned {
                    device_public_key: "ab".repeat(32),
                },
                can_switch_identity: false,
                local_backup_available: false,
                retired_public_keys: Vec::new(),
            }),
            catalog: Ok(CatalogSnapshot {
                games: Vec::new(),
                failures: None,
            }),
            peers: Ok(PeerList { peers: Vec::new() }),
            elapsed: Duration::from_millis(7),
        }
    }

    fn all_failed(problem: Problem) -> Inspection {
        Inspection {
            health: Err(problem.clone()),
            identity: Err(problem.clone()),
            catalog: Err(problem.clone()),
            peers: Err(problem),
            ..healthy()
        }
    }

    #[test]
    fn before_the_first_answer_nothing_claims_health() {
        let report = present(&session(None));
        assert_eq!(report.tone, Tone::Checking);
        assert!(report.build[1].value.contains("not checked"));
    }

    #[test]
    fn healthy_empty_device_says_so_honestly() {
        let report = present(&session(Some(healthy())));
        assert_eq!(report.tone, Tone::Healthy);
        assert_eq!(report.catalog_summary, "No games in this device's catalog.");
        assert!(report.games.is_empty());
        assert!(report.peers_summary.starts_with("No verified peers"));
        assert!(report.identity[0].value.starts_with("Unowned"));
        assert!(
            report.identity[1].unbroken,
            "device key wraps between characters"
        );
        assert!(!report.identity[0].unbroken);
        assert_eq!(
            report.build[1].value,
            "korrid-v0 (reported by system.health)"
        );
    }

    #[test]
    fn rejected_credential_is_never_healthy() {
        let report = present(&session(Some(all_failed(Problem::Unauthorized(401)))));
        assert_eq!(report.tone, Tone::Failed);
        assert_eq!(report.headline, "korrid refused this window");
        assert!(report.catalog_summary.starts_with("Not available"));
        assert!(report.build[1].value.starts_with("Not available"));
    }

    #[test]
    fn closed_port_is_never_healthy() {
        let report = present(&session(Some(all_failed(Problem::Unreachable(
            "connection refused".into(),
        )))));
        assert_eq!(report.tone, Tone::Failed);
        assert_eq!(report.headline, "korrid is not reachable");
        assert!(report.detail.contains("loopback port"));
        assert!(!report.detail.contains("4100"));
    }

    #[test]
    fn one_typed_failure_makes_the_check_partial() {
        let mut inspection = healthy();
        inspection.catalog = Err(Problem::Rejected {
            code: "UpstreamUnavailable".into(),
            message: "no host".into(),
        });
        let report = present(&session(Some(inspection)));
        assert_eq!(report.tone, Tone::Partial);
        assert!(report.detail.contains("3 of 4"));
        assert!(report
            .detail
            .contains("app.catalog.snapshot: UpstreamUnavailable: no host"));
    }

    #[test]
    fn refreshing_keeps_old_facts_but_does_not_claim_health() {
        let mut state = session(Some(healthy()));
        state.refreshing = true;
        let report = present(&state);
        assert_eq!(report.tone, Tone::Checking);
        assert!(report.detail.contains("check 1"));
    }

    #[test]
    fn plan_columns_change_one_pixel_either_side() {
        // Three columns need 3 * 340 + 2 * 24 + 2 * 24 padding = 1116.
        assert_eq!(plan(1115.0, 900.0).columns, 2);
        assert_eq!(plan(1116.0, 900.0).columns, 3);
        // Two columns need 2 * 340 + 24 + 48 = 752.
        assert_eq!(plan(751.0, 900.0).columns, 1);
        assert_eq!(plan(752.0, 900.0).columns, 2);
        assert_eq!(plan(200.0, 900.0).columns, 1);
        assert_eq!(plan(4000.0, 900.0).columns, 3);
    }

    #[test]
    fn plan_axes_are_independent() {
        assert!(!plan(559.0, 900.0).header_inline);
        assert!(plan(560.0, 900.0).header_inline);
        assert!(plan(1280.0, 479.0).compact);
        assert!(!plan(1280.0, 480.0).compact);
        // Wide-short keeps its width-driven columns; narrow-tall stays single.
        assert_eq!(plan(1280.0, 300.0).columns, 3);
        assert_eq!(plan(390.0, 844.0).columns, 1);
    }
}
