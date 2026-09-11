//! Unshipped build-machine feasibility probe. The loader only sees a prepared
//! in-memory graph and optional pinned pure platform libraries and real timers.
//! No host I/O is exposed. All caps and preparation here are experimental only.
mod timers;
use oxc::{
    allocator::Allocator,
    codegen::Codegen,
    parser::Parser,
    semantic::SemanticBuilder,
    span::SourceType,
    transformer::{TransformOptions, Transformer},
};
use rquickjs::{
    loader::{BuiltinLoader, Resolver},
    Context, Ctx, Error, Function, Module, Runtime, Value,
};
use serde::Deserialize;
use serde_json::{json, Value as Json};
use std::{
    cell::Cell,
    collections::HashMap,
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Source {
    source: String,
    dependencies: HashMap<String, String>,
}
#[derive(Deserialize)]
struct PlatformModule {
    name: String,
    source: String,
}
#[derive(Deserialize)]
struct Graph {
    entry: String,
    modules: HashMap<String, Source>,
    metadata: Json,
    #[serde(default)]
    platform_modules: Vec<PlatformModule>,
    #[serde(default)]
    primitive_source: String,
    #[serde(default)]
    primitive_expected: Json,
}
struct ApprovedResolver(HashMap<String, HashMap<String, String>>);
impl Resolver for ApprovedResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
    ) -> rquickjs::Result<String> {
        self.0
            .get(base)
            .and_then(|deps| deps.get(name))
            .cloned()
            .ok_or_else(|| Error::new_resolving(base, name))
    }
}
// Same Oxc options as script::transpile_ts. Probe reports sizes rather than
// changing or pretending to enforce the product's single-source size limits.
fn transpile(source: &str, path: &str) -> String {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(
        parsed.diagnostics.is_empty(),
        "{path}: {:?}",
        parsed.diagnostics
    );
    let mut program = parsed.program;
    let scoping = SemanticBuilder::new()
        .with_enum_eval(true)
        .build(&program)
        .semantic
        .into_scoping();
    let mut options = TransformOptions::default();
    options.typescript.only_remove_type_imports = true;
    let transformed = Transformer::new(&allocator, Path::new(path), &options)
        .build_with_scoping(scoping, &mut program);
    assert!(
        transformed.diagnostics.is_empty(),
        "{path}: {:?}",
        transformed.diagnostics
    );
    Codegen::new().build(&program).code
}
fn failure(ctx: &Ctx<'_>, error: Error) -> String {
    if error.is_exception() {
        let exception = ctx.catch();
        if let Some(object) = exception.as_object() {
            let message = object.get::<_, String>("message").unwrap_or_default();
            let stack = object.get::<_, String>("stack").unwrap_or_default();
            return format!("{error}: {message}\n{stack}");
        }
        return format!("{error}: {exception:?}");
    }
    error.to_string()
}
fn observed_heap(ctx: &Ctx<'_>) -> i64 {
    // The context already holds the runtime borrow. Use the same QuickJS
    // accounting function as Runtime::memory_usage without borrowing it twice.
    unsafe {
        let mut usage = std::mem::zeroed();
        rquickjs::qjs::JS_ComputeMemoryUsage(
            rquickjs::qjs::JS_GetRuntime(ctx.as_raw().as_ptr()),
            &mut usage,
        );
        usage.memory_used_size
    }
}
fn primitive_checks(ctx: &Ctx<'_>, source: &str, expected: &Json, report: &mut Json) {
    let checks_start = Instant::now();
    let checked = (|| {
        let module = Module::declare(ctx.clone(), "primitive-cases.js", source.as_bytes())?;
        let (module, promise) = module.eval()?;
        promise.finish::<()>()?;
        let function: Function = module.get("primitiveCases")?;
        let output: Value = function.call(())?;
        ctx.json_stringify(output)?
            .ok_or(Error::Unknown)?
            .to_string()
    })();
    report["primitive_checks_ms"] = (checks_start.elapsed().as_secs_f64() * 1000.).into();
    match checked {
        Ok(output) => {
            let output: Json = serde_json::from_str(&output).unwrap();
            let checks: Vec<_> = expected.as_object().unwrap().iter().map(|(name, expected)|
                json!({"case":name,"passed":output.get(name)==Some(expected),"native_bun":expected,"quickjs":output.get(name)})).collect();
            report["primitive_checks"] = checks.into();
        }
        Err(error) => report["primitive_error"] = failure(ctx, error).into(),
    }
    report["observed_heap_after_primitive_checks_bytes"] = observed_heap(ctx).into();
}
fn cases() -> Vec<(&'static str, Json, bool, Json)> {
    vec![
        (
            "nested-video-audio",
            json!({"video":{"vsync":false,"aspectRatio":"core-provided","sync":{"frameDelay":12,"hardSyncFrames":2},"hdr":{"maxNits":1000}},"audio":{"outputRate":48000,"latencyMs":0,"volumeDb":-3,"device":null}}),
            true,
            json!({"video_vsync":false,"aspect_ratio_index":22,"video_frame_delay":12,"video_hard_sync_frames":2,"video_hdr_max_nits":1000,"audio_out_rate":48000,"audio_latency":0,"audio_volume":-3}),
        ),
        (
            "wrong-video-type",
            json!({"video":{"vsync":"true"}}),
            false,
            json!({}),
        ),
        (
            "wrong-audio-type",
            json!({"audio":{"enable":1}}),
            false,
            json!({}),
        ),
        (
            "frame-delay-range",
            json!({"video":{"sync":{"frameDelay":100}}}),
            false,
            json!({}),
        ),
        (
            "audio-negative-range",
            json!({"audio":{"latencyMs":-1}}),
            false,
            json!({}),
        ),
        (
            "audio-fractional-integer",
            json!({"audio":{"outputRate":1.5}}),
            false,
            json!({}),
        ),
        (
            "unknown-root",
            json!({"video_vsync":true}),
            false,
            json!({}),
        ),
        (
            "unknown-nested",
            json!({"video":{"sync":{"extra":true}}}),
            false,
            json!({}),
        ),
        (
            "https-valid",
            json!({"updater":{"buildbotUrl":"https://example.invalid/cores?x=1#y"}}),
            true,
            json!({"core_updater_buildbot_url":"https://example.invalid/cores?x=1#y"}),
        ),
        (
            "https-assets-valid",
            json!({"updater":{"buildbotAssetsUrl":"https://example.invalid/assets"}}),
            true,
            json!({"core_updater_buildbot_assets_url":"https://example.invalid/assets"}),
        ),
        (
            "http-invalid",
            json!({"updater":{"buildbotUrl":"http://example.invalid"}}),
            false,
            json!({}),
        ),
        (
            "https-malformed",
            json!({"updater":{"buildbotUrl":"https://"}}),
            false,
            json!({}),
        ),
        (
            "https-relative",
            json!({"updater":{"buildbotUrl":"/relative"}}),
            false,
            json!({}),
        ),
        (
            "https-null",
            json!({"updater":{"buildbotUrl":null}}),
            true,
            json!({}),
        ),
    ]
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("cases") {
        println!("{}", serde_json::to_string(&cases()).unwrap());
        return;
    }
    let read = Instant::now();
    let mut graph: Graph = serde_json::from_slice(&std::fs::read(&args[1]).unwrap()).unwrap();
    graph
        .metadata
        .as_object_mut()
        .unwrap()
        .remove("original_source_sha256");
    graph
        .metadata
        .as_object_mut()
        .unwrap()
        .remove("platform_source_sha256");
    let read_ms = read.elapsed().as_secs_f64() * 1000.;
    let cap_mib: usize = args[2].parse().unwrap();
    let deadline_ms: u64 = args[3].parse().unwrap();
    let samples: usize = args[4].parse().unwrap();
    if samples == 0 {
        eprintln!("samples must be positive");
        std::process::exit(2);
    }
    let mode = args.get(5).map(String::as_str).unwrap_or("policy");
    let primitives_only = mode == "primitives";
    let timers_enabled = args.get(6).map(String::as_str) == Some("timers");
    assert!(!mode.starts_with("timer-") || timers_enabled);
    assert!(!primitives_only || !graph.platform_modules.is_empty());
    let preparation = Instant::now();
    let mut ts_bytes = 0;
    for (name, module) in &mut graph.modules {
        if name.ends_with(".ts") {
            ts_bytes += module.source.len();
            module.source = transpile(&module.source, name);
        }
    }
    let transpile_ms = preparation.elapsed().as_secs_f64() * 1000.;
    let primitive_preparation = Instant::now();
    let primitive_source = transpile(&graph.primitive_source, "primitive-cases.ts");
    let primitive_transpile_ms = primitive_preparation.elapsed().as_secs_f64() * 1000.;
    println!(
        "{}",
        json!({"kind":"preparation", "metadata":graph.metadata,
        "manifest_read_parse_ms":read_ms,"oxc_transpile_ms":transpile_ms,"typescript_source_bytes":ts_bytes,
        "quickjs_input_bytes":graph.modules.values().map(|m| m.source.len()).sum::<usize>(),
        "primitive_transpile_ms":primitive_transpile_ms,"primitive_js_bytes":primitive_source.len(),
        "timer_js_bytes":if timers_enabled { include_str!("timers.js").len() } else { 0 },
        "platform_js_bytes":graph.platform_modules.iter().map(|m| m.source.len()).sum::<usize>()})
    );
    for sample in 0..samples {
        let runtime = Runtime::new().unwrap();
        runtime.set_memory_limit(cap_mib * 1024 * 1024);
        runtime.set_max_stack_size(512 * 1024);
        let mut loader = BuiltinLoader::default();
        let mut dependencies = HashMap::new();
        for (name, module) in &graph.modules {
            if name != &graph.entry {
                loader.add_module(name.clone(), module.source.as_bytes().to_vec());
            }
            dependencies.insert(name.clone(), module.dependencies.clone());
        }
        runtime.set_loader(ApprovedResolver(dependencies), loader);
        let started = Instant::now();
        let deadline = started + Duration::from_millis(deadline_ms);
        let enforce_deadline = Rc::new(Cell::new(true));
        let interrupt_enabled = enforce_deadline.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || {
            interrupt_enabled.get() && Instant::now() >= deadline
        })));
        let context = match Context::full(&runtime) {
            Ok(context) => context,
            Err(error) => {
                println!(
                    "{}",
                    json!({"kind":"sample", "sample":sample, "heap_cap_mib":cap_mib,
                    "deadline_ms":deadline_ms, "context_error":error.to_string()})
                );
                continue;
            }
        };
        let initial_heap = runtime.memory_usage().memory_used_size;
        let mut report = json!({"kind":"sample", "sample":sample,"heap_cap_mib":cap_mib,"deadline_ms":deadline_ms,
            "observed_heap_context_bytes":initial_heap,"primitives_only":primitives_only,"mode":mode,"timers_enabled":timers_enabled});
        context.with(|ctx| {
            let timer_install = Instant::now();
            let timers = if timers_enabled {
                match timers::Timers::install(&ctx, started, deadline) {
                    Ok(timers) => Some(timers),
                    Err(error) => { report["timer_install_error"] = failure(&ctx, error).into(); return; }
                }
            } else { None };
            report["timer_install_ms"] = (timer_install.elapsed().as_secs_f64()*1000.).into();
            report["observed_heap_after_timer_install_bytes"] = observed_heap(&ctx).into();
            // All evaluation exits pass through queue cleanup while Ctx is alive.
            (|| {
            report["globals"] = ctx.eval::<String, _>("JSON.stringify(Object.fromEntries(['setTimeout','clearTimeout','setInterval','URL','URLSearchParams','TextEncoder','TextDecoder','fetch','process','require'].map(k=>[k,typeof globalThis[k]])))").unwrap().into();
            if mode.starts_with("timer-") {
                let source = match mode {
                    "timer-checks" => include_str!("timer-cases.js"),
                    "timer-deadline" => "setTimeout(() => { throw new Error('must not fire'); }, 10000)",
                    "timer-nested-deadline" => "setTimeout(function repeat() { setTimeout(repeat, 0); }, 0)",
                    "timer-interrupt" => "setTimeout(() => { while (true) {} }, 0)",
                    "timer-throw" => "setTimeout(() => { throw new Error('callback failure'); }, 0); setTimeout(() => {}, 10000)",
                    "timer-cleanup" => "setTimeout(() => {}, 10000)",
                    _ => panic!("Unknown timer conformance mode"),
                };
                if let Err(error) = ctx.eval::<(), _>(source) {
                    report["timer_case_error"] = failure(&ctx, error).into();
                    return;
                }
                if mode != "timer-cleanup" {
                    if let Err(error) = timers.as_ref().unwrap().drain(&ctx) {
                        report["timer_drain_error"] = error.into();
                        return;
                    }
                }
                if mode == "timer-checks" {
                    let output: String = ctx.eval("JSON.stringify(timerResults)").unwrap();
                    report["timer_checks"] = serde_json::from_str::<Json>(&output).unwrap();
                }
                return;
            }
            let mut platform_results = Vec::new();
            for source in &graph.platform_modules {
                let init = Instant::now();
                let result = Module::declare(ctx.clone(), source.name.clone(), source.source.as_bytes())
                    .and_then(|module| module.eval())
                    .and_then(|(_, promise)| promise.finish::<()>());
                let mut stage = json!({"module":source.name,"init_ms":init.elapsed().as_secs_f64()*1000.,
                    "observed_heap_bytes":observed_heap(&ctx)});
                if let Err(error) = result {
                    stage["error"] = failure(&ctx, error).into();
                    platform_results.push(stage);
                    report["platform_init"] = platform_results.into();
                    return;
                }
                platform_results.push(stage);
            }
            report["platform_init"] = platform_results.into();
            if let Some(timers) = &timers {
                report["timers_after_platform_init"] = timers.stats().unwrap_or_else(|error| json!({"error":failure(&ctx,error)}));
            }
            report["globals_after_platform"] = match ctx.eval::<String, _>("JSON.stringify(Object.fromEntries(['setTimeout','clearTimeout','setInterval','URL','URLSearchParams','TextEncoder','TextDecoder','fetch','process','require'].map(k=>[k,typeof globalThis[k]])))") {
                Ok(globals) => globals.into(),
                Err(error) => { report["globals_error"] = failure(&ctx,error).into(); return; }
            };
            if primitives_only {
                primitive_checks(&ctx, &primitive_source, &graph.primitive_expected, &mut report);
                if let Some(timers) = &timers {
                    if let Err(error) = timers.drain(&ctx) { report["timer_drain_error"] = error.into(); }
                }
                return;
            }
            let init = Instant::now();
            let initialized = (|| {
                let module = Module::declare(ctx.clone(), graph.entry.clone(), graph.modules[&graph.entry].source.as_bytes())?;
                let (module, promise) = module.eval()?;
                promise.finish::<()>()?;
                module.get::<_, Function>("probe")
            })();
            report["module_init_ms"] = (init.elapsed().as_secs_f64() * 1000.).into();
            report["observed_heap_after_init_bytes"] = observed_heap(&ctx).into();
            let probe = match initialized {
                Ok(probe) => probe,
                Err(error) => { report["init_error"] = failure(&ctx, error).into(); return; }
            };
            if let Some(timers) = &timers {
                if let Err(error) = timers.drain(&ctx) { report["timer_drain_error"] = error.into(); return; }
                report["timers_after_policy_init"] = timers.stats().unwrap_or_else(|error| json!({"error":failure(&ctx,error)}));
            }
            report["cold_ready_ms"] = (started.elapsed().as_secs_f64()*1000.).into();
            let mut results = Vec::new();
            // Three rounds distinguish first decode from repeated decode. One shared
            // deadline includes initialization and all calls, like the product budget.
            'rounds: for round in 0..3 {
                for (name, input, accepted, expected_pairs) in cases() {
                    let argument: Value = match ctx.json_parse(input.to_string()) {
                        Ok(argument) => argument,
                        Err(error) => {
                            results.push(json!({"case":name,"round":round,"passed":false,"input_error":failure(&ctx,error)}));
                            break 'rounds;
                        }
                    };
                    let decode = Instant::now();
                    let result = probe.call::<_, String>((argument,));
                    let elapsed = decode.elapsed().as_secs_f64() * 1_000_000.;
                    match result {
                        Ok(output) => {
                            let output: Json = serde_json::from_str(&output).unwrap();
                            let actual_pairs: serde_json::Map<String, Json> = output["pairs"].as_array().map(|pairs|
                                pairs.iter().map(|p| (p[0].as_str().unwrap().to_owned(), p[1].clone())).collect()).unwrap_or_default();
                            let passed = output["ok"] == accepted && if accepted {
                                expected_pairs.as_object().unwrap().iter().all(|(k,v)| actual_pairs.get(k) == Some(v))
                            } else {
                                output["errorKind"] == "SchemaError"
                                    && !output["error"].as_str().unwrap_or_default().contains("ReferenceError")
                            };
                            results.push(json!({"case":name,"round":round,"decode_render_serialize_us":elapsed,"passed":passed,"output":output}));
                        }
                        Err(error) => {
                            results.push(json!({"case":name,"round":round,"decode_render_serialize_us":elapsed,"passed":false,"call_error":failure(&ctx,error)}));
                            break 'rounds;
                        }
                    }
                    if let Some(timers) = &timers {
                        if let Err(error) = timers.drain(&ctx) {
                            report["timer_drain_error"] = error.into();
                            break 'rounds;
                        }
                    }
                }
            }
            if let Some(timers) = &timers {
                report["timers_after_policy_calls"] = timers.stats().unwrap_or_else(|error| json!({"error":failure(&ctx,error)}));
            }
            report["cases"] = results.into();
            report["observed_heap_after_calls_bytes"] = observed_heap(&ctx).into();
            ctx.run_gc();
            report["observed_heap_after_gc_bytes"] = observed_heap(&ctx).into();
            report["policy_finished_ms"] = (started.elapsed().as_secs_f64()*1000.).into();
            })();
            // The deadline stops untrusted work. Only bounded private cleanup and
            // accounting run after it; no user callback runs with interrupts off.
            enforce_deadline.set(false);
            if let Some(timers) = &timers {
                report["timers_before_cleanup"] = timers.stats().unwrap_or_else(|error| json!({"error":failure(&ctx,error)}));
                match timers.close() {
                    Ok(discarded) => report["timer_cleanup_discarded"] = discarded.into(),
                    Err(error) => report["timer_cleanup_error"] = failure(&ctx,error).into(),
                }
                report["timers_after_cleanup"] = timers.stats().unwrap_or_else(|error| json!({"error":failure(&ctx,error)}));
            }
        });
        report["total_context_init_calls_ms"] = (started.elapsed().as_secs_f64() * 1000.).into();
        println!("{report}");
    }
}
