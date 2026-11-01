//! Port of `clients/portal/src/surface/game-downloads.ts`: what each local
//! game still needs downloaded, and the downloads a person asks for from the
//! game rather than from a launch (ADR 0013).
//!
//! korrid owns the queue, the bytes and the cache. This store keeps two
//! things it learns from korrid and folds them into one fact per game: which
//! artifacts each game needs (and which of those its release names), from
//! game-scoped status reads; and where each artifact stands, from any read.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use korri_surface_treaty::{SurfaceProgress, SurfaceStatus};

use super::{korrid::Korrid, Notify};
use crate::{
    acquisition::{AcquisitionOutcome, ArtifactState, ArtifactStatus},
    RpcFailure,
};

/// Per-game status reads during a refresh run this many at once.
const READ_CONCURRENCY: usize = 8;
pub const ACQUISITION_POLL_INTERVAL: Duration = Duration::from_millis(1000);

/// One game's download, as korrid's artifacts add up.
#[derive(Clone, Debug, PartialEq)]
pub struct GameDownloadFacts {
    /// Some artifact the game needs is not Ready.
    pub needs: bool,
    /// The declared size of everything not Ready; absent when one is unknown.
    pub bytes: Option<f64>,
    /// Something is queued, downloading or unpacking for it.
    pub active: bool,
    /// Cancel would stop something.
    pub stoppable: bool,
    /// Something of it is queued or downloading that Cancel leaves alone,
    /// because another game's pending download needs it too.
    pub shared: bool,
    /// Active, and waiting behind another download.
    pub queued: bool,
    /// While it downloads, in the treaty's Busy.progress shape.
    pub progress: Option<SurfaceProgress>,
    /// Some artifact its release names is Ready: there is a download to remove.
    pub own_ready: bool,
    /// korrid's words for the last failed try, while nothing is under way.
    pub failure: Option<String>,
}

/// The game an action is about: korrid's id, the surface's id, and its title.
#[derive(Clone, Debug)]
pub struct DownloadGameRef {
    pub id: String,
    pub surface_id: String,
    pub title: String,
}

#[derive(Clone, Debug)]
struct Member {
    id: String,
    release: bool,
}

pub fn is_active(artifact: &ArtifactStatus) -> bool {
    matches!(
        artifact.state,
        ArtifactState::Queued | ArtifactState::Downloading { .. } | ArtifactState::Unpacking
    )
}
fn is_ready(artifact: &ArtifactStatus) -> bool {
    matches!(artifact.state, ArtifactState::Ready { .. })
}
pub fn is_stoppable(artifact: &ArtifactStatus) -> bool {
    matches!(
        artifact.state,
        ArtifactState::Downloading { .. } | ArtifactState::Queued
    )
}

/// The declared size of every artifact, or None when one is unknown.
pub fn total_bytes<'a>(artifacts: impl IntoIterator<Item = &'a ArtifactStatus>) -> Option<f64> {
    let mut sum = 0.0;
    for artifact in artifacts {
        sum += artifact.bytes_total? as f64;
    }
    Some(sum)
}

/// korrid's words, except where they are not words a person reads.
fn plain_reason(failure: &RpcFailure, game: &DownloadGameRef) -> String {
    match failure.code.as_str() {
        "ActiveSessionConflict" => format!(
            "{} is running. Stop it first, then remove the download.",
            game.title
        ),
        "BrainUnreachable" => "Korri did not answer. Try again in a moment.".into(),
        _ => failure.message.clone(),
    }
}

#[derive(Default)]
struct Inner {
    membership: BTreeMap<String, Vec<Member>>,
    states: HashMap<String, ArtifactStatus>,
    batches: HashMap<String, BTreeSet<String>>,
    learning: HashSet<String>,
    unresolved: HashSet<String>,
    pending: HashSet<String>,
    requested: BTreeSet<String>,
    facts: BTreeMap<String, GameDownloadFacts>,
    problem: Option<SurfaceStatus>,
    last_ids: Vec<String>,
    complete: bool,
    refresh_seq: u64,
    refreshing: u32,
    timer: bool,
}

pub type HeldGames = Arc<dyn Fn() -> Vec<String> + Send + Sync>;

pub struct GameDownloads {
    korrid: Korrid,
    inner: Mutex<Inner>,
    notify: Notify,
    held_games: Mutex<Option<HeldGames>>,
    poll_interval: Duration,
}

