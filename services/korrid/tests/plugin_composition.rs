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
        "{}\nmodifiers: {{'@test:wrapper/main': {record}}}",
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
    let spec = launch_route(root, &snapshot, &registry, &route, None, None).unwrap();
    let input = serde_json::from_str(&spec.command[3]).unwrap();
    let composition = serde_json::from_str(&spec.command[4]).unwrap();
    (spec.command, input, composition)
}
fn cli(command: &[String]) -> Command {
    let mut process = Command::new(env!("CARGO_BIN_EXE_korrid"));
    process
        .env_clear()
        .args(&command[1..])
        .env("REMOVED", "must disappear");
    process
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
        "locations:\n  '@korri:steam/401710': [{provider: '@korri:steam', ref: '401710'}]\nmodifiers: {'@test:wrapper/main': {enabled: true}}\n",
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
    let spec = launch_route(root.path(), &snapshot, &registry, &route, None, None).unwrap();
    assert_eq!(spec.command[1], "plugin-launch");
    let input: PluginLaunchInput = serde_json::from_str(&spec.command[3]).unwrap();
    assert!(input.content_path.is_none());
    assert_eq!(input.provider_ref.as_deref(), Some("401710"));
    let composition: LaunchComposition = serde_json::from_str(&spec.command[4]).unwrap();
    assert_eq!(composition.modifiers.len(), 1);
    let plan = prepare(Path::new(&spec.command[2]), &input, &composition).unwrap();
    assert_eq!(plan.args[3..], ["/bin/sh", "401710"]);
    let home = root.path().join("users/default/steam");
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
        let result = cli(&command).output().unwrap();
        let reason = String::from_utf8_lossy(&result.stderr);
        assert!(!result.status.success());
        assert!(reason.contains(expected), "{reason}");
        assert!(reason.contains("@test:wrapper/main"), "{reason}");
        assert!(!reason.contains("never-publish"), "{reason}");
        assert!(!Path::new(&input.account_root).exists());
    }
    for remove in ["manifest.json", "plugin.ts", "compose.ts"] {
        let root = tempfile::tempdir().unwrap();
        let (command, input, _) = setup(root.path(), "{enabled: true}");
        fs::remove_file(root.path().join("wrapper").join(remove)).unwrap();
        let result = cli(&command).output().unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("@test:wrapper/main"));
        assert!(!Path::new(&input.account_root).exists());
    }
    let root = tempfile::tempdir().unwrap();
    let (command, input, _) = setup(root.path(), "{enabled: true}");
    fs::write(root.path().join("wrapper/manifest.json"), r#"{"entry":"plugin.ts","sources":["compose.ts","plugin.ts"],"files":{"program":"/no/such/modifier-file"}}"#).unwrap();
    let result = cli(&command).output().unwrap();
    assert!(String::from_utf8_lossy(&result.stderr).contains("modifier file program is missing"));
    assert!(!Path::new(&input.account_root).exists());
}
#[test]
fn refused_compose_reports_its_reason_through_native_systemd_status() {
    let root = tempfile::tempdir().unwrap();
    let (command, _, _) = setup(root.path(), "{enabled: true, settings: {fail: true}}");
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
        status.starts_with("STATUS=plugin-launch: modifier @test:wrapper/main:"),
        "{status}"
    );
    assert!(status.contains("compose refused"), "{status}");
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
            "{}\nmodifiers: {{'{id}': {{enabled: {enabled}}}}}",
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
        let result = launch_route(root.path(), &snapshot, &registry, &route, None, None);
        assert_eq!(result.is_ok(), success, "{id}: {result:?}");
        if let Err(error) = result {
            assert!(error.to_string().contains(id));
        }
    }
    let (command, _, composition) = setup(root.path(), "{settings: {width: 1240}}");
    assert!(composition.modifiers.is_empty(), "absent enabled is off");
    let result = cli(&command).output().unwrap();
    assert!(result.status.success());
    assert!(!root.path().join("users/default/wrapped").exists());
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
        "{}\nmodifiers:\n  '@test:z/main': {{enabled: true}}\n  '@test:a/main': {{enabled: true}}",
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
    let spec = launch_route(root.path(), &snapshot, &registry, &route, None, None).unwrap();
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
        let result = cli(&command).output().unwrap();
        let reason = String::from_utf8_lossy(&result.stderr);
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
    let result = cli(&command).output().unwrap();
    let reason = String::from_utf8_lossy(&result.stderr);
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
    let result = cli(&command).output().unwrap();
    let reason = String::from_utf8_lossy(&result.stderr);
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
