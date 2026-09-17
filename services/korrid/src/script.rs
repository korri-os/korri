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

#[path = "script/commonjs.rs"]
mod commonjs;
#[path = "script/completion.rs"]
mod completion;
#[path = "script/packages.rs"]
mod packages;
#[path = "script/preparation.rs"]
mod preparation;
#[path = "script/source.rs"]
pub mod source;

use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

use rquickjs::{function::This, Context, Filter, Function, Object, Runtime, Type, Value};

/// The operation that turns a selected runner and target into a launch plan.
/// A runner that korrid must start needs this handler and nothing else.
pub const LAUNCH_PREPARE: &str = "launch.prepare";

/// The runner's own settings schema, valid for the build that answered.
pub const SETTINGS_DESCRIBE: &str = "settings.describe";

/// What this build cannot apply from the authored values.
pub const SETTINGS_VALIDATE: &str = "settings.validate";

/// Whether the runner can start this target now, and what is missing if not.
/// The runner owns its runtime, so only the runner can answer this.
pub const RUNTIME_RESOLVE: &str = "runtime.resolve";

/// Why one operation call produced no result.
///
/// A caller that treats an operation as optional checks `Unimplemented`
/// instead of matching on message text, so an unimplemented operation can
/// never be mistaken for a successful empty answer.
#[derive(Debug)]
pub enum OperationFailure {
    Unimplemented { operation: String },
    Failed(String),
}

impl std::fmt::Display for OperationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unimplemented { operation } => {
                write!(formatter, "plugin does not implement {operation}")
            }
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl OperationFailure {
    pub fn is_unimplemented(&self) -> bool {
        matches!(self, Self::Unimplemented { .. })
    }
}

/// Transpile TypeScript to JavaScript, in-process, at load time.
pub fn transpile_ts(source: &str) -> Result<String, String> {
    preparation::transpile(source, "plugin.ts")
}

/// Evaluate plugin JavaScript and return its declaration as JSON text.
///
/// The loader reads only prepared snapshot bytes: no host bindings or I/O.
pub fn eval_plugin(source: &str) -> Result<String, String> {
    let snapshot = source::SourceSnapshot::javascript(source)?;
    evaluate_module(
        preparation::PreparedGraph::new(&snapshot, "plugin.js")?,
        None,
    )
}

/// One operation call: an operation name from the plugin operation contract
/// and its JSON request. The host names the operation; a plugin never picks
/// which of its handlers runs.
#[derive(Clone, Copy)]
pub struct Invocation<'a> {
    pub operation: &'a str,
    pub input: &'a str,
}

/// Call one operation handler with JSON input in a fresh, empty interpreter.
/// Evaluation and invocation share the same resource budget.
pub fn call_plugin_operation_ts(
    source: &str,
    operation: &str,
    input_json: &str,
) -> Result<String, OperationFailure> {
    let snapshot = source::SourceSnapshot::plugin(source).map_err(OperationFailure::Failed)?;
    call_plugin_operation_snapshot(&snapshot, operation, input_json)
}

/// Prepare the retained graph, then invoke one handler in a fresh interpreter.
pub fn call_plugin_operation_snapshot(
    snapshot: &source::SourceSnapshot,
    operation: &str,
    input_json: &str,
) -> Result<String, OperationFailure> {
    if input_json.len() > 512 * 1024 {
        return Err(OperationFailure::Failed(format!(
            "plugin {operation} input exceeds 512 KiB"
        )));
    }
    let graph =
        preparation::PreparedGraph::new(snapshot, "plugin.ts").map_err(OperationFailure::Failed)?;
    evaluate_module(
        graph,
        Some(Invocation {
            operation,
            input: input_json,
        }),
    )
    .map_err(|error| {
        if error == unimplemented_message(operation) {
            OperationFailure::Unimplemented {
                operation: operation.to_owned(),
            }
        } else {
            OperationFailure::Failed(error)
        }
    })
}

