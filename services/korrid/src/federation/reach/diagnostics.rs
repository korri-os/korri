//! Privacy-safe labels and a monotonic clock for reach stage diagnostics.
//! `reset=0` starts at directory creation. Only detected local network changes
//! advance the shared reset epoch; endpoint/settings refreshes do not.
//! Stage lines use the existing stderr-to-journal convention. No global logger
//! or third-party tracing is enabled, and no raw protocol errors are printed.
use crate::relay::{PublishState, RelayError};
use iroh::{endpoint::Connection, TransportAddr};
use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    sync::{Arc, RwLock},
    time::Instant,
};

#[derive(Clone)]
pub(super) struct Diagnostics(Arc<RwLock<Epoch>>);
struct Epoch {
    origin: Instant,
    reset: u64,
}
#[derive(Clone, Copy)]
pub(super) struct Stamp {
    reset: u64,
    elapsed_ms: u128,
}
impl Epoch {
    fn new(origin: Instant) -> Self {
        Self { origin, reset: 0 }
    }
    fn reset_at(&mut self, now: Instant) {
        self.origin = now;
        self.reset = self.reset.saturating_add(1);
    }
    fn stamp_at(&self, now: Instant) -> Stamp {
        Stamp {
            reset: self.reset,
            elapsed_ms: now.saturating_duration_since(self.origin).as_millis(),
        }
    }
}
impl Default for Diagnostics {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(Epoch::new(Instant::now()))))
    }
}
#[derive(Default)]
pub(super) struct PublicationProgress {
    pub(super) relay_count: usize,
    pub(super) completed_relays: usize,
    pub(super) first_ack_ms: Option<u128>,
    pub(super) operation_ms: u128,
}
impl Diagnostics {
    pub(super) fn stamp(&self) -> Stamp {
        let epoch = self.0.read().expect("reach diagnostic clock");
        epoch.stamp_at(Instant::now())
    }
    pub(super) fn publication(
        &self,
        stage: &'static str,
        peer: &str,
        result: &Result<PublishState, RelayError>,
        progress: &PublicationProgress,
    ) {
        let first_ack_ms = progress
            .first_ack_ms
            .map(|ms| ms.to_string())
            .unwrap_or_else(|| "none".into());
        let final_outcome = progress.completed_relays == progress.relay_count;
        match result {
            Ok(state) => {
                let outcomes = publication_outcomes(state);
                eprintln!(
                    "korrid: reach: {} stage={stage} peer={} relay_count={} completed_relays={} outcomes={} operation_ms={} first_ack_ms={first_ack_ms} final={final_outcome}",
                    self.stamp(), short_peer(peer), progress.relay_count, progress.completed_relays, outcomes.join(","), progress.operation_ms
                );
            }
            Err(_) => eprintln!(
                "korrid: reach: {} stage={stage} peer={} relay_count=0 error=queue_failed operation_ms={} first_ack_ms=none final=true",
                self.stamp(), short_peer(peer), progress.operation_ms
            ),
        }
    }
    pub(super) fn route_wait(&self, peer: &str) -> RouteWait {
        let peer = short_peer(peer).to_owned();
        eprintln!(
            "korrid: reach: {} stage=invite_route_wait_start peer={peer}",
            self.stamp()
        );
        RouteWait {
            diagnostics: self.clone(),
            peer,
            started: Instant::now(),
            outcome: "cancelled",
        }
    }
    pub(super) fn network_reset(&self) {
        let stamp = {
            let mut epoch = self.0.write().expect("reach diagnostic clock");
            let now = Instant::now();
            epoch.reset_at(now);
            epoch.stamp_at(now)
        };
        eprintln!("korrid: reach: {stamp} stage=network_reset");
    }
}
pub(super) struct RouteWait {
    diagnostics: Diagnostics,
    peer: String,
    started: Instant,
    outcome: &'static str,
}
impl RouteWait {
    pub(super) fn finish(&mut self, success: bool) {
        self.outcome = if success { "ok" } else { "failed" };
    }
}
impl Drop for RouteWait {
    fn drop(&mut self) {
        eprintln!(
            "korrid: reach: {} stage=invite_route_wait_end peer={} outcome={} wait_ms={}",
            self.diagnostics.stamp(),
            self.peer,
            self.outcome,
            self.started.elapsed().as_millis()
        );
    }
}
impl fmt::Display for Stamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "reset={} elapsed_ms={}", self.reset, self.elapsed_ms)
    }
}
pub(super) fn server_label(server: &str) -> String {
    if super::settings::StunList::configured(vec![server.to_owned()]).is_err() {
        return "invalid".into();
    }
    server
        .parse()
        .map(mapped_address)
        .unwrap_or_else(|_| server.to_owned())
}

