use super::*;
use crate::{
    config::{
        resolver::resolve_launchable_routes, snapshot::ConfigSnapshotCoordinator, ConfigSnapshot,
    },
    discovery::{DiscoveryCoordinator, DiscoveryOptions},
    plugin::PluginRegistry,
};
use std::{fs, path::PathBuf};

struct DiscoveredGame {
    root: tempfile::TempDir,
    folder: tempfile::TempDir,
    snapshot: ConfigSnapshot,
    route: ResolvedRoute,
    registry: PluginRegistry,
}

impl DiscoveredGame {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        // Use the same GBA input as the existing Linux RetroArch route tests.
        fs::write(folder.path().join("wl4.gba"), b"rom").unwrap();
        let report = DiscoveryCoordinator::for_tests(root.path(), private.path())
            .pair(
                folder.path().to_path_buf(),
                "gba",
                &DiscoveryOptions {
                    first_seen_at: "2026-08-05T00:00:00Z".into(),
                    ..DiscoveryOptions::default()
                },
            )
            .unwrap();
        assert_eq!(report.added_games, 1);
        let state = ConfigSnapshotCoordinator::new(root.path()).reload();
        assert!(state.diagnostic.is_none(), "{:?}", state.diagnostic);
        let snapshot = (*state.snapshot).clone();
        let registry = crate::plugin_test_fixtures::installed(root.path());
        let mut catalog =
            resolve_launchable_routes(root.path(), &snapshot, &registry, std::iter::empty());
        assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
        assert_eq!(catalog.routes.len(), 1);
        let route = catalog.routes.pop().unwrap();
        assert_eq!(
            route.file_target.as_ref().unwrap().storage_id,
            report.storage_id.unwrap()
        );
        assert_ne!(route.file_target.as_ref().unwrap().storage_id, "roms");

        let executable = root.path().join("retroarch");
        let core = root.path().join("mgba.so");
        let autoconfig = root.path().join("autoconfig");
        fs::write(&executable, b"binary").unwrap();
        fs::write(&core, b"core").unwrap();
        fs::create_dir(&autoconfig).unwrap();
        Self {
            root,
            folder,
            snapshot,
            route,
            registry,
        }
    }

    fn rom(&self) -> PathBuf {
        self.folder
            .path()
            .join(&self.route.file_target.as_ref().unwrap().path)
    }

    fn launch(&self) -> Result<LinuxLaunchSpec, LaunchError> {
        launch_route(
            self.root.path(),
            &self.snapshot,
            &self.registry,
            &self.route,
            None,
            None,
            "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
        )
    }

    fn assert_unavailable(&self) {
        assert!(matches!(
            self.launch(),
            Err(LaunchError::RouteUnavailable(_))
        ));
        assert!(!self.root.path().join("users").exists());
    }
}

#[test]
fn discovery_registered_gba_launches_from_its_snapshot_storage_root() {
    let game = DiscoveredGame::new();
    let launch = game.launch().unwrap();
    assert_eq!(
        serde_json::from_str::<PluginLaunchInput>(&launch.command[3])
            .unwrap()
            .content_path,
        Some(game.rom().canonicalize().unwrap().display().to_string())
    );
    let wire: serde_json::Value = serde_json::from_str(&launch.command[3]).unwrap();
    assert!(
        wire.get("ref").is_none(),
        "file launch keeps its existing wire shape"
    );
    assert!(game.route.provider_target.is_none());
    assert!(
        !game.root.path().join("users").exists(),
        "no daemon-owned runtime artifacts"
    );
}

#[test]
fn configured_target_missing_after_snapshot_is_rom_missing() {
    let game = DiscoveredGame::new();
    fs::remove_file(game.rom()).unwrap();
    assert!(matches!(game.launch(), Err(LaunchError::RomMissing(_))));
    assert!(!game.root.path().join("users").exists());
}

#[test]
fn configured_target_replaced_by_directory_is_unavailable() {
    let game = DiscoveredGame::new();
    fs::remove_file(game.rom()).unwrap();
    fs::create_dir(game.rom()).unwrap();
    game.assert_unavailable();
}

