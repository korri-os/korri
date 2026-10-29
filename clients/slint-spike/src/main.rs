//! THROWAWAY SPIKE: a native Slint window over the real embedded korrid.
//! Composition root: environment guard, brain start, window, refresh worker.

mod brain;
mod report;

use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use brain::{Brain, Credential};
use report::{present, Session};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

slint::include_modules!();

const BUILD_ID: &str = match option_env!("KORRI_BUILD_ID") {
    Some(id) => id,
    None => "unstamped build",
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Window,
    SelfTest,
    ProofUnauthorized,
    ProofOffline,
}

fn parse_mode() -> Result<Mode, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => Ok(Mode::Window),
        ["--self-test"] => Ok(Mode::SelfTest),
        ["--proof-failure=unauthorized"] => Ok(Mode::ProofUnauthorized),
        ["--proof-failure=offline"] => Ok(Mode::ProofOffline),
        _ => Err(
            "usage: korri-slint-spike [--self-test | --proof-failure=unauthorized|offline]".into(),
        ),
    }
}

fn main() -> ExitCode {
    // Block SIGINT/SIGTERM before any thread exists, so every thread inherits
    // the mask and one waiter can quit the event loop. The scratch directories
    // then go through the normal shutdown instead of being left behind.
    let signals = block_termination_signals();
    // As in clients/linux: no core dump may carry the in-memory capability.
    let no_core = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: plain setrlimit on this process with a valid struct.
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &no_core) } != 0 {
        eprintln!("refusing to start: could not disable core dumps");
        return ExitCode::from(2);
    }
    let mode = match parse_mode() {
        Ok(mode) => mode,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        }
    };
    let inherited = brain::inherited_korri_environment();
    if !inherited.is_empty() {
        eprintln!(
            "refusing to start: inherited korrid configuration could reach real peers: {}. \
             Unset these names and run again.",
            inherited.join(", ")
        );
        return ExitCode::from(2);
    }

    let mut brain = match Brain::start() {
        Ok(brain) => brain,
        Err(error) => {
            eprintln!("korrid did not start: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!("local brain started (loopback only)");
    if mode == Mode::ProofOffline {
        if let Err(error) = brain.stop_server() {
            eprintln!("could not stop korrid for the offline proof: {error}");
            return ExitCode::FAILURE;
        }
        println!("korrid stopped for the offline proof");
    }
    let brain = Arc::new(brain);
    let outcome = run_window(brain.clone(), mode, signals);

    // A refresh in flight ends within brain::INSPECTION_BUDGET (4 s total);
    // the self-test's auth probe adds one more budget.
    let deadline = Instant::now() + 2 * brain::INSPECTION_BUDGET + Duration::from_secs(2);
    while Arc::strong_count(&brain) > 1 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let brain = match Arc::try_unwrap(brain) {
        Ok(brain) => brain,
        Err(_) => {
            eprintln!("a refresh worker still holds korrid past its budget");
            return ExitCode::FAILURE;
        }
    };
    match brain.shutdown() {
        Ok(()) => println!("korrid stopped; scratch storage removed"),
        Err(error) => {
            eprintln!("shutdown: {error}");
            return ExitCode::FAILURE;
        }
    }
    outcome
}

fn run_window(brain: Arc<Brain>, mode: Mode, signals: libc::sigset_t) -> ExitCode {
    let window = match InspectorWindow::new() {
        Ok(window) => window,
        Err(error) => {
            eprintln!("could not open the window: {error}");
            return ExitCode::FAILURE;
        }
    };
    spawn_signal_waiter(signals);

    let session = Arc::new(Mutex::new(Session {
        build_id: BUILD_ID,
        refreshing: false,
        checks: 0,
        last: None,
    }));
    window.set_report(to_slint(&present(&session.lock().unwrap())));
    window.on_plan(|width, height| {
        let plan = report::plan(width, height);
        Plan {
            columns: plan.columns,
            header_inline: plan.header_inline,
            compact: plan.compact,
        }
    });
    window.on_close_window(|| {
        let _ = slint::quit_event_loop();
    });

    let credential = match mode {
        Mode::ProofUnauthorized => Credential::Wrong,
        _ => Credential::Real,
    };
    let self_test = mode == Mode::SelfTest;
    {
        let weak = window.as_weak();
        let session = session.clone();
        let brain = brain.clone();
        window.on_refresh(move || {
            {
                let mut state = session.lock().unwrap();
                if state.refreshing {
                    return;
                }
                state.refreshing = true;
                if let Some(window) = weak.upgrade() {
                    window.set_report(to_slint(&present(&state)));
                }
            }
            let weak = weak.clone();
            let session = session.clone();
            let brain = brain.clone();
            std::thread::spawn(move || {
                let inspection = brain.inspect(credential);
                let probe = self_test.then(|| brain.probe_authorization());
                drop(brain);
                let _ = weak.upgrade_in_event_loop(move |window| {
                    let report = {
                        let mut state = session.lock().unwrap();
                        state.refreshing = false;
                        state.checks += 1;
                        state.last = Some(inspection);
                        present(&state)
                    };
                    window.set_report(to_slint(&report));
                    println!(
                        "refresh {} complete: {:?}, {}",
                        report.checks, report.tone, report.headline
                    );
                    if let Some(probe) = probe {
                        self_test_step(&window, &report, probe);
                    }
                });
            });
        });
    }

    // First check runs through the same callback that Refresh and Return use.
    let weak = window.as_weak();
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(window) = weak.upgrade() {
            window.invoke_refresh();
        }
    });

    if let Err(error) = window.run() {
        eprintln!("event loop: {error}");
        return ExitCode::FAILURE;
    }
    if self_test {
        return if SELF_TEST_PASSED.load(std::sync::atomic::Ordering::SeqCst) {
            println!("SELF-TEST PASS");
            ExitCode::SUCCESS
        } else {
            println!("SELF-TEST FAIL");
            ExitCode::FAILURE
        };
    }
    ExitCode::SUCCESS
}

