//! Memory across switches: compiling, creating and dropping both real
//! packages' components 20 times must not grow by a font file per round
//! (Zone's Anton is 123 KiB). `--nocapture` prints each step.

use korri_shell::{
    backend::headless::Headless,
    engine, fonts, memory,
    package::{self, Package},
};
use slint::ComponentHandle;
use std::path::Path;

fn rss() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}

fn trim() {
    // SAFETY: glibc's malloc_trim returns freed heap to the system.
    unsafe { libc::malloc_trim(0) };
}

#[test]
fn memory_stays_flat_over_20_rounds_of_both_packages() {
    // As korri-shell's main does: without it a freed frame buffer pair can
    // stay in the heap and show as one 2.5 MiB step in a random round.
    memory::fix_mmap_threshold();
    Headless::install();
    let zone = Package::at(&Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/zone")).unwrap();
    let pico = Package::at(&Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/pico")).unwrap();
    let report = |label: &str, f: &mut dyn FnMut()| {
        for _ in 0..3 {
            f();
        }
        trim();
        let before = rss();
        for _ in 0..20 {
            f();
        }
        trim();
        let after = rss();
        let per_round = (after as i64 - before as i64) / 20;
        eprintln!("{label}: {before} -> {after} KiB ({per_round:+} KiB per round)");
        assert!(per_round < 48, "{label} grows {per_round} KiB per round");
    };
    report("compile zone+pico", &mut || {
        drop(package::compile(&zone).unwrap());
        drop(package::compile(&pico).unwrap());
    });
    let zone_views = package::compile(&zone).unwrap();
    let pico_views = package::compile(&pico).unwrap();
    report("create+show+drop", &mut || {
        for views in [&zone_views, &pico_views] {
            let c = views.surface.create().unwrap();
            c.show().unwrap();
            Headless::render();
            c.hide().unwrap();
            drop(c);
        }
    });
    report("create+drop (no show)", &mut || {
        for views in [&zone_views, &pico_views] {
            drop(views.surface.create().unwrap());
        }
    });
    report("zone create+show+drop", &mut || {
        let c = zone_views.surface.create().unwrap();
        c.show().unwrap();
        Headless::render();
        c.hide().unwrap();
    });
    report("pico create+show+drop", &mut || {
        let c = pico_views.surface.create().unwrap();
        c.show().unwrap();
        Headless::render();
        c.hide().unwrap();
    });
    eprintln!(
        "faces registered: Anton {}, Pico Tiny5 {}",
        fonts::registered_faces("Anton"),
        fonts::registered_faces("Pico Tiny5")
    );
    let source = zone.logic_source().unwrap();
    report("engine load", &mut || {
        drop(engine::Engine::load(&source, engine::Limits::default()).unwrap());
    });
}
