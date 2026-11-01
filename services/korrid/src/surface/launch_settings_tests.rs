//! Port of `launch-settings.test.ts`. The portal ran these against its
//! in-memory korrid, which folded values itself; here each snapshot is
//! written out as korrid sends it, resolved values included, so the tests
//! cover the model's reading of korrid's snapshot and the controller's
//! reads and writes.

use std::{collections::HashMap, sync::Arc, time::Duration};

use korri_surface_treaty::{
    SurfaceLaunchSettings, SurfaceLaunchSettingsContent, SurfaceSettingChoice, SurfaceSettingGroup,
    SurfaceSettingInteraction, SurfaceSettingItem, SurfaceSettingScope, SurfaceSettingsStatus,
};
use serde_json::{json, Value};

use super::acquisition_gate::{push, scripted};
use crate::surface::{
    launch_model::{
        cpu_steps, device_launch_from, governors, gpu_steps, launch_editor_from, launch_id,
        layer_key, Encoder, LaunchEncoded, PendingSetting,
    },
    launch_settings::LaunchSettings,
};

const MGBA: &str = "game:wario/runner:retroarch/mgba";
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
                layers.push(("system:gba".into(), "System gba".into()));
            }
            layers.push(("game:wario".into(), "Game".into()));
            if self.system_and_release {
                layers.push(("release:usa".into(), "Release usa".into()));
            }
            for (id, title) in &self.profiles {
                layers.push((format!("profile:{id}"), format!("Profile {title}")));
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

struct Editor {
    places: Vec<korri_surface_treaty::SurfaceLaunchSettingsPlace>,
    notes: Vec<String>,
}

fn edit(fixture: &Fixture) -> Editor {
    edit_with(fixture, &HashMap::new())
}
fn edit_with(fixture: &Fixture, pending: &HashMap<String, Vec<PendingSetting>>) -> Editor {
    let editor = launch_editor_from(&fixture.snapshot(), Some("wario"), pending);
    Editor {
        places: editor.places,
        notes: editor.notes,
    }
}
fn adding(place_id: &str, settings: Vec<PendingSetting>) -> HashMap<String, Vec<PendingSetting>> {
    HashMap::from([(place_id.to_owned(), settings)])
}
fn runner_key(key: &str) -> PendingSetting {
    PendingSetting::Runner { key: key.into() }
}

impl Editor {
    fn groups(&self, place_id: &str) -> Vec<SurfaceSettingGroup> {
        self.places
            .iter()
            .find(|place| place.id == place_id)
            .map(|place| place.groups.clone())
            .unwrap_or_default()
    }
    fn rows(&self, place_id: &str) -> Vec<SurfaceSettingItem> {
        self.groups(place_id)
            .into_iter()
            .flat_map(|group| group.items)
            .collect()
    }
    fn row(&self, place_id: &str, label: &str) -> Option<SurfaceSettingItem> {
        self.rows(place_id)
            .into_iter()
            .find(|item| item.label == label)
    }
    fn group(&self, place_id: &str, title: &str) -> Option<SurfaceSettingGroup> {
        self.groups(place_id)
            .into_iter()
            .find(|group| group.title == title)
    }
    fn titles(&self, place_id: &str) -> Vec<String> {
        self.groups(place_id)
            .into_iter()
            .map(|group| group.title)
            .collect()
    }
}

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
fn live(editor: &SurfaceLaunchSettings) -> Editor {
    let content = content(editor);
    Editor {
        places: content.places,
        notes: content.notes,
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
    launch.open("wario".into(), "Wario Land 4".into(), choose);
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

// --- the game editor --------------------------------------------------------------

#[tokio::test]
async fn editor_opens_loading_then_ready_with_a_place_per_layer_and_runner_scope_most_general_first(
) {
    let (korrid, script) = scripted();
    // korrid's order is not the editor's: the editor sorts.
    let mut snapshot = wario().snapshot();
    snapshot["places"].as_array_mut().unwrap().reverse();
    push(&script, SNAPSHOT, ok_snapshot(snapshot));
    let launch = controller(korrid);
    launch.open("wario".into(), "Wario Land 4".into(), None);
    assert_eq!(tag(&launch.editor()), "Loading");
    settle().await;
    let editor = launch.editor();
    assert_eq!(tag(&editor), "Ready");
    let places = content(&editor).places;
    let shown: Vec<String> = places
        .iter()
        .map(|place| {
            format!(
                "{} | {} {}",
                place.layer.title,
                serde_json::to_value(place.scope.kind)
                    .unwrap()
                    .as_str()
                    .unwrap(),
                place.scope.label
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            "Device | any Any runner",
            "Device | family Family retroarch",
            "Device | runner mGBA",
            "Device | runner gpSP",
            "System gba | any Any runner",
            "System gba | family Family retroarch",
            "System gba | runner mGBA",
            "System gba | runner gpSP",
            "Game | any Any runner",
            "Game | family Family retroarch",
            "Game | runner mGBA",
            "Game | runner gpSP",
            "Release usa | any Any runner",
            "Release usa | family Family retroarch",
            "Release usa | runner mGBA",
            "Release usa | runner gpSP",
        ]
    );
    let ids: std::collections::HashSet<&String> = places.iter().map(|place| &place.id).collect();
    assert_eq!(ids.len(), places.len());
    assert!(content(&editor).notes.is_empty());
    assert_eq!(content(&editor).game_title.as_deref(), Some("Wario Land 4"));
}

#[test]
fn editor_says_where_each_value_in_force_comes_from_and_when_a_runner_place_is_not_in_force() {
    let fixture = wario()
        .facts(rg353m())
        .body(
            "device/runner:retroarch/mgba",
            json!({ "cpu": { "max": 61 } }),
        )
        .value(sourced(
            editable(json!({ "_tag": "CpuMax" })),
            json!(61),
            at("device/runner:retroarch/mgba"),
        ));
    let editor = edit(&fixture);
    let own = editor.row("device/runner:retroarch/mgba", "CPU max");
    assert_eq!(value(&own).as_deref(), Some("61 % (1.10 GHz)"));
    assert_eq!(source(&own).as_deref(), Some("from Device for mGBA"));
    let game = editor.row("game:wario/any", "CPU max");
    assert_eq!(value(&game).as_deref(), Some("Inherit"));
    assert_eq!(
        source(&game).as_deref(),
        Some("61 % (1.10 GHz) from Device for mGBA")
    );
    assert_eq!(
        game.unwrap().description.as_deref(),
        Some("Applies at next launch. Limits the game's session.")
    );
    assert_eq!(
        source(&editor.row("game:wario/runner:retroarch/gpsp", "CPU max")).as_deref(),
        Some("Not in force: mGBA runs this game")
    );
    assert!(editor.row("game:wario/any", "CPU governor").is_none());
    assert_eq!(
        editor.row("device/any", "CPU governor").unwrap().scope,
        Some(SurfaceSettingScope::Device)
    );
}

#[tokio::test]
async fn editor_sets_and_clears_a_value_on_one_place() {
    let (launch, script) = opened(&wario(), None).await;
    let id = launch_id::set("game:wario/any", "gpu.min");
    push(&script, SNAPSHOT, ok_snapshot(wario().snapshot()));
    assert!(launch.change(&id, "30"));
    assert_eq!(
        launch.status(),
        Some(SurfaceSettingsStatus::Saving {
            setting_id: id.clone()
        })
    );
    settle().await;
    assert_eq!(launch.status(), None);
    assert!(launch.change(&id, "inherit"));
    settle().await;
    let sent = calls(&script, SET);
    assert_eq!(sent[0]["place"], place("game:wario/any"));
    assert_eq!(sent[0]["path"], json!({ "_tag": "GpuMin" }));
    assert_eq!(sent[0]["value"], json!(30));
    assert_eq!(sent[0]["expectedRevision"], json!("games-1"));
    // Inherit sends no value and no clear: the place's value goes.
    assert_eq!(sent[1].get("value"), None);
    assert_eq!(sent[1]["clear"], json!(false));
}

#[test]
fn editor_chooses_the_runner_with_a_typed_set_shared_across_devices() {
    let editor = edit(&wario());
    let runner = editor.row("game:wario/any", "Runner");
    assert_eq!(value(&runner).as_deref(), Some("mGBA"));
    assert_eq!(
        runner.as_ref().unwrap().scope,
        Some(SurfaceSettingScope::Shared)
    );
    assert_eq!(source(&runner).as_deref(), Some("from Game"));
    assert_eq!(
        runner.unwrap().interaction,
        Some(SurfaceSettingInteraction::Choice {
            selected: "retroarch/mgba".into(),
            choices: vec![
                choice("inherit", "Inherit"),
                choice("retroarch/mgba", "mGBA"),
                choice("retroarch/gpsp", "gpSP")
            ],
        })
    );
    assert!(editor.row(MGBA, "Runner").is_none());
    assert_eq!(
        source(&editor.row("device/any", "Runner")).as_deref(),
        Some("mGBA from Game")
    );
    // gpSP chosen: mGBA's places are no longer in force.
    let mut chosen = wario().body("game:wario/any", json!({ "runner": "retroarch/gpsp" }));
    chosen.selected = Some("retroarch/gpsp");
    chosen.resolved = vec![sourced(
        editable(json!({ "_tag": "Runner" })),
        json!("retroarch/gpsp"),
        at("game:wario/any"),
    )];
    let editor = edit(&chosen);
    assert_eq!(
        source(&editor.row("device/any", "Runner")).as_deref(),
        Some("gpSP from Game")
    );
    assert_eq!(
        source(&editor.row(MGBA, "CPU min")).as_deref(),
        Some("Not in force: gpSP runs this game")
    );
}

#[test]
fn editor_groups_a_runners_settings_by_section_each_with_its_own_add_setting_row() {
    let editor = edit(&wario());
    assert_eq!(
        editor.titles(MGBA)[..3],
        ["mGBA · Video", "mGBA · Input", "mGBA · Language"]
    );
    let mut add = SurfaceSettingItem::new(launch_id::set(MGBA, "add-setting:video"), "Add setting");
    add.value = Some("Choose a setting".into());
    add.description = Some("2 more settings mGBA offers here.".into());
    add.interaction = Some(SurfaceSettingInteraction::Choice {
        selected: String::new(),
        choices: vec![
            choice("video.frameskip", "Frameskip"),
            choice("video.color-correction", "Colour correction"),
        ],
    });
    assert_eq!(editor.group(MGBA, "mGBA · Video").unwrap().items, vec![add]);
    // A deeper leaf reads from its titles; its value is its dotted path.
    assert_eq!(
        choices(
            &editor
                .group(MGBA, "mGBA · Input")
                .unwrap()
                .items
                .first()
                .cloned()
        ),
        vec![choice("input.solar.level", "Solar sensor level")]
    );
    // The place for any runner holds no runner settings.
    assert!(!editor
        .titles("game:wario/any")
        .iter()
        .any(|title| title.starts_with("mGBA")));
}

#[test]
fn editor_shows_an_added_runner_setting_pending_until_it_has_a_value() {
    let fixture = wario().value(sourced(
        editable(setting("video.frameskip")),
        json!(0),
        plugin_default("retroarch/mgba"),
    ));
    let editor = edit_with(&fixture, &adding(MGBA, vec![runner_key("video.frameskip")]));
    let pending = editor.row(MGBA, "Frameskip").unwrap();
    assert_eq!(pending.value.as_deref(), Some("Not set here"));
    assert_eq!(pending.source.as_deref(), Some("0 from mGBA (default)"));
    assert_eq!(pending.scope, Some(SurfaceSettingScope::Shared));
    assert_eq!(
        pending.description.as_deref(),
        Some("Frames skipped between drawn ones.")
    );
    assert_eq!(
        pending.interaction,
        Some(SurfaceSettingInteraction::Number {
            value: None,
            min: Some(0.0),
            max: Some(10.0),
            step: Some(1.0),
            clearable: Some(true)
        })
    );
    assert_eq!(
        labels(&editor.group(MGBA, "mGBA · Video").unwrap().items),
        ["Frameskip", "Default Frameskip", "Add setting"]
    );

    // Written, it is the place's own value.
    let written = fixture
        .clone()
        .body(MGBA, json!({ "settings": { "video": { "frameskip": 2 } } }))
        .value(sourced(
            editable(setting("video.frameskip")),
            json!(2),
            at(MGBA),
        ));
    let mut written = written;
    written.resolved.retain(|value| value["value"] != json!(0));
    let row = edit(&written).row(MGBA, "Frameskip");
    assert_eq!(value(&row).as_deref(), Some("2"));
    assert_eq!(source(&row).as_deref(), Some("from Game for mGBA"));

    // A deeper leaf and a device-only one.
    let editor = edit_with(
        &wario(),
        &adding(
            MGBA,
            vec![
                runner_key("input.solar.level"),
                runner_key("locale.language"),
            ],
        ),
    );
    assert_eq!(
        editor.row(MGBA, "Language").unwrap().scope,
        Some(SurfaceSettingScope::Device)
    );
    assert_eq!(
        choices(&editor.row(MGBA, "Solar sensor level"))[2..],
        [
            choice("\"off\"", "off"),
            choice("\"low\"", "low"),
            choice("\"high\"", "high")
        ]
    );
}

#[tokio::test]
async fn editor_adds_a_runner_setting_refuses_a_value_out_of_range_and_writes_it_by_path() {
    let (launch, script) = opened(&wario(), None).await;
    let written = wario()
        .body(MGBA, json!({ "settings": { "video": { "frameskip": 2 } } }))
        .value(sourced(
            editable(setting("video.frameskip")),
            json!(2),
            at(MGBA),
        ));
    push(&script, SNAPSHOT, ok_snapshot(written.snapshot()));
    assert!(launch.change(
        &launch_id::set(MGBA, "add-setting:video"),
        "video.frameskip"
    ));
    assert_eq!(launch.status(), None);
    assert!(live(&launch.editor()).row(MGBA, "Frameskip").is_some());
    let frameskip = launch_id::set(MGBA, "setting:video.frameskip");
    launch.change(&frameskip, "12");
    assert_eq!(
        launch.status(),
        Some(SurfaceSettingsStatus::Problem {
            setting_id: frameskip.clone(),
            message: "That value isn't allowed here.".into()
        })
    );
    assert!(calls(&script, SET).is_empty());
    assert!(launch.dismiss_problem());
    launch.change(&frameskip, "2");
    settle().await;
    let sent = calls(&script, SET);
    assert_eq!(sent[0]["path"], setting("video.frameskip"));
    assert_eq!(sent[0]["value"], json!(2));
    assert_eq!(sent[0]["clear"], json!(false));
    let row = live(&launch.editor()).row(MGBA, "Frameskip");
    assert_eq!(
        (value(&row).as_deref(), source(&row).as_deref()),
        (Some("2"), Some("from Game for mGBA"))
    );

    // Inherit on a setting never written takes it away again, unwritten.
    launch.change(
        &launch_id::set(MGBA, "add-setting:video"),
        "video.color-correction",
    );
    launch.change(
        &launch_id::set(MGBA, "setting:video.color-correction"),
        "inherit",
    );
    assert_eq!(launch.status(), None);
    assert!(live(&launch.editor())
        .row(MGBA, "Colour correction")
        .is_none());
    assert_eq!(calls(&script, SET).len(), 1);

    // Clearing a written number takes it away.
    launch.change(&frameskip, "");
    settle().await;
    let sent = calls(&script, SET);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1].get("value"), None);
    assert_eq!(sent[1]["clear"], json!(false));
}

#[test]
fn editor_offers_a_family_place_every_installed_runners_settings_in_that_family_by_section() {
    let editor = edit(&wario());
    let family = "game:wario/family:retroarch";
    assert_eq!(
        editor.titles(family)[..4],
        [
            "Family retroarch · Video",
            "Family retroarch · Input",
            "Family retroarch · Language",
            "Family retroarch · BIOS"
        ]
    );
    assert_eq!(
        choices(
            &editor
                .group(family, "Family retroarch · BIOS")
                .unwrap()
                .items
                .first()
                .cloned()
        ),
        vec![choice("bios.enable", "Use BIOS")]
    );
    assert!(editor.row("game:wario/any", "Add setting").is_none());
}

fn contributed(kind: &str, parts: Vec<(Value, Value, Option<&str>)>) -> Value {
    json!({
        "path": { "_tag": kind },
        "value": [],
        "contributions": parts.into_iter().map(|(source, value, routine)| {
            let mut part = json!({ "source": source, "value": value });
            if let Some(routine) = routine {
                part["routine"] = json!(routine);
            }
            part
        }).collect::<Vec<_>>(),
        "adjusted": false,
    })
}

#[test]
fn editor_shows_file_only_values_read_only_with_source_and_device_only_scope() {
    let fixture = wario()
        .body(
            "game:wario/any",
            json!({
                "runner": "retroarch/mgba",
                "env": { "SDL_AUDIODRIVER": "pipewire", "MESA_DEBUG": null },
                "args": { "prepend": ["gamemoderun"], "append": ["--verbose"] },
                "cwd": "/srv/wario",
                "hooks": { "before": [{ "run": "fan quiet" }], "after": [{ "run": "fan auto" }] },
            }),
        )
        .value(contributed(
            "ArgsPrepend",
            vec![(at("game:wario/any"), json!(["gamemoderun"]), None)],
        ))
        .value(contributed(
            "ArgsAppend",
            vec![(at("game:wario/any"), json!(["--verbose"]), None)],
        ))
        .value(contributed(
            "HooksBefore",
            vec![(at("game:wario/any"), json!([{ "run": "fan quiet" }]), None)],
        ))
        .value(contributed(
            "HooksAfter",
            vec![(at("game:wario/any"), json!([{ "run": "fan auto" }]), None)],
        ));
    let editor = edit(&fixture);
    let files = editor.group("game:wario/any", "From files").unwrap().items;
    let shown: Vec<(String, String)> = files
        .iter()
        .map(|item| (item.label.clone(), item.value.clone().unwrap()))
        .collect();
    let expected = [
        ("Environment SDL_AUDIODRIVER", "pipewire"),
        ("Environment MESA_DEBUG", "Unset"),
        ("Working folder", "/srv/wario"),
        ("Arguments before", "gamemoderun"),
        ("Arguments after", "--verbose"),
        ("Before launch", "fan quiet"),
        ("After the session", "fan auto"),
    ];
    assert_eq!(
        shown,
        expected.map(|(label, value)| (label.to_owned(), value.to_owned()))
    );
    for item in &files {
        assert_eq!(item.interaction, None);
        assert_eq!(item.scope, Some(SurfaceSettingScope::Device));
        assert_eq!(
            item.description.as_deref(),
            Some("Set in library.yaml. Change it there.")
        );
    }
    let sources: Vec<String> = files
        .iter()
        .map(|item| item.source.clone().unwrap())
        .collect();
    assert_eq!(
        sources,
        [
            "from Game",
            "from Game",
            "from Game",
            "In force: from Game",
            "In force: from Game",
            "In force: from Game",
            "In force: from Game"
        ]
    );
    let env = editor.row(MGBA, "Environment MGBA_AUDIO");
    assert_eq!(value(&env).as_deref(), Some("pulse"));
    assert_eq!(source(&env).as_deref(), Some("from mGBA (default)"));
    assert_eq!(value(&editor.row(MGBA, "Arguments")).as_deref(), Some("-f"));
}

#[test]
fn editor_shows_each_place_only_its_own_parts_of_a_list_kind_its_routines_first_marked_in_force_or_not(
) {
    let mut fixture = wario()
        .body("device/any", json!({ "hooks": { "use": ["night-mode"], "before": [{ "run": "sync" }] } }))
        .body("game:wario/any", json!({ "runner": "retroarch/mgba", "hooks": { "before": [{ "run": "notify-send 'Wario Land 4'" }] } }))
        .body("game:wario/runner:retroarch/gpsp", json!({ "hooks": { "use": ["night-mode"], "before": [{ "run": "gpsp-bios-check" }] } }))
        .body("profile:quiet/any", json!({ "args": { "append": ["--quiet"] } }))
        .value(contributed(
            "HooksBefore",
            vec![
                (at("device/any"), json!([{ "run": "brightnessctl set 30%" }, { "run": "fan quiet" }]), Some("night-mode")),
                (at("device/any"), json!([{ "run": "sync" }]), None),
                (at("game:wario/any"), json!([{ "run": "notify-send 'Wario Land 4'" }]), None),
            ],
        ))
        .value(contributed("HooksAfter", vec![(at("device/any"), json!([{ "run": "brightnessctl set 100%" }]), Some("night-mode"))]));
    fixture.profiles = vec![("quiet", "Quiet")];
    let editor = edit(&fixture);
    let files = |place_id: &str| -> Vec<(String, String, String, String)> {
        editor
            .rows(place_id)
            .into_iter()
            .filter(|item| item.id.contains("#file:"))
            .map(|item| {
                (
                    item.label,
                    item.value.unwrap(),
                    item.source.unwrap(),
                    item.description.unwrap(),
                )
            })
            .collect()
    };
    let device = "Set in device.yaml. Change it there.";
    let library = "Set in library.yaml. Change it there.";
    let owned = |rows: &[(&str, &str, &str, &str)]| -> Vec<(String, String, String, String)> {
        rows.iter()
            .map(|(a, b, c, d)| (a.to_string(), b.to_string(), c.to_string(), d.to_string()))
            .collect()
    };
    assert_eq!(
        files("device/any"),
        owned(&[
            (
                "Before launch",
                "brightnessctl set 30%; fan quiet",
                "In force: routine night-mode, used by Device",
                device
            ),
            ("Before launch", "sync", "In force: from Device", device),
            (
                "After the session",
                "brightnessctl set 100%",
                "In force: routine night-mode, used by Device",
                device
            ),
        ])
    );
    assert_eq!(
        files("game:wario/any"),
        owned(&[(
            "Before launch",
            "notify-send 'Wario Land 4'",
            "In force: from Game",
            library
        )])
    );
    assert_eq!(
        files("game:wario/runner:retroarch/gpsp"),
        owned(&[
            (
                "Before launch",
                "gpsp-bios-check",
                "Not in force: mGBA runs this game",
                library
            ),
            (
                "Routine",
                "night-mode",
                "Not in force: mGBA runs this game",
                device
            ),
        ])
    );
    assert_eq!(
        files("profile:quiet/any"),
        owned(&[(
            "Arguments after",
            "--quiet",
            "Not in force: this profile isn't picked",
            library
        )])
    );
    assert!(files(MGBA).is_empty());
}

#[test]
fn editor_says_when_a_runners_facts_are_unavailable_in_a_note_and_in_its_places() {
    let mut fixture = wario();
    fixture.runners[1].prepare =
        json!({ "_tag": "Unavailable", "payload": "launch.prepare timed out." });
    let editor = edit(&fixture);
    assert_eq!(
        editor.notes,
        ["gpSP facts are unavailable: launch.prepare timed out."]
    );
    let row = editor.row(
        "game:wario/runner:retroarch/gpsp",
        "Runner facts unavailable",
    );
    assert_eq!(value(&row).as_deref(), Some("launch.prepare timed out."));
    assert_eq!(row.unwrap().interaction, None);
}

#[test]
fn editor_says_when_a_place_names_a_runner_that_isnt_installed() {
    let mut fixture = wario()
        .body("system:gba/any", json!({ "runner": "retroarch/mgba" }))
        .body("game:wario/any", json!({ "runner": "retroarch/vba" }));
    fixture.notices = vec![
        json!({ "_tag": "RunnerUnavailable", "payload": { "runnerId": "retroarch/vba", "place": place("game:wario/any") } }),
    ];
    fixture.resolved = vec![sourced(
        editable(json!({ "_tag": "Runner" })),
        json!("retroarch/mgba"),
        at("system:gba/any"),
    )];
    let editor = edit(&fixture);
    assert_eq!(
        editor.notes,
        ["Game names runner retroarch/vba, which isn't installed. Korri skips it and its runner settings there and uses mGBA."]
    );
    let runner = editor.row("game:wario/any", "Runner").unwrap();
    assert_eq!(
        runner.value.as_deref(),
        Some("retroarch/vba (not installed)")
    );
    assert_eq!(
        runner.description.as_deref(),
        Some("This runner isn't installed. Korri skips it and its runner settings here.")
    );
}

#[tokio::test]
async fn editor_creates_picks_renames_and_deletes_a_profile_and_remembers_the_pick_for_play() {
    let (launch, script) = opened(&wario(), None).await;
    launch.change(launch_id::CREATE, "  ");
    assert_eq!(
        launch.status(),
        Some(SurfaceSettingsStatus::Problem {
            setting_id: launch_id::CREATE.into(),
            message: "A profile needs a name.".into()
        })
    );
    assert!(launch.dismiss_problem());
    let mut created = wario();
    created.profiles = vec![("profile-1", "Speedrun")];
    push(&script, SNAPSHOT, ok_snapshot(created.snapshot()));
    push(&script, "app.launch-settings.profile.save", written_ok());
    launch.change(launch_id::CREATE, "Speedrun");
    settle().await;
    assert_eq!(
        calls(&script, "app.launch-settings.profile.save")[0],
        json!({ "gameId": "wario", "title": "Speedrun", "expectedRevision": "games-1" })
    );
    let editor = launch.editor();
    assert_eq!(content(&editor).profiles[0].title, "Speedrun");
    assert_eq!(
        content(&editor).places.last().unwrap().layer.title,
        "Profile Speedrun"
    );
    assert_eq!(launch.active_profiles().get("wario"), Some(&None));

    let mut picked = created.clone();
    picked.selected_profile = Some("profile-1");
    push(&script, SNAPSHOT, ok_snapshot(picked.snapshot()));
    push(&script, "app.launch-settings.profile.select", written_ok());
    launch.change(launch_id::PICK, "profile-1");
    settle().await;
    assert_eq!(
        calls(&script, "app.launch-settings.profile.select")[0],
        json!({ "gameId": "wario", "profileId": "profile-1", "expectedRevision": "games-1" })
    );
    let editor = launch.editor();
    assert_eq!(
        content(&editor).selected_profile_id.as_deref(),
        Some("profile-1")
    );
    assert_eq!(
        value(&live(&editor).row("game:wario/any", "Profile at next launch")).as_deref(),
        Some("Speedrun")
    );
    assert_eq!(
        launch.active_profiles().get("wario"),
        Some(&Some("Speedrun".into()))
    );

    // A pick of a profile this read never published writes nothing.
    launch.change(launch_id::PICK, "profile-9");
    settle().await;
    assert_eq!(
        calls(&script, "app.launch-settings.profile.select").len(),
        1
    );

    push(&script, "app.launch-settings.profile.save", written_ok());
    launch.change(&launch_id::rename("profile-1"), "Race");
    settle().await;
    assert_eq!(
        calls(&script, "app.launch-settings.profile.save")[1],
        json!({ "gameId": "wario", "profileId": "profile-1", "title": "Race", "expectedRevision": "games-1" })
    );

    push(&script, SNAPSHOT, ok_snapshot(wario().snapshot()));
    push(&script, "app.launch-settings.profile.remove", written_ok());
    assert!(launch.act(&launch_id::remove("profile-1")));
    settle().await;
    assert_eq!(
        calls(&script, "app.launch-settings.profile.remove")[0],
        json!({ "gameId": "wario", "profileId": "profile-1", "expectedRevision": "games-1" })
    );
    let editor = launch.editor();
    assert!(content(&editor).profiles.is_empty());
    assert_eq!(
        value(&live(&editor).row("game:wario/any", "Profile at next launch")).as_deref(),
        Some("No profile")
    );
    assert_eq!(launch.active_profiles().get("wario"), Some(&None));
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
async fn editor_asks_for_a_reload_after_a_write_korrid_refuses_as_stale() {
    let (korrid, script) = scripted();
    push(&script, SNAPSHOT, ok_snapshot(wario().snapshot()));
    push(&script, SET, refused("SettingsConflict", "changed"));
    let launch = controller(korrid);
    launch.open("wario".into(), "Wario Land 4".into(), None);
    settle().await;
    launch.change(&launch_id::set("game:wario/any", "cpu.min"), "20");
    settle().await;
    let editor = launch.editor();
    let SurfaceLaunchSettings::Conflict(problem) = &editor else {
        panic!("{}", tag(&editor))
    };
    assert!(problem.message.contains("changed elsewhere"));
    assert_eq!(action_ids(&editor), [launch_id::RELOAD, launch_id::CANCEL]);
    launch.act(launch_id::RELOAD);
    settle().await;
    assert_eq!(tag(&launch.editor()), "Ready");
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
    launch.open("wario".into(), "Wario Land 4".into(), None);
    settle().await;
    let editor = launch.editor();
    let SurfaceLaunchSettings::Error(problem) = &editor else {
        panic!("{}", tag(&editor))
    };
    assert_eq!(problem.message, "Korri could not read launch settings.");
    assert!(problem.content.places.is_empty());
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
    launch.open("wario".into(), "Wario Land 4".into(), None);
    tokio::task::yield_now().await;
    launch.cancel();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(launch.editor(), SurfaceLaunchSettings::Closed);
}

#[tokio::test]
async fn editor_without_a_runner_choice_says_what_is_missing_and_offers_the_chooser() {
    let mut fixture = wario();
    fixture.system_and_release = false;
    fixture.runners = vec![mgba()];
    fixture.selected = None;
    fixture.bodies.clear();
    fixture.resolved.clear();
    fixture.route_layers = json!({ "_tag": "NeedsRunnerChoice" });
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let opened_count = Arc::clone(&count);
    let (launch, _) = opened(
        &fixture,
        Some(Arc::new(move || {
            opened_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })),
    )
    .await;
    let editor = launch.editor();
    assert_eq!(tag(&editor), "Ready");
    let mut layers: Vec<String> = Vec::new();
    for place in content(&editor).places {
        if !layers.contains(&place.layer.title) {
            layers.push(place.layer.title);
        }
    }
    assert_eq!(layers, ["Device", "Game"]);
    assert_eq!(content(&editor).notes, ["System and release settings appear once you choose a runner. Values in force leave them out."]);
    assert_eq!(
        action_ids(&editor),
        [launch_id::RUNNER, launch_id::RELOAD, launch_id::CANCEL]
    );
    assert!(launch.act(launch_id::RUNNER));
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn editor_does_not_offer_the_chooser_when_the_game_page_has_none() {
    let mut fixture = wario();
    fixture.route_layers = json!({ "_tag": "NeedsRunnerChoice" });
    let (launch, _) = opened(&fixture, None).await;
    assert_eq!(
        action_ids(&launch.editor()),
        [launch_id::RELOAD, launch_id::CANCEL]
    );
}

#[test]
fn editor_names_why_route_places_are_unavailable() {
    let mut fixture = wario();
    fixture.route_layers =
        json!({ "_tag": "Unavailable", "message": "The runner plugin did not load." });
    assert_eq!(
        edit(&fixture).notes,
        ["System and release settings aren't available: The runner plugin did not load. Values in force leave them out."]
    );
}

// --- profiles and choices -------------------------------------------------------------

fn two_quiet() -> Fixture {
    let mut fixture = wario()
        .body("profile:p2/any", json!({ "cpu": { "max": 50 } }))
        .value(sourced(
            editable(json!({ "_tag": "CpuMax" })),
            json!(50),
            at("profile:p2/any"),
        ));
    fixture.profiles = vec![("p1", "Quiet"), ("p2", "Quiet")];
    fixture.selected_profile = Some("p2");
    fixture
}

#[test]
fn profiles_are_picked_by_id_marked_by_value_and_duplicate_titles_told_apart() {
    let editor = edit(&two_quiet());
    let pick = editor.row("game:wario/any", "Profile at next launch");
    assert_eq!(
        pick.as_ref().unwrap().interaction,
        Some(SurfaceSettingInteraction::Choice {
            selected: "p2".into(),
            choices: vec![
                choice("", "No profile"),
                choice("p1", "Quiet"),
                choice("p2", "Quiet (2)")
            ],
        })
    );
    assert_eq!(value(&pick).as_deref(), Some("Quiet (2)"));
    let mut layers: Vec<String> = Vec::new();
    for place in &editor.places {
        if !layers.contains(&place.layer.title) {
            layers.push(place.layer.title.clone());
        }
    }
    assert_eq!(
        layers[layers.len() - 2..],
        ["Profile Quiet", "Profile Quiet (2)"]
    );
    assert_eq!(
        source(&editor.row("game:wario/any", "CPU max")).as_deref(),
        Some("50 % from Profile Quiet (2)")
    );
    let delete = editor.row("profile:p2/any", "Delete profile").unwrap();
    let Some(SurfaceSettingInteraction::Action {
        confirmation: Some(confirmation),
        destructive,
        ..
    }) = delete.interaction
    else {
        panic!()
    };
    assert_eq!(confirmation.title, "Delete Quiet (2)?");
    assert_eq!(destructive, Some(true));
}

#[test]
fn every_launch_choice_row_is_marked_by_value() {
    let fixture = wario().facts(rg353m()).body(
        "device/any",
        json!({ "cpu": { "max": 61, "governor": "schedutil" } }),
    );
    let editor = edit(&fixture);
    assert_eq!(
        selected(&editor.row("device/any", "CPU max")).as_deref(),
        Some("61")
    );
    assert_eq!(
        selected(&editor.row("device/any", "CPU min")).as_deref(),
        Some("inherit")
    );
    assert_eq!(
        selected(&editor.row("device/any", "CPU governor")).as_deref(),
        Some("schedutil")
    );
}

#[test]
fn a_zero_percent_floor_is_offered_on_minimums_only() {
    for facts in [Some(rg353m()), None] {
        let mut fixture = wario();
        fixture.facts = facts;
        let editor = edit(&fixture);
        for label in ["CPU min", "GPU min"] {
            assert_eq!(
                choices(&editor.row("game:wario/any", label))[1],
                choice("0", "0 % (no floor)")
            );
        }
        for label in ["CPU max", "GPU max"] {
            assert!(!choices(&editor.row("game:wario/any", label))
                .iter()
                .any(|choice| choice.value == "0"));
        }
    }
}

// --- modifiers ---------------------------------------------------------------------

fn with_modifiers() -> Fixture {
    let gamescope = json!({
        "id": "korri/gamescope",
        "title": "Gamescope",
        "schema": {
            "type": "object",
            "properties": {
                "video": section("Video", json!({
                    "output": section("Output", json!({ "width": { "type": "integer", "title": "Width", "minimum": 320, "maximum": 7680 } })),
                    "scaling": section("Scaling", json!({ "filter": { "type": "string", "title": "Filter", "enum": ["linear", "fsr"] } })),
                })),
                "window": section("Window", json!({ "borderless": { "type": "boolean", "title": "Borderless" } })),
            },
        },
    });
    let enabled = json!({ "_tag": "ModifierEnabled", "payload": { "id": "korri/gamescope" } });
    let mut fixture = wario()
        .body("device/any", json!({ "modifiers": { "korri/gamescope": { "enabled": true, "settings": { "video": { "output": { "width": 1280 } } } } } }))
        .body("profile:quiet/any", json!({ "modifiers": { "korri/gamescope": { "enabled": false } } }))
        .body("game:wario/any", json!({ "runner": "retroarch/mgba", "modifiers": { "korri/vkbasalt": { "enabled": true } } }))
        .value(sourced(editable(enabled), json!(true), at("device/any")))
        .value(sourced(
            editable(json!({ "_tag": "ModifierSetting", "payload": { "id": "korri/gamescope", "key": "video.output.width" } })),
            json!(1280),
            at("device/any"),
        ));
    fixture.modifiers = vec![
        gamescope,
        json!({ "id": "korri/mangohud", "title": "MangoHud" }),
    ];
    fixture.profiles = vec![("quiet", "Quiet")];
    fixture
}

#[test]
fn modifiers_are_listed_on_every_place_inherited_with_their_source_and_settings_by_section() {
    let editor = edit(&with_modifiers());
    let gamescope = editor.group(MGBA, "Modifier Gamescope").unwrap();
    assert_eq!(gamescope.items.len(), 1);
    let row = &gamescope.items[0];
    assert_eq!(
        (
            row.label.as_str(),
            row.value.as_deref(),
            row.source.as_deref()
        ),
        ("Gamescope", Some("Inherit"), Some("On from Device"))
    );
    assert_eq!(row.scope, Some(SurfaceSettingScope::Device));
    assert_eq!(
        row.interaction,
        Some(SurfaceSettingInteraction::Choice {
            selected: "inherit".into(),
            choices: vec![
                choice("inherit", "Inherit"),
                choice("true", "On"),
                choice("false", "Off")
            ],
        })
    );
    let named: Vec<String> = editor
        .titles(MGBA)
        .into_iter()
        .filter(|title| title.contains("Gamescope"))
        .collect();
    assert_eq!(
        named,
        [
            "Modifier Gamescope",
            "Gamescope · Video",
            "Gamescope · Window"
        ]
    );
    assert_eq!(
        labels(&editor.group(MGBA, "Gamescope · Video").unwrap().items),
        ["Add setting"]
    );
    let device: Vec<(String, Option<String>)> = editor
        .group("device/any", "Gamescope · Video")
        .unwrap()
        .items
        .into_iter()
        .map(|item| (item.label, item.value))
        .collect();
    assert_eq!(
        device,
        [
            ("Output width".to_owned(), Some("1280".to_owned())),
            ("Default Output width".to_owned(), Some("Use".to_owned())),
            (
                "Add setting".to_owned(),
                Some("Choose a setting".to_owned())
            ),
        ]
    );
}

#[test]
fn a_profile_turning_a_modifier_off_shows_and_is_in_force_once_picked() {
    let mut fixture = with_modifiers();
    fixture.selected_profile = Some("quiet");
    fixture.resolved[1] = sourced(
        editable(json!({ "_tag": "ModifierEnabled", "payload": { "id": "korri/gamescope" } })),
        json!(false),
        at("profile:quiet/any"),
    );
    let editor = edit(&fixture);
    let own = editor
        .group("profile:quiet/any", "Modifier Gamescope")
        .unwrap()
        .items[0]
        .clone();
    assert_eq!(
        (own.value.as_deref(), own.source.as_deref()),
        (Some("Off"), Some("from Profile Quiet"))
    );
    let game = editor
        .group("game:wario/any", "Modifier Gamescope")
        .unwrap()
        .items[0]
        .clone();
    assert_eq!(
        (game.value.as_deref(), game.source.as_deref()),
        (Some("Inherit"), Some("Off from Profile Quiet"))
    );
}

#[test]
fn a_modifier_with_no_schema_has_its_on_off_row_and_nothing_else() {
    let editor = edit(&with_modifiers());
    assert_eq!(
        labels(
            &editor
                .group("game:wario/any", "Modifier MangoHud")
                .unwrap()
                .items
        ),
        ["MangoHud"]
    );
    assert!(!editor
        .titles("game:wario/any")
        .iter()
        .any(|title| title.starts_with("MangoHud")));
}

#[test]
fn a_modifier_a_place_sets_that_isnt_installed_is_named() {
    let editor = edit(&with_modifiers());
    let row = editor
        .group("game:wario/any", "Modifier korri/vkbasalt")
        .unwrap()
        .items[0]
        .clone();
    assert_eq!(row.value.as_deref(), Some("On"));
    assert_eq!(
        row.description.as_deref(),
        Some("This modifier isn't installed or is turned off on this device.")
    );
}

#[test]
fn a_modifier_setting_added_pending_names_what_holds_and_writes_a_modifier_setting() {
    let place_id = "game:wario/any";
    let pending = adding(
        place_id,
        vec![PendingSetting::Modifier {
            id: "korri/gamescope".into(),
            key: "video.output.width".into(),
        }],
    );
    let editor = launch_editor_from(&with_modifiers().snapshot(), Some("wario"), &pending);
    let live = Editor {
        places: editor.places.clone(),
        notes: Vec::new(),
    };
    let row = live.row(place_id, "Output width").unwrap();
    assert_eq!(
        (row.value.as_deref(), row.source.as_deref()),
        (Some("Not set here"), Some("1280 from Device"))
    );
    assert_eq!(row.scope, Some(SurfaceSettingScope::Device));
    assert_eq!(
        row.interaction,
        Some(SurfaceSettingInteraction::Number {
            value: None,
            min: Some(320.0),
            max: Some(7680.0),
            step: Some(1.0),
            clearable: Some(true)
        })
    );
    let id = launch_id::set(
        place_id,
        "modifier:korri/gamescope:setting:video.output.width",
    );
    let write = editor.rows.writes.get(&id).unwrap();
    assert_eq!(
        write.path,
        json!({ "_tag": "ModifierSetting", "payload": { "id": "korri/gamescope", "key": "video.output.width" } })
    );
    assert_eq!(write.encoder.encode("100"), None);
    assert_eq!(
        write.encoder.encode("1920"),
        Some(LaunchEncoded {
            value: Some(json!(1920)),
            clear: false
        })
    );
    // A modifier's pending setting goes back through Inherit alone.
    assert!(editor.rows.pending.contains_key(&id));
}

// --- string settings -------------------------------------------------------------------

#[test]
fn string_settings_hold_raw_text_take_an_empty_string_and_go_back_to_inherit_through_their_own_row()
{
    let lang = "setting:locale.language";
    let editor = edit_with(&wario(), &adding(MGBA, vec![runner_key("locale.language")]));
    let pending = editor.row(MGBA, "Language").unwrap();
    assert_eq!(pending.scope, Some(SurfaceSettingScope::Device));
    assert_eq!(pending.value, None);
    assert_eq!(
        pending.interaction,
        Some(SurfaceSettingInteraction::Text {
            placeholder: Some("Not set here".into()),
            max_length: None,
            allow_empty: Some(true)
        })
    );
    assert_eq!(
        editor.row(MGBA, "Inherit Language").unwrap().interaction,
        Some(SurfaceSettingInteraction::Action {
            action_id: launch_id::set(MGBA, &format!("clear:{lang}")),
            destructive: None,
            confirmation: None
        })
    );
    assert_eq!(
        editor.row(MGBA, "Default Language").unwrap().interaction,
        Some(SurfaceSettingInteraction::Action {
            action_id: launch_id::set(MGBA, &format!("default:{lang}")),
            destructive: None,
            confirmation: None
        })
    );

    let set = |text: &str| {
        wario()
            .body(
                MGBA,
                json!({ "settings": { "locale": { "language": text } } }),
            )
            .value(sourced(
                editable(setting("locale.language")),
                json!(text),
                at(MGBA),
            ))
    };
    let de = edit(&set("de")).row(MGBA, "Language");
    assert_eq!(
        (value(&de).as_deref(), source(&de).as_deref()),
        (Some("de"), Some("from Game for mGBA"))
    );
    // An empty string is a value of its own, not a clear.
    let empty = edit(&set("")).row(MGBA, "Language").unwrap();
    assert_eq!(empty.value, None);
    assert_eq!(empty.source.as_deref(), Some("from Game for mGBA"));
    let Some(SurfaceSettingInteraction::Text { placeholder, .. }) = empty.interaction else {
        panic!()
    };
    assert_eq!(placeholder.as_deref(), Some("Empty"));
    // Without a value the Add setting row offers it again.
    let add = edit(&wario()).group(MGBA, "mGBA · Language").unwrap().items[0].clone();
    assert!(choices(&Some(add))
        .iter()
        .any(|choice| choice.value == "locale.language"));
}

#[tokio::test]
async fn a_pending_string_is_withdrawn_through_inherit_without_writing() {
    let (launch, script) = opened(&wario(), None).await;
    launch.change(
        &launch_id::set(MGBA, "add-setting:locale"),
        "locale.language",
    );
    assert!(live(&launch.editor()).row(MGBA, "Language").is_some());
    assert!(launch.act(&launch_id::set(MGBA, "clear:setting:locale.language")));
    assert!(live(&launch.editor()).row(MGBA, "Language").is_none());
    settle().await;
    assert!(calls(&script, SET).is_empty());
}

#[tokio::test]
async fn a_written_modifier_string_is_taken_away_through_inherit() {
    let mut fixture = wario().body("device/any", json!({ "modifiers": { "korri/gamescope": { "settings": { "backend": { "name": "wayland" } } } } }));
    fixture.modifiers = vec![json!({
        "id": "korri/gamescope",
        "title": "Gamescope",
        "schema": { "properties": { "backend": section("Backend", json!({ "name": { "type": "string", "title": "Backend" } })) } },
    })];
    let (launch, script) = opened(&fixture, None).await;
    assert_eq!(
        value(&live(&launch.editor()).row("device/any", "Backend")).as_deref(),
        Some("wayland")
    );
    push(&script, SNAPSHOT, ok_snapshot(wario().snapshot()));
    assert!(launch.act(&launch_id::set(
        "device/any",
        "clear:modifier:korri/gamescope:setting:backend.name"
    )));
    settle().await;
    let sent = calls(&script, SET);
    assert_eq!(
        sent[0],
        json!({
            "place": place("device/any"),
            "path": { "_tag": "ModifierSetting", "payload": { "id": "korri/gamescope", "key": "backend.name" } },
            "clear": false,
            "expectedRevision": "device-1",
        })
    );
    assert!(live(&launch.editor())
        .row("device/any", "Backend")
        .is_none());
}

// --- Default ------------------------------------------------------------------------

#[tokio::test]
async fn default_is_offered_on_every_choice_row_and_writes_clear_true() {
    let device_mgba = "device/runner:retroarch/mgba";
    let fixture = wario()
        .body(
            device_mgba,
            json!({ "settings": { "video": { "frameskip": 4, "color-correction": true } } }),
        )
        .value(sourced(
            editable(setting("video.frameskip")),
            json!(4),
            at(device_mgba),
        ))
        .value(sourced(
            editable(setting("video.color-correction")),
            json!(true),
            at(device_mgba),
        ));
    let (launch, script) = opened(&fixture, None).await;
    let color = live(&launch.editor()).row(device_mgba, "Colour correction");
    assert_eq!(
        color.unwrap().interaction,
        Some(SurfaceSettingInteraction::Choice {
            selected: "true".into(),
            choices: vec![
                choice("inherit", "Inherit"),
                choice("default", "Default"),
                choice("true", "On"),
                choice("false", "Off")
            ],
        })
    );
    let cleared = wario()
        .body(
            device_mgba,
            json!({ "settings": { "video": { "frameskip": null, "color-correction": null } } }),
        )
        .value(sourced(
            editable(setting("video.frameskip")),
            json!(0),
            plugin_default("retroarch/mgba"),
        ));
    push(&script, SNAPSHOT, ok_snapshot(cleared.snapshot()));
    launch.change(
        &launch_id::set(device_mgba, "setting:video.color-correction"),
        "default",
    );
    settle().await;
    assert_eq!(
        calls(&script, SET)[0],
        json!({ "place": place(device_mgba), "path": setting("video.color-correction"), "clear": true, "expectedRevision": "device-1" })
    );
    // No default in mGBA's schema: mGBA decides.
    let editor = live(&launch.editor());
    let color = editor.row(device_mgba, "Colour correction");
    assert_eq!(
        (value(&color).as_deref(), source(&color).as_deref()),
        (Some("Default"), Some("mGBA decides"))
    );
    assert_eq!(selected(&color).as_deref(), Some("default"));
    // A number's Default is a row of its own.
    let frameskip = editor.row(device_mgba, "Frameskip").unwrap();
    assert_eq!(
        (frameskip.value.as_deref(), frameskip.source.as_deref()),
        (Some("Default"), Some("0 from mGBA (default)"))
    );
    assert_eq!(
        frameskip.interaction,
        Some(SurfaceSettingInteraction::Number {
            value: None,
            min: Some(0.0),
            max: Some(10.0),
            step: Some(1.0),
            clearable: Some(true)
        })
    );
    assert!(editor.row(device_mgba, "Default Frameskip").is_none());
    // Inherit from Default takes the place's value away: no value, no clear.
    launch.change(&launch_id::set(device_mgba, "setting:video.frameskip"), "");
    settle().await;
    assert_eq!(
        calls(&script, SET)[1],
        json!({ "place": place(device_mgba), "path": setting("video.frameskip"), "clear": false, "expectedRevision": "device-1" })
    );
}

#[tokio::test]
async fn a_numbers_default_row_writes_a_cleared_setting() {
    let device_mgba = "device/runner:retroarch/mgba";
    let fixture = wario()
        .body(
            device_mgba,
            json!({ "settings": { "video": { "frameskip": 4 } } }),
        )
        .value(sourced(
            editable(setting("video.frameskip")),
            json!(4),
            at(device_mgba),
        ));
    let (launch, script) = opened(&fixture, None).await;
    assert!(launch.act(&launch_id::set(
        device_mgba,
        "default:setting:video.frameskip"
    )));
    settle().await;
    let sent = calls(&script, SET);
    assert_eq!(sent[0]["path"], setting("video.frameskip"));
    assert_eq!(sent[0]["clear"], json!(true));
    assert_eq!(sent[0].get("value"), None);
}

#[test]
fn default_clears_what_less_specific_places_set_so_a_more_specific_row_names_the_plugins_default() {
    let fixture = wario()
        .body(
            "device/runner:retroarch/mgba",
            json!({ "settings": { "video": { "frameskip": 4 } } }),
        )
        .body(
            MGBA,
            json!({ "settings": { "video": { "frameskip": null } } }),
        )
        .value(sourced(
            editable(setting("video.frameskip")),
            json!(0),
            plugin_default("retroarch/mgba"),
        ));
    let editor = edit(&fixture);
    let device = editor.row("device/runner:retroarch/mgba", "Frameskip");
    assert_eq!(
        (value(&device).as_deref(), source(&device).as_deref()),
        (Some("4"), Some("0 from mGBA (default)"))
    );
    let game = editor.row(MGBA, "Frameskip");
    assert_eq!(
        (value(&game).as_deref(), source(&game).as_deref()),
        (Some("Default"), Some("0 from mGBA (default)"))
    );
}

// --- skips ----------------------------------------------------------------------------

fn shared_fps() -> Fixture {
    let mut fixture = wario();
    fixture.runners[0].schema = json!({ "properties": { "video": section("Video", json!({ "fps": { "type": "integer", "title": "FPS", "maximum": 60 } })) } });
    fixture.runners.push(moonlight());
    fixture
}

fn skipped(key: &str, message: &str, runner_id: Option<&str>) -> Value {
    let mut payload = json!({ "path": setting(key), "message": message });
    if let Some(runner_id) = runner_id {
        payload["runnerId"] = json!(runner_id);
    }
    json!({ "_tag": "SettingSkipped", "payload": payload })
}

#[test]
fn skips_show_the_selected_runners_skip_on_its_rows_only_not_on_moonlights_same_path() {
    let mut fixture = shared_fps()
        .body(
            "device/runner:retroarch/mgba",
            json!({ "settings": { "video": { "fps": 90 } } }),
        )
        .body(
            &format!("device/runner:{MOONLIGHT}"),
            json!({ "settings": { "video": { "fps": 30 } } }),
        );
    fixture.notices = vec![skipped("video.fps", "90 is above the maximum of 60.", None)];
    fixture.runner_resolved = vec![(
        MOONLIGHT,
        vec![sourced(
            editable(setting("video.fps")),
            json!(30),
            at(&format!("device/runner:{MOONLIGHT}")),
        )],
    )];
    let editor = edit(&fixture);
    assert_eq!(
        source(&editor.row("device/runner:retroarch/mgba", "FPS")).as_deref(),
        Some("Not applied: 90 is above the maximum of 60")
    );
    assert_eq!(
        source(&editor.row(&format!("device/runner:{MOONLIGHT}"), "FPS")).as_deref(),
        Some("from Device for Moonlight")
    );
    assert_eq!(
        editor.notes,
        ["FPS (mGBA) isn't applied: 90 is above the maximum of 60."]
    );
}

#[test]
fn skips_show_moonlights_skip_on_moonlights_rows_when_korrid_names_moonlight() {
    let mut fixture = shared_fps()
        .body(
            "device/runner:retroarch/mgba",
            json!({ "settings": { "video": { "fps": 50 } } }),
        )
        .body(
            &format!("device/runner:{MOONLIGHT}"),
            json!({ "settings": { "video": { "fps": 500 } } }),
        )
        .value(sourced(
            editable(setting("video.fps")),
            json!(50),
            at("device/runner:retroarch/mgba"),
        ));
    fixture.notices = vec![skipped(
        "video.fps",
        "500 is above the maximum of 240.",
        Some(MOONLIGHT),
    )];
    let editor = edit(&fixture);
    assert_eq!(
        source(&editor.row(&format!("device/runner:{MOONLIGHT}"), "FPS")).as_deref(),
        Some("Not applied: 500 is above the maximum of 240")
    );
    assert_eq!(
        source(&editor.row("device/runner:retroarch/mgba", "FPS")).as_deref(),
        Some("from Device for mGBA")
    );
    assert_eq!(
        editor.notes,
        ["FPS (Moonlight) isn't applied: 500 is above the maximum of 240."]
    );
    // A Game row names what is in force instead: nothing, so it says when it applies.
    let game = format!("game:wario/runner:{MOONLIGHT}");
    let editor = edit_with(&fixture, &adding(&game, vec![runner_key("video.fps")]));
    assert_eq!(
        source(&editor.row(&game, "FPS")).as_deref(),
        Some("Applies when this device streams the game")
    );
}

#[test]
fn skips_say_on_their_rows_and_in_a_note_that_the_value_is_not_applied_and_why() {
    let mut fixture = wario()
        .body(
            "device/runner:retroarch/mgba",
            json!({ "settings": { "video": { "frameskip": 30 } } }),
        )
        .value(sourced(
            editable(setting("video.frameskip")),
            json!(0),
            plugin_default("retroarch/mgba"),
        ));
    fixture.notices = vec![skipped(
        "video.frameskip",
        "30 is above the maximum of 10.",
        None,
    )];
    let editor = edit(&fixture);
    let row = editor.row("device/runner:retroarch/mgba", "Frameskip");
    assert_eq!(
        (value(&row).as_deref(), source(&row).as_deref()),
        (
            Some("30"),
            Some("Not applied: 30 is above the maximum of 10")
        )
    );
    assert!(editor.row("game:wario/any", "Frameskip").is_none());
    let pending = edit_with(&fixture, &adding(MGBA, vec![runner_key("video.frameskip")]));
    assert_eq!(
        source(&pending.row(MGBA, "Frameskip")).as_deref(),
        Some("0 from mGBA (default)")
    );
    assert_eq!(
        editor.notes,
        ["Frameskip (mGBA) isn't applied: 30 is above the maximum of 10."]
    );
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

#[test]
fn moonlight_is_its_own_scope_with_groups_by_section_its_values_from_korrids_stream_fold_and_never_a_runner_choice(
) {
    let fixture = with_moonlight();
    let editor = edit(&fixture);
    let game = format!("game:wario/runner:{MOONLIGHT}");
    let device = format!("device/runner:{MOONLIGHT}");
    let scopes: Vec<String> = editor
        .places
        .iter()
        .filter(|place| place.layer.id == "game:wario")
        .map(|place| place.scope.label.clone())
        .collect();
    assert_eq!(
        scopes,
        [
            "Any runner",
            "Family retroarch",
            "mGBA",
            "gpSP",
            "Moonlight"
        ]
    );
    assert_eq!(
        editor.titles(&game)[..2],
        ["Moonlight · Video", "Moonlight · Audio"]
    );
    let runner: Vec<String> = choices(&editor.row("game:wario/any", "Runner"))
        .into_iter()
        .map(|choice| choice.label)
        .collect();
    assert_eq!(runner, ["Inherit", "mGBA", "gpSP"]);
    let channels = editor.row(&game, "Channels").unwrap();
    assert_eq!(
        (channels.value.as_deref(), channels.source.as_deref()),
        (Some("stereo"), Some("from Game for Moonlight"))
    );
    assert_eq!(channels.scope, Some(SurfaceSettingScope::Device));
    assert_eq!(
        channels.description.as_deref(),
        Some("Applies when this device streams the game.")
    );
    assert_eq!(
        source(&editor.row(&game, "CPU max")).as_deref(),
        Some("Applies when this device streams the game")
    );
    let codec = editor.row(&device, "Codec");
    assert_eq!(
        (value(&codec).as_deref(), source(&codec).as_deref()),
        (Some("Default"), Some("h264 from Moonlight (default)"))
    );
    let fps = editor.row(&device, "FPS");
    assert_eq!(
        (value(&fps).as_deref(), source(&fps).as_deref()),
        (Some("30"), Some("from Device for Moonlight"))
    );
    let pending = edit_with(
        &fixture,
        &adding(
            &game,
            vec![
                runner_key("video.fps"),
                runner_key("video.codec"),
                runner_key("video.hdr.enable"),
            ],
        ),
    );
    let fps = pending.row(&game, "FPS");
    assert_eq!(
        (value(&fps).as_deref(), source(&fps).as_deref()),
        (Some("Not set here"), Some("30 from Device for Moonlight"))
    );
    assert_eq!(
        source(&pending.row(&game, "Codec")).as_deref(),
        Some("h264 from Moonlight (default)")
    );
    assert_eq!(
        source(&pending.row(&game, "HDR")).as_deref(),
        Some("Applies when this device streams the game")
    );
    assert_eq!(
        choices(
            &editor
                .group(&device, "Moonlight · Video")
                .unwrap()
                .items
                .last()
                .cloned()
        ),
        vec![choice("video.hdr.enable", "HDR")]
    );
    assert!(editor.notes.is_empty());
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

#[tokio::test]
async fn a_refused_runner_shows_korrids_message_with_retry() {
    let (korrid, script) = scripted();
    push(&script, SNAPSHOT, ok_snapshot(with_moonlight().snapshot()));
    push(
        &script,
        SET,
        refused(
            "SettingsInvalid",
            "@korri:moonlight/moonlight runs no game; it views streams.",
        ),
    );
    let launch = controller(korrid);
    launch.open("wario".into(), "Wario Land 4".into(), None);
    settle().await;
    launch.change(&launch_id::set("game:wario/any", "runner"), MOONLIGHT);
    settle().await;
    let editor = launch.editor();
    let SurfaceLaunchSettings::Error(problem) = &editor else {
        panic!("{}", tag(&editor))
    };
    assert_eq!(
        problem.message,
        "@korri:moonlight/moonlight runs no game; it views streams."
    );
    // Retry repeats the write from the editor it was made in.
    push(&script, SET, written_ok());
    assert!(launch.act(launch_id::RETRY));
    settle().await;
    assert_eq!(calls(&script, SET).len(), 2);
    assert_eq!(tag(&launch.editor()), "Ready");
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
fn sunshine_has_groups_by_section_on_every_place_device_only_and_writes_a_service_setting() {
    let fixture = with_sunshine();
    let editor = edit(&fixture);
    for place_id in ["device/any", "game:wario/any", MGBA] {
        let named: Vec<String> = editor
            .titles(place_id)
            .into_iter()
            .filter(|title| title.starts_with("Sunshine"))
            .collect();
        assert_eq!(named, ["Sunshine · Video", "Sunshine · Input"]);
    }
    let row = editor.row("device/any", "Max bitrate (kbps)").unwrap();
    assert_eq!(
        (row.value.as_deref(), row.source.as_deref(), row.scope),
        (
            Some("3000"),
            Some("from Device"),
            Some(SurfaceSettingScope::Device)
        )
    );
    let pending = launch_editor_from(
        &fixture.snapshot(),
        Some("wario"),
        &adding(
            MGBA,
            vec![PendingSetting::Service {
                id: SUNSHINE.into(),
                key: "video.max-bitrate-kbps".into(),
            }],
        ),
    );
    let live = Editor {
        places: pending.places.clone(),
        notes: Vec::new(),
    };
    let row = live.row(MGBA, "Max bitrate (kbps)").unwrap();
    assert_eq!(
        (row.value.as_deref(), row.source.as_deref()),
        (Some("Not set here"), Some("3000 from Device"))
    );
    let write = &pending.rows.writes[&launch_id::set(
        MGBA,
        &format!("service:{SUNSHINE}:setting:video.max-bitrate-kbps"),
    )];
    assert_eq!(
        write.path,
        json!({ "_tag": "ServiceSetting", "payload": { "id": SUNSHINE, "key": "video.max-bitrate-kbps" } })
    );
    assert_eq!(
        write.encoder.encode("8000"),
        Some(LaunchEncoded {
            value: Some(json!(8000)),
            clear: false
        })
    );
    assert_eq!(
        editor
            .group("game:wario/any", "Sunshine · Video")
            .unwrap()
            .items[0]
            .label,
        "Add setting"
    );
    // A cleared service setting names the service's default.
    let cleared = fixture.body("game:wario/any", json!({ "runner": "retroarch/mgba", "services": { SUNSHINE: { "settings": { "input": { "gamepad": null } } } } }));
    let row = edit(&cleared).row("game:wario/any", "Gamepad");
    assert_eq!(
        (value(&row).as_deref(), source(&row).as_deref()),
        (Some("Default"), Some("auto from Sunshine (default)"))
    );
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

// --- input ports ----------------------------------------------------------------------

fn with_ports(body: Option<Value>) -> Fixture {
    let mut fixture = wario();
    fixture.runners[0].schema = json!({
        "type": "object",
        "title": "RetroArch",
        "properties": {
            "input": {
                "type": "object",
                "title": "Input",
                "additionalProperties": false,
                "properties": {
                    "max-users": { "type": "integer", "title": "Max users", "minimum": 1, "maximum": 9_007_199_254_740_991_u64 },
                    "ports": {
                        "type": "object",
                        "title": "Ports",
                        "additionalProperties": false,
                        "patternProperties": {
                            "^[1-9][0-9]*$": {
                                "type": "object",
                                "title": "Input port",
                                "additionalProperties": false,
                                "properties": {
                                    "libretro-device": { "type": "integer", "title": "Libretro device", "minimum": 0, "maximum": 9_007_199_254_740_991_u64 },
                                    "joypad-index": { "type": "integer", "title": "Joypad index", "minimum": 0, "maximum": 9_007_199_254_740_991_u64 },
                                    "analog-dpad-mode": { "type": "string", "title": "Analog dpad mode", "enum": ["none", "left-analog", "right-analog", "left-analog-forced", "right-analog-forced"] },
                                },
                            },
                        },
                    },
                },
            },
        },
    });
    if let Some(body) = body {
        let device = "device/runner:retroarch/mgba";
        fixture = fixture.body(device, body).value(sourced(
            editable(setting("input.ports.1.joypad-index")),
            json!(0),
            at(device),
        ));
    }
    fixture
}

#[test]
fn ports_offer_port_1s_settings_when_no_port_is_set_each_named_by_its_port() {
    let editor = edit(&with_ports(None));
    assert_eq!(
        choices(
            &editor
                .group(MGBA, "mGBA · Input")
                .unwrap()
                .items
                .last()
                .cloned()
        ),
        vec![
            choice("input.max-users", "Max users"),
            choice(
                "input.ports.1.libretro-device",
                "Input port 1 libretro device"
            ),
            choice("input.ports.1.joypad-index", "Input port 1 joypad index"),
            choice(
                "input.ports.1.analog-dpad-mode",
                "Input port 1 analog dpad mode"
            ),
        ]
    );
}

#[test]
fn an_authored_port_leaf_has_its_schema_and_the_port_and_the_next_free_port_are_offered() {
    let device = "device/runner:retroarch/mgba";
    let editor = edit(&with_ports(Some(
        json!({ "settings": { "input": { "ports": { "1": { "joypad-index": 0 } } } } }),
    )));
    let row = editor.row(device, "Input port 1 joypad index").unwrap();
    assert_eq!(
        (row.value.as_deref(), row.source.as_deref()),
        (Some("0"), Some("from Device for mGBA"))
    );
    let Some(SurfaceSettingInteraction::Number { min, step, .. }) = row.interaction else {
        panic!()
    };
    assert_eq!((min, step), (Some(0.0), Some(1.0)));
    assert_eq!(row.description, None);
    let offered: Vec<String> = choices(
        &editor
            .group(device, "mGBA · Input")
            .unwrap()
            .items
            .last()
            .cloned(),
    )
    .into_iter()
    .map(|choice| choice.value)
    .collect();
    assert_eq!(
        offered,
        [
            "input.max-users",
            "input.ports.1.libretro-device",
            "input.ports.1.analog-dpad-mode",
            "input.ports.2.libretro-device",
            "input.ports.2.joypad-index",
            "input.ports.2.analog-dpad-mode",
        ]
    );
}

#[test]
fn a_leaf_of_the_next_port_is_added_and_written_under_that_ports_key() {
    let device = "device/runner:retroarch/mgba";
    let fixture = with_ports(Some(
        json!({ "settings": { "input": { "ports": { "1": { "joypad-index": 0 } } } } }),
    ));
    let built = launch_editor_from(
        &fixture.snapshot(),
        Some("wario"),
        &adding(device, vec![runner_key("input.ports.2.analog-dpad-mode")]),
    );
    let editor = Editor {
        places: built.places.clone(),
        notes: Vec::new(),
    };
    assert!(matches!(
        editor
            .row(device, "Input port 2 analog dpad mode")
            .unwrap()
            .interaction,
        Some(SurfaceSettingInteraction::Choice { .. })
    ));
    // With port 2 in use, port 3 is the next one offered.
    assert!(choices(
        &editor
            .group(device, "mGBA · Input")
            .unwrap()
            .items
            .last()
            .cloned()
    )
    .iter()
    .any(|choice| choice.value == "input.ports.3.joypad-index"));
    let write =
        &built.rows.writes[&launch_id::set(device, "setting:input.ports.2.analog-dpad-mode")];
    assert_eq!(write.path, setting("input.ports.2.analog-dpad-mode"));
    assert_eq!(
        write.encoder.encode("\"left-analog\""),
        Some(LaunchEncoded {
            value: Some(json!("left-analog")),
            clear: false
        })
    );
}

// --- korrid's snapshot as it is ------------------------------------------------------

mod snapshot_as_it_is {
    use super::*;

    const MOON: &str = "@korri:moonlight/moonlight";
    const GAMESCOPE: &str = "@korri:gamescope/gamescope";
    const DEVICE_MOON: &str = "device/runner:@korri:moonlight/moonlight";
    const GAME_MOON: &str = "game:wario/runner:@korri:moonlight/moonlight";
    const MALFORMED: &str = "setting values must be scalars, nested objects or null";

    fn entry(id: &str, title: &str, body: Value, any: bool) -> Value {
        json!({
            "place": place(id),
            "title": title,
            "body": body,
            "editableKinds": if any { json!(["runner", "cpu", "gpu", "modifiers", "services"]) } else { json!(["cpu", "gpu", "modifiers", "settings", "services"]) },
        })
    }

    fn codec(value: Value, source: Value) -> Value {
        sourced(editable(setting("video.codec")), value, source)
    }

    /// Moonlight's video.codec is "vp9" on the Device: korrid rejects it,
    /// leaves it out of runnerResolved and puts its package's default in force.
    fn snapshot() -> Value {
        json!({
            "places": [
                entry("device/any", "Device", json!({ "modifiers": { GAMESCOPE: { "enabled": true } } }), true),
                entry(DEVICE_MOON, "Device for Moonlight", json!({ "settings": { "video": { "codec": "vp9" } } }), false),
                entry("game:wario/any", "Game", json!({}), true),
                entry(GAME_MOON, "Game for Moonlight", json!({}), false),
            ],
            "resolved": { "values": {
                "modifiers.@korri:gamescope/gamescope.settings.video.fps-limit": sourced(
                    editable(json!({ "_tag": "ModifierSetting", "payload": { "id": GAMESCOPE, "key": "video.fps-limit" } })),
                    json!(60),
                    plugin_default("@korri:gamescope"),
                ),
                "modifiers.@korri:gamescope/gamescope.settings.video.nested.fps": sourced(
                    editable(json!({ "_tag": "ModifierSetting", "payload": { "id": GAMESCOPE, "key": "video.nested.fps" } })),
                    json!(30),
                    plugin_default("@korri:unknown"),
                ),
            } },
            "runners": [{ "id": MOON, "title": "Moonlight", "selected": false, "choosable": false, "schema": moonlight_schema(), "revision": "r1", "prepare": { "_tag": "Ready", "payload": { "env": {}, "args": [] } } }],
            "runnerResolved": [{ "runnerId": MOON, "values": { "settings.video.codec": codec(json!("h264"), plugin_default("@korri:moonlight")) } }],
            "modifiers": [{
                "id": GAMESCOPE,
                "title": "Gamescope",
                "schema": { "type": "object", "properties": { "video": section("Video", json!({
                    "fps-limit": { "type": "integer", "title": "FPS limit", "default": 60 },
                    "nested": section("Nested", json!({ "fps": { "type": "integer", "title": "FPS" } })),
                })) } },
            }],
            "services": [],
            "profiles": [],
            "notices": [skipped("video.codec", "\"vp9\" is not one of auto, h264, h265, av1.", Some(MOON))],
            "routeLayers": { "_tag": "Present" },
            "revisions": { "device": "device-1", "games": "games-1" },
        })
    }

    fn rows(snapshot: &Value, pending: HashMap<String, Vec<PendingSetting>>) -> Editor {
        let editor = launch_editor_from(snapshot, Some("wario"), &pending);
        Editor {
            places: editor.places,
            notes: editor.notes,
        }
    }

    fn two_places(device: Value, game: Value, message: &str) -> Value {
        let mut built = snapshot();
        built["places"][1]["body"] = json!({ "settings": { "video": { "codec": device } } });
        built["places"][3]["body"] = json!({ "settings": { "video": { "codec": game.clone() } } });
        built["runnerResolved"] = json!([{ "runnerId": MOON, "values": { "settings.video.codec": codec(game, at(GAME_MOON)) } }]);
        built["notices"] = json!([skipped("video.codec", message, Some(MOON))]);
        built
    }

    #[test]
    fn reports_the_rejected_value_on_the_place_that_set_it_and_the_value_in_force_with_its_own_source_elsewhere(
    ) {
        let editor = rows(
            &snapshot(),
            adding(GAME_MOON, vec![runner_key("video.codec")]),
        );
        let device = editor.row(DEVICE_MOON, "Codec");
        assert_eq!(
            (value(&device).as_deref(), source(&device).as_deref()),
            (
                Some("vp9"),
                Some("Not applied: \"vp9\" is not one of auto, h264, h265, av1")
            )
        );
        let game = editor.row(GAME_MOON, "Codec");
        assert_eq!(
            (value(&game).as_deref(), source(&game).as_deref()),
            (Some("Not set here"), Some("h264 from Moonlight (default)"))
        );
        assert_eq!(
            rows(&snapshot(), HashMap::new()).notes,
            ["Codec (Moonlight) isn't applied: \"vp9\" is not one of auto, h264, h265, av1."]
        );
    }

    #[test]
    fn never_marks_the_valid_value_in_force_as_rejected_the_malformed_less_specific_value_carries_the_skip(
    ) {
        let editor = rows(
            &two_places(json!([1, 2]), json!("h265"), MALFORMED),
            HashMap::new(),
        );
        let game = editor.row(GAME_MOON, "Codec");
        assert_eq!(
            (value(&game).as_deref(), source(&game).as_deref()),
            (Some("h265"), Some("from Game for Moonlight"))
        );
        assert_eq!(
            source(&editor.row(DEVICE_MOON, "Codec")),
            Some(format!("Not applied: {MALFORMED}"))
        );
        assert_eq!(
            editor.notes,
            [format!("Codec (Moonlight) isn't applied: {MALFORMED}.")]
        );
    }

    #[test]
    fn keeps_a_skip_it_cannot_lay_on_a_place_in_the_notes_only() {
        let editor = rows(
            &two_places(json!("h264"), json!("h265"), "rejected by the plugin"),
            HashMap::new(),
        );
        assert_eq!(
            source(&editor.row(DEVICE_MOON, "Codec")).as_deref(),
            Some("h265 from Game for Moonlight")
        );
        assert_eq!(
            source(&editor.row(GAME_MOON, "Codec")).as_deref(),
            Some("from Game for Moonlight")
        );
        assert_eq!(
            editor.notes,
            ["Codec (Moonlight) isn't applied: rejected by the plugin."]
        );
    }

    #[test]
    fn still_reads_a_single_rejected_value_on_its_place_with_the_default_in_force_elsewhere() {
        let editor = rows(&snapshot(), HashMap::new());
        assert_eq!(
            source(&editor.row(DEVICE_MOON, "Codec")).as_deref(),
            Some("Not applied: \"vp9\" is not one of auto, h264, h265, av1")
        );
        let pending = rows(
            &snapshot(),
            adding(GAME_MOON, vec![runner_key("video.codec")]),
        );
        assert_eq!(
            source(&pending.row(GAME_MOON, "Codec")).as_deref(),
            Some("h264 from Moonlight (default)")
        );
    }

    #[test]
    fn names_a_plugin_default_by_the_title_of_the_contribution_in_its_package_and_by_the_package_id_only_when_none_matches(
    ) {
        let editor = rows(
            &snapshot(),
            adding(
                "game:wario/any",
                vec![
                    PendingSetting::Modifier {
                        id: GAMESCOPE.into(),
                        key: "video.fps-limit".into(),
                    },
                    PendingSetting::Modifier {
                        id: GAMESCOPE.into(),
                        key: "video.nested.fps".into(),
                    },
                ],
            ),
        );
        assert_eq!(
            source(&editor.row("game:wario/any", "FPS limit")).as_deref(),
            Some("60 from Gamescope (default)")
        );
        assert_eq!(
            source(&editor.row("game:wario/any", "Nested FPS")).as_deref(),
            Some("30 from @korri:unknown (default)")
        );
    }
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
