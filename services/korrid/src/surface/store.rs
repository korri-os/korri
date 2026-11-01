//! Port of `clients/portal/src/surface/store-acquisition.ts`: getting a game
//! from a provider (ADR 0016). The providers, one search, one open entry, and
//! every download a person started. korrid owns the job, the download and its
//! staging file; this owns the question before a download, the observation
//! and the words. Every action id carries the version of what it answers, so
//! a press that lands after the state moved on sends nothing.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use korri_surface_treaty::{
    SurfaceAction, SurfaceProgress, SurfaceStore, SurfaceStoreDetails, SurfaceStoreEntry,
    SurfaceStoreGet, SurfaceStoreProvider, SurfaceStoreProviders, SurfaceStoreSearch, SurfaceTrue,
};

use super::{
    acquisition_gate::format_eta,
    korrid::{ask, Korrid},
    shared_play::format_bytes,
    Notify,
};
use crate::acquisition::{
    claims::{ProviderClaim, ProviderClaimDetails},
    AcquireArtifactRequest, AcquireCancelRequest, AcquireJobOutcome, AcquireJobState,
    AcquireJobStatus, AcquireStatusRequest, AcquisitionPluginsPayload, AcquisitionProvider,
    DetailsOutcome, DetailsRequest, PluginListOutcome, SearchOutcome, SearchRequest,
};

/// Unreachable reads tolerated in a row while watching a job.
pub const LOST_AFTER_POLLS: u32 = 30;
pub const NO_SYSTEM_REASON: &str =
    "This provider doesn't say which system this game is for, so it can't be downloaded yet.";
pub const STOPPED_KICKER: &str = "Download stopped";
pub const STOPPED_REASON: &str = "Korri restarted before the download finished. Nothing was kept, and your library is unchanged.";
pub const CANCELLED_NOTE: &str = "Download cancelled. Nothing was kept.";
pub const NOT_AVAILABLE_KICKER: &str = "Not available";
pub const QUEUED_DETAIL: &str = "Waiting for the download before it";

const UNKNOWN_JOB: &str = "AcquisitionUnknownJob";
const UNKNOWN_CLAIM: &str = "AcquisitionUnknownClaim";
const SYSTEM_UNKNOWN: &str = "AcquisitionSystemUnknown";
const NOT_FINAL: &str = "DownloadNotFinal";
const RESOLUTION_FAILED: &str = "DownloadResolutionFailed";

fn unavailable_code(code: &str) -> bool {
    code == SYSTEM_UNKNOWN || code == NOT_FINAL
}

/// The opaque id a surface hands back for one claim.
pub fn store_entry_id(provider_id: &str, id: &str) -> String {
    serde_json::to_string(&(provider_id, id)).expect("entry id serializes")
}

fn key_of(claim: &ProviderClaim) -> String {
    store_entry_id(&claim.provider_id, &claim.id)
}

#[derive(Clone, Debug)]
enum Job {
    Confirming,
    Starting {
        cancel_requested: bool,
    },
    Active {
        job_id: String,
        state: AcquireJobState,
        cancelling: bool,
    },
    Imported {
        game_id: String,
        seen: bool,
    },
    Failed {
        kicker: String,
        reason: String,
        retry: bool,
    },
    Unavailable {
        reason: String,
    },
    Cancelled,
}

#[derive(Clone)]
struct JobSlot {
    claim: ProviderClaim,
    job: Job,
    version: u64,
}

#[derive(Clone)]
enum Providers {
    Loading,
    Problem { message: String, version: u64 },
    Ready(Vec<AcquisitionProvider>),
}

#[derive(Clone)]
enum Search {
    Idle,
    Searching {
        provider_id: String,
        query: String,
    },
    Problem {
        provider_id: String,
        query: String,
        message: String,
        version: u64,
    },
    Ready {
        provider_id: String,
        query: String,
        claims: Vec<ProviderClaim>,
    },
}

// One value per store, replaced whole on each step: variant size is moot.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
enum Details {
    Closed,
    Loading(ProviderClaim),
    Problem(ProviderClaim, String, u64),
    Ready(ProviderClaim, ProviderClaimDetails),
}

