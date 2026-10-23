//! Sunshine packets remain unreliable. This module bounds the spike's
//! version/port/packet/fragment framing before it allocates buffers.
//! The reassembler drops a packet if a fragment is missing.
use iroh::endpoint::Connection;
use std::{
    collections::HashMap,
    io,
    sync::atomic::{AtomicU64, Ordering},
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
pub(super) fn send(connection: &Connection, port: u16, payload: &[u8]) -> io::Result<()> {
    let mtu = connection
        .max_datagram_size()
        .ok_or_else(|| io::Error::other("no datagram path"))?
        .min(MAX_FRAGMENT);
    for part in fragments(port, NEXT.fetch_add(1, Ordering::Relaxed), payload, mtu)? {
        connection
            .send_datagram(part.into())
            .map_err(io::Error::other)?;
    }
    Ok(())
}
struct Partial {
    started: Instant,
    parts: Vec<Option<Vec<u8>>>,
    bytes: usize,
}
#[derive(Default)]
pub(super) struct Receiver {
    partial: HashMap<(u16, u64), Partial>,
}
impl Receiver {
    pub(super) fn push(&mut self, bytes: &[u8]) -> Option<(u16, Vec<u8>)> {
        if bytes.len() < HEADER || bytes.len() > MAX_FRAGMENT || bytes[0] != 1 {
            return None;
        }
        self.partial
            .retain(|_, p| p.started.elapsed() < Duration::from_secs(2));
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
        let key = (port, id);
        if !self.partial.contains_key(&key) && self.partial.len() >= MAX_PARTIAL {
            return None;
        }
        let p = self.partial.entry(key).or_insert_with(|| Partial {
            started: Instant::now(),
            parts: vec![None; count],
            bytes: 0,
        });
        if p.parts.len() != count || p.parts[index].is_some() {
            return None;
        }
        p.bytes += bytes.len() - HEADER;
        if p.bytes > MAX_PACKET {
            self.partial.remove(&key);
            return None;
        }
        p.parts[index] = Some(bytes[HEADER..].to_vec());
        if p.parts.iter().all(Option::is_some) {
            let p = self.partial.remove(&key)?;
            return Some((port, p.parts.into_iter().flatten().flatten().collect()));
        }
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reach_fragmentation_reorders_and_drops_missing_or_invalid_packets() {
        let input = vec![42; 65507];
        let mut parts = fragments(47998, 7, &input, 1100).unwrap();
        let mut receiver = Receiver::default();
        let last = parts.remove(0);
        for part in parts.into_iter().rev() {
            assert!(receiver.push(&part).is_none());
        }
        assert_eq!(receiver.push(&last), Some((47998, input)));
        assert!(receiver.push(&[1, 2, 3]).is_none());
        assert!(fragments(47990, 0, b"admin", 1100).is_err());
        assert!(fragments(47998, 0, &vec![0; 65508], 1100).is_err());
    }
}
