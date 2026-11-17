//! Joining a new device to its person's devices (ADRs 0069 and 0070).
//!
//! An owned device shows a join confirmation code. The person enters it on
//! the new device. The two devices meet on public Nostr relays at the
//! code's nameplate and run CPace with the code's secret. When both
//! confirmations match, the owned device asks the person for one Accept,
//! then signs a vouch for the new device. The new device stores the vouch
//! chain as its owner statement and becomes owned.
//!
//! A device that already has an owner joins only after the person enters
//! the recovery words of its person key (ADR 0082). That key signs the
//! removal of the device's old owner statement, and the join becomes an
//! identity switch to a new device key that keeps the device's data (ADR
//! 0023).
//!
//! One join runs on a device at a time. The coordinator holds its status;
//! the session tasks in `session.rs` do the work.

mod code;
mod cpace;
mod rendezvous;
mod session;
#[cfg(test)]
mod tests;

use crate::{
    federation::FederationDirectory,
    identity::IdentityState,
    identity_switch::IdentitySwitchCoordinator,
    network::NetworkConnection,
    peer_rpc::PeerCredentials,
    relay::{RelayList, RelayTransport},
    RpcFailure,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{oneshot, watch};
use typeshare::typeshare;
use zeroize::Zeroize;

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JoinStartRequest {}

#[typeshare]
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinEnterRequest {
    /// The 10 digits of the join confirmation code.
    pub code: String,
    /// Only on a device that has an owner: the 12 recovery words of its
    /// person key, which signs the device's removal (ADR 0082).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_words: Option<String>,
    /// Only on a device that has an owner: the person accepted that the
    /// join resets Korri's own device trust (ADR 0023).
    #[serde(default)]
    pub trust_loss_confirmed: bool,
}
impl std::fmt::Debug for JoinEnterRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JoinEnterRequest")
            .finish_non_exhaustive()
    }
}
impl Drop for JoinEnterRequest {
    fn drop(&mut self) {
        self.code.zeroize();
        if let Some(words) = &mut self.current_words {
            words.zeroize();
        }
    }
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JoinStatusRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JoinAcceptRequest {
    /// True accepts the joining device; false refuses it.
    pub accept: bool,
}

/// What the owned device shows the person before Accept (ADR 0070). The
/// joining device supplies these facts, so they can be false. Only the code
/// proves which device joins.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoiningDevice {
    /// The device's label, from the producer that endpoint publication uses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The device's operating system and processor architecture.
    pub kind: String,
    /// The device's network connection, as NetworkManager reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<NetworkConnection>,
}

impl JoiningDevice {
    const MAX_TEXT_CHARS: usize = 64;

    /// Facts from another device, bounded before anyone shows them.
    fn checked(self) -> Option<Self> {
        let text = |value: &str| {
            !value.trim().is_empty()
                && value.chars().count() <= Self::MAX_TEXT_CHARS
                && !value.chars().any(char::is_control)
        };
        let connection_ok = match &self.connection {
            Some(NetworkConnection::Wifi { ssid, .. }) => text(ssid),
            Some(NetworkConnection::Wired | NetworkConnection::Offline) | None => true,
        };
        (self.name.as_deref().is_none_or(text) && text(&self.kind) && connection_ok).then_some(self)
    }
}

#[typeshare]
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinCodeShown {
    /// The nameplate, a space, then the secret.
    pub code: String,
    #[typeshare(serialized_as = "number")]
    pub expires_at: u64,
    /// The code before this one ended because a wrong code was tried.
    pub wrong_code_tried: bool,
}

/// Logs and Debug output show the nameplate, never the secret. JoinStatus
/// and every response that carries it print through this.
impl std::fmt::Debug for JoinCodeShown {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let nameplate: String = self.code.chars().take(3).collect();
        formatter
            .debug_struct("JoinCodeShown")
            .field("nameplate", &nameplate)
            .field("expires_at", &self.expires_at)
            .field("wrong_code_tried", &self.wrong_code_tried)
            .finish_non_exhaustive()
    }
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinedDevice {
    pub device_public_key: String,
    pub owner_public_key: String,
}

