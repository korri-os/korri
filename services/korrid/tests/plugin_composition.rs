//! Exercise the shipped CLI with real package manifests, module snapshots and exec.
#[path = "fixtures/readable.rs"]
mod readable;
use korrid::{
    config::decode_config_documents,
    launcher::{
        linux_plugin::launch_route,
        plugin_launch::{prepare, LaunchComposition, PluginLaunchInput},
    },
    plugin::{load_plugin_source, PluginRegistry},
    plugin_installation::EnabledPackage,
};
use std::{
    collections::BTreeMap, fs, os::unix::net::UnixDatagram, path::Path, process::Command,
    time::Duration,
};

const RUNNER: &str = r#"
export const name = 'mgba';
export const runners = {mgba: {id: '@korri:mgba/mgba', systems:['gba'], program:'program'}};
export const handlers = {'launch.prepare': input => ({
 command: input.program, args: ['-c', 'test "$PRESERVED" = yes && test -z "${REMOVED+x}" && test -f provisioned && printf game > game-ran'],
 env:{PRESERVED:'yes', PASSWORD:'very-private'}, envUnset:['REMOVED'], cwd:input.accountRoot,
 directories:[input.accountRoot], files:[{path:input.accountRoot+'/provisioned', content:'runner file'}]
})};
"#;
const MODIFIER: &str = r#"
import {compose} from './compose';
export const name = 'wrapper';
export const modifiers = {main: {id:'@test:wrapper/main'}};
export const handlers = {'launch.compose': compose};
"#;
const COMPOSE: &str = r#"
export function compose(input) {
 if (input.modifierId !== '@test:wrapper/main' || input.runnerId !== '@korri:mgba/mgba') throw new Error('wrong selection');
 if (input.settings.requireDisplay && (input.display?.width !== 1240 || input.display?.height !== 1080)) throw Object.assign(new Error('missing logical display'), {code:'display-unavailable',retryable:false});
 if (input.settings.sharpness > 20) throw Object.assign(new Error('setting sharpness must be an integer 0..20'), {code:'setting-range',retryable:false});
 if (input.settings.invalid) throw new Error('width must be a positive integer');
 if (input.settings.fail) throw new Error('compose refused');
 if (input.settings.invalidOutput) return {...input.plan, [input.config.append.split(' ').join('')]: true};
 if (input.settings.invalidPlan) return {...input.plan, command: ''};
 if (input.settings.nativeFailure) return {...input.plan, command:'/bin/sh',args:['-c', 'printf "native startup failure $PASSWORD $INHERITED_SECRET\n" >&2; exit 1']};
 if (input.settings.logPlan) return {...input.plan, command:'/bin/sh',args:['-c','exit 0','wrapper','--backend','wayland','--sharpness','20','--raw','private value','--',input.plan.command,...input.plan.args]};
 if (input.config.append) throw new Error('append rejected: ' + input.config.append);
 return {...input.plan, command: input.files.program, args:['-c','printf wrapped > wrapped; exec "$@"','wrapper',input.plan.command,...input.plan.args]};
}
"#;
fn package(root: &Path, owner: &str, source: &str, extra: Option<(&str, &str)>) -> EnabledPackage {
    package_with_files(
        root,
        owner,
        source,
        extra,
        BTreeMap::from([("program".into(), Path::new("/bin/sh").to_path_buf())]),
    )
}
fn package_with_files(
    root: &Path,
    owner: &str,
    source: &str,
    extra: Option<(&str, &str)>,
    files: BTreeMap<String, std::path::PathBuf>,
) -> EnabledPackage {
    let dir = root.join(owner.split(':').nth(1).unwrap());
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("plugin.ts"), source).unwrap();
    let mut sources = vec!["plugin.ts".into()];
    if let Some((name, text)) = extra {
        fs::write(dir.join(name), text).unwrap();
        sources.push(name.into());
        sources.sort();
    }
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec(
            &serde_json::json!({"entry":"plugin.ts","sources":sources,"files":files}),
        )
        .unwrap(),
    )
    .unwrap();
    EnabledPackage {
        id: owner.into(),
        package: dir,
        entry: "plugin.ts".into(),
        sources,
        files,
        requires: Vec::new(),
        hosts: vec![],
    }
}
fn setup(root: &Path, record: &str) -> (Vec<String>, PluginLaunchInput, LaunchComposition) {
    let runner = package(root, "@korri:mgba", RUNNER, None);
    let modifier = package(
        root,
        "@test:wrapper",
        MODIFIER,
        Some(("compose.ts", COMPOSE)),
    );
    let registry = PluginRegistry::from_installed(vec![runner, modifier]).unwrap();
    let device = format!(
        "{}\nhost: {{modifiers: {{'@test:wrapper/main': {record}}}}}",
        readable::gba_locations("roms", "wl4.gba", false)
    );
    fs::create_dir_all(root.join("roms")).unwrap();
    fs::write(root.join("roms/wl4.gba"), b"rom").unwrap();
    let snapshot = decode_config_documents(&device, &readable::gba_library()).unwrap();
    let route = korrid::config::resolver::resolve_linux_route(
        root,
        &snapshot,
        &registry,
        readable::GBA_ID,
        Some("@korri:mgba/mgba"),
    )
    .unwrap();
    let spec = launch_route(
        root,
        &snapshot,
        &registry,
        &route,
        None,
        None,
        "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
    )
    .unwrap();
    let input = serde_json::from_str(&spec.command[3]).unwrap();
    let composition = serde_json::from_str(&spec.command[4]).unwrap();
    (spec.command, input, composition)
}
fn cli(command: &[String]) -> Command {
    let command = korrid::launcher::plugin_launch::prepare_command(command)
        .expect("host prepares the plan before the unit starts");
    let mut process = Command::new(env!("CARGO_BIN_EXE_korrid"));
    process
        .env_clear()
        .args(&command[1..])
        .env("REMOVED", "must disappear");
    process
}
#[test]
fn plugin_prepare_prints_the_core_handoff_without_declared_effects() {
    let root = tempfile::tempdir().unwrap();
    let (command, input, _) = setup(root.path(), "{enabled: true}");
    let expected = korrid::launcher::plugin_launch::prepare_command(&command).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_korrid"))
        .env_clear()
        .arg("plugin-prepare")
        .args(&command[2..])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let record: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        record,
        serde_json::from_str::<serde_json::Value>(&expected[5]).unwrap()
    );
    assert!(!Path::new(&input.account_root).exists());

    // The emitted record is accepted by the actual launch entry point.
    let launched = Command::new(env!("CARGO_BIN_EXE_korrid"))
        .env_clear()
        .arg("plugin-launch")
        .args(&command[2..5])
        .arg(String::from_utf8(output.stdout).unwrap().trim())
        .output()
        .unwrap();
    assert!(
        launched.status.success(),
        "{}",
        String::from_utf8_lossy(&launched.stderr)
    );
    assert!(Path::new(&input.account_root).join("game-ran").exists());
}

