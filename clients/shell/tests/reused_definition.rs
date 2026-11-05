//! The shell compiles a package once per process and creates each later load's
//! component from the same definition (`Shell::load`). A component created from a
//! definition that already served another, dropped one draws the same pixels as one
//! from a fresh compile.

use korri_shell::{
    backend::headless::Headless,
    package::{self, Package},
};
use slint::ComponentHandle;
use std::{path::Path, time::Duration};

/// The first frames of a component from `definition`, on the manual clock.
fn frames(definition: &slint_interpreter::ComponentDefinition) -> Vec<Vec<u16>> {
    let component = definition.create().unwrap();
    component.show().unwrap();
    let mut frames = Vec::new();
    for _ in 0..4 {
        frames.push(Headless::render().expect("a frame"));
        Headless::advance(Duration::from_millis(16));
    }
    component.hide().unwrap();
    frames
}

#[test]
fn a_reused_definition_draws_what_a_fresh_compile_draws() {
    Headless::install();
    Headless::manual_clock();
    Headless::freeze_motion();
    for id in ["zone", "pico"] {
        let package = Package::at(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("packages")
                .join(id),
        )
        .unwrap();
        let kept = package::compile(&package).unwrap();
        let first = frames(&kept.surface);
        let reused = frames(&kept.surface);
        let fresh = frames(&package::compile(&package).unwrap().surface);
        assert!(
            first.iter().any(|f| f.iter().any(|&p| p != first[0][0])),
            "{id} drew nothing"
        );
        assert!(
            reused == fresh,
            "{id}: the reused definition drew other pixels"
        );
        assert!(
            first == fresh,
            "{id}: the first component drew other pixels"
        );
    }
}
