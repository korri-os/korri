//! The launch settings model and its controller against a scripted korrid.
//! Each snapshot is written out as korrid sends it, resolved values included,
//! so the tests cover the reading of korrid's snapshot and the reads and writes.
//! Device settings keep their rows by place; a game's editor is one list (ADR 0066).

use std::{collections::HashMap, sync::Arc, time::Duration};

use korri_surface_treaty::{
    SurfaceLaunchSettings, SurfaceLaunchSettingsContent, SurfaceSettingChoice,
    SurfaceSettingInteraction, SurfaceSettingItem, SurfaceSettingScope, SurfaceSettingsStatus,
};
use serde_json::{json, Value};

use super::acquisition_gate::{push, scripted};
use crate::surface::{
    launch_model::{
        cpu_steps, device_launch_from, governors, gpu_steps, launch_id, layer_key, Encoder,
        LaunchEncoded,
    },
    launch_settings::{Closing, LaunchSettings},
};

const MOONLIGHT: &str = "@korri:moonlight/moonlight";
const SUNSHINE: &str = "@korri:sunshine/sunshine";
const SNAPSHOT: &str = "app.launch-settings.snapshot";
const SET: &str = "app.launch-settings.set";

fn section(title: &str, properties: Value) -> Value {
    json!({ "type": "object", "title": title, "additionalProperties": false, "properties": properties })
}

fn mgba_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "video": section("Video", json!({
                "frameskip": { "type": "integer", "title": "Frameskip", "description": "Frames skipped between drawn ones.", "minimum": 0, "maximum": 10, "default": 0 },
                "color-correction": { "type": "boolean", "title": "Colour correction" },
            })),
            "input": section("Input", json!({ "solar": section("Solar sensor", json!({ "level": { "type": "string", "title": "Level", "enum": ["off", "low", "high"] } })) })),
            "locale": section("Language", json!({ "language": { "type": "string", "title": "Language", "x-korri-scope": "device" } })),
        },
    })
}

fn moonlight_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "video": section("Video", json!({
                "fps": { "type": "integer", "title": "FPS", "minimum": 10, "maximum": 240 },
                "codec": { "type": "string", "title": "Codec", "enum": ["auto", "h264", "h265", "av1"], "default": "h264" },
                "hdr": section("HDR", json!({ "enable": { "type": "boolean", "title": "Enable" } })),
            })),
            "audio": section("Audio", json!({ "channels": { "type": "string", "title": "Channels", "enum": ["stereo", "5.1", "7.1"] } })),
        },
    })
}

fn rg353m() -> Value {
    json!({
        "defaults": { "cpu": { "min": 0, "max": 100 }, "gpu": { "min": 0, "max": 100 }, "governor": "schedutil" },
        "cpuPolicies": [{
            "name": "policy0",
            "cpus": [0, 1, 2, 3],
            "cpuinfoMaxFreq": 1_800_000,
            "availableFrequencies": [408_000, 600_000, 816_000, 1_104_000, 1_416_000, 1_608_000, 1_800_000],
            "availableGovernors": ["conservative", "ondemand", "userspace", "powersave", "performance", "schedutil"],
        }],
        "gpuDevfreq": [{
            "name": "fde60000.gpu",
            "availableFrequencies": [200_000_000, 300_000_000, 400_000_000, 600_000_000, 700_000_000, 800_000_000],
        }],
    })
}

#[derive(Clone)]
struct Runner {
    id: &'static str,
    title: &'static str,
    family: Option<&'static str>,
    schema: Value,
    choosable: bool,
    prepare: Value,
}

fn runner(
    id: &'static str,
    title: &'static str,
    family: Option<&'static str>,
    schema: Value,
) -> Runner {
    Runner {
        id,
        title,
        family,
        schema,
        choosable: true,
        prepare: json!({ "_tag": "Ready", "payload": { "env": {}, "args": [] } }),
    }
}

fn mgba() -> Runner {
    Runner {
        prepare: json!({ "_tag": "Ready", "payload": { "env": { "MGBA_AUDIO": "pulse" }, "args": ["-f"] } }),
        ..runner("retroarch/mgba", "mGBA", Some("retroarch"), mgba_schema())
    }
}
fn gpsp() -> Runner {
    runner(
        "retroarch/gpsp",
        "gpSP",
        Some("retroarch"),
        json!({ "type": "object", "properties": { "bios": section("BIOS", json!({ "enable": { "type": "boolean", "title": "Use BIOS" } })) } }),
    )
}
fn moonlight() -> Runner {
    Runner {
        choosable: false,
        ..runner(MOONLIGHT, "Moonlight", None, moonlight_schema())
    }
}

/// A launch place as korrid names it, from the editor's place id.
fn place(id: &str) -> Value {
    let (layer, scope) = id.split_once('/').expect("place id");
    let layer = match layer.split_once(':') {
        None => json!({ "_tag": "Device" }),
        Some(("system", id)) => json!({ "_tag": "System", "id": id }),
        Some(("game", id)) => json!({ "_tag": "Game", "id": id }),
        Some(("release", id)) => {
            json!({ "_tag": "Release", "id": { "gameId": "wario", "releaseId": id } })
        }
        Some(("profile", id)) => {
            json!({ "_tag": "Profile", "id": { "gameId": "wario", "profileId": id } })
        }
        Some(_) => panic!("layer {layer}"),
    };
    let scope = match scope.split_once(':') {
        None => json!({ "_tag": "Any" }),
        Some(("family", id)) => json!({ "_tag": "Family", "id": id }),
        Some((_, id)) => json!({ "_tag": "Runner", "id": id }),
    };
    json!({ "layer": layer, "scope": scope })
}

fn editable(path: Value) -> Value {
    json!({ "_tag": "Editable", "payload": path })
}
fn setting(key: &str) -> Value {
    json!({ "_tag": "Setting", "payload": { "key": key } })
}
fn at(id: &str) -> Value {
    json!({ "_tag": "Place", "payload": place(id) })
}
fn plugin_default(id: &str) -> Value {
    json!({ "_tag": "PluginDefault", "payload": { "pluginId": id } })
}
fn device_default() -> Value {
    json!({ "_tag": "DeviceDefault" })
}
fn sourced(path: Value, value: Value, source: Value) -> Value {
    json!({ "path": path, "value": value, "source": source, "adjusted": false })
}

/// One snapshot korrid could send for Wario Land 4 (or the device page).
#[derive(Clone)]
struct Fixture {
    game: bool,
    system_and_release: bool,
    runners: Vec<Runner>,
    selected: Option<&'static str>,
    bodies: Vec<(String, Value)>,
    profiles: Vec<(&'static str, &'static str)>,
    selected_profile: Option<&'static str>,
    facts: Option<Value>,
    resolved: Vec<Value>,
    runner_resolved: Vec<(&'static str, Vec<Value>)>,
    notices: Vec<Value>,
    modifiers: Vec<Value>,
    services: Vec<Value>,
    route_layers: Value,
    revisions: (String, String),
}

fn wario() -> Fixture {
    Fixture {
        game: true,
        system_and_release: true,
        runners: vec![mgba(), gpsp()],
        selected: Some("retroarch/mgba"),
        bodies: vec![(
            "game:wario/any".into(),
            json!({ "runner": "retroarch/mgba" }),
        )],
        profiles: Vec::new(),
        selected_profile: None,
        facts: None,
        resolved: vec![sourced(
            editable(json!({ "_tag": "Runner" })),
            json!("retroarch/mgba"),
            at("game:wario/any"),
        )],
        runner_resolved: Vec::new(),
        notices: Vec::new(),
        modifiers: Vec::new(),
        services: Vec::new(),
        route_layers: json!({ "_tag": "Present" }),
        revisions: ("device-1".into(), "games-1".into()),
    }
}

fn device_page() -> Fixture {
    Fixture {
        game: false,
        runners: Vec::new(),
        selected: None,
        bodies: Vec::new(),
        resolved: Vec::new(),
        ..wario()
    }
}

impl Fixture {
    fn body(mut self, id: &str, body: Value) -> Self {
        self.bodies.retain(|(known, _)| known != id);
        self.bodies.push((id.into(), body));
        self
    }
    fn value(mut self, value: Value) -> Self {
        self.resolved.push(value);
        self
    }
    fn facts(mut self, facts: Value) -> Self {
        self.facts = Some(facts);
        self
    }
    fn layers(&self) -> Vec<(String, String)> {
        let mut layers = vec![("device".to_owned(), "Device".to_owned())];
        if self.game {
            if self.system_and_release {
                layers.push(("system:gba".into(), "Game Boy Advance".into()));
            }
            layers.push(("game:wario".into(), "Wario Land 4".into()));
            if self.system_and_release {
                layers.push(("release:usa".into(), "usa".into()));
            }
            for (id, title) in &self.profiles {
                layers.push((format!("profile:{id}"), (*title).to_owned()));
            }
        }
        layers
    }
    fn snapshot(&self) -> Value {
        let mut scopes = vec![("any".to_owned(), None::<String>)];
        let mut families: Vec<&str> = Vec::new();
        for runner in &self.runners {
            if let Some(family) = runner.family.filter(|family| !families.contains(family)) {
                families.push(family);
                scopes.push((format!("family:{family}"), Some(format!("family {family}"))));
            }
        }
        for runner in &self.runners {
            scopes.push((format!("runner:{}", runner.id), Some(runner.title.into())));
        }
        let body_of = |id: &str| {
            self.bodies
                .iter()
                .find(|(known, _)| known == id)
                .map_or(json!({}), |(_, body)| body.clone())
        };
        let places: Vec<Value> = self
            .layers()
            .iter()
            .flat_map(|(layer, title)| {
                scopes.iter().map(move |(scope, named)| {
                    (layer.clone(), title.clone(), scope.clone(), named.clone())
                })
            })
            .map(|(layer, title, scope, named)| {
                let id = format!("{layer}/{scope}");
                json!({
                    "place": place(&id),
                    "title": named.map_or(title.clone(), |named| format!("{title} for {named}")),
                    "body": body_of(&id),
                    "editableKinds": if scope == "any" {
                        json!(["runner", "cpu", "gpu", "modifiers", "services"])
                    } else {
                        json!(["cpu", "gpu", "modifiers", "settings", "services"])
                    },
                })
            })
            .collect();
        let resolved: serde_json::Map<String, Value> = self
            .resolved
            .iter()
            .enumerate()
            .map(|(index, value)| (format!("v{index}"), value.clone()))
            .collect();
        let mut snapshot = json!({
            "places": places,
            "resolved": { "values": resolved },
            "runners": self.runners.iter().map(|runner| {
                let mut out = json!({
                    "id": runner.id,
                    "title": runner.title,
                    "selected": self.selected == Some(runner.id),
                    "choosable": runner.choosable,
                    "schema": runner.schema,
                    "revision": "r1",
                    "prepare": runner.prepare,
                });
                if let Some(family) = runner.family {
                    out["family"] = json!(family);
                }
                out
            }).collect::<Vec<_>>(),
            "runnerResolved": self.runner_resolved.iter().map(|(id, values)| json!({
                "runnerId": id,
                "values": values.iter().enumerate().map(|(index, value)| (format!("r{index}"), value.clone())).collect::<serde_json::Map<_, _>>(),
            })).collect::<Vec<_>>(),
            "modifiers": self.modifiers,
            "services": self.services,
            "profiles": self.profiles.iter().map(|(id, title)| json!({ "id": id, "title": title, "body": {} })).collect::<Vec<_>>(),
            "notices": self.notices,
            "routeLayers": self.route_layers,
            "revisions": { "device": self.revisions.0, "games": self.revisions.1 },
        });
        if let Some(profile) = self.selected_profile {
            snapshot["selectedProfile"] = json!(profile);
        }
        if let Some(facts) = &self.facts {
            snapshot["deviceFacts"] = facts.clone();
        }
        snapshot
    }
}

// --- readers ------------------------------------------------------------------

fn content(editor: &SurfaceLaunchSettings) -> SurfaceLaunchSettingsContent {
    match editor {
        SurfaceLaunchSettings::Closed => SurfaceLaunchSettingsContent::default(),
        SurfaceLaunchSettings::Loading(content) | SurfaceLaunchSettings::Ready(content) => {
            content.clone()
        }
        SurfaceLaunchSettings::Conflict(problem) | SurfaceLaunchSettings::Error(problem) => {
            problem.content.clone()
        }
    }
}
fn tag(editor: &SurfaceLaunchSettings) -> &'static str {
    match editor {
        SurfaceLaunchSettings::Closed => "Closed",
        SurfaceLaunchSettings::Loading(_) => "Loading",
        SurfaceLaunchSettings::Ready(_) => "Ready",
        SurfaceLaunchSettings::Conflict(_) => "Conflict",
        SurfaceLaunchSettings::Error(_) => "Error",
    }
}
fn action_ids(editor: &SurfaceLaunchSettings) -> Vec<String> {
    content(editor)
        .actions
        .into_iter()
        .map(|action| action.id)
        .collect()
}

