//! Port of `clients/portal/src/surface/launch-settings-model.ts`: launch
//! settings as settings rows. korrid's snapshot in, surface places out.
//! Pure. The portal read the snapshot as JSON; this port does the same over
//! `serde_json::Value`, so each rule reads as it did there.

use std::collections::{BTreeMap, HashMap};

use korri_surface_treaty::{
    SurfaceLaunchSettingsLayer, SurfaceLaunchSettingsPlace, SurfaceLaunchSettingsScope,
    SurfaceLaunchSettingsScopeKind, SurfaceSettingChoice, SurfaceSettingConfirmation,
    SurfaceSettingGroup, SurfaceSettingInteraction, SurfaceSettingItem, SurfaceSettingScope,
};
use serde_json::{json, Map, Value};

/// Choice value for "no value on this place".
pub const INHERIT: &str = "inherit";
/// The pick's value for "No profile".
pub const NO_PROFILE: &str = "";
/// Choice value for a Cleared setting: the plugin's default holds here.
pub const DEFAULT: &str = "default";
const NEXT_LAUNCH: &str = "Applies at next launch.";
const MAX_PROFILE_TITLE: u32 = 40;

pub mod launch_id {
    pub fn device(setting: &str) -> String {
        format!("launch-device:{setting}")
    }
    pub fn set(place_id: &str, path: &str) -> String {
        format!("launch:set:{place_id}#{path}")
    }
    pub fn device_set(place_id: &str, path: &str) -> String {
        format!("launch-device:set:{place_id}#{path}")
    }
    pub const PICK: &str = "launch:profile:pick";
    pub const CREATE: &str = "launch:profile:new";
    pub fn rename(profile_id: &str) -> String {
        format!("launch:profile:rename:{profile_id}")
    }
    pub fn remove(profile_id: &str) -> String {
        format!("launch:profile:remove:{profile_id}")
    }
    pub const RELOAD: &str = "launch:reload";
    pub const DEVICE_RETRY: &str = "launch-device:retry";
    pub const RUNNER: &str = "launch:runner";
    pub const RETRY: &str = "launch:retry";
    pub const CANCEL: &str = "launch:cancel";
}

mod kind {
    pub const RUNNER: &str = "runner";
    pub const CPU: &str = "cpu";
    pub const GPU: &str = "gpu";
    pub const MODIFIERS: &str = "modifiers";
    pub const SETTINGS: &str = "settings";
    pub const SERVICES: &str = "services";
}

// --- JSON helpers -----------------------------------------------------------

fn record(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object()
}
fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}
fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}
fn tag(value: &Value) -> &str {
    str_of(value, "_tag").unwrap_or("")
}
fn strings(value: &Value) -> Vec<String> {
    value.as_array().map_or(Vec::new(), |items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .map(Into::into)
            .collect()
    })
}

/// JavaScript `String(number)`.
fn number_text(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e21 {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

/// JavaScript `Number(text)` for the texts a surface sends.
fn js_number(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some(0.0);
    }
    match trimmed {
        "Infinity" | "+Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        _ => trimmed.parse::<f64>().ok().filter(|_| {
            !trimmed.contains(|c: char| c.is_ascii_alphabetic() && c != 'e' && c != 'E')
        }),
    }
}

/// A JSON value as one line of display text.
pub fn value_text(value: &Value) -> String {
    match value {
        Value::Null => "Unset".into(),
        Value::Bool(true) => "On".into(),
        Value::Bool(false) => "Off".into(),
        Value::String(text) if text.is_empty() => "(empty)".into(),
        Value::String(text) => text.clone(),
        Value::Number(number) => number_text(number.as_f64().unwrap_or(0.0)),
        Value::Array(items) => items.iter().map(value_text).collect::<Vec<_>>().join(" "),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn choice_value(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// A canonical string for a JSON object's entries, sorted.
fn sorted_entries(value: &Value) -> String {
    match value.as_object() {
        None => String::new(),
        Some(map) => {
            let sorted: BTreeMap<&String, &Value> = map.iter().collect();
            serde_json::to_string(&sorted).unwrap_or_default()
        }
    }
}

fn editable_path_key(path: &Value) -> String {
    format!(
        "{}\u{0}{}",
        tag(path),
        sorted_entries(field(path, "payload"))
    )
}

/// A resolved value's path as one comparable string.
pub fn value_path_key(path: &Value) -> String {
    let payload = if tag(path) == "Editable" {
        editable_path_key(field(path, "payload"))
    } else {
        sorted_entries(field(path, "payload"))
    };
    format!("{}\u{0}{payload}", tag(path))
}

fn editable(path: &Value) -> Value {
    json!({ "_tag": "Editable", "payload": path })
}

fn bare_path(tag: &str) -> Value {
    json!({ "_tag": tag })
}

fn limit_path(key: &str) -> Value {
    bare_path(match key {
        "cpu.min" => "CpuMin",
        "cpu.max" => "CpuMax",
        "gpu.min" => "GpuMin",
        "gpu.max" => "GpuMax",
        _ => "CpuGovernor",
    })
}

// --- places -----------------------------------------------------------------

pub fn layer_key(layer: &Value) -> String {
    match tag(layer) {
        "Device" => "device".into(),
        "System" => format!("system:{}", str_of(layer, "id").unwrap_or("")),
        "Game" => format!("game:{}", str_of(layer, "id").unwrap_or("")),
        "Release" => format!(
            "release:{}",
            str_of(field(layer, "id"), "releaseId").unwrap_or("")
        ),
        "Profile" => format!(
            "profile:{}",
            str_of(field(layer, "id"), "profileId").unwrap_or("")
        ),
        _ => "launch".into(),
    }
}

fn scope_key(scope: &Value) -> String {
    match tag(scope) {
        "Any" => "any".into(),
        "Family" => format!("family:{}", str_of(scope, "id").unwrap_or("")),
        _ => format!("runner:{}", str_of(scope, "id").unwrap_or("")),
    }
}

/// A stable place id, unique within one snapshot.
pub fn place_id_of(place: &Value) -> String {
    format!(
        "{}/{}",
        layer_key(field(place, "layer")),
        scope_key(field(place, "scope"))
    )
}

fn same_place(a: &Value, b: &Value) -> bool {
    place_id_of(a) == place_id_of(b)
}

/// Device and System live in device.yaml; the rest in the library.
pub fn revision_of(layer: &Value) -> &'static str {
    if matches!(tag(layer), "Device" | "System") {
        "device"
    } else {
        "games"
    }
}

fn file_of(layer: &Value) -> &'static str {
    if revision_of(layer) == "device" {
        "device.yaml"
    } else {
        "library.yaml"
    }
}

fn layer_rank(layer: &Value) -> i64 {
    match tag(layer) {
        "Device" => 0,
        "System" => 1,
        "Game" => 2,
        "Release" => 3,
        "Profile" => 4,
        _ => 5,
    }
}
fn scope_rank(scope: &Value) -> i64 {
    match tag(scope) {
        "Any" => 0,
        "Family" => 1,
        _ => 2,
    }
}

/// Each profile's title as the editor shows it ("Quiet", "Quiet (2)").
pub fn profile_titles(profiles: &[Value]) -> HashMap<String, String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    profiles
        .iter()
        .map(|profile| {
            let title = str_of(profile, "title").unwrap_or("").to_owned();
            let count = seen.entry(title.clone()).or_default();
            *count += 1;
            let shown = if *count == 1 {
                title
            } else {
                format!("{title} ({count})")
            };
            (str_of(profile, "id").unwrap_or("").to_owned(), shown)
        })
        .collect()
}

fn runner_titles(runners: &[Value]) -> HashMap<String, String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for runner in runners {
        *counts
            .entry(str_of(runner, "title").unwrap_or(""))
            .or_default() += 1;
    }
    runners
        .iter()
        .map(|runner| {
            let title = str_of(runner, "title").unwrap_or("");
            let id = str_of(runner, "id").unwrap_or("");
            (
                id.to_owned(),
                if counts[title] > 1 {
                    format!("{title} ({id})")
                } else {
                    title.to_owned()
                },
            )
        })
        .collect()
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The places the editor shows, most general first: by layer, then scope.
pub fn ordered_places(snapshot: &Value) -> Vec<Value> {
    let runners = array(snapshot, "runners");
    let runner_rank: HashMap<&str, i64> = runners
        .iter()
        .enumerate()
        .map(|(index, runner)| (str_of(runner, "id").unwrap_or(""), index as i64))
        .collect();
    let mut family_rank: HashMap<&str, i64> = HashMap::new();
    for (index, runner) in runners.iter().enumerate() {
        if let Some(family) = str_of(runner, "family") {
            family_rank.entry(family).or_insert(index as i64);
        }
    }
    let profile_rank: HashMap<&str, i64> = array(snapshot, "profiles")
        .iter()
        .enumerate()
        .map(|(index, profile)| (str_of(profile, "id").unwrap_or(""), index as i64))
        .collect();
    let rank = |place: &Value| -> [i64; 4] {
        let layer = field(place, "layer");
        let scope = field(place, "scope");
        [
            layer_rank(layer),
            if tag(layer) == "Profile" {
                *profile_rank
                    .get(str_of(field(layer, "id"), "profileId").unwrap_or(""))
                    .unwrap_or(&0)
            } else {
                0
            },
            scope_rank(scope),
            match tag(scope) {
                "Runner" => *runner_rank
                    .get(str_of(scope, "id").unwrap_or(""))
                    .unwrap_or(&0),
                "Family" => *family_rank
                    .get(str_of(scope, "id").unwrap_or(""))
                    .unwrap_or(&0),
                _ => 0,
            },
        ]
    };
    let mut all: Vec<(usize, &Value)> = array(snapshot, "places")
        .iter()
        .filter(|place| tag(field(field(place, "place"), "layer")) != "Launch")
        .enumerate()
        .collect();
    all.sort_by(|(a_index, a), (b_index, b)| {
        rank(field(a, "place"))
            .cmp(&rank(field(b, "place")))
            .then(a_index.cmp(b_index))
    });
    all.into_iter().map(|(_, place)| place.clone()).collect()
}

// --- clock steps ------------------------------------------------------------

#[derive(Clone)]
pub struct Steps {
    pub choices: Vec<SurfaceSettingChoice>,
    pub guessed: bool,
    unavailable: bool,
    by_percent: Option<BTreeMap<i64, f64>>,
    units_per_ghz: f64,
}

impl Steps {
    fn label_of(&self, percent: f64) -> String {
        let text = number_text(percent);
        match &self.by_percent {
            Some(map) if percent.fract() == 0.0 => match map.get(&(percent as i64)) {
                Some(step) => format!("{text} % ({:.2} GHz)", step / self.units_per_ghz),
                None => format!("{text} %"),
            },
            _ => format!("{text} %"),
        }
    }
}

fn guessed_steps() -> Steps {
    Steps {
        guessed: true,
        unavailable: false,
        by_percent: None,
        units_per_ghz: 1.0,
        choices: (1..=10)
            .map(|step| step * 10)
            .map(|percent| SurfaceSettingChoice {
                value: percent.to_string(),
                label: format!("{percent} %"),
            })
            .collect(),
    }
}

/// Real steps, by the launch helper's rule (ADR 0020).
fn clock_steps(top: f64, steps: &[f64], units_per_ghz: f64) -> Steps {
    let mut sorted = steps.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut by_percent: BTreeMap<i64, f64> = BTreeMap::new();
    let mut order = Vec::new();
    for step in sorted {
        // JavaScript Math.round rounds halves up.
        let percent = ((step * 100.0) / top + 0.5).floor() as i64;
        if percent > 0 && percent <= 100 && !by_percent.contains_key(&percent) {
            by_percent.insert(percent, step);
            order.push(percent);
        }
    }
    let mut out = Steps {
        choices: Vec::new(),
        guessed: false,
        unavailable: false,
        by_percent: Some(by_percent),
        units_per_ghz,
    };
    out.choices = order
        .iter()
        .map(|percent| SurfaceSettingChoice {
            value: percent.to_string(),
            label: out.label_of(*percent as f64),
        })
        .collect();
    out
}

