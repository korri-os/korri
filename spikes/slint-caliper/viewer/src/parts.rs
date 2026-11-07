//! Discovery: every `*.preview.slint` under the packages is a part, and each exported
//! component in it is a state. Caliper's rules (docs/guide.md, "How Caliper finds
//! things"): the layer comes from the file name's suffix (`Home.page.part.tsx`, here
//! `PicoMenu.organism.preview.slint`); the list runs Pages, Templates, Organisms,
//! Molecules, Atoms, then Unclassified, in file-path order inside each layer.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const LAYERS: [&str; 5] = ["page", "template", "organism", "molecule", "atom"];
pub const LAYER_TITLES: [&str; 6] = [
    "Pages",
    "Templates",
    "Organisms",
    "Molecules",
    "Atoms",
    "Unclassified",
];

pub struct Part {
    pub path: PathBuf,
    /// Path under the packages folder, for the list's order and the heading.
    pub rel: String,
    pub name: String,
    /// Index into LAYER_TITLES.
    pub layer: usize,
    pub states: Vec<String>,
}

pub fn discover(root: &Path) -> Vec<Part> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let file = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !file.starts_with('.') && file != "node_modules" {
                    stack.push(path);
                }
            } else if file.ends_with(".preview.slint") {
                files.push(path);
            }
        }
    }
    let mut parts: Vec<Part> = files
        .into_iter()
        .map(|path| {
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            let stem = file.trim_end_matches(".preview.slint");
            let (name, layer) = match stem.rsplit_once('.') {
                Some((name, layer)) if LAYERS.contains(&layer) => (
                    name.to_string(),
                    LAYERS.iter().position(|l| *l == layer).unwrap(),
                ),
                _ => (stem.to_string(), 5),
            };
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let states = std::fs::read_to_string(&path)
                .map(|s| states_of(&s))
                .unwrap_or_default();
            Part {
                path,
                rel,
                name,
                layer,
                states,
            }
        })
        .collect();
    parts.sort_by(|a, b| (a.layer, &a.rel).cmp(&(b.layer, &b.rel)));
    parts
}

/// Each `export component Name` is one state, in source order.
pub fn states_of(source: &str) -> Vec<String> {
    blocks(source).into_iter().map(|b| b.name).collect()
}

/// Caliper's label from an export name: "NoResults" is "No results".
pub fn label(export: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = export.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0
            && c.is_uppercase()
            && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit())
        {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else if i > 0 && c.is_ascii_digit() && chars[i - 1].is_alphabetic() {
            out.push(' ');
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

pub struct Block {
    pub name: String,
    /// Byte range of `export component … { … }`.
    pub start: usize,
    pub end: usize,
}

/// Top-level `export component` blocks, by brace matching that skips strings and comments.
pub fn blocks(source: &str) -> Vec<Block> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
                continue;
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b'e' if depth == 0 && source[i..].starts_with("export component ") => {
                let rest = &source[i + "export component ".len()..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                // The block ends at its matching brace.
                let Some(open) = source[i..].find('{').map(|o| i + o) else {
                    break;
                };
                let mut j = open;
                let mut d = 0usize;
                while j < bytes.len() {
                    match bytes[j] {
                        b'"' => {
                            j += 1;
                            while j < bytes.len() && bytes[j] != b'"' {
                                if bytes[j] == b'\\' {
                                    j += 1;
                                }
                                j += 1;
                            }
                        }
                        b'/' if bytes.get(j + 1) == Some(&b'/') => {
                            while j < bytes.len() && bytes[j] != b'\n' {
                                j += 1;
                            }
                            continue;
                        }
                        b'{' => d += 1,
                        b'}' => {
                            d -= 1;
                            if d == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                let end = (j + 1).min(bytes.len());
                out.push(Block {
                    name,
                    start: i,
                    end,
                });
                i = end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// The file with every other state's block blanked (newlines kept, so diagnostics keep
/// their lines): one state compiled alone, so an error in another state does not fail it.
pub fn alone(source: &str, state: &str) -> String {
    let mut bytes = source.as_bytes().to_vec();
    for block in blocks(source).into_iter().filter(|b| b.name != state) {
        for b in &mut bytes[block.start..block.end] {
            if *b != b'\n' {
                *b = b' ';
            }
        }
    }
    String::from_utf8(bytes).unwrap_or_default()
}

/// The `.slint` files `file` imports, transitively, itself included: the files whose
/// change can change its states.
pub fn imports(file: &Path) -> BTreeSet<PathBuf> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![canon(file)];
    while let Some(path) = stack.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let dir = path.parent().unwrap_or(Path::new("."));
        for line in source.lines() {
            let line = line.trim_start();
            if !line.starts_with("import") {
                continue;
            }
            // import { A } from "x.slint";  or  import "font.ttf";
            let Some(q) = line.find('"') else { continue };
            let Some(e) = line[q + 1..].find('"') else {
                continue;
            };
            let target = &line[q + 1..q + 1 + e];
            if target.ends_with(".slint") {
                stack.push(canon(&dir.join(target)));
            } else {
                seen.insert(canon(&dir.join(target)));
            }
        }
    }
    seen
}

pub fn canon(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_order_and_labels() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../clients/shell/packages");
        let parts = discover(&root);
        assert!(parts.len() > 100, "{} parts", parts.len());
        let layers: Vec<usize> = parts.iter().map(|p| p.layer).collect();
        assert!(layers.windows(2).all(|w| w[0] <= w[1]));
        let menu = parts.iter().find(|p| p.name == "PicoMenu").unwrap();
        assert_eq!(LAYER_TITLES[menu.layer], "Organisms");
        assert_eq!(menu.states, ["Default", "Short"]);
        assert_eq!(label("NoResults"), "No results");
    }

    #[test]
    fn one_state_alone() {
        let src = "import { A } from \"a.slint\";\nexport component One inherits Window {\n  Text { text: \"}\"; }\n}\n\nexport component Two inherits Window { x: oops; }\n";
        assert_eq!(states_of(src), ["One", "Two"]);
        let one = alone(src, "One");
        assert!(one.contains("component One") && !one.contains("oops"));
        assert_eq!(one.lines().count(), src.lines().count());
    }
}
