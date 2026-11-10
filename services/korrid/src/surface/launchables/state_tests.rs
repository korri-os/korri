//! Port of `clients/portal/src/launchables/state.test.ts`. Fixtures are the
//! portal's JSON, decoded into korrid's own RPC types.

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use super::state::*;
use crate::{
    CatalogSnapshotOutcome, Game, LocalGamesListOutcome, SessionEndOutcome, SessionStatusOutcome,
};

fn from<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("fixture decodes")
}

fn tag(state: &LaunchablesState) -> &'static str {
    match state {
        LaunchablesState::Loading => "Loading",
        LaunchablesState::Loaded(Loaded { phase, .. }) => match phase {
            Phase::Ready => "Ready",
            Phase::Preparing { .. } => "Preparing",
            Phase::Launching { .. } => "Launching",
            Phase::Stopping { .. } => "Stopping",
            Phase::Starting { .. } => "Starting",
            Phase::Choosing { .. } => "Choosing",
            Phase::Recovery => "Recovery",
        },
    }
}

fn content(state: &LaunchablesState) -> &Content {
    &state.loaded().expect("loaded").content
}

fn notice(state: &LaunchablesState) -> Option<&LaunchNotice> {
    content(state).notice.as_ref()
}

fn now_playing(state: &LaunchablesState) -> Entry {
    state
        .entries()
        .iter()
        .find(|entry| entry.is_now_playing())
        .expect("a now-playing entry")
        .clone()
}

fn session_json(entry: &Entry) -> Value {
    serde_json::to_value(entry.session().unwrap()).unwrap()
}

fn source(label: &str) -> Value {
    json!({ "label": label, "isLocal": false })
}

fn games_ok() -> CatalogSnapshotOutcome {
    from(json!({ "_tag": "Ok", "payload": { "games": [
        { "id": "skate3", "title": "Skate 3", "supportsRunnerSelection": false, "source": source("aka") },
        { "id": "neverball", "title": "Neverball", "supportsRunnerSelection": false, "source": source("zao") },
    ] } }))
}

fn games_err() -> CatalogSnapshotOutcome {
    from(
        json!({ "_tag": "Err", "payload": { "code": "UpstreamUnreachable", "message": "host offline" } }),
    )
}

fn ready() -> LaunchablesState {
    LaunchablesState::from_sources(&games_ok(), None, None, &[], None)
}

fn catalog_of(games: Vec<Value>) -> CatalogSnapshotOutcome {
    from(json!({ "_tag": "Ok", "payload": { "games": games } }))
}

fn status(value: Value) -> SessionStatusOutcome {
    from(value)
}

fn ready_with(entries: Vec<Entry>) -> LaunchablesState {
    LaunchablesState::recover_startup(
        Content {
            entries,
            ..Content::default()
        },
        None,
        &[],
    )
}

mod one_game_one_card {
    use super::*;
    use crate::surface::model::{catalog_games, launch_locations_for_entry};

    #[test]
    fn remote_session_matches_an_alternative_and_preserves_the_whole_folded_card() {
        let shared = json!({ "kind": "hash", "value": "same-rom" });
        let catalog = catalog_of(vec![
            json!({ "id": "other", "title": "Other", "source": source("aka"), "host": "aka", "supportsRunnerSelection": false }),
            json!({ "id": "aka-id", "title": "Game", "source": source("aka"), "host": "aka", "supportsRunnerSelection": false, "identity": shared }),
            json!({ "id": "zao-id", "title": "Game", "source": source("zao"), "host": "zao", "supportsRunnerSelection": false, "identity": shared }),
        ]);
        let before = LaunchablesState::from_sources(&catalog, None, None, &[], None);
        let normal_cards = catalog_games(&before).unwrap();
        let active = status(
            json!({ "_tag": "Ok", "payload": { "active": { "launchId": "remote-launch", "gameId": "zao-id", "host": "zao" } } }),
        );
        let running = before.clone().with_session_status(&active);
        assert_eq!(running.entries().len(), 2);
        let card = &running.entries()[0];
        assert_eq!(card.key(), before.entries()[1].key());
        assert_eq!(card.session().unwrap().launch_id, "remote-launch");
        assert_eq!(
            catalog_games(&running).unwrap()[0].subtitle.as_deref(),
            Some("zao")
        );
        assert_eq!(card.alternatives().len(), 1);
        assert_eq!(card.catalog_games().count(), 2);
        // Launch locations cannot replace the one exact resumable session.
        assert!(launch_locations_for_entry(card).is_empty());
        let again = running.with_session_status(&active);
        assert_eq!(again.entries().len(), 2);
        let ended = again.with_session_status(&status(json!({ "_tag": "Ok", "payload": {} })));
        assert_eq!(catalog_games(&ended).unwrap(), normal_cards);
        assert_eq!(launch_locations_for_entry(&ended.entries()[1]).len(), 2);
    }

    #[test]
    fn local_session_moves_the_catalog_card_and_stop_restores_its_original_position() {
        let catalog = catalog_of(vec![
            json!({ "id": "other", "title": "Other", "source": source("aka"), "host": "aka", "supportsRunnerSelection": false }),
            json!({ "id": "local", "title": "Local", "source": { "label": "this", "isLocal": true }, "host": "this", "supportsRunnerSelection": false }),
        ]);
        let before = LaunchablesState::from_sources(&catalog, None, None, &[], None);
        let active = status(
            json!({ "_tag": "Ok", "payload": { "active": { "launchId": "local-launch", "gameId": "local" } } }),
        );
        let running = before.clone().with_session_status(&active);
        assert_eq!(running.entries().len(), 2);
        assert_eq!(running.entries()[0].key(), before.entries()[1].key());
        let stopped = running
            .clone()
            .begin_stopping(&running.entries()[0])
            .with_status_after_stop(&status(json!({ "_tag": "Ok", "payload": {} })));
        assert_eq!(
            catalog_games(&stopped).unwrap(),
            catalog_games(&before).unwrap()
        );
    }
}

mod catalog_session_locality {
    use super::*;

