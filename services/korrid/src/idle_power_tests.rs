use super::*;

#[test]
fn fifteen_minutes_needs_a_delivered_warning_and_never_repeats_the_effect() {
    let start = Instant::now();
    let policy = IdlePower::new(start);
    assert_eq!(
        policy.check(start + Duration::from_secs(839), true),
        Decision::Wait
    );
    assert_eq!(
        policy.check(start + Duration::from_secs(840), true),
        Decision::Warn
    );
    let token = policy.warning_token().unwrap();
    assert_eq!(
        policy.check(start + Duration::from_secs(1900), true),
        Decision::Wait
    );
    policy.delivered(token, start + Duration::from_secs(1900));
    assert_eq!(
        policy.check(start + Duration::from_secs(1959), true),
        Decision::Wait
    );
    assert_eq!(
        policy.check(start + Duration::from_secs(1960), true),
        Decision::Act
    );
    assert_eq!(
        policy.check(start + Duration::from_secs(3000), true),
        Decision::Wait
    );
}

#[test]
fn activity_and_each_inhibitor_cancel_the_warning_and_start_a_fresh_countdown() {
    let start = Instant::now();
    for input in [false, true] {
        let policy = IdlePower::new(start);
        assert_eq!(
            policy.check(start + Duration::from_secs(840), true),
            Decision::Warn
        );
        let stale = policy.warning_token().unwrap();
        if input {
            policy.activity_at(start + Duration::from_secs(899));
        } else {
            assert_eq!(
                policy.check(start + Duration::from_secs(899), false),
                Decision::Wait
            );
        }
        assert!(policy.warning_token().is_none());
        policy.delivered(stale, start + Duration::from_secs(900));
        assert_eq!(
            policy.check(start + Duration::from_secs(1739), true),
            Decision::Warn
        );
        assert_ne!(policy.warning_token(), Some(stale));
        assert_eq!(
            policy.check(start + Duration::from_secs(1800), true),
            Decision::Wait
        );
    }
}

#[test]
fn games_streams_installs_save_sync_unknown_signals_and_external_power_all_inhibit() {
    let idle = Signals {
        on_battery: true,
        known: true,
        ..Default::default()
    };
    assert!(idle.eligible());
    for busy in [
        Signals {
            on_battery: false,
            ..idle
        },
        Signals {
            known: false,
            ..idle
        },
        Signals { game: true, ..idle },
        Signals {
            stream: true,
            ..idle
        },
        Signals {
            install: true,
            ..idle
        },
        Signals {
            save_sync: true,
            ..idle
        },
    ] {
        assert!(!busy.eligible(), "{busy:?}");
    }
}

#[test]
fn work_accounting_survives_overlap_error_and_cancellation() {
    let work = Work::default();
    assert!(!work.busy());
    let first = work.begin();
    let second = work.begin();
    assert!(work.busy());
    drop(first);
    assert!(work.busy());
    drop(second);
    assert!(!work.busy());
}

#[test]
fn warning_uses_the_existing_surface_confirmation_and_one_cancel_action() {
    let start = Instant::now();
    let policy = IdlePower::new(start);
    assert!(policy.status().is_none());
    policy.check(start + Duration::from_secs(840), true);
    let SurfaceStatus::Confirm {
        actions,
        decline_action_id,
        ..
    } = policy.status().unwrap()
    else {
        panic!("expected confirmation")
    };
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].id, CANCEL_ACTION);
    assert_eq!(actions[0].label, "Keep on");
    assert!(actions[0].enabled);
    assert_eq!(decline_action_id.as_deref(), Some(CANCEL_ACTION));
}

#[test]
fn observing_the_same_old_input_or_finished_work_does_not_cancel_a_warning() {
    let start = Instant::now();
    let policy = IdlePower::new(start);
    policy.check(start + Duration::from_secs(840), true);
    let token = policy.warning_token().unwrap();
    policy.activity_at(start);
    assert_eq!(policy.warning_token(), Some(token));
    policy.delivered(token, start + Duration::from_secs(840));
    assert_eq!(
        policy.check(start + Duration::from_secs(900), true),
        Decision::Act
    );
}

#[cfg(target_os = "linux")]
struct LoginManager(std::sync::Arc<std::sync::atomic::AtomicUsize>);
#[cfg(target_os = "linux")]
#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl LoginManager {
    fn power_off(&self, interactive: bool) {
        assert!(
            !interactive,
            "daemon must not open a system authentication prompt"
        );
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn os_action_calls_logind_poweroff_noninteractively_on_a_private_bus() {
    use std::{
        io::{BufRead, BufReader},
        process::{Command, Stdio},
    };
    struct Bus(std::process::Child);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut address = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let _bus = Bus(child);
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let _server = zbus::connection::Builder::address(address.trim())
        .unwrap()
        .name("org.freedesktop.login1")
        .unwrap()
        .serve_at("/org/freedesktop/login1", LoginManager(calls.clone()))
        .unwrap()
        .build()
        .await
        .unwrap();
    let client = zbus::connection::Builder::address(address.trim())
        .unwrap()
        .build()
        .await
        .unwrap();
    power_off_on(&client).await.unwrap();
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn input_after_the_deadline_cancels_an_action_that_has_not_reached_the_os() {
    let start = Instant::now();
    let policy = IdlePower::new(start);
    policy.check(start + Duration::from_secs(840), true);
    policy.delivered(
        policy.warning_token().unwrap(),
        start + Duration::from_secs(840),
    );
    assert_eq!(
        policy.check(start + Duration::from_secs(900), true),
        Decision::Act
    );
    assert!(policy.action_pending());
    policy.activity_at(start + Duration::from_secs(901));
    assert!(!policy.action_pending());
}
