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

const DEVICE: &str = "host: {cpu: {min: 10, max: 95, governor: schedutil}, gpu: {max: 99}}\nsystems: {gba: {cpu: {max: 85}}}\nrunners: {'@korri:mgba/mgba': {gpu: {min: 15}}}\n";
const LIBRARY: &str = "library:\n  game:\n    title: Game\n    cpu: {min: 20}\n    releases: [{id: gba, system: gba, identity: {kind: provider, value: '@korri:content/one'}, gpu: {max: 80}}]\n    profiles:\n      quiet: {title: Quiet, cpu: {max: 60}}\n    profile: quiet\n";
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
    ResolvedRoute {
        playable_id: "game".into(),
        title: None,
        release_id: "@korri:content/one".into(),
        identity: None,
        provider_id: "@korri:mgba".into(),
        system_id: "gba".into(),
        system_title: None,
        runner_id: "@korri:mgba/mgba".into(),
        family_id: None,
        integration_token: String::new(),
        flattened_target: String::new(),
        linux_runner: None,
        core_path: None,
        file_target: None,
        provider_target: None,
        artifact_target: None,
        package_target: None,
        cache_target: None,
    }
}
fn facts() -> LaunchDeviceFacts {
    serde_json::from_value(json!({"defaults":{"cpu":{"min":0,"max":61},"gpu":{"min":0,"max":100},"governor":"schedutil"},"cpuPolicies":[{"name":"policy0","cpus":[0,1],"cpuinfoMaxFreq":1800000,"availableFrequencies":[408000,1800000],"availableGovernors":["schedutil","performance"]}],"gpuDevfreq":[{"name":"gpu","availableFrequencies":[200000000,600000000]}]})).unwrap()
}
#[test]
fn launch_settings_each_layer_contributes_and_independent_values_record_sources() {
    let config = decode_config_documents(DEVICE, LIBRARY).unwrap();
    let layers = resolution::layers(&config, Some("game"), Some(&route())).unwrap();
    assert_eq!(layers.len(), 6);
    let expected = [
        (10, 95, 0, 99),
        (10, 85, 0, 99),
        (10, 85, 15, 99),
        (20, 85, 15, 99),
        (20, 85, 15, 80),
        (20, 60, 15, 80),
    ];
    for (n, (cmin, cmax, gmin, gmax)) in expected.into_iter().enumerate() {
        let r = resolve(&layers[..=n]);
        assert_eq!(
            (
                r.cpu.min.value,
                r.cpu.max.value,
                r.gpu.min.value,
                r.gpu.max.value
            ),
            (cmin, cmax, gmin, gmax)
        );
    }
    let r = resolve(&layers);
    assert!(matches!(r.cpu.min.source, LaunchSettingsScope::Game(_)));
    assert!(matches!(
        r.cpu.max.source,
        LaunchSettingsScope::Profile { .. }
    ));
    assert!(matches!(r.gpu.min.source, LaunchSettingsScope::Runner(_)));
    assert!(matches!(
        r.gpu.max.source,
        LaunchSettingsScope::Release { .. }
    ));
    assert_eq!(r.governor.as_deref(), Some("schedutil"));
    let session =
        session::SessionLimits::for_route(&config, &route(), "build".into(), None).unwrap();
    assert_eq!(
        (
            session.cpu.min,
            session.cpu.max,
            session.gpu.min,
            session.gpu.max
        ),
        (20, 60, 15, 80)
    );
    let empty = decode_config_documents(
        "{}",
        &LIBRARY
            .replace("    cpu: {min: 20}\n", "")
            .replace("gpu: {max: 80}", "gpu: {}")
            .replace("cpu: {max: 60}", "cpu: {}"),
    )
    .unwrap();
    let session =
        session::SessionLimits::for_route(&empty, &route(), "build".into(), None).unwrap();
    assert_eq!(
        (
            session.cpu.min,
            session.cpu.max,
            session.gpu.min,
            session.gpu.max
        ),
        (0, 100, 0, 100)
    );
}
#[test]
fn launch_settings_more_specific_min_or_max_moves_the_other_side_and_records_adjustment() {
    for (min, max, profile, rmin, rmax, adjmin, adjmax) in [
        (80, 90, 40, 40, 40, true, false),
        (10, 40, 80, 80, 80, false, true),
    ] {
        let device = format!("host: {{cpu: {{min: {min}, max: {max}}}}}");
        let library = if adjmin {
            LIBRARY
                .replace("cpu: {min: 20}", "cpu: {}")
                .replace("max: 60", &format!("max: {profile}"))
        } else {
            LIBRARY
                .replace("cpu: {min: 20}", "cpu: {}")
                .replace("max: 60", &format!("min: {profile}"))
        };
        let config = decode_config_documents(&device, &library).unwrap();
        let layers = resolution::layers(&config, Some("game"), Some(&route())).unwrap();
        let r = resolve(&layers);
        assert_eq!(
            (
                r.cpu.min.value,
                r.cpu.max.value,
                r.cpu.min.adjusted,
                r.cpu.max.adjusted
            ),
            (rmin, rmax, adjmin, adjmax)
        );
    }
}
#[test]
fn launch_settings_schema_rejects_governor_outside_host_and_invalid_ranges() {
    for device in [
        "host: {gpu: {governor: schedutil}}",
        "systems: {gba: {cpu: {governor: schedutil}}}",
        "runners: {'@korri:mgba/mgba': {cpu: {governor: schedutil}}}",
        "profiles: {}",
        "host: {cpu: {max: 0}}",
        "host: {cpu: {min: 101}}",
        "host: {cpu: {min: 1.5}}",
        "host: {cpu: {min: null}}",
        "host: {cpu: {min: 90, max: 80}}",
    ] {
        assert!(
            decode_config_documents(device, LIBRARY).is_err(),
            "{device}"
        );
    }
    for library in [
        LIBRARY.replace("cpu: {min: 20}", "cpu: {governor: schedutil}"),
        LIBRARY.replace("gpu: {max: 80}", "cpu: {governor: schedutil}"),
        LIBRARY.replace("cpu: {max: 60}", "cpu: {governor: schedutil}"),
        LIBRARY.replace("profile: quiet", "profile: absent"),
    ] {
        assert!(
            decode_config_documents(DEVICE, &library).is_err(),
            "{library}"
        );
    }
    assert!(crate::config::classify_snapshot_support(
        &decode_config_documents("hooks: {check: {}}", "{}").unwrap()
    )
    .is_err());
}
#[test]
fn launch_settings_atomic_setters_clear_inherit_and_reject_conflicts_without_mutation() {
    let root = root();
    let private = tempfile::tempdir().unwrap();
    let lock = Mutex::new(());
    let client = unavailable(root.path());
    let before = settings::runner_choice_revisions(root.path()).unwrap();
    let request = LaunchSettingsSetRequest {
        scope: LaunchSettingsScope::Game("game".into()),
        setting: LaunchSetting::CpuMax,
        value: Some(LaunchLimitValue::Percent(10)),
        expected_revision: before.games.clone(),
    };
    let bytes = fs::read(root.path().join("catalog/library.yaml")).unwrap();
    let error = set(root.path(), private.path(), &lock, &request, &client).unwrap_err();
    assert!(matches!(error, SettingsError::Invalid(_)));
    let text = error.to_string();
    assert!(
        text.contains("CPU min 20 %") && text.contains("CPU max 10 %") && text.contains("Game"),
        "{text}"
    );
    assert_eq!(
        fs::read(root.path().join("catalog/library.yaml")).unwrap(),
        bytes
    );
    let mut request = LaunchSettingsSetRequest {
        value: Some(LaunchLimitValue::Percent(70)),
        ..request
    };
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
    for setting in [
        LaunchSetting::CpuMin,
        LaunchSetting::GpuMin,
        LaunchSetting::CpuMax,
        LaunchSetting::GpuMax,
    ] {
        let req = LaunchSettingsSetRequest {
            scope: LaunchSettingsScope::Device,
            setting,
            value: Some(LaunchLimitValue::Percent(101)),
            expected_revision: before.device.clone(),
        };
        assert!(matches!(
            set(root.path(), private.path(), &lock, &req, &client),
            Err(SettingsError::Invalid(_))
        ));
    }
}
#[test]
fn launch_settings_set_writes_every_scope_to_its_existing_document() {
    let root = root();
    let private = tempfile::tempdir().unwrap();
    let lock = Mutex::new(());
    let client = unavailable(root.path());
    let scopes = [
        LaunchSettingsScope::Device,
        LaunchSettingsScope::System("gba".into()),
        LaunchSettingsScope::Runner("@korri:mgba/mgba".into()),
        LaunchSettingsScope::Game("game".into()),
        LaunchSettingsScope::Release {
            game_id: "game".into(),
            release_id: "gba".into(),
        },
        LaunchSettingsScope::Profile {
            game_id: "game".into(),
            profile_id: "quiet".into(),
        },
    ];
    for (n, scope) in scopes.into_iter().enumerate() {
        let before = settings::runner_choice_revisions(root.path()).unwrap();
        let device = n < 3;
        let req = LaunchSettingsSetRequest {
            scope,
            setting: LaunchSetting::GpuMin,
            value: Some(LaunchLimitValue::Percent(22)),
            expected_revision: if device {
                before.device.clone()
            } else {
                before.games.clone()
            },
        };
        let after = set(root.path(), private.path(), &lock, &req, &client).unwrap();
        if device {
            assert_ne!(after.device, before.device);
            assert_eq!(after.games, before.games);
        } else {
            assert_ne!(after.games, before.games);
            assert_eq!(after.device, before.device);
        }
    }
    let snap = settings::runner_choice_snapshot(root.path()).unwrap().0;
    let layers = resolution::layers(&snap, Some("game"), Some(&route())).unwrap();
    assert!(layers.iter().all(|l| l.gpu.min == Some(22)));
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
/// The server speaks the helper's actual newline protocol on an actual socket.
fn server(
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
fn launch_settings_describe_defaults_snapshot_apply_device_governor_validation_and_unavailable_facts(
) {
    let root = tempfile::tempdir().unwrap();
    settings::runner_choice_snapshot(root.path()).unwrap();
    let private = tempfile::tempdir().unwrap();
    let lock = Mutex::new(());
    let ok = json!({"_tag":"Ok","payload":facts()});
    let (client, handle) = server(
        root.path(),
        vec![
            ok.clone(),
            ok.clone(),
            json!({"_tag":"Ok","payload":{}}),
            ok,
        ],
    );
    let snapshot = snapshot(root.path(), None, None, &client).unwrap();
    assert_eq!(snapshot.layers.len(), 1);
    assert_eq!(snapshot.resolved.cpu.max.value, 61);
    assert!(matches!(
        snapshot.resolved.cpu.max.source,
        LaunchSettingsScope::Device
    ));
    resolution::apply_device(root.path(), &client).unwrap();
    let req = LaunchSettingsSetRequest {
        scope: LaunchSettingsScope::Device,
        setting: LaunchSetting::CpuGovernor,
        value: Some(LaunchLimitValue::Governor("invalid".into())),
        expected_revision: snapshot.revisions.device,
    };
    assert!(matches!(
        set(root.path(), private.path(), &lock, &req, &client),
        Err(SettingsError::Invalid(_))
    ));
    let requests = handle.join().unwrap();
    assert_eq!(requests[0]["_tag"], "Describe");
    assert_eq!(
        requests[2],
        json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":0,"max":61},"gpu":{"min":0,"max":100},"governor":"schedutil"}})
    );
    let snapshot = super::snapshot(root.path(), None, None, &unavailable(root.path())).unwrap();
    assert!(snapshot.device_facts.is_none());
    assert_eq!(snapshot.resolved.cpu.max.value, 100);
}
#[test]
fn launch_settings_helper_client_apply_end_error_and_warning_path() {
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
        runner_id: "runner".into(),
        build: "build".into(),
    };
    assert!(limits
        .apply(&client, "0123456789abcdef0123456789abcdef")
        .is_none());
    client
        .end_session("korri-game-0123456789abcdef0123456789abcdef.service")
        .unwrap();
    let warning = limits
        .apply(&client, "0123456789abcdef0123456789abcdef")
        .unwrap();
    assert!(warning.message.contains("write refused"));
    assert_eq!(warning.runner_id, "runner");
    let requests = handle.join().unwrap();
    assert_eq!(
        requests[0],
        json!({"_tag":"ApplySession","payload":{"unit":"korri-game-0123456789abcdef0123456789abcdef.service","cpu":{"min":0,"max":40},"gpu":{"min":20,"max":80}}})
    );
    assert_eq!(requests[1]["_tag"], "EndSession");
    assert!(limits
        .apply(
            &unavailable(root.path()),
            "0123456789abcdef0123456789abcdef"
        )
        .is_some());
}
#[test]
fn launch_settings_rpc_wire_and_authority_match_runner_choice() {
    use crate::{
        authorization::{self, AuthorizationContext, Principal},
        RpcRequest,
    };
    for (tag, payload) in [
        ("app.launch-settings.snapshot", json!({})),
        (
            "app.launch-settings.set",
            json!({"scope":{"_tag":"Device"},"setting":"cpu.max","value":60,"expectedRevision":"rev"}),
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
        let req: RpcRequest =
            serde_json::from_value(json!({"_tag":tag,"payload":payload})).unwrap();
        assert_eq!(
            authorization::policy_for(&req),
            authorization::PeerPolicy::OwnerDeviceOnly
        );
        assert!(authorization::authorize(&AuthorizationContext::LocalBrowser, &req).is_ok());
        assert!(authorization::authorize(
            &AuthorizationContext::Peer(Principal::Unknown {
                device_public_key: "peer".into()
            }),
            &req
        )
        .is_err());
        assert_eq!(
            authorization::is_security_mutation(&req),
            tag != "app.launch-settings.snapshot"
        );
    }
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
            &LaunchSettingsSetRequest {
                scope: LaunchSettingsScope::Game("game".into()),
                setting: LaunchSetting::CpuMax,
                value: Some(LaunchLimitValue::Percent(50)),
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
fn launch_settings_valid_device_edit_applies_merged_describe_defaults_immediately() {
    let root = tempfile::tempdir().unwrap();
    let before = settings::runner_choice_revisions(root.path()).unwrap();
    let private = tempfile::tempdir().unwrap();
    let lock = Mutex::new(());
    let (client, server) = server(
        root.path(),
        vec![
            json!({"_tag":"Ok","payload":facts()}),
            json!({"_tag":"Ok","payload":facts()}),
            json!({"_tag":"Ok","payload":{}}),
        ],
    );
    let after = set(
        root.path(),
        private.path(),
        &lock,
        &LaunchSettingsSetRequest {
            scope: LaunchSettingsScope::Device,
            setting: LaunchSetting::CpuGovernor,
            value: Some(LaunchLimitValue::Governor("performance".into())),
            expected_revision: before.device,
        },
        &client,
    )
    .unwrap();
    assert_eq!(after.games, before.games);
    let requests = server.join().unwrap();
    assert_eq!(
        requests[2],
        json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":0,"max":61},"gpu":{"min":0,"max":100},"governor":"performance"}})
    );
}

#[test]
fn launch_settings_session_gpu_inherits_device_limit_while_cpu_uclamp_excludes_device() {
    let root = tempfile::tempdir().unwrap();
    let library = LIBRARY.replace("gpu: {max: 80}", "gpu: {}");
    let config =
        decode_config_documents("host: {cpu: {max: 65}, gpu: {max: 80}}", &library).unwrap();
    let limits =
        session::SessionLimits::for_route(&config, &route(), "build".into(), None).unwrap();
    assert_eq!(limits.gpu.max, 80);
    let (client, server) = server(root.path(), vec![json!({"_tag":"Ok","payload":{}})]);
    assert!(limits
        .apply(&client, "0123456789abcdef0123456789abcdef")
        .is_none());
    let requests = server.join().unwrap();
    assert_eq!(requests[0]["payload"]["gpu"]["max"], 80);
    let config = decode_config_documents(
        "host: {cpu: {max: 65}, gpu: {max: 80}}",
        &library.replace("cpu: {max: 60}", "cpu: {}"),
    )
    .unwrap();
    let limits =
        session::SessionLimits::for_route(&config, &route(), "build".into(), None).unwrap();
    assert_eq!(limits.cpu.max, 100);
    let config = decode_config_documents("{}", &library).unwrap();
    let mut defaults = facts();
    defaults.defaults.gpu.max = 55;
    let limits =
        session::SessionLimits::for_route(&config, &route(), "build".into(), Some(&defaults))
            .unwrap();
    assert_eq!(limits.gpu.max, 55);
}
