//! #553: play here and the list of installs, against a configurable install
//! manager (`Configured`), the surface's in-memory korrid for the holder's
//! routes, and the hub for the requests.

use super::*;
use crate::surface::installs;
use crate::{
    acquisition::{
        pieces::{DeviceInstallState, DeviceInstallStatus},
        Member, Place,
    },
    surface::installs::{
        Checked, Holder, Installs, InstallsAccess, InstallsHost, LineEntry, LineState,
        INSTALL_PLUGIN,
    },
    surface::model::{peer_release, PeerRelease},
    surface::plugins::{CatalogView, PluginFacts},
    RpcFailure,
};
use korri_surface_treaty::{
    SurfaceInstallBlock, SurfaceInstallHere, SurfaceInstallPace, SurfaceInstallState,
    SurfaceInstallYield, SurfaceInstallYieldReason, SurfaceProgress, SurfaceTrue,
};

/// The install manager with configured answers, recording what it was asked.
#[derive(Default)]
struct Configured {
    line: std::sync::Mutex<Vec<LineEntry>>,
    systems: std::sync::Mutex<Vec<String>>,
    check: std::sync::Mutex<Option<Result<Checked, (SurfaceInstallBlock, Option<u64>)>>>,
    jobs: std::sync::Mutex<HashMap<String, DeviceInstallStatus>>,
    started: std::sync::Mutex<Vec<(String, Place)>>,
    cancelled: std::sync::Mutex<Vec<Member>>,
    reordered: std::sync::Mutex<Vec<(Member, usize)>>,
    /// This device already holds the release.
    held: std::sync::Mutex<bool>,
}

impl Configured {
    fn set_job(&self, job: &str, state: DeviceInstallState) {
        self.jobs.lock().unwrap().insert(
            job.into(),
            DeviceInstallStatus {
                job_id: job.into(),
                release_key: KEY.into(),
                state,
                yielding: None,
            },
        );
    }
}

impl InstallsAccess for Configured {
    fn line(&self) -> Vec<LineEntry> {
        self.line.lock().unwrap().clone()
    }
    fn runs_system(&self, system: &str) -> bool {
        self.systems.lock().unwrap().iter().any(|id| id == system)
    }
    fn check(
        &self,
        _key: String,
        _holders: Vec<Holder>,
    ) -> futures::future::BoxFuture<'static, Result<Checked, (SurfaceInstallBlock, Option<u64>)>>
    {
        let answer = self
            .check
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(Err((SurfaceInstallBlock::NoSource, None)));
        Box::pin(async move { answer })
    }
    fn start(
        &self,
        key: &str,
        _title: &str,
        _holders: Vec<Holder>,
        place: Place,
    ) -> Result<String, RpcFailure> {
        self.started.lock().unwrap().push((key.into(), place));
        if !self.jobs.lock().unwrap().contains_key("j1") {
            self.set_job("j1", DeviceInstallState::Waiting);
        }
        Ok("j1".into())
    }
    fn status(&self, job: &str) -> Option<DeviceInstallStatus> {
        self.jobs.lock().unwrap().get(job).cloned()
    }
    fn holds(&self, _key: &str) -> bool {
        *self.held.lock().unwrap()
    }
    fn cancel(
        &self,
        member: Member,
    ) -> futures::future::BoxFuture<'static, Result<(), RpcFailure>> {
        if let Member::Job(id) = &member {
            self.jobs.lock().unwrap().remove(id);
        }
        self.cancelled.lock().unwrap().push(member);
        Box::pin(async { Ok(()) })
    }
    fn reorder(&self, member: &Member, index: usize) -> bool {
        self.reordered.lock().unwrap().push((member.clone(), index));
        true
    }
}

const KEY: &str = "0123456789abcdef0123456789abcdef01234567";

fn peer_game(key: Option<&str>) -> Entry {
    let mut game = json!({ "id": "anguna", "title": "Anguna", "host": "zao", "supportsRunnerSelection": true,
        "source": { "devicePublicKey": "zao-key", "label": "zao", "isLocal": false } });
    if let Some(key) = key {
        game["identity"] = json!({ "kind": "hash", "value": key });
    }
    Entry::Game {
        game: from(game),
        alternatives: Vec::new(),
    }
}

fn release() -> PeerRelease {
    peer_release(&peer_game(Some(KEY))).expect("a peer-only file release")
}

fn routes(system: &str, runner: &str) -> Value {
    json!({ "_tag": "Ok", "payload": {
        "gameId": "anguna",
        "routes": [{ "runnerId": runner, "systemId": system, "runnerBuild": "b", "program": "p", "warnings": [] }],
        "selection": { "_tag": "Choose" }, "systemRunners": {}, "revisions": { "device": "1", "games": "1" }
    } })
}

#[derive(Default)]
struct Calls {
    played: std::sync::Mutex<Vec<String>>,
    library_reads: std::sync::atomic::AtomicUsize,
    streamed: std::sync::Mutex<Vec<(String, Option<String>)>>,
    busy: std::sync::atomic::AtomicBool,
}

