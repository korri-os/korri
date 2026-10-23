//! Local network facts and activation. NetworkManager owns profiles and keys.
use crate::RpcFailure;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use typeshare::typeshare;
use zeroize::Zeroize;

mod dbus;
pub mod nearby;
#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NetworkStatusRequest {}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WifiScanRequest {}

#[typeshare]
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WifiConnectRequest {
    pub network_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}
impl std::fmt::Debug for WifiConnectRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WifiConnectRequest")
            .field("network_id", &self.network_id)
            .finish_non_exhaustive()
    }
}
// Every clone is zeroized by its own Drop. These requests are local-browser-only,
// so the peer retry clone (upstream_native.rs:515) never carries them. Covered by
// authorization::tests::network_and_onboarding_require_the_local_portal_and_deny_every_peer.
impl Drop for WifiConnectRequest {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}
#[typeshare]
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WifiConnectHiddenRequest {
    pub ssid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}
impl std::fmt::Debug for WifiConnectHiddenRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WifiConnectHiddenRequest")
            .field("ssid", &self.ssid)
            .finish_non_exhaustive()
    }
}
// Every clone is zeroized by its own Drop. These requests are local-browser-only,
// so the peer retry clone (upstream_native.rs:515) never carries them. Covered by
// authorization::tests::network_and_onboarding_require_the_local_portal_and_deny_every_peer.
impl Drop for WifiConnectHiddenRequest {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

// These facts follow the surface treaty and NetworkManager's native properties.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum NetworkConnection {
    Offline,
    Wired,
    Wifi {
        ssid: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "strengthPercent")]
        strength_percent: Option<u8>,
    },
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiNetwork {
    pub id: String,
    pub ssid: String,
    pub strength_percent: u8,
    pub secured: bool,
    pub known: bool,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum WifiAttempt {
    Idle,
    Connecting {
        ssid: String,
    },
    Failed {
        ssid: String,
        message: String,
        #[serde(rename = "passwordRejected")]
        password_rejected: bool,
    },
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum Wifi {
    NoAdapter,
    Available {
        scanning: bool,
        networks: Vec<WifiNetwork>,
        attempt: WifiAttempt,
    },
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NetworkSnapshot {
    pub connection: NetworkConnection,
    pub wifi: Wifi,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum NetworkStatusOutcome {
    Ok(NetworkSnapshot),
    Err(RpcFailure),
}

#[derive(Debug)]
pub struct NetworkFailure {
    message: &'static str,
    password_rejected: bool,
}
impl NetworkFailure {
    fn unavailable() -> Self {
        Self {
            message: "Network controls are unavailable. Try again.",
            password_rejected: false,
        }
    }
    fn state_reason(reason: u32) -> Self {
        let password_rejected = (7..=11).contains(&reason);
        Self {
            message: if password_rejected {
                "The network did not accept the password. Try again."
            } else {
                "Could not join the network. Check that it is in range and try again."
            },
            password_rejected,
        }
    }
    fn timeout() -> Self {
        Self {
            message: "The network did not answer in time. Try again.",
            password_rejected: false,
        }
    }
}

pub trait NetworkManager: Send + Sync {
    fn read(&self) -> BoxFuture<'_, Result<NetworkSnapshot, NetworkFailure>>;
    fn scan(&self) -> BoxFuture<'_, Result<(), NetworkFailure>>;
    fn connect<'a>(
        &'a self,
        target: &'a str,
        hidden: bool,
        password: Option<&'a str>,
    ) -> BoxFuture<'a, Result<(), NetworkFailure>>;
}

#[derive(Clone)]
pub struct NetworkService {
    manager: Arc<dyn NetworkManager>,
    operation: Arc<Mutex<()>>,
    state: Arc<Mutex<(bool, WifiAttempt)>>,
}
impl Default for NetworkService {
    fn default() -> Self {
        let mut service = Self::new(Arc::new(dbus::DbusNetworkManager));
        service.operation = nearby::radio_operation();
        service
    }
}
impl NetworkService {
    pub fn new(manager: Arc<dyn NetworkManager>) -> Self {
        Self {
            manager,
            operation: Arc::new(Mutex::new(())),
            state: Arc::new(Mutex::new((false, WifiAttempt::Idle))),
        }
    }
    pub async fn status(&self) -> NetworkStatusOutcome {
        match self.manager.read().await {
            Ok(mut snapshot) => {
                let state = self.state.lock().await;
                if let Wifi::Available {
                    scanning, attempt, ..
                } = &mut snapshot.wifi
                {
                    *scanning = state.0;
                    *attempt = state.1.clone();
                }
                NetworkStatusOutcome::Ok(snapshot)
            }
            Err(error) => NetworkStatusOutcome::Err(RpcFailure {
                code: "NetworkUnavailable".into(),
                message: error.message.into(),
            }),
        }
    }
    pub async fn scan(&self) -> NetworkStatusOutcome {
        let Ok(_operation) = self.operation.try_lock() else {
            return self.status().await;
        };
        self.state.lock().await.0 = true;
        let result = self.manager.scan().await;
        self.state.lock().await.0 = false;
        match result {
            Ok(()) => self.status().await,
            Err(error) => NetworkStatusOutcome::Err(RpcFailure {
                code: "WifiScanFailed".into(),
                message: error.message.into(),
            }),
        }
    }
    pub async fn connect(
        &self,
        target: &str,
        hidden: bool,
        password: Option<&str>,
    ) -> NetworkStatusOutcome {
        let Ok(_operation) = self.operation.try_lock() else {
            return self.status().await;
        };
        let snapshot = match self.manager.read().await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return NetworkStatusOutcome::Err(RpcFailure {
                    code: "NetworkUnavailable".into(),
                    message: error.message.into(),
                })
            }
        };
        let Wifi::Available { networks, .. } = &snapshot.wifi else {
            return NetworkStatusOutcome::Err(RpcFailure {
                code: "NoWifiAdapter".into(),
                message: "This device has no Wi-Fi adapter.".into(),
            });
        };
        let ssid = if hidden {
            target.to_owned()
        } else {
            let Some(network) = networks.iter().find(|network| network.id == target) else {
                return NetworkStatusOutcome::Err(RpcFailure {
                    code: "WifiNetworkGone".into(),
                    message: "That network is no longer available. Scan again.".into(),
                });
            };
            network.ssid.clone()
        };
        if ssid.is_empty()
            || ssid.len() > 32
            || password.is_some_and(|password| {
                !(8..=63).contains(&password.len())
                    && !(password.len() == 64
                        && password.bytes().all(|byte| byte.is_ascii_hexdigit()))
            })
        {
            self.state.lock().await.1 = WifiAttempt::Failed { ssid, message: "Use a network name of 1 to 32 bytes and a Wi-Fi password of 8 to 63 characters (or 64 hexadecimal digits).".into(), password_rejected: false };
            return self.status().await;
        }
        self.state.lock().await.1 = WifiAttempt::Connecting { ssid: ssid.clone() };
        let result = self.manager.connect(target, hidden, password).await;
        self.state.lock().await.1 = match result {
            Ok(()) => WifiAttempt::Idle,
            Err(error) => WifiAttempt::Failed {
                ssid,
                message: error.message.into(),
                password_rejected: error.password_rejected,
            },
        };
        self.status().await
    }
}
