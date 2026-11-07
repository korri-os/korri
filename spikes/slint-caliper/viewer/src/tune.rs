//! THROWAWAY SPIKE (README.md, "Knobs and takes"): Caliper's knobs and takes for the
//! Slint packages.
//!
//! Knobs (Caliper docs/guide.md, "Tune design inputs with knobs"): each literal property
//! of an exported global is one knob. A value that reads another property, calls a
//! function or the clock is an output and is listed as "not a knob" with its reason.
//! A knob's value is set at run time through slint-interpreter on every live frame;
//! letting go writes the literal back into its declaration (src/knobs/edit.js: the write
//! names the version of the file it read, and refuses when the file changed).
//!
//! Slint makes an `out` property read-only from outside, and its constant binding may be
//! inlined. So the viewer compiles each token file through a file loader that declares
//! each knob `in-out`, and re-exports the global from the compiled preview under an alias.
//! The real file does not change.
//!
//! Takes (src/takes/store.js): `<root>/.caliper/takes/<n>/<path of the real file>` holds
//! the take's copies, `<n>.json` its record. A take compiles through the same file loader,
//! which reads the take's copy of a file when it has one. Accept copies the take's files
//! over the real files and removes the take; Discard removes it.

use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// `#rrggbb` or `#rrggbbaa`.
    Color,
    /// A number with its unit as written: "", "px", "ms", "%".
    Number { unit: String, int: bool },
    /// The name of a sibling colour literal (Caliper's `var(--p8-black)` token picker).
    Ref,
}