fn relay_label(relay: &str) -> String {
    let Ok(url) = url::Url::parse(relay) else {
        return "invalid".into();
    };
    if !matches!(url.scheme(), "ws" | "wss") {
        return "invalid".into();
    }
    let Some(host) = url.host() else {
        return "invalid".into();
    };
    let host = match host {
        url::Host::Domain(host) => host.to_owned(),
        url::Host::Ipv4(ip) => redacted_ip(ip.into()),
        url::Host::Ipv6(ip) => redacted_ip(ip.into()),
    };
    let mut label = format!("{}://{host}", url.scheme());
    if let Some(port) = url.port() {
        label.push_str(&format!(":{port}"));
    }
    label
}

fn publication_outcomes(state: &PublishState) -> Vec<String> {
    let (accepted, failed) = match state {
        PublishState::Published { accepted_relays } => (accepted_relays.as_slice(), &[][..]),
        PublishState::Partial {
            accepted_relays,
            failed_relays,
        } => (accepted_relays.as_slice(), failed_relays.as_slice()),
        PublishState::Failed { failed_relays } => (&[][..], failed_relays.as_slice()),
    };
    accepted
        .iter()
        .map(|relay| format!("{}=ok", relay_label(relay)))
        .chain(
            failed
                .iter()
                .map(|failure| format!("{}=failed", relay_label(&failure.relay))),
        )
        .collect()
}

fn path_kind(address: Option<&TransportAddr>) -> &'static str {
    match address {
        Some(TransportAddr::Ip(address)) if address.is_ipv4() => "direct_v4",
        Some(TransportAddr::Ip(_)) => "direct_v6",
        Some(TransportAddr::Relay(_)) => "relayed",
        _ => "unknown",
    }
}
pub(super) fn connection_path(connection: &Connection) -> &'static str {
    let paths = connection.paths();
    let selected = paths.iter().find(|path| path.is_selected());
    path_kind(selected.as_ref().map(|path| path.remote_addr()))
}

fn redacted_ip(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => format!("{}.x.x.x", ip.octets()[0]),
        IpAddr::V6(ip) => format!("[{:x}:x:x:x:x:x:x:x]", ip.segments()[0]),
    }
}
pub(super) fn mapped_address(address: SocketAddr) -> String {
    let host = redacted_ip(address.ip());
    format!("{host}:{}", address.port())
}