fn choice(value: &str, label: &str) -> SurfaceSettingChoice {
    SurfaceSettingChoice {
        value: value.into(),
        label: label.into(),
    }
}
fn choices(item: &Option<SurfaceSettingItem>) -> Vec<SurfaceSettingChoice> {
    match item.as_ref().and_then(|item| item.interaction.clone()) {
        Some(SurfaceSettingInteraction::Choice { choices, .. }) => choices,
        _ => Vec::new(),
    }
}
fn selected(item: &Option<SurfaceSettingItem>) -> Option<String> {
    match item.as_ref().and_then(|item| item.interaction.clone()) {
        Some(SurfaceSettingInteraction::Choice { selected, .. }) => Some(selected),
        _ => None,
    }
}
fn source(item: &Option<SurfaceSettingItem>) -> Option<String> {
    item.as_ref().and_then(|item| item.source.clone())
}
fn value(item: &Option<SurfaceSettingItem>) -> Option<String> {
    item.as_ref().and_then(|item| item.value.clone())
}
fn labels(items: &[SurfaceSettingItem]) -> Vec<String> {
    items.iter().map(|item| item.label.clone()).collect()
}

// --- the controller against a scripted korrid -----------------------------------

fn ok_snapshot(snapshot: Value) -> Value {
    json!({ "_tag": "Ok", "payload": snapshot })
}
fn written_ok() -> Value {
    json!({ "_tag": "Ok", "payload": { "device": "device-2", "games": "games-2" } })
}
fn refused(code: &str, message: &str) -> Value {
    json!({ "_tag": "Err", "payload": { "code": code, "message": message } })
}

fn controller(korrid: crate::surface::korrid::Korrid) -> Arc<LaunchSettings> {
    LaunchSettings::new(korrid, Arc::new(|| {}))
}

async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(Duration::from_millis(5)).await;
}

fn calls(script: &Arc<std::sync::Mutex<super::acquisition_gate::Script>>, tag: &str) -> Vec<Value> {
    script
        .lock()
        .unwrap()
        .calls
        .iter()
        .filter(|(called, _)| called == tag)
        .map(|(_, payload)| payload.clone())
        .collect()
}

async fn opened(
    fixture: &Fixture,
    choose: Option<Arc<dyn Fn() + Send + Sync>>,
) -> (
    Arc<LaunchSettings>,
    Arc<std::sync::Mutex<super::acquisition_gate::Script>>,
) {
    let (korrid, script) = scripted();
    push(&script, SNAPSHOT, ok_snapshot(fixture.snapshot()));
    push(&script, SET, written_ok());
    let launch = controller(korrid);
    launch.open("wario".into(), "Wario Land 4".into(), None, choose);
    settle().await;
    (launch, script)
}

// --- clock steps ------------------------------------------------------------------

#[test]
fn clock_steps_are_the_rg353ms_real_steps_labelled_with_ghz() {
    let facts = rg353m();
    let steps = cpu_steps(Some(&facts));
    let values: Vec<String> = steps_choices(&steps)
        .iter()
        .map(|choice| choice.value.clone())
        .collect();
    assert_eq!(values, ["23", "33", "45", "61", "79", "89", "100"]);
    assert_eq!(steps_choices(&steps)[2].label, "45 % (0.82 GHz)");
    assert_eq!(steps_choices(&steps)[6].label, "100 % (1.80 GHz)");
    assert!(steps_choices(&gpu_steps(Some(&facts)))
        .iter()
        .any(|choice| choice.label == "50 % (0.40 GHz)"));
}

fn steps_choices(steps: &crate::surface::launch_model::Steps) -> Vec<SurfaceSettingChoice> {
    steps.choices.clone()
}

#[test]
fn clock_steps_fall_back_to_10_percent_steps_without_device_facts() {
    let steps = cpu_steps(None);
    assert!(steps.guessed);
    let labels: Vec<String> = steps_choices(&steps)
        .iter()
        .map(|choice| choice.label.clone())
        .collect();
    assert_eq!(
        labels,
        ["10 %", "20 %", "30 %", "40 %", "50 %", "60 %", "70 %", "80 %", "90 %", "100 %"]
    );
    assert!(governors(None).is_empty());
}

#[test]
fn clock_steps_come_from_the_policy_with_the_highest_top_clock() {
    let mut facts = rg353m();
    facts["cpuPolicies"] = json!([
        { "name": "policy0", "cpus": [0], "cpuinfoMaxFreq": 2_000_000, "availableFrequencies": [1_000_000, 2_000_000], "availableGovernors": ["schedutil", "performance"] },
        { "name": "policy4", "cpus": [4], "cpuinfoMaxFreq": 3_000_000, "availableFrequencies": [1_500_000, 3_000_000], "availableGovernors": ["schedutil"] },
    ]);
    let labels: Vec<String> = steps_choices(&cpu_steps(Some(&facts)))
        .iter()
        .map(|choice| choice.label.clone())
        .collect();
    assert_eq!(labels, ["50 % (1.50 GHz)", "100 % (3.00 GHz)"]);
    assert_eq!(governors(Some(&facts)), ["schedutil"]);
}

// --- device launch settings ---------------------------------------------------------

fn device_with_facts() -> Fixture {
    device_page().facts(rg353m()).value(sourced(
        editable(json!({ "_tag": "CpuMax" })),
        json!(100),
        device_default(),
    ))
}

#[tokio::test]
async fn device_settings_publish_a_launch_settings_group_with_source_device_only_mark_and_next_launch_note(
) {
    let (korrid, script) = scripted();
    push(
        &script,
        SNAPSHOT,
        ok_snapshot(device_with_facts().snapshot()),
    );
    let after = device_with_facts().body("device/any", json!({ "cpu": { "max": 45 } }));
    let mut after = after;
    after.resolved = vec![sourced(
        editable(json!({ "_tag": "CpuMax" })),
        json!(45),
        at("device/any"),
    )];
    push(&script, SNAPSHOT, ok_snapshot(after.snapshot()));
    push(&script, SET, written_ok());
    let launch = controller(korrid);
    launch.refresh_device().await;
    let groups = launch.device_groups().unwrap();
    assert_eq!(groups[0].title, "Launch settings");
    assert_eq!(
        labels(&groups[0].items),
        ["CPU min", "CPU max", "GPU min", "GPU max", "CPU governor"]
    );
    let max = &groups[0].items[1];
    assert_eq!(max.value.as_deref(), Some("Inherit"));
    assert_eq!(
        max.source.as_deref(),
        Some("100 % (1.80 GHz) from Device (default)")
    );
    assert_eq!(max.scope, Some(SurfaceSettingScope::Device));
    assert_eq!(
        max.description.as_deref(),
        Some("Applies at next launch. Sets the device's clock limits.")
    );
    assert_eq!(groups[0].items[4].value.as_deref(), Some("Inherit"));
    assert_eq!(
        groups[0].items[4].source.as_deref(),
        Some("schedutil from Device (default)")
    );

    assert!(launch.change(&launch_id::device("cpu.max"), "45"));
    assert_eq!(
        launch.status(),
        Some(SurfaceSettingsStatus::Saving {
            setting_id: launch_id::device("cpu.max")
        })
    );
    settle().await;
    assert_eq!(launch.status(), None);
    let sent = calls(&script, SET);
    assert_eq!(
        sent[0],
        json!({ "place": { "layer": { "_tag": "Device" }, "scope": { "_tag": "Any" } }, "path": { "_tag": "CpuMax" }, "value": 45, "clear": false, "expectedRevision": "device-1" })
    );
    let max = launch.device_groups().unwrap()[0].items[1].clone();
    assert_eq!(max.value.as_deref(), Some("45 % (0.82 GHz)"));
    assert_eq!(max.source.as_deref(), Some("from Device"));
}

#[test]
fn device_settings_show_one_read_only_gpu_line_on_a_device_without_a_devfreq_gpu() {
    let mut facts = rg353m();
    facts["gpuDevfreq"] = json!([]);
    let device = device_launch_from(&device_page().facts(facts).snapshot(), &HashMap::new());
    let items = &device.groups[0].items;
    assert_eq!(
        labels(items),
        [
            "CPU min",
            "CPU max",
            "GPU limits aren't available on this device",
            "CPU governor"
        ]
    );
    assert_eq!(items[2].interaction, None);
}

#[test]
fn device_settings_mark_steps_unknown_when_the_device_states_no_facts() {
    let device = device_launch_from(&device_page().snapshot(), &HashMap::new());
    assert_eq!(
        device.groups[0].items[0].description.as_deref(),
        Some("Applies at next launch. Sets the device's clock limits. Clock steps unknown.")
    );
    assert_eq!(
        device.groups[0].items[4].description.as_deref(),
        Some("Applies at next launch. Governors unknown.")
    );
}

#[tokio::test]
async fn device_settings_keep_the_group_after_a_failed_read_with_korris_message_and_retry() {
    let (korrid, script) = scripted();
    push(
        &script,
        SNAPSHOT,
        refused("Unavailable", "Korri could not read launch settings."),
    );
    push(
        &script,
        SNAPSHOT,
        ok_snapshot(device_with_facts().snapshot()),
    );
    let launch = controller(korrid);
    launch.refresh_device().await;
    let mut retry =
        SurfaceSettingItem::new(launch_id::DEVICE_RETRY, "Launch settings could not be read");
    retry.description = Some("Korri could not read launch settings.".into());
    retry.interaction = Some(SurfaceSettingInteraction::Action {
        action_id: launch_id::DEVICE_RETRY.into(),
        destructive: None,
        confirmation: None,
    });
    assert_eq!(launch.device_groups().unwrap()[0].items, vec![retry]);
    assert!(launch.act(launch_id::DEVICE_RETRY));
    settle().await;
    assert_eq!(
        labels(&launch.device_groups().unwrap()[0].items),
        ["CPU min", "CPU max", "GPU min", "GPU max", "CPU governor"]
    );
}

#[tokio::test]
async fn editor_reads_a_games_profile_pick_once_for_its_page() {
    let (korrid, script) = scripted();
    let mut fixture = wario();
    fixture.profiles = vec![("p1", "Quiet")];
    fixture.selected_profile = Some("p1");
    script
        .lock()
        .unwrap()
        .delays
        .insert(SNAPSHOT.into(), Duration::from_millis(5));
    push(&script, SNAPSHOT, ok_snapshot(fixture.snapshot()));
    let launch = controller(korrid);
    launch.learn("wario");
    launch.learn("wario");
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(
        launch.active_profiles().get("wario"),
        Some(&Some("Quiet".into()))
    );
    launch.learn("wario");
    settle().await;
    assert_eq!(calls(&script, SNAPSHOT).len(), 1);
}

