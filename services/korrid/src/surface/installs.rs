//! ADR 0092 (#553): play here, install only, and the list of installs.
//!
//! On a game that only other devices hold, the person may install it here
//! and play it when it is ready ("play here"), or only install it. Korri
//! never picks it for them. korrid learns the release's size from the
//! devices that hold it when the person asks (`app.installs.manifest`),
//! never from the catalog, and offers the install only when this device
//! has a runner for the game (or a plugin offers one), enough free storage
//! and a reachable install source.
//!
//! The install manager is the truth of the line (`AcquisitionCoordinator::
//! line`, ADR 0092); this controller reads it through `InstallsAccess` and
//! keeps only what the person asked of the installs it started: play when
//! ready, and the stream-meanwhile question.
//!
//! The predicted time: korrid has no speed before the first pieces arrive,
//! so a device install measures its own. Its speed is the bytes that
//! arrived since it began installing, over the time since then, once that
//! time is at least `SPEED_SAMPLE`. A provider download reports its own
//! speed (`bytesPerSecond`). When the predicted time of a play-here install
//! is above `STREAM_MEANWHILE_ABOVE`, or nothing arrived in the sample,
//! korrid asks once whether to stream meanwhile.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use futures::future::BoxFuture;
use korri_surface_treaty::{
    SurfaceAction, SurfaceGameDownload, SurfaceInstall, SurfaceInstallBlock, SurfaceInstallHere,
    SurfaceInstallState, SurfaceInstallYield, SurfaceProgress, SurfaceStatus, SurfaceTrue,
};

use super::{
    korrid::{ask, Korrid},
    launchables::state::{Entry, LaunchablesState},
    model::PeerRelease,
    plugins::{self, PluginFacts, ReleaseTarget},
    Notify,
};
use crate::{
    acquisition::{
        pieces::{DeviceInstallState, DeviceInstallStatus},
        Member, Place,
    },
    RpcFailure,
};

/// ADR 0092: above this predicted time, play here asks whether to stream
/// meanwhile. One fixed constant.
pub const STREAM_MEANWHILE_ABOVE: Duration = Duration::from_secs(120);

/// How long a device install runs before its speed counts as measured.
pub const SPEED_SAMPLE: Duration = if cfg!(test) {
    Duration::from_millis(60)
} else {
    Duration::from_secs(5)
};

/// How often the line is read while something installs.
const POLL: Duration = if cfg!(test) {
    Duration::from_millis(10)
} else {
    Duration::from_secs(1)
};
/// How often the line is read while it is empty: another flow (a launch's
/// download, the store) can fill it.
const IDLE_POLL: Duration = if cfg!(test) {
    Duration::from_millis(10)
} else {
    Duration::from_secs(5)
};

pub const INSTALL_PLUGIN: &str = "install-here:install-plugin";
pub const CANCEL_PLUGIN: &str = "install-here:cancel";

// ---------------------------------------------------------------- korrid

/// One device that holds a release: its device key and its label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holder {
    pub device: String,
    pub label: String,
}

/// One install as korrid's line reports it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineEntry {
    pub member: Member,
    /// Absent for an install from devices: the line does not know its game.
    pub title: Option<String>,
    pub state: LineState,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LineState {
    Waiting {
        bytes: Option<u64>,
    },
    Running {
        done: u64,
        total: Option<u64>,
        /// The speed the install source measured, when it reports one.
        bytes_per_second: Option<f64>,
        /// Slowed or paused for play, and why (#556); None at full pace.
        yielding: Option<SurfaceInstallYield>,
    },
    Failed {
        reason: String,
    },
}

/// What the devices that hold a release said of it, and what this device
/// makes of it: the size and the systems whose content rules take its files.
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    pub bytes: u64,
    pub systems: Vec<String>,
}

/// korrid's install manager and install sources, read in process (lib.rs).
pub(crate) trait InstallsAccess: Send + Sync {
    /// The line, the running install first, then those that wait, then
    /// artifacts whose last try failed.
    fn line(&self) -> Vec<LineEntry>;
    /// A runner on this device plays games of this system.
    fn runs_system(&self, system: &str) -> bool;
    /// Ask the holders for the release (`app.installs.manifest`) and check
    /// the free space against its size. A refusal carries the size when the
    /// holders told it.
    fn check(
        &self,
        key: String,
        holders: Vec<Holder>,
    ) -> BoxFuture<'static, Result<Checked, (SurfaceInstallBlock, Option<u64>)>>;
    /// Start an install from devices, or join the one of the same release.
    fn start(
        &self,
        key: &str,
        title: &str,
        holders: Vec<Holder>,
        place: Place,
    ) -> Result<String, RpcFailure>;
    fn status(&self, job: &str) -> Option<DeviceInstallStatus>;
    /// Cancel an install that waits or runs, or throw away one that failed.
    fn cancel(&self, member: Member) -> BoxFuture<'static, Result<(), RpcFailure>>;
    /// Move a waiting install to `index` among those that wait.
    fn reorder(&self, member: &Member, index: usize) -> bool;
}

