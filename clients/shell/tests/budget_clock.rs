use korri_shell::{
    budget_clock::BudgetClock,
    engine::{Engine, Limits},
    shell::Options,
};
use std::time::Duration;

#[test]
fn wall_clock_remains_the_device_runtime_default() {
    assert!(matches!(Limits::default().clock, BudgetClock::Wall));
    assert!(matches!(Options::default().limits.clock, BudgetClock::Wall));
}

#[test]
fn thread_cpu_time_excludes_sleep_and_other_threads() {
    let wall = BudgetClock::Wall.start();
    let cpu = BudgetClock::ThreadCpu.start();
    std::thread::spawn(|| {
        let cpu = BudgetClock::ThreadCpu.start();
        while cpu.elapsed() < Duration::from_millis(100) {
            std::hint::spin_loop();
        }
    })
    .join()
    .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!(wall.elapsed() >= Duration::from_millis(200));
    assert!(cpu.elapsed() < Duration::from_millis(50));
}

#[test]
fn shared_load_budget_uses_the_explicit_clock() {
    const SOURCE: &str = "globalThis.surface = { init: () => 0, update: s => ({state: s, requests: []}), view: () => ({}) };";
    let limits = Limits {
        clock: BudgetClock::ThreadCpu,
        load: Duration::from_millis(50),
        ..Limits::default()
    };
    let mut cpu = Engine::load(SOURCE, limits).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    cpu.set_model("{}").unwrap();
    cpu.init(&Default::default()).unwrap();
    assert!(cpu.load_time() < limits.load);

    let wall = Engine::load(
        SOURCE,
        Limits {
            clock: BudgetClock::Wall,
            ..limits
        },
    );
    // A descheduled load can already fail; otherwise the wait crosses its
    // shared wall-clock load budget. Neither can happen with the CPU clock.
    let error = match wall {
        Err(error) => error,
        Ok(mut wall) => {
            std::thread::sleep(Duration::from_millis(100));
            wall.init(&Default::default()).unwrap_err()
        }
    };
    assert_eq!(error.kind(), "time");
}