fn controller(
    access: Arc<Configured>,
    outcomes: HashMap<String, Value>,
    facts: PluginFacts,
) -> (Arc<Installs>, Arc<Calls>) {
    let calls = Arc::new(Calls::default());
    let (played, streamed, busy, read) =
        (calls.clone(), calls.clone(), calls.clone(), calls.clone());
    let installs = Installs::new(
        in_memory_korrid(Arc::new(std::sync::Mutex::new(outcomes))).with_installs(access),
        Arc::new(|| {}),
        InstallsHost {
            state: Box::new(|| ready(vec![peer_game(Some(KEY))], None, None)),
            plugin_facts: Box::new(move || facts.clone()),
            play: Box::new(move |installed| played.played.lock().unwrap().push(installed.key)),
            stream: Box::new(move |id, location| {
                streamed.streamed.lock().unwrap().push((id, location))
            }),
            library_changed: Box::new(move || {
                read.library_reads
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }),
            busy: Box::new(move || busy.busy.load(std::sync::atomic::Ordering::SeqCst)),
        },
    );
    (installs, calls)
}

async fn until(what: &str, ready: impl Fn() -> bool) {
    for _ in 0..400 {
        if ready() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("never: {what}")
}

fn gba_runner_here() -> Arc<Configured> {
    Arc::new(Configured {
        systems: std::sync::Mutex::new(vec!["gba".into()]),
        ..Configured::default()
    })
}

fn fits(bytes: u64) -> Option<Result<Checked, (SurfaceInstallBlock, Option<u64>)>> {
    Some(Ok(Checked {
        bytes,
        systems: vec!["gba".into()],
    }))
}

#[test]
fn only_a_file_release_that_only_other_devices_hold_can_be_installed_here() {
    let found = release();
    assert_eq!(found.key, KEY);
    assert_eq!(found.holder_game_id, "anguna");
    assert_eq!(
        found.holders,
        [Holder {
            device: "zao-key".into(),
            label: "zao".into()
        }]
    );
    // A provider identity or none: devices cannot serve it.
    assert_eq!(peer_release(&peer_game(None)), None);
    // This device holds it already.
    assert_eq!(peer_release(&local_game()), None);
    let held_here = Entry::Game {
        game: from(
            json!({ "id": "anguna", "title": "Anguna", "supportsRunnerSelection": false,
            "identity": { "kind": "hash", "value": KEY }, "source": { "label": "This device", "isLocal": true } }),
        ),
        alternatives: Vec::new(),
    };
    assert_eq!(peer_release(&held_here), None);
}

#[tokio::test]
async fn offered_until_the_holders_routes_say_otherwise() {
    // No runner fact yet: nothing stops it.
    let (installs, _) = controller(gba_runner_here(), idle_outcomes(0), PluginFacts::default());
    assert_eq!(installs.here(&release()), Some(SurfaceInstallHere::Offered));

    // The holder runs it on snes; no runner here and no plugin offers one.
    let mut outcomes = idle_outcomes(0);
    outcomes.insert(
        "app.local-games.routes".into(),
        routes("snes", "@korri:snes9x/snes9x"),
    );
    let (installs, _) = controller(gba_runner_here(), outcomes.clone(), PluginFacts::default());
    installs.learn(release());
    until("no runner", || {
        installs.here(&release())
            == Some(SurfaceInstallHere::Unavailable {
                reason: SurfaceInstallBlock::NoRunner,
                bytes: None,
            })
    })
    .await;

    // A repository offers the holder's runner's plugin.
    let mut facts = PluginFacts::default();
    facts.catalogs.insert(
        "https://plugins.example".into(),
        CatalogView::Ready(vec![from(json!({
            "pluginId": "@korri:snes9x", "title": "Snes9x", "releaseVersion": "1.0.0",
            "publisher": { "namespace": "@korri", "state": "Bound" }
        }))]),
    );
    let (installs, _) = controller(gba_runner_here(), outcomes, facts);
    installs.learn(release());
    until("needs a plugin", || {
        installs.here(&release())
            == Some(SurfaceInstallHere::NeedsPlugin {
                plugin_title: "Snes9x".into(),
            })
    })
    .await;
}

#[tokio::test]
async fn each_refusal_of_the_check_shows_with_its_reason() {
    for (refusal, bytes) in [
        (SurfaceInstallBlock::NotEnoughSpace, Some(4_700_000_000)),
        (SurfaceInstallBlock::NoSource, None),
        (SurfaceInstallBlock::NotAllowed, None),
    ] {
        let access = gba_runner_here();
        *access.check.lock().unwrap() = Some(Err((refusal, bytes)));
        let (installs, _) = controller(access.clone(), idle_outcomes(0), PluginFacts::default());
        installs.install(release(), true);
        until("the refusal", || {
            installs.here(&release())
                == Some(SurfaceInstallHere::Unavailable {
                    reason: refusal,
                    bytes: bytes.map(|bytes| bytes as f64),
                })
        })
        .await;
        assert!(access.started.lock().unwrap().is_empty(), "{refusal:?}");
    }
    // The holder's routes could not be read, and no runner here takes the
    // release's own files.
    let access = Arc::new(Configured::default());
    *access.check.lock().unwrap() = fits(10);
    let (installs, _) = controller(access.clone(), idle_outcomes(0), PluginFacts::default());
    installs.install(release(), false);
    until("no runner", || {
        installs.here(&release())
            == Some(SurfaceInstallHere::Unavailable {
                reason: SurfaceInstallBlock::NoRunner,
                bytes: Some(10.0),
            })
    })
    .await;
}

#[tokio::test]
async fn play_here_goes_to_the_front_and_install_only_to_the_back() {
    for (play, place) in [(true, Place::Front), (false, Place::Back)] {
        let access = gba_runner_here();
        *access.check.lock().unwrap() = fits(1_000);
        let (installs, _) = controller(access.clone(), idle_outcomes(0), PluginFacts::default());
        installs.install(release(), play);
        until("started", || !access.started.lock().unwrap().is_empty()).await;
        assert_eq!(*access.started.lock().unwrap(), [(KEY.to_owned(), place)]);
        assert_eq!(
            installs.here(&release()),
            Some(SurfaceInstallHere::Installing {
                install_id: "job:j1".into()
            })
        );
        // Waiting: the game's download says so, as any download does.
        let download = installs.download(&release()).unwrap();
        assert_eq!(download.queued, Some(SurfaceTrue));
        *access.line.lock().unwrap() = vec![LineEntry {
            member: Member::Job("j1".into()),
            title: None,
            state: LineState::Waiting { bytes: None },
        }];
        installs.read_line();
        let list = installs
            .list(&ready(vec![peer_game(Some(KEY))], None, None))
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "job:j1");
        assert_eq!(list[0].title, "Anguna");
        assert_eq!(list[0].game_id.as_deref(), Some("game:zao:anguna"));
        assert_eq!(list[0].state, SurfaceInstallState::Waiting { bytes: None });
        assert_eq!(list[0].play_when_ready, play.then_some(SurfaceTrue));
    }
}

