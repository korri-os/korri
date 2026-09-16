use korrid::script::{
    self,
    source::{SnapshotLimits, SourceSnapshot},
};
use std::{fs, os::unix::fs::symlink};

fn package(root: &std::path::Path, sources: &[&str]) -> SourceSnapshot {
    let sources = sources
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    SourceSnapshot::package_plugin(root, "plugin.ts", &sources).unwrap()
}

fn limits() -> SnapshotLimits {
    SnapshotLimits {
        bytes: 128 * 1024,
        entries: 32,
        path_bytes: 16 * 1024,
        steps: 256,
    }
}

#[test]
fn memory_and_directory_snapshots_use_the_same_evaluator_and_keep_original_bytes() {
    let root = tempfile::tempdir().unwrap();
    let text = b"export const name: string = 'snapshot';";
    fs::write(root.path().join("plugin.ts"), text).unwrap();
    let disk = SourceSnapshot::from_directory(root.path(), &["plugin.ts"], limits()).unwrap();
    let memory = SourceSnapshot::from_memory(&[("plugin.ts", text.as_slice())], limits()).unwrap();
    fs::write(root.path().join("plugin.ts"), "not JavaScript").unwrap();
    assert_eq!(disk.bytes("plugin.ts").unwrap(), text);
    assert_eq!(
        script::eval_plugin_snapshot(&disk).unwrap(),
        script::eval_plugin_snapshot(&memory).unwrap()
    );
    fs::remove_file(root.path().join("plugin.ts")).unwrap();
    assert_eq!(disk.bytes("plugin.ts").unwrap(), text);
    assert!(disk.bytes("unselected.ts").is_err());
}

#[test]
fn aggregate_bytes_are_reserved_before_reading_or_copying_the_next_input() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.ts"), [0xff; 8]).unwrap();
    // Invalid UTF-8 is admissible bytes, not a parsing side effect. The second
    // sparse file exceeds the aggregate without any content read/allocation.
    let file = fs::File::create(root.path().join("b.ts")).unwrap();
    file.set_len(9).unwrap();
    let bounded = SnapshotLimits {
        bytes: 16,
        ..limits()
    };
    assert!(
        SourceSnapshot::from_directory(root.path(), &["a.ts", "b.ts"], bounded)
            .unwrap_err()
            .contains("byte budget")
    );
    assert!(
        SourceSnapshot::from_memory(&[("a.ts", &[0; 8][..]), ("b.ts", &[0; 9][..])], bounded)
            .unwrap_err()
            .contains("byte budget")
    );
    file.set_len(1 << 30).unwrap();
    assert!(
        SourceSnapshot::from_directory(root.path(), &["b.ts"], bounded)
            .unwrap_err()
            .contains("byte budget")
    );
}

#[test]
fn canonical_aliases_share_one_copy_but_do_not_grant_unselected_source() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/plugin.ts"), "12345678").unwrap();
    symlink("src/plugin.ts", root.path().join("plugin.ts")).unwrap();
    let bounded = SnapshotLimits {
        bytes: 8,
        ..limits()
    };
    let snapshot =
        SourceSnapshot::from_directory(root.path(), &["plugin.ts", "src/plugin.ts"], bounded)
            .unwrap();
    assert_eq!(snapshot.identity("plugin.ts").unwrap(), "src/plugin.ts");
    assert!(std::ptr::eq(
        snapshot.bytes("plugin.ts").unwrap().as_ptr(),
        snapshot.bytes("src/plugin.ts").unwrap().as_ptr()
    ));
    assert!(SourceSnapshot::from_directory(root.path(), &["plugin.ts"], bounded).is_err());
    symlink("src", root.path().join("alias")).unwrap();
    let snapshot =
        SourceSnapshot::from_directory(root.path(), &["alias/plugin.ts", "src/plugin.ts"], bounded)
            .unwrap();
    assert_eq!(
        snapshot.identity("alias/plugin.ts").unwrap(),
        "src/plugin.ts"
    );
}

