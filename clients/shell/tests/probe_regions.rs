//! surface-probe's 8.4 judge on a small package with two regions: tabs above
//! a grid. The package follows its declared plan, so the walk passes; a model
//! that makes it jump on `left` inside the grid must fail.

use std::path::{Path, PathBuf};
use std::process::Command;

fn probe() -> PathBuf {
    // Cargo builds examples before integration tests, next to deps/.
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().parent().unwrap();
    dir.join("examples").join("surface-probe")
}

fn walk(model: &Path) -> std::process::Output {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = tempfile::tempdir().unwrap();
    Command::new(probe())
        .arg("walk")
        .arg(root.join("tests/packages/regions"))
        .arg("--model")
        .arg(model)
        .args(["--sizes", "640x480", "--max", "40", "--out"])
        .arg(out.path())
        .output()
        .expect("run surface-probe")
}

#[test]
fn the_probe_judges_moves_in_regions_and_declared_exits() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let model = root.join("fixtures/model.json");
    let good = walk(&model);
    let stdout = String::from_utf8_lossy(&good.stdout);
    assert!(
        good.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&good.stderr)
    );
    assert!(stdout.contains("0 failures"), "{stdout}");

    let dir = tempfile::tempdir().unwrap();
    let mut wrong: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&model).unwrap()).unwrap();
    wrong["clockLabel"] = "WRONG".into();
    let wrong_model = dir.path().join("model.json");
    std::fs::write(&wrong_model, wrong.to_string()).unwrap();
    let bad = walk(&wrong_model);
    let stdout = String::from_utf8_lossy(&bad.stdout);
    assert!(!bad.status.success(), "{stdout}");
    assert!(
        stdout.contains(
            r#"expected {"label":"B"}#1 at (160,100): the nearest left in region "cells""#
        ),
        "{stdout}"
    );
}