#[test]
fn configured_targets_reject_traversal_absolute_and_empty_paths() {
    let mut game = DiscoveredGame::new();
    let absolute = game.rom().display().to_string();
    for path in ["../wl4.gba", absolute.as_str(), ""] {
        game.route.file_target.as_mut().unwrap().path = path.into();
        game.assert_unavailable();
    }
}

#[cfg(unix)]
#[test]
fn configured_target_replaced_by_escaping_symlink_is_unavailable() {
    let game = DiscoveredGame::new();
    let outside = game.root.path().join("outside.gba");
    fs::write(&outside, b"outside").unwrap();
    fs::remove_file(game.rom()).unwrap();
    std::os::unix::fs::symlink(outside, game.rom()).unwrap();
    game.assert_unavailable();
}

#[cfg(unix)]
#[test]
fn configured_root_and_internal_target_symlinks_use_the_canonical_file() {
    let mut game = DiscoveredGame::new();
    let canonical = game.folder.path().join("canonical.gba");
    fs::rename(game.rom(), &canonical).unwrap();
    std::os::unix::fs::symlink(&canonical, game.rom()).unwrap();
    let root_alias = game.root.path().join("selected-folder");
    std::os::unix::fs::symlink(game.folder.path(), &root_alias).unwrap();
    let storage_id = &game.route.file_target.as_ref().unwrap().storage_id;
    game.snapshot.storage.get_mut(storage_id).unwrap().root.0 = root_alias.display().to_string();
    assert_eq!(
        serde_json::from_str::<PluginLaunchInput>(&game.launch().unwrap().command[3])
            .unwrap()
            .content_path,
        Some(canonical.canonicalize().unwrap().display().to_string())
    );
}

#[cfg(unix)]
#[test]
fn implicit_roms_root_cannot_escape_korri_through_a_symlink() {
    let mut game = DiscoveredGame::new();
    std::os::unix::fs::symlink(game.folder.path(), game.root.path().join("roms")).unwrap();
    game.route.file_target.as_mut().unwrap().storage_id = "roms".into();
    game.assert_unavailable();
}

#[test]
fn launch_preparation_takes_the_display_fact_as_plain_input() {
    let game = DiscoveredGame::new();
    let display = PluginDisplay {
        width: 1240,
        height: 1080,
    };
    let launch = launch_route(
        game.root.path(),
        &game.snapshot,
        &game.registry,
        &game.route,
        None,
        Some(display.clone()),
        "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
    )
    .unwrap();
    let input: LaunchComposition = serde_json::from_str(&launch.command[4]).unwrap();
    assert_eq!(input.display, Some(display));
    assert!(
        serde_json::from_str::<LaunchComposition>(&game.launch().unwrap().command[4])
            .unwrap()
            .display
            .is_none()
    );
}

#[test]
fn launch_scopes_account_and_save_root_to_person_and_game() {
    let game = DiscoveredGame::new();
    let spec = game.launch().unwrap();
    let input: PluginLaunchInput = serde_json::from_str(&spec.command[3]).unwrap();
    assert_eq!(
        input.account_root,
        game.root
            .path()
            .join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9")
            .display()
            .to_string()
    );
    assert_eq!(
        input.save_root,
        std::path::Path::new(&input.account_root)
            .join("saves")
            .join(crate::host::play_log::encode_uri_component(
                &game.route.playable_id
            ))
            .display()
            .to_string()
    );
}

#[test]
fn prepared_plan_environment_never_reads_the_korrid_process_environment() {
    let sentinel = "KORRI_TEST_PRIVATE_RPC_CAPABILITY_SENTINEL";
    std::env::set_var(sentinel, "never-forward-this");
    let root = tempfile::tempdir().unwrap();
    let input = serde_json::json!({"runnerId":"@korri:test/runner","program":"/bin/true","accountRoot":root.path(),"saveRoot":root.path(),"cacheRoot":root.path(),"files":{},"artifacts":{}});
    let prepared = serde_json::json!({"plan":{"command":"/bin/true","args":[],"env":{"GAME":"declared"},"envUnset":["REMOVED"]},"runnerArgv":["/bin/true"]});
    let command = vec![
        "korrid".into(),
        "plugin-launch".into(),
        "unused.ts".into(),
        input.to_string(),
        "{\"modifiers\":[]}".into(),
        prepared.to_string(),
    ];
    let plan_env = resolved_plan_environment_changes(&command).unwrap();
    std::env::remove_var(sentinel);
    assert_eq!(
        plan_env.values,
        std::collections::BTreeMap::from([("GAME".into(), "declared".into())])
    );
    assert!(!plan_env.values.contains_key(sentinel));
    assert!(!plan_env.values.contains_key("REMOVED"));
}

