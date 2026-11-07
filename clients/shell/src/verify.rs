//! Install-time verification and the startup index (contract 3.9).
//!
//! At install, [`verify`] compiles `main.slint` (`Surface`, and `Overlay`
//! when exported), checks the preferences declaration, and sweeps every
//! message plus 500 seeded presses against fixture models of 0, 12 and
//! 10,000 games, checking every `view()` and `overlay()` against the declared
//! properties and every request against the treaty. A small in-process
//! catalog answers `MoveCatalog`, `FocusGame` and `RememberPreference` the
//! way korrid would, so paging is exercised. The result is a derived summary
//! in the shell's cache.
//!
//! At startup, [`Index::load`] reads one combined index and compares file
//! sizes and modification times. It never hashes. A package that is missing
//! from the index or changed on disk is verified again, and the index is
//! rewritten. The summary and the index are the shell's cache, not
//! plugin-host schema, and are rebuilt whenever they are missing.

use crate::{
    catalog,
    engine::{self, Engine, Limits},
    images::ImageCache,
    messages,
    package::{self, Package},
    targets, view,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use slint::ComponentHandle;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

/// Changes whenever the checks change; older summaries are re-derived.
pub const VERIFIER: &str = "korri-shell-verify-3";
pub const SLINT: &str = "1.18.1";
pub const FIXTURE_GAMES: [usize; 3] = [0, 12, 10_000];
pub const SEEDED_PRESSES: usize = 500;

/// Contract A2 (8.3): every package is swept at these window sizes, in
/// logical pixels.
pub const RESIZED_SIZES: [(f32, f32); 5] = [
    (320.0, 240.0),
    (640.0, 480.0),
    (480.0, 800.0),
    (1280.0, 720.0),
    (1920.0, 1080.0),
];
pub const SEED: u32 = 0x00c0_ffee;
const FIXTURE_MODEL: &str = include_str!("../fixtures/model.json");

/// The fixture model (contract shape) and its 12 example games.
pub fn fixture() -> (Value, Vec<Value>) {
    let mut model: Value = serde_json::from_str(FIXTURE_MODEL).expect("fixture model is JSON");
    let games = model["catalog"]["games"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    model["catalog"] = json!({"_tag": "Loading"});
    (model, games)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FixtureRun {
    pub games: usize,
    pub overlay: bool,
    /// Displays in `model.displays`; 0 when the model has none.
    pub displays: usize,
    pub views: usize,
    pub updates: usize,
    pub requests: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub verifier: String,
    pub slint: String,
    pub surface_id: String,
    pub package_sha256: String,
    pub diagnostics: Vec<String>,
    pub preferences: engine::Declaration,
    /// `globalThis.surface.displays`; absent when the package left it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub displays: Option<engine::Displays>,
    pub has_overlay: bool,
    pub runs: Vec<FixtureRun>,
    pub max_call_ms: f64,
    /// The slowest load measured: bundle evaluation through `init`, against
    /// the 2 s load budget.
    pub load_ms: f64,
    pub peak_heap_bytes: usize,
    pub pass: bool,
    pub reasons: Vec<String>,
}

/// SHA-256 over every file's relative path and bytes, sorted. Install only.
pub fn digest(dir: &Path) -> Result<String> {
    let mut sha = Sha256::new();
    for (relative, _) in files(dir)? {
        let data = std::fs::read(dir.join(&relative))?;
        let name = relative.to_string_lossy();
        sha.update((name.len() as u64).to_le_bytes());
        sha.update(name.as_bytes());
        sha.update((data.len() as u64).to_le_bytes());
        sha.update(&data);
    }
    Ok(format!("{:x}", sha.finalize()))
}

/// `(path, (size, mtime ns))` for every regular file, sorted. Symlinks fail.
pub fn files(dir: &Path) -> Result<Vec<(PathBuf, (u64, u128))>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, (u64, u128))>) -> Result<()> {
        let mut entries = std::fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let kind = entry.file_type()?;
            let path = entry.path();
            if kind.is_symlink() {
                bail!("symlink in package: {}", path.display());
            } else if kind.is_dir() {
                walk(root, &path, out)?;
            } else if kind.is_file() {
                let metadata = entry.metadata()?;
                let mtime = metadata
                    .modified()?
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                out.push((path.strip_prefix(root)?.to_owned(), (metadata.len(), mtime)));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out)?;
    Ok(out)
}

/// A mini korrid for the sweep.
struct Sweep<'a> {
    base: Value,
    games: &'a [Value],
    query: Value,
    from: Value,
    count: usize,
    focused: Option<String>,
    overlay: bool,
    displays: Option<Value>,
}

