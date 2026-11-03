//! The view boundary: one `view()`/`overlay()` JSON object becomes the
//! component's `in` properties.
//!
//! | View JSON | Slint value |
//! | --- | --- |
//! | key `fooBar` | property or field `foo-bar` |
//! | `{"$image": url}`, `{"$image": url, "pico8": mode}` | `image` (contract 3.8) |
//! | other object | struct |
//! | array | model |
//! | string, number, boolean | string, int or float by the declared type, bool |
//! | `null` or a missing key | the property or field keeps its value |
//! | `focusPlan` (top level), `focusRegion`, `focusActive` (in a focus target) | not bound: the focus plan for the probe (contract 8.4) |
//!
//! Conversion follows the declared type (`i_slint_compiler::langtype::Type`,
//! from the pinned interpreter's `internal` metadata), so `int` and `float`
//! stay distinct and every unknown key or wrong type is an error. The
//! install-time verifier uses the same conversion, so a view that passes
//! verification converts the same way at run time.

use crate::{images::ImageCache, pico8};
use anyhow::{bail, Context, Result};
use i_slint_compiler::langtype::Type;
use serde_json::Value as Json;
use slint::{Model, ModelRc, VecModel};
use slint_interpreter::{ComponentDefinition, ComponentInstance, Struct, Value};
use std::collections::{BTreeMap, HashMap};

/// Contract 8.4, the shell's convention: the region of a focus target (an
/// entry with `focused`).
pub const FOCUS_REGION: &str = "focusRegion";
/// The region's active target (for example the active tab).
pub const FOCUS_ACTIVE: &str = "focusActive";
/// Top level: `{ region: { direction: { to: region, entry: rule } } }`.
pub const FOCUS_PLAN: &str = "focusPlan";
/// The entry rules an exit may name.
pub const ENTRY_RULES: [&str; 3] = ["remembered", "active", "first"];
pub const DIRECTIONS: [&str; 4] = ["up", "down", "left", "right"];

/// The focus plan's keys are never bound to Slint.
fn reserved(name: &str, top: bool) -> bool {
    (top && name == FOCUS_PLAN) || (!top && (name == FOCUS_REGION || name == FOCUS_ACTIVE))
}

