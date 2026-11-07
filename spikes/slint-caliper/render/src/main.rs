//! Throwaway native counterpart of Caliper's bin/caliper-render.mjs.
mod checks;

use anyhow::{bail, Context, Result};
use i_slint_backend_testing::{AccessibleRole, ElementRoot};
use korri_shell::{backend::headless::Headless, images::ImageCache, targets, view::Binder};
use serde_json::{json, Value};
use slint::ComponentHandle;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// Sources and orientation are recorded in README.md. These are logical pixels at scale 1.
const DEVICES: &[(&str, u32, u32)] = &[
    ("rg35xxsp", 640, 480),
    ("rg35xxpro", 640, 480),
    ("rpminiv2", 1240, 1080),
    ("rg353m", 640, 480),
    ("odin2portal", 1920, 1080),
];
const ENVIRONMENT: &str = "slint-1.18.1-korri-rgb565-linux-x86_64-render-spike-1";
const HELP: &str = "slint-caliper-render --part FILE|DIR|'*' [--state NAME|all|'*'] [--device ID]... [--out DIR] [--check] [--baselines DIR] [--view FILE]\nslint-caliper-render --approve REPORT --baselines DIR\nslint-caliper-render --part DIR --list\nNative previews need no --url or Chromium. Default output: /tmp/slint-caliper-render.\n--check saves two fresh-instance renders and a Caliper v1 report. Exit 0 means written, not passed.\nWithout --check: exit 0 Rendered, 1 Empty/Failed. Invalid request: exit 2.\n--view binds a real shell JSON view to an exported component (e.g. main.slint --state Surface).\nPreview literals lack JSON entries: entry-to-element focus coverage is Inconclusive.\nAccessibility covers selectable roles/labels and at most one selected element only; no axe.\nApproval uses saved images only. Inspect both PNGs before using --approve.";

#[derive(Default)]
struct Args {
    part: Option<PathBuf>,
    state: Option<String>,
    devices: Vec<String>,
    out: Option<PathBuf>,
    check: bool,
    baselines: Option<PathBuf>,
    approve: Option<PathBuf>,
    list: bool,
    view: Option<PathBuf>,
}
fn args() -> Result<Args> {
    let mut out = Args::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--check" {
            out.check = true;
            continue;
        }
        if arg == "--list" {
            out.list = true;
            continue;
        }
        if arg == "--help" {
            println!("{HELP}");
            std::process::exit(0);
        }
        let value = args
            .next()
            .with_context(|| format!("{arg} needs a value"))?;
        match arg.as_str() {
            "--part" => out.part = Some(value.into()),
            "--state" => out.state = Some(value),
            "--device" => out.devices.extend(value.split(',').map(str::to_owned)),
            "--out" => out.out = Some(value.into()),
            "--baselines" => out.baselines = Some(value.into()),
            "--approve" => out.approve = Some(value.into()),
            "--view" => out.view = Some(value.into()),
            _ => bail!("Unknown option {arg}. Use --help."),
        }
    }
    if out.approve.is_some() {
        if out.baselines.is_none()
            || out.part.is_some()
            || out.state.is_some()
            || !out.devices.is_empty()
            || out.check
            || out.list
            || out.view.is_some()
        {
            bail!("Use --approve REPORT --baselines DIR without a render request");
        }
    } else {
        if out.part.is_none() {
            bail!("Pass --part FILE|DIR or '*' (from the Korri root)");
        }
        if out.baselines.is_some() && !out.check {
            bail!("--baselines needs --check or --approve");
        }
        if out.list && out.check {
            bail!("Use --list or --check, not both");
        }
    }
    Ok(out)
}
fn files(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_file() {
        out.push(path.to_owned());
        return Ok(());
    }
    for entry in std::fs::read_dir(path).with_context(|| format!("Read {}", path.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            continue;
        }
        let p = entry.path();
        if p.is_dir() {
            files(&p, out)?;
        } else if p.to_string_lossy().ends_with(".preview.slint") {
            out.push(p);
        }
    }
    Ok(())
}
fn selected_devices(ids: &[String]) -> Result<Vec<(&'static str, u32, u32)>> {
    if ids.is_empty() {
        return Ok(vec![DEVICES[0]]);
    }
    if ids.iter().any(|s| s == "*" || s == "all") {
        return Ok(DEVICES.to_vec());
    }
    let mut seen = BTreeSet::new();
    ids.iter()
        .filter(|id| seen.insert(id.as_str()))
        .map(|id| {
            DEVICES
                .iter()
                .find(|d| d.0 == id)
                .copied()
                .with_context(|| format!("Unknown device {id}; use --list"))
        })
        .collect()
}
fn pixels(instance: &slint_interpreter::ComponentInstance, w: u32, h: u32) -> Result<Vec<u16>> {
    instance.window().set_size(slint::PhysicalSize::new(w, h));
    let mut frame = Vec::new();
    // surface-probe's 500 ms settle, on its manual clock. Looping motion is frozen.
    for _ in 0..20 {
        Headless::advance(Duration::from_millis(25));
        instance.window().request_redraw();
        frame = Headless::render().unwrap_or_default();
    }
    if frame.len() != (w * h) as usize {
        bail!(
            "Window did not draw the requested {w}x{h} buffer ({} pixels)",
            frame.len()
        );
    }
    Ok(frame)
}
fn save(pixels: &[u16], w: u32, h: u32, path: &Path) -> Result<()> {
    let mut image = image::RgbImage::new(w, h);
    for (i, p) in pixels.iter().enumerate() {
        let (r, g, b) = ((p >> 11) & 31, (p >> 5) & 63, p & 31);
        image.put_pixel(
            i as u32 % w,
            i as u32 / w,
            image::Rgb([
                (r << 3 | r >> 2) as u8,
                (g << 2 | g >> 4) as u8,
                (b << 3 | b >> 2) as u8,
            ]),
        );
    }
    image.save(path)?;
    Ok(())
}