impl Inner {
    fn absorb(&mut self, artifacts: &[ArtifactStatus], game_id: Option<&str>) {
        for artifact in artifacts {
            self.states.insert(artifact.id.clone(), artifact.clone());
        }
        if let Some(game_id) = game_id {
            self.membership.insert(
                game_id.to_owned(),
                artifacts
                    .iter()
                    .map(|artifact| Member {
                        id: artifact.id.clone(),
                        release: artifact.release == Some(true),
                    })
                    .collect(),
            );
        }
    }

    fn other_pending(&self, game_id: &str, held: &[String]) -> Vec<String> {
        let mine: HashSet<&str> = self
            .membership
            .get(game_id)
            .into_iter()
            .flatten()
            .map(|member| member.id.as_str())
            .collect();
        self.membership
            .iter()
            .filter(|(other, members)| {
                other.as_str() != game_id
                    && (self.requested.contains(*other)
                        || held.contains(other)
                        || members.iter().any(|member| {
                            self.states.get(&member.id).is_some_and(|status| {
                                is_active(status) && !mine.contains(member.id.as_str())
                            })
                        }))
            })
            .map(|(other, _)| other.clone())
            .collect()
    }

    /// What Cancel on `game_id` stops: everything of its own release that is
    /// queued or downloading, and any other queued or downloading artifact no
    /// other pending download needs.
    fn cancel_targets(&self, game_id: &str, held: &[String]) -> (Vec<String>, bool) {
        let needed_elsewhere: HashSet<String> = self
            .other_pending(game_id, held)
            .iter()
            .flat_map(|other| self.membership.get(other).into_iter().flatten())
            .map(|member| member.id.clone())
            .collect();
        let stoppable: Vec<&Member> = self
            .membership
            .get(game_id)
            .into_iter()
            .flatten()
            .filter(|member| self.states.get(&member.id).is_some_and(is_stoppable))
            .collect();
        let targets: Vec<String> = stoppable
            .iter()
            .filter(|member| member.release || !needed_elsewhere.contains(&member.id))
            .map(|member| member.id.clone())
            .collect();
        let shared = targets.len() < stoppable.len();
        (targets, shared)
    }

    fn facts_for(
        &mut self,
        game_id: &str,
        members: &[Member],
        held: &[String],
    ) -> GameDownloadFacts {
        let artifacts: Vec<ArtifactStatus> = members
            .iter()
            .filter_map(|member| self.states.get(&member.id).cloned())
            .collect();
        let missing: Vec<&ArtifactStatus> = artifacts
            .iter()
            .filter(|artifact| !is_ready(artifact))
            .collect();
        let active = artifacts.iter().any(is_active);
        let batch = if !active {
            self.batches.remove(game_id);
            None
        } else {
            let batch = self.batches.entry(game_id.to_owned()).or_default();
            for artifact in &missing {
                batch.insert(artifact.id.clone());
            }
            Some(batch.clone())
        };
        let downloading = artifacts
            .iter()
            .any(|artifact| matches!(artifact.state, ArtifactState::Downloading { .. }));
        let unpacking = artifacts
            .iter()
            .any(|artifact| matches!(artifact.state, ArtifactState::Unpacking));
        let progress = match (&batch, downloading, unpacking) {
            (Some(batch), true, _) => {
                // The whole is what was missing when this download began, so
                // the bar only moves forward as each artifact finishes.
                let whole: Vec<&ArtifactStatus> =
                    batch.iter().filter_map(|id| self.states.get(id)).collect();
                let total = total_bytes(whole.iter().copied());
                let done = whole
                    .iter()
                    .fold(0.0, |sum, artifact| match &artifact.state {
                        ArtifactState::Downloading { bytes_done, .. } => sum + *bytes_done as f64,
                        ArtifactState::Ready { .. } | ArtifactState::Unpacking => {
                            sum + artifact.bytes_total.unwrap_or(0) as f64
                        }
                        _ => sum,
                    });
                Some(SurfaceProgress { done, total })
            }
            // Unpacking has no position to show and no known end.
            (_, _, true) => Some(SurfaceProgress {
                done: 0.0,
                total: None,
            }),
            _ => None,
        };
        let failed = artifacts.iter().find_map(|artifact| match &artifact.state {
            ArtifactState::Failed { reason } => Some(reason.message.clone()),
            _ => None,
        });
        let bytes = total_bytes(missing.iter().copied());
        let (targets, shared) = self.cancel_targets(game_id, held);
        GameDownloadFacts {
            needs: !missing.is_empty(),
            bytes,
            active,
            stoppable: !targets.is_empty(),
            shared,
            queued: active && !downloading && !unpacking,
            progress,
            own_ready: members.iter().any(|member| {
                member.release
                    && self
                        .states
                        .get(&member.id)
                        .is_some_and(|status| matches!(status.state, ArtifactState::Ready { .. }))
            }),
            failure: failed.filter(|_| !active),
        }
    }