#[tokio::test]
async fn editor_shows_a_failed_read_with_retry_and_closes_on_the_close_action() {
    let (korrid, script) = scripted();
    push(
        &script,
        SNAPSHOT,
        refused("Unavailable", "Korri could not read launch settings."),
    );
    let launch = controller(korrid);
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    let editor = launch.editor();
    let SurfaceLaunchSettings::Error(problem) = &editor else {
        panic!("{}", tag(&editor))
    };
    assert_eq!(problem.message, "Korri could not read launch settings.");
    assert!(problem.content.groups.is_empty());
    assert!(problem.content.notes.is_empty());
    let shown: Vec<String> = problem
        .content
        .actions
        .iter()
        .map(|action| action.label.clone())
        .collect();
    assert_eq!(shown, ["Retry", "Close"]);
    launch.act(launch_id::CANCEL);
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
}

#[tokio::test]
async fn editor_ignores_a_reply_that_arrives_after_the_editor_closed() {
    let (korrid, script) = scripted();
    script
        .lock()
        .unwrap()
        .delays
        .insert(SNAPSHOT.into(), Duration::from_millis(5));
    push(&script, SNAPSHOT, ok_snapshot(wario().snapshot()));
    let launch = controller(korrid);
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    tokio::task::yield_now().await;
    launch.cancel();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
}

fn skipped(key: &str, message: &str, runner_id: Option<&str>) -> Value {
    let mut payload = json!({ "path": setting(key), "message": message });
    if let Some(runner_id) = runner_id {
        payload["runnerId"] = json!(runner_id);
    }
    json!({ "_tag": "SettingSkipped", "payload": payload })
}

// --- Moonlight ------------------------------------------------------------------------

fn with_moonlight() -> Fixture {
    let device = format!("device/runner:{MOONLIGHT}");
    let game = format!("game:wario/runner:{MOONLIGHT}");
    let mut fixture = wario()
        .body(
            &device,
            json!({ "settings": { "video": { "fps": 30, "codec": null } } }),
        )
        .body(
            &game,
            json!({ "settings": { "audio": { "channels": "stereo" } } }),
        );
    fixture.runners.push(moonlight());
    fixture.runner_resolved = vec![(
        MOONLIGHT,
        vec![
            sourced(editable(setting("video.fps")), json!(30), at(&device)),
            sourced(
                editable(setting("video.codec")),
                json!("h264"),
                plugin_default("@korri:moonlight"),
            ),
            sourced(
                editable(setting("audio.channels")),
                json!("stereo"),
                at(&game),
            ),
        ],
    )];
    fixture
}

#[tokio::test]
async fn moonlight_is_offered_on_the_device_settings_page_with_add_setting_and_default() {
    let device = format!("device/runner:{MOONLIGHT}");
    let mut fixture = with_moonlight();
    fixture.game = false;
    fixture.runner_resolved[0].1.truncate(2);
    let (korrid, script) = scripted();
    push(&script, SNAPSHOT, ok_snapshot(fixture.snapshot()));
    push(&script, SET, written_ok());
    let launch = controller(korrid);
    launch.refresh_device().await;
    let groups = launch.device_groups().unwrap();
    let titles: Vec<String> = groups.iter().map(|group| group.title.clone()).collect();
    assert_eq!(
        titles,
        ["Launch settings", "Moonlight · Video", "Moonlight · Audio"]
    );
    let video: Vec<(String, Option<String>, Option<String>)> = groups[1]
        .items
        .iter()
        .map(|item| (item.label.clone(), item.value.clone(), item.source.clone()))
        .collect();
    let some = |text: &str| Some(text.to_owned());
    assert_eq!(
        video,
        [
            (
                "FPS".to_owned(),
                some("30"),
                some("from Device for Moonlight")
            ),
            ("Default FPS".to_owned(), some("Use"), None),
            (
                "Codec".to_owned(),
                some("Default"),
                some("h264 from Moonlight (default)")
            ),
            ("Add setting".to_owned(), some("Choose a setting"), None),
        ]
    );
    assert_eq!(
        groups[1].items[0].id,
        launch_id::device_set(&device, "setting:video.fps")
    );

    // Add setting shows the row with nothing written; a choice writes it.
    assert!(launch.change(
        &launch_id::device_set(&device, "add-setting:audio"),
        "audio.channels"
    ));
    assert!(calls(&script, SET).is_empty());
    assert_eq!(
        labels(&launch.device_groups().unwrap()[2].items),
        ["Channels"]
    );
    push(&script, SNAPSHOT, ok_snapshot(fixture.snapshot()));
    launch.change(
        &launch_id::device_set(&device, "setting:audio.channels"),
        "\"5.1\"",
    );
    settle().await;
    let sent = calls(&script, SET);
    assert_eq!(sent[0]["path"], setting("audio.channels"));
    assert_eq!(sent[0]["value"], json!("5.1"));

    // Default on a number writes a Cleared setting.
    assert!(launch.act(&launch_id::device_set(&device, "default:setting:video.fps")));
    settle().await;
    let sent = calls(&script, SET);
    assert_eq!(sent[1]["path"], setting("video.fps"));
    assert_eq!(sent[1]["clear"], json!(true));
}

// --- Sunshine ---------------------------------------------------------------------------

fn with_sunshine() -> Fixture {
    let mut fixture = wario()
        .body("device/any", json!({ "services": { SUNSHINE: { "settings": { "video": { "max-bitrate-kbps": 3000 } } } } }))
        .value(sourced(
            editable(json!({ "_tag": "ServiceSetting", "payload": { "id": SUNSHINE, "key": "video.max-bitrate-kbps" } })),
            json!(3000),
            at("device/any"),
        ))
        .value(sourced(
            editable(json!({ "_tag": "ServiceSetting", "payload": { "id": SUNSHINE, "key": "input.gamepad" } })),
            json!("auto"),
            plugin_default("@korri:sunshine"),
        ));
    fixture.services = vec![json!({
        "id": SUNSHINE,
        "title": "Sunshine",
        "schema": {
            "type": "object",
            "properties": {
                "video": section("Video", json!({ "max-bitrate-kbps": { "type": "integer", "title": "Max bitrate (kbps)", "minimum": 0 } })),
                "input": section("Input", json!({ "gamepad": { "type": "string", "title": "Gamepad", "enum": ["auto", "ds5"], "default": "auto" } })),
            },
        },
    })];
    fixture
}

#[test]
fn sunshine_shows_on_the_device_settings_page() {
    let mut fixture = with_sunshine();
    fixture.game = false;
    let device = device_launch_from(&fixture.snapshot(), &HashMap::new());
    let titles: Vec<String> = device
        .groups
        .iter()
        .map(|group| group.title.clone())
        .collect();
    assert_eq!(
        titles,
        ["Launch settings", "Sunshine · Video", "Sunshine · Input"]
    );
}

// --- encoders and place keys ----------------------------------------------------------

#[test]
fn encoders_take_only_what_a_row_offers() {
    assert_eq!(
        Encoder::Percent.encode("inherit"),
        Some(LaunchEncoded {
            value: None,
            clear: false
        })
    );
    assert_eq!(
        Encoder::Percent.encode("45"),
        Some(LaunchEncoded {
            value: Some(json!(45)),
            clear: false
        })
    );
    assert_eq!(Encoder::Percent.encode("101"), None);
    assert_eq!(Encoder::Percent.encode("4.5"), None);
    assert_eq!(
        Encoder::Boolean.encode("default"),
        Some(LaunchEncoded {
            value: None,
            clear: true
        })
    );
    assert_eq!(Encoder::Boolean.encode("maybe"), None);
    let number = Encoder::Number {
        integer: true,
        minimum: Some(0.0),
        maximum: Some(10.0),
    };
    assert_eq!(
        number.encode(""),
        Some(LaunchEncoded {
            value: None,
            clear: false
        })
    );
    assert_eq!(number.encode("2.5"), None);
    assert_eq!(number.encode("Infinity"), None);
    assert_eq!(number.encode("abc"), None);
    let floating = Encoder::Number {
        integer: false,
        minimum: None,
        maximum: None,
    };
    assert_eq!(
        floating.encode("2.5"),
        Some(LaunchEncoded {
            value: Some(json!(2.5)),
            clear: false
        })
    );
    let choices = Encoder::Enum(vec![json!("h264"), json!(5)]);
    assert_eq!(
        choices.encode("\"h264\""),
        Some(LaunchEncoded {
            value: Some(json!("h264")),
            clear: false
        })
    );
    assert_eq!(
        choices.encode("5"),
        Some(LaunchEncoded {
            value: Some(json!(5)),
            clear: false
        })
    );
    assert_eq!(choices.encode("h264"), None);
    assert_eq!(
        Encoder::Text.encode(""),
        Some(LaunchEncoded {
            value: Some(json!("")),
            clear: false
        })
    );
}

#[test]
fn place_keys_are_stable_per_layer() {
    assert_eq!(layer_key(&json!({ "_tag": "Device" })), "device");
    assert_eq!(
        layer_key(&json!({ "_tag": "System", "id": "gba" })),
        "system:gba"
    );
    assert_eq!(
        layer_key(&json!({ "_tag": "Game", "id": "wl4" })),
        "game:wl4"
    );
    assert_eq!(
        layer_key(&json!({ "_tag": "Release", "id": { "gameId": "wl4", "releaseId": "usa" } })),
        "release:usa"
    );
    assert_eq!(
        layer_key(&json!({ "_tag": "Profile", "id": { "gameId": "wl4", "profileId": "p1" } })),
        "profile:p1"
    );
    assert_eq!(layer_key(&json!({ "_tag": "Launch" })), "launch");
}

// --- the one list (ADR 0066) -----------------------------------------------------

const FRAMESKIP: &str = "launch:row:setting:video.frameskip";
const RENDER_SCALE: &str = "launch:row:setting:video.render-scale";
const SPEED: &str = "launch:row:runner:retroarch/mgba:setting:plugin.speed";
const GPSP_SPEED: &str = "launch:row:runner:retroarch/gpsp:setting:plugin.speed";
const CODEC: &str = "launch:stream:@korri:moonlight/moonlight#video.codec";

fn limit(tag: &str, value: Value) -> Value {
    sourced(editable(json!({ "_tag": tag })), value, device_default())
}

/// Wario Land 4 as a device with clock facts resolves it: frameskip 3 from
/// its system, the device's own limits and governor.
fn resolved_wario() -> Fixture {
    wario()
        .facts(rg353m())
        .value(sourced(
            editable(setting("video.frameskip")),
            json!(3),
            at("system:gba/any"),
        ))
        .value(limit("CpuMin", json!(0)))
        .value(limit("CpuMax", json!(100)))
        .value(limit("GpuMin", json!(0)))
        .value(limit("GpuMax", json!(100)))
        .value(limit("CpuGovernor", json!("schedutil")))
}

/// A runner that takes Korri's render scale and has one setting of its own.
fn shared_and_own() -> Fixture {
    let mut fixture = resolved_wario().value(sourced(
        editable(setting("video.render-scale")),
        json!(4),
        at("device/any"),
    ));
    fixture.runners[0].schema = korri_settings_treaty::schema(
        &[korri_settings_treaty::SharedSetting::RenderScale],
        json!({ "type": "object", "properties": { "speed": { "type": "integer", "title": "Speed" } } }),
    );
    fixture.modifiers = vec![json!({ "id": "korri/gamescope", "title": "Gamescope" })];
    fixture
}

