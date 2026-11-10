//! Projects transient drafts through the existing launch editor treaty.
use super::{
    launch_model::{launch_editor_from, place_id_of, LaunchEditor, PendingSetting},
    one_launch::OneLaunch,
};
use crate::{config::cascade::leaves, launch_settings::LaunchRunnerScope};
use korri_surface_treaty::{
    SurfaceSettingChoice, SurfaceSettingGroup, SurfaceSettingInteraction, SurfaceSettingItem,
};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone)]
pub(crate) struct Keep {
    pub path: String,
    pub value: Value,
    pub targets: HashMap<String, Value>,
}

pub(crate) fn editor(
    snapshot: &Value,
    game: Option<&str>,
    pending: &HashMap<String, Vec<PendingSetting>>,
    drafts: &OneLaunch,
) -> (LaunchEditor, HashMap<String, Keep>) {
    let mut projected = snapshot.clone();
    let mut keeps = HashMap::new();
    let Some(game) = game else {
        return (launch_editor_from(snapshot, None, pending), keeps);
    };
    let mut scopes = vec![json!(LaunchRunnerScope::Any)];
    for entry in snapshot
        .get("places")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let scope = &entry["place"]["scope"];
        if !scopes.contains(scope) {
            scopes.push(scope.clone());
        }
    }
    if let Some(places) = projected.get_mut("places").and_then(Value::as_array_mut) {
        for scope in scopes {
            let settings = drafts
                .pending(game)
                .filter(|draft| json!(draft.scope) == scope)
                .map(|draft| json!(draft.values))
                .unwrap_or_else(|| json!({}));
            places.push(json!({"place":{"layer":{"_tag":"Launch"},"scope":scope},"title":"This launch","body":{"settings":settings},"editableKinds":["settings"]}));
        }
    }
    let mut editor = launch_editor_from(&projected, Some(game), pending);
    if let Some(draft) = drafts.keepable(game) {
        let mut items = Vec::new();
        let mut values = leaves(&draft.values).0;
        values.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, value) in values {
            let shared = korri_settings_treaty::SharedSetting::from_path(&path).is_some();
            let mut targets: Vec<_> = snapshot
                .get("places")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|entry| {
                    (shared || entry["place"]["scope"] == json!(draft.scope))
                        && entry["place"]["layer"]["_tag"] != "Launch"
                })
                .collect();
            targets.sort_by_key(|entry| {
                if entry["place"]["layer"]["_tag"] == "Game" {
                    0
                } else {
                    1
                }
            });
            let id = format!("launch:keep:{path}");
            let choices: Vec<_> = targets
                .iter()
                .map(|entry| SurfaceSettingChoice {
                    value: place_id_of(&entry["place"]),
                    label: entry
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("Place")
                        .into(),
                })
                .collect();
            if choices.is_empty() {
                continue;
            }
            keeps.insert(
                id.clone(),
                Keep {
                    path: path.clone(),
                    value: json!(value),
                    targets: targets
                        .iter()
                        .map(|entry| (place_id_of(&entry["place"]), entry["place"].clone()))
                        .collect(),
                },
            );
            let source_place = place_id_of(&json!({"layer":{"_tag":"Launch"},"scope":draft.scope}));
            let label = editor
                .places
                .iter()
                .find(|place| place.id == source_place)
                .into_iter()
                .flat_map(|place| &place.groups)
                .flat_map(|group| &group.items)
                .find_map(|item| {
                    if editor
                        .rows
                        .writes
                        .get(&item.id)
                        .is_some_and(|write| write.path["payload"]["key"] == path)
                    {
                        return Some(item.label.clone());
                    }
                    if let Some(SurfaceSettingInteraction::Choice { choices, .. }) =
                        &item.interaction
                    {
                        return choices
                            .iter()
                            .find(|choice| choice.value == path)
                            .map(|choice| choice.label.clone());
                    }
                    None
                })
                .unwrap_or_else(|| {
                    path.split('.')
                        .map(super::launch_model::humanize)
                        .collect::<Vec<_>>()
                        .join(" / ")
                });
            let mut shown = if json!(value).is_null() {
                "Default".into()
            } else {
                super::launch_model::value_text(&json!(value))
            };
            if korri_settings_treaty::SharedSetting::from_path(&path)
                == Some(korri_settings_treaty::SharedSetting::RenderScale)
                && !json!(value).is_null()
            {
                shown.push('x');
            }
            items.push(SurfaceSettingItem {
                value: Some(if json!(value).is_null() {
                    "Default".into()
                } else {
                    super::launch_model::value_text(&json!(value))
                }),
                description: Some(format!("{label}: {shown}. Choose where to keep it.")),
                interaction: Some(SurfaceSettingInteraction::Choice {
                    choices,
                    selected: String::new(),
                }),
                ..SurfaceSettingItem::new(id, format!("Keep {label}"))
            });
        }
        if let Some(place) = editor.places.iter_mut().find(|place| {
            place.id == place_id_of(&json!({"layer":{"_tag":"Launch"},"scope":draft.scope}))
        }) {
            if !items.is_empty() {
                place.groups.push(SurfaceSettingGroup {
                    title: "Keep one-launch values".into(),
                    items,
                });
            }
        }
    }
    (editor, keeps)
}