#[test]
fn snapshot_dry_run_and_schema_defaults_do_not_change_launch_input_and_set_validates_schema() {
    use crate::launch_settings::{
        self as ls, LaunchLayer, LaunchPath, LaunchPlace, LaunchRunnerFacts, LaunchRunnerScope,
        LaunchSetRequest, LaunchSource,
    };
    let game = DiscoveredGame::new();
    let package = game
        .registry
        .installed_package(&game.route.runner_id)
        .unwrap();
    let source=include_str!("../../examples/libretro-core.plugin.ts").replace("export { handlers } from \"./retroarch\"",r#"export const handlers = {
      'settings.describe': () => ({revision:'schema-rev-1',schema:{properties:{scale:{type:'integer',minimum:1,maximum:4,default:2,title:'Scale',description:'Pixel scale'}}}}),
      'launch.prepare': input => ({command:input.program,args:[input.overrides?.settings.scale === undefined ? 'default-not-passed' : 'authored'],env:{PLUGIN:'default',REMOVE:'plugin'},cwd:'/plugin'})
    }"#);
    fs::write(package.package.join("plugin.ts"), source).unwrap();
    let device_path = game.root.path().join("device.yaml");
    let mut device: serde_json::Value =
        serde_yaml::from_str(&fs::read_to_string(&device_path).unwrap()).unwrap();
    device["host"] = serde_json::json!({"env":{"LAYER":"authored","REMOVE":null},"args":{"prepend":["before"],"append":["after"]},"cwd":"/layer"});
    fs::write(&device_path, serde_yaml::to_string(&device).unwrap()).unwrap();
    let person = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
    let client = ls::helper::Client {
        socket: game.root.path().join("absent.sock"),
    };
    let snapshot = ls::resolution::snapshot_for_person(
        game.root.path(),
        Some(&game.registry),
        Some(&game.route.playable_id),
        &client,
        Some(person),
    )
    .unwrap();
    assert_eq!(snapshot.runners.len(), 1);
    assert_eq!(snapshot.runners[0].revision, "schema-rev-1");
    let LaunchRunnerFacts::Ready(facts) = &snapshot.runners[0].prepare else {
        panic!("{:?}", snapshot.runners[0].prepare)
    };
    assert_eq!(facts.args, ["default-not-passed"]);
    assert_eq!(facts.env["REMOVE"], "plugin");
    assert!(!facts.env.contains_key("LAYER"));
    assert_eq!(facts.cwd.as_deref(), Some("/plugin"));
    assert_eq!(snapshot.resolved.values["settings.scale"].value, 2);
    assert!(matches!(
        snapshot.resolved.values["settings.scale"].source,
        Some(LaunchSource::PluginDefault {ref plugin_id}) if plugin_id=="@korri:mgba"
    ));
    assert_eq!(
        snapshot.resolved.values["env.REMOVE"].value,
        serde_json::Value::Null
    );
    let config = crate::config::settings::runner_choice_snapshot(game.root.path())
        .unwrap()
        .0;
    let spec = launch_route(
        game.root.path(),
        &config,
        &game.registry,
        &game.route,
        None,
        None,
        person,
    )
    .unwrap();
    let input: PluginLaunchInput = serde_json::from_str(&spec.command[3]).unwrap();
    assert!(!input.overrides.unwrap().settings.contains_key("scale"));
    let plan = crate::launcher::plugin_launch::prepared_plan(&spec.command).unwrap();
    assert_eq!(plan.args, ["before", "default-not-passed", "after"]);
    assert_eq!(plan.cwd.as_deref(), Some("/layer"));
    assert_eq!(plan.env["PLUGIN"], "default");
    assert_eq!(plan.env["LAYER"], "authored");
    assert!(!plan.env.contains_key("REMOVE"));
    assert!(plan.env_unset.iter().any(|v| v == "REMOVE"));
    let plan_env = resolved_plan_environment_changes(&spec.command).unwrap();
    assert_eq!(plan_env.values, plan.env.into_iter().collect());
    assert_eq!(plan_env.unset, plan.env_unset);
    let private = tempfile::tempdir().unwrap();
    let lock = std::sync::Mutex::new(());
    let mut request = LaunchSetRequest {
        clear: false,
        place: LaunchPlace {
            layer: LaunchLayer::Game(game.route.playable_id.clone()),
            scope: LaunchRunnerScope::Family("@korri:retroarch".into()),
        },
        path: LaunchPath::Setting {
            key: "scale".into(),
        },
        value: Some(serde_json::json!(9)),
        expected_revision: snapshot.revisions.games,
    };
    assert!(ls::set_with_registry(
        game.root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&game.registry)
    )
    .is_err());
    request.value = Some(serde_json::json!(3));
    ls::set_with_registry(
        game.root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&game.registry),
    )
    .unwrap();
}