fn titles(shown: &SurfaceLaunchSettingsContent) -> Vec<String> {
    shown
        .groups
        .iter()
        .map(|group| group.title.clone())
        .collect()
}
fn row(shown: &SurfaceLaunchSettingsContent, label: &str) -> Option<SurfaceSettingItem> {
    shown
        .groups
        .iter()
        .flat_map(|group| &group.items)
        .find(|item| item.label == label)
        .cloned()
}
fn changes(shown: &SurfaceLaunchSettingsContent) -> Vec<(String, String)> {
    shown
        .changes
        .iter()
        .map(|change| (change.id.clone(), change.was.clone()))
        .collect()
}
fn shown(launch: &LaunchSettings) -> SurfaceLaunchSettingsContent {
    content(&launch.editor())
}

type Calls = Arc<std::sync::Mutex<Vec<(String, Value)>>>;

/// A korrid that answers by what a request asks, as a holder and this device
/// answer differently for one call.
fn answering(
    answer: impl Fn(&str, &Value) -> Value + Send + Sync + 'static,
) -> (crate::surface::korrid::Korrid, Calls) {
    let calls: Calls = Arc::default();
    let seen = Arc::clone(&calls);
    let answer = Arc::new(answer);
    let korrid =
        crate::surface::korrid::Korrid::new(Arc::new(move |request: crate::RpcRequest| {
            let seen = Arc::clone(&seen);
            let answer = Arc::clone(&answer);
            Box::pin(async move {
                let value = serde_json::to_value(&request).unwrap();
                let tag = value["_tag"].as_str().unwrap().to_owned();
                seen.lock()
                    .unwrap()
                    .push((tag.clone(), value["payload"].clone()));
                let outcome = answer(&tag, &value["payload"]);
                serde_json::from_value::<crate::RpcResponse>(
                    json!({ "_tag": tag, "outcome": outcome }),
                )
                .map_err(|error| crate::surface::korrid::Refusal::Unreachable(error.to_string()))
            })
        }));
    (korrid, calls)
}
fn asked(calls: &Calls, tag: &str) -> Vec<Value> {
    calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(called, _)| called == tag)
        .map(|(_, payload)| payload.clone())
        .collect()
}

/// This device's own launch settings: a device page with a stream viewer.
fn this_device() -> Fixture {
    let mut fixture = with_moonlight();
    fixture.game = false;
    fixture.runners.retain(|runner| !runner.choosable);
    fixture.resolved.clear();
    fixture.bodies.clear();
    fixture.runner_resolved[0].1.truncate(2);
    fixture
}

/// The phone: Wario Land 4 runs on aka. `reachable` says whether aka answers.
fn streamed(
    holder: Fixture,
    device: Fixture,
    reachable: Arc<std::sync::atomic::AtomicBool>,
) -> (Arc<LaunchSettings>, Calls) {
    let (korrid, calls) = answering(move |tag, payload| {
        let to_holder = payload["holder"] == "aka";
        if to_holder && !reachable.load(std::sync::atomic::Ordering::SeqCst) {
            return refused("UpstreamUnreachable", "upstream unreachable: timed out");
        }
        match tag {
            SNAPSHOT if to_holder => ok_snapshot(holder.snapshot()),
            SNAPSHOT => ok_snapshot(device.snapshot()),
            _ => written_ok(),
        }
    });
    (controller(korrid), calls)
}
fn open_on_aka(launch: &Arc<LaunchSettings>) {
    launch.open(
        "wario".into(),
        "Wario Land 4".into(),
        Some("aka".into()),
        None,
    );
}
fn up() -> Arc<std::sync::atomic::AtomicBool> {
    Arc::new(std::sync::atomic::AtomicBool::new(true))
}

#[tokio::test]
async fn the_editor_is_one_list_with_each_setting_once_and_the_value_the_next_launch_uses() {
    let (launch, script) = opened(&resolved_wario(), None).await;
    let shown = shown(&launch);
    assert_eq!(tag(&launch.editor()), "Ready");
    assert_eq!(shown.runs_on.as_deref(), Some("Runs here"));
    assert_eq!(
        titles(&shown),
        [
            "Video",
            "Input",
            "Language",
            "Performance",
            "Runs with",
            "Profile"
        ]
    );
    let mut ids: Vec<String> = shown
        .groups
        .iter()
        .flat_map(|group| &group.items)
        .map(|item| item.id.clone())
        .collect();
    let all = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), all, "no setting appears twice");
    // The value in force, wherever it is stored; a setting nothing sets reads Default.
    assert_eq!(value(&row(&shown, "Frameskip")), Some("3".into()));
    assert_eq!(
        value(&row(&shown, "Colour correction")),
        Some("Default".into())
    );
    assert_eq!(
        value(&row(&shown, "CPU max")),
        Some("100 % (1.80 GHz)".into())
    );
    assert_eq!(
        value(&row(&shown, "CPU governor")),
        Some("schedutil".into())
    );
    assert_eq!(value(&row(&shown, "Runner")), Some("mGBA".into()));
    // No changes yet: no mark and no keep row. Nothing was written.
    assert!(shown.changes.is_empty());
    assert!(shown.keep.is_none());
    assert_eq!(calls(&script, SNAPSHOT), [json!({ "gameId": "wario" })]);
    assert!(calls(&script, SET).is_empty());
}

#[tokio::test]
async fn a_changed_row_is_marked_with_its_earlier_value_and_setting_it_back_removes_the_mark() {
    let (launch, script) = opened(&resolved_wario(), None).await;
    assert!(launch.change(FRAMESKIP, "5"));
    let now = shown(&launch);
    assert_eq!(value(&row(&now, "Frameskip")), Some("5".into()));
    assert_eq!(changes(&now), [(FRAMESKIP.to_owned(), "3".to_owned())]);
    let keep = now.keep.clone();
    assert_eq!(
        keep.as_ref().map(|keep| keep.label.as_str()),
        Some("Keep 1 change")
    );
    // This game is the default; the other places follow in a fixed order.
    assert_eq!(value(&keep), Some("This game".into()));
    assert_eq!(selected(&keep), Some("game:wario".into()));
    assert_eq!(
        choices(&keep),
        [
            choice("launch", "This launch"),
            choice("game:wario", "This game"),
            choice("device", "Every game"),
            choice("system:gba", "System Game Boy Advance"),
            choice("release:usa", "Release usa"),
        ]
    );
    assert!(launch.change("launch:row:cpu.max", "89"));
    let now = shown(&launch);
    assert_eq!(
        now.keep.as_ref().map(|keep| keep.label.as_str()),
        Some("Keep 2 changes")
    );
    assert_eq!(
        changes(&now),
        [
            (FRAMESKIP.to_owned(), "3".to_owned()),
            (
                "launch:row:cpu.max".to_owned(),
                "100 % (1.80 GHz)".to_owned()
            ),
        ]
    );
    // Back at the earlier value: the mark goes, and with the last one the keep row.
    assert!(launch.change(FRAMESKIP, "3"));
    assert_eq!(changes(&shown(&launch)).len(), 1);
    assert!(launch.change("launch:row:cpu.max", "100"));
    let now = shown(&launch);
    assert!(now.changes.is_empty());
    assert!(now.keep.is_none());
    assert!(calls(&script, SET).is_empty(), "a change is not a write");
}

#[tokio::test]
async fn a_value_a_row_does_not_take_is_refused_on_the_row() {
    let (launch, _) = opened(&resolved_wario(), None).await;
    assert!(launch.change(FRAMESKIP, "99"));
    assert_eq!(
        launch.status(),
        Some(SurfaceSettingsStatus::Problem {
            setting_id: FRAMESKIP.into(),
            message: "That value isn't allowed here.".into(),
        })
    );
    assert!(shown(&launch).changes.is_empty());
}

#[tokio::test]
async fn closing_the_editor_keeps_the_changes_for_this_game_each_at_its_scope() {
    let fixture = shared_and_own();
    let (launch, script) = opened(&fixture, None).await;
    assert_eq!(
        value(&row(&shown(&launch), "Render scale")),
        Some("4x".into())
    );
    assert!(launch.change(RENDER_SCALE, "2"));
    assert!(launch.change(SPEED, "3"));
    assert!(launch.change("launch:row:modifier:korri/gamescope:enabled", "true"));
    assert!(launch.change("launch:row:cpu.governor", "performance"));
    let now = shown(&launch);
    assert_eq!(value(&row(&now, "Render scale")), Some("2x".into()));
    assert_eq!(changes(&now)[0], (RENDER_SCALE.to_owned(), "4x".to_owned()));
    assert!(calls(&script, SET).is_empty());

    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    let writes = calls(&script, SET);
    let game = |scope: Value| json!({ "layer": { "_tag": "Game", "id": "wario" }, "scope": scope });
    let sent: Vec<(Value, Value, Value, Value)> = writes
        .iter()
        .map(|write| {
            (
                write["place"].clone(),
                write["path"].clone(),
                write["value"].clone(),
                write["expectedRevision"].clone(),
            )
        })
        .collect();
    assert_eq!(
        sent,
        [
            // A shared value is kept for any runner.
            (
                game(json!({ "_tag": "Any" })),
                setting("video.render-scale"),
                json!(2),
                json!("games-1")
            ),
            // A runner-only value is kept for its runner. Nobody chose a scope.
            (
                game(json!({ "_tag": "Runner", "id": "retroarch/mgba" })),
                setting("plugin.speed"),
                json!(3),
                json!("games-2")
            ),
            (
                game(json!({ "_tag": "Any" })),
                json!({ "_tag": "ModifierEnabled", "payload": { "id": "korri/gamescope" } }),
                json!(true),
                json!("games-2")
            ),
            // The governor is one device value: the device keeps it.
            (
                json!({ "layer": { "_tag": "Device" }, "scope": { "_tag": "Any" } }),
                json!({ "_tag": "CpuGovernor" }),
                json!("performance"),
                json!("device-2")
            ),
        ]
    );
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    // Kept: the next launch carries nothing, and the list has no change left.
    assert!(launch.submit_one_launch("wario", None).1.is_none());
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    assert!(shown(&launch).changes.is_empty());
}

#[tokio::test]
async fn a_cleared_number_is_kept_as_the_runners_default() {
    let (launch, script) = opened(&resolved_wario(), None).await;
    assert!(launch.change(FRAMESKIP, ""));
    let now = shown(&launch);
    assert_eq!(value(&row(&now, "Frameskip")), Some("Default".into()));
    assert_eq!(changes(&now), [(FRAMESKIP.to_owned(), "3".to_owned())]);
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    let writes = calls(&script, SET);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0]["clear"], json!(true));
    assert!(writes[0].get("value").is_none());
}

#[tokio::test]
async fn changes_kept_for_every_game_go_to_the_device() {
    let (launch, script) = opened(&resolved_wario(), None).await;
    assert!(launch.change(FRAMESKIP, "5"));
    assert!(launch.change("launch:keep", "device"));
    let keep = shown(&launch).keep;
    assert_eq!(value(&keep), Some("Every game".into()));
    assert_eq!(
        keep.and_then(|keep| keep.description),
        Some("Used by every game that has no value of its own.".into())
    );
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    let writes = calls(&script, SET);
    assert_eq!(
        writes[0]["place"],
        json!({ "layer": { "_tag": "Device" }, "scope": { "_tag": "Any" } })
    );
    assert_eq!(writes[0]["expectedRevision"], json!("device-1"));
}