#[tokio::test]
async fn play_here_on_an_install_that_waits_moves_it_to_the_front_and_plays_it() {
    let access = gba_runner_here();
    *access.check.lock().unwrap() = fits(1_000);
    let (installs, _) = controller(access.clone(), idle_outcomes(0), PluginFacts::default());
    installs.install(release(), false);
    until("started", || !access.started.lock().unwrap().is_empty()).await;
    installs.install(release(), true);
    assert_eq!(
        *access.started.lock().unwrap(),
        [
            (KEY.to_owned(), Place::Back),
            (KEY.to_owned(), Place::Front)
        ]
    );
    *access.line.lock().unwrap() = vec![LineEntry {
        member: Member::Job("j1".into()),
        title: None,
        state: LineState::Waiting { bytes: None },
    }];
    installs.read_line();
    let list = installs.list(&LaunchablesState::Loading).unwrap();
    assert_eq!(list[0].play_when_ready, Some(SurfaceTrue));
}

/// The install slice of the line as a running device install.
fn running(access: &Configured, done: u64, total: u64) {
    access.set_job(
        "j1",
        DeviceInstallState::Installing {
            bytes_done: done,
            bytes_total: total,
        },
    );
    *access.line.lock().unwrap() = vec![LineEntry {
        member: Member::Job("j1".into()),
        title: None,
        state: LineState::Running {
            done,
            total: Some(total),
            bytes_per_second: None,
            yielding: None,
        },
    }];
}

async fn started_play_here(total: u64) -> (Arc<Configured>, Arc<Installs>, Arc<Calls>) {
    let access = gba_runner_here();
    *access.check.lock().unwrap() = fits(total);
    let (installs, calls) = controller(access.clone(), idle_outcomes(0), PluginFacts::default());
    installs.install(release(), true);
    until("started", || !access.started.lock().unwrap().is_empty()).await;
    (access, installs, calls)
}

#[tokio::test]
async fn a_slow_play_here_asks_once_whether_to_stream_meanwhile_and_waiting_keeps_the_play() {
    let (access, installs, calls) = started_play_here(1_000_000_000).await;
    running(&access, 0, 1_000_000_000);
    installs.read_line();
    let state = LaunchablesState::Loading;
    // No measured speed yet: no prediction and no question.
    let first = installs.list(&state).unwrap().remove(0);
    assert!(matches!(
        first.state,
        SurfaceInstallState::Installing {
            seconds_left: None,
            ..
        }
    ));
    assert_eq!(first.stream_meanwhile, None);
    tokio::time::sleep(installs::SPEED_SAMPLE * 2).await;
    // A few bytes over the sample: far above two minutes.
    running(&access, 1_000, 1_000_000_000);
    installs.read_line();
    let asked = installs.list(&state).unwrap().remove(0);
    assert_eq!(asked.stream_meanwhile, Some(SurfaceTrue));
    let SurfaceInstallState::Installing {
        progress,
        seconds_left: Some(left),
        ..
    } = asked.state
    else {
        panic!("a prediction: {:?}", asked.state)
    };
    assert_eq!(
        progress,
        SurfaceProgress {
            done: 1_000.0,
            total: Some(1_000_000_000.0)
        }
    );
    assert!(left > installs::STREAM_MEANWHILE_ABOVE.as_secs_f64());
    // The person waits: the question goes and the play stays.
    installs.answer("job:j1", false, None);
    installs.read_line();
    let answered = installs.list(&state).unwrap().remove(0);
    assert_eq!(answered.stream_meanwhile, None);
    assert_eq!(answered.play_when_ready, Some(SurfaceTrue));
    // Asked once only.
    installs.read_line();
    assert_eq!(installs.list(&state).unwrap()[0].stream_meanwhile, None);
    assert!(calls.streamed.lock().unwrap().is_empty());
    // Done: the game starts.
    access.set_job(
        "j1",
        DeviceInstallState::Imported {
            game_id: "local-anguna".into(),
        },
    );
    access.line.lock().unwrap().clear();
    installs.read_line();
    assert_eq!(*calls.played.lock().unwrap(), [KEY]);
    assert!(installs.list(&state).unwrap().is_empty());
}

