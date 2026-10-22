use super::*;
use crate::config::settings;
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::Mutex,
    thread,
};

#[test]
fn snapshot_keeps_device_values_authored_and_distinguishes_clear_from_explicit_default() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let before = settings::runner_choice_revisions(root.path()).unwrap();
    let socket = root.path().join("limits.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let facts = json!({
        "defaults": {"cpu": {"min": 0, "max": 61}, "gpu": {"min": 0, "max": 100}, "governor": "schedutil"},
        "cpuPolicies": [], "gpuDevfreq": []
    });
    let describe = json!({"_tag": "Ok", "payload": facts});
    let ack = json!({"_tag": "Ok", "payload": {}});
    let server = thread::spawn(move || {
        [
            describe.clone(),
            ack.clone(),
            describe.clone(),
            describe.clone(),
            ack,
            describe,
        ]
        .into_iter()
        .map(|reply| {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            stream.write_all(format!("{reply}\n").as_bytes()).unwrap();
            serde_json::from_str::<serde_json::Value>(&line).unwrap()
        })
        .collect::<Vec<_>>()
    });
    let client = helper::Client { socket };
    let lock = Mutex::new(());
    let mut request = LaunchSettingsSetRequest {
        scope: LaunchSettingsScope::Device,
        setting: LaunchSetting::CpuMax,
        value: Some(LaunchLimitValue::Percent(61)),
        expected_revision: before.device,
    };
    let authored_revision = set(root.path(), private.path(), &lock, &request, &client).unwrap();
    let authored = snapshot(root.path(), None, None, &client).unwrap();
    assert_eq!(authored.layers[0].cpu.max, Some(61));
    assert!(authored.layers[0].cpu.min.is_none());
    assert!(authored.layers[0].gpu.min.is_none());
    assert!(authored.layers[0].gpu.max.is_none());
    assert!(authored.layers[0].governor.is_none());
    assert_eq!(authored.resolved.cpu.max.value, 61);
    assert!(matches!(
        authored.resolved.cpu.max.source,
        LaunchSettingsScope::Device
    ));
    assert!(!authored.resolved.cpu.max.device_default);

    request.value = None;
    request.expected_revision = authored_revision.device;
    set(root.path(), private.path(), &lock, &request, &client).unwrap();
    let cleared = snapshot(root.path(), None, None, &client).unwrap();
    assert!(cleared.layers[0].cpu.max.is_none());
    assert_eq!(cleared.resolved.cpu.max.value, 61);
    assert!(matches!(
        cleared.resolved.cpu.max.source,
        LaunchSettingsScope::Device
    ));
    assert!(cleared.resolved.cpu.max.device_default);
    assert_eq!(cleared.device_facts.as_ref().unwrap().defaults.cpu.max, 61);
    assert_eq!(cleared.resolved.governor.as_deref(), Some("schedutil"));
    let wire = serde_json::to_value(&cleared).unwrap();
    assert_eq!(wire["layers"][0]["cpu"], json!({}));
    assert_eq!(wire["layers"][0]["gpu"], json!({}));
    assert!(wire["layers"][0].get("governor").is_none());
    assert_eq!(wire["resolved"]["cpu"]["max"]["deviceDefault"], true);

    let requests = server.join().unwrap();
    // Clearing an explicit value equal to the shipped default changes only its
    // reported origin, never the limits or governor sent to the helper.
    assert_eq!(requests[1]["_tag"], "ApplyDevice");
    assert_eq!(requests[1], requests[4]);
}