#[tokio::test]
async fn changes_for_this_launch_go_with_that_launch_and_end_with_it() {
    let (launch, script) = opened(&shared_and_own(), None).await;
    assert!(launch.change(RENDER_SCALE, "2"));
    assert!(launch.change(SPEED, "3"));
    assert!(launch.change("launch:row:cpu.governor", "performance"));
    assert!(launch.change("launch:keep", "launch"));
    let keep = shown(&launch).keep;
    assert_eq!(value(&keep), Some("This launch".into()));
    assert_eq!(
        keep.and_then(|keep| keep.description),
        Some("They end when you stop playing.".into())
    );
    // Closing writes nothing: the values wait for the launch.
    assert!(launch.act(launch_id::CANCEL));
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    settle().await;
    assert!(calls(&script, SET).is_empty());
    let (ticket, values) = launch.submit_one_launch("wario", None);
    assert_eq!(
        serde_json::to_value(values.expect("one-launch values")).unwrap(),
        json!({
            // No runner scope on a shared value; a runner-only value names its runner.
            "settings": { "video": { "render-scale": 2 } },
            "plugin": { "retroarch/mgba": { "speed": 3 } },
            "modifiers": {},
            "cpu": { "governor": "performance" },
        })
    );
    // A failed launch keeps them; a launch that started ends them.
    launch.complete_one_launch(ticket, false);
    let (ticket, values) = launch.submit_one_launch("wario", None);
    assert!(values.is_some());
    launch.complete_one_launch(ticket, true);
    assert!(launch.submit_one_launch("wario", None).1.is_none());
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    assert!(shown(&launch).changes.is_empty());
    assert!(calls(&script, SET).is_empty());
}

#[tokio::test]
async fn a_write_korrid_refuses_as_stale_asks_for_a_reload_and_keeps_the_change() {
    let (korrid, script) = scripted();
    push(&script, SNAPSHOT, ok_snapshot(resolved_wario().snapshot()));
    push(&script, SET, refused("SettingsConflict", "changed"));
    let launch = controller(korrid);
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    assert!(launch.change(FRAMESKIP, "5"));
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    let editor = launch.editor();
    let SurfaceLaunchSettings::Conflict(problem) = &editor else {
        panic!("{}", tag(&editor))
    };
    assert!(problem.message.contains("changed elsewhere"));
    assert_eq!(action_ids(&editor), [launch_id::RELOAD, launch_id::CANCEL]);
    assert_eq!(changes(&problem.content).len(), 1, "the change is not lost");
    launch.act(launch_id::RELOAD);
    settle().await;
    assert_eq!(tag(&launch.editor()), "Ready");
    assert_eq!(changes(&shown(&launch)).len(), 1);
    // Closing an editor that could not write leaves without writing again.
    push(&script, SET, refused("SettingsConflict", "changed"));
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    assert_eq!(tag(&launch.editor()), "Conflict");
    assert!(launch.act(launch_id::CANCEL));
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    assert_eq!(calls(&script, SET).len(), 2);
}

/// mGBA and gpSP each have a setting of their own with the same key.
fn two_runners_with_speed(selected: &'static str) -> Fixture {
    let mut fixture = shared_and_own();
    fixture.runners[1].schema = fixture.runners[0].schema.clone();
    fixture.selected = Some(selected);
    fixture
}

#[tokio::test]
async fn a_runner_only_change_stays_with_its_runner_when_another_runner_is_chosen() {
    let (korrid, script) = scripted();
    let mgba = ok_snapshot(two_runners_with_speed("retroarch/mgba").snapshot());
    let gpsp = ok_snapshot(two_runners_with_speed("retroarch/gpsp").snapshot());
    push(&script, SNAPSHOT, mgba.clone());
    push(&script, SET, written_ok());
    let launch = controller(korrid);
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    assert_eq!(
        row(&shown(&launch), "Speed").map(|item| item.id),
        Some(SPEED.into())
    );
    assert!(launch.change(SPEED, "3"));
    assert_eq!(
        changes(&shown(&launch)),
        [(SPEED.to_owned(), "Default".to_owned())]
    );

    // gpSP is chosen. Its row of the same key is its own row: mGBA's change
    // is not shown on it and is not counted.
    push(&script, SNAPSHOT, gpsp);
    assert!(launch.act(launch_id::RELOAD));
    settle().await;
    let now = shown(&launch);
    assert!(titles(&now).contains(&"gpSP only".to_owned()));
    let speed = row(&now, "Speed").expect("gpSP's own row");
    assert_eq!(speed.id, GPSP_SPEED);
    assert_eq!(speed.value.as_deref(), Some("Default"));
    assert!(now.changes.is_empty());
    assert!(now.keep.is_none());
    assert!(launch.change(RENDER_SCALE, "2"));
    assert!(launch.change(GPSP_SPEED, "7"));
    let now = shown(&launch);
    assert_eq!(
        now.keep.as_ref().map(|keep| keep.label.as_str()),
        Some("Keep 2 changes")
    );
    assert_eq!(
        changes(&now),
        [
            (RENDER_SCALE.to_owned(), "4x".to_owned()),
            (GPSP_SPEED.to_owned(), "Default".to_owned()),
        ]
    );
    // A launch carries what the list shows, and mGBA's change is not in it.
    assert!(launch.change("launch:keep", "launch"));
    let (ticket, values) = launch.submit_one_launch("wario", None);
    assert_eq!(
        serde_json::to_value(values.expect("one-launch values")).unwrap(),
        json!({
            "settings": { "video": { "render-scale": 2 } },
            "plugin": { "retroarch/gpsp": { "speed": 7 } },
            "modifiers": {},
        })
    );
    launch.complete_one_launch(ticket, true);
    assert!(shown(&launch).changes.is_empty());
    // Closing writes nothing: mGBA's change waits for mGBA.
    assert_eq!(launch.close().await, Closing::Kept);
    assert!(calls(&script, SET).is_empty());

    // mGBA is chosen again: its change shows again, and is kept for mGBA.
    push(&script, SNAPSHOT, mgba);
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    let now = shown(&launch);
    assert_eq!(value(&row(&now, "Speed")), Some("3".into()));
    assert_eq!(changes(&now), [(SPEED.to_owned(), "Default".to_owned())]);
    assert_eq!(
        now.keep.as_ref().map(|keep| keep.label.as_str()),
        Some("Keep 1 change")
    );
    assert_eq!(launch.close().await, Closing::Kept);
    let writes = calls(&script, SET);
    assert_eq!(writes.len(), 1);
    assert_eq!(
        writes[0]["place"],
        json!({
            "layer": { "_tag": "Game", "id": "wario" },
            "scope": { "_tag": "Runner", "id": "retroarch/mgba" },
        })
    );
    assert_eq!(writes[0]["path"], setting("plugin.speed"));
    assert_eq!(writes[0]["value"], json!(3));
}

/// Launch once names its runner. The launch carries that runner's values
/// and ends only those; the saved runner's values still wait.
#[tokio::test]
async fn a_launch_carries_and_ends_only_the_one_launch_values_of_the_runner_it_uses() {
    let (korrid, script) = scripted();
    let mgba = ok_snapshot(two_runners_with_speed("retroarch/mgba").snapshot());
    let gpsp = ok_snapshot(two_runners_with_speed("retroarch/gpsp").snapshot());
    push(&script, SNAPSHOT, mgba.clone());
    let launch = controller(korrid);
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    assert!(launch.change(SPEED, "3"));
    assert!(launch.change("launch:keep", "launch"));
    // gpSP is saved, and gets a one-launch value of its own.
    push(&script, SNAPSHOT, gpsp);
    assert!(launch.act(launch_id::RELOAD));
    settle().await;
    assert!(shown(&launch).changes.is_empty());
    assert!(launch.change(GPSP_SPEED, "7"));
    assert!(launch.change("launch:keep", "launch"));

    // Launch once with mGBA.
    let (ticket, values) = launch.submit_one_launch("wario", Some("retroarch/mgba"));
    assert_eq!(
        serde_json::to_value(values.expect("one-launch values")).unwrap(),
        json!({ "plugin": { "retroarch/mgba": { "speed": 3 } }, "modifiers": {} })
    );
    launch.complete_one_launch(ticket, true);
    // gpSP's value was not used: it still waits, for this launch.
    let now = shown(&launch);
    assert_eq!(
        changes(&now),
        [(GPSP_SPEED.to_owned(), "Default".to_owned())]
    );
    assert_eq!(value(&now.keep), Some("This launch".into()));
    // A launch that names no runner uses the saved one.
    let (ticket, values) = launch.submit_one_launch("wario", None);
    assert_eq!(
        serde_json::to_value(values.expect("one-launch values")).unwrap(),
        json!({ "plugin": { "retroarch/gpsp": { "speed": 7 } }, "modifiers": {} })
    );
    launch.complete_one_launch(ticket, true);
    assert!(shown(&launch).changes.is_empty());
    // mGBA's value ended with its launch.
    push(&script, SNAPSHOT, mgba);
    assert!(launch.act(launch_id::RELOAD));
    settle().await;
    let now = shown(&launch);
    assert!(now.changes.is_empty());
    assert_eq!(value(&row(&now, "Speed")), Some("Default".into()));
    assert!(calls(&script, SET).is_empty());
}

/// A change waits only for a runner the game still offers.
#[tokio::test]
async fn a_runner_only_change_goes_when_the_game_no_longer_offers_its_runner() {
    let (korrid, script) = scripted();
    push(
        &script,
        SNAPSHOT,
        ok_snapshot(two_runners_with_speed("retroarch/mgba").snapshot()),
    );
    let launch = controller(korrid);
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    assert!(launch.change(SPEED, "3"));
    let mut gone = two_runners_with_speed("retroarch/gpsp");
    gone.runners.remove(0);
    push(&script, SNAPSHOT, ok_snapshot(gone.snapshot()));
    assert!(launch.act(launch_id::RELOAD));
    settle().await;
    assert!(shown(&launch).changes.is_empty());
    push(
        &script,
        SNAPSHOT,
        ok_snapshot(two_runners_with_speed("retroarch/mgba").snapshot()),
    );
    assert!(launch.act(launch_id::RELOAD));
    settle().await;
    assert!(shown(&launch).changes.is_empty());
    assert_eq!(
        value(&row(&shown(&launch), "Speed")),
        Some("Default".into())
    );
}

/// The game's start closes the editor through `close`, and starts only on
/// `Kept`.
#[tokio::test]
async fn closing_for_a_game_start_says_whether_every_change_was_kept() {
    // Nothing waits, then one change that the device takes.
    let (launch, script) = opened(&resolved_wario(), None).await;
    assert_eq!(launch.close().await, Closing::Kept);
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    assert!(launch.change(FRAMESKIP, "5"));
    assert_eq!(launch.close().await, Closing::Kept);
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    assert_eq!(calls(&script, SET).len(), 1);

    // Changes for this launch are not written: the launch carries them.
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    assert!(launch.change(FRAMESKIP, "7"));
    assert!(launch.change("launch:keep", "launch"));
    assert_eq!(launch.close().await, Closing::Kept);
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    assert_eq!(calls(&script, SET).len(), 1);
    assert!(launch.submit_one_launch("wario", None).1.is_some());

    for (code, state) in [
        ("SettingsConflict", "Conflict"),
        ("SettingsInvalid", "Error"),
    ] {
        let (korrid, script) = scripted();
        push(&script, SNAPSHOT, ok_snapshot(resolved_wario().snapshot()));
        push(&script, SET, refused(code, "refused"));
        let launch = controller(korrid);
        launch.open("wario".into(), "Wario Land 4".into(), None, None);
        settle().await;
        assert!(launch.change(FRAMESKIP, "5"));
        assert!(launch.change("launch:row:cpu.max", "89"));
        // The first write is refused: the editor stays open with both changes.
        assert_eq!(launch.close().await, Closing::NotKept);
        assert_eq!(tag(&launch.editor()), state);
        assert_eq!(changes(&shown(&launch)).len(), 2);
        assert_eq!(calls(&script, SET).len(), 1);
        // A second start finds the changes still not written. It writes
        // nothing and the editor stays as it is.
        assert_eq!(launch.close().await, Closing::NotKept);
        assert_eq!(tag(&launch.editor()), state);
        assert_eq!(changes(&shown(&launch)).len(), 2);
        assert_eq!(calls(&script, SET).len(), 1);
        // The Close action still leaves, and the changes stay with the game.
        assert!(launch.act(launch_id::CANCEL));
        assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
        assert_eq!(launch.close().await, Closing::Kept, "no editor is open");
    }
}

