use super::*;
use crate::config::{
    decode_config_documents,
    resolver::ResolvedRoute,
    settings::{self, SettingsError},
};
use serde_json::json;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::Mutex,
    thread,
};
const RUNNER: &str = "@korri:test/runner";
const FAMILY: &str = "@korri:test";
const DEVICE:&str="host: {cpu: {min: 10, max: 95, governor: schedutil}, gpu: {max: 99}, runners: {'@korri:test/runner': {gpu: {min: 15}}}}\nsystems: {gba: {cpu: {max: 85}}}\n";
const LIBRARY:&str="library:\n  game:\n    title: Game\n    cpu: {min: 20}\n    releases: [{id: gba, system: gba, identity: {kind: provider, value: '@korri:content/one'}, gpu: {max: 80}}]\n    profiles:\n      quiet: {title: Quiet, cpu: {max: 60}}\n    profile: quiet\n";
fn root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("catalog")).unwrap();
    fs::write(root.path().join("device.yaml"), DEVICE).unwrap();
    fs::write(root.path().join("catalog/library.yaml"), LIBRARY).unwrap();
    root
}
fn unavailable(root: &std::path::Path) -> helper::Client {
    helper::Client {
        socket: root.join("absent.sock"),
    }
}
fn route() -> ResolvedRoute {
    let c = decode_config_documents(DEVICE, LIBRARY).unwrap();
    let mut r = resolution::context(&c, Some("game"));
    r.runner_id = RUNNER.into();
    r.family_id = Some(FAMILY.into());
    r
}
pub(super) fn facts() -> LaunchDeviceFacts {
    serde_json::from_value(json!({"defaults":{"cpu":{"min":0,"max":61},"gpu":{"min":0,"max":100},"governor":"schedutil"},"cpuPolicies":[{"name":"policy0","cpus":[0,1],"cpuinfoMaxFreq":1800000,"availableFrequencies":[408000,1800000],"availableGovernors":["schedutil","performance"]}],"gpuDevfreq":[]})).unwrap()
}
fn req(
    place: LaunchPlace,
    path: LaunchPath,
    value: Option<serde_json::Value>,
    revision: String,
) -> LaunchSetRequest {
    LaunchSetRequest {
        place,
        path,
        value,
        expected_revision: revision,
    }
}
fn any(layer: LaunchLayer) -> LaunchPlace {
    LaunchPlace {
        layer,
        scope: LaunchRunnerScope::Any,
    }
}

