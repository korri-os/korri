//! THROWAWAY SPIKE (spikes/slint-caliper/viewer/README.md): Caliper's part list, states
//! and true-size device frames for Korri's Slint surface packages, with reload on save.
//!
//! Each state is compiled with slint-interpreter, instantiated on a MinimalSoftwareWindow
//! of the device's own pixel size and drawn by the software renderer into an RGB image.
//! The viewer's window shows that image scaled to the device's millimetres on the
//! monitor the window is on.

mod devices;
mod monitor;
mod parts;
mod tune;

use anyhow::{bail, Context, Result};
use devices::DEVICES;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::WindowAdapter;
use slint::{ComponentHandle, Image, Model, ModelRc, Rgb8Pixel, SharedPixelBuffer, VecModel};
use slint_interpreter::{ComponentDefinition, ComponentInstance, DiagnosticLevel};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

slint::slint! {
    #[style = "fluent-dark"]
    export { Viewer, ListRow, Frame, Group, Chip, KnobRow } from "ui/viewer.slint";
}

/// One compiled preview file: each state's definition or its diagnostics.
struct Built {
    states: HashMap<String, Result<ComponentDefinition, String>>,
    deps: BTreeSet<PathBuf>,
    /// The token files it imports: their globals are re-exported, so knobs reach them.
    tokens: Vec<PathBuf>,
    ms: u128,
}

/// A state drawn for one device: its own window, at the device's pixel size.
struct Live {
    group: usize,
    row: usize,
    window: Rc<MinimalSoftwareWindow>,
    _instance: ComponentInstance,
    tokens: Vec<PathBuf>,
    size: (u32, u32),
    checked: bool,
}

struct App {
    root: PathBuf,
    parts: Vec<parts::Part>,
    selected: Option<usize>,
    alone: Option<String>,
    expanded: BTreeSet<usize>,
    on: Vec<bool>,
    cache: HashMap<PathBuf, Rc<Built>>,
    live: Vec<Live>,
    groups: Vec<Rc<VecModel<Frame>>>,
    monitor: Option<String>,
    ui: slint::Weak<Viewer>,
    // Knobs: the token files, the live values that differ from the files, and which
    // knob each row of the panel shows.
    tokens: Vec<tune::TokenFile>,
    overrides: BTreeMap<(PathBuf, String), String>,
    knob_keys: Vec<(usize, usize)>,
    knob_rows: Rc<VecModel<KnobRow>>,
    // Takes: the records, and each take's compiled part.
    takes: Vec<tune::Take>,
    take_cache: HashMap<(String, PathBuf), Rc<Built>>,
}

/// Compile one preview file. A file loader serves each token file with its knobs declared
/// `in-out` (tune.rs), and, for a take, the take's copy of any file it has. The preview
/// re-exports each imported token global under its alias.
fn compile(path: &Path, root: &Path, take: Option<&str>, token_paths: &BTreeSet<PathBuf>) -> Built {
    let started = Instant::now();
    let base = take.map(|t| tune::take_folder(root, t));
    let overlay = {
        let root = root.to_path_buf();
        let base = base.clone();
        move |p: &Path| -> Option<PathBuf> {
            let base = base.as_ref()?;
            let copy = base.join(p.strip_prefix(&root).ok()?);
            copy.is_file().then_some(copy)
        }
    };
    let read = |p: &Path| {
        std::fs::read_to_string(overlay(p).unwrap_or(p.to_path_buf())).unwrap_or_default()
    };
    let mut deps = parts::imports(path);
    if let Some(base) = &base {
        if let Some(t) = take {
            for rel in tune::list(root)
                .into_iter()
                .filter(|x| x.id == t)
                .flat_map(|x| x.files)
            {
                deps.insert(parts::canon(&base.join(rel)));
            }
        }
    }
    let imported: Vec<tune::TokenFile> = token_paths
        .iter()
        .filter(|p| deps.contains(*p))
        .map(|p| tune::scan(p, &read(p)))
        .collect();
    let tail = tune::exports(&imported.iter().collect::<Vec<_>>());
    let source = read(path);
    let names = parts::states_of(&source);
    let token_set: Rc<BTreeSet<PathBuf>> =
        Rc::new(imported.iter().map(|f| f.path.clone()).collect());
    let build = |src: String| {
        let mut compiler = slint_interpreter::Compiler::default();
        let overlay = overlay.clone();
        let token_set = token_set.clone();
        compiler.set_file_loader(move |p: &Path| {
            let p = parts::canon(p);
            let copy = overlay(&p);
            let is_token = token_set.contains(&p);
            let text = if copy.is_some() || is_token {
                std::fs::read_to_string(copy.unwrap_or(p.clone()))
                    .ok()
                    .map(|text| {
                        if is_token {
                            tune::settable(&tune::scan(&p, &text), &text)
                        } else {
                            text
                        }
                    })
            } else {
                None
            };
            Box::pin(async move { text.map(Ok) })
        });
        pollster::block_on(compiler.build_from_source(src + &tail, path.to_path_buf()))
    };
    let errors = |r: &slint_interpreter::CompilationResult| -> Vec<String> {
        r.diagnostics()
            .filter(|d| d.level() == DiagnosticLevel::Error)
            .map(|d| {
                let file = d
                    .source_file()
                    .and_then(|f| f.file_name())
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let (line, col) = d.line_column();
                format!("{file}:{line}:{col}: {}", d.message())
            })
            .collect()
    };
    let whole = build(source.clone());
    let mut states = HashMap::new();
    if !whole.has_errors() {
        for name in &names {
            let def = whole
                .component(name)
                .with_context(|| format!("{name} is not exported"))
                .map_err(|e| e.to_string());
            states.insert(name.clone(), def);
        }
    } else {
        // Each state alone, so an error in one state's block fails only that state.
        for name in &names {
            let result = build(parts::alone(&source, name));
            let def = if result.has_errors() {
                Err(errors(&result).join("\n"))
            } else {
                result
                    .component(name)
                    .with_context(|| format!("{name} is not exported"))
                    .map_err(|e| e.to_string())
            };
            states.insert(name.clone(), def);
        }
    }
    Built {
        states,
        deps,
        tokens: imported.into_iter().map(|f| f.path).collect(),
        ms: started.elapsed().as_millis(),
    }
}