pub(super) fn short_peer(peer: &str) -> &str {
    if peer.len() == 64 && peer.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        &peer[..8]
    } else {
        "invalid"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reach_server_labels_never_include_credentials_paths_or_queries() {
        assert_eq!(
            server_label("stun.cloudflare.com:3478"),
            "stun.cloudflare.com:3478"
        );
        assert_eq!(server_label("user:private@stun.invalid:3478"), "invalid");
        assert_eq!(server_label("203.0.113.1:3478"), "203.x.x.x:3478");
        assert_eq!(
            relay_label("wss://user:private@relay.invalid:8443/private?token=private"),
            "wss://relay.invalid:8443"
        );
        assert_eq!(relay_label("not a relay"), "invalid");
    }

    #[test]
    fn reach_publication_reports_each_relay_without_its_failure_message() {
        let state = crate::relay::PublishState::Partial {
            accepted_relays: vec!["wss://accepted.invalid/private?token=private".into()],
            failed_relays: vec![crate::relay::RelayFailure {
                relay: "wss://user:private@failed.invalid".into(),
                message: "private server response".into(),
            }],
        };
        assert_eq!(
            publication_outcomes(&state),
            ["wss://accepted.invalid=ok", "wss://failed.invalid=failed"]
        );
        assert_eq!(
            publication_outcomes(&PublishState::Published {
                accepted_relays: vec!["wss://only.invalid".into()]
            }),
            ["wss://only.invalid=ok"]
        );
        assert_eq!(
            publication_outcomes(&PublishState::Failed {
                failed_relays: vec![crate::relay::RelayFailure {
                    relay: "wss://only.invalid".into(),
                    message: "private".into()
                }]
            }),
            ["wss://only.invalid=failed"]
        );
    }

    #[test]
    fn reach_path_labels_distinguish_direct_families_from_a_relay() {
        assert_eq!(
            path_kind(Some(&iroh::TransportAddr::Ip(
                "127.0.0.1:1234".parse().unwrap()
            ))),
            "direct_v4"
        );
        assert_eq!(
            path_kind(Some(&iroh::TransportAddr::Ip(
                "[::1]:1234".parse().unwrap()
            ))),
            "direct_v6"
        );
        assert_eq!(
            path_kind(Some(&iroh::TransportAddr::Relay(
                "http://relay.invalid".parse().unwrap()
            ))),
            "relayed"
        );
        assert_eq!(path_kind(None), "unknown");
    }

    #[tokio::test]
    async fn reach_path_reports_the_selected_real_iroh_connection() {
        let (a, b, outgoing, _incoming) = super::super::tests::pair().await;
        assert_eq!(connection_path(&outgoing), "direct_v4");
        a.close().await;
        b.close().await;
    }

    #[test]
    fn reach_cloned_diagnostics_share_the_last_network_reset() {
        let diagnostics = Diagnostics::default();
        let clone = diagnostics.clone();
        diagnostics.network_reset();
        assert_eq!(clone.stamp().reset, 1);
        assert_eq!(diagnostics.stamp().reset, 1);
    }

    #[test]
    fn reach_mapped_addresses_hide_the_host_but_keep_the_family_and_port() {
        assert_eq!(
            mapped_address("203.0.113.81:45001".parse().unwrap()),
            "203.x.x.x:45001"
        );
        assert_eq!(
            mapped_address("[2001:db8:1234:5678::418]:45002".parse().unwrap()),
            "[2001:x:x:x:x:x:x:x]:45002"
        );
    }

    #[test]
    fn reach_elapsed_time_is_monotonic_and_only_a_network_reset_changes_its_origin() {
        let start = std::time::Instant::now();
        let mut epoch = Epoch::new(start);
        assert_eq!(epoch.stamp_at(start).elapsed_ms, 0);
        assert_eq!(
            epoch
                .stamp_at(start + std::time::Duration::from_millis(42))
                .elapsed_ms,
            42
        );
        epoch.reset_at(start + std::time::Duration::from_millis(50));
        let stamp = epoch.stamp_at(start + std::time::Duration::from_millis(57));
        assert_eq!(stamp.reset, 1);
        assert_eq!(stamp.elapsed_ms, 7);
    }

    #[test]
    fn reach_peer_label_never_contains_a_full_key_or_unvalidated_input() {
        assert_eq!(short_peer(&"0123456789abcdef".repeat(4)), "01234567");
        assert_eq!(short_peer(""), "invalid");
        assert_eq!(short_peer("private input"), "invalid");
        assert_eq!(short_peer(&"🦀".repeat(16)), "invalid");
    }
}