    fn catalog_game() -> Value {
        json!({ "id": "wl4", "title": "Wario Land 4", "host": "local-label", "supportsRunnerSelection": false,
            "source": { "label": "local-label", "isLocal": true } })
    }
    fn session() -> Value {
        json!({ "launchId": "exact-launch", "gameId": "wl4", "host": "local-label" })
    }
    fn game_entry(game: Value) -> Entry {
        Entry::Game {
            game: from(game),
            alternatives: Vec::new(),
        }
    }

    #[test]
    fn matches_the_source_local_catalog_when_linux_status_omits_the_catalog_label() {
        let entries = vec![game_entry(catalog_game())];
        let check = |session: Value| is_local_catalog_session(&from(session), &entries);
        assert!(check(session()));
        assert!(!check(
            json!({ "launchId": "exact-launch", "gameId": "wl4", "host": "peer" })
        ));
        assert!(check(
            json!({ "launchId": "exact-launch", "gameId": "wl4", "phase": "running" })
        ));
        assert!(!check(
            json!({ "launchId": "exact-launch", "gameId": "other", "host": "local-label" })
        ));
        let mut remote = catalog_game();
        remote["source"]["isLocal"] = json!(false);
        assert!(!is_local_catalog_session(
            &from(session()),
            &[game_entry(remote)]
        ));
    }

    #[test]
    fn keeps_same_id_remote_copies_isolated_including_folded_alternatives() {
        let mut peer = catalog_game();
        peer["host"] = json!("peer");
        peer["source"] = source("peer");
        let entries = vec![Entry::Game {
            game: from(peer.clone()),
            alternatives: vec![super::super::fold::GameCopy::Remote(from(catalog_game()))],
        }];
        let linux: Value =
            json!({ "launchId": "linux-launch", "gameId": "wl4", "phase": "running" });
        assert!(is_local_catalog_session(&from(linux.clone()), &entries));
        let mut hosted = linux.clone();
        hosted["host"] = json!("peer");
        assert!(!is_local_catalog_session(&from(hosted), &entries));
        assert!(!is_local_catalog_session(
            &from(linux.clone()),
            &[game_entry(peer.clone())]
        ));
        let mut hostless = peer;
        hostless.as_object_mut().unwrap().remove("host");
        assert!(!is_local_catalog_session(
            &from(linux),
            &[game_entry(hostless)]
        ));
        assert!(!is_local_catalog_session(
            &from(json!({ "launchId": "unknown-game" })),
            &entries
        ));
    }

    #[test]
    fn finds_a_local_catalog_route_folded_under_another_primary_copy() {
        let entries = vec![Entry::LocalGame {
            game: from(json!({ "id": "peer-copy", "title": "Wario Land 4", "system": "GBA" })),
            alternatives: vec![super::super::fold::GameCopy::Remote(from(catalog_game()))],
        }];
        assert!(is_local_catalog_session(&from(session()), &entries));
    }

    #[test]
    fn publishes_prepare_identity_immediately_then_retains_it_across_failed_status_until_idle() {
        let game: Game = from(catalog_game());
        let ready = ready_with(vec![game_entry(catalog_game())]);
        let preparing = ready.clone().begin_preparing(&game.title, None);
        let acknowledged = preparing.with_local_catalog_prepare_outcome(
            &from(json!({ "_tag": "Ok", "payload": { "session": { "gameId": "wl4", "launchId": "exact-launch" }, "warnings": [] } })),
            &game,
        );
        assert_eq!(tag(&acknowledged), "Ready");
        let entry = now_playing(&acknowledged);
        assert_eq!(
            session_json(&entry),
            json!({ "launchId": "exact-launch", "gameId": "wl4", "host": "local-label", "title": "Wario Land 4" })
        );
        assert!(
            matches!(&entry, Entry::NowPlaying { local_catalog_game: Some(game), .. } if game.id == "wl4")
        );
        let timeout = status(
            json!({ "_tag": "Err", "payload": { "code": "StatusTimeout", "message": "timeout" } }),
        );
        let failed_read = acknowledged.with_session_status(&timeout);
        assert_eq!(tag(&failed_read), "Ready");
        assert!(content(&failed_read).unavailable_session_status.is_some());
        let healthy =
            failed_read.with_session_status(&status(json!({ "_tag": "Ok", "payload": {} })));
        assert_eq!(tag(&healthy), "Ready");
        assert!(!healthy.entries().iter().any(Entry::is_now_playing));
        assert_eq!(healthy.entries().len(), ready.entries().len());
        assert!(content(&healthy).unavailable_session_status.is_none());
    }

    #[test]
    fn treats_session_completed_and_no_active_session_as_authoritative() {
        for code in ["SessionCompleted", "NoActiveSession"] {
            let running = status(
                json!({ "_tag": "Ok", "payload": { "active": { "launchId": "exact-launch", "gameId": "wl4", "phase": "running" } } }),
            );
            let completed = status(
                json!({ "_tag": "Err", "payload": { "code": code, "message": "no host launch is active" } }),
            );
            let catalog = catalog_of(vec![catalog_game()]);
            let loaded = LaunchablesState::from_sources(&catalog, Some(&running), None, &[], None);
            let stopped = loaded.clone().with_session_status(&completed);
            assert_eq!(tag(&stopped), "Ready");
            assert!(notice(&stopped).is_none());
            assert!(!stopped.entries().iter().any(Entry::is_now_playing));
            let fresh = LaunchablesState::from_sources(&catalog, Some(&completed), None, &[], None);
            assert_eq!(tag(&fresh), "Ready");
            assert_eq!(fresh.entries().len(), stopped.entries().len());
            let stopping = loaded.clone().begin_stopping(&now_playing(&loaded));
            let after = stopping.with_status_after_stop(&completed);
            assert_eq!(tag(&after), "Ready");
            assert!(!after.entries().iter().any(Entry::is_now_playing));
            let unavailable = status(
                json!({ "_tag": "Err", "payload": { "code": "BrainUnreachable", "message": "disconnected" } }),
            );
            let kept = loaded.with_session_status(&unavailable);
            assert_eq!(tag(&kept), "Ready");
            assert!(kept.entries().iter().any(Entry::is_now_playing));
            assert!(content(&kept).unavailable_session_status.is_some());
        }
    }

