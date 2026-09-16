use korrid::script::{
    self,
    source::{SnapshotLimits, SourceSnapshot},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn snapshot(plugin: &str, files: &[(&str, &str)]) -> SourceSnapshot {
    let mut bytes = vec![("plugin.ts", plugin.as_bytes())];
    bytes.extend(
        files
            .iter()
            .map(|(name, source)| (*name, source.as_bytes())),
    );
    SourceSnapshot::from_memory(&bytes, limits()).unwrap()
}

fn limits() -> SnapshotLimits {
    SnapshotLimits {
        bytes: 64 * 1024 * 1024,
        entries: 4096,
        path_bytes: 2 * 1024 * 1024,
        steps: 10000,
    }
}

// Test-only selection of original installed package artifacts. The evaluator
// receives only retained bytes, never these paths or a filesystem resolver.
fn native_snapshot(plugin: &str, packages: &[(&str, &str)]) -> SourceSnapshot {
    native_snapshot_with_files(plugin, packages, &[])
}

fn native_snapshot_with_files(
    plugin: &str,
    packages: &[(&str, &str)],
    extra: &[(&str, &str)],
) -> SourceSnapshot {
    fn collect(root: &Path, directory: &Path, prefix: &str, files: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().unwrap() != "node_modules" {
                    collect(root, &path, prefix, files);
                }
            } else if matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("js" | "cjs" | "mjs" | "json")
            ) {
                files.push((
                    format!(
                        "node_modules/{prefix}/{}",
                        path.strip_prefix(root).unwrap().display()
                    ),
                    fs::read(path).unwrap(),
                ));
            }
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = vec![("plugin.ts".to_owned(), plugin.as_bytes().to_vec())];
    files.extend(
        extra
            .iter()
            .map(|(name, source)| ((*name).to_owned(), source.as_bytes().to_vec())),
    );
    for (directory, name) in packages {
        let path = root.join(directory).join("node_modules").join(name);
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("package.json")).unwrap()).unwrap();
        let version = match *name {
            "effect" => "4.0.0-beta.78",
            "fast-check" => "4.9.0",
            "pure-rand" => "8.4.2",
            "@kayahr/text-encoding" => "2.2.0",
            "whatwg-url" => "17.1.1",
            "@exodus/bytes" => "1.15.1",
            "tr46" => "6.0.0",
            "punycode" => "2.3.1",
            "webidl-conversions" => "8.0.1",
            _ => panic!("unselected native fixture package: {name}"),
        };
        assert_eq!(metadata["version"], version, "native fixture pin: {name}");
        collect(&path, &path, name, &mut files);
    }
    let sources: Vec<_> = files
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    SourceSnapshot::from_memory(&sources, limits()).unwrap()
}

#[test]
fn native_decoder_sources_and_lazy_json_are_not_rewritten() {
    let graph = native_snapshot("import { TextEncoder, TextDecoder } from '@kayahr/text-encoding/no-encodings'; import load from './node_modules/@exodus/bytes/fallback/multi-byte.encodings.cjs'; export const name = 'native'; export const handlers = {'launch.prepare': function () { const data = new TextEncoder().encode('A😀'); const tables = load(); return { bytes: Array.from(data), text: new TextDecoder().decode(data), same: tables === load(), jis: Array.isArray(tables.jis0208) }; }}", &[(PROBE, "@kayahr/text-encoding"), (PROBE, "@exodus/bytes")]);
    assert_eq!(
        script::eval_plugin_snapshot(&graph).unwrap(),
        r#"{"name":"native"}"#
    );
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"bytes":[65,240,159,152,128],"text":"A😀","same":true,"jis":true}"#
    );
}

#[test]
fn native_browser_mapping_and_cjs_require_of_esm_share_identity() {
    let graph = native_snapshot_with_files("import * as array from '@exodus/bytes/array.js'; import load from './load.cjs'; export const name = 'interop'; export const handlers = {'launch.prepare': function () { return load(array); }}", &[(PROBE, "@exodus/bytes")], &[("load.cjs", "module.exports = array => { const a = require('@exodus/bytes/array.js'); const auto = require('./node_modules/@exodus/bytes/fallback/utf8.auto.js'); const browser = require('./node_modules/@exodus/bytes/fallback/utf8.auto.browser.js'); return { same: a === array, bytes: Array.from(a.typedView(Uint8Array.of(7), 'uint8')), browser: auto === browser && auto.encode === null && auto.decodeFast === null }; };")]);
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"same":true,"bytes":[7],"browser":true}"#
    );
}