#[tokio::test]
async fn a_setting_the_runner_cannot_honour_cannot_change_and_says_so() {
    let mut fixture = shared_and_own();
    fixture.notices = vec![skipped(
        "video.fps-limit",
        "mGBA does not support this setting",
        Some("retroarch/mgba"),
    )];
    let (launch, _) = opened(&fixture, None).await;
    let now = shown(&launch);
    let fixed = row(&now, "FPS limit").expect("the value that waits on the runner shows");
    assert!(fixed.interaction.is_none());
    assert_eq!(fixed.description.as_deref(), Some("Not supported by mGBA."));
    assert!(row(&now, "Render scale").is_some_and(|item| item.interaction.is_some()));
    // A shared setting the runner cannot honour and nothing sets is not listed.
    assert!(row(&now, "Vsync").is_none());
    assert!(!launch.change("launch:row:setting:video.fps-limit", "30") || now.changes.is_empty());
    assert!(shown(&launch).changes.is_empty());
}

#[tokio::test]
async fn a_runner_that_declares_no_settings_shows_only_the_rows_korri_owns() {
    let mut fixture = resolved_wario();
    fixture.runners[0].schema =
        korri_settings_treaty::schema(&[], json!({ "type": "object", "properties": {} }));
    fixture.modifiers = vec![json!({ "id": "korri/gamescope", "title": "Gamescope" })];
    let (launch, _) = opened(&fixture, None).await;
    assert_eq!(tag(&launch.editor()), "Ready");
    let now = shown(&launch);
    assert_eq!(titles(&now), ["Performance", "Runs with", "Profile"]);
    assert_eq!(
        labels(&now.groups[0].items),
        ["CPU min", "CPU max", "CPU governor", "GPU min", "GPU max"]
    );
    assert_eq!(labels(&now.groups[1].items), ["Runner", "Gamescope"]);
    assert_eq!(value(&row(&now, "Gamescope")), Some("Off".into()));
}

#[tokio::test]
async fn the_runner_row_opens_the_chooser_when_the_page_offers_it() {
    let mut fixture = resolved_wario();
    fixture.selected = None;
    fixture.route_layers = json!({ "_tag": "NeedsRunnerChoice" });
    let chosen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = Arc::clone(&chosen);
    let (launch, _) = opened(
        &fixture,
        Some(Arc::new(move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })),
    )
    .await;
    let now = shown(&launch);
    let runner = row(&now, "Runner").unwrap();
    assert_eq!(runner.value.as_deref(), Some("Not chosen"));
    assert_eq!(
        runner.interaction,
        Some(SurfaceSettingInteraction::Action {
            action_id: launch_id::RUNNER.into(),
            destructive: None,
            confirmation: None,
        })
    );
    // No runner yet: its settings wait, and the list says so.
    assert_eq!(titles(&now), ["Performance", "Runs with", "Profile"]);
    assert!(now.notes[0].contains("once you choose a runner"));
    assert_eq!(
        action_ids(&launch.editor()),
        [launch_id::RUNNER, launch_id::RELOAD, launch_id::CANCEL]
    );
    assert!(launch.act(launch_id::RUNNER));
    assert_eq!(chosen.load(std::sync::atomic::Ordering::SeqCst), 1);

    // A page without the chooser shows the runner and cannot change it.
    let (launch, _) = opened(&resolved_wario(), None).await;
    let runner = row(&shown(&launch), "Runner").unwrap();
    assert_eq!(runner.value.as_deref(), Some("mGBA"));
    assert!(runner.interaction.is_none());
}

#[tokio::test]
async fn a_profile_is_picked_made_named_and_deleted_from_the_list() {
    let mut fixture = resolved_wario();
    fixture.profiles = vec![("p1", "Quiet")];
    fixture.selected_profile = Some("p1");
    let (korrid, script) = scripted();
    push(&script, SNAPSHOT, ok_snapshot(fixture.snapshot()));
    for call in [
        "app.launch-settings.profile.select",
        "app.launch-settings.profile.save",
        "app.launch-settings.profile.remove",
    ] {
        push(&script, call, written_ok());
    }
    let launch = controller(korrid);
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    let now = shown(&launch);
    assert_eq!(
        labels(&now.groups.last().unwrap().items),
        [
            "Profile at next launch",
            "New profile",
            "Name",
            "Delete profile"
        ]
    );
    assert_eq!(now.selected_profile_id.as_deref(), Some("p1"));
    // The profile is a place the changes can be kept in.
    assert!(launch.change(FRAMESKIP, "5"));
    assert!(choices(&shown(&launch).keep).contains(&choice("profile:p1", "Profile Quiet")));
    assert!(launch.change(FRAMESKIP, "3"));

    assert!(launch.change(launch_id::PICK, ""));
    settle().await;
    assert_eq!(
        calls(&script, "app.launch-settings.profile.select"),
        [json!({ "gameId": "wario", "expectedRevision": "games-1" })]
    );
    assert!(launch.change(launch_id::CREATE, " Loud "));
    settle().await;
    assert_eq!(
        calls(&script, "app.launch-settings.profile.save")[0]["title"],
        json!("Loud")
    );
    assert!(launch.act(&launch_id::remove("p1")));
    settle().await;
    assert_eq!(
        calls(&script, "app.launch-settings.profile.remove")[0]["profileId"],
        json!("p1")
    );
}

// --- a streamed game ------------------------------------------------------------------

#[tokio::test]
async fn a_streamed_game_shows_the_same_list_read_from_and_written_to_the_device_that_runs_it() {
    let (launch, calls) = streamed(shared_and_own(), this_device(), up());
    open_on_aka(&launch);
    settle().await;
    assert_eq!(tag(&launch.editor()), "Ready");
    let now = shown(&launch);
    assert_eq!(now.runs_on.as_deref(), Some("Runs on aka"));
    // The same list, with this device's stream section in it.
    assert_eq!(
        titles(&now),
        [
            "Video",
            "mGBA only",
            "Stream",
            "Performance",
            "Runs with",
            "Profile"
        ]
    );
    // Read from aka; this device's own settings are read from this device.
    let reads = asked(&calls, SNAPSHOT);
    assert!(reads.contains(&json!({ "gameId": "wario", "holder": "aka" })));
    assert!(reads.contains(&json!({})));
    // A value that describes a device says which device keeps it.
    assert_eq!(
        source(&row(&now, "Render scale")),
        Some("Stays on aka".into())
    );
    assert_eq!(source(&row(&now, "CPU max")), Some("Stays on aka".into()));
    assert_eq!(
        source(&row(&now, "Codec")),
        Some("Stays on this device".into())
    );
    assert_eq!(value(&row(&now, "Codec")), Some("H264".into()));

    assert!(launch.change(RENDER_SCALE, "2"));
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    let writes = asked(&calls, SET);
    assert_eq!(writes.len(), 1);
    assert_eq!(
        writes[0]["holder"],
        json!("aka"),
        "written where the game runs"
    );
    assert_eq!(
        writes[0]["place"],
        json!({ "layer": { "_tag": "Game", "id": "wario" }, "scope": { "_tag": "Any" } })
    );
    assert_eq!(writes[0]["value"], json!(2));
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
}

#[tokio::test]
async fn a_streamed_games_changes_and_this_devices_own_copy_do_not_mix() {
    let (launch, _) = streamed(shared_and_own(), this_device(), up());
    open_on_aka(&launch);
    settle().await;
    assert!(launch.change(RENDER_SCALE, "2"));
    assert!(launch.change("launch:keep", "launch"));
    assert!(launch.act(launch_id::CANCEL));
    // The streamed launch carries them; a launch of this device's copy does not.
    assert!(launch.submit_one_launch("wario", None).1.is_none());
    let key = crate::surface::one_launch::game_key("wario", Some("aka"));
    assert!(launch.submit_one_launch(&key, None).1.is_some());
}

/// The place this device's stream viewer reads its values from.
fn viewer_place() -> Value {
    json!({ "layer": { "_tag": "Device" }, "scope": { "_tag": "Runner", "id": MOONLIGHT } })
}

#[tokio::test]
async fn this_devices_stream_rows_wait_as_changes_and_are_kept_on_this_device() {
    let (launch, calls) = streamed(shared_and_own(), this_device(), up());
    open_on_aka(&launch);
    settle().await;
    assert!(launch.change(CODEC, "\"av1\""));
    settle().await;
    // A stream row is a change like any other: marked, counted, not written.
    assert!(asked(&calls, SET).is_empty(), "a change is not a write");
    let now = shown(&launch);
    assert_eq!(tag(&launch.editor()), "Ready");
    assert_eq!(value(&row(&now, "Codec")), Some("AV1".into()));
    assert_eq!(changes(&now), [(CODEC.to_owned(), "H264".to_owned())]);
    assert_eq!(
        now.keep.as_ref().map(|keep| keep.label.as_str()),
        Some("Keep 1 change")
    );
    assert!(launch.change(RENDER_SCALE, "2"));
    let now = shown(&launch);
    assert_eq!(
        now.keep.as_ref().map(|keep| keep.label.as_str()),
        Some("Keep 2 changes")
    );
    assert_eq!(value(&now.keep), Some("This game".into()));
    // Back at its earlier value: the mark goes.
    assert!(launch.change(CODEC, "\"h264\""));
    assert_eq!(changes(&shown(&launch)).len(), 1);
    assert!(launch.change(CODEC, "\"av1\""));

    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    let writes = asked(&calls, SET);
    assert_eq!(writes.len(), 2);
    // aka's value goes to aka, in the place the person chose.
    assert_eq!(writes[0]["holder"], json!("aka"));
    assert_eq!(writes[0]["path"], setting("video.render-scale"));
    assert_eq!(
        writes[0]["place"],
        json!({ "layer": { "_tag": "Game", "id": "wario" }, "scope": { "_tag": "Any" } })
    );
    // The stream value goes to this device, where its viewer reads it.
    assert!(writes[1].get("holder").is_none(), "this device keeps it");
    assert_eq!(writes[1]["place"], viewer_place());
    assert_eq!(writes[1]["path"], setting("video.codec"));
    assert_eq!(writes[1]["value"], json!("av1"));
    assert_eq!(
        writes[1]["expectedRevision"],
        json!("device-1"),
        "from this device's own snapshot"
    );
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    let key = crate::surface::one_launch::game_key("wario", Some("aka"));
    assert!(launch.submit_one_launch(&key, None).1.is_none());
}