#[test]
fn one_order_table_covers_every_place_for_every_kind() {
    fn body(n: u8) -> serde_json::Value {
        json!({"cpu":{"min":n},"gpu":{"min":n},"env":{"ORDER":n.to_string()},"args":{"prepend":[n.to_string()],"append":[n.to_string()]},"hooks":{"before":[{"run":n.to_string()}],"after":[{"run":n.to_string()}]},"modifiers":{"@korri:test.foo/wrapper":{"enabled":true,"settings":{"order":n}}}})
    }
    fn layer(start: u8) -> serde_json::Value {
        let mut v = body(start);
        let mut family = body(start + 1);
        family["settings"] = json!({"order":start+1});
        let mut runner = body(start + 2);
        runner["settings"] = json!({"order":start+2});
        v["families"] = json!({FAMILY:family});
        v["runners"] = json!({RUNNER:runner});
        v
    }
    let mut host = layer(1);
    host["families"][FAMILY] = body(3);
    host["runners"][RUNNER] = body(4);
    let mut system = layer(2);
    system["families"][FAMILY] = body(5);
    system["runners"][RUNNER] = body(6);
    let mut game = layer(7);
    let mut release = layer(10);
    release["id"] = json!("gba");
    release["system"] = json!("gba");
    release["identity"] = json!({"kind":"provider","value":"@korri:content/one"});
    game["releases"] = json!([release]);
    let mut profile = layer(13);
    profile["title"] = json!("Quiet");
    game["profiles"] = json!({"quiet":profile});
    game["profile"] = json!("quiet");
    let snapshot = decode_config_documents(
        &json!({"host":host,"systems":{"gba":system}}).to_string(),
        &json!({"library":{"game":game}}).to_string(),
    )
    .unwrap();
    let one: crate::config::place::PlaceBody = serde_json::from_value(layer(16)).unwrap();
    let route = route();
    let cells = launch::cells(&snapshot, &route, Some(&one));
    assert_eq!(cells.len(), 18);
    assert_eq!(
        cells
            .iter()
            .map(|v| v.values.env["ORDER"].clone().unwrap())
            .collect::<Vec<_>>(),
        (1..=18).map(|n| n.to_string()).collect::<Vec<_>>()
    );
    let resolved = resolve_launch(&snapshot, &route, None, None).unwrap();
    assert_eq!(resolved.env["ORDER"].as_deref(), Some("15"));
    assert_eq!(
        resolved.args.prepend,
        (1..=15).map(|n| n.to_string()).collect::<Vec<_>>()
    );
    assert_eq!(resolved.args.prepend, resolved.args.append);
    assert_eq!(
        resolved
            .hooks
            .before
            .iter()
            .map(|s| s.run.0.clone())
            .collect::<Vec<_>>(),
        resolved.args.prepend
    );
    assert_eq!(
        resolved
            .hooks
            .after
            .iter()
            .map(|s| s.run.0.clone())
            .collect::<Vec<_>>(),
        resolved.args.prepend
    );
    assert_eq!(resolved.limits.cpu.min.value, 15);
    assert_eq!(resolved.limits.gpu.min.value, 15);
    assert_eq!(
        serde_json::to_value(&resolved.runner.settings["order"]).unwrap(),
        15
    );
    let value = &resolved.values.values["modifiers.@korri:test.foo/wrapper.settings.order"];
    assert!(
        matches!(&value.path,LaunchValuePath::Editable(LaunchPath::ModifierSetting{id,key}) if id=="@korri:test.foo/wrapper" && key=="order")
    );
    assert!(
        matches!(&value.source,Some(LaunchSource::Place(LaunchPlace {layer:LaunchLayer::Profile{..},scope:LaunchRunnerScope::Runner(id)})) if id==RUNNER)
    );
}
#[test]
fn routines_expand_before_inline_steps_and_each_kind_keeps_its_merge_rule() {
    let device=json!({"routines":{"setup":{"before":[{"run":"routine"}],"after":[{"run":"cleanup"}]}},"host":{"env":{"KEEP":"yes","REMOVE":"old"},"cwd":"/device","hooks":{"use":["setup"],"before":[{"run":"inline"}]},"runners":{RUNNER:{"settings":{"a":1,"b":2},"config":{"prepend":"first","append":"first"},"modifiers":{"@korri:test.foo/wrapper":{"enabled":true,"settings":{"a":1,"b":2},"config":{"append":"first"}}}}}}}).to_string();
    let mut library: serde_json::Value = serde_yaml::from_str(LIBRARY).unwrap();
    let game = &mut library["library"]["game"];
    game["env"] = json!({"REMOVE":null,"ADD":"final"});
    game["cwd"] = json!("/game");
    game["runners"] = json!({RUNNER:{"settings":{"b":3},"config":{"append":"last"},"modifiers":{"@korri:test.foo/wrapper":{"enabled":false,"settings":{"b":4},"config":{"append":"last"}}}}});
    let library = library.to_string();
    let snapshot = decode_config_documents(&device, &library).unwrap();
    let r = resolve_launch(&snapshot, &route(), None, None).unwrap();
    assert_eq!(r.env["REMOVE"], None);
    assert_eq!(r.env["KEEP"].as_deref(), Some("yes"));
    assert_eq!(r.cwd.as_deref(), Some("/game"));
    assert_eq!(
        serde_json::to_value(&r.runner.settings).unwrap(),
        json!({"a":1,"b":3})
    );
    assert_eq!(
        r.runner.config.as_ref().unwrap().prepend.as_deref(),
        Some("first")
    );
    assert_eq!(
        r.runner.config.as_ref().unwrap().append.as_deref(),
        Some("last")
    );
    let m = &r.modifiers["@korri:test.foo/wrapper"];
    assert_eq!(m.enabled, Some(false));
    assert_eq!(
        serde_json::to_value(&m.settings).unwrap(),
        json!({"a":1,"b":4})
    );
    assert_eq!(m.config.as_ref().unwrap().append.as_deref(), Some("last"));
    assert_eq!(
        r.hooks
            .before
            .iter()
            .map(|v| v.run.0.as_str())
            .collect::<Vec<_>>(),
        ["routine", "inline"]
    );
    assert_eq!(r.hooks.after[0].run.0, "cleanup");
}
#[test]
fn runner_choice_falls_back_and_skips_unselected_scope_values() {
    let device=format!("host: {{runner: '{RUNNER}', runners: {{'@korri:missing/main': {{env: {{BAD: wrong}}}}, '{RUNNER}': {{env: {{GOOD: selected}}}}}}}}\nsystems: {{gba: {{runner: '@korri:missing/system'}}}}");
    let library = LIBRARY.replace(
        "    title: Game",
        "    title: Game\n    runner: '@korri:missing/game'",
    );
    let snapshot = decode_config_documents(&device, &library).unwrap();
    let route = route();
    let (selected, notices) = launch::choose_runner(&snapshot, std::slice::from_ref(&route), None);
    assert_eq!(selected.as_deref(), Some(RUNNER));
    assert_eq!(notices.len(), 2);
    assert!(
        matches!(&notices[0],LaunchNotice::RunnerUnavailable {runner_id,place} if runner_id=="@korri:missing/game" && matches!(place.layer,LaunchLayer::Game(_)))
    );
    let resolved = resolve_launch(&snapshot, &route, None, None).unwrap();
    assert!(!resolved.env.contains_key("BAD"));
    assert_eq!(resolved.env["GOOD"].as_deref(), Some("selected"));
    let mut other = route.clone();
    other.runner_id = "@korri:other/main".into();
    let empty = decode_config_documents("{}", LIBRARY).unwrap();
    assert!(launch::choose_runner(&empty, &[route, other], None)
        .0
        .is_none());
}
#[test]
fn cpu_device_any_is_cpufreq_but_device_runner_is_session_and_gpu_inherits_device() {
    let library = LIBRARY
        .replace("cpu: {min: 20}", "cpu: {}")
        .replace("cpu: {max: 60}", "cpu: {}")
        .replace("gpu: {max: 80}", "gpu: {}");
    let config =
        decode_config_documents("host: {cpu: {max: 65}, gpu: {max: 80}}", &library).unwrap();
    let limits =
        session::SessionLimits::for_route(&config, &route(), "build".into(), None).unwrap();
    assert_eq!(limits.cpu.max, 100);
    assert_eq!(limits.gpu.max, 80);
    let config = decode_config_documents(
        &format!("host: {{cpu: {{max: 65}}, runners: {{'{RUNNER}': {{cpu: {{max: 40}}}}}}}}"),
        &library,
    )
    .unwrap();
    let limits =
        session::SessionLimits::for_route(&config, &route(), "build".into(), None).unwrap();
    assert_eq!(limits.cpu.max, 40);
    let config = decode_config_documents("{}", &library).unwrap();
    let mut facts = facts();
    facts.defaults.gpu.max = 55;
    let limits =
        session::SessionLimits::for_route(&config, &route(), "build".into(), Some(&facts)).unwrap();
    assert_eq!(limits.gpu.max, 55);
    assert_eq!(limits.cpu.max, 100);
}
#[test]
fn across_place_range_conflicts_move_only_the_less_specific_side() {
    for (min, max, profile, expected, adjusted) in [
        (80, 90, "max: 40", 40, "cpu.min"),
        (10, 40, "min: 80", 80, "cpu.max"),
    ] {
        let device = format!("host: {{cpu: {{min: {min}, max: {max}}}}}");
        let library = LIBRARY
            .replace("cpu: {min: 20}", "cpu: {}")
            .replace("max: 60", profile);
        let config = decode_config_documents(&device, &library).unwrap();
        let values = resolve_launch(&config, &route(), None, None)
            .unwrap()
            .values
            .values;
        assert_eq!(values["cpu.min"].value, expected);
        assert_eq!(values["cpu.max"].value, expected);
        assert!(values[adjusted].adjusted);
    }
}
#[test]
fn schema_rejects_removed_shapes_bad_ranges_unknown_routines_and_nonhost_governor() {
    for device in [
        "runners: {}",
        "families: {}",
        "modifiers: {}",
        "hooks: {}",
        "host: {argsAppend: []}",
        "host: {hooks: {trust-removable: true}}",
        "host: {preferences: {playerCount: 2}}",
        "host: {gpu: {governor: schedutil}}",
        "systems: {gba: {cpu: {governor: schedutil}}}",
        "host: {hooks: {use: [missing]}}",
        "host: {cpu: {max: 0}}",
        "host: {cpu: {min: 101}}",
        "host: {cpu: {min: 1.5}}",
        "host: {cpu: {min: null}}",
        "host: {cpu: {min: 90, max: 80}}",
        "host: {runners: {'@korri:test/runner': {runner: nope}}}",
    ] {
        assert!(
            decode_config_documents(device, LIBRARY).is_err(),
            "{device}"
        );
    }
    for library in [
        LIBRARY.replace("cpu: {min: 20}", "cpu: {governor: schedutil}"),
        LIBRARY.replace("profile: quiet", "profile: absent"),
    ] {
        assert!(decode_config_documents(DEVICE, &library).is_err());
    }
}
#[test]
fn atomic_set_clears_to_inherit_and_rejects_conflicts_without_mutation() {
    let root = root();
    let private = tempfile::tempdir().unwrap();
    let lock = Mutex::new(());
    let client = unavailable(root.path());
    let before = settings::runner_choice_revisions(root.path()).unwrap();
    let bytes = fs::read(root.path().join("catalog/library.yaml")).unwrap();
    let mut request = req(
        any(LaunchLayer::Game("game".into())),
        LaunchPath::CpuMax,
        Some(json!(10)),
        before.games.clone(),
    );
    assert!(matches!(
        set(root.path(), private.path(), &lock, &request, &client),
        Err(SettingsError::Invalid(_))
    ));
    assert_eq!(
        fs::read(root.path().join("catalog/library.yaml")).unwrap(),
        bytes
    );
    request.value = Some(json!(70));
    let after = set(root.path(), private.path(), &lock, &request, &client).unwrap();
    assert_eq!(before.device, after.device);
    assert_ne!(before.games, after.games);
    assert!(matches!(
        set(root.path(), private.path(), &lock, &request, &client),
        Err(SettingsError::Conflict)
    ));
    request.expected_revision = after.games;
    request.value = None;
    set(root.path(), private.path(), &lock, &request, &client).unwrap();
    assert!(settings::runner_choice_snapshot(root.path())
        .unwrap()
        .0
        .library["game"]
        .cpu
        .as_ref()
        .unwrap()
        .max
        .is_none());
}
#[test]
fn setters_write_every_layer_and_runner_place_to_the_correct_document() {
    let root = root();
    let private = tempfile::tempdir().unwrap();
    let lock = Mutex::new(());
    let client = unavailable(root.path());
    for layer in [
        LaunchLayer::Device,
        LaunchLayer::System("gba".into()),
        LaunchLayer::Game("game".into()),
        LaunchLayer::Release {
            game_id: "game".into(),
            release_id: "gba".into(),
        },
        LaunchLayer::Profile {
            game_id: "game".into(),
            profile_id: "quiet".into(),
        },
    ] {
        for scope in [
            LaunchRunnerScope::Any,
            LaunchRunnerScope::Family(FAMILY.into()),
            LaunchRunnerScope::Runner(RUNNER.into()),
        ] {
            let before = settings::runner_choice_revisions(root.path()).unwrap();
            let device = matches!(layer, LaunchLayer::Device | LaunchLayer::System(_));
            let revision = if device {
                before.device.clone()
            } else {
                before.games.clone()
            };
            let after = set(
                root.path(),
                private.path(),
                &lock,
                &req(
                    LaunchPlace {
                        layer: layer.clone(),
                        scope,
                    },
                    LaunchPath::GpuMin,
                    Some(json!(22)),
                    revision,
                ),
                &client,
            )
            .unwrap();
            assert_eq!(after.device == before.device, !device);
            assert_eq!(after.games == before.games, device);
        }
    }
    let snapshot = settings::runner_choice_snapshot(root.path()).unwrap().0;
    assert!(launch::cells(&snapshot, &route(), None).iter().all(|v| v
        .values
        .gpu
        .as_ref()
        .unwrap()
        .min
        == Some(22)));
}
#[test]
fn override_and_set_wire_refuse_every_file_only_kind_and_removed_rpc() {
    for key in [
        "env", "args", "cwd", "hooks", "routines", "config", "runner", "unknown",
    ] {
        assert!(
            serde_json::from_value::<LaunchOverride>(json!({key:{}})).is_err(),
            "{key}"
        );
    }
    for path in ["Env", "Args", "Cwd", "Hooks", "Config"] {
        assert!(serde_json::from_value::<LaunchSetRequest>(json!({"place":{"layer":{"_tag":"Device"},"scope":{"_tag":"Any"}},"path":{"_tag":path},"value":{},"expectedRevision":"rev"})).is_err());
    }
    assert!(serde_json::from_value::<crate::RpcRequest>(
        json!({"_tag":"app.local-games.runner.set","payload":{}})
    )
    .is_err());
    assert!(serde_json::from_value::<crate::SessionStartRequest>(
        json!({"gameId":"game","expectedLaunchId":"id","overrides":{}})
    )
    .is_err());
    let config = decode_config_documents(DEVICE, LIBRARY).unwrap();
    let one:LaunchOverride=serde_json::from_value(json!({"cpu":{"max":45},"settings":{"scope":{"_tag":"Runner","id":RUNNER},"values":{"speed":2}}})).unwrap();
    let resolved = resolve_launch(&config, &route(), Some(&one), None).unwrap();
    assert_eq!(resolved.limits.cpu.max.value, 45);
    assert_eq!(
        serde_json::to_value(&resolved.runner.settings["speed"]).unwrap(),
        2
    );
}
#[test]
fn runner_property_validation_uses_schema_type_enum_and_range() {
    let schema = json!({"type":"integer","minimum":1,"maximum":4});
    assert!(store::validate_property(&schema, &json!(2)).is_ok());
    for v in [json!(0), json!(5), json!(1.5), json!("2")] {
        assert!(store::validate_property(&schema, &v).is_err());
    }
    assert!(
        store::validate_property(&json!({"type":"string","enum":["a","b"]}), &json!("c")).is_err()
    );
}
pub(super) fn server(
    root: &std::path::Path,
    replies: Vec<serde_json::Value>,
) -> (helper::Client, thread::JoinHandle<Vec<serde_json::Value>>) {
    let socket = root.join("limits.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let handle = thread::spawn(move || {
        replies
            .into_iter()
            .map(|reply| {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();
                stream.write_all(format!("{reply}\n").as_bytes()).unwrap();
                serde_json::from_str(&line).unwrap()
            })
            .collect()
    });
    (helper::Client { socket }, handle)
}
#[test]
fn defaults_are_not_authored_and_apply_device_does_not_use_session_cpu() {
    let root = tempfile::tempdir().unwrap();
    settings::runner_choice_snapshot(root.path()).unwrap();
    let describe = json!({"_tag":"Ok","payload":facts()});
    let (client, handle) = server(
        root.path(),
        vec![
            describe.clone(),
            describe,
            json!({"_tag":"Ok","payload":{}}),
        ],
    );
    let snapshot = snapshot(root.path(), None, None, &client).unwrap();
    assert_eq!(snapshot.resolved.values["cpu.max"].value, 61);
    assert!(matches!(
        snapshot.resolved.values["cpu.max"].source,
        Some(LaunchSource::DeviceDefault)
    ));
    assert!(snapshot.places[0].body.get("cpu").is_none());
    resolution::apply_device(root.path(), &client).unwrap();
    let requests = handle.join().unwrap();
    assert_eq!(
        requests[2],
        json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":0,"max":61},"gpu":{"min":0,"max":100},"governor":"schedutil"}})
    );
    let snapshot = super::snapshot(root.path(), None, None, &unavailable(root.path())).unwrap();
    assert!(snapshot.device_facts.is_none());
    assert_eq!(snapshot.resolved.values["cpu.max"].value, 100);
}
#[test]
fn helper_apply_end_and_warning_use_the_actual_socket_protocol() {
    let root = tempfile::tempdir().unwrap();
    let (client, handle) = server(
        root.path(),
        vec![
            json!({"_tag":"Ok","payload":{}}),
            json!({"_tag":"Ok","payload":{}}),
            json!({"_tag":"Err","payload":{"message":"write refused"}}),
        ],
    );
    let limits = session::SessionLimits {
        cpu: LaunchPercentRange { min: 0, max: 40 },
        gpu: LaunchPercentRange { min: 20, max: 80 },
        runner_id: RUNNER.into(),
        build: "build".into(),
    };
    let id = "0123456789abcdef0123456789abcdef";
    assert!(limits.apply(&client, id).is_none());
    client.end_session(&session::unit(id)).unwrap();
    assert!(limits
        .apply(&client, id)
        .unwrap()
        .message
        .contains("write refused"));
    let requests = handle.join().unwrap();
    assert_eq!(requests[0]["_tag"], "ApplySession");
    assert_eq!(requests[0]["payload"]["cpu"]["max"], 40);
    assert_eq!(requests[1]["_tag"], "EndSession");
    assert!(limits.apply(&unavailable(root.path()), id).is_some());
}
#[test]
fn kiosk_and_owner_authority_cover_set_and_profile_rpcs() {
    use crate::{
        authorization::{self, AuthorizationContext, Principal},
        RpcRequest,
    };
    for (tag, payload) in [
        ("app.launch-settings.snapshot", json!({})),
        (
            "app.launch-settings.set",
            json!({"place":{"layer":{"_tag":"Device"},"scope":{"_tag":"Any"}},"path":{"_tag":"CpuMax"},"value":60,"expectedRevision":"rev"}),
        ),
        (
            "app.launch-settings.profile.save",
            json!({"gameId":"game","title":"Quiet","expectedRevision":"rev"}),
        ),
        (
            "app.launch-settings.profile.remove",
            json!({"gameId":"game","profileId":"quiet","expectedRevision":"rev"}),
        ),
        (
            "app.launch-settings.profile.select",
            json!({"gameId":"game","expectedRevision":"rev"}),
        ),
    ] {
        let request: RpcRequest =
            serde_json::from_value(json!({"_tag":tag,"payload":payload})).unwrap();
        assert_eq!(
            authorization::policy_for(&request),
            authorization::PeerPolicy::OwnerDeviceOnly
        );
        assert!(authorization::authorize(&AuthorizationContext::LocalBrowser, &request).is_ok());
        assert!(authorization::authorize(
            &AuthorizationContext::Peer(Principal::Unknown {
                device_public_key: "peer".into()
            }),
            &request
        )
        .is_err());
        assert_eq!(
            authorization::is_security_mutation(&request),
            tag != "app.launch-settings.snapshot"
        );
    }
}