    fn any_active(&self) -> bool {
        self.states.values().any(is_active)
    }
}

impl GameDownloads {
    pub fn new(korrid: Korrid, notify: Notify, poll_interval: Duration) -> Arc<Self> {
        Arc::new(Self {
            korrid,
            inner: Mutex::new(Inner::default()),
            notify,
            held_games: Mutex::new(None),
            poll_interval,
        })
    }

    /// korrid ids of games a launch gate is fetching for right now.
    pub fn set_held_games(&self, held: HeldGames) {
        *self.held_games.lock().expect("downloads poisoned") = Some(held);
    }

    fn held(&self) -> Vec<String> {
        let held = self.held_games.lock().expect("downloads poisoned").clone();
        held.map(|held| held()).unwrap_or_default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("downloads poisoned")
    }

    /// korrid game id -> facts, for every game whose artifacts are known.
    pub fn facts(&self) -> BTreeMap<String, GameDownloadFacts> {
        self.lock().facts.clone()
    }
    /// A Problem from a download action, until dismissed.
    pub fn status(&self) -> Option<SurfaceStatus> {
        self.lock().problem.clone()
    }
    /// An action for this game was sent and korrid has not answered.
    pub fn is_pending(&self, game_id: &str) -> bool {
        self.lock().pending.contains(game_id)
    }

    fn publish(&self) {
        let held = self.held();
        {
            let mut inner = self.lock();
            // A download asked for ends when nothing of it is under way.
            let done: Vec<String> = inner
                .requested
                .iter()
                .filter(|game_id| {
                    !inner
                        .membership
                        .get(*game_id)
                        .into_iter()
                        .flatten()
                        .any(|member| inner.states.get(&member.id).is_some_and(is_active))
                })
                .cloned()
                .collect();
            for game_id in done {
                inner.requested.remove(&game_id);
            }
            let membership = inner.membership.clone();
            let mut next = BTreeMap::new();
            for (game_id, members) in &membership {
                let facts = inner.facts_for(game_id, members, &held);
                next.insert(game_id.clone(), facts);
            }
            inner.facts = next;
        }
        (self.notify)();
    }

