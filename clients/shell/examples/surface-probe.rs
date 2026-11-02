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
//!     failure, written to OUT with the presses that reached it. FILE's own
//!     `displays` reach the logic unchanged: only the window takes each size,
//!     so a package that sizes itself from a display instead of its window
//!     fails here.
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
use std::collections::{BTreeMap, HashSet, VecDeque};
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
    let presses: Vec<&str> = vec![
        "up", "down", "left", "right", "confirm", "back", "menu", "options",
    ];
    let mut queue: VecDeque<Vec<&str>> = VecDeque::from([vec![]]);
    let mut seen: HashSet<String> = HashSet::new();
    let mut failures = Vec::new();
    let mut states = 0;
    while let Some(path) = queue.pop_front() {
        if states >= max {
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
        if !seen.insert(key) {
            continue;
        }
        states += 1;
        if has_focus(&view) {
            let mut binder = Binder::new(&definition);
            binder.apply(&instance, &view, &mut images)?;
            let a = render(&instance, size);
            let mut quiet = Binder::new(&definition);
            quiet.apply(&instance, &unfocused(&view), &mut images)?;
            let b = render(&instance, size);
            // Clocks (the walking meter, the focus cycle) can move between
            // frames: a third frame tells motion from focus.
            let mut again = Binder::new(&definition);
            again.apply(&instance, &unfocused(&view), &mut images)?;
            let c = render(&instance, size);
            let differs = a
                .iter()
                .zip(&b)
                .zip(&c)
                .filter(|((x, y), z)| x != y && y == z)
                .count();
            if differs == 0 {
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
            }
            // Restore every property for the next state.
            Binder::new(&definition).apply(&instance, &view, &mut images)?;
        }
        if path.len() < 40 {
            for press in &presses {
                let mut next = path.clone();
                next.push(press);
                queue.push_back(next);
            }
        }
    }
    instance.hide()?;
    Ok((states, failures))
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
    let sizes = sizes(&flag("--sizes").unwrap_or_else(|| "640x480".into()));
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
                    println!("{model} {}x{}: {states} states, {} unseen", size.0, size.1, failures.len());
                    all.extend(failures);
                }
            }
            for failure in &all {
                println!("FAIL {failure}");
            }
            if !all.is_empty() {
                bail!("{} focus states not visible", all.len());
            }
            Ok(())
        }
        _ => bail!("usage: surface-probe previews DIR --sizes … --out OUT | walk PACKAGE --model FILE --sizes … [--max N] [--out OUT]"),
    }
}
