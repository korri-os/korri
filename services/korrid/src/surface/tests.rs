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

/// The shell declares twice per switch: before init, and again once its old
/// window retired. Each declare places anew, even with the same surface and
/// scope; the second leaves preferences as they were.
#[tokio::test]
async fn a_second_declare_places_the_new_window_and_keeps_preferences() {
    let declarations = || -> korri_surface_treaty::SurfacePreferenceDeclarations {
        [(
            "zone.look".to_owned(),
            SurfacePreferenceDeclaration {
                mark: SurfacePreferenceMark::Shared,
                initial: "modern".into(),
            },
        )]
        .into()
    };
    let window = |id: &str| json!({ "_tag": "Ok", "payload": [{ "id": id, "main": true, "x": 0, "y": 0, "width": 640, "height": 480, "touch": false }] });
    let shown = |hub: &Arc<SurfaceHub>| {
        hub.model("zone").unwrap().displays.map(|displays| {
            displays
                .into_iter()
                .map(|display| display.id)
                .collect::<Vec<_>>()
        })
    };
    let no_window = json!({ "_tag": "Err", "payload": { "code": "SurfacePlacementFailed", "message": "no unique portal window" } });
    // The first placement finds no window, or the old one.
    for first in [no_window, window("old-window")] {
        let mut outcomes = idle_outcomes(1);
        outcomes.insert("app.surface.displays".into(), first.clone());
        let (hub, outcomes, _directory) = hub_with(outcomes);
        hub.declare(
            "zone",
            declarations(),
            korri_surface_treaty::SurfacePackageDisplays::All,
        )
        .unwrap();
        hub.request(
            "zone",
            SurfaceRequest::RememberPreference {
                key: "zone.look".into(),
                value: "retro".into(),
            },
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        if first["_tag"] == "Ok" {
            assert_eq!(shown(&hub), Some(vec!["old-window".to_owned()]));
        } else {
            assert_eq!(shown(&hub), None);
        }
        // The old window retired; the new one is the only one. The same
        // declaration again must place it, whatever the first attempt did.
        outcomes
            .lock()
            .unwrap()
            .insert("app.surface.displays".into(), window("new-window"));
        hub.declare(
            "zone",
            declarations(),
            korri_surface_treaty::SurfacePackageDisplays::All,
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            shown(&hub),
            Some(vec!["new-window".to_owned()]),
            "after {first}"
        );
        assert_eq!(hub.preferences("zone").unwrap()["zone.look"], "retro");
    }
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
        pub(super) answers: HashMap<String, Vec<Value>>,
        last: HashMap<String, Value>,
        pub(super) calls: Vec<(String, Value)>,
        pub(super) delays: HashMap<String, Duration>,
    }

    pub(super) fn scripted() -> (Korrid, Arc<std::sync::Mutex<Script>>) {
        let script = Arc::new(std::sync::Mutex::new(Script::default()));
        let shared = Arc::clone(&script);
        let korrid = Korrid::new(Arc::new(move |request: RpcRequest| {
            let script = Arc::clone(&shared);
            Box::pin(async move {
                let value = serde_json::to_value(&request).unwrap();
                let tag = value["_tag"].as_str().unwrap().to_owned();
                let delay = script.lock().unwrap().delays.get(&tag).copied();
                if let Some(delay) = delay {
                    tokio::time::sleep(delay).await;
                }
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

mod save_sync {
    //! Port of `SurfaceRoot.saves.test.tsx` at the save-sync seam.
    use super::acquisition_gate::{push, scripted};
    use super::*;
    use crate::surface::saves::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const HERE: &str = "here-key";
    const ZAO: &str = "zao-key";

    fn entry(id: &str, device: &str, ended: &str, parents: &[&str]) -> Value {
        json!({ "snapshotId": id, "record": { "parents": parents, "gameId": "wl4", "runnerId": "r", "device": device,
            "endedAt": ended, "durationSeconds": 60, "interrupted": false, "locations": [] } })
    }

    fn sync() -> (
        Arc<SaveSync>,
        Arc<std::sync::Mutex<super::acquisition_gate::Script>>,
    ) {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.peer.list",
            json!({ "_tag": "Ok", "payload": { "peers": [
            { "devicePublicKey": ZAO, "label": "zao", "state": "ready", "updatedAt": 1 }
        ] } }),
        );
        let sync = SaveSync::new(
            korrid,
            Arc::new(|| {}),
            Arc::new(|| [HERE.to_owned()].into()),
            Duration::from_millis(200),
        );
        (sync, script)
    }

    fn game() -> SaveGameRef {
        SaveGameRef {
            id: "wl4".into(),
            surface_id: "game:wl4".into(),
            title: "Wario Land 4".into(),
            runner_id: None,
        }
    }

    fn counter() -> (Arc<AtomicUsize>, crate::surface::acquisition_gate::Step) {
        let count = Arc::new(AtomicUsize::new(0));
        let step = Arc::clone(&count);
        (
            count,
            Arc::new(move || {
                step.fetch_add(1, Ordering::SeqCst);
            }),
        )
    }

    async fn settle() {
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    fn action(status: &Option<SurfaceStatus>, label: &str) -> String {
        match status {
            Some(SurfaceStatus::Busy {
                actions: Some(actions),
                ..
            })
            | Some(SurfaceStatus::Confirm { actions, .. })
            | Some(SurfaceStatus::ChooseSave { actions, .. }) => actions
                .iter()
                .find(|action| action.label == label)
                .expect("action")
                .id
                .clone(),
            other => panic!("no actions on {other:?}"),
        }
    }

    #[test]
    fn marks_and_history_follow_korrid() {
        let split = SaveMarkFact {
            mark: crate::saves::types::SaveMark::Split,
            heads: vec![],
        };
        assert_eq!(
            serde_json::to_value(surface_saves_from(Some(&split))).unwrap(),
            json!({ "_tag": "Split", "choose": { "id": "save-choose", "label": "Choose a save", "enabled": true } })
        );
        let synced = SaveMarkFact {
            mark: crate::saves::types::SaveMark::Synced,
            heads: vec![],
        };
        assert_eq!(
            serde_json::to_value(surface_saves_from(Some(&synced))).unwrap(),
            json!({ "_tag": "Synced" })
        );
        assert!(offers_save_history(Some(&synced)));
        assert!(!offers_save_history(Some(&SaveMarkFact {
            mark: crate::saves::types::SaveMark::NotSynced,
            heads: vec![]
        })));
        assert!(!offers_save_history(Some(&SaveMarkFact {
            mark: crate::saves::types::SaveMark::SyncedBySteam,
            heads: vec![]
        })));
        assert!(surface_saves_from(None).is_none());
    }

    #[test]
    fn no_failure_code_reaches_the_person() {
        for code in [
            "SavesSplit",
            "SaveContentUnavailable",
            "ActiveSessionConflict",
            "SavePayloadUnavailable",
            "DeviceNeedsOwner",
            "SavesUnavailable",
            "BrainUnreachable",
            "PermissionDenied",
            "GameNotFound",
            "SomethingNew",
        ] {
            let reason = save_failure_reason(&crate::RpcFailure {
                code: code.into(),
                message: format!("{code}: raw"),
            });
            assert!(!reason.contains(code) && !reason.contains("raw"), "{code}");
        }
        assert_eq!(
            save_failure_reason(&crate::RpcFailure {
                code: "ActiveSessionConflict".into(),
                message: String::new()
            }),
            "A game is running. Stop it, then try again."
        );
    }

    #[test]
    fn newest_first_by_ancestry() {
        // A child whose clock is behind its parent's still comes first.
        let late: crate::saves::types::SaveHistoryEntry =
            from(entry("p", HERE, "2026-10-21T12:00:00.000Z", &[]));
        let early: crate::saves::types::SaveHistoryEntry =
            from(entry("q", ZAO, "2026-10-01T00:00:00.000Z", &["p"]));
        assert_eq!(
            newest_first(&[late, early])
                .iter()
                .map(|e| e.snapshot_id.as_str())
                .collect::<Vec<_>>(),
            vec!["q", "p"]
        );
        let base = from(entry("a", HERE, "2026-10-01T00:00:00.000Z", &[]));
        let zao = from(entry("b", ZAO, "2026-10-03T00:00:00.000Z", &["a"]));
        let here = from(entry("c", HERE, "2026-10-02T00:00:00.000Z", &["a"]));
        assert_eq!(
            newest_first(&[base, zao, here])
                .iter()
                .map(|e| e.snapshot_id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c", "a"]
        );
    }

    #[tokio::test]
    async fn checking_offers_skip_and_skip_launches_not_checked() {
        let (sync, script) = sync();
        push(
            &script,
            "app.saves.status",
            json!({ "_tag": "Ok", "payload": { "games": [{ "gameId": "wl4", "status": "Synced", "heads": ["h"] }] } }),
        );
        sync.refresh().await;
        let (proceeded, proceed) = counter();
        // The check answers late: it waits until Skip or the limit.
        push(
            &script,
            "app.saves.check",
            json!({ "_tag": "Ok", "payload": { "warnings": [], "status": "Synced", "inUse": [] } }),
        );
        script
            .lock()
            .unwrap()
            .delays
            .insert("app.saves.check".into(), Duration::from_millis(150));
        sync.launch(game(), proceed, Arc::new(|| {}));
        settle().await;
        let status = sync.status();
        assert!(
            matches!(&status, Some(SurfaceStatus::Busy { kicker, game_id: Some(id), .. }) if kicker == "Checking other devices…" && id == "game:wl4")
        );
        sync.act(&action(&status, "Skip"));
        settle().await;
        assert_eq!(proceeded.load(Ordering::SeqCst), 1);
        assert_eq!(
            sync.marks()["wl4"].mark,
            crate::saves::types::SaveMark::NotChecked
        );
    }

    #[tokio::test]
    async fn a_split_asks_with_two_sides_by_name_and_a_pick_launches() {
        let (sync, script) = sync();
        push(
            &script,
            "app.saves.status",
            json!({ "_tag": "Ok", "payload": { "games": [{ "gameId": "wl4", "status": "Split", "heads": ["b", "c"] }] } }),
        );
        push(
            &script,
            "app.saves.check",
            json!({ "_tag": "Ok", "payload": { "warnings": [], "status": "Split", "inUse": [] } }),
        );
        let history = json!({ "_tag": "Ok", "payload": { "records": [
            entry("a", HERE, "2026-10-01T00:00:00.000Z", &[]),
            entry("b", ZAO, "2026-10-03T00:00:00.000Z", &["a"]),
            entry("c", HERE, "2026-10-02T00:00:00.000Z", &["a"])
        ] } });
        push(&script, "app.saves.history", history.clone());
        push(&script, "app.saves.choose", history);
        let (proceeded, proceed) = counter();
        sync.launch(game(), proceed, Arc::new(|| {}));
        settle().await;
        let status = sync.status();
        let Some(SurfaceStatus::ChooseSave {
            sides,
            actions,
            kicker,
            ..
        }) = &status
        else {
            panic!("{status:?}")
        };
        assert_eq!(kicker, "Saves split between two devices");
        assert_eq!(
            sides
                .iter()
                .map(|side| side.device.as_str())
                .collect::<Vec<_>>(),
            vec!["zao", "This device"]
        );
        assert_eq!(
            actions
                .iter()
                .map(|action| action.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Not now"]
        );
        sync.act(&sides[0].action_id.clone());
        settle().await;
        assert_eq!(proceeded.load(Ordering::SeqCst), 1);
        assert!(script
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|(tag, payload)| tag == "app.saves.choose" && payload["snapshotId"] == "b"));
    }

    #[tokio::test]
    async fn in_use_elsewhere_confirms_and_launch_anyway_goes_on() {
        let (sync, script) = sync();
        push(
            &script,
            "app.saves.status",
            json!({ "_tag": "Ok", "payload": { "games": [] } }),
        );
        push(
            &script,
            "app.saves.check",
            json!({ "_tag": "Ok", "payload": { "warnings": ["The release changed."], "status": "Synced",
            "inUse": [{ "device": ZAO, "since": 1_000_000u64 }] } }),
        );
        sync.refresh().await;
        let (proceeded, proceed) = counter();
        let (abandoned, abandon) = counter();
        sync.launch(game(), proceed, abandon);
        settle().await;
        let status = sync.status();
        let Some(SurfaceStatus::Confirm { kicker, detail, .. }) = &status else {
            panic!("{status:?}")
        };
        assert!(kicker.starts_with("In use on zao since "));
        assert!(detail
            .as_deref()
            .unwrap()
            .starts_with("If you play here too, Korri asks which save to keep afterwards."));
        assert!(detail.as_deref().unwrap().ends_with("The release changed."));
        sync.act(&action(&status, "Launch anyway"));
        settle().await;
        assert_eq!(
            (
                proceeded.load(Ordering::SeqCst),
                abandoned.load(Ordering::SeqCst)
            ),
            (1, 0)
        );
    }

    #[tokio::test]
    async fn device_needs_owner_is_plain_words_with_nothing_to_retry() {
        let (sync, _) = sync();
        let (abandoned, abandon) = counter();
        sync.refuse_start(
            game(),
            &crate::RpcFailure {
                code: "DeviceNeedsOwner".into(),
                message: "raw".into(),
            },
            abandon,
        );
        assert_eq!(
            serde_json::to_value(sync.status()).unwrap(),
            json!({ "_tag": "Problem", "kicker": "Couldn't start Wario Land 4", "reason": "This device needs an owner before it can start a game.",
                "canRetry": false, "gameId": "game:wl4", "gameTitle": "Wario Land 4" })
        );
        assert!(sync.dismiss());
        assert_eq!(abandoned.load(Ordering::SeqCst), 1);
        assert!(sync.status().is_none());
    }

    #[tokio::test]
    async fn history_is_newest_first_by_name_and_restore_asks_first() {
        let (sync, script) = sync();
        push(
            &script,
            "app.saves.status",
            json!({ "_tag": "Ok", "payload": { "games": [] } }),
        );
        sync.refresh().await;
        push(
            &script,
            "app.saves.history",
            json!({ "_tag": "Ok", "payload": { "current": "b", "records": [
            entry("a", HERE, "2026-10-01T00:00:00.000Z", &[]),
            entry("b", ZAO, "2026-10-03T00:00:00.000Z", &["a"])
        ] } }),
        );
        sync.open_history(game());
        settle().await;
        let Some(SurfaceSaveHistory::Ready {
            entries,
            actions,
            game_id,
        }) = sync.history()
        else {
            panic!("not ready")
        };
        assert_eq!(game_id, "game:wl4");
        assert_eq!(actions.len(), 1);
        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.device.as_str(), entry.current))
                .collect::<Vec<_>>(),
            vec![("zao", true), ("This device", false)]
        );
        assert!(entries[0].restore.is_none());
        let restore = entries[1].restore.clone().unwrap();
        assert_eq!(restore.destructive, Some(true));
        push(
            &script,
            "app.saves.restore",
            json!({ "_tag": "Err", "payload": { "code": "SavePayloadUnavailable", "message": "raw" } }),
        );
        sync.act(&restore.id);
        settle().await;
        assert!(
            matches!(sync.history(), Some(SurfaceSaveHistory::Error { message, .. }) if message == "Couldn't restore that save. That older save's files are not on any device in reach. Try again when the device that made it is on.")
        );
    }

    use korri_surface_treaty::SurfaceSaveHistory;
}

mod store_acquisition {
    //! Port of `store-acquisition.test.ts`, against a scripted korrid.
    use super::acquisition_gate::{push, scripted, Script};
    use super::*;
    use crate::surface::store::*;
    use korri_surface_treaty::{
        SurfaceStoreDetails, SurfaceStoreGet, SurfaceStoreProviders, SurfaceStoreSearch,
    };
    use std::sync::Mutex;

    const BBS: &str = "@korri:pico8-bbs";

    fn claim(id: &str, system: Option<&str>) -> Value {
        let mut claim = json!({ "_tag": "ProviderClaim", "providerId": BBS, "id": id, "title": id.to_uppercase(),
            "url": format!("https://www.lexaloffle.com/bbs/?tid={id}"), "platform": "PICO-8" });
        if let Some(system) = system {
            claim["artifact"] = json!({ "kind": "content", "system": system, "format": { "id": "p8.png" },
                "file": { "name": format!("{id}.p8.png"), "sizeBytes": 48_000 } });
        }
        claim
    }

    fn details(id: &str, system: Option<&str>) -> Value {
        let mut details = claim(id, system);
        details["_tag"] = json!("ProviderClaimDetails");
        details.as_object_mut().unwrap().remove("platform");
        details["description"] = json!("A cart.");
        json!({ "_tag": "Ok", "payload": details })
    }

    type Store = (
        Arc<StoreAcquisition>,
        Arc<Mutex<Script>>,
        Arc<Mutex<Vec<String>>>,
    );

    fn store(owned: Arc<Mutex<Option<String>>>) -> Store {
        let (korrid, script) = scripted();
        let imported = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&imported);
        let store = StoreAcquisition::new(
            korrid,
            Arc::new(|| {}),
            Arc::new(move |_, _| owned.lock().unwrap().clone()),
            Arc::new(move |game_id| log.lock().unwrap().push(game_id)),
            Duration::from_millis(1),
        );
        (store, script, imported)
    }

    async fn settle() {
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    fn get(store: &Arc<StoreAcquisition>) -> SurfaceStoreGet {
        match store.snapshot().details {
            SurfaceStoreDetails::Ready { get, .. } => get,
            other => panic!("details not ready: {other:?}"),
        }
    }

    fn press(store: &Arc<StoreAcquisition>, label: &str) {
        let actions = match get(store) {
            SurfaceStoreGet::Offer { actions, .. }
            | SurfaceStoreGet::Confirm { actions, .. }
            | SurfaceStoreGet::Working { actions, .. }
            | SurfaceStoreGet::Problem { actions, .. } => actions,
            other => panic!("no actions on {other:?}"),
        };
        let id = actions
            .into_iter()
            .find(|action| action.label == label)
            .expect("action")
            .id;
        assert!(store.act(&id));
    }

    async fn open(
        store: &Arc<StoreAcquisition>,
        script: &Arc<Mutex<Script>>,
        id: &str,
        system: Option<&str>,
    ) {
        push(
            script,
            "app.acquisition.providers",
            json!({ "_tag": "Ok", "payload": { "providers": [{ "id": BBS, "title": "PICO-8 BBS", "pluginId": BBS }] } }),
        );
        push(
            script,
            "app.acquisition.search",
            json!({ "_tag": "Ok", "payload": { "claims": [claim(id, system)] } }),
        );
        push(script, "app.acquisition.details", details(id, system));
        store.open_store();
        settle().await;
        store.search(BBS.into(), "celeste".into());
        settle().await;
        store.open_entry(store_entry_id(BBS, id));
        settle().await;
    }

    #[tokio::test]
    async fn providers_load_and_a_refused_list_offers_retry() {
        let (store, script, _) = store(Arc::default());
        push(
            &script,
            "app.acquisition.providers",
            json!({ "_tag": "Err", "payload": { "code": "X", "message": "No providers." } }),
        );
        store.open_store();
        settle().await;
        let SurfaceStoreProviders::Problem { message, actions } = store.snapshot().providers else {
            panic!()
        };
        assert_eq!(message, "No providers.");
        push(
            &script,
            "app.acquisition.providers",
            json!({ "_tag": "Ok", "payload": { "providers": [{ "id": BBS, "title": "PICO-8 BBS", "pluginId": BBS }] } }),
        );
        assert!(store.act(&actions[0].id));
        settle().await;
        assert_eq!(
            serde_json::to_value(store.snapshot().providers).unwrap(),
            json!({ "_tag": "Ready", "providers": [{ "id": BBS, "title": "PICO-8 BBS" }] })
        );
        // A search only for a listed provider.
        store.search("elsewhere".into(), "x".into());
        settle().await;
        assert_eq!(store.snapshot().search, SurfaceStoreSearch::Idle);
    }

    #[tokio::test]
    async fn a_claim_with_no_system_cannot_be_got() {
        let (store, script, _) = store(Arc::default());
        open(&store, &script, "nosystem", None).await;
        assert_eq!(
            get(&store),
            SurfaceStoreGet::Unavailable {
                reason: NO_SYSTEM_REASON.into()
            }
        );
    }

    #[tokio::test]
    async fn get_asks_with_the_size_then_downloads_with_progress_and_lands_in_the_library() {
        let (store, script, imported) = store(Arc::default());
        open(&store, &script, "celeste", Some("pico8")).await;
        assert!(matches!(
            get(&store),
            SurfaceStoreGet::Offer { note: None, .. }
        ));
        press(&store, "Get");
        let SurfaceStoreGet::Confirm { kicker, detail, .. } = get(&store) else {
            panic!()
        };
        assert_eq!(
            (kicker.as_str(), detail.as_deref()),
            ("Download to this device", Some("CELESTE, 48 KB"))
        );
        let job = |state: Value| json!({ "_tag": "Ok", "payload": { "jobId": "j1", "providerId": BBS, "id": "celeste", "state": state } });
        push(
            &script,
            "app.acquisition.acquire",
            job(json!({ "_tag": "Downloading", "bytesDone": 24_000, "bytesTotal": 48_000 })),
        );
        push(
            &script,
            "app.acquisition.acquire-status",
            job(json!({ "_tag": "Placing" })),
        );
        press(&store, "Download");
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
        settle().await;
        push(
            &script,
            "app.acquisition.acquire-status",
            job(json!({ "_tag": "Imported", "gameId": "game-celeste" })),
        );
        settle().await;
        assert_eq!(
            get(&store),
            SurfaceStoreGet::InLibrary {
                detail: Some("In your library".into()),
                game_id: Some("game-celeste".into())
            }
        );
        assert_eq!(*imported.lock().unwrap(), vec!["game-celeste".to_owned()]);
    }

    #[tokio::test]
    async fn an_owned_cart_is_play_at_once_and_a_removed_one_offers_get_again() {
        let owned = Arc::new(Mutex::new(Some("game-celeste".to_owned())));
        let (store, script, _) = store(Arc::clone(&owned));
        open(&store, &script, "celeste", Some("pico8")).await;
        assert!(
            matches!(get(&store), SurfaceStoreGet::InLibrary { game_id: Some(id), .. } if id == "game-celeste")
        );
        *owned.lock().unwrap() = None;
        store.library_changed();
        assert!(matches!(get(&store), SurfaceStoreGet::Offer { .. }));
    }

    #[tokio::test]
    async fn cancel_during_the_download_stops_it_and_get_is_offered_again() {
        let (store, script, _) = store(Arc::default());
        open(&store, &script, "celeste", Some("pico8")).await;
        press(&store, "Get");
        let job = |state: Value| json!({ "_tag": "Ok", "payload": { "jobId": "j1", "providerId": BBS, "id": "celeste", "state": state } });
        let downloading =
            job(json!({ "_tag": "Downloading", "bytesDone": 1, "bytesTotal": 48_000 }));
        push(&script, "app.acquisition.acquire", downloading.clone());
        push(&script, "app.acquisition.acquire-status", downloading);
        press(&store, "Download");
        settle().await;
        let SurfaceStoreGet::Working {
            kicker, progress, ..
        } = get(&store)
        else {
            panic!()
        };
        assert_eq!(kicker, "Downloading CELESTE");
        assert_eq!(progress.unwrap().total, Some(48_000.0));
        push(
            &script,
            "app.acquisition.acquire-cancel",
            job(json!({ "_tag": "Cancelled" })),
        );
        press(&store, "Cancel");
        settle().await;
        assert_eq!(
            get(&store),
            SurfaceStoreGet::Offer {
                note: Some(CANCELLED_NOTE.into()),
                actions: match get(&store) {
                    SurfaceStoreGet::Offer { actions, .. } => actions,
                    _ => unreachable!(),
                }
            }
        );
    }

    #[tokio::test]
    async fn korrid_restarting_under_a_download_says_download_stopped() {
        let (store, script, _) = store(Arc::default());
        open(&store, &script, "celeste", Some("pico8")).await;
        press(&store, "Get");
        push(
            &script,
            "app.acquisition.acquire",
            json!({ "_tag": "Ok", "payload": { "jobId": "j1", "providerId": BBS, "id": "celeste", "state": { "_tag": "Resolving" } } }),
        );
        push(
            &script,
            "app.acquisition.acquire-status",
            json!({ "_tag": "Err", "payload": { "code": "AcquisitionUnknownJob", "message": "gone" } }),
        );
        press(&store, "Download");
        settle().await;
        let SurfaceStoreGet::Problem {
            kicker,
            reason,
            actions,
        } = get(&store)
        else {
            panic!("{:?}", get(&store))
        };
        assert_eq!(
            (kicker.as_str(), reason.as_str()),
            (STOPPED_KICKER, STOPPED_REASON)
        );
        assert_eq!(actions[0].label, "Retry");
    }

    #[tokio::test]
    async fn a_stale_press_sends_nothing() {
        let (store, script, _) = store(Arc::default());
        open(&store, &script, "celeste", Some("pico8")).await;
        press(&store, "Get");
        let SurfaceStoreGet::Confirm { actions, .. } = get(&store) else {
            panic!()
        };
        let stale_download = actions[0].id.clone();
        press(&store, "Cancel");
        // The Confirm is gone: its Download sends nothing.
        assert!(store.act(&stale_download));
        settle().await;
        assert!(matches!(get(&store), SurfaceStoreGet::Offer { .. }));
        assert!(!script
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|(tag, _)| tag == "app.acquisition.acquire"));
    }

    #[test]
    fn download_detail_reads_like_the_portal() {
        assert_eq!(
            store_download_detail(612_000, Some(1_700_000), Some(48_000.0)),
            "612 KB of 1.7 MB · 48 KB/s · 0:23 left"
        );
        assert_eq!(store_download_detail(612_000, None, None), "612 KB so far");
    }
}

mod plugin_repositories {
    //! Ports of `plugin-repositories.test.ts` and
    //! `plugin-repositories-controller.test.ts`.
    use super::acquisition_gate::{push, scripted};
    use super::*;
    use crate::surface::plugins::*;
    use korri_surface_treaty::{
        SurfaceSettingGroup, SurfaceSettingInteraction, SurfaceSettingsStatus,
    };
    use std::sync::Mutex;

    const OFFICIAL: &str = "https://plugins.korri.dev/";
    const RETRO: &str = "https://retro.example/plugins";

    fn publisher(namespace: &str, state: &str) -> Value {
        json!({ "namespace": namespace, "state": state, "fingerprint": format!("{namespace}-fp"), "publicKey": format!("{namespace}-key") })
    }

    fn repositories(user_added: Vec<&str>, installs: Value) -> PluginFacts {
        PluginFacts {
            repositories: Read::Ready(from(
                json!({ "official": null, "userAdded": user_added, "repositoryInstalls": installs, "otherInstalls": [] }),
            )),
            publishers: Read::Ready(from(json!({ "entries": [] }))),
            ..PluginFacts::default()
        }
    }

    fn retro_facts() -> PluginFacts {
        let mut facts = repositories(
            vec![RETRO],
            json!([{ "id": "@ada:netplay", "sourceUrl": RETRO, "releaseVersion": "0.4.0", "enabled": true }]),
        );
        facts.catalogs.insert(
            RETRO.into(),
            CatalogView::Ready(from(json!([
                { "pluginId": "@ada:netplay", "title": "Netplay", "releaseVersion": "0.4.0", "installed": "Enabled", "publisher": publisher("@ada", "Trusted") },
                { "pluginId": "@ada:chiptune", "title": "Chiptune", "description": "Bleeps.", "releaseVersion": "1.0.0", "publisher": publisher("@ada", "Trusted") }
            ]))),
        );
        facts
    }

    fn chiptune() -> ReleaseTarget {
        ReleaseTarget {
            source_url: RETRO.into(),
            plugin_id: "@ada:chiptune".into(),
            release_version: "1.0.0".into(),
        }
    }

    fn inspection(publisher_state: &str, brings: Value) -> crate::plugin_admin::PluginInspection {
        from(
            json!({ "publisher": publisher("@ada", publisher_state), "id": "@ada:chiptune", "title": "Chiptune",
            "approval": "sha256:digest", "warning": "Plays loud.", "ports": { "allowedTCPPorts": [55435], "allowedUDPPorts": [] },
            "nativeUnits": ["chiptune.service"], "brings": brings }),
        )
    }

    fn group<'a>(groups: &'a [SurfaceSettingGroup], title: &str) -> &'a SurfaceSettingGroup {
        groups
            .iter()
            .find(|group| group.title == title)
            .expect("group")
    }

    #[test]
    fn official_only_shows_not_set_up_and_the_add_row() {
        let groups = plugin_setting_groups(&repositories(vec![], json!([])), Some(&[]));
        assert_eq!(
            groups
                .iter()
                .map(|group| group.title.as_str())
                .collect::<Vec<_>>(),
            vec![PLUGINS_GROUP, PUBLISHERS_GROUP]
        );
        assert_eq!(groups[0].items[0].id, OFFICIAL_REPOSITORY_ROW);
        assert_eq!(groups[0].items[0].value.as_deref(), Some("Not set up yet"));
        assert!(groups[0].items[0].interaction.is_none());
        assert_eq!(groups[0].items[1].id, ADD_REPOSITORY_SETTING);
        let loading = plugin_setting_groups(&PluginFacts::default(), None);
        assert_eq!(loading.len(), 1);
        assert_eq!(loading[0].items[0].value.as_deref(), Some("Checking…"));
    }

    #[test]
    fn an_added_repository_lists_its_plugins_and_ends_with_remove() {
        let groups = plugin_setting_groups(&retro_facts(), Some(&[]));
        let retro = group(&groups, "retro.example/plugins");
        assert_eq!(
            retro
                .items
                .iter()
                .map(|item| item.id.clone())
                .collect::<Vec<_>>(),
            vec![
                toggle_setting_id("@ada:netplay"),
                release_row_id(&chiptune()),
                remove_repository_action_id(RETRO)
            ]
        );
        assert_eq!(retro.items[0].value.as_deref(), Some("On"));
        assert_eq!(retro.items[1].value.as_deref(), Some("Install"));
        assert_eq!(retro.items[1].description.as_deref(), Some("Bleeps."));
        let Some(SurfaceSettingInteraction::Action {
            confirmation: Some(confirmation),
            destructive: Some(true),
            ..
        }) = &retro.items[2].interaction
        else {
            panic!()
        };
        assert_eq!(confirmation.message, "Korri stops showing plugins from retro.example/plugins. The plugin you installed from it stays, and still turns on and off under \"Installed from removed repositories\".");
        assert_eq!(
            group(&groups, PLUGINS_GROUP).items[1].value.as_deref(),
            Some("2 plugins")
        );
    }

    #[test]
    fn an_inspected_release_becomes_review_and_turn_on_whose_question_is_the_report() {
        let report = inspection("Trusted", json!([]));
        let facts = with_review(retro_facts(), &chiptune(), report.clone());
        let groups = plugin_setting_groups(&facts, Some(&[]));
        let row = group(&groups, "retro.example/plugins")
            .items
            .iter()
            .find(|item| item.id == release_row_id(&chiptune()))
            .unwrap();
        assert_eq!(row.value.as_deref(), Some("Review and turn on"));
        let Some(SurfaceSettingInteraction::Action {
            action_id,
            confirmation: Some(confirmation),
            destructive,
            ..
        }) = &row.interaction
        else {
            panic!()
        };
        assert_eq!(*action_id, review_action_id(&chiptune()));
        assert_eq!(*destructive, None);
        assert_eq!(confirmation.title, "Turn on Chiptune?");
        assert_eq!(confirmation.message, review_message(&report));
        assert_eq!(
            review_message(&report),
            "Publisher @ada, trusted on this device. Key @ada-fp.\nWarning: Plays loud.\nOpens network ports: TCP 55435.\nRuns in the background: chiptune.service."
        );
    }

    #[test]
    fn a_new_publisher_is_trusted_first_with_its_key_in_the_question() {
        let report = inspection("New", json!([]));
        let facts = with_review(retro_facts(), &chiptune(), report);
        let groups = plugin_setting_groups(&facts, Some(&[]));
        let row = group(&groups, "retro.example/plugins")
            .items
            .iter()
            .find(|item| item.id == release_row_id(&chiptune()))
            .unwrap();
        assert_eq!(row.value.as_deref(), Some("Trust @ada"));
        let Some(SurfaceSettingInteraction::Action {
            action_id,
            confirmation: Some(confirmation),
            ..
        }) = &row.interaction
        else {
            panic!()
        };
        assert_eq!(*action_id, trust_action_id(&chiptune(), "@ada"));
        assert_eq!(
            confirmation.message,
            "Chiptune comes from @ada. This device has not trusted it before.\nKey @ada-fp.\nTrust it only if the plugin maker gave you this key.\nLater plugins from @ada must have the same key."
        );
    }

    #[test]
    fn a_stale_approval_shows_needs_review() {
        let facts = with_needs_review(retro_facts(), "@ada:netplay", false);
        let groups = plugin_setting_groups(&facts, Some(&[]));
        let row = &group(&groups, "retro.example/plugins").items[0];
        assert_eq!(row.value.as_deref(), Some("Needs review"));
        assert_eq!(
            row.description.as_deref(),
            Some("It changed since you approved it. Check it again to turn it off.")
        );
    }

    #[test]
    fn the_publishers_list_offers_revoke_only_for_owner_trusted_ones() {
        let mut facts = repositories(vec![], json!([]));
        facts.publishers = Read::Ready(from(json!({ "entries": [
            { "namespace": "@korri", "fingerprint": "k", "state": "Bound", "installed": [] },
            { "namespace": "@ada", "fingerprint": "a", "state": "Trusted", "installed": ["@ada:netplay"] }
        ] })));
        let groups = plugin_setting_groups(&facts, Some(&[]));
        let publishers = groups.last().unwrap();
        assert_eq!(publishers.title, PUBLISHERS_GROUP);
        assert!(publishers.items[0].interaction.is_none());
        assert_eq!(publishers.items[1].value.as_deref(), Some("Revoke"));
        assert_eq!(
            publishers.items[1].description.as_deref(),
            Some("Trusted on this device. Key a. 1 plugin installed from it.")
        );
    }

    #[test]
    fn ids_round_trip_with_colons_and_slashes() {
        let target = chiptune();
        assert_eq!(
            plugin_action_from(&inspect_action_id(&target)),
            Some(PluginAction::Inspect(target.clone()))
        );
        assert_eq!(
            plugin_action_from(&review_action_id(&target)),
            Some(PluginAction::Review(target.clone()))
        );
        assert_eq!(
            plugin_action_from(&trust_action_id(&target, "@a:b/c")),
            Some(PluginAction::Trust(target, "@a:b/c".into()))
        );
        assert_eq!(
            plugin_action_from(&remove_repository_action_id(RETRO)),
            Some(PluginAction::Remove(RETRO.into()))
        );
        assert_eq!(
            plugin_action_from(&revoke_action_id("@ada")),
            Some(PluginAction::Revoke("@ada".into()))
        );
        assert_eq!(
            plugin_change_from(&toggle_setting_id("@ada:x"), "true"),
            Some(PluginChange::Toggle("@ada:x".into(), true))
        );
        assert_eq!(
            plugin_change_from(&toggle_setting_id("@ada:x"), "maybe"),
            None
        );
        assert_eq!(plugin_change_from("device-name", "x"), None);
        assert_eq!(
            https_address(" https://retro.example/plugins "),
            Some("https://retro.example/plugins".into())
        );
        assert_eq!(https_address("http://retro.example"), None);
        assert_eq!(https_address("https://"), None);
        assert_eq!(enc("a b:c/é"), "a%20b%3Ac%2F%C3%A9");
        assert_eq!(dec("%E0%A4%A"), None);
    }

    struct Host {
        facts: Arc<Mutex<PluginFacts>>,
        status: Arc<Mutex<Vec<SurfaceSettingsStatus>>>,
        changed: Arc<Mutex<usize>>,
    }

    fn controller(korrid: Korrid, facts: PluginFacts) -> (Arc<PluginsController>, Host) {
        let facts = Arc::new(Mutex::new(facts));
        let status = Arc::new(Mutex::new(Vec::new()));
        let changed = Arc::new(Mutex::new(0));
        let busy = Arc::new(Mutex::new(false));
        let (read, write, log, count, get, set) = (
            Arc::clone(&facts),
            Arc::clone(&facts),
            Arc::clone(&status),
            Arc::clone(&changed),
            Arc::clone(&busy),
            Arc::clone(&busy),
        );
        let controller = PluginsController::new(
            korrid,
            PluginsHost {
                facts: Box::new(move || read.lock().unwrap().clone()),
                publish: Box::new(move |next| *write.lock().unwrap() = next),
                status: Box::new(move |next| log.lock().unwrap().push(next)),
                busy_get: Box::new(move || *get.lock().unwrap()),
                busy_set: Box::new(move |value| *set.lock().unwrap() = value),
                changed: Box::new(move || *count.lock().unwrap() += 1),
            },
        );
        (
            controller,
            Host {
                facts,
                status,
                changed,
            },
        )
    }

    async fn settle() {
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    #[tokio::test]
    async fn install_needs_an_inspection_and_passes_exactly_its_digest_then_turns_on() {
        let (korrid, script) = scripted();
        let (controller, host) = controller(korrid, retro_facts());
        // No kept report: nothing installs.
        assert!(controller.run(&review_action_id(&chiptune())));
        settle().await;
        assert!(script.lock().unwrap().calls.is_empty());
        push(
            &script,
            "system.plugins.inspect",
            json!({ "_tag": "Ok", "payload": serde_json::to_value(inspection("Trusted", json!([]))).unwrap() }),
        );
        controller.run(&inspect_action_id(&chiptune()));
        settle().await;
        assert!(review_for(&host.facts.lock().unwrap(), &chiptune()).is_some());
        push(
            &script,
            "system.plugins.install",
            json!({ "_tag": "Ok", "payload": {} }),
        );
        push(
            &script,
            "system.plugins.setEnabled",
            json!({ "_tag": "Ok", "payload": {} }),
        );
        controller.run(&review_action_id(&chiptune()));
        settle().await;
        let calls = script.lock().unwrap().calls.clone();
        let install = calls
            .iter()
            .find(|(tag, _)| tag == "system.plugins.install")
            .unwrap();
        assert_eq!(
            install.1,
            json!({ "url": RETRO, "pluginId": "@ada:chiptune", "releaseVersion": "1.0.0", "approval": "sha256:digest" })
        );
        let enable = calls
            .iter()
            .find(|(tag, _)| tag == "system.plugins.setEnabled")
            .unwrap();
        assert_eq!(enable.1, json!({ "id": "@ada:chiptune", "enabled": true }));
        assert_eq!(*host.changed.lock().unwrap(), 1);
        assert!(review_for(&host.facts.lock().unwrap(), &chiptune()).is_none());
        assert_eq!(
            host.status.lock().unwrap().last(),
            Some(&SurfaceSettingsStatus::Idle)
        );
    }

    #[tokio::test]
    async fn a_stale_approval_marks_needs_review_and_an_invalid_address_asks_nothing() {
        let (korrid, script) = scripted();
        let (controller, host) = controller(korrid, retro_facts());
        push(
            &script,
            "system.plugins.setEnabled",
            json!({ "_tag": "Err", "payload": { "code": NEEDS_REVIEW_CODE, "message": "changed" } }),
        );
        assert!(controller.change(&toggle_setting_id("@ada:netplay"), "false"));
        settle().await;
        assert_eq!(
            host.facts.lock().unwrap().needs_review.get("@ada:netplay"),
            Some(&false)
        );
        assert!(controller.change(ADD_REPOSITORY_SETTING, "http://nope"));
        assert_eq!(
            host.status.lock().unwrap().last(),
            Some(&SurfaceSettingsStatus::Problem {
                setting_id: ADD_REPOSITORY_SETTING.into(),
                message: INVALID_ADDRESS.into()
            })
        );
        assert_eq!(script.lock().unwrap().calls.len(), 1);
    }

    #[test]
    fn the_newest_trusted_release_wins() {
        let mut facts = retro_facts();
        facts.catalogs.insert(
            OFFICIAL.into(),
            CatalogView::Ready(from(json!([
                { "pluginId": "@ada:chiptune", "releaseVersion": "1.10.0", "publisher": publisher("@ada", "Trusted") },
                { "pluginId": "@ada:chiptune", "releaseVersion": "9.0.0", "publisher": publisher("@ada", "New") }
            ]))),
        );
        assert_eq!(
            newest_release(&facts, "@ada:chiptune")
                .unwrap()
                .release_version,
            "1.10.0"
        );
        assert_eq!(
            natural_compare("2026.10.2", "2026.9.30"),
            std::cmp::Ordering::Greater
        );
    }
}

mod placement {
    //! The surface window counts as placed only after korrid placed it.
    use super::acquisition_gate::{push, scripted};
    use super::*;
    use crate::surface::{placement::backoff, root::Root};
    use korri_surface_treaty::SurfacePackageDisplays;

    const DISPLAYS: &str = "app.surface.displays";

    fn placed(id: &str) -> Value {
        json!({ "_tag": "Ok", "payload": [{ "id": id, "main": true, "x": 0, "y": 0, "width": 640, "height": 480, "touch": false }] })
    }
    fn no_window() -> Value {
        json!({ "_tag": "Err", "payload": { "code": "SurfacePlacementFailed", "message": "no unique portal window" } })
    }
    fn shown(root: &Root) -> Option<Vec<String>> {
        root.display_facts()
            .map(|facts| facts.into_iter().map(|display| display.id).collect())
    }
    async fn settle() {
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    #[tokio::test]
    async fn a_failed_placement_is_retried_after_its_backoff() {
        let (korrid, script) = scripted();
        push(&script, DISPLAYS, no_window());
        push(&script, DISPLAYS, placed("DSI-1"));
        let root = Root::new(korrid, Arc::new(|| {}), Arc::default());
        root.show_package(SurfacePackageDisplays::All);
        settle().await;
        assert_eq!(shown(&root), None);
        // The next presentation change or window event comes inside the
        // backoff: korrid asks again once it has passed, not at once.
        root.place();
        settle().await;
        assert_eq!(
            script
                .lock()
                .unwrap()
                .calls
                .iter()
                .filter(|(tag, _)| tag == DISPLAYS)
                .count(),
            1
        );
        tokio::time::sleep(backoff(1)).await;
        settle().await;
        assert_eq!(
            script
                .lock()
                .unwrap()
                .calls
                .iter()
                .filter(|(tag, _)| tag == DISPLAYS)
                .count(),
            2
        );
        assert_eq!(shown(&root), Some(vec!["DSI-1".to_owned()]));
        // Placed: later events place nothing more.
        root.place();
        settle().await;
        assert_eq!(
            script
                .lock()
                .unwrap()
                .calls
                .iter()
                .filter(|(tag, _)| tag == DISPLAYS)
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn a_redeclaration_after_a_failed_switch_applies_its_displays() {
        let (korrid, script) = scripted();
        // The switch's placement fails: the new window is not mapped yet.
        push(&script, DISPLAYS, no_window());
        push(&script, DISPLAYS, placed("DSI-2"));
        let root = Root::new(korrid, Arc::new(|| {}), Arc::default());
        root.show_package(SurfacePackageDisplays::All);
        settle().await;
        assert_eq!(shown(&root), None);
        // The shell declares again once its window has mapped: placed at
        // once, inside the failure's backoff.
        root.show_package(SurfacePackageDisplays::All);
        settle().await;
        let calls: Vec<Value> = script
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(tag, _)| tag == DISPLAYS)
            .map(|(_, payload)| payload.clone())
            .collect();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1]["displays"], json!("all"));
        assert_eq!(shown(&root), Some(vec!["DSI-2".to_owned()]));
    }

    #[tokio::test]
    async fn a_placement_answered_after_a_redeclaration_counts_for_nothing() {
        let (korrid, script) = scripted();
        script
            .lock()
            .unwrap()
            .delays
            .insert(DISPLAYS.into(), Duration::from_millis(30));
        push(&script, DISPLAYS, placed("old-window"));
        push(&script, DISPLAYS, placed("new-window"));
        let root = Root::new(korrid, Arc::new(|| {}), Arc::default());
        root.show_package(SurfacePackageDisplays::All);
        tokio::task::yield_now().await;
        root.show_package(SurfacePackageDisplays::All);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(shown(&root), Some(vec!["new-window".to_owned()]));
    }
}

#[path = "launch_settings_tests.rs"]
mod launch_settings;

mod shared_play {
    //! Ports of `shared-play.test.ts`, `shared-play.model.test.ts` and
    //! `shared-play.netplay.test.ts`.
    use super::acquisition_gate::{push, scripted};
    use super::*;
    use crate::surface::shared_play::*;
    use std::sync::Mutex;

    fn netplay(phase: Value) -> crate::netplay::NetplayStatus {
        from(
            json!({ "launchId": "N1", "deviceLabel": "Odin", "gameTitle": "Turtles", "phase": phase }),
        )
    }

    fn invite(netplay: Value) -> crate::invites::Invite {
        let mut invite = json!({ "inviteId": "i1", "devicePublicKey": "k", "deviceLabel": "Odin", "personName": "Simon",
            "session": { "launchId": "L1", "gameId": "turtles", "title": "Turtles" } });
        if !netplay.is_null() {
            invite["netplay"] = netplay;
        }
        from(invite)
    }

    #[test]
    fn invites_carry_opaque_actions_and_one_netplay_route_with_its_words() {
        let state = LaunchablesState::Loading;
        let check = |route: Value, label: Option<&str>| {
            let invites = surface_invites_from(&[invite(route)], &state);
            let labels: Vec<String> = invites[0]
                .actions
                .iter()
                .map(|action| action.label.clone())
                .collect();
            let mut expected = vec!["Play on stream".to_owned()];
            expected.extend(label.map(Into::into));
            expected.push("Dismiss".into());
            assert_eq!(labels, expected);
            assert_eq!(invites[0].actions[0].id, "invite:stream:i1");
        };
        check(json!({ "_tag": "Ready" }), Some("Play here"));
        check(
            json!({ "_tag": "NeedsCopy", "bytes": 4.7e9 }),
            Some("Copy and play here (4.7 GB)"),
        );
        check(
            json!({ "_tag": "NeedsPlugin", "pluginId": "@korri:mgba", "title": "mGBA" }),
            Some("Install mGBA and play here"),
        );
        check(
            json!({ "_tag": "Unavailable", "reason": "NoNetplayRoute" }),
            None,
        );
        check(Value::Null, None);
    }

    #[test]
    fn korrids_report_becomes_busy_paused_or_reconnecting() {
        assert_eq!(
            serde_json::to_value(netplay_status_from(
                &netplay(
                    json!({ "_tag": "Copying", "done": 512_000_000u64, "total": 1_024_000_000u64 })
                ),
                Some("game:t".into())
            ))
            .unwrap(),
            json!({ "_tag": "Busy", "kicker": "Copying Turtles from Odin", "detail": "50% of 1 GB", "progress": { "done": 512_000_000.0, "total": 1_024_000_000.0 },
                "actions": [{ "id": "netplay:cancel", "label": "Cancel", "enabled": true }], "gameId": "game:t" })
        );
        assert!(
            matches!(netplay_status_from(&netplay(json!({ "_tag": "HolderLeft" })), None), Some(SurfaceStatus::Paused { actions, .. }) if actions[0].id == "netplay:leave:N1")
        );
        assert!(matches!(
            netplay_status_from(&netplay(json!({ "_tag": "Reconnecting" })), None),
            Some(SurfaceStatus::Busy { actions: None, .. })
        ));
        assert!(netplay_status_from(&netplay(json!({ "_tag": "Playing" })), None).is_none());
    }

    #[test]
    fn every_problem_names_the_game_and_the_device_and_codes_never_reach_the_person() {
        let facts = NetplayProblemFacts {
            game_title: "Turtles".into(),
            device_label: "Odin".into(),
            core: None,
        };
        let reason = |code: &str, message: &str| match netplay_problem(
            &crate::RpcFailure {
                code: code.into(),
                message: message.into(),
            },
            &facts,
        ) {
            SurfaceStatus::Problem { kicker, reason, .. } => (kicker, reason),
            _ => unreachable!(),
        };
        assert_eq!(
            reason("NoNetplayRoute", "x"),
            (
                "Netplay unavailable".into(),
                "Turtles can't play here with Odin right now. Play on stream still works.".into()
            )
        );
        assert_eq!(
            reason("CopyFailed", "x"),
            (
                "Copy failed".into(),
                "Turtles did not copy from Odin. Nothing was kept.".into()
            )
        );
        assert_eq!(
            reason("NotEnoughSpace", "The copy needs 4700000000 bytes."),
            (
                "Not enough space".into(),
                "Turtles needs 4.7 GB free on this device.".into()
            )
        );
        assert_eq!(
            reason("NotEnoughSpace", "odd"),
            (
                "Not enough space".into(),
                "Turtles needs more free space on this device.".into()
            )
        );
        assert_eq!(
            reason("CoreVersionMismatch", "x").1,
            "Odin runs a different version of the game's core. Play on stream still works."
        );
    }

    #[test]
    fn a_read_decides_between_idle_and_observing_and_keeps_the_game_only_for_the_same_launch() {
        let observing = shared_play_step(
            SharedPlay::Unread {
                game_id: Some("game:t".into()),
            },
            SharedPlayEvent::Read(Some(netplay(json!({ "_tag": "Connecting" })))),
        );
        assert!(
            matches!(&observing, SharedPlay::Observing { game_id: Some(id), .. } if id == "game:t")
        );
        let mut other = netplay(json!({ "_tag": "Connecting" }));
        other.launch_id = "N2".into();
        assert!(matches!(
            shared_play_step(observing.clone(), SharedPlayEvent::Read(Some(other))),
            SharedPlay::Observing { game_id: None, .. }
        ));
        assert!(matches!(
            shared_play_step(observing, SharedPlayEvent::Read(None)),
            SharedPlay::Idle
        ));
    }

    #[test]
    fn cancel_on_korrids_report_cancels_copying_and_connecting_only() {
        for (phase, cancels) in [
            ("Copying", true),
            ("Connecting", true),
            ("Playing", false),
            ("HolderLeft", false),
            ("Reconnecting", false),
        ] {
            let report = if phase == "Copying" {
                json!({ "_tag": phase, "done": 1, "total": 2 })
            } else {
                json!({ "_tag": phase })
            };
            let next = shared_play_step(
                SharedPlay::Observing {
                    netplay: netplay(report),
                    game_id: None,
                },
                SharedPlayEvent::CancelPressed,
            );
            assert_eq!(
                matches!(next, SharedPlay::CancellingNetplay { launch_id: Some(ref id), sent: false, pending: None } if id == "N1"),
                cancels,
                "{phase}"
            );
        }
    }

    #[test]
    fn one_piece_of_work_at_a_time_and_problems_end_with_dismiss() {
        let starting = shared_play_step(
            SharedPlay::Idle,
            SharedPlayEvent::StreamRequested {
                op: 1,
                device_label: "Odin".into(),
            },
        );
        let still = shared_play_step(
            starting.clone(),
            SharedPlayEvent::LeaveRequested {
                op: 2,
                kicker: "Leaving".into(),
            },
        );
        assert!(matches!(still, SharedPlay::StreamStarting { op: 1, .. }));
        let cancelling = shared_play_step(starting, SharedPlayEvent::CancelPressed);
        assert!(matches!(
            cancelling,
            SharedPlay::CancellingStream { pending: 1 }
        ));
        assert_eq!(
            serde_json::to_value(shared_play_status(&cancelling)).unwrap()["kicker"],
            "Leaving stream"
        );
        let problem = shared_play_step(
            cancelling,
            SharedPlayEvent::Failed(SurfaceStatus::Problem {
                kicker: "k".into(),
                reason: "r".into(),
                can_retry: false,
                game_id: None,
                game_title: None,
            }),
        );
        assert!(shared_play_status(&problem).is_some());
        assert!(matches!(
            shared_play_step(problem, SharedPlayEvent::Dismissed),
            SharedPlay::Unread { .. }
        ));
    }

    fn controller(korrid: crate::surface::korrid::Korrid) -> Arc<SharedPlayController> {
        SharedPlayController::new(
            korrid,
            Arc::new(|| {}),
            SharedPlayContext {
                state: Box::new(|| {
                    LaunchablesState::recover_startup(Default::default(), None, &[])
                }),
                refresh_session: Box::new(|| Box::pin(async {})),
                plugins_changed: Box::new(|| {}),
            },
        )
    }

    async fn settle() {
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    #[tokio::test]
    async fn play_on_stream_starts_and_a_cancel_waits_then_leaves_only_this_viewer() {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.invites.list",
            json!({ "_tag": "Ok", "payload": { "invites": [serde_json::to_value(invite(Value::Null)).unwrap()] } }),
        );
        push(
            &script,
            "app.session.status",
            json!({ "_tag": "Ok", "payload": {} }),
        );
        push(
            &script,
            "app.invites.streamPlay",
            json!({ "_tag": "Ok", "payload": { "launchId": "V1" } }),
        );
        script
            .lock()
            .unwrap()
            .delays
            .insert("app.invites.streamPlay".into(), Duration::from_millis(200));
        push(
            &script,
            "app.session.leave",
            json!({ "_tag": "Ok", "payload": { "launchId": "V1", "state": "frozen", "changed": true } }),
        );
        let shared = controller(korrid);
        shared.refresh().await;
        assert!(shared.act("invite:stream:i1"));
        settle().await;
        assert_eq!(
            serde_json::to_value(shared.status()).unwrap()["kicker"],
            "Connecting to Odin"
        );
        assert!(shared.act("stream-play:cancel"));
        tokio::time::sleep(Duration::from_millis(400)).await;
        let calls: Vec<(String, Value)> = script.lock().unwrap().calls.clone();
        assert!(calls.iter().any(
            |(tag, payload)| tag == "app.session.leave" && payload["expectedLaunchId"] == "V1"
        ));
        assert!(!calls.iter().any(|(tag, _)| tag == "app.session.end"));
        assert!(shared.status().is_none());
    }

    #[tokio::test]
    async fn a_refused_netplay_is_a_problem_in_plain_words_and_dismiss_ends_it() {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.invites.list",
            json!({ "_tag": "Ok", "payload": { "invites": [serde_json::to_value(invite(json!({ "_tag": "Ready" }))).unwrap()] } }),
        );
        push(
            &script,
            "app.invites.netplay",
            json!({ "_tag": "Err", "payload": { "code": "NoNetplayRoute", "message": "raw" } }),
        );
        let shared = controller(korrid);
        shared.refresh().await;
        assert!(shared.act("invite:netplay:i1"));
        settle().await;
        assert_eq!(
            serde_json::to_value(shared.status()).unwrap(),
            json!({ "_tag": "Problem", "kicker": "Netplay unavailable", "reason": "Turtles can't play here with Odin right now. Play on stream still works.", "canRetry": false })
        );
        assert!(shared.dismiss());
        assert!(shared.status().is_none());
    }

    #[tokio::test]
    async fn no_repository_offering_the_plugin_is_a_problem_that_says_so() {
        let (korrid, script) = scripted();
        push(
            &script,
            "app.invites.list",
            json!({ "_tag": "Ok", "payload": { "invites": [serde_json::to_value(invite(json!({ "_tag": "NeedsPlugin", "pluginId": "@korri:mgba", "title": "mGBA" }))).unwrap()] } }),
        );
        push(
            &script,
            "system.plugins.repositories",
            json!({ "_tag": "Ok", "payload": { "userAdded": [], "repositoryInstalls": [], "otherInstalls": [] } }),
        );
        push(
            &script,
            "system.plugins.publishers",
            json!({ "_tag": "Ok", "payload": { "entries": [] } }),
        );
        let shared = controller(korrid);
        shared.refresh().await;
        assert!(shared.act("invite:netplay:i1"));
        settle().await;
        assert_eq!(
            serde_json::to_value(shared.status()).unwrap(),
            json!({ "_tag": "Problem", "kicker": "Install failed", "reason": "No plugin repository on this device offers mGBA.", "canRetry": false })
        );
        let _ = Mutex::new(());
    }
}