    fn schedule_poll(self: &Arc<Self>) {
        {
            let mut inner = self.lock();
            if inner.timer || !inner.any_active() {
                return;
            }
            inner.timer = true;
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(this.poll_interval).await;
            this.lock().timer = false;
            this.tick().await;
        });
    }

    /// One device-wide read, folded through the known membership.
    async fn tick(self: &Arc<Self>) {
        if self.lock().refreshing > 0 {
            self.schedule_poll();
            return;
        }
        let all = self.korrid.acquisition_status(None, None).await;
        if let AcquisitionOutcome::Ok(payload) = all {
            let incomplete = {
                let mut inner = self.lock();
                inner.absorb(&payload.artifacts, None);
                !inner.complete && !payload.artifacts.iter().all(is_ready)
            };
            if incomplete {
                Box::pin(self.refresh(None)).await;
                return;
            }
        }
        self.publish();
        self.schedule_poll();
    }

    /// Re-read what the given games (or the last given) need.
    pub async fn refresh(self: &Arc<Self>, game_ids: Option<Vec<String>>) {
        let (ids, seq) = {
            let mut inner = self.lock();
            if let Some(game_ids) = game_ids {
                inner.last_ids = game_ids;
            }
            inner.refresh_seq += 1;
            inner.unresolved.clear();
            inner.refreshing += 1;
            (inner.last_ids.clone(), inner.refresh_seq)
        };
        self.refresh_inner(ids, seq).await;
        self.lock().refreshing -= 1;
    }

    async fn refresh_inner(self: &Arc<Self>, ids: Vec<String>, seq: u64) {
        let AcquisitionOutcome::Ok(all) = self.korrid.acquisition_status(None, None).await else {
            return;
        };
        if self.lock().refresh_seq != seq {
            return;
        }
        self.lock().absorb(&all.artifacts, None);
        // Only when something on the device is not Ready can any game need a
        // download, so the common steady state costs one read.
        if !all.artifacts.iter().all(is_ready) && !ids.is_empty() {
            for slice in ids.chunks(READ_CONCURRENCY) {
                let results = futures::future::join_all(
                    slice
                        .iter()
                        .map(|id| self.korrid.acquisition_status(Some(id.clone()), None)),
                )
                .await;
                let mut inner = self.lock();
                if inner.refresh_seq != seq {
                    return;
                }
                for (id, result) in slice.iter().zip(results) {
                    if let AcquisitionOutcome::Ok(payload) = result {
                        inner.absorb(&payload.artifacts, Some(id));
                    }
                }
            }
        }
        {
            let mut inner = self.lock();
            inner.complete = ids.iter().all(|id| inner.membership.contains_key(id));
        }
        self.publish();
        self.schedule_poll();
    }

    /// Read one game's artifacts if they are not known yet. Answers later.
    pub fn learn(self: &Arc<Self>, game_id: &str) {
        {
            let mut inner = self.lock();
            if inner.membership.contains_key(game_id)
                || inner.learning.contains(game_id)
                || inner.unresolved.contains(game_id)
            {
                return;
            }
            inner.learning.insert(game_id.to_owned());
        }
        let this = Arc::clone(self);
        let game_id = game_id.to_owned();
        tokio::spawn(async move {
            let result = this
                .korrid
                .acquisition_status(Some(game_id.clone()), None)
                .await;
            {
                let mut inner = this.lock();
                inner.learning.remove(&game_id);
                match &result {
                    AcquisitionOutcome::Ok(payload) => {
                        inner.absorb(&payload.artifacts, Some(&game_id))
                    }
                    AcquisitionOutcome::Err(_) => {
                        inner.unresolved.insert(game_id);
                        return;
                    }
                }
            }
            this.publish();
            this.schedule_poll();
        });
    }

    /// Run one action against korrid for a game, at most one at a time.
    fn act<F, Fut>(self: &Arc<Self>, game: DownloadGameRef, kicker: String, run: F)
    where
        F: FnOnce(Arc<Self>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Option<RpcFailure>> + Send + 'static,
    {
        if !self.lock().pending.insert(game.id.clone()) {
            return;
        }
        self.publish();
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let failure = run(Arc::clone(&this)).await;
            {
                let mut inner = this.lock();
                inner.pending.remove(&game.id);
                if let Some(failure) = failure {
                    inner.problem = Some(SurfaceStatus::Problem {
                        kicker,
                        reason: plain_reason(&failure, &game),
                        can_retry: false,
                        game_id: Some(game.surface_id.clone()),
                        game_title: Some(game.title.clone()),
                    });
                }
            }
            this.publish();
            this.schedule_poll();
        });
    }

    pub fn download(self: &Arc<Self>, game: DownloadGameRef) {
        let kicker = format!("Couldn't download {}", game.title);
        let id = game.id.clone();
        self.act(game, kicker, move |this| async move {
            match this.korrid.acquisition_download(id.clone()).await {
                AcquisitionOutcome::Err(failure) => Some(failure),
                AcquisitionOutcome::Ok(payload) => {
                    let mut inner = this.lock();
                    inner.absorb(&payload.artifacts, Some(&id));
                    inner.requested.insert(id);
                    None
                }
            }
        });
    }

    /// Cancel stops this game's own files and what only it is waiting for;
    /// korrid deletes the partial files. A runtime another pending download
    /// needs goes on, and unpacking finishes on its own.
    pub fn cancel(self: &Arc<Self>, game: DownloadGameRef) {
        let id = game.id.clone();
        self.act(
            game,
            "Couldn't cancel the download".into(),
            move |this| async move {
                let held = this.held();
                let targets = {
                    let mut inner = this.lock();
                    inner.requested.remove(&id);
                    inner.cancel_targets(&id, &held).0
                };
                for target in targets {
                    match this.korrid.acquisition_cancel(target).await {
                        AcquisitionOutcome::Err(failure) => return Some(failure),
                        AcquisitionOutcome::Ok(payload) => {
                            this.lock().absorb(&payload.artifacts, None)
                        }
                    }
                }
                if let AcquisitionOutcome::Ok(payload) =
                    this.korrid.acquisition_status(Some(id.clone()), None).await
                {
                    this.lock().absorb(&payload.artifacts, Some(&id));
                }
                None
            },
        );
    }

    pub fn remove(self: &Arc<Self>, game: DownloadGameRef) {
        let id = game.id.clone();
        self.act(
            game,
            "Couldn't remove the download".into(),
            move |this| async move {
                match this.korrid.acquisition_remove(id.clone()).await {
                    AcquisitionOutcome::Err(failure) => Some(failure),
                    AcquisitionOutcome::Ok(payload) => {
                        this.lock().absorb(&payload.artifacts, Some(&id));
                        None
                    }
                }
            },
        );
    }

    /// The surface's Dismiss. True when a download problem was showing.
    pub fn dismiss(&self) -> bool {
        if self.lock().problem.take().is_none() {
            return false;
        }
        self.publish();
        true
    }
}