    #[test]
    fn retains_source_evidence_only_for_the_same_launch_without_restoring_stale_catalog_games() {
        let active = json!({ "launchId": "exact-launch", "gameId": "wl4" });
        let running = status(json!({ "_tag": "Ok", "payload": { "active": active } }));
        let loaded = LaunchablesState::from_sources(
            &catalog_of(vec![catalog_game()]),
            Some(&running),
            None,
            &[],
            None,
        );
        let failed: CatalogSnapshotOutcome = from(
            json!({ "_tag": "Err", "payload": { "code": "BrainUnreachable", "message": "disconnected" } }),
        );
        let refreshed =
            LaunchablesState::from_sources(&failed, Some(&running), None, loaded.entries(), None);
        assert!(!refreshed
            .entries()
            .iter()
            .any(|entry| matches!(entry, Entry::Game { .. })));
        assert!(is_local_catalog_session(
            &from(active.clone()),
            refreshed.entries()
        ));
        let polled = refreshed.with_session_status(&running);
        assert!(is_local_catalog_session(&from(active), polled.entries()));
        for replacement in [
            json!({ "launchId": "replacement", "gameId": "wl4" }),
            json!({ "launchId": "exact-launch", "gameId": "wl4", "host": "peer" }),
            json!({ "launchId": "exact-launch", "gameId": "other" }),
        ] {
            let replaced = polled.clone().with_session_status(&status(
                json!({ "_tag": "Ok", "payload": { "active": replacement } }),
            ));
            assert_eq!(tag(&replaced), "Ready");
            assert!(!is_local_catalog_session(
                &from(replacement),
                replaced.entries()
            ));
        }
    }

    #[test]
    fn a_status_observation_cannot_unlock_a_command_in_progress() {
        let preparing = ready().begin_preparing("Game", None);
        let observed =
            preparing.with_session_status(&status(json!({ "_tag": "Ok", "payload": {} })));
        assert_eq!(tag(&observed), "Preparing");
    }
}

mod from_sources {
    use super::*;

    #[test]
    fn shows_matching_local_and_zao_copies_as_one_local_first_game() {
        let identity = json!({ "kind": "hash", "value": "sha256:wario" });
        let state = LaunchablesState::from_sources(
            &catalog_of(vec![
                json!({ "id": "wl4", "title": "Wario Land 4", "host": "zao", "identity": identity, "supportsRunnerSelection": false, "source": source("zao") }),
            ]),
            None,
            Some(&from::<LocalGamesListOutcome>(
                json!({ "_tag": "Ok", "payload": { "games": [
                { "id": "wl4", "title": "Wario Land 4", "system": "Game Boy Advance", "identity": identity }
            ] } }),
            )),
            &[],
            None,
        );
        assert_eq!(state.entries().len(), 1);
        let Entry::LocalGame { game, alternatives } = &state.entries()[0] else {
            panic!("expected a local game")
        };
        assert_eq!(game.id, "wl4");
        assert_eq!(alternatives.len(), 1);
        assert_eq!(
            alternatives[0].remote().unwrap().host.as_deref(),
            Some("zao")
        );
    }

    #[test]
    fn degrades_a_failed_local_game_source_to_a_notice_while_entries_remain() {
        let state = LaunchablesState::from_sources(
            &games_ok(),
            None,
            Some(&from(
                json!({ "_tag": "Err", "payload": { "code": "LocalStorageUnavailable", "message": "storage denied" } }),
            )),
            &[],
            None,
        );
        assert_eq!(tag(&state), "Ready");
        assert_eq!(
            notice(&state),
            Some(&LaunchNotice::Catalog {
                message: "Korri couldn't read this device's games.".into()
            })
        );
    }

    #[test]
    fn surfaces_local_configuration_failures_while_keeping_healthy_local_games() {
        let state = LaunchablesState::from_sources(
            &games_ok(),
            None,
            Some(&from(json!({ "_tag": "Ok", "payload": {
                "games": [{ "id": "wl4", "title": "Wario Land 4", "system": "Game Boy Advance" }],
                "failures": [{ "code": "LocalConfigReloadFailed", "message": "library.yaml is malformed" }]
            } }))),
            &[],
            None,
        );
        assert_eq!(
            notice(&state).map(LaunchNotice::message),
            Some("This device's library settings couldn't be read.")
        );
        assert!(matches!(&state.entries()[0], Entry::LocalGame { game, .. } if game.id == "wl4"));
    }

    #[test]
    fn a_game_without_a_runner_does_not_hide_the_games_that_have_one() {
        let state = LaunchablesState::from_sources(
            &games_ok(),
            None,
            Some(&from(json!({ "_tag": "Ok", "payload": {
                "games": [{ "id": "anguna", "title": "Anguna", "system": "Game Boy Advance" }],
                "failures": [
                    { "code": "LocalRouteUnavailable", "message": "game eggshell-1 has no installed Linux runner" },
                    { "code": "LocalRouteUnavailable", "message": "game eggshell-2 has no installed Linux runner" },
                    { "code": "LocalRomMissing", "message": "The ROM file is missing" },
                    { "code": "LaunchSettingUnsupported", "message": "shader skipped" }
                ]
            } }))),
            &[],
            None,
        );
        assert_eq!(tag(&state), "Ready");
        assert!(notice(&state).is_none());
        assert!(state
            .entries()
            .iter()
            .any(|entry| matches!(entry, Entry::LocalGame { game, .. } if game.id == "anguna")));
    }

    #[test]
    fn an_empty_library_says_in_plain_words_why_its_games_are_missing() {
        let state = LaunchablesState::from_sources(
            &catalog_of(vec![]),
            None,
            Some(&from(json!({ "_tag": "Ok", "payload": {
                "games": [],
                "failures": [
                    { "code": "LocalRouteUnavailable", "message": "game eggshell-1 has no installed Linux runner" },
                    { "code": "LocalRouteUnavailable", "message": "game eggshell-2 has no installed Linux runner" },
                    { "code": "LocalConfigReloadFailed", "message": "library.yaml is malformed" }
                ]
            } }))),
            &[],
            None,
        );
        assert_eq!(
            notice(&state).map(LaunchNotice::message),
            Some("2 games need a runner that isn't installed. This device's library settings couldn't be read.")
        );
    }

