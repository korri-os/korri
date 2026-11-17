//! The host's side of the rendezvous: it finds its public address with STUN, publishes its
//! record for every device that has a secret, looks for those devices' records, and answers
//! their punches with a tunnel to its own ports.

use crate::{
    crypto::Key,
    node::{unix_now, Node},
    punch,
    record::{self, Keys, Nat, Ports, Record, Role},
    stun,
    tunnel::{self, Bridge, State, Tunnel},
    wire, Millis,
};
use std::{
    collections::{HashMap, HashSet},
    io,
    net::{IpAddr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    thread,
    time::Duration,
};

/// STUN again this often: a router's public address can change.
const STUN_EVERY: Millis = 5 * 60_000;
/// Republish a record this often, well inside the two hours DHT nodes keep it.
const PUBLISH_EVERY: Millis = 30 * 60_000;
/// Look for the record of a device heard from in the last [`ACTIVE_FOR`] this often,
const POLL_EVERY: Millis = punch::HOST_POLL;
/// and for every other device's this often. The device's punch budget covers this wait.
const POLL_IDLE: Millis = punch::HOST_POLL_IDLE;
/// How long a device counts as active after its record last answered this host, or while it
/// has a path open.
const ACTIVE_FOR: Millis = 10 * 60_000;
// A record published just after a slow poll began is found within the device's punch budget:
// the interval from that poll's start, the loop's lateness, one lookup (both windows run at
// once), and the punches.
const _: () =
    assert!(POLL_IDLE + punch::HOST_LOOP + crate::node::LOOKUP_DEADLINE < punch::AFTER_PUBLISH);
/// A device's path problem shows this long after the device last tried: one still trying
/// renews it with every punch or record.
const PATH_ERROR_FOR: Millis = 30_000;
/// At most this many devices' records are looked for at once, each on a thread of its own, so
/// one device's slow lookups never hold another's back.
const MAX_POLLS: usize = 4;

#[derive(Clone, Debug)]
pub struct HostConfig {
    pub ports: Ports,
    /// Where the host's own services listen: 127.0.0.1 when they listen everywhere.
    pub service: IpAddr,
    /// Tunnels connect from a loopback address per device (127.77.x.y), so the host tells
    /// devices apart; needs `service` on loopback.
    pub per_device_source: bool,
    /// Where the rendezvous socket binds (IPv4, port 0 for any).
    pub bind: SocketAddr,
    pub stun_servers: Vec<String>,
    pub bootstrap: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// A stable id: the host uses the device certificate's SHA-256.
    pub id: String,
    pub name: String,
    pub secret: Key,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathStatus {
    pub device: String,
    pub peer: Option<SocketAddr>,
    pub connected: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// The rendezvous socket's public addresses, as STUN servers see them.
    pub public: Vec<SocketAddr>,
    pub nat: Nat,
    /// Why STUN found nothing, if it did not.
    pub stun_error: Option<String>,
    /// Devices whose record the DHT took on the last publish.
    pub published: usize,
    pub devices: usize,
    pub dht_error: Option<String>,
    pub paths: Vec<PathStatus>,
    /// Devices whose path did not start lately, by name, and why.
    pub path_errors: Vec<(String, String)>,
}

struct DeviceState {
    device: Device,
    keys: Keys,
    tag: [u8; 8],
    published: Option<(u64, Millis, Vec<SocketAddr>)>,
    last_poll: Option<Millis>,
    /// A look for its record is under way.
    polling: bool,
    /// When a fresh record of the device last named this host's session.
    heard: Option<Millis>,
    answered: Option<[u8; 16]>,
}

/// How often a device's record is looked for: more often while it is trying to connect, or was
/// lately (its record answered this host, or it has a path open in `open`).
fn poll_interval(state: &DeviceState, open: &[[u8; 8]], now: Millis) -> Millis {
    let heard = state
        .heard
        .is_some_and(|at| now.saturating_sub(at) < ACTIVE_FOR);
    if heard || open.contains(&state.tag) {
        POLL_EVERY
    } else {
        POLL_IDLE
    }
}

/// What starting a device's path needs, copied out of its state so no lock is held meanwhile.
#[derive(Clone)]
struct Peer {
    tag: [u8; 8],
    keys: Keys,
    device: Device,
}

impl Peer {
    fn of(state: &DeviceState) -> Self {
        Self {
            tag: state.tag,
            keys: state.keys.clone(),
            device: state.device.clone(),
        }
    }
}

/// Each open tunnel by its path tag, with the device session it was opened for.
type Tunnels = HashMap<[u8; 8], ([u8; 16], Tunnel)>;

/// Why a device's path did not start, by its tag: (device name, words, when).
type PathErrors = HashMap<[u8; 8], (String, String, Millis)>;

struct Shared {
    node: Node,
    config: HostConfig,
    session: [u8; 16],
    devices: Mutex<Vec<DeviceState>>,
    /// Read for every datagram, written when a path starts or ends.
    tunnels: RwLock<Tunnels>,
    status: Mutex<Status>,
    path_errors: Mutex<PathErrors>,
    /// Paths a punch is starting (tag and device session), so repeated punches start one.
    starting: Mutex<HashSet<([u8; 8], [u8; 16])>>,
    /// The records are being published.
    publishing: AtomicBool,
    stop: AtomicBool,
}

#[derive(Clone)]
pub struct HostService {
    shared: Arc<Shared>,
}

impl HostService {
    pub fn start(config: HostConfig) -> io::Result<Self> {
        let node = Node::bind(config.bind, config.bootstrap.clone())?;
        let shared = Arc::new(Shared {
            node: node.clone(),
            config,
            session: crate::crypto::random(),
            devices: Mutex::new(Vec::new()),
            tunnels: RwLock::new(HashMap::new()),
            status: Mutex::new(Status::default()),
            path_errors: Mutex::new(HashMap::new()),
            starting: Mutex::new(HashSet::new()),
            publishing: AtomicBool::new(false),
            stop: AtomicBool::new(false),
        });
        let handling = Arc::downgrade(&shared);
        node.set_handler(Box::new(move |datagram, from| {
            if let Some(shared) = handling.upgrade() {
                HostService { shared }.receive(datagram, from);
            }
        }));
        let service = Self { shared };
        let running = service.clone();
        thread::Builder::new()
            .name("internet play host".into())
            .spawn(move || running.run())?;
        Ok(service)
    }

    /// The host session nonce, which `/internet-play/v1` also hands a device on the LAN.
    pub fn session(&self) -> [u8; 16] {
        self.shared.session
    }

    /// The rendezvous socket's address on this PC.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.shared.node.local_addr()
    }

    /// The devices that have a secret; others' paths close.
    pub fn set_devices(&self, devices: Vec<Device>) {
        let mut states = self.shared.devices.lock().unwrap();
        let mut next = Vec::with_capacity(devices.len());
        for device in devices {
            match states.iter().position(|state| state.device == device) {
                Some(index) => next.push(states.swap_remove(index)),
                None => {
                    let keys = Keys::new(device.secret);
                    let tag = keys.device_tag(&self.shared.session);
                    next.push(DeviceState {
                        device,
                        keys,
                        tag,
                        published: None,
                        last_poll: None,
                        polling: false,
                        heard: None,
                        answered: None,
                    });
                }
            }
        }
        *states = next;
        let tags: Vec<[u8; 8]> = states.iter().map(|state| state.tag).collect();
        drop(states);
        let mut tunnels = self.shared.tunnels.write().unwrap();
        tunnels.retain(|tag, (_, tunnel)| {
            let keep = tags.contains(tag);
            if !keep {
                tunnel.close("the device is no longer paired");
            }
            keep
        });
    }

    pub fn status(&self) -> Status {
        let mut status = self.shared.status.lock().unwrap().clone();
        let devices = self.shared.devices.lock().unwrap();
        status.devices = devices.len();
        let names: HashMap<[u8; 8], String> = devices
            .iter()
            .map(|state| (state.tag, state.device.name.clone()))
            .collect();
        drop(devices);
        status.paths = self
            .shared
            .tunnels
            .read()
            .unwrap()
            .iter()
            .filter(|(_, (_, tunnel))| !tunnel.is_closed())
            .map(|(tag, (_, tunnel))| {
                let state = tunnel.state();
                PathStatus {
                    device: names.get(tag).cloned().unwrap_or_default(),
                    peer: match state {
                        State::Connected(peer) => Some(peer),
                        _ => None,
                    },
                    connected: matches!(state, State::Connected(_)),
                }
            })
            .collect();
        let now = self.shared.node.now();
        status.path_errors = self
            .shared
            .path_errors
            .lock()
            .unwrap()
            .values()
            .filter(|(_, _, at)| now.saturating_sub(*at) < PATH_ERROR_FOR)
            .map(|(name, words, _)| (name.clone(), words.clone()))
            .collect();
        status
    }

    /// Notes why `peer`'s path did not start, for [`Status::path_errors`].
    fn path_error(&self, peer: &Peer, words: String) {
        let entry = (peer.device.name.clone(), words, self.shared.node.now());
        self.shared
            .path_errors
            .lock()
            .unwrap()
            .insert(peer.tag, entry);
    }

    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::Release);
        for (_, (_, tunnel)) in self.shared.tunnels.write().unwrap().drain() {
            tunnel.close("the host stopped");
        }
        self.shared.node.stop();
    }

    fn stopped(&self) -> bool {
        self.shared.stop.load(Ordering::Acquire)
    }

    fn source(&self, device: &Device) -> IpAddr {
        if self.shared.config.per_device_source {
            tunnel::loopback_for(77, &device.id)
        } else {
            self.shared.config.service
        }
    }

    /// Without a source address per device, every tunnel reaches the host from the same
    /// address, and the host keys sessions and launches by address: one device at a time, so
    /// one device can never step into another's session.
    fn busy(&self, tunnels: &Tunnels, tag: &[u8; 8]) -> bool {
        !self.shared.config.per_device_source
            && tunnels
                .iter()
                .any(|(other, (_, tunnel))| other != tag && !tunnel.is_closed())
    }

    fn waits(&self, peer: &Peer) {
        self.path_error(peer, format!(
            "{} waits: another device is playing over the internet, and the host listens on one address",
            peer.device.name
        ));
    }

    /// Starts (or restarts) the path to a device for its session `client`. The tunnel starts
    /// (sockets bound, threads started) with no lock held, so other paths' media and the
    /// console's status never wait on it.
    fn open(&self, peer: &Peer, client: [u8; 16], candidates: Vec<SocketAddr>) -> Option<Tunnel> {
        {
            let tunnels = self.shared.tunnels.read().unwrap();
            if let Some((session, tunnel)) = tunnels.get(&peer.tag) {
                if *session == client && !tunnel.is_closed() {
                    tunnel.add_candidates(&candidates);
                    return Some(tunnel.clone());
                }
            }
            if self.busy(&tunnels, &peer.tag) {
                drop(tunnels);
                self.waits(peer);
                return None;
            }
        }
        let keys = peer.keys.path(&self.shared.session, &client);
        let bridge = Bridge::Host {
            service: self.shared.config.service,
            source: self.source(&peer.device),
            ports: self.shared.config.ports,
            device: peer.device.id.clone(),
        };
        let tunnel = match Tunnel::start(
            self.shared.node.clone(),
            Role::Host,
            keys,
            client,
            candidates.clone(),
            bridge,
        ) {
            Ok(tunnel) => tunnel,
            Err(error) => {
                self.path_error(
                    peer,
                    format!("a path to {} could not start: {error}", peer.device.name),
                );
                return None;
            }
        };
        let mut tunnels = self.shared.tunnels.write().unwrap();
        // Meanwhile a path for this session, or another device's path, may have started.
        if let Some((session, existing)) = tunnels.get(&peer.tag) {
            if *session == client && !existing.is_closed() {
                let existing = existing.clone();
                drop(tunnels);
                tunnel.close("the path started twice");
                existing.add_candidates(&candidates);
                return Some(existing);
            }
        }
        if self.busy(&tunnels, &peer.tag) {
            drop(tunnels);
            tunnel.close("another device is playing over the internet");
            self.waits(peer);
            return None;
        }
        let old = tunnels.insert(peer.tag, (client, tunnel.clone()));
        drop(tunnels);
        if let Some((_, old)) = old {
            old.close("the device connected again");
        }
        self.shared.path_errors.lock().unwrap().remove(&peer.tag);
        Some(tunnel)
    }

    /// Takes a datagram for a path; an open path decrypts it in place.
    fn receive(&self, datagram: &mut [u8], from: SocketAddr) {
        let Some(tag) = wire::tag(datagram) else {
            return;
        };
        let punch_session = wire::punch_session(datagram);
        {
            // Read-locked through the tunnel's handling, which never touches this map.
            let tunnels = self.shared.tunnels.read().unwrap();
            if let Some((session, tunnel)) = tunnels.get(&tag) {
                if !tunnel.is_closed() && punch_session.map_or(true, |punched| punched == *session)
                {
                    tunnel.receive(datagram, from);
                    return;
                }
                // A new session replaces a connected path only once the old one is gone, so an
                // old punch replayed by someone on the way can't break a working stream.
                if !tunnel.is_closed() && matches!(tunnel.state(), State::Connected(_)) {
                    return;
                }
            }
        }
        let Some(client) = punch_session else { return };
        let peer = {
            let devices = self.shared.devices.lock().unwrap();
            let Some(state) = devices.iter().find(|state| state.tag == tag) else {
                return;
            };
            Peer::of(state)
        };
        // Only a punch that opens with this device's keys starts a path.
        let keys = peer.keys.path(&self.shared.session, &client);
        // On a copy: the path the punch starts opens it again.
        if wire::Opener::new(&keys.client_to_host, keys.tag)
            .open(datagram)
            .is_none()
        {
            return;
        }
        // A device that has to wait is told here, without a thread for each of its punches.
        if self.busy(&self.shared.tunnels.read().unwrap(), &tag) {
            self.waits(&peer);
            return;
        }
        // The path starts on a thread of its own: this thread takes every path's media. Punches
        // that come meanwhile are dropped; the device sends more.
        if !self.shared.starting.lock().unwrap().insert((tag, client)) {
            return;
        }
        let opening = self.clone();
        let mut datagram = datagram.to_vec();
        let spawned = thread::Builder::new()
            .name("internet play open".into())
            .spawn(move || {
                let tunnel = opening.open(&peer, client, vec![from]);
                opening
                    .shared
                    .starting
                    .lock()
                    .unwrap()
                    .remove(&(tag, client));
                if let Some(tunnel) = tunnel {
                    tunnel.receive(&mut datagram, from);
                }
            });
        if spawned.is_err() {
            self.shared.starting.lock().unwrap().remove(&(tag, client));
        }
    }

    fn run(&self) {
        let mut next_stun: Millis = 0;
        while !self.stopped() {
            let now = self.shared.node.now();
            if now >= next_stun {
                next_stun = now + STUN_EVERY;
                self.discover();
            }
            self.serve_devices();
            self.shared
                .tunnels
                .write()
                .unwrap()
                .retain(|_, (_, tunnel)| !tunnel.is_closed());
            let now = self.shared.node.now();
            self.shared
                .path_errors
                .lock()
                .unwrap()
                .retain(|_, (_, _, at)| now.saturating_sub(*at) < PATH_ERROR_FOR);
            for _ in 0..10 {
                if self.stopped() {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    fn discover(&self) {
        let answers = self.shared.node.stun(&self.shared.config.stun_servers);
        let local = self.shared.node.local_addr().ok();
        let mut public: Vec<SocketAddr> = Vec::new();
        for answer in &answers {
            if !public.contains(answer) {
                public.push(*answer);
            }
        }
        let nat = local.map_or(Nat::Unknown, |local| {
            Nat::from_mapping(stun::mapping(local, &answers))
        });
        let changed = {
            let mut status = self.shared.status.lock().unwrap();
            status.stun_error = answers.is_empty().then(|| {
                "no STUN server answered: the PC may have no internet connection, or a firewall blocks UDP".to_owned()
            });
            let changed = status.public != public;
            status.public = public;
            status.nat = nat;
            changed
        };
        // A changed address is republished right away.
        if changed {
            for state in self.shared.devices.lock().unwrap().iter_mut() {
                state.published = None;
            }
        }
    }

    /// Starts the publications and the looks for devices' records that are due; both run on
    /// threads of their own, so a slow DHT never holds this loop or another device back.
    fn serve_devices(&self) {
        let (public, nat) = {
            let status = self.shared.status.lock().unwrap();
            (status.public.clone(), status.nat)
        };
        let window = record::window(unix_now());
        let open: Vec<[u8; 8]> = self
            .shared
            .tunnels
            .read()
            .unwrap()
            .iter()
            .filter(|(_, (_, tunnel))| !tunnel.is_closed())
            .map(|(tag, _)| *tag)
            .collect();
        let now = self.shared.node.now();
        let mut devices = self.shared.devices.lock().unwrap();
        let publish: Vec<(String, Keys)> = devices
            .iter()
            .filter(|state| {
                !public.is_empty()
                    && match &state.published {
                        None => true,
                        Some((at_window, at, endpoints)) => {
                            // Saturating: a publication that ended after `now` was taken.
                            *at_window != window
                                || now.saturating_sub(*at) > PUBLISH_EVERY
                                || *endpoints != public
                        }
                    }
            })
            .map(|state| (state.device.id.clone(), state.keys.clone()))
            .collect();
        // The longest waiting first, while threads are free.
        let mut due: Vec<usize> = (0..devices.len())
            .filter(|&index| {
                let state = &devices[index];
                !state.polling
                    && state.last_poll.map_or(true, |at| {
                        now.saturating_sub(at) >= poll_interval(state, &open, now)
                    })
            })
            .collect();
        due.sort_by_key(|&index| devices[index].last_poll);
        let free = MAX_POLLS.saturating_sub(devices.iter().filter(|state| state.polling).count());
        let mut poll = Vec::new();
        for index in due.into_iter().take(free) {
            // The interval counts from the poll's start, so a slow lookup does not stretch it.
            devices[index].last_poll = Some(now);
            devices[index].polling = true;
            poll.push((
                devices[index].device.id.clone(),
                devices[index].keys.clone(),
            ));
        }
        let published = devices
            .iter()
            .filter(|state| state.published.is_some())
            .count();
        drop(devices);
        self.shared.status.lock().unwrap().published = published;

        for (id, keys) in poll {
            let polling = self.clone();
            let device = id.clone();
            let spawned = thread::Builder::new()
                .name("internet play poll".into())
                .spawn(move || polling.poll_device(&device, &keys, unix_now()));
            if spawned.is_err() {
                let mut devices = self.shared.devices.lock().unwrap();
                if let Some(state) = devices.iter_mut().find(|state| state.device.id == id) {
                    state.polling = false;
                }
            }
        }
        if !publish.is_empty() && !self.shared.publishing.swap(true, Ordering::AcqRel) {
            let publishing = self.clone();
            let spawned = thread::Builder::new()
                .name("internet play publish".into())
                .spawn(move || {
                    publishing.publish(publish, public, nat);
                    publishing.shared.publishing.store(false, Ordering::Release);
                });
            if spawned.is_err() {
                self.shared.publishing.store(false, Ordering::Release);
            }
        }
    }

    /// Publishes the host's record for each device in `due`, one after another.
    fn publish(&self, due: Vec<(String, Keys)>, public: Vec<SocketAddr>, nat: Nat) {
        for (id, keys) in due {
            if self.stopped() {
                return;
            }
            let unix = unix_now();
            let window = record::window(unix);
            let now = self.shared.node.now();
            let record = Record {
                role: Role::Host,
                time: unix,
                session: self.shared.session,
                peer_session: [0; 16],
                nat,
                endpoints: public.clone(),
                ports: Some(self.shared.config.ports),
            };
            let confirmed =
                self.shared
                    .node
                    .put(&record.item(&keys, window, crate::crypto::random()));
            let mut devices = self.shared.devices.lock().unwrap();
            if let Some(state) = devices.iter_mut().find(|state| state.device.id == id) {
                if confirmed > 0 {
                    state.published = Some((window, now, public.clone()));
                }
            }
            drop(devices);
            self.shared.status.lock().unwrap().dht_error = (confirmed == 0).then(|| {
                "no DHT node stored the PC's record: UDP to the internet may be blocked".to_owned()
            });
        }
    }

    fn poll_device(&self, id: &str, keys: &Keys, unix: u64) {
        let window = record::window(unix);
        let get = |window: u64| {
            let (item, _) = self
                .shared
                .node
                .get(&keys.public(Role::Client), &keys.salt(Role::Client, window));
            item.and_then(|item| Record::from_item(keys, Role::Client, window, &item))
        };
        // A record of the previous window is older than the seconds this window has run, so it
        // can be fresh only in this window's first CLIENT_FRESH_SECONDS; then both windows are
        // asked at once, so the look takes one lookup's time.
        let fresh_previous = unix % record::WINDOW_SECONDS <= record::CLIENT_FRESH_SECONDS;
        let previous = window.saturating_sub(1);
        let found = if fresh_previous {
            thread::scope(|scope| {
                let earlier = thread::Builder::new()
                    .name("internet play poll".into())
                    .spawn_scoped(scope, || get(previous));
                let current = get(window);
                match earlier {
                    Ok(earlier) => current.or_else(|| earlier.join().ok().flatten()),
                    Err(_) => current.or_else(|| get(previous)),
                }
            })
        } else {
            get(window)
        };
        let now = self.shared.node.now();
        let mut devices = self.shared.devices.lock().unwrap();
        let Some(state) = devices.iter_mut().find(|state| state.device.id == id) else {
            return;
        };
        state.polling = false;
        let Some(record) = found else { return };
        let fresh = unix.abs_diff(record.time) <= record::CLIENT_FRESH_SECONDS;
        if !fresh || record.peer_session != self.shared.session {
            return;
        }
        state.heard = Some(now);
        if state.answered == Some(record.session) {
            return;
        }
        state.answered = Some(record.session);
        let peer = Peer::of(state);
        drop(devices);
        // A punch may be starting this very path: one start per device session, so no two
        // tunnels ever seal under the same keys.
        let claim = (peer.tag, record.session);
        if !self.shared.starting.lock().unwrap().insert(claim) {
            return;
        }
        let candidates = punch::candidates(&record.endpoints, record.nat);
        self.open(&peer, record.session, candidates);
        self.shared.starting.lock().unwrap().remove(&claim);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> HostConfig {
        HostConfig {
            ports: Ports::default(),
            service: "127.0.0.1".parse().unwrap(),
            per_device_source: false,
            bind: "127.0.0.1:0".parse().unwrap(),
            stun_servers: Vec::new(),
            bootstrap: Vec::new(),
        }
    }

    /// The host's paths, once `count` of them are there (a punch starts its path on a thread of
    /// its own), or after two seconds.
    fn paths(host: &HostService, count: usize) -> Vec<PathStatus> {
        for _ in 0..100 {
            let paths = host.status().paths;
            if paths.len() >= count && paths.iter().all(|path| path.connected) {
                return paths;
            }
            thread::sleep(Duration::from_millis(20));
        }
        host.status().paths
    }

    #[test]
    fn a_device_that_is_not_trying_is_looked_for_less_often() {
        let device = Device {
            id: "a".into(),
            name: "Phone".into(),
            secret: [3; 32],
        };
        let keys = Keys::new(device.secret);
        let mut state = DeviceState {
            tag: keys.device_tag(&[1; 16]),
            keys,
            device,
            published: None,
            last_poll: None,
            polling: false,
            heard: None,
            answered: None,
        };
        let now = 3_600_000;
        assert_eq!(poll_interval(&state, &[], now), POLL_IDLE);
        // An open path, or a record that answered lately, keeps it fast.
        assert_eq!(poll_interval(&state, &[state.tag], now), POLL_EVERY);
        state.heard = Some(now - ACTIVE_FOR + 1);
        assert_eq!(poll_interval(&state, &[], now), POLL_EVERY);
        state.heard = Some(now - ACTIVE_FOR);
        assert_eq!(poll_interval(&state, &[], now), POLL_IDLE);
    }

    #[test]
    fn only_a_paired_devices_punch_opens_a_path() {
        let host = HostService::start(config()).unwrap();
        let device = Device {
            id: "a".into(),
            name: "Phone".into(),
            secret: [3; 32],
        };
        host.set_devices(vec![device.clone()]);
        let keys = Keys::new(device.secret);
        let path = keys.path(&host.session(), &[9; 16]);
        let probe = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let from = probe.local_addr().unwrap();

        // A stranger's punch with the right tag but another secret opens nothing.
        let stranger = Keys::new([4; 32]).path(&host.session(), &[9; 16]);
        let forged = wire::Sealer::new(&stranger.client_to_host, path.tag, [9; 16]).seal(
            wire::Kind::Punch,
            0,
            &[],
        );
        host.receive(&mut forged.clone(), from);
        assert!(host.status().paths.is_empty());

        let punch = wire::Sealer::new(&path.client_to_host, path.tag, [9; 16]).seal(
            wire::Kind::Punch,
            0,
            &[],
        );
        host.receive(&mut punch.clone(), from);
        let paths = paths(&host, 1);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].device, "Phone");
        assert!(paths[0].connected);

        // Unpairing closes it.
        host.set_devices(Vec::new());
        assert!(host.status().paths.is_empty());
        host.stop();
    }

    #[test]
    fn a_host_on_one_address_serves_one_device_at_a_time() {
        let host = HostService::start(config()).unwrap();
        let phone = Device {
            id: "a".into(),
            name: "Phone".into(),
            secret: [3; 32],
        };
        let tablet = Device {
            id: "b".into(),
            name: "Tablet".into(),
            secret: [5; 32],
        };
        host.set_devices(vec![phone.clone(), tablet.clone()]);
        let punch = |device: &Device, session: [u8; 16]| {
            let path = Keys::new(device.secret).path(&host.session(), &session);
            wire::Sealer::new(&path.client_to_host, path.tag, session).seal(
                wire::Kind::Punch,
                0,
                &[],
            )
        };
        let probe = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let from = probe.local_addr().unwrap();

        host.receive(&mut punch(&phone, [9; 16]), from);
        assert_eq!(paths(&host, 1).len(), 1);
        host.receive(&mut punch(&tablet, [8; 16]), from);
        for _ in 0..100 {
            if !host.status().path_errors.is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let status = host.status();
        assert_eq!(status.paths.len(), 1);
        assert_eq!(status.paths[0].device, "Phone");
        // The tablet's wait is its own, not the DHT's.
        assert_eq!(status.path_errors.len(), 1);
        assert_eq!(status.path_errors[0].0, "Tablet");
        assert!(status.path_errors[0].1.contains("waits"));
        assert_eq!(status.dht_error, None);

        // Once the phone's path is gone, the tablet gets one, and its wait is over.
        host.set_devices(vec![tablet.clone()]);
        host.receive(&mut punch(&tablet, [8; 16]), from);
        let paths = paths(&host, 1);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].device, "Tablet");
        assert!(host.status().path_errors.is_empty());
        host.stop();
    }
}
