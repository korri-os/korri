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
use rquickjs::{function::This, Context, Filter, Function, Module, Object, Runtime, Type, Value};

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

    let mut options = TransformOptions::default();
    // Value imports must reach the loader-less interpreter even if unused.
    options.typescript.only_remove_type_imports = true;
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
    evaluate_module(source, None)
}

/// Call the module's synchronous launch export with JSON input in a fresh,
/// empty interpreter. Evaluation and invocation share the same resource budget.
pub fn call_plugin_launch_ts(source: &str, input_json: &str) -> Result<String, String> {
    if input_json.len() > 512 * 1024 {
        return Err("plugin launch input exceeds 512 KiB".into());
    }
    evaluate_module(&transpile_ts(source)?, Some(input_json))
}

fn evaluate_module(source: &str, input: Option<&str>) -> Result<String, String> {
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
        let module = Module::declare(ctx.clone(), "plugin", source)
            .map_err(|error| format!("plugin evaluation failed: {error}"))?;
        let (module, evaluated) = module
            .eval()
            .map_err(|error| format!("plugin evaluation failed: {error}"))?;
        evaluated
            .finish::<()>()
            .map_err(|error| format!("plugin evaluation failed: {error}"))?;
        let exports = module.namespace().map_err(|error| error.to_string())?;
        let data = Object::new(ctx.clone()).map_err(|error| error.to_string())?;
        for property in exports.props::<String, Value>() {
            let (name, value) = property.map_err(|error| error.to_string())?;
            match name.as_str() {
                "name" | "title" | "description" | "providers" | "systems" | "launchers"
                | "transports" | "runtimes" | "sessionControls" | "discovery" | "android"
                | "services" | "config" => {
                    data.set(name, value).map_err(|error| error.to_string())?;
                }
                "launch" if value.is_function() => {}
                "launch" => return Err("plugin export launch must be a function".into()),
                _ => return Err(format!("unsupported plugin export: {name}")),
            }
        }
        let name: Value = data.get("name").map_err(|error| error.to_string())?;
        if !name.is_string()
            || name
                .get::<String>()
                .map_err(|error| error.to_string())?
                .is_empty()
        {
            return Err("plugin must export a non-empty name string".into());
        }
        let mut budget = OutputBudget {
            nodes: 8192,
            string_bytes: 512 * 1024,
        };
        let declaration = json_data_from_js(
            data.as_value(),
            "$",
            0,
            &plain_object_prototype,
            &object_to_string,
            &mut budget,
        )?;
        let result = if let Some(input) = input {
            let launch: Function = exports
                .get("launch")
                .map_err(|_| "plugin has no callable launch export".to_owned())?;
            let argument = ctx.json_parse(input).map_err(|error| error.to_string())?;
            let value: Value = launch
                .call((argument,))
                .map_err(|error| format!("plugin launch failed: {error}"))?;
            json_data_from_js(
                &value,
                "$.launch",
                0,
                &plain_object_prototype,
                &object_to_string,
                &mut budget,
            )?
        } else {
            declaration
        };
        serde_json::to_string(&result)
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

    #[test]
    fn named_module_exports_are_the_only_declaration_source() {
        let json = eval_plugin_ts("export const name: string = 'clock'; export const transports = {}; export const sessionControls = {};").expect("named module");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap()["name"],
            "clock"
        );
        for source in [
            "({name: 'clock'})",
            "export default {name: 'clock'}",
            "export const namespace = '@impostor'; export const name = 'clock';",
            "export const name = 'clock'; export const contributes = {};",
            "export const name = 'clock'; export const unknown = {};",
            "export const name = 'clock'; export const launch = {};",
            "export const name = 'clock'; export const discovery = () => ({});",
            "export const name = 'clock'; export const systems = {x: undefined};",
        ] {
            assert!(eval_plugin_ts(source).is_err(), "accepted {source}");
        }
    }

    #[test]
    fn typescript_example_runs_and_types_are_erased() {
        let source = include_str!("../examples/catalog.plugin.ts");
        let json = eval_plugin_ts(source).expect("example plugin should run");
        assert!(json.contains("\"name\":\"catalog\""), "got: {json}");
        assert!(json.contains("\"id\":\"gba\""), "got: {json}");
        let javascript = transpile_ts(source).unwrap();
        assert!(!javascript.contains("interface"));
        assert!(!javascript.contains(": string"));
    }

    #[test]
    fn shared_strings_cannot_expand_into_unbounded_rust_output() {
        let error = eval_plugin("export const name = 'clock'; const value = 'x'.repeat(10000); export const systems = Array(1000).fill(value)").unwrap_err();
        assert!(error.contains("string budget"), "{error}");
    }

    #[test]
    fn plugin_computes_rather_than_merely_parses() {
        let json = eval_plugin_ts("const items: number[] = [1, 2, 3]; export const name = 'clock'; export const systems = {total: items.reduce((sum, n) => sum + n, 0)}").unwrap();
        assert_eq!(json, "{\"name\":\"clock\",\"systems\":{\"total\":6}}");
    }

    #[test]
    fn syntax_and_missing_names_are_errors_not_panics() {
        assert!(eval_plugin("this is not javascript {{{")
            .unwrap_err()
            .contains("plugin evaluation failed"));
        assert!(eval_plugin_ts("const x: = 3")
            .unwrap_err()
            .contains("failed to parse"));
        assert!(eval_plugin_ts("const unused = 1")
            .unwrap_err()
            .contains("name"));
    }

    #[test]
    fn sandbox_exposes_no_host_capabilities_or_imports() {
        for probe in [
            "typeof require",
            "typeof process",
            "typeof fetch",
            "typeof globalThis.XMLHttpRequest",
        ] {
            let json = eval_plugin(&format!(
                "export const name = 'clock'; export const description = {probe}"
            ))
            .unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&json).unwrap()["description"],
                "undefined"
            );
        }
        for source in [
            "import x from 'fs'; export const name = 'clock';",
            "export const name = 'clock'; await import('fs');",
            "export const name = 'clock'; await new Promise(() => {});",
        ] {
            assert!(eval_plugin_ts(source).is_err(), "accepted {source}");
        }
    }

    #[test]
    fn launch_is_deferred_callable_and_does_not_keep_state_between_calls() {
        let source = "export const name = 'clock'; let calls = 0; export function launch(input) { calls++; return {args: [input.path], calls}; }";
        assert_eq!(eval_plugin_ts(source).unwrap(), "{\"name\":\"clock\"}");
        let input = r#"{"path":"/games/a.gba"}"#;
        let first = call_plugin_launch_ts(source, input).unwrap();
        assert_eq!(first, "{\"args\":[\"/games/a.gba\"],\"calls\":1}");
        assert_eq!(call_plugin_launch_ts(source, input).unwrap(), first);
        assert!(call_plugin_launch_ts("export const name = 'clock'", input).is_err());
        assert!(call_plugin_launch_ts(source, "invalid JSON").is_err());
        assert!(call_plugin_launch_ts(source, &" ".repeat(512 * 1024 + 1)).is_err());
    }

    #[test]
    fn both_module_and_callback_share_bounded_execution_and_json_output() {
        for body in [
            "while (true) {}",
            "const a=[]; while(true) a.push(new Array(100000).fill(1))",
            "throw new Error('bad launch')",
            "return undefined",
            "return Promise.resolve({})",
            "return {nested: () => {}}",
            "return Array(10000).fill(0)",
            "return Array(1000).fill('x'.repeat(10000))",
        ] {
            let source =
                format!("export const name = 'clock'; export function launch(input) {{ {body} }}");
            assert!(eval_plugin_ts(&source).is_ok());
            assert!(
                call_plugin_launch_ts(&source, "null").is_err(),
                "accepted {body}"
            );
        }
        assert!(eval_plugin_ts("export const name = 'clock'; while (true) {}").is_err());
        assert!(eval_plugin_ts(&" ".repeat(128 * 1024 + 1)).is_err());
    }
}
