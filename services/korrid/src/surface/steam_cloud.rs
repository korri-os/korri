//! A launch on the Steam engine runner that waits for the person (ADR 0083,
//! owner decision 2026-11-21): at the launch sync, Steam reported that
//! another device of the account has a save that has not finished uploading.
//! korrid holds the launch at its "Syncing saves" step and asks, as Valve's
//! client does: Play anyway or Retry. Back runs the launch's own Cancel,
//! which ends the launch without playing.
//!
//! The question is the treaty's `Confirm`. The shape of the Steam Cloud
//! conflict pick (`ChooseSave`) does not fit: it offers two saves to keep,
//! and here there is one save that Korri does not have yet.
use std::sync::{Arc, Mutex};

use korri_surface_treaty::{SurfaceAction, SurfaceStatus};

use super::{korrid::Korrid, steam_account::SteamEngineAccess, Notify};
use crate::steam_engine::{protocol::Answer, sessions::CloudQuestion};

/// The launch's own Cancel (`model::status_from`, `Phase::Starting`).
pub const CANCEL_LAUNCH: &str = "cancel-launch";
const PREFIX: &str = "steam-cloud:";
/// The owner's words (2026-11-21).
const KICKER: &str = "Your save on another device hasn't finished uploading";

#[derive(Default)]
struct Inner {
    /// The question korrid's sessions hold now.
    open: Option<CloudQuestion>,
    /// Answered or cancelled here; korrid's session may hold it a moment longer.
    settled: Option<CloudQuestion>,
    /// A new question gets new action ids, so an old press answers nothing.
    generation: u64,
}

pub struct SteamCloudQuestion {
    access: Option<Arc<dyn SteamEngineAccess>>,
    inner: Mutex<Inner>,
    notify: Notify,
}

impl SteamCloudQuestion {
    pub fn new(korrid: &Korrid, notify: Notify) -> Arc<Self> {
        Arc::new(Self {
            access: korrid.steam_engine.clone(),
            inner: Mutex::new(Inner::default()),
            notify,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("steam cloud question poisoned")
    }

    /// Read korrid's sessions (cheap, no disk); a change redraws.
    pub fn poll(&self) {
        let Some(access) = &self.access else { return };
        let now = access.cloud_question();
        let changed = {
            let mut inner = self.lock();
            if inner.open == now {
                false
            } else {
                if now.is_some() {
                    inner.generation += 1;
                }
                if inner.settled != now {
                    inner.settled = None;
                }
                inner.open = now;
                true
            }
        };
        if changed {
            (self.notify)();
        }
    }

    fn shown(&self) -> Option<(CloudQuestion, u64)> {
        let inner = self.lock();
        inner
            .open
            .clone()
            .filter(|open| inner.settled.as_ref() != Some(open))
            .map(|open| (open, inner.generation))
    }

    fn action(generation: u64, answer: Answer, label: &str) -> SurfaceAction {
        let word = match answer {
            Answer::PlayAnyway => "play-anyway",
            Answer::Retry => "retry",
        };
        SurfaceAction::new(format!("{PREFIX}{generation}:{word}"), label, true)
    }

    /// The launch's Busy, while a question is open, becomes the question. The
    /// launch keeps its steps (`Root::with_launch_steps` saw the Busy) and
    /// comes back to them after the answer. A launch that is already
    /// cancelling shows as it is.
    pub fn over(&self, status: SurfaceStatus) -> SurfaceStatus {
        let SurfaceStatus::Busy {
            actions, game_id, ..
        } = &status
        else {
            return status;
        };
        let cancellable = actions
            .iter()
            .flatten()
            .any(|action| action.id == CANCEL_LAUNCH && action.enabled);
        let Some((question, generation)) = self.shown().filter(|_| cancellable) else {
            return status;
        };
        SurfaceStatus::Confirm {
            kicker: KICKER.into(),
            detail: Some(detail(&question)),
            actions: vec![
                Self::action(generation, Answer::PlayAnyway, "Play anyway"),
                Self::action(generation, Answer::Retry, "Retry"),
            ],
            decline_action_id: Some(CANCEL_LAUNCH.into()),
            game_id: game_id.clone(),
        }
    }

    /// One of this module's actions: answer the host. True when the id was
    /// one of ours, also when it was an old one that answers nothing.
    pub fn act(self: &Arc<Self>, action_id: &str) -> bool {
        let Some(rest) = action_id.strip_prefix(PREFIX) else {
            return false;
        };
        let answer = match rest.split_once(':') {
            Some((generation, "play-anyway")) => (generation, Answer::PlayAnyway),
            Some((generation, "retry")) => (generation, Answer::Retry),
            _ => return true,
        };
        let Some((question, generation)) = self.shown() else {
            return true;
        };
        if answer.0 != generation.to_string() {
            return true;
        }
        let Some(access) = self.access.clone() else {
            return true;
        };
        self.lock().settled = Some(question.clone());
        (self.notify)();
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            if let Err(error) = access.answer_cloud(&question.launch_id, question.appid, answer.1) {
                eprintln!("korrid surface: Steam Cloud answer not sent: {error}");
                // Ask again rather than leave the launch waiting unseen.
                let mut inner = this.lock();
                if inner.settled.as_ref() == Some(&question) {
                    inner.settled = None;
                }
                drop(inner);
                (this.notify)();
            }
        });
        true
    }