/// A display list larger than any swept window (A2 8.3): a package must lay
/// out from `Resized`, never from `displays`.
fn large_displays() -> Value {
    json!([
        {"id": "large", "main": true, "x": 0, "y": 0, "width": 3840, "height": 2160, "touch": false},
        {"id": "side", "main": false, "x": 3840, "y": 0, "width": 2560, "height": 1440, "touch": false},
    ])
}

/// Two 640x480 touch displays, the main one above: the Anbernic RG DS as
/// Pico's own fixture states it (`packages/pico/test/fixtures/displays.ts`).
fn two_displays() -> Value {
    json!([
        {"id": "top", "main": true, "x": 0, "y": 0, "width": 640, "height": 480, "touch": true},
        {"id": "bottom", "main": false, "x": 0, "y": 480, "width": 640, "height": 480, "touch": true},
    ])
}

impl Sweep<'_> {
    fn model(&self) -> Value {
        let mut model = self.base.clone();
        if let Some(displays) = &self.displays {
            model["displays"] = displays.clone();
        }
        model["catalog"] = catalog::window(self.games, &self.query, &self.from, self.count);
        if let Some(id) = &self.focused {
            model["focusedGame"] = json!({"gameId": id, "actions": []});
        }
        if self.overlay {
            model["presentation"] = json!({"kind": "gameplay-overlay", "title": "Paused", "controls": [{"id": "resume", "label": "Resume", "enabled": true, "destructive": false, "dismissOnSuccess": true, "interaction": {"kind": "command"}}], "groups": []});
        }
        model
    }
}

struct Checker<'a> {
    engine: Engine,
    surface: &'a BTreeMap<String, i_slint_compiler::langtype::Type>,
    overlay: Option<&'a BTreeMap<String, i_slint_compiler::langtype::Type>>,
    images: ImageCache,
    run: FixtureRun,
}

