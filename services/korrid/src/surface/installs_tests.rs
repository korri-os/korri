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
    SurfaceInstallBlock, SurfaceInstallHere, SurfaceInstallState, SurfaceProgress, SurfaceTrue,
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
}

impl Configured {
    fn set_job(&self, job: &str, state: DeviceInstallState) {
        self.jobs.lock().unwrap().insert(
            job.into(),
            DeviceInstallStatus {
                job_id: job.into(),
                release_key: KEY.into(),
                state,
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
    streamed: std::sync::Mutex<Vec<(String, Option<String>)>>,
    busy: std::sync::atomic::AtomicBool,
}

fn controller(
    access: Arc<Configured>,
    outcomes: HashMap<String, Value>,
    facts: PluginFacts,
) -> (Arc<Installs>, Arc<Calls>) {
    let calls = Arc::new(Calls::default());
    let (played, streamed, busy) = (calls.clone(), calls.clone(), calls.clone());
    let installs = Installs::new(
        in_memory_korrid(Arc::new(std::sync::Mutex::new(outcomes))).with_installs(access),
        Arc::new(|| {}),
        InstallsHost {
            state: Box::new(|| ready(vec![peer_game(Some(KEY))], None, None)),
            plugin_facts: Box::new(move || facts.clone()),
            play: Box::new(move |id| played.played.lock().unwrap().push(id)),
            stream: Box::new(move |id, location| {
                streamed.streamed.lock().unwrap().push((id, location))
            }),
            library_changed: Box::new(|| {}),
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
    assert_eq!(*calls.played.lock().unwrap(), ["local-anguna"]);
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
            seconds_left: Some(10.0)
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