fn observe(
    definition: &slint_interpreter::ComponentDefinition,
    w: u32,
    h: u32,
    png: &Path,
    view: Option<&Value>,
    image_root: &Path,
) -> Result<Value> {
    let instance = definition.create()?;
    instance.show()?;
    let mut binder = Binder::new(definition);
    let mut images = ImageCache::new(image_root);
    if let Some(view) = view {
        binder.apply(&instance, view, &mut images)?;
    }
    let frame = pixels(&instance, w, h)?;
    save(&frame, w, h, png)?;
    let empty = frame.is_empty() || frame.iter().all(|p| *p == frame[0]); // surface-probe's exact rule
    let mut spill = Vec::new();
    let mut violations = Vec::new();
    for (index, e) in instance
        .root_element()
        .query_descendants()
        .find_all()
        .into_iter()
        .enumerate()
    {
        if !e.is_valid() || e.is_hidden() {
            continue;
        }
        let pos = e.absolute_position();
        let size = e.size();
        let target = e
            .id()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("element#{index}"));
        if size.width > 0.0
            && size.height > 0.0
            && (pos.x < -0.5
                || pos.y < -0.5
                || pos.x + size.width > w as f32 + 0.5
                || pos.y + size.height > h as f32 + 0.5)
        {
            spill.push(json!({"element": e.type_name().map(|s| s.to_string()).unwrap_or_default(), "target":target,"left":pos.x,"top":pos.y,"right":pos.x+size.width,"bottom":pos.y+size.height}));
        }
        // Korri uses selectable/selected for semantic focus, not OS keyboard focus.
        if e.accessible_item_selectable() == Some(true) {
            if e.accessible_role()
                .is_none_or(|r| r == AccessibleRole::None)
            {
                violations.push(format!("{target}: focusable element has no role"));
            }
            if e.accessible_label().is_none_or(|s| s.trim().is_empty()) {
                violations.push(format!("{target}: focusable element has no label"));
            }
        }
    }
    let elements = targets::elements(&instance);
    let selected: Vec<_> = elements.iter().filter(|e| e.selected).collect();
    if selected.len() > 1 {
        violations.push(format!("{} focused elements exist at once", selected.len()));
    }
    // Plain Window does not clip children in ElementHandle::is_visible(). Keep the
    // shell's ancestor-clip/hidden rules and also intersect the actual viewport.
    let bad_focus = elements.iter().any(|e| {
        e.selected
            && (e.hidden
                || !e.visible
                || e.w <= 0.0
                || e.h <= 0.0
                || e.x >= w as f32
                || e.y >= h as f32
                || e.x + e.w <= 0.0
                || e.y + e.h <= 0.0)
    });
    let entry_focus = view.map(|v| targets::drawn(&instance, &mut binder, v, &mut images));
    let focus = match entry_focus {
        Some(Err(error)) => checks::check("focus", "Failed", format!("{error:#}")),
        _ if bad_focus => checks::check("focus", "Failed", "A selected target is not drawn and visible (shell targets::elements plus actual window intersection)."),
        Some(Ok(_)) => checks::check("focus", "Passed", "JSON view entries matched drawn, visible targets with shell targets::drawn."),
        None => checks::check("focus", "Inconclusive", "Selected target visibility checked with targets::elements. Preview exports carry nested literal props, not JSON view entries: an entry selecting no element cannot be detected. Use --view for entry-to-element coverage."),
    };
    let a11y = checks::check("accessibility", if violations.is_empty() { "Review" } else { "Failed" }, format!("{} Two native rules only: selectable role+label; at most one selected element. All other axe WCAG A/AA checks are a gap; see README axe-gaps.txt.", if violations.is_empty() { "No violations of these two rules.".to_owned() } else { violations.join("; ") }));
    let spill = if spill.is_empty() {
        Value::Null
    } else {
        json!({"left":spill.iter().map(|v|v["left"].as_f64().unwrap()).fold(0.0,f64::min),"top":spill.iter().map(|v|v["top"].as_f64().unwrap()).fold(0.0,f64::min),"right":spill.iter().map(|v|v["right"].as_f64().unwrap()).fold(w as f64,f64::max),"bottom":spill.iter().map(|v|v["bottom"].as_f64().unwrap()).fold(h as f64,f64::max),"elements":spill,"complete":true})
    };
    instance.hide()?;
    Ok(
        json!({"frame":if empty {"Empty"} else {"Rendered"},"png":png,"problems":[],"console":[],"spill":spill,"accessibilityCheck":a11y,"focusCheck":focus}),
    )
}
fn run() -> Result<i32> {
    let args = args()?;
    if let Some(report) = args.approve {
        println!(
            "{}",
            checks::approve(&report, args.baselines.as_ref().unwrap())?
        );
        return Ok(0);
    }
    let start = Instant::now();
    let path = args.part.unwrap();
    let mut parts = Vec::new();
    if path == Path::new("*") {
        for name in ["pico", "zone"] {
            files(&Path::new("clients/shell/packages").join(name), &mut parts)?;
        }
    } else {
        files(&path, &mut parts)?;
    }
    parts.sort();
    parts.dedup();
    if parts.is_empty() {
        bail!("No preview parts found");
    }
    let devices = selected_devices(&args.devices)?;
    let out = args
        .out
        .unwrap_or_else(|| "/tmp/slint-caliper-render".into());
    let run = std::env::current_dir()?.join(out).join(format!(
        "check-{}-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        std::process::id()
    ));
    if !args.list {
        std::fs::create_dir_all(run.join("first"))?;
        std::fs::create_dir_all(run.join("repeat"))?;
    }
    let view: Option<Value> = args
        .view
        .map(|p| -> Result<Value> { Ok(serde_json::from_slice(&std::fs::read(p)?)?) })
        .transpose()?;
    Headless::install();
    Headless::manual_clock();
    Headless::freeze_motion();
    let mut results = Vec::new();
    let mut discovery = Vec::new();
    for (part_index, part) in parts.iter().enumerate() {
        let result =
            pollster::block_on(slint_interpreter::Compiler::default().build_from_path(part));
        let names: Vec<_> = result.component_names().map(str::to_owned).collect();
        if args.list {
            discovery.push(json!({"file":part,"states":names,"problems":result.diagnostics().map(|d|d.to_string()).collect::<Vec<_>>()}));
            continue;
        }
        let states = match args.state.as_deref() {
            Some("*" | "all") => {
                if names.is_empty() {
                    vec!["default".to_owned()]
                } else {
                    names.clone()
                }
            }
            Some(state) => vec![state.to_owned()],
            None => vec![names
                .iter()
                .find(|s| s.as_str() == "Default")
                .or(names.first())
                .cloned()
                .unwrap_or_else(|| "default".into())],
        };
        for state in states {
            if !result.has_errors() && !names.contains(&state) {
                bail!(
                    "{} exports no state {state}; available: {}",
                    part.display(),
                    names.join(", ")
                );
            }
            for &(device, w, h) in &devices {
                let filename = format!(
                    "{part_index}-{}.{state}.{device}.png",
                    part.file_stem().unwrap_or_default().to_string_lossy()
                );
                let first_path = run.join("first").join(&filename);
                let repeat_path = run.join("repeat").join(&filename);
                let sample = |png: &Path| -> Value {
                    let observation = (|| -> Result<Value> {
                        if result.has_errors() {
                            bail!(
                                "{}",
                                result
                                    .diagnostics()
                                    .map(|d| d.to_string())
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            );
                        }
                        observe(
                            &result.component(&state).context("No exported state")?,
                            w,
                            h,
                            png,
                            view.as_ref(),
                            part.parent().unwrap_or(Path::new(".")),
                        )
                    })();
                    observation.unwrap_or_else(|error| json!({"frame":"Failed","png":"","problems":[{"kind":"error","title":"Slint render failed","detail":format!("{error:#}")}],"console":[],"spill":null}))
                };
                let mut first = sample(&first_path);
                first["part"] = json!(part);
                first["state"] = json!(state);
                first["device"] = json!(device);
                first["viewport"] = json!({"width":w,"height":h});
                first["environment"] = json!(ENVIRONMENT);
                if args.check {
                    let repeat = sample(&repeat_path);
                    checks::compare(&mut first, &repeat, args.baselines.as_deref())?;
                }
                first.as_object_mut().unwrap().remove("accessibilityCheck");
                first.as_object_mut().unwrap().remove("focusCheck");
                results.push(first);
            }
        }
    }
    if args.list {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"project":"korri","parts":discovery,"devices":DEVICES.iter().map(|(id,w,h)|json!({"id":id,"cssWidth":w,"cssHeight":h})).collect::<Vec<_>>()})
            )?
        );
        return Ok(0);
    }
    let failed = results.iter().any(|r| r["frame"] != "Rendered");
    let mut output = json!({"results":results});
    if args.check {
        let report = json!({"version":1,"project":"korri","environment":ENVIRONMENT,"createdAt":chrono::Utc::now().to_rfc3339(),"coverage":"Only listed states and devices; native role/label and selected count, no axe. Preview entry-to-element focus is Inconclusive without --view. No interactions, scenario execution, source revision monitoring or consumer coverage.","results":output["results"]});
        let report_path = run.join("report.json");
        std::fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
        output["report"] = report;
        output["reportPath"] = json!(report_path);
    }
    eprintln!(
        "{} results in {:.3}s",
        output["results"].as_array().unwrap().len(),
        start.elapsed().as_secs_f64()
    );
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(if args.check || !failed { 0 } else { 1 })
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            println!("{}", json!({"error":format!("{error:#}")}));
            std::process::exit(2);
        }
    }
}
