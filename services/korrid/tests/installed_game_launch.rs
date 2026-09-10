#[path = "fixtures/readable.rs"]
mod readable;
use korrid::{plugin::load_plugin_source, script::call_plugin_launch_ts};
use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn package(
    root: &Path,
    id: &str,
    source: &str,
    files: BTreeMap<String, std::path::PathBuf>,
) -> korrid::plugin_installation::EnabledPackage {
    let path = root.join(id.replace(['@', ':'], "-"));
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("plugin.ts"), source).unwrap();
    korrid::plugin_installation::EnabledPackage {
        id: id.into(),
        package: path,
        files,
        requires: vec![],
    }
}

#[test]
fn a_chosen_runtime_borrows_the_kind_callback_but_uses_its_instances_own_program() {
    use korrid::{
        config::{resolver::resolve_linux_route, snapshot::ConfigSnapshotCoordinator},
        launcher::{
            linux_plugin::launch_route,
            plugin_launch::{evaluate, PluginLaunchInput},
        },
        plugin::PluginRegistry,
    };
    let root = tempfile::tempdir().unwrap();
    readable::combined(root.path());
    fs::create_dir_all(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let default = package(
        root.path(),
        "@korri:retroarch",
        include_str!("../../../plugins/retroarch/plugin.ts"),
        BTreeMap::from([
            ("retroarch".into(), root.path().join("default-retroarch")),
            ("autoconfig".into(), root.path().join("kind-autoconfig")),
        ]),
    );
    let mgba = package(
        root.path(),
        "@korri:mgba",
        include_str!("../../../plugins/mgba/plugin.ts"),
        BTreeMap::from([("mgba".into(), root.path().join("default-mgba.so"))]),
    );
    let alternate = package(
        root.path(),
        "@simon:build",
        r#"
        export const name = 'build';
        export const launchers = {
            retroarch: {id:'@simon:build/retroarch', kind:'@korri:retroarch/retroarch', program:'retroarch'},
            other: {id:'@simon:build/other', kind:'@simon:build/other', program:'other'},
            second: {id:'@simon:build/second', kind:'@simon:build/second', program:'other'}
        };
        export const runtimes = {
            mgba: {id:'@simon:build/mgba', kind:'libretro-core', launcher:'@simon:build/retroarch', path:'mgba', supports:{systems:['gba']}},
            other: {id:'@simon:build/other', kind:'emulator', launcher:'@simon:build/other', path:'other', supports:{systems:['gba']}},
            second: {id:'@simon:build/second', kind:'emulator', launcher:'@simon:build/second', path:'other', supports:{systems:['gba']}}
        };
        export function launch(input) { return {command: input.program, args: [input.launcherKind, input.contentPath]}; }
    "#,
        BTreeMap::from([
            ("retroarch".into(), root.path().join("alternate-retroarch")),
            ("other".into(), root.path().join("other-emulator")),
            ("mgba".into(), root.path().join("alternate-mgba.so")),
            (
                "autoconfig".into(),
                root.path().join("wrong-instance-autoconfig"),
            ),
        ]),
    );
    let callback_path = default.package.join("plugin.ts");
    let registry = PluginRegistry::from_installed(vec![default, mgba, alternate]).unwrap();
    let snapshot = ConfigSnapshotCoordinator::new(root.path())
        .reload()
        .snapshot;
    assert!(
        resolve_linux_route(root.path(), &snapshot, &registry, readable::GBA_ID, None)
            .unwrap_err()
            .message
            .contains("choose a runtime")
    );
    let route = resolve_linux_route(
        root.path(),
        &snapshot,
        &registry,
        readable::GBA_ID,
        Some("@simon:build/mgba"),
    )
    .unwrap();
    assert_eq!(route.launcher_id, "@simon:build/retroarch");
    let spec = launch_route(root.path(), &snapshot, &registry, &route, None).unwrap();
    assert_eq!(spec.command[2], callback_path.display().to_string());
    assert!(!root.path().join("users").exists());
    let input: PluginLaunchInput = serde_json::from_str(&spec.command[3]).unwrap();
    assert_eq!(input.launcher_kind, "@korri:retroarch/retroarch");
    assert_eq!(
        input.program,
        root.path()
            .join("alternate-retroarch")
            .display()
            .to_string()
    );
    assert_eq!(
        input.runtime_path,
        root.path().join("alternate-mgba.so").display().to_string()
    );
    assert_eq!(
        input.files["autoconfig"],
        root.path().join("kind-autoconfig").display().to_string()
    );
    let output = evaluate(&fs::read_to_string(callback_path).unwrap(), &input).unwrap();
    assert!(output.files[0].content.contains("states/@simon:build/mgba"));
    let route = resolve_linux_route(
        root.path(),
        &snapshot,
        &registry,
        readable::GBA_ID,
        Some("@simon:build/other"),
    )
    .unwrap();
    let spec = launch_route(root.path(), &snapshot, &registry, &route, None).unwrap();
    let input: PluginLaunchInput = serde_json::from_str(&spec.command[3]).unwrap();
    let output = evaluate(&fs::read_to_string(&spec.command[2]).unwrap(), &input).unwrap();
    assert_eq!(
        output.command,
        root.path().join("other-emulator").display().to_string()
    );
    assert_eq!(
        output.args,
        [
            "@simon:build/other".into(),
            root.path().join("roms/wl4.gba").display().to_string()
        ]
    );
    assert!(output.files.is_empty());
    let route = resolve_linux_route(
        root.path(),
        &snapshot,
        &registry,
        readable::GBA_ID,
        Some("@simon:build/second"),
    )
    .unwrap();
    let spec = launch_route(root.path(), &snapshot, &registry, &route, None).unwrap();
    let input: PluginLaunchInput = serde_json::from_str(&spec.command[3]).unwrap();
    let output = evaluate(&fs::read_to_string(&spec.command[2]).unwrap(), &input).unwrap();
    assert_eq!(output.args[0], "@simon:build/second");
}

#[test]
fn callback_files_environment_and_exec_are_effects_of_the_unprivileged_launch_process() {
    use std::os::unix::fs::MetadataExt;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("plugin.ts");
    let file = root.path().join("nested/config");
    let result = serde_json::json!({"command":"/bin/sh", "args":["-c", "printf '%s/%s' \"$VALUE\" \"${REMOVE-meant-to-be-unset}\""], "env":{"VALUE":"from-plugin"}, "envUnset":["REMOVE"], "cwd":root.path(), "directories":[], "files":[{"path":file,"content":"runtime content"}]});
    fs::write(
        &source,
        format!(
            "export const name = 'execution'; export function launch(input) {{ return {result}; }}"
        ),
    )
    .unwrap();
    let input = serde_json::json!({"launcherId":"@test:execution/execution", "launcherKind":"@test:execution/execution", "runtimeId":"@test:runtime/runtime", "program":"/bin/sh", "runtimePath":"/core", "contentPath":"/rom", "accountRoot":root.path(), "files":{}});
    let output = Command::new(env!("CARGO_BIN_EXE_korrid"))
        .args([
            "plugin-launch",
            source.to_str().unwrap(),
            &input.to_string(),
        ])
        .env("REMOVE", "inherited")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"from-plugin/meant-to-be-unset");
    assert_eq!(fs::read_to_string(&file).unwrap(), "runtime content");
    assert_eq!(fs::metadata(file).unwrap().uid(), unsafe {
        libc::geteuid()
    });
}