#[test]
fn plugin_prepare_rejects_invalid_input_and_sandbox_effects() {
    let root = tempfile::tempdir().unwrap();
    let (command, input, _) = setup(root.path(), "{enabled: true}");
    for args in [
        vec![],
        command[2..5].to_vec(),
        vec![
            command[2].clone(),
            "{}".into(),
            command[4].clone(),
            command[5].clone(),
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_korrid"))
            .env_clear()
            .arg("plugin-prepare")
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!Path::new(&input.account_root).exists());
    }
    fs::write(
        &command[2],
        RUNNER.replace("command: input.program", "command: process.env.PROGRAM"),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_korrid"))
        .env_clear()
        .arg("plugin-prepare")
        .args(&command[2..])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("process"));
    assert!(!Path::new(&input.account_root).exists());
}

#[test]
fn plugin_prepare_refuses_non_utf8_arguments() {
    use std::os::unix::ffi::OsStringExt;
    let output = Command::new(env!("CARGO_BIN_EXE_korrid"))
        .env_clear()
        .arg("plugin-prepare")
        .arg(std::ffi::OsString::from_vec(vec![0xff]))
        .args(["{}", "{}", "release"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("UTF-8"));
}

#[test]
fn plugin_run_prepares_and_executes_a_fixture_through_the_public_cli() {
    let root = tempfile::tempdir().unwrap();
    let (command, input, _) = setup(root.path(), "{enabled: true}");
    let result = Command::new(env!("CARGO_BIN_EXE_korrid"))
        .env_clear()
        .env("REMOVED", "must disappear")
        .arg("plugin-run")
        .args(&command[2..])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let account = Path::new(&input.account_root);
    assert_eq!(
        fs::read_to_string(account.join("game-ran")).unwrap(),
        "game"
    );
    assert_eq!(
        fs::read_to_string(account.join("wrapped")).unwrap(),
        "wrapped"
    );
    assert_eq!(
        fs::read_to_string(account.join("provisioned")).unwrap(),
        "runner file"
    );
}

#[test]
fn plugin_run_keeps_sandbox_refusals_and_requires_the_full_cli_shape() {
    let help = Command::new(env!("CARGO_BIN_EXE_korrid"))
        .env_clear()
        .args(["plugin-run", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout)
        .contains("SOURCE INPUT_JSON COMPOSITION_JSON RELEASE_ID"));
    let root = tempfile::tempdir().unwrap();
    let (command, input, _) = setup(root.path(), "{enabled: true}");
    // No daemon-side test helper prepares this plan. The actual CLI must
    // enter the empty sandbox and fail before applying any declared effects.
    fs::write(
        &command[2],
        RUNNER.replace("command: input.program", "command: process.env.PROGRAM"),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_korrid"))
        .env_clear()
        .arg("plugin-run")
        .args(&command[2..])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("process"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!Path::new(&input.account_root).exists());
    for verb in ["plugin-run", "plugin-launch"] {
        let missing = Command::new(env!("CARGO_BIN_EXE_korrid"))
            .env_clear()
            .arg(verb)
            .args(&command[2..5])
            .output()
            .unwrap();
        assert!(!missing.status.success());
        assert!(String::from_utf8_lossy(&missing.stderr).contains("usage:"));
    }
    assert!(!Path::new(&input.account_root).exists());
}

#[test]
fn plugin_launch_execs_the_composed_plan_and_preserves_runner_effects() {
    let root = tempfile::tempdir().unwrap();
    let (mut command, input, mut composition) = setup(
        root.path(),
        "{enabled: true, settings: {requireDisplay: true}}",
    );
    composition.display = Some(korrid::launcher::plugin_launch::PluginDisplay {
        width: 1240,
        height: 1080,
    });
    command[4] = serde_json::to_string(&composition).unwrap();
    assert_eq!(
        composition.modifiers[0].manifest,
        root.path().join("wrapper/manifest.json")
    );
    let plan = prepare(Path::new(&command[2]), &input, &composition).unwrap();
    assert_eq!(plan.command, "/bin/sh");
    assert_eq!(
        &plan.args[0..4],
        [
            "-c",
            "printf wrapped > wrapped; exec \"$@\"",
            "wrapper",
            "/bin/sh"
        ]
    );
    assert_eq!(plan.env["PRESERVED"], "yes");
    assert_eq!(plan.env_unset, ["REMOVED"]);
    assert_eq!(plan.files[0].content, "runner file");
    assert_eq!(plan.cwd.as_deref(), Some(input.account_root.as_str()));
    assert!(
        !Path::new(&input.account_root).exists(),
        "prepare has no effects"
    );
    let result = cli(&command).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read_to_string(Path::new(&input.account_root).join("wrapped")).unwrap(),
        "wrapped"
    );
    assert_eq!(
        fs::read_to_string(Path::new(&input.account_root).join("game-ran")).unwrap(),
        "game"
    );
    assert!(!String::from_utf8_lossy(&result.stderr).contains("very-private"));
}
#[test]
fn steam_provider_launch_composes_the_prepared_session_before_native_exec() {
    let root = tempfile::tempdir().unwrap();
    // The provider identity and location are upstream's Flinthook case. The
    // shipped handler still produces steam-session <ref> without a contentPath.
    // /bin/sh reads the numeric ref fixture without executing a fresh script
    // inode, avoiding the existing generated-executable ETXTBSY fixture race.
    // Steam's launch.prepare reads its exact required FEX and Proton files.
    let fex = package_with_files(
        root.path(),
        "@korri:fex",
        include_str!("fixtures/first-party/fex/plugin.ts"),
        None,
        BTreeMap::from([
            ("fex-unpack".into(), root.path().join("fex-unpack")),
            ("config".into(), root.path().join("fex-config")),
        ]),
    );
    let proton = package_with_files(
        root.path(),
        "@korri:proton",
        include_str!("fixtures/first-party/proton/plugin.ts"),
        None,
        BTreeMap::from([("cachyos-unpack".into(), root.path().join("cachyos-unpack"))]),
    );
    let mut runner = package_with_files(
        root.path(),
        "@korri:steam",
        include_str!("fixtures/first-party/steam/plugin.ts"),
        None,
        BTreeMap::from([("steam-session".into(), Path::new("/bin/sh").to_path_buf())]),
    );
    runner.requires = vec![fex.package.clone(), proton.package.clone()];
    let modifier = package(
        root.path(),
        "@test:wrapper",
        r#"
export const name = 'wrapper';
export const modifiers = {main: {id: '@test:wrapper/main'}};
export const handlers = {'launch.compose': input => {
  if (!input.plan.env.STEAM_HOME || input.plan.args.length !== 1) throw new Error('runner preparation must precede compose');
  return {...input.plan, command:input.files.program,
    args:['-c', 'printf wrapped > "$STEAM_HOME/wrapped"; exec "$@"', 'wrapper', input.plan.command, ...input.plan.args]};
}};
"#,
        None,
    );
    let registry = PluginRegistry::from_installed(vec![runner, fex, proton, modifier]).unwrap();
    for artifact in registry.artifacts().values() {
        let (owner, key) = artifact.id.split_once('/').unwrap();
        let name = owner.split_once(':').unwrap().1;
        fs::create_dir_all(
            root.path()
                .join("cache")
                .join(name)
                .join(key)
                .join(&artifact.version),
        )
        .unwrap();
    }
    let snapshot = decode_config_documents(
        "locations:\n  '@korri:steam/401710': [{provider: '@korri:steam', ref: '401710'}]\nhost: {modifiers: {'@test:wrapper/main': {enabled: true}}}\n",
        "library:\n  flinthook:\n    title: Flinthook\n    releases:\n      - id: steam\n        system: steam\n        identity: {kind: provider, value: '@korri:steam/401710'}\n",
    ).unwrap();
    let route = korrid::config::resolver::resolve_linux_route(
        root.path(),
        &snapshot,
        &registry,
        "flinthook",
        Some("@korri:steam/steam"),
    )
    .unwrap();
    let spec = launch_route(
        root.path(),
        &snapshot,
        &registry,
        &route,
        None,
        None,
        "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
    )
    .unwrap();
    assert_eq!(spec.command[1], "plugin-prepare");
    let input: PluginLaunchInput = serde_json::from_str(&spec.command[3]).unwrap();
    assert!(input.content_path.is_none());
    assert_eq!(input.provider_ref.as_deref(), Some("401710"));
    let composition: LaunchComposition = serde_json::from_str(&spec.command[4]).unwrap();
    assert_eq!(composition.modifiers.len(), 1);
    let plan = prepare(Path::new(&spec.command[2]), &input, &composition).unwrap();
    assert_eq!(plan.args[3..], ["/bin/sh", "401710"]);
    let home = root
        .path()
        .join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/steam");
    assert_eq!(plan.env["STEAM_HOME"], home.display().to_string());
    assert_eq!(plan.directories, [home.display().to_string()]);
    assert!(!home.exists(), "prepare and compose have no effects");
    fs::write(root.path().join("401710"), "test -f \"$STEAM_HOME/wrapped\" || exit 81\nprintf '%s' \"$0\" > \"$STEAM_HOME/game-ran\"\n").unwrap();
    let result = cli(&spec.command)
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read_to_string(home.join("wrapped")).unwrap(), "wrapped");
    assert_eq!(fs::read_to_string(home.join("game-ran")).unwrap(), "401710");
}

