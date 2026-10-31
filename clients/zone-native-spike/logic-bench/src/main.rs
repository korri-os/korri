//! THROWAWAY BENCHMARK. Loads Zone's real `update` (bundled from TypeScript
//! by bun into `zone-logic.js`) into QuickJS through rquickjs, korrid's
//! engine, and prints JSON timings. Usage: `zone-logic-bench [STEPS]`.

use rquickjs::{Context, Function, Runtime};
use std::time::Instant;

const BUNDLE: &str = include_str!("../zone-logic.js");

fn status_kib(field: &str) -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    text.lines()
        .find(|line| line.starts_with(field))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
}

fn pss_kib() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/smaps_rollup").ok()?;
    text.lines()
        .find(|line| line.starts_with("Pss:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
}

fn main() {
    let steps: u32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20_000);
    let clock = Instant::now();
    let rss_before = status_kib("VmRSS:");
    let pss_before = pss_kib();

    let t_runtime = Instant::now();
    let runtime = Runtime::new().expect("runtime");
    let context = Context::full(&runtime).expect("context");
    let runtime_us = t_runtime.elapsed().as_secs_f64() * 1e6;

    let (load_us, bench_json) = context.with(|ctx| {
        let now = Function::new(ctx.clone(), move || clock.elapsed().as_secs_f64() * 1e6).expect("now");
        ctx.globals().set("__now_us", now).expect("set now");
        let t_load = Instant::now();
        ctx.eval::<(), _>(BUNDLE).expect("evaluate bundle");
        let load_us = t_load.elapsed().as_secs_f64() * 1e6;
        let run: Function = ctx.globals().get("runBench").expect("runBench");
        let json: String = run.call((steps,)).expect("runBench call");
        (load_us, json)
    });

    let rss_after = status_kib("VmRSS:");
    let pss_after = pss_kib();
    let usage = runtime.memory_usage();

    // Reference: the same JSON parse in native Rust (serde_json), so the
    // engine's cost can be set beside native code for one identical task.
    let bench: serde_json::Value = serde_json::from_str(&bench_json).expect("bench json");
    let mut native = serde_json::Map::new();
    if let Some(text) = bench.get("bigModelText").and_then(|v| v.as_str()) {
        let mut samples = Vec::new();
        for _ in 0..200 {
            let t = Instant::now();
            let value: serde_json::Value = serde_json::from_str(text).expect("parse");
            std::hint::black_box(&value);
            samples.push(t.elapsed().as_secs_f64() * 1e6);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        native.insert("serde_json parse 500-game model p50 us".into(), samples[samples.len() / 2].into());
    }

    let report = serde_json::json!({
        "steps": steps,
        "runtime_and_context_us": runtime_us,
        "bundle_bytes": BUNDLE.len(),
        "bundle_eval_us": load_us,
        "rss_kib": { "before": rss_before, "after": rss_after },
        "pss_kib": { "before": pss_before, "after": pss_after },
        "quickjs_memory_used_bytes": usage.memory_used_size,
        "quickjs_malloc_bytes": usage.malloc_size,
        "results": bench.get("results"),
        "model_bytes": bench.get("modelBytes"),
        "native_reference": native,
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