#[test]
fn nested_settings_store_clear_remove_defaults_and_warn_skip_use_real_plugin_operations() {
    use crate::launch_settings::{
        self as ls, LaunchLayer, LaunchNotice, LaunchPath, LaunchPlace, LaunchRunnerScope,
        LaunchSetRequest, LaunchSource,
    };
    use serde_json::json;
    let game = DiscoveredGame::new();
    let package = game
        .registry
        .installed_package(&game.route.runner_id)
        .unwrap();
    let source = include_str!("../../examples/libretro-core.plugin.ts").replace("export { handlers } from \"./retroarch\"", r#"export const handlers = {
        'settings.describe': input => { if (input.id !== '@korri:mgba/mgba') throw Error('id expected'); return {revision:'nested-v1',schema:{type:'object',additionalProperties:false,properties:{video:{type:'object',additionalProperties:false,properties:{fps:{type:'integer',default:60},codec:{type:'string',default:'h264'},'no-default':{type:'boolean'}}}}}}; },
        'settings.validate': input => { if(input.revision !== 'nested-v1') throw Error('revision expected'); return {valid: input.values.video?.fps !== 999,diagnostics:input.values.video?.fps===999 ? [{code:'bad-fps',severity:'error',path:['video','fps'],message:'fps is not supported'}] : []}; },
        'launch.prepare': input => ({command:input.program,args:[JSON.stringify(input.overrides.settings)],env:{}})
    }"#);
    fs::write(package.package.join("plugin.ts"), source).unwrap();
    let path = game.root.path().join("device.yaml");
    let mut device: serde_json::Value =
        serde_yaml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    device["host"] = json!({"families":{"@korri:retroarch":{"settings":{"video":{"fps":30,"codec":"h265","no-default":true}}}}});
    fs::write(&path, serde_yaml::to_string(&device).unwrap()).unwrap();
    device["host"]["families"]["@korri:retroarch"]["settings"]["video"]["array-leaf"] =
        json!([1, 2]);
    fs::write(&path, serde_yaml::to_string(&device).unwrap()).unwrap();
    let array_config = crate::config::settings::runner_choice_snapshot(game.root.path())
        .unwrap()
        .0;
    let array_launch = launch_route(
        game.root.path(),
        &array_config,
        &game.registry,
        &game.route,
        None,
        None,
        "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
    )
    .unwrap();
    assert!(array_launch
        .warnings
        .iter()
        .any(|w| w.setting == "video.array-leaf"));
    let array_plan = crate::launcher::plugin_launch::prepared_plan(&array_launch.command).unwrap();
    assert!(
        serde_json::from_str::<serde_json::Value>(&array_plan.args[0]).unwrap()["video"]
            .get("array-leaf")
            .is_none()
    );
    let private = tempfile::tempdir().unwrap();
    let lock = std::sync::Mutex::new(());
    let client = ls::helper::Client {
        socket: game.root.path().join("absent.sock"),
    };
    let mut request = LaunchSetRequest {
        clear: false,
        place: LaunchPlace {
            layer: LaunchLayer::Game(game.route.playable_id.clone()),
            scope: LaunchRunnerScope::Runner(game.route.runner_id.clone()),
        },
        path: LaunchPath::Setting {
            key: "video.fps".into(),
        },
        value: Some(json!(45)),
        expected_revision: crate::config::settings::runner_choice_revisions(game.root.path())
            .unwrap()
            .games,
    };
    let write = |request: &mut LaunchSetRequest| {
        let revisions = ls::set_with_registry(
            game.root.path(),
            private.path(),
            &lock,
            request,
            &client,
            Some(&game.registry),
        )
        .unwrap();
        request.expected_revision = revisions.games;
    };
    write(&mut request);
    let document: serde_json::Value = serde_yaml::from_str(
        &fs::read_to_string(game.root.path().join("catalog/library.yaml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        document["library"][&game.route.playable_id]["runners"][&game.route.runner_id]["settings"]
            ["video"]["fps"],
        45
    );
    assert!(
        document["library"][&game.route.playable_id]["runners"][&game.route.runner_id]["settings"]
            .get("video.fps")
            .is_none()
    );
    request.clear = true;
    request.value = None;
    write(&mut request);
    let snapshot = ls::snapshot(
        game.root.path(),
        Some(&game.registry),
        Some(&game.route.playable_id),
        &client,
    )
    .unwrap();
    assert!(snapshot.notices.iter().any(|n|matches!(n,LaunchNotice::SettingSkipped {path:LaunchPath::Setting {key},runner_id:Some(id),..} if key=="video.array-leaf" && id==&game.route.runner_id)));
    assert_eq!(snapshot.resolved.values["settings.video.fps"].value, 60);
    assert!(matches!(
        snapshot.resolved.values["settings.video.fps"].source,
        Some(LaunchSource::PluginDefault { .. })
    ));
    request.path = LaunchPath::Setting {
        key: "video.no-default".into(),
    };
    write(&mut request);
    let snapshot = ls::snapshot(
        game.root.path(),
        Some(&game.registry),
        Some(&game.route.playable_id),
        &client,
    )
    .unwrap();
    assert!(!snapshot
        .resolved
        .values
        .contains_key("settings.video.no-default"));
    request.path = LaunchPath::Setting {
        key: "video.fps".into(),
    };
    request.clear = false;
    write(&mut request);
    let snapshot = ls::snapshot(
        game.root.path(),
        Some(&game.registry),
        Some(&game.route.playable_id),
        &client,
    )
    .unwrap();
    assert_eq!(snapshot.resolved.values["settings.video.fps"].value, 30);
    request.value = Some(json!(999));
    write(&mut request);
    let snapshot = ls::snapshot(
        game.root.path(),
        Some(&game.registry),
        Some(&game.route.playable_id),
        &client,
    )
    .unwrap();
    assert!(snapshot.notices.iter().any(|n|matches!(n,LaunchNotice::SettingSkipped {path:LaunchPath::Setting {key},message,runner_id:Some(id)} if key=="video.fps" && message=="fps is not supported" && id==&game.route.runner_id)));
    let config = crate::config::settings::runner_choice_snapshot(game.root.path())
        .unwrap()
        .0;
    let spec = launch_route(
        game.root.path(),
        &config,
        &game.registry,
        &game.route,
        None,
        None,
        "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
    )
    .unwrap();
    assert!(spec.warnings.iter().any(|w| w.setting == "video.fps"));
    let plan = crate::launcher::plugin_launch::prepared_plan(&spec.command).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&plan.args[0]).unwrap(),
        json!({"video":{"codec":"h265"}})
    );
    request.clear = true;
    assert!(ls::set_with_registry(
        game.root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&game.registry)
    )
    .is_err());
    request.value = None;
    request.path = LaunchPath::Setting {
        key: "video".into(),
    };
    assert!(ls::set_with_registry(
        game.root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&game.registry)
    )
    .is_err());
    request.clear = false;
    request.path = LaunchPath::Setting {
        key: "video.fps".into(),
    };
    request.value = Some(json!([1, 2]));
    assert!(ls::set_with_registry(
        game.root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&game.registry)
    )
    .is_err());
}

