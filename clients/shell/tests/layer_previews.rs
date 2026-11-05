//! Zone's previews drawn with every layer cached, ZoneCaps included, and with no layer
//! cached are byte-identical (vendored `render_layer_image`): an elided caption's
//! ellipsis past its box (ZoneSignal "Cut") and the Modern ground at fractional pixel
//! positions (ZoneGames "Modern"), at the five sizes. Before, the first lost part of its
//! ellipsis and the second differed in 12-24 RGB565 pixels.

use std::path::{Path, PathBuf};
use std::process::Command;

fn probe() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
        .join("surface-probe")
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Zone with only the two previews, its layers set to `hint` (`None`: as shipped, plus
/// ZoneCaps cached).
fn zone(dir: &Path, cached: bool) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/zone");
    let package = dir.join(if cached { "cached" } else { "direct" });
    copy(&root, &package);
    let keep = [
        "ZoneSignal.atom.preview.slint",
        "ZoneGames.page.preview.slint",
    ];
    let mut stack = vec![package.clone()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if name.ends_with(".preview.slint") && !keep.contains(&name.as_str()) {
                std::fs::remove_file(&path).unwrap();
            } else if name.ends_with(".slint") {
                let text = std::fs::read_to_string(&path).unwrap();
                let text = if !cached {
                    set_hints(&text, "false")
                } else if name == "ZoneCaps.slint" {
                    // Every caption cached, elided ones too.
                    if text.contains("cache-rendering-hint:") {
                        set_hints(&text, "true")
                    } else {
                        text.replace(
                            "export component ZoneCaps {\n",
                            "export component ZoneCaps {\n    cache-rendering-hint: true;\n",
                        )
                    }
                } else {
                    text
                };
                std::fs::write(&path, text).unwrap();
            }
        }
    }
    package
}

/// Every `cache-rendering-hint: …;` set to `value`.
fn set_hints(text: &str, value: &str) -> String {
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        match line.find("cache-rendering-hint:") {
            Some(at) => {
                let end = line[at..].find(';').map_or(line.len(), |e| at + e + 1);
                out.push_str(&line[..at]);
                out.push_str(&format!("cache-rendering-hint: {value};"));
                out.push_str(&line[end..]);
            }
            None => out.push_str(line),
        }
    }
    out
}

#[test]
fn zone_previews_are_byte_identical_cached_and_direct() {
    let dir = tempfile::tempdir().unwrap();
    let mut outputs = Vec::new();
    for cached in [true, false] {
        let package = zone(dir.path(), cached);
        let out = dir.path().join(format!("out-{cached}"));
        let run = Command::new(probe())
            .arg("previews")
            .arg(&package)
            .args([
                "--sizes",
                "320x240,640x480,480x800,1280x720,1920x1080",
                "--out",
            ])
            .arg(&out)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        outputs.push(out);
    }
    let mut names: Vec<_> = std::fs::read_dir(&outputs[1])
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    names.sort();
    assert!(names.len() >= 40, "{} stills", names.len());
    let differing: Vec<_> = names
        .iter()
        .filter(|n| {
            std::fs::read(outputs[0].join(n)).unwrap() != std::fs::read(outputs[1].join(n)).unwrap()
        })
        .collect();
    assert!(
        differing.is_empty(),
        "differ cached and direct: {differing:?}"
    );
}
