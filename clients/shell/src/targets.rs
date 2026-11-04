//! Focus targets as drawn (contract 8.4, clients/shell/README.md, Focus
//! targets): every element that draws a focus target sets
//! `accessible-item-selectable: true`, and `accessible-item-selected` while
//! its view entry is `focused`. A target's box is that element's geometry in
//! the window. Its identity is its region and its place in that region's
//! drawn order, never its label or its values: labels repeat legitimately
//! (two route rows with the same buttons) and values change in place (a
//! choice row stepped with left and right).
//!
//! Which element draws which view entry is found by lighting each entry alone
//! (every other `focused` false) and reading which element is selected.

use crate::{
    images::ImageCache,
    view::{self, Binder},
};
use anyhow::{bail, Result};
use i_slint_backend_testing::ElementRoot;
use serde_json::Value;
use slint_interpreter::ComponentInstance;

/// One element that draws a focus target, in item-tree (drawing) order.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub selected: bool,
}

impl Element {
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// Every element marked `accessible-item-selectable`, in drawing order.
pub fn elements(instance: &ComponentInstance) -> Vec<Element> {
    instance
        .root_element()
        .query_descendants()
        .match_predicate(|e| e.accessible_item_selectable() == Some(true))
        .find_all()
        .iter()
        .filter(|e| e.is_valid())
        .map(|e| {
            let at = e.absolute_position();
            let size = e.size();
            Element {
                x: at.x,
                y: at.y,
                w: size.width,
                h: size.height,
                selected: e.accessible_item_selected() == Some(true),
            }
        })
        .collect()
}

/// A view entry with `focused`: where it is in the view, its region, and its flags.
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: Vec<Value>,
    pub region: String,
    pub active: bool,
    pub focused: bool,
}

/// The view's focus entries, in the view's order.
pub fn entries(view: &Value) -> Vec<Entry> {
    fn walk(value: &Value, path: &mut Vec<Value>, out: &mut Vec<Entry>) {
        match value {
            Value::Object(map) => {
                if map.contains_key("focused") {
                    out.push(Entry {
                        path: path.clone(),
                        region: map
                            .get(view::FOCUS_REGION)
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        active: map.get(view::FOCUS_ACTIVE) == Some(&Value::Bool(true)),
                        focused: map.get("focused") == Some(&Value::Bool(true)),
                    });
                }
                for (k, v) in map {
                    if k != view::FOCUS_PLAN {
                        path.push(Value::String(k.clone()));
                        walk(v, path, out);
                        path.pop();
                    }
                }
            }
            Value::Array(items) => {
                for (i, v) in items.iter().enumerate() {
                    path.push(Value::from(i));
                    walk(v, path, out);
                    path.pop();
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(view, &mut Vec::new(), &mut out);
    out
}

fn at_mut<'a>(view: &'a mut Value, path: &[Value]) -> &'a mut Value {
    path.iter().fold(view, |v, step| match step {
        Value::String(k) => &mut v[k.as_str()],
        Value::Number(i) => &mut v[i.as_u64().unwrap_or_default() as usize],
        _ => v,
    })
}

/// `view` with every entry's `focused` false, or only the entry at `lit` true.
pub fn lit(view: &Value, entries: &[Entry], lit: Option<usize>) -> Value {
    let mut out = view.clone();
    for (i, entry) in entries.iter().enumerate() {
        at_mut(&mut out, &entry.path)["focused"] = Value::Bool(Some(i) == lit);
    }
    out
}

/// A drawn focus target: its entry, its element, its identity.
#[derive(Clone, Debug)]
pub struct Target {
    pub entry: Entry,
    pub element: Element,
    /// `region#k`: the k-th of its region's targets in drawing order.
    pub identity: String,
}

/// The targets of `view` as drawn by `instance`, with the view applied as it is at the end.
/// Fails when an entry lights no element, or more than one, or when an element is selected
/// with no entry focused.
pub fn drawn(
    instance: &ComponentInstance,
    binder: &mut Binder,
    view: &Value,
    images: &mut ImageCache,
) -> Result<Vec<Target>> {
    let entries = entries(view);
    binder.apply(instance, &lit(view, &entries, None), images)?;
    if let Some(i) = elements(instance).iter().position(|e| e.selected) {
        binder.apply(instance, view, images)?;
        bail!("focus target element {i} is selected with no entry focused");
    }
    let mut found: Vec<(usize, Entry, Element)> = Vec::new();
    let mut problem = None;
    for (n, entry) in entries.iter().enumerate() {
        binder.apply(instance, &lit(view, &entries, Some(n)), images)?;
        let selected: Vec<(usize, Element)> = elements(instance)
            .into_iter()
            .enumerate()
            .filter(|(_, e)| e.selected)
            .collect();
        match selected.as_slice() {
            [(index, element)] => found.push((*index, entry.clone(), element.clone())),
            [] => {
                problem.get_or_insert(format!(
                    "the focus entry at {} ({:?}) selects no element with accessible-item-selectable",
                    path_text(&entry.path),
                    entry.region
                ));
            }
            more => {
                problem.get_or_insert(format!(
                    "the focus entry at {} selects {} elements",
                    path_text(&entry.path),
                    more.len()
                ));
            }
        }
    }
    binder.apply(instance, view, images)?;
    if let Some(problem) = problem {
        bail!(problem);
    }
    // Identity: the region and the place in its drawing order.
    let mut order: Vec<usize> = (0..found.len()).collect();
    order.sort_by_key(|&i| found[i].0);
    let mut seen: std::collections::BTreeMap<String, usize> = Default::default();
    let mut identity = vec![String::new(); found.len()];
    for i in order {
        let n = seen.entry(found[i].1.region.clone()).or_default();
        identity[i] = format!("{}#{}", found[i].1.region, n);
        *n += 1;
    }
    Ok(found
        .into_iter()
        .zip(identity)
        .map(|((_, entry, element), identity)| Target {
            entry,
            element,
            identity,
        })
        .collect())
}

pub fn path_text(path: &[Value]) -> String {
    let mut text = String::from("view");
    for step in path {
        match step {
            Value::String(k) => text += &format!(".{k}"),
            other => text += &format!("[{other}]"),
        }
    }
    text
}
