//! Exercise catalog, runner choice, and launch through the real host RPC router.
use super::{control::InMemoryLaunchUnitBackend, HostRuntime};
use axum::{body::Body, http::Request, Router};
use serde_json::{json, Value};
use std::{fs, path::Path, sync::Arc};
use tower::ServiceExt;

const GAME: &str = crate::plugin_test_fixtures::GBA_ID;
const RUNNER: &str = "@korri:mgba/mgba";

fn library(root: &Path, pair: bool) -> Router {
    crate::plugin_test_fixtures::write_gba_library(root);
    fs::create_dir(root.join("roms")).unwrap();
    fs::write(root.join("roms/wl4.gba"), b"rom").unwrap();
    let registry = if pair {
        crate::plugin_test_fixtures::installed_pair(root)
    } else {
        crate::plugin_test_fixtures::installed(root)
    };
    library_router(root, registry)
}

fn library_router(root: &Path, registry: crate::plugin::PluginRegistry) -> Router {
    let config = root.join("host.toml");
    fs::write(&config, "label = \"rpminiv2\"\n").unwrap();
    let runtime = HostRuntime::from_paths_with_backend(
        &config,
        None,
        root.join("private"),
        Arc::new(InMemoryLaunchUnitBackend::default()),
    )
    .with_route_registry(root.into(), registry);
    runtime.claim_save_owner();
    crate::plain_host_routers_for_tests(runtime).0
}

async fn rpc(app: &Router, method: &str, payload: Value) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::post("/rpc")
                .extension(axum::extract::ConnectInfo(
                    "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"_tag": method, "payload": payload}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice::<Value>(&bytes).unwrap()["outcome"].clone()
}

fn save_runner(root: &Path, runner: &str) {
    let path = root.join("catalog/library.yaml");
    let mut games: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    games["library"][GAME]["runner"] = runner.into();
    fs::write(path, serde_yaml::to_string(&games).unwrap()).unwrap();
}

#[tokio::test]
async fn catalog_keeps_a_game_needing_runner_choice_without_a_startup_failure() {
    let root = tempfile::tempdir().unwrap();
    let app = library(root.path(), true);
    let catalog = rpc(&app, "app.catalog.snapshot", json!({})).await;
    assert_eq!(catalog["_tag"], "Ok", "{catalog}");
    assert_eq!(catalog["payload"]["games"][0]["id"], GAME);
    assert_eq!(
        catalog["payload"]["games"][0]["supportsRunnerSelection"],
        true
    );
    assert!(catalog["payload"]["failures"].is_null(), "{catalog}");
    let routes = rpc(&app, "app.local-games.routes", json!({"gameId": GAME})).await;
    assert_eq!(routes["_tag"], "Ok", "{routes}");
    assert_eq!(routes["payload"]["selection"]["_tag"], "Choose");
    assert_eq!(routes["payload"]["routes"].as_array().unwrap().len(), 2);
    assert!(
        !root.path().join("users").exists(),
        "catalog must not perform launch effects"
    );
}

#[tokio::test]
async fn sole_and_saved_runners_resolve_to_selected_without_catalog_failures() {
    for pair in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let app = library(root.path(), pair);
        if pair {
            save_runner(root.path(), RUNNER);
        }
        let routes = rpc(&app, "app.local-games.routes", json!({"gameId": GAME})).await;
        assert_eq!(routes["_tag"], "Ok", "{routes}");
        assert_eq!(
            routes["payload"]["selection"],
            json!({"_tag": "Selected", "runnerId": RUNNER})
        );
        let catalog = rpc(&app, "app.catalog.snapshot", json!({})).await;
        assert!(catalog["payload"]["failures"].is_null(), "{catalog}");
    }
}

