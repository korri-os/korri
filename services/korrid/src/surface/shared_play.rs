//! Port of the pure parts of `clients/portal/src/surface/shared-play.ts`:
//! Paused and Ended of shared play this device takes part in, and the
//! invites another device of the same person offers.

use korri_surface_treaty::{SurfaceAction, SurfaceInvite, SurfaceProgress, SurfaceStatus};

use super::launchables::state::{Entry, LaunchablesState, Loaded, Phase};
use crate::{
    invites::Invite,
    netplay::{InviteNetplay, NetplayPhase, NetplayStatus},
    stream_play::{StreamPlayEndCause, StreamPlayEnded},
};

pub const STREAM: &str = "invite:stream:";
pub const NETPLAY: &str = "invite:netplay:";
pub const DISMISS: &str = "invite:dismiss:";
pub const LEAVE: &str = "stream-play:leave:";

/// The presented entry that holds a copy of `game_id`. Cover art was never
/// published for these (the portal's production client had none).
fn game_entry_id(state: &LaunchablesState, game_id: Option<&str>) -> Option<String> {
    let game_id = game_id?;
    state.entries().iter().find_map(|entry| {
        let own = match entry {
            Entry::NowPlaying { .. } => return None,
            Entry::LocalGame { game, .. } => game.id.as_str(),
            Entry::Game { game, .. } => game.id.as_str(),
        };
        (own == game_id || entry.alternatives().iter().any(|copy| copy.id() == game_id))
            .then(|| entry.key())
    })
}

pub fn stream_end_reason(ended: &StreamPlayEnded) -> String {
    match ended.cause {
        StreamPlayEndCause::DeviceUnreachable => format!("Lost {}", ended.device_label),
        StreamPlayEndCause::SessionEnded => format!(
            "{} ended on {}",
            ended.game_title.as_deref().unwrap_or("The session"),
            ended.device_label
        ),
    }
}

/// Paused and Ended of shared play this device takes part in. Ended covers
/// both kinds: korrid reports a netplay session that ended as it reports a
/// stream that ended, so the words name neither.
pub fn stream_status_from(state: &LaunchablesState) -> Option<SurfaceStatus> {
    let LaunchablesState::Loaded(Loaded {
        phase: Phase::Ready,
        content,
    }) = state
    else {
        return None;
    };
    let active = content.entries.iter().find_map(|entry| {
        entry
            .session()
            .filter(|session| Some(&session.launch_id) == content.stream_play_launch_id.as_ref())
    });
    if let Some(session) = active.filter(|session| session.phase.as_deref() == Some("frozen")) {
        return Some(SurfaceStatus::Paused {
            kicker: format!(
                "Paused on {}",
                session.host.as_deref().unwrap_or("the other device")
            ),
            detail: None,
            actions: vec![SurfaceAction::new(
                format!("{LEAVE}{}", session.launch_id),
                "Leave",
                true,
            )],
            game_id: game_entry_id(state, session.game_id.as_deref()),
        });
    }
    let ended = content.stream_play_ended.as_ref()?;
    Some(SurfaceStatus::Ended {
        kicker: "Session ended".into(),
        reason: stream_end_reason(ended),
        game_id: game_entry_id(state, ended.game_id.as_deref()),
        game_title: ended.game_title.clone(),
    })
}

/// "4.7 GB", "512 MB": a size as a person reads it.
pub fn format_bytes(bytes: f64) -> String {
    let scale = |value: f64, unit: &str| {
        let text = if value < 10.0 {
            format!("{value:.1}")
        } else {
            format!("{}", value.round())
        };
        let text = text.strip_suffix(".0").unwrap_or(&text).to_owned();
        format!("{text} {unit}")
    };
    if bytes >= 1e9 {
        scale(bytes / 1e9, "GB")
    } else if bytes >= 1e6 {
        scale(bytes / 1e6, "MB")
    } else if bytes >= 1e3 {
        scale(bytes / 1e3, "KB")
    } else {
        format!("{} B", bytes.round())
    }
}

/// The words of the netplay route korrid offers for an invite, or nothing.
pub fn netplay_label(netplay: Option<&InviteNetplay>) -> Option<String> {
    match netplay? {
        InviteNetplay::Ready => Some("Play here".into()),
        InviteNetplay::NeedsCopy { bytes } => {
            Some(format!("Copy and play here ({})", format_bytes(*bytes)))
        }
        InviteNetplay::NeedsPlugin { title, .. } => Some(format!("Install {title} and play here")),
        InviteNetplay::Unavailable { .. } => None,
    }
}

pub fn surface_invites_from(invites: &[Invite], _state: &LaunchablesState) -> Vec<SurfaceInvite> {
    invites
        .iter()
        .map(|invite| {
            let netplay = netplay_label(invite.netplay.as_ref());
            let mut actions = vec![SurfaceAction::new(
                format!("{STREAM}{}", invite.invite_id),
                "Play on stream",
                true,
            )];
            if let Some(label) = netplay {
                actions.push(SurfaceAction::new(
                    format!("{NETPLAY}{}", invite.invite_id),
                    label,
                    true,
                ));
            }
            actions.push(SurfaceAction::new(
                format!("{DISMISS}{}", invite.invite_id),
                "Dismiss",
                true,
            ));
            SurfaceInvite {
                id: invite.invite_id.clone(),
                person_name: invite.person_name.clone(),
                device_label: invite.device_label.clone(),
                game_title: invite
                    .session
                    .title
                    .clone()
                    .or_else(|| invite.session.game_id.clone())
                    .unwrap_or_else(|| "Game".into()),
                cover_art_url: None,
                actions,
            }
        })
        .collect()
}

