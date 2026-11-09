//! Focus targets as drawn (contract 8.4, clients/shell/README.md, Focus
//! targets): every element that draws a focus target sets
//! `accessible-item-selectable: true`, and `accessible-item-selected` while
//! its view entry is `focused`. A target's box is that element's geometry in
//! the window. Its identity is its region and its place among that region's
//! entries in the view, never its label or its values: labels repeat
//! legitimately (two route rows with the same buttons), values change in place
//! (a choice row stepped with left and right), and a list that draws only the
//! entries in view, or recentres them, changes what is drawn but not the view.
//!
//! Which element draws which view entry is found by lighting each entry alone
//! (every other `focused` false) and reading which element is selected. An
//! entry that selects no element, or one on a hidden layer (`visible: false` or
//! opacity zero on it or an ancestor), is not drawn. An entry whose element is
//! only clipped out of view (by a scrolling or clipping ancestor, or the window)
//! is drawn, at its real geometry, and not visible: a press can scroll it in.

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
    /// Inside the window and every clipping ancestor.
    pub visible: bool,
    /// On a hidden layer: `visible: false` or opacity zero on it or an ancestor.
    pub hidden: bool,
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
            let visible = e.is_visible();
            let hidden = e.is_hidden();
            let at = e.absolute_position();
            let size = e.size();
            Element {
                x: at.x,
                y: at.y,
                w: size.width,
                h: size.height,
                selected: e.accessible_item_selected() == Some(true),
                visible,
                hidden,
            }
        })
        .collect()
}

/// Whether the selected element's centre is inside every ancestor clip.
/// `ElementHandle::is_visible` also accepts a sliver whose centre is clipped;
/// the tap probe must wait for the focus walk to reveal that centre instead.
/// This reads Slint's existing item geometry; it adds no package convention.
pub fn selected_center_is_visible(instance: &ComponentInstance, x: f32, y: f32) -> bool {
    use i_slint_core::{
        accessibility::AccessibleStringProperty,
        item_tree::{ItemRc, ParentItemTraversalMode},
    };
    use std::ops::ControlFlow;
    let tree = instance.item_tree();
    let root = ItemRc::new_root(tree.clone());
    root.visit_descendants(|item| {
        if item
            .accessible_string_property(AccessibleStringProperty::ItemSelected)
            .is_none_or(|selected| selected != "true")
        {
            return ControlFlow::Continue(());
        }
        let mut parent = item.parent_item(ParentItemTraversalMode::StopAtPopups);
        while let Some(ancestor) = parent {
            if ancestor.borrow().as_ref().clips_children() {
                let rect = ancestor
                    .transform_to_item_tree(&tree)
                    .outer_transformed_rect(&ancestor.geometry().cast());
                if !rect.contains((x, y).into()) {
                    return ControlFlow::Break(false);
                }
            }
            parent = ancestor.parent_item(ParentItemTraversalMode::StopAtPopups);
        }
        ControlFlow::Break(true)
    })
    .unwrap_or(false)
}

/// A view entry with `focused`: where it is in the view, its region, and its flags.
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: Vec<Value>,
    /// The package's existing activation target id (ADR 0055).
    pub id: Option<String>,
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
                        id: map
                            .get("focusId")
                            .and_then(Value::as_str)
                            .filter(|id| !id.is_empty())
                            .map(str::to_owned),
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

/// A focus target: its entry, its element when it is drawn, its identity.
#[derive(Clone, Debug)]
pub struct Target {
    pub entry: Entry,
    /// The element when it is drawn: not on a hidden layer, with a size. It may be clipped
    /// out of view (`Element::visible` false).
    pub element: Option<Element>,
    /// `region#k`: the k-th of its region's entries in the view.
    pub identity: String,
    /// k.
    pub index: usize,
}

/// The targets of `view` as drawn by `instance`, with the view applied as it is at the end.
/// Fails when an entry selects more than one element, when an element is selected with no
/// entry focused, or when the focused entry is not drawn and visible (focus stays in view).
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
    let drawn = |e: &Element| !e.hidden && e.w > 0.0 && e.h > 0.0;
    let mut targets = Vec::new();
    let mut problem = None;
    let mut seen: std::collections::BTreeMap<String, usize> = Default::default();
    for (n, entry) in entries.iter().enumerate() {
        binder.apply(instance, &lit(view, &entries, Some(n)), images)?;
        let selected: Vec<Element> = elements(instance)
            .into_iter()
            .filter(|e| e.selected)
            .collect();
        if selected.len() > 1 {
            problem.get_or_insert(format!(
                "the focus entry at {} selects {} elements",
                path_text(&entry.path),
                selected.len()
            ));
        }
        let element = selected.into_iter().next().filter(drawn);
        if entry.focused && !element.as_ref().is_some_and(|e| e.visible) {
            problem.get_or_insert(format!(
                "the focused entry at {} ({:?}) selects no element with accessible-item-selectable in the window",
                path_text(&entry.path),
                entry.region
            ));
        }
        let index = seen.entry(entry.region.clone()).or_default();
        targets.push(Target {
            identity: format!("{}#{}", entry.region, index),
            index: *index,
            entry: entry.clone(),
            element,
        });
        *index += 1;
    }
    binder.apply(instance, view, images)?;
    if let Some(problem) = problem {
        bail!(problem);
    }
    Ok(targets)
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