#[tokio::test]
async fn stream_values_stay_on_this_device_when_the_changes_are_for_this_launch() {
    let (launch, calls) = streamed(shared_and_own(), this_device(), up());
    open_on_aka(&launch);
    settle().await;
    assert!(launch.change(RENDER_SCALE, "2"));
    assert!(launch.change("launch:keep", "launch"));
    assert_eq!(
        shown(&launch).keep.and_then(|keep| keep.description),
        Some("They end when you stop playing.".into())
    );
    assert!(launch.change(CODEC, "\"av1\""));
    let keep = shown(&launch).keep;
    assert_eq!(
        keep.as_ref().map(|keep| keep.label.as_str()),
        Some("Keep 2 changes")
    );
    assert_eq!(value(&keep), Some("This launch".into()));
    assert_eq!(
        keep.and_then(|keep| keep.description),
        Some("They end when you stop playing. Stream values stay on this device.".into())
    );
    // Only stream values are changed: nothing ends with the launch.
    assert!(launch.change(RENDER_SCALE, "4"));
    assert!(launch.change("launch:keep", "launch"));
    assert_eq!(
        shown(&launch).keep.and_then(|keep| keep.description),
        Some("Stream values stay on this device.".into())
    );
    assert!(launch.change(RENDER_SCALE, "2"));
    // Any other place keeps its own words.
    assert!(launch.change("launch:keep", "game:wario"));
    assert_eq!(
        shown(&launch).keep.and_then(|keep| keep.description),
        Some("Used every time you play this game.".into())
    );
    assert!(launch.change("launch:keep", "launch"));

    // The game starts: the stream value is written to this device, and the
    // launch carries aka's value.
    assert_eq!(launch.close().await, Closing::Kept);
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    let writes = asked(&calls, SET);
    assert_eq!(writes.len(), 1);
    assert!(writes[0].get("holder").is_none());
    assert_eq!(writes[0]["place"], viewer_place());
    assert_eq!(writes[0]["value"], json!("av1"));
    let key = crate::surface::one_launch::game_key("wario", Some("aka"));
    let (ticket, values) = launch.submit_one_launch(&key, None);
    assert_eq!(
        serde_json::to_value(values.expect("one-launch values")).unwrap(),
        json!({ "settings": { "video": { "render-scale": 2 } }, "modifiers": {} })
    );
    launch.complete_one_launch(ticket, true);
    open_on_aka(&launch);
    settle().await;
    assert!(shown(&launch).changes.is_empty());
    assert_eq!(asked(&calls, SET).len(), 1);
}

#[tokio::test]
async fn a_stream_value_this_device_refuses_stops_the_start_and_stays_marked() {
    for (code, state) in [
        ("SettingsConflict", "Conflict"),
        ("SettingsInvalid", "Error"),
    ] {
        let refuse = up();
        let refusing = Arc::clone(&refuse);
        let (korrid, calls) = answering(move |tag, payload| match tag {
            SNAPSHOT if payload["holder"] == "aka" => ok_snapshot(shared_and_own().snapshot()),
            SNAPSHOT => ok_snapshot(this_device().snapshot()),
            _ if payload.get("holder").is_none()
                && refusing.load(std::sync::atomic::Ordering::SeqCst) =>
            {
                refused(code, "This device said no.")
            }
            _ => written_ok(),
        });
        let launch = controller(korrid);
        open_on_aka(&launch);
        settle().await;
        assert!(launch.change(CODEC, "\"av1\""));
        assert!(launch.change("launch:keep", "launch"));
        assert_eq!(launch.close().await, Closing::NotKept);
        let editor = launch.editor();
        assert_eq!(tag(&editor), state);
        assert_eq!(
            changes(&shown(&launch)),
            [(CODEC.to_owned(), "H264".to_owned())]
        );
        if let SurfaceLaunchSettings::Error(problem) = &editor {
            // This device refused, not aka.
            assert_eq!(problem.message, "This device said no.");
            assert_eq!(action_ids(&editor), [launch_id::RETRY, launch_id::CANCEL]);
            // Retry writes it again, then the editor closes.
            refuse.store(false, std::sync::atomic::Ordering::SeqCst);
            assert!(launch.act(launch_id::RETRY));
            settle().await;
            assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
            assert_eq!(asked(&calls, SET).len(), 2);
        } else {
            // Reload reads this device again too.
            let reads = asked(&calls, SNAPSHOT).len();
            assert!(launch.act(launch_id::RELOAD));
            settle().await;
            assert_eq!(asked(&calls, SNAPSHOT).len(), reads + 2);
            assert_eq!(tag(&launch.editor()), "Ready");
            assert_eq!(changes(&shown(&launch)).len(), 1);
        }
    }
}

#[tokio::test]
async fn the_stream_section_shows_only_for_a_streamed_game_whose_viewer_declares_settings() {
    // A phone today: its viewer declares no settings (issue #395).
    let mut phone = this_device();
    phone.runners.clear();
    phone.runner_resolved.clear();
    let (launch, _) = streamed(shared_and_own(), phone, up());
    open_on_aka(&launch);
    settle().await;
    assert!(!titles(&shown(&launch)).contains(&"Stream".to_owned()));

    // A game this device runs has no stream section, viewer or not.
    let (launch, _) = streamed(shared_and_own(), this_device(), up());
    launch.refresh_device().await;
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    let now = shown(&launch);
    assert_eq!(now.runs_on.as_deref(), Some("Runs here"));
    assert!(!titles(&now).contains(&"Stream".to_owned()));
    assert_eq!(
        source(&row(&now, "CPU max")),
        Some("Stays on this device".into())
    );
}

#[tokio::test]
async fn a_holder_that_cannot_be_reached_says_so_and_this_devices_rows_still_work() {
    let reachable = up();
    let (launch, calls) = streamed(resolved_wario(), this_device(), Arc::clone(&reachable));
    open_on_aka(&launch);
    settle().await;
    assert_eq!(tag(&launch.editor()), "Ready");
    assert!(launch.act(launch_id::CANCEL));

    reachable.store(false, std::sync::atomic::Ordering::SeqCst);
    open_on_aka(&launch);
    settle().await;
    let editor = launch.editor();
    let SurfaceLaunchSettings::Error(problem) = &editor else {
        panic!("{}", tag(&editor))
    };
    assert_eq!(
        problem.message,
        "aka cannot be reached. Its settings cannot change now."
    );
    let offered: Vec<String> = problem
        .content
        .actions
        .iter()
        .map(|action| action.label.clone())
        .collect();
    assert_eq!(offered, ["Retry", "Close"]);
    // aka's rows show what it last said. They cannot change, and say why.
    let holder_row = row(&problem.content, "Frameskip").expect("the holder's row");
    assert_eq!(holder_row.value.as_deref(), Some("3"));
    assert!(holder_row.interaction.is_none());
    assert_eq!(
        holder_row.description.as_deref(),
        Some("aka cannot be reached. This cannot change now.")
    );
    assert!(launch.change(FRAMESKIP, "5"));
    assert!(shown(&launch).changes.is_empty());
    // This device's own rows still work: a change waits, marked.
    assert!(row(&problem.content, "Codec").is_some_and(|item| item.interaction.is_some()));
    assert!(launch.change(CODEC, "\"av1\""));
    settle().await;
    assert!(asked(&calls, SET).is_empty());
    assert_eq!(tag(&launch.editor()), "Error");
    assert_eq!(
        changes(&shown(&launch)),
        [(CODEC.to_owned(), "H264".to_owned())]
    );

    // aka answers again: its rows change again, and the stream change still waits.
    reachable.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(launch.act(launch_id::RETRY));
    settle().await;
    assert_eq!(tag(&launch.editor()), "Ready");
    assert!(row(&shown(&launch), "Frameskip").is_some_and(|item| item.interaction.is_some()));
    assert_eq!(changes(&shown(&launch)).len(), 1);

    // Closing keeps it on this device.
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    let writes = asked(&calls, SET);
    assert_eq!(writes.len(), 1);
    assert!(writes[0].get("holder").is_none(), "written to this device");
}

/// The values of the last write this device took for its stream viewer.
fn kept_here(calls: &Calls) -> Vec<Value> {
    asked(calls, SET)
        .into_iter()
        .filter(|write| write.get("holder").is_none())
        .map(|write| {
            assert_eq!(write["place"], viewer_place());
            write["value"].clone()
        })
        .collect()
}

/// aka is lost while the editor closes. This device's stream values are its
/// own: they are still written, and their rows still work in that Error.
#[tokio::test]
async fn a_holder_lost_while_the_editor_closes_does_not_stop_this_devices_stream_values() {
    let reachable = up();
    let (launch, calls) = streamed(resolved_wario(), this_device(), Arc::clone(&reachable));
    open_on_aka(&launch);
    settle().await;
    assert!(launch.change(FRAMESKIP, "5"));
    assert!(launch.change(CODEC, "\"av1\""));
    reachable.store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    let editor = launch.editor();
    let SurfaceLaunchSettings::Error(problem) = &editor else {
        panic!("{}", tag(&editor))
    };
    assert_eq!(
        problem.message,
        "aka cannot be reached. Its settings cannot change now."
    );
    // aka's write failed. The stream value was written all the same.
    assert_eq!(kept_here(&calls), [json!("av1")]);
    assert_eq!(
        changes(&problem.content),
        [(FRAMESKIP.to_owned(), "3".to_owned())]
    );

    // The stream rows still take changes in that Error.
    assert!(row(&problem.content, "Codec").is_some_and(|item| item.interaction.is_some()));
    assert!(launch.change(CODEC, "\"h265\""));
    assert_eq!(tag(&launch.editor()), "Error");
    assert_eq!(
        changes(&shown(&launch)),
        [
            (FRAMESKIP.to_owned(), "3".to_owned()),
            (CODEC.to_owned(), "H264".to_owned()),
        ]
    );
    // A game start writes the stream value, and still does not start: aka's
    // change is not written.
    assert_eq!(launch.close().await, Closing::NotKept);
    assert_eq!(kept_here(&calls), [json!("av1"), json!("h265")]);
    assert_eq!(tag(&launch.editor()), "Error");
    assert_eq!(
        changes(&shown(&launch)),
        [(FRAMESKIP.to_owned(), "3".to_owned())]
    );
    // A second Close writes a stream value that waits, then leaves.
    assert!(launch.change(CODEC, "\"av1\""));
    let to_aka = asked(&calls, SET).len() - kept_here(&calls).len();
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    assert_eq!(
        kept_here(&calls),
        [json!("av1"), json!("h265"), json!("av1")]
    );
    assert_eq!(
        asked(&calls, SET).len() - kept_here(&calls).len(),
        to_aka,
        "aka is not asked again without Retry"
    );
    // aka's change still waits with the game.
    reachable.store(true, std::sync::atomic::Ordering::SeqCst);
    open_on_aka(&launch);
    settle().await;
    assert_eq!(
        changes(&shown(&launch)),
        [(FRAMESKIP.to_owned(), "3".to_owned())]
    );
}

/// aka refuses a stale write. That is aka's Conflict: this device's stream
/// values are written, and their rows still work.
#[tokio::test]
async fn a_holders_conflict_does_not_stop_this_devices_stream_values() {
    let (korrid, calls) = answering(move |tag, payload| match tag {
        SNAPSHOT if payload["holder"] == "aka" => ok_snapshot(resolved_wario().snapshot()),
        SNAPSHOT => ok_snapshot(this_device().snapshot()),
        _ if payload["holder"] == "aka" => refused("SettingsConflict", "changed"),
        _ => written_ok(),
    });
    let launch = controller(korrid);
    open_on_aka(&launch);
    settle().await;
    assert!(launch.change(FRAMESKIP, "5"));
    assert!(launch.change(CODEC, "\"av1\""));
    assert_eq!(launch.close().await, Closing::NotKept);
    assert_eq!(tag(&launch.editor()), "Conflict");
    assert_eq!(kept_here(&calls), [json!("av1")]);
    assert_eq!(
        changes(&shown(&launch)),
        [(FRAMESKIP.to_owned(), "3".to_owned())]
    );
    assert!(launch.change(CODEC, "\"h265\""));
    assert_eq!(tag(&launch.editor()), "Conflict");
    assert_eq!(changes(&shown(&launch)).len(), 2);
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    assert_eq!(kept_here(&calls), [json!("av1"), json!("h265")]);
    assert_eq!(asked(&calls, SET).len(), 3, "aka was asked once");
}

