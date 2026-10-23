use super::{
    hooks::{Context, LaunchHooks},
    identity::{read_active, replace_active, ActiveSession},
    input_seat::DisabledInputSeats,
    session_state::{HostSessionControl, HostSessionStatus, HostSessionStop},
    systemd_unit::{LaunchUnitBackend, LaunchUnitError, LaunchUnitErrorKind, LaunchUnitState},
};
use crate::launch_settings::{
    helper::Client, session::SessionLimits, LaunchPercentRange, ResolvedHooks,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

#[derive(Default)]
struct RecordingBackend {
    events: Arc<Mutex<Vec<String>>>,
    unit: Mutex<Option<(String, LaunchUnitState)>>,
    fail_game: Mutex<bool>,
    hooks: Mutex<Vec<(String, Duration, BTreeMap<String, String>)>>,
}
impl LaunchUnitBackend for RecordingBackend {
    fn launch(
        &self,
        id: &str,
        _: &[String],
        _: &BTreeMap<String, String>,
    ) -> Result<(), LaunchUnitError> {
        self.events.lock().unwrap().push("game".into());
        if *self.fail_game.lock().unwrap() {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "start refused",
            ));
        }
        *self.unit.lock().unwrap() = Some((id.into(), LaunchUnitState::Running));
        Ok(())
    }
    fn run_hook(
        &self,
        _: &str,
        _: usize,
        _: &[String],
        run: &str,
        timeout: Duration,
        env: &BTreeMap<String, String>,
    ) -> Result<(), LaunchUnitError> {
        self.events.lock().unwrap().push(run.into());
        self.hooks
            .lock()
            .unwrap()
            .push((run.into(), timeout, env.clone()));
        match run {
            "fail" => Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Failed,
                "status 1",
            )),
            "timeout" => Err(LaunchUnitError::new(
                LaunchUnitErrorKind::Timeout,
                "timed out",
            )),
            _ => Ok(()),
        }
    }
    fn state(&self, _: &str) -> Result<LaunchUnitState, LaunchUnitError> {
        if *self.fail_game.lock().unwrap() && self.unit.lock().unwrap().is_some() {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::UnitFailed,
                "signal 9",
            ));
        }
        Ok(self
            .unit
            .lock()
            .unwrap()
            .as_ref()
            .map_or(LaunchUnitState::Completed, |(_, state)| *state))
    }
    fn exit_status(&self, _: &str) -> Result<Option<i32>, LaunchUnitError> {
        Ok(Some(0))
    }
    fn stop(&self, _: &str) -> Result<(), LaunchUnitError> {
        self.events.lock().unwrap().push("stop".into());
        *self.unit.lock().unwrap() = None;
        Ok(())
    }
    fn live_launch_ids(&self) -> Result<Vec<String>, LaunchUnitError> {
        Ok(self
            .unit
            .lock()
            .unwrap()
            .as_ref()
            .filter(|(_, state)| *state != LaunchUnitState::Completed)
            .map(|(id, _)| vec![id.clone()])
            .unwrap_or_default())
    }
    fn freeze(&self, _: &str) -> Result<(), LaunchUnitError> {
        Ok(())
    }
    fn thaw(&self, _: &str) -> Result<(), LaunchUnitError> {
        Ok(())
    }
}

fn hooks(before: serde_json::Value, after: serde_json::Value) -> LaunchHooks {
    LaunchHooks {
        resolved: ResolvedHooks {
            before: serde_json::from_value(before).unwrap(),
            after: serde_json::from_value(after).unwrap(),
        },
        game_environment: BTreeMap::new(),
        context: Context {
            release_id: Some("release-1".into()),
            system_id: Some("system-1".into()),
            runner_id: Some("@test:runner/main".into()),
            profile_id: Some("fast".into()),
        },
    }
}
fn control(root: &std::path::Path, backend: Arc<RecordingBackend>) -> HostSessionControl {
    HostSessionControl::with_input_seats(root, backend, Arc::new(DisabledInputSeats))
}
fn prepare(
    control: &HostSessionControl,
    hooks: &LaunchHooks,
) -> Result<crate::game_routes::SelectedGameLaunch, crate::RpcFailure> {
    control.prepare_with_limits(
        "game-1",
        None,
        Ok(&["game".into()]),
        &BTreeMap::from([("GAME_ENV".into(), "resolved".into())]),
        None,
        vec![],
        Some(hooks),
    )
}