#[tokio::test]
async fn streaming_meanwhile_starts_the_stream_and_the_game_does_not_start_later() {
    let (access, installs, calls) = started_play_here(1_000_000_000).await;
    running(&access, 0, 1_000_000_000);
    installs.read_line();
    tokio::time::sleep(installs::SPEED_SAMPLE * 2).await;
    running(&access, 10, 1_000_000_000);
    installs.read_line();
    // An answer to a question korrid did not ask does nothing.
    installs.answer("job:other", true, None);
    installs.answer("job:j1", true, Some("location".into()));
    assert_eq!(
        *calls.streamed.lock().unwrap(),
        [("game:zao:anguna".to_owned(), Some("location".to_owned()))]
    );
    let state = LaunchablesState::Loading;
    let entry = installs.list(&state).unwrap().remove(0);
    assert_eq!(
        (entry.play_when_ready, entry.stream_meanwhile),
        (None, None)
    );
    access.set_job(
        "j1",
        DeviceInstallState::Imported {
            game_id: "local-anguna".into(),
        },
    );
    installs.read_line();
    assert!(calls.played.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_fast_play_here_never_asks() {
    let (access, installs, _) = started_play_here(1_000).await;
    running(&access, 0, 1_000);
    installs.read_line();
    tokio::time::sleep(installs::SPEED_SAMPLE * 2).await;
    running(&access, 900, 1_000);
    installs.read_line();
    let entry = installs.list(&LaunchablesState::Loading).unwrap().remove(0);
    assert_eq!(entry.stream_meanwhile, None);
}

#[tokio::test]
async fn something_else_started_meanwhile_keeps_the_game_from_starting() {
    let (access, installs, calls) = started_play_here(1_000).await;
    installs.started_something_else();
    access.set_job(
        "j1",
        DeviceInstallState::Imported {
            game_id: "local-anguna".into(),
        },
    );
    installs.read_line();
    assert!(calls.played.lock().unwrap().is_empty());
    // A launch under way when it ends: the same.
    let (access, installs, calls) = started_play_here(1_000).await;
    calls.busy.store(true, std::sync::atomic::Ordering::SeqCst);
    access.set_job(
        "j1",
        DeviceInstallState::Imported {
            game_id: "local-anguna".into(),
        },
    );
    installs.read_line();
    assert!(calls.played.lock().unwrap().is_empty());
    // The game does not start, but the library still reads it.
    assert_eq!(reads(&calls), 1);
}

fn reads(calls: &Calls) -> usize {
    calls
        .library_reads
        .load(std::sync::atomic::Ordering::SeqCst)
}

/// Bug (device check, 2026-11-24): an install without play that ended left
/// the catalog stale.
#[tokio::test]
async fn every_install_that_lands_reads_the_library_with_play_or_without() {
    let access = gba_runner_here();
    *access.check.lock().unwrap() = fits(1_000);
    let (installs, calls) = controller(access.clone(), idle_outcomes(0), PluginFacts::default());
    installs.install(release(), false);
    until("started", || !access.started.lock().unwrap().is_empty()).await;
    running(&access, 0, 1_000);
    installs.read_line();
    assert_eq!(reads(&calls), 0);
    access.set_job(
        "j1",
        DeviceInstallState::Imported {
            game_id: "anguna".into(),
        },
    );
    access.line.lock().unwrap().clear();
    installs.read_line();
    assert_eq!(reads(&calls), 1);
    assert!(calls.played.lock().unwrap().is_empty());
    // A provider job or an artifact that leaves the line also ends.
    *access.line.lock().unwrap() = vec![LineEntry {
        member: Member::Job("provider".into()),
        title: Some("anguna.gba".into()),
        state: LineState::Waiting { bytes: None },
    }];
    installs.read_line();
    assert_eq!(reads(&calls), 1);
    access.line.lock().unwrap().clear();
    installs.read_line();
    assert_eq!(reads(&calls), 2);
    // Nothing changed: no read.
    installs.read_line();
    assert_eq!(reads(&calls), 2);
}

/// A stale entry: the install just finished, the catalog still shows the
/// game on the holder only. Pressing again installs nothing again.
#[tokio::test]
async fn a_release_this_device_already_holds_is_never_installed_again() {
    for play in [true, false] {
        let access = gba_runner_here();
        *access.check.lock().unwrap() = fits(1_000);
        *access.held.lock().unwrap() = true;
        let (installs, calls) =
            controller(access.clone(), idle_outcomes(0), PluginFacts::default());
        installs.install(release(), play);
        until("settled", || {
            installs.here(&release()) == Some(SurfaceInstallHere::Offered)
        })
        .await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(access.started.lock().unwrap().is_empty(), "play {play}");
        if play {
            assert_eq!(*calls.played.lock().unwrap(), [KEY]);
        } else {
            assert_eq!(reads(&calls), 1);
            assert!(calls.played.lock().unwrap().is_empty());
        }
    }
}

#[tokio::test]
async fn the_list_runs_first_waits_next_and_failed_last_and_each_can_go() {
    let (access, installs, _) = started_play_here(1_000).await;
    access.set_job(
        "j1",
        DeviceInstallState::Failed {
            reason: RpcFailure {
                code: "InstallSourceUnavailable".into(),
                message: "The devices that hold this release stopped answering.".into(),
            },
        },
    );
    *access.line.lock().unwrap() = vec![
        LineEntry {
            member: Member::Artifact("@korri:mgba/core".into()),
            title: Some("mGBA core".into()),
            state: LineState::Running {
                done: 10,
                total: Some(100),
                bytes_per_second: Some(9.0),
                yielding: Some(SurfaceInstallYield {
                    pace: SurfaceInstallPace::Slowed,
                    reason: SurfaceInstallYieldReason::Game,
                }),
            },
        },
        LineEntry {
            member: Member::Job("provider".into()),
            title: Some("anguna.gba".into()),
            state: LineState::Waiting { bytes: Some(5) },
        },
        LineEntry {
            member: Member::Artifact("@korri:mgba/bios".into()),
            title: Some("BIOS".into()),
            state: LineState::Failed {
                reason: "Download failed.".into(),
            },
        },
    ];
    installs.read_line();
    let list = installs.list(&LaunchablesState::Loading).unwrap();
    let ids: Vec<&str> = list.iter().map(|install| install.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "artifact:@korri:mgba/core",
            "job:provider",
            "artifact:@korri:mgba/bios",
            "job:j1"
        ]
    );
    assert_eq!(
        list[0].state,
        SurfaceInstallState::Installing {
            progress: SurfaceProgress {
                done: 10.0,
                total: Some(100.0)
            },
            seconds_left: Some(10.0),
            // Slowed for a game here: the model says how and why.
            yielding: Some(SurfaceInstallYield {
                pace: SurfaceInstallPace::Slowed,
                reason: SurfaceInstallYieldReason::Game,
            }),
        }
    );
    assert_eq!(
        list[1].state,
        SurfaceInstallState::Waiting { bytes: Some(5.0) }
    );
    assert_eq!(
        list[3].state,
        SurfaceInstallState::Failed {
            reason: "The devices that hold this release stopped answering.".into()
        }
    );
    // A failed install is not in the line: the game can be asked again.
    assert_eq!(installs.here(&release()), Some(SurfaceInstallHere::Offered));
    // Index 0 while one runs means next: the first place among those that wait.
    installs.move_install("job:provider", 0);
    installs.move_install("job:provider", 2);
    assert_eq!(
        *access.reordered.lock().unwrap(),
        [
            (Member::Job("provider".into()), 0),
            (Member::Job("provider".into()), 1)
        ]
    );
    // Throw away the failed install from devices; cancel one that waits.
    installs.cancel("job:j1");
    installs.cancel("job:provider");
    installs.cancel("job:unknown");
    until("both cancelled", || {
        access.cancelled.lock().unwrap().len() == 2
    })
    .await;
    let cancelled = access.cancelled.lock().unwrap().clone();
    assert!(cancelled.contains(&Member::Job("j1".into())));
    assert!(cancelled.contains(&Member::Job("provider".into())));
    until("the failed install left the list", || {
        !installs
            .list(&LaunchablesState::Loading)
            .unwrap()
            .iter()
            .any(|install| install.id == "job:j1")
    })
    .await;
}

#[tokio::test]
async fn a_status_korrid_forgot_is_gone() {
    let (access, installs, _) = started_play_here(1_000).await;
    access.jobs.lock().unwrap().clear();
    installs.read_line();
    assert_eq!(installs.here(&release()), Some(SurfaceInstallHere::Offered));
    assert!(installs
        .list(&LaunchablesState::Loading)
        .unwrap()
        .is_empty());
}

fn plugin_outcomes() -> HashMap<String, Value> {
    let publisher = json!({ "namespace": "@korri", "state": "Bound", "fingerprint": "fp" });
    let mut outcomes = idle_outcomes(0);
    for (tag, payload) in [
        (
            "app.local-games.routes",
            routes("snes", "@korri:snes9x/snes9x"),
        ),
        (
            "system.plugins.repositories",
            json!({ "_tag": "Ok", "payload": { "official": "https://plugins.example", "userAdded": [], "repositoryInstalls": [], "otherInstalls": [] } }),
        ),
        (
            "system.plugins.publishers",
            json!({ "_tag": "Ok", "payload": { "entries": [] } }),
        ),
        (
            "system.plugins.catalog",
            json!({ "_tag": "Ok", "payload": { "entries": [
                { "pluginId": "@korri:snes9x", "title": "Snes9x", "releaseVersion": "1.0.0", "publisher": publisher }
            ] } }),
        ),
        (
            "system.plugins.inspect",
            json!({ "_tag": "Ok", "payload": { "publisher": publisher, "id": "@korri:snes9x", "title": "Snes9x",
                "approval": "sha256:digest", "warning": "Runs games.", "ports": { "allowedTCPPorts": [], "allowedUDPPorts": [] },
                "nativeUnits": [], "brings": [] } }),
        ),
        (
            "system.plugins.install",
            json!({ "_tag": "Ok", "payload": {} }),
        ),
        (
            "system.plugins.setEnabled",
            json!({ "_tag": "Ok", "payload": {} }),
        ),
    ] {
        outcomes.insert(tag.into(), payload);
    }
    outcomes
}

fn snes_offered() -> PluginFacts {
    let mut facts = PluginFacts::default();
    facts.catalogs.insert(
        "https://plugins.example".into(),
        CatalogView::Ready(vec![from(json!({
            "pluginId": "@korri:snes9x", "title": "Snes9x", "releaseVersion": "1.0.0",
            "publisher": { "namespace": "@korri", "state": "Bound" }
        }))]),
    );
    facts
}

/// ADR 0092, as ADR 0040: no runner here but a plugin offers one. The
/// plugin's review comes first; approved, it installs, and then the game.
#[tokio::test]
async fn a_plugin_that_offers_the_runner_is_reviewed_and_installed_first() {
    let access = Arc::new(Configured::default());
    *access.check.lock().unwrap() = fits(1_000);
    let (installs, _) = controller(access.clone(), plugin_outcomes(), snes_offered());
    installs.install(release(), true);
    until("the review", || {
        matches!(installs.status(), Some(SurfaceStatus::Confirm { .. }))
    })
    .await;
    let Some(SurfaceStatus::Confirm {
        kicker,
        actions,
        decline_action_id,
        ..
    }) = installs.status()
    else {
        unreachable!()
    };
    assert_eq!(kicker, "Install Snes9x?");
    assert_eq!(actions[0].id, INSTALL_PLUGIN);
    assert_eq!(decline_action_id.as_deref(), Some(installs::CANCEL_PLUGIN));
    assert_eq!(
        installs.here(&release()),
        Some(SurfaceInstallHere::Checking)
    );
    // Approved: the plugin installs, the runner is read again, the game installs.
    access.systems.lock().unwrap().push("snes".into());
    assert!(installs.act(INSTALL_PLUGIN));
    until("the game installs", || {
        !access.started.lock().unwrap().is_empty()
    })
    .await;
    assert_eq!(
        *access.started.lock().unwrap(),
        [(KEY.to_owned(), Place::Front)]
    );
    assert_eq!(installs.status(), None);
}

#[tokio::test]
async fn cancel_backs_out_of_the_plugin_review_and_installs_nothing() {
    let access = Arc::new(Configured::default());
    *access.check.lock().unwrap() = fits(1_000);
    let (installs, _) = controller(access.clone(), plugin_outcomes(), snes_offered());
    installs.install(release(), false);
    until("the review", || {
        matches!(installs.status(), Some(SurfaceStatus::Confirm { .. }))
    })
    .await;
    assert!(installs.act(installs::CANCEL_PLUGIN));
    assert_eq!(installs.status(), None);
    assert!(!installs.act(INSTALL_PLUGIN));
    assert!(access.started.lock().unwrap().is_empty());
}

#[tokio::test]
async fn no_repository_offering_the_plugin_is_a_problem_that_says_so() {
    let mut outcomes = idle_outcomes(0);
    outcomes.insert(
        "app.local-games.routes".into(),
        routes("snes", "@korri:snes9x/snes9x"),
    );
    let access = Arc::new(Configured::default());
    let (installs, _) = controller(access.clone(), outcomes, snes_offered());
    installs.install(release(), true);
    until("the problem", || {
        matches!(installs.status(), Some(SurfaceStatus::Problem { .. }))
    })
    .await;
    let Some(SurfaceStatus::Problem { kicker, reason, .. }) = installs.status() else {
        unreachable!()
    };
    assert_eq!(kicker, "Install failed");
    assert_eq!(reason, "No plugin repository on this device offers Snes9x.");
    assert!(installs.dismiss());
    assert_eq!(installs.status(), None);
    assert!(access.started.lock().unwrap().is_empty());
}

/// The hub decorates a peer-only game and routes the new requests.
#[tokio::test]
async fn the_hub_publishes_install_here_and_performs_install_game() {
    let access = gba_runner_here();
    *access.check.lock().unwrap() = fits(1_000);
    let mut outcomes = idle_outcomes(0);
    outcomes.insert(
        "app.catalog.snapshot".into(),
        json!({"_tag":"Ok","payload":{"games":[{
            "id":"anguna","title":"Anguna","host":"zao","supportsRunnerSelection":true,
            "identity":{"kind":"hash","value":KEY},
            "source":{"devicePublicKey":"zao-key","label":"zao","isLocal":false}}]}}),
    );
    let outcomes = Arc::new(std::sync::Mutex::new(outcomes));
    let directory = tempfile::tempdir().unwrap();
    let intent = Arc::new(crate::overlay_intent::OverlayIntent::default());
    let hub = SurfaceHub::new(
        in_memory_korrid(Arc::clone(&outcomes)).with_installs(access.clone()),
        directory.path().join("private"),
        intent.subscribe(),
        Arc::default(),
    );
    let model = settled(&hub, "velvet").await;
    assert_eq!(model.installs, Some(Vec::new()));
    let game = |model: &SurfaceModel| match &model.catalog {
        SurfaceCatalog::Ready { window, .. } => window[0].clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(game(&model).install_here, Some(SurfaceInstallHere::Offered));
    // Play here is not a launch location (Zone's "Play here" means held here).
    assert_eq!(game(&model).launch_locations, None);
    let id = game(&model).id;
    hub.request(
        "velvet",
        SurfaceRequest::InstallGame {
            game_id: id.clone(),
            play: true,
        },
    )
    .unwrap();
    let model = model_when(&hub, "installing", |model| {
        matches!(
            game(model).install_here,
            Some(SurfaceInstallHere::Installing { .. })
        )
    })
    .await;
    assert_eq!(
        *access.started.lock().unwrap(),
        [(KEY.to_owned(), Place::Front)]
    );
    assert_eq!(
        game(&model).download.and_then(|download| download.queued),
        Some(SurfaceTrue)
    );
    *access.line.lock().unwrap() = vec![LineEntry {
        member: Member::Job("j1".into()),
        title: None,
        state: LineState::Waiting { bytes: None },
    }];
    let model = model_when(&hub, "the list holds it", |model| {
        model.installs.as_ref().is_some_and(|list| list.len() == 1)
    })
    .await;
    let entry = &model.installs.unwrap()[0];
    assert_eq!(entry.game_id.as_deref(), Some(id.as_str()));
    assert_eq!(entry.play_when_ready, Some(SurfaceTrue));
    hub.request(
        "velvet",
        SurfaceRequest::CancelInstall {
            install_id: entry.id.clone(),
        },
    )
    .unwrap();
    until("cancelled", || !access.cancelled.lock().unwrap().is_empty()).await;
}

// ------------------------------------------------- the real surface root

/// The catalog as korrid answers it: the holder's copy only, or also the
/// local copy that the install made.
fn catalog_outcome(local: bool) -> Value {
    let identity = json!({ "kind": "hash", "value": KEY });
    let mut games = vec![json!({
        "id": "anguna", "title": "Anguna", "host": "ibuki", "supportsRunnerSelection": false,
        "identity": identity, "source": { "devicePublicKey": "ibuki-key", "label": "ibuki", "isLocal": false } })];
    if local {
        games.push(json!({
            "id": "anguna", "title": "Anguna", "host": "aka", "supportsRunnerSelection": false,
            "identity": identity, "source": { "devicePublicKey": "aka-key", "label": "aka", "isLocal": true } }));
    }
    json!({ "_tag": "Ok", "payload": { "games": games } })
}

fn local_games_outcome(local: bool) -> Value {
    let games: Vec<Value> = if local {
        vec![
            json!({ "id": "anguna", "title": "Anguna", "system": "GBA", "systemId": "gba",
            "identity": { "kind": "hash", "value": KEY } }),
        ]
    } else {
        Vec::new()
    };
    json!({ "_tag": "Ok", "payload": { "games": games } })
}

/// The surface root with its real launchables, on a korrid whose catalog
/// reads take time and are counted, and that records every request.
struct Device {
    hub: Arc<SurfaceHub>,
    outcomes: Outcomes,
    requests: Arc<std::sync::Mutex<Vec<Value>>>,
    catalog_reads: Arc<std::sync::atomic::AtomicUsize>,
    _directory: tempfile::TempDir,
}

fn device(access: Arc<Configured>) -> Device {
    let mut outcomes = idle_outcomes(0);
    outcomes.insert("app.catalog.snapshot".into(), catalog_outcome(false));
    outcomes.insert("app.local-games.list".into(), local_games_outcome(false));
    outcomes.insert(
        "app.session.reserve".into(),
        json!({ "_tag": "Ok", "payload": { "gameId": "anguna", "launchId": "exact" } }),
    );
    outcomes.insert(
        "app.session.start".into(),
        json!({ "_tag": "Ok", "payload": { "session": { "gameId": "anguna", "launchId": "exact" }, "warnings": [] } }),
    );
    let outcomes: Outcomes = Arc::new(std::sync::Mutex::new(outcomes));
    let requests = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let catalog_reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let base = in_memory_korrid(Arc::clone(&outcomes));
    let korrid = Korrid::new(Arc::new({
        let (requests, reads) = (Arc::clone(&requests), Arc::clone(&catalog_reads));
        move |request: RpcRequest| {
            let (base, requests, reads) = (base.clone(), Arc::clone(&requests), Arc::clone(&reads));
            Box::pin(async move {
                let value = serde_json::to_value(&request).unwrap();
                if value["_tag"] == "app.catalog.snapshot" {
                    reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    // A catalog read takes time on a device.
                    tokio::time::sleep(Duration::from_millis(30)).await;
                }
                requests.lock().unwrap().push(value);
                base.rpc(crate::surface::korrid::Mapping::Unreachable, request)
                    .await
                    .map_err(|failure| Refusal::Unreachable(failure.message))
            })
        }
    }))
    .with_installs(access);
    let directory = tempfile::tempdir().unwrap();
    let intent = crate::overlay_intent::OverlayIntent::default();
    let hub = SurfaceHub::new(
        korrid,
        directory.path().join("private"),
        intent.subscribe(),
        Arc::default(),
    );
    Device {
        hub,
        outcomes,
        requests,
        catalog_reads,
        _directory: directory,
    }
}

fn only_game(model: &SurfaceModel) -> korri_surface_treaty::SurfaceGame {
    match &model.catalog {
        SurfaceCatalog::Ready { window, .. } => {
            assert_eq!(window.len(), 1, "one card: {window:?}");
            window[0].clone()
        }
        other => panic!("{other:?}"),
    }
}

/// The library now holds the release, as discovery and the catalog report
/// it a moment after the install ends.
fn library_holds_it(outcomes: &Outcomes) {
    let mut outcomes = outcomes.lock().unwrap();
    outcomes.insert("app.catalog.snapshot".into(), catalog_outcome(true));
    outcomes.insert("app.local-games.list".into(), local_games_outcome(true));
}

async fn install_from_the_hub(device: &Device, access: &Configured, play: bool) {
    let model = settled(&device.hub, "velvet").await;
    let game = only_game(&model);
    assert_eq!(game.install_here, Some(SurfaceInstallHere::Offered));
    device
        .hub
        .request(
            "velvet",
            SurfaceRequest::InstallGame {
                game_id: game.id,
                play,
            },
        )
        .unwrap();
    until("started", || !access.started.lock().unwrap().is_empty()).await;
    running(access, 1_000, 1_000);
    tokio::time::sleep(Duration::from_millis(40)).await;
}

/// Bug 1 of the device check: an install without play ended, and the
/// catalog stayed stale until a Reload.
#[tokio::test]
async fn an_install_without_play_that_lands_reads_the_catalog_again() {
    let access = gba_runner_here();
    *access.check.lock().unwrap() = fits(1_000);
    let device = device(access.clone());
    install_from_the_hub(&device, &access, false).await;
    let before = device
        .catalog_reads
        .load(std::sync::atomic::Ordering::SeqCst);
    library_holds_it(&device.outcomes);
    access.set_job(
        "j1",
        DeviceInstallState::Imported {
            game_id: "anguna".into(),
        },
    );
    access.line.lock().unwrap().clear();
    let model = model_until(&device.hub, "the local game", |model| {
        matches!(&model.catalog, SurfaceCatalog::Ready { window, .. }
            if window.len() == 1 && window[0].install_here.is_none())
    })
    .await;
    assert!(
        device
            .catalog_reads
            .load(std::sync::atomic::Ordering::SeqCst)
            > before
    );
    let game = only_game(&model);
    assert_eq!(game.section.as_deref(), Some("This device"));
    assert_eq!(model.installs, Some(Vec::new()));
    // Install only: nothing starts.
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(
        !device
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|call| call["_tag"] == "app.session.reserve"
                || call["_tag"] == "app.session.prepare")
    );
}

/// Bug 2 of the device check: play here installed the game, looked for it
/// before the catalog held it, and failed to start it. It must wait for a
/// catalog that holds the local copy, then start that copy, never the
/// stream.
#[tokio::test]
async fn play_here_waits_for_the_catalog_then_starts_the_local_copy() {
    let access = gba_runner_here();
    *access.check.lock().unwrap() = fits(1_000);
    let device = device(access.clone());
    install_from_the_hub(&device, &access, true).await;
    access.set_job(
        "j1",
        DeviceInstallState::Imported {
            game_id: "anguna".into(),
        },
    );
    access.line.lock().unwrap().clear();
    // Discovery records it a while after the install ends: the first
    // catalog reads after the end do not hold it yet.
    let outcomes = Arc::clone(&device.outcomes);
    let reads = Arc::clone(&device.catalog_reads);
    let at_end = reads.load(std::sync::atomic::Ordering::SeqCst);
    tokio::spawn(async move {
        while reads.load(std::sync::atomic::Ordering::SeqCst) < at_end + 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        library_holds_it(&outcomes);
    });
    until("the local start", || {
        device
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|call| call["_tag"] == "app.session.reserve")
    })
    .await;
    let requests = device.requests.lock().unwrap().clone();
    let reserve = requests
        .iter()
        .find(|call| call["_tag"] == "app.session.reserve")
        .unwrap();
    assert_eq!(reserve["payload"]["gameId"], json!("anguna"));
    assert!(
        !requests
            .iter()
            .any(|call| call["_tag"] == "app.session.prepare"),
        "never the stream"
    );
    let model = device.hub.model("velvet").unwrap();
    assert!(
        !matches!(model.status, SurfaceStatus::Problem { .. }),
        "{:?}",
        model.status
    );
}