/// Draw `window` if it changed. Returns the frame and whether it is one colour.
fn draw(window: &MinimalSoftwareWindow, (w, h): (u32, u32)) -> Option<(Image, bool)> {
    let mut out = None;
    window.draw_if_needed(|renderer| {
        let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(w, h);
        renderer.render(buffer.make_mut_slice(), w as usize);
        let first = buffer.as_slice()[0];
        let flat = buffer.as_slice().iter().all(|p| *p == first);
        out = Some((Image::from_rgb8(buffer), flat));
    });
    out
}

impl App {
    fn ui(&self) -> Viewer {
        self.ui.upgrade().expect("viewer")
    }

    fn token_paths(&self) -> BTreeSet<PathBuf> {
        self.tokens.iter().map(|f| f.path.clone()).collect()
    }

    fn built(&mut self, path: &Path) -> Rc<Built> {
        if let Some(b) = self.cache.get(path) {
            return b.clone();
        }
        let b = Rc::new(compile(path, &self.root, None, &self.token_paths()));
        self.cache.insert(path.to_path_buf(), b.clone());
        b
    }

    fn take_built(&mut self, take: &str, path: &Path) -> Rc<Built> {
        let key = (take.to_string(), path.to_path_buf());
        if let Some(b) = self.take_cache.get(&key) {
            return b.clone();
        }
        let b = Rc::new(compile(path, &self.root, Some(take), &self.token_paths()));
        self.take_cache.insert(key, b.clone());
        b
    }

    /// The takes of the shown part and state (Caliper shows them for one state).
    fn shown_takes(&self) -> Vec<String> {
        let (Some(i), Some(state)) = (self.selected, &self.alone) else {
            return Vec::new();
        };
        let rel = &self.parts[i].rel;
        self.takes
            .iter()
            .filter(|t| &t.part == rel && &t.state == state)
            .map(|t| t.id.clone())
            .collect()
    }

    /// Set every live value that differs from its file on one new state.
    fn apply_overrides(&self, instance: &ComponentInstance, tokens: &[PathBuf]) {
        for ((file, name), text) in &self.overrides {
            if !tokens.contains(file) {
                continue;
            }
            let Some((f, knob)) = self.knob(file, name) else {
                continue;
            };
            if let Some(v) = tune::value(knob, text, &self.colours(f)) {
                let _ = instance.set_global_property(&tune::alias(file, &knob.global), name, v);
            }
        }
    }

    fn knob(&self, file: &Path, name: &str) -> Option<(usize, &tune::Knob)> {
        let f = self.tokens.iter().position(|t| t.path == file)?;
        Some((f, self.tokens[f].knobs.iter().find(|k| k.name == name)?))
    }

    /// A file's colour literals by name, with their live values: what a Ref knob names.
    fn colours(&self, f: usize) -> BTreeMap<String, String> {
        let file = &self.tokens[f];
        file.knobs
            .iter()
            .filter(|k| k.kind == tune::Kind::Color)
            .map(|k| {
                let live = self.overrides.get(&(file.path.clone(), k.name.clone()));
                (k.name.clone(), live.unwrap_or(&k.value).clone())
            })
            .collect()
    }