#[test]
fn canonical_identity_resolves_directory_links_before_parent_components() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("real/deep")).unwrap();
    fs::write(root.path().join("real/plugin.ts"), "correct").unwrap();
    fs::write(root.path().join("plugin.ts"), "wrong").unwrap();
    symlink("real/deep", root.path().join("redirect")).unwrap();
    symlink("redirect/../plugin.ts", root.path().join("alias.ts")).unwrap();
    let snapshot = SourceSnapshot::from_directory(
        root.path(),
        &["alias.ts", "real/plugin.ts", "plugin.ts"],
        limits(),
    )
    .unwrap();
    assert_eq!(snapshot.identity("alias.ts").unwrap(), "real/plugin.ts");
    assert_eq!(snapshot.bytes("alias.ts").unwrap(), b"correct");
}

#[test]
fn installed_registry_rejects_oversized_and_linked_source_at_admission() {
    let root = tempfile::tempdir().unwrap();
    let selection = korrid::plugin_installation::EnabledPackage {
        id: "@test:entry".into(),
        package: root.path().into(),
        files: Default::default(),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
    };
    fs::write(
        root.path().join("plugin.ts"),
        "export const name = 'entry';",
    )
    .unwrap();
    assert!(korrid::plugin::PluginRegistry::from_installed(vec![selection.clone()]).is_ok());
    fs::File::create(root.path().join("plugin.ts"))
        .unwrap()
        .set_len(1 << 30)
        .unwrap();
    let error = korrid::plugin::PluginRegistry::from_installed(vec![selection.clone()])
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("byte budget"), "{error}");
    fs::remove_file(root.path().join("plugin.ts")).unwrap();
    fs::write(root.path().join("other.ts"), "export const name = 'entry';").unwrap();
    symlink("other.ts", root.path().join("plugin.ts")).unwrap();
    assert!(korrid::plugin::PluginRegistry::from_installed(vec![selection]).is_err());
}

#[test]
fn installed_registry_evaluates_manifest_selected_relative_and_package_imports() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::create_dir_all(root.path().join("node_modules/@fixture/title")).unwrap();
    fs::write(
        root.path().join("plugin.ts"),
        "import { prefix } from './src/prefix.ts'; import { title } from '@fixture/title'; export const name = prefix + title;",
    )
    .unwrap();
    fs::write(
        root.path().join("src/prefix.ts"),
        "export const prefix = 'package-';",
    )
    .unwrap();
    fs::write(
        root.path().join("node_modules/@fixture/title/package.json"),
        r#"{"type":"module","exports":"./index.ts"}"#,
    )
    .unwrap();
    fs::write(
        root.path().join("node_modules/@fixture/title/index.ts"),
        "export const title = 'graph';",
    )
    .unwrap();
    let selection = korrid::plugin_installation::EnabledPackage {
        id: "@test:package-graph".into(),
        package: root.path().into(),
        files: Default::default(),
        entry: "plugin.ts".into(),
        sources: vec![
            "node_modules/@fixture/title/index.ts".into(),
            "node_modules/@fixture/title/package.json".into(),
            "plugin.ts".into(),
            "src/prefix.ts".into(),
        ],
    };
    let registry = korrid::plugin::PluginRegistry::from_installed(vec![selection]).unwrap();
    assert!(registry
        .enabled_plugin_ids()
        .contains(&"@test:package-graph"));
}

#[test]
fn package_inventory_requires_the_fixed_entry_and_sorted_unique_existing_sources() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("plugin.ts"),
        "export const name = 'entry';",
    )
    .unwrap();
    fs::write(root.path().join("helper.ts"), "export const value = 1;").unwrap();
    for (entry, sources) in [
        ("other.ts", vec!["plugin.ts".to_owned()]),
        ("plugin.ts", vec![]),
        ("plugin.ts", vec!["helper.ts".into()]),
        ("plugin.ts", vec!["plugin.ts".into(), "helper.ts".into()]),
        ("plugin.ts", vec!["plugin.ts".into(), "plugin.ts".into()]),
        ("plugin.ts", vec!["../plugin.ts".into(), "plugin.ts".into()]),
        ("plugin.ts", vec!["missing.ts".into(), "plugin.ts".into()]),
    ] {
        assert!(
            SourceSnapshot::package_plugin(root.path(), entry, &sources).is_err(),
            "accepted entry={entry} sources={sources:?}"
        );
    }
}

