//! Port of `clients/portal/src/surface/surface-model.test.ts`, and the hub,
//! catalog window, focus, preference and watch tests of the native surface
//! wire.

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::{SinkExt, StreamExt};
use korri_surface_treaty::{
    CatalogFrom, CatalogOrder, CatalogQuery, SurfaceAction, SurfaceCatalog, SurfaceModel,
    SurfacePreferenceDeclaration, SurfacePreferenceMark, SurfaceRequest, SurfaceStatus,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use super::{
    downloads::GameDownloadFacts,
    korrid::{Korrid, Refusal},
    launchables::state::{Content, Entry, LaunchNotice, LaunchSubject, LaunchablesState},
    model::*,
    SurfaceHub,
};
use crate::{RpcRequest, RpcResponse};

fn from<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("fixture decodes")
}

fn source(label: &str) -> Value {
    json!({ "label": label, "isLocal": false })
}

fn ready(
    entries: Vec<Entry>,
    notice: Option<&str>,
    subject: Option<LaunchSubject>,
) -> LaunchablesState {
    LaunchablesState::recover_startup(
        Content {
            entries,
            notice: notice.map(|message| LaunchNotice::Launch {
                message: message.into(),
                subject,
            }),
            ..Content::default()
        },
        None,
        &[],
    )
}

fn local_game() -> Entry {
    Entry::LocalGame {
        game: from(json!({ "id": "wl4", "title": "Wario Land 4", "system": "GBA" })),
        alternatives: Vec::new(),
    }
}

fn host_game_json() -> Value {
    json!({ "id": "neverball", "title": "Neverball", "host": "zao", "supportsRunnerSelection": false, "source": source("zao") })
}

fn host_game() -> Entry {
    Entry::Game {
        game: from(host_game_json()),
        alternatives: Vec::new(),
    }
}

fn now_playing() -> Entry {
    Entry::NowPlaying {
        session: from(json!({ "launchId": "L1", "title": "Skate 3", "host": "aka" })),
        local_catalog_game: None,
    }
}

fn games(state: &LaunchablesState) -> Vec<korri_surface_treaty::SurfaceGame> {
    catalog_games(state).expect("not loading")
}

mod surface_model {
    use super::*;
    use crate::surface::launchables::fold::GameCopy;

    #[test]
    fn empty_failed_catalogs_keep_their_cause() {
        let state = LaunchablesState::from_sources(
            &from(
                json!({ "_tag": "Ok", "payload": { "games": [], "failures": [
                { "host": "rpminiv2", "code": "LocalRomMissing", "message": "The ROM file is missing" }
            ] } }),
            ),
            None,
            None,
            &[],
            None,
        );
        assert_eq!(
            serde_json::to_value(status_from(&state)).unwrap(),
            json!({ "_tag": "Problem", "kicker": "Catalog problem", "reason": "rpminiv2: LocalRomMissing: The ROM file is missing", "canRetry": false })
        );
        assert!(games(&state).is_empty());
    }

    #[test]
    fn stop_failures_are_not_labelled_as_attempted_launches() {
        let stopping = ready(vec![now_playing()], None, None).begin_stopping(&now_playing());
        let state = stopping.stop_timed_out();
        assert!(
            matches!(status_from(&state), SurfaceStatus::Problem { kicker, .. } if kicker == "Couldn't end the session")
        );
    }

    #[test]
    fn labels_a_source_local_catalog_route_as_this_device() {
        let mut local = host_game_json();
        local["source"] = json!({ "label": "device-label", "isLocal": true });
        let mut aka = host_game_json();
        aka["host"] = json!("aka");
        aka["source"] = source("aka");
        let entry = Entry::Game {
            game: from(local),
            alternatives: vec![GameCopy::Remote(from(aka))],
        };
        let state = ready(vec![entry.clone()], None, None);
        let rendered = &games(&state)[0];
        assert_eq!(rendered.section.as_deref(), Some("This device"));
        assert_eq!(
            rendered.subtitle.as_deref(),
            Some("This device · Also on aka")
        );
        let locations = rendered.launch_locations.as_ref().unwrap();
        assert_eq!(
            locations
                .iter()
                .map(|location| location.label.as_str())
                .collect::<Vec<_>>(),
            vec!["This device", "aka"]
        );
        let chosen = entry_for_launch_location(&entry, &locations[0].id).unwrap();
        assert_eq!(chosen.key(), "game:zao:neverball");
    }