    #[test]
    fn an_absent_inventory_capability_is_not_a_failed_read() {
        let state = LaunchablesState::from_sources(
            &games_ok(),
            None,
            Some(&from(
                json!({ "_tag": "Err", "payload": { "code": "OperationUnsupported", "message": "no" } }),
            )),
            &[],
            None,
        );
        assert!(notice(&state).is_none());
    }

    #[test]
    fn degrades_a_failed_korrid_catalog_to_a_notice() {
        let state = LaunchablesState::from_sources(&games_err(), None, None, &[], None);
        assert_eq!(
            notice(&state).map(LaunchNotice::message),
            Some("Korri couldn't read the game library.")
        );
    }

    #[test]
    fn keeps_healthy_games_browsable_when_a_peer_catalog_fails() {
        let state = LaunchablesState::from_sources(
            &from(json!({ "_tag": "Ok", "payload": {
                "games": [{ "id": "legacy", "title": "Legacy game", "host": "aka", "supportsRunnerSelection": false, "source": source("aka") }],
                "failures": [{ "host": "zao", "code": "UpstreamUnreachable", "message": "connection refused" }]
            } })),
            None,
            None,
            &[],
            None,
        );
        assert_eq!(tag(&state), "Ready");
        assert!(notice(&state).is_none());
        assert_eq!(state.entries().len(), 1);
    }

    #[test]
    fn an_empty_read_is_provisional_while_peers_connect() {
        let peers = from(json!({ "_tag": "Ok", "payload": { "peers": [
            { "devicePublicKey": "k", "label": "zao", "state": "loading", "updatedAt": 1 }
        ] } }));
        let state =
            LaunchablesState::from_sources(&catalog_of(vec![]), None, None, &[], Some(&peers));
        assert_eq!(tag(&state), "Loading");
        let failed_peers =
            from(json!({ "_tag": "Err", "payload": { "code": "X", "message": "y" } }));
        let state = LaunchablesState::from_sources(
            &catalog_of(vec![]),
            None,
            None,
            &[],
            Some(&failed_peers),
        );
        assert_eq!(
            notice(&state).map(LaunchNotice::message),
            Some("Korri couldn't list your other devices.")
        );
    }

    #[test]
    fn qualifies_duplicate_game_ids_by_host() {
        let keys: Vec<String> = ["aka", "zao"]
            .iter()
            .map(|host| {
                Entry::Game {
                    game: from(json!({ "id": "shared", "title": "Shared", "host": host, "supportsRunnerSelection": false, "source": source(host) })),
                    alternatives: Vec::new(),
                }
                .key()
            })
            .collect();
        assert_eq!(keys, vec!["game:aka:shared", "game:zao:shared"]);
    }
}

mod now_playing {
    use super::*;

    fn session_active() -> SessionStatusOutcome {
        status(
            json!({ "_tag": "Ok", "payload": { "active": { "launchId": "l1", "gameId": "skate3", "title": "Skate 3", "phase": "running" } } }),
        )
    }
    fn session_idle() -> SessionStatusOutcome {
        status(json!({ "_tag": "Ok", "payload": {} }))
    }

    #[test]
    fn renders_an_active_session_as_a_selectable_banner_entry_first() {
        let state =
            LaunchablesState::from_sources(&games_ok(), Some(&session_active()), None, &[], None);
        assert_eq!(
            session_json(&state.entries()[0]),
            json!({ "launchId": "l1", "gameId": "skate3", "title": "Skate 3", "phase": "running" })
        );
    }

    #[test]
    fn shows_no_banner_when_nothing_is_playing() {
        let state =
            LaunchablesState::from_sources(&games_ok(), Some(&session_idle()), None, &[], None);
        assert!(!state.entries().iter().any(Entry::is_now_playing));
    }

    #[test]
    fn degrades_a_status_failure_silently() {
        let error = status(
            json!({ "_tag": "Err", "payload": { "code": "HostUnavailable", "message": "host is unavailable" } }),
        );
        let state = LaunchablesState::from_sources(&games_ok(), Some(&error), None, &[], None);
        assert!(!state.entries().iter().any(Entry::is_now_playing));
        assert!(notice(&state).is_none());
    }

    #[test]
    fn stop_ok_enters_an_input_locked_stopping_case_until_status_is_idle() {
        let with_banner =
            LaunchablesState::from_sources(&games_ok(), Some(&session_active()), None, &[], None);
        let ok: SessionEndOutcome = from(json!({ "_tag": "Ok", "payload": { "phase": "ended" } }));
        let stop_requested = with_banner
            .clone()
            .begin_stopping(&now_playing(&with_banner));
        assert_eq!(tag(&stop_requested), "Stopping");
        let stopping = stop_requested.clone().with_stop_outcome(&ok);
        assert_eq!(tag(&stopping), "Stopping");
        let still_active = stopping.clone().with_status_after_stop(&session_active());
        assert_eq!(tag(&still_active), "Stopping");
        let stopped = still_active.with_status_after_stop(&session_idle());
        assert_eq!(tag(&stopped), "Ready");
        assert!(!stopped.entries().iter().any(Entry::is_now_playing));
        let replacement = status(
            json!({ "_tag": "Ok", "payload": { "active": { "launchId": "l2", "title": "Different game" } } }),
        );
        assert_eq!(tag(&stopping.with_status_after_stop(&replacement)), "Ready");
        let failed = stop_requested.with_stop_outcome(&from(
            json!({ "_tag": "Err", "payload": { "code": "HostUnavailable", "message": "host is unavailable" } }),
        ));
        assert_eq!(
            notice(&failed).map(LaunchNotice::message),
            Some("HostUnavailable: host is unavailable")
        );
    }

    #[test]
    fn returns_to_the_list_with_a_truthful_notice_when_pending_stop_times_out() {
        let with_banner =
            LaunchablesState::from_sources(&games_ok(), Some(&session_active()), None, &[], None);
        let stopping = with_banner
            .clone()
            .begin_stopping(&now_playing(&with_banner))
            .with_stop_outcome(&from(
                json!({ "_tag": "Ok", "payload": { "phase": "pending" } }),
            ));
        let timed_out = stopping.stop_timed_out();
        assert_eq!(tag(&timed_out), "Ready");
        assert_eq!(
            notice(&timed_out),
            Some(&LaunchNotice::Stop {
                message: "The session is still ending.".into()
            })
        );
    }

