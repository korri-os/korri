use korri_shell::{
    budget_clock::BudgetClock,
    engine::{Engine, Limits},
};
use std::time::Duration;

/// A finite QuickJS workload, calibrated in CPU time. A Date-based spin can
/// return after descheduling without ever crossing the CPU budget (#368).
pub fn cpu_spin(ms: u64) -> String {
    let mut iterations = 1_000;
    loop {
        let spin = format!("(() => {{ const iterations = {iterations}; for (let i = 0; i < iterations; i++) {{}} }})();");
        // Measure the exact generated code. Reading the loop bound from a
        // message compiles differently in QuickJS and overestimates its cost.
        let source = format!("{spin} globalThis.surface = {{ init: () => 0, update: s => ({{ state: s, requests: [] }}), view: () => ({{}}) }};");
        let mut engine = Engine::load(
            &source,
            Limits {
                clock: BudgetClock::ThreadCpu,
                load: Duration::from_secs(60),
                ..Limits::default()
            },
        )
        .unwrap();
        engine.set_model("{}").unwrap();
        engine.init(&Default::default()).unwrap();
        let elapsed = engine.load_time();
        let target = Duration::from_millis(ms);
        if elapsed >= target && elapsed <= target.mul_f64(1.1) {
            return spin;
        }
        // Aim just above the requested CPU time without doubling past a
        // load budget (for example the shared 1.5 s evaluation/init test).
        let ratio = (target.as_secs_f64() * 1.05 / elapsed.as_secs_f64()).min(2.0);
        iterations = (iterations as f64 * ratio).ceil() as u64;
    }
}
