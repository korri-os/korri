//! Running TypeScript and JavaScript plugins at runtime.
//!
//! A plugin is source text that evaluates to a **declaration**: what content
//! exists, what it needs, how it could be fulfilled. korrid evaluates the
//! plugin and then performs any effects itself. The plugin never touches a
//! filesystem, a network, or a process — the sandbox below exposes nothing, so
//! that rule is enforced by construction rather than by trust.
//!
//! This is what lets a plugin be federation-portable: source travels between
//! devices and runs identically on each, where a compiled artifact could not.
//!
//! Nothing is compiled ahead of time. TypeScript arrives as text and is
//! transpiled in-process at load, so adding or editing a plugin never requires
//! rebuilding korrid or the app that embeds it.

use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};

use oxc::allocator::Allocator;
use oxc::codegen::Codegen;
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::SourceType;
use oxc::transformer::{TransformOptions, Transformer};
use rquickjs::{function::This, Context, Filter, Function, Object, Runtime, Type, Value};

/// Transpile TypeScript to JavaScript, in-process, at load time.
pub fn transpile_ts(source: &str) -> Result<String, String> {
    if source.len() > 128 * 1024 {
        return Err("plugin source exceeds 128 KiB".into());
    }
    let allocator = Allocator::default();
    let source_type = SourceType::ts();

    let parsed = Parser::new(&allocator, source, source_type).parse();
    if let Some(first) = parsed.diagnostics.first() {
        return Err(format!("plugin failed to parse: {first}"));
    }

    let mut program = parsed.program;
    // `with_enum_eval` is required for TS `enum` lowering; without it the
    // transformer panics instead of returning an error.
    let scoping = SemanticBuilder::new()
        .with_enum_eval(true)
        .build(&program)
        .semantic
        .into_scoping();

    let options = TransformOptions::default();
    let transformed = Transformer::new(&allocator, Path::new("plugin.ts"), &options)
        .build_with_scoping(scoping, &mut program);
    if let Some(first) = transformed.diagnostics.first() {
        return Err(format!("plugin failed to transpile: {first}"));
    }

    Ok(Codegen::new().build(&program).code)
}

/// Evaluate plugin JavaScript and return its declaration as JSON text.
///
/// The sandbox is empty: no module loader, no host bindings, no I/O. A plugin
/// that tries to reach the outside world finds nothing there.
pub fn eval_plugin(source: &str) -> Result<String, String> {
    if source.len() > 512 * 1024 {
        return Err("plugin JavaScript exceeds 512 KiB".into());
    }
    let runtime = Runtime::new().map_err(|error| error.to_string())?;
    // External declarations run before permission approval. Resource limits
    // therefore belong to the empty interpreter, not to the installed payload.
    runtime.set_memory_limit(16 * 1024 * 1024);
    runtime.set_max_stack_size(512 * 1024);
    let deadline = Instant::now() + Duration::from_millis(250);
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    let context = Context::full(&runtime).map_err(|error| error.to_string())?;

    context.with(|ctx| {
        let object_constructor: Object = ctx
            .globals()
            .get("Object")
            .map_err(|error| format!("plugin sandbox not inspectable: {error}"))?;
        let plain_object_prototype: Object = object_constructor
            .get("prototype")
            .map_err(|error| format!("plugin sandbox not inspectable: {error}"))?;
        let object_to_string: Function = plain_object_prototype
            .get("toString")
            .map_err(|error| format!("plugin sandbox not inspectable: {error}"))?;
        let value: rquickjs::Value = ctx
            .eval(source)
            .map_err(|error| format!("plugin evaluation failed: {error}"))?;

        let mut budget = OutputBudget {
            nodes: 8192,
            string_bytes: 512 * 1024,
        };
        let declaration = json_data_from_js(
            &value,
            "$",
            0,
            &plain_object_prototype,
            &object_to_string,
            &mut budget,
        )?;
        serde_json::to_string(&declaration)
            .map_err(|error| format!("plugin result not serialisable: {error}"))
    })
}

// QuickJS accounts for shared strings once. Rust's JSON tree copies them, so
// its output needs a separate budget to prevent amplification outside the VM.
struct OutputBudget {
    nodes: usize,
    string_bytes: usize,
}