#[test]
fn compose_refusals_missing_sources_files_and_invalid_plans_stop_before_all_effects() {
    for (record, expected) in [
        (
            "{enabled: true, settings: {invalid: true}}",
            "width must be a positive integer",
        ),
        ("{enabled: true, settings: {fail: true}}", "compose refused"),
        (
            "{enabled: true, settings: {invalidPlan: true}}",
            "launch command must be non-empty",
        ),
        (
            "{enabled: true, config: {append: '--secret=never-publish'}}",
            "append rejected",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let (command, input, _) = setup(root.path(), record);
        let reason = korrid::launcher::plugin_launch::prepare_command(&command).unwrap_err();
        assert!(reason.contains(expected), "{reason}");
        assert!(reason.contains("@test:wrapper/main"), "{reason}");
        assert!(!reason.contains("never-publish"), "{reason}");
        assert!(!Path::new(&input.account_root).exists());
    }
    for remove in ["manifest.json", "plugin.ts", "compose.ts"] {
        let root = tempfile::tempdir().unwrap();
        let (command, input, _) = setup(root.path(), "{enabled: true}");
        fs::remove_file(root.path().join("wrapper").join(remove)).unwrap();
        let reason = korrid::launcher::plugin_launch::prepare_command(&command).unwrap_err();
        assert!(reason.contains("@test:wrapper/main"));
        assert!(!Path::new(&input.account_root).exists());
    }
    let root = tempfile::tempdir().unwrap();
    let (command, input, _) = setup(root.path(), "{enabled: true}");
    fs::write(root.path().join("wrapper/manifest.json"), r#"{"entry":"plugin.ts","sources":["compose.ts","plugin.ts"],"files":{"program":"/no/such/modifier-file"}}"#).unwrap();
    let reason = korrid::launcher::plugin_launch::prepare_command(&command).unwrap_err();
    assert!(reason.contains("modifier file program is missing"));
    assert!(!Path::new(&input.account_root).exists());
}
#[test]
fn unit_revalidates_the_prepared_plan_and_reports_refusal_through_systemd_status() {
    let root = tempfile::tempdir().unwrap();
    let (command, _, _) = setup(root.path(), "{enabled: true}");
    let mut command = korrid::launcher::plugin_launch::prepare_command(&command).unwrap();
    let mut handoff: serde_json::Value = serde_json::from_str(&command[5]).unwrap();
    handoff["plan"]["command"] = "".into();
    command[5] = handoff.to_string();
    let socket = root.path().join("notify.sock");
    let notify = UnixDatagram::bind(&socket).unwrap();
    notify
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let result = cli(&command)
        .env("NOTIFY_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(!result.status.success());
    let mut bytes = [0; 4096];
    let len = notify.recv(&mut bytes).unwrap();
    let status = std::str::from_utf8(&bytes[..len]).unwrap();
    assert!(
        status.starts_with("STATUS=plugin-launch: launch command must be non-empty"),
        "{status}"
    );
    assert!(
        status.contains("launch command must be non-empty"),
        "{status}"
    );
}
#[test]
fn modifier_declarations_cannot_claim_another_package_or_add_runner_fields() {
    for record in [
        "{id:'@other:wrapper/main'}",
        "{id:'@test:wrapper/main',program:'program'}",
    ] {
        let source =
            format!("export const name='wrapper'; export const modifiers={{main:{record}}};");
        assert!(load_plugin_source("@test", &source).is_err());
    }
}

#[test]
fn enabled_modifiers_require_their_exact_installed_contribution() {
    let root = tempfile::tempdir().unwrap();
    let runner = package(root.path(), "@korri:mgba", RUNNER, None);
    let modifier = package(
        root.path(),
        "@test:wrapper",
        MODIFIER,
        Some(("compose.ts", COMPOSE)),
    );
    fs::create_dir_all(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let registry = PluginRegistry::from_installed(vec![runner, modifier]).unwrap();
    for (id, enabled, success) in [
        ("@test:missing/main", "true", false),
        ("@test:wrapper/undeclared", "true", false),
        ("@test:missing/main", "false", true),
        ("@test:wrapper/main", "true", true),
    ] {
        let device = format!(
            "{}\nhost: {{modifiers: {{'{id}': {{enabled: {enabled}}}}}}}",
            readable::gba_locations("roms", "wl4.gba", false)
        );
        let snapshot = decode_config_documents(&device, &readable::gba_library()).unwrap();
        let route = korrid::config::resolver::resolve_linux_route(
            root.path(),
            &snapshot,
            &registry,
            readable::GBA_ID,
            Some("@korri:mgba/mgba"),
        )
        .unwrap();
        let result = launch_route(
            root.path(),
            &snapshot,
            &registry,
            &route,
            None,
            None,
            "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
        );
        assert_eq!(result.is_ok(), success, "{id}: {result:?}");
        if let Err(error) = result {
            assert!(error.to_string().contains(id));
        }
    }
    let (command, _, composition) = setup(root.path(), "{settings: {width: 1240}}");
    assert!(composition.modifiers.is_empty(), "absent enabled is off");
    let result = cli(&command).output().unwrap();
    assert!(result.status.success());
    assert!(!root
        .path()
        .join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/wrapped")
        .exists());
}

#[test]
fn multiple_modifiers_compose_the_previous_plan_in_device_key_order() {
    let root = tempfile::tempdir().unwrap();
    let runner = package(root.path(), "@korri:mgba", RUNNER, None);
    let modifier = |name: &str| {
        package(root.path(), &format!("@test:{name}"), &format!("export const name='{name}'; export const modifiers={{main:{{id:'@test:{name}/main'}}}}; export const handlers={{'launch.compose': input => ({{...input.plan,args:[...input.plan.args,'{name}']}})}};"), None)
    };
    let registry =
        PluginRegistry::from_installed(vec![runner, modifier("z"), modifier("a")]).unwrap();
    fs::create_dir(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let device = format!(
        "{}\nhost:\n  modifiers:\n    '@test:z/main': {{enabled: true}}\n    '@test:a/main': {{enabled: true}}",
        readable::gba_locations("roms", "wl4.gba", false)
    );
    let snapshot = decode_config_documents(&device, &readable::gba_library()).unwrap();
    let route = korrid::config::resolver::resolve_linux_route(
        root.path(),
        &snapshot,
        &registry,
        readable::GBA_ID,
        Some("@korri:mgba/mgba"),
    )
    .unwrap();
    let spec = launch_route(
        root.path(),
        &snapshot,
        &registry,
        &route,
        None,
        None,
        "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
    )
    .unwrap();
    let input = serde_json::from_str(&spec.command[3]).unwrap();
    let composition: LaunchComposition = serde_json::from_str(&spec.command[4]).unwrap();
    assert_eq!(
        composition
            .modifiers
            .iter()
            .map(|v| v.modifier_id.as_str())
            .collect::<Vec<_>>(),
        ["@test:z/main", "@test:a/main"]
    );
    let plan = prepare(Path::new(&spec.command[2]), &input, &composition).unwrap();
    assert_eq!(&plan.args[plan.args.len() - 2..], ["z", "a"]);
}

#[test]
fn compose_causes_survive_empty_and_nonempty_native_append() {
    for append in ["", "   \t", "--native-option 'private value'"] {
        let record = serde_json::json!({"enabled":true,"settings":{"sharpness":21},"config":{"append":append}}).to_string();
        let root = tempfile::tempdir().unwrap();
        let (command, _, _) = setup(root.path(), &record);
        let reason = korrid::launcher::plugin_launch::prepare_command(&command).unwrap_err();
        assert!(
            reason.contains("setting sharpness must be an integer 0..20 (setting-range)"),
            "{reason}"
        );
        assert!(!reason.contains("private value"), "{reason}");
    }
    let root = tempfile::tempdir().unwrap();
    let (command, _, _) = setup(
        root.path(),
        "{enabled: true, settings: {requireDisplay: true}, config: {append: '--native-option'}}",
    );
    let reason = korrid::launcher::plugin_launch::prepare_command(&command).unwrap_err();
    assert!(
        reason.contains("missing logical display (display-unavailable)"),
        "{reason}"
    );
}

#[test]
fn malformed_append_refuses_with_a_fixed_message_without_echoing_words() {
    let root = tempfile::tempdir().unwrap();
    let record =
        serde_json::json!({"enabled":true,"config":{"append":"--native 'private unfinished"}})
            .to_string();
    let (command, _, _) = setup(root.path(), &record);
    let reason = korrid::launcher::plugin_launch::prepare_command(&command).unwrap_err();
    assert!(
        reason.contains("config.append could not be split into words"),
        "{reason}"
    );
    assert!(!reason.contains("private"), "{reason}");
}

#[test]
fn the_final_plan_log_keeps_fixed_typed_and_runner_argv_visible() {
    let root = tempfile::tempdir().unwrap();
    let (command, _, _) = setup(
        root.path(),
        "{enabled: true, settings: {logPlan: true}, config: {append: \"--raw 'private value'\"}}",
    );
    let result = cli(&command).output().unwrap();
    assert!(result.status.success());
    let stderr = String::from_utf8(result.stderr).unwrap();
    let logged = stderr
        .lines()
        .find_map(|line| line.strip_prefix("korrid plugin-launch plan: "))
        .unwrap();
    let plan: serde_json::Value = serde_json::from_str(logged).unwrap();
    assert_eq!(
        plan["args"].as_array().unwrap()[3..12],
        serde_json::json!([
            "--backend",
            "wayland",
            "--sharpness",
            "20",
            "[redacted]",
            "[redacted]",
            "--",
            "/bin/sh",
            "-c"
        ])
        .as_array()
        .unwrap()[..]
    );
    assert!(plan["args"][12].as_str().unwrap().contains("printf game"));
    assert!(!stderr.contains("private value"));
    assert!(!stderr.contains("very-private"));
}

#[test]
fn plugin_launch_records_native_words_and_final_environment_before_exec() {
    let root = tempfile::tempdir().unwrap();
    let (command, _, _) = setup(root.path(), "{enabled: true, settings: {nativeFailure: true}, config: {append: '--native private-value'}}");
    let diagnostics = root.path().join("diagnostics");
    fs::create_dir(&diagnostics).unwrap();
    let result = cli(&command)
        .env("KORRI_LAUNCH_DIAGNOSTICS_DIRECTORY", &diagnostics)
        .env(
            "KORRI_LAUNCH_DIAGNOSTICS_READER_UID",
            (unsafe { libc::geteuid() } + 1).to_string(),
        )
        .env("INHERITED_SECRET", "inherited-private")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(
        stderr.contains("native startup failure very-private inherited-private"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("cannot record launch redaction"),
        "{stderr}"
    );
    let words: Vec<String> =
        serde_json::from_slice(&fs::read(diagnostics.join("redaction-words.json")).unwrap())
            .unwrap();
    for private in [
        "--native",
        "private-value",
        "very-private",
        "inherited-private",
    ] {
        assert!(words.contains(&private.to_string()), "{words:?}");
    }
    assert!(
        !words.contains(&"must disappear".to_string()),
        "envUnset is not in the final environment"
    );
}

#[test]
fn snapshot_lists_installed_modifiers_without_authored_values_and_validates_optional_schemas() {
    use korrid::launch_settings::{
        self as ls, LaunchLayer, LaunchPath, LaunchPlace, LaunchRunnerScope, LaunchSetRequest,
        LaunchSource,
    };
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    readable::install(
        root.path(),
        "{}",
        &format!(
            "{}    profiles: {{quiet: {{title: Quiet}}}}\n",
            readable::gba_library()
        ),
    );
    let described = package(
        root.path(),
        "@test:described",
        r#"export const name='described';export const title='Described wrapper';export const modifiers={main:{id:'@test:described/main'}};export const handlers={'settings.describe':()=>({revision:'modifier-rev-1',schema:{properties:{width:{type:'integer',minimum:1,maximum:100,default:50}}}}),'launch.compose':input=>input.plan};"#,
        None,
    );
    let no_schema = package(
        root.path(),
        "@test:plain",
        r#"export const name='plain';export const title='Plain wrapper';export const modifiers={main:{id:'@test:plain/main'}};export const handlers={'launch.compose':input=>input.plan};"#,
        None,
    );
    let registry = PluginRegistry::from_installed(vec![described, no_schema]).unwrap();
    let client = ls::helper::Client {
        socket: root.path().join("absent.sock"),
    };
    let snapshot = ls::snapshot(
        root.path(),
        Some(&registry),
        Some(readable::GBA_ID),
        &client,
    )
    .unwrap();
    assert_eq!(snapshot.modifiers.len(), 2);
    assert_eq!(snapshot.modifiers[0].title, "Described wrapper");
    assert_eq!(
        snapshot.modifiers[0].revision.as_deref(),
        Some("modifier-rev-1")
    );
    assert_eq!(
        snapshot.modifiers[0].schema.as_ref().unwrap()["properties"]["width"]["maximum"],
        100
    );
    assert!(snapshot.modifiers[1].schema.is_none());
    assert!(snapshot.modifiers[1].revision.is_none());
    assert!(matches!(
        snapshot.resolved.values["modifiers.@test:described/main.settings.width"].source,
        Some(LaunchSource::PluginDefault {ref plugin_id}) if plugin_id=="@test:described"
    ));
    assert!(snapshot
        .places
        .iter()
        .all(|v| v.body.get("modifiers").is_none()));
    let lock = std::sync::Mutex::new(());
    let mut request = LaunchSetRequest {
        clear: false,
        place: LaunchPlace {
            layer: LaunchLayer::Profile {
                game_id: readable::GBA_ID.into(),
                profile_id: "quiet".into(),
            },
            scope: LaunchRunnerScope::Any,
        },
        path: LaunchPath::ModifierEnabled {
            id: "@test:described/main".into(),
        },
        value: Some(serde_json::json!(true)),
        expected_revision: snapshot.revisions.games,
    };
    let revisions = ls::set_with_registry(
        root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&registry),
    )
    .unwrap();
    request.expected_revision = revisions.games;
    request.path = LaunchPath::ModifierSetting {
        id: "@test:described/main".into(),
        key: "width".into(),
    };
    request.value = Some(serde_json::json!(101));
    let before = fs::read(root.path().join("catalog/library.yaml")).unwrap();
    assert!(ls::set_with_registry(
        root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&registry)
    )
    .is_err());
    assert_eq!(
        fs::read(root.path().join("catalog/library.yaml")).unwrap(),
        before
    );
    request.value = Some(serde_json::json!(75));
    let revisions = ls::set_with_registry(
        root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&registry),
    )
    .unwrap();
    request.expected_revision = revisions.games;
    request.path = LaunchPath::ModifierSetting {
        id: "@test:plain/main".into(),
        key: "pluginOwnedKey".into(),
    };
    request.value = Some(serde_json::json!("native"));
    ls::set_with_registry(
        root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&registry),
    )
    .unwrap();
    let uninstalled=PluginRegistry::new(vec![load_plugin_source("@test","export const name='uninstalled';export const modifiers={main:{id:'@test:uninstalled/main'}};").unwrap()],vec!["@test:uninstalled".into()]).unwrap();
    assert!(ls::snapshot(root.path(), Some(&uninstalled), None, &client)
        .unwrap()
        .modifiers
        .is_empty());
}

#[test]
fn nested_services_and_nonchoosable_runner_snapshots_use_plugin_schemas_and_render() {
    use korrid::launch_settings::{
        self as ls, LaunchLayer, LaunchPath, LaunchPlace, LaunchRunnerScope, LaunchSetRequest,
        LaunchSource,
    };
    use serde_json::json;
    let root = tempfile::tempdir().unwrap();
    let sunshine = "@korri:sunshine/sunshine";
    let moonlight = "@korri:moonlight/moonlight";
    readable::install(root.path(), &json!({"host":{"services":{sunshine:{"settings":{"video":{"max-bitrate-kbps":3000}}}},"runners":{moonlight:{"settings":{"video":{"fps":30}}}}}}).to_string(), &readable::gba_library());
    let library_path = root.path().join("catalog/library.yaml");
    let mut library: serde_json::Value =
        serde_yaml::from_str(&fs::read_to_string(&library_path).unwrap()).unwrap();
    library["library"][readable::GBA_ID]["runners"] =
        json!({moonlight:{"settings":{"video":{"fps":45}}}});
    library["library"][readable::GBA_ID]["profiles"] =
        json!({"quiet":{"title":"Quiet","runners":{moonlight:{"settings":{"video":{"fps":90}}}}}});
    library["library"][readable::GBA_ID]["profile"] = json!("quiet");
    fs::write(library_path, serde_yaml::to_string(&library).unwrap()).unwrap();
    let mut document: serde_json::Value =
        serde_yaml::from_str(&fs::read_to_string(root.path().join("device.yaml")).unwrap())
            .unwrap();
    let locations: serde_json::Value =
        serde_yaml::from_str(&readable::gba_locations("roms", "wl4.gba", false)).unwrap();
    document["locations"] = locations["locations"].clone();
    fs::write(
        root.path().join("device.yaml"),
        serde_yaml::to_string(&document).unwrap(),
    )
    .unwrap();
    fs::create_dir(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let schema = r#"{type:'object',additionalProperties:false,properties:{video:{type:'object',additionalProperties:false,properties:{'max-bitrate-kbps':{type:'integer'},codec:{type:'string',default:'h264'},fps:{type:'integer'}}}}}"#;
    let sun = package(
        root.path(),
        "@korri:sunshine",
        &format!(
            r#"export const name='sunshine'; export const title='Sunshine';export const handlers={{'settings.describe':input=>{{if(input.id !== '@korri:sunshine/sunshine')throw Error('wrong id');return {{schema:{schema},revision:'sun-v1'}};}},'settings.validate':input=>({{valid:true,diagnostics:[]}}),'settings.render':input=>({{args:['max_bitrate='+input.settings.video['max-bitrate-kbps']]}})}};"#
        ),
        None,
    );
    let moon = package(
        root.path(),
        "@korri:moonlight",
        &format!(
            r#"export const name='moonlight';export const runners={{moonlight:{{id:'@korri:moonlight/moonlight',program:'program'}}}};export const handlers={{'settings.describe':()=>({{schema:{schema},revision:'moon-v1'}}),'settings.validate':input=>({{valid:input.values.video?.fps!==999,diagnostics:input.values.video?.fps===999?[{{severity:'error',code:'fps',path:['video','fps'],message:'unsupported fps'}}]:[]}}),'settings.render':input=>({{args:['-fps',String(input.settings.video.fps),'-width',String(input.display.width)]}})}};"#
        ),
        None,
    );
    let runner = package(root.path(), "@korri:mgba", RUNNER, None);
    let registry = PluginRegistry::from_installed(vec![sun, moon, runner]).unwrap();
    let client = ls::helper::Client {
        socket: root.path().join("absent.sock"),
    };
    for game in [None, Some(readable::GBA_ID)] {
        let snapshot = ls::snapshot(root.path(), Some(&registry), game, &client).unwrap();
        assert!(
            !snapshot
                .runners
                .iter()
                .find(|r| r.id == moonlight)
                .unwrap()
                .choosable
        );
        let projection = snapshot
            .runner_resolved
            .iter()
            .find(|r| r.runner_id == moonlight)
            .unwrap();
        assert_eq!(
            projection.values["settings.video.fps"].value,
            if game.is_some() { 90 } else { 30 }
        );
        assert!(
            matches!(projection.values["settings.video.codec"].source,Some(LaunchSource::PluginDefault {ref plugin_id}) if plugin_id=="@korri:moonlight")
        );
        assert!(!snapshot.resolved.values.contains_key("settings.video.fps"));
        assert_eq!(snapshot.services.len(), 1);
        assert_eq!(snapshot.services[0].id, sunshine);
        assert_eq!(snapshot.services[0].revision.as_deref(), Some("sun-v1"));
        assert!(snapshot
            .places
            .iter()
            .all(|p| p.editable_kinds.contains(&"services".into())));
        assert!(snapshot
            .places
            .iter()
            .any(|p| matches!(&p.place.scope,LaunchRunnerScope::Runner(id) if id==moonlight)));
        assert!(matches!(
            snapshot.resolved.values[&format!("services.{sunshine}.settings.video.codec")].source,
            Some(LaunchSource::PluginDefault { .. })
        ));
        assert_eq!(
            snapshot.resolved.values
                [&format!("services.{sunshine}.settings.video.max-bitrate-kbps")]
                .value,
            3000
        );
    }
    let private = tempfile::tempdir().unwrap();
    let lock = std::sync::Mutex::new(());
    let mut request = LaunchSetRequest {
        clear: false,
        place: LaunchPlace {
            layer: LaunchLayer::Game(readable::GBA_ID.into()),
            scope: LaunchRunnerScope::Any,
        },
        path: LaunchPath::Runner,
        value: Some(json!(moonlight)),
        expected_revision: korrid::config::settings::runner_choice_revisions(root.path())
            .unwrap()
            .games,
    };
    assert!(ls::set_with_registry(
        root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&registry)
    )
    .unwrap_err()
    .to_string()
    .contains("cannot be chosen"));
    request.path = LaunchPath::ServiceSetting {
        id: sunshine.into(),
        key: "video.max-bitrate-kbps".into(),
    };
    request.value = Some(json!(5000));
    let revisions = ls::set_with_registry(
        root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&registry),
    )
    .unwrap();
    request.expected_revision = revisions.games;
    let config = korrid::config::settings::runner_choice_snapshot(root.path())
        .unwrap()
        .0;
    let route = korrid::config::resolver::resolve_linux_route(
        root.path(),
        &config,
        &registry,
        readable::GBA_ID,
        None,
    )
    .unwrap();
    let launch = launch_route(
        root.path(),
        &config,
        &registry,
        &route,
        None,
        None,
        "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
    )
    .unwrap();
    let composition: LaunchComposition = serde_json::from_str(&launch.command[4]).unwrap();
    assert_eq!(composition.sunshine_args.unwrap(), ["max_bitrate=5000"]);
    request.value = None;
    request.clear = true;
    let revisions = ls::set_with_registry(
        root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&registry),
    )
    .unwrap();
    request.expected_revision = revisions.games;
    let snapshot = ls::snapshot(
        root.path(),
        Some(&registry),
        Some(readable::GBA_ID),
        &client,
    )
    .unwrap();
    assert!(!snapshot.resolved.values.contains_key(&format!(
        "services.{sunshine}.settings.video.max-bitrate-kbps"
    )));
    request.clear = false;
    ls::set_with_registry(
        root.path(),
        private.path(),
        &lock,
        &request,
        &client,
        Some(&registry),
    )
    .unwrap();
    let snapshot = ls::snapshot(
        root.path(),
        Some(&registry),
        Some(readable::GBA_ID),
        &client,
    )
    .unwrap();
    assert_eq!(
        snapshot.resolved.values[&format!("services.{sunshine}.settings.video.max-bitrate-kbps")]
            .value,
        3000
    );
    assert!(korrid::config::resolver::resolve_linux_route(
        root.path(),
        &config,
        &registry,
        readable::GBA_ID,
        Some(moonlight)
    )
    .is_err());
    let library_path = root.path().join("catalog/library.yaml");
    let mut library: serde_json::Value =
        serde_yaml::from_str(&fs::read_to_string(&library_path).unwrap()).unwrap();
    library["library"][readable::GBA_ID]["profiles"]["quiet"]["runners"][moonlight]["settings"]
        ["video"]["fps"] = json!(999);
    fs::write(library_path, serde_yaml::to_string(&library).unwrap()).unwrap();
    let snapshot = ls::snapshot(
        root.path(),
        Some(&registry),
        Some(readable::GBA_ID),
        &client,
    )
    .unwrap();
    let projection = snapshot
        .runner_resolved
        .iter()
        .find(|r| r.runner_id == moonlight)
        .unwrap();
    assert!(!projection.values.contains_key("settings.video.fps"));
    assert!(snapshot.notices.iter().any(|n|matches!(n,ls::LaunchNotice::SettingSkipped {path:LaunchPath::Setting {key},runner_id:Some(id),message} if id==moonlight && key=="video.fps" && message=="unsupported fps")));
}