/// The surface's opaque id of a member of the line.
pub(crate) fn install_id(member: &Member) -> String {
    match member {
        Member::Artifact(id) => format!("artifact:{id}"),
        Member::Job(id) => format!("job:{id}"),
    }
}

pub(crate) fn member_of(install_id: &str) -> Option<Member> {
    if let Some(id) = install_id.strip_prefix("artifact:") {
        return Some(Member::Artifact(id.to_owned()));
    }
    install_id
        .strip_prefix("job:")
        .map(|id| Member::Job(id.to_owned()))
}

// ---------------------------------------------------------------- pure

/// The predicted seconds left, from a speed in bytes per second. None when
/// the speed is unknown or nothing moves.
pub fn seconds_left(done: u64, total: u64, bytes_per_second: Option<f64>) -> Option<f64> {
    let speed = bytes_per_second.filter(|speed| *speed > 0.0)?;
    Some((total.saturating_sub(done) as f64 / speed).ceil())
}

/// The speed of a device install: bytes since it began installing, over the
/// time since then, once that time is at least `SPEED_SAMPLE`.
pub fn measured_speed(since: (Instant, u64), now: (Instant, u64)) -> Option<f64> {
    let elapsed = now.0.saturating_duration_since(since.0);
    if elapsed < SPEED_SAMPLE {
        return None;
    }
    Some(now.1.saturating_sub(since.1) as f64 / elapsed.as_secs_f64())
}

/// Ask whether to stream meanwhile: a measured speed predicts more than the
/// threshold, or nothing arrived in the sample.
pub fn asks_stream_meanwhile(done: u64, total: u64, speed: Option<f64>) -> bool {
    match speed {
        None => false,
        Some(speed) if speed <= 0.0 => done < total,
        Some(speed) => seconds_left(done, total, Some(speed))
            .is_some_and(|left| left > STREAM_MEANWHILE_ABOVE.as_secs_f64()),
    }
}

/// What this device can say of a runner for a peer's game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunnerFact {
    Here,
    Plugin { plugin_id: String, title: String },
    None,
}

/// From the holder's routes (system and runner per route): a runner here
/// for one of the systems, else a plugin that offers the holder's runner.
pub fn runner_fact(
    routes: &[(String, String)],
    runs_system: impl Fn(&str) -> bool,
    facts: &PluginFacts,
) -> RunnerFact {
    if routes.iter().any(|(system, _)| runs_system(system)) {
        return RunnerFact::Here;
    }
    for (_, runner) in routes {
        // A runner id is `<plugin id>/<runner>` (plugin.rs, plugin_record_id).
        let Some((plugin_id, _)) = runner.split_once('/') else {
            continue;
        };
        if plugins::newest_release(facts, plugin_id).is_some() {
            return RunnerFact::Plugin {
                plugin_id: plugin_id.to_owned(),
                title: plugin_title(facts, plugin_id),
            };
        }
    }
    RunnerFact::None
}

fn plugin_title(facts: &PluginFacts, plugin_id: &str) -> String {
    facts
        .catalogs
        .values()
        .filter_map(|catalog| match catalog {
            plugins::CatalogView::Ready(entries) => Some(entries),
            _ => None,
        })
        .flatten()
        .find(|entry| entry.plugin_id == plugin_id)
        .and_then(|entry| entry.title.clone())
        .unwrap_or_else(|| plugin_id.to_owned())
}

// ---------------------------------------------------------------- state

#[derive(Clone, Debug, PartialEq)]
enum Check {
    Idle,
    Checking,
    Blocked {
        reason: SurfaceInstallBlock,
        bytes: Option<u64>,
    },
}

/// What korrid learned of one release that only other devices hold.
#[derive(Clone, Debug)]
struct Release {
    runner: Option<RunnerFact>,
    check: Check,
}

