//! Recovery for a public NAT remap that produces no local interface event.
//! Connection handles remain the health source. This table records only loss
//! history and retry policy; it is never persisted or sent to another device.
use iroh::endpoint::Connection;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use tokio::time::Instant;

#[derive(Default)]
pub(super) struct RefreshPolicy {
    lost: BTreeMap<String, u32>,
    next_allowed: Option<Instant>,
    refreshes: u32,
}
impl RefreshPolicy {
    fn established(&mut self, peer: &str) {
        self.lost.remove(peer);
        self.refreshes = 0;
    }
    fn lost(&mut self, peer: String) {
        self.lost.entry(peer).or_insert(0);
    }
    fn failed(&mut self, peer: &str) {
        if let Some(failures) = self.lost.get_mut(peer) {
            *failures = failures.saturating_add(1);
        }
    }
    fn take(&mut self, now: Instant, healthy: bool) -> bool {
        if healthy
            || !self.lost.values().any(|failures| *failures >= 2)
            || self.next_allowed.is_some_and(|allowed| now < allowed)
        {
            return false;
        }
        let delay = Duration::from_secs(
            60u64.saturating_mul(1u64.checked_shl(self.refreshes).unwrap_or(u64::MAX)),
        )
        .min(Duration::from_secs(300));
        self.next_allowed = Some(now + delay);
        self.refreshes = self.refreshes.saturating_add(1);
        self.lost.values_mut().for_each(|failures| *failures = 0);
        true
    }
}

#[derive(Default)]
pub(super) struct Recovery {
    connections: BTreeMap<usize, (String, Connection)>,
    policy: RefreshPolicy,
    refreshing: bool,
}
impl Recovery {
    fn observe_closed(&mut self) {
        let closed = self
            .connections
            .iter()
            .filter(|(_, (_, c))| c.close_reason().is_some())
            .map(|(id, (peer, _))| (*id, peer.clone()))
            .collect::<Vec<_>>();
        for (id, peer) in closed {
            self.connections.remove(&id);
            self.policy.lost(peer);
        }
    }
    /// Call before publishing a route or serving an accepted connection. A
    /// refresh claim fences new admissions until the new endpoint is bound.
    pub fn register(&mut self, peer: String, connection: Connection) -> bool {
        self.observe_closed();
        if self.refreshing || connection.close_reason().is_some() {
            connection.close(0u32.into(), b"endpoint refresh in progress");
            return false;
        }
        self.policy.established(&peer);
        self.connections
            .insert(connection.stable_id(), (peer, connection));
        true
    }
    pub fn failed(&mut self, peer: &str) {
        self.observe_closed();
        self.policy.failed(peer);
    }
    pub fn take_refresh(&mut self, now: Instant) -> bool {
        self.observe_closed();
        if !self.policy.take(now, !self.connections.is_empty()) {
            return false;
        }
        self.refreshing = true;
        true
    }
    pub fn stop(&mut self, forget_losses: bool) {
        self.refreshing = true;
        self.connections.clear();
        if forget_losses {
            self.policy.lost.clear();
        }
    }
    pub fn started(&mut self) {
        self.refreshing = false;
    }
    pub fn retain_members(&mut self, members: &BTreeSet<String>) {
        self.policy.lost.retain(|peer, _| members.contains(peer));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reach_refresh_requires_loss_two_failures_and_no_healthy_connection() {
        let now = Instant::now();
        let mut policy = RefreshPolicy::default();
        policy.failed("A");
        policy.failed("A");
        assert!(!policy.take(now, false));
        policy.lost("A".into());
        policy.failed("A");
        assert!(!policy.take(now, false));
        policy.failed("A");
        assert!(!policy.take(now, true));
        assert!(policy.take(now, false));
        policy.failed("A");
        policy.failed("A");
        assert!(!policy.take(now + Duration::from_secs(59), false));
        assert!(policy.take(now + Duration::from_secs(60), false));
        policy.failed("A");
        policy.failed("A");
        assert!(!policy.take(now + Duration::from_secs(179), false));
        assert!(policy.take(now + Duration::from_secs(180), false));
        policy.established("A");
        policy.failed("A");
        policy.failed("A");
        assert!(!policy.take(now + Duration::from_secs(1000), false));
    }
    #[test]
    fn reach_refresh_backoff_caps_at_five_minutes_and_reset_preserves_rate_limit() {
        let now = Instant::now();
        let mut recovery = Recovery::default();
        recovery.policy.lost("A".into());
        recovery.failed("A");
        recovery.failed("A");
        assert!(recovery.take_refresh(now));
        recovery.stop(true);
        recovery.started();
        recovery.policy.lost("B".into());
        recovery.failed("B");
        recovery.failed("B");
        assert!(!recovery.take_refresh(now + Duration::from_secs(59)));
        let mut at = now + Duration::from_secs(60);
        let mut last_delay = Duration::ZERO;
        for _ in 0..8 {
            recovery.failed("B");
            recovery.failed("B");
            assert!(recovery.take_refresh(at));
            let allowed = recovery.policy.next_allowed.unwrap();
            assert!(allowed - at >= Duration::from_secs(60));
            assert!(allowed - at <= Duration::from_secs(300));
            last_delay = allowed - at;
            at = allowed;
        }
        assert_eq!(last_delay, Duration::from_secs(300));
    }
    #[tokio::test]
    async fn reach_refresh_tracks_loss_of_a_real_established_connection() {
        let (a, b, outgoing, incoming) = crate::federation::reach::tests::pair().await;
        let mut recovery = Recovery::default();
        assert!(recovery.register("A".into(), outgoing.clone()));
        recovery.failed("A");
        recovery.failed("A");
        assert!(!recovery.take_refresh(Instant::now()));
        incoming.close(0u32.into(), b"test loss");
        outgoing.closed().await;
        recovery.failed("A");
        assert!(!recovery.take_refresh(Instant::now()));
        recovery.failed("A");
        assert!(recovery.take_refresh(Instant::now()));
        a.close().await;
        b.close().await;
    }
    #[tokio::test]
    async fn reach_refresh_counts_accepted_connections_and_fences_new_admissions() {
        let (a, b, _outgoing, incoming) = crate::federation::reach::tests::pair().await;
        let mut recovery = Recovery::default();
        recovery.policy.lost("lost-peer".into());
        recovery.failed("lost-peer");
        recovery.failed("lost-peer");
        assert!(recovery.register("accepted-peer".into(), incoming.clone()));
        assert!(!recovery.take_refresh(Instant::now()));
        incoming.close(0u32.into(), b"test loss");
        incoming.closed().await;
        assert!(recovery.take_refresh(Instant::now()));
        let (c, d, new_connection, _other) = crate::federation::reach::tests::pair().await;
        assert!(new_connection.close_reason().is_none());
        assert!(!recovery.register("new-peer".into(), new_connection));
        a.close().await;
        b.close().await;
        c.close().await;
        d.close().await;
    }
}