// ---------------------------------------------------------------- netplay

const CANCEL: &str = "stream-play:cancel";
const NETPLAY_CANCEL: &str = "netplay:cancel";
const NETPLAY_INSTALL: &str = "netplay:install";
const NETPLAY_LEAVE: &str = "netplay:leave:";

/// `RpcFailure.code` of `app.invites.netplay`, and `InviteNetplay.Unavailable.reason`.
pub mod netplay_failure {
    pub const NO_NETPLAY_ROUTE: &str = "NoNetplayRoute";
    pub const COPY_FAILED: &str = "CopyFailed";
    pub const NOT_ENOUGH_SPACE: &str = "NotEnoughSpace";
    pub const CORE_VERSION_MISMATCH: &str = "CoreVersionMismatch";
}

/// The bytes a NotEnoughSpace message names ("The copy needs 4700000000 bytes.").
pub fn needed_bytes(message: Option<&str>) -> Option<f64> {
    let text = message?.trim();
    let digits = text
        .strip_prefix("The copy needs ")?
        .strip_suffix(" bytes.")?;
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

/// "42% of 512 MB": how far the copy is.
pub fn copy_detail(done: u64, total: u64) -> String {
    let percent = done
        .saturating_mul(100)
        .checked_div(total)
        .map_or(0, |percent| percent.min(100));
    format!("{percent}% of {}", format_bytes(total as f64))
}

fn netplay_cancel() -> SurfaceAction {
    SurfaceAction::new(NETPLAY_CANCEL, "Cancel", true)
}

/// What this device's netplay shows while korrid reports it. Playing shows
/// nothing: the game has the screen.
pub fn netplay_status_from(
    observation: &NetplayStatus,
    game_id: Option<String>,
) -> Option<SurfaceStatus> {
    let device = &observation.device_label;
    let busy = |kicker: String,
                detail: Option<String>,
                progress: Option<SurfaceProgress>,
                actions: Option<Vec<SurfaceAction>>| {
        SurfaceStatus::Busy {
            kicker,
            detail,
            progress,
            actions,
            game_id: game_id.clone(),
        }
    };
    Some(match &observation.phase {
        NetplayPhase::Copying { done, total } => busy(
            format!(
                "Copying {} from {device}",
                observation.game_title.as_deref().unwrap_or("the game")
            ),
            Some(copy_detail(*done, *total)),
            Some(SurfaceProgress {
                done: *done as f64,
                total: Some(*total as f64),
            }),
            Some(vec![netplay_cancel()]),
        ),
        NetplayPhase::Connecting => busy(
            format!("Connecting to {device}"),
            None,
            None,
            Some(vec![netplay_cancel()]),
        ),
        NetplayPhase::Reconnecting => busy(format!("Reconnecting to {device}"), None, None, None),
        NetplayPhase::HolderLeft => SurfaceStatus::Paused {
            kicker: format!("Paused on {device}"),
            detail: None,
            actions: vec![SurfaceAction::new(
                format!("{NETPLAY_LEAVE}{}", observation.launch_id),
                "Leave",
                true,
            )],
            game_id: game_id.clone(),
        },
        NetplayPhase::Playing => return None,
    })
}

/// Who and what a netplay problem is about, for its words.
#[derive(Clone, Debug)]
pub struct NetplayProblemFacts {
    pub game_title: String,
    pub device_label: String,
    pub core: Option<String>,
}

const STREAM_STILL_WORKS: &str = "Play on stream still works.";

/// The words for a netplay failure code; None for a code Korri has no words for.
pub fn netplay_reason(
    code: &str,
    facts: &NetplayProblemFacts,
    message: Option<&str>,
) -> Option<String> {
    use netplay_failure::*;
    Some(match code {
        NO_NETPLAY_ROUTE => format!(
            "{} can't play here with {} right now. {STREAM_STILL_WORKS}",
            facts.game_title, facts.device_label
        ),
        COPY_FAILED => format!(
            "{} did not copy from {}. Nothing was kept.",
            facts.game_title, facts.device_label
        ),
        NOT_ENOUGH_SPACE => match needed_bytes(message) {
            None => format!("{} needs more free space on this device.", facts.game_title),
            Some(bytes) => format!(
                "{} needs {} free on this device.",
                facts.game_title,
                format_bytes(bytes)
            ),
        },
        CORE_VERSION_MISMATCH => format!(
            "{} runs a different version of {}. {STREAM_STILL_WORKS}",
            facts.device_label,
            facts.core.as_deref().unwrap_or("the game's core")
        ),
        _ => return None,
    })
}

pub fn netplay_problem(failure: &crate::RpcFailure, facts: &NetplayProblemFacts) -> SurfaceStatus {
    SurfaceStatus::Problem {
        kicker: match failure.code.as_str() {
            netplay_failure::COPY_FAILED => "Copy failed",
            netplay_failure::NOT_ENOUGH_SPACE => "Not enough space",
            _ => "Netplay unavailable",
        }
        .into(),
        reason: netplay_reason(&failure.code, facts, Some(&failure.message))
            .unwrap_or_else(|| failure.message.clone()),
        can_retry: false,
        game_id: None,
        game_title: None,
    }
}

/// What a netplay start needs to show itself before korrid reports it.
#[derive(Clone, Debug)]
pub struct NetplayStart {
    pub invite_id: String,
    pub device_label: String,
    pub game_title: Option<String>,
    pub game_id: Option<String>,
    pub copy_bytes: Option<u64>,
    pub core: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InstallStep {
    Finding,
    Checking,
    Installing,
}

impl InstallStep {
    fn word(self) -> &'static str {
        match self {
            Self::Finding => "Finding",
            Self::Checking => "Checking",
            Self::Installing => "Installing",
        }
    }
}

/// Where shared play stands on this device: one case at a time.
// One value per device, replaced whole on each step: variant size is moot.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum SharedPlay {
    Unread {
        game_id: Option<String>,
    },
    Idle,
    Observing {
        netplay: NetplayStatus,
        game_id: Option<String>,
    },
    StreamStarting {
        op: u64,
        device_label: String,
    },
    NetplayStarting {
        op: u64,
        start: NetplayStart,
        netplay: Option<NetplayStatus>,
    },
    Installing {
        op: u64,
        invite: Invite,
        title: String,
        step: InstallStep,
    },
    Reviewing {
        op: u64,
        invite: Invite,
        title: String,
        target: super::plugins::ReleaseTarget,
        facts: Box<super::plugins::PluginFacts>,
        question: SurfaceStatus,
    },
    CancellingStream {
        pending: u64,
    },
    CancellingNetplay {
        pending: Option<u64>,
        launch_id: Option<String>,
        sent: bool,
    },
    Leaving {
        op: u64,
        kicker: String,
    },
    Problem(SurfaceStatus),
}