    #[test]
    fn prepare_err_restores_the_list_with_a_notice() {
        let game: Game = from(
            json!({ "id": "skate3", "title": "Skate 3", "supportsRunnerSelection": false, "source": source("aka") }),
        );
        let failed = ready()
            .begin_preparing("Skate 3", None)
            .with_local_catalog_prepare_outcome(
                &from(json!({ "_tag": "Err", "payload": { "code": "UpstreamUnreachable", "message": "host offline" } })),
                &game,
            );
        assert_eq!(
            notice(&failed).map(LaunchNotice::message),
            Some("UpstreamUnreachable: host offline")
        );
    }
}

mod owned_local_startup {
    use super::*;

    fn game() -> Value {
        json!({ "id": "owned", "title": "Owned game", "supportsRunnerSelection": false, "source": { "label": "This device", "isLocal": true } })
    }
    fn reserved() -> LaunchablesState {
        let game: Game = from(game());
        LaunchablesState::from_sources(
            &catalog_of(vec![super::owned_local_startup::game()]),
            None,
            None,
            &[],
            None,
        )
        .begin_startup(&game)
        .with_startup_reservation(
            &from(json!({ "gameId": "owned", "launchId": "exact-owned" })),
            &game,
        )
    }
    fn observed(focus: Option<&str>) -> SessionStatusOutcome {
        let mut active = json!({ "gameId": "owned", "launchId": "exact-owned", "phase": "running",
            "initialHandoff": if focus == Some("launch") { "observed" } else { "waiting" } });
        if let Some(focus) = focus {
            active["focusOwnership"] = json!(focus);
        }
        status(json!({ "_tag": "Ok", "payload": { "active": active } }))
    }

    #[test]
    fn ack_and_focus_do_not_prove_readiness() {
        for focus in [None, Some("excluded"), Some("other")] {
            assert_eq!(
                tag(&reserved().with_session_status(&observed(focus))),
                "Starting"
            );
        }
    }

    #[test]
    fn pending_daemon_preparation_retains_startup_and_exact_initial_handoff_resolves() {
        let pending = status(json!({ "_tag": "Ok", "payload": { "pendingLaunches": [
            { "session": { "gameId": "owned", "launchId": "exact-owned" }, "phase": "preparing" }
        ] } }));
        assert_eq!(tag(&reserved().with_session_status(&pending)), "Starting");
        let handed_off = reserved().with_session_status(&observed(Some("launch")));
        assert_eq!(tag(&handed_off), "Ready");
        assert_eq!(
            tag(&handed_off.with_session_status(&observed(None))),
            "Ready"
        );
    }

    #[test]
    fn reports_a_failed_host_launch_without_treating_it_as_idle() {
        for cancelling in [false, true] {
            let initial = reserved();
            let state = if cancelling {
                initial.cancel_startup()
            } else {
                initial
            };
            let failure = status(
                json!({ "_tag": "Err", "payload": { "code": "HostLaunchFailed", "message": "game unit failed with signal 6" } }),
            );
            let observed = state.clone().with_session_status(&failure);
            let LaunchablesState::Loaded(Loaded {
                phase:
                    Phase::Starting {
                        launch_id,
                        cancelling: now_cancelling,
                        cancel_retry_available,
                        ..
                    },
                content,
            }) = &observed
            else {
                panic!("expected Starting")
            };
            assert_eq!(launch_id.as_deref(), Some("exact-owned"));
            assert_eq!(*now_cancelling, cancelling);
            assert_eq!(*cancel_retry_available, Some(true));
            assert_eq!(content.entries.len(), state.entries().len());
            assert!(
                matches!(&content.notice, Some(LaunchNotice::Launch { message, subject: Some(subject) })
                if message == "game unit failed with signal 6" && subject.id.as_deref() == Some("owned") && subject.title == "Owned game")
            );
            let game: Game = from(game());
            assert_eq!(tag(&observed.clone().begin_startup(&game)), "Starting");
        }
    }

    #[test]
    fn observed_exact_exit_retires_a_still_unacknowledged_start() {
        let pending = reserved().with_session_status(&observed(None));
        assert_eq!(tag(&pending), "Starting");
        assert_eq!(
            tag(&pending.with_session_status(&status(json!({ "_tag": "Ok", "payload": {} })))),
            "Ready"
        );
    }

    #[test]
    fn a_missed_final_focus_reply_recovers_on_frozen_or_focus_failed() {
        for phase in ["frozen", "focus-failed"] {
            let snapshot = status(json!({ "_tag": "Ok", "payload": { "active": {
                "gameId": "owned", "launchId": "exact-owned", "phase": phase, "initialHandoff": "waiting" } } }));
            assert_eq!(tag(&reserved().with_session_status(&snapshot)), "Ready");
        }
    }

    #[test]
    fn pending_cancellation_ignores_focus_and_observation_failure_until_exact_exit() {
        let state = reserved().cancel_startup();
        assert_eq!(
            tag(&state.clone().with_session_status(&observed(Some("launch")))),
            "Starting"
        );
        let offline = state.clone().with_session_status(&status(json!({ "_tag": "Err", "payload": { "code": "HostUnavailable", "message": "offline" } })));
        assert_eq!(tag(&offline), "Starting");
        let ended = state.clone().with_session_status(&status(
            json!({ "_tag": "Err", "payload": { "code": "SessionCompleted", "message": "ended" } }),
        ));
        assert_eq!(tag(&ended), "Ready");
        let replacement = state.with_session_status(&status(json!({ "_tag": "Ok", "payload": { "active": {
            "gameId": "owned", "launchId": "replacement", "phase": "running", "initialHandoff": "waiting" } } })));
        assert!(
            matches!(&replacement, LaunchablesState::Loaded(Loaded { phase: Phase::Starting { launch_id: Some(id), cancelling: false, .. }, .. }) if id == "replacement")
        );
        assert!(replacement.entries().iter().any(|entry| entry
            .session()
            .is_some_and(|session| session.launch_id == "replacement")));
    }
}

