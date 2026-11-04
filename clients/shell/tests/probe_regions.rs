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
    walk_package("regions", model)
}

fn walk_package(package: &str, model: &Path) -> std::process::Output {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = tempfile::tempdir().unwrap();
    Command::new(probe())
        .arg("walk")
        .arg(root.join("tests/packages").join(package))
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
        stdout.contains(r#"expected cells#1 at (160,100): the nearest left in region "cells""#),
        "{stdout}"
    );
}

/// Targets are told apart by region and drawing order, never by label or value:
/// two route rows with the same button labels, a header with a button's word,
/// and a choice row whose value left and right step in place (focus stays).
#[test]
fn repeated_labels_and_a_stepped_value_keep_their_targets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let run = walk_package("repeats", &root.join("fixtures/model.json"));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout.contains("0 failures"), "{stdout}");
    // The choice row at each of its three values is a state of its own.
    assert!(
        stdout.contains("640x480: 15 states, 0 failures"),
        "{stdout}"
    );
}

/// A focus target whose element does not set accessible-item-selectable fails.
#[test]
fn a_target_without_its_element_fails() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join("bare");
    std::fs::create_dir_all(&package).unwrap();
    for file in ["logic.js", "fonts/tiny5.ttf"] {
        let to = package.join(file);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(root.join("tests/packages/regions").join(file), to).unwrap();
    }
    let slint = std::fs::read_to_string(root.join("tests/packages/regions/main.slint"))
        .unwrap()
        .replace("accessible-item-selectable: true;", "")
        .replace("accessible-item-selected: tab.focused;", "")
        .replace("accessible-item-selected: cell.focused;", "");
    std::fs::write(package.join("main.slint"), slint).unwrap();
    let out = tempfile::tempdir().unwrap();
    let run = Command::new(probe())
        .arg("walk")
        .arg(&package)
        .arg("--model")
        .arg(root.join("fixtures/model.json"))
        .args(["--sizes", "640x480", "--max", "4", "--out"])
        .arg(out.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(!run.status.success(), "{stdout}");
    assert!(
        stdout.contains("selects no element with accessible-item-selectable"),
        "{stdout}"
    );
}

/// A list that shows four of ten rows: the rows scrolled out are not drawn, and
/// down from the last row in view goes to the next row, scrolled into view.
#[test]
fn a_scrolling_list_moves_into_rows_out_of_view() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let run = walk_package("scroller", &root.join("fixtures/model.json"));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout.contains(" states, 0 failures"), "{stdout}");
}

/// A band that draws only the focused card and its neighbours and recentres
/// on every move: identity is the entry's place in the view, so a move right
/// is judged as the move to the next card, not as a rename.
#[test]
fn a_recentring_band_keeps_its_cards_apart() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let run = walk_package("band", &root.join("fixtures/model.json"));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout.contains(" states, 0 failures"), "{stdout}");
}

fn model_with_clock(label: &str) -> (tempfile::TempDir, PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::tempdir().unwrap();
    let mut model: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("fixtures/model.json")).unwrap())
            .unwrap();
    model["clockLabel"] = label.into();
    let path = dir.path().join("model.json");
    std::fs::write(&path, model.to_string()).unwrap();
    (dir, path)
}

/// A grid showing two of six rows: down from the last row in view goes to the cell below,
/// clipped by the scrolling box, judged at its real geometry (the nearest down, same
/// column), and scrolled in. Right from the grid exits to a side list scrolled entirely
/// out of its box: the exit is valid, entering at the first row by geometry.
#[test]
fn targets_below_the_fold_and_a_region_scrolled_out_are_reachable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let run = walk_package("fold", &root.join("fixtures/model.json"));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout.contains(" states, 0 failures"), "{stdout}");
}

/// The same side list on a hidden layer (`visible: false`) is not a target: the exit to
/// it finds none.
#[test]
fn targets_on_a_hidden_layer_are_not_reachable() {
    let (_dir, model) = model_with_clock("HIDE");
    let run = walk_package("fold", &model);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(!run.status.success(), "{stdout}");
    assert!(
        stdout.contains(r#"right exit to "side" (first) finds no drawn target"#),
        "{stdout}"
    );
}
