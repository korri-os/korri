//! Port of `clients/portal/src/surface/acquisition-gate.ts`: before a local
//! launch, check what the game's runner needs, ask before a download, fetch
//! it, then go on into the launch (ADR 0012, ADR 0013).

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};

use korri_surface_treaty::{SurfaceAction, SurfaceProgress, SurfaceStatus};

use super::{
    downloads::{is_active, is_stoppable, total_bytes},
    korrid::Korrid,
    shared_play::format_bytes,
    Notify,
};
use crate::{
    acquisition::{AcquisitionOutcome, ArtifactState, ArtifactStatus},
    RpcFailure,
};

/// The game a launch is about: korrid's id, the surface's id, and its title.
#[derive(Clone, Debug)]
pub struct AcquisitionGameRef {
    pub id: String,
    pub surface_id: String,
    pub title: String,
    /// The runner whose artifacts to resolve, when the user chose one.
    pub runner_id: Option<String>,
}

pub type Step = Arc<dyn Fn() + Send + Sync>;

/// One gated launch.
#[derive(Clone)]
struct GatedLaunch {
    game: AcquisitionGameRef,
    proceed: Step,
    abandon: Step,
    /// Already under way when the gate first saw them: never the gate's to stop.
    held: Arc<Mutex<HashSet<String>>>,
    /// Asked for by this gate: only these does it cancel.
    owned: Arc<Mutex<HashSet<String>>>,
}

#[derive(Clone, Copy, PartialEq)]
enum RetryKind {
    Check,
    Start,
    None,
}

#[derive(Clone)]
enum Phase {
    Idle,
    Checking(GatedLaunch),
    Confirming(GatedLaunch, Vec<ArtifactStatus>),
    Acquiring {
        launch: GatedLaunch,
        artifacts: Vec<ArtifactStatus>,
        starting: bool,
        cancel_requested: bool,
    },
    Cancelling(GatedLaunch),
    Failed {
        launch: GatedLaunch,
        artifacts: Vec<ArtifactStatus>,
        kicker: String,
        reason: String,
        retry: RetryKind,
    },
}

fn is_ready(artifact: &ArtifactStatus) -> bool {
    matches!(artifact.state, ArtifactState::Ready { .. })
}

/// "0:45", "1:30", "1:02:03".
pub fn format_eta(seconds: f64) -> String {
    let whole = seconds.round().max(0.0) as u64;
    let (hours, minutes, rest) = (whole / 3600, (whole % 3600) / 60, whole % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{rest:02}")
    } else {
        format!("{minutes}:{rest:02}")
    }
}