impl Checker<'_> {
    fn views(&mut self, sweep: &Sweep) -> Result<()> {
        let surface_view = self.engine.view()?;
        view::check(&surface_view, self.surface, &mut self.images).context("view()")?;
        view::check_focus_plan(&surface_view).context("view() focus plan")?;
        self.run.views += 1;
        if sweep.overlay {
            if let (Some(types), Some(overlay)) = (self.overlay, self.engine.overlay()?) {
                view::check(&overlay, types, &mut self.images).context("overlay()")?;
                view::check_focus_plan(&overlay).context("overlay() focus plan")?;
                self.run.views += 1;
            }
        }
        Ok(())
    }

    /// One message, then everything its requests cause, as korrid would.
    fn deliver(&mut self, sweep: &mut Sweep, message: Value) -> Result<()> {
        let mut queue = vec![message];
        let mut rounds = 0;
        while let Some(message) = queue.pop() {
            rounds += 1;
            if rounds > 16 {
                bail!("requests keep causing more requests");
            }
            let requests = self
                .engine
                .update(&message)
                .with_context(|| format!("update({message})"))?;
            self.run.updates += 1;
            self.views(sweep)?;
            let mut republish = false;
            for request in requests {
                self.run.requests += 1;
                let tag = messages::request_tag(&request)
                    .with_context(|| format!("unknown request {}", short(&request)))?;
                match tag {
                    "RememberPreference" => {
                        let (key, value) = messages::remembered(&request)
                            .context("RememberPreference needs key and value")?;
                        if !self.engine.declaration().contains_key(&key)
                            || value.len() > engine::MAX_PREFERENCE_BYTES
                        {
                            bail!("RememberPreference of undeclared key or oversized value: {key}");
                        }
                        queue.push(messages::preference_stored(&key));
                    }
                    "MoveCatalog" => {
                        let count = request
                            .get("count")
                            .and_then(Value::as_u64)
                            .context("MoveCatalog count")?;
                        if count == 0 || count > messages::MAX_CATALOG_COUNT {
                            bail!("MoveCatalog count {count} is not 1..=200");
                        }
                        sweep.query = request.get("query").cloned().context("MoveCatalog query")?;
                        sweep.from = request.get("from").cloned().context("MoveCatalog from")?;
                        sweep.count = count as usize;
                        republish = true;
                    }
                    "FocusGame" => {
                        sweep.focused = request
                            .get("gameId")
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                        republish = true;
                    }
                    "DismissGameplayOverlay" if sweep.overlay => {
                        sweep.overlay = false;
                        republish = true;
                    }
                    _ => {}
                }
            }
            if republish {
                self.engine.set_model(&sweep.model().to_string())?;
                queue.push(messages::korri_published());
            }
        }
        Ok(())
    }
}

fn short(value: &Value) -> String {
    value.to_string().chars().take(120).collect()
}

/// What one fixture sweep checks against.
struct Plan<'a> {
    source: &'a str,
    package: &'a Package,
    surface: &'a BTreeMap<String, i_slint_compiler::langtype::Type>,
    overlay: Option<&'a BTreeMap<String, i_slint_compiler::langtype::Type>>,
    limits: Limits,
}

/// The heaviest call and heap across sweeps.
#[derive(Default)]
struct Peaks {
    heap: usize,
    call: Duration,
    load: Duration,
}

fn sweep_one(
    plan: &Plan,
    games: &[Value],
    gameplay_overlay: bool,
    displays: Option<Value>,
    peaks: &mut Peaks,
) -> Result<FixtureRun> {
    let Plan {
        source,
        package,
        surface,
        overlay,
        limits,
    } = *plan;
    let (base, _) = fixture();
    let display_count = displays
        .as_ref()
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut sweep = Sweep {
        base,
        games,
        query: catalog::default_query(),
        from: json!({"_tag": "Start"}),
        count: catalog::DEFAULT_COUNT,
        focused: None,
        overlay: gameplay_overlay,
        displays,
    };
    let engine = Engine::load(source, limits)?;
    let mut checker = Checker {
        engine,
        surface,
        overlay,
        images: ImageCache::new(&package.dir),
        run: FixtureRun {
            games: games.len(),
            overlay: gameplay_overlay,
            displays: display_count,
            views: 0,
            updates: 0,
            requests: 0,
        },
    };
    let outcome = (|| -> Result<()> {
        let preferences =
            engine::effective_preferences(checker.engine.declaration(), &BTreeMap::new());
        checker.engine.set_model(&sweep.model().to_string())?;
        checker.engine.init(&preferences)?;
        // A2 (8.1): one Resized after init, before the first view.
        let first = messages::resized(640.0, 480.0);
        checker.engine.update(&first)?;
        checker.views(&sweep)?;
        let keys: Vec<String> = checker.engine.declaration().keys().cloned().collect();
        let mut every: Vec<Value> = messages::BUTTONS
            .iter()
            .map(|b| messages::pressed(*b))
            .collect();
        every.push(messages::korri_published());
        every.extend([0, 1_000, 60_000, u32::MAX].map(messages::tick));
        every.extend(RESIZED_SIZES.map(|(w, h)| messages::resized(w, h)));
        every.extend(keys.iter().map(|k| messages::preference_stored(k)));
        for message in every {
            checker.deliver(&mut sweep, message)?;
        }
        let mut seed = SEED;
        for _ in 0..SEEDED_PRESSES {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let button = messages::BUTTONS[seed as usize % messages::BUTTONS.len()];
            checker.deliver(&mut sweep, messages::pressed(button))?;
            // A tick between presses, as the shell sends once a second.
            if seed.is_multiple_of(16) {
                let (w, h) = RESIZED_SIZES[(seed >> 4) as usize % RESIZED_SIZES.len()];
                checker.deliver(&mut sweep, messages::resized(w, h))?;
            }
            if seed.is_multiple_of(4) {
                let idle = (seed >> 8) % 600_000;
                checker.deliver(&mut sweep, messages::tick(idle))?;
            }
        }
        Ok(())
    })();
    peaks.heap = peaks.heap.max(checker.engine.heap_bytes());
    peaks.call = peaks.call.max(checker.engine.max_call());
    peaks.load = peaks.load.max(checker.engine.load_time());
    outcome.map(|()| checker.run.clone()).with_context(|| {
        format!(
            "{} games{}",
            games.len(),
            if gameplay_overlay {
                ", gameplay overlay"
            } else {
                ""
            }
        )
    })
}

