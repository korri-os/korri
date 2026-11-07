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

use anyhow::{bail, Context, Result};
use devices::DEVICES;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::WindowAdapter;
use slint::{ComponentHandle, Image, Model, ModelRc, Rgb8Pixel, SharedPixelBuffer, VecModel};
use slint_interpreter::{ComponentDefinition, ComponentInstance, DiagnosticLevel};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

slint::slint! {
    export { Viewer, ListRow, Frame, Group, Chip } from "ui/viewer.slint";
}

/// One compiled preview file: each state's definition or its diagnostics.
struct Built {
    states: HashMap<String, Result<ComponentDefinition, String>>,
    deps: BTreeSet<PathBuf>,
    ms: u128,
}

/// A state drawn for one device: its own window, at the device's pixel size.
struct Live {
    group: usize,
    row: usize,
    window: Rc<MinimalSoftwareWindow>,
    _instance: ComponentInstance,
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
}

fn compile(path: &Path) -> Built {
    let started = Instant::now();
    let source = std::fs::read_to_string(path).unwrap_or_default();
    let names = parts::states_of(&source);
    let build = |src: String| {
        pollster::block_on(
            slint_interpreter::Compiler::default().build_from_source(src, path.to_path_buf()),
        )
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
        deps: parts::imports(path),
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

    fn built(&mut self, path: &Path) -> Rc<Built> {
        if let Some(b) = self.cache.get(path) {
            return b.clone();
        }
        let b = Rc::new(compile(path));
        self.cache.insert(path.to_path_buf(), b.clone());
        b
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
            return 0;
        };
        let path = self.parts[index].path.clone();
        let built = self.built(&path);
        let part = &self.parts[index];
        let states: Vec<String> = match &self.alone {
            Some(s) => vec![s.clone()],
            None => part.states.clone(),
        };
        ui.set_heading(
            match &self.alone {
                Some(s) => format!("{} / {}", part.name, parts::label(s)),
                None => part.name.clone(),
            }
            .into(),
        );
        ui.set_detail(
            format!(
                "{}   {} states   compiled in {} ms",
                part.rel,
                part.states.len(),
                built.ms
            )
            .into(),
        );
        let mut groups = Vec::new();
        let mut live = Vec::new();
        for (d, device) in DEVICES.iter().enumerate().filter(|(d, _)| self.on[*d]) {
            let g = groups.len();
            let mut frames = Vec::new();
            for (row, state) in states.iter().enumerate() {
                let mut frame = Frame {
                    label: parts::label(state).into(),
                    state: state.into(),
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
                                if let Some((image, _)) = draw(&window, size) {
                                    frame.image = image;
                                }
                                live.push(Live {
                                    group: g,
                                    row,
                                    window,
                                    _instance: instance,
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
        self.live.len()
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
            if !self.cache.contains_key(&path) {
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
                let names: Vec<String> = paths
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
    }));
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