#[derive(Clone, Debug)]
pub enum SharedPlayEvent {
    Read(Option<NetplayStatus>),
    StreamRequested {
        op: u64,
        device_label: String,
    },
    NetplayRequested {
        op: u64,
        start: NetplayStart,
    },
    InstallRequested {
        op: u64,
        invite: Invite,
        title: String,
    },
    InstallStepped {
        op: u64,
        step: InstallStep,
    },
    ReviewReady {
        op: u64,
        target: super::plugins::ReleaseTarget,
        facts: Box<super::plugins::PluginFacts>,
        question: SurfaceStatus,
    },
    Approved,
    CancelPressed,
    CancelSent,
    LeaveRequested {
        op: u64,
        kicker: String,
    },
    Settled(Option<String>),
    Failed(SurfaceStatus),
    Dismissed,
}

fn cancellable(netplay: &NetplayStatus) -> bool {
    matches!(
        netplay.phase,
        NetplayPhase::Copying { .. } | NetplayPhase::Connecting
    )
}

/// The cases with work of the host's own under way.
pub fn shared_play_busy(model: &SharedPlay) -> bool {
    matches!(
        model,
        SharedPlay::StreamStarting { .. }
            | SharedPlay::NetplayStarting { .. }
            | SharedPlay::Installing { .. }
            | SharedPlay::Reviewing { .. }
            | SharedPlay::CancellingStream { .. }
            | SharedPlay::CancellingNetplay { .. }
            | SharedPlay::Leaving { .. }
    )
}