    /// The launch's Cancel ran: the question is not shown again while the
    /// launch ends.
    pub fn cancelled(&self) {
        let changed = {
            let mut inner = self.lock();
            let open = inner.open.clone();
            let changed = open.is_some() && inner.settled != open;
            inner.settled = open;
            changed
        };
        if changed {
            (self.notify)();
        }
    }
}

/// What Steam said, in words: whose save, or where the game is still open.
fn detail(question: &CloudQuestion) -> String {
    let machine = question
        .operations
        .iter()
        .find_map(|operation| operation.machine.as_deref());
    let open = !question.operations.is_empty()
        && question.operations.iter().all(|operation| {
            matches!(
                operation.operation.as_str(),
                "app_session_active" | "app_session_suspended"
            )
        });
    let what = match (open, machine) {
        (true, Some(machine)) => format!("The game is still open on {machine}."),
        (true, None) => "The game is still open on another device.".into(),
        (false, Some(machine)) => format!("Steam is still waiting for the save from {machine}."),
        (false, None) => "Steam is still waiting for the save from another device.".into(),
    };
    format!("{what} Play anyway starts with the save in Steam Cloud now.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam_engine::protocol::PendingOperation;

    fn question(operations: &[(&str, Option<&str>)]) -> CloudQuestion {
        CloudQuestion {
            launch_id: "0123456789abcdef0123456789abcdef".into(),
            serial: 1,
            appid: 584400,
            operations: operations
                .iter()
                .map(|(operation, machine)| PendingOperation {
                    operation: (*operation).into(),
                    machine: machine.map(Into::into),
                })
                .collect(),
        }
    }

    #[test]
    fn the_words_say_whose_save_or_where_the_game_is_open() {
        assert_eq!(
            detail(&question(&[("upload_pending", Some("Living room"))])),
            "Steam is still waiting for the save from Living room. Play anyway starts with the save in Steam Cloud now."
        );
        assert_eq!(
            detail(&question(&[("unknown", None)])),
            "Steam is still waiting for the save from another device. Play anyway starts with the save in Steam Cloud now."
        );
        assert_eq!(
            detail(&question(&[("app_session_active", Some("zao"))])),
            "The game is still open on zao. Play anyway starts with the save in Steam Cloud now."
        );
        assert_eq!(
            detail(&question(&[
                ("app_session_active", None),
                ("upload_pending", None)
            ])),
            "Steam is still waiting for the save from another device. Play anyway starts with the save in Steam Cloud now."
        );
    }

    #[test]
    fn only_a_launch_that_can_still_be_cancelled_is_asked_over() {
        let busy = |enabled: bool| SurfaceStatus::Busy {
            kicker: "Starting Sonic Mania…".into(),
            detail: None,
            progress: None,
            steps: None,
            actions: Some(vec![SurfaceAction::new(CANCEL_LAUNCH, "Cancel", enabled)]),
            decline_action_id: Some(CANCEL_LAUNCH.into()),
            game_id: Some("game:sonic".into()),
        };
        let korrid = Korrid::new(Arc::new(|_| {
            Box::pin(async { Err(super::super::korrid::Refusal::Unreachable("test".into())) })
        }));
        let asks = SteamCloudQuestion::new(&korrid, Arc::new(|| {}));
        assert_eq!(asks.over(busy(true)), busy(true), "no question");
        {
            let mut inner = asks.lock();
            inner.open = Some(question(&[("upload_pending", None)]));
            inner.generation = 3;
        }
        assert!(matches!(
            asks.over(busy(true)),
            SurfaceStatus::Confirm { .. }
        ));
        assert_eq!(asks.over(busy(false)), busy(false), "cancelling");
        let browsing = SurfaceStatus::Browsing;
        assert_eq!(asks.over(browsing.clone()), browsing);
        asks.cancelled();
        assert_eq!(asks.over(busy(true)), busy(true), "cancelled here");
    }
}
