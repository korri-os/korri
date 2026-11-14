//! One game's launch settings as one list (ADR 0066): each setting once,
//! grouped by setting section, with the value the next launch would use. The
//! same list serves a game this device runs and a streamed game. Pure: the
//! snapshot of the device that runs the game in, surface rows out.

use korri_surface_treaty::SurfaceLaunchSettingsChange;

use super::*;
use crate::surface::one_launch::{Change, Side};

/// The keep value for changes that end with the next launch.
pub const KEEP_LAUNCH: &str = "launch";

/// A row that takes a change. The change waits until the editor closes.
#[derive(Clone, Debug)]
pub(crate) struct ListRow {
    /// korrid's typed path (`LaunchPath`), as JSON.
    pub path: Value,
    /// The device the value belongs to.
    pub side: Side,
    /// The value before any change. Null: the plugin's default.
    pub stored: Value,
    pub encoder: Encoder,
}

impl ListRow {
    /// The value a surface text stands for. None: not a value of this row.
    /// Null: the plugin's default.
    pub fn value_of(&self, text: &str) -> Option<Value> {
        if matches!(self.encoder, Encoder::Text) && text.is_empty() {
            return Some(Value::Null);
        }
        let encoded = self.encoder.encode(text)?;
        match encoded.value {
            Some(value) => Some(value),
            None if encoded.clear => Some(Value::Null),
            // An empty number box: the default. No row offers Inherit.
            None if matches!(self.encoder, Encoder::Number { .. }) => Some(Value::Null),
            None => None,
        }
    }
}

/// A place the changes can be kept in, by its keep value.
#[derive(Clone, Debug)]
pub(crate) struct KeepPlace {
    /// korrid's layer, as JSON. None: this launch only.
    pub layer: Option<Value>,
}

#[derive(Default)]
pub(crate) struct LaunchList {
    pub groups: Vec<SurfaceSettingGroup>,
    pub changes: Vec<SurfaceLaunchSettingsChange>,
    pub keep: Option<SurfaceSettingItem>,
    pub keep_places: HashMap<String, KeepPlace>,
    /// The keep value in force: the person's, or else this game.
    pub keep_selected: String,
    pub rows: HashMap<String, ListRow>,
    pub notes: Vec<String>,
}

pub(crate) struct ListInput<'a> {
    /// The launch settings of the device that runs the game. None: never read.
    pub snapshot: Option<&'a Value>,
    pub game_id: &'a str,
    /// The label of the device that runs a streamed game. None: this device.
    pub holder: Option<&'a str>,
    /// False: the holder did not answer, so its rows cannot change now.
    pub reachable: bool,
    /// This device's own launch settings, for its stream section. Only a
    /// streamed game has one.
    pub viewer: Option<&'a Value>,
    pub changes: &'a [Change],
    pub keep: Option<&'a str>,
    /// The page offers the runner chooser.
    pub can_choose_runner: bool,
}

impl ListInput<'_> {
    fn stays(&self) -> String {
        match self.holder {
            Some(holder) => format!("Stays on {holder}"),
            None => "Stays on this device".into(),
        }
    }
    fn unreachable(&self) -> Option<String> {
        (!self.reachable).then(|| {
            format!(
                "{} cannot be reached. This cannot change now.",
                self.holder.unwrap_or("This device")
            )
        })
    }
}

/// A value as a person reads it. Null is the plugin's own default.
fn shown(key: &str, value: &Value) -> String {
    if value.is_null() {
        return "Default".into();
    }
    let last = key.rsplit('.').next().unwrap_or(key);
    match value {
        Value::Number(_) if key == "video.render-scale" => format!("{}x", value_text(value)),
        Value::Number(_) if last.ends_with("-percent") => format!("{}%", value_text(value)),
        _ => value_text(value),
    }
}

/// A named choice as words: "fill-view" reads "Fill view".
fn choice_label(key: &str, value: &Value) -> String {
    match value {
        Value::String(text) if !text.is_empty() => humanize(text),
        _ => shown(key, value),
    }
}