/// Every transition. Pure: effects follow from the case it returns.
pub fn shared_play_step(model: SharedPlay, event: SharedPlayEvent) -> SharedPlay {
    use SharedPlay as M;
    match event {
        SharedPlayEvent::Read(netplay) => match model {
            M::Unread { .. } | M::Idle | M::Observing { .. } => match netplay {
                None => M::Idle,
                Some(netplay) => {
                    let game_id = match &model {
                        M::Unread { game_id } => game_id.clone(),
                        M::Observing {
                            netplay: previous,
                            game_id,
                        } if previous.launch_id == netplay.launch_id => game_id.clone(),
                        _ => None,
                    };
                    M::Observing { netplay, game_id }
                }
            },
            M::NetplayStarting { op, start, .. } => M::NetplayStarting { op, start, netplay },
            M::CancellingNetplay {
                pending,
                launch_id: None,
                sent,
            } if netplay.is_some() => M::CancellingNetplay {
                pending,
                launch_id: netplay.map(|netplay| netplay.launch_id),
                sent,
            },
            other => other,
        },
        SharedPlayEvent::StreamRequested { op, device_label } => {
            if shared_play_busy(&model) {
                model
            } else {
                M::StreamStarting { op, device_label }
            }
        }
        SharedPlayEvent::NetplayRequested { op, start } => {
            if shared_play_busy(&model)
                && !matches!(
                    model,
                    M::Installing {
                        step: InstallStep::Installing,
                        ..
                    }
                )
            {
                model
            } else {
                M::NetplayStarting {
                    op,
                    start,
                    netplay: None,
                }
            }
        }
        SharedPlayEvent::InstallRequested { op, invite, title } => {
            if shared_play_busy(&model) {
                model
            } else {
                M::Installing {
                    op,
                    invite,
                    title,
                    step: InstallStep::Finding,
                }
            }
        }
        SharedPlayEvent::InstallStepped { op, step } => match model {
            M::Installing {
                op: current,
                invite,
                title,
                ..
            } if current == op => M::Installing {
                op,
                invite,
                title,
                step,
            },
            other => other,
        },
        SharedPlayEvent::ReviewReady {
            op,
            target,
            facts,
            question,
        } => match model {
            M::Installing {
                op: current,
                invite,
                title,
                ..
            } if current == op => M::Reviewing {
                op,
                invite,
                title,
                target,
                facts,
                question,
            },
            other => other,
        },
        SharedPlayEvent::Approved => match model {
            M::Reviewing {
                op, invite, title, ..
            } => M::Installing {
                op,
                invite,
                title,
                step: InstallStep::Installing,
            },
            other => other,
        },
        SharedPlayEvent::CancelPressed => match model {
            M::Reviewing { .. } => M::Unread { game_id: None },
            // Installing has no Cancel: the plugin is half in place until it ends.
            M::Installing {
                step: InstallStep::Installing,
                ..
            } => model,
            M::Installing { .. } => M::Unread { game_id: None },
            M::StreamStarting { op, .. } => M::CancellingStream { pending: op },
            M::NetplayStarting { op, netplay, .. } => M::CancellingNetplay {
                pending: Some(op),
                launch_id: netplay.map(|netplay| netplay.launch_id),
                sent: false,
            },
            // No request of ours is under way: korrid's report names the launch.
            M::Observing { netplay, game_id } => {
                if cancellable(&netplay) {
                    M::CancellingNetplay {
                        pending: None,
                        launch_id: Some(netplay.launch_id),
                        sent: false,
                    }
                } else {
                    M::Observing { netplay, game_id }
                }
            }
            other => other,
        },
        SharedPlayEvent::CancelSent => match model {
            M::CancellingNetplay {
                pending, launch_id, ..
            } => M::CancellingNetplay {
                pending,
                launch_id,
                sent: true,
            },
            other => other,
        },
        SharedPlayEvent::LeaveRequested { op, kicker } => {
            if shared_play_busy(&model) {
                model
            } else {
                M::Leaving { op, kicker }
            }
        }
        SharedPlayEvent::Settled(game_id) => M::Unread { game_id },
        SharedPlayEvent::Failed(status) => M::Problem(status),
        SharedPlayEvent::Dismissed => match model {
            M::Problem(_) => M::Unread { game_id: None },
            other => other,
        },
    }
}

/// What the model shows. Playing shows nothing: the game has the screen.
pub fn shared_play_status(model: &SharedPlay) -> Option<SurfaceStatus> {
    let busy = |kicker: String, actions: Option<Vec<SurfaceAction>>| SurfaceStatus::Busy {
        kicker,
        detail: None,
        progress: None,
        actions,
        game_id: None,
    };
    match model {
        SharedPlay::Unread { .. } | SharedPlay::Idle => None,
        SharedPlay::Observing { netplay, game_id } => netplay_status_from(netplay, game_id.clone()),
        SharedPlay::StreamStarting { device_label, .. } => Some(busy(
            format!("Connecting to {device_label}"),
            Some(vec![SurfaceAction::new(CANCEL, "Cancel", true)]),
        )),
        SharedPlay::NetplayStarting { start, netplay, .. } => match netplay {
            Some(netplay) => netplay_status_from(netplay, start.game_id.clone()),
            None => netplay_status_from(
                &NetplayStatus {
                    launch_id: String::new(),
                    device_label: start.device_label.clone(),
                    game_title: start.game_title.clone(),
                    phase: match start.copy_bytes {
                        None => NetplayPhase::Connecting,
                        Some(total) => NetplayPhase::Copying { done: 0, total },
                    },
                },
                start.game_id.clone(),
            ),
        },
        SharedPlay::Installing {
            title,
            step: InstallStep::Installing,
            ..
        } => Some(busy(format!("Installing {title}"), None)),
        SharedPlay::Installing { title, step, .. } => Some(busy(
            format!("{} {title}", step.word()),
            Some(vec![netplay_cancel()]),
        )),
        SharedPlay::Reviewing { question, .. } => Some(question.clone()),
        SharedPlay::CancellingNetplay { .. } => Some(busy("Cancelling".into(), None)),
        SharedPlay::CancellingStream { .. } => Some(busy("Leaving stream".into(), None)),
        SharedPlay::Leaving { kicker, .. } => Some(busy(kicker.clone(), None)),
        SharedPlay::Problem(status) => Some(status.clone()),
    }
}

/// Whether korrid's report is worth reading every second.
fn watching(model: &SharedPlay) -> bool {
    matches!(
        model,
        SharedPlay::Unread { .. }
            | SharedPlay::Observing { .. }
            | SharedPlay::NetplayStarting { .. }
            | SharedPlay::CancellingNetplay { .. }
    )
}