#[test]
fn native_fast_check_and_pure_rand_execute_without_platform_polyfills() {
    let graph = native_snapshot("import * as fc from 'fast-check'; export const name = 'fast-check'; export const handlers = {'launch.prepare': function () { return { values: fc.sample(fc.constant(42), { seed: 1, numRuns: 3 }) }; }}", &[(RETROARCH, "fast-check"), (RETROARCH, "pure-rand")]);
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"values":[42,42,42]}"#
    );
}

#[test]
fn native_fast_check_require_branch_uses_its_nested_commonjs_scope() {
    let graph = native_snapshot_with_files("import sample from './sample.cjs'; export const name = 'commonjs-fast-check'; export const handlers = {'launch.prepare': function () { return { values: sample() }; }}", &[(RETROARCH, "fast-check"), (RETROARCH, "pure-rand")], &[("sample.cjs", "const fc = require('fast-check'); module.exports = () => fc.sample(fc.constant(9), { seed: 1, numRuns: 2 });")]);
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"values":[9,9]}"#
    );
}

#[test]
fn exports_preserve_native_condition_order_and_wildcard_specificity() {
    for metadata in [
        r#"{"type":"module","exports":{".":{"default":"./chosen.js","browser":"./missing.js","import":"./missing.js"}}}"#,
        r#"{"type":"module","exports":{".":{"browser":{"types":"./missing.d.ts","default":"./chosen.js"},"default":"./missing.js"}}}"#,
    ] {
        let graph = snapshot(
            "import { value } from 'package'; export const name = value;",
            &[
                ("node_modules/package/package.json", metadata),
                (
                    "node_modules/package/chosen.js",
                    "export const value = 'ordered';",
                ),
            ],
        );
        assert_eq!(
            script::eval_plugin_snapshot(&graph).unwrap(),
            r#"{"name":"ordered"}"#
        );
    }
    for path in [
        "effect/internal/core",
        "effect/unstable/cli/internal/help",
        "effect/Option/index",
    ] {
        let graph = native_snapshot(
            &format!("import '{path}'; export const name = 'blocked';"),
            &[(RETROARCH, "effect")],
        );
        assert!(
            script::eval_plugin_snapshot(&graph)
                .unwrap_err()
                .contains("not exported"),
            "{path}"
        );
    }
}

#[test]
fn malformed_metadata_missing_sources_and_forbidden_requests_fail_before_execution() {
    for (source, files) in [
        ("require('./missing.js')", vec![]),
        ("require('node:fs')", vec![]),
        ("require('../outside.js')", vec![]),
        ("require('./value.js?query')", vec![]),
        ("require('./value.js#fragment')", vec![]),
        ("require('./value.js' + '')", vec![]),
        ("import('./value.js')", vec![]),
        ("import.meta", vec![]),
        ("require('./value.json')", vec![("value.json", "not JSON")]),
        (
            "require('package')",
            vec![("node_modules/package/package.json", "not JSON")],
        ),
        (
            "require('package')",
            vec![(
                "node_modules/package/package.json",
                r#"{"exports":{".":null}}"#,
            )],
        ),
        (
            "require('package')",
            vec![(
                "node_modules/package/package.json",
                r#"{"exports":"../outside.js"}"#,
            )],
        ),
        (
            "require('./async.mjs')",
            vec![("async.mjs", "export const value = await 1;")],
        ),
        (
            "require('./async.mjs')",
            vec![
                ("async.mjs", "import './nested.mjs';"),
                ("nested.mjs", "await 1;"),
            ],
        ),
    ] {
        let source = format!("module.exports = () => {{ {source}; }};");
        let mut files = files;
        files.push(("load.cjs", &source));
        let graph = snapshot("import load from './load.cjs'; throw new Error('EXECUTED'); export const name = 'closed';", &files);
        let error = script::eval_plugin_snapshot(&graph).unwrap_err();
        assert!(
            !error.contains("initialization") && !error.contains("evaluation"),
            "{source}: {error}"
        );
        assert!(script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").is_err());
    }
}

#[test]
fn commonjs_cache_is_cleared_after_throw_but_successful_dependencies_stay_cached() {
    let graph = snapshot("import run from './load.cjs'; export const name = 'cache'; export const handlers = {'launch.prepare': function () { return run(); }}", &[
        ("load.cjs", "module.exports = () => { let errors = 0; for (let i = 0; i < 2; i++) { try { require('./throws.cjs'); } catch { errors++; } } const value = require('./value.cjs'); return { errors, runs: value.runs, replacement: typeof value === 'function' }; };"),
        ("throws.cjs", "require('./value.cjs'); throw new Error('expected');"),
        ("value.cjs", "globalThis.runs = (globalThis.runs || 0) + 1; module.exports = function() {}; module.exports.runs = globalThis.runs;"),
    ]);
    for _ in 0..3 {
        assert_eq!(
            script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
            r#"{"errors":2,"runs":1,"replacement":true}"#
        );
    }
}

#[test]
fn commonjs_canonical_aliases_keep_their_parent_and_do_not_reopen_files() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("real")).unwrap();
    fs::write(root.path().join("plugin.ts"), "import a from './alias.cjs'; import b from './real/value.cjs'; export const name = 'retained'; export const handlers = {'launch.prepare': function () { return { same: a === b, text: a() }; }}").unwrap();
    fs::write(
        root.path().join("real/value.cjs"),
        "module.exports = () => require('./value.json').text;",
    )
    .unwrap();
    fs::write(
        root.path().join("real/value.json"),
        r#"{"text":"original"}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink("real/value.cjs", root.path().join("alias.cjs")).unwrap();
    let graph = SourceSnapshot::from_directory(
        root.path(),
        &[
            "plugin.ts",
            "alias.cjs",
            "real/value.cjs",
            "real/value.json",
        ],
        limits(),
    )
    .unwrap();
    root.close().unwrap();
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"same":true,"text":"original"}"#
    );
}