fn setting_path(key: &str) -> Value {
    json!({ "_tag": "Setting", "payload": { "key": key } })
}

/// How a list row changes its value: the values themselves, and Default.
fn list_interaction(
    key: &str,
    leaf: Option<&Leaf>,
    value: &Value,
) -> (SurfaceSettingInteraction, Encoder, String) {
    let default = || choice(DEFAULT, "Default");
    if let Some(values) = leaf
        .and_then(|leaf| leaf.choices.clone())
        .filter(|values| !values.is_empty())
    {
        let mut choices = vec![default()];
        choices.extend(
            values
                .iter()
                .map(|value| choice(choice_value(value), choice_label(key, value))),
        );
        let selected = if value.is_null() {
            DEFAULT.to_owned()
        } else {
            choice_value(value)
        };
        let text = if value.is_null() {
            "Default".to_owned()
        } else {
            choice_label(key, value)
        };
        return (
            choice_interaction(choices, selected),
            Encoder::Enum(values),
            text,
        );
    }
    let kind = leaf
        .and_then(|leaf| leaf.kind.clone())
        .unwrap_or_else(|| match value {
            Value::Bool(_) => "boolean".into(),
            Value::Number(_) => "number".into(),
            _ => "string".into(),
        });
    match kind.as_str() {
        "boolean" => (
            choice_interaction(
                vec![default(), choice("true", "On"), choice("false", "Off")],
                match value {
                    Value::Bool(on) => on.to_string(),
                    _ => DEFAULT.to_owned(),
                },
            ),
            Encoder::Boolean,
            shown(key, value),
        ),
        "number" | "integer" => {
            let integer = kind == "integer";
            let minimum = leaf.and_then(|leaf| leaf.minimum);
            let maximum = leaf.and_then(|leaf| leaf.maximum);
            (
                SurfaceSettingInteraction::Number {
                    value: value.as_f64(),
                    min: minimum,
                    max: maximum,
                    step: integer.then_some(1.0),
                    clearable: Some(true),
                },
                Encoder::Number {
                    integer,
                    minimum,
                    maximum,
                },
                shown(key, value),
            )
        }
        _ => (
            SurfaceSettingInteraction::Text {
                placeholder: Some("Default".into()),
                max_length: None,
                allow_empty: Some(true),
            },
            Encoder::Text,
            shown(key, value),
        ),
    }
}

struct Builder<'a> {
    input: &'a ListInput<'a>,
    list: LaunchList,
}

/// One row before the list takes it.
struct RowSpec<'a> {
    id: String,
    label: String,
    description: Option<String>,
    /// The setting key, for the value's unit.
    key: &'a str,
    leaf: Option<&'a Leaf>,
    stored: Value,
    path: Value,
    runner: Option<String>,
    /// The value stays with the device that runs the game.
    device: bool,
    /// Why this row cannot change, when it cannot.
    fixed: Option<String>,
}