fn problem_of(reason: impl Into<String>, kicker: &str) -> SurfaceStatus {
    SurfaceStatus::Problem {
        kicker: kicker.into(),
        reason: reason.into(),
        can_retry: false,
        game_id: None,
        game_title: None,
    }
}

// ---------------------------------------------------------------- effects

/// The parts of the host shared play reads and refreshes.
pub struct SharedPlayContext {
    pub state: Box<dyn Fn() -> LaunchablesState + Send + Sync>,
    pub refresh_session: Box<dyn Fn() -> futures::future::BoxFuture<'static, ()> + Send + Sync>,
    /// A plugin was installed: the library re-reads what it can play.
    pub plugins_changed: Box<dyn Fn() + Send + Sync>,
}

struct SharedInner {
    invites: Vec<Invite>,
    model: SharedPlay,
    next_op: u64,
    refresh_id: u64,
}

pub struct SharedPlayController {
    korrid: super::korrid::Korrid,
    inner: std::sync::Mutex<SharedInner>,
    notify: super::Notify,
    context: SharedPlayContext,
}

impl SharedPlayController {
    pub fn new(
        korrid: super::korrid::Korrid,
        notify: super::Notify,
        context: SharedPlayContext,
    ) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            korrid,
            inner: std::sync::Mutex::new(SharedInner {
                invites: Vec::new(),
                model: SharedPlay::Unread { game_id: None },
                next_op: 0,
                refresh_id: 0,
            }),
            notify,
            context,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SharedInner> {
        self.inner.lock().expect("shared play poisoned")
    }

    pub fn invites(&self) -> Vec<Invite> {
        self.lock().invites.clone()
    }
    pub fn status(&self) -> Option<SurfaceStatus> {
        shared_play_status(&self.lock().model)
    }
    pub fn model(&self) -> SharedPlay {
        self.lock().model.clone()
    }

    fn next_op(&self) -> u64 {
        let mut inner = self.lock();
        inner.next_op += 1;
        inner.next_op
    }

    /// One transition, then the effect its case asks for.
    fn dispatch(self: &std::sync::Arc<Self>, event: SharedPlayEvent) {
        let send = {
            let mut inner = self.lock();
            let model = std::mem::replace(&mut inner.model, SharedPlay::Idle);
            inner.model = shared_play_step(model, event);
            // A netplay cancel goes to korrid's exact launch as soon as it is known.
            match &inner.model {
                SharedPlay::CancellingNetplay {
                    pending,
                    launch_id: Some(launch_id),
                    sent: false,
                } => Some((launch_id.clone(), *pending)),
                _ => None,
            }
        };
        (self.notify)();
        if let Some((launch_id, waiting)) = send {
            self.dispatch(SharedPlayEvent::CancelSent);
            let this = std::sync::Arc::clone(self);
            tokio::spawn(async move {
                this.korrid.session_cancel(launch_id).await;
                // With a request still under way, its reply ends the cancel.
                if waiting.is_some()
                    || !matches!(this.lock().model, SharedPlay::CancellingNetplay { .. })
                {
                    return;
                }
                this.settle(None).await;
            });
        }
    }

    pub async fn refresh(&self) {
        let id = {
            let mut inner = self.lock();
            inner.refresh_id += 1;
            inner.refresh_id
        };
        let result = super::korrid::ask!(
            self.korrid,
            Unreachable,
            InvitesList,
            crate::invites::InvitesListRequest {},
            crate::invites::InvitesListOutcome
        );
        {
            let mut inner = self.lock();
            if id != inner.refresh_id {
                return;
            }
            inner.invites = match result {
                crate::invites::InvitesListOutcome::Ok(list) => list.invites,
                crate::invites::InvitesListOutcome::Err(_) => Vec::new(),
            };
        }
        (self.notify)();
    }

    /// korrid's report of this device's netplay. A failed read changes nothing.
    async fn observe(self: &std::sync::Arc<Self>) {
        if let crate::SessionStatusOutcome::Ok(status) = self.korrid.session_status(None).await {
            self.dispatch(SharedPlayEvent::Read(status.netplay));
        }
    }

    /// The work ended: korrid's report and the invites decide what shows.
    async fn settle(self: &std::sync::Arc<Self>, game_id: Option<String>) {
        self.dispatch(SharedPlayEvent::Settled(game_id));
        (self.context.refresh_session)().await;
        self.observe().await;
        self.refresh().await;
    }

    fn fail(self: &std::sync::Arc<Self>, reason: impl Into<String>, kicker: &str) {
        self.dispatch(SharedPlayEvent::Failed(problem_of(reason, kicker)));
    }