#[test]
fn mixed_require_cycles_are_rejected_before_quickjs_reentry() {
    for files in [
        vec![("load.cjs", "module.exports = require('./plugin.ts');")],
        vec![
            ("load.cjs", "module.exports = require('./value.mjs');"),
            (
                "value.mjs",
                "import load from './load.cjs'; export { load };",
            ),
        ],
    ] {
        let graph = snapshot(
            "import load from './load.cjs'; export const name = 'cycle';",
            &files,
        );
        assert!(script::eval_plugin_snapshot(&graph)
            .unwrap_err()
            .to_string()
            .contains("cyclic require"));
    }
}

// QuickJS aborts the process when a module that is already evaluating is
// evaluated again, so the evaluation runs in a child process. A host abort
// fails this test with the child's signal instead of hanging the suite.
const REENTRY_CHILD: &str = "KORRI_SCRIPT_REENTRY_CHILD";

#[test]
fn a_guest_callback_cannot_reenter_an_evaluating_es_module() {
    if std::env::var_os(REENTRY_CHILD).is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "a_guest_callback_cannot_reenter_an_evaluating_es_module",
                "--nocapture",
            ])
            .env(REENTRY_CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            output.status.code(),
            Some(0),
            "child {:?}\n{stdout}{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(stdout.contains("REENTRY CHILD OK"), "{stdout}");
        return;
    }
    // The require target has no dependency edge back to the CommonJS module,
    // so static reachability cannot reject this graph.
    let graph = snapshot(
        "import './setup.cjs'; import './value.mjs'; export const name = 'reentry';",
        &[
            (
                "setup.cjs",
                "globalThis.reenter = () => require('./value.mjs');",
            ),
            ("value.mjs", "globalThis.reenter(); export const value = 1;"),
        ],
    );
    for error in [
        script::eval_plugin_snapshot(&graph).unwrap_err(),
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null")
            .unwrap_err()
            .to_string(),
    ] {
        println!("reentry error: {error}");
        assert!(error.starts_with("plugin "), "{error}");
    }
    // A fresh interpreter after the rejected re-entry still evaluates.
    assert_eq!(
        script::eval_plugin_ts("export const name = 'fresh';").unwrap(),
        r#"{"name":"fresh"}"#
    );
    println!("REENTRY CHILD OK");
}