#[test]
fn hooks_before_in_order_after_reversed_and_after_end_session() {
    let root = tempfile::tempdir().unwrap();
    let backend = Arc::new(RecordingBackend::default());
    let socket = root.path().join("limits.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let events = backend.events.clone();
    let server = thread::spawn(move || {
        for expected in ["ApplySession", "EndSession"] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            let request: serde_json::Value = serde_json::from_str(&request).unwrap();
            assert_eq!(request["_tag"], expected);
            events.lock().unwrap().push(expected.into());
            stream
                .write_all(b"{\"_tag\":\"Applied\",\"payload\":{}}\n")
                .unwrap();
        }
    });
    let mut control = control(root.path(), backend.clone());
    control.set_limits_client(Client { socket });
    let steps = hooks(
        json!([{"run":"before-1"},{"run":"before-2"}]),
        json!([{"run":"after-1"},{"run":"fail"},{"run":"after-2"}]),
    );
    let limits = SessionLimits {
        cpu: LaunchPercentRange { min: 0, max: 100 },
        gpu: LaunchPercentRange { min: 0, max: 100 },
        runner_id: "@test:runner/main".into(),
        build: "test".into(),
    };
    let launch = control
        .prepare_with_limits(
            "game-1",
            None,
            Ok(&["game".into()]),
            &BTreeMap::new(),
            Some(&limits),
            vec![],
            Some(&steps),
        )
        .unwrap();
    assert!(matches!(
        control.stop(&launch.session.launch_id),
        HostSessionStop::Completed { .. }
    ));
    server.join().unwrap();
    assert_eq!(
        *backend.events.lock().unwrap(),
        [
            "before-1",
            "before-2",
            "game",
            "ApplySession",
            "stop",
            "EndSession",
            "after-2",
            "fail",
            "after-1"
        ]
    );
}