impl OutputBudget {
    fn string(&mut self, value: &str) -> Result<(), String> {
        self.string_bytes = self
            .string_bytes
            .checked_sub(value.len())
            .ok_or_else(|| "plugin result exceeds its 512 KiB string budget".to_owned())?;
        Ok(())
    }
}

fn json_data_from_js<'js>(
    value: &Value<'js>,
    path: &str,
    depth: usize,
    plain_object_prototype: &Object<'js>,
    object_to_string: &Function<'js>,
    budget: &mut OutputBudget,
) -> Result<serde_json::Value, String> {
    budget.nodes = budget
        .nodes
        .checked_sub(1)
        .ok_or_else(|| "plugin result exceeds its 8192 node budget".to_owned())?;
    if depth > 64 {
        return Err(format!(
            "plugin result is not JSON data at {path}: nesting exceeds 64 levels"
        ));
    }

    match value.type_of() {
        Type::Null => Ok(serde_json::Value::Null),
        Type::Bool => value
            .get::<bool>()
            .map(serde_json::Value::Bool)
            .map_err(|error| format!("plugin result not inspectable at {path}: {error}")),
        Type::Int => value
            .get::<i32>()
            .map(serde_json::Number::from)
            .map(serde_json::Value::Number)
            .map_err(|error| format!("plugin result not inspectable at {path}: {error}")),
        Type::Float => {
            let number: f64 = value
                .get()
                .map_err(|error| format!("plugin result not inspectable at {path}: {error}"))?;
            let number = serde_json::Number::from_f64(number).ok_or_else(|| {
                format!("plugin result is not JSON data at {path}: non-finite number")
            })?;
            Ok(serde_json::Value::Number(number))
        }
        Type::String => {
            let string = value
                .get::<String>()
                .map_err(|error| format!("plugin result not inspectable at {path}: {error}"))?;
            budget.string(&string)?;
            Ok(serde_json::Value::String(string))
        }
        Type::Array => {
            let array = value
                .clone()
                .into_array()
                .expect("value type was checked as an array");
            if array.len() > budget.nodes {
                return Err("plugin result exceeds its 8192 node budget".into());
            }
            reject_symbol_properties(array.as_object(), path)?;
            let keys: BTreeSet<String> = array
                .as_object()
                .keys::<String>()
                .take(8193)
                .collect::<rquickjs::Result<_>>()
                .map_err(|error| format!("plugin result not inspectable at {path}: {error}"))?;
            let expected_keys: BTreeSet<String> =
                (0..array.len()).map(|index| index.to_string()).collect();
            if keys != expected_keys {
                return Err(format!(
                    "plugin result is not JSON data at {path}: arrays must be dense and have no named properties"
                ));
            }
            let mut items = Vec::with_capacity(array.len());
            for index in 0..array.len() {
                let item: Value = array.get(index).map_err(|error| {
                    format!("plugin result not inspectable at {path}[{index}]: {error}")
                })?;
                items.push(json_data_from_js(
                    &item,
                    &format!("{path}[{index}]"),
                    depth + 1,
                    plain_object_prototype,
                    object_to_string,
                    budget,
                )?);
            }
            Ok(serde_json::Value::Array(items))
        }
        Type::Object => {
            let object = value
                .clone()
                .into_object()
                .expect("value type was checked as an object");
            let object_tag: String = object_to_string
                .call((This(value.clone()),))
                .map_err(|error| format!("plugin result not inspectable at {path}: {error}"))?;
            if object_tag != "[object Object]" {
                return Err(format!(
                    "plugin result is not JSON data at {path}: {object_tag}"
                ));
            }
            if let Some(prototype) = object.get_prototype() {
                if &prototype != plain_object_prototype {
                    return Err(format!(
                        "plugin result is not JSON data at {path}: non-plain object"
                    ));
                }
            }
            reject_symbol_properties(&object, path)?;
            let mut properties = serde_json::Map::new();
            for property in object.props::<String, Value>() {
                let (key, property_value) = property
                    .map_err(|error| format!("plugin result not inspectable at {path}: {error}"))?;
                budget.string(&key)?;
                let property_path = format!("{path}.{key}");
                properties.insert(
                    key,
                    json_data_from_js(
                        &property_value,
                        &property_path,
                        depth + 1,
                        plain_object_prototype,
                        object_to_string,
                        budget,
                    )?,
                );
            }
            Ok(serde_json::Value::Object(properties))
        }
        unsupported => Err(format!(
            "plugin result is not JSON data at {path}: found {unsupported}"
        )),
    }
}