impl Builder<'_> {
    fn change_of(&self, id: &str) -> Option<&Change> {
        self.input.changes.iter().find(|change| change.row == id)
    }

    /// The row's value now: the change not kept yet, or else the stored one.
    fn mark(&mut self, id: &str, stored: &Value, text: &dyn Fn(&Value) -> String) -> Value {
        match self.change_of(id).map(|change| change.value.clone()) {
            Some(value) if value != *stored => {
                self.list.changes.push(SurfaceLaunchSettingsChange {
                    id: id.into(),
                    was: text(stored),
                });
                value
            }
            _ => stored.clone(),
        }
    }

    fn finish(
        &mut self,
        id: String,
        mut item: SurfaceSettingItem,
        row: ListRow,
        device: bool,
        fixed: Option<String>,
    ) -> SurfaceSettingItem {
        item.scope = Some(if device {
            SurfaceSettingScope::Device
        } else {
            SurfaceSettingScope::Shared
        });
        if device {
            item.source = Some(self.input.stays());
        }
        match fixed.or_else(|| self.input.unreachable()) {
            // A row that cannot change says why and takes no input.
            Some(reason) => {
                item.interaction = None;
                item.description = Some(reason);
            }
            None => {
                self.list.rows.insert(id, row);
            }
        }
        item
    }

    fn setting(&mut self, spec: RowSpec) -> SurfaceSettingItem {
        let key = spec.key;
        let leaf = spec.leaf;
        let value = self.mark(&spec.id, &spec.stored, &|value| {
            list_interaction(key, leaf, value).2
        });
        let (interaction, encoder, text) = list_interaction(key, leaf, &value);
        let item = SurfaceSettingItem {
            value: Some(text),
            description: spec.description,
            interaction: Some(interaction),
            ..SurfaceSettingItem::new(spec.id.clone(), spec.label)
        };
        self.finish(
            spec.id,
            item,
            ListRow {
                path: spec.path,
                side: Side::Holder {
                    runner: spec.runner,
                },
                stored: spec.stored,
                encoder,
            },
            spec.device,
            spec.fixed,
        )
    }

    /// A row with a closed set of values that are not a plugin's: a clock
    /// limit, the governor, a modifier's switch.
    #[allow(clippy::too_many_arguments)]
    fn fixed_choice(
        &mut self,
        id: String,
        label: &str,
        description: Option<String>,
        choices: Vec<SurfaceSettingChoice>,
        stored: Value,
        path: Value,
        encoder: Encoder,
        device: bool,
    ) -> SurfaceSettingItem {
        let key_of = |value: &Value| match value {
            Value::Number(number) => number_text(number.as_f64().unwrap_or(0.0)),
            Value::Bool(on) => on.to_string(),
            other => value_text(other),
        };
        let text = |value: &Value| {
            let key = key_of(value);
            choices
                .iter()
                .find(|choice| choice.value == key)
                .map_or_else(|| value_text(value), |choice| choice.label.clone())
        };
        let value = self.mark(&id, &stored, &text);
        let mut offered = choices.clone();
        if !offered.iter().any(|choice| choice.value == key_of(&value)) {
            offered.push(choice(key_of(&value), text(&value)));
        }
        let item = SurfaceSettingItem {
            value: Some(text(&value)),
            description,
            interaction: Some(choice_interaction(offered, key_of(&value))),
            ..SurfaceSettingItem::new(id.clone(), label)
        };
        self.finish(
            id,
            item,
            ListRow {
                path,
                side: Side::Holder { runner: None },
                stored,
                encoder,
            },
            device,
            None,
        )
    }
}

fn resolved_value(resolved: Option<&Value>) -> Value {
    resolved.map_or(Value::Null, |resolved| field(resolved, "value").clone())
}