/// Check a view's focus plan (contract 8.4): every focus target names its
/// region, at most one target per region is active, and every exit names a
/// direction, a region with targets in this view, and an entry rule.
pub fn check_focus_plan(view: &Json) -> Result<()> {
    fn walk(value: &Json, path: &str, regions: &mut BTreeMap<String, usize>) -> Result<()> {
        match value {
            Json::Object(map) => {
                if map.contains_key("focused") {
                    let region = map
                        .get(FOCUS_REGION)
                        .and_then(Json::as_str)
                        .filter(|r| !r.is_empty())
                        .with_context(|| format!("{path}: a focus target needs {FOCUS_REGION}"))?;
                    let active = match map.get(FOCUS_ACTIVE) {
                        None | Some(Json::Null) | Some(Json::Bool(false)) => 0,
                        Some(Json::Bool(true)) => 1,
                        Some(_) => bail!("{path}: {FOCUS_ACTIVE} must be a boolean"),
                    };
                    let count = regions.entry(region.to_owned()).or_default();
                    *count += active;
                    if *count > 1 {
                        bail!("{path}: region {region:?} has more than one {FOCUS_ACTIVE} target");
                    }
                }
                for (k, v) in map {
                    if k != FOCUS_PLAN {
                        walk(v, &format!("{path}.{k}"), regions)?;
                    }
                }
            }
            Json::Array(items) => {
                for (i, v) in items.iter().enumerate() {
                    walk(v, &format!("{path}[{i}]"), regions)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut regions = BTreeMap::new();
    walk(view, "view", &mut regions)?;
    let Some(plan) = view.get(FOCUS_PLAN).filter(|p| !p.is_null()) else {
        return Ok(());
    };
    for (region, exits) in plan
        .as_object()
        .with_context(|| format!("{FOCUS_PLAN} must be an object"))?
    {
        for (direction, exit) in exits
            .as_object()
            .with_context(|| format!("{FOCUS_PLAN}.{region} must be an object"))?
        {
            if !DIRECTIONS.contains(&direction.as_str()) {
                bail!("{FOCUS_PLAN}.{region}: {direction:?} is not up, down, left or right");
            }
            let to = exit
                .get("to")
                .and_then(Json::as_str)
                .with_context(|| format!("{FOCUS_PLAN}.{region}.{direction}: needs \"to\""))?;
            if !regions.contains_key(to) {
                bail!("{FOCUS_PLAN}.{region}.{direction}: region {to:?} has no focus target in this view");
            }
            let entry = exit.get("entry").and_then(Json::as_str).unwrap_or_default();
            if !ENTRY_RULES.contains(&entry) {
                bail!(
                    "{FOCUS_PLAN}.{region}.{direction}: entry must be remembered, active or first"
                );
            }
            if let Some(key) = exit
                .as_object()
                .and_then(|o| o.keys().find(|k| *k != "to" && *k != "entry"))
            {
                bail!("{FOCUS_PLAN}.{region}.{direction}: unknown key {key:?}");
            }
        }
    }
    Ok(())
}

/// `fooBar` → `foo-bar`.
pub fn kebab(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            result.push('-');
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// The public `in` properties of a component and their declared types.
pub fn declared(definition: &ComponentDefinition) -> BTreeMap<String, Type> {
    let public: BTreeMap<_, _> = definition.properties().collect();
    definition
        .properties_and_callbacks()
        .filter(|(name, _)| public.contains_key(name))
        .map(|(name, (ty, _))| (name, ty))
        .collect()
}

fn image_request(
    fields: &serde_json::Map<String, Json>,
) -> Result<Option<(&str, Option<pico8::Mode>)>> {
    let Some(url) = fields.get("$image") else {
        return Ok(None);
    };
    let url = url.as_str().context("$image must be a string")?;
    let mut mode = None;
    for (key, value) in fields {
        match key.as_str() {
            "$image" => {}
            "pico8" => {
                mode = Some(
                    value
                        .as_str()
                        .and_then(pico8::Mode::parse)
                        .context("pico8 must be \"flat\" or \"vivid\"")?,
                )
            }
            other => bail!("unknown image key {other}"),
        }
    }
    Ok(Some((url, mode)))
}

/// Convert one JSON value to the declared type. `previous` supplies values
/// for `null` and omitted struct fields.
pub fn convert(
    json: &Json,
    ty: &Type,
    previous: &Value,
    images: &mut ImageCache,
    path: &str,
) -> Result<Value> {
    if json.is_null() {
        return Ok(previous.clone());
    }
    Ok(match (json, ty) {
        (Json::Bool(v), Type::Bool) => Value::Bool(*v),
        (Json::String(v), Type::String) => Value::String(v.as_str().into()),
        (Json::Number(n), Type::Int32) => {
            let n = n.as_f64().context("number")?.trunc();
            if !(i32::MIN as f64..=i32::MAX as f64).contains(&n) {
                bail!("{path}: integer outside Slint int range");
            }
            Value::Number(n)
        }
        (Json::Number(n), Type::Float32) => {
            let n = n.as_f64().context("number")? as f32;
            if !n.is_finite() {
                bail!("{path}: number outside Slint float range");
            }
            Value::Number(n as f64)
        }
        (Json::Array(items), Type::Array(item)) => {
            let old = match previous {
                Value::Model(model) => Some(model),
                _ => None,
            };
            let values = items
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let prior = old.and_then(|m| m.row_data(i)).unwrap_or_default();
                    convert(v, item, &prior, images, &format!("{path}[{i}]"))
                })
                .collect::<Result<Vec<_>>>()?;
            Value::Model(ModelRc::new(VecModel::from(values)))
        }
        (Json::Object(fields), Type::Image) => {
            let (url, mode) =
                image_request(fields)?.with_context(|| format!("{path}: image needs $image"))?;
            Value::Image(
                images
                    .get(url, mode)
                    .with_context(|| format!("{path}: image decode"))?,
            )
        }
        (Json::Object(fields), Type::Struct(declared)) if !fields.contains_key("$image") => {
            let mut value = match previous {
                Value::Struct(s) => s.clone(),
                _ => Struct::default(),
            };
            for (name, json) in fields {
                if reserved(name, false) {
                    continue;
                }
                let field = kebab(name);
                let ty = declared
                    .fields
                    .get(field.as_str())
                    .with_context(|| format!("{path}: undeclared field {field}"))?;
                let prior = value.get_field(&field).cloned().unwrap_or_default();
                let converted = convert(json, ty, &prior, images, &format!("{path}.{field}"))?;
                value.set_field(field, converted);
            }
            // A declared field the object omits and that has no value yet
            // gets its type's default; Slint panics on a field with none.
            for (field, ty) in &declared.fields {
                if value.get_field(field).is_none_or(|v| *v == Value::Void) {
                    value.set_field(field.to_string(), default_value(ty));
                }
            }
            Value::Struct(value)
        }
        _ => bail!("{path}: wrong type, expected {ty}, got {}", short(json)),
    })
}

/// A type's default value, recursively for structs: what Slint gives a
/// declared field with no binding.
fn default_value(ty: &Type) -> Value {
    match ty {
        Type::Bool => Value::Bool(false),
        Type::String => Value::String(Default::default()),
        Type::Int32
        | Type::Float32
        | Type::Duration
        | Type::PhysicalLength
        | Type::LogicalLength
        | Type::Rem
        | Type::Angle
        | Type::Percent
        | Type::UnitProduct(_) => Value::Number(0.0),
        Type::Array(_) => Value::Model(ModelRc::new(VecModel::<Value>::default())),
        Type::Image => Value::Image(Default::default()),
        Type::Color => Value::Brush(Default::default()),
        Type::Brush => Value::Brush(Default::default()),
        Type::Struct(declared) => {
            let mut value = Struct::default();
            for (field, ty) in &declared.fields {
                value.set_field(field.to_string(), default_value(ty));
            }
            Value::Struct(value)
        }
        _ => Value::Void,
    }
}

fn short(json: &Json) -> String {
    let text = json.to_string();
    if text.len() > 80 {
        format!(
            "{}…",
            &text[..text.char_indices().nth(80).map_or(text.len(), |(i, _)| i)]
        )
    } else {
        text
    }
}

/// Check a whole view against declared properties without a component.
pub fn check(view: &Json, types: &BTreeMap<String, Type>, images: &mut ImageCache) -> Result<()> {
    for (name, value) in view.as_object().context("view must return an object")? {
        if reserved(name, true) {
            continue;
        }
        let property = kebab(name);
        let ty = types
            .get(&property)
            .with_context(|| format!("undeclared property {property}"))?;
        convert(value, ty, &Value::Void, images, &property)?;
    }
    Ok(())
}

/// Applies views to one component instance. It remembers the last JSON of
/// each property and leaves unchanged properties alone, so models are not
/// rebuilt and repeaters not re-instantiated when nothing changed.
pub struct Binder {
    types: BTreeMap<String, Type>,
    last: HashMap<String, Json>,
    /// The last `apply`'s time converting JSON to Slint values, and setting them.
    pub timing: (std::time::Duration, std::time::Duration),
}

impl Binder {
    pub fn new(definition: &ComponentDefinition) -> Self {
        Self {
            types: declared(definition),
            last: HashMap::new(),
            timing: Default::default(),
        }
    }

    pub fn apply(
        &mut self,
        component: &ComponentInstance,
        view: &Json,
        images: &mut ImageCache,
    ) -> Result<usize> {
        let fields = view.as_object().context("view must return an object")?;
        let mut changed = 0;
        self.timing = Default::default();
        for (name, json) in fields {
            if json.is_null() || reserved(name, true) {
                continue;
            }
            let property = kebab(name);
            if self.last.get(&property) == Some(json) {
                continue;
            }
            let ty = self
                .types
                .get(&property)
                .with_context(|| format!("undeclared property {property}"))?;
            let started = std::time::Instant::now();
            let previous = component.get_property(&property)?;
            let value = convert(json, ty, &previous, images, &property)?;
            let converted = std::time::Instant::now();
            component
                .set_property(&property, value)
                .with_context(|| format!("set {property}"))?;
            self.timing.0 += converted - started;
            self.timing.1 += converted.elapsed();
            self.last.insert(property, json.clone());
            changed += 1;
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn compile(source: &str) -> ComponentDefinition {
        let result = pollster::block_on(
            slint_interpreter::Compiler::default()
                .build_from_source(source.into(), Default::default()),
        );
        assert!(
            !result.has_errors(),
            "{:?}",
            result.diagnostics().collect::<Vec<_>>()
        );
        result.component("Surface").expect("Surface")
    }

    const SOURCE: &str = r#"
        export struct Item { item-count: int, fraction: float, label: string, art: image }
        export component Surface inherits Window {
            in property <int> count;
            in property <float> fraction;
            in property <[Item]> items;
            in property <image> art;
            in property <bool> lit;
        }"#;

    #[test]
    fn the_focus_plan_is_checked_and_never_bound() -> Result<()> {
        let source = r#"
            export struct Row { label: string, focused: bool }
            export component Surface inherits Window { in property <[Row]> rows; in property <[Row]> tabs; }"#;
        let definition = compile(source);
        let dir = tempfile::tempdir()?;
        let mut images = ImageCache::new(dir.path());
        let view = json!({
            "tabs": [
                {"label": "Games", "focused": false, "focusRegion": "tabs", "focusActive": true},
                {"label": "Options", "focused": false, "focusRegion": "tabs"}
            ],
            "rows": [{"label": "A", "focused": true, "focusRegion": "rows"}],
            "focusPlan": {
                "rows": {"up": {"to": "tabs", "entry": "active"}},
                "tabs": {"down": {"to": "rows", "entry": "remembered"}}
            }
        });
        check_focus_plan(&view)?;
        check(&view, &declared(&definition), &mut images)?;
        crate::backend::headless::Headless::install();
        let instance = definition.create()?;
        Binder::new(&definition).apply(&instance, &view, &mut images)?;
        let refused = |change: &dyn Fn(&mut Json)| {
            let mut bad = view.clone();
            change(&mut bad);
            check_focus_plan(&bad).unwrap_err().to_string()
        };
        assert!(refused(&|v| {
            v["rows"][0].as_object_mut().unwrap().remove("focusRegion");
        })
        .contains("needs focusRegion"));
        assert!(refused(&|v| v["tabs"][1]["focusActive"] = json!(true)).contains("more than one"));
        assert!(
            refused(&|v| v["focusPlan"]["rows"]["up"]["to"] = json!("keys"))
                .contains("no focus target")
        );
        assert!(
            refused(&|v| v["focusPlan"]["rows"]["up"]["entry"] = json!("last"))
                .contains("entry must be")
        );
        assert!(refused(
            &|v| v["focusPlan"]["rows"]["north"] = json!({"to": "tabs", "entry": "first"})
        )
        .contains("not up, down"));
        Ok(())
    }

    #[test]
    fn omitted_struct_fields_get_their_type_default() -> Result<()> {
        let source = r#"
            export struct Inner { shown: bool, label: string }
            export struct Entry { label: string, focused: bool, size: int, inner: Inner, tags: [string] }
            export component Surface inherits Window {
                in property <[Entry]> entries;
                Text { text: root.entries[0].focused || root.entries[0].inner.shown ? "on" : root.entries[0].tags.length; }
                in property <string> heading: "kept";
            }"#;
        let definition = compile(source);
        let dir = tempfile::tempdir()?;
        let mut images = ImageCache::new(dir.path());
        crate::backend::headless::Headless::install();
        let instance = definition.create()?;
        // `focused`, `size`, `inner` and `tags` omitted; `heading` absent.
        let view = json!({"entries": [{"label": "Resume"}]});
        Binder::new(&definition).apply(&instance, &view, &mut images)?;
        let Value::Model(entries) = instance.get_property("entries")? else {
            panic!()
        };
        let Some(Value::Struct(entry)) = entries.row_data(0) else {
            panic!()
        };
        assert_eq!(entry.get_field("focused"), Some(&Value::Bool(false)));
        assert_eq!(entry.get_field("size"), Some(&Value::Number(0.0)));
        let Some(Value::Struct(inner)) = entry.get_field("inner") else {
            panic!()
        };
        assert_eq!(inner.get_field("shown"), Some(&Value::Bool(false)));
        assert_eq!(inner.get_field("label"), Some(&Value::String("".into())));
        // A top-level property the view omits keeps its value.
        assert_eq!(
            instance.get_property("heading")?,
            Value::String("kept".into())
        );
        Ok(())
    }

    #[test]
    fn kebab_follows_the_contract() {
        assert_eq!(kebab("focusedCard"), "focused-card");
        assert_eq!(kebab("hasArt"), "has-art");
    }

    #[test]
    fn converts_by_declared_type_including_nested_models() -> Result<()> {
        let types = declared(&compile(SOURCE));
        let dir = tempfile::tempdir()?;
        let mut images = ImageCache::new(dir.path());
        let value = convert(
            &json!(4.9),
            &types["count"],
            &Value::Void,
            &mut images,
            "count",
        )?;
        assert_eq!(value, Value::Number(4.0));
        let value = convert(
            &json!(4.5),
            &types["fraction"],
            &Value::Void,
            &mut images,
            "fraction",
        )?;
        assert_eq!(value, Value::Number(4.5));
        let items = convert(
            &json!([{"itemCount": 3.9, "fraction": 1.5}]),
            &types["items"],
            &Value::Void,
            &mut images,
            "items",
        )?;
        let Value::Model(items) = items else {
            panic!("model")
        };
        let Some(Value::Struct(item)) = items.row_data(0) else {
            panic!("struct")
        };
        assert_eq!(item.get_field("item-count"), Some(&Value::Number(3.0)));
        assert_eq!(item.get_field("fraction"), Some(&Value::Number(1.5)));
        Ok(())
    }

    #[test]
    fn rejects_unknown_keys_and_wrong_types() -> Result<()> {
        let types = declared(&compile(SOURCE));
        let dir = tempfile::tempdir()?;
        let mut images = ImageCache::new(dir.path());
        assert!(check(&json!({"nope": 1}), &types, &mut images).is_err());
        assert!(check(&json!({"count": "1"}), &types, &mut images).is_err());
        assert!(check(&json!({"items": [{"label": false}]}), &types, &mut images).is_err());
        assert!(check(&json!({"items": [{"other": 1}]}), &types, &mut images).is_err());
        assert!(check(&json!({"count": 1e300}), &types, &mut images).is_err());
        assert!(check(
            &json!({"art": {"$image": crate::images::tests::png_data_url(), "pico8": "loud"}}),
            &types,
            &mut images
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn accepts_images_with_pico8_and_null_partial_structs() -> Result<()> {
        let types = declared(&compile(SOURCE));
        let dir = tempfile::tempdir()?;
        let mut images = ImageCache::new(dir.path());
        let url = crate::images::tests::png_data_url();
        check(
            &json!({"count": null, "art": {"$image": url, "pico8": "vivid"}, "items": [{"label": "ok"}, {}]}),
            &types,
            &mut images,
        )?;
        Ok(())
    }

    #[test]
    fn null_keeps_previous_struct_fields() -> Result<()> {
        let types = declared(&compile(SOURCE));
        let dir = tempfile::tempdir()?;
        let mut images = ImageCache::new(dir.path());
        let ty = types["items"].clone();
        let first = convert(
            &json!([{"label": "a", "itemCount": 2}]),
            &ty,
            &Value::Void,
            &mut images,
            "items",
        )?;
        let second = convert(
            &json!([{"label": null, "itemCount": 3}]),
            &ty,
            &first,
            &mut images,
            "items",
        )?;
        let Value::Model(model) = second else {
            panic!()
        };
        let Some(Value::Struct(item)) = model.row_data(0) else {
            panic!()
        };
        assert_eq!(item.get_field("label"), Some(&Value::String("a".into())));
        assert_eq!(item.get_field("item-count"), Some(&Value::Number(3.0)));
        Ok(())
    }
}
