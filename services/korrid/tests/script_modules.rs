use korrid::script::{
    self,
    source::{SnapshotLimits, SourceSnapshot},
};
use std::{fs, os::unix::fs::symlink};

fn snapshot(sources: &[(&str, &str)]) -> SourceSnapshot {
    let sources: Vec<_> = sources
        .iter()
        .map(|(name, text)| (*name, text.as_bytes()))
        .collect();
    SourceSnapshot::from_memory(&sources, limits()).unwrap()
}

fn limits() -> SnapshotLimits {
    SnapshotLimits {
        bytes: 128 * 1024,
        entries: 32,
        path_bytes: 32 * 1024,
        steps: 256,
    }
}

#[test]
fn a_cycle_back_into_the_entry_uses_its_native_binding() {
    let graph = snapshot(&[
        ("plugin.ts", "import { read } from './helper.ts'; export const name = 'entry-cycle'; export const handlers = {'launch.prepare': function () { return {name: read()}; }}"),
        ("helper.ts", "import { name } from './plugin.ts'; export function read() { return name; }"),
    ]);
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"name":"entry-cycle"}"#
    );
}

#[test]
fn imported_module_initialization_obeys_the_same_completion_rules() {
    for source in [
        "await Promise.reject(1);",
        "await new Promise(resolve => setTimeout(resolve, 0));",
    ] {
        let graph = snapshot(&[
            ("plugin.ts", "import './helper.ts'; export const name = 'no'; export const handlers = {'launch.prepare': function () { return {}; }}"),
            ("helper.ts", source),
        ]);
        assert!(script::eval_plugin_snapshot(&graph).is_err());
        assert!(script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").is_err());
    }
}

#[test]
fn relative_typescript_and_javascript_form_one_native_module_graph() {
    let graph = snapshot(&[
        ("plugin.ts", "import { label } from './lib/label'; import { suffix } from './suffix.js'; export const name: string = label + suffix;"),
        ("lib/label.ts", "export { label } from '../value.ts';"),
        ("value.ts", "export const label: string = 'closed';"),
        ("suffix.js", "export const suffix = '-graph';"),
    ]);
    assert_eq!(
        script::eval_plugin_snapshot(&graph).unwrap(),
        r#"{"name":"closed-graph"}"#
    );
}

#[test]
fn native_cycles_reexports_and_live_bindings_are_preserved() {
    let graph = snapshot(&[
        ("plugin.ts", "export { name } from './a.ts'; export * from './launch.ts';"),
        ("a.ts", "import { suffix } from './b.ts'; export const name = 'cycle' + suffix(); export let calls = 0; export function bump() { calls++; }"),
        ("b.ts", "import { calls } from './a.ts'; export function suffix() { return '-ok'; } export function count() { return calls; }"),
        ("launch.ts", "import { bump } from './a.ts'; import { count } from './b.ts'; export const handlers = {'launch.prepare': function () { bump(); return { calls: count() }; }}"),
    ]);
    assert_eq!(
        script::eval_plugin_snapshot(&graph).unwrap(),
        r#"{"name":"cycle-ok"}"#
    );
    for _ in 0..2 {
        assert_eq!(
            script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
            r#"{"calls":1}"#
        );
    }
}

#[test]
fn preparation_checks_static_dependencies_before_any_plugin_code_runs() {
    for dependency in [
        "import { unused } from './missing.ts'; export const handlers = {'launch.prepare': function () { return unused; }}",
        "export * from './missing.ts';",
        "import './missing.ts';",
        "import {} from './missing.ts';",
        "import { unused } from './missing.ts';",
        // TS preserves this statement's side effect as `import {}`. Only
        // `import type` removes the dependency, not inline type specifiers.
        "import { type B } from './missing.ts';",
    ] {
        let graph = snapshot(&[(
            "plugin.ts",
            &format!("throw new Error('EXECUTED'); export const name = 'closed'; {dependency}"),
        )]);
        let error = script::eval_plugin_snapshot(&graph).unwrap_err();
        assert!(error.contains("not selected"), "{dependency}: {error}");
        assert!(script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null")
            .unwrap_err()
            .contains("not selected"));
    }
}

#[test]
fn true_type_only_dependencies_do_not_require_admission() {
    for declaration in [
        "import type { A } from 'unavailable';",
        "export type { C } from './also-missing';",
        "export { type C } from './also-missing';",
        "export type * from './types';",
        "type D = import('./only-a-type').D;",
    ] {
        let graph = snapshot(&[(
            "plugin.ts",
            &format!("{declaration} export const name: string = 'types';"),
        )]);
        assert_eq!(
            script::eval_plugin_snapshot(&graph)
                .unwrap_or_else(|error| panic!("{declaration}: {error}")),
            r#"{"name":"types"}"#
        );
    }
}

#[test]
fn unselected_packages_and_unsupported_relative_forms_remain_rejected() {
    for specifier in [
        "fs",
        "node:fs",
        "effect",
        "/outside.ts",
        "../outside.ts",
        "./../../outside.ts",
        "file:///outside.ts",
        "https://example.org/a.ts",
        "./helper.ts?query",
        "./helper.ts#fragment",
        "./helper.json",
        "./folder",
        "./helper.js",
        "./helper/.",
        "./helper/child/..",
        "./helper.ts/",
        "./%2e%2e/helper.ts",
        "./helper\\\\file.ts",
    ] {
        let graph = snapshot(&[
            (
                "plugin.ts",
                &format!("import '{specifier}'; export const name = 'closed';"),
            ),
            ("helper.ts", "export const value = 1;"),
            ("helper.json", "{}"),
            ("folder/index.ts", "export const value = 1;"),
        ]);
        assert!(
            script::eval_plugin_snapshot(&graph).is_err(),
            "accepted {specifier}"
        );
    }
}

#[test]
fn dynamic_import_and_import_meta_are_rejected_even_in_deferred_code() {
    for body in [
        "return import('./helper.ts')",
        "return import.meta",
        "if (false) return import('./helper.ts'); return {}",
    ] {
        let graph = snapshot(&[
            (
                "plugin.ts",
                &format!("export const name = 'closed'; export const handlers = {{'launch.prepare': function () {{ {body} }}}}"),
            ),
            ("helper.ts", "export const value = 1;"),
        ]);
        assert!(
            script::eval_plugin_snapshot(&graph).is_err(),
            "accepted {body}"
        );
    }
}

#[test]
fn frozen_filesystem_aliases_use_canonical_identity_and_canonical_parent() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("real")).unwrap();
    fs::write(root.path().join("plugin.ts"), "import { value as a } from './alias.ts'; import { value as b } from './real/value.ts'; export const name = 'aliases'; export const config = { same: a === b, label: a.label }; export const handlers = {'launch.prepare': function () { return { same: a === b }; }}").unwrap();
    fs::write(
        root.path().join("real/value.ts"),
        "import { label } from './label'; export const value = { label };",
    )
    .unwrap();
    fs::write(
        root.path().join("real/label.ts"),
        "export const label = 'retained';",
    )
    .unwrap();
    symlink("real/value.ts", root.path().join("alias.ts")).unwrap();
    let graph = SourceSnapshot::from_directory(
        root.path(),
        &["plugin.ts", "alias.ts", "real/value.ts", "real/label.ts"],
        limits(),
    )
    .unwrap();
    root.close().unwrap();
    assert_eq!(
        script::eval_plugin_snapshot(&graph).unwrap(),
        r#"{"config":{"same":true,"label":"retained"},"name":"aliases"}"#
    );
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"same":true}"#
    );
}