/// README, Focus targets: in the first view at 640x480 and after one press of each button,
/// every focus entry is drawn by exactly one element that sets `accessible-item-selectable`
/// and is `accessible-item-selected` when that entry alone is focused (`targets::drawn`).
fn drawn_targets(
    plan: &Plan,
    definition: &slint_interpreter::ComponentDefinition,
    games: &[Value],
) -> Result<()> {
    crate::backend::headless::Headless::install();
    let (base, _) = fixture();
    let sweep = Sweep {
        base,
        games,
        query: catalog::default_query(),
        from: json!({"_tag": "Start"}),
        count: catalog::DEFAULT_COUNT,
        focused: None,
        overlay: false,
        displays: None,
    };
    let instance = definition.create()?;
    instance
        .window()
        .set_size(slint::PhysicalSize::new(640, 480));
    let mut binder = view::Binder::new(definition);
    let mut images = ImageCache::new(&plan.package.dir);
    let mut presses = vec![None];
    presses.extend(messages::BUTTONS.iter().map(|b| Some(*b)));
    for press in presses {
        let mut engine = Engine::load(plan.source, plan.limits)?;
        engine.set_model(&sweep.model().to_string())?;
        engine.init(&engine::effective_preferences(
            engine.declaration(),
            &BTreeMap::new(),
        ))?;
        engine.update(&messages::resized(640.0, 480.0))?;
        if let Some(button) = press {
            engine.update(&messages::pressed(button))?;
        }
        let view = engine.view()?;
        targets::drawn(&instance, &mut binder, &view, &mut images).with_context(
            || match press {
                Some(button) => format!("after {button:?}"),
                None => "in the first view".to_owned(),
            },
        )?;
    }
    Ok(())
}