#[test]
fn private_module_records_do_not_call_poisoned_prototype_setters() {
    let graph = snapshot("import load from './load.cjs'; export const name = 'records'; export const handlers = {'launch.prepare': function () { Object.defineProperty(Object.prototype, 'exports', { set() { throw new Error('poisoned'); } }); return load(); }}", &[("load.cjs", "module.exports = () => require('./value.json');"), ("value.json", r#"{"value":7}"#)]);
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"value":7.0}"#
    );
}

#[test]
fn commonjs_failures_queued_work_and_cache_roots_teardown_with_the_shared_limits() {
    for body in [
        "while (true) {}",
        "const a = []; while (true) a.push(new Array(100000).fill(1));",
        "setTimeout(() => { while (true) {} }, 0); return {};",
        "setTimeout(() => { throw new Error('timer'); }, 0); return {};",
        "Promise.reject('rejected'); return {};",
        "return { value: 'x'.repeat(512 * 1024 + 1) };",
        "return require('node:fs');",
    ] {
        let source = format!("module.exports = () => {{ {body} }};");
        let graph = snapshot("import run from './load.cjs'; export const name = 'bounded'; export const handlers = {'launch.prepare': function () { return run(); }}", &[("load.cjs", &source)]);
        for _ in 0..2 {
            assert!(
                script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").is_err(),
                "{body}"
            );
        }
        assert_eq!(
            script::eval_plugin_ts("export const name = 'fresh';").unwrap(),
            r#"{"name":"fresh"}"#
        );
    }
}

#[test]
fn caught_esm_initialization_failure_still_fails_shared_completion() {
    let graph = snapshot("import run from './load.cjs'; export const name = 'caught'; export const handlers = {'launch.prepare': function () { return run(); }}", &[
        ("load.cjs", "module.exports = () => { let errors = 0; for (let i = 0; i < 2; i++) { try { require('./fails.mjs'); } catch { errors++; } } return { errors, runs: globalThis.runs }; };"),
        ("fails.mjs", "globalThis.runs = (globalThis.runs || 0) + 1; throw new Error('expected');"),
    ]);
    // This is an engine limitation, not successful validation of the error.
    // QuickJS rejects an internal promise as well as the returned eval promise.
    assert!(script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null")
        .unwrap_err()
        .to_string()
        .contains("unhandled promise rejection"));
}

#[test]
fn commonjs_cannot_create_a_global_require_or_reopen_dynamic_imports() {
    let graph = snapshot("import run from './load.cjs'; export const name = 'closed'; export const config = { blocked: await run(), require: typeof globalThis.require };", &[("load.cjs", "module.exports = () => { require('./value.cjs'); return Function('return import(\"./value.cjs\")')().then(() => false, () => true); };"), ("value.cjs", "module.exports = {};")]);
    assert_eq!(
        script::eval_plugin_snapshot(&graph).unwrap(),
        r#"{"config":{"blocked":true,"require":"undefined"},"name":"closed"}"#
    );
    let graph = snapshot("import run from './load.cjs'; export const name = 'closed'; export const handlers = {'launch.prepare': function () { return run(); }}", &[("load.cjs", "module.exports = () => { const require = x => x + 1; return { value: require(4) }; };")]);
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"value":5}"#
    );
}

const RETROARCH: &str = "plugins/retroarch";
const PROBE: &str = "docs/research/retroarch-effect-quickjs-probe";

#[test]
fn real_effect_option_subpath_runs_in_both_consumers() {
    let graph = native_snapshot("import * as Option from 'effect/Option'; export const name = Option.getOrElse(Option.some('effect'), () => 'absent'); export const handlers = {'launch.prepare': function () { return { value: Option.getOrElse(Option.map(Option.some(20), n => n + 2), () => 0) }; }}", &[(RETROARCH, "effect")]);
    assert_eq!(
        script::eval_plugin_snapshot(&graph).unwrap(),
        r#"{"name":"effect"}"#
    );
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"value":22}"#
    );
}

#[test]
fn real_pure_rand_uses_distinct_native_import_and_require_branches() {
    let consumer = "module.exports = { generateN: require('pure-rand/utils/generateN').generateN, values: require('pure-rand/utils/generateN').generateN({ next: () => 3 }, 2) };";
    let graph = native_snapshot_with_files("import { generateN } from 'pure-rand/utils/generateN'; import cjs from './consumer.cjs'; export const name = 'rand'; export const handlers = {'launch.prepare': function () { return { values: generateN({ next: () => 7 }, 2), cjs: cjs.values, distinct: cjs.generateN !== generateN }; }}", &[(RETROARCH, "pure-rand")], &[("consumer.cjs", consumer)]);
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"values":[7,7],"cjs":[3,3],"distinct":true}"#
    );
}

