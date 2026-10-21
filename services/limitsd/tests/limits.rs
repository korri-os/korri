use korri_limitsd::{serve_connection, Defaults, Limits, Limitsd, Request, Response, Roots};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{fs::symlink, net::UnixStream},
    path::Path,
};
use tempfile::TempDir;

const UNIT: &str = "korri-game-0123456789abcdef0123456789abcdef.service";

fn put(root: &Path, path: &str, value: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value).unwrap();
}

fn defaults() -> Defaults {
    Defaults {
        cpu: Limits { min: 0, max: 61 },
        gpu: Limits { min: 0, max: 100 },
        governor: "schedutil".into(),
    }
}

fn fixture() -> (TempDir, Limitsd) {
    let dir = tempfile::tempdir().unwrap();
    for (name, value) in [
        ("cpuinfo_min_freq", "400"),
        ("cpuinfo_max_freq", "1800"),
        ("related_cpus", "0 1 2 3"),
        ("scaling_available_frequencies", "1800 1200 600 400"),
        ("scaling_available_governors", "schedutil performance"),
        ("scaling_min_freq", "400"),
        ("scaling_max_freq", "1800"),
        ("scaling_governor", "performance"),
    ] {
        put(
            dir.path(),
            &format!("sys/devices/system/cpu/cpufreq/policy0/{name}"),
            value,
        );
    }
    for (name, value) in [
        ("available_frequencies", "200 400 800"),
        ("min_freq", "200"),
        ("max_freq", "800"),
    ] {
        put(dir.path(), &format!("sys/class/devfreq/gpu/{name}"), value);
    }
    for (name, value) in [
        ("cgroup.events", "populated 1\nfrozen 0\n"),
        ("cpu.uclamp.min", "0"),
        ("cpu.uclamp.max", "max"),
    ] {
        put(
            dir.path(),
            &format!("cgroup/korri.slice/korri-games.slice/{UNIT}/{name}"),
            value,
        );
    }
    put(dir.path(), "cgroup/korri.slice/cgroup.subtree_control", "");
    put(dir.path(), "cgroup/korri.slice/cpu.uclamp.min", "0");
    put(
        dir.path(),
        "cgroup/korri.slice/korri-games.slice/cgroup.subtree_control",
        "",
    );
    put(
        dir.path(),
        "cgroup/korri.slice/korri-games.slice/cpu.uclamp.min",
        "0",
    );
    let service = Limitsd::start(
        Roots {
            sysfs: dir.path().join("sys"),
            cgroup: dir.path().join("cgroup"),
        },
        defaults(),
        &["gpu".into()],
    )
    .unwrap();
    (dir, service)
}

fn request(service: &mut Limitsd, value: Value) -> Value {
    serde_json::to_value(service.handle(serde_json::from_value(value).unwrap())).unwrap()
}
fn apply_session(service: &mut Limitsd, cpu: Value, gpu: Value) -> Value {
    request(
        service,
        json!({"_tag":"ApplySession","payload":{"unit":UNIT,"cpu":cpu,"gpu":gpu}}),
    )
}
fn read(dir: &TempDir, path: &str) -> String {
    fs::read_to_string(dir.path().join(path))
        .unwrap()
        .trim()
        .into()
}
fn cpu(dir: &TempDir, name: &str) -> String {
    read(
        dir,
        &format!("sys/devices/system/cpu/cpufreq/policy0/{name}"),
    )
}

