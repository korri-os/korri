//! Port of `clients/portal/src/surface/network-onboarding.ts`: one owner for
//! network reads and commands and setup progress. Passwords exist only in
//! call arguments.

use std::sync::{Arc, Mutex};

use korri_surface_treaty::{
    SurfaceNetwork, SurfaceNetworkConnection, SurfaceWifi, SurfaceWifiAttempt, SurfaceWifiNetwork,
};

use super::{korrid::Korrid, Notify};
use crate::{
    network::{NetworkConnection, NetworkSnapshot, NetworkStatusOutcome, Wifi, WifiAttempt},
    onboarding::OnboardingStatusOutcome,
};

fn attempt_from(attempt: &WifiAttempt) -> SurfaceWifiAttempt {
    match attempt {
        WifiAttempt::Idle => SurfaceWifiAttempt::Idle,
        WifiAttempt::Connecting { ssid } => SurfaceWifiAttempt::Connecting { ssid: ssid.clone() },
        WifiAttempt::Failed {
            ssid,
            message,
            password_rejected,
        } => SurfaceWifiAttempt::Failed {
            ssid: ssid.clone(),
            message: message.clone(),
            password_rejected: *password_rejected,
        },
    }
}

pub fn network_from(snapshot: &NetworkSnapshot) -> SurfaceNetwork {
    SurfaceNetwork {
        connection: match &snapshot.connection {
            NetworkConnection::Offline => SurfaceNetworkConnection::Offline,
            NetworkConnection::Wired => SurfaceNetworkConnection::Wired,
            NetworkConnection::Wifi {
                ssid,
                strength_percent,
            } => SurfaceNetworkConnection::Wifi {
                ssid: ssid.clone(),
                strength_percent: strength_percent.map(u32::from),
            },
        },
        wifi: match &snapshot.wifi {
            Wifi::NoAdapter => SurfaceWifi::NoAdapter,
            Wifi::Available {
                scanning,
                networks,
                attempt,
            } => SurfaceWifi::Available {
                scanning: *scanning,
                networks: networks
                    .iter()
                    .map(|network| SurfaceWifiNetwork {
                        id: network.id.clone(),
                        ssid: network.ssid.clone(),
                        strength_percent: u32::from(network.strength_percent),
                        secured: network.secured,
                        known: network.known,
                    })
                    .collect(),
                attempt: attempt_from(attempt),
            },
        },
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkOnboardingState {
    pub network: Option<SurfaceNetwork>,
    pub onboarding_needed: Option<bool>,
    pub problem: Option<String>,
}

const FAILURE: &str = "Could not change the network or save setup progress. Try again.";

/// Velvet's `barsOf` (`clients/shell/packages/velvet/src/settings.ts`): the
/// 0 to 4 bars a strength shows.
fn bars(strength_percent: u32) -> u32 {
    match strength_percent {
        75.. => 4,
        50.. => 3,
        25.. => 2,
        1.. => 1,
        0 => 0,
    }
}

/// The network as a surface shows it: each strength as its bars, and the
/// networks in one order. A change inside one bar is no change (ADR 0094).
fn in_bars(network: &SurfaceNetwork) -> SurfaceNetwork {
    let mut shown = network.clone();
    if let SurfaceNetworkConnection::Wifi {
        strength_percent, ..
    } = &mut shown.connection
    {
        *strength_percent = strength_percent.map(bars);
    }
    if let SurfaceWifi::Available { networks, .. } = &mut shown.wifi {
        for network in networks.iter_mut() {
            network.strength_percent = bars(network.strength_percent);
        }
        networks.sort_by(|a, b| a.id.cmp(&b.id));
    }
    shown
}

/// Whether a read shows the same connection, network list and bars.
fn same_shown(current: Option<&SurfaceNetwork>, read: Option<&SurfaceNetwork>) -> bool {
    match (current, read) {
        (Some(current), Some(read)) => in_bars(current) == in_bars(read),
        (None, None) => true,
        _ => false,
    }
}

#[derive(Default)]
struct Inner {
    state: NetworkOnboardingState,
    generation: u64,
    busy: bool,
    reading: bool,
    read_generation: u64,
    onboarding_revision: u64,
    finish_generation: u64,
    finishing: bool,
    /// A notice came while a command or a read was under way: read again
    /// when it ends.
    missed: bool,
}

pub struct NetworkOnboarding {
    korrid: Korrid,
    inner: Mutex<Inner>,
    notify: Notify,
    again: tokio::sync::Notify,
}

/// The next notice of one fact; never, when korrid gives none.
async fn notice(changes: &mut Option<tokio::sync::watch::Receiver<()>>) {
    match changes {
        Some(receiver) => {
            if receiver.changed().await.is_err() {
                *changes = None;
                std::future::pending::<()>().await;
            }
        }
        None => std::future::pending().await,
    }
}

fn with_attempt(network: &SurfaceNetwork, attempt: SurfaceWifiAttempt) -> Option<SurfaceNetwork> {
    match &network.wifi {
        SurfaceWifi::Available {
            scanning, networks, ..
        } => Some(SurfaceNetwork {
            connection: network.connection.clone(),
            wifi: SurfaceWifi::Available {
                scanning: *scanning,
                networks: networks.clone(),
                attempt,
            },
        }),
        SurfaceWifi::NoAdapter => None,
    }
}

fn with_scanning(network: &SurfaceNetwork, scanning: bool) -> Option<SurfaceNetwork> {
    match &network.wifi {
        SurfaceWifi::Available {
            networks, attempt, ..
        } => Some(SurfaceNetwork {
            connection: network.connection.clone(),
            wifi: SurfaceWifi::Available {
                scanning,
                networks: networks.clone(),
                attempt: attempt.clone(),
            },
        }),
        SurfaceWifi::NoAdapter => None,
    }
}

impl NetworkOnboarding {
    pub fn new(korrid: Korrid, notify: Notify) -> Arc<Self> {
        Arc::new(Self {
            korrid,
            inner: Mutex::new(Inner::default()),
            notify,
            again: tokio::sync::Notify::new(),
        })
    }

    /// Read again for a notice that came while the facts were busy.
    fn read_missed(&self, inner: &mut Inner) {
        if std::mem::take(&mut inner.missed) {
            self.again.notify_one();
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("network onboarding poisoned")
    }

    pub fn snapshot(&self) -> NetworkOnboardingState {
        self.lock().state.clone()
    }

    fn publish(&self, change: impl FnOnce(&mut NetworkOnboardingState)) {
        let changed = {
            let mut inner = self.lock();
            let mut next = inner.state.clone();
            change(&mut next);
            let changed = next != inner.state;
            inner.state = next;
            changed
        };
        if changed {
            (self.notify)();
        }
    }

    /// Read now, then on each notice that the network or setup may have
    /// changed (ADR 0094). No timer reads them.
    pub fn start(self: &Arc<Self>) {
        let mut network = self
            .korrid
            .network_changes
            .as_ref()
            .map(|changes| changes());
        let mut onboarding = self
            .korrid
            .onboarding_changes
            .as_ref()
            .map(|changes| changes());
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                // Notices up to here are covered by this read.
                for changes in [&mut network, &mut onboarding].into_iter().flatten() {
                    changes.borrow_and_update();
                }
                this.refresh().await;
                tokio::select! {
                    () = notice(&mut network) => {}
                    () = notice(&mut onboarding) => {}
                    () = this.again.notified() => {}
                }
            }
        });
    }

    pub async fn refresh(&self) {
        let (read, current, revision) = {
            let mut inner = self.lock();
            if inner.busy || inner.reading {
                inner.missed = true;
                return;
            }
            inner.reading = true;
            inner.read_generation += 1;
            inner.generation += 1;
            (
                inner.read_generation,
                inner.generation,
                inner.onboarding_revision,
            )
        };
        let (network, onboarding) = tokio::join!(
            self.korrid.network_status(),
            self.korrid.onboarding_status()
        );
        let fresh = {
            let inner = self.lock();
            current == inner.generation
        };
        if fresh {
            let same_revision = self.lock().onboarding_revision == revision;
            self.publish(|state| {
                let read = match &network {
                    NetworkStatusOutcome::Ok(snapshot) => Some(network_from(snapshot)),
                    NetworkStatusOutcome::Err(_) => None,
                };
                if !same_shown(state.network.as_ref(), read.as_ref()) {
                    state.network = read;
                }
                if same_revision {
                    match &onboarding {
                        OnboardingStatusOutcome::Ok(status) => {
                            state.onboarding_needed = Some(status.needed)
                        }
                        OnboardingStatusOutcome::Err(_) => {
                            state.problem = Some("Could not read setup progress. Try again.".into())
                        }
                    }
                }
            });
        }
        let mut inner = self.lock();
        if read == inner.read_generation {
            inner.reading = false;
            self.read_missed(&mut inner);
        }
    }

    async fn connect(&self, target: String, hidden: bool, password: Option<String>) {
        let (current, ssid) = {
            let mut inner = self.lock();
            if inner.busy {
                return;
            }
            inner.busy = true;
            inner.generation += 1;
            let ssid = if hidden {
                Some(target.clone())
            } else {
                match inner.state.network.as_ref().map(|network| &network.wifi) {
                    Some(SurfaceWifi::Available { networks, .. }) => networks
                        .iter()
                        .find(|item| item.id == target)
                        .map(|item| item.ssid.clone()),
                    _ => None,
                }
            };
            (inner.generation, ssid)
        };
        self.publish(|state| {
            state.problem = None;
            if let (Some(network), Some(ssid)) = (&state.network, &ssid) {
                if let Some(next) = with_attempt(
                    network,
                    SurfaceWifiAttempt::Connecting { ssid: ssid.clone() },
                ) {
                    state.network = Some(next);
                }
            }
        });
        let outcome = if hidden {
            self.korrid.connect_hidden_wifi(target, password).await
        } else {
            self.korrid.connect_wifi(target, password).await
        };
        if self.lock().generation == current {
            self.publish(|state| match &outcome {
                NetworkStatusOutcome::Ok(snapshot) => state.network = Some(network_from(snapshot)),
                NetworkStatusOutcome::Err(_) => {
                    state.problem = Some("Could not join the network. Scan and try again.".into());
                    if let (Some(network), Some(ssid)) = (&state.network, &ssid) {
                        if let Some(next) = with_attempt(
                            network,
                            SurfaceWifiAttempt::Failed {
                                ssid: ssid.clone(),
                                message: "Could not join the network. Try again.".into(),
                                password_rejected: false,
                            },
                        ) {
                            state.network = Some(next);
                        }
                    }
                }
            });
            let mut inner = self.lock();
            inner.busy = false;
            self.read_missed(&mut inner);
        }
    }

    pub fn connect_wifi(self: &Arc<Self>, network_id: String, password: Option<String>) {
        let this = Arc::clone(self);
        tokio::spawn(async move { this.connect(network_id, false, password).await });
    }

    pub fn connect_hidden_wifi(self: &Arc<Self>, ssid: String, password: Option<String>) {
        let this = Arc::clone(self);
        tokio::spawn(async move { this.connect(ssid, true, password).await });
    }

    pub fn scan_wifi(self: &Arc<Self>) {
        let current = {
            let mut inner = self.lock();
            if inner.busy {
                return;
            }
            inner.busy = true;
            inner.generation += 1;
            inner.generation
        };
        self.publish(|state| {
            state.problem = None;
            if let Some(next) = state
                .network
                .as_ref()
                .and_then(|network| with_scanning(network, true))
            {
                state.network = Some(next);
            }
        });
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = this.korrid.scan_wifi().await;
            if this.lock().generation != current {
                return;
            }
            this.publish(|state| match &outcome {
                NetworkStatusOutcome::Ok(snapshot) => state.network = Some(network_from(snapshot)),
                NetworkStatusOutcome::Err(_) => {
                    state.problem = Some("Could not scan for networks. Try again.".into())
                }
            });
            {
                let mut inner = this.lock();
                inner.busy = false;
                this.read_missed(&mut inner);
            }
            this.publish(|state| {
                if let Some(next) = state
                    .network
                    .as_ref()
                    .and_then(|network| with_scanning(network, false))
                {
                    state.network = Some(next);
                }
            });
        });
    }

    /// Finishing or restarting setup is independent of a radio scan. One
    /// command at a time; a reply from an older command is dropped.
    fn change_onboarding(self: &Arc<Self>, restart: bool, failure: &'static str) {
        let current = {
            let mut inner = self.lock();
            if inner.finishing {
                return;
            }
            inner.finishing = true;
            inner.onboarding_revision += 1;
            inner.finish_generation += 1;
            inner.finish_generation
        };
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = if restart {
                this.korrid.restart_onboarding().await
            } else {
                this.korrid.finish_onboarding().await
            };
            if this.lock().finish_generation != current {
                return;
            }
            this.lock().onboarding_revision += 1;
            this.publish(|state| match &outcome {
                OnboardingStatusOutcome::Ok(status) => {
                    state.onboarding_needed = Some(status.needed);
                    state.problem = None;
                }
                // ADR 0082: setup on a device without an owner ends only
                // when the person confirms its recovery words. No surface
                // shows them yet, so Korri says where setup ends instead.
                OnboardingStatusOutcome::Err(refused) if refused.code == "RecoveryWordsNeeded" => {
                    state.problem = Some(refused.message.clone())
                }
                OnboardingStatusOutcome::Err(_) => state.problem = Some(failure.into()),
            });
            this.lock().finishing = false;
        });
    }

    pub fn finish_onboarding(self: &Arc<Self>) {
        self.change_onboarding(false, "Could not save setup progress. Try again.");
    }
    pub fn restart_onboarding(self: &Arc<Self>) {
        self.change_onboarding(true, "Could not restart setup. Try again.");
    }
    pub fn dismiss_problem(&self) {
        self.publish(|state| state.problem = None);
    }
}