#[tokio::test]
async fn valid_system_preferences_on_multiple_releases_require_an_explicit_choice() {
    const OTHER_RUNNER: &str = "@korri:beetle-gba/beetle-gba";
    const OTHER_RELEASE: &str =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    for chosen_runner in [RUNNER, OTHER_RUNNER] {
        let root = tempfile::tempdir().unwrap();
        crate::plugin_test_fixtures::write_gba_library(root.path());
        fs::create_dir(root.path().join("roms")).unwrap();
        fs::write(root.path().join("roms/wl4.gba"), b"gba rom").unwrap();
        fs::write(root.path().join("roms/wl4.sfc"), b"snes rom").unwrap();
        let pair = crate::plugin_test_fixtures::installed_pair(root.path());
        let packages =
            [RUNNER, OTHER_RUNNER].map(|runner| pair.installed_package(runner).unwrap().clone());
        let other_source = packages[1].package.join("plugin.ts");
        let source = fs::read_to_string(&other_source).unwrap();
        fs::write(
            &other_source,
            source
                .replace("\"gba\"", "\"snes\"")
                .replace("gba-files", "snes-files")
                .replace("Game Boy Advance", "Super Nintendo"),
        )
        .unwrap();
        let path = root.path().join("catalog/library.yaml");
        let mut library: serde_yaml::Value =
            serde_yaml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let mut other = library["library"][GAME]["releases"][0].clone();
        other["id"] = "snes".into();
        other["system"] = "snes".into();
        other["identity"]["value"] = OTHER_RELEASE.into();
        library["library"][GAME]["releases"]
            .as_sequence_mut()
            .unwrap()
            .push(other);
        fs::write(path, serde_yaml::to_string(&library).unwrap()).unwrap();
        let path = root.path().join("device.yaml");
        let mut device: serde_yaml::Value =
            serde_yaml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let mut location = device["locations"]
            .as_mapping()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .clone();
        location[0]["path"] = "wl4.sfc".into();
        device["locations"][OTHER_RELEASE] = location;
        fs::write(path, serde_yaml::to_string(&device).unwrap()).unwrap();
        let registry = crate::plugin::PluginRegistry::from_installed(packages.into()).unwrap();
        let app = library_router(root.path(), registry);
        let idle = rpc(&app, "app.session.status", json!({})).await;
        let initial = rpc(&app, "app.local-games.routes", json!({"gameId": GAME})).await;
        assert_eq!(initial["_tag"], "Ok", "{initial}");
        let mut revisions = initial["payload"]["revisions"].clone();
        for (system, runner) in [("gba", RUNNER), ("snes", OTHER_RUNNER)] {
            let saved = rpc(
                &app,
                "app.launch-settings.set",
                json!({
                    "place": {"layer": {"_tag": "System", "id": system}, "scope": {"_tag": "Any"}},
                    "path": {"_tag": "Runner"},
                    "value": runner,
                    "expectedRevision": revisions["device"],
                }),
            )
            .await;
            assert_eq!(saved["_tag"], "Ok", "{saved}");
            revisions = saved["payload"].clone();
        }
        let device_preferences = fs::read(root.path().join("device.yaml")).unwrap();
        let game_preferences = fs::read(root.path().join("catalog/library.yaml")).unwrap();
        let routes = rpc(&app, "app.local-games.routes", json!({"gameId": GAME})).await;
        assert_eq!(routes["_tag"], "Ok", "{routes}");
        assert_eq!(routes["payload"]["selection"], json!({"_tag": "Choose"}));
        assert!(routes["payload"]["gameRunner"].is_null());
        assert_eq!(
            routes["payload"]["systemRunners"],
            json!({"gba": RUNNER, "snes": OTHER_RUNNER})
        );
        assert_eq!(routes["payload"]["revisions"], revisions);
        assert_eq!(routes["payload"]["routes"].as_array().unwrap().len(), 2);
        for (system, runner) in [("gba", RUNNER), ("snes", OTHER_RUNNER)] {
            assert!(routes["payload"]["routes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|route| route["systemId"] == system && route["runnerId"] == runner));
        }
        let catalog = rpc(&app, "app.catalog.snapshot", json!({})).await;
        assert_eq!(catalog["_tag"], "Ok", "{catalog}");
        assert_eq!(catalog["payload"]["games"][0]["id"], GAME);
        assert!(catalog["payload"]["failures"].is_null(), "{catalog}");
        assert_eq!(rpc(&app, "app.session.status", json!({})).await, idle);
        assert!(!root.path().join("users").exists());
        let ordinary = rpc(&app, "app.session.prepare", json!({"gameId": GAME})).await;
        assert_eq!(ordinary["_tag"], "Err", "{ordinary}");
        assert_eq!(ordinary["payload"]["code"], "LocalRouteUnavailable");
        assert!(ordinary["payload"]["message"]
            .as_str()
            .unwrap()
            .contains("choose a runner"));
        assert_eq!(rpc(&app, "app.session.status", json!({})).await, idle);
        let launched = rpc(
            &app,
            "app.local-games.launch.selected",
            json!({"gameId": GAME, "runnerId": chosen_runner}),
        )
        .await;
        assert_eq!(launched["_tag"], "Ok", "{launched}");
        assert_eq!(launched["payload"]["session"]["gameId"], GAME);
        assert_eq!(
            fs::read(root.path().join("device.yaml")).unwrap(),
            device_preferences
        );
        assert_eq!(
            fs::read(root.path().join("catalog/library.yaml")).unwrap(),
            game_preferences
        );
        let unchanged = rpc(&app, "app.local-games.routes", json!({"gameId": GAME})).await;
        assert_eq!(unchanged, routes);
    }
}

#[tokio::test]
async fn a_missing_saved_runner_is_reported_as_a_notice_while_multiple_routes_still_need_choice() {
    let root = tempfile::tempdir().unwrap();
    let app = library(root.path(), true);
    save_runner(root.path(), "@korri:removed/core");
    let saved_preferences = fs::read(root.path().join("catalog/library.yaml")).unwrap();
    let idle = rpc(&app, "app.session.status", json!({})).await;
    let routes = rpc(&app, "app.local-games.routes", json!({"gameId": GAME})).await;
    assert_eq!(routes["_tag"], "Ok", "{routes}");
    assert_eq!(routes["payload"]["selection"], json!({"_tag": "Choose"}));
    assert_eq!(routes["payload"]["gameRunner"], "@korri:removed/core");
    assert_eq!(routes["payload"]["routes"].as_array().unwrap().len(), 2);
    assert!(routes["payload"]["revisions"]["games"].is_string());
    assert!(routes["payload"]["revisions"]["device"].is_string());
    assert_eq!(rpc(&app, "app.session.status", json!({})).await, idle);
    let catalog = rpc(&app, "app.catalog.snapshot", json!({})).await;
    assert_eq!(catalog["payload"]["games"][0]["id"], GAME);
    assert!(catalog["payload"]["failures"].is_null(), "{catalog}");
    let settings = rpc(
        &app,
        "app.launch-settings.snapshot",
        json!({"gameId": GAME}),
    )
    .await;
    assert_eq!(
        settings["payload"]["notices"][0]["_tag"], "RunnerUnavailable",
        "{settings}"
    );
    assert_eq!(
        settings["payload"]["notices"][0]["payload"]["runnerId"],
        "@korri:removed/core"
    );
    let launch = rpc(&app, "app.session.prepare", json!({"gameId": GAME})).await;
    assert_eq!(launch["_tag"], "Err", "{launch}");
    assert_eq!(launch["payload"]["code"], "LocalRouteUnavailable");
    assert_eq!(rpc(&app, "app.session.status", json!({})).await, idle);
    assert_eq!(
        fs::read(root.path().join("catalog/library.yaml")).unwrap(),
        saved_preferences
    );
}

#[tokio::test]
async fn stale_runner_clear_and_replace_use_current_revisions_without_auto_launch() {
    for (scope, revision_key) in [
        (json!({"_tag": "Game", "id": GAME}), "games"),
        (json!({"_tag": "System", "id": "gba"}), "device"),
    ] {
        for replacement in [None, Some(RUNNER)] {
            let root = tempfile::tempdir().unwrap();
            let app = library(root.path(), true);
            let idle = rpc(&app, "app.session.status", json!({})).await;
            let original = rpc(&app, "app.local-games.routes", json!({"gameId": GAME})).await;
            let stale = rpc(
                &app,
                "app.launch-settings.set",
                json!({
                    "place": {"layer": scope, "scope": {"_tag": "Any"}},
                    "path": {"_tag": "Runner"},
                    "value": "@korri:removed/core",
                    "expectedRevision": original["payload"]["revisions"][revision_key],
                }),
            )
            .await;
            assert_eq!(stale["_tag"], "Ok", "{stale}");
            let routes = rpc(&app, "app.local-games.routes", json!({"gameId": GAME})).await;
            assert_eq!(routes["_tag"], "Ok", "{routes}");
            assert_eq!(routes["payload"]["selection"], json!({"_tag": "Choose"}));
            assert_eq!(routes["payload"]["revisions"], stale["payload"]);
            let saved_id = if revision_key == "games" {
                &routes["payload"]["gameRunner"]
            } else {
                &routes["payload"]["systemRunners"]["gba"]
            };
            assert_eq!(saved_id, "@korri:removed/core");
            let saved = rpc(
                &app,
                "app.launch-settings.set",
                json!({
                    "place": {"layer": scope, "scope": {"_tag": "Any"}},
                    "path": {"_tag": "Runner"},
                    "value": replacement,
                    "expectedRevision": routes["payload"]["revisions"][revision_key],
                }),
            )
            .await;
            assert_eq!(saved["_tag"], "Ok", "{saved}");
            assert_eq!(rpc(&app, "app.session.status", json!({})).await, idle);
            let corrected = rpc(&app, "app.local-games.routes", json!({"gameId": GAME})).await;
            assert_eq!(corrected["_tag"], "Ok", "{corrected}");
            assert_eq!(corrected["payload"]["revisions"], saved["payload"]);
            let expected = replacement.map_or(
                json!({"_tag": "Choose"}),
                |id| json!({"_tag": "Selected", "runnerId": id}),
            );
            assert_eq!(corrected["payload"]["selection"], expected);
            let saved_id = if revision_key == "games" {
                &corrected["payload"]["gameRunner"]
            } else {
                &corrected["payload"]["systemRunners"]["gba"]
            };
            assert_eq!(saved_id, &json!(replacement));
            let catalog = rpc(&app, "app.catalog.snapshot", json!({})).await;
            assert!(catalog["payload"]["failures"].is_null(), "{catalog}");
            assert_eq!(rpc(&app, "app.session.status", json!({})).await, idle);
            let prepared = rpc(&app, "app.session.prepare", json!({"gameId": GAME})).await;
            assert_eq!(
                prepared["_tag"],
                if replacement.is_some() { "Ok" } else { "Err" },
                "{prepared}"
            );
            if replacement.is_none() {
                assert_eq!(prepared["payload"]["code"], "LocalRouteUnavailable");
                assert_eq!(rpc(&app, "app.session.status", json!({})).await, idle);
            }
        }
    }
}

#[tokio::test]
async fn missing_runtime_remains_a_catalog_and_route_failure() {
    let root = tempfile::tempdir().unwrap();
    let app = library(root.path(), false);
    let source_path = root.path().join("plugin-mgba/retroarch.ts");
    let source = fs::read_to_string(&source_path).unwrap();
    fs::write(&source_path, source.replace("export const handlers = {", "export const handlers = {\n  'runtime.resolve': () => ({ ready: false, missing: ['runtime is absent'] }),")).unwrap();
    let catalog = rpc(&app, "app.catalog.snapshot", json!({})).await;
    assert_eq!(
        catalog["payload"]["failures"][0]["code"], "LocalRouteUnavailable",
        "{catalog}"
    );
    assert!(catalog["payload"]["failures"][0]["message"]
        .as_str()
        .unwrap()
        .contains("runtime is absent"));
    save_runner(root.path(), "@korri:removed/core");
    let routes = rpc(&app, "app.local-games.routes", json!({"gameId": GAME})).await;
    assert_eq!(routes["_tag"], "Err", "{routes}");
    assert!(routes["payload"]["message"]
        .as_str()
        .unwrap()
        .contains("runtime is absent"));
}

#[tokio::test]
async fn selected_launch_revalidates_content_after_the_route_read() {
    let root = tempfile::tempdir().unwrap();
    let app = library(root.path(), false);
    let routes = rpc(&app, "app.local-games.routes", json!({"gameId": GAME})).await;
    assert_eq!(routes["_tag"], "Ok", "{routes}");
    fs::remove_file(root.path().join("roms/wl4.gba")).unwrap();
    let launch = rpc(
        &app,
        "app.local-games.launch.selected",
        json!({"gameId": GAME, "runnerId": RUNNER}),
    )
    .await;
    assert_eq!(launch["_tag"], "Err", "{launch}");
    assert_eq!(launch["payload"]["code"], "LocalRomMissing");
    let catalog = rpc(&app, "app.catalog.snapshot", json!({})).await;
    assert_eq!(catalog["payload"]["failures"][0]["code"], "LocalRomMissing");
}

#[tokio::test]
async fn ordinary_prepare_returns_to_the_exact_live_session_after_content_disappears() {
    let root = tempfile::tempdir().unwrap();
    let app = library(root.path(), false);
    let started = rpc(&app, "app.session.prepare", json!({"gameId": GAME})).await;
    assert_eq!(started["_tag"], "Ok", "{started}");
    fs::remove_file(root.path().join("roms/wl4.gba")).unwrap();
    let returned = rpc(&app, "app.session.prepare", json!({"gameId": GAME})).await;
    assert_eq!(returned["_tag"], "Ok", "{returned}");
    assert_eq!(
        returned["payload"]["session"]["launchId"],
        started["payload"]["session"]["launchId"]
    );
}

#[tokio::test]
async fn unowned_device_can_browse_and_reserve_but_native_start_needs_an_owner() {
    async fn rpc(app: &Router, method: &str, payload: Value) -> Value {
        let response = app
            .clone()
            .oneshot(
                Request::post("/rpc")
                    .extension(axum::extract::ConnectInfo(
                        "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
                    ))
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer unowned-test")
                    .header("origin", "http://127.0.0.1:8099")
                    .body(Body::from(
                        json!({"_tag":method,"payload":payload}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice::<Value>(&bytes).unwrap()["outcome"].clone()
    }
    let root = tempfile::tempdir().unwrap();
    crate::plugin_test_fixtures::write_gba_library(root.path());
    fs::create_dir(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let registry = crate::plugin_test_fixtures::installed(root.path());
    let config = root.path().join("host.toml");
    fs::write(&config, "label = \"unowned\"\n").unwrap();
    let backend = Arc::new(InMemoryLaunchUnitBackend::default());
    let runtime = HostRuntime::from_paths_with_backend(
        &config,
        None,
        root.path().join("private"),
        backend.clone(),
    )
    .with_route_registry(root.path().into(), registry);
    assert_eq!(runtime.owner_public_key(), None);
    let app = crate::secure_host_routers(
        runtime,
        &root.path().join("private"),
        Some(crate::PortalAccess::new(
            "unowned-test",
            "http://127.0.0.1:8099",
            crate::PortalPermission::LocalSessions,
        )),
    )
    .0;
    let catalog = rpc(&app, "app.catalog.snapshot", json!({})).await;
    assert_eq!(catalog["_tag"], "Ok", "{catalog}");
    let routes = rpc(&app, "app.local-games.routes", json!({"gameId":GAME})).await;
    assert_eq!(routes["_tag"], "Ok", "{routes}");
    let reserved = rpc(&app, "app.session.reserve", json!({"gameId":GAME})).await;
    assert_eq!(reserved["_tag"], "Ok", "{reserved}");
    let started = rpc(
        &app,
        "app.session.start",
        json!({"gameId":GAME,"expectedLaunchId":reserved["payload"]["launchId"]}),
    )
    .await;
    assert_eq!(started["payload"]["code"], "DeviceNeedsOwner", "{started}");
    assert_eq!(
        started["payload"]["message"],
        "This device needs an owner before starting a game."
    );
    assert_eq!(backend.launch_count(), 0);
    assert!(!root.path().join("users").exists());
}

#[tokio::test]
async fn launch_settings_snapshot_explains_missing_route_layers_and_resolves_only_present_layers() {
    for choose in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let app = library(root.path(), choose);
        let path = root.path().join("catalog/library.yaml");
        let mut value: serde_yaml::Value =
            serde_yaml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        value["library"][GAME]["cpu"] = serde_yaml::from_str("{max: 75}").unwrap();
        value["library"][GAME]["profiles"] =
            serde_yaml::from_str("{quiet: {title: Quiet, cpu: {max: 40}}}").unwrap();
        value["library"][GAME]["profile"] = "quiet".into();
        value["library"][GAME]["releases"][0]["gpu"] = serde_yaml::from_str("{max: 66}").unwrap();
        fs::write(path, serde_yaml::to_string(&value).unwrap()).unwrap();
        let snap = rpc(&app, "app.launch-settings.snapshot", json!({"gameId":GAME})).await;
        assert_eq!(snap["_tag"], "Ok", "{snap}");
        let payload = &snap["payload"];
        assert_eq!(payload["resolved"]["values"]["cpu.max"]["value"], 40);
        assert_eq!(
            payload["resolved"]["values"]["cpu.max"]["path"],
            json!({"_tag":"Editable","payload":{"_tag":"CpuMax"}})
        );
        assert_eq!(
            payload["resolved"]["values"]["cpu.max"]["source"]["payload"]["layer"],
            json!({"_tag":"Profile","id":{"gameId":GAME,"profileId":"quiet"}})
        );
        assert_eq!(payload["profiles"][0]["id"], "quiet");
        assert_eq!(payload["selectedProfile"], "quiet");
        if choose {
            assert_eq!(payload["routeLayers"]["_tag"], "NeedsRunnerChoice");
            assert!(payload["places"]
                .as_array()
                .unwrap()
                .iter()
                .any(|place| place["place"]["layer"]["_tag"] == "Game"));
            assert_eq!(payload["resolved"]["values"]["gpu.max"]["value"], 66);
        } else {
            assert_eq!(payload["routeLayers"]["_tag"], "Present");
            assert!(payload["places"]
                .as_array()
                .unwrap()
                .iter()
                .any(|place| place["place"]["scope"]["_tag"] == "Runner"));
            assert_eq!(payload["resolved"]["values"]["gpu.max"]["value"], 66);
        }
        fs::remove_file(root.path().join("roms/wl4.gba")).unwrap();
        let snap = rpc(&app, "app.launch-settings.snapshot", json!({"gameId":GAME})).await;
        assert_eq!(snap["_tag"], "Ok", "{snap}");
        assert_eq!(snap["payload"]["routeLayers"]["_tag"], "Unavailable");
        assert!(snap["payload"]["routeLayers"]["message"].is_string());
        assert!(snap["payload"]["places"]
            .as_array()
            .unwrap()
            .iter()
            .any(|place| place["place"]["layer"]["_tag"] == "Profile"));
        assert_eq!(
            snap["payload"]["resolved"]["values"]["cpu.max"]["value"],
            40
        );
    }
}

#[tokio::test]
async fn system_places_without_game_can_be_inspected_and_written_through_rpc() {
    let root = tempfile::tempdir().unwrap();
    let app = library(root.path(), true);
    let before = rpc(&app, "app.launch-settings.snapshot", json!({})).await;
    assert_eq!(before["_tag"], "Ok", "{before}");
    let place = json!({"layer":{"_tag":"System","id":"gba"},"scope":{"_tag":"Any"}});
    let systems: Vec<_> = before["payload"]["places"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["place"] == place)
        .collect();
    assert_eq!(systems.len(), 1, "{before}");
    let library_before = fs::read(root.path().join("catalog/library.yaml")).unwrap();
    let written = rpc(
        &app,
        "app.launch-settings.set",
        json!({
            "place":place,
            "path":{"_tag":"CpuMax"},
            "value":70,
            "expectedRevision":before["payload"]["revisions"]["device"]
        }),
    )
    .await;
    assert_eq!(written["_tag"], "Ok", "{written}");
    let after = rpc(&app, "app.launch-settings.snapshot", json!({})).await;
    assert_eq!(after["_tag"], "Ok", "{after}");
    let system = after["payload"]["places"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["place"] == place)
        .unwrap();
    assert_eq!(system["body"]["cpu"]["max"], 70);
    assert_eq!(
        before["payload"]["resolved"], after["payload"]["resolved"],
        "System inspection must not select a game or change Device resolution"
    );
    assert_eq!(
        fs::read(root.path().join("catalog/library.yaml")).unwrap(),
        library_before
    );
}