/// "A", "A and B", "A, B and C".
fn list_titles(titles: &[String]) -> String {
    match titles {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// "FEX rootfs and Proton-CachyOS, 1.5 GB" or ", size unknown".
pub fn confirm_detail(missing: &[&ArtifactStatus]) -> String {
    let titles: Vec<String> = missing
        .iter()
        .map(|artifact| artifact.title.clone())
        .collect();
    format!(
        "{}, {}",
        list_titles(&titles),
        total_bytes(missing.iter().copied()).map_or("size unknown".into(), format_bytes)
    )
}

/// "612 MB of 1.2 GB · 6.4 MB/s · 1:30 left", with unknown parts left out.
fn download_detail(done: u64, total: Option<u64>, per_second: Option<f64>) -> String {
    let mut parts = vec![match total {
        None => format!("{} so far", format_bytes(done as f64)),
        Some(total) => format!(
            "{} of {}",
            format_bytes(done as f64),
            format_bytes(total as f64)
        ),
    }];
    if let Some(rate) = per_second.filter(|rate| *rate > 0.0) {
        parts.push(format!("{}/s", format_bytes(rate)));
        if let Some(total) = total {
            parts.push(format!(
                "{} left",
                format_eta((total as f64 - done as f64) / rate)
            ));
        }
    }
    parts.join(" · ")
}

/// The Busy status for artifacts korrid is working on.
pub fn acquiring_status(
    artifacts: &[ArtifactStatus],
    game_id: &str,
    cancel: SurfaceAction,
) -> SurfaceStatus {
    let busy = |kicker: String,
                detail: Option<String>,
                progress: Option<SurfaceProgress>,
                actions: Option<Vec<SurfaceAction>>| {
        SurfaceStatus::Busy {
            kicker,
            detail,
            progress,
            steps: None,
            // Back is the cancel on offer, as the portal's Back was.
            decline_action_id: actions
                .as_ref()
                .and_then(|actions| actions.first())
                .map(|action| action.id.clone()),
            actions,
            game_id: Some(game_id.into()),
        }
    };
    for artifact in artifacts {
        if let ArtifactState::Downloading {
            bytes_done,
            bytes_total,
            bytes_per_second,
            ..
        } = &artifact.state
        {
            return busy(
                format!("Downloading {}", artifact.title),
                Some(download_detail(
                    *bytes_done,
                    *bytes_total,
                    *bytes_per_second,
                )),
                Some(SurfaceProgress {
                    done: *bytes_done as f64,
                    total: bytes_total.map(|total| total as f64),
                }),
                Some(vec![cancel]),
            );
        }
    }
    if let Some(unpacking) = artifacts
        .iter()
        .find(|artifact| matches!(artifact.state, ArtifactState::Unpacking))
    {
        return busy(
            format!("Unpacking {}", unpacking.title),
            None,
            Some(SurfaceProgress {
                done: 0.0,
                total: None,
            }),
            None,
        );
    }
    busy(
        "Starting download…".into(),
        Some("Waiting for the first bytes".into()),
        None,
        Some(vec![cancel]),
    )
}

const CANCELLING_DETAIL: &str = "Deleting the partial download";

fn stoppable(launch: &GatedLaunch, artifacts: &[ArtifactStatus]) -> Vec<String> {
    let owned = launch.owned.lock().expect("owned poisoned");
    let held = launch.held.lock().expect("held poisoned");
    artifacts
        .iter()
        .filter(|artifact| {
            is_stoppable(artifact) && owned.contains(&artifact.id) && !held.contains(&artifact.id)
        })
        .map(|artifact| artifact.id.clone())
        .collect()
}

struct Inner {
    phase: Phase,
    status: Option<SurfaceStatus>,
    generation: u64,
}

pub struct AcquisitionGate {
    korrid: Korrid,
    inner: Mutex<Inner>,
    notify: Notify,
    on_settled: Step,
    poll_interval: Duration,
}

impl AcquisitionGate {
    pub fn new(
        korrid: Korrid,
        notify: Notify,
        on_settled: Step,
        poll_interval: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            korrid,
            inner: Mutex::new(Inner {
                phase: Phase::Idle,
                status: None,
                generation: 0,
            }),
            notify,
            on_settled,
            poll_interval,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("acquisition gate poisoned")
    }

    /// The status to show in place of the launch status; None while idle.
    pub fn status(&self) -> Option<SurfaceStatus> {
        self.lock().status.clone()
    }

    /// korrid's id of the game the open gate fetches for.
    pub fn game_id(&self) -> Option<String> {
        match &self.lock().phase {
            Phase::Idle => None,
            Phase::Checking(launch)
            | Phase::Confirming(launch, _)
            | Phase::Acquiring { launch, .. }
            | Phase::Cancelling(launch)
            | Phase::Failed { launch, .. } => Some(launch.game.id.clone()),
        }
    }

    fn status_for(phase: &Phase, generation: u64) -> Option<SurfaceStatus> {
        let action = |id: &str, label: &str| {
            SurfaceAction::new(format!("acquisition:{generation}:{id}"), label, true)
        };
        let busy = |kicker: String, detail: &str, launch: &GatedLaunch| SurfaceStatus::Busy {
            kicker,
            detail: Some(detail.into()),
            progress: None,
            steps: None,
            actions: None,
            decline_action_id: None,
            game_id: Some(launch.game.surface_id.clone()),
        };
        Some(match phase {
            Phase::Idle => return None,
            // Back abandoned the launch while korrid checked: the same cancel.
            Phase::Checking(launch) => {
                let cancel = action("cancel", "Cancel");
                SurfaceStatus::Busy {
                    kicker: format!("Starting {}…", launch.game.title),
                    detail: Some("Checking what it needs".into()),
                    progress: None,
                    steps: None,
                    decline_action_id: Some(cancel.id.clone()),
                    actions: Some(vec![cancel]),
                    game_id: Some(launch.game.surface_id.clone()),
                }
            }
            Phase::Confirming(launch, artifacts) => SurfaceStatus::Confirm {
                kicker: "Needs a download".into(),
                detail: Some(confirm_detail(
                    &artifacts
                        .iter()
                        .filter(|artifact| !is_ready(artifact))
                        .collect::<Vec<_>>(),
                )),
                actions: vec![action("download", "Download"), action("cancel", "Cancel")],
                decline_action_id: Some(action("cancel", "Cancel").id),
                game_id: Some(launch.game.surface_id.clone()),
            },
            Phase::Acquiring {
                launch,
                artifacts,
                starting,
                cancel_requested,
            } => {
                if *cancel_requested {
                    busy("Cancelling download…".into(), CANCELLING_DETAIL, launch)
                } else {
                    let label = if *starting || !stoppable(launch, artifacts).is_empty() {
                        "Cancel"
                    } else {
                        "Play later"
                    };
                    acquiring_status(artifacts, &launch.game.surface_id, action("cancel", label))
                }
            }
            Phase::Cancelling(launch) => {
                busy("Cancelling download…".into(), CANCELLING_DETAIL, launch)
            }
            Phase::Failed {
                launch,
                kicker,
                reason,
                retry,
                ..
            } => SurfaceStatus::Problem {
                kicker: kicker.clone(),
                reason: reason.clone(),
                can_retry: *retry != RetryKind::None,
                game_id: Some(launch.game.surface_id.clone()),
                game_title: Some(launch.game.title.clone()),
            },
        })
    }

    fn publish(&self, next: Phase) {
        {
            let mut inner = self.lock();
            inner.status = Self::status_for(&next, inner.generation);
            inner.phase = next;
        }
        (self.notify)();
    }

    fn bump(&self) -> u64 {
        let mut inner = self.lock();
        inner.generation += 1;
        inner.generation
    }

    fn current(&self, operation: u64) -> bool {
        self.lock().generation == operation
    }

    fn phase(&self) -> Phase {
        self.lock().phase.clone()
    }

    fn abandon(&self, launch: &GatedLaunch) {
        self.bump();
        self.publish(Phase::Idle);
        (launch.abandon)();
    }

    fn failed(
        &self,
        launch: GatedLaunch,
        artifacts: Vec<ArtifactStatus>,
        kicker: String,
        failure: RpcFailure,
        retry: RetryKind,
    ) {
        self.bump();
        self.publish(Phase::Failed {
            launch,
            artifacts,
            kicker,
            reason: failure.message,
            retry,
        });
    }

    fn finish(&self, launch: &GatedLaunch) {
        self.bump();
        self.publish(Phase::Idle);
        (self.on_settled)();
        (launch.proceed)();
    }

    async fn read(&self, game: &AcquisitionGameRef) -> AcquisitionOutcome {
        self.korrid
            .acquisition_status(Some(game.id.clone()), game.runner_id.clone())
            .await
    }

    /// Fold an observed artifact list into the acquiring phase.
    fn settle(&self, launch: GatedLaunch, cancel_requested: bool, artifacts: Vec<ArtifactStatus>) {
        if artifacts.iter().all(is_ready) {
            self.finish(&launch);
            return;
        }
        if let Some((title, reason)) = artifacts.iter().find_map(|artifact| match &artifact.state {
            ArtifactState::Failed { reason } => Some((artifact.title.clone(), reason.clone())),
            _ => None,
        }) {
            self.failed(
                launch,
                artifacts,
                format!("Couldn't download {title}"),
                reason,
                RetryKind::Start,
            );
            return;
        }
        if !artifacts.iter().any(is_active) {
            self.bump();
            self.publish(Phase::Confirming(launch, artifacts));
            return;
        }
        self.publish(Phase::Acquiring {
            launch,
            artifacts,
            starting: false,
            cancel_requested,
        });
    }

    async fn observe(self: Arc<Self>, operation: u64) {
        loop {
            if !self.current(operation) || !matches!(self.phase(), Phase::Acquiring { .. }) {
                return;
            }
            tokio::time::sleep(self.poll_interval).await;
            let Phase::Acquiring {
                launch,
                artifacts,
                cancel_requested,
                ..
            } = self.phase()
            else {
                return;
            };
            if !self.current(operation) {
                return;
            }
            let result = self.read(&launch.game).await;
            if !self.current(operation) || !matches!(self.phase(), Phase::Acquiring { .. }) {
                return;
            }
            match result {
                AcquisitionOutcome::Err(failure) => {
                    self.failed(
                        launch,
                        artifacts,
                        "Lost sight of the download".into(),
                        failure,
                        RetryKind::Start,
                    );
                    return;
                }
                AcquisitionOutcome::Ok(snapshot) => {
                    self.settle(launch, cancel_requested, snapshot.artifacts)
                }
            }
        }
    }

    async fn start(self: Arc<Self>, launch: GatedLaunch, artifacts: Vec<ArtifactStatus>) {
        let operation = self.bump();
        let ids: Vec<String> = artifacts
            .iter()
            .filter(|artifact| !is_ready(artifact))
            .map(|artifact| artifact.id.clone())
            .collect();
        {
            let held = launch.held.lock().expect("held poisoned");
            let mut owned = launch.owned.lock().expect("owned poisoned");
            for id in &ids {
                if !held.contains(id) {
                    owned.insert(id.clone());
                }
            }
        }
        self.publish(Phase::Acquiring {
            launch: launch.clone(),
            artifacts,
            starting: true,
            cancel_requested: false,
        });
        let result = self.korrid.acquisition_start(ids).await;
        let Phase::Acquiring {
            launch,
            artifacts,
            cancel_requested,
            ..
        } = self.phase()
        else {
            return;
        };
        if !self.current(operation) {
            return;
        }
        match result {
            AcquisitionOutcome::Err(failure) => {
                if cancel_requested {
                    self.abandon(&launch);
                    (self.on_settled)();
                } else {
                    self.failed(
                        launch,
                        artifacts,
                        "Couldn't start the download".into(),
                        failure,
                        RetryKind::Start,
                    );
                }
            }
            AcquisitionOutcome::Ok(snapshot) => {
                if cancel_requested {
                    self.cancel_downloads(launch, snapshot.artifacts).await;
                    return;
                }
                self.settle(launch, false, snapshot.artifacts);
                tokio::spawn(Arc::clone(&self).observe(operation));
            }
        }
    }

    async fn check(self: Arc<Self>, launch: GatedLaunch) {
        let operation = self.bump();
        self.publish(Phase::Checking(launch.clone()));
        let result = self.read(&launch.game).await;
        if !self.current(operation) || !matches!(self.phase(), Phase::Checking(_)) {
            return;
        }
        let artifacts = match result {
            AcquisitionOutcome::Err(failure) => {
                self.failed(
                    launch,
                    Vec::new(),
                    "Couldn't check for downloads".into(),
                    failure,
                    RetryKind::Check,
                );
                return;
            }
            AcquisitionOutcome::Ok(snapshot) => snapshot.artifacts,
        };
        if artifacts.iter().all(is_ready) {
            self.finish(&launch);
            return;
        }
        // What korrid already has under way is someone else's to stop.
        {
            let owned = launch.owned.lock().expect("owned poisoned");
            let mut held = launch.held.lock().expect("held poisoned");
            for artifact in &artifacts {
                if is_active(artifact) && !owned.contains(&artifact.id) {
                    held.insert(artifact.id.clone());
                }
            }
        }
        if artifacts.iter().any(is_active) {
            let queued: Vec<String> = artifacts
                .iter()
                .filter(|artifact| matches!(artifact.state, ArtifactState::Queued))
                .map(|artifact| artifact.id.clone())
                .collect();
            self.publish(Phase::Acquiring {
                launch: launch.clone(),
                artifacts: artifacts.clone(),
                starting: false,
                cancel_requested: false,
            });
            if !queued.is_empty() {
                let moved = self.korrid.acquisition_start(queued).await;
                let Phase::Acquiring {
                    launch,
                    cancel_requested,
                    ..
                } = self.phase()
                else {
                    return;
                };
                if !self.current(operation) {
                    return;
                }
                // Refused: the queue still gets there, so keep watching.
                if let AcquisitionOutcome::Ok(snapshot) = moved {
                    let merged = artifacts
                        .iter()
                        .map(|artifact| {
                            snapshot
                                .artifacts
                                .iter()
                                .find(|update| update.id == artifact.id)
                                .cloned()
                                .unwrap_or_else(|| artifact.clone())
                        })
                        .collect();
                    self.settle(launch, cancel_requested, merged);
                }
                if !self.current(operation) {
                    return;
                }
            }
            tokio::spawn(Arc::clone(&self).observe(operation));
            return;
        }
        self.publish(Phase::Confirming(launch, artifacts));
    }

    /// Stop what this gate started and korrid still has queued or
    /// downloading, then close without launching.
    async fn cancel_downloads(&self, launch: GatedLaunch, artifacts: Vec<ArtifactStatus>) {
        let operation = self.bump();
        self.publish(Phase::Cancelling(launch.clone()));
        for id in stoppable(&launch, &artifacts) {
            let result = self.korrid.acquisition_cancel(id).await;
            if !self.current(operation) {
                return;
            }
            if let AcquisitionOutcome::Err(failure) = result {
                self.bump();
                self.publish(Phase::Failed {
                    launch,
                    artifacts,
                    kicker: "Couldn't cancel the download".into(),
                    reason: failure.message,
                    retry: RetryKind::None,
                });
                return;
            }
        }
        self.abandon(&launch);
        (self.on_settled)();
    }

    fn request_cancel(
        self: &Arc<Self>,
        launch: GatedLaunch,
        artifacts: Vec<ArtifactStatus>,
        starting: bool,
        cancel_requested: bool,
    ) {
        if cancel_requested {
            return;
        }
        if starting {
            self.publish(Phase::Acquiring {
                launch,
                artifacts,
                starting,
                cancel_requested: true,
            });
            return;
        }
        if stoppable(&launch, &artifacts).is_empty() {
            self.abandon(&launch);
            (self.on_settled)();
            return;
        }
        let this = Arc::clone(self);
        tokio::spawn(async move { this.cancel_downloads(launch, artifacts).await });
    }

    fn leave_failed(self: &Arc<Self>, launch: GatedLaunch, artifacts: Vec<ArtifactStatus>) {
        if !stoppable(&launch, &artifacts).is_empty() {
            let this = Arc::clone(self);
            tokio::spawn(async move { this.cancel_downloads(launch, artifacts).await });
            return;
        }
        self.abandon(&launch);
        (self.on_settled)();
    }

    /// Check, ask, download, then `proceed` into the launch. No-op while open.
    pub fn launch(self: &Arc<Self>, game: AcquisitionGameRef, proceed: Step, abandon: Step) {
        if !matches!(self.phase(), Phase::Idle) {
            return;
        }
        tokio::spawn(Arc::clone(self).check(GatedLaunch {
            game,
            proceed,
            abandon,
            held: Arc::default(),
            owned: Arc::default(),
        }));
    }

    /// A published `acquisition:` action id. True when the gate consumed it.
    pub fn act(self: &Arc<Self>, action_id: &str) -> bool {
        if !action_id.starts_with("acquisition:") {
            return false;
        }
        // Only what is published now, enabled, is authority.
        let published = match &self.lock().status {
            Some(SurfaceStatus::Confirm { actions, .. }) => actions
                .iter()
                .any(|action| action.id == action_id && action.enabled),
            Some(SurfaceStatus::Busy {
                actions: Some(actions),
                ..
            }) => actions
                .iter()
                .any(|action| action.id == action_id && action.enabled),
            _ => false,
        };
        if !published {
            return true;
        }
        let kind = action_id.rsplit(':').next().unwrap_or_default();
        match self.phase() {
            Phase::Checking(launch) if kind == "cancel" => self.abandon(&launch),
            Phase::Confirming(launch, artifacts) => {
                if kind == "download" {
                    tokio::spawn(Arc::clone(self).start(launch, artifacts));
                } else if kind == "cancel" {
                    self.abandon(&launch);
                }
            }
            Phase::Acquiring {
                launch,
                artifacts,
                starting,
                cancel_requested,
            } if kind == "cancel" => {
                self.request_cancel(launch, artifacts, starting, cancel_requested)
            }
            _ => {}
        }
        true
    }

    /// The host's Back while the gate is open. True when the gate consumed it.
    pub fn back(self: &Arc<Self>) -> bool {
        match self.phase() {
            Phase::Idle => false,
            Phase::Checking(launch) | Phase::Confirming(launch, _) => {
                self.abandon(&launch);
                true
            }
            Phase::Failed {
                launch, artifacts, ..
            } => {
                self.leave_failed(launch, artifacts);
                true
            }
            Phase::Acquiring {
                launch,
                artifacts,
                starting,
                cancel_requested,
            } => {
                // Back is Cancel only where Cancel is on offer.
                let cancel = matches!(&self.lock().status, Some(SurfaceStatus::Busy { actions: Some(actions), .. }) if actions.iter().any(|action| action.enabled));
                if cancel {
                    self.request_cancel(launch, artifacts, starting, cancel_requested);
                }
                true
            }
            Phase::Cancelling(_) => true,
        }
    }

    /// The surface's Retry. True when the gate consumed it.
    pub fn retry(self: &Arc<Self>) -> bool {
        let Phase::Failed {
            launch,
            artifacts,
            retry,
            ..
        } = self.phase()
        else {
            return false;
        };
        match retry {
            RetryKind::Check => {
                tokio::spawn(Arc::clone(self).check(launch));
            }
            RetryKind::Start => {
                tokio::spawn(Arc::clone(self).start(launch, artifacts));
            }
            RetryKind::None => {}
        }
        true
    }

    /// The surface's Dismiss. True when the gate consumed it.
    pub fn dismiss(self: &Arc<Self>) -> bool {
        let Phase::Failed {
            launch, artifacts, ..
        } = self.phase()
        else {
            return false;
        };
        self.leave_failed(launch, artifacts);
        true
    }
}
