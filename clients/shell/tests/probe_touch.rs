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