#[derive(Clone, Debug)]
pub struct Knob {
    pub file: PathBuf,
    pub global: String,
    pub name: String,
    pub ty: String,
    pub kind: Kind,
    /// The value as the source writes it, and its byte span in the file.
    pub value: String,
    pub start: usize,
    pub end: usize,
    pub doc: String,
    pub label: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct NotKnob {
    pub global: String,
    pub name: String,
    pub reason: String,
}

/// One file with exported globals, as the knobs read it.
pub struct TokenFile {
    pub path: PathBuf,
    pub globals: Vec<String>,
    pub knobs: Vec<Knob>,
    pub not_knobs: Vec<NotKnob>,
    pub version: u64,
}

pub fn version_of(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// Every `.slint` file under `root` that exports a global (dot-folders skipped).
pub fn token_files(root: &Path) -> Vec<TokenFile> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !name.starts_with('.') && name != "node_modules" {
                    stack.push(path);
                }
            } else if name.ends_with(".slint") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if text.contains("export global ") {
                        out.push(scan(&crate::parts::canon(&path), &text));
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn is_number(v: &str) -> Option<(String, bool)> {
    let digits: String = v
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if digits.is_empty() || digits.parse::<f64>().is_err() {
        return None;
    }
    let unit = &v[digits.len()..];
    if !["", "px", "phx", "ms", "s", "%", "deg"].contains(&unit) {
        return None;
    }
    Some((unit.to_string(), !digits.contains('.')))
}

fn is_color(v: &str) -> bool {
    v.starts_with('#') && matches!(v.len(), 7 | 9) && v[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// The knobs and the outputs of each `export global` in one file.
pub fn scan(path: &Path, text: &str) -> TokenFile {
    let mut file = TokenFile {
        path: path.to_path_buf(),
        globals: Vec::new(),
        knobs: Vec::new(),
        not_knobs: Vec::new(),
        version: version_of(text),
    };
    let mut global: Option<String> = None;
    let mut depth = 0i32;
    let mut doc: Option<String> = None;
    let mut in_doc = false;
    let mut doc_buf = String::new();
    let mut offset = 0usize;
    // (global, name, type, value, start) of literal declarations, resolved after the scan,
    // so that a Ref can name a sibling declared below it.
    let mut found: Vec<(String, String, String, String, usize, String)> = Vec::new();
    for line in text.split_inclusive('\n') {
        let at = offset;
        offset += line.len();
        let trimmed = line.trim();
        if in_doc {
            doc_buf.push_str(
                trimmed
                    .trim_end_matches("*/")
                    .trim_start_matches('*')
                    .trim(),
            );
            doc_buf.push(' ');
            if trimmed.ends_with("*/") {
                in_doc = false;
                doc = Some(doc_buf.trim().to_string());
            }
            continue;
        }
        if trimmed.starts_with("/**") {
            doc_buf = trimmed
                .trim_start_matches("/**")
                .trim_end_matches("*/")
                .trim()
                .to_string();
            doc_buf.push(' ');
            if trimmed.ends_with("*/") {
                doc = Some(doc_buf.trim().to_string());
            } else {
                in_doc = true;
            }
            continue;
        }
        if global.is_none() {
            if let Some(rest) = trimmed.strip_prefix("export global ") {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                file.globals.push(name.clone());
                global = Some(name);
                depth = 0;
            }
        }
        let Some(g) = global.clone() else {
            doc = None;
            continue;
        };
        let opening = depth;
        for c in line.chars() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        if opening == 1 {
            if let Some((access, rest)) = [
                "in-out property <",
                "out property <",
                "in property <",
                "property <",
            ]
            .iter()
            .find_map(|p| trimmed.strip_prefix(p).map(|r| (*p, r)))
            {
                let ty: String = rest.chars().take_while(|c| *c != '>').collect();
                let after = &rest[ty.len() + 1..];
                let name: String = after
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                let value = after.split_once(':').map(|(_, v)| v.trim()).unwrap_or("");
                let not = |reason: &str| NotKnob {
                    global: g.clone(),
                    name: name.clone(),
                    reason: reason.into(),
                };
                if access.starts_with("in") {
                    file.not_knobs
                        .push(not("an input: main.slint or the preview frame writes it"));
                } else if !value.ends_with(';') {
                    file.not_knobs.push(not("computed over several lines"));
                } else {
                    let value = value.trim_end_matches(';').trim().to_string();
                    let start = at + line.find(&format!(": {value}")).map_or(0, |i| i + 2);
                    found.push((
                        g.clone(),
                        name,
                        ty,
                        value,
                        start,
                        doc.take().unwrap_or_default(),
                    ));
                }
            }
        }
        if depth <= 0 && opening >= 1 {
            global = None;
        }
        if !trimmed.is_empty() {
            doc = None;
        }
    }
    let colours: Vec<String> = found
        .iter()
        .filter(|f| f.2 == "color" && is_color(&f.3))
        .map(|f| f.1.clone())
        .collect();
    for (g, name, ty, value, start, doc) in found {
        let kind = if ty == "color" && is_color(&value) {
            Some(Kind::Color)
        } else if ty == "color" && colours.contains(&value) {
            Some(Kind::Ref)
        } else if ["float", "int", "length", "duration", "percent", "angle"].contains(&ty.as_str())
        {
            is_number(&value).map(|(unit, int)| Kind::Number { unit, int })
        } else {
            None
        };
        let Some(kind) = kind else {
            let reason = if value.contains("animation-tick") {
                "animated by the clock"
            } else if value.contains('(') {
                "computed with a function"
            } else if value.starts_with('{') {
                "a struct: no control yet"
            } else if value.starts_with('"') {
                "a string: no control yet"
            } else {
                "computed from other values"
            };
            file.not_knobs.push(NotKnob {
                global: g,
                name,
                reason: reason.into(),
            });
            continue;
        };
        let hint = |tag: &str| {
            doc.split_whitespace()
                .skip_while(|w| *w != tag)
                .nth(1)
                .map(str::to_string)
        };
        if doc.contains("@knob ignore") {
            continue;
        }
        let label = doc
            .split_once("@label ")
            .map(|(_, r)| r.split(" @").next().unwrap_or("").trim().to_string());
        let prose = doc.split(" @").next().unwrap_or("").trim().to_string();
        let end = start + value.len();
        debug_assert_eq!(&text[start..end], value);
        file.knobs.push(Knob {
            file: path.to_path_buf(),
            global: g,
            name,
            ty,
            kind,
            value,
            start,
            end,
            doc: if prose.starts_with('@') {
                String::new()
            } else {
                prose
            },
            label,
            min: hint("@min").and_then(|v| v.parse().ok()),
            max: hint("@max").and_then(|v| v.parse().ok()),
            step: hint("@step").and_then(|v| v.parse().ok()),
        });
    }
    file
}

/// The token file as the viewer compiles it: each knob declared `in-out`, so the
/// interpreter can set it and the compiler keeps it a property.
pub fn settable(file: &TokenFile, text: &str) -> String {
    let mut out = text.to_string();
    // From the end, so earlier offsets stay true.
    for knob in file.knobs.iter().rev() {
        let line_start = out[..knob.start].rfind('\n').map_or(0, |i| i + 1);
        if let Some(i) = out[line_start..knob.start].find("out property") {
            let i = line_start + i;
            out.replace_range(i..i + 3, "in-out");
        }
    }
    out
}

/// The alias a global is re-exported under from the compiled preview.
pub fn alias(file: &Path, global: &str) -> String {
    format!(
        "Tune{}{global}",
        version_of(&file.to_string_lossy()) % 100000
    )
}

/// Lines appended to a compiled preview: each global of each token file it imports,
/// re-exported under its alias, so `set_global_property` reaches it.
pub fn exports(files: &[&TokenFile]) -> String {
    let mut out = String::from("\n");
    for f in files {
        for g in &f.globals {
            out.push_str(&format!(
                "export {{ {g} as {} }} from \"{}\";\n",
                alias(&f.path, g),
                f.path.display()
            ));
        }
    }
    out
}

/// A knob's value as the interpreter takes it. `colours` resolves a Ref.
pub fn value(
    knob: &Knob,
    text: &str,
    colours: &BTreeMap<String, String>,
) -> Option<slint_interpreter::Value> {
    use slint_interpreter::Value;
    match &knob.kind {
        Kind::Color => {
            let hex = text.trim_start_matches('#');
            let n = u32::from_str_radix(hex, 16).ok()?;
            let (r, g, b, a) = match hex.len() {
                6 => (n >> 16, (n >> 8) & 255, n & 255, 255),
                8 => (n >> 24, (n >> 16) & 255, (n >> 8) & 255, n & 255),
                _ => return None,
            };
            Some(Value::Brush(slint::Brush::SolidColor(
                slint::Color::from_argb_u8(a as u8, r as u8, g as u8, b as u8),
            )))
        }
        Kind::Number { unit, .. } => {
            let n: f64 = text.trim_end_matches(unit.as_str()).parse().ok()?;
            Some(Value::Number(match unit.as_str() {
                "s" => n * 1000.0,
                "%" => n / 100.0,
                _ => n,
            }))
        }
        Kind::Ref => {
            let hex = colours.get(text)?;
            let as_colour = Knob {
                kind: Kind::Color,
                ..knob.clone()
            };
            value(&as_colour, hex, colours)
        }
    }
}

/// Write `value` into the knob's declaration. The write names the version of the file
/// the knob read; a file that changed since gets no write (Caliper's editSource).
pub fn save(knob: &Knob, version: u64, value: &str) -> Result<()> {
    let text = std::fs::read_to_string(&knob.file)?;
    if version_of(&text) != version {
        bail!(
            "{} changed after the knob read it. The viewer did not write your value.",
            knob.file.display()
        );
    }
    if text.get(knob.start..knob.end) != Some(knob.value.as_str()) {
        bail!(
            "The declaration is not where the knob found it. The viewer did not write your value."
        );
    }
    if value.is_empty() || value.contains([';', '{', '}', '\n']) {
        bail!("\"{value}\" would change more than this declaration.");
    }
    let next = format!("{}{value}{}", &text[..knob.start], &text[knob.end..]);
    std::fs::write(&knob.file, next)?;
    Ok(())
}

// ---- Takes ----------------------------------------------------------------------------

pub const TAKES_DIR: &str = ".caliper/takes";

#[allow(dead_code)]
pub struct Take {
    pub id: String,
    /// The record's `part` (the preview file, root-relative), `state` and `device`.
    pub part: String,
    pub state: String,
    pub device: String,
    pub created: u64,
    /// The take's copies, root-relative.
    pub files: Vec<String>,
}

pub fn takes_dir(root: &Path) -> PathBuf {
    root.join(TAKES_DIR)
}

pub fn take_folder(root: &Path, id: &str) -> PathBuf {
    takes_dir(root).join(id)
}

fn files_of(base: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![base.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(rel) = p.strip_prefix(base) {
                out.push(rel.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}

pub fn list(root: &Path) -> Vec<Take> {
    let dir = takes_dir(root);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<Take> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".json")?.to_string();
            if id.parse::<u32>().ok()? == 0 {
                return None;
            }
            let record: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
            let s = |k: &str| {
                record
                    .get(k)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            Some(Take {
                files: files_of(&dir.join(&id)),
                part: s("part"),
                state: s("state"),
                device: s("device"),
                created: record.get("created").and_then(|v| v.as_u64()).unwrap_or(0),
                id,
            })
        })
        .collect();
    out.sort_by_key(|t| t.id.parse::<u32>().unwrap_or(0));
    out
}

/// A new take of one part and state, with copies of the part's files: its preview file
/// and the component file beside it (`Name.slint` for `Name.<layer>.preview.slint`).
pub fn create(root: &Path, part_rel: &str, state: &str, device: &str) -> Result<Take> {
    let dir = takes_dir(root);
    std::fs::create_dir_all(&dir)?;
    let ignore = root.join(".caliper/.gitignore");
    if !ignore.exists() {
        std::fs::write(
            &ignore,
            "# Caliper's takes. Nothing here belongs in Git.\n*\n",
        )?;
    }
    let used = std::fs::read_dir(&dir)?
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .split('.')
                .next()
                .and_then(|n| n.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0);
    let id = (used + 1).to_string();
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    let preview = Path::new(part_rel);
    let file = preview
        .file_name()
        .context("part")?
        .to_string_lossy()
        .into_owned();
    let stem = file.split('.').next().unwrap_or_default();
    let mut copy = vec![part_rel.to_string()];
    let component = preview.with_file_name(format!("{stem}.slint"));
    if root.join(&component).is_file() {
        copy.push(component.to_string_lossy().into_owned());
    }
    let base = take_folder(root, &id);
    for rel in &copy {
        let to = base.join(rel);
        std::fs::create_dir_all(to.parent().unwrap())?;
        std::fs::copy(root.join(rel), &to)?;
    }
    let record = serde_json::json!({ "part": part_rel, "state": state, "device": device, "created": created });
    std::fs::write(
        dir.join(format!("{id}.json")),
        format!("{}\n", serde_json::to_string_pretty(&record)?),
    )?;
    Ok(Take {
        id,
        part: part_rel.into(),
        state: state.into(),
        device: device.into(),
        created,
        files: copy,
    })
}

pub fn discard(root: &Path, id: &str) -> Result<()> {
    if id.parse::<u32>().unwrap_or(0) == 0 {
        bail!("\"{id}\" is not a take number");
    }
    let base = take_folder(root, id);
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(base.with_extension("images"));
    let _ = std::fs::remove_file(base.with_extension("json"));
    Ok(())
}

/// Copy the take's files over the real files, log the accept in `.caliper/accepted.json`
/// as Caliper does, then remove the take. Returns the real files that changed.
pub fn accept(root: &Path, take: &Take) -> Result<Vec<String>> {
    let base = take_folder(root, &take.id);
    for rel in &take.files {
        if rel.split('/').any(|s| s == ".." || s == ".caliper") {
            bail!("{rel} leaves the packages");
        }
        std::fs::copy(base.join(rel), root.join(rel))?;
    }
    let log = root.join(".caliper/accepted.json");
    let mut entries: Vec<serde_json::Value> = std::fs::read_to_string(&log)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    entries.push(serde_json::json!({ "take": take.id, "created": take.created, "part": take.part, "state": take.state, "files": take.files, "at": at }));
    std::fs::write(
        &log,
        format!("{}\n", serde_json::to_string_pretty(&entries)?),
    )?;
    discard(root, &take.id)?;
    Ok(take.files.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_and_pico_knobs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../clients/shell/packages");
        let files = token_files(&root);
        let zone = files
            .iter()
            .find(|f| f.path.ends_with("zone/ui/tokens.slint"))
            .unwrap();
        let mint = zone.knobs.iter().find(|k| k.name == "mint").unwrap();
        assert_eq!(mint.kind, Kind::Color);
        assert_eq!(mint.value, "#4ad39c");
        let rows = zone.knobs.iter().find(|k| k.name == "art-rows").unwrap();
        assert!(rows.doc.starts_with("Art rows"));
        assert!(zone.not_knobs.iter().any(|n| n.name == "ground-fill"));
        assert!(zone
            .not_knobs
            .iter()
            .any(|n| n.name == "clock-ms" && n.reason.contains("clock")));
        assert!(!zone
            .knobs
            .iter()
            .any(|k| k.name == "a" || k.name == "width"));
        let p8 = files
            .iter()
            .find(|f| f.path.ends_with("pico/ui/palette.slint"))
            .unwrap();
        assert_eq!(
            p8.knobs.iter().find(|k| k.name == "bg").unwrap().kind,
            Kind::Ref
        );
        let text = std::fs::read_to_string(&zone.path).unwrap();
        let s = settable(zone, &text);
        assert!(s.contains("in-out property <color> mint: #4ad39c;"));
        assert!(s.contains("out property <length> a: screen.a * 1px;"));
    }
}