/// A plugin's settings as groups by section: one row for each setting.
fn plugin_groups(
    builder: &mut Builder,
    context: &Context,
    sections: &[Section],
    title: &str,
    kind: OwnerKind,
    plugin: Option<&str>,
    runner_title: Option<&str>,
) -> Vec<SurfaceSettingGroup> {
    let owner_path = |key: &str| match kind {
        OwnerKind::Runner => setting_path(key),
        OwnerKind::Modifier => {
            json!({ "_tag": "ModifierSetting", "payload": { "id": plugin, "key": key } })
        }
        OwnerKind::Service => {
            json!({ "_tag": "ServiceSetting", "payload": { "id": plugin, "key": key } })
        }
    };
    // A runner-only row is its runner's own row: another runner's setting of
    // the same key is another row.
    let row_path = |key: &str, runner_only: bool| match kind {
        OwnerKind::Runner if runner_only => {
            format!("runner:{}:setting:{key}", plugin.unwrap_or(""))
        }
        OwnerKind::Runner => format!("setting:{key}"),
        OwnerKind::Modifier => format!("modifier:{}:setting:{key}", plugin.unwrap_or("")),
        OwnerKind::Service => format!("service:{}:setting:{key}", plugin.unwrap_or("")),
    };
    // Pattern settings show the keys in use, and the next free one.
    let wanted = match kind {
        OwnerKind::Runner => "Setting",
        OwnerKind::Modifier => "ModifierSetting",
        OwnerKind::Service => "ServiceSetting",
    };
    let mut used: Vec<String> = record(field(field(context.snapshot, "resolved"), "values"))
        .into_iter()
        .flat_map(|values| values.values())
        .filter_map(|value| {
            let path = field(field(value, "path"), "payload");
            let payload = field(path, "payload");
            (tag(path) == wanted && (kind == OwnerKind::Runner || str_of(payload, "id") == plugin))
                .then(|| str_of(payload, "key").map(str::to_owned))
                .flatten()
        })
        .collect();
    used.sort();
    let mut groups = Vec::new();
    for section in sections {
        let mut items = Vec::new();
        for leaf in section_leaves(section, &used) {
            let path = owner_path(&leaf.path);
            let skip = context.skip(&path, None).cloned();
            let resolved = context.resolved(&path);
            // ADR 0057: a shared setting this runner cannot honour shows only
            // when a value waits on it, and then it cannot change here.
            let fixed = if leaf.supported {
                None
            } else {
                if skip.is_none() && resolved.is_none() {
                    continue;
                }
                Some(format!(
                    "Not supported by {}.",
                    runner_title.unwrap_or(title)
                ))
            };
            let description = match (leaf.description.clone(), skip.filter(|_| leaf.supported)) {
                (said, None) => said,
                (None, Some(skip)) => Some(format!("Not applied: {}.", skip.trim_end_matches('.'))),
                (Some(said), Some(skip)) => Some(format!(
                    "{said} Not applied: {}.",
                    skip.trim_end_matches('.')
                )),
            };
            let runner_only = kind == OwnerKind::Runner && section.key == "plugin";
            let item = builder.setting(RowSpec {
                id: launch_id::row(&row_path(&leaf.path, runner_only)),
                // The value carries the unit: "Volume", "80%".
                label: leaf
                    .label
                    .strip_suffix(" percent")
                    .filter(|_| leaf.path.ends_with("-percent"))
                    .unwrap_or(&leaf.label)
                    .to_owned(),
                description,
                key: &leaf.path,
                leaf: Some(&leaf),
                stored: resolved_value(resolved),
                path,
                runner: runner_only.then(|| plugin.unwrap_or("").to_owned()),
                // A modifier's and a service's values describe one device.
                device: leaf.device || kind != OwnerKind::Runner,
                fixed,
            });
            items.push(item);
        }
        if items.is_empty() {
            continue;
        }
        groups.push(SurfaceSettingGroup {
            title: match kind {
                OwnerKind::Runner if section.key == "plugin" => format!("{title} only"),
                OwnerKind::Runner if !section.key.is_empty() => section.title.clone(),
                _ if section.key.is_empty() => title.to_owned(),
                _ => format!("{title} · {}", section.title),
            },
            items,
        });
    }
    groups
}