#[test]
fn compose_validation_drops_error_paths_but_keeps_other_nested_leaves() {
    let root = tempfile::tempdir().unwrap();
    let runner = package(root.path(), "@korri:mgba", RUNNER, None);
    let modifier = package(
        root.path(),
        "@test:nested",
        r#"export const name='nested';export const modifiers={main:{id:'@test:nested/main'}};export const handlers={'settings.describe':()=>({revision:'one',schema:{}}),'settings.validate':input=>({valid:false,diagnostics:[{severity:'error',code:'bad',path:['video','fps'],message:'bad fps'}]}),'launch.compose':input=>({...input.plan,args:[JSON.stringify(input.settings)]})};"#,
        None,
    );
    let input:PluginLaunchInput=serde_json::from_value(serde_json::json!({"runnerId":"@korri:mgba/mgba","program":"/bin/sh","accountRoot":"/account","saveRoot":"/account/saves/game","cacheRoot":"/cache","files":{},"artifacts":{}})).unwrap();
    let composition:LaunchComposition=serde_json::from_value(serde_json::json!({"modifiers":[{"modifierId":"@test:nested/main","manifest":modifier.package.join("manifest.json"),"entry":"plugin.ts","settings":{"video":{"fps":999,"codec":"h264"}},"config":{}}]})).unwrap();
    let plan = prepare(&runner.package.join("plugin.ts"), &input, &composition).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&plan.args[0]).unwrap(),
        serde_json::json!({"video":{"codec":"h264"}})
    );
}