#[test]
fn failed_native_status_retains_the_actual_pending_and_error_payload() {
    let game = json!({ "id": "local", "title": "Local game", "supportsRunnerSelection": false, "source": { "label": "This device", "isLocal": true } });
    let catalog = catalog_of(vec![game]);
    let known = LaunchablesState::from_sources(
        &catalog,
        Some(&status(json!({ "_tag": "Ok", "payload": { "active": {
            "launchId": "A", "gameId": "local", "phase": "frozen", "initialHandoff": "observed" } } }))),
        None,
        &[],
        None,
    );
    let unavailable = status(json!({ "_tag": "Ok", "payload": {
        "pendingLaunches": [{ "session": { "gameId": "local", "launchId": "B" }, "phase": "preparing" }],
        "observationFailure": { "code": "HostRecoveryBlocked", "message": "native observation failed" }
    } }));
    let refreshed =
        LaunchablesState::from_sources(&catalog, Some(&unavailable), None, known.entries(), None);
    assert_eq!(tag(&refreshed), "Ready");
    assert!(content(&refreshed).unavailable_session_status.is_some());
    assert_eq!(
        session_json(&now_playing(&refreshed)),
        session_json(&now_playing(&known))
    );
    let polled = known.clone().with_session_status(&unavailable);
    assert_eq!(tag(&polled), "Ready");
    assert!(content(&polled).unavailable_session_status.is_some());
    assert_eq!(
        session_json(&now_playing(&polled)),
        session_json(&now_playing(&known))
    );
}

mod manual_pending_launch_choices {
    use super::*;

    fn local() -> Value {
        json!({ "id": "wl4", "title": "Local title", "supportsRunnerSelection": false, "source": { "label": "This device", "isLocal": true } })
    }
    fn ready_content() -> Content {
        content(&LaunchablesState::from_sources(
            &catalog_of(vec![local()]),
            None,
            None,
            &[],
            None,
        ))
        .clone()
    }
    fn pending(phase: &str) -> SessionStatusOutcome {
        status(json!({ "_tag": "Ok", "payload": { "pendingLaunches": [
            { "session": { "gameId": "wl4", "launchId": "B" }, "phase": phase },
            { "session": { "gameId": "wl4", "launchId": "A" }, "phase": "preparing" },
        ] } }))
    }
    fn choices(state: &LaunchablesState) -> &Vec<PendingLaunchChoice> {
        match state {
            LaunchablesState::Loaded(Loaded {
                phase: Phase::Choosing { choices },
                ..
            }) => choices,
            _ => panic!("expected Choosing, got {}", tag(state)),
        }
    }

    #[test]
    fn candidate_sorting_and_per_launch_cancel_intent_survive_reordered_facts() {
        use crate::PendingLaunchPhase;
        let first =
            LaunchablesState::recover_startup(ready_content(), Some(&pending("reserved")), &[]);
        assert_eq!(
            choices(&first),
            &vec![
                PendingLaunchChoice {
                    launch_id: "A".into(),
                    game_id: Some("wl4".into()),
                    title: "Local title".into(),
                    phase: ChoicePhase::Pending(PendingLaunchPhase::Preparing),
                    cancel: CancelState::Idle,
                },
                PendingLaunchChoice {
                    launch_id: "B".into(),
                    game_id: Some("wl4".into()),
                    title: "Local title".into(),
                    phase: ChoicePhase::Pending(PendingLaunchPhase::Reserved),
                    cancel: CancelState::Idle,
                },
            ]
        );
        let sent = first
            .begin_pending_cancellation("A")
            .unwrap()
            .begin_pending_cancellation("B")
            .unwrap();
        let failed = sent.with_pending_cancellation_outcome(
            "B",
            &from(json!({ "_tag": "Err", "payload": { "code": "BrainUnreachable", "message": "Exact cancel failed" } })),
        );
        let reversed = status(json!({ "_tag": "Ok", "payload": { "pendingLaunches": [
            { "session": { "gameId": "wl4", "launchId": "A" }, "phase": "preparing" },
            { "session": { "gameId": "wl4", "launchId": "B" }, "phase": "reserved" },
        ] } }));
        let reordered = failed.clone().with_session_status(&reversed);
        assert_eq!(choices(&reordered), choices(&failed));
        assert!(reordered.begin_pending_cancellation("A").is_none());
        assert!(reordered.begin_pending_cancellation("unknown").is_none());
        let cancelling = reordered.with_session_status(&pending("cancelling"));
        assert_eq!(
            choices(&cancelling)
                .iter()
                .map(|choice| (choice.launch_id.as_str(), choice.cancel))
                .collect::<Vec<_>>(),
            vec![("A", CancelState::Sent), ("B", CancelState::Sent)]
        );
        assert!(cancelling.begin_pending_cancellation("B").is_none());
    }

    #[test]
    fn unknown_observation_retains_every_choice_and_positive_absence_retires_them() {
        let choosing =
            LaunchablesState::recover_startup(ready_content(), Some(&pending("reserved")), &[]);
        let failed_status = status(
            json!({ "_tag": "Ok", "payload": { "observationFailure": { "code": "NativeQueryFailed", "message": "Window unknown" } } }),
        );
        let unknown = choosing.clone().with_session_status(&failed_status);
        assert_eq!(choices(&unknown), choices(&choosing));
        assert!(content(&unknown).unavailable_session_status.is_some());
        assert_eq!(
            notice(&unknown),
            Some(&LaunchNotice::Launch {
                message: "Window unknown".into(),
                subject: None
            })
        );
        let retired = unknown.with_session_status(&status(json!({ "_tag": "Ok", "payload": {} })));
        assert_eq!(tag(&retired), "Ready");
        assert!(notice(&retired).is_none());
        assert_eq!(
            tag(&LaunchablesState::recover_startup(
                ready_content(),
                Some(&failed_status),
                &[]
            )),
            "Recovery"
        );
    }