#[test]
#[ignore = "requires the built plugin.nix payload; set KORRI_TEST_GAME_PACKAGE"]
fn built_plugin_nix_payload_resolves_its_exact_launcher_and_runs_the_packaged_program() {
    #[derive(serde::Deserialize)]
    struct Manifest {
        files: BTreeMap<String, std::path::PathBuf>,
        requires: Vec<std::path::PathBuf>,
        publisher: Publisher,
    }
    #[derive(serde::Deserialize)]
    struct Publisher {
        namespace: String,
    }
    fn selection(path: std::path::PathBuf) -> korrid::plugin_installation::EnabledPackage {
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(path.join("manifest.json")).unwrap()).unwrap();
        let source = fs::read_to_string(path.join("plugin.ts")).unwrap();
        let plugin = load_plugin_source(&manifest.publisher.namespace, &source).unwrap();
        korrid::plugin_installation::EnabledPackage {
            id: plugin.id().into(),
            package: path,
            files: manifest.files,
            requires: manifest.requires,
        }
    }
    let runtime = selection(
        std::env::var_os("KORRI_TEST_GAME_PACKAGE")
            .expect("built mGBA package")
            .into(),
    );
    assert_eq!(runtime.id, "@korri:mgba");
    assert_eq!(runtime.requires.len(), 1);
    let launcher = selection(runtime.requires[0].clone());
    let program = launcher.files["retroarch"].clone();
    assert!(launcher.files["autoconfig"]
        .join("udev/Microsoft X-Box 360 pad.cfg")
        .is_file());
    assert!(runtime.files["mgba"].is_file());
    let registry = korrid::plugin::PluginRegistry::from_installed(vec![runtime, launcher]).unwrap();
    assert_eq!(
        registry
            .file_release_discovery_claims_for_extension("gba")
            .len(),
        1
    );
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    fs::write(folder.path().join("game.gba"), b"rom").unwrap();
    let report = korrid::discovery::DiscoveryCoordinator::new(root.path(), private.path())
        .with_registry_source(korrid::plugin_policy::RegistrySource::Selected(
            registry.clone().into(),
        ))
        .add_location(
            folder.path(),
            &korrid::discovery::DiscoveryOptions::default(),
        )
        .unwrap();
    assert_eq!(report.added_games, 1);
    let snapshot = korrid::config::snapshot::ConfigSnapshotCoordinator::new(root.path())
        .reload()
        .snapshot;
    let game = snapshot.games.keys().next().unwrap();
    let route = korrid::config::resolver::resolve_linux_route(
        root.path(),
        &snapshot,
        &registry,
        game,
        Some("@korri:mgba/mgba"),
    )
    .unwrap();
    let spec = korrid::launcher::linux_plugin::launch_route(
        root.path(),
        &snapshot,
        &registry,
        &route,
        None,
    )
    .unwrap();
    let input = serde_json::from_str(&spec.command[3]).unwrap();
    let output = korrid::launcher::plugin_launch::evaluate(
        &fs::read_to_string(&spec.command[2]).unwrap(),
        &input,
    )
    .unwrap();
    assert_eq!(output.command, program.display().to_string());
    assert_eq!(output.args[2], "-L");
    assert_eq!(output.args[3], route.runtime.unwrap().path);
    assert!(output.files[0].content.contains("states/@korri:mgba/mgba"));
    let version = Command::new(program)
        .arg("--version")
        .env("HOME", root.path())
        .output()
        .unwrap();
    assert!(
        version.status.success(),
        "{}",
        String::from_utf8_lossy(&version.stderr)
    );
    assert!(String::from_utf8_lossy(&version.stdout).contains("RetroArch"));
}

