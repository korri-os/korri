//! Local discovery (ADR 0089): devices on one local network find each other
//! with no server. An announcement holds no device key, only one rotating tag
//! per known owner device; only that pair can derive it, and the two
//! directions differ (`DeviceIdentity::nearby_announcement_tag`). A matched
//! announcement's addresses join the dial pool for a short time, and a new
//! address ends that peer's reach retry backoff. They are process memory,
//! never persisted, and never proof of liveness: the connection still
//! authenticates by device key.
use crate::identity::DeviceIdentity;
use std::{
    collections::{BTreeMap, BTreeSet},
    net::{IpAddr, SocketAddr},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
mod mdns;
#[cfg(target_os = "linux")]
pub use mdns::run;

/// Tags rotate every five minutes.
pub(crate) const WINDOW_SECONDS: u64 = 300;
/// A roaming device's local address is short-lived.
pub(crate) const FOUND_LIFETIME: Duration = Duration::from_secs(600);
/// The same bound as a published endpoint record's candidates.
const MAX_FOUND_PER_DEVICE: usize = 8;
/// One 8-byte tag per known device keeps an announcement inside one
/// ordinary packet. Devices past this bound still reach this one through
/// its endpoint record and the relays.
pub(crate) const MAX_TAGS: usize = 64;

pub(crate) type Tag = [u8; 8];

pub(crate) fn window(unix_time: u64) -> u64 {
    unix_time / WINDOW_SECONDS
}

/// The tags this device announces now: one per known device, current window.
pub(crate) fn announcement_tags(
    identity: &DeviceIdentity,
    peers: &[String],
    unix_time: u64,
) -> Vec<Tag> {
    peers
        .iter()
        .take(MAX_TAGS)
        .filter_map(|peer| {
            identity
                .nearby_announcement_tag(peer, window(unix_time))
                .ok()
        })
        .collect()
}

/// The known device whose tag to this device an announcement carries, for
/// the current or the previous window. A stranger's announcement, and this
/// device's own, match nothing.
pub(crate) fn matching_device(
    identity: &DeviceIdentity,
    peers: &[String],
    announced: &BTreeSet<Tag>,
    unix_time: u64,
) -> Option<String> {
    let current = window(unix_time);
    peers
        .iter()
        .find(|peer| {
            [Some(current), current.checked_sub(1)]
                .into_iter()
                .flatten()
                .filter_map(|window| identity.nearby_announcement_tag_from(peer, window).ok())
                .any(|tag| announced.contains(&tag))
        })
        .cloned()
}

/// Peer `/rpc` origins for the addresses a matched announcement carried. The
/// candidate rule is the same as for every other address.
pub(crate) fn origins(addresses: impl IntoIterator<Item = IpAddr>, port: u16) -> Vec<String> {
    if port == 0 {
        return Vec::new();
    }
    let mut seen = BTreeSet::new();
    addresses
        .into_iter()
        .map(|ip| ip.to_canonical())
        .filter(|ip| super::interfaces::is_candidate(*ip) && seen.insert(*ip))
        .take(MAX_FOUND_PER_DEVICE)
        .map(|ip| format!("http://{}", SocketAddr::new(ip, port)))
        .collect()
}

/// Found addresses by device. Entries expire `FOUND_LIFETIME` after the
/// announcement that last carried them.
#[derive(Default)]
pub(crate) struct Pool {
    found: Mutex<BTreeMap<String, Vec<(String, Instant)>>>,
    /// Devices with an address that was not in the pool before. The reach
    /// controller takes them and ends their retry backoff.
    woken: Mutex<BTreeSet<String>>,
}
impl Pool {
    pub(crate) fn add(&self, device: &str, origins: Vec<String>, now: Instant) {
        if origins.is_empty() {
            return;
        }
        let Ok(mut found) = self.found.lock() else {
            return;
        };
        found.retain(|_, entries| {
            entries.retain(|(_, until)| *until > now);
            !entries.is_empty()
        });
        let entries = found.entry(device.to_owned()).or_default();
        if origins
            .iter()
            .any(|origin| !entries.iter().any(|(known, _)| known == origin))
        {
            if let Ok(mut woken) = self.woken.lock() {
                woken.insert(device.to_owned());
            }
        }
        entries.retain(|(origin, _)| !origins.contains(origin));
        let until = now + FOUND_LIFETIME;
        let mut fresh: Vec<_> = origins.into_iter().map(|origin| (origin, until)).collect();
        fresh.append(entries);
        fresh.truncate(MAX_FOUND_PER_DEVICE);
        *entries = fresh;
    }
    /// Unexpired origins, most recently announced first.
    pub(crate) fn origins(&self, device: &str, now: Instant) -> Vec<String> {
        self.found
            .lock()
            .ok()
            .and_then(|found| {
                found.get(device).map(|entries| {
                    entries
                        .iter()
                        .filter(|(_, until)| *until > now)
                        .map(|(origin, _)| origin.clone())
                        .collect()
                })
            })
            .unwrap_or_default()
    }
    /// Takes the woken devices among `devices`; others stay for their owner.
    pub(crate) fn take_woken(&self, devices: &BTreeSet<String>) -> Vec<String> {
        let Ok(mut woken) = self.woken.lock() else {
            return Vec::new();
        };
        let taken: Vec<_> = woken.intersection(devices).cloned().collect();
        for device in &taken {
            woken.remove(device);
        }
        taken
    }
}

static FOUND: LazyLock<Pool> = LazyLock::new(Pool::default);

/// The live local-discovery fact for one device, read like the nearby link.
pub(crate) fn found(device: &str) -> Vec<String> {
    FOUND.origins(device, Instant::now())
}

/// Known devices that local discovery found at a new address since the last
/// call. Their next reach attempt should not wait for its backoff.
pub(crate) fn take_woken(devices: &BTreeSet<String>) -> Vec<String> {
    FOUND.take_woken(devices)
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn record_found(device: &str, origins: Vec<String>) {
    FOUND.add(device, origins, Instant::now());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identities(count: usize) -> (Vec<tempfile::TempDir>, Vec<DeviceIdentity>) {
        let roots: Vec<_> = (0..count).map(|_| tempfile::tempdir().unwrap()).collect();
        let identities = roots
            .iter()
            .map(|root| DeviceIdentity::load_or_create(root.path()).unwrap())
            .collect();
        (roots, identities)
    }
    fn key(identity: &DeviceIdentity) -> String {
        identity.device_public_key().unwrap().to_owned()
    }

    #[test]
    fn members_match_each_other_and_a_non_member_key_matches_nothing() {
        let (_roots, ids) = identities(4);
        let [a, b, c, stranger] = ids.as_slice() else {
            unreachable!()
        };
        let now = 1_800_000_000;
        // A announces one tag for each device it knows.
        let announced: BTreeSet<_> = announcement_tags(a, &[key(b), key(c)], now)
            .into_iter()
            .collect();
        assert_eq!(announced.len(), 2);
        // B and C each recognise A among the devices they know.
        assert_eq!(
            matching_device(b, &[key(c), key(a)], &announced, now),
            Some(key(a))
        );
        assert_eq!(
            matching_device(c, &[key(b), key(a)], &announced, now),
            Some(key(a))
        );
        // A key outside the federation cannot derive any of A's tags, even
        // when it guesses A's and B's public keys.
        assert_eq!(
            matching_device(stranger, &[key(a), key(b), key(c)], &announced, now),
            None
        );
    }

    #[test]
    fn two_devices_announcements_in_one_window_share_no_tag() {
        let (_roots, ids) = identities(3);
        let [a, b, c] = ids.as_slice() else {
            unreachable!()
        };
        let now = 1_800_000_000;
        let from_a: BTreeSet<_> = announcement_tags(a, &[key(b), key(c)], now)
            .into_iter()
            .collect();
        let from_b: BTreeSet<_> = announcement_tags(b, &[key(a), key(c)], now)
            .into_iter()
            .collect();
        assert_eq!(from_a.len(), 2);
        assert!(
            from_a.is_disjoint(&from_b),
            "an observer can link A and B by a shared tag"
        );
        // Each still recognises the other, and nobody recognises itself.
        assert_eq!(matching_device(b, &[key(a)], &from_a, now), Some(key(a)));
        assert_eq!(matching_device(a, &[key(b)], &from_b, now), Some(key(b)));
        assert_eq!(matching_device(a, &[key(b), key(c)], &from_a, now), None);
    }

    #[test]
    fn a_new_address_wakes_its_device_once_and_a_refresh_does_not() {
        let pool = Pool::default();
        let now = Instant::now();
        let devices: BTreeSet<String> = ["b".to_owned()].into();
        pool.add("b", vec!["http://192.168.1.20:39217".into()], now);
        pool.add("z", vec!["http://192.168.1.90:39217".into()], now);
        assert_eq!(pool.take_woken(&devices), ["b"]);
        assert!(pool.take_woken(&devices).is_empty());
        // The same address again only refreshes its lifetime.
        pool.add("b", vec!["http://192.168.1.20:39217".into()], now);
        assert!(pool.take_woken(&devices).is_empty());
        pool.add("b", vec!["http://192.168.1.21:39217".into()], now);
        assert_eq!(pool.take_woken(&devices), ["b"]);
        // Another controller's device stays for that controller.
        assert_eq!(pool.take_woken(&["z".to_owned()].into()), ["z"]);
    }

    #[test]
    fn a_listener_accepts_the_current_and_previous_window_only() {
        let (_roots, ids) = identities(2);
        let [a, b] = ids.as_slice() else {
            unreachable!()
        };
        let start = 1_800_000_000 - 1_800_000_000 % WINDOW_SECONDS;
        let announced: BTreeSet<_> = announcement_tags(a, &[key(b)], start).into_iter().collect();
        for now in [start, start + WINDOW_SECONDS - 1, start + WINDOW_SECONDS] {
            assert_eq!(
                matching_device(b, &[key(a)], &announced, now),
                Some(key(a)),
                "window offset {}",
                now - start
            );
        }
        // Two windows on, and one window before, the tag no longer matches.
        assert_eq!(
            matching_device(b, &[key(a)], &announced, start + 2 * WINDOW_SECONDS),
            None
        );
        assert_eq!(matching_device(b, &[key(a)], &announced, start - 1), None);
        // The window change itself changes the announced tag.
        assert_ne!(
            announcement_tags(a, &[key(b)], start),
            announcement_tags(a, &[key(b)], start + WINDOW_SECONDS)
        );
    }

    #[test]
    fn found_origins_follow_the_candidate_rule() {
        let ips = [
            "127.0.0.1",
            "fe80::1",
            "169.254.3.4",
            "192.168.1.20",
            "100.64.0.7",
            "fd7a:115c:a1e0::7",
            "::ffff:192.168.1.20",
        ]
        .map(|ip| ip.parse().unwrap());
        assert_eq!(
            origins(ips, 39217),
            [
                "http://192.168.1.20:39217",
                "http://100.64.0.7:39217",
                "http://[fd7a:115c:a1e0::7]:39217",
            ]
        );
        assert!(origins(ips, 0).is_empty());
    }

    #[test]
    fn found_addresses_live_ten_minutes_after_their_last_announcement() {
        let pool = Pool::default();
        let start = Instant::now();
        pool.add("b", vec!["http://192.168.1.20:39217".into()], start);
        let later = start + Duration::from_secs(300);
        pool.add(
            "b",
            vec![
                "http://192.168.1.21:39217".into(),
                "http://192.168.1.20:39217".into(),
            ],
            later,
        );
        assert_eq!(
            pool.origins("b", start + FOUND_LIFETIME),
            ["http://192.168.1.21:39217", "http://192.168.1.20:39217"]
        );
        assert!(pool.origins("b", later + FOUND_LIFETIME).is_empty());
        assert!(pool.origins("c", start).is_empty());
    }
}