#[test]
fn launch_settings_profile_create_rename_select_remove_and_remembered_pick() {
    let root = root();
    let private = tempfile::tempdir().unwrap();
    let lock = Mutex::new(());
    let mut revision = settings::runner_choice_revisions(root.path())
        .unwrap()
        .games;
    for _ in 0..2 {
        revision = save_profile(
            root.path(),
            private.path(),
            &lock,
            &LaunchProfileSaveRequest {
                game_id: "game".into(),
                profile_id: None,
                title: "  My Quiet  ".into(),
                expected_revision: revision,
            },
        )
        .unwrap()
        .games;
    }
    let snap = settings::runner_choice_snapshot(root.path()).unwrap().0;
    assert!(snap.library["game"].profiles.contains_key("my-quiet"));
    assert!(snap.library["game"].profiles.contains_key("my-quiet-2"));
    revision = select_profile(
        root.path(),
        private.path(),
        &lock,
        &LaunchProfileSelectRequest {
            game_id: "game".into(),
            profile_id: Some("my-quiet".into()),
            expected_revision: revision,
        },
    )
    .unwrap()
    .games;
    revision = save_profile(
        root.path(),
        private.path(),
        &lock,
        &LaunchProfileSaveRequest {
            game_id: "game".into(),
            profile_id: Some("my-quiet".into()),
            title: "Renamed".into(),
            expected_revision: revision,
        },
    )
    .unwrap()
    .games;
    let snap = settings::runner_choice_snapshot(root.path()).unwrap().0;
    assert_eq!(snap.library["game"].profile.as_ref().unwrap().0, "my-quiet");
    assert_eq!(snap.library["game"].profiles["my-quiet"].title.0, "Renamed");
    let req = LaunchProfileRemoveRequest {
        game_id: "game".into(),
        profile_id: "my-quiet".into(),
        expected_revision: revision,
    };
    revision = remove_profile(root.path(), private.path(), &lock, &req)
        .unwrap()
        .games;
    assert!(matches!(
        remove_profile(root.path(), private.path(), &lock, &req),
        Err(SettingsError::Conflict)
    ));
    let snap = settings::runner_choice_snapshot(root.path()).unwrap().0;
    assert!(snap.library["game"].profile.is_none());
    assert!(!snap.library["game"].profiles.contains_key("my-quiet"));
    assert!(select_profile(
        root.path(),
        private.path(),
        &lock,
        &LaunchProfileSelectRequest {
            game_id: "game".into(),
            profile_id: Some("absent".into()),
            expected_revision: revision.clone()
        }
    )
    .is_err());
    select_profile(
        root.path(),
        private.path(),
        &lock,
        &LaunchProfileSelectRequest {
            game_id: "game".into(),
            profile_id: None,
            expected_revision: revision,
        },
    )
    .unwrap();
}