#[test]
fn commonjs_is_lazy_and_preserves_cycles_replacement_and_json_identity() {
    let graph = snapshot("import load from './load.cjs'; export const name = 'closed'; export const config = { lazy: globalThis.started === undefined, require: typeof globalThis.require }; export const handlers = {'launch.prepare': function () { return load(); }}", &[
        ("load.cjs", "module.exports = () => { const a = require('./a'); const again = require('./a.js'); const json = require('./value.json'); return { same: a === again, cycle: a.b.a === a, value: a.value, json: json === require('./value.json') }; };"),
        ("a.js", "globalThis.started = true; exports.value = 4; exports.b = require('./b');"),
        ("b.js", "exports.a = require('./a');"),
        ("value.json", "{\"value\":9}"),
        ("package.json", "{\"type\":\"commonjs\"}"),
    ]);
    assert_eq!(
        script::eval_plugin_snapshot(&graph).unwrap(),
        r#"{"config":{"lazy":true,"require":"undefined"},"name":"closed"}"#
    );
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"same":true,"cycle":true,"value":4,"json":true}"#
    );
}

#[test]
fn require_of_esm_is_lazy_and_shares_native_namespace_identity() {
    let graph = snapshot("import load from './load.cjs'; export const name = 'closed'; export const config = { lazy: globalThis.started === undefined }; export const handlers = {'launch.prepare': function () { return load(); }}", &[
        ("load.cjs", "module.exports = () => { const ns = require('./value.mjs'); return { same: ns === require('./value.mjs'), value: ns.value }; };"),
        ("value.mjs", "globalThis.started = true; export const value = 42;"),
    ]);
    assert_eq!(
        script::eval_plugin_snapshot(&graph).unwrap(),
        r#"{"config":{"lazy":true},"name":"closed"}"#
    );
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"same":true,"value":42}"#
    );
}

#[test]
fn full_schema_and_url_graphs_prepare_without_running_missing_platform_apis() {
    for (name, packages) in [
        (
            "effect/Schema",
            vec![
                (RETROARCH, "effect"),
                (RETROARCH, "fast-check"),
                (RETROARCH, "pure-rand"),
            ],
        ),
        (
            "whatwg-url",
            vec![
                (PROBE, "whatwg-url"),
                (PROBE, "@exodus/bytes"),
                (PROBE, "tr46"),
                (PROBE, "punycode"),
                (PROBE, "webidl-conversions"),
            ],
        ),
    ] {
        let loader = format!("module.exports = () => require('{name}');");
        let graph = native_snapshot_with_files("import load from './load.cjs'; export const name = 'prepared'; export const handlers = {'launch.prepare': function () { return { deferred: typeof load === 'function' }; }}", &packages, &[("load.cjs", &loader)]);
        assert_eq!(
            script::eval_plugin_snapshot(&graph).unwrap_or_else(|error| panic!("{name}: {error}")),
            r#"{"name":"prepared"}"#
        );
        assert_eq!(
            script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null")
                .unwrap_or_else(|error| panic!("{name}: {error}")),
            r#"{"deferred":true}"#
        );
    }
}

#[test]
fn combined_native_graph_fits_the_unchanged_vm_guards_without_initializing_schema_or_url() {
    let graph = native_snapshot_with_files(
        "import load from './load.cjs'; import { TextEncoder } from '@kayahr/text-encoding/no-encodings'; export const name = 'combined'; export const handlers = {'launch.prepare': function () { return { deferred: typeof load === 'function', encoded: Array.from(new TextEncoder().encode('ok')) }; }}",
        &[(RETROARCH, "effect"), (RETROARCH, "fast-check"), (RETROARCH, "pure-rand"), (PROBE, "whatwg-url"), (PROBE, "@exodus/bytes"), (PROBE, "tr46"), (PROBE, "punycode"), (PROBE, "webidl-conversions"), (PROBE, "@kayahr/text-encoding")],
        &[("load.cjs", "module.exports = () => ({ schema: require('effect/Schema'), url: require('whatwg-url') });")],
    );
    assert_eq!(
        script::eval_plugin_snapshot(&graph).unwrap(),
        r#"{"name":"combined"}"#
    );
    assert_eq!(
        script::call_plugin_operation_snapshot(&graph, "launch.prepare", "null").unwrap(),
        r#"{"deferred":true,"encoded":[111,107]}"#
    );
}

#[test]
fn real_package_exports_block_unexported_decoder_paths() {
    let graph = native_snapshot("import load from '@exodus/bytes/fallback/multi-byte.encodings.cjs'; export const name = 'tables'; export const handlers = {'launch.prepare': function () { const a = load(); return { same: a === load(), jis: Array.isArray(a.jis0208) }; }}", &[(PROBE, "@exodus/bytes")]);
    // This internal path is not exported by the package.
    assert!(script::eval_plugin_snapshot(&graph)
        .unwrap_err()
        .to_string()
        .contains("export"));
}
