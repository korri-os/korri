//! Sunshine packets remain unreliable. This module bounds the spike's
//! version/port/packet/fragment framing before it allocates buffers.
//! The reassembler drops a packet if a fragment is missing.
//! `Counters` count each loss on this side; `Report` writes them with the
//! connection's own QUIC counters when the stream ends (#601).
use iroh::endpoint::Connection;
use std::{
    collections::HashMap,
    io,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
const HEADER: usize = 15;
const MAX_PACKET: usize = 65_507;
const MAX_PARTIAL: usize = 64;
const MAX_FRAGMENT: usize = 1100;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn fragments(port: u16, id: u64, payload: &[u8], mtu: usize) -> io::Result<Vec<Vec<u8>>> {
    if !super::tunnel::UDP_PORTS.contains(&port) || payload.len() > MAX_PACKET || mtu <= HEADER {
        return Err(io::Error::other("invalid stream datagram"));
    }
    let size = mtu - HEADER;
    let count = payload.len().div_ceil(size).max(1);
    if count > 128 {
        return Err(io::Error::other("datagram path MTU is too small"));
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let mut part = vec![1];
        part.extend_from_slice(&port.to_be_bytes());
        part.extend_from_slice(&id.to_be_bytes());
        part.extend_from_slice(&(i as u16).to_be_bytes());
        part.extend_from_slice(&(count as u16).to_be_bytes());
        part.extend_from_slice(
            &payload[(i * size).min(payload.len())..((i + 1) * size).min(payload.len())],
        );
        out.push(part);
    }
    Ok(out)
}
/// One stream's datagram counters on this side, shared by its tasks.
#[derive(Default)]
pub(super) struct Counters {
    /// Packets from a local socket that `send` was given.
    sent_packets: AtomicU64,
    /// Fragments handed to QUIC. QUIC can still drop one from its send buffer.
    sent_fragments: AtomicU64,
    /// Packets that `send` could not hand over whole.
    send_errors: AtomicU64,
    /// Fragments read from QUIC.
    received_fragments: AtomicU64,
    /// Whole packets put back together.
    delivered_packets: AtomicU64,
    /// Fragments refused: bad framing, a duplicate, or a count that disagrees.
    rejected_fragments: AtomicU64,
    /// Incomplete packets removed to admit a newer packet.
    evicted_packets: AtomicU64,
    /// Incomplete packets removed after `PARTIAL_LIFETIME`.
    expired_packets: AtomicU64,
}

impl Counters {
    fn add(counter: &AtomicU64, count: u64) {
        counter.fetch_add(count, Ordering::Relaxed);
    }
    fn get(counter: &AtomicU64) -> u64 {
        counter.load(Ordering::Relaxed)
    }
}

pub(super) fn send(
    connection: &Connection,
    counters: &Counters,
    port: u16,
    payload: &[u8],
    response: bool,
) -> io::Result<()> {
    Counters::add(&counters.sent_packets, 1);
    let result = send_fragments(connection, counters, port, payload, response);
    if result.is_err() {
        Counters::add(&counters.send_errors, 1);
    }
    result
}
fn send_fragments(
    connection: &Connection,
    counters: &Counters,
    port: u16,
    payload: &[u8],
    response: bool,
) -> io::Result<()> {
    let mtu = connection
        .max_datagram_size()
        .ok_or_else(|| io::Error::other("no datagram path"))?
        .min(MAX_FRAGMENT);
    for mut part in fragments(port, NEXT.fetch_add(1, Ordering::Relaxed), payload, mtu)? {
        if response {
            part[0] = 2;
        }
        connection
            .send_datagram(part.into())
            .map_err(io::Error::other)?;
        Counters::add(&counters.sent_fragments, 1);
    }
    Ok(())
}

/// Writes one line with this side's counters and the connection's QUIC
/// counters when it is dropped: when the stream's tasks end or are aborted.
/// A connection that carried no stream datagram writes nothing.
pub(super) struct Report {
    pub side: &'static str,
    pub connection: Connection,
    pub counters: Arc<Counters>,
}

impl Drop for Report {
    fn drop(&mut self) {
        let c = &*self.counters;
        let get = Counters::get;
        if get(&c.sent_fragments) == 0 && get(&c.received_fragments) == 0 {
            return;
        }
        let quic = self.connection.stats();
        let rtt = self
            .connection
            .paths()
            .iter()
            .find(|path| path.is_selected())
            .map(|path| {
                let stats = path.stats();
                format!(
                    "{:?} rtt={:?} cwnd={} congestion_events={} lost_packets={}",
                    path.remote_addr(),
                    path.rtt(),
                    stats.cwnd,
                    stats.congestion_events,
                    stats.lost_packets,
                )
            })
            .unwrap_or_else(|| "none".into());
        eprintln!(
            "korrid: reach: stream datagrams side={} sent_packets={} sent_fragments={} send_errors={} quic_datagram_frames_sent={} received_fragments={} quic_datagram_frames_received={} delivered_packets={} rejected_fragments={} evicted_packets={} expired_packets={} quic_lost_packets={} quic_sent_packets={} path=[{rtt}]",
            self.side,
            get(&c.sent_packets),
            get(&c.sent_fragments),
            get(&c.send_errors),
            quic.frame_tx.datagram,
            get(&c.received_fragments),
            quic.frame_rx.datagram,
            get(&c.delivered_packets),
            get(&c.rejected_fragments),
            get(&c.evicted_packets),
            get(&c.expired_packets),
            quic.lost_packets,
            quic.sent_packets,
        );
    }
}
struct Partial {
    started: Instant,
    parts: Vec<Option<Vec<u8>>>,
    bytes: usize,
}
const PARTIAL_LIFETIME: Duration = Duration::from_secs(2);
pub(super) struct Receiver {
    partial: HashMap<(u16, u64), Partial>,
    counters: Arc<Counters>,
}
impl Receiver {
    pub(super) fn new(counters: Arc<Counters>) -> Self {
        Self {
            partial: HashMap::new(),
            counters,
        }
    }
    pub(super) fn push(&mut self, bytes: &[u8]) -> Option<(u16, Vec<u8>)> {
        Counters::add(&self.counters.received_fragments, 1);
        let packet = self.reassemble(bytes);
        if packet.is_some() {
            Counters::add(&self.counters.delivered_packets, 1);
        }
        packet
    }
    fn reassemble(&mut self, bytes: &[u8]) -> Option<(u16, Vec<u8>)> {
        let Some(fragment) = Fragment::parse(bytes) else {
            Counters::add(&self.counters.rejected_fragments, 1);
            return None;
        };
        let before = self.partial.len();
        self.partial
            .retain(|_, p| p.started.elapsed() < PARTIAL_LIFETIME);
        Counters::add(
            &self.counters.expired_packets,
            (before - self.partial.len()) as u64,
        );
        let Fragment { key, index, count } = fragment;
        if !self.partial.contains_key(&key) && self.partial.len() >= MAX_PARTIAL {
            // A packet whose fragment was lost would otherwise hold its slot
            // for PARTIAL_LIFETIME and refuse every newer packet meanwhile.
            let oldest = self
                .partial
                .iter()
                .min_by_key(|(_, p)| p.started)
                .map(|(key, _)| *key)?;
            self.partial.remove(&oldest);
            Counters::add(&self.counters.evicted_packets, 1);
        }
        let p = self.partial.entry(key).or_insert_with(|| Partial {
            started: Instant::now(),
            parts: vec![None; count],
            bytes: 0,
        });
        if p.parts.len() != count || p.parts[index].is_some() {
            Counters::add(&self.counters.rejected_fragments, 1);
            return None;
        }
        p.bytes += bytes.len() - HEADER;
        if p.bytes > MAX_PACKET {
            self.partial.remove(&key);
            Counters::add(&self.counters.rejected_fragments, 1);
            return None;
        }
        p.parts[index] = Some(bytes[HEADER..].to_vec());
        if p.parts.iter().all(Option::is_some) {
            let p = self.partial.remove(&key)?;
            return Some((key.0, p.parts.into_iter().flatten().flatten().collect()));
        }
        None
    }
}
struct Fragment {
    key: (u16, u64),
    index: usize,
    count: usize,
}
impl Fragment {
    fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < HEADER || bytes.len() > MAX_FRAGMENT || !matches!(bytes[0], 1 | 2) {
            return None;
        }
        let port = u16::from_be_bytes(bytes[1..3].try_into().ok()?);
        if !super::tunnel::UDP_PORTS.contains(&port) {
            return None;
        }
        let id = u64::from_be_bytes(bytes[3..11].try_into().ok()?);
        let index = u16::from_be_bytes(bytes[11..13].try_into().ok()?) as usize;
        let count = u16::from_be_bytes(bytes[13..15].try_into().ok()?) as usize;
        if count == 0 || count > 128 || index >= count {
            return None;
        }
        Some(Self {
            key: (port, id),
            index,
            count,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reach_fragmentation_reorders_and_drops_missing_or_invalid_packets() {
        let input = vec![42; 65507];
        let mut parts = fragments(47998, 7, &input, 1100).unwrap();
        let counters = Arc::new(Counters::default());
        let mut receiver = Receiver::new(counters.clone());
        let last = parts.remove(0);
        for part in parts.into_iter().rev() {
            assert!(receiver.push(&part).is_none());
        }
        assert_eq!(receiver.push(&last), Some((47998, input)));
        assert!(receiver.push(&[1, 2, 3]).is_none());
        assert!(fragments(47990, 0, b"admin", 1100).is_err());
        assert!(fragments(47998, 0, &vec![0; 65508], 1100).is_err());
        assert_eq!(Counters::get(&counters.delivered_packets), 1);
        assert_eq!(Counters::get(&counters.rejected_fragments), 1);
    }

    /// Sunshine's video packets are larger than one fragment. Before #601 a
    /// lost half held a slot for two seconds, and 64 such losses refused
    /// every later packet until the oldest expired.
    #[test]
    fn reach_lost_fragments_do_not_block_later_packets() {
        let counters = Arc::new(Counters::default());
        let mut receiver = Receiver::new(counters.clone());
        let packet = vec![7; 1400];
        for id in 0..(MAX_PARTIAL as u64 + 10) {
            let parts = fragments(47998, id, &packet, MAX_FRAGMENT).unwrap();
            assert_eq!(parts.len(), 2);
            assert!(receiver.push(&parts[0]).is_none());
        }
        let parts = fragments(47998, 1_000, &packet, MAX_FRAGMENT).unwrap();
        assert!(receiver.push(&parts[0]).is_none());
        assert_eq!(receiver.push(&parts[1]), Some((47998, packet)));
        assert_eq!(Counters::get(&counters.evicted_packets), 11);
        assert_eq!(receiver.partial.len(), MAX_PARTIAL - 1);
    }
}