fn evaluate_module(
    graph: preparation::PreparedGraph,
    invocation: Option<Invocation<'_>>,
) -> Result<String, String> {
    let runtime = Runtime::new().map_err(|error| error.to_string())?;
    // External declarations run before permission approval. Resource limits
    // therefore belong to the empty interpreter, not to the installed payload.
    runtime.set_memory_limit(16 * 1024 * 1024);
    runtime.set_max_stack_size(512 * 1024);
    let installed = graph.install(&runtime);
    let rejections = completion::Rejections::install(&runtime);
    // Preparation has finished. One deadline includes context initialization,
    // extraction, queued jobs and timers. It never resets between phases.
    let started = Instant::now();
    let deadline = started + Duration::from_millis(250);
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    let context = Context::full(&runtime).map_err(|error| error.to_string())?;

    context.with(|ctx| {
        // Own all module roots inside this scope. Loader/native callbacks retain
        // only Weak references and cannot keep the interpreter alive on errors.
        let mut installed = installed;
        let completion = completion::Completion::install(&ctx, started, deadline)?;
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
        let (module, evaluated) = installed.evaluate(&ctx)?;
        completion.initialize(&ctx, &evaluated)?;
        let exports = module.namespace().map_err(|error| error.to_string())?;
        let data = Object::new(ctx.clone()).map_err(|error| error.to_string())?;
        for property in exports.props::<String, Value>() {
            let (name, value) = property.map_err(|error| error.to_string())?;
            match name.as_str() {
                "name" | "title" | "description" | "providers" | "systems" | "families"
                | "runners" | "transports" | "sessionControls" | "discovery" | "android"
                | "services" | "config" => {
                    data.set(name, value).map_err(|error| error.to_string())?;
                }
                // Handlers are callable operation code, never declaration data.
                "handlers" => validate_handlers(&value)?,
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
            deadline,
        };
        let declaration = json_data_from_js(
            data.as_value(),
            "$",
            0,
            &plain_object_prototype,
            &object_to_string,
            &mut budget,
        )?;
        // Capture declaration bytes before queued mutation, then complete all
        // initialization work before inspecting the callable or invoking it.
        completion.drain(&ctx, &rejections)?;
        // A native kind owns the module callback. Inspect the export only;
        // admission must never invoke it or impose policy on its output.
        if declaration
            .get("runners")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|runners| {
                runners.values().any(|runner| {
                    runner
                        .get("program")
                        .and_then(serde_json::Value::as_str)
                        .is_some()
                })
            })
        {
            handler(&exports, LAUNCH_PREPARE).map_err(|_| {
                "native runner kind has no callable launch.prepare handler".to_owned()
            })?;
        }
        let result = if let Some(invocation) = invocation {
            let operation = invocation.operation;
            let handler = handler(&exports, operation)?;
            let argument = ctx
                .json_parse(invocation.input)
                .map_err(|error| error.to_string())?;
            let value: Value = handler
                .call((argument,))
                .map_err(|error| format!("plugin {operation} failed: {error}"))?;
            json_data_from_js(
                &value,
                "$.result",
                0,
                &plain_object_prototype,
                &object_to_string,
                &mut budget,
            )?
        } else {
            declaration
        };
        completion.drain(&ctx, &rejections)?;
        let output = serde_json::to_string(&result)
            .map_err(|error| format!("plugin result not serialisable: {error}"))?;
        completion.check_deadline()?;
        Ok(output)
    })
}

/// Every entry of the `handlers` export must be callable. Admission inspects
/// the shape only: it never invokes a handler and never judges its name, so a
/// plugin written for a newer host still loads and fails only when the host
/// asks for an operation this plugin does not implement.
fn validate_handlers(value: &Value<'_>) -> Result<(), String> {
    // A function or an array is an object to the interpreter. The handlers
    // export is a map of operation name to handler, and nothing else.
    let handlers = match value.as_object() {
        Some(handlers) if !value.is_function() && !handlers.is_array() => handlers,
        _ => return Err("plugin export handlers must be an object".to_owned()),
    };
    for property in handlers.props::<String, Value>() {
        let (name, handler) = property.map_err(|error| error.to_string())?;
        if !handler.is_function() {
            return Err(format!("plugin handler {name} must be a function"));
        }
    }
    Ok(())
}

fn unimplemented_message(operation: &str) -> String {
    format!("plugin has no handler for {operation}")
}

/// Resolve one operation handler. An operation the plugin does not implement
/// is an explicit failure, never a successful empty result.
fn handler<'js>(exports: &Object<'js>, operation: &str) -> Result<Function<'js>, String> {
    exports
        .get::<_, Object>("handlers")
        .map_err(|_| unimplemented_message(operation))?
        .get::<_, Function>(operation)
        .map_err(|_| unimplemented_message(operation))
}