#[test]
fn escapes_loops_noncanonical_names_and_symlink_roots_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let package = root.path().join("package");
    fs::create_dir(&package).unwrap();
    fs::write(root.path().join("outside.ts"), "secret").unwrap();
    symlink("../outside.ts", package.join("escape.ts")).unwrap();
    symlink(root.path().join("outside.ts"), package.join("absolute.ts")).unwrap();
    symlink("loop.ts", package.join("loop.ts")).unwrap();
    symlink(&package, root.path().join("linked")).unwrap();
    for name in [
        "escape.ts",
        "absolute.ts",
        "loop.ts",
        "../outside.ts",
        "/etc/passwd",
        "./escape.ts",
        "a//b",
        "",
    ] {
        assert!(
            SourceSnapshot::from_directory(&package, &[name], limits()).is_err(),
            "accepted {name}"
        );
    }
    assert!(
        SourceSnapshot::from_directory(&root.path().join("linked"), &["x.ts"], limits()).is_err()
    );
    assert!(SourceSnapshot::from_memory(&[("../x.ts", b"")], limits()).is_err());
    assert!(SourceSnapshot::from_memory(&[("x.ts", b""), ("x.ts", b"")], limits()).is_err());
}

#[test]
fn directory_fifo_socket_and_missing_source_fail_without_blocking() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("directory")).unwrap();
    let fifo =
        std::ffi::CString::new(root.path().join("fifo").as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let _socket = std::os::unix::net::UnixListener::bind(root.path().join("socket")).unwrap();
    for name in ["directory", "fifo", "socket", "missing"] {
        assert!(
            SourceSnapshot::from_directory(root.path(), &[name], limits()).is_err(),
            "accepted {name}"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn rejected_size_and_unselected_targets_do_not_read_content() {
    use std::{
        io::Read,
        os::fd::{AsRawFd, FromRawFd},
    };
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("unselected.ts");
    fs::write(&path, "123456789").unwrap();
    symlink("unselected.ts", root.path().join("alias.ts")).unwrap();
    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    assert!(fd >= 0);
    let mut events = unsafe { fs::File::from_raw_fd(fd) };
    let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert!(
        unsafe { libc::inotify_add_watch(events.as_raw_fd(), path.as_ptr(), libc::IN_ACCESS) } >= 0
    );
    let bounded = SnapshotLimits {
        bytes: 8,
        ..limits()
    };
    assert!(SourceSnapshot::from_directory(root.path(), &["unselected.ts"], bounded).is_err());
    assert!(SourceSnapshot::from_directory(root.path(), &["alias.ts"], limits()).is_err());
    let mut buffer = [0u8; 4096];
    assert_eq!(
        events.read(&mut buffer).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    // Positive control: the real filesystem monitor observes a content read.
    fs::read(root.path().join("unselected.ts")).unwrap();
    assert!(events.read(&mut buffer).unwrap() > 0);
}

#[test]
fn entry_identity_and_traversal_budgets_apply_even_to_empty_files() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.ts"), "").unwrap();
    fs::write(root.path().join("b.ts"), "").unwrap();
    let sources: &[(&str, &[u8])] = &[("a.ts", b""), ("b.ts", b"")];
    for bounded in [
        SnapshotLimits {
            entries: 1,
            ..limits()
        },
        SnapshotLimits {
            path_bytes: 1,
            ..limits()
        },
    ] {
        assert!(SourceSnapshot::from_memory(sources, bounded).is_err());
        assert!(SourceSnapshot::from_directory(root.path(), &["a.ts", "b.ts"], bounded).is_err());
    }
    let bounded = SnapshotLimits {
        steps: 0,
        ..limits()
    };
    assert!(
        SourceSnapshot::from_directory(root.path(), &["a.ts"], bounded)
            .unwrap_err()
            .contains("traversal budget")
    );
}

#[test]
fn resolution_metadata_has_no_separate_unmetered_read_path() {
    let root = tempfile::tempdir().unwrap();
    for (name, bytes) in [
        ("plugin.ts", "12345678"),
        ("package.json", "{}"),
        ("bun.lock", "{}"),
    ] {
        fs::write(root.path().join(name), bytes).unwrap();
    }
    let names = &["plugin.ts", "package.json", "bun.lock"];
    let bounded = SnapshotLimits {
        bytes: 11,
        ..limits()
    };
    assert!(SourceSnapshot::from_directory(root.path(), names, bounded)
        .unwrap_err()
        .contains("byte budget"));
    let snapshot = SourceSnapshot::from_directory(
        root.path(),
        names,
        SnapshotLimits {
            bytes: 12,
            ..limits()
        },
    )
    .unwrap();
    assert_eq!(snapshot.bytes("package.json").unwrap(), b"{}");
    assert_eq!(snapshot.bytes("bun.lock").unwrap(), b"{}");
}

#[test]
fn symlink_cycles_exhaust_traversal_without_recursion() {
    let root = tempfile::tempdir().unwrap();
    symlink("b.ts", root.path().join("a.ts")).unwrap();
    symlink("a.ts", root.path().join("b.ts")).unwrap();
    let bounded = SnapshotLimits {
        steps: 64,
        path_bytes: 1024 * 1024,
        ..limits()
    };
    assert!(
        SourceSnapshot::from_directory(root.path(), &["a.ts", "b.ts"], bounded)
            .unwrap_err()
            .contains("traversal budget")
    );
}

#[test]
fn the_existing_source_ceiling_is_exact_and_diagnostics_do_not_print_bytes() {
    let bytes = " ".repeat(128 * 1024);
    assert_eq!(
        SourceSnapshot::plugin(&bytes)
            .unwrap()
            .bytes("plugin.ts")
            .unwrap()
            .len(),
        bytes.len()
    );
    assert!(SourceSnapshot::plugin(&(bytes + " ")).is_err());
    let snapshot = SourceSnapshot::plugin("SENTINEL_SOURCE_SECRET").unwrap();
    assert!(!format!("{snapshot:?}").contains("SENTINEL_SOURCE_SECRET"));
    assert!(
        SourceSnapshot::from_memory(&[("plugin.ts", &[0xff])], limits())
            .unwrap()
            .text("plugin.ts")
            .is_err()
    );
}

#[test]
fn package_registration_grants_only_the_manifest_selected_graph() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("plugin.ts"),
        "export const name = 'entry';",
    )
    .unwrap();
    fs::write(root.path().join("native"), "not source").unwrap();
    let snapshot = package(root.path(), &["plugin.ts"]);
    assert!(snapshot.bytes("native").is_err());
    fs::write(
        root.path().join("plugin.ts"),
        "import './helper.ts'; export const name = 'entry';",
    )
    .unwrap();
    fs::write(root.path().join("helper.ts"), "export const secret = 1").unwrap();
    assert!(script::eval_plugin_snapshot(&package(root.path(), &["plugin.ts"])).is_err());
    assert_eq!(
        script::eval_plugin_snapshot(&package(root.path(), &["helper.ts", "plugin.ts"])).unwrap(),
        r#"{"name":"entry"}"#
    );
    fs::File::create(root.path().join("plugin.ts"))
        .unwrap()
        .set_len(128 * 1024 + 1)
        .unwrap();
    let oversized = SourceSnapshot::package_plugin(
        root.path(),
        "plugin.ts",
        &["helper.ts".into(), "plugin.ts".into()],
    )
    .unwrap();
    assert!(script::eval_plugin_snapshot(&oversized)
        .unwrap_err()
        .contains("source exceeds 128 KiB"));
}