static SELF_TEST_PASSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Runs after each self-test refresh, reading back what the window holds.
fn self_test_step(window: &InspectorWindow, report: &report::Report, probe: brain::AuthProbe) {
    let shown = window.get_report();
    let mut failures = Vec::new();
    let mut check = |ok: bool, what: &str| {
        println!("  {} {what}", if ok { "ok  " } else { "FAIL" });
        if !ok {
            failures.push(what.to_owned());
        }
    };
    check(shown.tone == Tone::Healthy, "window shows a healthy korrid");
    check(
        shown.checks == report.checks as i32,
        "window holds the report of this refresh",
    );
    check(
        report
            .build
            .iter()
            .any(|fact| fact.value.starts_with(korrid::VERSION)),
        "system.health version is shown",
    );
    check(
        report
            .identity
            .first()
            .is_some_and(|fact| fact.value.starts_with("Unowned")),
        "fresh scratch device is unowned",
    );
    check(
        report.catalog_summary == "No games in this device's catalog.",
        "catalog is honestly empty",
    );
    check(window.get_refresh_focused(), "Refresh holds keyboard focus");
    check(
        probe.without_bearer == Some(401),
        "system.health without a bearer is rejected with 401",
    );
    check(
        probe.wrong_bearer == Some(401),
        "system.health with a wrong bearer is rejected with 401",
    );
    check(
        probe.real_bearer == Some(200),
        "system.health with the real bearer is accepted",
    );
    if !failures.is_empty() {
        let _ = slint::quit_event_loop();
        return;
    }
    if report.checks < 2 {
        // Second pass: the callback a click or Return key invokes.
        println!("  invoking the Refresh callback again");
        window.invoke_refresh();
    } else {
        SELF_TEST_PASSED.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = slint::quit_event_loop();
    }
}

fn to_slint(report: &report::Report) -> Report {
    let facts = |facts: &[report::Fact]| -> ModelRc<Fact> {
        ModelRc::new(VecModel::from(
            facts
                .iter()
                .map(|fact| Fact {
                    label: fact.label.as_str().into(),
                    value: fact.value.as_str().into(),
                    unbroken: fact.unbroken,
                })
                .collect::<Vec<_>>(),
        ))
    };
    let lines = |lines: &[String]| -> ModelRc<SharedString> {
        ModelRc::new(VecModel::from(
            lines
                .iter()
                .map(|line| SharedString::from(line.as_str()))
                .collect::<Vec<_>>(),
        ))
    };
    Report {
        tone: match report.tone {
            report::Tone::Checking => Tone::Checking,
            report::Tone::Healthy => Tone::Healthy,
            report::Tone::Partial => Tone::Partial,
            report::Tone::Failed => Tone::Failed,
        },
        headline: report.headline.as_str().into(),
        detail: report.detail.as_str().into(),
        connection: facts(&report.connection),
        identity: facts(&report.identity),
        catalog_summary: report.catalog_summary.as_str().into(),
        games: lines(&report.games),
        peers_summary: report.peers_summary.as_str().into(),
        peers: lines(&report.peers),
        build: facts(&report.build),
        refreshing: report.refreshing,
        checks: report.checks as i32,
    }
}

fn block_termination_signals() -> libc::sigset_t {
    // SAFETY: sigset_t is plain data; the calls only fill and apply the set.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        set
    }
}

fn spawn_signal_waiter(set: libc::sigset_t) {
    std::thread::spawn(move || {
        let mut signal = 0;
        // SAFETY: `set` was built by block_termination_signals.
        if unsafe { libc::sigwait(&set, &mut signal) } == 0 {
            println!("signal {signal}: closing");
            let _ = slint::invoke_from_event_loop(|| {
                let _ = slint::quit_event_loop();
            });
        }
    });
}
