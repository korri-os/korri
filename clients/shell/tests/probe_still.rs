//! surface-probe compares frames rendered at different times. It runs on a
//! manual clock and holds `animation-tick()` still, and cached layers replay
//! the same draw calls as direct drawing, so an unchanged view applied again
//! changes no pixel: Zone's moving ring dots and breathing triangles (drawn
//! from `animation-tick()`) are not measured as focus boxes.

use std::path::{Path, PathBuf};
use std::process::Command;

fn probe() -> PathBuf {
    // Cargo builds examples before integration tests, next to deps/.
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().parent().unwrap();
    dir.join("examples").join("surface-probe")
}

#[test]
fn zone_states_applied_again_unchanged_change_no_pixel() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scenarios = root.join("packages/zone/preview/scenarios");
    let out = tempfile::tempdir().unwrap();
    // A scenario's first model is a SurfaceModel.
    let models = ["games", "launch"]
        .map(|name| {
            let scenario: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(scenarios.join(format!("{name}.json"))).unwrap(),
            )
            .unwrap();
            let file = out.path().join(format!("{name}.model.json"));
            std::fs::write(&file, scenario["models"][0].to_string()).unwrap();
            file.to_string_lossy().into_owned()
        })
        .join(",");
    let run = Command::new(probe())
        .arg("walk")
        .arg(root.join("packages/zone"))
        .args([
            "--model",
            &models,
            "--sizes",
            "640x480,320x240",
            "--max",
            "12",
            "--still",
            "--out",
        ])
        .arg(out.path())
        .output()
        .expect("run surface-probe");
    let stdout = String::from_utf8_lossy(&run.stdout);
    let still: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains("applied again changed"))
        .collect();
    assert!(still.is_empty(), "{}", still.join("\n"));
    assert!(
        stdout.contains(" states, "),
        "{stdout}{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
