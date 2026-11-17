//! UDP hole punching without IO. Both sides send punches to every address they know for the
//! other at the same time; each punch opens the sender's NAT towards that address, so the
//! other side's punches get through. The first authenticated punch or answer fixes the path at
//! the address it came from: that also finds a peer whose NAT gave it another port than its
//! record says (a symmetric NAT), as long as this side's NAT lets that port in.
//!
//! For a peer behind a symmetric NAT, [`candidates`] adds the next few ports after the one it
//! reported: many such NATs hand out ports in sequence.

use crate::{
    node::{LOOKUP_DEADLINE, PUT_WAIT},
    record::Nat,
    Millis,
};
use std::net::SocketAddr;

/// How long punching goes on before it gives up, unless held longer ([`Puncher::hold_until`]).
pub const TIMEOUT: Millis = 30_000;

// The time budget of a punched path, in one place: the device's wait, the host's polling and the
// engine's wait all derive from these.

/// The longest a record takes to be stored in the DHT: a lookup, then the nodes' confirmations.
pub const PUBLISH: Millis = LOOKUP_DEADLINE + PUT_WAIT;
/// How often the host looks for the record of a device it heard from lately ([`crate::host`]),
pub const HOST_POLL: Millis = 10_000;
/// and of every other paired device, so the DHT is asked less often for devices that are not
/// trying to connect. A device from before the punch budget stops punching 30 s after it
/// starts; this, a lookup and the punches still fit in that.
pub const HOST_POLL_IDLE: Millis = 15_000;
/// How late the host's loop may start a due poll: it goes round once a second.
pub const HOST_LOOP: Millis = 1_000;
/// How long a device keeps punching after its record is stored: the host's next look for it at
/// the slower rate (counted from the start of the look before), that lookup, and a margin for
/// the punches to meet. A host behind a NAT that filters by port learns the device's endpoints
/// only this way.
pub const AFTER_PUBLISH: Millis = HOST_POLL_IDLE + HOST_LOOP + LOOKUP_DEADLINE + 15_000;

/// Until when a device keeps punching that starts publishing its record at `now`.
pub fn device_deadline(now: Millis) -> Millis {
    now + PUBLISH + AFTER_PUBLISH
}
/// Punches go out this often at first,
const FAST: Millis = 100;
/// for this long,
const FAST_FOR: Millis = 3_000;
/// then this often.
const SLOW: Millis = 500;
/// How many ports past a symmetric NAT's reported one are tried.
pub const PREDICTED_PORTS: u16 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Punching,
    Connected(SocketAddr),
    /// No punch got through before the timeout.
    Failed,
}

pub struct Puncher {
    candidates: Vec<SocketAddr>,
    started: Millis,
    /// When punching gives up.
    deadline: Millis,
    next: Millis,
    state: State,
}

/// The addresses to punch for a peer's record: its endpoints, and for a symmetric NAT the
/// ports after each IPv4 one.
pub fn candidates(endpoints: &[SocketAddr], nat: Nat) -> Vec<SocketAddr> {
    let mut out: Vec<SocketAddr> = Vec::new();
    let mut push = |address: SocketAddr| {
        if address.port() != 0 && !out.contains(&address) {
            out.push(address);
        }
    };
    for &endpoint in endpoints {
        push(endpoint);
    }
    if nat == Nat::Symmetric {
        for &endpoint in endpoints.iter().filter(|endpoint| endpoint.is_ipv4()) {
            for step in 1..=PREDICTED_PORTS {
                if let Some(port) = endpoint.port().checked_add(step) {
                    push(SocketAddr::new(endpoint.ip(), port));
                }
            }
        }
    }
    out
}

impl Puncher {
    pub fn new(candidates: Vec<SocketAddr>, now: Millis) -> Self {
        Self {
            candidates,
            started: now,
            deadline: now + TIMEOUT,
            next: now,
            state: State::Punching,
        }
    }

    /// Punches on at least until `deadline`: the peer finds this side's record later.
    pub fn hold_until(&mut self, deadline: Millis) {
        self.deadline = self.deadline.max(deadline);
    }

