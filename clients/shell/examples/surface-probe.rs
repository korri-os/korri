//! Render .slint previews and walk a package's focus, headless (never shipped).
//!
//! ```text
//! surface-probe previews DIR --sizes 640x480,… --out OUT
//!     Every exported component of every *.preview.slint under DIR, at each
//!     size: OUT/<file>.<Component>.<W>x<H>.png. Fails a preview that does not
//!     compile or draws nothing but its background.
//! surface-probe walk PACKAGE --model FILE --sizes 640x480,… [--max N] [--out OUT]
//!     Run the package's logic on FILE (a SurfaceModel; a Ready catalog's
//!     `games` are the games), answer MoveCatalog and FocusGame as korrid does,
//!     and press every button from every state reached, up to N states. At
//!     each state with a focused control, render the view twice: as it is,
//!     and with every `focused` flag cleared. The focused control is visible
//!     inside the window when the two differ; a state where they do not is a
//!     failure, written to OUT with the presses that reached it.
//!
//!     Contract A2 (8.3): the logic gets `Resized` with each size right after
//!     `init`, and `displays` larger than any window, so a package that sizes
//!     itself from a display instead of its window fails. At each size the
//!     walk then checks every direction from every focused state: the press
//!     must move focus to the target drawn nearest in that direction (the
//!     portal's spatial-focus rule: distance along the pressed axis plus three
//!     times the distance across it, ahead by more than 1 px), or leave it
//!     where it is when no target lies that way. A focus target is a view
//!     entry with a `focused` field. Its drawn box: render the view with that
//!     entry alone focused and with none focused; the largest connected group
//!     of changed pixels. (The element tree has no per-target boxes: packages
//!     draw targets with plain elements, without ids or labels.) A target's
//!     identity across states is its entry without `focused` and without
//!     booleans (other highlight flags).
//!
//!     Default sizes: 320x240, 640x480, 480x800, 1280x720, 1920x1080.
//! ```
//!
//! Same Slint software renderer and view conversion as the shell.

use anyhow::{bail, Context, Result};
use korri_shell::{
    backend::headless::Headless,
    catalog,
    engine::{self, Engine, Limits},
    images::ImageCache,
    messages,
    package::{self, Package},
    view::Binder,
};
use serde_json::{json, Value};
use slint::ComponentHandle;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};

fn sizes(text: &str) -> Vec<(u32, u32)> {
    text.split(',')
        .filter_map(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .collect()
}

fn save(pixels: &[u16], (w, h): (u32, u32), path: &Path) -> Result<()> {
    let mut img = image::RgbImage::new(w, h);
    for (i, p) in pixels.iter().enumerate() {
        let (r, g, b) = ((p >> 11) & 0x1f, (p >> 5) & 0x3f, p & 0x1f);
        img.put_pixel(
            i as u32 % w,
            i as u32 / w,
            image::Rgb([
                (r << 3 | r >> 2) as u8,
                (g << 2 | g >> 4) as u8,
                (b << 3 | b >> 2) as u8,
            ]),
        );
    }
    img.save(path)
        .with_context(|| format!("save {}", path.display()))
}

/// One target lit alone: a few quick frames are enough to place its box.
fn render_quick(instance: &slint_interpreter::ComponentInstance) -> Vec<u16> {
    let mut frame = Vec::new();
    for _ in 0..3 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        instance.window().request_redraw();
        frame = Headless::render().unwrap_or_default();
    }
    frame
}

fn render(instance: &slint_interpreter::ComponentInstance, size: (u32, u32)) -> Vec<u16> {
    instance
        .window()
        .set_size(slint::PhysicalSize::new(size.0, size.1));
    // Changed callbacks (the tokens follow the window) and the 16 ms reveal
    // (the cursor's control brought into sight) settle over a few passes.
    let mut frame = Vec::new();
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        instance.window().request_redraw();
        frame = Headless::render().unwrap_or_default();
    }
    frame
}