#[test]
fn abort_and_timeout_prevent_game_limits_and_after_steps_while_warn_continues() {
    for (run, policy, succeeds) in [
        ("fail", None, false),
        ("timeout", None, false),
        ("fail", Some("warn"), true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(RecordingBackend::default());
        let control = control(root.path(), backend.clone());
        let mut step = json!({"run":run,"timeout":2});
        if let Some(policy) = policy {
            step["on-failure"] = policy.into();
        }
        let steps = hooks(json!([step,{"run":"next"}]), json!([{"run":"after"}]));
        let result = prepare(&control, &steps);
        if succeeds {
            let result = result.unwrap();
            assert_eq!(result.warnings.len(), 1);
            assert_eq!(*backend.events.lock().unwrap(), ["fail", "next", "game"]);
        } else {
            let failure = result.unwrap_err();
            assert_eq!(failure.code, "HookFailed");
            assert!(failure.message.contains(if run == "timeout" {
                "timed out"
            } else {
                "status 1"
            }));
            assert_eq!(*backend.events.lock().unwrap(), [run]);
            assert!(!root.path().join("host-session/active.json").exists());
        }
        assert_eq!(backend.hooks.lock().unwrap()[0].1, Duration::from_secs(2));
    }
}

#[test]
fn after_steps_receive_exit_failure_stop_and_restart_results_and_exact_launch_identity() {
    for result in [
        "exited",
        "failed",
        "stopped",
        "restart-exited",
        "restart-failed",
        "restart-stopped",
        "start-failed",
    ] {
        let root = tempfile::tempdir().unwrap();
        let backend = Arc::new(RecordingBackend::default());
        let first = control(root.path(), backend.clone());
        let steps = hooks(json!([{"run":"before"}]), json!([{"run":"after"}]));
        if result == "start-failed" {
            *backend.fail_game.lock().unwrap() = true;
        }
        let launched = prepare(&first, &steps);
        if result == "start-failed" {
            assert_eq!(launched.unwrap_err().code, "HostLaunchFailed");
        } else {
            let launched = launched.unwrap().session;
            let before = backend.hooks.lock().unwrap()[0].2.clone();
            assert_eq!(before["KORRI_LAUNCH_ID"], launched.launch_id);
            assert_eq!(before["KORRI_GAME_ID"], "game-1");
            assert_eq!(before["KORRI_RELEASE_ID"], "release-1");
            assert_eq!(before["KORRI_SYSTEM_ID"], "system-1");
            assert_eq!(before["KORRI_RUNNER_ID"], "@test:runner/main");
            assert_eq!(before["KORRI_PROFILE_ID"], "fast");
            assert_eq!(before["GAME_ENV"], "resolved");
            assert!(!before.contains_key("KORRI_SESSION_RESULT"));
            if result.ends_with("failed") {
                *backend.fail_game.lock().unwrap() = true;
            }
            if result == "stopped" {
                assert!(matches!(
                    first.stop(&launched.launch_id),
                    HostSessionStop::Completed { .. }
                ));
            } else if result == "restart-stopped" {
                // Model the persisted stop intent and a daemon lost while systemd
                // was stopping the exact service.
                let journal = root.path().join("host-session");
                let mut record = read_active(&journal).unwrap().unwrap();
                if let ActiveSession::Running {
                    after_hooks: Some(hooks),
                    ..
                } = &mut record
                {
                    hooks.result = Some(super::hooks::SessionResult::Stopped);
                }
                replace_active(&journal, &record).unwrap();
                *backend.unit.lock().unwrap() = None;
            } else if !result.ends_with("failed") {
                backend.unit.lock().unwrap().as_mut().unwrap().1 = LaunchUnitState::Completed;
            }
            if result.starts_with("restart") {
                drop(first);
                let restarted = control(root.path(), backend.clone());
                assert!(matches!(
                    restarted.status(),
                    HostSessionStatus::NoActive | HostSessionStatus::Failed { .. }
                ));
                restarted.status();
            } else if result != "stopped" {
                assert!(matches!(
                    first.status(),
                    HostSessionStatus::Completed { .. } | HostSessionStatus::Failed { .. }
                ));
            }
        }
        let recorded = backend.hooks.lock().unwrap();
        assert_eq!(
            recorded.iter().filter(|(run, _, _)| run == "after").count(),
            1,
            "{result}"
        );
        let after = &recorded.last().unwrap().2;
        assert_eq!(after["KORRI_HOOK_PHASE"], "after");
        let expected = if result.ends_with("failed") {
            "failed"
        } else if result.ends_with("stopped") {
            "stopped"
        } else {
            "exited"
        };
        assert_eq!(after["KORRI_SESSION_RESULT"], expected, "{result}");
        assert_eq!(
            after.get("KORRI_EXIT_STATUS").map(String::as_str),
            if expected == "exited" {
                Some("0")
            } else {
                None
            },
            "{result}"
        );
        assert_eq!(recorded[0].1, Duration::from_secs(30));
    }
}

#[test]
fn reboot_does_not_run_persisted_after_steps() {
    let root = tempfile::tempdir().unwrap();
    let backend = Arc::new(RecordingBackend::default());
    let first = control(root.path(), backend.clone());
    let steps = hooks(json!([]), json!([{"run":"after"}]));
    prepare(&first, &steps).unwrap();
    let journal = root.path().join("host-session");
    let mut record = read_active(&journal).unwrap().unwrap();
    if let ActiveSession::Running {
        after_hooks: Some(hooks),
        ..
    } = &mut record
    {
        hooks.boot_id = "previous-boot".into();
    }
    replace_active(&journal, &record).unwrap();
    *backend.unit.lock().unwrap() = None;
    drop(first);
    assert_eq!(
        control(root.path(), backend.clone()).status(),
        HostSessionStatus::NoActive
    );
    assert!(backend.hooks.lock().unwrap().is_empty());
}

#[tokio::test]
async fn prepared_hook_environment_uses_only_the_games_explicit_baseline_and_plan_not_daemon_secrets(
) {
    const SENTINEL: &str = "KORRID_HOST_HOOK_TEST_PRIVATE_SENTINEL";
    struct ClearSentinel;
    impl Drop for ClearSentinel {
        fn drop(&mut self) {
            std::env::remove_var(SENTINEL);
        }
    }
    std::env::set_var(SENTINEL, "must-not-reach-runtime");
    let _clear = ClearSentinel;
    let root = tempfile::tempdir().unwrap();
    crate::plugin_test_fixtures::write_gba_library(root.path());
    std::fs::create_dir(root.path().join("roms")).unwrap();
    std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let registry = crate::plugin_test_fixtures::installed(root.path());
    let device = root.path().join("device.yaml");
    let mut value: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&device).unwrap()).unwrap();
    value["host"]["env"] = serde_yaml::from_str(
        "{LAYER: plan-value, SHARED: plan-wins, REMOVED: null, KORRI_PROFILE_ID: forged}",
    )
    .unwrap();
    value["host"]["hooks"] = serde_yaml::from_str(
        "{before: [{run: capture-environment}], after: [{run: capture-after}]}",
    )
    .unwrap();
    std::fs::write(device, serde_yaml::to_string(&value).unwrap()).unwrap();
    let config = root.path().join("host.toml");
    std::fs::write(&config, "label = 'test'\n[environment]\nBASELINE = 'explicit-value'\nSHARED = 'baseline-value'\nREMOVED = 'baseline-value'\n").unwrap();
    let backend = Arc::new(RecordingBackend::default());
    let runtime = super::HostRuntime::from_paths_with_backend(
        &config,
        None,
        root.path().join("private"),
        backend.clone(),
    )
    .with_route_registry(root.path().into(), registry);
    runtime.claim_save_owner();
    let launched = runtime
        .prepare(crate::plugin_test_fixtures::GBA_ID, None)
        .await
        .unwrap();
    let captured = backend.hooks.lock().unwrap()[0].2.clone();
    assert_eq!(captured["BASELINE"], "explicit-value");
    assert_eq!(captured["LAYER"], "plan-value");
    assert_eq!(captured["SHARED"], "plan-wins");
    assert!(!captured.contains_key("REMOVED"));
    assert!(!captured.contains_key("KORRI_PROFILE_ID"));
    assert!(!captured.contains_key(SENTINEL));
    assert_eq!(captured["KORRI_LAUNCH_ID"], launched.session.launch_id);
    assert_eq!(captured["KORRI_RUNNER_ID"], "@korri:mgba/mgba");
}