    fn list(&self) {
        let mut rows = Vec::new();
        for (layer, title) in parts::LAYER_TITLES.iter().enumerate() {
            let count = self.parts.iter().filter(|p| p.layer == layer).count();
            if count == 0 {
                continue;
            }
            rows.push(ListRow {
                kind: 0,
                label: (*title).into(),
                count: count as i32,
                part: -1,
                ..Default::default()
            });
            for (i, part) in self
                .parts
                .iter()
                .enumerate()
                .filter(|(_, p)| p.layer == layer)
            {
                let expanded = self.expanded.contains(&i);
                let failed = self
                    .cache
                    .get(&part.path)
                    .is_some_and(|b| b.states.values().any(|s| s.is_err()));
                rows.push(ListRow {
                    kind: 1,
                    label: if failed {
                        format!("{}  ⚠", part.name)
                    } else {
                        part.name.clone()
                    }
                    .into(),
                    part: i as i32,
                    selected: self.selected == Some(i) && self.alone.is_none(),
                    expanded,
                    count: part.states.len() as i32,
                    ..Default::default()
                });
                if expanded {
                    for state in &part.states {
                        rows.push(ListRow {
                            kind: 2,
                            label: parts::label(state).into(),
                            part: i as i32,
                            state: state.into(),
                            selected: self.selected == Some(i)
                                && self.alone.as_deref() == Some(state),
                            ..Default::default()
                        });
                        // Caliper nests takes under their editing state.
                        for take in self
                            .takes
                            .iter()
                            .filter(|t| t.part == part.rel && &t.state == state)
                        {
                            rows.push(ListRow {
                                kind: 3,
                                label: format!("Take {}", take.id).into(),
                                part: i as i32,
                                state: state.into(),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
        self.ui().set_rows(ModelRc::new(VecModel::from(rows)));
    }

    fn chips(&self) {
        let chips: Vec<Chip> = DEVICES
            .iter()
            .zip(&self.on)
            .map(|(d, on)| Chip {
                id: d.id.into(),
                name: d.name.into(),
                on: *on,
            })
            .collect();
        self.ui().set_chips(ModelRc::new(VecModel::from(chips)));
    }

    /// Compile (or reuse) the selected part and draw each state on each device that is on.
    fn frames(&mut self) -> usize {
        for live in self.live.drain(..) {
            let _ = live._instance.hide();
        }
        self.groups.clear();
        let ui = self.ui();
        let Some(index) = self.selected else {
            ui.set_groups(ModelRc::default());
            ui.set_can_take(false);
            self.knobs();
            return 0;
        };
        let path = self.parts[index].path.clone();
        let built = self.built(&path);
        let takes: Vec<(String, Rc<Built>)> = self
            .shown_takes()
            .into_iter()
            .map(|t| {
                let b = self.take_built(&t, &path);
                (t, b)
            })
            .collect();
        let part = &self.parts[index];
        let states: Vec<String> = match &self.alone {
            Some(s) => vec![s.clone()],
            None => part.states.clone(),
        };
        ui.set_can_take(self.alone.is_some());
        ui.set_heading(
            match &self.alone {
                Some(s) => format!("{} / {}", part.name, parts::label(s)),
                None => part.name.clone(),
            }
            .into(),
        );
        let take_files: Vec<String> = self
            .takes
            .iter()
            .filter(|t| takes.iter().any(|(id, _)| *id == t.id))
            .map(|t| {
                format!(
                    "take {}: .caliper/takes/{}/{}",
                    t.id,
                    t.id,
                    t.files.join(", ")
                )
            })
            .collect();
        ui.set_detail(
            format!(
                "{}   {} states   compiled in {} ms{}",
                part.rel,
                part.states.len(),
                built.ms,
                if take_files.is_empty() {
                    String::new()
                } else {
                    format!("   {}", take_files.join("   "))
                }
            )
            .into(),
        );
        // One column per version: the original, then each take of the shown state.
        let mut versions: Vec<(String, Rc<Built>)> = vec![(String::new(), built.clone())];
        versions.extend(takes);
        let mut groups = Vec::new();
        let mut live = Vec::new();
        for (d, device) in DEVICES.iter().enumerate().filter(|(d, _)| self.on[*d]) {
            let g = groups.len();
            let mut frames = Vec::new();
            for state in states.iter() {
                for (take, built) in &versions {
                    let row = frames.len();
                    let mut frame = Frame {
                        label: if take.is_empty() {
                            if versions.len() > 1 {
                                format!("{}, original", parts::label(state))
                            } else {
                                parts::label(state)
                            }
                        } else {
                            format!("Take {take}")
                        }
                        .into(),
                        state: state.into(),
                        take: take.into(),
                        ..Default::default()
                    };
                    match built.states.get(state) {
                        Some(Ok(def)) => {
                            let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
                            let size = (device.css_width, device.css_height);
                            window.set_size(slint::PhysicalSize::new(size.0, size.1));
                            // As slint-interpreter does for a window it creates: the state's
                            // `import "font.ttf"` needs the renderer's Slint context.
                            let adapter: Rc<dyn WindowAdapter> = window.clone();
                            adapter.renderer().set_window_adapter(&adapter);
                            match def
                                .create_with_existing_window(window.window())
                                .and_then(|i| i.show().map(|_| i))
                            {
                                Ok(instance) => {
                                    self.apply_overrides(&instance, &built.tokens);
                                    if let Some((image, _)) = draw(&window, size) {
                                        frame.image = image;
                                    }
                                    live.push(Live {
                                        group: g,
                                        row,
                                        window,
                                        _instance: instance,
                                        tokens: built.tokens.clone(),
                                        size,
                                        checked: false,
                                    });
                                }
                                Err(e) => frame.error = format!("could not create: {e}").into(),
                            }
                        }
                        Some(Err(e)) => frame.error = e.as_str().into(),
                        None => frame.error = "not compiled".into(),
                    }
                    frames.push(frame);
                }
            }
            let _ = d;
            let model = Rc::new(VecModel::from(frames));
            self.groups.push(model.clone());
            groups.push(Group {
                id: device.id.into(),
                name: device.name.into(),
                px_w: device.css_width as i32,
                px_h: device.css_height as i32,
                width_mm: device.width_mm,
                height_mm: device.height_mm,
                note: device.note.into(),
                unresolved: device.mm_unresolved,
                frames: ModelRc::from(model),
            });
        }
        self.live = live;
        ui.set_groups(ModelRc::new(VecModel::from(groups)));
        self.knobs();
        self.live.len()
    }

    /// The knobs panel: the knobs of each token file the shown part imports, then the
    /// values that are outputs, with the reason.
    fn knobs(&mut self) {
        let ui = self.ui();
        let imported: Vec<PathBuf> = match self.selected {
            Some(i) => self
                .cache
                .get(&self.parts[i].path)
                .map(|b| b.tokens.clone())
                .unwrap_or_default(),
            None => Vec::new(),
        };
        let mut rows = Vec::new();
        let mut keys = Vec::new();
        let mut files = Vec::new();
        for (f, file) in self
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| imported.contains(&t.path))
        {
            let rel = file
                .path
                .strip_prefix(&self.root)
                .unwrap_or(&file.path)
                .display()
                .to_string();
            files.push(rel.clone());
            rows.push(KnobRow {
                kind: 0,
                key: -1,
                label: format!("{rel}   {}", file.globals.join(", ")).into(),
                ..Default::default()
            });
            let colours = self.colours(f);
            for (k, knob) in file.knobs.iter().enumerate() {
                let live = self.overrides.get(&(file.path.clone(), knob.name.clone()));
                let text = live.unwrap_or(&knob.value).clone();
                let line = 1 + std::fs::read_to_string(&file.path)
                    .map(|t| t[..knob.start.min(t.len())].matches('\n').count())
                    .unwrap_or(0);
                let mut row = KnobRow {
                    key: keys.len() as i32,
                    label: knob.label.clone().unwrap_or(knob.name.clone()).into(),
                    doc: if knob.doc.is_empty() {
                        format!("{rel}:{line}")
                    } else {
                        knob.doc.clone()
                    }
                    .into(),
                    text: text.clone().into(),
                    changed: live.is_some(),
                    ..Default::default()
                };
                match &knob.kind {
                    tune::Kind::Color => {
                        row.kind = 1;
                        if let Some(slint_interpreter::Value::Brush(b)) =
                            tune::value(knob, &text, &colours)
                        {
                            row.color = b.color();
                        }
                    }
                    tune::Kind::Number { unit, int } => {
                        row.kind = 2;
                        let n: f64 = text.trim_end_matches(unit.as_str()).parse().unwrap_or(0.0);
                        row.num = n as f32;
                        let decimals = text
                            .split_once('.')
                            .map_or(0, |(_, d)| d.trim_end_matches(unit.as_str()).len());
                        row.step = knob.step.unwrap_or(if *int {
                            1.0
                        } else {
                            10f64.powi(-(decimals as i32))
                        }) as f32;
                        if let (Some(lo), Some(hi)) = (knob.min, knob.max) {
                            row.ranged = true;
                            row.min = lo as f32;
                            row.max = hi as f32;
                        }
                    }
                    tune::Kind::Ref => {
                        row.kind = 3;
                        let names: Vec<slint::SharedString> =
                            colours.keys().map(|n| n.into()).collect();
                        row.option = names
                            .iter()
                            .position(|n| n.as_str() == text)
                            .map_or(-1, |i| i as i32);
                        row.options = ModelRc::new(VecModel::from(names));
                        if let Some(slint_interpreter::Value::Brush(b)) =
                            tune::value(knob, &text, &colours)
                        {
                            row.color = b.color();
                        }
                    }
                }
                keys.push((f, k));
                rows.push(row);
            }
        }
        let outputs: Vec<KnobRow> = self
            .tokens
            .iter()
            .filter(|t| imported.contains(&t.path))
            .flat_map(|t| t.not_knobs.iter())
            .map(|n| KnobRow {
                kind: 4,
                key: -1,
                label: n.name.as_str().into(),
                doc: n.reason.as_str().into(),
                ..Default::default()
            })
            .collect();
        if !outputs.is_empty() {
            rows.push(KnobRow {
                kind: 0,
                key: -1,
                label: format!("Not knobs   {}", outputs.len()).into(),
                ..Default::default()
            });
            rows.extend(outputs);
        }
        self.knob_keys = keys;
        self.knob_rows.set_vec(rows);
        ui.set_knob_files(
            if files.is_empty() {
                "The part imports no exported global with literal values.".to_string()
            } else {
                format!("Edits {}", files.join(", "))
            }
            .into(),
        );
    }

    /// Set a knob's value on every live frame that imports its file, and draw them.
    /// `save` also writes the literal into the file (Caliper: when you let go).
    fn knob_set(&mut self, key: usize, text: String, save: bool) {
        let Some(&(f, k)) = self.knob_keys.get(key) else {
            return;
        };
        let started = Instant::now();
        let knob = self.tokens[f].knobs[k].clone();
        let text = match &knob.kind {
            // A typed number with no unit takes the knob's unit.
            tune::Kind::Number { unit, .. } if text.parse::<f64>().is_ok() => {
                format!("{text}{unit}")
            }
            _ => text.trim().to_string(),
        };
        let ui = self.ui();
        let Some(value) = tune::value(&knob, &text, &self.colours(f)) else {
            ui.set_status(format!("{}: \"{text}\" is not a {}", knob.name, knob.ty).into());
            return;
        };
        let alias = tune::alias(&knob.file, &knob.global);
        let mut set = 0;
        for live in &self.live {
            if live.tokens.contains(&knob.file)
                && live
                    ._instance
                    .set_global_property(&alias, &knob.name, value.clone())
                    .is_ok()
            {
                set += 1;
            }
        }
        let key_ = (knob.file.clone(), knob.name.clone());
        if text == knob.value {
            self.overrides.remove(&key_);
        } else {
            self.overrides.insert(key_, text.clone());
        }
        let set_us = started.elapsed().as_micros();
        self.tick();
        let drawn_us = started.elapsed().as_micros();
        // Update the knob's own row (the whole list when a colour a Ref names changed).
        if let Some(row) = (0..self.knob_rows.row_count()).find(|r| {
            self.knob_rows
                .row_data(*r)
                .is_some_and(|x| x.key == key as i32 && x.kind != 0 && x.kind != 4)
        }) {
            let mut r = self.knob_rows.row_data(row).unwrap();
            r.text = text.as_str().into();
            r.changed = self
                .overrides
                .contains_key(&(knob.file.clone(), knob.name.clone()));
            if let slint_interpreter::Value::Brush(b) = &value {
                r.color = b.color();
            }
            if let slint_interpreter::Value::Number(n) = &value {
                if !text.ends_with('%') && !text.ends_with('s') || text.ends_with("ms") {
                    r.num = *n as f32;
                }
            }
            self.knob_rows.set_row_data(row, r);
        }
        let mut line = format!(
            "knob: {}.{} = {text}: set on {set} frames in {:.2} ms, frames drawn {:.2} ms after the change",
            knob.global,
            knob.name,
            set_us as f64 / 1000.0,
            drawn_us as f64 / 1000.0
        );
        if save && text != knob.value {
            let rel = knob
                .file
                .strip_prefix(&self.root)
                .unwrap_or(&knob.file)
                .display()
                .to_string();
            match tune::save(&knob, self.tokens[f].version, &text) {
                Ok(()) => line.push_str(&format!("; saved {} -> {text} in {rel}", knob.value)),
                Err(e) => {
                    // Caliper: the knob shows the file's value again.
                    line.push_str(&format!("; not saved: {e}"));
                    self.overrides
                        .remove(&(knob.file.clone(), knob.name.clone()));
                    if let Some(v) = tune::value(&knob, &knob.value, &self.colours(f)) {
                        for live in &self.live {
                            if live.tokens.contains(&knob.file) {
                                let _ = live._instance.set_global_property(
                                    &alias,
                                    &knob.name,
                                    v.clone(),
                                );
                            }
                        }
                    }
                    self.knobs();
                }
            }
        }
        if save || std::env::var_os("KORRI_VIEWER_KNOB_LOG").is_some() {
            eprintln!("{line}");
        }
        ui.set_status(line.into());
    }

    /// Redraw every state whose window changed (animations, changed callbacks).
    fn tick(&mut self) {
        for live in &mut self.live {
            if let Some((image, flat)) = draw(&live.window, live.size) {
                let model = &self.groups[live.group];
                if let Some(mut frame) = model.row_data(live.row) {
                    frame.image = image;
                    if !live.checked {
                        frame.note = if flat {
                            "draws nothing".into()
                        } else {
                            Default::default()
                        };
                    }
                    model.set_row_data(live.row, frame);
                }
                live.checked = true;
            }
        }
    }

    fn measure(&mut self, force: bool) {
        let ui = self.ui();
        let scale = ui.window().scale_factor();
        let found = {
            use slint::winit_030::WinitWindowAccessor;
            ui.window().with_winit_window(|w| {
                w.current_monitor().map(|m| monitor::Monitor {
                    name: m.name().unwrap_or_else(|| "unknown".into()),
                    width_px: m.size().width,
                    height_px: m.size().height,
                    scale,
                })
            })
        }
        .flatten();
        let Some(found) = found else { return };
        if !force && self.monitor.as_deref() == Some(&found.name) {
            return;
        }
        let m = monitor::measure(&found);
        eprintln!(
            "monitor {}: {:.3} px/mm ({}), window scale {scale}",
            found.name, m.px_per_mm, m.source
        );
        self.monitor = Some(found.name);
        ui.set_px_per_mm(m.px_per_mm);
        ui.set_calibrated(m.calibrated);
        ui.set_monitor(format!("{}. Saved in {}.", m.source, monitor::store().display()).into());
    }

    /// Files changed on disk: recompile the parts that import them and redraw the shown one.
    fn changed(&mut self, paths: &BTreeSet<PathBuf>, saved_at: Instant) {
        let all_paths = paths.clone();
        // Takes: a changed copy evicts its take; a changed real file evicts every take
        // that reads it.
        let takes_dir = parts::canon(&tune::takes_dir(&self.root));
        let take_ids: BTreeSet<String> = paths
            .iter()
            .filter_map(|p| p.strip_prefix(&takes_dir).ok())
            .filter_map(|r| r.iter().next().map(|s| s.to_string_lossy().into_owned()))
            .collect();
        self.take_cache
            .retain(|(t, _), b| !take_ids.contains(t) && b.deps.is_disjoint(paths));
        let paths: BTreeSet<PathBuf> = paths
            .iter()
            .filter(|p| !p.starts_with(&takes_dir))
            .cloned()
            .collect();
        let paths = &paths;
        // Knobs: read the token files again, and drop live values the files now hold.
        self.tokens = tune::token_files(&self.root);
        let tokens = &self.tokens;
        self.overrides.retain(|(file, name), text| {
            tokens
                .iter()
                .find(|t| &t.path == file)
                .and_then(|t| t.knobs.iter().find(|k| &k.name == name))
                .is_some_and(|k| &k.value != text)
        });
        let previews_moved = paths.iter().any(|p| {
            p.to_string_lossy().ends_with(".preview.slint")
                && !self.parts.iter().any(|q| parts::canon(&q.path) == *p)
        }) || self.parts.iter().any(|p| !p.path.exists());
        if previews_moved {
            let keep = self.selected.map(|i| self.parts[i].path.clone());
            self.parts = parts::discover(&self.root);
            self.selected = keep.and_then(|k| self.parts.iter().position(|p| p.path == k));
            self.expanded.clear();
        }
        self.cache.retain(|_, b| b.deps.is_disjoint(paths));
        for part in &mut self.parts {
            if paths.contains(&parts::canon(&part.path)) {
                part.states = std::fs::read_to_string(&part.path)
                    .map(|s| parts::states_of(&s))
                    .unwrap_or_default();
            }
        }
        if let Some(i) = self.selected {
            if self
                .alone
                .as_ref()
                .is_some_and(|s| !self.parts[i].states.contains(s))
            {
                self.alone = None;
            }
            let path = self.parts[i].path.clone();
            let take_missing = self
                .shown_takes()
                .iter()
                .any(|t| !self.take_cache.contains_key(&(t.clone(), path.clone())));
            if !self.cache.contains_key(&path) || take_missing {
                let waited = saved_at.elapsed().as_millis();
                let started = Instant::now();
                let n = self.frames();
                self.tick();
                let built = self.cache.get(&path).cloned();
                let compile_ms = built.as_ref().map_or(0, |b| b.ms);
                let draw_ms = started.elapsed().as_millis().saturating_sub(compile_ms);
                let failed = built
                    .as_ref()
                    .map_or(0, |b| b.states.values().filter(|s| s.is_err()).count());
                let names: Vec<String> = all_paths
                    .iter()
                    .filter_map(|p| p.strip_prefix(&self.root).ok())
                    .map(|p| p.display().to_string())
                    .collect();
                let line = format!(
                    "reload: {} changed; {} recompiled in {compile_ms} ms; {n} frames created and drawn in {draw_ms} ms; on screen {} ms after the save (first event to work: {waited} ms); {failed} states fail",
                    names.join(", "),
                    self.parts[i].name,
                    saved_at.elapsed().as_millis()
                );
                eprintln!("{line}");
                self.ui().set_status(line.into());
            }
        }
        self.list();
    }
}

impl App {
    /// A take of the shown state, made by hand: copies of the part's files.
    fn new_take(&mut self) {
        let (Some(i), Some(state)) = (self.selected, self.alone.clone()) else {
            return;
        };
        let device = DEVICES
            .iter()
            .zip(&self.on)
            .find(|(_, on)| **on)
            .map_or("", |(d, _)| d.id);
        let line = match tune::create(&self.root, &self.parts[i].rel, &state, device) {
            Ok(t) => format!(
                "take {}: edit {}",
                t.id,
                t.files
                    .iter()
                    .map(|f| tune::take_folder(&self.root, &t.id)
                        .join(f)
                        .display()
                        .to_string())
                    .collect::<Vec<_>>()
                    .join(" and ")
            ),
            Err(e) => format!("no take: {e}"),
        };
        eprintln!("{line}");
        self.takes = tune::list(&self.root);
        self.frames();
        self.list();
        self.ui().set_status(line.into());
    }

    fn end_take(&mut self, id: &str, accept: bool) {
        let Some(take) = tune::list(&self.root).into_iter().find(|t| t.id == id) else {
            return;
        };
        let line = if accept {
            match tune::accept(&self.root, &take) {
                Ok(files) => format!("take {id} accepted: wrote {}", files.join(", ")),
                Err(e) => format!("take {id} not accepted: {e}"),
            }
        } else {
            match tune::discard(&self.root, id) {
                Ok(()) => format!("take {id} discarded"),
                Err(e) => format!("take {id} not discarded: {e}"),
            }
        };
        eprintln!("{line}");
        self.take_cache.retain(|(t, _), _| t != id);
        self.takes = tune::list(&self.root);
        self.frames();
        self.list();
        self.ui().set_status(line.into());
    }
}

fn packages_dir(arg: Option<String>) -> Result<PathBuf> {
    if let Some(dir) = arg.or_else(|| std::env::var("KORRI_PACKAGES").ok()) {
        return Ok(parts::canon(Path::new(&dir)));
    }
    let mut dir = std::env::current_dir()?;
    loop {
        let candidate = dir.join("clients/shell/packages");
        if candidate.is_dir() {
            return Ok(parts::canon(&candidate));
        }
        if !dir.pop() {
            // The Nix package's own copy (default.nix), read-only.
            if let Some(copy) = option_env!("KORRI_VIEWER_FALLBACK_PACKAGES")
                .map(PathBuf::from)
                .filter(|p| p.is_dir())
            {
                eprintln!("no clients/shell/packages above the working directory; showing the package's copy {}", copy.display());
                return Ok(copy);
            }
            bail!("no clients/shell/packages above the working directory: pass --packages DIR");
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("slint-caliper-viewer [--packages DIR] [--devices ID,ID] [--part PATH-SUFFIX] [--state NAME]");
        println!(
            "devices: {}",
            DEVICES.iter().map(|d| d.id).collect::<Vec<_>>().join(", ")
        );
        return Ok(());
    }
    let root = packages_dir(flag("--packages"))?;
    let parts = parts::discover(&root);
    eprintln!("{} parts under {}", parts.len(), root.display());

    let mut on = vec![false; DEVICES.len()];
    match flag("--devices") {
        Some(list) => {
            for id in list.split(',') {
                on[devices::index_of(id).with_context(|| format!("unknown device {id}"))?] = true;
            }
        }
        // Caliper: the first device is the default one.
        None => on[0] = true,
    }

    let ui = Viewer::new()?;
    let app = Rc::new(RefCell::new(App {
        root: root.clone(),
        parts,
        selected: None,
        alone: None,
        expanded: BTreeSet::new(),
        on,
        cache: HashMap::new(),
        live: Vec::new(),
        groups: Vec::new(),
        monitor: None,
        ui: ui.as_weak(),
        tokens: tune::token_files(&root),
        overrides: BTreeMap::new(),
        knob_keys: Vec::new(),
        knob_rows: Rc::new(VecModel::default()),
        takes: tune::list(&root),
        take_cache: HashMap::new(),
    }));
    ui.set_knob_rows(ModelRc::from(app.borrow().knob_rows.clone()));
    if let Some(suffix) = flag("--part") {
        let mut a = app.borrow_mut();
        let i = a
            .parts
            .iter()
            .position(|p| p.rel.ends_with(&suffix))
            .with_context(|| format!("no part {suffix}"))?;
        a.selected = Some(i);
        a.expanded.insert(i);
        a.alone = flag("--state");
    }
    {
        let mut a = app.borrow_mut();
        a.list();
        a.chips();
        a.frames();
    }

    let a = app.clone();
    ui.on_pick_part(move |i| {
        let mut a = a.borrow_mut();
        a.selected = Some(i as usize);
        a.alone = None;
        a.expanded.insert(i as usize);
        a.frames();
        a.list();
    });
    let a = app.clone();
    ui.on_toggle_part(move |i| {
        let mut a = a.borrow_mut();
        if !a.expanded.remove(&(i as usize)) {
            a.expanded.insert(i as usize);
        }
        a.list();
    });
    let a = app.clone();
    ui.on_pick_state(move |i, state| {
        let mut a = a.borrow_mut();
        a.selected = Some(i as usize);
        a.alone = Some(state.to_string());
        a.frames();
        a.list();
    });
    let a = app.clone();
    ui.on_pick_frame(move |state| {
        let mut a = a.borrow_mut();
        a.alone = Some(state.to_string());
        a.frames();
        a.list();
    });
    let a = app.clone();
    ui.on_knob_text(move |key, text, save| {
        a.borrow_mut()
            .knob_set(key as usize, text.to_string(), save);
    });
    let a = app.clone();
    ui.on_knob_number(move |key, value, save| {
        let mut a = a.borrow_mut();
        let Some(&(f, k)) = a.knob_keys.get(key as usize) else {
            return;
        };
        let text = match &a.tokens[f].knobs[k].kind {
            tune::Kind::Number { unit, int: true } => format!("{}{unit}", value.round() as i64),
            tune::Kind::Number { unit, .. } => {
                let v = a.tokens[f].knobs[k]
                    .value
                    .trim_end_matches(unit.as_str())
                    .to_string();
                let decimals = v.split_once('.').map_or(0, |(_, d)| d.len()).max(1);
                let s = format!("{:.*}", decimals, value);
                format!("{s}{unit}")
            }
            _ => return,
        };
        a.knob_set(key as usize, text, save);
    });
    let a = app.clone();
    ui.on_new_take(move || a.borrow_mut().new_take());
    let a = app.clone();
    ui.on_accept_take(move |id| a.borrow_mut().end_take(id.as_str(), true));
    let a = app.clone();
    ui.on_discard_take(move |id| a.borrow_mut().end_take(id.as_str(), false));
    let a = app.clone();
    ui.on_toggle_device(move |id: slint::SharedString| {
        let mut a = a.borrow_mut();
        if let Some(d) = devices::index_of(&id) {
            a.on[d] = !a.on[d];
            if !a.on.iter().any(|o| *o) {
                a.on[d] = true;
            }
        }
        a.chips();
        a.frames();
    });
    let a = app.clone();
    ui.on_set_px_per_mm(move |v| {
        let a = a.borrow();
        if let Some(name) = &a.monitor {
            if let Err(e) = monitor::save(name, Some(v)) {
                eprintln!("calibration not saved: {e}");
            }
            let ui = a.ui();
            ui.set_px_per_mm(v);
            ui.set_calibrated(true);
            ui.set_monitor(
                format!(
                    "{name}: calibrated. Saved in {}.",
                    monitor::store().display()
                )
                .into(),
            );
        }
    });
    let a = app.clone();
    ui.on_reset_calibration(move || {
        let mut a = a.borrow_mut();
        if let Some(name) = a.monitor.clone() {
            let _ = monitor::save(&name, None);
        }
        a.measure(true);
    });

    // Reload on save: a watcher thread hands changed .slint files to the UI thread.
    let (tx, rx) = mpsc::channel::<(Instant, PathBuf)>();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            if matches!(event.kind, notify::EventKind::Access(_)) {
                return;
            }
            for path in event.paths {
                if path.extension().is_some_and(|e| e == "slint") {
                    let _ = tx.send((Instant::now(), path));
                }
            }
        }
    })?;
    notify::Watcher::watch(&mut watcher, &root, notify::RecursiveMode::Recursive)?;

    // One timer: changed files (after 40 ms of quiet, as an editor writes in steps), the
    // monitor under the window, and the frames that changed.
    let pending: RefCell<Option<(Instant, Instant, BTreeSet<PathBuf>)>> = RefCell::new(None);
    let mut last_monitor = Instant::now() - Duration::from_secs(1);
    let a = app.clone();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            let mut pending = pending.borrow_mut();
            while let Ok((at, path)) = rx.try_recv() {
                let entry = pending.get_or_insert_with(|| (at, at, BTreeSet::new()));
                entry.1 = at;
                entry.2.insert(parts::canon(&path));
            }
            if pending
                .as_ref()
                .is_some_and(|(_, last, _)| last.elapsed() > Duration::from_millis(40))
            {
                let (first, _, paths) = pending.take().unwrap();
                a.borrow_mut().changed(&paths, first);
            }
            // Ask winit for a frame directly, past Slint's own pending flag: under sway a redraw
            // the window asked for could be lost, and the window then never drew again. A frame
            // with no damage is not presented, so this costs a dirty-region walk.
            if let Some(ui) = a.borrow().ui.upgrade() {
                use slint::winit_030::WinitWindowAccessor;
                ui.window().with_winit_window(|w| w.request_redraw());
            }
            if last_monitor.elapsed() > Duration::from_millis(500) {
                last_monitor = Instant::now();
                a.borrow_mut().measure(false);
            }
            a.borrow_mut().tick();
        },
    );

    ui.run()?;
    drop(watcher);
    Ok(())
}