// QuickJS accounts for shared strings once. Rust's JSON tree copies them, so
// its output needs a separate budget to prevent amplification outside the VM.
struct OutputBudget {
    nodes: usize,
    string_bytes: usize,
    deadline: Instant,
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
    if Instant::now() >= budget.deadline {
        return Err("plugin execution deadline exceeded".into());
    }
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
    eval_plugin_snapshot(&source::SourceSnapshot::plugin(source)?)
}

/// Eagerly prepare the closed source graph from retained bytes only.
/// Preparation has no hard host-memory or time guarantee; VM limits start after it.
pub fn eval_plugin_snapshot(snapshot: &source::SourceSnapshot) -> Result<String, String> {
    evaluate_module(
        preparation::PreparedGraph::new(snapshot, "plugin.ts")?,
        None,
    )
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
            "export const name = 'clock'; export const handlers = () => {};",
            "export const name = 'clock'; export const handlers = {'launch.prepare': 42};",
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
            .contains("failed to parse"));
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
    fn an_operation_is_deferred_callable_and_does_not_keep_state_between_calls() {
        let source = "export const name = 'clock'; let calls = 0; export const handlers = {'launch.prepare': function (input) { calls++; return {args: [input.path], calls}; }}";
        assert_eq!(eval_plugin_ts(source).unwrap(), "{\"name\":\"clock\"}");
        let input = r#"{"path":"/games/a.gba"}"#;
        let first = call_plugin_operation_ts(source, LAUNCH_PREPARE, input).unwrap();
        assert_eq!(first, "{\"args\":[\"/games/a.gba\"],\"calls\":1}");
        assert_eq!(
            call_plugin_operation_ts(source, LAUNCH_PREPARE, input).unwrap(),
            first
        );
        assert!(
            call_plugin_operation_ts("export const name = 'clock'", LAUNCH_PREPARE, input).is_err()
        );
        assert!(call_plugin_operation_ts(source, LAUNCH_PREPARE, "invalid JSON").is_err());
        assert!(
            call_plugin_operation_ts(source, LAUNCH_PREPARE, &" ".repeat(512 * 1024 + 1)).is_err()
        );
    }

    #[test]
    fn the_host_names_the_operation_and_an_unimplemented_one_fails_explicitly() {
        let source = "export const name = 'clock'; export const handlers = {'launch.prepare': () => ({picked: 'launch.prepare'}), 'settings.describe': () => ({picked: 'settings.describe'})}";
        assert_eq!(
            call_plugin_operation_ts(source, "settings.describe", "null").unwrap(),
            "{\"picked\":\"settings.describe\"}"
        );
        assert_eq!(
            call_plugin_operation_ts(source, LAUNCH_PREPARE, "null").unwrap(),
            "{\"picked\":\"launch.prepare\"}"
        );
        // An operation this plugin does not implement is reported as its own
        // case, never answered with an empty success and never confused with a
        // handler that ran and failed.
        let failure = call_plugin_operation_ts(source, "session.control", "null").unwrap_err();
        assert!(failure.is_unimplemented(), "{failure}");
        assert!(failure.to_string().contains("session.control"), "{failure}");
        let failure = call_plugin_operation_ts("export const name = 'clock'", "job.status", "null")
            .unwrap_err();
        assert!(failure.is_unimplemented(), "{failure}");
        let failure = call_plugin_operation_ts(
            "export const name = 'clock'; export const handlers = {'job.status': () => { throw new Error('boom'); }}",
            "job.status",
            "null",
        )
        .unwrap_err();
        assert!(!failure.is_unimplemented(), "{failure}");
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
                format!("export const name = 'clock'; export const handlers = {{'launch.prepare': function (input) {{ {body} }}}}");
            assert!(eval_plugin_ts(&source).is_ok());
            assert!(
                call_plugin_operation_ts(&source, "launch.prepare", "null").is_err(),
                "accepted {body}"
            );
        }
        assert!(eval_plugin_ts("export const name = 'clock'; while (true) {}").is_err());
        assert!(eval_plugin_ts(&" ".repeat(128 * 1024 + 1)).is_err());
    }
}
