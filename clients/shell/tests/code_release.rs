//! `memory::release_code`: after a compile, the binary's resident code pages are dropped,
//! and the code runs again from the page cache. A process of its own: other tests running
//! code meanwhile would fault pages back in between the measurements.

use korri_shell::memory;

/// The resident pages of this program's code mapping (`r-xp`), in KiB.
fn code_kib() -> u64 {
    let exe = std::fs::read_link("/proc/self/exe").unwrap();
    let exe = exe.to_string_lossy().into_owned();
    let smaps = std::fs::read_to_string("/proc/self/smaps").unwrap();
    let mut in_code = false;
    let mut kib = 0;
    for line in smaps.lines() {
        if line.contains(" r-xp ") {
            in_code = line.ends_with(exe.as_str());
        } else if in_code && line.starts_with("Rss:") {
            kib += line
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse::<u64>()
                .unwrap();
            in_code = false;
        }
    }
    kib
}

#[test]
fn released_code_pages_are_dropped_and_run_again() {
    // Run a good part of the code (the compiler) without `package::compile`, which
    // releases it itself.
    let compile = || {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/pico/main.slint");
        let result =
            pollster::block_on(slint_interpreter::Compiler::default().build_from_path(path));
        assert!(!result.has_errors());
    };
    compile();
    let before = code_kib();
    memory::release_code();
    let after = code_kib();
    assert!(
        after * 4 < before,
        "code resident {before} KiB, after release {after} KiB"
    );
    // The code runs as before.
    compile();
    assert!(code_kib() > after);
}
