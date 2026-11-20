//! Local discovery (ADR 0089): devices on one local network find each other
//! with no server. An announcement holds no device key, only one rotating tag
//! per known owner device; only that pair can derive it, and the two
//! directions differ (`identity::NearbyPair`). Random filler tags pad every
//! announcement to a fixed count, so the count does not tell how many devices
//! the person has. A matched
//! announcement's addresses join the dial pool for a short time, and a new
//! address ends that peer's reach retry backoff. They are process memory,
//! never persisted, and never proof of liveness: the connection still
//! authenticates by device key.
use crate::identity::{DeviceIdentity, NearbyPair};
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
/// Every announcement carries this many tags, real ones and random filler,
/// or `MAX_TAGS` when the person has more devices than this.
pub(crate) const PADDED_TAGS: usize = 16;

pub(crate) type Tag = [u8; 8];

pub(crate) fn window(unix_time: u64) -> u64 {
    unix_time / WINDOW_SECONDS
}

/// The pair keys for this device and each known device, in directory order.
/// Each pair costs one ECDH, made when the device first appears and kept
/// while the device set and this device's own key stay the same, so a flood
/// of announcements costs only HMACs.
#[derive(Default)]
pub(crate) struct PairKeys {
    local: Option<String>,
    pairs: Vec<(String, NearbyPair)>,
    /// How many pairs were derived, each one ECDH.
    derived: usize,
}
impl PairKeys {
    pub(crate) fn refresh(&mut self, identity: &DeviceIdentity, peers: &[String]) {
        let local = identity.device_public_key().map(str::to_owned);
        if local != self.local {
            self.local = local;
            self.pairs.clear();
        }
        let current = self.pairs.iter().map(|(peer, _)| peer);
        if current.eq(peers.iter()) {
            return;
        }
        let mut kept: BTreeMap<String, NearbyPair> = self.pairs.drain(..).collect();
        for peer in peers {
            let pair = match kept.remove(peer) {
                Some(pair) => Some(pair),
                None => {
                    self.derived += 1;
                    identity.nearby_pair(peer).ok()
                }
            };
            if let Some(pair) = pair {
                self.pairs.push((peer.clone(), pair));
            }
        }
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
    /// The tags this device announces now: one per known device (at most
    /// `MAX_TAGS`), padded with random tags to a fixed count, in random order.
    pub(crate) fn announcement_tags(&self, unix_time: u64) -> Vec<Tag> {
        let window = window(unix_time);
        let mut tags: BTreeSet<Tag> = self
            .pairs
            .iter()
            .take(MAX_TAGS)
            .map(|(_, pair)| pair.tag_to_peer(window))
            .collect();
        let count = if tags.len() <= PADDED_TAGS {
            PADDED_TAGS
        } else {
            MAX_TAGS
        };
        while tags.len() < count {
            tags.insert(rand::random());
        }
        let mut tags: Vec<Tag> = tags.into_iter().collect();
        rand::seq::SliceRandom::shuffle(tags.as_mut_slice(), &mut rand::rng());
        tags
    }
    /// The known device whose tag to this device an announcement carries,
    /// for the current or the previous window. A stranger's announcement,
    /// filler, and this device's own announcement match nothing.
    pub(crate) fn matching_device(
        &self,
        announced: &BTreeSet<Tag>,
        unix_time: u64,
    ) -> Option<String> {
        let current = window(unix_time);
        self.pairs
            .iter()
            .find(|(_, pair)| {
                [Some(current), current.checked_sub(1)]
                    .into_iter()
                    .flatten()
                    .any(|window| announced.contains(&pair.tag_from_peer(window)))
            })
            .map(|(peer, _)| peer.clone())
    }
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

    fn keys(identity: &DeviceIdentity, peers: &[String]) -> PairKeys {
        let mut keys = PairKeys::default();
        keys.refresh(identity, peers);
        keys
    }
    fn tags(identity: &DeviceIdentity, peers: &[String], now: u64) -> BTreeSet<Tag> {
        keys(identity, peers)
            .announcement_tags(now)
            .into_iter()
            .collect()
    }

    #[test]
    fn members_match_each_other_and_a_non_member_key_matches_nothing() {
        let (_roots, ids) = identities(4);
        let [a, b, c, stranger] = ids.as_slice() else {
            unreachable!()
        };
        let now = 1_800_000_000;
        // A announces one tag for each device it knows, among filler.
        let announced = tags(a, &[key(b), key(c)], now);
        // B and C each recognise A among the devices they know.
        assert_eq!(
            keys(b, &[key(c), key(a)]).matching_device(&announced, now),
            Some(key(a))
        );
        assert_eq!(
            keys(c, &[key(b), key(a)]).matching_device(&announced, now),
            Some(key(a))
        );
        // A key outside the federation cannot derive any of A's tags, even
        // when it guesses A's and B's public keys.
        assert_eq!(
            keys(stranger, &[key(a), key(b), key(c)]).matching_device(&announced, now),
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
        let from_a = tags(a, &[key(b), key(c)], now);
        let from_b = tags(b, &[key(a), key(c)], now);
        assert!(
            from_a.is_disjoint(&from_b),
            "an observer can link A and B by a shared tag"
        );
        // Each still recognises the other, and nobody recognises itself.
        assert_eq!(
            keys(b, &[key(a)]).matching_device(&from_a, now),
            Some(key(a))
        );
        assert_eq!(
            keys(a, &[key(b)]).matching_device(&from_b, now),
            Some(key(b))
        );
        assert_eq!(
            keys(a, &[key(b), key(c)]).matching_device(&from_a, now),
            None
        );
    }

    #[test]
    fn announcements_carry_the_same_tag_count_whatever_the_device_count() {
        let (_roots, ids) = identities(5);
        let [a, b, c, d, e] = ids.as_slice() else {
            unreachable!()
        };
        let now = 1_800_000_000;
        let one = keys(a, &[key(b)]).announcement_tags(now);
        let four = keys(a, &[key(b), key(c), key(d), key(e)]).announcement_tags(now);
        let none = PairKeys::default().announcement_tags(now);
        assert_eq!(one.len(), PADDED_TAGS);
        assert_eq!(four.len(), PADDED_TAGS);
        assert_eq!(none.len(), PADDED_TAGS);
        // Filler is fresh randomness, and every tag is distinct.
        assert_eq!(one.iter().collect::<BTreeSet<_>>().len(), PADDED_TAGS);
        let again = keys(a, &[key(b)]).announcement_tags(now);
        assert_eq!(
            one.iter()
                .collect::<BTreeSet<_>>()
                .intersection(&again.iter().collect())
                .count(),
            1,
            "only the real tag repeats within a window"
        );
        // The real tags still match among the filler.
        let announced: BTreeSet<_> = four.into_iter().collect();
        assert_eq!(
            keys(e, &[key(a)]).matching_device(&announced, now),
            Some(key(a))
        );
    }

    #[test]
    fn pair_keys_derive_one_ecdh_per_new_device_and_keep_it() {
        let (_roots, ids) = identities(4);
        let [a, b, c, d] = ids.as_slice() else {
            unreachable!()
        };
        let mut pairs = PairKeys::default();
        pairs.refresh(a, &[key(b), key(c)]);
        assert_eq!(pairs.derived, 2);
        // An unchanged device set, and any number of announcements matched
        // and made, derive nothing more.
        let flood = tags(b, &[key(a)], 1_800_000_000);
        for now in 0..1000 {
            pairs.refresh(a, &[key(b), key(c)]);
            let _ = pairs.matching_device(&flood, 1_800_000_000 + now);
            let _ = pairs.announcement_tags(1_800_000_000 + now);
        }
        assert_eq!(pairs.derived, 2);
        assert_eq!(pairs.matching_device(&flood, 1_800_000_000), Some(key(b)));
        // A changed set derives only the new device's pair.
        pairs.refresh(a, &[key(b), key(c), key(d)]);
        assert_eq!(pairs.derived, 3);
        pairs.refresh(a, &[key(c)]);
        assert_eq!(pairs.derived, 3);
        assert_eq!(pairs.matching_device(&flood, 1_800_000_000), None);
        // Another local identity starts over.
        pairs.refresh(d, &[key(c)]);
        assert_eq!(pairs.derived, 4);
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
        let announced = tags(a, &[key(b)], start);
        let listener = keys(b, &[key(a)]);
        for now in [start, start + WINDOW_SECONDS - 1, start + WINDOW_SECONDS] {
            assert_eq!(
                listener.matching_device(&announced, now),
                Some(key(a)),
                "window offset {}",
                now - start
            );
        }
        // Two windows on, and one window before, the tag no longer matches.
        assert_eq!(
            listener.matching_device(&announced, start + 2 * WINDOW_SECONDS),
            None
        );
        assert_eq!(listener.matching_device(&announced, start - 1), None);
        // The window change itself changes the announced real tag.
        let next = tags(a, &[key(b)], start + WINDOW_SECONDS);
        assert_eq!(listener.matching_device(&next, start), None);
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