/// Verify one package. Always returns a summary; `pass` says whether it may load.
pub fn verify(package: &Package, limits: Limits) -> Summary {
    // Verification is a build/test gate, not device response time (#368).
    let limits = Limits {
        clock: crate::budget_clock::BudgetClock::ThreadCpu,
        ..limits
    };
    let mut summary = Summary {
        verifier: VERIFIER.into(),
        slint: SLINT.into(),
        surface_id: package.id.clone(),
        package_sha256: String::new(),
        diagnostics: Vec::new(),
        preferences: engine::Declaration::new(),
        displays: None,
        has_overlay: false,
        runs: Vec::new(),
        max_call_ms: 0.0,
        load_ms: 0.0,
        peak_heap_bytes: 0,
        pass: false,
        reasons: Vec::new(),
    };
    let mut peaks = Peaks::default();
    let outcome = (|| -> Result<()> {
        summary.package_sha256 = digest(&package.dir)?;
        let views = package::compile(package)?;
        summary.diagnostics = views.diagnostics.clone();
        let surface = view::declared(&views.surface);
        let overlay = views.overlay.as_ref().map(view::declared);
        let source = package.logic_source()?;
        let probe = Engine::load(&source, limits)?;
        summary.preferences = probe.declaration().clone();
        summary.displays = probe.displays();
        summary.has_overlay = probe.has_overlay();
        if probe.has_overlay() != views.overlay.is_some() {
            bail!("surface.overlay and the exported Overlay component must come together");
        }
        let displays = probe.displays();
        drop(probe);
        let (_, examples) = fixture();
        let plan = Plan {
            source: &source,
            package,
            surface: &surface,
            overlay: overlay.as_ref(),
            limits,
        };
        for count in FIXTURE_GAMES {
            let games = catalog::games(&examples, count);
            summary
                .runs
                .push(sweep_one(&plan, &games, false, None, &mut peaks)?);
        }
        if displays == Some(engine::Displays::All) {
            // korrid publishes `model.displays` only to an "all" surface.
            let games = catalog::games(&examples, 12);
            summary.runs.push(sweep_one(
                &plan,
                &games,
                false,
                Some(two_displays()),
                &mut peaks,
            )?);
        }
        {
            let games = catalog::games(&examples, 12);
            summary.runs.push(sweep_one(
                &plan,
                &games,
                false,
                Some(large_displays()),
                &mut peaks,
            )?);
        }
        if overlay.is_some() {
            let games = catalog::games(&examples, 12);
            summary
                .runs
                .push(sweep_one(&plan, &games, true, None, &mut peaks)?);
        }
        drawn_targets(&plan, &views.surface, &catalog::games(&examples, 12))
            .context("focus targets (README, Focus targets)")?;
        Ok(())
    })();
    summary.max_call_ms = peaks.call.as_secs_f64() * 1e3;
    summary.load_ms = peaks.load.as_secs_f64() * 1e3;
    summary.peak_heap_bytes = peaks.heap;
    match outcome {
        Ok(()) => summary.pass = true,
        Err(error) => summary.reasons.push(format!("{error:#}")),
    }
    summary
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub dir: PathBuf,
    pub pass: bool,
    /// `(relative path, size, mtime ns)` of every file at verification.
    pub files: Vec<(PathBuf, u64, u128)>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub verifier: String,
    pub entries: Vec<Entry>,
}