/// The portal's generic failure copy, kept for the host's transport errors.
pub const TRANSPORT_FAILURE: &str = FAILURE;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RpcRequest, RpcResponse};
    use serde_json::{json, Value};
    use std::{
        sync::atomic::{AtomicU32, Ordering},
        time::Duration,
    };

    struct Device {
        network: Mutex<Value>,
        needed: Mutex<bool>,
        reads: AtomicU32,
    }

    fn network(connected: u32, nearby: u32) -> Value {
        json!({ "_tag": "Ok", "payload": {
            "connection": { "_tag": "Wifi", "payload": { "ssid": "Home", "strengthPercent": connected } },
            "wifi": { "_tag": "Available", "payload": { "scanning": false, "attempt": { "_tag": "Idle" },
                "networks": [
                    { "id": "/ap/1", "ssid": "Home", "strengthPercent": connected, "secured": true, "known": true },
                    { "id": "/ap/2", "ssid": "Cafe", "strengthPercent": nearby, "secured": false, "known": false }
                ] } }
        } })
    }

    fn korrid(device: &Arc<Device>) -> Korrid {
        let device = Arc::clone(device);
        Korrid::new(Arc::new(move |request: RpcRequest| {
            let device = Arc::clone(&device);
            Box::pin(async move {
                let tag = serde_json::to_value(&request).unwrap()["_tag"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                let outcome = match tag.as_str() {
                    "system.network.status" => {
                        device.reads.fetch_add(1, Ordering::SeqCst);
                        device.network.lock().unwrap().clone()
                    }
                    "system.onboarding.status" => {
                        json!({ "_tag": "Ok", "payload": { "needed": *device.needed.lock().unwrap() } })
                    }
                    _ => return Err(super::super::korrid::Refusal::Unreachable(tag)),
                };
                Ok(serde_json::from_value::<RpcResponse>(
                    json!({ "_tag": tag, "outcome": outcome }),
                )
                .unwrap())
            })
        }))
    }

    fn changes(sender: &tokio::sync::watch::Sender<()>) -> super::super::korrid::Changes {
        let sender = sender.clone();
        Arc::new(move || sender.subscribe())
    }

    async fn until(what: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !what() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("in time");
    }

    struct Watched {
        device: Arc<Device>,
        hub: Arc<NetworkOnboarding>,
        published: Arc<AtomicU32>,
        network: tokio::sync::watch::Sender<()>,
        onboarding: tokio::sync::watch::Sender<()>,
    }

    async fn watched() -> Watched {
        let device = Arc::new(Device {
            network: Mutex::new(network(80, 60)),
            needed: Mutex::new(true),
            reads: AtomicU32::new(0),
        });
        let network = tokio::sync::watch::channel(()).0;
        let onboarding = tokio::sync::watch::channel(()).0;
        let published = Arc::new(AtomicU32::new(0));
        let hub = NetworkOnboarding::new(
            korrid(&device)
                .with_network_changes(changes(&network))
                .with_onboarding_changes(changes(&onboarding)),
            Arc::new({
                let published = Arc::clone(&published);
                move || {
                    published.fetch_add(1, Ordering::SeqCst);
                }
            }),
        );
        hub.start();
        until(|| hub.snapshot().network.is_some()).await;
        Watched {
            device,
            hub,
            published,
            network,
            onboarding,
        }
    }

    fn connected_strength(hub: &NetworkOnboarding) -> Option<u32> {
        match hub.snapshot().network?.connection {
            SurfaceNetworkConnection::Wifi {
                strength_percent, ..
            } => strength_percent,
            _ => None,
        }
    }

    #[test]
    fn bars_are_velvets_bars_of() {
        // `barsOf` in clients/shell/packages/velvet/src/settings.ts.
        for (strength, expected) in [
            (0, 0),
            (1, 1),
            (24, 1),
            (25, 2),
            (49, 2),
            (50, 3),
            (74, 3),
            (75, 4),
            (100, 4),
        ] {
            assert_eq!(bars(strength), expected, "{strength}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn no_notice_no_read() {
        let watched = watched().await;
        // A 2 s poll would have read five more times by now.
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(watched.device.reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_strength_change_inside_one_bar_publishes_nothing_and_a_bar_change_publishes() {
        let watched = watched().await;
        let published = watched.published.load(Ordering::SeqCst);
        // 80 to 76 stays at 4 bars, 60 to 55 at 3.
        *watched.device.network.lock().unwrap() = network(76, 55);
        watched.network.send_replace(());
        until(|| watched.device.reads.load(Ordering::SeqCst) == 2).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(watched.published.load(Ordering::SeqCst), published);
        assert_eq!(connected_strength(&watched.hub), Some(80));
        // 76 to 74 is 3 bars.
        *watched.device.network.lock().unwrap() = network(74, 55);
        watched.network.send_replace(());
        until(|| watched.published.load(Ordering::SeqCst) > published).await;
        assert_eq!(connected_strength(&watched.hub), Some(74));
        // Another network's bars are part of the list.
        let published = watched.published.load(Ordering::SeqCst);
        *watched.device.network.lock().unwrap() = network(74, 20);
        watched.network.send_replace(());
        until(|| watched.published.load(Ordering::SeqCst) > published).await;
    }

    #[tokio::test]
    async fn korrids_own_setup_change_publishes_the_new_status() {
        let watched = watched().await;
        until(|| watched.hub.snapshot().onboarding_needed == Some(true)).await;
        let published = watched.published.load(Ordering::SeqCst);
        *watched.device.needed.lock().unwrap() = false;
        watched.onboarding.send_replace(());
        until(|| watched.hub.snapshot().onboarding_needed == Some(false)).await;
        assert!(watched.published.load(Ordering::SeqCst) > published);
    }

    #[tokio::test]
    async fn a_notice_during_a_read_reads_again_after_it() {
        let watched = watched().await;
        // A read under way (as Reload's) takes the notice for later.
        watched.hub.lock().reading = true;
        *watched.device.network.lock().unwrap() = network(30, 60);
        watched.network.send_replace(());
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(watched.device.reads.load(Ordering::SeqCst), 1);
        {
            let mut inner = watched.hub.lock();
            inner.reading = false;
            watched.hub.read_missed(&mut inner);
        }
        until(|| connected_strength(&watched.hub) == Some(30)).await;
    }
}