/// This device refuses its own stream value. The next Close leaves without
/// asking again; aka's change is written all the same.
#[tokio::test]
async fn a_stream_value_this_device_refuses_does_not_stop_the_holders_changes() {
    let (korrid, calls) = answering(move |tag, payload| match tag {
        SNAPSHOT if payload["holder"] == "aka" => ok_snapshot(resolved_wario().snapshot()),
        SNAPSHOT => ok_snapshot(this_device().snapshot()),
        _ if payload.get("holder").is_none() => refused("SettingsInvalid", "This device said no."),
        _ => written_ok(),
    });
    let launch = controller(korrid);
    open_on_aka(&launch);
    settle().await;
    assert!(launch.change(CODEC, "\"av1\""));
    assert!(launch.change(FRAMESKIP, "5"));
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    let editor = launch.editor();
    let SurfaceLaunchSettings::Error(problem) = &editor else {
        panic!("{}", tag(&editor))
    };
    assert_eq!(problem.message, "This device said no.");
    assert_eq!(
        changes(&problem.content),
        [(CODEC.to_owned(), "H264".to_owned())]
    );
    let writes = asked(&calls, SET);
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[1]["holder"], json!("aka"));
    assert_eq!(writes[1]["value"], json!(5));
    // Close leaves. It does not ask this device again; Retry would.
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    assert_eq!(asked(&calls, SET).len(), 2);
}

/// This device cannot say what its stream viewer has now, so the list has
/// no stream row to show the change on.
#[tokio::test]
async fn a_stream_change_the_list_cannot_show_now_waits_and_is_not_in_the_way() {
    let device_up = up();
    let answers = Arc::clone(&device_up);
    let (korrid, calls) = answering(move |tag, payload| match tag {
        SNAPSHOT if payload["holder"] == "aka" => ok_snapshot(shared_and_own().snapshot()),
        SNAPSHOT if answers.load(std::sync::atomic::Ordering::SeqCst) => {
            ok_snapshot(this_device().snapshot())
        }
        SNAPSHOT => refused("SettingsUnavailable", "Launch settings could not be read."),
        _ => written_ok(),
    });
    let launch = controller(korrid);
    open_on_aka(&launch);
    settle().await;
    assert!(launch.change(CODEC, "\"av1\""));
    assert_eq!(changes(&shown(&launch)).len(), 1);
    device_up.store(false, std::sync::atomic::Ordering::SeqCst);
    launch.refresh_device().await;
    let now = shown(&launch);
    assert!(!titles(&now).contains(&"Stream".to_owned()));
    assert!(now.changes.is_empty());
    assert!(now.keep.is_none());
    // Nothing shown waits, so the game may start. Nothing is written.
    assert_eq!(launch.close().await, Closing::Kept);
    assert!(asked(&calls, SET).is_empty());
    // The change shows again once this device answers.
    device_up.store(true, std::sync::atomic::Ordering::SeqCst);
    open_on_aka(&launch);
    settle().await;
    assert_eq!(
        changes(&shown(&launch)),
        [(CODEC.to_owned(), "H264".to_owned())]
    );
}

/// aka cannot be reached and one of its changes waits. Closing still keeps
/// this device's own stream value; aka's change stays with the game.
#[tokio::test]
async fn closing_with_the_holder_out_of_reach_keeps_this_devices_stream_values() {
    let reachable = up();
    let (launch, calls) = streamed(resolved_wario(), this_device(), Arc::clone(&reachable));
    open_on_aka(&launch);
    settle().await;
    assert!(launch.change(FRAMESKIP, "5"));
    reachable.store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    assert_eq!(tag(&launch.editor()), "Error");
    assert!(launch.act(launch_id::CANCEL));
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    let before = asked(&calls, SET).len();

    open_on_aka(&launch);
    settle().await;
    assert_eq!(tag(&launch.editor()), "Error");
    assert!(launch.change(CODEC, "\"av1\""));
    assert_eq!(
        changes(&shown(&launch)),
        [
            (FRAMESKIP.to_owned(), "3".to_owned()),
            (CODEC.to_owned(), "H264".to_owned()),
        ]
    );
    // The Close action writes what this device can keep, and leaves.
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    let writes = asked(&calls, SET);
    assert_eq!(writes.len(), before + 1);
    let last = writes.last().unwrap();
    assert!(last.get("holder").is_none(), "written to this device");
    assert_eq!(last["place"], viewer_place());
    assert_eq!(last["value"], json!("av1"));

    // A game start writes it too, and does not start: aka's change waits.
    open_on_aka(&launch);
    settle().await;
    assert!(launch.change(CODEC, "\"h265\""));
    assert_eq!(launch.close().await, Closing::NotKept);
    assert_eq!(asked(&calls, SET).len(), before + 2);
    assert_eq!(tag(&launch.editor()), "Error");
    assert_eq!(
        changes(&shown(&launch)),
        [(FRAMESKIP.to_owned(), "3".to_owned())]
    );
    assert!(launch.act(launch_id::CANCEL));
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);

    reachable.store(true, std::sync::atomic::Ordering::SeqCst);
    open_on_aka(&launch);
    settle().await;
    assert_eq!(
        changes(&shown(&launch)),
        [(FRAMESKIP.to_owned(), "3".to_owned())]
    );
}

#[tokio::test]
async fn a_holder_never_reached_shows_only_this_devices_rows() {
    let reachable = up();
    reachable.store(false, std::sync::atomic::Ordering::SeqCst);
    let (launch, _) = streamed(resolved_wario(), this_device(), reachable);
    open_on_aka(&launch);
    settle().await;
    assert_eq!(tag(&launch.editor()), "Error");
    assert_eq!(titles(&shown(&launch)), ["Stream"]);
}

#[tokio::test]
async fn a_holder_that_goes_away_while_the_editor_closes_keeps_the_changes() {
    let reachable = up();
    let (launch, calls) = streamed(resolved_wario(), this_device(), Arc::clone(&reachable));
    open_on_aka(&launch);
    settle().await;
    assert!(launch.change(FRAMESKIP, "5"));
    reachable.store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    let editor = launch.editor();
    let SurfaceLaunchSettings::Error(problem) = &editor else {
        panic!("{}", tag(&editor))
    };
    assert_eq!(
        problem.message,
        "aka cannot be reached. Its settings cannot change now."
    );
    assert_eq!(changes(&problem.content).len(), 1);
    // Retry writes what is left, then closes.
    reachable.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(launch.act(launch_id::RETRY));
    settle().await;
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
    assert_eq!(asked(&calls, SET).len(), 2);
}

#[tokio::test]
async fn a_holder_that_is_no_peer_now_reads_as_unreachable_and_a_refusal_keeps_its_own_words() {
    for (code, message, expected) in [
        (
            "UpstreamFailure",
            "upstream call failed: upstream host \"aka\" is not configured",
            "aka cannot be reached. Its settings cannot change now.",
        ),
        (
            "PermissionDenied",
            "This device cannot change the settings of aka. Only a device of its owner can.",
            "This device cannot change the settings of aka. Only a device of its owner can.",
        ),
    ] {
        let (korrid, _) = answering(move |_, payload| {
            if payload["holder"] == "aka" {
                refused(code, message)
            } else {
                ok_snapshot(this_device().snapshot())
            }
        });
        let launch = controller(korrid);
        open_on_aka(&launch);
        settle().await;
        let editor = launch.editor();
        let SurfaceLaunchSettings::Error(problem) = &editor else {
            panic!("{}", tag(&editor))
        };
        assert_eq!(problem.message, expected);
        assert_eq!(action_ids(&editor), [launch_id::RETRY, launch_id::CANCEL]);
    }
}

#[tokio::test]
async fn a_change_the_device_already_has_after_a_reload_is_no_change() {
    let fixture = resolved_wario();
    let (korrid, script) = scripted();
    push(&script, SNAPSHOT, ok_snapshot(fixture.snapshot()));
    let launch = controller(korrid);
    launch.open("wario".into(), "Wario Land 4".into(), None, None);
    settle().await;
    assert!(launch.change(FRAMESKIP, "5"));
    // Another device set the same value meanwhile.
    let mut moved = wario().facts(rg353m()).value(sourced(
        editable(setting("video.frameskip")),
        json!(5),
        at("game:wario/any"),
    ));
    moved.revisions = ("device-1".into(), "games-9".into());
    push(&script, SNAPSHOT, ok_snapshot(moved.snapshot()));
    assert!(launch.act(launch_id::RELOAD));
    settle().await;
    let now = shown(&launch);
    assert_eq!(value(&row(&now, "Frameskip")), Some("5".into()));
    assert!(now.changes.is_empty());
    assert!(now.keep.is_none());
    // Closing writes nothing: there is nothing to keep.
    assert!(launch.act(launch_id::CANCEL));
    settle().await;
    assert!(calls(&script, SET).is_empty());
}

#[tokio::test]
async fn a_percent_setting_reads_its_unit_on_the_value_not_in_its_name() {
    let mut fixture = resolved_wario().value(sourced(
        editable(setting("audio.volume-percent")),
        json!(80),
        at("device/any"),
    ));
    fixture.runners[0].schema = korri_settings_treaty::schema(
        &[
            korri_settings_treaty::SharedSetting::Volume,
            korri_settings_treaty::SharedSetting::AspectRatio,
        ],
        json!({ "type": "object", "properties": {} }),
    );
    let (launch, _) = opened(&fixture, None).await;
    let now = shown(&launch);
    assert_eq!(value(&row(&now, "Volume")), Some("80%".into()));
    // A named choice reads as words.
    assert_eq!(
        choices(&row(&now, "Aspect ratio")),
        [
            choice("default", "Default"),
            choice("\"original\"", "Original"),
            choice("\"fill-view\"", "Fill view"),
            choice("\"stretch\"", "Stretch"),
            choice("\"4:3\"", "4:3"),
            choice("\"16:9\"", "16:9"),
        ]
    );
}

/// The surfaces say where a value stays only from `source`. Every row whose
/// value stays with one device carries it, changeable or not.
#[tokio::test]
async fn every_row_that_stays_on_a_device_says_which_device_in_its_source() {
    fn check(shown: &SurfaceLaunchSettingsContent, holder: &str) {
        let mut seen = 0;
        for item in shown.groups.iter().flat_map(|group| &group.items) {
            if item.scope != Some(SurfaceSettingScope::Device) {
                continue;
            }
            seen += 1;
            let stream = item.id.starts_with("launch:stream:");
            assert_eq!(
                item.source.as_deref(),
                Some(if stream {
                    "Stays on this device"
                } else {
                    holder
                }),
                "{}",
                item.label
            );
        }
        assert!(seen > 0);
    }
    // A game this device runs, on a device without GPU limits.
    let mut local = shared_and_own();
    local.facts = Some(json!({
        "defaults": { "cpu": { "min": 0, "max": 100 }, "gpu": { "min": 0, "max": 100 }, "governor": "schedutil" },
        "cpuPolicies": [], "gpuDevfreq": [],
    }));
    let (launch, _) = opened(&local, None).await;
    let now = shown(&launch);
    assert!(row(&now, "GPU limits").is_some_and(|item| item.interaction.is_none()));
    check(&now, "Stays on this device");
    // A streamed game, reachable and then not.
    let reachable = up();
    let (launch, _) = streamed(shared_and_own(), this_device(), Arc::clone(&reachable));
    open_on_aka(&launch);
    settle().await;
    check(&shown(&launch), "Stays on aka");
    assert!(launch.act(launch_id::CANCEL));
    reachable.store(false, std::sync::atomic::Ordering::SeqCst);
    open_on_aka(&launch);
    settle().await;
    assert_eq!(tag(&launch.editor()), "Error");
    let gone = shown(&launch);
    assert!(!gone.groups.is_empty(), "the holder's rows still show");
    check(&gone, "Stays on aka");
}