/// Where the derived files live: `<cache>/surfaces/<id>.json` and `<cache>/index.json`.
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
        }
    }

    /// `$XDG_CACHE_HOME/korri-shell`, else `$HOME/.cache/korri-shell`.
    pub fn default_root() -> Option<PathBuf> {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .map(|base| base.join("korri-shell"))
    }

    fn index_path(&self) -> PathBuf {
        self.root.join("index.json")
    }

    pub fn summary_path(&self, id: &str) -> PathBuf {
        self.root.join("surfaces").join(format!("{id}.json"))
    }

    pub fn read_index(&self) -> Option<Index> {
        let index: Index = serde_json::from_slice(&std::fs::read(self.index_path()).ok()?).ok()?;
        (index.verifier == VERIFIER).then_some(index)
    }

    fn write(&self, path: &Path, value: &impl Serialize) -> Result<()> {
        std::fs::create_dir_all(path.parent().context("cache path")?)?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
        std::fs::rename(temporary, path)?;
        Ok(())
    }

    /// Verify a package and record its summary and index entry.
    pub fn install(&self, package: &Package, limits: Limits) -> Result<Summary> {
        let before = files(&package.dir)?;
        let summary = verify(package, limits);
        let after = files(&package.dir)?;
        if before != after {
            bail!("{} changed during verification", package.id);
        }
        self.write(&self.summary_path(&package.id), &summary)?;
        let mut index = self.read_index().unwrap_or_else(|| Index {
            verifier: VERIFIER.into(),
            entries: Vec::new(),
        });
        index.entries.retain(|e| e.id != package.id);
        index.entries.push(Entry {
            id: package.id.clone(),
            dir: package.dir.clone(),
            pass: summary.pass,
            files: after.into_iter().map(|(p, (s, m))| (p, s, m)).collect(),
        });
        index.entries.sort_by(|a, b| a.id.cmp(&b.id));
        self.write(&self.index_path(), &index)?;
        Ok(summary)
    }

    /// Startup: the packages that may load, in id order. Reads the index and
    /// file metadata only; a package without a current entry is verified
    /// now, which also rebuilds a missing index.
    /// The index entry for `package` when its files still match what was
    /// verified and its summary exists.
    fn unchanged<'a>(&self, index: &'a Index, package: &Package) -> Option<&'a Entry> {
        let entry = index
            .entries
            .iter()
            .find(|e| e.id == package.id && e.dir == package.dir)?;
        let same = files(&package.dir).is_ok_and(|now| {
            now.into_iter()
                .map(|(p, (s, m))| (p, s, m))
                .collect::<Vec<_>>()
                == entry.files
        });
        (same && self.summary_path(&package.id).is_file()).then_some(entry)
    }

    /// The packages `install` passed and that have not changed since, without
    /// verifying anything: a read-only, installed cache (the device image).
    pub fn installed(&self, packages: &[Package]) -> Vec<Package> {
        let index = self.read_index().unwrap_or_default();
        let mut loadable = Vec::new();
        for package in packages {
            match self.unchanged(&index, package) {
                Some(entry) if entry.pass => loadable.push(package.clone()),
                Some(_) => eprintln!("korri-shell: {} failed verification at install", package.id),
                None => eprintln!(
                    "korri-shell: {} is not installed or changed since install; not loaded",
                    package.id
                ),
            }
        }
        loadable
    }

    pub fn loadable(&self, packages: &[Package], limits: Limits) -> Result<Vec<Package>> {
        let index = self.read_index().unwrap_or_default();
        let mut loadable = Vec::new();
        for package in packages {
            let current = self.unchanged(&index, package);
            let unchanged = current.is_some();
            let pass = if unchanged {
                current.is_some_and(|e| e.pass)
            } else {
                eprintln!(
                    "korri-shell: verifying {} (not in the index or changed)",
                    package.id
                );
                let summary = self.install(package, limits)?;
                if !summary.pass {
                    eprintln!(
                        "korri-shell: {} failed verification: {}",
                        package.id,
                        summary.reasons.join("; ")
                    );
                }
                summary.pass
            };
            if pass {
                loadable.push(package.clone());
            }
        }
        Ok(loadable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planted(dir: &Path, slint: &str, logic: &str) -> Package {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("main.slint"), slint).unwrap();
        std::fs::write(dir.join("logic.js"), logic).unwrap();
        Package::at(dir).unwrap()
    }

    const VIEW: &str = "export struct Row { label: string } export component Surface inherits Window { in property <int> count; in property <[Row]> rows; in property <image> art; }";

    fn logic(view: &str) -> String {
        format!(
            "globalThis.surface = {{ preferences: {{ 'demo.size': {{ mark: 'device', initial: 'm' }} }},
               init: (model, prefs) => ({{ n: 0, size: prefs['demo.size'] }}),
               update: (s, m, model) => m._tag === 'Pressed' && m.button === 'right'
                 ? {{ state: {{ ...s, n: s.n + 1 }}, requests: [{{ _tag: 'MoveCatalog', query: {{ text: '', section: null, order: 'korri' }}, from: {{ _tag: 'Start' }}, count: 50 }}, {{ _tag: 'RememberPreference', key: 'demo.size', value: 'l' }}] }}
                 : {{ state: s, requests: [] }},
               view: (s, model) => {{ {view} }} }};"
        )
    }

    fn reasons(view_body: &str) -> String {
        let root = tempfile::tempdir().unwrap();
        let package = planted(&root.path().join("demo"), VIEW, &logic(view_body));
        let summary = verify(&package, Limits::default());
        assert!(!summary.pass, "expected a failure for {view_body}");
        summary.reasons.join(" ")
    }

    #[test]
    fn fixture_models_are_treaty_models() {
        let (base, examples) = fixture();
        for count in FIXTURE_GAMES {
            for overlay in [false, true] {
                let games = catalog::games(&examples, count);
                let sweep = Sweep {
                    base: base.clone(),
                    games: &games,
                    query: catalog::default_query(),
                    from: json!({"_tag": "Start"}),
                    count: catalog::DEFAULT_COUNT,
                    focused: Some("celeste".into()),
                    overlay,
                    displays: None,
                };
                let model = sweep.model();
                if let Err(error) =
                    serde_json::from_value::<korri_surface_treaty::SurfaceModel>(model)
                {
                    panic!("{count} games, overlay {overlay}: {error}");
                }
            }
        }
    }

    #[test]
    fn a_conforming_package_passes_every_fixture() {
        let root = tempfile::tempdir().unwrap();
        let package = planted(&root.path().join("demo"), VIEW, &logic("return { count: s.n, rows: (model.catalog.window ?? []).map(g => ({ label: g.title })) }"));
        let summary = verify(&package, Limits::default());
        assert!(summary.pass, "{:?}", summary.reasons);
        assert_eq!(
            summary.runs.iter().map(|r| r.games).collect::<Vec<_>>(),
            vec![0, 12, 10_000, 12]
        );
        // The last run lays out under a display list larger than the window.
        assert_eq!(summary.runs[3].displays, 2);
        assert!(summary
            .runs
            .iter()
            .all(|r| r.updates >= 9 + 1 + 4 + RESIZED_SIZES.len() + 1 + SEEDED_PRESSES));
        assert!(summary.runs.iter().all(|r| r.requests > 0));
        assert_eq!(summary.preferences["demo.size"].initial, "m");
        assert!(
            summary.load_ms > 0.0 && summary.load_ms < 2_000.0,
            "{}",
            summary.load_ms
        );
    }

    #[test]
    fn displays_must_be_main_all_or_absent_and_all_is_swept_with_two_displays() {
        let body = "return { count: (model.displays ?? []).length }";
        let with = |displays: &str| {
            let root = tempfile::tempdir().unwrap();
            let source = logic(body).replacen(
                "globalThis.surface = {",
                &format!("globalThis.surface = {{ displays: {displays},"),
                1,
            );
            verify(
                &planted(&root.path().join("demo"), VIEW, &source),
                Limits::default(),
            )
        };
        let all = with("'all'");
        assert!(all.pass, "{:?}", all.reasons);
        assert_eq!(all.displays, Some(engine::Displays::All));
        assert_eq!(
            all.runs.iter().map(|r| r.displays).collect::<Vec<_>>(),
            vec![0, 0, 0, 2, 2]
        );
        let main = with("'main'");
        assert!(main.pass && main.displays == Some(engine::Displays::Main));
        assert_eq!(main.runs.len(), 4);
        for wrong in ["'both'", "true", "null"] {
            let summary = with(wrong);
            assert!(!summary.pass, "{wrong}");
            assert!(
                summary.reasons.join(" ").contains("displays"),
                "{wrong}: {:?}",
                summary.reasons
            );
        }
    }

    #[test]
    fn rejects_unknown_properties_and_types() {
        assert!(reasons("return { nope: 1 }").contains("undeclared property"));
        assert!(reasons("return { count: 'one' }").contains("wrong type"));
        assert!(reasons("return { rows: [{ label: false }] }").contains("rows[0].label"));
        assert!(
            reasons("return { art: { $image: 'data:image/png;base64,AAAA' } }")
                .contains("image decode")
        );
    }

    #[test]
    fn rejects_throws_loops_and_heap_blow_ups() {
        assert!(reasons("throw new Error('planted')").contains("planted"));
        assert!(reasons("while (true) {}").contains("still running"));
        assert!(
            reasons("const a = []; while (true) a.push(new Uint8Array(1 << 20));").contains("heap")
        );
    }

    #[test]
    fn rejects_unknown_requests_and_bad_declarations() {
        let root = tempfile::tempdir().unwrap();
        let bad_request = planted(&root.path().join("one"), VIEW, "globalThis.surface = { init: () => 0, update: (s) => ({ state: s, requests: [{ _tag: 'Teleport' }] }), view: () => ({}) };");
        assert!(verify(&bad_request, Limits::default())
            .reasons
            .join(" ")
            .contains("unknown request"));
        let bad_declaration = planted(&root.path().join("two"), VIEW, "globalThis.surface = { preferences: { 'UPPER': { mark: 'device', initial: '' } }, init: () => 0, update: (s) => ({ state: s, requests: [] }), view: () => ({}) };");
        assert!(verify(&bad_declaration, Limits::default())
            .reasons
            .join(" ")
            .contains("preference key"));
    }

    #[test]
    fn the_index_skips_reverification_until_a_file_changes() {
        let root = tempfile::tempdir().unwrap();
        let cache = Cache::new(&root.path().join("cache"));
        let package = planted(
            &root.path().join("packages/demo"),
            VIEW,
            &logic("return { count: s.n }"),
        );
        let loadable = cache
            .loadable(std::slice::from_ref(&package), Limits::default())
            .unwrap();
        assert_eq!(loadable, vec![package.clone()]);
        let index = cache.read_index().unwrap();
        // A summary that claims failure is believed while files are unchanged.
        let mut failed = index.clone();
        failed.entries[0].pass = false;
        cache.write(&cache.index_path(), &failed).unwrap();
        assert!(cache
            .loadable(std::slice::from_ref(&package), Limits::default())
            .unwrap()
            .is_empty());
        // Changing a file makes the shell verify again.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(
            package.dir.join("logic.js"),
            logic("return { count: s.n + 1 }"),
        )
        .unwrap();
        assert_eq!(
            cache
                .loadable(std::slice::from_ref(&package), Limits::default())
                .unwrap(),
            vec![package]
        );
        // A missing index is rebuilt.
        std::fs::remove_file(cache.index_path()).unwrap();
        assert!(cache.read_index().is_none());
        assert_eq!(
            cache
                .loadable(
                    &[Package::at(&root.path().join("packages/demo")).unwrap()],
                    Limits::default()
                )
                .unwrap()
                .len(),
            1
        );
        assert!(cache.read_index().is_some());
    }

    #[test]
    fn installed_reads_summaries_and_never_verifies() {
        let root = tempfile::tempdir().unwrap();
        let cache = Cache::new(&root.path().join("cache"));
        let package = planted(
            &root.path().join("packages/demo"),
            VIEW,
            &logic("return { count: s.n }"),
        );
        // Nothing installed: nothing loads, and nothing is written.
        assert!(cache.installed(std::slice::from_ref(&package)).is_empty());
        assert!(cache.read_index().is_none());
        cache.install(&package, Limits::default()).unwrap();
        assert_eq!(
            cache.installed(std::slice::from_ref(&package)),
            vec![package.clone()]
        );
        // A changed file is refused, not re-verified.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(
            package.dir.join("logic.js"),
            logic("return { count: s.n + 1 }"),
        )
        .unwrap();
        let index = std::fs::read(cache.index_path()).unwrap();
        assert!(cache.installed(std::slice::from_ref(&package)).is_empty());
        assert_eq!(std::fs::read(cache.index_path()).unwrap(), index);
    }
}