    #[test]
    fn publishes_only_playable_things_and_groups_them_by_where_they_play() {
        let state = ready(vec![now_playing(), local_game(), host_game()], None, None);
        let games = games(&state);
        assert_eq!(
            games
                .iter()
                .map(|game| game.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Skate 3", "Wario Land 4", "Neverball"]
        );
        assert_eq!(
            games
                .iter()
                .map(|game| game.section.clone().unwrap())
                .collect::<Vec<_>>(),
            vec!["Continue", "This device", "zao"]
        );
        assert_eq!(games[0].resumable, Some(true));
    }

    #[test]
    fn never_invents_art_or_metadata_korrid_does_not_have() {
        let state = ready(vec![local_game()], None, None);
        let game = &games(&state)[0];
        assert!(game.cover_art_url.is_none());
        assert_eq!(game.subtitle.as_deref(), Some("GBA"));
        assert!(
            game.last_played_at.is_none()
                && game.play_count.is_none()
                && game.total_playtime_seconds.is_none()
        );
    }

    #[test]
    fn publishes_play_facts_as_utc_millis_and_merges_folded_copies() {
        let entry = Entry::LocalGame {
            game: from(
                json!({ "id": "wl4", "title": "Wario Land 4", "system": "GBA",
                "playStats": { "lastPlayed": "2026-01-02T03:04:05Z", "playCount": 2, "totalPlaytimeSeconds": 60.0 } }),
            ),
            alternatives: vec![GameCopy::Remote(from(
                json!({ "id": "wl4", "title": "Wario Land 4", "host": "zao",
                "supportsRunnerSelection": false, "source": source("zao"),
                "playStats": { "lastPlayed": "2026-03-01T00:00:00Z", "playCount": 1, "totalPlaytimeSeconds": 30.0 } }),
            ))],
        };
        let state = ready(vec![entry], None, None);
        let game = &games(&state)[0];
        assert_eq!(game.last_played_at, Some(1_772_323_200_000.0));
        assert_eq!(game.play_count, Some(3));
        assert_eq!(game.total_playtime_seconds, Some(90.0));
    }

    #[test]
    fn reports_retained_host_copies_without_adding_another_game() {
        let entry = Entry::LocalGame {
            game: from(json!({ "id": "wl4", "title": "Wario Land 4", "system": "GBA" })),
            alternatives: vec![
                GameCopy::Remote(from(
                    json!({ "id": "wl4", "title": "Wario Land 4", "host": "zao", "supportsRunnerSelection": false, "source": source("zao") }),
                )),
                GameCopy::Remote(from(
                    json!({ "id": "wl4-aka", "title": "Wario Land 4", "host": "aka", "supportsRunnerSelection": false, "source": source("aka") }),
                )),
            ],
        };
        let state = ready(vec![entry.clone()], None, None);
        let games = games(&state);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].subtitle.as_deref(), Some("GBA · Also on aka, zao"));
        assert_eq!(
            serde_json::to_value(&games[0].launch_locations).unwrap(),
            json!([
                { "id": "[\"local\",null,\"wl4\"]", "label": "This device" },
                { "id": "[\"remote\",\"aka\",\"wl4-aka\"]", "label": "aka" },
                { "id": "[\"remote\",\"zao\",\"wl4\"]", "label": "zao" }
            ])
        );
        // A host choice resolves to exactly that copy.
        let zao = entry_for_launch_location(&entry, "[\"remote\",\"zao\",\"wl4\"]").unwrap();
        assert_eq!(zao.key(), "game:zao:wl4");
        // A local record without a local catalog route never falls back.
        let local = entry_for_launch_location(&entry, "[\"local\",null,\"wl4\"]").unwrap();
        assert_eq!(local.key(), "local-game:wl4");
        assert!(entry_for_launch_location(&entry, "unknown-location").is_none());
    }

    #[test]
    fn a_notice_becomes_a_problem_the_user_acknowledges() {
        let state = ready(vec![local_game()], Some("local ROM is missing"), None);
        assert_eq!(
            serde_json::to_value(status_from(&state)).unwrap(),
            json!({ "_tag": "Problem", "kicker": "Couldn't start", "reason": "local ROM is missing", "canRetry": false })
        );
    }

    #[test]
    fn a_failure_names_the_game_it_belongs_to() {
        let state = ready(
            vec![local_game()],
            Some("ActiveSessionConflict: a session must end"),
            Some(LaunchSubject {
                id: Some("wl4".into()),
                entry_id: None,
                title: "Wario Land 4".into(),
            }),
        );
        assert_eq!(
            serde_json::to_value(status_from(&state)).unwrap(),
            json!({ "_tag": "Problem", "kicker": "Couldn't start Wario Land 4", "reason": "ActiveSessionConflict: a session must end",
                "canRetry": false, "gameId": "local-game:wl4", "gameTitle": "Wario Land 4" })
        );
    }

    #[test]
    fn in_flight_work_is_busy_and_loading_is_not_empty() {
        let preparing = ready(vec![host_game()], None, None).begin_preparing("Neverball", None);
        assert!(
            matches!(status_from(&preparing), SurfaceStatus::Busy { kicker, .. } if kicker.contains("Neverball"))
        );
        assert!(catalog_games(&LaunchablesState::Loading).is_none());
        assert_eq!(
            status_from(&LaunchablesState::Loading),
            SurfaceStatus::Browsing
        );
    }

    #[test]
    fn only_the_running_session_has_actions_and_stopping_is_destructive() {
        let actions = game_actions_for_entry(Some(&now_playing()), None);
        assert_eq!(
            serde_json::to_value(&actions).unwrap(),
            json!([{ "id": "resume", "label": "Resume", "enabled": true }, { "id": "end", "label": "End", "enabled": true, "destructive": true }])
        );
        assert!(game_actions_for_entry(Some(&local_game()), None).is_empty());
    }

    #[test]
    fn surface_ids_round_trip_and_unknown_ids_resolve_to_nothing() {
        let state = ready(vec![now_playing(), local_game(), host_game()], None, None);
        for game in games(&state) {
            assert_eq!(entry_for_id(&state, &game.id).unwrap().key(), game.id);
        }
        assert!(entry_for_id(&state, "game:nowhere").is_none());
    }

    #[test]
    fn manual_choices_publish_every_exact_phase_and_stable_same_title_labels() {
        use crate::surface::launchables::state::{
            CancelState, ChoicePhase, Loaded, PendingLaunchChoice, Phase,
        };
        use crate::PendingLaunchPhase::*;
        let phases = [
            ChoicePhase::Pending(Reserved),
            ChoicePhase::Pending(Preparing),
            ChoicePhase::Pending(Committing),
            ChoicePhase::Waiting,
            ChoicePhase::Pending(Cancelling),
        ];
        let mut choices: Vec<PendingLaunchChoice> = phases
            .into_iter()
            .enumerate()
            .map(|(index, phase)| PendingLaunchChoice {
                launch_id: index.to_string(),
                game_id: Some("neverball".into()),
                title: if index < 2 {
                    "Same title".into()
                } else {
                    format!("Title {index}")
                },
                phase,
                cancel: CancelState::Idle,
            })
            .collect();
        choices.push(PendingLaunchChoice {
            launch_id: "5".into(),
            game_id: None,
            title: "Retry title".into(),
            phase: ChoicePhase::Pending(Reserved),
            cancel: CancelState::Retry,
        });
        let state = LaunchablesState::Loaded(Loaded {
            phase: Phase::Choosing { choices },
            content: Content {
                entries: vec![host_game()],
                ..Content::default()
            },
        });
        assert_eq!(
            serde_json::to_value(status_from(&state)).unwrap(),
            json!({ "_tag": "Busy", "kicker": "6 launches are starting", "detail": "Cancel each launch you do not want.", "actions": [
                { "id": "cancel-pending:0", "label": "Cancel Same title (1)", "description": "Waiting to start", "enabled": true },
                { "id": "cancel-pending:1", "label": "Cancel Same title (2)", "description": "Preparing", "enabled": true },
                { "id": "cancel-pending:2", "label": "Cancel Title 2", "description": "Starting", "enabled": true },
                { "id": "cancel-pending:3", "label": "Cancel Title 3", "description": "Waiting for its window", "enabled": true },
                { "id": "cancel-pending:4", "label": "Cancel Title 4", "description": "Cancelling", "enabled": false },
                { "id": "cancel-pending:5", "label": "Cancel Retry title", "description": "Cancel failed. Try again.", "enabled": true }
            ] })
        );
        let sent = state.begin_pending_cancellation("0").unwrap();
        let SurfaceStatus::Busy {
            actions: Some(actions),
            ..
        } = status_from(&sent)
        else {
            panic!("expected Busy")
        };
        assert!(!actions[0].enabled);
    }

    fn idle() -> GameDownloadFacts {
        GameDownloadFacts {
            needs: false,
            bytes: None,
            active: false,
            stoppable: false,
            shared: false,
            queued: false,
            progress: None,
            own_ready: false,
            failure: None,
        }
    }

    fn ids(facts: GameDownloadFacts, live: bool, enabled: bool) -> Vec<String> {
        let entry: Entry = Entry::Game {
            game: from(
                json!({ "id": "mm", "title": "Mega Man Rock N Roll", "supportsRunnerSelection": false, "source": { "label": "This device", "isLocal": true } }),
            ),
            alternatives: Vec::new(),
        };
        game_actions_for_entry(
            Some(&entry),
            Some(&GameDownloadContext {
                facts,
                live,
                enabled,
            }),
        )
        .into_iter()
        .map(|action| {
            format!(
                "{}{}",
                action.id,
                if action.enabled { "" } else { " (dimmed)" }
            )
        })
        .collect()
    }

    #[test]
    fn download_actions_follow_adr_0013() {
        let progress = Some(korri_surface_treaty::SurfaceProgress {
            done: 1.0,
            total: Some(2.0),
        });
        assert_eq!(
            ids(
                GameDownloadFacts {
                    needs: true,
                    bytes: Some(500.0),
                    ..idle()
                },
                false,
                true
            ),
            vec!["download"]
        );
        assert_eq!(
            ids(
                GameDownloadFacts {
                    needs: true,
                    active: true,
                    stoppable: true,
                    queued: true,
                    ..idle()
                },
                false,
                true
            ),
            vec!["cancel-download"]
        );
        assert_eq!(
            ids(
                GameDownloadFacts {
                    needs: true,
                    active: true,
                    stoppable: true,
                    progress: progress.clone(),
                    ..idle()
                },
                false,
                true
            ),
            vec!["cancel-download"]
        );
        assert_eq!(
            ids(
                GameDownloadFacts {
                    needs: true,
                    active: true,
                    ..idle()
                },
                false,
                true
            ),
            vec!["cancel-download (dimmed)"]
        );
        assert_eq!(
            ids(
                GameDownloadFacts {
                    needs: true,
                    active: true,
                    shared: true,
                    progress,
                    ..idle()
                },
                false,
                true
            ),
            vec!["cancel-download (dimmed)"]
        );
        assert_eq!(
            ids(
                GameDownloadFacts {
                    own_ready: true,
                    ..idle()
                },
                false,
                true
            ),
            vec!["remove-download"]
        );
        assert_eq!(
            ids(
                GameDownloadFacts {
                    needs: true,
                    own_ready: true,
                    ..idle()
                },
                false,
                true
            ),
            vec!["download", "remove-download"]
        );
        assert!(ids(idle(), false, true).is_empty());
        assert!(ids(
            GameDownloadFacts {
                own_ready: true,
                ..idle()
            },
            true,
            true
        )
        .is_empty());
        assert_eq!(
            ids(
                GameDownloadFacts {
                    needs: true,
                    ..idle()
                },
                false,
                false
            ),
            vec!["download (dimmed)"]
        );
        assert_eq!(
            serde_json::to_value(surface_download_from(Some(&GameDownloadFacts {
                needs: true,
                bytes: Some(500.0),
                active: true,
                queued: true,
                ..idle()
            })))
            .unwrap(),
            json!({ "queued": true, "bytes": 500.0 })
        );
        assert!(surface_download_from(Some(&GameDownloadFacts {
            own_ready: true,
            ..idle()
        }))
        .is_none());
        assert!(surface_download_from(None).is_none());
    }
}