impl Default for Release {
    fn default() -> Self {
        Self {
            runner: None,
            check: Check::Idle,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Question {
    None,
    Asked,
    Answered,
}

/// One install from devices that this surface started.
#[derive(Clone, Debug)]
struct Job {
    key: String,
    title: String,
    /// The `SurfaceGame.id` of the game when it was started.
    game_id: String,
    play: bool,
    question: Question,
    state: DeviceInstallState,
    /// When it began installing, and the bytes it had then.
    since: Option<(Instant, u64)>,
    speed: Option<f64>,
}

#[derive(Clone, Debug)]
enum PluginPhase {
    Finding,
    Checking,
    Reviewing {
        target: ReleaseTarget,
        facts: Box<PluginFacts>,
        question: SurfaceStatus,
    },
    Installing,
}

/// Play here or install only, waiting on a plugin first (ADR 0040's flow).
#[derive(Clone, Debug)]
struct PluginFlow {
    release: PeerRelease,
    play: bool,
    title: String,
    phase: PluginPhase,
}

#[derive(Default)]
struct Inner {
    releases: BTreeMap<String, Release>,
    /// Releases whose runner is being read now.
    learning: BTreeSet<String>,
    jobs: BTreeMap<String, Job>,
    line: Vec<LineEntry>,
    plugin: Option<PluginFlow>,
    problem: Option<SurfaceStatus>,
}

impl Inner {
    fn live_job(&self, key: &str) -> Option<(&String, &Job)> {
        self.jobs.iter().find(|(_, job)| {
            job.key == key
                && matches!(
                    job.state,
                    DeviceInstallState::Waiting | DeviceInstallState::Installing { .. }
                )
        })
    }
}

/// What the controller needs of the rest of the host.
pub struct InstallsHost {
    pub state: Box<dyn Fn() -> LaunchablesState + Send + Sync>,
    pub plugin_facts: Box<dyn Fn() -> PluginFacts + Send + Sync>,
    /// A play-here install is done: start this korrid game on this device.
    pub play: Box<dyn Fn(String) + Send + Sync>,
    /// Stream play of a game now, as `LaunchGame` with this location.
    pub stream: Box<dyn Fn(String, Option<String>) + Send + Sync>,
    /// A plugin was installed, or a game: the library reads again.
    pub library_changed: Box<dyn Fn() + Send + Sync>,
    /// The person is starting something now (a launch is under way, or a
    /// game runs): a play-here install that ends then does not start.
    pub busy: Box<dyn Fn() -> bool + Send + Sync>,
}

pub struct Installs {
    korrid: Korrid,
    access: Option<Arc<dyn InstallsAccess>>,
    inner: Mutex<Inner>,
    notify: Notify,
    host: InstallsHost,
}

impl Installs {
    pub fn new(korrid: Korrid, notify: Notify, host: InstallsHost) -> Arc<Self> {
        Arc::new(Self {
            access: korrid.installs.clone(),
            korrid,
            inner: Mutex::new(Inner::default()),
            notify,
            host,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("installs poisoned")
    }

    /// This host offers installs at all.
    pub fn available(&self) -> bool {
        self.access.is_some()
    }

    // ------------------------------------------------------------ model

    /// `SurfaceGame.installHere` of one peer-only game.
    pub fn here(&self, release: &PeerRelease) -> Option<SurfaceInstallHere> {
        self.access.as_ref()?;
        let inner = self.lock();
        if let Some((id, _)) = inner.live_job(&release.key) {
            return Some(SurfaceInstallHere::Installing {
                install_id: install_id(&Member::Job(id.clone())),
            });
        }
        if inner
            .plugin
            .as_ref()
            .is_some_and(|flow| flow.release.key == release.key)
        {
            return Some(SurfaceInstallHere::Checking);
        }
        let known = inner
            .releases
            .get(&release.key)
            .cloned()
            .unwrap_or_default();
        Some(match (known.check, known.runner) {
            (Check::Checking, _) => SurfaceInstallHere::Checking,
            (Check::Blocked { reason, bytes }, _) => SurfaceInstallHere::Unavailable {
                reason,
                bytes: bytes.map(|bytes| bytes as f64),
            },
            (Check::Idle, Some(RunnerFact::None)) => SurfaceInstallHere::Unavailable {
                reason: SurfaceInstallBlock::NoRunner,
                bytes: None,
            },
            (Check::Idle, Some(RunnerFact::Plugin { title, .. })) => {
                SurfaceInstallHere::NeedsPlugin {
                    plugin_title: title,
                }
            }
            (Check::Idle, _) => SurfaceInstallHere::Offered,
        })
    }

    /// `SurfaceGame.download` of a peer-only game while its install is in
    /// the line: the same facts as any download.
    pub fn download(&self, release: &PeerRelease) -> Option<SurfaceGameDownload> {
        let inner = self.lock();
        let (_, job) = inner.live_job(&release.key)?;
        Some(match job.state {
            DeviceInstallState::Installing {
                bytes_done,
                bytes_total,
            } => SurfaceGameDownload {
                queued: None,
                progress: Some(SurfaceProgress {
                    done: bytes_done as f64,
                    total: Some(bytes_total as f64),
                }),
                bytes: Some(bytes_total.saturating_sub(bytes_done) as f64),
            },
            _ => SurfaceGameDownload {
                queued: Some(SurfaceTrue),
                progress: None,
                bytes: None,
            },
        })
    }

    /// `SurfaceModel.installs`: the line's order, then failed installs.
    pub fn list(&self, state: &LaunchablesState) -> Option<Vec<SurfaceInstall>> {
        self.access.as_ref()?;
        let inner = self.lock();
        let mut list = Vec::new();
        let mut shown: Vec<Member> = Vec::new();
        for entry in &inner.line {
            let job = match &entry.member {
                Member::Job(id) => inner.jobs.get(id),
                Member::Artifact(_) => None,
            };
            shown.push(entry.member.clone());
            let install_state = match (&entry.state, job) {
                (
                    LineState::Running {
                        done,
                        total,
                        bytes_per_second,
                        yielding,
                    },
                    job,
                ) => {
                    // A device install measures its own speed; a provider
                    // download reports one.
                    let speed = match job {
                        Some(job) => job.speed,
                        None => *bytes_per_second,
                    };
                    SurfaceInstallState::Installing {
                        progress: SurfaceProgress {
                            done: *done as f64,
                            total: total.map(|total| total as f64),
                        },
                        seconds_left: total.and_then(|total| seconds_left(*done, total, speed)),
                        yielding: *yielding,
                    }
                }
                (LineState::Waiting { bytes }, _) => SurfaceInstallState::Waiting {
                    bytes: bytes.map(|bytes| bytes as f64),
                },
                (LineState::Failed { reason }, _) => SurfaceInstallState::Failed {
                    reason: reason.clone(),
                },
            };
            list.push(SurfaceInstall {
                id: install_id(&entry.member),
                title: job
                    .map(|job| job.title.clone())
                    .or_else(|| entry.title.clone())
                    .unwrap_or_else(|| "Game".into()),
                game_id: job
                    .map(|job| job.game_id.clone())
                    .filter(|id| state_has(state, id)),
                state: install_state,
                play_when_ready: job.filter(|job| job.play).map(|_| SurfaceTrue),
                stream_meanwhile: job
                    .filter(|job| job.play && job.question == Question::Asked)
                    .map(|_| SurfaceTrue),
            });
        }
        // An install from devices that failed has left the line.
        let mut failed: Vec<SurfaceInstall> = inner
            .jobs
            .iter()
            .filter(|(id, _)| !shown.contains(&Member::Job((*id).clone())))
            .filter_map(|(id, job)| match &job.state {
                DeviceInstallState::Failed { reason } => Some(SurfaceInstall {
                    id: install_id(&Member::Job(id.clone())),
                    title: job.title.clone(),
                    game_id: Some(job.game_id.clone()).filter(|id| state_has(state, id)),
                    state: SurfaceInstallState::Failed {
                        reason: reason.message.clone(),
                    },
                    play_when_ready: None,
                    stream_meanwhile: None,
                }),
                _ => None,
            })
            .collect();
        // Failed entries go last, whatever the line reported first.
        list.sort_by_key(|install| matches!(install.state, SurfaceInstallState::Failed { .. }));
        list.append(&mut failed);
        Some(list)
    }

    /// The plugin review and its steps, or a problem, until dismissed.
    pub fn status(&self) -> Option<SurfaceStatus> {
        let inner = self.lock();
        if let Some(problem) = &inner.problem {
            return Some(problem.clone());
        }
        let flow = inner.plugin.as_ref()?;
        let busy = |kicker: String, cancel: bool| SurfaceStatus::Busy {
            kicker,
            detail: None,
            progress: None,
            steps: None,
            actions: cancel.then(|| vec![SurfaceAction::new(CANCEL_PLUGIN, "Cancel", true)]),
            decline_action_id: cancel.then(|| CANCEL_PLUGIN.into()),
            game_id: Some(flow.release.game_id.clone()),
        };
        Some(match &flow.phase {
            PluginPhase::Finding => busy(format!("Finding {}", flow.title), true),
            PluginPhase::Checking => busy(format!("Checking {}", flow.title), true),
            PluginPhase::Reviewing { question, .. } => question.clone(),
            // Installing has no Cancel: the plugin is half in place until it ends.
            PluginPhase::Installing => busy(format!("Installing {}", flow.title), false),
        })
    }

    // ------------------------------------------------------------ requests

    /// Learn whether this device has a runner for a peer's game, once.
    pub fn learn(self: &Arc<Self>, release: PeerRelease) {
        if self.access.is_none() {
            return;
        }
        {
            let mut inner = self.lock();
            let known = inner.releases.entry(release.key.clone()).or_default();
            if known.runner.is_some() || !inner.learning.insert(release.key.clone()) {
                return;
            }
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let fact = this.read_runner(&release).await;
            let mut inner = this.lock();
            inner.learning.remove(&release.key);
            if let Some(fact) = fact {
                inner
                    .releases
                    .entry(release.key.clone())
                    .or_default()
                    .runner = Some(fact);
                drop(inner);
                (this.notify)();
            }
        });
    }

    /// The holder's routes for the game, and what they mean here. None when
    /// no holder answered.
    async fn read_runner(&self, release: &PeerRelease) -> Option<RunnerFact> {
        let access = self.access.clone()?;
        for holder in &release.holders {
            let outcome = ask!(
                self.korrid,
                Route,
                GameRoutes,
                crate::game_routes::GameRoutesRequest {
                    game_id: release.holder_game_id.clone(),
                    holder: Some(holder.label.clone()),
                },
                crate::game_routes::GameRoutesOutcome
            );
            if let crate::game_routes::GameRoutesOutcome::Ok(routes) = outcome {
                let routes: Vec<(String, String)> = routes
                    .routes
                    .into_iter()
                    .map(|route| (route.system_id, route.runner_id))
                    .collect();
                let facts = (self.host.plugin_facts)();
                return Some(runner_fact(
                    &routes,
                    |system| access.runs_system(system),
                    &facts,
                ));
            }
        }
        None
    }

    /// `InstallGame`: check, then install at the front (play) or the back.
    pub fn install(self: &Arc<Self>, release: PeerRelease, play: bool) {
        let Some(access) = self.access.clone() else {
            return;
        };
        {
            let mut inner = self.lock();
            if inner.plugin.is_some() {
                return;
            }
            let live = inner.live_job(&release.key).map(|(id, _)| id.clone());
            if let Some(id) = live {
                if play {
                    if let Some(job) = inner.jobs.get_mut(&id) {
                        job.play = true;
                    }
                    drop(inner);
                    // A second start of the same release joins the first; at
                    // the front it moves the install there (#554 fix).
                    let _ = access.start(
                        &release.key,
                        &release.title,
                        release.holders.clone(),
                        Place::Front,
                    );
                    (self.notify)();
                }
                return;
            }
            let known = inner.releases.entry(release.key.clone()).or_default();
            if known.check == Check::Checking {
                return;
            }
            known.check = Check::Checking;
        }
        (self.notify)();
        let this = Arc::clone(self);
        tokio::spawn(async move { this.check_and_start(release, play).await });
    }

    fn set_check(&self, key: &str, check: Check) {
        self.lock()
            .releases
            .entry(key.to_owned())
            .or_default()
            .check = check;
        (self.notify)();
    }

    async fn check_and_start(self: Arc<Self>, release: PeerRelease, play: bool) {
        let Some(access) = self.access.clone() else {
            return;
        };
        let runner = self.read_runner(&release).await;
        if let Some(fact) = &runner {
            self.lock()
                .releases
                .entry(release.key.clone())
                .or_default()
                .runner = Some(fact.clone());
        }
        match runner {
            Some(RunnerFact::None) => {
                return self.set_check(
                    &release.key,
                    Check::Blocked {
                        reason: SurfaceInstallBlock::NoRunner,
                        bytes: None,
                    },
                )
            }
            Some(RunnerFact::Plugin { plugin_id, title }) => {
                self.set_check(&release.key, Check::Idle);
                return self.install_plugin(release, play, plugin_id, title).await;
            }
            Some(RunnerFact::Here) | None => (),
        }
        let checked = match access
            .check(release.key.clone(), release.holders.clone())
            .await
        {
            Ok(checked) => checked,
            Err((reason, bytes)) => {
                return self.set_check(&release.key, Check::Blocked { reason, bytes });
            }
        };
        // The holder's runners could not be read (only a device of the
        // holder's owner may read them): the release's own files name its
        // system.
        if runner.is_none()
            && !checked
                .systems
                .iter()
                .any(|system| access.runs_system(system))
        {
            return self.set_check(
                &release.key,
                Check::Blocked {
                    reason: SurfaceInstallBlock::NoRunner,
                    bytes: Some(checked.bytes),
                },
            );
        }
        let place = if play { Place::Front } else { Place::Back };
        match access.start(&release.key, &release.title, release.holders.clone(), place) {
            Ok(job) => {
                let mut inner = self.lock();
                inner.releases.entry(release.key.clone()).or_default().check = Check::Idle;
                let entry = inner.jobs.entry(job).or_insert_with(|| Job {
                    key: release.key.clone(),
                    title: release.title.clone(),
                    game_id: release.game_id.clone(),
                    play,
                    question: Question::None,
                    state: DeviceInstallState::Waiting,
                    since: None,
                    speed: None,
                });
                entry.play |= play;
            }
            Err(failure) => {
                let mut inner = self.lock();
                inner.releases.entry(release.key.clone()).or_default().check = Check::Idle;
                inner.problem = Some(problem(&release, failure.message));
            }
        }
        (self.notify)();
    }

    /// Install the plugin that offers the runner, with its review (ADR
    /// 0040), then go on with the game.
    async fn install_plugin(
        self: Arc<Self>,
        release: PeerRelease,
        play: bool,
        plugin_id: String,
        title: String,
    ) {
        {
            let mut inner = self.lock();
            if inner.plugin.is_some() {
                return;
            }
            inner.plugin = Some(PluginFlow {
                release: release.clone(),
                play,
                title: title.clone(),
                phase: PluginPhase::Finding,
            });
        }
        (self.notify)();
        let (found, _) = super::shared_play::plugin_step(
            &self.korrid,
            PluginFacts::default(),
            None,
            Box::new(|| {}),
        )
        .await;
        if !self.flow_in(|phase| matches!(phase, PluginPhase::Finding)) {
            return;
        }
        let Some(target) = plugins::newest_release(&found, &plugin_id) else {
            return self.flow_failed(format!(
                "No plugin repository on this device offers {title}."
            ));
        };
        self.set_phase(PluginPhase::Checking);
        let (checked, failure) = super::shared_play::plugin_step(
            &self.korrid,
            found,
            Some(plugins::inspect_action_id(&target)),
            Box::new(|| {}),
        )
        .await;
        if !self.flow_in(|phase| matches!(phase, PluginPhase::Checking)) {
            return;
        }
        let review = plugins::review_for(&checked, &target).cloned();
        let (Some(review), None) = (review, failure.clone()) else {
            return self
                .flow_failed(failure.unwrap_or_else(|| format!("{title} could not be checked.")));
        };
        let untrusted: Vec<String> = plugins::publishers_to_trust(&review.report)
            .into_iter()
            .map(|publisher| publisher.namespace)
            .collect();
        let name = review.report.title.clone().unwrap_or_else(|| title.clone());
        if !untrusted.is_empty() {
            return self.flow_failed(format!(
                "{name} needs you to trust {} first. Check the key in Settings, under Plugins.",
                untrusted.join(", ")
            ));
        }
        let question = SurfaceStatus::Confirm {
            kicker: format!("Install {name}?"),
            detail: Some(plugins::review_message(&review.report)),
            actions: vec![
                SurfaceAction::new(INSTALL_PLUGIN, format!("Install {name}"), true),
                SurfaceAction::new(CANCEL_PLUGIN, "Cancel", true),
            ],
            // A stray press must never approve a plugin or its permissions.
            decline_action_id: Some(CANCEL_PLUGIN.into()),
            game_id: Some(release.game_id.clone()),
        };
        self.set_phase(PluginPhase::Reviewing {
            target,
            facts: Box::new(checked),
            question,
        });
    }

    async fn approve(self: Arc<Self>) {
        let (release, play, target, facts) = {
            let mut inner = self.lock();
            let Some(flow) = inner.plugin.as_mut() else {
                return;
            };
            let PluginPhase::Reviewing { target, facts, .. } = flow.phase.clone() else {
                return;
            };
            flow.phase = PluginPhase::Installing;
            (flow.release.clone(), flow.play, target, facts)
        };
        (self.notify)();
        let (_, failure) = super::shared_play::plugin_step(
            &self.korrid,
            *facts,
            Some(plugins::review_action_id(&target)),
            Box::new(|| {}),
        )
        .await;
        if let Some(failure) = failure {
            return self.flow_failed(failure);
        }
        (self.host.library_changed)();
        {
            let mut inner = self.lock();
            inner.plugin = None;
            // The runner is read again with the new plugin in place.
            let known = inner.releases.entry(release.key.clone()).or_default();
            known.runner = None;
            known.check = Check::Checking;
        }
        (self.notify)();
        Box::pin(self.check_and_start(release, play)).await;
    }

    fn flow_in(&self, test: impl Fn(&PluginPhase) -> bool) -> bool {
        self.lock()
            .plugin
            .as_ref()
            .is_some_and(|flow| test(&flow.phase))
    }

    fn set_phase(&self, phase: PluginPhase) {
        if let Some(flow) = self.lock().plugin.as_mut() {
            flow.phase = phase;
        }
        (self.notify)();
    }

    fn flow_failed(&self, reason: String) {
        {
            let mut inner = self.lock();
            if let Some(flow) = inner.plugin.take() {
                inner.problem = Some(SurfaceStatus::Problem {
                    kicker: "Install failed".into(),
                    reason,
                    can_retry: false,
                    game_id: Some(flow.release.game_id.clone()),
                    game_title: Some(flow.release.title.clone()),
                });
            }
        }
        (self.notify)();
    }

    /// Run an action of the plugin review. True when it was ours.
    pub fn act(self: &Arc<Self>, action_id: &str) -> bool {
        match action_id {
            INSTALL_PLUGIN => {
                if !self.flow_in(|phase| matches!(phase, PluginPhase::Reviewing { .. })) {
                    return false;
                }
                tokio::spawn(Arc::clone(self).approve());
                true
            }
            CANCEL_PLUGIN => {
                let cancelled = {
                    let mut inner = self.lock();
                    let cancellable = inner
                        .plugin
                        .as_ref()
                        .is_some_and(|flow| !matches!(flow.phase, PluginPhase::Installing));
                    if cancellable {
                        inner.plugin = None;
                    }
                    cancellable
                };
                if cancelled {
                    (self.notify)();
                }
                cancelled
            }
            _ => false,
        }
    }

    /// The surface's Dismiss. True when an install problem was showing.
    pub fn dismiss(&self) -> bool {
        if self.lock().problem.take().is_none() {
            return false;
        }
        (self.notify)();
        true
    }

    /// `MoveInstall`: `index` is a position in `installs`.
    pub fn move_install(&self, install: &str, index: u32) {
        let (Some(access), Some(member)) = (self.access.as_ref(), member_of(install)) else {
            return;
        };
        let running = {
            let inner = self.lock();
            inner
                .line
                .first()
                .is_some_and(|entry| matches!(entry.state, LineState::Running { .. }))
        };
        let index = (index as usize).saturating_sub(usize::from(running));
        if access.reorder(&member, index) {
            self.read_line();
        }
    }

    /// `CancelInstall`: cancel one that waits or runs, or throw away one
    /// that failed.
    pub fn cancel(self: &Arc<Self>, install: &str) {
        let (Some(access), Some(member)) = (self.access.clone(), member_of(install)) else {
            return;
        };
        let known = {
            let inner = self.lock();
            inner.line.iter().any(|entry| entry.member == member)
                || matches!(&member, Member::Job(id) if inner.jobs.contains_key(id))
        };
        if !known {
            return;
        }
        if let Member::Job(id) = &member {
            if let Some(job) = self.lock().jobs.get_mut(id) {
                job.play = false;
            }
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let result = access.cancel(member.clone()).await;
            {
                let mut inner = this.lock();
                if let Member::Job(id) = &member {
                    // A thrown-away or cancelled install leaves the list.
                    if result.is_ok() {
                        inner.jobs.remove(id);
                    }
                }
                if let Err(failure) = result {
                    inner.problem = Some(SurfaceStatus::Problem {
                        kicker: "Couldn't cancel the install".into(),
                        reason: failure.message,
                        can_retry: false,
                        game_id: None,
                        game_title: None,
                    });
                }
            }
            this.read_line();
        });
    }

    /// `AnswerStreamMeanwhile`.
    pub fn answer(&self, install: &str, stream: bool, launch_location_id: Option<String>) {
        let Some(Member::Job(id)) = member_of(install) else {
            return;
        };
        let game_id = {
            let mut inner = self.lock();
            let Some(job) = inner.jobs.get_mut(&id) else {
                return;
            };
            if job.question != Question::Asked {
                return;
            }
            job.question = Question::Answered;
            if stream {
                // The stream comes first; the game does not start later.
                job.play = false;
            }
            job.game_id.clone()
        };
        (self.notify)();
        if stream {
            (self.host.stream)(game_id, launch_location_id);
        }
    }

    /// The person started a game or a stream: no play-here install starts
    /// its game when it ends.
    pub fn started_something_else(&self) {
        let mut changed = false;
        for job in self.lock().jobs.values_mut() {
            changed |= std::mem::replace(&mut job.play, false);
        }
        if changed {
            (self.notify)();
        }
    }

    // ------------------------------------------------------------ polling

    /// Read the line and the jobs this surface started; publish a change.
    pub fn read_line(&self) {
        let Some(access) = self.access.as_ref() else {
            return;
        };
        let line = access.line();
        let now = Instant::now();
        let mut imported = Vec::new();
        let changed = {
            let mut inner = self.lock();
            let before = (inner.line.clone(), jobs_view(&inner.jobs));
            inner.line = line;
            let ids: Vec<String> = inner.jobs.keys().cloned().collect();
            for id in ids {
                let Some(status) = access.status(&id) else {
                    // korrid no longer knows it: it is gone.
                    inner.jobs.remove(&id);
                    continue;
                };
                let job = inner.jobs.get_mut(&id).expect("job listed");
                if let DeviceInstallState::Installing {
                    bytes_done,
                    bytes_total,
                } = status.state
                {
                    let since = *job.since.get_or_insert((now, bytes_done));
                    if let Some(speed) = measured_speed(since, (now, bytes_done)) {
                        job.speed = Some(speed);
                    }
                    if job.play
                        && job.question == Question::None
                        && asks_stream_meanwhile(bytes_done, bytes_total, job.speed)
                    {
                        job.question = Question::Asked;
                    }
                }
                match &status.state {
                    DeviceInstallState::Imported { game_id } => {
                        if job.play {
                            imported.push(game_id.clone());
                        }
                        inner.jobs.remove(&id);
                    }
                    DeviceInstallState::Cancelled => {
                        inner.jobs.remove(&id);
                    }
                    state => job.state = state.clone(),
                }
            }
            before != (inner.line.clone(), jobs_view(&inner.jobs)) || !imported.is_empty()
        };
        if !imported.is_empty() {
            (self.host.library_changed)();
        }
        for game_id in imported {
            // Unless the person started something else meanwhile.
            if !(self.host.busy)() {
                (self.host.play)(game_id);
            }
        }
        if changed {
            (self.notify)();
        }
    }

    pub fn start(self: &Arc<Self>) {
        if self.access.is_none() {
            return;
        }
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                let Some(this) = weak.upgrade() else {
                    return;
                };
                let this2 = Arc::clone(&this);
                let _ = tokio::task::spawn_blocking(move || this2.read_line()).await;
                let idle = {
                    let inner = this.lock();
                    inner.line.is_empty() && inner.jobs.is_empty()
                };
                drop(this);
                tokio::time::sleep(if idle { IDLE_POLL } else { POLL }).await;
            }
        });
    }
}

