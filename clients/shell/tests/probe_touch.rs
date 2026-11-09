//! The tap judge uses native Slint hit-testing, not an invocation of the root callback.
use std::{path::Path, process::Command};

fn run(package: &Path, sizes: &str) -> std::process::Output {
    let executable = std::env::current_exe().unwrap();
    let probe = executable
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/surface-probe");
    Command::new(probe)
        .arg("walk")
        .arg(package)
        .arg("--model")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/model.json"))
        .args(["--sizes", sizes, "--max", "4"])
        .output()
        .unwrap()
}

#[test]
fn every_drawn_target_is_tapped_at_every_contract_size_and_wrong_hits_fail() {
    let root = tempfile::tempdir().unwrap();
    let package = root.path().join("touch");
    std::fs::create_dir_all(package.join("fonts")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/packages/probe/fonts/tiny5.ttf"),
        package.join("fonts/tiny5.ttf"),
    )
    .unwrap();
    let view = r#"
        import "fonts/tiny5.ttf";
        export struct Target { focused: bool, focus-id: string }
        export component Surface inherits Window {
            callback activated(target: string, at: float, released: bool);
            callback dragged(region: string, by: int);
            in property <[Target]> targets;
            background: #102030;
            for target[i] in root.targets: Rectangle {
                x: i * root.width / 2; y: 20px;
                width: root.width / 2; height: 40px;
                accessible-role: button;
                accessible-item-selectable: true;
                accessible-item-selected: target.focused;
                background: target.focused ? #f0c040 : #304050;
                TouchArea { clicked => { root.activated(target.focus-id, -1, false); } }
            }
        }
    "#;
    std::fs::write(package.join("main.slint"), view).unwrap();
    std::fs::write(package.join("logic.js"), r#"
        globalThis.surface = {
            init: () => 0,
            update: (s, m) => ({ state: m._tag === "Pressed" ? (m.button === "right" ? 1 : m.button === "left" ? 0 : s) : s, requests: [] }),
            view: s => ({ targets: [0, 1].map(i => ({ focused: s === i, focusId: "target:" + i, focusRegion: "row" })) }),
        };
    "#).unwrap();
    let good = run(&package, "320x240,640x480,480x800,1280x720,1920x1080");
    let stdout = String::from_utf8_lossy(&good.stdout);
    assert!(
        good.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&good.stderr)
    );
    assert_eq!(stdout.matches("0 failures").count(), 5, "{stdout}");
    std::fs::write(
        package.join("main.slint"),
        view.replace("root.activated(target.focus-id", "root.activated(\"wrong\""),
    )
    .unwrap();
    let bad = run(&package, "640x480");
    let stdout = String::from_utf8_lossy(&bad.stdout);
    assert!(!bad.status.success(), "{stdout}");
    assert!(
        stdout.contains("synthetic tap") && stdout.contains("expected Activated with target"),
        "{stdout}{}",
        String::from_utf8_lossy(&bad.stderr)
    );
}

fn package_with_rows(root: &Path, view: &str, logic: &str) -> std::path::PathBuf {
    let package = root.join("touch");
    std::fs::create_dir_all(package.join("fonts")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/packages/probe/fonts/tiny5.ttf"),
        package.join("fonts/tiny5.ttf"),
    )
    .unwrap();
    std::fs::write(package.join("main.slint"), view).unwrap();
    std::fs::write(package.join("logic.js"), logic).unwrap();
    package
}

#[test]
fn never_visible_virtual_targets_do_not_fail_but_visible_untappable_targets_do() {
    let root = tempfile::tempdir().unwrap();
    let view = r#"
        import "fonts/tiny5.ttf";
        export struct Target { focused: bool, focus-id: string }
        export component Surface inherits Window {
            callback activated(string, float, bool);
            callback dragged(string, int);
            in property <[Target]> rows;
            background: #102030;
            for row[i] in root.rows: Rectangle {
                // One fully visible target, one window-clipped sliver, and one
                // entirely off-window target from a focus-paged virtual row.
                x: i == 0 ? 20px : i == 1 ? 300px : 400px;
                y: 40px; width: 80px; height: 40px;
                accessible-role: button;
                accessible-item-selectable: true;
                accessible-item-selected: row.focused;
                background: row.focused ? #f0c040 : #304050;
                TouchArea { clicked => { root.activated(row.focus-id, -1, false); } }
            }
            Rectangle {
                x: 20px; y: 120px; width: 80px; height: 10px; clip: true;
                Rectangle {
                    width: 80px; height: 40px;
                    accessible-role: button;
                    accessible-item-selectable: true;
                    accessible-item-selected: root.rows[3].focused;
                    background: root.rows[3].focused ? #f0c040 : #304050;
                    TouchArea { clicked => { root.activated(root.rows[3].focus-id, -1, false); } }
                }
            }
        }
    "#;
    // Hide the fourth repeated row; its actual element is the ancestor-clipped row.
    let view = view.replace(
        "for row[i] in root.rows: Rectangle {",
        "for row[i] in root.rows: Rectangle { visible: i < 3;",
    );
    let package = package_with_rows(
        root.path(),
        &view,
        r#"
        globalThis.surface = {
            init: () => 0,
            update: s => ({ state: s, requests: [] }),
            view: () => ({ rows: [0, 1, 2, 3].map(i => ({ focused: i === 0, focusId: "row:" + i, focusRegion: "row:" + i })) }),
        };
    "#,
    );
    let good = run(&package, "320x240");
    assert!(
        good.status.success(),
        "{}{}",
        String::from_utf8_lossy(&good.stdout),
        String::from_utf8_lossy(&good.stderr)
    );
    std::fs::write(
        package.join("main.slint"),
        view.replace("root.activated(row.focus-id, -1, false);", ""),
    )
    .unwrap();
    let bad = run(&package, "320x240");
    let stdout = String::from_utf8_lossy(&bad.stdout);
    assert!(!bad.status.success(), "{stdout}");
    assert!(
        stdout.contains("tap \"row:0\"") && stdout.contains("expected Activated"),
        "{stdout}{}",
        String::from_utf8_lossy(&bad.stderr)
    );
    // A target fully visible in the walked view can disappear during the
    // preceding tap's settlement. It must still fail the coverage guard.
    let vanished = view
        .replace(
            "in property <[Target]> rows;",
            "in property <[Target]> rows; property <bool> hide-next;",
        )
        .replace(
            "visible: i < 3;",
            "visible: i < 3 && !(i == 1 && root.hide-next);",
        )
        .replace("i == 1 ? 300px", "i == 1 ? 120px")
        .replace(
            "root.activated(row.focus-id, -1, false);",
            "root.activated(row.focus-id, -1, false); if (i == 0) { root.hide-next = true; }",
        );
    std::fs::write(package.join("main.slint"), vanished).unwrap();
    let bad = run(&package, "320x240");
    let stdout = String::from_utf8_lossy(&bad.stdout);
    assert!(!bad.status.success(), "{stdout}");
    assert!(
        stdout.contains("target \"row:1\" was fully visible but no walked state"),
        "{stdout}{}",
        String::from_utf8_lossy(&bad.stderr)
    );
}