    /// Read every second; observe korrid's report while it is worth it.
    pub fn start(self: &std::sync::Arc<Self>) {
        let this = std::sync::Arc::clone(self);
        tokio::spawn(async move {
            loop {
                this.refresh().await;
                if watching(&this.lock().model) {
                    this.observe().await;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
    }

    /// The reply to a start request, for the case that sent it. Cancelled:
    /// a launch that started anyway is left, so the holder plays on.
    async fn answered(
        self: &std::sync::Arc<Self>,
        op: u64,
        outcome: crate::stream_play::SessionStreamPlayOutcome,
        failed: impl FnOnce(&std::sync::Arc<Self>, crate::RpcFailure),
        kicker: &str,
        game_id: Option<String>,
    ) {
        let current = self.model();
        let pending = match &current {
            SharedPlay::CancellingStream { pending } => Some(*pending),
            SharedPlay::CancellingNetplay { pending, .. } => *pending,
            _ => None,
        };
        if pending == Some(op) {
            if let crate::stream_play::SessionStreamPlayOutcome::Ok(session) = &outcome {
                let left = self.korrid.session_leave(session.launch_id.clone()).await;
                if let crate::SessionTransitionOutcome::Err(failure) = left {
                    self.fail(failure.message, kicker);
                    return;
                }
            }
            self.settle(None).await;
            return;
        }
        let owner = match &current {
            SharedPlay::StreamStarting { op, .. }
            | SharedPlay::NetplayStarting { op, .. }
            | SharedPlay::Installing { op, .. }
            | SharedPlay::Reviewing { op, .. }
            | SharedPlay::Leaving { op, .. } => Some(*op),
            _ => None,
        };
        if owner != Some(op) {
            return;
        }
        match outcome {
            crate::stream_play::SessionStreamPlayOutcome::Err(failure) => failed(self, failure),
            crate::stream_play::SessionStreamPlayOutcome::Ok(_) => self.settle(game_id).await,
        }
    }

    async fn play(self: std::sync::Arc<Self>, invite: Invite) {
        let op = self.next_op();
        self.dispatch(SharedPlayEvent::StreamRequested {
            op,
            device_label: invite.device_label.clone(),
        });
        // Cancel cannot cancel a far session. Once the daemon's request
        // settles, Leave retires only this device's exact viewer.
        let outcome = super::korrid::ask!(
            self.korrid,
            Unreachable,
            InvitesStreamPlay,
            crate::invites::InviteRequest {
                invite_id: invite.invite_id.clone()
            },
            crate::stream_play::SessionStreamPlayOutcome
        );
        self.answered(
            op,
            outcome,
            |this, failure| this.fail(failure.message, "Stream unavailable"),
            "Stream unavailable",
            None,
        )
        .await;
    }

    fn facts(invite: &Invite, core: Option<String>) -> NetplayProblemFacts {
        NetplayProblemFacts {
            game_title: invite
                .session
                .title
                .clone()
                .or_else(|| invite.session.game_id.clone())
                .unwrap_or_else(|| "The game".into()),
            device_label: invite.device_label.clone(),
            core,
        }
    }

    /// Copy when needed, then netplay: korrid does both in one request.
    async fn netplay(self: std::sync::Arc<Self>, invite: Invite, core: Option<String>) {
        let op = self.next_op();
        let game_id = game_entry_id(&(self.context.state)(), invite.session.game_id.as_deref());
        self.dispatch(SharedPlayEvent::NetplayRequested {
            op,
            start: NetplayStart {
                invite_id: invite.invite_id.clone(),
                device_label: invite.device_label.clone(),
                game_title: invite.session.title.clone(),
                game_id: game_id.clone(),
                copy_bytes: match &invite.netplay {
                    Some(InviteNetplay::NeedsCopy { bytes }) => Some(*bytes as u64),
                    _ => None,
                },
                core: core.clone(),
            },
        });
        let outcome = super::korrid::ask!(
            self.korrid,
            Unreachable,
            InvitesNetplay,
            crate::invites::InviteRequest {
                invite_id: invite.invite_id.clone()
            },
            crate::stream_play::SessionStreamPlayOutcome
        );
        let facts = Self::facts(&invite, core);
        self.answered(
            op,
            outcome,
            move |this, failure| {
                this.dispatch(SharedPlayEvent::Failed(netplay_problem(&failure, &facts)))
            },
            "Netplay unavailable",
            game_id,
        )
        .await;
    }

    /// Settings' own plugin controller, on facts this flow holds for one step.
    async fn controller_step(
        self: &std::sync::Arc<Self>,
        seed: super::plugins::PluginFacts,
        action_id: Option<String>,
    ) -> (super::plugins::PluginFacts, Option<String>) {
        use super::plugins::{PluginsController, PluginsHost};
        let facts = std::sync::Arc::new(std::sync::Mutex::new(seed));
        let locked = std::sync::Arc::new(std::sync::Mutex::new(false));
        let (sender, receiver) = tokio::sync::oneshot::channel::<Option<String>>();
        let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
        let (read, write, get, set) = (
            std::sync::Arc::clone(&facts),
            std::sync::Arc::clone(&facts),
            std::sync::Arc::clone(&locked),
            std::sync::Arc::clone(&locked),
        );
        let resolve = std::sync::Arc::clone(&sender);
        let this = std::sync::Arc::clone(self);
        let controller = PluginsController::new(
            self.korrid.clone(),
            PluginsHost {
                facts: Box::new(move || read.lock().expect("facts poisoned").clone()),
                publish: Box::new(move |next| *write.lock().expect("facts poisoned") = next),
                status: Box::new(move |next| {
                    let failure = match next {
                        korri_surface_treaty::SurfaceSettingsStatus::Saving { .. } => return,
                        korri_surface_treaty::SurfaceSettingsStatus::Problem {
                            message, ..
                        } => Some(message),
                        korri_surface_treaty::SurfaceSettingsStatus::Idle => None,
                    };
                    if let Some(sender) = resolve.lock().expect("step poisoned").take() {
                        let _ = sender.send(failure);
                    }
                }),
                busy_get: Box::new(move || *get.lock().expect("lock poisoned")),
                busy_set: Box::new(move |value| *set.lock().expect("lock poisoned") = value),
                changed: Box::new(move || (this.context.plugins_changed)()),
            },
        );
        let failure = match action_id {
            None => {
                controller.refresh().await;
                None
            }
            Some(action_id) => {
                controller.run(&action_id);
                receiver.await.ok().flatten()
            }
        };
        let facts = facts.lock().expect("facts poisoned").clone();
        (facts, failure)
    }

    fn still(&self, op: u64, step: InstallStep) -> bool {
        matches!(&self.lock().model, SharedPlay::Installing { op: current, step: now, .. } if *current == op && *now == step)
    }

    /// Install the plugin korrid names, then netplay: Settings' own inspect
    /// and approve flow, waiting in Reviewing for the person's answer.
    async fn install(self: std::sync::Arc<Self>, invite: Invite, plugin_id: String, title: String) {
        use super::plugins::*;
        let op = self.next_op();
        self.dispatch(SharedPlayEvent::InstallRequested {
            op,
            invite: invite.clone(),
            title: title.clone(),
        });
        let (found, _) = self.controller_step(PluginFacts::default(), None).await;
        if !self.still(op, InstallStep::Finding) {
            return;
        }
        let Some(target) = newest_release(&found, &plugin_id) else {
            self.fail(
                format!("No plugin repository on this device offers {title}."),
                "Install failed",
            );
            return;
        };
        self.dispatch(SharedPlayEvent::InstallStepped {
            op,
            step: InstallStep::Checking,
        });
        let (checked, failure) = self
            .controller_step(found, Some(inspect_action_id(&target)))
            .await;
        if !self.still(op, InstallStep::Checking) {
            return;
        }
        let review = review_for(&checked, &target).cloned();
        let (Some(review), None) = (review, failure.clone()) else {
            self.fail(
                failure.unwrap_or_else(|| format!("{title} could not be checked.")),
                "Install failed",
            );
            return;
        };
        // Trusting a key is Settings' question, asked with the key in view.
        let untrusted: Vec<String> = publishers_to_trust(&review.report)
            .into_iter()
            .map(|publisher| publisher.namespace)
            .collect();
        let name = review.report.title.clone().unwrap_or_else(|| title.clone());
        if !untrusted.is_empty() {
            self.fail(
                format!(
                    "{name} needs you to trust {} first. Check the key in Settings, under Plugins.",
                    untrusted.join(", ")
                ),
                "Install failed",
            );
            return;
        }
        let game_id = game_entry_id(&(self.context.state)(), invite.session.game_id.as_deref());
        self.dispatch(SharedPlayEvent::ReviewReady {
            op,
            target,
            facts: Box::new(checked),
            question: SurfaceStatus::Confirm {
                kicker: format!("Install {name}?"),
                detail: Some(review_message(&review.report)),
                actions: vec![
                    SurfaceAction::new(NETPLAY_INSTALL, "Install and play here", true),
                    netplay_cancel(),
                ],
                // A stray press must never approve a plugin or its permissions.
                decline_action_id: Some(NETPLAY_CANCEL.into()),
                game_id,
            },
        });
    }

    /// The person said yes to the review.
    async fn approve(self: std::sync::Arc<Self>) {
        let SharedPlay::Reviewing {
            op,
            invite,
            title,
            target,
            facts,
            ..
        } = self.model()
        else {
            return;
        };
        self.dispatch(SharedPlayEvent::Approved);
        let (_, failure) = self
            .controller_step(*facts, Some(super::plugins::review_action_id(&target)))
            .await;
        if !self.still(op, InstallStep::Installing) {
            return;
        }
        if let Some(failure) = failure {
            self.fail(failure, "Install failed");
            return;
        }
        // korrid decides the route again with the new core.
        self.refresh().await;
        if !self.still(op, InstallStep::Installing) {
            return;
        }
        let Some(again) = self
            .invites()
            .into_iter()
            .find(|candidate| candidate.invite_id == invite.invite_id)
        else {
            self.settle(None).await;
            return;
        };
        if let Some(InviteNetplay::Unavailable { reason }) = &again.netplay {
            let failure = crate::RpcFailure {
                code: reason.clone(),
                message: reason.clone(),
            };
            self.dispatch(SharedPlayEvent::Failed(netplay_problem(
                &failure,
                &Self::facts(&again, Some(title)),
            )));
            return;
        }
        Box::pin(self.netplay(again, Some(title))).await;
    }

    async fn leave(
        self: std::sync::Arc<Self>,
        launch_id: String,
        kicker: &'static str,
        failed: &'static str,
    ) {
        let op = self.next_op();
        self.dispatch(SharedPlayEvent::LeaveRequested {
            op,
            kicker: kicker.into(),
        });
        let outcome = self.korrid.session_leave(launch_id).await;
        if !matches!(&self.lock().model, SharedPlay::Leaving { op: current, .. } if *current == op)
        {
            return;
        }
        if let crate::SessionTransitionOutcome::Err(failure) = outcome {
            self.fail(failure.message, failed);
            return;
        }
        self.settle(None).await;
    }

    /// Cancel or Back on the work in progress; true when it was the host's.
    fn cancel(self: &std::sync::Arc<Self>) -> bool {
        let before = self.model();
        let busy = shared_play_busy(&before);
        self.dispatch(SharedPlayEvent::CancelPressed);
        busy || format!("{before:?}") != format!("{:?}", self.model())
    }

    fn held_by(&self) -> Option<NetplayStatus> {
        match &self.lock().model {
            SharedPlay::Observing { netplay, .. }
                if matches!(netplay.phase, NetplayPhase::HolderLeft) =>
            {
                Some(netplay.clone())
            }
            _ => None,
        }
    }

    pub fn act(self: &std::sync::Arc<Self>, id: &str) -> bool {
        if id == CANCEL || id == NETPLAY_CANCEL {
            return self.cancel();
        }
        if id == NETPLAY_INSTALL {
            if !matches!(self.lock().model, SharedPlay::Reviewing { .. }) {
                return false;
            }
            tokio::spawn(std::sync::Arc::clone(self).approve());
            return true;
        }
        if let Some(held) = self
            .held_by()
            .filter(|held| id == format!("{NETPLAY_LEAVE}{}", held.launch_id))
        {
            tokio::spawn(std::sync::Arc::clone(self).leave(
                held.launch_id,
                "Leaving",
                "Netplay unavailable",
            ));
            return true;
        }
        let state = (self.context.state)();
        if let Some(SurfaceStatus::Paused { actions, .. }) = stream_status_from(&state) {
            if actions.first().is_some_and(|action| action.id == id)
                && !shared_play_busy(&self.model())
            {
                let launch_id = id[LEAVE.len()..].to_owned();
                tokio::spawn(std::sync::Arc::clone(self).leave(
                    launch_id,
                    "Leaving stream",
                    "Stream unavailable",
                ));
                return true;
            }
        }
        let Some(invite) = self.invites().into_iter().find(|invite| {
            [STREAM, NETPLAY, DISMISS]
                .iter()
                .any(|prefix| id == format!("{prefix}{}", invite.invite_id))
        }) else {
            return false;
        };
        if shared_play_busy(&self.model()) || !state.is_ready() {
            return true;
        }
        if id == format!("{STREAM}{}", invite.invite_id) {
            tokio::spawn(std::sync::Arc::clone(self).play(invite));
        } else if id == format!("{NETPLAY}{}", invite.invite_id) {
            match invite.netplay.clone() {
                Some(InviteNetplay::NeedsPlugin {
                    plugin_id, title, ..
                }) => {
                    tokio::spawn(std::sync::Arc::clone(self).install(invite, plugin_id, title));
                }
                Some(InviteNetplay::Ready | InviteNetplay::NeedsCopy { .. }) => {
                    tokio::spawn(std::sync::Arc::clone(self).netplay(invite, None));
                }
                _ => {}
            }
        } else {
            let this = std::sync::Arc::clone(self);
            tokio::spawn(async move {
                let outcome = super::korrid::ask!(
                    this.korrid,
                    Unreachable,
                    InvitesDismiss,
                    crate::invites::InviteRequest {
                        invite_id: invite.invite_id.clone()
                    },
                    crate::invites::InviteDismissOutcome
                );
                match outcome {
                    crate::invites::InviteDismissOutcome::Err(failure) => {
                        this.fail(failure.message, "Stream unavailable")
                    }
                    crate::invites::InviteDismissOutcome::Ok(_) => this.refresh().await,
                }
            });
        }
        true
    }

    /// The host's Back: true when shared play took it.
    pub fn back(self: &std::sync::Arc<Self>) -> bool {
        if self.cancel() {
            return true;
        }
        if let Some(held) = self.held_by() {
            tokio::spawn(std::sync::Arc::clone(self).leave(
                held.launch_id,
                "Leaving",
                "Netplay unavailable",
            ));
            return true;
        }
        // A drop is korrid's to retry; Back waits with it.
        if matches!(&self.lock().model, SharedPlay::Observing { netplay, .. } if matches!(netplay.phase, NetplayPhase::Reconnecting))
        {
            return true;
        }
        let Some(SurfaceStatus::Paused { actions, .. }) =
            stream_status_from(&(self.context.state)())
        else {
            return false;
        };
        let launch_id = actions[0].id[LEAVE.len()..].to_owned();
        tokio::spawn(std::sync::Arc::clone(self).leave(
            launch_id,
            "Leaving stream",
            "Stream unavailable",
        ));
        true
    }

    pub fn dismiss(self: &std::sync::Arc<Self>) -> bool {
        if !matches!(self.lock().model, SharedPlay::Problem(_)) {
            return false;
        }
        self.dispatch(SharedPlayEvent::Dismissed);
        true
    }
}