#[test]
fn boot_defaults_and_describe_wire_contract() {
    let (dir, mut service) = fixture();
    assert_eq!(cpu(&dir, "scaling_governor"), "schedutil");
    assert_eq!(cpu(&dir, "scaling_min_freq"), "400");
    assert_eq!(cpu(&dir, "scaling_max_freq"), "600");
    assert_eq!(read(&dir, "cgroup/korri.slice/cpu.uclamp.min"), "max");
    assert_eq!(
        read(&dir, "cgroup/korri.slice/cgroup.subtree_control"),
        "+cpu"
    );
    assert_eq!(
        read(
            &dir,
            "cgroup/korri.slice/korri-games.slice/cgroup.subtree_control"
        ),
        "+cpu"
    );
    assert_eq!(
        read(&dir, "cgroup/korri.slice/korri-games.slice/cpu.uclamp.min"),
        "max"
    );
    assert_eq!(
        request(&mut service, json!({"_tag":"Describe","payload":{}})),
        json!({"_tag":"Ok","payload":{
            "defaults":{"cpu":{"min":0,"max":61},"gpu":{"min":0,"max":100},"governor":"schedutil"},
            "cpuPolicies":[{"name":"policy0","cpus":[0,1,2,3],"cpuinfoMaxFreq":1800,"availableFrequencies":[400,600,1200,1800],"availableGovernors":["schedutil","performance"]}],
            "gpuDevfreq":[{"name":"gpu","availableFrequencies":[200,400,800]}]
        }})
    );
}

#[test]
fn session_rounding_uclamp_and_end_restore() {
    let (dir, mut service) = fixture();
    assert_eq!(
        apply_session(
            &mut service,
            json!({"min":30,"max":70}),
            json!({"min":30,"max":70})
        ),
        json!({"_tag":"Ok","payload":{}})
    );
    assert_eq!(cpu(&dir, "scaling_max_freq"), "1200");
    assert_eq!(cpu(&dir, "scaling_min_freq"), "400");
    assert_eq!(
        read(
            &dir,
            &format!("cgroup/korri.slice/korri-games.slice/{UNIT}/cpu.uclamp.min")
        ),
        "30"
    );
    assert_eq!(
        read(
            &dir,
            &format!("cgroup/korri.slice/korri-games.slice/{UNIT}/cpu.uclamp.max")
        ),
        "70"
    );
    assert_eq!(read(&dir, "sys/class/devfreq/gpu/min_freq"), "400");
    assert_eq!(read(&dir, "sys/class/devfreq/gpu/max_freq"), "400");
    assert_eq!(
        request(
            &mut service,
            json!({"_tag":"EndSession","payload":{"unit":UNIT}})
        )["_tag"],
        "Ok"
    );
    assert_eq!(cpu(&dir, "scaling_max_freq"), "600");
    assert_eq!(read(&dir, "sys/class/devfreq/gpu/max_freq"), "800");
    assert_eq!(
        read(
            &dir,
            &format!("cgroup/korri.slice/korri-games.slice/{UNIT}/cpu.uclamp.max")
        ),
        "100"
    );
}

#[test]
fn device_updates_are_deferred_and_revert_when_cgroup_empties_or_disappears() {
    for disappeared in [false, true] {
        let (dir, mut service) = fixture();
        apply_session(
            &mut service,
            json!({"min":0,"max":100}),
            json!({"min":0,"max":100}),
        );
        assert_eq!(
            request(
                &mut service,
                json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":30,"max":70},"gpu":{"min":0,"max":50},"governor":"performance"}})
            )["_tag"],
            "Ok"
        );
        assert_eq!(cpu(&dir, "scaling_max_freq"), "1800");
        assert_eq!(cpu(&dir, "scaling_governor"), "schedutil");
        service.reassert().unwrap();
        assert_eq!(cpu(&dir, "scaling_max_freq"), "1800");
        if disappeared {
            fs::remove_dir_all(
                dir.path()
                    .join(format!("cgroup/korri.slice/korri-games.slice/{UNIT}")),
            )
            .unwrap();
        } else {
            put(
                dir.path(),
                &format!("cgroup/korri.slice/korri-games.slice/{UNIT}/cgroup.events"),
                "populated 0\n",
            );
        }
        service.poll_session().unwrap();
        assert_eq!(cpu(&dir, "scaling_max_freq"), "1200");
        assert_eq!(cpu(&dir, "scaling_min_freq"), "600");
        assert_eq!(cpu(&dir, "scaling_governor"), "performance");
        assert_eq!(read(&dir, "sys/class/devfreq/gpu/max_freq"), "400");
    }
}