// ---------------------------------------------------------------------------
// The hub over an in-memory korrid
// ---------------------------------------------------------------------------

/// A real korrid stand-in configured with one outcome per request tag, the
/// way the portal's in-memory client answered. Unknown tags are unreachable.
fn in_memory_korrid(outcomes: Arc<std::sync::Mutex<HashMap<String, Value>>>) -> Korrid {
    Korrid::new(Arc::new(move |request: RpcRequest| {
        let outcomes = Arc::clone(&outcomes);
        Box::pin(async move {
            let tag = serde_json::to_value(&request).unwrap()["_tag"]
                .as_str()
                .unwrap()
                .to_owned();
            let outcome = outcomes.lock().unwrap().get(&tag).cloned();
            match outcome {
                Some(outcome) => serde_json::from_value::<RpcResponse>(
                    json!({ "_tag": tag, "outcome": outcome }),
                )
                .map_err(|error| Refusal::Unreachable(error.to_string())),
                None => Err(Refusal::Unreachable(format!("no answer for {tag}"))),
            }
        })
    }))
}

fn idle_outcomes(game_count: usize) -> HashMap<String, Value> {
    let games: Vec<Value> = (0..game_count)
        .map(|index| json!({ "id": format!("g{index:05}"), "title": format!("Game {index:05}"), "supportsRunnerSelection": false,
            "source": { "label": "This device", "isLocal": true } }))
        .collect();
    let unsupported =
        json!({ "_tag": "Err", "payload": { "code": "OperationUnsupported", "message": "no" } });
    [
        (
            "app.catalog.snapshot",
            json!({ "_tag": "Ok", "payload": { "games": games } }),
        ),
        (
            "app.local-games.list",
            json!({ "_tag": "Ok", "payload": { "games": [] } }),
        ),
        ("app.session.status", json!({ "_tag": "Ok", "payload": {} })),
        (
            "app.peer.list",
            json!({ "_tag": "Ok", "payload": { "peers": [] } }),
        ),
        (
            "system.health",
            json!({ "_tag": "Ok", "payload": { "version": "test" } }),
        ),
        ("system.settings.snapshot", unsupported.clone()),
        ("app.library.folders.snapshot", unsupported.clone()),
        ("system.identity.status", unsupported.clone()),
        (
            "system.identity.owner-binding.snapshot",
            unsupported.clone(),
        ),
        ("system.network.status", unsupported.clone()),
        (
            "system.onboarding.status",
            json!({ "_tag": "Ok", "payload": { "needed": false } }),
        ),
        (
            "app.acquisition.status",
            json!({ "_tag": "Ok", "payload": { "artifacts": [] } }),
        ),
        ("system.plugins.repositories", unsupported),
    ]
    .into_iter()
    .map(|(tag, outcome)| (tag.to_owned(), outcome))
    .collect()
}

type Outcomes = Arc<std::sync::Mutex<HashMap<String, Value>>>;

thread_local! {
    /// The Home intent of the hub a test made last.
    static INTENT: std::cell::RefCell<Option<Arc<crate::overlay_intent::OverlayIntent>>> = const { std::cell::RefCell::new(None) };
}

fn hub_with(outcomes: HashMap<String, Value>) -> (Arc<SurfaceHub>, Outcomes, tempfile::TempDir) {
    let outcomes = Arc::new(std::sync::Mutex::new(outcomes));
    let directory = tempfile::tempdir().unwrap();
    let intent = Arc::new(crate::overlay_intent::OverlayIntent::default());
    let hub = SurfaceHub::new(
        in_memory_korrid(Arc::clone(&outcomes)),
        directory.path().join("private"),
        intent.subscribe(),
        Arc::default(),
    );
    INTENT.with(|slot| *slot.borrow_mut() = Some(intent));
    (hub, outcomes, directory)
}

/// What a Home freeze does to korrid's intent.
fn home_intent(launch_id: &str) {
    INTENT.with(|slot| {
        *slot.borrow().as_ref().unwrap().lock().unwrap() = Some(launch_id.into());
    });
}