fn entry_from(claim: &ProviderClaim) -> SurfaceStoreEntry {
    SurfaceStoreEntry {
        id: key_of(claim),
        title: claim.title.clone(),
        system: claim.platform.clone().or_else(|| {
            claim
                .artifact
                .as_ref()
                .and_then(|artifact| artifact.system.clone())
        }),
    }
}

/// "612 KB of 1.7 MB · 48 KB/s · 0:23 left", with unknown parts left out.
pub fn store_download_detail(done: u64, total: Option<u64>, per_second: Option<f64>) -> String {
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

fn job_from(status: AcquireJobStatus, cancelling: bool) -> Job {
    match status.state {
        AcquireJobState::Imported { game_id } => Job::Imported {
            game_id,
            seen: false,
        },
        AcquireJobState::Cancelled => Job::Cancelled,
        AcquireJobState::Failed { reason } => {
            if unavailable_code(&reason.code) {
                Job::Unavailable {
                    reason: reason.message,
                }
            } else {
                // The provider said it cannot give the file now; it may later.
                Job::Failed {
                    kicker: if reason.code == RESOLUTION_FAILED {
                        NOT_AVAILABLE_KICKER
                    } else {
                        "Download failed"
                    }
                    .into(),
                    reason: reason.message,
                    retry: true,
                }
            }
        }
        state => Job::Active {
            job_id: status.job_id,
            state,
            cancelling,
        },
    }
}

/// The korrid id of the playable game this device holds for a claim.
pub type OwnedGame = Arc<dyn Fn(&str, &str) -> Option<String> + Send + Sync>;
pub type Imported = Arc<dyn Fn(String) + Send + Sync>;

struct Inner {
    version: u64,
    providers: Providers,
    providers_seq: u64,
    search: Search,
    search_seq: u64,
    details: Details,
    details_seq: u64,
    jobs: HashMap<String, JobSlot>,
    watchers: HashMap<String, u64>,
}

pub struct StoreAcquisition {
    korrid: Korrid,
    inner: Mutex<Inner>,
    notify: Notify,
    owned_game: OwnedGame,
    on_imported: Imported,
    poll_interval: Duration,
}

fn action(id: String, label: &str) -> SurfaceAction {
    SurfaceAction::new(format!("store:{id}"), label, true)
}

impl StoreAcquisition {
    pub fn new(
        korrid: Korrid,
        notify: Notify,
        owned_game: OwnedGame,
        on_imported: Imported,
        poll_interval: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            korrid,
            inner: Mutex::new(Inner {
                version: 0,
                providers: Providers::Loading,
                providers_seq: 0,
                search: Search::Idle,
                search_seq: 0,
                details: Details::Closed,
                details_seq: 0,
                jobs: HashMap::new(),
                watchers: HashMap::new(),
            }),
            notify,
            owned_game,
            on_imported,
            poll_interval,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("store poisoned")
    }

    fn next(inner: &mut Inner) -> u64 {
        inner.version += 1;
        inner.version
    }

    fn get_view(
        &self,
        inner: &Inner,
        claim: &ProviderClaim,
        details: &ProviderClaimDetails,
    ) -> SurfaceStoreGet {
        let key = key_of(claim);
        let slot = inner.jobs.get(&key);
        let job_action = |kind: &str, label: &str| {
            action(
                format!("{}:{kind}:{key}", slot.map_or(0, |slot| slot.version)),
                label,
            )
        };
        let job = slot.map(|slot| &slot.job);
        let artifact = details.artifact.as_ref().or(claim.artifact.as_ref());
        // The library's fact outranks any job that has ended: an owned cart is Play.
        let owned = (self.owned_game)(&claim.provider_id, &claim.id);
        if let Some(owned) = owned {
            if matches!(
                job,
                None | Some(Job::Cancelled | Job::Failed { .. } | Job::Imported { .. })
            ) {
                return SurfaceStoreGet::InLibrary {
                    detail: Some("In your library".into()),
                    game_id: Some(owned),
                };
            }
        }
        let working = |kicker: &str, detail: Option<&str>, actions: Vec<SurfaceAction>| {
            SurfaceStoreGet::Working {
                kicker: kicker.into(),
                detail: detail.map(Into::into),
                actions,
                queued: None,
                progress: None,
            }
        };
        let Some(job) = job else {
            if artifact
                .and_then(|artifact| artifact.system.as_ref())
                .is_none()
            {
                return SurfaceStoreGet::Unavailable {
                    reason: NO_SYSTEM_REASON.into(),
                };
            }
            return SurfaceStoreGet::Offer {
                note: None,
                actions: vec![job_action("get", "Get")],
            };
        };
        match job {
            Job::Cancelled => SurfaceStoreGet::Offer {
                note: Some(CANCELLED_NOTE.into()),
                actions: vec![job_action("get", "Get")],
            },
            Job::Confirming => {
                let bytes = artifact
                    .and_then(|artifact| artifact.file.as_ref())
                    .and_then(|file| file.size_bytes);
                SurfaceStoreGet::Confirm {
                    kicker: "Download to this device".into(),
                    detail: Some(format!(
                        "{}, {}",
                        details.title,
                        bytes.map_or("size unknown".into(), |bytes| format_bytes(bytes as f64))
                    )),
                    actions: vec![
                        job_action("download", "Download"),
                        job_action("cancel", "Cancel"),
                    ],
                }
            }
            Job::Starting {
                cancel_requested: true,
            }
            | Job::Active {
                cancelling: true, ..
            } => working(
                "Cancelling download…",
                Some("Nothing will be kept"),
                Vec::new(),
            ),
            Job::Starting { .. } => working(
                "Starting download…",
                Some("Asking the provider for the file"),
                vec![job_action("cancel", "Cancel")],
            ),
            Job::Active { state, .. } => match state {
                AcquireJobState::Downloading {
                    bytes_done,
                    bytes_total,
                    bytes_per_second,
                } => SurfaceStoreGet::Working {
                    kicker: format!("Downloading {}", details.title),
                    detail: Some(store_download_detail(
                        *bytes_done,
                        *bytes_total,
                        *bytes_per_second,
                    )),
                    actions: vec![job_action("cancel", "Cancel")],
                    queued: None,
                    progress: Some(SurfaceProgress {
                        done: *bytes_done as f64,
                        total: bytes_total.map(|total| total as f64),
                    }),
                },
                // korrid downloads one thing at a time: this one waits its turn.
                AcquireJobState::Queued => SurfaceStoreGet::Working {
                    kicker: "Waiting to download".into(),
                    detail: Some(QUEUED_DETAIL.into()),
                    actions: vec![job_action("cancel", "Cancel")],
                    queued: Some(SurfaceTrue),
                    progress: None,
                },
                // ADR 0030: korrid unpacks an archive; it can still be cancelled.
                AcquireJobState::Unpacking => working(
                    "Unpacking the download…",
                    None,
                    vec![job_action("cancel", "Cancel")],
                ),
                // Placing has no position and no Cancel.
                AcquireJobState::Placing => working("Adding to your library…", None, Vec::new()),
                _ => working(
                    "Starting download…",
                    Some("Asking the provider for the file"),
                    vec![job_action("cancel", "Cancel")],
                ),
            },
            Job::Imported { game_id, .. } => SurfaceStoreGet::InLibrary {
                detail: Some("In your library".into()),
                game_id: Some(game_id.clone()),
            },
            Job::Failed {
                kicker,
                reason,
                retry,
            } => SurfaceStoreGet::Problem {
                kicker: kicker.clone(),
                reason: reason.clone(),
                actions: if *retry {
                    vec![job_action("retry", "Retry")]
                } else {
                    Vec::new()
                },
            },
            Job::Unavailable { reason } => SurfaceStoreGet::Unavailable {
                reason: reason.clone(),
            },
        }
    }

    /// `InLibrary.gameId` is korrid's game id here; the root maps it to the
    /// surface's id once the catalog holds it.
    pub fn snapshot(&self) -> SurfaceStore {
        let inner = self.lock();
        let providers = match &inner.providers {
            Providers::Loading => SurfaceStoreProviders::Loading,
            Providers::Problem { message, version } => SurfaceStoreProviders::Problem {
                message: message.clone(),
                actions: vec![action(format!("{version}:providers-retry"), "Retry")],
            },
            Providers::Ready(providers) => SurfaceStoreProviders::Ready {
                providers: providers
                    .iter()
                    .map(|provider| SurfaceStoreProvider {
                        id: provider.id.clone(),
                        title: provider.title.clone(),
                    })
                    .collect(),
            },
        };
        let search = match &inner.search {
            Search::Idle => SurfaceStoreSearch::Idle,
            Search::Searching { provider_id, query } => SurfaceStoreSearch::Searching {
                provider_id: provider_id.clone(),
                query: query.clone(),
            },
            Search::Problem {
                provider_id,
                query,
                message,
                version,
            } => SurfaceStoreSearch::Problem {
                provider_id: provider_id.clone(),
                query: query.clone(),
                message: message.clone(),
                actions: vec![action(format!("{version}:search-retry"), "Retry")],
            },
            Search::Ready {
                provider_id,
                query,
                claims,
            } => SurfaceStoreSearch::Ready {
                provider_id: provider_id.clone(),
                query: query.clone(),
                entries: claims.iter().map(entry_from).collect(),
            },
        };
        let details = match &inner.details {
            Details::Closed => SurfaceStoreDetails::Closed,
            Details::Loading(claim) => SurfaceStoreDetails::Loading {
                entry: entry_from(claim),
            },
            Details::Problem(claim, message, version) => SurfaceStoreDetails::Problem {
                entry: entry_from(claim),
                message: message.clone(),
                actions: vec![action(format!("{version}:details-retry"), "Retry")],
            },
            Details::Ready(claim, details) => SurfaceStoreDetails::Ready {
                entry: SurfaceStoreEntry {
                    title: details.title.clone(),
                    ..entry_from(claim)
                },
                description: details.description.clone(),
                get: self.get_view(&inner, claim, details),
            },
        };
        SurfaceStore {
            providers,
            search,
            details,
        }
    }

    fn publish(&self) {
        (self.notify)();
    }

    fn set_job(&self, key: &str, claim: &ProviderClaim, job: Job) {
        {
            let mut inner = self.lock();
            let version = Self::next(&mut inner);
            inner.jobs.insert(
                key.into(),
                JobSlot {
                    claim: claim.clone(),
                    job,
                    version,
                },
            );
        }
        self.publish();
    }

    fn settle(&self, key: &str, claim: &ProviderClaim, job: Job) {
        self.lock().watchers.remove(key);
        let imported = match &job {
            Job::Imported { game_id, .. } => Some(game_id.clone()),
            _ => None,
        };
        self.set_job(key, claim, job);
        if let Some(game_id) = imported {
            (self.on_imported)(game_id);
        }
    }

    fn job(&self, key: &str) -> Option<Job> {
        self.lock().jobs.get(key).map(|slot| slot.job.clone())
    }

    async fn watch(self: Arc<Self>, key: String, claim: ProviderClaim, job_id: String) {
        let token = {
            let mut inner = self.lock();
            let token = Self::next(&mut inner);
            inner.watchers.insert(key.clone(), token);
            token
        };
        let mine = |this: &Self| this.lock().watchers.get(&key) == Some(&token);
        let mut unreachable = 0;
        while mine(&self) {
            tokio::time::sleep(self.poll_interval).await;
            if !mine(&self) || !matches!(self.job(&key), Some(Job::Active { .. })) {
                return;
            }
            let result = ask!(
                self.korrid,
                Unreachable,
                AcquisitionAcquireStatus,
                AcquireStatusRequest {
                    job_id: job_id.clone()
                },
                AcquireJobOutcome
            );
            let Some(Job::Active {
                state: now_state,
                cancelling,
                ..
            }) = self.job(&key)
            else {
                return;
            };
            if !mine(&self) {
                return;
            }
            match result {
                AcquireJobOutcome::Err(failure) => {
                    // korrid restarted: the job is gone, and with it any partial file.
                    if failure.code == UNKNOWN_JOB {
                        self.settle(
                            &key,
                            &claim,
                            Job::Failed {
                                kicker: STOPPED_KICKER.into(),
                                reason: STOPPED_REASON.into(),
                                retry: true,
                            },
                        );
                        return;
                    }
                    if failure.code == "BrainUnreachable" {
                        unreachable += 1;
                        if unreachable < LOST_AFTER_POLLS {
                            continue;
                        }
                    }
                    self.settle(
                        &key,
                        &claim,
                        Job::Failed {
                            kicker: "Lost sight of the download".into(),
                            reason: failure.message,
                            retry: true,
                        },
                    );
                    return;
                }
                AcquireJobOutcome::Ok(status) => {
                    unreachable = 0;
                    let job = job_from(status, cancelling);
                    match &job {
                        Job::Active { state, .. } => {
                            // Only a change is worth a new model.
                            if serde_json::to_value(state).ok()
                                != serde_json::to_value(&now_state).ok()
                            {
                                self.set_job(&key, &claim, job);
                            }
                        }
                        _ => {
                            self.settle(&key, &claim, job);
                            return;
                        }
                    }
                }
            }
        }
    }

    async fn cancel_job(
        self: Arc<Self>,
        key: String,
        claim: ProviderClaim,
        job_id: String,
        state: AcquireJobState,
    ) {
        self.lock().watchers.remove(&key);
        self.set_job(
            &key,
            &claim,
            Job::Active {
                job_id: job_id.clone(),
                state,
                cancelling: true,
            },
        );
        let result = ask!(
            self.korrid,
            Unreachable,
            AcquisitionAcquireCancel,
            AcquireCancelRequest { job_id },
            AcquireJobOutcome
        );
        if !matches!(self.job(&key), Some(Job::Active { .. })) {
            return;
        }
        match result {
            AcquireJobOutcome::Err(failure) => {
                if failure.code == UNKNOWN_JOB {
                    self.settle(&key, &claim, Job::Cancelled);
                } else {
                    self.settle(
                        &key,
                        &claim,
                        Job::Failed {
                            kicker: "Couldn't cancel the download".into(),
                            reason: failure.message,
                            retry: false,
                        },
                    );
                }
            }
            // One that finished first keeps its outcome; one still moving is cancelled.
            AcquireJobOutcome::Ok(status) => {
                let job = job_from(status, false);
                let job = if matches!(job, Job::Active { .. }) {
                    Job::Cancelled
                } else {
                    job
                };
                self.settle(&key, &claim, job);
            }
        }
    }

    async fn start(self: Arc<Self>, key: String, claim: ProviderClaim) {
        self.set_job(
            &key,
            &claim,
            Job::Starting {
                cancel_requested: false,
            },
        );
        let request = || AcquireArtifactRequest {
            provider_id: claim.provider_id.clone(),
            id: claim.id.clone(),
            file_name: None,
        };
        let mut result = ask!(
            self.korrid,
            Unreachable,
            AcquisitionAcquire,
            request(),
            AcquireJobOutcome
        );
        // korrid holds claims in memory: after a restart it holds this one
        // again once details returns it. Read it once, then ask again.
        if matches!(&result, AcquireJobOutcome::Err(failure) if failure.code == UNKNOWN_CLAIM)
            && matches!(self.job(&key), Some(Job::Starting { .. }))
        {
            let reread = ask!(
                self.korrid,
                Unreachable,
                AcquisitionDetails,
                DetailsRequest {
                    provider_id: claim.provider_id.clone(),
                    id: claim.id.clone()
                },
                DetailsOutcome
            );
            result = match reread {
                DetailsOutcome::Ok(_) => ask!(
                    self.korrid,
                    Unreachable,
                    AcquisitionAcquire,
                    request(),
                    AcquireJobOutcome
                ),
                DetailsOutcome::Err(failure) => AcquireJobOutcome::Err(failure),
            };
        }
        let Some(Job::Starting { cancel_requested }) = self.job(&key) else {
            return;
        };
        match result {
            AcquireJobOutcome::Err(failure) => {
                if cancel_requested {
                    self.set_job(&key, &claim, Job::Cancelled);
                } else if unavailable_code(&failure.code) {
                    self.set_job(
                        &key,
                        &claim,
                        Job::Unavailable {
                            reason: failure.message,
                        },
                    );
                } else {
                    self.set_job(
                        &key,
                        &claim,
                        Job::Failed {
                            kicker: "Couldn't start the download".into(),
                            reason: failure.message,
                            retry: true,
                        },
                    );
                }
            }
            AcquireJobOutcome::Ok(status) => match job_from(status, false) {
                Job::Active { job_id, state, .. } => {
                    // A Cancel pressed while the acquire was unanswered.
                    if cancel_requested {
                        Arc::clone(&self)
                            .cancel_job(key, claim, job_id, state)
                            .await;
                        return;
                    }
                    self.set_job(
                        &key,
                        &claim,
                        Job::Active {
                            job_id: job_id.clone(),
                            state,
                            cancelling: false,
                        },
                    );
                    tokio::spawn(Arc::clone(&self).watch(key, claim, job_id));
                }
                // Already in the library, or finished.
                job => self.settle(&key, &claim, job),
            },
        }
    }

    async fn load_providers(self: Arc<Self>) {
        let seq = {
            let mut inner = self.lock();
            inner.providers_seq += 1;
            inner.providers = Providers::Loading;
            inner.providers_seq
        };
        self.publish();
        let result = ask!(
            self.korrid,
            Unreachable,
            AcquisitionProviders,
            AcquisitionPluginsPayload {},
            PluginListOutcome
        );
        {
            let mut inner = self.lock();
            if seq != inner.providers_seq {
                return;
            }
            inner.providers = match result {
                PluginListOutcome::Ok(list) => Providers::Ready(list.providers),
                PluginListOutcome::Err(failure) => {
                    let version = Self::next(&mut inner);
                    Providers::Problem {
                        message: failure.message,
                        version,
                    }
                }
            };
        }
        self.publish();
    }

    async fn run_search(self: Arc<Self>, provider_id: String, query: String) {
        let seq = {
            let mut inner = self.lock();
            inner.search_seq += 1;
            inner.search = Search::Searching {
                provider_id: provider_id.clone(),
                query: query.clone(),
            };
            inner.search_seq
        };
        self.publish();
        let result = ask!(
            self.korrid,
            Unreachable,
            AcquisitionSearch,
            SearchRequest {
                query: query.clone(),
                provider_ids: vec![provider_id.clone()]
            },
            SearchOutcome
        );
        {
            let mut inner = self.lock();
            if seq != inner.search_seq {
                return;
            }
            inner.search = match result {
                SearchOutcome::Ok(found) => Search::Ready {
                    provider_id,
                    query,
                    claims: found.claims,
                },
                SearchOutcome::Err(failure) => {
                    let version = Self::next(&mut inner);
                    Search::Problem {
                        provider_id,
                        query,
                        message: failure.message,
                        version,
                    }
                }
            };
        }
        self.publish();
    }

    async fn load_details(self: Arc<Self>, claim: ProviderClaim) {
        let seq = {
            let mut inner = self.lock();
            inner.details_seq += 1;
            inner.details = Details::Loading(claim.clone());
            inner.details_seq
        };
        self.publish();
        let result = ask!(
            self.korrid,
            Unreachable,
            AcquisitionDetails,
            DetailsRequest {
                provider_id: claim.provider_id.clone(),
                id: claim.id.clone()
            },
            DetailsOutcome
        );
        {
            let mut inner = self.lock();
            if seq != inner.details_seq {
                return;
            }
            inner.details = match result {
                DetailsOutcome::Ok(details) => Details::Ready(claim, details),
                DetailsOutcome::Err(failure) => {
                    let version = Self::next(&mut inner);
                    Details::Problem(claim, failure.message, version)
                }
            };
        }
        self.publish();
    }

    pub fn open_store(self: &Arc<Self>) {
        tokio::spawn(Arc::clone(self).load_providers());
    }

    pub fn search(self: &Arc<Self>, provider_id: String, query: String) {
        // Only a provider Korri listed: an id from nowhere sends nothing.
        let listed = matches!(&self.lock().providers, Providers::Ready(providers) if providers.iter().any(|provider| provider.id == provider_id));
        if listed {
            tokio::spawn(Arc::clone(self).run_search(provider_id, query));
        }
    }

    pub fn open_entry(self: &Arc<Self>, entry_id: String) {
        let claim = match &self.lock().search {
            Search::Ready { claims, .. } => claims
                .iter()
                .find(|claim| key_of(claim) == entry_id)
                .cloned(),
            _ => None,
        };
        if let Some(claim) = claim {
            tokio::spawn(Arc::clone(self).load_details(claim));
        }
    }

    fn published(&self) -> Vec<String> {
        let snapshot = self.snapshot();
        let mut actions: Vec<SurfaceAction> = Vec::new();
        if let SurfaceStoreProviders::Problem { actions: list, .. } = snapshot.providers {
            actions.extend(list);
        }
        if let SurfaceStoreSearch::Problem { actions: list, .. } = snapshot.search {
            actions.extend(list);
        }
        match snapshot.details {
            SurfaceStoreDetails::Problem { actions: list, .. }
            | SurfaceStoreDetails::Ready {
                get:
                    SurfaceStoreGet::Offer { actions: list, .. }
                    | SurfaceStoreGet::Confirm { actions: list, .. }
                    | SurfaceStoreGet::Working { actions: list, .. }
                    | SurfaceStoreGet::Problem { actions: list, .. },
                ..
            } => actions.extend(list),
            _ => {}
        }
        actions
            .into_iter()
            .filter(|action| action.enabled)
            .map(|action| action.id)
            .collect()
    }

    fn act_on_job(self: &Arc<Self>, kind: &str, key: &str) {
        let open = match &self.lock().details {
            Details::Ready(claim, _) => Some(claim.clone()),
            _ => None,
        };
        let Some(open) = open.filter(|claim| key_of(claim) == key) else {
            return;
        };
        let job = self.job(key);
        match (kind, job) {
            ("get", None | Some(Job::Cancelled)) => self.set_job(key, &open, Job::Confirming),
            ("download", Some(Job::Confirming))
            | ("retry", Some(Job::Failed { retry: true, .. })) => {
                tokio::spawn(Arc::clone(self).start(key.into(), open));
            }
            ("cancel", Some(Job::Confirming)) => {
                self.lock().jobs.remove(key);
                self.publish();
            }
            (
                "cancel",
                Some(Job::Starting {
                    cancel_requested: false,
                }),
            ) => self.set_job(
                key,
                &open,
                Job::Starting {
                    cancel_requested: true,
                },
            ),
            (
                "cancel",
                Some(Job::Active {
                    job_id,
                    state,
                    cancelling: false,
                }),
            ) => {
                tokio::spawn(Arc::clone(self).cancel_job(key.into(), open, job_id, state));
            }
            _ => {}
        }
    }

    /// A `store:` action id. True when this module owns the id.
    pub fn act(self: &Arc<Self>, action_id: &str) -> bool {
        if !action_id.starts_with("store:") {
            return false;
        }
        if !self.published().iter().any(|id| id == action_id) {
            return true;
        }
        let mut parts = action_id.splitn(4, ':');
        let (_, _, kind, key) = (
            parts.next(),
            parts.next(),
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
        );
        match kind {
            "providers-retry" => {
                tokio::spawn(Arc::clone(self).load_providers());
            }
            "search-retry" => {
                if let Search::Problem {
                    provider_id, query, ..
                } = self.lock().search.clone()
                {
                    tokio::spawn(Arc::clone(self).run_search(provider_id, query));
                }
            }
            "details-retry" => {
                if let Details::Problem(claim, ..) = self.lock().details.clone() {
                    tokio::spawn(Arc::clone(self).load_details(claim));
                }
            }
            kind => self.act_on_job(kind, key),
        }
        true
    }

    /// The catalog changed: an Imported job the library no longer holds was
    /// removed, and its entry offers Get again.
    pub fn library_changed(&self) {
        {
            let mut inner = self.lock();
            let keys: Vec<String> = inner.jobs.keys().cloned().collect();
            for key in keys {
                let slot = inner.jobs[&key].clone();
                let Job::Imported { game_id, seen } = &slot.job else {
                    continue;
                };
                if (self.owned_game)(&slot.claim.provider_id, &slot.claim.id).is_some() {
                    inner.jobs.insert(
                        key,
                        JobSlot {
                            job: Job::Imported {
                                game_id: game_id.clone(),
                                seen: true,
                            },
                            ..slot
                        },
                    );
                } else if *seen {
                    inner.jobs.remove(&key);
                }
            }
        }
        self.publish();
    }
}