/// The library never shows the release: the person learns why and can
/// retry, and Retry tries the start again.
#[tokio::test]
async fn play_here_that_cannot_find_the_installed_game_says_so_and_retries() {
    let access = gba_runner_here();
    *access.check.lock().unwrap() = fits(1_000);
    let device = device(access.clone());
    install_from_the_hub(&device, &access, true).await;
    access.set_job(
        "j1",
        DeviceInstallState::Imported {
            game_id: "anguna".into(),
        },
    );
    access.line.lock().unwrap().clear();
    let model = model_until(&device.hub, "the problem", |model| {
        matches!(model.status, SurfaceStatus::Problem { .. })
    })
    .await;
    let SurfaceStatus::Problem {
        kicker, can_retry, ..
    } = model.status
    else {
        unreachable!()
    };
    assert_eq!(kicker, "Couldn't start Anguna");
    assert!(can_retry);
    library_holds_it(&device.outcomes);
    device.hub.request("velvet", SurfaceRequest::Retry).unwrap();
    until("the local start", || {
        device
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|call| call["_tag"] == "app.session.reserve")
    })
    .await;
}

async fn model_until(
    hub: &Arc<SurfaceHub>,
    what: &str,
    ready: impl Fn(&SurfaceModel) -> bool,
) -> SurfaceModel {
    for _ in 0..1000 {
        let model = hub.model("velvet").unwrap();
        if ready(&model) {
            return model;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("never: {what}: {:?}", hub.model("velvet").unwrap())
}