#[test]
fn launch_settings_pending_publication_blocks_every_mutation_without_rewriting_files() {
    let root = root();
    let private = tempfile::tempdir().unwrap();
    let lock = Mutex::new(());
    let client = unavailable(root.path());
    let before = settings::runner_choice_revisions(root.path()).unwrap();
    let device = fs::read_to_string(root.path().join("device.yaml")).unwrap();
    let library = fs::read_to_string(root.path().join("catalog/library.yaml")).unwrap();
    let journal = private.path().join("game-discovery");
    fs::create_dir(&journal).unwrap();
    fs::write(journal.join("repair.json"),json!({"pending_write":{"expected":{"device":device,"library":library},"candidate":{"device":"{}","library":"{}"},"ownership":{}}}).to_string()).unwrap();
    assert!(matches!(
        set(
            root.path(),
            private.path(),
            &lock,
            &LaunchSetRequest {
                place: any(LaunchLayer::Game("game".into())),
                path: LaunchPath::CpuMax,
                value: Some(json!(50)),
                expected_revision: before.games.clone()
            },
            &client
        ),
        Err(SettingsError::Conflict)
    ));
    assert!(matches!(
        save_profile(
            root.path(),
            private.path(),
            &lock,
            &LaunchProfileSaveRequest {
                game_id: "game".into(),
                profile_id: None,
                title: "Quiet 2".into(),
                expected_revision: before.games.clone()
            }
        ),
        Err(SettingsError::Conflict)
    ));
    assert!(matches!(
        remove_profile(
            root.path(),
            private.path(),
            &lock,
            &LaunchProfileRemoveRequest {
                game_id: "game".into(),
                profile_id: "quiet".into(),
                expected_revision: before.games.clone()
            }
        ),
        Err(SettingsError::Conflict)
    ));
    assert!(matches!(
        select_profile(
            root.path(),
            private.path(),
            &lock,
            &LaunchProfileSelectRequest {
                game_id: "game".into(),
                profile_id: None,
                expected_revision: before.games
            }
        ),
        Err(SettingsError::Conflict)
    ));
    assert_eq!(
        fs::read_to_string(root.path().join("device.yaml")).unwrap(),
        device
    );
    assert_eq!(
        fs::read_to_string(root.path().join("catalog/library.yaml")).unwrap(),
        library
    );
}