#[test]
fn real_linux_plugins_declare_immutable_file_keys_and_return_the_existing_retroarch_command() {
    let source = include_str!("../../../plugins/retroarch/plugin.ts");
    load_plugin_source("@korri", source).unwrap();
    let data: serde_json::Value =
        serde_json::from_str(&korrid::script::eval_plugin_ts(source).unwrap()).unwrap();
    assert_eq!(
        data["launchers"]["retroarch"]["kind"],
        "@korri:retroarch/retroarch"
    );
    assert_eq!(data["launchers"]["retroarch"]["program"], "retroarch");
    let input = serde_json::json!({
        "launcherId": "@korri:retroarch/retroarch",
        "launcherKind": "@korri:retroarch/retroarch",
        "runtimeId": "@korri:mgba/mgba",
        "program": "/nix/store/build/bin/retroarch",
        "runtimePath": "/nix/store/core/mgba.so",
        "contentPath": "/korri/roms/wl4.gba",
        "accountRoot": "/korri/users/default",
        "files": {"autoconfig": "/nix/store/pads/share/libretro/autoconfig"},
        "overrides": {"config": {"prepend": "video_vsync = false", "append": "video_vsync = true"}}
    });
    let result: serde_json::Value =
        serde_json::from_str(&call_plugin_launch_ts(source, &input.to_string()).unwrap()).unwrap();
    assert_eq!(result["command"], input["program"]);
    assert_eq!(
        result["args"],
        serde_json::json!([
            "--config",
            "/korri/users/default/retroarch.cfg",
            "-L",
            "/nix/store/core/mgba.so",
            "/korri/roms/wl4.gba"
        ])
    );
    let config = result["files"][0]["content"].as_str().unwrap();
    for line in [
        "input_driver = \"udev\"",
        "input_quit_gamepad_combo = \"4\"",
        "menu_driver = \"null\"",
        "config_save_on_exit = \"false\"",
        "savestate_directory = \"/korri/users/default/states/@korri:mgba/mgba\"",
    ] {
        assert!(config.contains(line), "{line}");
    }
    assert!(config.ends_with("video_vsync = false\nvideo_vsync = true\n"));
}

#[test]
fn retroarch_raw_overrides_preserve_the_legacy_add_only_contract() {
    let source = include_str!("../../../plugins/retroarch/plugin.ts");
    for config in [
        serde_json::json!({"replace": "video_vsync = true"}),
        serde_json::json!({"append": "menu_driver = rgui"}),
        serde_json::json!({"prepend": "cheevos_password = secret"}),
    ] {
        let input = serde_json::json!({"launcherId":"@korri:retroarch/retroarch", "launcherKind":"@korri:retroarch/retroarch", "runtimeId":"@korri:mgba/mgba", "program":"/retroarch", "runtimePath":"/mgba.so", "contentPath":"/rom.gba", "accountRoot":"/korri/users/default", "files":{"autoconfig":"/pads"}, "overrides":{"config":config}});
        assert!(call_plugin_launch_ts(source, &input.to_string()).is_err());
    }
}