fn reject_symbol_properties(object: &rquickjs::Object<'_>, path: &str) -> Result<(), String> {
    let symbol = object
        .own_keys::<rquickjs::Atom>(Filter::new().symbol().enum_only())
        .next()
        .transpose()
        .map_err(|error| format!("plugin result not inspectable at {path}: {error}"))?;
    if symbol.is_some() {
        Err(format!(
            "plugin result is not JSON data at {path}: symbol property"
        ))
    } else {
        Ok(())
    }
}

/// Load a TypeScript plugin end to end: transpile, then evaluate.
pub fn eval_plugin_ts(source: &str) -> Result<String, String> {
    let javascript = transpile_ts(source)?;
    eval_plugin(&javascript)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS_PLUGIN: &str = r#"
        interface Game {
          id: string
          title: string
          system: System
        }

        enum System {
          Gba = "gba",
        }

        function declare(): { kind: string; games: Game[] } {
          const games: Game[] = [
            { id: "wl4", title: "Wario Land 4", system: System.Gba },
          ]
          return { kind: "catalog", games }
        }

        declare()
    "#;

    #[test]
    fn typescript_plugin_runs_and_returns_its_declaration() {
        let json = eval_plugin_ts(TS_PLUGIN).expect("TS plugin should run");

        assert!(json.contains("\"kind\":\"catalog\""), "got: {json}");
        assert!(json.contains("Wario Land 4"), "got: {json}");
        assert!(json.contains("\"system\":\"gba\""), "got: {json}");
    }

    #[test]
    fn transpile_erases_types_and_lowers_typescript_only_syntax() {
        let javascript = transpile_ts(TS_PLUGIN).expect("should transpile");

        assert!(!javascript.contains("interface"), "got: {javascript}");
        assert!(!javascript.contains(": string"), "got: {javascript}");
    }

    #[test]
    fn the_shipped_example_plugin_still_runs() {
        // Guards the example against silent rot: it is documentation that has
        // to keep working.
        let source = include_str!("../examples/catalog.plugin.ts");
        let json = eval_plugin_ts(source).expect("example plugin should run");

        assert!(json.contains("\"kind\":\"catalog\""), "got: {json}");
        assert!(json.contains("\"routes\""), "got: {json}");
    }

    #[test]
    fn shared_strings_cannot_expand_into_unbounded_rust_output() {
        let result =
            eval_plugin("const value = 'x'.repeat(10000); ({values: Array(1000).fill(value)})");
        assert!(
            result.is_err(),
            "output expansion must be bounded outside QuickJS too"
        );
    }

    #[test]
    fn plugin_computes_rather_than_merely_parses() {
        let json = eval_plugin_ts(
            r#"
            const items: number[] = [1, 2, 3]
            ;({ total: items.reduce((sum, n) => sum + n, 0) })
        "#,
        )
        .expect("should run");

        assert_eq!(json, "{\"total\":6}");
    }

    #[test]
    fn plugin_syntax_error_is_reported_not_panicked() {
        let error = eval_plugin("this is not javascript {{{").expect_err("should fail");
        assert!(error.contains("plugin evaluation failed"), "got: {error}");
    }

    #[test]
    fn typescript_syntax_error_is_reported_at_transpile_time() {
        let error = eval_plugin_ts("const x: = 3").expect_err("should fail");
        assert!(error.contains("failed to parse"), "got: {error}");
    }

    #[test]
    fn plugin_returning_nothing_is_an_error_not_a_silent_empty() {
        let error = eval_plugin_ts("const unused = 1").expect_err("should fail");
        assert!(error.contains("undefined"), "got: {error}");
    }

    #[test]
    fn sandbox_exposes_no_host_capabilities() {
        // The declaration-only rule is enforced by the empty sandbox, not by
        // trust: a plugin has no way to reach the filesystem, network, or
        // process even if it tries.
        for probe in [
            "typeof require",
            "typeof process",
            "typeof fetch",
            "typeof globalThis.XMLHttpRequest",
        ] {
            let json = eval_plugin(probe).expect("probe should evaluate");
            assert_eq!(json, "\"undefined\"", "probe {probe} leaked a capability");
        }
    }
}