#[test]
fn every_flattened_record_and_routine_step_rejects_and_names_unknown_keys() {
    let records = [
        (
            "host: {cpu-maxx: 3}".to_string(),
            LIBRARY.to_string(),
            "cpu-maxx",
        ),
        (
            "systems: {gba: {runer: bad}}".to_string(),
            LIBRARY.to_string(),
            "runer",
        ),
        (
            "{}".into(),
            LIBRARY.replace("title: Game", "title: Game\n    runer: bad"),
            "runer",
        ),
        (
            "{}".into(),
            LIBRARY.replace("id: gba,", "id: gba, cpu-maxx: 3,"),
            "cpu-maxx",
        ),
        (
            "{}".into(),
            LIBRARY.replace("title: Quiet,", "title: Quiet, runer: bad,"),
            "runer",
        ),
        (
            format!("host: {{families: {{'{FAMILY}': {{cpu-maxx: 3}}}}}}"),
            LIBRARY.into(),
            "cpu-maxx",
        ),
        (
            format!("host: {{runners: {{'{RUNNER}': {{runer: bad}}}}}}"),
            LIBRARY.into(),
            "runer",
        ),
        (
            "routines: {check: {before: [{run: 'true', runer: bad}]}}".into(),
            LIBRARY.into(),
            "runer",
        ),
    ];
    for (device, library, key) in records {
        let error = decode_config_documents(&device, &library)
            .unwrap_err()
            .to_string();
        assert!(error.contains(key), "{device} {library}: {error}");
    }
    // Every flattened kind also accepts a real value, not just unknown-key errors.
    decode_config_documents(DEVICE, LIBRARY).unwrap();
}