async fn settled(hub: &Arc<SurfaceHub>, surface: &str) -> SurfaceModel {
    for _ in 0..200 {
        let model = hub.model(surface).unwrap();
        if !matches!(model.catalog, SurfaceCatalog::Loading) {
            return model;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the catalog never loaded")
}

fn window_ids(model: &SurfaceModel) -> (u32, u32, Vec<String>) {
    let SurfaceCatalog::Ready {
        total,
        first_index,
        window,
        ..
    } = &model.catalog
    else {
        panic!("expected Ready, got {:?}", model.catalog)
    };
    (
        *total,
        *first_index,
        window.iter().map(|game| game.id.clone()).collect(),
    )
}

#[tokio::test]
async fn the_default_window_is_fifty_games_from_the_start_and_each_surface_moves_its_own() {
    let (hub, _, _directory) = hub_with(idle_outcomes(120));
    let model = settled(&hub, "pico").await;
    let (total, first, window) = window_ids(&model);
    assert_eq!((total, first, window.len()), (120, 0, 50));
    assert_eq!(window[0], "game:g00000");
    let SurfaceCatalog::Ready { after, query, .. } = &model.catalog else {
        unreachable!()
    };
    assert_eq!(*query, CatalogQuery::default());
    hub.request(
        "pico",
        SurfaceRequest::MoveCatalog {
            query: CatalogQuery::default(),
            from: CatalogFrom::After {
                cursor: after.clone().unwrap(),
            },
            count: 500,
        },
    )
    .unwrap();
    let (_, first, window) = window_ids(&hub.model("pico").unwrap());
    assert_eq!((first, window.len()), (50, 70));
    // Zone keeps its own query.
    assert_eq!(window_ids(&hub.model("zone").unwrap()).1, 0);
    hub.request(
        "zone",
        SurfaceRequest::MoveCatalog {
            query: CatalogQuery {
                text: " GAME 0011".into(),
                section: None,
                order: CatalogOrder::Title,
            },
            from: CatalogFrom::Start,
            count: 50,
        },
    )
    .unwrap();
    let (total, _, window) = window_ids(&hub.model("zone").unwrap());
    assert_eq!(total, 10);
    assert_eq!(window[0], "game:g00110");
    assert_eq!(window_ids(&hub.model("pico").unwrap()).1, 50);
}

#[tokio::test]
async fn an_empty_library_is_empty_and_a_bad_surface_id_is_refused() {
    let (hub, _, _directory) = hub_with(idle_outcomes(0));
    assert_eq!(settled(&hub, "pico").await.catalog, SurfaceCatalog::Empty);
    assert_eq!(hub.model("../pico").unwrap_err().code, "InvalidSurfaceId");
    assert_eq!(hub.model("").unwrap_err().code, "InvalidSurfaceId");
}

#[tokio::test]
async fn focus_game_publishes_that_games_actions_only_for_a_game_korri_knows() {
    let mut outcomes = idle_outcomes(3);
    outcomes.insert(
        "app.session.status".into(),
        json!({ "_tag": "Ok", "payload": { "active": { "launchId": "L1", "gameId": "g00001", "title": "Game 00001", "phase": "running", "initialHandoff": "observed" } } }),
    );
    let (hub, _, _directory) = hub_with(outcomes);
    let model = settled(&hub, "pico").await;
    assert!(model.focused_game.is_none());
    hub.request(
        "pico",
        SurfaceRequest::FocusGame {
            game_id: Some("nope".into()),
        },
    )
    .unwrap();
    assert!(hub.model("pico").unwrap().focused_game.is_none());
    hub.request(
        "pico",
        SurfaceRequest::FocusGame {
            game_id: Some("now-playing:L1".into()),
        },
    )
    .unwrap();
    let focused = hub.model("pico").unwrap().focused_game.unwrap();
    assert_eq!(focused.game_id, "now-playing:L1");
    assert_eq!(
        focused.actions,
        vec![
            SurfaceAction::new("resume", "Resume", true),
            SurfaceAction::new("end", "End", true).destructive()
        ]
    );
    hub.request(
        "pico",
        SurfaceRequest::FocusGame {
            game_id: Some("game:g00002".into()),
        },
    )
    .unwrap();
    let focused = hub.model("pico").unwrap().focused_game.unwrap();
    assert_eq!(
        focused
            .actions
            .iter()
            .map(|action| action.id.as_str())
            .collect::<Vec<_>>(),
        vec!["launch-settings"]
    );
    hub.request("pico", SurfaceRequest::FocusGame { game_id: None })
        .unwrap();
    assert!(hub.model("pico").unwrap().focused_game.is_none());
}

#[tokio::test]
async fn preferences_follow_the_declaration() {
    let (hub, _, _directory) = hub_with(idle_outcomes(0));
    let remember = |value: &str| SurfaceRequest::RememberPreference {
        key: "zone.look".into(),
        value: value.into(),
    };
    assert_eq!(
        hub.request("zone", remember("retro")).unwrap_err().code,
        "SurfaceNotDeclared"
    );
    assert_eq!(
        hub.preferences("zone").unwrap_err().code,
        "SurfaceNotDeclared"
    );
    hub.declare(
        "zone",
        [(
            "zone.look".to_owned(),
            SurfacePreferenceDeclaration {
                mark: SurfacePreferenceMark::Shared,
                initial: "modern".into(),
            },
        )]
        .into(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(hub.preferences("zone").unwrap()["zone.look"], "modern");
    hub.request("zone", remember("retro")).unwrap();
    assert_eq!(hub.preferences("zone").unwrap()["zone.look"], "retro");
    assert_eq!(
        hub.request(
            "zone",
            SurfaceRequest::RememberPreference {
                key: "zone.other".into(),
                value: "x".into()
            }
        )
        .unwrap_err()
        .code,
        "PreferenceRefused"
    );
}

#[tokio::test]
async fn a_launch_request_reserves_and_starts_the_exact_local_game() {
    let mut outcomes = idle_outcomes(2);
    outcomes.insert(
        "app.session.reserve".into(),
        json!({ "_tag": "Ok", "payload": { "gameId": "g00001", "launchId": "exact" } }),
    );
    outcomes.insert(
        "app.session.start".into(),
        json!({ "_tag": "Ok", "payload": { "session": { "gameId": "g00001", "launchId": "exact" }, "warnings": [] } }),
    );
    let (hub, outcomes, _directory) = hub_with(outcomes);
    settled(&hub, "pico").await;
    // korrid keeps reporting the exact pending launch until it hands off.
    outcomes.lock().unwrap().insert(
        "app.session.status".into(),
        json!({ "_tag": "Ok", "payload": { "pendingLaunches": [
            { "session": { "gameId": "g00001", "launchId": "exact" }, "phase": "committing" }
        ] } }),
    );
    hub.request(
        "pico",
        SurfaceRequest::LaunchGame {
            game_id: "game:g00001".into(),
            launch_location_id: None,
        },
    )
    .unwrap();
    let mut busy = None;
    for _ in 0..200 {
        if let SurfaceStatus::Busy {
            kicker,
            actions,
            game_id,
            ..
        } = hub.model("pico").unwrap().status
        {
            busy = Some((kicker, actions, game_id));
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let (kicker, actions, game_id) = busy.unwrap_or_else(|| {
        panic!(
            "the launch shows as Busy: {:?} {:?}",
            hub.model("pico").unwrap().status,
            hub.root().launchables.state()
        )
    });
    assert_eq!(kicker, "Starting Game 00001…");
    assert_eq!(game_id.as_deref(), Some("game:g00001"));
    assert_eq!(actions.unwrap()[0].id, "cancel-launch");
    // The exact launch hands off: the status observation resolves it.
    outcomes.lock().unwrap().insert(
        "app.session.status".into(),
        json!({ "_tag": "Ok", "payload": { "active": { "launchId": "exact", "gameId": "g00001", "phase": "running", "initialHandoff": "observed" } } }),
    );
    for _ in 0..400 {
        let model = hub.model("pico").unwrap();
        if model.status == SurfaceStatus::Browsing {
            let (_, _, window) = window_ids(&model);
            assert_eq!(window[0], "now-playing:exact");
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the handoff never resolved the launch");
}

/// The watch rides the native-input socket: Origin, Bearer, then the watch
/// frame. Pushes wait for their exact ack.
#[tokio::test]
async fn watch_pushes_whole_models_and_waits_for_each_ack() {
    use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
    let (hub, _, _directory) = hub_with(idle_outcomes(60));
    let slot: super::SurfaceSlot = Arc::default();
    let _ = slot.hub.set(Arc::clone(&hub));
    let access = crate::PortalAccess::new(
        "watch-capability",
        "http://portal.test",
        crate::PortalPermission::Full,
    );
    let (router, _source) =
        crate::portal_input::router_with_surfaces(access, slot, Default::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut request = format!("ws://{address}/").into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Origin", "http://portal.test".parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket
        .send(Message::Text("Bearer watch-capability".into()))
        .await
        .unwrap();
    socket
        .send(Message::Text(
            json!({ "_tag": "app.surface.watch", "payload": { "surfaceId": "pico" } })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    // Until the catalog loads, pushes arrive as the model changes.
    let mut push = receive(&mut socket).await;
    loop {
        assert_eq!(push["kind"], "model");
        let revision = push["revision"].as_str().unwrap().to_owned();
        assert!(revision.parse::<u64>().is_ok());
        socket
            .send(Message::Text(
                json!({ "kind": "ack", "revision": revision })
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        if push["model"]["catalog"]["_tag"] == "Ready" {
            break;
        }
        push = receive(&mut socket).await;
    }
    assert_eq!(push["model"]["catalog"]["total"], 60);
    assert_eq!(push["model"]["catalog"]["firstIndex"], 0);
    // A move by this surface is pushed as a whole new model.
    hub.request(
        "pico",
        SurfaceRequest::MoveCatalog {
            query: CatalogQuery::default(),
            from: CatalogFrom::End,
            count: 10,
        },
    )
    .unwrap();
    let mut moved = None;
    for _ in 0..20 {
        let push = receive(&mut socket).await;
        let revision = push["revision"].as_str().unwrap().to_owned();
        socket
            .send(Message::Text(
                json!({ "kind": "ack", "revision": revision })
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        if push["model"]["catalog"]["firstIndex"] == 50 {
            moved = Some(push);
            break;
        }
    }
    let moved = moved.expect("the moved window was pushed");
    assert_eq!(
        moved["model"]["catalog"]["window"]
            .as_array()
            .unwrap()
            .len(),
        10
    );
    assert!(moved["model"]["catalog"].get("after").is_none());
    // A wrong ack closes the watch.
    hub.request(
        "pico",
        SurfaceRequest::FocusGame {
            game_id: Some("game:g00001".into()),
        },
    )
    .unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap();
    socket
        .send(Message::Text(
            json!({ "kind": "ack", "revision": "0" }).to_string().into(),
        ))
        .await
        .unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap();
    assert!(matches!(
        closed,
        None | Some(Ok(Message::Close(_))) | Some(Err(_))
    ));
}

async fn receive<S>(socket: &mut S) -> Value
where
    S: futures::Stream<
            Item = Result<
                tokio_tungstenite::tungstenite::Message,
                tokio_tungstenite::tungstenite::Error,
            >,
        > + Unpin,
{
    let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let tokio_tungstenite::tungstenite::Message::Text(text) = message else {
        panic!("expected text")
    };
    serde_json::from_str(&text).unwrap()
}

mod overlay {
    use super::*;
    use crate::surface::overlay::gameplay_overlay_presentation_from;

    /// Port of `overlay-model.test.ts`: authority is stripped, disabled
    /// controls and empty groups are omitted, core controls come first.
    #[test]
    fn presentation_strips_authority_and_omits_disabled_controls() {
        let controls: crate::SessionControls = from(json!({
            "launchId": "L1", "title": "Wario Land 4",
            "groups": [
                { "id": "retroarch", "label": "RetroArch", "controls": [
                    { "id": "save", "label": "Save state", "enabled": true, "destructive": false, "dismissOnSuccess": true,
                      "interaction": { "kind": "command" } },
                    { "id": "speed", "label": "Speed", "enabled": true, "destructive": false, "dismissOnSuccess": false,
                      "interaction": { "kind": "range", "payload": { "value": 1.0, "min": 0.5, "max": 2.0, "step": 0.5 } } },
                    { "id": "off", "label": "Off", "enabled": false, "disabledReason": "No", "destructive": false, "dismissOnSuccess": false,
                      "interaction": { "kind": "command" } }
                ] },
                { "id": "empty", "label": "Empty", "controls": [
                    { "id": "gone", "label": "Gone", "enabled": false, "destructive": false, "dismissOnSuccess": false,
                      "interaction": { "kind": "toggle", "payload": { "value": true, "trueLabel": "On", "falseLabel": "Off" } } }
                ] }
            ]
        }));
        let presentation = gameplay_overlay_presentation_from(&controls, &["Simon".into()]);
        let value = serde_json::to_value(&presentation).unwrap();
        assert_eq!(value["kind"], "gameplay-overlay");
        assert_eq!(value["title"], "Wario Land 4");
        assert_eq!(
            value["controls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|control| control["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["overlay:resume", "overlay:end", "overlay:open-korri"]
        );
        assert_eq!(value["groups"].as_array().unwrap().len(), 1);
        assert_eq!(
            value["groups"][0]["controls"][1]["interaction"],
            json!({ "kind": "range", "value": 1.0, "min": 0.5, "max": 2.0, "step": 0.5 })
        );
        assert_eq!(value["groups"][0]["controls"].as_array().unwrap().len(), 2);
        assert_eq!(value["streamPlayers"], json!(["Simon"]));
        assert!(serde_json::to_string(&value)
            .unwrap()
            .find("launchId")
            .is_none());
    }

    #[tokio::test]
    async fn a_home_frozen_launch_shows_the_overlay_and_open_korri_returns_to_the_catalog() {
        let mut outcomes = idle_outcomes(2);
        let frozen = json!({ "launchId": "L1", "gameId": "g00001", "title": "Game 00001", "phase": "frozen", "initialHandoff": "observed" });
        outcomes.insert(
            "app.session.status".into(),
            json!({ "_tag": "Ok", "payload": { "active": frozen, "overlay": frozen } }),
        );
        outcomes.insert(
            "app.session.controls".into(),
            json!({ "_tag": "Ok", "payload": { "launchId": "L1", "title": "Game 00001", "groups": [] } }),
        );
        let (hub, _, _directory) = hub_with(outcomes);
        hub.model("pico").unwrap();
        home_intent("L1");
        let mut shown = None;
        for _ in 0..400 {
            let model = hub.model("pico").unwrap();
            if let SurfacePresentation::GameplayOverlay(presentation) = &model.presentation {
                if model.status == SurfaceStatus::Browsing {
                    shown = Some(presentation.clone());
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let presentation = shown.expect("the overlay shows");
        assert_eq!(presentation.title.as_deref(), Some("Game 00001"));
        hub.request(
            "pico",
            SurfaceRequest::InvokeGameplayControl {
                control_id: "overlay:open-korri".into(),
                value: None,
            },
        )
        .unwrap();
        for _ in 0..200 {
            if matches!(
                hub.model("pico").unwrap().presentation,
                SurfacePresentation::Catalog(_)
            ) {
                // Still frozen: Open Korri keeps the catalog in front.
                tokio::time::sleep(Duration::from_millis(1300)).await;
                assert!(matches!(
                    hub.model("pico").unwrap().presentation,
                    SurfacePresentation::Catalog(_)
                ));
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("Open Korri never showed the catalog");
    }

    use korri_surface_treaty::SurfacePresentation;
}

mod acquisition_gate {
    //! Port of `acquisition-gate.test.ts`.
    use super::*;
    use crate::surface::{
        acquisition_gate::{confirm_detail, format_eta, AcquisitionGameRef, AcquisitionGate},
        shared_play::format_bytes,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Each call takes the next scripted answer; the last answer stays
    /// current, as a polled daemon's state would.
    #[derive(Default)]
    pub(super) struct Script {
        answers: HashMap<String, Vec<Value>>,
        last: HashMap<String, Value>,
        pub(super) calls: Vec<(String, Value)>,
    }

    pub(super) fn scripted() -> (Korrid, Arc<std::sync::Mutex<Script>>) {
        let script = Arc::new(std::sync::Mutex::new(Script::default()));
        let shared = Arc::clone(&script);
        let korrid = Korrid::new(Arc::new(move |request: RpcRequest| {
            let script = Arc::clone(&shared);
            Box::pin(async move {
                let value = serde_json::to_value(&request).unwrap();
                let tag = value["_tag"].as_str().unwrap().to_owned();
                let mut script = script.lock().unwrap();
                script.calls.push((tag.clone(), value["payload"].clone()));
                let next = script
                    .answers
                    .get_mut(&tag)
                    .and_then(|queue| (!queue.is_empty()).then(|| queue.remove(0)));
                let answer = match next {
                    Some(answer) => {
                        script.last.insert(tag.clone(), answer.clone());
                        answer
                    }
                    None => script
                        .last
                        .get(&tag)
                        .cloned()
                        .ok_or_else(|| Refusal::Unreachable(format!("no answer for {tag}")))?,
                };
                serde_json::from_value::<RpcResponse>(json!({ "_tag": tag, "outcome": answer }))
                    .map_err(|error| Refusal::Unreachable(error.to_string()))
            })
        }));
        (korrid, script)
    }

    pub(super) fn push(script: &Arc<std::sync::Mutex<Script>>, tag: &str, answer: Value) {
        script
            .lock()
            .unwrap()
            .answers
            .entry(tag.into())
            .or_default()
            .push(answer);
    }

    fn rootfs(state: Value) -> Value {
        json!({ "id": "@korri:fex/rootfs", "title": "FEX rootfs", "version": "1", "bytesTotal": 1_160_000_000u64, "state": state })
    }
    fn proton(state: Value) -> Value {
        json!({ "id": "@korri:proton/cachyos", "title": "Proton-CachyOS", "version": "11.0", "bytesTotal": 340_000_000u64, "state": state })
    }
    fn ok(artifacts: Vec<Value>) -> Value {
        json!({ "_tag": "Ok", "payload": { "artifacts": artifacts } })
    }
    fn missing() -> Value {
        json!({ "_tag": "Missing" })
    }
    fn ready() -> Value {
        json!({ "_tag": "Ready", "path": "/cache/x" })
    }
    fn game() -> AcquisitionGameRef {
        AcquisitionGameRef {
            id: "steam:1".into(),
            surface_id: "game:steam:1".into(),
            title: "Hollow Knight".into(),
            runner_id: None,
        }
    }
    fn gate(korrid: Korrid) -> Arc<AcquisitionGate> {
        AcquisitionGate::new(
            korrid,
            Arc::new(|| {}),
            Arc::new(|| {}),
            Duration::from_millis(1),
        )
    }
    async fn settle() {
        for _ in 0..20 {
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    fn action_id(status: Option<SurfaceStatus>, label: &str) -> String {
        let actions = match status {
            Some(SurfaceStatus::Confirm { actions, .. }) => actions,
            Some(SurfaceStatus::Busy {
                actions: Some(actions),
                ..
            }) => actions,
            other => panic!("no actions on {other:?}"),
        };
        actions
            .into_iter()
            .find(|action| action.label == label)
            .expect("action")
            .id
    }

    #[test]
    fn formatting_reads_the_way_the_contract_shows_it() {
        assert_eq!(format_bytes(612_000_000.0), "612 MB");
        assert_eq!(format_bytes(1_160_000_000.0), "1.2 GB");
        assert_eq!(format_bytes(1_500_000_000.0), "1.5 GB");
        assert_eq!(format_bytes(2_000_000_000.0), "2 GB");
        assert_eq!(format_bytes(48_000.0), "48 KB");
        assert_eq!(format_bytes(900.0), "900 B");
        assert_eq!(format_eta(90.0), "1:30");
        assert_eq!(format_eta(5.0), "0:05");
        assert_eq!(format_eta(3723.0), "1:02:03");
        let artifacts: Vec<crate::acquisition::ArtifactStatus> =
            vec![from(rootfs(missing())), from(proton(missing()))];
        assert_eq!(
            confirm_detail(&artifacts.iter().collect::<Vec<_>>()),
            "FEX rootfs and Proton-CachyOS, 1.5 GB"
        );
        assert_eq!(confirm_detail(&[&artifacts[0]]), "FEX rootfs, 1.2 GB");
        let mut unknown = proton(missing());
        unknown.as_object_mut().unwrap().remove("bytesTotal");
        let unknown: crate::acquisition::ArtifactStatus = from(unknown);
        assert_eq!(
            confirm_detail(&[&artifacts[0], &unknown]),
            "FEX rootfs and Proton-CachyOS, size unknown"
        );
    }

    #[tokio::test]
    async fn every_artifact_ready_steps_aside_and_the_launch_proceeds() {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.acquisition.status",
            ok(vec![rootfs(ready()), proton(ready())]),
        );
        let gate = gate(korrid);
        let proceeded = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&proceeded);
        gate.launch(
            game(),
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
            Arc::new(|| {}),
        );
        settle().await;
        assert_eq!(proceeded.load(Ordering::SeqCst), 1);
        assert!(gate.status().is_none());
        assert_eq!(
            script.lock().unwrap().calls[0].1,
            json!({ "gameId": "steam:1" })
        );
    }

    #[tokio::test]
    async fn missing_confirm_download_progress_unpacking_ready_then_the_start() {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.acquisition.status",
            ok(vec![rootfs(missing()), proton(missing())]),
        );
        let gate = gate(korrid);
        let proceeded = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&proceeded);
        gate.launch(
            game(),
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
            Arc::new(|| {}),
        );
        settle().await;
        let confirm = gate.status();
        let Some(SurfaceStatus::Confirm {
            kicker,
            detail,
            game_id,
            ..
        }) = &confirm
        else {
            panic!("{confirm:?}")
        };
        assert_eq!(
            (kicker.as_str(), detail.as_deref(), game_id.as_deref()),
            (
                "Needs a download",
                Some("FEX rootfs and Proton-CachyOS, 1.5 GB"),
                Some("game:steam:1")
            )
        );
        let ten_percent = ok(vec![
            rootfs(
                json!({ "_tag": "Downloading", "file": 1, "fileCount": 11, "bytesDone": 116_000_000u64, "bytesTotal": 1_160_000_000u64, "bytesPerSecond": 6_400_000.0 }),
            ),
            proton(missing()),
        ]);
        push(&script, "app.acquisition.start", ten_percent.clone());
        push(&script, "app.acquisition.status", ten_percent);
        assert!(gate.act(&action_id(confirm, "Download")));
        settle().await;
        assert!(script
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|(tag, payload)| tag == "app.acquisition.start"
                && *payload
                    == json!({ "artifactIds": ["@korri:fex/rootfs", "@korri:proton/cachyos"] })));
        assert_eq!(
            serde_json::to_value(gate.status()).unwrap(),
            json!({ "_tag": "Busy", "kicker": "Downloading FEX rootfs", "detail": "116 MB of 1.2 GB · 6.4 MB/s · 2:43 left",
                "progress": { "done": 116_000_000.0, "total": 1_160_000_000.0 },
                "actions": [{ "id": action_id(gate.status(), "Cancel"), "label": "Cancel", "enabled": true }],
                "gameId": "game:steam:1" })
        );
        push(
            &script,
            "app.acquisition.status",
            ok(vec![
                rootfs(json!({ "_tag": "Unpacking" })),
                proton(missing()),
            ]),
        );
        settle().await;
        assert_eq!(
            serde_json::to_value(gate.status()).unwrap(),
            json!({ "_tag": "Busy", "kicker": "Unpacking FEX rootfs", "progress": { "done": 0.0 }, "gameId": "game:steam:1" })
        );
        push(
            &script,
            "app.acquisition.status",
            ok(vec![rootfs(ready()), proton(ready())]),
        );
        settle().await;
        assert_eq!(proceeded.load(Ordering::SeqCst), 1);
        assert!(gate.status().is_none());
    }

    #[tokio::test]
    async fn cancel_on_confirm_sends_nothing_and_cancel_while_downloading_stops_only_the_live_download(
    ) {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.acquisition.status",
            ok(vec![rootfs(missing()), proton(missing())]),
        );
        let gate = gate(korrid);
        let proceeded = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&proceeded);
        let proceed: crate::surface::acquisition_gate::Step = Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        gate.launch(game(), Arc::clone(&proceed), Arc::new(|| {}));
        settle().await;
        assert!(gate.act(&action_id(gate.status(), "Cancel")));
        assert!(gate.status().is_none());
        assert!(script
            .lock()
            .unwrap()
            .calls
            .iter()
            .all(|(tag, _)| tag == "app.acquisition.status"));
        gate.launch(game(), proceed, Arc::new(|| {}));
        settle().await;
        let begun = ok(vec![
            rootfs(
                json!({ "_tag": "Downloading", "file": 1, "fileCount": 11, "bytesDone": 10, "bytesTotal": 1_160_000_000u64 }),
            ),
            proton(missing()),
        ]);
        push(&script, "app.acquisition.start", begun.clone());
        push(&script, "app.acquisition.status", begun);
        gate.act(&action_id(gate.status(), "Download"));
        settle().await;
        push(
            &script,
            "app.acquisition.cancel",
            ok(vec![rootfs(missing())]),
        );
        push(
            &script,
            "app.acquisition.status",
            ok(vec![rootfs(missing()), proton(missing())]),
        );
        assert!(gate.act(&action_id(gate.status(), "Cancel")));
        settle().await;
        let cancels: Vec<Value> = script
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(tag, _)| tag == "app.acquisition.cancel")
            .map(|(_, payload)| payload.clone())
            .collect();
        assert_eq!(cancels, vec![json!({ "artifactId": "@korri:fex/rootfs" })]);
        assert!(gate.status().is_none());
        assert_eq!(proceeded.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn failed_shows_korrids_words_and_dismiss_closes_without_starting() {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.acquisition.status",
            ok(vec![rootfs(
                json!({ "_tag": "Failed", "reason": { "code": "X", "message": "The connection stalled." } }),
            )]),
        );
        let gate = gate(korrid);
        gate.launch(game(), Arc::new(|| {}), Arc::new(|| {}));
        settle().await;
        // A Failed artifact without anything under way is a question again.
        assert!(matches!(gate.status(), Some(SurfaceStatus::Confirm { .. })));
        push(
            &script,
            "app.acquisition.start",
            json!({ "_tag": "Err", "payload": { "code": "X", "message": "No space left." } }),
        );
        gate.act(&action_id(gate.status(), "Download"));
        settle().await;
        assert_eq!(
            serde_json::to_value(gate.status()).unwrap(),
            json!({ "_tag": "Problem", "kicker": "Couldn't start the download", "reason": "No space left.", "canRetry": true,
                "gameId": "game:steam:1", "gameTitle": "Hollow Knight" })
        );
        assert!(gate.dismiss());
        assert!(gate.status().is_none());
    }
}

/// The native input korrid publishes to the portal socket feeds `controllers`.
#[tokio::test]
async fn native_input_reports_controllers() {
    use crate::native_input::*;
    let link: super::SurfaceSlot = Arc::default();
    let access = crate::PortalAccess::new("c", "http://portal.test", crate::PortalPermission::Full);
    let (_router, source) =
        crate::portal_input::router_with_surfaces(access, Arc::clone(&link), Default::default());
    let device = |id: &str, class| NativeInputDeviceInfo {
        device_id: id.into(),
        class,
        name: format!("{id} name"),
        capabilities: Vec::new(),
        axes: None,
    };
    source
        .publish_device(device("pad", NativeInputDeviceClass::Gamepad), Vec::new())
        .unwrap();
    source
        .publish_device(device("keys", NativeInputDeviceClass::Keyboard), Vec::new())
        .unwrap();
    assert_eq!(
        serde_json::to_value(link.controllers.snapshot()).unwrap(),
        json!([{ "id": "pad", "name": "pad name", "heard": false }])
    );
    source
        .publish_frame(
            "pad",
            vec![NativeInputInput {
                kind: NativeInputInputKind::Input,
                device_id: "pad".into(),
                class: NativeInputDeviceClass::Gamepad,
                input_type: 1.0,
                code: 304.0,
                value: 1.0,
                timestamp: 1.0,
            }],
        )
        .unwrap();
    assert!(link.controllers.snapshot()[0].heard);
    source
        .publish(NativeInputEvent::DeviceRemoved(NativeInputDeviceRemoved {
            kind: NativeInputDeviceRemovedKind::DeviceRemoved,
            device_id: "pad".into(),
        }))
        .unwrap();
    assert!(link.controllers.snapshot().is_empty());
}

/// Port of `SurfaceRoot.displays.test.tsx`: a package that draws on all
/// displays gets korrid's display facts; one on the main display gets none.
#[tokio::test]
async fn displays_follow_the_declared_scope() {
    use korri_surface_treaty::SurfacePackageDisplays;
    let mut outcomes = idle_outcomes(1);
    outcomes.insert(
        "app.surface.displays".into(),
        json!({ "_tag": "Ok", "payload": [
            { "id": "DSI-1", "main": true, "x": 0, "y": 0, "width": 1080, "height": 1240, "touch": true },
            { "id": "DP-1", "main": false, "x": 1080, "y": 0, "width": 1920, "height": 1080, "touch": false }
        ] }),
    );
    let (hub, _, _directory) = hub_with(outcomes);
    settled(&hub, "pico").await;
    hub.declare("pico", HashMap::new(), SurfacePackageDisplays::All)
        .unwrap();
    let mut displays = None;
    for _ in 0..100 {
        displays = hub.model("pico").unwrap().displays;
        if displays.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let displays = displays.expect("pico gets the display facts");
    assert_eq!(displays.len(), 2);
    assert_eq!((displays[1].x, displays[1].width), (1080.0, 1920.0));
    hub.declare("zone", HashMap::new(), SurfacePackageDisplays::Main)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(hub.model("zone").unwrap().displays.is_none());
}

mod runner_chooser {
    //! Port of `runner-chooser.test.ts`, against a scripted korrid.
    use super::acquisition_gate::{push, scripted, Script};
    use super::*;
    use crate::surface::runner::{Intent, RunnerChooser, RunnerLaunchIntegration};
    use korri_surface_treaty::SurfaceRunnerChoice;
    use std::sync::Mutex;

    fn routes(game_runner: Option<&str>, selection: Value) -> Value {
        json!({ "_tag": "Ok", "payload": {
            "gameId": "wl4",
            "routes": [{ "runnerId": "@korri:retroarch/mgba", "familyId": "@korri:retroarch", "systemId": "gba",
                "runnerBuild": "/nix/store/x-mgba", "program": "retroarch", "warnings": [] }],
            "selection": selection,
            "gameRunner": game_runner,
            "systemRunners": {},
            "revisions": { "device": "d1", "games": "g1" }
        } })
    }

    struct Started(Arc<Mutex<Vec<(String, String)>>>, Arc<Mutex<Vec<String>>>);

    fn chooser(korrid: Korrid, start_answer: Option<Value>) -> (Arc<RunnerChooser>, Started) {
        let started = Arc::new(Mutex::new(Vec::new()));
        let returned = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&started);
        let back = Arc::clone(&returned);
        let chooser = RunnerChooser::new(
            korrid,
            Arc::new(|| {}),
            RunnerLaunchIntegration {
                start_launch: Box::new(move |id, runner| {
                    log.lock().unwrap().push((id, runner));
                    let answer = start_answer.clone();
                    Box::pin(async move { answer.map(from) })
                }),
                return_session: Box::new(move |session| {
                    back.lock().unwrap().push(session.launch_id)
                }),
                reload: Box::new(|| {}),
            },
        );
        (chooser, Started(started, returned))
    }

    async fn settle() {
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    fn content(state: &SurfaceRunnerChoice) -> &korri_surface_treaty::SurfaceRunnerChoiceContent {
        match state {
            SurfaceRunnerChoice::Closed => panic!("closed"),
            SurfaceRunnerChoice::Loading(content)
            | SurfaceRunnerChoice::Ready(content)
            | SurfaceRunnerChoice::Stale(content)
            | SurfaceRunnerChoice::Conflict(content)
            | SurfaceRunnerChoice::Error(content)
            | SurfaceRunnerChoice::Busy(content)
            | SurfaceRunnerChoice::Warnings(content) => content,
        }
    }

    /// Press a published action by its command, like a surface button.
    fn press(chooser: &Arc<RunnerChooser>, command: &str) {
        let state = chooser.snapshot();
        let content = content(&state);
        let id = content
            .actions
            .iter()
            .chain(content.routes.iter().flat_map(|route| &route.actions))
            .find(|action| action.id.ends_with(&format!(":{command}")))
            .map(|action| action.id["runner:".len()..].to_owned())
            .unwrap_or_else(|| "unpublished".into());
        chooser.act(id);
    }

    fn sets(script: &Arc<Mutex<Script>>) -> Vec<Value> {
        script
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(tag, _)| tag == "app.launch-settings.set")
            .map(|(_, payload)| payload.clone())
            .collect()
    }

    #[tokio::test]
    async fn a_repairable_stale_read_offers_repair_without_launching() {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.session.status",
            json!({ "_tag": "Ok", "payload": {} }),
        );
        push(
            &script,
            "app.local-games.routes",
            routes(Some("@korri:removed/core"), json!({ "_tag": "Choose" })),
        );
        push(
            &script,
            "app.launch-settings.set",
            json!({ "_tag": "Ok", "payload": { "device": "d1", "games": "g2" } }),
        );
        let (chooser, started) = chooser(korrid, None);
        chooser.open("wl4".into(), "Wario Land 4".into(), Intent::Launch);
        settle().await;
        let state = chooser.snapshot();
        assert!(matches!(state, SurfaceRunnerChoice::Stale(_)));
        assert_eq!(
            serde_json::to_value(&content(&state).saved).unwrap(),
            json!([{ "label": "This game", "runnerId": "@korri:removed/core" }])
        );
        assert!(content(&state)
            .actions
            .iter()
            .any(|action| action.id.ends_with(":clear-game")));
        push(
            &script,
            "app.local-games.routes",
            routes(
                Some("@korri:retroarch/mgba"),
                json!({ "_tag": "Selected", "runnerId": "@korri:retroarch/mgba" }),
            ),
        );
        press(&chooser, "game:0");
        settle().await;
        assert_eq!(
            sets(&script),
            vec![
                json!({ "place": { "layer": { "_tag": "Game", "id": "wl4" }, "scope": { "_tag": "Any" } },
                "path": { "_tag": "Runner" }, "value": "@korri:retroarch/mgba", "clear": false, "expectedRevision": "g1" })
            ]
        );
        assert!(matches!(chooser.snapshot(), SurfaceRunnerChoice::Ready(_)));
        assert!(started.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_old_button_cannot_act_on_a_newer_read() {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.local-games.routes",
            routes(None, json!({ "_tag": "Choose" })),
        );
        let (chooser, started) = chooser(korrid, None);
        chooser.open("wl4".into(), "Wario Land 4".into(), Intent::Inspect);
        settle().await;
        let old =
            content(&chooser.snapshot()).routes[0].actions[0].id["runner:".len()..].to_owned();
        chooser.open("wl4".into(), "Wario Land 4".into(), Intent::Inspect);
        settle().await;
        chooser.act(old);
        settle().await;
        assert!(started.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn launch_with_a_saved_runner_starts_it_and_shows_its_warnings() {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.session.status",
            json!({ "_tag": "Ok", "payload": {} }),
        );
        push(
            &script,
            "app.local-games.routes",
            routes(
                Some("@korri:retroarch/mgba"),
                json!({ "_tag": "Selected", "runnerId": "@korri:retroarch/mgba" }),
            ),
        );
        let warning = json!({ "setting": "video_filter", "runnerId": "@korri:retroarch/mgba", "build": "1.20", "message": "ignored" });
        let (chooser, started) = chooser(
            korrid,
            Some(
                json!({ "_tag": "Ok", "payload": { "session": { "gameId": "wl4", "launchId": "L" }, "warnings": [warning] } }),
            ),
        );
        chooser.open("wl4".into(), "Wario Land 4".into(), Intent::Launch);
        settle().await;
        assert_eq!(
            *started.0.lock().unwrap(),
            vec![("wl4".to_owned(), "@korri:retroarch/mgba".to_owned())]
        );
        let state = chooser.snapshot();
        assert!(matches!(state, SurfaceRunnerChoice::Warnings(_)));
        assert_eq!(
            content(&state).warnings,
            vec!["video_filter · @korri:retroarch/mgba · 1.20: ignored"]
        );
        press(&chooser, "dismiss");
        settle().await;
        assert!(chooser.is_closed());
    }

    #[tokio::test]
    async fn launching_a_game_already_live_returns_to_that_exact_session() {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.session.status",
            json!({ "_tag": "Ok", "payload": { "active": { "launchId": "L1", "gameId": "wl4" } } }),
        );
        let (chooser, started) = chooser(korrid, None);
        chooser.open("wl4".into(), "Wario Land 4".into(), Intent::Launch);
        settle().await;
        assert!(chooser.is_closed());
        assert_eq!(*started.1.lock().unwrap(), vec!["L1".to_owned()]);
    }

    #[tokio::test]
    async fn an_active_session_conflict_offers_to_end_it() {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.local-games.routes",
            routes(None, json!({ "_tag": "Choose" })),
        );
        let (chooser, _) = chooser(
            korrid,
            Some(
                json!({ "_tag": "Err", "payload": { "code": "ActiveSessionConflict", "message": "busy" } }),
            ),
        );
        chooser.open("wl4".into(), "Wario Land 4".into(), Intent::Inspect);
        settle().await;
        push(
            &script,
            "app.session.status",
            json!({ "_tag": "Ok", "payload": { "active": { "launchId": "other", "title": "Turtles" } } }),
        );
        press(&chooser, "launch:0");
        settle().await;
        let state = chooser.snapshot();
        assert!(matches!(state, SurfaceRunnerChoice::Conflict(_)));
        assert_eq!(content(&state).message, "End Turtles before switching runners. Nothing will launch automatically after ending it.");
        assert!(content(&state)
            .actions
            .iter()
            .any(|action| action.id.ends_with(":end")));
    }
}