#[test]
fn retroarch_pattern_port_settings_set_clear_and_report_concrete_validation_paths() {
    use crate::launch_settings::{
        self as ls, LaunchLayer, LaunchNotice, LaunchPath, LaunchPlace, LaunchRunnerScope,
        LaunchSetRequest,
    };
    use serde_json::json;
    let game = DiscoveredGame::new();
    let package = game
        .registry
        .installed_package(&game.route.runner_id)
        .unwrap();
    // Exact ports shape and leaf constraints from RetroArch's policy.ts:
    // ports(section({...})), with the numeric-key pattern owned by that plugin.
    let schema = json!({"type":"object","additionalProperties":false,"properties":{"input":{"type":"object","additionalProperties":false,"properties":{"ports":{"type":"object","additionalProperties":false,"patternProperties":{"^[1-9][0-9]*$":{"type":"object","additionalProperties":false,"properties":{"libretro-device":{"type":"integer","minimum":0,"maximum":9007199254740991_u64},"joypad-index":{"type":"integer","minimum":0,"maximum":9007199254740991_u64},"analog-dpad-mode":{"type":"string","enum":["none","left-analog","right-analog","left-analog-forced","right-analog-forced"]}}}}}}}}});
    let handlers = format!(
        r#"export const handlers = {{
      'settings.describe':()=>({{schema:{schema},revision:'ports-v1'}}),
      'settings.validate':input=>({{valid:true,diagnostics:input.values.input?.ports?.['2']?.['joypad-index']===-1?[{{code:'port-index',severity:'error',path:['input','ports','2','joypad-index'],message:'invalid joypad index'}}]:[]}}),
      'launch.prepare':input=>({{command:input.program,args:[JSON.stringify(input.overrides.settings)],env:{{}}}})
    }}"#
    );
    let source = include_str!("../../examples/libretro-core.plugin.ts")
        .replace("export { handlers } from \"./retroarch\"", &handlers);
    fs::write(package.package.join("plugin.ts"), source).unwrap();
    let private = tempfile::tempdir().unwrap();
    let lock = std::sync::Mutex::new(());
    let client = ls::helper::Client {
        socket: game.root.path().join("absent.sock"),
    };
    let mut request = LaunchSetRequest {
        clear: false,
        place: LaunchPlace {
            layer: LaunchLayer::Game(game.route.playable_id.clone()),
            scope: LaunchRunnerScope::Runner(game.route.runner_id.clone()),
        },
        path: LaunchPath::Setting { key: String::new() },
        value: None,
        expected_revision: crate::config::settings::runner_choice_revisions(game.root.path())
            .unwrap()
            .games,
    };
    for scope in [
        LaunchRunnerScope::Runner(game.route.runner_id.clone()),
        LaunchRunnerScope::Family("@korri:retroarch".into()),
    ] {
        request.place.scope = scope;
        for (port, leaf, value) in [
            ("1", "libretro-device", json!(1)),
            ("2", "joypad-index", json!(0)),
            ("12", "analog-dpad-mode", json!("left-analog")),
        ] {
            let key = format!("input.ports.{port}.{leaf}");
            request.path = LaunchPath::Setting { key };
            request.clear = false;
            request.value = Some(value.clone());
            request.expected_revision = ls::set_with_registry(
                game.root.path(),
                private.path(),
                &lock,
                &request,
                &client,
                Some(&game.registry),
            )
            .unwrap()
            .games;
            let document: serde_json::Value = serde_yaml::from_str(
                &fs::read_to_string(game.root.path().join("catalog/library.yaml")).unwrap(),
            )
            .unwrap();
            let (kind, id) = match &request.place.scope {
                LaunchRunnerScope::Runner(id) => ("runners", id),
                LaunchRunnerScope::Family(id) => ("families", id),
                _ => unreachable!(),
            };
            assert_eq!(
                document["library"][&game.route.playable_id][kind][id]["settings"]["input"]
                    ["ports"][port][leaf],
                value
            );
            request.clear = true;
            request.value = None;
            request.expected_revision = ls::set_with_registry(
                game.root.path(),
                private.path(),
                &lock,
                &request,
                &client,
                Some(&game.registry),
            )
            .unwrap()
            .games;
            let document: serde_json::Value = serde_yaml::from_str(
                &fs::read_to_string(game.root.path().join("catalog/library.yaml")).unwrap(),
            )
            .unwrap();
            assert!(
                document["library"][&game.route.playable_id][kind][id]["settings"]["input"]
                    ["ports"][port][leaf]
                    .is_null()
            );
            request.clear = false;
            request.expected_revision = ls::set_with_registry(
                game.root.path(),
                private.path(),
                &lock,
                &request,
                &client,
                Some(&game.registry),
            )
            .unwrap()
            .games;
        }
    }
    request.clear = false;
    for key in [
        "input.ports.0.joypad-index",
        "input.ports.invalid.joypad-index",
        "input.ports.2.unknown",
    ] {
        request.path = LaunchPath::Setting { key: key.into() };
        request.value = Some(json!(0));
        assert!(
            ls::set_with_registry(
                game.root.path(),
                private.path(),
                &lock,
                &request,
                &client,
                Some(&game.registry)
            )
            .is_err(),
            "{key}"
        );
    }
    // An externally authored bad value still skips only the concrete port leaf.
    let path = game.root.path().join("device.yaml");
    let mut device: serde_json::Value =
        serde_yaml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    device["host"] = json!({"runners":{game.route.runner_id.clone():{"settings":{"input":{"ports":{"2":{"joypad-index":-1,"libretro-device":1}}}}}}});
    fs::write(&path, serde_yaml::to_string(&device).unwrap()).unwrap();
    let snapshot = ls::snapshot(
        game.root.path(),
        Some(&game.registry),
        Some(&game.route.playable_id),
        &client,
    )
    .unwrap();
    assert!(snapshot.notices.iter().any(|notice|matches!(notice,LaunchNotice::SettingSkipped {path:LaunchPath::Setting {key},runner_id:Some(id),..} if key=="input.ports.2.joypad-index" && id==&game.route.runner_id)));
    assert_eq!(
        snapshot.resolved.values["settings.input.ports.2.libretro-device"].value,
        1
    );
    assert!(!snapshot
        .resolved
        .values
        .contains_key("settings.input.ports.2.joypad-index"));
    // A pattern default applies only after an authored key instantiates the
    // pattern, even if that key's only authored leaf is cleared.
    let mut default_schema = schema.clone();
    default_schema["properties"]["input"]["properties"]["ports"]["patternProperties"]
        ["^[1-9][0-9]*$"]["properties"]["joypad-index"]["default"] = json!(0);
    let source = fs::read_to_string(package.package.join("plugin.ts"))
        .unwrap()
        .replace(&schema.to_string(), &default_schema.to_string());
    fs::write(package.package.join("plugin.ts"), source).unwrap();
    request.place.scope = LaunchRunnerScope::Runner(game.route.runner_id.clone());
    request.path = LaunchPath::Setting {
        key: "input.ports.3.joypad-index".into(),
    };
    request.clear = true;
    request.value = None;
    ls::set_with_registry(
        game.root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&game.registry),
    )
    .unwrap();
    let snapshot = ls::snapshot(
        game.root.path(),
        Some(&game.registry),
        Some(&game.route.playable_id),
        &client,
    )
    .unwrap();
    let value = &snapshot.resolved.values["settings.input.ports.3.joypad-index"];
    assert_eq!(value.value, 0);
    assert!(matches!(
        value.source,
        Some(ls::LaunchSource::PluginDefault { .. })
    ));
    assert!(!snapshot
        .resolved
        .values
        .contains_key("settings.input.ports.4.joypad-index"));
}

#[test]
fn default_launch_refuses_authored_settings_before_validation_can_discard_them() {
    let mut game = DiscoveredGame::new();
    let package = game
        .registry
        .installed_package(&game.route.runner_id)
        .unwrap();
    let source = include_str!("../../examples/libretro-core.plugin.ts").replace(
        "export { handlers } from \"./retroarch\"",
        "export const handlers = {'settings.validate': () => { throw Error('must not validate'); }}",
    );
    fs::write(package.package.join("plugin.ts"), source).unwrap();
    let device_path = game.root.path().join("device.yaml");
    let mut device: serde_json::Value =
        serde_yaml::from_str(&fs::read_to_string(&device_path).unwrap()).unwrap();
    device["host"] = serde_json::json!({"runners":{"@korri:mgba/mgba":{"settings":{"speed":2}}}});
    fs::write(&device_path, serde_yaml::to_string(&device).unwrap()).unwrap();
    game.snapshot = (*ConfigSnapshotCoordinator::new(game.root.path())
        .reload()
        .snapshot)
        .clone();
    let error = game.launch().unwrap_err().to_string();
    assert!(
        error.contains(
            "runner @korri:mgba/mgba has no launch.prepare handler, so it takes no settings"
        ),
        "{error}"
    );
}