#[test]
fn every_record_accepts_all_approved_place_fields_including_the_host_hook_fixture() {
    let body = json!({"cpu":{"min":25,"max":75},"gpu":{"min":10,"max":80},"env":{"LAYER":"plan-value","SHARED":"plan-wins","REMOVED":null,"KORRI_PROFILE_ID":"forged"},"args":{"prepend":["--before"],"append":["--after"]},"cwd":"/tmp","hooks":{"before":[{"run":"true","name":"before","timeout":4,"on-failure":"warn"}],"after":[{"run":"true","name":"after","timeout":5}]},"modifiers":{"@korri:test.foo/wrapper":{"enabled":true,"settings":{"pluginCamelCase":true},"config":{"append":"native"}}}});
    for kind in [
        "host", "system", "game", "release", "profile", "family", "runner",
    ] {
        let mut device = json!({});
        let mut library: serde_json::Value = serde_yaml::from_str(LIBRARY).unwrap();
        let target = match kind {
            "host" => &mut device["host"],
            "system" => &mut device["systems"]["gba"],
            "game" => &mut library["library"]["game"],
            "release" => &mut library["library"]["game"]["releases"][0],
            "profile" => &mut library["library"]["game"]["profiles"]["quiet"],
            "family" => &mut device["host"]["families"][FAMILY],
            _ => &mut device["host"]["runners"][RUNNER],
        };
        if !target.is_object() {
            *target = json!({});
        }
        target
            .as_object_mut()
            .unwrap()
            .extend(body.as_object().unwrap().clone());
        if kind == "family" || kind == "runner" {
            target["settings"] = json!({"pluginOwnedKey":true});
            target["config"] = json!({"prepend":"native","append":"native","replace":"native"});
        }
        decode_config_documents(&device.to_string(), &library.to_string())
            .unwrap_or_else(|e| panic!("{kind}: {e}"));
    }
    decode_config_documents("routines: {check: {before: [{run: 'true', name: before, timeout: 4, on-failure: warn}], after: [{run: 'true', name: after, timeout: 5}]}}",LIBRARY).unwrap();
}