fn previews(dir: &Path, sizes: &[(u32, u32)], out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.to_string_lossy().ends_with(".preview.slint") {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut failures = Vec::new();
    let mut rendered = 0;
    for file in &files {
        let compiler = slint_interpreter::Compiler::default();
        let result = pollster::block_on(compiler.build_from_path(file));
        let name = file
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace(".preview.slint", "");
        if result.has_errors() {
            let errors: Vec<String> = result.diagnostics().map(|d| d.to_string()).collect();
            failures.push(format!("{name}: does not compile: {}", errors.join("; ")));
            continue;
        }
        let names: Vec<String> = result.component_names().map(str::to_owned).collect();
        if names.is_empty() {
            failures.push(format!("{name}: exports no state"));
        }
        for component in names {
            let definition = result.component(&component).context("component")?;
            let instance = definition.create()?;
            instance.show()?;
            for &size in sizes {
                let frame = render(&instance, size);
                let ground = frame.first().copied().unwrap_or_default();
                if frame.is_empty() || frame.iter().all(|&p| p == ground) {
                    failures.push(format!(
                        "{name}.{component} at {}x{}: draws nothing",
                        size.0, size.1
                    ));
                }
                save(
                    &frame,
                    size,
                    &out.join(format!("{name}.{component}.{}x{}.png", size.0, size.1)),
                )?;
                rendered += 1;
            }
            instance.hide()?;
        }
    }
    println!("{} preview files, {rendered} renders", files.len());
    for failure in &failures {
        println!("FAIL {failure}");
    }
    if !failures.is_empty() {
        bail!("{} previews failed", failures.len());
    }
    Ok(())
}

/// Korri as the probe plays it: the base model, a cursor window over games.
struct Korri {
    base: Value,
    games: Vec<Value>,
    fixed: Option<Value>,
    query: Value,
    from: Value,
    count: usize,
    focused: Option<String>,
}

impl Korri {
    fn model(&self) -> Value {
        let mut model = self.base.clone();
        model["catalog"] = match &self.fixed {
            Some(catalog) => catalog.clone(),
            None => catalog::window(&self.games, &self.query, &self.from, self.count),
        };
        if let Some(id) = &self.focused {
            let actions = self.base["fixtureGameActions"][id].clone();
            model["focusedGame"] = json!({"gameId": id, "actions": if actions.is_array() { actions } else { json!([]) }});
        }
        if let Some(object) = model.as_object_mut() {
            object.remove("fixtureGameActions");
        }
        model
    }

    /// Answer the requests korrid would answer; true when the model changed.
    fn answer(&mut self, requests: &[Value]) -> bool {
        let mut changed = false;
        for request in requests {
            match messages::request_tag(request) {
                Some("MoveCatalog") => {
                    self.query = request["query"].clone();
                    self.from = request["from"].clone();
                    self.count = request["count"].as_u64().unwrap_or(50) as usize;
                    changed = true;
                }
                Some("FocusGame") => {
                    self.focused = request["gameId"].as_str().map(str::to_owned);
                    changed = true;
                }
                _ => {}
            }
        }
        changed
    }
}

/// Every `"focused": true` in a view, cleared.
fn unfocused(view: &Value) -> Value {
    match view {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        if k == "focused" {
                            json!(false)
                        } else {
                            unfocused(v)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(unfocused).collect()),
        other => other.clone(),
    }
}

fn has_focus(view: &Value) -> bool {
    match view {
        Value::Object(map) => map
            .iter()
            .any(|(k, v)| (k == "focused" && v == &json!(true)) || has_focus(v)),
        Value::Array(items) => items.iter().any(has_focus),
        _ => false,
    }
}

/// A focus target as drawn: its box (logical px) and its identity.
#[derive(Clone, Debug)]
struct Drawn {
    rect: Rect,
    /// The view entry without `focused` and booleans, plus its occurrence.
    identity: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// Paths (keys and indices) of every object in a view that has `focused`.
fn targets(view: &Value, path: &mut Vec<Value>, out: &mut Vec<Vec<Value>>) {
    match view {
        Value::Object(map) => {
            if map.contains_key("focused") {
                out.push(path.clone());
            }
            for (k, v) in map {
                path.push(json!(k));
                targets(v, path, out);
                path.pop();
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                path.push(json!(i));
                targets(v, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

fn at_mut<'a>(view: &'a mut Value, path: &[Value]) -> &'a mut Value {
    path.iter().fold(view, |v, step| match step {
        Value::String(k) => &mut v[k.as_str()],
        Value::Number(i) => &mut v[i.as_u64().unwrap_or_default() as usize],
        _ => v,
    })
}

/// An entry's identity: the entry without `focused` or any boolean.
fn identity(entry: &Value) -> String {
    match entry {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, v)| k.as_str() != "focused" && !v.is_boolean())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
        .to_string(),
        other => other.to_string(),
    }
}

/// The box of the largest connected group of pixels that change with the
/// `focused` flags (and stay put between two unfocused frames): the focus
/// mark. Other groups (a panel describing the focused item) are smaller.
fn changed_box(a: &[u16], b: &[u16], c: &[u16], width: u32) -> Option<Rect> {
    // 4 px cells, so a dotted ring or anti-aliased text joins into one group.
    const CELL: u32 = 4;
    let height = a.len() as u32 / width.max(1);
    let (cw, ch) = (width.div_ceil(CELL), height.div_ceil(CELL));
    let mut cells = vec![false; (cw * ch) as usize];
    for (i, ((p, q), r)) in a.iter().zip(b).zip(c).enumerate() {
        if p != q && q == r {
            let (x, y) = (i as u32 % width, i as u32 / width);
            cells[((y / CELL) * cw + x / CELL) as usize] = true;
        }
    }
    let mut best: Option<(usize, (u32, u32, u32, u32))> = None;
    let mut seen = vec![false; cells.len()];
    for start in 0..cells.len() {
        if !cells[start] || seen[start] {
            continue;
        }
        let mut stack = vec![start];
        seen[start] = true;
        let (mut count, mut x0, mut y0, mut x1, mut y1) = (0, u32::MAX, u32::MAX, 0, 0);
        while let Some(i) = stack.pop() {
            count += 1;
            let (x, y) = (i as u32 % cw, i as u32 / cw);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            for (nx, ny) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ] {
                if nx < cw && ny < ch {
                    let j = (ny * cw + nx) as usize;
                    if cells[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        if best.is_none_or(|(n, _)| count > n) {
            best = Some((count, (x0, y0, x1, y1)));
        }
    }
    best.map(|(_, (x0, y0, x1, y1))| Rect {
        x: (x0 * CELL) as f32,
        y: (y0 * CELL) as f32,
        w: ((x1 - x0 + 1) * CELL) as f32,
        h: ((y1 - y0 + 1) * CELL) as f32,
    })
}

/// One walked state at one size.
struct State {
    path: Vec<&'static str>,
    /// Every focus target drawn in the window.
    targets: Vec<Drawn>,
    /// The focused target, when it is drawn.
    focused: Option<usize>,
}

/// The portal's spatial-focus rule (spatial-focus.ts): the target ahead by
/// more than 1 px with the lowest along + 3 x across; the first in tree order
/// on a tie. None when nothing lies that way.
fn nearest(state: &State, from: usize, direction: &str) -> Option<usize> {
    const OFF_AXIS_PENALTY: f32 = 3.0;
    let (fx, fy) = state.targets[from].rect.center();
    let mut best: Option<(usize, f32)> = None;
    for (i, candidate) in state.targets.iter().enumerate() {
        if i == from {
            continue;
        }
        let (cx, cy) = candidate.rect.center();
        let (dx, dy) = (cx - fx, cy - fy);
        let along = match direction {
            "left" => -dx,
            "right" => dx,
            "up" => -dy,
            _ => dy,
        };
        if along <= 1.0 {
            continue;
        }
        let across = if direction == "left" || direction == "right" {
            dy.abs()
        } else {
            dx.abs()
        };
        let score = along + across * OFF_AXIS_PENALTY;
        if best.is_none_or(|(_, s)| score < s) {
            best = Some((i, score));
        }
    }
    best.map(|(i, _)| i)
}

const DIRECTIONS: [&str; 4] = ["up", "down", "left", "right"];

/// A display list larger than any window the probe draws (A2 8.3).
fn large_displays() -> Value {
    json!([
        {"id": "large", "main": true, "x": 0, "y": 0, "width": 3840, "height": 2160, "touch": false},
        {"id": "side", "main": false, "x": 3840, "y": 0, "width": 2560, "height": 1440, "touch": false},
    ])
}

fn walk(
    dir: &Path,
    model_file: &Path,
    size: (u32, u32),
    max: usize,
    out: &Path,
) -> Result<(usize, Vec<String>)> {
    let package = Package::at(dir)?;
    let views = package::compile(&package)?;
    let source = package.logic_source()?;
    let mut model: Value = serde_json::from_str(&std::fs::read_to_string(model_file)?)?;
    let catalog = model["catalog"].take();
    model["displays"] = large_displays();
    let (games, fixed) = if catalog["_tag"] == "Ready" {
        (
            catalog["games"].as_array().cloned().unwrap_or_default(),
            None,
        )
    } else {
        (Vec::new(), Some(catalog))
    };
    let overlay = model["presentation"]["kind"] == "gameplay-overlay";
    let base_korri = Korri {
        base: model,
        games,
        fixed,
        query: catalog::default_query(),
        from: json!({"_tag": "Start"}),
        count: catalog::DEFAULT_COUNT,
        focused: None,
    };
    let definition = if overlay {
        views.overlay.clone().context("no Overlay")?
    } else {
        views.surface.clone()
    };
    let instance = definition.create()?;
    instance.show()?;
    let mut images = ImageCache::new(&package.dir);
    let presses: Vec<&'static str> = vec![
        "up", "down", "left", "right", "confirm", "back", "menu", "options",
    ];
    // (path, parent state, direction pressed from it)
    let mut queue: VecDeque<(Vec<&'static str>, Option<usize>, &'static str)> =
        VecDeque::from([(vec![], None, "")]);
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut walked: Vec<State> = Vec::new();
    let mut edges: Vec<(usize, &'static str, usize)> = Vec::new();
    let mut failures = Vec::new();
    while let Some((path, parent, pressed)) = queue.pop_front() {
        if walked.len() >= max {
            break;
        }
        // Replay the presses from a fresh start: the logic's state stays inside QuickJS.
        let mut korri = Korri {
            ..clone_korri(&base_korri)
        };
        let mut engine = Engine::load(
            &source,
            Limits {
                call: std::time::Duration::from_secs(5),
                ..Limits::default()
            },
        )?;
        engine.set_model(&korri.model().to_string())?;
        engine.init(&engine::effective_preferences(
            engine.declaration(),
            &BTreeMap::new(),
        ))?;
        let deliver = |engine: &mut Engine, korri: &mut Korri, message: Value| -> Result<()> {
            let mut queue = vec![message];
            let mut rounds = 0;
            while let Some(message) = queue.pop() {
                rounds += 1;
                if rounds > 16 {
                    bail!("requests keep causing requests");
                }
                let requests = engine.update(&message)?;
                if korri.answer(&requests) {
                    engine.set_model(&korri.model().to_string())?;
                    queue.push(messages::korri_published());
                }
            }
            Ok(())
        };
        // A2 (8.1): the window's size right after init, before any view.
        deliver(
            &mut engine,
            &mut korri,
            messages::resized(size.0 as f32, size.1 as f32),
        )?;
        deliver(&mut engine, &mut korri, messages::korri_published())?;
        for press in &path {
            let message = json!({"_tag": "Pressed", "button": press});
            deliver(&mut engine, &mut korri, message)?;
        }
        let view = if overlay {
            engine.overlay()?.unwrap_or(json!({}))
        } else {
            engine.view()?
        };
        let key = view.to_string();
        if let Some(&index) = seen.get(&key) {
            if let Some(parent) = parent {
                edges.push((parent, pressed, index));
            }
            continue;
        }
        let index = walked.len();
        seen.insert(key, index);
        if let Some(parent) = parent {
            edges.push((parent, pressed, index));
        }
        let mut state = State {
            path: path.clone(),
            targets: Vec::new(),
            focused: None,
        };
        if has_focus(&view) {
            let mut binder = Binder::new(&definition);
            binder.apply(&instance, &view, &mut images)?;
            let a = render(&instance, size);
            let quiet_view = unfocused(&view);
            let mut quiet = Binder::new(&definition);
            quiet.apply(&instance, &quiet_view, &mut images)?;
            let b = render(&instance, size);
            // Clocks (the walking meter, the focus cycle) can move between
            // frames: a third frame tells motion from focus.
            let mut again = Binder::new(&definition);
            again.apply(&instance, &quiet_view, &mut images)?;
            let c = render(&instance, size);
            if changed_box(&a, &b, &c, size.0).is_none() {
                let name = format!("unseen-{}x{}-{}.png", size.0, size.1, failures.len());
                std::fs::create_dir_all(out)?;
                save(&a, size, &out.join(&name))?;
                failures.push(format!(
                    "{} at {}x{}: focus not visible after [{}] ({name})",
                    model_file.display(),
                    size.0,
                    size.1,
                    path.join(" ")
                ));
            } else {
                // A2 (8.3): every target's drawn box, one entry focused at a time.
                let mut paths = Vec::new();
                targets(&view, &mut Vec::new(), &mut paths);
                let mut occurrences: HashMap<String, usize> = HashMap::new();
                for target in &paths {
                    let mut alone = quiet_view.clone();
                    at_mut(&mut alone, target)["focused"] = json!(true);
                    Binder::new(&definition).apply(&instance, &alone, &mut images)?;
                    let lit = render_quick(&instance);
                    let mut whole = view.clone();
                    let entry = at_mut(&mut whole, target).clone();
                    let base = identity(&entry);
                    let n = occurrences.entry(base.clone()).or_default();
                    *n += 1;
                    if let Some(rect) = changed_box(&lit, &b, &c, size.0) {
                        if entry["focused"] == json!(true) {
                            state.focused = Some(state.targets.len());
                        }
                        state.targets.push(Drawn {
                            rect,
                            identity: format!("{base}#{n}"),
                        });
                    }
                }
            }
            // Restore every property for the next state.
            Binder::new(&definition).apply(&instance, &view, &mut images)?;
        }
        walked.push(state);
        if path.len() < 40 {
            for press in &presses {
                let mut next = path.clone();
                next.push(press);
                queue.push_back((next, Some(index), press));
            }
        }
    }
    instance.hide()?;
    failures.extend(directions(model_file, size, &walked, &edges));
    Ok((walked.len(), failures))
}

/// A2 (8.3): every direction from every focused state goes where the
/// geometry says.
fn directions(
    model_file: &Path,
    size: (u32, u32),
    walked: &[State],
    edges: &[(usize, &'static str, usize)],
) -> Vec<String> {
    let mut failures = Vec::new();
    for &(from, pressed, to) in edges {
        if !DIRECTIONS.contains(&pressed) {
            continue;
        }
        let (before, after) = (&walked[from], &walked[to]);
        let (Some(focused), Some(now)) = (before.focused, after.focused) else {
            continue;
        };
        let expected = nearest(before, focused, pressed);
        let want = expected.map_or(&before.targets[focused], |i| &before.targets[i]);
        let got = &after.targets[now];
        if want.identity != got.identity {
            failures.push(format!(
                "{} at {}x{}: [{}] then {pressed} focuses {} at ({:.0},{:.0}); the nearest {pressed} is {}",
                model_file.display(),
                size.0,
                size.1,
                before.path.join(" "),
                got.identity,
                got.rect.x,
                got.rect.y,
                match expected {
                    Some(_) => format!("{} at ({:.0},{:.0})", want.identity, want.rect.x, want.rect.y),
                    None => format!("none, so focus stays on {}", want.identity),
                }
            ));
        }
    }
    failures
}

fn clone_korri(k: &Korri) -> Korri {
    Korri {
        base: k.base.clone(),
        games: k.games.clone(),
        fixed: k.fixed.clone(),
        query: k.query.clone(),
        from: k.from.clone(),
        count: k.count,
        focused: k.focused.clone(),
    }
}

fn main() -> Result<()> {
    Headless::install();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let out = PathBuf::from(flag("--out").unwrap_or_else(|| "/tmp/surface-probe".into()));
    let sizes = sizes(
        &flag("--sizes").unwrap_or_else(|| "320x240,640x480,480x800,1280x720,1920x1080".into()),
    );
    match args.first().map(String::as_str) {
        Some("previews") => previews(Path::new(args.get(1).context("DIR")?), &sizes, &out),
        Some("load") => {
            // How long a package's logic takes to load, as the shell measures it.
            let package = Package::at(Path::new(args.get(1).context("PACKAGE")?))?;
            let source = package.logic_source()?;
            let mut times = Vec::new();
            for _ in 0..20 {
                let started = std::time::Instant::now();
                Engine::load(&source, Limits { call: std::time::Duration::from_secs(5), ..Limits::default() })?;
                times.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            println!("load ms: min {:.1} median {:.1} max {:.1}", times[0], times[10], times[19]);
            Ok(())
        }
        Some("walk") => {
            let dir = PathBuf::from(args.get(1).context("PACKAGE")?);
            let max = flag("--max").and_then(|m| m.parse().ok()).unwrap_or(300);
            let mut all = Vec::new();
            for model in flag("--model").context("--model FILE[,FILE…]")?.split(',') {
                for &size in &sizes {
                    let (states, failures) = walk(&dir, Path::new(model), size, max, &out)?;
                    println!("{model} {}x{}: {states} states, {} failures", size.0, size.1, failures.len());
                    all.extend(failures);
                }
            }
            for failure in &all {
                println!("FAIL {failure}");
            }
            if !all.is_empty() {
                bail!("{} focus failures", all.len());
            }
            Ok(())
        }
        _ => bail!("usage: surface-probe previews DIR --sizes … --out OUT | walk PACKAGE --model FILE --sizes … [--max N] [--out OUT]"),
    }
}