    /// More addresses learned while punching (the peer's record arrived later).
    pub fn add(&mut self, more: &[SocketAddr]) {
        for &address in more {
            if !self.candidates.contains(&address) {
                self.candidates.push(address);
            }
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// The addresses to punch now; none once connected or failed.
    pub fn poll(&mut self, now: Millis) -> Vec<SocketAddr> {
        if self.state != State::Punching {
            return Vec::new();
        }
        if now >= self.deadline {
            self.state = State::Failed;
            return Vec::new();
        }
        if now < self.next {
            return Vec::new();
        }
        let interval = if now - self.started < FAST_FOR {
            FAST
        } else {
            SLOW
        };
        self.next = now + interval;
        self.candidates.clone()
    }

    pub fn next_wakeup(&self) -> Option<Millis> {
        (self.state == State::Punching).then_some(self.next.min(self.deadline))
    }

    /// An authenticated punch arrived from `from`: answer it there, and the path is that address.
    pub fn on_punch(&mut self, from: SocketAddr) {
        self.connect(from);
    }

    /// An authenticated answer to a punch arrived from `from`.
    pub fn on_ack(&mut self, from: SocketAddr) {
        self.connect(from);
    }

    fn connect(&mut self, from: SocketAddr) {
        // A later punch from another address (the peer roamed, or a second candidate opened)
        // moves the path there only while not yet connected.
        if !matches!(self.state, State::Connected(_)) {
            self.state = State::Connected(from);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punches_start_fast_slow_down_and_give_up() {
        let peer: SocketAddr = "203.0.113.5:40000".parse().unwrap();
        let mut puncher = Puncher::new(vec![peer], 1_000);
        assert_eq!(puncher.poll(1_000), vec![peer]);
        assert!(puncher.poll(1_050).is_empty());
        assert_eq!(puncher.poll(1_100), vec![peer]);
        let mut sent = 2;
        for now in (1_101..1_000 + TIMEOUT).step_by(10) {
            sent += puncher.poll(now).len();
        }
        // 3 s at 10 per second, then 27 s at 2 per second.
        assert!((80..=90).contains(&sent), "{sent}");
        assert_eq!(puncher.poll(1_000 + TIMEOUT), Vec::<SocketAddr>::new());
        assert_eq!(puncher.state(), State::Failed);
        assert_eq!(puncher.next_wakeup(), None);
    }

    #[test]
    fn a_held_puncher_punches_on_past_the_timeout() {
        let peer: SocketAddr = "203.0.113.5:40000".parse().unwrap();
        let mut puncher = Puncher::new(vec![peer], 0);
        puncher.hold_until(device_deadline(0));
        // A shorter hold never cuts it.
        puncher.hold_until(1_000);
        assert_eq!(puncher.poll(TIMEOUT + 100), vec![peer]);
        assert_eq!(puncher.state(), State::Punching);
        assert_eq!(puncher.poll(device_deadline(0) - 1), vec![peer]);
        assert!(puncher.poll(device_deadline(0)).is_empty());
        assert_eq!(puncher.state(), State::Failed);
        // The budget covers the host's poll and lookup after the longest publication.
        assert!(device_deadline(0) >= PUBLISH + HOST_POLL_IDLE + LOOKUP_DEADLINE);
    }

    #[test]
    fn the_first_authenticated_punch_fixes_the_path() {
        let reported: SocketAddr = "203.0.113.5:40000".parse().unwrap();
        let actual: SocketAddr = "203.0.113.5:40007".parse().unwrap();
        let mut puncher = Puncher::new(vec![reported], 0);
        puncher.on_punch(actual);
        assert_eq!(puncher.state(), State::Connected(actual));
        puncher.on_ack(reported);
        assert_eq!(puncher.state(), State::Connected(actual));
        assert!(puncher.poll(100).is_empty());
    }

    #[test]
    fn a_symmetric_peer_gets_its_next_ports_tried() {
        let v4: SocketAddr = "203.0.113.5:65530".parse().unwrap();
        let v6: SocketAddr = "[2001:db8::5]:40000".parse().unwrap();
        assert_eq!(candidates(&[v4, v6, v4], Nat::Cone), vec![v4, v6]);
        let symmetric = candidates(&[v4, v6], Nat::Symmetric);
        // 65531..=65535: the ports that exist.
        assert_eq!(symmetric.len(), 2 + 5);
        assert!(symmetric.contains(&"203.0.113.5:65535".parse().unwrap()));
    }
}