#[test]
fn concatenated_values_preserve_each_place_and_routine_part_without_an_overall_source() {
    let device="routines: {setup: {before: [{run: routine-before}], after: [{run: routine-after}]}}\nhost: {args: {prepend: [device], append: [device-end]}, hooks: {use: [setup], before: [{run: inline-before}], after: [{run: inline-after}]}}";
    let library=LIBRARY.replace("    title: Game","    title: Game\n    args: {prepend: [game], append: [game-end]}\n    hooks: {use: [setup], before: [{run: game-before}], after: [{run: game-after}]}");
    let config = decode_config_documents(device, &library).unwrap();
    let resolved = resolve_launch(&config, &route(), None, None).unwrap();
    for key in ["args.prepend", "args.append", "hooks.before", "hooks.after"] {
        let value = &resolved.values.values[key];
        assert!(value.source.is_none(), "{key}");
        let contributions = value.contributions.as_ref().unwrap();
        let flattened: Vec<_> = contributions
            .iter()
            .flat_map(|c| c.value.as_array().unwrap().clone())
            .collect();
        assert_eq!(serde_json::Value::Array(flattened), value.value);
        let wire = serde_json::to_value(value).unwrap();
        assert!(wire.get("source").is_none());
    }
    let args = resolved.values.values["args.prepend"]
        .contributions
        .as_ref()
        .unwrap();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0].value, json!(["device"]));
    assert_eq!(args[1].value, json!(["game"]));
    assert!(args.iter().all(|c| c.routine.is_none()));
    for key in ["hooks.before", "hooks.after"] {
        let parts = resolved.values.values[key].contributions.as_ref().unwrap();
        assert_eq!(parts.len(), 4);
        assert_eq!(
            parts
                .iter()
                .map(|c| c.routine.as_deref())
                .collect::<Vec<_>>(),
            [Some("setup"), None, Some("setup"), None]
        );
        assert!(matches!(
            &parts[0].source,
            LaunchSource::Place(LaunchPlace {
                layer: LaunchLayer::Device,
                scope: LaunchRunnerScope::Any
            })
        ));
        assert!(
            matches!(&parts[2].source,LaunchSource::Place(LaunchPlace {layer:LaunchLayer::Game(id),scope:LaunchRunnerScope::Any}) if id=="game")
        );
    }
    assert!(resolved.values.values["cpu.max"].source.is_some());
    assert!(resolved.values.values["cpu.max"].contributions.is_none());
}