#[test]
fn rejects_bad_ranges_units_governors_and_stale_ends_without_writes() {
    let (dir, mut service) = fixture();
    for range in [
        json!({"min":0,"max":0}),
        json!({"min":101,"max":100}),
        json!({"min":90,"max":30}),
    ] {
        assert_eq!(
            apply_session(&mut service, range, json!({"min":0,"max":100}))["_tag"],
            "Err"
        );
    }
    for unit in [
        "../oops",
        "korri-game-0123456789ABCDEF0123456789abcdef.service",
        "korri-game-0123.service",
        "sshd.service",
        "korri-game-0123456789abcdef0123456789abcdef.service/extra",
    ] {
        assert_eq!(
            request(
                &mut service,
                json!({"_tag":"ApplySession","payload":{"unit":unit,"cpu":{"min":0,"max":100},"gpu":{"min":0,"max":100}}})
            )["_tag"],
            "Err"
        );
    }
    assert_eq!(
        request(
            &mut service,
            json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":0,"max":100},"gpu":{"min":0,"max":100},"governor":"unknown"}})
        )["_tag"],
        "Err"
    );
    assert_eq!(
        request(
            &mut service,
            json!({"_tag":"EndSession","payload":{"unit":UNIT}})
        )["_tag"],
        "Err"
    );
    assert_eq!(cpu(&dir, "scaling_max_freq"), "600");
    for min in [json!(-1), json!(1.5), json!("2")] {
        assert!(serde_json::from_value::<Request>(json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":min,"max":100},"gpu":{"min":0,"max":100},"governor":"schedutil"}})).is_err());
    }
}

#[test]
fn rejects_cgroup_outside_slice_or_alias_to_another_unit() {
    for outside_slice in [true, false] {
        let (dir, mut service) = fixture();
        let path = dir
            .path()
            .join(format!("cgroup/korri.slice/korri-games.slice/{UNIT}"));
        let target = if outside_slice {
            dir.path().join("outside")
        } else {
            dir.path().join("cgroup/korri.slice/korri-games.slice/korri-game-ffffffffffffffffffffffffffffffff.service")
        };
        fs::rename(&path, &target).unwrap();
        symlink(target, &path).unwrap();
        assert_eq!(
            apply_session(
                &mut service,
                json!({"min":0,"max":100}),
                json!({"min":0,"max":100})
            )["_tag"],
            "Err"
        );
    }
}

#[test]
fn socket_checks_real_peer_credentials_and_replies_once() {
    for authorized in [true, false] {
        let (_dir, mut service) = fixture();
        let (server, mut client) = UnixStream::pair().unwrap();
        let uid = unsafe { libc::getuid() };
        let allowed = if authorized { uid } else { uid + 1 };
        let worker = std::thread::spawn(move || serve_connection(server, allowed, &mut service));
        client
            .write_all(b"{\"_tag\":\"Describe\",\"payload\":{}}\n")
            .unwrap();
        let mut line = String::new();
        BufReader::new(client).read_line(&mut line).unwrap();
        let response: Response = serde_json::from_str(&line).unwrap();
        assert_eq!(matches!(response, Response::Ok(_)), authorized);
        worker.join().unwrap().unwrap();
    }
}

#[test]
fn fallback_without_cpu_step_list_clamps_to_hardware_range() {
    let (dir, _) = fixture();
    fs::remove_file(
        dir.path()
            .join("sys/devices/system/cpu/cpufreq/policy0/scaling_available_frequencies"),
    )
    .unwrap();
    let service = Limitsd::start(
        Roots {
            sysfs: dir.path().join("sys"),
            cgroup: dir.path().join("cgroup"),
        },
        defaults(),
        &["gpu".into()],
    )
    .unwrap();
    assert_eq!(cpu(&dir, "scaling_max_freq"), "1098");
    drop(service);
}

#[test]
fn rg353m_cpu_step_percent_choices_roundtrip() {
    let (dir, _) = fixture();
    for (name, value) in [
        ("cpuinfo_min_freq", "408000"),
        ("cpuinfo_max_freq", "1800000"),
        ("scaling_min_freq", "408000"),
        ("scaling_max_freq", "1800000"),
        (
            "scaling_available_frequencies",
            "408000 600000 816000 1104000 1416000 1608000 1800000",
        ),
    ] {
        put(
            dir.path(),
            &format!("sys/devices/system/cpu/cpufreq/policy0/{name}"),
            value,
        );
    }
    let mut service = Limitsd::start(
        Roots {
            sysfs: dir.path().join("sys"),
            cgroup: dir.path().join("cgroup"),
        },
        defaults(),
        &["gpu".into()],
    )
    .unwrap();
    assert_eq!(cpu(&dir, "scaling_max_freq"), "1104000");
    assert_eq!(
        request(
            &mut service,
            json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":61,"max":100},"gpu":{"min":0,"max":100},"governor":"schedutil"}})
        )["_tag"],
        "Ok"
    );
    assert_eq!(cpu(&dir, "scaling_min_freq"), "1104000");
    assert_eq!(
        request(
            &mut service,
            json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":0,"max":60},"gpu":{"min":0,"max":100},"governor":"schedutil"}})
        )["_tag"],
        "Ok"
    );
    assert_eq!(cpu(&dir, "scaling_max_freq"), "816000");
    for (percent, step) in [
        (23, 408000),
        (33, 600000),
        (45, 816000),
        (61, 1104000),
        (79, 1416000),
        (89, 1608000),
        (100, 1800000),
    ] {
        assert_eq!(
            request(
                &mut service,
                json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":percent,"max":percent},"gpu":{"min":0,"max":100},"governor":"schedutil"}})
            )["_tag"],
            "Ok"
        );
        assert_eq!(cpu(&dir, "scaling_min_freq"), step.to_string());
        assert_eq!(cpu(&dir, "scaling_max_freq"), step.to_string());
    }
}

#[test]
fn gpu_step_percent_labels_round_half_up() {
    let (dir, _) = fixture();
    put(
        dir.path(),
        "sys/class/devfreq/gpu/available_frequencies",
        "500 1000 1010 2000",
    );
    put(dir.path(), "sys/class/devfreq/gpu/min_freq", "500");
    put(dir.path(), "sys/class/devfreq/gpu/max_freq", "2000");
    let mut service = Limitsd::start(
        Roots {
            sysfs: dir.path().join("sys"),
            cgroup: dir.path().join("cgroup"),
        },
        defaults(),
        &["gpu".into()],
    )
    .unwrap();
    // 1010 / 2000 = 50.5%, so its label is 51, not 50.
    assert_eq!(
        request(
            &mut service,
            json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":0,"max":61},"gpu":{"min":0,"max":50},"governor":"schedutil"}})
        )["_tag"],
        "Ok"
    );
    assert_eq!(read(&dir, "sys/class/devfreq/gpu/max_freq"), "1000");
    assert_eq!(
        request(
            &mut service,
            json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":0,"max":61},"gpu":{"min":51,"max":100},"governor":"schedutil"}})
        )["_tag"],
        "Ok"
    );
    assert_eq!(read(&dir, "sys/class/devfreq/gpu/min_freq"), "1010");
}

#[test]
fn configured_rg353m_gpu_node_is_described_and_managed() {
    let (dir, _) = fixture();
    fs::rename(
        dir.path().join("sys/class/devfreq/gpu"),
        dir.path().join("sys/class/devfreq/fde60000.gpu"),
    )
    .unwrap();
    let mut service = Limitsd::start(
        Roots {
            sysfs: dir.path().join("sys"),
            cgroup: dir.path().join("cgroup"),
        },
        defaults(),
        &["fde60000.gpu".into()],
    )
    .unwrap();
    let description = request(&mut service, json!({"_tag":"Describe","payload":{}}));
    assert_eq!(
        description["payload"]["gpuDevfreq"][0]["name"],
        "fde60000.gpu"
    );
    assert_eq!(
        request(
            &mut service,
            json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":0,"max":61},"gpu":{"min":50,"max":50},"governor":"schedutil"}})
        )["_tag"],
        "Ok"
    );
    assert_eq!(read(&dir, "sys/class/devfreq/fde60000.gpu/min_freq"), "400");
    assert_eq!(read(&dir, "sys/class/devfreq/fde60000.gpu/max_freq"), "400");
}

#[test]
fn rounded_max_wins_when_no_frequency_step_fits_interval() {
    let (dir, mut service) = fixture();
    assert_eq!(
        request(
            &mut service,
            json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":50,"max":50},"gpu":{"min":30,"max":30},"governor":"schedutil"}})
        )["_tag"],
        "Ok"
    );
    assert_eq!(cpu(&dir, "scaling_min_freq"), "600");
    assert_eq!(cpu(&dir, "scaling_max_freq"), "600");
    assert_eq!(read(&dir, "sys/class/devfreq/gpu/min_freq"), "200");
    assert_eq!(read(&dir, "sys/class/devfreq/gpu/max_freq"), "200");
    apply_session(
        &mut service,
        json!({"min":0,"max":100}),
        json!({"min":0,"max":100}),
    );
    assert_eq!(cpu(&dir, "scaling_min_freq"), "600");
    assert_eq!(cpu(&dir, "scaling_max_freq"), "1800");
}

#[test]
fn failed_session_application_retries_revert_even_with_populated_game() {
    let (dir, mut service) = fixture();
    fs::remove_file(dir.path().join("sys/class/devfreq/gpu/max_freq")).unwrap();
    assert_eq!(
        apply_session(
            &mut service,
            json!({"min":0,"max":100}),
            json!({"min":0,"max":50})
        )["_tag"],
        "Err"
    );
    assert_eq!(cpu(&dir, "scaling_max_freq"), "600");
    put(dir.path(), "sys/class/devfreq/gpu/max_freq", "400");
    service.poll_session().unwrap();
    assert_eq!(read(&dir, "sys/class/devfreq/gpu/max_freq"), "800");
    service.reassert().unwrap();
    assert_eq!(cpu(&dir, "scaling_max_freq"), "600");
    assert_eq!(
        apply_session(
            &mut service,
            json!({"min":0,"max":100}),
            json!({"min":0,"max":100})
        )["_tag"],
        "Ok"
    );
}

#[test]
fn every_policy_uses_its_own_top_clock_and_validates_governor() {
    let (dir, _) = fixture();
    for (name, value) in [
        ("cpuinfo_min_freq", "100"),
        ("cpuinfo_max_freq", "1000"),
        ("related_cpus", "4 5"),
        ("scaling_available_frequencies", "100 400 800 1000"),
        ("scaling_available_governors", "schedutil"),
        ("scaling_min_freq", "100"),
        ("scaling_max_freq", "1000"),
        ("scaling_governor", "schedutil"),
    ] {
        put(
            dir.path(),
            &format!("sys/devices/system/cpu/cpufreq/policy4/{name}"),
            value,
        );
    }
    let mut service = Limitsd::start(
        Roots {
            sysfs: dir.path().join("sys"),
            cgroup: dir.path().join("cgroup"),
        },
        defaults(),
        &["gpu".into()],
    )
    .unwrap();
    assert_eq!(
        read(
            &dir,
            "sys/devices/system/cpu/cpufreq/policy4/scaling_max_freq"
        ),
        "400"
    );
    apply_session(
        &mut service,
        json!({"min":0,"max":80}),
        json!({"min":0,"max":100}),
    );
    assert_eq!(
        read(
            &dir,
            "sys/devices/system/cpu/cpufreq/policy4/scaling_max_freq"
        ),
        "800"
    );
    assert_eq!(cpu(&dir, "scaling_max_freq"), "1200");
    assert_eq!(
        request(
            &mut service,
            json!({"_tag":"ApplyDevice","payload":{"cpu":{"min":0,"max":100},"gpu":{"min":0,"max":100},"governor":"performance"}})
        )["_tag"],
        "Err"
    );
    assert_eq!(cpu(&dir, "scaling_governor"), "schedutil");
}

#[test]
fn reassert_repairs_external_resets_and_stale_end_cannot_stop_session() {
    let (dir, mut service) = fixture();
    put(dir.path(), "cgroup/korri.slice/cpu.uclamp.min", "0");
    put(
        dir.path(),
        "cgroup/korri.slice/korri-games.slice/cpu.uclamp.min",
        "0",
    );
    put(
        dir.path(),
        "sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq",
        "1800",
    );
    service.reassert().unwrap();
    assert_eq!(read(&dir, "cgroup/korri.slice/cpu.uclamp.min"), "max");
    assert_eq!(
        read(&dir, "cgroup/korri.slice/korri-games.slice/cpu.uclamp.min"),
        "max"
    );
    assert_eq!(cpu(&dir, "scaling_max_freq"), "600");
    apply_session(
        &mut service,
        json!({"min":0,"max":70}),
        json!({"min":0,"max":50}),
    );
    put(
        dir.path(),
        "sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq",
        "1800",
    );
    put(
        dir.path(),
        "sys/devices/system/cpu/cpufreq/policy0/scaling_governor",
        "performance",
    );
    put(dir.path(), "sys/class/devfreq/gpu/max_freq", "800");
    put(
        dir.path(),
        &format!("cgroup/korri.slice/korri-games.slice/{UNIT}/cpu.uclamp.max"),
        "100",
    );
    assert_eq!(
        request(
            &mut service,
            json!({"_tag":"EndSession","payload":{"unit":"korri-game-ffffffffffffffffffffffffffffffff.service"}})
        )["_tag"],
        "Err"
    );
    assert_eq!(
        apply_session(
            &mut service,
            json!({"min":0,"max":100}),
            json!({"min":0,"max":100})
        )["_tag"],
        "Err"
    );
    service.reassert().unwrap();
    assert_eq!(cpu(&dir, "scaling_max_freq"), "1200");
    assert_eq!(cpu(&dir, "scaling_governor"), "schedutil");
    assert_eq!(read(&dir, "sys/class/devfreq/gpu/max_freq"), "400");
    assert_eq!(
        read(
            &dir,
            &format!("cgroup/korri.slice/korri-games.slice/{UNIT}/cpu.uclamp.max")
        ),
        "70"
    );
}

#[test]
fn reassert_discovers_cpu_policies_registered_after_boot() {
    let (dir, _) = fixture();
    let policy = dir.path().join("sys/devices/system/cpu/cpufreq/policy0");
    let late = dir.path().join("late-policy");
    fs::rename(&policy, &late).unwrap();
    fs::write(late.join("scaling_max_freq"), "1800").unwrap();
    fs::write(late.join("scaling_governor"), "performance").unwrap();
    let mut service = Limitsd::start(
        Roots {
            sysfs: dir.path().join("sys"),
            cgroup: dir.path().join("cgroup"),
        },
        defaults(),
        &["gpu".into()],
    )
    .unwrap();
    fs::rename(late, policy).unwrap();
    service.reassert().unwrap();
    assert_eq!(cpu(&dir, "scaling_max_freq"), "600");
    assert_eq!(cpu(&dir, "scaling_governor"), "schedutil");
}

#[test]
fn transport_rejects_unknown_fields_oversize_and_non_json() {
    for bytes in [
        b"not json\n".to_vec(),
        b"{\"_tag\":\"Describe\",\"payload\":{\"path\":\"/etc/passwd\"}}\n".to_vec(),
        b"{\"_tag\":\"ApplySession\",\"payload\":{\"unit\":\"anything\",\"cpu\":{\"min\":0,\"max\":100},\"gpu\":{\"min\":0,\"max\":100},\"command\":\"true\"}}\n".to_vec(),
        vec![b'x'; 4097],
    ] {
        let (_dir, mut service) = fixture();
        let (server, mut client) = UnixStream::pair().unwrap();
        let uid = unsafe { libc::getuid() };
        let worker = std::thread::spawn(move || serve_connection(server, uid, &mut service));
        client.write_all(&bytes).unwrap();
        let mut line = String::new();
        BufReader::new(client).read_line(&mut line).unwrap();
        let response: Response = serde_json::from_str(&line).unwrap();
        assert!(matches!(response, Response::Err { .. }));
        worker.join().unwrap().unwrap();
    }
}
