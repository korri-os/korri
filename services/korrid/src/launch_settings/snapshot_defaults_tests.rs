use super::tests::{facts, server};
use super::*;
use crate::config::settings;
use serde_json::json;
use std::sync::Mutex;

#[test]
fn snapshot_distinguishes_clear_from_an_authored_value_equal_to_device_default() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let before = settings::runner_choice_revisions(root.path()).unwrap();
    let describe = json!({"_tag":"Ok","payload":facts()});
    let ack = json!({"_tag":"Ok","payload":{}});
    let (client, server) = server(
        root.path(),
        vec![
            describe.clone(),
            ack.clone(),
            describe.clone(),
            describe.clone(),
            ack,
            describe,
        ],
    );
    let lock = Mutex::new(());
    let mut request = LaunchSetRequest {
        place: LaunchPlace {
            layer: LaunchLayer::Device,
            scope: LaunchRunnerScope::Any,
        },
        path: LaunchPath::CpuMax,
        value: Some(json!(61)),
        expected_revision: before.device,
    };
    let revisions = set(root.path(), private.path(), &lock, &request, &client).unwrap();
    let authored = snapshot(root.path(), None, None, &client).unwrap();
    assert_eq!(authored.places[0].body["cpu"]["max"], 61);
    assert!(matches!(
        authored.resolved.values["cpu.max"].source,
        Some(LaunchSource::Place(_))
    ));
    request.expected_revision = revisions.device;
    request.value = None;
    set(root.path(), private.path(), &lock, &request, &client).unwrap();
    let cleared = snapshot(root.path(), None, None, &client).unwrap();
    assert!(cleared.places[0].body["cpu"].get("max").is_none());
    assert_eq!(cleared.resolved.values["cpu.max"].value, 61);
    assert!(matches!(
        cleared.resolved.values["cpu.max"].source,
        Some(LaunchSource::DeviceDefault)
    ));
    let requests = server.join().unwrap();
    assert_eq!(requests[1], requests[4]);
}