/// CPU and GPU limits and the governor, as the device that runs the game has them.
fn performance_group(builder: &mut Builder, context: &Context) -> SurfaceSettingGroup {
    let percent = |builder: &mut Builder, key: &str| {
        let (kind, bound) = key.split_once('.').unwrap_or((key, ""));
        let steps = if kind == "cpu" {
            &context.cpu
        } else {
            &context.gpu
        };
        let mut choices = if bound == "min" {
            vec![choice("0", "0 % (no floor)")]
        } else {
            Vec::new()
        };
        choices.extend(steps.choices.iter().cloned());
        let stored = resolved_value(context.resolved(&limit_path(key)));
        if let Some(own) = percent_of(&stored) {
            if !choices
                .iter()
                .any(|choice| choice.value == number_text(own))
            {
                choices.push(choice(number_text(own), steps.label_of(own)));
                choices.sort_by(|a, b| {
                    js_number(&a.value)
                        .unwrap_or(0.0)
                        .partial_cmp(&js_number(&b.value).unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }
        builder.fixed_choice(
            launch_id::row(key),
            &format!("{} {bound}", kind.to_uppercase()),
            steps
                .guessed
                .then(|| "The clock steps of this device are not known.".to_owned()),
            choices,
            stored,
            limit_path(key),
            Encoder::Percent,
            true,
        )
    };
    let mut items = vec![percent(builder, "cpu.min"), percent(builder, "cpu.max")];
    let governor = resolved_value(context.resolved(&limit_path("cpu.governor")));
    if !context.governors.is_empty() || governor.is_string() {
        items.push(
            builder.fixed_choice(
                launch_id::row("cpu.governor"),
                "CPU governor",
                Some(
                    "One value for the whole device. Kept for every game, or for this launch."
                        .into(),
                ),
                context
                    .governors
                    .iter()
                    .map(|governor| choice(governor.clone(), governor.clone()))
                    .collect(),
                governor,
                limit_path("cpu.governor"),
                Encoder::TextOrInherit,
                true,
            ),
        );
    }
    if context.gpu.unavailable {
        let mut item = SurfaceSettingItem::new(launch_id::row("gpu"), "GPU limits");
        item.description = Some("This device has no GPU limits.".into());
        item.scope = Some(SurfaceSettingScope::Device);
        // `source` is the only text that says where a value stays.
        item.source = Some(builder.input.stays());
        items.push(item);
    } else {
        items.push(percent(builder, "gpu.min"));
        items.push(percent(builder, "gpu.max"));
    }
    SurfaceSettingGroup {
        title: "Performance".into(),
        items,
    }
}

/// The runner, and each modifier's switch.
fn runs_with_group(builder: &mut Builder, context: &Context) -> SurfaceSettingGroup {
    let choosable = array(context.snapshot, "runners")
        .iter()
        .filter(|runner| field(runner, "choosable") == &json!(true))
        .count();
    let mut runner = SurfaceSettingItem::new(launch_id::row("runner"), "Runner");
    runner.value = Some(match context.selected {
        Some(selected) => context.runner_title(selected),
        None => "Not chosen".into(),
    });
    runner.scope = Some(SurfaceSettingScope::Shared);
    match builder.input.unreachable() {
        Some(reason) => runner.description = Some(reason),
        None if builder.input.can_choose_runner => {
            runner.interaction = Some(action_interaction(launch_id::RUNNER.into(), None, None));
            if context.selected.is_none() {
                runner.description = Some("Choose a runner to see its settings.".into());
            }
        }
        None if choosable == 1 => {
            runner.description = Some("This game has one runner.".into());
        }
        None => {}
    }
    let mut items = vec![runner];
    for modifier in array(context.snapshot, "modifiers") {
        let id = str_of(modifier, "id").unwrap_or("");
        let path = json!({ "_tag": "ModifierEnabled", "payload": { "id": id } });
        let stored = match resolved_value(context.resolved(&path)) {
            Value::Bool(on) => json!(on),
            _ => json!(false),
        };
        items.push(builder.fixed_choice(
            launch_id::row(&format!("modifier:{id}:enabled")),
            str_of(modifier, "title").unwrap_or(id),
            None,
            vec![choice("true", "On"), choice("false", "Off")],
            stored,
            path,
            Encoder::Toggle,
            true,
        ));
    }
    SurfaceSettingGroup {
        title: "Runs with".into(),
        items,
    }
}

/// This device's own stream viewer settings. They describe the playing
/// device, so they stay with it: a change waits like any other row's, and is
/// then kept on this device whatever place the person chose.
fn stream_group(builder: &mut Builder, viewer: &Value) -> Option<SurfaceSettingGroup> {
    let context = Context::new(viewer, None);
    let mut items = Vec::new();
    for runner in array(viewer, "runners")
        .iter()
        .filter(|runner| field(runner, "choosable") != &json!(true))
    {
        let runner_id = str_of(runner, "id").unwrap_or("");
        for section in sections_of(field(runner, "schema")) {
            for leaf in &section.leaves {
                let id = launch_id::stream(runner_id, &leaf.path);
                let path = setting_path(&leaf.path);
                let stored = resolved_value(context.stream_resolved(runner_id, &path));
                let value = builder.mark(&id, &stored, &|value| {
                    list_interaction(&leaf.path, Some(leaf), value).2
                });
                let (interaction, encoder, text) = list_interaction(&leaf.path, Some(leaf), &value);
                let said = "Always kept on this device.";
                items.push(SurfaceSettingItem {
                    value: Some(text),
                    description: Some(match &leaf.description {
                        Some(description) => format!("{description} {said}"),
                        None => said.into(),
                    }),
                    source: Some("Stays on this device".into()),
                    scope: Some(SurfaceSettingScope::Device),
                    interaction: Some(interaction),
                    ..SurfaceSettingItem::new(id.clone(), leaf.label.clone())
                });
                builder.list.rows.insert(
                    id,
                    ListRow {
                        path,
                        side: Side::Viewer {
                            runner: runner_id.to_owned(),
                        },
                        stored,
                        encoder,
                    },
                );
            }
        }
    }
    (!items.is_empty()).then(|| SurfaceSettingGroup {
        title: "Stream".into(),
        items,
    })
}

fn keep_title(context: &Context, layer: &Value) -> (String, String) {
    match tag(layer) {
        "Game" => (
            "This game".into(),
            "Used every time you play this game.".into(),
        ),
        "Device" => (
            "Every game".into(),
            "Used by every game that has no value of its own.".into(),
        ),
        "System" => {
            let title = context
                .places
                .iter()
                .find(|entry| layer_key(field(field(entry, "place"), "layer")) == layer_key(layer))
                .and_then(|entry| str_of(entry, "title"))
                .unwrap_or_else(|| str_of(layer, "id").unwrap_or(""))
                .to_owned();
            (
                format!("System {title}"),
                format!("Used by every {title} game that has no value of its own."),
            )
        }
        "Release" => (
            layer_title(context, layer),
            "Used every time you play this release.".into(),
        ),
        _ => (
            layer_title(context, layer),
            "Used when this profile is picked.".into(),
        ),
    }
}

/// The one keep control: this launch, this game, every game, then the
/// system, the release and each profile.
fn keep_row(builder: &mut Builder, context: Option<&Context>) {
    let mut places: Vec<(String, String, String, Option<Value>)> = vec![(
        KEEP_LAUNCH.into(),
        "This launch".into(),
        "They end when you stop playing.".into(),
        None,
    )];
    if let Some(context) = context {
        let mut layers: Vec<&Value> = Vec::new();
        for entry in &context.places {
            let place = field(entry, "place");
            let layer = field(place, "layer");
            if tag(field(place, "scope")) == "Any" && !layers.contains(&layer) {
                layers.push(layer);
            }
        }
        let rank = |layer: &Value| match tag(layer) {
            "Game" => 0,
            "Device" => 1,
            "System" => 2,
            "Release" => 3,
            _ => 4,
        };
        layers.sort_by_key(|layer| rank(layer));
        for layer in layers {
            let (title, note) = keep_title(context, layer);
            places.push((layer_key(layer), title, note, Some(layer.clone())));
        }
    }
    let default = places
        .iter()
        .find(|(_, _, _, layer)| layer.as_ref().is_some_and(|layer| tag(layer) == "Game"))
        .map_or(KEEP_LAUNCH.to_owned(), |(id, ..)| id.clone());
    let selected = builder
        .input
        .keep
        .filter(|keep| places.iter().any(|(id, ..)| id == keep))
        .map_or(default, str::to_owned);
    builder.list.keep_selected = selected.clone();
    builder.list.keep_places = places
        .iter()
        .map(|(id, _, _, layer)| {
            (
                id.clone(),
                KeepPlace {
                    layer: layer.clone(),
                },
            )
        })
        .collect();
    let count = builder.list.changes.len();
    if count == 0 {
        return;
    }
    // A stream value is kept on this device whatever place is chosen. For
    // this launch that is not what the place's own words say.
    let stream = builder
        .list
        .changes
        .iter()
        .filter(|change| {
            builder
                .list
                .rows
                .get(&change.id)
                .is_some_and(|row| matches!(row.side, Side::Viewer { .. }))
        })
        .count();
    const STREAM_STAYS: &str = "Stream values stay on this device.";
    let chosen = places.iter().find(|(id, ..)| *id == selected);
    builder.list.keep = Some(SurfaceSettingItem {
        value: chosen.map(|(_, title, ..)| title.clone()),
        description: chosen.map(|(_, _, note, layer)| match layer {
            None if stream == count => STREAM_STAYS.into(),
            None if stream > 0 => format!("{note} {STREAM_STAYS}"),
            _ => note.clone(),
        }),
        interaction: Some(choice_interaction(
            places
                .iter()
                .map(|(id, title, ..)| choice(id.clone(), title.clone()))
                .collect(),
            selected,
        )),
        ..SurfaceSettingItem::new(
            launch_id::KEEP,
            format!(
                "Keep {count} {}",
                if count == 1 { "change" } else { "changes" }
            ),
        )
    });
}

/// The selected profile's name and its removal, then the pick and New profile.
fn profile_group(context: &Context) -> SurfaceSettingGroup {
    let mut group = profile_rows(context, None);
    if let Some(selected) = str_of(context.snapshot, "selectedProfile") {
        group
            .items
            .extend(profile_rows(context, Some(selected)).items);
    }
    group
}

/// The runner the game has chosen now, and every runner it offers, by id.
pub(crate) fn runners_of(snapshot: &Value) -> (Option<&str>, Vec<&str>) {
    (
        selected_runner(snapshot).and_then(|runner| str_of(runner, "id")),
        array(snapshot, "runners")
            .iter()
            .filter_map(|runner| str_of(runner, "id"))
            .collect(),
    )
}

pub(crate) fn launch_list(input: &ListInput) -> LaunchList {
    let mut builder = Builder {
        input,
        list: LaunchList::default(),
    };
    let context = input
        .snapshot
        .map(|snapshot| Context::new(snapshot, Some(input.game_id)));
    let mut groups = Vec::new();
    let mut tail = Vec::new();
    if let Some(context) = &context {
        if let Some(selected) = context.selected {
            let title = context.runner_title(selected);
            groups.extend(plugin_groups(
                &mut builder,
                context,
                &sections_of(field(selected, "schema")),
                &title,
                OwnerKind::Runner,
                str_of(selected, "id"),
                Some(&title),
            ));
        }
        tail.push(performance_group(&mut builder, context));
        tail.push(runs_with_group(&mut builder, context));
        for (kind, listed) in [
            (OwnerKind::Modifier, "modifiers"),
            (OwnerKind::Service, "services"),
        ] {
            for plugin in array(context.snapshot, listed) {
                let title = str_of(plugin, "title").unwrap_or("");
                tail.extend(plugin_groups(
                    &mut builder,
                    context,
                    &sections_of(field(plugin, "schema")),
                    title,
                    kind,
                    str_of(plugin, "id"),
                    None,
                ));
            }
        }
        if input.reachable {
            tail.push(profile_group(context));
        }
        builder.list.notes = launch_notes_from(context.snapshot, Some(input.game_id));
    }
    if let Some(stream) = input
        .viewer
        .filter(|_| input.holder.is_some())
        .and_then(|viewer| stream_group(&mut builder, viewer))
    {
        groups.push(stream);
    }
    groups.extend(tail);
    keep_row(&mut builder, context.as_ref());
    builder.list.groups = groups;
    builder.list
}