/// What of the jobs a surface can see, to tell whether a read changed it.
fn jobs_view(jobs: &BTreeMap<String, Job>) -> Vec<(String, String, bool, Question, Option<u64>)> {
    jobs.iter()
        .map(|(id, job)| {
            (
                id.clone(),
                format!("{:?}", job.state),
                job.play,
                job.question,
                job.speed.map(|speed| speed as u64),
            )
        })
        .collect()
}

fn state_has(state: &LaunchablesState, id: &str) -> bool {
    state
        .entries()
        .iter()
        .any(|entry: &Entry| entry.key() == id)
}

fn problem(release: &PeerRelease, reason: String) -> SurfaceStatus {
    SurfaceStatus::Problem {
        kicker: format!("Couldn't install {}", release.title),
        reason,
        can_retry: false,
        game_id: Some(release.game_id.clone()),
        game_title: Some(release.title.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prediction_waits_for_a_measured_speed() {
        let start = Instant::now();
        // Before the sample time there is no speed, so no prediction.
        assert_eq!(
            measured_speed((start, 0), (start + SPEED_SAMPLE / 2, 10)),
            None
        );
        let speed = measured_speed((start, 100), (start + SPEED_SAMPLE * 2, 100 + 1000)).unwrap();
        let expected = 1000.0 / (SPEED_SAMPLE * 2).as_secs_f64();
        assert!((speed - expected).abs() < 1e-6);
        // Resumed bytes do not count as speed.
        assert_eq!(seconds_left(500, 1500, Some(10.0)), Some(100.0));
        assert_eq!(seconds_left(0, 10, None), None);
        assert_eq!(seconds_left(0, 10, Some(0.0)), None);
    }

    #[test]
    fn stream_meanwhile_is_asked_only_above_two_minutes() {
        assert_eq!(STREAM_MEANWHILE_ABOVE, Duration::from_secs(120));
        // No measured speed yet: no question.
        assert!(!asks_stream_meanwhile(0, 1_000, None));
        // 1000 bytes at 10 B/s: 100 s, under the threshold.
        assert!(!asks_stream_meanwhile(0, 1_000, Some(10.0)));
        // Exactly 120 s is not above it.
        assert!(!asks_stream_meanwhile(0, 1_200, Some(10.0)));
        assert!(asks_stream_meanwhile(0, 1_210, Some(10.0)));
        // Nothing arrived in the sample.
        assert!(asks_stream_meanwhile(0, 1_000, Some(0.0)));
        assert!(!asks_stream_meanwhile(1_000, 1_000, Some(0.0)));
    }

    #[test]
    fn install_ids_name_one_member_of_the_line() {
        for member in [
            Member::Artifact("@korri:mgba/core".into()),
            Member::Job("5f2c".into()),
        ] {
            assert_eq!(member_of(&install_id(&member)), Some(member));
        }
        assert_eq!(member_of("other:x"), None);
    }

    fn catalog(plugin_id: &str, title: Option<&str>) -> PluginFacts {
        let mut facts = PluginFacts::default();
        facts.catalogs.insert(
            "https://plugins.korri.example".into(),
            plugins::CatalogView::Ready(vec![serde_json::from_value(serde_json::json!({
                "pluginId": plugin_id,
                "title": title,
                "releaseVersion": "1.0.0",
                "publisher": {"namespace": "@korri", "state": "Bound"}
            }))
            .unwrap()]),
        );
        facts
    }

    #[test]
    fn a_runner_here_a_plugin_that_offers_one_or_none() {
        let routes = vec![("gba".to_owned(), "@korri:mgba/mgba".to_owned())];
        let facts = catalog("@korri:mgba", Some("mGBA"));
        assert_eq!(
            runner_fact(&routes, |system| system == "gba", &facts),
            RunnerFact::Here
        );
        assert_eq!(
            runner_fact(&routes, |_| false, &facts),
            RunnerFact::Plugin {
                plugin_id: "@korri:mgba".into(),
                title: "mGBA".into()
            }
        );
        assert_eq!(
            runner_fact(&routes, |_| false, &PluginFacts::default()),
            RunnerFact::None
        );
    }
}
