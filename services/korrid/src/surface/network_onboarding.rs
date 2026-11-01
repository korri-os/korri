//! Port of `clients/portal/src/surface/network-onboarding.ts`: one owner for
//! network reads and commands and setup progress. Passwords exist only in
//! call arguments.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

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
const POLL: Duration = Duration::from_secs(2);

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
}

pub struct NetworkOnboarding {
    korrid: Korrid,
    inner: Mutex<Inner>,
    notify: Notify,
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
        })
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

    /// Read now, then every two seconds.
    pub fn start(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                this.refresh().await;
                tokio::time::sleep(POLL).await;
            }
        });
    }

    pub async fn refresh(&self) {
        let (read, current, revision) = {
            let mut inner = self.lock();
            if inner.busy || inner.reading {
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
                state.network = match &network {
                    NetworkStatusOutcome::Ok(snapshot) => Some(network_from(snapshot)),
                    NetworkStatusOutcome::Err(_) => None,
                };
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
            self.lock().busy = false;
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
            this.lock().busy = false;
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