    #[test]
    fn a_sent_surviving_choice_enters_single_cancelling_and_accepts_only_its_exact_late_reply() {
        let sent =
            LaunchablesState::recover_startup(ready_content(), Some(&pending("reserved")), &[])
                .begin_pending_cancellation("A")
                .unwrap();
        let single = sent.with_session_status(&status(
            json!({ "_tag": "Ok", "payload": { "pendingLaunches": [
            { "session": { "gameId": "wl4", "launchId": "A" }, "phase": "preparing" }
        ] } }),
        ));
        assert!(
            matches!(&single, LaunchablesState::Loaded(Loaded { phase: Phase::Starting { launch_id: Some(id), cancelling: true, .. }, .. }) if id == "A")
        );
        let failure: SessionEndOutcome = from(
            json!({ "_tag": "Err", "payload": { "code": "BrainUnreachable", "message": "Exact retry" } }),
        );
        let unchanged = single
            .clone()
            .with_pending_cancellation_outcome("B", &failure);
        assert_eq!(notice(&unchanged), notice(&single));
        let retried = single.with_pending_cancellation_outcome("A", &failure);
        assert!(matches!(
            &retried,
            LaunchablesState::Loaded(Loaded {
                phase: Phase::Starting {
                    cancelling: true,
                    cancel_retry_available: Some(true),
                    ..
                },
                ..
            })
        ));
        assert_eq!(
            notice(&retried).map(LaunchNotice::message),
            Some("Exact retry")
        );
    }

    #[test]
    fn a_waiting_session_without_game_metadata_keeps_it_absent_and_uses_its_real_title() {
        use crate::PendingLaunchPhase;
        let result = LaunchablesState::recover_startup(
            ready_content(),
            Some(&status(json!({ "_tag": "Ok", "payload": {
                "active": { "launchId": "A", "title": "Real live title", "phase": "running", "initialHandoff": "waiting" },
                "pendingLaunches": [{ "session": { "launchId": "B", "gameId": "removed-game" }, "phase": "committing" }]
            } }))),
            &[],
        );
        assert_eq!(
            choices(&result),
            &vec![
                PendingLaunchChoice {
                    launch_id: "A".into(),
                    game_id: None,
                    title: "Real live title".into(),
                    phase: ChoicePhase::Waiting,
                    cancel: CancelState::Idle,
                },
                PendingLaunchChoice {
                    launch_id: "B".into(),
                    game_id: Some("removed-game".into()),
                    title: "removed-game".into(),
                    phase: ChoicePhase::Pending(PendingLaunchPhase::Committing),
                    cancel: CancelState::Idle,
                },
            ]
        );
    }

    #[test]
    fn stopping_with_waiting_history_is_not_a_startup_candidate() {
        let active = json!({ "launchId": "A", "gameId": "wl4", "phase": "stopping", "initialHandoff": "waiting" });
        let initial = Content::default();
        assert_eq!(
            tag(&LaunchablesState::recover_startup(
                initial.clone(),
                Some(&status(
                    json!({ "_tag": "Ok", "payload": { "active": active } })
                )),
                &[]
            )),
            "Ready"
        );
        let pending_a =
            json!([{ "session": { "launchId": "A", "gameId": "wl4" }, "phase": "cancelling" }]);
        assert_eq!(
            tag(&LaunchablesState::recover_startup(
                initial.clone(),
                Some(&status(
                    json!({ "_tag": "Ok", "payload": { "active": active, "pendingLaunches": pending_a } })
                )),
                &[]
            )),
            "Ready"
        );
        let both = json!([
            { "session": { "launchId": "A", "gameId": "wl4" }, "phase": "cancelling" },
            { "session": { "launchId": "B", "gameId": "wl4" }, "phase": "reserved" }
        ]);
        let result = LaunchablesState::recover_startup(
            initial,
            Some(&status(
                json!({ "_tag": "Ok", "payload": { "active": active, "pendingLaunches": both } }),
            )),
            &[],
        );
        assert!(
            matches!(&result, LaunchablesState::Loaded(Loaded { phase: Phase::Starting { launch_id: Some(id), cancelling: false, .. }, .. }) if id == "B")
        );
    }

    #[test]
    fn candidate_exclusions_remain_for_returned_or_handed_off_sessions() {
        for (phase, handoff) in [
            ("frozen", "waiting"),
            ("focus-failed", "waiting"),
            ("running", "observed"),
            ("running", "recovered"),
        ] {
            let result = LaunchablesState::recover_startup(
                ready_content(),
                Some(&status(json!({ "_tag": "Ok", "payload": {
                    "pendingLaunches": [
                        { "session": { "gameId": "wl4", "launchId": "B" }, "phase": "reserved" },
                        { "session": { "gameId": "wl4", "launchId": "A" }, "phase": "preparing" }
                    ],
                    "active": { "launchId": "A", "gameId": "wl4", "phase": phase, "initialHandoff": handoff }
                } }))),
                &[],
            );
            assert!(
                matches!(&result, LaunchablesState::Loaded(Loaded { phase: Phase::Starting { launch_id: Some(id), cancelling: false, .. }, .. }) if id == "B"),
                "{phase} {handoff}"
            );
        }
    }
}

#[test]
fn terminal_resume_returns_to_library_with_only_failure_noticed() {
    for code in ["SessionCompleted", "NoActiveSession", "HostLaunchFailed"] {
        let running = ready().with_session_status(&status(json!({"_tag":"Ok", "payload": {
            "active": {"launchId":"exact", "gameId":"skate3", "host":"aka", "phase":"running"}
        }})));
        let session = running.entries()[0].session().unwrap();
        let resuming = LaunchablesState::begin_return(content(&running).clone(), session);
        let ended = resuming.with_resume_outcome(Some(&crate::RpcFailure {
            code: code.into(),
            message: "far launch ended".into(),
        }));
        assert_eq!(tag(&ended), "Ready");
        assert!(
            ended
                .entries()
                .iter()
                .all(|entry| entry.session().is_none()),
            "{code}"
        );
        if code == "HostLaunchFailed" {
            assert_eq!(
                notice(&ended).unwrap().message(),
                "HostLaunchFailed: far launch ended"
            );
        } else {
            assert!(notice(&ended).is_none(), "{code}");
        }
    }
}