/// The one join on this device. The owned device moves through
/// `ShowingCode` and `AwaitingAccept`; the new device through `Matching`
/// and `WaitingForAccept`. Both end in `Joined`, `Refused` or `Failed`.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum JoinStatus {
    Idle,
    /// The owned device is choosing a nameplate.
    Starting,
    ShowingCode(JoinCodeShown),
    /// The new device is checking the code with the owned device.
    Matching,
    /// The code matched. The person must accept or refuse this device.
    AwaitingAccept(JoiningDevice),
    /// The code matched. The person must accept on the owned device.
    WaitingForAccept,
    Joined(JoinedDevice),
    Refused,
    Failed(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum JoinOutcome {
    Ok(JoinStatus),
    Err(RpcFailure),
}

/// The join's time limits.
#[derive(Clone, Debug)]
pub struct JoinTiming {
    /// How long one code is valid (ADR 0070: 5 minutes).
    pub code_lifetime: u64,
    /// How long a matched join waits for the person's Accept.
    pub accept_wait: u64,
    /// How long the owned device waits for an answer after a device said
    /// hello. Then the hello counts as the code's one guess.
    pub answer_wait: u64,
    /// An owned device's join ends when no one has asked for its status for
    /// this long, so a closed terminal does not keep showing codes.
    pub watch: u64,
    /// How often a device reads the relays while it waits.
    pub poll: Duration,
}

impl Default for JoinTiming {
    fn default() -> Self {
        Self {
            code_lifetime: 300,
            accept_wait: 300,
            answer_wait: 30,
            watch: 60,
            poll: Duration::from_millis(1500),
        }
    }
}

/// Everything a join needs from its device.
pub struct JoinContext {
    pub credentials: PeerCredentials,
    /// The owned device adds a device it vouched for at once.
    pub directory: Option<FederationDirectory>,
    pub private_state_root: PathBuf,
    pub relays: Option<RelayList>,
    pub transport: Arc<dyn RelayTransport>,
    pub clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    pub timing: JoinTiming,
    /// Runs after the new device stored its vouch and its status was read,
    /// so the device restarts with its owner.
    pub joined: Arc<dyn Fn() + Send + Sync>,
    /// Moves an owned device to the person key it joins (ADR 0023). A
    /// device without it joins only when it has no owner.
    pub switch: Option<Arc<IdentitySwitchCoordinator>>,
}

#[derive(Clone)]
pub struct JoinCoordinator(Arc<Shared>);

struct Shared {
    context: JoinContext,
    status: watch::Sender<JoinStatus>,
    running: Mutex<Running>,
    /// When the status was last asked for.
    watched: AtomicU64,
    /// How often the status was asked for.
    reads: AtomicU64,
}

#[derive(Default)]
struct Running {
    generation: u64,
    task: Option<tokio::task::JoinHandle<()>>,
    decision: Option<oneshot::Sender<bool>>,
}

/// A session's only way to change the status. A session that a newer join
/// replaced changes nothing.
#[derive(Clone)]
struct Reporter {
    shared: Arc<Shared>,
    generation: u64,
}

impl Reporter {
    fn set(&self, status: JoinStatus) {
        let running = self.shared.running.lock().expect("join lock poisoned");
        if running.generation == self.generation {
            self.shared.status.send_replace(status);
        }
    }

    fn unwatched(&self, now: u64) -> bool {
        now.saturating_sub(self.shared.watched.load(Ordering::Relaxed))
            > self.shared.context.timing.watch
    }
}

// Both stay below the CLI's ordinary 30-second request deadline. A caller
// that sees the old status still follows the join with its status.
const START_WAIT: Duration = Duration::from_secs(20);
const DECISION_WAIT: Duration = Duration::from_secs(20);
/// How long a joined device waits for its status to be read before it
/// restarts with its owner.
const RESTART_WAIT: Duration = Duration::from_secs(10);

impl JoinCoordinator {
    pub fn new(context: JoinContext) -> Self {
        Self(Arc::new(Shared {
            context,
            status: watch::channel(JoinStatus::Idle).0,
            running: Mutex::new(Running::default()),
            watched: AtomicU64::new(0),
            reads: AtomicU64::new(0),
        }))
    }

    pub fn status(&self) -> JoinStatus {
        self.touch();
        self.0.reads.fetch_add(1, Ordering::SeqCst);
        self.0.status.borrow().clone()
    }

    /// Whether this device is joining another device right now. Setup must
    /// not end then (ADR 0022): it would bind an automatic owner, and the
    /// join's vouch could no longer bind the device.
    pub fn joining(&self) -> bool {
        matches!(
            *self.0.status.borrow(),
            JoinStatus::Matching | JoinStatus::WaitingForAccept
        )
    }

    /// Start showing codes on this owned device. Returns the first code.
    pub async fn start(&self) -> Result<JoinStatus, RpcFailure> {
        let identity = self.identity()?;
        let IdentityState::Owned { .. } = identity.state() else {
            return Err(failure(
                "JoinNeedsOwner",
                "Only a device that has an owner can show a join code.",
            ));
        };
        let rendezvous = self.rendezvous(identity)?;
        let (reporter, decision) = self.begin(JoinStatus::Starting);
        let generation = reporter.generation;
        let shared = Arc::clone(&self.0);
        let task = tokio::spawn(async move {
            let status = session::owned(&shared.context, &rendezvous, &reporter, decision).await;
            reporter.set(status);
        });
        self.attach(generation, task);
        self.wait_while(|status| matches!(status, JoinStatus::Starting), START_WAIT)
            .await;
        Ok(self.status())
    }

    /// Enter a code on this device and join the device that shows it. A
    /// device without an owner joins as it is. A device with an owner needs
    /// its person key's recovery words and the person's acceptance that its
    /// device trust resets; it then joins with a new device key.
    pub async fn enter(
        &self,
        code: &str,
        current_words: Option<&str>,
        trust_loss_confirmed: bool,
        device: JoiningDevice,
    ) -> Result<JoinStatus, RpcFailure> {
        let code = code::JoinCode::parse(code)
            .map_err(|error| failure("JoinCodeInvalid", &error.to_string()))?;
        let device = device
            .checked()
            .ok_or_else(|| failure("JoinUnavailable", "This device cannot describe itself."))?;
        let identity = self.identity()?;
        let (identity, prepared) = match (identity.state(), current_words) {
            (IdentityState::Unowned { .. }, None) => (identity, None),
            (IdentityState::Unowned { .. }, Some(_)) => {
                return Err(failure(
                    "JoinWordsNotNeeded",
                    "This device has no owner. It joins without recovery words.",
                ))
            }
            (IdentityState::Owned { .. }, None) => {
                return Err(failure(
                    "JoinNeedsWords",
                    "This device has its own person key. Enter that key's 12 recovery words to join another person's devices.",
                ))
            }
            (IdentityState::Owned { .. }, Some(_)) if !trust_loss_confirmed => {
                return Err(failure(
                    "JoinNeedsConfirmation",
                    "Joining resets Korri's own device trust on this device. Accept that to join.",
                ))
            }
            (IdentityState::Owned { .. }, Some(words)) => {
                let switch = self.0.context.switch.as_ref().ok_or_else(|| {
                    failure(
                        "JoinUnavailable",
                        "This device cannot change its person key.",
                    )
                })?;
                let prepared = switch
                    .begin_join(words)
                    .await
                    .map_err(|message| failure("JoinWordsRefused", &message))?;
                (prepared.staged_identity().clone(), Some(prepared))
            }
            _ => {
                return Err(failure(
                    "JoinNeedsNewDevice",
                    "This device's identity cannot join another device.",
                ))
            }
        };
        let rendezvous = self.rendezvous(identity)?;
        let (reporter, _) = self.begin(JoinStatus::Matching);
        let generation = reporter.generation;
        let shared = Arc::clone(&self.0);
        let task = tokio::spawn(async move {
            let status =
                match session::joining(&shared.context, &rendezvous, &reporter, code, device).await
                {
                    Err(status) => status,
                    Ok(vouched) => match prepared {
                        None => session::store_vouch(&shared.context, &vouched).await,
                        Some(prepared) => {
                            session::switch_to_vouch(&shared.context, prepared, &vouched).await
                        }
                    },
                };
            let joined = matches!(status, JoinStatus::Joined(_));
            reporter.set(status);
            let reads = shared.reads.load(Ordering::SeqCst);
            if joined {
                // The device restarts with its owner. Let whoever follows
                // the join see that it joined first.
                let _ = tokio::time::timeout(RESTART_WAIT, async {
                    while shared.reads.load(Ordering::SeqCst) == reads {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                })
                .await;
                (shared.context.joined)();
            }
        });
        self.attach(generation, task);
        Ok(self.status())
    }

    /// The person's answer on the owned device: accept or refuse.
    pub async fn decide(&self, accept: bool) -> Result<JoinStatus, RpcFailure> {
        let decision = {
            let mut running = self.0.running.lock().expect("join lock poisoned");
            if !matches!(*self.0.status.borrow(), JoinStatus::AwaitingAccept(_)) {
                return Err(failure(
                    "JoinNotAwaitingAccept",
                    "No joining device is waiting for an answer.",
                ));
            }
            running.decision.take()
        };
        let sent = decision.is_some_and(|sender| sender.send(accept).is_ok());
        if !sent {
            return Err(failure(
                "JoinNotAwaitingAccept",
                "No joining device is waiting for an answer.",
            ));
        }
        self.wait_while(
            |status| matches!(status, JoinStatus::AwaitingAccept(_)),
            DECISION_WAIT,
        )
        .await;
        Ok(self.status())
    }

    fn touch(&self) {
        self.0
            .watched
            .store((self.0.context.clock)(), Ordering::Relaxed);
    }

    fn identity(&self) -> Result<crate::identity::DeviceIdentity, RpcFailure> {
        self.0
            .context
            .credentials
            .identity_snapshot()
            .map_err(|_| failure("IdentityUnavailable", "Device identity is unavailable."))
    }

    fn rendezvous(
        &self,
        identity: crate::identity::DeviceIdentity,
    ) -> Result<rendezvous::Rendezvous, RpcFailure> {
        let relays = self.0.context.relays.clone().ok_or_else(|| {
            failure(
                "JoinNeedsRelays",
                "This device has no relays configured, so it cannot join.",
            )
        })?;
        Ok(rendezvous::Rendezvous::new(
            relays,
            Arc::clone(&self.0.context.transport),
            identity,
        ))
    }

    /// End any running join and begin a new one with this status.
    fn begin(&self, status: JoinStatus) -> (Reporter, oneshot::Receiver<bool>) {
        self.touch();
        let (sender, receiver) = oneshot::channel();
        let mut running = self.0.running.lock().expect("join lock poisoned");
        if let Some(task) = running.task.take() {
            task.abort();
        }
        running.generation += 1;
        running.decision = Some(sender);
        self.0.status.send_replace(status);
        (
            Reporter {
                shared: Arc::clone(&self.0),
                generation: running.generation,
            },
            receiver,
        )
    }

    fn attach(&self, generation: u64, task: tokio::task::JoinHandle<()>) {
        let mut running = self.0.running.lock().expect("join lock poisoned");
        if running.generation == generation {
            running.task = Some(task);
        } else {
            task.abort();
        }
    }

    async fn wait_while(&self, condition: impl Fn(&JoinStatus) -> bool, limit: Duration) {
        let mut receiver = self.0.status.subscribe();
        let _ = tokio::time::timeout(limit, receiver.wait_for(|status| !condition(status))).await;
    }
}

fn failure(code: &str, message: &str) -> RpcFailure {
    RpcFailure {
        code: code.into(),
        message: message.into(),
    }
}