fn numbers(value: &Value, key: &str) -> Vec<f64> {
    array(value, key).iter().filter_map(Value::as_f64).collect()
}

pub fn cpu_steps(facts: Option<&Value>) -> Steps {
    let mut policies: Vec<&Value> = facts
        .map(|facts| array(facts, "cpuPolicies").iter().collect())
        .unwrap_or_default();
    policies.retain(|policy| {
        field(policy, "cpuinfoMaxFreq").as_f64().unwrap_or(0.0) > 0.0
            && !numbers(policy, "availableFrequencies").is_empty()
    });
    policies.sort_by(|a, b| {
        field(b, "cpuinfoMaxFreq")
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&field(a, "cpuinfoMaxFreq").as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    match policies.first() {
        None => guessed_steps(),
        Some(policy) => clock_steps(
            field(policy, "cpuinfoMaxFreq").as_f64().unwrap_or(1.0),
            &numbers(policy, "availableFrequencies"),
            1e6,
        ),
    }
}

pub fn gpu_steps(facts: Option<&Value>) -> Steps {
    let Some(facts) = facts else {
        return guessed_steps();
    };
    let top = |device: &Value| {
        numbers(device, "availableFrequencies")
            .into_iter()
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let mut devices: Vec<&Value> = array(facts, "gpuDevfreq")
        .iter()
        .filter(|device| !numbers(device, "availableFrequencies").is_empty())
        .collect();
    devices.sort_by(|a, b| {
        top(b)
            .partial_cmp(&top(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    match devices.first() {
        None => Steps {
            guessed: false,
            unavailable: true,
            choices: Vec::new(),
            ..guessed_steps()
        },
        Some(device) => clock_steps(top(device), &numbers(device, "availableFrequencies"), 1e9),
    }
}

pub fn governors(facts: Option<&Value>) -> Vec<String> {
    let policies = facts
        .map(|facts| array(facts, "cpuPolicies"))
        .unwrap_or(&[]);
    let Some((first, rest)) = policies.split_first() else {
        return Vec::new();
    };
    strings(field(first, "availableGovernors"))
        .into_iter()
        .filter(|governor| {
            rest.iter()
                .all(|policy| strings(field(policy, "availableGovernors")).contains(governor))
        })
        .collect()
}

// --- writes -----------------------------------------------------------------

/// What a changed row writes after `encode`: no value takes this place's
/// value away (Inherit); `clear` writes a Cleared setting (Default).
#[derive(Clone, Debug, PartialEq)]
pub struct LaunchEncoded {
    pub value: Option<Value>,
    pub clear: bool,
}

impl LaunchEncoded {
    fn inherit() -> Self {
        Self {
            value: None,
            clear: false,
        }
    }
    fn value(value: Value) -> Self {
        Self {
            value: Some(value),
            clear: false,
        }
    }
    fn cleared() -> Self {
        Self {
            value: None,
            clear: true,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Encoder {
    /// Inherit or a whole percent 0..=100.
    Percent,
    /// Inherit or the text itself (governor, runner).
    TextOrInherit,
    /// Inherit or true/false (modifier enabled).
    Toggle,
    /// Inherit, Default, or one of the JSON values.
    Enum(Vec<Value>),
    /// Inherit, Default, true or false.
    Boolean,
    /// A number within the leaf's limits; empty text inherits.
    Number {
        integer: bool,
        minimum: Option<f64>,
        maximum: Option<f64>,
    },
    /// The text is the value, an empty one too.
    Text,
}

impl Encoder {
    pub fn encode(&self, text: &str) -> Option<LaunchEncoded> {
        let keep = |text: &str| match text {
            INHERIT => Some(LaunchEncoded::inherit()),
            DEFAULT => Some(LaunchEncoded::cleared()),
            _ => None,
        };
        match self {
            Encoder::Percent => {
                if text == INHERIT {
                    return Some(LaunchEncoded::inherit());
                }
                let percent = js_number(text)?;
                (percent.fract() == 0.0 && (0.0..=100.0).contains(&percent))
                    .then(|| LaunchEncoded::value(json!(percent as i64)))
            }
            Encoder::TextOrInherit => Some(if text == INHERIT {
                LaunchEncoded::inherit()
            } else {
                LaunchEncoded::value(json!(text))
            }),
            Encoder::Toggle => match text {
                INHERIT => Some(LaunchEncoded::inherit()),
                "true" => Some(LaunchEncoded::value(json!(true))),
                "false" => Some(LaunchEncoded::value(json!(false))),
                _ => None,
            },
            Encoder::Enum(values) => keep(text).or_else(|| {
                values
                    .iter()
                    .find(|value| choice_value(value) == text)
                    .map(|value| LaunchEncoded::value(value.clone()))
            }),
            Encoder::Boolean => keep(text).or(match text {
                "true" => Some(LaunchEncoded::value(json!(true))),
                "false" => Some(LaunchEncoded::value(json!(false))),
                _ => None,
            }),
            Encoder::Number {
                integer,
                minimum,
                maximum,
            } => {
                if text.trim().is_empty() {
                    return Some(LaunchEncoded::inherit());
                }
                let number = js_number(text).filter(|number| number.is_finite())?;
                if *integer && number.fract() != 0.0 {
                    return None;
                }
                if minimum.is_some_and(|minimum| number < minimum)
                    || maximum.is_some_and(|maximum| number > maximum)
                {
                    return None;
                }
                Some(LaunchEncoded::value(
                    if number.fract() == 0.0 && number.abs() < 9e15 {
                        json!(number as i64)
                    } else {
                        json!(number)
                    },
                ))
            }
            Encoder::Text => Some(LaunchEncoded::value(json!(text))),
        }
    }
}

/// What a changed row writes: the place and the typed `set` path.
#[derive(Clone, Debug)]
pub struct LaunchWrite {
    pub place: Value,
    pub path: Value,
    pub encoder: Encoder,
}

/// A setting added with Add setting and not yet given a value.
#[derive(Clone, Debug, PartialEq)]
pub enum PendingSetting {
    Runner { key: String },
    Modifier { id: String, key: String },
    Service { id: String, key: String },
}

#[derive(Clone, Debug)]
pub enum PendingKind {
    Runner,
    Modifier(String),
    Service(String),
}

impl PendingKind {
    pub fn pending_of(&self, key: &str) -> PendingSetting {
        match self {
            Self::Runner => PendingSetting::Runner { key: key.into() },
            Self::Modifier(id) => PendingSetting::Modifier {
                id: id.clone(),
                key: key.into(),
            },
            Self::Service(id) => PendingSetting::Service {
                id: id.clone(),
                key: key.into(),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct LaunchAdd {
    pub place_id: String,
    pub kind: PendingKind,
}

#[derive(Clone, Debug, Default)]
pub struct Rows {
    pub writes: HashMap<String, LaunchWrite>,
    pub adds: HashMap<String, LaunchAdd>,
    pub pending: HashMap<String, (String, PendingSetting)>,
    pub clears: HashMap<String, String>,
    pub defaults: HashMap<String, String>,
}

pub struct LaunchEditor {
    pub places: Vec<SurfaceLaunchSettingsPlace>,
    pub notes: Vec<String>,
    pub rows: Rows,
}

pub struct DeviceLaunch {
    pub groups: Vec<SurfaceSettingGroup>,
    pub rows: Rows,
}

// --- context ----------------------------------------------------------------

struct Context<'a> {
    snapshot: &'a Value,
    device_page: bool,
    places: Vec<Value>,
    cpu: Steps,
    gpu: Steps,
    governors: Vec<String>,
    runner_titles: HashMap<String, String>,
    selected: Option<&'a Value>,
    by_path: HashMap<String, Value>,
    streams: HashMap<String, HashMap<String, Value>>,
    skipped: HashMap<String, String>,
}

impl<'a> Context<'a> {
    fn new(snapshot: &'a Value, game_id: Option<&str>) -> Self {
        let selected = array(snapshot, "runners").iter().find(|runner| {
            field(runner, "selected") == &json!(true) && field(runner, "choosable") == &json!(true)
        });
        let values_of = |values: &Value| -> HashMap<String, Value> {
            record(values).map_or(HashMap::new(), |map| {
                map.values()
                    .map(|value| (value_path_key(field(value, "path")), value.clone()))
                    .collect()
            })
        };
        let selected_id = selected
            .and_then(|runner| str_of(runner, "id"))
            .unwrap_or("")
            .to_owned();
        let skipped = array(snapshot, "notices")
            .iter()
            .filter(|notice| tag(notice) == "SettingSkipped")
            .map(|notice| {
                let payload = field(notice, "payload");
                let path = field(payload, "path");
                let runner = if tag(path) == "Setting" {
                    str_of(payload, "runnerId").map_or(selected_id.clone(), Into::into)
                } else {
                    String::new()
                };
                (
                    format!("{runner}\u{0}{}", editable_path_key(path)),
                    str_of(payload, "message").unwrap_or("").to_owned(),
                )
            })
            .collect();
        Self {
            snapshot,
            device_page: game_id.is_none(),
            places: ordered_places(snapshot),
            cpu: cpu_steps(snapshot.get("deviceFacts")),
            gpu: gpu_steps(snapshot.get("deviceFacts")),
            governors: governors(snapshot.get("deviceFacts")),
            runner_titles: runner_titles(array(snapshot, "runners")),
            selected,
            by_path: values_of(field(field(snapshot, "resolved"), "values")),
            streams: array(snapshot, "runnerResolved")
                .iter()
                .map(|entry| {
                    (
                        str_of(entry, "runnerId").unwrap_or("").to_owned(),
                        values_of(field(entry, "values")),
                    )
                })
                .collect(),
            skipped,
        }
    }
    fn selected_id(&self) -> Option<&str> {
        self.selected.and_then(|runner| str_of(runner, "id"))
    }
    fn resolved(&self, path: &Value) -> Option<&Value> {
        self.by_path.get(&value_path_key(&editable(path)))
    }
    fn list(&self, kind: &str) -> Option<&Value> {
        self.by_path.get(&value_path_key(&bare_path(kind)))
    }
    fn skip(&self, path: &Value, runner_id: Option<&str>) -> Option<&String> {
        let runner = if tag(path) == "Setting" {
            runner_id.or(self.selected_id()).unwrap_or("")
        } else {
            ""
        };
        self.skipped
            .get(&format!("{runner}\u{0}{}", editable_path_key(path)))
    }
    fn stream_resolved(&self, runner_id: &str, path: &Value) -> Option<&Value> {
        self.streams
            .get(runner_id)
            .and_then(|values| values.get(&value_path_key(&editable(path))))
    }
    fn runner_title(&self, runner: &Value) -> String {
        let id = str_of(runner, "id").unwrap_or("");
        self.runner_titles
            .get(id)
            .cloned()
            .unwrap_or_else(|| str_of(runner, "title").unwrap_or("").to_owned())
    }
}

fn source_title(context: &Context, source: &Value, prefer: Option<&str>) -> String {
    match tag(source) {
        "Place" => {
            let wanted = field(source, "payload");
            match context
                .places
                .iter()
                .find(|candidate| same_place(field(candidate, "place"), wanted))
            {
                None => layer_title(context, field(wanted, "layer")),
                Some(place) => place_title(context, place),
            }
        }
        "PluginDefault" => format!(
            "{} (default)",
            plugin_title(
                context,
                str_of(field(source, "payload"), "pluginId").unwrap_or(""),
                prefer
            )
        ),
        _ => "Device (default)".into(),
    }
}

/// A plugin's title by its id: the contribution's, the one a row is about
/// first, then the selected runner, then any. The id stands only when none
/// matches.
fn plugin_title(context: &Context, plugin_id: &str, prefer: Option<&str>) -> String {
    let mut titled: Vec<(String, String)> = array(context.snapshot, "runners")
        .iter()
        .map(|runner| {
            (
                str_of(runner, "id").unwrap_or("").to_owned(),
                context.runner_title(runner),
            )
        })
        .collect();
    for plugin in array(context.snapshot, "modifiers")
        .iter()
        .chain(array(context.snapshot, "services"))
    {
        titled.push((
            str_of(plugin, "id").unwrap_or("").to_owned(),
            str_of(plugin, "title").unwrap_or("").to_owned(),
        ));
    }
    let of_package = |id: &str| id == plugin_id || id.starts_with(&format!("{plugin_id}/"));
    let exact = titled.iter().find(|(id, _)| id == plugin_id);
    let preferred = [prefer, context.selected_id()]
        .into_iter()
        .flatten()
        .filter(|id| of_package(id))
        .find_map(|id| titled.iter().find(|(candidate, _)| candidate == id));
    exact
        .or(preferred)
        .or_else(|| titled.iter().find(|(id, _)| of_package(id)))
        .map_or(plugin_id.to_owned(), |(_, title)| title.clone())
}

fn stream_runner(context: &Context, scope: &Value) -> bool {
    tag(scope) == "Runner"
        && array(context.snapshot, "runners").iter().any(|runner| {
            str_of(runner, "id") == str_of(scope, "id")
                && field(runner, "choosable") != &json!(true)
        })
}

fn stream_line(context: &Context) -> &'static str {
    if context.device_page {
        "Applies when this device streams a game"
    } else {
        "Applies when this device streams the game"
    }
}

fn applies(context: &Context, scope: &Value) -> bool {
    match tag(scope) {
        "Any" => true,
        "Family" => {
            context.selected.and_then(|runner| str_of(runner, "family")) == str_of(scope, "id")
        }
        _ => context.selected_id() == str_of(scope, "id"),
    }
}

fn not_in_force_scope(context: &Context, scope: &Value) -> Option<String> {
    if stream_runner(context, scope) {
        return Some(stream_line(context).into());
    }
    if applies(context, scope) {
        return None;
    }
    Some(match context.selected {
        None => "Applies once a runner is chosen".into(),
        Some(runner) => format!(
            "Not in force: {} runs this game",
            context.runner_title(runner)
        ),
    })
}

/// Where the value in force comes from, said from one place's row.
fn source_text(
    context: &Context,
    here: &Value,
    resolved: Option<&Value>,
    show: &dyn Fn(&Value) -> String,
) -> Option<String> {
    if let Some(waiting) = not_in_force_scope(context, field(here, "scope")) {
        return Some(waiting);
    }
    let resolved = resolved?;
    let source = resolved.get("source")?;
    let title = source_title(context, source, None);
    let value = show(field(resolved, "value"));
    if field(resolved, "adjusted") == &json!(true) {
        return Some(format!("{value} from {title}, adjusted"));
    }
    Some(
        if tag(source) == "Place" && same_place(field(source, "payload"), here) {
            format!("from {title}")
        } else {
            format!("{value} from {title}")
        },
    )
}

fn layer_title(context: &Context, layer: &Value) -> String {
    match tag(layer) {
        "Device" => "Device".into(),
        "System" => format!("System {}", str_of(layer, "id").unwrap_or("")),
        "Game" => "Game".into(),
        "Release" => format!(
            "Release {}",
            str_of(field(layer, "id"), "releaseId").unwrap_or("")
        ),
        "Profile" => {
            let id = str_of(field(layer, "id"), "profileId").unwrap_or("");
            format!(
                "Profile {}",
                profile_titles(array(context.snapshot, "profiles"))
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| id.into())
            )
        }
        _ => "This launch".into(),
    }
}

/// A place's title: korrid's, except a profile's.
fn place_title(context: &Context, entry: &Value) -> String {
    let place = field(entry, "place");
    let layer = field(place, "layer");
    if tag(layer) != "Profile" {
        return str_of(entry, "title").unwrap_or("").to_owned();
    }
    let title = layer_title(context, layer);
    let scope = field(place, "scope");
    if tag(scope) == "Any" {
        title
    } else {
        format!("{title} for {}", scope_label(context, scope))
    }
}

fn scope_label(context: &Context, scope: &Value) -> String {
    match tag(scope) {
        "Any" => "Any runner".into(),
        "Family" => format!("Family {}", str_of(scope, "id").unwrap_or("")),
        _ => {
            let id = str_of(scope, "id").unwrap_or("");
            context
                .runner_titles
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.into())
        }
    }
}

// --- rows -------------------------------------------------------------------

fn choice(value: impl Into<String>, label: impl Into<String>) -> SurfaceSettingChoice {
    SurfaceSettingChoice {
        value: value.into(),
        label: label.into(),
    }
}

fn choice_interaction(
    choices: Vec<SurfaceSettingChoice>,
    selected: impl Into<String>,
) -> SurfaceSettingInteraction {
    SurfaceSettingInteraction::Choice {
        choices,
        selected: selected.into(),
    }
}

fn action_interaction(
    action_id: String,
    destructive: Option<bool>,
    confirmation: Option<(String, String, String)>,
) -> SurfaceSettingInteraction {
    SurfaceSettingInteraction::Action {
        action_id,
        destructive,
        confirmation: confirmation.map(|(title, message, confirm_label)| {
            SurfaceSettingConfirmation {
                title,
                message,
                confirm_label,
            }
        }),
    }
}

fn percent_of(value: &Value) -> Option<f64> {
    value.as_f64().filter(|number| number.is_finite())
}

fn percent_row(
    context: &Context,
    rows: &mut Rows,
    id: String,
    place: &Value,
    key: &str,
    own: Option<f64>,
    describe: &str,
) -> SurfaceSettingItem {
    let (kind, bound) = key.split_once('.').unwrap_or((key, ""));
    let steps = if kind == "cpu" {
        &context.cpu
    } else {
        &context.gpu
    };
    let mut offered: Vec<SurfaceSettingChoice> = if bound == "min" {
        vec![choice("0", "0 % (no floor)")]
    } else {
        Vec::new()
    };
    offered.extend(steps.choices.iter().cloned());
    let choices = match own {
        Some(own)
            if !offered
                .iter()
                .any(|choice| choice.value == number_text(own)) =>
        {
            let mut all = offered.clone();
            all.push(choice(number_text(own), steps.label_of(own)));
            all.sort_by(|a, b| {
                js_number(&a.value)
                    .unwrap_or(0.0)
                    .partial_cmp(&js_number(&b.value).unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            all
        }
        _ => offered,
    };
    rows.writes.insert(
        id.clone(),
        LaunchWrite {
            place: place.clone(),
            path: limit_path(key),
            encoder: Encoder::Percent,
        },
    );
    let source = source_text(
        context,
        place,
        context.resolved(&limit_path(key)),
        &|value| match percent_of(value) {
            None => value_text(value),
            Some(percent) => steps.label_of(percent),
        },
    );
    let selected = own.map_or(INHERIT.to_owned(), number_text);
    let mut all_choices = vec![choice(INHERIT, "Inherit")];
    all_choices.extend(choices.iter().cloned());
    SurfaceSettingItem {
        value: Some(match own {
            None => "Inherit".into(),
            Some(own) => choices
                .iter()
                .find(|choice| choice.value == number_text(own))
                .map_or(steps.label_of(own), |choice| choice.label.clone()),
        }),
        description: Some(if steps.guessed {
            format!("{describe} Clock steps unknown.")
        } else {
            describe.to_owned()
        }),
        source,
        scope: Some(SurfaceSettingScope::Device),
        interaction: Some(choice_interaction(all_choices, selected)),
        ..SurfaceSettingItem::new(id, format!("{} {bound}", kind.to_uppercase()))
    }
}

fn governor_row(
    context: &Context,
    rows: &mut Rows,
    id: String,
    place: &Value,
    own: Option<String>,
) -> SurfaceSettingItem {
    let known = &context.governors;
    let mut offered = known.clone();
    if let Some(own) = own.as_ref().filter(|own| !known.contains(own)) {
        offered.push(own.clone());
    }
    let resolved = context.resolved(&limit_path("cpu.governor"));
    let fallback = context
        .snapshot
        .get("deviceFacts")
        .and_then(|facts| str_of(field(facts, "defaults"), "governor"));
    rows.writes.insert(
        id.clone(),
        LaunchWrite {
            place: place.clone(),
            path: limit_path("cpu.governor"),
            encoder: Encoder::TextOrInherit,
        },
    );
    let source = if own.is_some() {
        "from Device".to_owned()
    } else if resolved.is_some() {
        source_text(context, place, resolved, &value_text)
            .unwrap_or_else(|| "from Device (default)".into())
    } else {
        fallback.map_or("from Device (default)".into(), |fallback| {
            format!("{fallback} from Device (default)")
        })
    };
    let mut choices = vec![choice(INHERIT, "Inherit")];
    choices.extend(
        offered
            .iter()
            .map(|governor| choice(governor.clone(), governor.clone())),
    );
    SurfaceSettingItem {
        value: Some(own.clone().unwrap_or_else(|| "Inherit".into())),
        description: Some(if known.is_empty() {
            format!("{NEXT_LAUNCH} Governors unknown.")
        } else {
            NEXT_LAUNCH.into()
        }),
        source: Some(source),
        scope: Some(SurfaceSettingScope::Device),
        interaction: Some(choice_interaction(
            choices,
            own.unwrap_or_else(|| INHERIT.into()),
        )),
        ..SurfaceSettingItem::new(id, "CPU governor")
    }
}

/// CPU and GPU rows for one place.
fn limit_groups(
    context: &Context,
    rows: &mut Rows,
    place: &Value,
    body: &Value,
    kinds: &[String],
    ids: &dyn Fn(&str) -> String,
) -> Vec<SurfaceSettingGroup> {
    let cpu = field(body, "cpu");
    let gpu = field(body, "gpu");
    let host_wide = tag(field(place, "layer")) == "Device" && tag(field(place, "scope")) == "Any";
    let describe = if host_wide {
        format!("{NEXT_LAUNCH} Sets the device's clock limits.")
    } else {
        format!("{NEXT_LAUNCH} Limits the game's session.")
    };
    let mut groups = Vec::new();
    if kinds.iter().any(|kind| kind == kind::CPU) {
        let mut items = vec![
            percent_row(
                context,
                rows,
                ids("cpu.min"),
                place,
                "cpu.min",
                percent_of(field(cpu, "min")),
                &describe,
            ),
            percent_row(
                context,
                rows,
                ids("cpu.max"),
                place,
                "cpu.max",
                percent_of(field(cpu, "max")),
                &describe,
            ),
        ];
        if host_wide {
            items.push(governor_row(
                context,
                rows,
                ids("cpu.governor"),
                place,
                str_of(cpu, "governor").map(Into::into),
            ));
        }
        groups.push(SurfaceSettingGroup {
            title: "CPU".into(),
            items,
        });
    }
    if kinds.iter().any(|kind| kind == kind::GPU) {
        let items = if context.gpu.unavailable {
            vec![SurfaceSettingItem::new(
                format!("{}:unavailable", ids("gpu.min")),
                "GPU limits aren't available on this device",
            )]
        } else {
            vec![
                percent_row(
                    context,
                    rows,
                    ids("gpu.min"),
                    place,
                    "gpu.min",
                    percent_of(field(gpu, "min")),
                    &describe,
                ),
                percent_row(
                    context,
                    rows,
                    ids("gpu.max"),
                    place,
                    "gpu.max",
                    percent_of(field(gpu, "max")),
                    &describe,
                ),
            ]
        };
        groups.push(SurfaceSettingGroup {
            title: "GPU".into(),
            items,
        });
    }
    groups
}

fn runner_group(
    context: &Context,
    rows: &mut Rows,
    place_id: &str,
    place: &Value,
    body: &Value,
) -> SurfaceSettingGroup {
    let own = str_of(body, "runner").map(str::to_owned);
    let id = launch_id::set(place_id, "runner");
    let installed: Vec<SurfaceSettingChoice> = array(context.snapshot, "runners")
        .iter()
        .filter(|runner| field(runner, "choosable") == &json!(true))
        .map(|runner| {
            choice(
                str_of(runner, "id").unwrap_or(""),
                context.runner_title(runner),
            )
        })
        .collect();
    let missing = own
        .as_ref()
        .is_some_and(|own| !installed.iter().any(|choice| &choice.value == own));
    let streamer = own.as_ref().and_then(|own| {
        array(context.snapshot, "runners").iter().find(|runner| {
            str_of(runner, "id") == Some(own) && field(runner, "choosable") != &json!(true)
        })
    });
    let mut choices = vec![choice(INHERIT, "Inherit")];
    choices.extend(installed);
    if missing {
        let own = own.clone().unwrap_or_default();
        let label = match streamer {
            None => format!("{own} (not installed)"),
            Some(streamer) => format!("{} (streams only)", context.runner_title(streamer)),
        };
        choices.push(choice(own, label));
    }
    rows.writes.insert(
        id.clone(),
        LaunchWrite {
            place: place.clone(),
            path: bare_path("Runner"),
            encoder: Encoder::TextOrInherit,
        },
    );
    let source = source_text(
        context,
        place,
        context.resolved(&bare_path("Runner")),
        &|value| match value.as_str() {
            Some(id) => context
                .runner_titles
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.into()),
            None => value_text(value),
        },
    );
    let selected = own.clone().unwrap_or_else(|| INHERIT.into());
    SurfaceSettingGroup {
        title: "Runner".into(),
        items: vec![SurfaceSettingItem {
            value: Some(
                choices
                    .iter()
                    .find(|choice| choice.value == selected)
                    .map_or("Inherit".into(), |choice| choice.label.clone()),
            ),
            description: Some(
                if streamer.is_some() {
                    "This runner only views streams; it runs no game. Korri skips it here."
                } else if missing {
                    "This runner isn't installed. Korri skips it and its runner settings here."
                } else {
                    "The most specific place that names an installed runner wins."
                }
                .into(),
            ),
            source,
            scope: Some(SurfaceSettingScope::Shared),
            interaction: Some(choice_interaction(choices, selected)),
            ..SurfaceSettingItem::new(id, "Runner")
        }],
    }
}

// --- plugin settings ----------------------------------------------------------

#[derive(Clone, Debug)]
struct Leaf {
    path: String,
    label: String,
    kind: Option<String>,
    description: Option<String>,
    choices: Option<Vec<Value>>,
    minimum: Option<f64>,
    maximum: Option<f64>,
    default: Option<Value>,
    device: bool,
}

#[derive(Clone, Debug)]
struct Pattern {
    prefix: Vec<String>,
    objects: Vec<String>,
    source: String,
    regex: regex::Regex,
    title: String,
    node: Value,
}

#[derive(Clone, Debug)]
struct Section {
    key: String,
    title: String,
    leaves: Vec<Leaf>,
    patterns: Vec<Pattern>,
}

const ACRONYMS: [&str; 15] = [
    "fps", "hdr", "sdr", "ip", "qp", "fec", "av1", "hevc", "ds4", "ds5", "vk", "cpu", "gpu", "id",
    "url",
];

/// A key as words, when its schema gives no title.
fn humanize(key: &str) -> String {
    let words: Vec<String> = key
        .split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            if ACRONYMS.contains(&word) {
                word.to_uppercase()
            } else {
                word.to_owned()
            }
        })
        .collect();
    let text = words.join(" ");
    let mut chars = text.chars();
    match chars.next() {
        None => key.to_owned(),
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
    }
}

/// A title inside a label: "Max nits" reads "max nits" after "HDR".
fn inner(title: &str) -> String {
    let mut chars = title.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(second))
            if first.is_ascii_uppercase() && second.is_ascii_lowercase() =>
        {
            format!("{}{}", first.to_ascii_lowercase(), &title[1..])
        }
        _ => title.to_owned(),
    }
}

fn leaf_label(objects: &[String], key: &str, title: Option<&str>) -> String {
    let own = title.map_or_else(|| humanize(key), str::to_owned);
    if objects.is_empty() {
        return own;
    }
    if key == "enable"
        && title.is_none_or(|title| {
            title.eq_ignore_ascii_case("enable") || title.eq_ignore_ascii_case("enabled")
        })
    {
        let mut parts = vec![objects[0].clone()];
        parts.extend(objects[1..].iter().map(|object| inner(object)));
        return parts.join(" ");
    }
    let mut named: Vec<String> = objects
        .iter()
        .filter(|object| !own.to_lowercase().contains(&object.to_lowercase()))
        .cloned()
        .collect();
    named.push(own);
    let mut parts = vec![named[0].clone()];
    parts.extend(named[1..].iter().map(|part| inner(part)));
    parts.join(" ")
}

fn type_of(node: &Value) -> Option<String> {
    match field(node, "type") {
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .find(|item| *item != "null")
            .map(Into::into),
        Value::String(kind) => Some(kind.clone()),
        _ => None,
    }
}

fn is_object_node(node: &Value) -> bool {
    type_of(node).as_deref() == Some("object")
        || (node.get("type").is_none()
            && (field(node, "properties").is_object()
                || field(node, "patternProperties").is_object()))
}

fn title_of(node: &Value, key: &str) -> String {
    str_of(node, "title").map_or_else(|| humanize(key), Into::into)
}

fn leaf_of(node: &Value, segments: &[String], objects: &[String]) -> Leaf {
    let key = segments.last().cloned().unwrap_or_default();
    Leaf {
        path: segments.join("."),
        label: leaf_label(objects, &key, str_of(node, "title")),
        kind: type_of(node),
        description: str_of(node, "description").map(Into::into),
        choices: node.get("enum").and_then(Value::as_array).cloned(),
        minimum: field(node, "minimum").as_f64(),
        maximum: field(node, "maximum").as_f64(),
        default: node.as_object().and_then(|map| map.get("default").cloned()),
        device: str_of(node, "x-korri-scope") == Some("device"),
    }
}

fn leaves_of(node: &Value, segments: &[String], objects: &[String]) -> (Vec<Leaf>, Vec<Pattern>) {
    let mut leaves = Vec::new();
    let mut patterns = Vec::new();
    if let Some(properties) = record(field(node, "properties")) {
        for (key, child) in properties {
            if !child.is_object() || key.contains('.') || key.is_empty() {
                continue;
            }
            let mut path = segments.to_vec();
            path.push(key.clone());
            if is_object_node(child) {
                let mut titles = objects.to_vec();
                titles.push(title_of(child, key));
                let (nested_leaves, nested_patterns) = leaves_of(child, &path, &titles);
                leaves.extend(nested_leaves);
                patterns.extend(nested_patterns);
            } else {
                leaves.push(leaf_of(child, &path, objects));
            }
        }
    }
    if let Some(pattern_properties) = record(field(node, "patternProperties")) {
        for (pattern, child) in pattern_properties {
            let Ok(regex) = regex::Regex::new(pattern) else {
                continue;
            };
            if !child.is_object() {
                continue;
            }
            let own = title_of(node, segments.last().map_or("", String::as_str));
            patterns.push(Pattern {
                prefix: segments.to_vec(),
                objects: objects[..objects.len().saturating_sub(1)].to_vec(),
                source: pattern.clone(),
                regex,
                title: str_of(child, "title").map_or(own, Into::into),
                node: child.clone(),
            });
        }
    }
    (leaves, patterns)
}

fn pattern_key_leaves(pattern: &Pattern, key: &str) -> Vec<Leaf> {
    let mut segments = pattern.prefix.clone();
    segments.push(key.into());
    let title = format!("{} {key}", pattern.title);
    if is_object_node(&pattern.node) {
        let mut objects = pattern.objects.clone();
        objects.push(title);
        return leaves_of(&pattern.node, &segments, &objects).0;
    }
    vec![Leaf {
        label: title,
        ..leaf_of(&pattern.node, &segments, &pattern.objects)
    }]
}

fn pattern_keys<'a>(pattern: &Pattern, paths: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut keys = Vec::new();
    for path in paths {
        let segments: Vec<&str> = path.split('.').collect();
        if segments.len() <= pattern.prefix.len()
            || pattern
                .prefix
                .iter()
                .enumerate()
                .any(|(index, segment)| segments[index] != segment)
        {
            continue;
        }
        let key = segments[pattern.prefix.len()];
        if pattern.regex.is_match(key) && !keys.iter().any(|known| known == key) {
            keys.push(key.to_owned());
        }
    }
    keys
}

fn next_free_key(pattern: &Pattern, used: &[String]) -> Option<String> {
    (1..=999)
        .map(|number: u32| number.to_string())
        .find(|key| !used.contains(key) && pattern.regex.is_match(key))
}

fn section_leaves(section: &Section, paths: &[String]) -> Vec<Leaf> {
    let mut leaves = section.leaves.clone();
    for pattern in &section.patterns {
        let used = pattern_keys(pattern, paths);
        let next = next_free_key(pattern, &used);
        for key in used.iter().chain(next.iter()) {
            leaves.extend(pattern_key_leaves(pattern, key));
        }
    }
    leaves
}

fn leaf_at(sections: &[Section], path: &str) -> Option<Leaf> {
    let wanted = vec![path.to_owned()];
    sections.iter().find_map(|section| {
        section
            .leaves
            .iter()
            .find(|leaf| leaf.path == path)
            .cloned()
            .or_else(|| {
                section
                    .patterns
                    .iter()
                    .flat_map(|pattern| {
                        pattern_keys(pattern, &wanted)
                            .into_iter()
                            .flat_map(|key| pattern_key_leaves(pattern, &key))
                            .collect::<Vec<_>>()
                    })
                    .find(|leaf| leaf.path == path)
            })
    })
}

/// A plugin's nested settings.describe schema as sections of leaves.
fn sections_of(schema: &Value) -> Vec<Section> {
    let mut loose = Vec::new();
    let mut sections = Vec::new();
    if let Some(properties) = record(field(schema, "properties")) {
        for (key, node) in properties {
            if !node.is_object() || key.contains('.') || key.is_empty() {
                continue;
            }
            if is_object_node(node) {
                let (leaves, patterns) = leaves_of(node, std::slice::from_ref(key), &[]);
                sections.push(Section {
                    key: key.clone(),
                    title: title_of(node, key),
                    leaves,
                    patterns,
                });
            } else {
                loose.push(leaf_of(node, std::slice::from_ref(key), &[]));
            }
        }
    }
    if loose.is_empty() {
        sections
    } else {
        let mut all = vec![Section {
            key: String::new(),
            title: String::new(),
            leaves: loose,
            patterns: Vec::new(),
        }];
        all.extend(sections);
        all
    }
}

fn merge_sections(all: Vec<Vec<Section>>) -> Vec<Section> {
    let mut merged: Vec<Section> = Vec::new();
    for sections in all {
        for section in sections {
            let entry = match merged
                .iter_mut()
                .position(|candidate| candidate.key == section.key)
            {
                Some(index) => &mut merged[index],
                None => {
                    merged.push(Section {
                        key: section.key.clone(),
                        title: section.title.clone(),
                        leaves: Vec::new(),
                        patterns: Vec::new(),
                    });
                    merged.last_mut().expect("pushed")
                }
            };
            for leaf in section.leaves {
                if !entry.leaves.iter().any(|known| known.path == leaf.path) {
                    entry.leaves.push(leaf);
                }
            }
            for pattern in section.patterns {
                if !entry
                    .patterns
                    .iter()
                    .any(|known| known.prefix == pattern.prefix && known.source == pattern.source)
                {
                    entry.patterns.push(pattern);
                }
            }
        }
    }
    merged
}

/// A place's own nested settings as leaves by dotted path, in file order.
fn own_leaves(settings: &Value, prefix: &[String]) -> Vec<(String, Value)> {
    let mut leaves = Vec::new();
    if let Some(map) = record(settings) {
        for (key, value) in map {
            if key.contains('.') || key.is_empty() {
                continue;
            }
            let mut path = prefix.to_vec();
            path.push(key.clone());
            if value.is_object() {
                leaves.extend(own_leaves(value, &path));
            } else {
                leaves.push((path.join("."), value.clone()));
            }
        }
    }
    leaves
}

fn section_key_of(path: &str) -> &str {
    path.find('.').map_or("", |at| &path[..at])
}

fn schema_for(context: &Context, scope: &Value) -> Vec<Section> {
    if tag(scope) == "Any" {
        return Vec::new();
    }
    let runners = array(context.snapshot, "runners").iter().filter(|runner| {
        if tag(scope) == "Runner" {
            str_of(runner, "id") == str_of(scope, "id")
        } else {
            str_of(runner, "family") == str_of(scope, "id")
        }
    });
    merge_sections(
        runners
            .map(|runner| sections_of(field(runner, "schema")))
            .collect(),
    )
}

fn keep_choices() -> Vec<SurfaceSettingChoice> {
    vec![choice(INHERIT, "Inherit"), choice(DEFAULT, "Default")]
}

/// How one value is changed, from its schema leaf or else its own type.
fn value_interaction(
    leaf: Option<&Leaf>,
    own: Option<&Value>,
    cleared: bool,
) -> (SurfaceSettingInteraction, Encoder) {
    let selected_of = |value: String| {
        if cleared {
            DEFAULT.to_owned()
        } else if own.is_none() {
            INHERIT.to_owned()
        } else {
            value
        }
    };
    if let Some(values) = leaf
        .and_then(|leaf| leaf.choices.clone())
        .filter(|values| !values.is_empty())
    {
        let mut choices = keep_choices();
        choices.extend(
            values
                .iter()
                .map(|value| choice(choice_value(value), value_text(value))),
        );
        let selected = selected_of(own.map_or("undefined".into(), choice_value));
        return (choice_interaction(choices, selected), Encoder::Enum(values));
    }
    let kind = leaf
        .and_then(|leaf| leaf.kind.clone())
        .unwrap_or_else(|| match own {
            Some(Value::Bool(_)) => "boolean".into(),
            Some(Value::Number(_)) => "number".into(),
            _ => "string".into(),
        });
    match kind.as_str() {
        "boolean" => {
            let mut choices = keep_choices();
            choices.extend([choice("true", "On"), choice("false", "Off")]);
            let selected = selected_of(if own == Some(&json!(true)) {
                "true".into()
            } else {
                "false".into()
            });
            (choice_interaction(choices, selected), Encoder::Boolean)
        }
        "number" | "integer" => {
            let integer = kind == "integer";
            (
                SurfaceSettingInteraction::Number {
                    value: own.and_then(Value::as_f64),
                    min: leaf.and_then(|leaf| leaf.minimum),
                    max: leaf.and_then(|leaf| leaf.maximum),
                    step: integer.then_some(1.0),
                    clearable: Some(true),
                },
                Encoder::Number {
                    integer,
                    minimum: leaf.and_then(|leaf| leaf.minimum),
                    maximum: leaf.and_then(|leaf| leaf.maximum),
                },
            )
        }
        _ => (
            SurfaceSettingInteraction::Text {
                placeholder: Some(leaf.map_or("Value".into(), |leaf| leaf.label.clone())),
                max_length: None,
                allow_empty: Some(true),
            },
            Encoder::Text,
        ),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum OwnerKind {
    Runner,
    Modifier,
    Service,
}

/// Whose settings a run of groups holds, and how its rows name, write and add them.
struct Owner<'a> {
    kind: OwnerKind,
    /// The modifier's or service's id; the runner's for a runner place.
    plugin: Option<String>,
    place_id: String,
    place: Value,
    title: String,
    sections: Vec<Section>,
    own: Value,
    added: Vec<String>,
    row_id: &'a dyn Fn(&str) -> String,
    device_scope: &'a dyn Fn(Option<&Leaf>) -> bool,
    missing: String,
    applies_at: &'a dyn Fn(&Value) -> bool,
}

impl Owner<'_> {
    fn path_of(&self, key: &str) -> Value {
        match self.kind {
            OwnerKind::Runner => json!({ "_tag": "Setting", "payload": { "key": key } }),
            OwnerKind::Modifier => {
                json!({ "_tag": "ModifierSetting", "payload": { "id": self.plugin, "key": key } })
            }
            OwnerKind::Service => {
                json!({ "_tag": "ServiceSetting", "payload": { "id": self.plugin, "key": key } })
            }
        }
    }
    fn row_path(&self, key: &str) -> String {
        match self.kind {
            OwnerKind::Runner => format!("setting:{key}"),
            OwnerKind::Modifier => format!(
                "modifier:{}:setting:{key}",
                self.plugin.as_deref().unwrap_or("")
            ),
            OwnerKind::Service => format!(
                "service:{}:setting:{key}",
                self.plugin.as_deref().unwrap_or("")
            ),
        }
    }
    fn add_path(&self, section: &str) -> String {
        match self.kind {
            OwnerKind::Runner => format!("add-setting:{section}"),
            OwnerKind::Modifier => format!(
                "modifier:{}:add-setting:{section}",
                self.plugin.as_deref().unwrap_or("")
            ),
            OwnerKind::Service => format!(
                "service:{}:add-setting:{section}",
                self.plugin.as_deref().unwrap_or("")
            ),
        }
    }
    fn pending_kind(&self) -> PendingKind {
        match self.kind {
            OwnerKind::Runner => PendingKind::Runner,
            OwnerKind::Modifier => PendingKind::Modifier(self.plugin.clone().unwrap_or_default()),
            OwnerKind::Service => PendingKind::Service(self.plugin.clone().unwrap_or_default()),
        }
    }
    fn authored<'v>(&self, body: &'v Value) -> &'v Value {
        match self.kind {
            OwnerKind::Runner => field(body, "settings"),
            OwnerKind::Modifier => field(
                field(
                    field(body, "modifiers"),
                    self.plugin.as_deref().unwrap_or(""),
                ),
                "settings",
            ),
            OwnerKind::Service => field(
                field(
                    field(body, "services"),
                    self.plugin.as_deref().unwrap_or(""),
                ),
                "settings",
            ),
        }
    }
    fn default_of(&self, leaf: Option<&Leaf>) -> Option<String> {
        leaf.and_then(|leaf| leaf.default.as_ref())
            .map(|value| format!("{} from {} (default)", value_text(value), self.title))
    }
}

fn merge_rank(place: &Value) -> i64 {
    let scope = scope_rank(field(place, "scope"));
    match tag(field(place, "layer")) {
        "Device" => {
            if scope == 0 {
                0
            } else {
                1 + scope
            }
        }
        "System" => {
            if scope == 0 {
                1
            } else {
                3 + scope
            }
        }
        "Game" => 10 + scope,
        "Release" => 13 + scope,
        "Profile" => 16 + scope,
        _ => 19 + scope,
    }
}

fn profile_in_force(context: &Context, place: &Value) -> bool {
    let layer = field(place, "layer");
    tag(layer) != "Profile"
        || str_of(field(layer, "id"), "profileId") == str_of(context.snapshot, "selectedProfile")
}

/// Whether an authored value is one the schema takes at its leaf.
fn takes(leaf: Option<&Leaf>, value: &Value) -> bool {
    let Some(leaf) = leaf else { return false };
    if !matches!(value, Value::String(_) | Value::Number(_) | Value::Bool(_)) {
        return false;
    }
    if let Some(choices) = &leaf.choices {
        return choices
            .iter()
            .any(|choice| choice_value(choice) == choice_value(value));
    }
    match leaf.kind.as_deref() {
        Some("boolean") if !value.is_boolean() => return false,
        Some("integer") if !value.as_f64().is_some_and(|number| number.fract() == 0.0) => {
            return false
        }
        Some("number") if !value.is_number() => return false,
        Some("string") if !value.is_string() => return false,
        _ => {}
    }
    if let Some(number) = value.as_f64() {
        if leaf.minimum.is_some_and(|minimum| number < minimum)
            || leaf.maximum.is_some_and(|maximum| number > maximum)
        {
            return false;
        }
    }
    true
}

/// Whether this place wrote the value a skip at a path names.
fn rejected_here(context: &Context, owner: &Owner, path: &str, resolved: Option<&Value>) -> bool {
    if let Some(source) = resolved.and_then(|resolved| resolved.get("source")) {
        if tag(source) == "Place" && same_place(field(source, "payload"), &owner.place) {
            return false;
        }
    }
    let leaf = leaf_at(&owner.sections, path);
    let mut author: Option<(i64, &Value)> = None;
    for entry in &context.places {
        let place = field(entry, "place");
        if !(owner.applies_at)(place) {
            continue;
        }
        let leaves = own_leaves(owner.authored(field(entry, "body")), &[]);
        let Some((_, value)) = leaves.iter().find(|(candidate, _)| candidate == path) else {
            continue;
        };
        if value.is_null() || takes(leaf.as_ref(), value) {
            continue;
        }
        let rank = merge_rank(place);
        if author.is_none_or(|(known, _)| rank > known) {
            author = Some((rank, place));
        }
    }
    author.is_some_and(|(_, place)| same_place(place, &owner.place))
}

fn in_force_text(
    context: &Context,
    owner: &Owner,
    resolved: Option<&Value>,
    none: &dyn Fn() -> Option<String>,
) -> Option<String> {
    let Some(resolved) = resolved
        .filter(|resolved| resolved.get("source").is_some() && !field(resolved, "value").is_null())
    else {
        return none();
    };
    let source = field(resolved, "source");
    let here = tag(source) == "Place" && same_place(field(source, "payload"), &owner.place);
    let title = source_title(context, source, owner.plugin.as_deref());
    let value = value_text(field(resolved, "value"));
    if field(resolved, "adjusted") == &json!(true) {
        return Some(format!("{value} from {title}, adjusted"));
    }
    Some(if here {
        format!("from {title}")
    } else {
        format!("{value} from {title}")
    })
}

fn setting_source(
    context: &Context,
    owner: &Owner,
    path: &str,
    leaf: Option<&Leaf>,
    set: bool,
    cleared: bool,
) -> Option<String> {
    let fallback = || {
        owner
            .default_of(leaf)
            .unwrap_or_else(|| format!("{} decides", owner.title))
    };
    let scope = field(&owner.place, "scope");
    let write_path = owner.path_of(path);
    let stream = stream_runner(context, scope);
    let rejected = |skip: Option<&String>, resolved: Option<&Value>| {
        skip.filter(|_| set && !cleared && rejected_here(context, owner, path, resolved))
            .cloned()
    };
    if stream && tag(&write_path) == "Setting" {
        let scope_id = str_of(scope, "id").unwrap_or("");
        let resolved = context.stream_resolved(scope_id, &write_path);
        if let Some(skip) = rejected(context.skip(&write_path, Some(scope_id)), resolved) {
            return Some(format!("Not applied: {}", skip.trim_end_matches('.')));
        }
        return in_force_text(context, owner, resolved, &|| {
            Some(if cleared {
                fallback()
            } else {
                stream_line(context).into()
            })
        });
    }
    if let Some(waiting) = not_in_force_scope(context, scope) {
        return Some(waiting);
    }
    let resolved = context.resolved(&write_path);
    if let Some(skip) = rejected(context.skip(&write_path, None), resolved) {
        return Some(format!("Not applied: {}", skip.trim_end_matches('.')));
    }
    in_force_text(context, owner, resolved, &|| cleared.then(fallback))
}

/// One setting's rows on one place: the setting, then for a string its
/// Inherit row, then for a number or a string a Default row.
fn setting_rows(
    context: &Context,
    rows: &mut Rows,
    owner: &Owner,
    path: &str,
    leaf: Option<&Leaf>,
    own: &[(String, Value)],
) -> Vec<SurfaceSettingItem> {
    let found = own.iter().find(|(candidate, _)| candidate == path);
    let set = found.is_some();
    let value = found.map(|(_, value)| value);
    let cleared = value.is_some_and(Value::is_null);
    let id = (owner.row_id)(&owner.row_path(path));
    let (interaction, encoder) = value_interaction(leaf, value.filter(|_| !cleared), cleared);
    rows.writes.insert(
        id.clone(),
        LaunchWrite {
            place: owner.place.clone(),
            path: owner.path_of(path),
            encoder,
        },
    );
    if !set {
        rows.pending.insert(
            id.clone(),
            (
                owner.place_id.clone(),
                owner.pending_kind().pending_of(path),
            ),
        );
    }
    let source = setting_source(context, owner, path, leaf, set, cleared);
    let label = leaf.map_or_else(
        || humanize(&path[path.rfind('.').map_or(0, |at| at + 1)..]),
        |leaf| leaf.label.clone(),
    );
    let text = matches!(interaction, SurfaceSettingInteraction::Text { .. });
    let shown = if cleared {
        (!text).then(|| "Default".to_owned())
    } else if text {
        value
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty() && set)
            .map(Into::into)
    } else {
        Some(if set {
            value_text(value.unwrap_or(&Value::Null))
        } else {
            "Not set here".into()
        })
    };
    let said = leaf
        .and_then(|leaf| leaf.description.clone())
        .or_else(|| leaf.is_none().then(|| owner.missing.clone()));
    let streams = (tag(&owner.path_of(path)) == "Setting"
        && stream_runner(context, field(&owner.place, "scope")))
    .then(|| format!("{}.", stream_line(context)));
    let description = match (streams, said) {
        (None, said) => said,
        (Some(streams), None) => Some(streams),
        (Some(streams), Some(said)) => Some(format!("{streams} {said}")),
    };
    let interaction = match interaction {
        SurfaceSettingInteraction::Text {
            max_length,
            allow_empty,
            ..
        } => SurfaceSettingInteraction::Text {
            placeholder: Some(
                if cleared {
                    "Default"
                } else if set {
                    "Empty"
                } else {
                    "Not set here"
                }
                .into(),
            ),
            max_length,
            allow_empty,
        },
        other => other,
    };
    let is_choice = matches!(interaction, SurfaceSettingInteraction::Choice { .. });
    let mut out = vec![SurfaceSettingItem {
        value: shown,
        description,
        source,
        scope: Some(if (owner.device_scope)(leaf) {
            SurfaceSettingScope::Device
        } else {
            SurfaceSettingScope::Shared
        }),
        interaction: Some(interaction),
        ..SurfaceSettingItem::new(id.clone(), label.clone())
    }];
    if text {
        let clear = (owner.row_id)(&format!("clear:{}", owner.row_path(path)));
        rows.clears.insert(clear.clone(), id.clone());
        out.push(SurfaceSettingItem {
            value: Some("Clear".into()),
            description: Some(
                "Takes the value set here away, so the value in force comes from elsewhere.".into(),
            ),
            interaction: Some(action_interaction(clear.clone(), None, None)),
            ..SurfaceSettingItem::new(clear, format!("Inherit {label}"))
        });
    }
    if !is_choice && !cleared {
        let reset = (owner.row_id)(&format!("default:{}", owner.row_path(path)));
        rows.defaults.insert(reset.clone(), id);
        out.push(SurfaceSettingItem {
            value: Some("Use".into()),
            description: Some(format!(
                "{}'s own default holds here, whatever less specific places set.",
                owner.title
            )),
            interaction: Some(action_interaction(reset.clone(), None, None)),
            ..SurfaceSettingItem::new(reset, format!("Default {label}"))
        });
    }
    out
}

/// One plugin's settings on one place: a group per section of its schema.
fn setting_groups(context: &Context, rows: &mut Rows, owner: &Owner) -> Vec<SurfaceSettingGroup> {
    let own = own_leaves(&owner.own, &[]);
    let mut used: Vec<String> = own.iter().map(|(path, _)| path.clone()).collect();
    used.extend(owner.added.iter().cloned());
    let expanded: Vec<Section> = owner
        .sections
        .iter()
        .map(|section| Section {
            leaves: section_leaves(section, &used),
            ..section.clone()
        })
        .collect();
    let described: HashMap<String, Leaf> = expanded
        .iter()
        .flat_map(|section| {
            section
                .leaves
                .iter()
                .map(|leaf| (leaf.path.clone(), leaf.clone()))
        })
        .collect();
    let mut extra_keys: Vec<String> = Vec::new();
    for path in used.iter().filter(|path| !described.contains_key(*path)) {
        let key = section_key_of(path).to_owned();
        if !extra_keys.contains(&key) && !expanded.iter().any(|section| section.key == key) {
            extra_keys.push(key);
        }
    }
    let mut sections = expanded;
    sections.extend(extra_keys.into_iter().map(|key| Section {
        title: if key.is_empty() {
            String::new()
        } else {
            humanize(&key)
        },
        key,
        leaves: Vec::new(),
        patterns: Vec::new(),
    }));
    let has_own = |path: &str| own.iter().any(|(candidate, _)| candidate == path);
    let mut groups = Vec::new();
    for section in &sections {
        let in_section = |path: &str| section_key_of(path) == section.key;
        let mut shown: Vec<String> = section
            .leaves
            .iter()
            .filter(|leaf| has_own(&leaf.path))
            .map(|leaf| leaf.path.clone())
            .collect();
        shown.extend(
            own.iter()
                .map(|(path, _)| path.clone())
                .filter(|path| in_section(path) && !described.contains_key(path)),
        );
        shown.extend(
            owner
                .added
                .iter()
                .filter(|path| in_section(path) && !has_own(path))
                .cloned(),
        );
        let mut items: Vec<SurfaceSettingItem> = shown
            .iter()
            .flat_map(|path| setting_rows(context, rows, owner, path, described.get(path), &own))
            .collect();
        let unset: Vec<&Leaf> = section
            .leaves
            .iter()
            .filter(|leaf| !shown.contains(&leaf.path))
            .collect();
        if !unset.is_empty() {
            let id = (owner.row_id)(&owner.add_path(&section.key));
            rows.adds.insert(
                id.clone(),
                LaunchAdd {
                    place_id: owner.place_id.clone(),
                    kind: owner.pending_kind(),
                },
            );
            items.push(SurfaceSettingItem {
                value: Some("Choose a setting".into()),
                description: Some(format!(
                    "{} more {} {} offers here.",
                    unset.len(),
                    if unset.len() == 1 {
                        "setting"
                    } else {
                        "settings"
                    },
                    owner.title
                )),
                interaction: Some(choice_interaction(
                    unset
                        .iter()
                        .map(|leaf| choice(leaf.path.clone(), leaf.label.clone()))
                        .collect(),
                    "",
                )),
                ..SurfaceSettingItem::new(id, "Add setting")
            });
        }
        if items.is_empty() {
            continue;
        }
        groups.push(SurfaceSettingGroup {
            title: if section.key.is_empty() {
                owner.title.clone()
            } else {
                format!("{} · {}", owner.title, section.title)
            },
            items,
        });
    }
    groups
}

fn runner_owner_title(context: &Context, scope: &Value) -> String {
    if tag(scope) == "Runner" {
        let id = str_of(scope, "id").unwrap_or("");
        return context
            .runner_titles
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.into());
    }
    scope_label(context, scope)
}

fn runner_setting_groups(
    context: &Context,
    rows: &mut Rows,
    place_id: &str,
    place: &Value,
    body: &Value,
    pending: &[PendingSetting],
    row_id: &dyn Fn(&str) -> String,
) -> Vec<SurfaceSettingGroup> {
    let scope = field(place, "scope");
    let title = runner_owner_title(context, scope);
    let stream = if tag(scope) == "Runner" && stream_runner(context, scope) {
        array(context.snapshot, "runners")
            .iter()
            .find(|runner| str_of(runner, "id") == str_of(scope, "id"))
    } else {
        None
    };
    let applies_at = |candidate: &Value| -> bool {
        if !profile_in_force(context, candidate) {
            return false;
        }
        let candidate_scope = field(candidate, "scope");
        match stream {
            None => applies(context, candidate_scope),
            Some(stream) => {
                let for_stream = (tag(candidate_scope) == "Runner"
                    && str_of(candidate_scope, "id") == str_of(stream, "id"))
                    || (tag(candidate_scope) == "Family"
                        && str_of(candidate_scope, "id") == str_of(stream, "family"));
                for_stream && !matches!(tag(field(candidate, "layer")), "Release" | "Launch")
            }
        }
    };
    let plugin = if tag(scope) == "Runner" {
        str_of(scope, "id").map(Into::into)
    } else if context.selected.and_then(|runner| str_of(runner, "family")) == str_of(scope, "id") {
        context.selected_id().map(Into::into)
    } else {
        None
    };
    let is_stream = stream_runner(context, scope);
    let device_scope = |leaf: Option<&Leaf>| leaf.is_some_and(|leaf| leaf.device) || is_stream;
    setting_groups(
        context,
        rows,
        &Owner {
            kind: OwnerKind::Runner,
            plugin,
            place_id: place_id.into(),
            place: place.clone(),
            title,
            sections: schema_for(context, scope),
            own: field(body, "settings").clone(),
            added: pending
                .iter()
                .filter_map(|setting| match setting {
                    PendingSetting::Runner { key } => Some(key.clone()),
                    _ => None,
                })
                .collect(),
            row_id,
            device_scope: &device_scope,
            missing: "This runner no longer describes this setting.".into(),
            applies_at: &applies_at,
        },
    )
}

struct Plugin {
    id: String,
    title: String,
    schema: Option<Value>,
    listed: bool,
}

fn plugins_of(context: &Context, listed: &[Value], kind: &str) -> Vec<Plugin> {
    let mut known: Vec<Plugin> = listed
        .iter()
        .map(|plugin| Plugin {
            id: str_of(plugin, "id").unwrap_or("").into(),
            title: str_of(plugin, "title").unwrap_or("").into(),
            schema: plugin
                .get("schema")
                .filter(|schema| !schema.is_null())
                .cloned(),
            listed: true,
        })
        .collect();
    let mut named: Vec<String> = Vec::new();
    for place in &context.places {
        if let Some(map) = record(field(field(place, "body"), kind)) {
            for id in map.keys() {
                if !named.contains(id) {
                    named.push(id.clone());
                }
            }
        }
    }
    let mut unlisted: Vec<String> = named
        .into_iter()
        .filter(|id| !known.iter().any(|plugin| &plugin.id == id))
        .collect();
    unlisted.sort();
    known.extend(unlisted.into_iter().map(|id| Plugin {
        title: id.clone(),
        id,
        schema: None,
        listed: false,
    }));
    known
}

fn modifier_groups(
    context: &Context,
    rows: &mut Rows,
    place_id: &str,
    place: &Value,
    body: &Value,
    pending: &[PendingSetting],
    row_id: &dyn Fn(&str) -> String,
) -> Vec<SurfaceSettingGroup> {
    let own = field(body, "modifiers");
    let mut groups = Vec::new();
    for modifier in plugins_of(context, array(context.snapshot, "modifiers"), "modifiers") {
        let authored = field(own, &modifier.id);
        let enabled = field(authored, "enabled").as_bool();
        let id = row_id(&format!("modifier:{}:enabled", modifier.id));
        let path = json!({ "_tag": "ModifierEnabled", "payload": { "id": modifier.id } });
        rows.writes.insert(
            id.clone(),
            LaunchWrite {
                place: place.clone(),
                path: path.clone(),
                encoder: Encoder::Toggle,
            },
        );
        let source = source_text(context, place, context.resolved(&path), &value_text);
        let enabled_row = SurfaceSettingItem {
            value: Some(
                match enabled {
                    None => "Inherit",
                    Some(true) => "On",
                    Some(false) => "Off",
                }
                .into(),
            ),
            description: Some(if modifier.listed {
                NEXT_LAUNCH.into()
            } else {
                "This modifier isn't installed or is turned off on this device.".into()
            }),
            source,
            scope: Some(SurfaceSettingScope::Device),
            interaction: Some(choice_interaction(
                vec![
                    choice(INHERIT, "Inherit"),
                    choice("true", "On"),
                    choice("false", "Off"),
                ],
                enabled.map_or(INHERIT.to_owned(), |enabled| enabled.to_string()),
            )),
            ..SurfaceSettingItem::new(id, modifier.title.clone())
        };
        let applies_at = |candidate: &Value| {
            profile_in_force(context, candidate) && applies(context, field(candidate, "scope"))
        };
        let device_scope = |_: Option<&Leaf>| true;
        let settings = setting_groups(
            context,
            rows,
            &Owner {
                kind: OwnerKind::Modifier,
                plugin: Some(modifier.id.clone()),
                place_id: place_id.into(),
                place: place.clone(),
                title: modifier.title.clone(),
                sections: modifier
                    .schema
                    .as_ref()
                    .map(sections_of)
                    .unwrap_or_default(),
                own: field(authored, "settings").clone(),
                added: pending
                    .iter()
                    .filter_map(|setting| match setting {
                        PendingSetting::Modifier { id, key } if *id == modifier.id => {
                            Some(key.clone())
                        }
                        _ => None,
                    })
                    .collect(),
                row_id,
                device_scope: &device_scope,
                missing: "This modifier no longer describes this setting.".into(),
                applies_at: &applies_at,
            },
        );
        groups.push(SurfaceSettingGroup {
            title: format!("Modifier {}", modifier.title),
            items: vec![enabled_row],
        });
        groups.extend(settings);
    }
    groups
}

fn service_groups(
    context: &Context,
    rows: &mut Rows,
    place_id: &str,
    place: &Value,
    body: &Value,
    pending: &[PendingSetting],
    row_id: &dyn Fn(&str) -> String,
) -> Vec<SurfaceSettingGroup> {
    let own = field(body, "services");
    let mut groups = Vec::new();
    for service in plugins_of(context, array(context.snapshot, "services"), "services") {
        let applies_at = |candidate: &Value| {
            profile_in_force(context, candidate) && applies(context, field(candidate, "scope"))
        };
        let device_scope = |_: Option<&Leaf>| true;
        groups.extend(setting_groups(
            context,
            rows,
            &Owner {
                kind: OwnerKind::Service,
                plugin: Some(service.id.clone()),
                place_id: place_id.into(),
                place: place.clone(),
                title: service.title.clone(),
                sections: service.schema.as_ref().map(sections_of).unwrap_or_default(),
                own: field(field(own, &service.id), "settings").clone(),
                added: pending
                    .iter()
                    .filter_map(|setting| match setting {
                        PendingSetting::Service { id, key } if *id == service.id => {
                            Some(key.clone())
                        }
                        _ => None,
                    })
                    .collect(),
                row_id,
                device_scope: &device_scope,
                missing: if service.listed {
                    "This service no longer describes this setting.".into()
                } else {
                    "This service isn't installed on this device.".into()
                },
                applies_at: &applies_at,
            },
        ));
    }
    groups
}

fn not_in_force_text(context: &Context, place: &Value) -> String {
    if !applies(context, field(place, "scope")) {
        return source_text(context, place, None, &value_text)
            .unwrap_or_else(|| "Not in force".into());
    }
    let layer = field(place, "layer");
    if tag(layer) == "Profile"
        && str_of(field(layer, "id"), "profileId") != str_of(context.snapshot, "selectedProfile")
    {
        return "Not in force: this profile isn't picked".into();
    }
    "Not in force".into()
}

const LISTS: [(&str, &str, &str); 4] = [
    ("ArgsPrepend", "Arguments before", "args:prepend"),
    ("ArgsAppend", "Arguments after", "args:append"),
    ("HooksBefore", "Before launch", "hooks:before"),
    ("HooksAfter", "After the session", "hooks:after"),
];

fn part_text(value: &Value) -> String {
    match value.as_array() {
        None => str_of(value, "run").map_or_else(|| value_text(value), Into::into),
        Some(items) => {
            let words = items.iter().all(Value::is_string);
            items
                .iter()
                .map(part_text)
                .collect::<Vec<_>>()
                .join(if words { " " } else { "; " })
        }
    }
}

fn own_part<'a>(body: &'a Value, kind: &str) -> &'a Value {
    match kind {
        "ArgsPrepend" => field(field(body, "args"), "prepend"),
        "ArgsAppend" => field(field(body, "args"), "append"),
        "HooksBefore" => field(field(body, "hooks"), "before"),
        _ => field(field(body, "hooks"), "after"),
    }
}

fn is_empty_part(value: &Value) -> bool {
    value.is_null() || value.as_array().is_some_and(Vec::is_empty)
}

/// Values set only in files: shown with where they are, never changed here.
fn file_group(context: &Context, entry: &Value, place_id: &str) -> Option<SurfaceSettingGroup> {
    let place = field(entry, "place");
    let body = field(entry, "body");
    let here = place_title(context, entry);
    let describe_of = |layer: &Value| format!("Set in {}. Change it there.", file_of(layer));
    let row = |key: &str,
               label: String,
               value: String,
               source: Option<String>,
               description: Option<String>| SurfaceSettingItem {
        value: Some(value),
        source: Some(source.unwrap_or_else(|| format!("from {here}"))),
        scope: Some(SurfaceSettingScope::Device),
        description: Some(description.unwrap_or_else(|| describe_of(field(place, "layer")))),
        ..SurfaceSettingItem::new(launch_id::set(place_id, &format!("file:{key}")), label)
    };
    let mut items = Vec::new();
    if let Some(env) = record(field(body, "env")) {
        for (key, value) in env {
            items.push(row(
                &format!("env:{key}"),
                format!("Environment {key}"),
                if value.is_null() {
                    "Unset".into()
                } else {
                    value_text(value)
                },
                None,
                None,
            ));
        }
    }
    if let Some(cwd) = str_of(body, "cwd") {
        items.push(row("cwd", "Working folder".into(), cwd.into(), None, None));
    }
    let own_contributions = |kind: &str| -> Vec<Value> {
        context
            .list(kind)
            .map(|list| array(list, "contributions"))
            .unwrap_or(&[])
            .iter()
            .filter(|part| {
                let source = field(part, "source");
                tag(source) == "Place"
                    && same_place(field(source, "payload"), place)
                    && !is_empty_part(field(part, "value"))
            })
            .cloned()
            .collect()
    };
    let in_force = LISTS
        .iter()
        .any(|(kind, _, _)| !own_contributions(kind).is_empty());
    let not_in_force = not_in_force_text(context, place);
    for (kind, label, key) in LISTS {
        if !in_force {
            let own = own_part(body, kind);
            if !is_empty_part(own) {
                items.push(row(
                    &format!("{key}:own"),
                    label.into(),
                    part_text(own),
                    Some(not_in_force.clone()),
                    None,
                ));
            }
            continue;
        }
        for (index, part) in own_contributions(kind).iter().enumerate() {
            let routine = str_of(part, "routine");
            items.push(row(
                &format!("{key}:{index}"),
                label.into(),
                part_text(field(part, "value")),
                Some(match routine {
                    None => format!("In force: from {here}"),
                    Some(routine) => format!("In force: routine {routine}, used by {here}"),
                }),
                Some(if routine.is_none() {
                    describe_of(field(place, "layer"))
                } else {
                    "Set in device.yaml. Change it there.".into()
                }),
            ));
        }
    }
    if !in_force {
        for name in strings(field(field(body, "hooks"), "use")) {
            items.push(row(
                &format!("hooks:use:{name}"),
                "Routine".into(),
                name.clone(),
                Some(not_in_force.clone()),
                Some("Set in device.yaml. Change it there.".into()),
            ));
        }
    }
    if let Some(config) = record(field(body, "config")).filter(|config| !config.is_empty()) {
        items.push(row(
            "config",
            "Runner config".into(),
            value_text(&Value::Object(config.clone())),
            None,
            None,
        ));
    }
    if let Some(modifiers) = record(field(body, "modifiers")) {
        for (modifier, raw) in modifiers {
            if let Some(config) = raw.get("config") {
                items.push(row(
                    &format!("modifier-config:{modifier}"),
                    format!("{modifier} config"),
                    value_text(config),
                    None,
                    None,
                ));
            }
        }
    }
    (!items.is_empty()).then(|| SurfaceSettingGroup {
        title: "From files".into(),
        items,
    })
}

fn runner_facts_group(
    context: &Context,
    place_id: &str,
    scope: &Value,
) -> Option<SurfaceSettingGroup> {
    if tag(scope) != "Runner" {
        return None;
    }
    let runner = array(context.snapshot, "runners")
        .iter()
        .find(|candidate| str_of(candidate, "id") == str_of(scope, "id"))?;
    let title = context.runner_title(runner);
    let from = format!("from {title} (default)");
    let row = |key: &str, label: String, value: String| SurfaceSettingItem {
        value: Some(value),
        source: Some(from.clone()),
        scope: Some(SurfaceSettingScope::Device),
        ..SurfaceSettingItem::new(
            launch_id::set(place_id, &format!("runner-facts:{key}")),
            label,
        )
    };
    let prepare = field(runner, "prepare");
    if tag(prepare) == "Unavailable" {
        return Some(SurfaceSettingGroup {
            title: "From the runner".into(),
            items: vec![SurfaceSettingItem {
                value: Some(str_of(prepare, "payload").unwrap_or("").into()),
                description: Some(format!("Korri could not ask {title} what it sets. Its environment, arguments and folder are not shown.")),
                ..SurfaceSettingItem::new(launch_id::set(place_id, "runner-facts:unavailable"), "Runner facts unavailable")
            }],
        });
    }
    let facts = field(prepare, "payload");
    let mut items: Vec<SurfaceSettingItem> = record(field(facts, "env"))
        .map(|env| {
            env.iter()
                .map(|(key, value)| {
                    row(
                        &format!("env:{key}"),
                        format!("Environment {key}"),
                        value.as_str().unwrap_or("").into(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let args = strings(field(facts, "args"));
    if !args.is_empty() {
        items.push(row("args", "Arguments".into(), args.join(" ")));
    }
    if let Some(cwd) = str_of(facts, "cwd") {
        items.push(row("cwd", "Working folder".into(), cwd.into()));
    }
    (!items.is_empty()).then(|| SurfaceSettingGroup {
        title: "From the runner".into(),
        items,
    })
}

fn profile_rows(context: &Context, own: Option<&str>) -> SurfaceSettingGroup {
    let snapshot = context.snapshot;
    let profiles = array(snapshot, "profiles");
    let titles = profile_titles(profiles);
    let selected_profile = str_of(snapshot, "selectedProfile");
    if let Some(profile_id) = own {
        let title = profiles
            .iter()
            .find(|profile| str_of(profile, "id") == Some(profile_id))
            .and_then(|profile| str_of(profile, "title"))
            .unwrap_or(profile_id)
            .to_owned();
        let shown = titles
            .get(profile_id)
            .cloned()
            .unwrap_or_else(|| title.clone());
        return SurfaceSettingGroup {
            title: "Profile".into(),
            items: vec![
                SurfaceSettingItem {
                    value: Some(title),
                    description: Some(
                        if selected_profile == Some(profile_id) {
                            "Used at next launch."
                        } else {
                            "Not used at next launch."
                        }
                        .into(),
                    ),
                    interaction: Some(SurfaceSettingInteraction::Text {
                        placeholder: Some("Profile name".into()),
                        max_length: Some(MAX_PROFILE_TITLE),
                        allow_empty: None,
                    }),
                    ..SurfaceSettingItem::new(launch_id::rename(profile_id), "Name")
                },
                SurfaceSettingItem {
                    interaction: Some(action_interaction(
                        launch_id::remove(profile_id),
                        Some(true),
                        Some((
                            format!("Delete {shown}?"),
                            "Its values go. The game's other launch settings stay.".into(),
                            "Delete profile".into(),
                        )),
                    )),
                    ..SurfaceSettingItem::new(launch_id::remove(profile_id), "Delete profile")
                },
            ],
        };
    }
    let selected = profiles
        .iter()
        .find(|profile| str_of(profile, "id") == selected_profile && selected_profile.is_some());
    let mut choices = vec![choice(NO_PROFILE, "No profile")];
    choices.extend(profiles.iter().map(|profile| {
        let id = str_of(profile, "id").unwrap_or("");
        choice(
            id,
            titles
                .get(id)
                .cloned()
                .unwrap_or_else(|| str_of(profile, "title").unwrap_or("").into()),
        )
    }));
    SurfaceSettingGroup {
        title: "Profile".into(),
        items: vec![
            SurfaceSettingItem {
                value: Some(selected.map_or("No profile".into(), |profile| {
                    let id = str_of(profile, "id").unwrap_or("");
                    titles
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| str_of(profile, "title").unwrap_or("").into())
                })),
                description: Some(
                    "Remembered for this game on this device. Applies at next launch.".into(),
                ),
                interaction: Some(choice_interaction(
                    choices,
                    selected
                        .and_then(|profile| str_of(profile, "id"))
                        .unwrap_or(NO_PROFILE),
                )),
                ..SurfaceSettingItem::new(launch_id::PICK, "Profile at next launch")
            },
            SurfaceSettingItem {
                description: Some("Starts with every value inherited.".into()),
                interaction: Some(SurfaceSettingInteraction::Text {
                    placeholder: Some("Profile name".into()),
                    max_length: Some(MAX_PROFILE_TITLE),
                    allow_empty: None,
                }),
                ..SurfaceSettingItem::new(launch_id::CREATE, "New profile")
            },
        ],
    }
}

fn skipped_name(context: &Context, path: &Value, runner_id: Option<&str>) -> String {
    let named = |title: String, schema: &Value, key: &str| {
        let leaf = leaf_at(&sections_of(schema), key);
        format!(
            "{} ({title})",
            leaf.map_or(key.to_owned(), |leaf| leaf.label)
        )
    };
    let key = str_of(field(path, "payload"), "key").unwrap_or("");
    match tag(path) {
        "Setting" => {
            let runner = match runner_id {
                None => context.selected,
                Some(id) => array(context.snapshot, "runners")
                    .iter()
                    .find(|candidate| str_of(candidate, "id") == Some(id)),
            };
            match runner {
                None => key.into(),
                Some(runner) => named(context.runner_title(runner), field(runner, "schema"), key),
            }
        }
        "ModifierSetting" | "ServiceSetting" => {
            let id = str_of(field(path, "payload"), "id").unwrap_or("");
            let plugin = array(context.snapshot, "modifiers")
                .iter()
                .chain(array(context.snapshot, "services"))
                .find(|candidate| str_of(candidate, "id") == Some(id));
            named(
                plugin
                    .and_then(|plugin| str_of(plugin, "title"))
                    .unwrap_or(id)
                    .into(),
                plugin.map_or(&Value::Null, |plugin| field(plugin, "schema")),
                key,
            )
        }
        other => other.into(),
    }
}

/// Plain lines about the whole editor.
pub fn launch_notes_from(snapshot: &Value, game_id: Option<&str>) -> Vec<String> {
    let context = Context::new(snapshot, game_id);
    let partial = "Values in force leave them out.";
    let route_layers = field(snapshot, "routeLayers");
    let mut notes = Vec::new();
    match tag(route_layers) {
        "NeedsRunnerChoice" => notes.push(format!(
            "System and release settings appear once you choose a runner. {partial}"
        )),
        "Unavailable" => notes.push(format!(
            "System and release settings aren't available: {}. {partial}",
            str_of(route_layers, "message")
                .unwrap_or("")
                .trim_end_matches('.')
        )),
        _ => {}
    }
    let in_use = context.selected.map(|runner| context.runner_title(runner));
    for notice in array(snapshot, "notices") {
        if tag(notice) != "RunnerUnavailable" {
            continue;
        }
        let payload = field(notice, "payload");
        let place = source_title(
            &context,
            &json!({ "_tag": "Place", "payload": field(payload, "place") }),
            None,
        );
        notes.push(format!(
            "{place} names runner {}, which isn't installed. Korri skips it and its runner settings there{}",
            str_of(payload, "runnerId").unwrap_or(""),
            in_use.as_ref().map_or(".".into(), |in_use| format!(" and uses {in_use}."))
        ));
    }
    for notice in array(snapshot, "notices") {
        if tag(notice) != "SettingSkipped" {
            continue;
        }
        let payload = field(notice, "payload");
        notes.push(format!(
            "{} isn't applied: {}.",
            skipped_name(
                &context,
                field(payload, "path"),
                str_of(payload, "runnerId")
            ),
            str_of(payload, "message")
                .unwrap_or("")
                .trim_end_matches('.')
        ));
    }
    for runner in array(snapshot, "runners") {
        let prepare = field(runner, "prepare");
        if tag(prepare) == "Unavailable" {
            notes.push(format!(
                "{} facts are unavailable: {}.",
                context.runner_title(runner),
                str_of(prepare, "payload")
                    .unwrap_or("")
                    .trim_end_matches('.')
            ));
        }
    }
    notes
}

/// One game's editor: a place per layer and runner scope, most general first.
pub fn launch_editor_from(
    snapshot: &Value,
    game_id: Option<&str>,
    pending: &HashMap<String, Vec<PendingSetting>>,
) -> LaunchEditor {
    let context = Context::new(snapshot, game_id);
    let mut rows = Rows::default();
    let places = context
        .places
        .iter()
        .map(|entry| {
            let place = field(entry, "place");
            let place_id = place_id_of(place);
            let body = field(entry, "body");
            let kinds = strings(field(entry, "editableKinds"));
            let has = |kind: &str| kinds.iter().any(|candidate| candidate == kind);
            let scope = field(place, "scope");
            let any = tag(scope) == "Any";
            let added = pending.get(&place_id).cloned().unwrap_or_default();
            let row_id = |path: &str| launch_id::set(&place_id, path);
            let layer = field(place, "layer");
            let mut groups: Vec<SurfaceSettingGroup> = Vec::new();
            if any && has(kind::RUNNER) {
                groups.push(runner_group(&context, &mut rows, &place_id, place, body));
            }
            if has(kind::SETTINGS) {
                groups.extend(runner_setting_groups(
                    &context, &mut rows, &place_id, place, body, &added, &row_id,
                ));
            }
            groups.extend(limit_groups(
                &context,
                &mut rows,
                place,
                body,
                &kinds,
                &|key| launch_id::set(&place_id, key),
            ));
            if has(kind::MODIFIERS) {
                groups.extend(modifier_groups(
                    &context, &mut rows, &place_id, place, body, &added, &row_id,
                ));
            }
            if has(kind::SERVICES) {
                groups.extend(service_groups(
                    &context, &mut rows, &place_id, place, body, &added, &row_id,
                ));
            }
            if tag(layer) == "Game" && any {
                groups.push(profile_rows(&context, None));
            }
            if tag(layer) == "Profile" && any {
                groups.push(profile_rows(
                    &context,
                    str_of(field(layer, "id"), "profileId"),
                ));
            }
            groups.extend(file_group(&context, entry, &place_id));
            groups.extend(runner_facts_group(&context, &place_id, scope));
            SurfaceLaunchSettingsPlace {
                id: place_id.clone(),
                title: place_title(&context, entry),
                layer: SurfaceLaunchSettingsLayer {
                    id: layer_key(layer),
                    title: layer_title(&context, layer),
                },
                scope: SurfaceLaunchSettingsScope {
                    kind: match tag(scope) {
                        "Any" => SurfaceLaunchSettingsScopeKind::Any,
                        "Family" => SurfaceLaunchSettingsScopeKind::Family,
                        _ => SurfaceLaunchSettingsScopeKind::Runner,
                    },
                    label: scope_label(&context, scope),
                },
                groups,
            }
        })
        .collect();
    LaunchEditor {
        places,
        notes: launch_notes_from(snapshot, game_id),
        rows,
    }
}

/// Device settings' launch groups, from a snapshot without a game.
pub fn device_launch_from(
    snapshot: &Value,
    pending: &HashMap<String, Vec<PendingSetting>>,
) -> DeviceLaunch {
    let context = Context::new(snapshot, None);
    let mut rows = Rows::default();
    let entry = context.places.iter().find(|place| {
        let place = field(place, "place");
        tag(field(place, "layer")) == "Device" && tag(field(place, "scope")) == "Any"
    });
    let device_any = json!({ "layer": { "_tag": "Device" }, "scope": { "_tag": "Any" } });
    let place = entry.map_or(&device_any, |entry| field(entry, "place"));
    let limits = limit_groups(
        &context,
        &mut rows,
        place,
        entry.map_or(&Value::Null, |entry| field(entry, "body")),
        &[kind::CPU.into(), kind::GPU.into()],
        &launch_id::device,
    );
    let items: Vec<SurfaceSettingItem> = limits.into_iter().flat_map(|group| group.items).collect();
    let governor_id = launch_id::device("cpu.governor");
    let (governor, mut ordered): (Vec<_>, Vec<_>) =
        items.into_iter().partition(|item| item.id == governor_id);
    ordered.extend(governor);
    let mut groups = vec![SurfaceSettingGroup {
        title: "Launch settings".into(),
        items: ordered,
    }];
    for candidate in context.places.iter().filter(|candidate| {
        let place = field(candidate, "place");
        tag(field(place, "layer")) == "Device"
            && stream_runner(&context, field(place, "scope"))
            && strings(field(candidate, "editableKinds"))
                .iter()
                .any(|kind| kind == kind::SETTINGS)
    }) {
        let place = field(candidate, "place");
        let place_id = place_id_of(place);
        let added = pending.get(&place_id).cloned().unwrap_or_default();
        groups.extend(runner_setting_groups(
            &context,
            &mut rows,
            &place_id,
            place,
            field(candidate, "body"),
            &added,
            &|path| launch_id::device_set(&place_id, path),
        ));
    }
    if let Some(entry) = entry.filter(|entry| {
        strings(field(entry, "editableKinds"))
            .iter()
            .any(|kind| kind == kind::SERVICES)
    }) {
        let place = field(entry, "place");
        let place_id = place_id_of(place);
        let added = pending.get(&place_id).cloned().unwrap_or_default();
        groups.extend(service_groups(
            &context,
            &mut rows,
            &place_id,
            place,
            field(entry, "body"),
            &added,
            &|path| launch_id::device_set(&place_id, path),
        ));
    }
    DeviceLaunch { groups, rows }
}

/// The profile a pick chooses: its id, or None for "No profile".
pub fn profile_from_pick(choice: &str) -> Option<String> {
    (choice != NO_PROFILE).then(|| choice.to_owned())
}

/// The selected profile's title, as the editor shows it.
pub fn active_profile_of(snapshot: &Value) -> Option<String> {
    let selected = str_of(snapshot, "selectedProfile")?;
    let profiles = array(snapshot, "profiles");
    let profile = profiles
        .iter()
        .find(|profile| str_of(profile, "id") == Some(selected))?;
    Some(
        profile_titles(profiles)
            .get(selected)
            .cloned()
            .unwrap_or_else(|| str_of(profile, "title").unwrap_or("").into()),
    )
}