#[test]
fn a_single_transformed_module_keeps_its_emitted_output_ceiling() {
    let helper = (0..3200)
        .map(|n| format!("enum E{n} {{ A, B, C, D }}\n"))
        .collect::<String>();
    let emitted = script::transpile_ts(&helper).unwrap().len();
    assert!(helper.len() < 128 * 1024);
    assert!(emitted > 512 * 1024 && emitted < 4 * 1024 * 1024);
    eprintln!(
        "single-module expansion: {} source bytes, {emitted} emitted bytes",
        helper.len()
    );
    let graph = snapshot(&[
        ("plugin.ts", "import './helper.ts'; export const name = 'bounded'; export const handlers = {'launch.prepare': function () { return {}; }}"),
        ("helper.ts", &helper),
    ]);
    for result in [
        script::eval_plugin_snapshot(&graph),
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null"),
    ] {
        let error = result.expect_err("oversized emitted module must fail preparation");
        assert!(error.contains("JavaScript exceeds 512 KiB"), "{error}");
    }
}

#[test]
fn transformed_output_is_bounded_in_aggregate_before_evaluation() {
    // Repeated TS enums expand well beyond their input size. Each module is
    // below the source ceiling; together they exceed the 4 MiB graph ceiling.
    let helper = (0..1100)
        .map(|n| format!("enum E{n} {{ A, B, C, D }}\n"))
        .collect::<String>();
    let emitted = script::transpile_ts(&helper).unwrap().len();
    assert!(emitted <= 512 * 1024 && emitted * 24 > 4 * 1024 * 1024);
    let names: Vec<_> = (0..24).map(|n| format!("helper{n}.ts")).collect();
    let entry = names
        .iter()
        .map(|name| format!("import './{name}';"))
        .collect::<String>()
        + "export const name = 'bounded';";
    let mut sources = vec![("plugin.ts", entry.as_bytes())];
    sources.extend(names.iter().map(|name| (name.as_str(), helper.as_bytes())));
    let graph = SourceSnapshot::from_memory(
        &sources,
        SnapshotLimits {
            bytes: 2 * 1024 * 1024,
            ..limits()
        },
    )
    .unwrap();
    let error = script::eval_plugin_snapshot(&graph).unwrap_err();
    assert!(error.contains("JavaScript graph exceeds 4 MiB"), "{error}");
    assert!(script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null")
        .unwrap_err()
        .contains("JavaScript graph exceeds 4 MiB"));
}

