//! The view binder brings a changed array's model up to date in place
//! (`view::update_rows`): rows whose JSON did not change keep their repeater
//! instances, changed rows are replaced, rows are added or removed at the end.
//! Zone and Pico draw the same pixels as with a new model for every changed
//! array (the shell before), frame by frame through presses.

use korri_shell::bench::{self, Options};
use std::{collections::BTreeMap, path::Path};

fn frames(
    surface: &str,
    reuse_models: bool,
    presses: &str,
    preferences: &[(&str, &str)],
) -> Vec<Vec<u8>> {
    let surface = surface.to_owned();
    let presses = presses.to_owned();
    let preferences: BTreeMap<String, String> = preferences
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    // Slint's platform and clock are per thread: one run per thread.
    std::thread::spawn(move || {
        let options = Options {
            packages: Path::new(env!("CARGO_MANIFEST_DIR")).join("packages"),
            surface,
            size: (640, 480),
            frames: 200,
            games: 12,
            presses: bench::parse_presses(&presses).unwrap(),
            preferences,
            full: true,
            threads: Some(1),
            cutoff: true,
            reuse_models,
            logic_thread: false,
        };
        bench::run_captured(options, true).unwrap().1
    })
    .join()
    .unwrap()
}

fn assert_same(name: &str, presses: &str, preferences: &[(&str, &str)]) {
    let reused = frames(name, true, presses, preferences);
    let fresh = frames(name, false, presses, preferences);
    assert!(reused.len() >= 190);
    for (n, (a, b)) in reused.iter().zip(&fresh).enumerate() {
        let differing = a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count();
        assert_eq!(
            differing, 0,
            "{name} [{presses}]: frame {n} differs in {differing} pixels"
        );
    }
}

#[test]
fn zone_draws_the_same_with_models_kept_and_with_new_models() {
    assert_same(
        "zone",
        "right,right,left,down,right,up,left,options,down,confirm",
        &[("zone.look", "modern")],
    );
}

#[test]
fn pico_draws_the_same_with_models_kept_and_with_new_models() {
    assert_same(
        "pico",
        "right,right,down,right,up,left,confirm,back,down",
        &[],
    );
}