#[test]
fn terminal_far_status_returns_to_library_with_only_failure_noticed() {
    for code in ["SessionCompleted", "NoActiveSession", "HostLaunchFailed"] {
        let running = ready().with_session_status(&status(json!({"_tag":"Ok", "payload": {
            "active": {"launchId":"exact", "gameId":"skate3", "host":"aka", "phase":"running"}
        }})));
        let failure = status(json!({"_tag":"Err", "payload": {
            "code": code, "message": "game unit failed: [redacted]"
        }}));
        let ended = running.with_session_status(&failure);
        assert_eq!(tag(&ended), "Ready", "{code}");
        assert!(
            ended
                .entries()
                .iter()
                .all(|entry| entry.session().is_none()),
            "{code}"
        );
        assert!(
            content(&ended).unavailable_session_status.is_none(),
            "{code}"
        );
        if code == "HostLaunchFailed" {
            assert_eq!(
                notice(&ended).unwrap().message(),
                "HostLaunchFailed: game unit failed: [redacted]"
            );
        } else {
            assert!(notice(&ended).is_none(), "{code}");
        }
        let mut dismissed = content(&ended).clone();
        dismissed.notice = None;
        let polled = LaunchablesState::Loaded(Loaded {
            phase: Phase::Ready,
            content: dismissed,
        })
        .with_session_status(&status(json!({"_tag":"Ok", "payload": {}})));
        assert!(notice(&polled).is_none(), "{code}");
    }
}

#[test]
fn catalog_refresh_keeps_a_terminal_far_failure_as_a_launch_notice() {
    let failure = status(json!({"_tag":"Err", "payload": {
        "code":"HostLaunchFailed", "message":"game unit failed: [redacted]"
    }}));
    let running = ready().with_session_status(&status(json!({"_tag":"Ok", "payload": {
        "active":{"launchId":"exact", "gameId":"skate3", "host":"aka", "phase":"running"}
    }})));
    let loaded =
        LaunchablesState::from_sources(&games_ok(), Some(&failure), None, running.entries(), None);
    assert_eq!(tag(&loaded), "Ready");
    assert!(
        matches!(notice(&loaded), Some(LaunchNotice::Launch { message, .. })
        if message == "HostLaunchFailed: game unit failed: [redacted]")
    );
}

mod terminal_far_review {
    use super::*;

    fn catalog() -> CatalogSnapshotOutcome {
        catalog_of(vec![
            json!({"id":"local", "title":"Local", "host":"this", "supportsRunnerSelection":false,
                "source":{"label":"this", "isLocal":true}}),
            json!({"id":"remote", "title":"Remote", "host":"aka", "supportsRunnerSelection":false,
                "source":source("aka")}),
        ])
    }
    fn running(local: bool) -> LaunchablesState {
        LaunchablesState::from_sources(
            &catalog(),
            Some(&status(json!({"_tag":"Ok", "payload":{
                "active":{"launchId":"exact", "gameId":if local {"local"} else {"remote"},
                    "host":if local {"this"} else {"aka"}, "phase":"running"}
            }}))),
            None,
            &[],
            None,
        )
    }
    fn failed() -> SessionStatusOutcome {
        status(
            json!({"_tag":"Err", "payload":{"code":"HostLaunchFailed", "message":"game unit failed: [redacted]"}}),
        )
    }
    fn timeout() -> SessionStatusOutcome {
        status(json!({"_tag":"Err", "payload":{"code":"StatusTimeout", "message":"timeout"}}))
    }
    #[test]
    fn mixed_catalog_retires_only_the_known_far_failure() {
        let remote = running(false);
        let loaded = LaunchablesState::from_sources(
            &catalog(),
            Some(&failed()),
            None,
            remote.entries(),
            None,
        );
        assert!(
            loaded.is_ready(),
            "a local game must not turn a far failure into Recovery"
        );
        assert!(loaded
            .entries()
            .iter()
            .all(|entry| entry.session().is_none()));
        assert!(content(&loaded).unavailable_session_status.is_none());
        assert_eq!(
            notice(&loaded).unwrap().message(),
            "HostLaunchFailed: game unit failed: [redacted]"
        );
        assert_eq!(tag(&loaded.begin_preparing("Remote", None)), "Preparing");
        let local = running(true);
        let loaded = LaunchablesState::from_sources(
            &catalog(),
            Some(&failed()),
            None,
            local.entries(),
            None,
        );
        assert!(loaded.entries().iter().any(Entry::is_now_playing));
        assert!(content(&loaded).unavailable_session_status.is_some());
        let unknown = LaunchablesState::from_sources(&catalog(), Some(&failed()), None, &[], None);
        assert_eq!(
            tag(&unknown),
            "Recovery",
            "unknown local cleanup must stay guarded"
        );
    }
    #[test]
    fn local_failure_after_timeout_keeps_exact_recovery_identity() {
        let unknown = running(true).with_session_status(&timeout());
        assert!(content(&unknown).unavailable_session_status.is_some());
        let failed = unknown.with_session_status(&failed());
        assert!(failed.entries().iter().any(|entry| entry
            .session()
            .is_some_and(|session| session.launch_id == "exact")));
        assert!(content(&failed).unavailable_session_status.is_some());
        let session = now_playing(&failed).session().unwrap().clone();
        let resumed = LaunchablesState::begin_return(content(&failed).clone(), &session)
            .with_resume_outcome(Some(&crate::RpcFailure {
                code: "HostLaunchFailed".into(),
                message: "unit failed".into(),
            }));
        assert!(resumed.entries().iter().any(Entry::is_now_playing));
        assert!(content(&resumed).unavailable_session_status.is_some());
    }
    #[test]
    fn far_failure_clears_a_stale_observation_fence_for_status_and_resume() {
        let mut loaded = running(false).loaded().unwrap().clone();
        loaded.content.unavailable_session_status = Some(timeout());
        let fenced = LaunchablesState::Loaded(loaded);
        let session = now_playing(&fenced).session().unwrap().clone();
        let resuming = LaunchablesState::begin_return(content(&fenced).clone(), &session);
        let recovering = running(false).with_session_status(&timeout());
        assert_eq!(tag(&recovering), "Recovery");
        for ended in [
            fenced.with_session_status(&failed()),
            recovering.with_session_status(&failed()),
            resuming.with_resume_outcome(Some(&crate::RpcFailure {
                code: "HostLaunchFailed".into(),
                message: "unit failed".into(),
            })),
        ] {
            assert!(ended.is_ready());
            assert!(ended
                .entries()
                .iter()
                .all(|entry| entry.session().is_none()));
            assert!(
                content(&ended).unavailable_session_status.is_none(),
                "terminal far failure must remove the prepare fence"
            );
        }
    }
}