#[test]
fn synthesized_dynamic_imports_cannot_load_even_an_already_linked_module() {
    for expression in [
        "eval('import(\"./helper.ts\")')",
        "Function('return import(\"./helper.ts\")')()",
        "(async () => {}).constructor('return import(\"./helper.ts\")')()",
    ] {
        let graph = snapshot(&[
            ("plugin.ts", &format!("import {{ label }} from './helper.ts'; export const name = label; export const config = {{ blocked: await {expression}.then(() => false, () => true) }};")),
            ("helper.ts", "export const label = 'closed';"),
        ]);
        let json: serde_json::Value =
            serde_json::from_str(&script::eval_plugin_snapshot(&graph).unwrap()).unwrap();
        assert_eq!(json["config"]["blocked"], true, "{expression}");
    }
}

#[test]
fn malformed_retained_dependencies_fail_preparation_and_unreachable_files_do_not_run() {
    for bytes in [b"export const broken: = 1".as_slice(), &[0xff]] {
        let graph = SourceSnapshot::from_memory(&[
            ("plugin.ts", b"import './helper.ts'; throw new Error('EXECUTED'); export const name = 'closed';"),
            ("helper.ts", bytes),
        ], limits()).unwrap();
        let error = script::eval_plugin_snapshot(&graph).unwrap_err();
        assert!(
            error.contains("parse") || error.contains("UTF-8"),
            "{error}"
        );
    }
    let graph = snapshot(&[
        ("plugin.ts", "export const name = 'closed';"),
        ("unreachable.ts", "throw new Error('UNREACHABLE');"),
    ]);
    assert_eq!(
        script::eval_plugin_snapshot(&graph).unwrap(),
        r#"{"name":"closed"}"#
    );
}

#[test]
fn lexical_relative_paths_and_entry_aliases_share_canonical_modules() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    symlink("src/entry.ts", root.path().join("plugin.ts")).unwrap();
    fs::write(root.path().join("src/entry.ts"), "import { value as a } from './helper'; import { value as b } from './unused/../helper.ts'; export const name = 'entry'; export const config = { same: a === b };").unwrap();
    fs::write(
        root.path().join("src/helper.ts"),
        "export const value = {};",
    )
    .unwrap();
    let graph = SourceSnapshot::from_directory(
        root.path(),
        &["plugin.ts", "src/entry.ts", "src/helper.ts"],
        limits(),
    )
    .unwrap();
    root.close().unwrap();
    assert_eq!(
        script::eval_plugin_snapshot(&graph).unwrap(),
        r#"{"config":{"same":true},"name":"entry"}"#
    );
}

#[test]
fn single_file_entrypoints_keep_their_existing_source_and_input_ceilings() {
    let prefix = "export const name = 'ceiling';";
    let ts = format!("{prefix}{}", " ".repeat(128 * 1024 - prefix.len()));
    assert!(script::eval_plugin_ts(&ts).is_ok());
    assert!(script::eval_plugin_ts(&(ts.clone() + " ")).is_err());
    let js = format!("{prefix}{}", " ".repeat(512 * 1024 - prefix.len()));
    assert!(script::eval_plugin(&js).is_ok());
    assert!(script::eval_plugin(&(js + " "))
        .unwrap_err()
        .contains("JavaScript exceeds 512 KiB"));
    let graph = snapshot(&[(
        "plugin.ts",
        "export const name = 'ceiling'; export const handlers = {'launch.prepare': function () { return {}; }}",
    )]);
    assert!(script::call_plugin_operation_snapshot(&graph, "launch.prepare",
        &format!("null{}", " ".repeat(512 * 1024 - 4))
    )
    .is_ok());
    assert!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", &" ".repeat(512 * 1024 + 1))
            .unwrap_err()
            .contains("input exceeds 512 KiB")
    );
    for source in [
        "export const name = 'closed'; export const handlers = {'launch.prepare': function () { return import('./missing.ts'); }}",
        "export const name = 'closed'; export const handlers = {'launch.prepare': function () { return import.meta; }}",
    ] {
        assert!(script::eval_plugin(source).is_err());
        assert!(script::eval_plugin_ts(source).is_err());
        assert!(script::call_plugin_operation_ts(source, "launch.prepare", "null").is_err());
    }
}

#[test]
fn dependencies_and_launch_share_vm_limits_without_host_capabilities() {
    for body in [
        "while (true) {}",
        "const a = []; while (true) a.push(new Array(100000).fill(1));",
        "return Array(1000).fill('x'.repeat(10000));",
        "return require('node:fs');",
        "return process.env;",
        "return fetch('https://example.org');",
    ] {
        let graph = snapshot(&[
            (
                "plugin.ts",
                "export const name = 'bounded'; export { handlers } from './helper.ts';",
            ),
            (
                "helper.ts",
                &format!("export const handlers = {{'launch.prepare': function () {{ {body} }}}}"),
            ),
        ]);
        assert!(script::eval_plugin_snapshot(&graph).is_ok(), "{body}");
        assert!(
            script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").is_err(),
            "{body}"
        );
    }
    let graph = snapshot(&[
        (
            "plugin.ts",
            "import './helper.ts'; export const name = 'bounded';",
        ),
        ("helper.ts", "while (true) {}"),
    ]);
    assert!(script::eval_plugin_snapshot(&graph).is_err());
}
