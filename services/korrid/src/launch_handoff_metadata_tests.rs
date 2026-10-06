//! Server-continuation ordering through both production RPC routers.
use super::*;

#[path = "launch_daemon_startup_tests.rs"]
mod startup;

struct GateRelease(Arc<RouteGate>);
impl Drop for GateRelease {
    fn drop(&mut self) {
        self.0.release();
    }
}

fn delay_session_reply(
    runtime: host::HostRuntime,
    method: &'static str,
    gate: &Arc<RouteGate>,
) -> host::HostRuntime {
    let blocked = gate.clone();
    runtime.with_session_rpc_probe(Arc::new(move |observed| {
        if observed == method && !blocked.entered.swap(true, Ordering::SeqCst) {
            blocked.block();
        }
    }))
}
fn native_routers(runtime: host::HostRuntime, root: &Path) -> (Router, Router) {
    secure_host_routers(
        runtime,
        &root.join("private"),
        Some(PortalAccess::new(
            TOKEN,
            ORIGIN,
            PortalPermission::LocalSessions,
        )),
    )
}
fn focusable_compositor() -> Arc<Compositor> {
    Arc::new(Compositor {
        gate: Mutex::new(None),
        tree: Mutex::new(json!({"id":1,"nodes":[
            {"id":2,"pid":4100,"app_id":"chromium-browser","focused":true},
            {"id":3,"pid":9100,"focused":false}
        ]})),
    })
}
async fn freeze(app: &Router, id: &str) -> Value {
    rpc(app, "app.session.freeze", json!({"expectedLaunchId":id})).await
}
async fn thaw(app: &Router, id: &str) -> Value {
    rpc(app, "app.session.thaw", json!({"expectedLaunchId":id})).await
}
async fn assert_overlay(app: &Router, id: &str) {
    let status = rpc(app, "app.session.status", json!({})).await;
    assert_eq!(
        status["outcome"]["payload"]["active"]["launchId"], id,
        "{status}"
    );
    assert_eq!(
        status["outcome"]["payload"]["overlay"]["launchId"], id,
        "{status}"
    );
}

async fn held_status_preserves_newer_home(replacement: bool, completed: bool) {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let _release = GateRelease(gate.clone());
    let (app, native) = native_routers(
        delay_session_reply(runtime, "app.session.status", &gate),
        root.path(),
    );
    let a = reserve(&app).await;
    assert_eq!(start(&app, &a).await["outcome"]["_tag"], "Ok");
    if completed {
        backend.complete_live();
    }
    let observing_app = app.clone();
    let observing =
        tokio::spawn(async move { rpc(&observing_app, "app.session.status", json!({})).await });
    gate.entered().await;
    let home_id = if replacement {
        if !completed {
            assert_eq!(
                cancel(&app, &a).await["outcome"]["payload"]["phase"],
                "stopped"
            );
        }
        let b = reserve(&app).await;
        assert_eq!(start(&app, &b).await["outcome"]["_tag"], "Ok");
        b
    } else {
        a.clone()
    };
    assert_eq!(freeze(&native, &home_id).await["outcome"]["_tag"], "Ok");
    assert_overlay(&app, &home_id).await;
    gate.release();
    let old = observing.await.unwrap();
    if completed {
        assert_eq!(
            old["outcome"]["payload"]["code"], "SessionCompleted",
            "{old}"
        );
    } else {
        assert_eq!(old["outcome"]["payload"]["active"]["launchId"], a, "{old}");
        assert_eq!(
            old["outcome"]["payload"]["active"]["phase"], "running",
            "{old}"
        );
    }
    assert_overlay(&app, &home_id).await;
}
#[tokio::test]
async fn metadata_held_running_status_preserves_newer_same_launch_home() {
    held_status_preserves_newer_home(false, false).await;
}
#[tokio::test]
async fn metadata_held_running_status_preserves_replacement_home() {
    held_status_preserves_newer_home(true, false).await;
}
#[tokio::test]
async fn metadata_held_terminal_status_preserves_replacement_home() {
    held_status_preserves_newer_home(true, true).await;
}

#[tokio::test]
async fn metadata_delayed_prepare_success_preserves_newer_native_home() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, _) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let _release = GateRelease(gate.clone());
    let (app, native) = native_routers(
        delay_session_reply(runtime, "app.session.prepare", &gate),
        root.path(),
    );
    let preparing_app = app.clone();
    let preparing = tokio::spawn(async move {
        rpc(
            &preparing_app,
            "app.session.prepare",
            json!({"gameId":"one"}),
        )
        .await
    });
    gate.entered().await;
    let accepted = rpc(&native, "app.session.status", json!({})).await;
    let id = accepted["outcome"]["payload"]["active"]["launchId"]
        .as_str()
        .unwrap();
    assert_eq!(freeze(&native, id).await["outcome"]["_tag"], "Ok");
    assert_overlay(&app, id).await;
    gate.release();
    let prepared = preparing.await.unwrap();
    assert_eq!(prepared["outcome"]["payload"]["launchId"], id, "{prepared}");
    assert_overlay(&app, id).await;
}

#[tokio::test]
async fn metadata_delayed_successful_return_preserves_newer_same_launch_home() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let _release = GateRelease(gate.clone());
    let runtime = runtime.with_compositor(focusable_compositor(), vec!["chromium-browser".into()]);
    let (app, native) = native_routers(
        delay_session_reply(runtime, "app.session.thaw", &gate),
        root.path(),
    );
    let id = reserve(&app).await;
    assert_eq!(start(&app, &id).await["outcome"]["_tag"], "Ok");
    backend.set_window_pids(&id, [9100].into());
    assert_eq!(freeze(&native, &id).await["outcome"]["_tag"], "Ok");
    let returning_app = app.clone();
    let returning_id = id.clone();
    let returning = tokio::spawn(async move { thaw(&returning_app, &returning_id).await });
    gate.entered().await;
    assert_eq!(freeze(&native, &id).await["outcome"]["_tag"], "Ok");
    assert_overlay(&app, &id).await;
    gate.release();
    assert_eq!(returning.await.unwrap()["outcome"]["_tag"], "Ok");
    assert_overlay(&app, &id).await;
}

#[tokio::test]
async fn metadata_delayed_native_home_cannot_overwrite_replacement_home() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, _) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let _release = GateRelease(gate.clone());
    let (app, native) = native_routers(
        delay_session_reply(runtime, "app.session.freeze", &gate),
        root.path(),
    );
    let a = reserve(&app).await;
    assert_eq!(start(&app, &a).await["outcome"]["_tag"], "Ok");
    let home_app = native.clone();
    let home_a = a.clone();
    let home = tokio::spawn(async move { freeze(&home_app, &home_a).await });
    gate.entered().await;
    assert_eq!(
        cancel(&app, &a).await["outcome"]["payload"]["phase"],
        "stopped"
    );
    let b = reserve(&app).await;
    assert_eq!(start(&app, &b).await["outcome"]["_tag"], "Ok");
    assert_eq!(freeze(&native, &b).await["outcome"]["_tag"], "Ok");
    assert_overlay(&app, &b).await;
    gate.release();
    assert_eq!(home.await.unwrap()["outcome"]["_tag"], "Ok");
    assert_overlay(&app, &b).await;
}

#[tokio::test]
async fn metadata_delayed_native_home_cannot_restore_intent_after_return_and_fullscreen_leave() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let _release = GateRelease(gate.clone());
    let runtime = runtime.with_compositor(focusable_compositor(), vec!["chromium-browser".into()]);
    let (app, native) = native_routers(
        delay_session_reply(runtime, "app.session.freeze", &gate),
        root.path(),
    );
    let id = reserve(&app).await;
    assert_eq!(start(&app, &id).await["outcome"]["_tag"], "Ok");
    backend.set_window_pids(&id, [9100].into());
    let home_app = native.clone();
    let home_id = id.clone();
    let home = tokio::spawn(async move { freeze(&home_app, &home_id).await });
    gate.entered().await;
    assert_eq!(thaw(&app, &id).await["outcome"]["_tag"], "Ok");
    assert_eq!(freeze(&app, &id).await["outcome"]["_tag"], "Ok");
    gate.release();
    assert_eq!(home.await.unwrap()["outcome"]["_tag"], "Ok");
    let status = rpc(&app, "app.session.status", json!({})).await;
    assert_eq!(
        status["outcome"]["payload"]["active"]["phase"], "frozen",
        "{status}"
    );
    assert!(
        status["outcome"]["payload"].get("overlay").is_none(),
        "fullscreen Leave is not native Home: {status}"
    );
}

async fn held_recovered_status_preserves_newer_control(replacement: bool) {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let initial = router(runtime, root.path(), TOKEN, PortalPermission::LocalSessions);
    let a = reserve(&initial).await;
    assert_eq!(start(&initial, &a).await["outcome"]["_tag"], "Ok");
    backend.set_window_pids(&a, [9100].into());
    let restarted = host::HostRuntime::from_paths_with_backend(
        &root.path().join("host.toml"),
        None,
        root.path().join("private"),
        backend.clone(),
    )
    .with_compositor(focusable_compositor(), vec!["chromium-browser".into()]);
    let gate = Arc::new(RouteGate::default());
    let _release = GateRelease(gate.clone());
    let (app, native) = native_routers(
        delay_session_reply(restarted, "app.session.status", &gate),
        root.path(),
    );
    let observing_app = app.clone();
    let observing =
        tokio::spawn(async move { rpc(&observing_app, "app.session.status", json!({})).await });
    gate.entered().await;
    let b = if replacement {
        assert_eq!(
            cancel(&app, &a).await["outcome"]["payload"]["phase"],
            "stopped"
        );
        let b = reserve(&app).await;
        assert_eq!(start(&app, &b).await["outcome"]["_tag"], "Ok");
        assert_eq!(freeze(&native, &b).await["outcome"]["_tag"], "Ok");
        assert_overlay(&app, &b).await;
        Some(b)
    } else {
        assert_eq!(freeze(&native, &a).await["outcome"]["_tag"], "Ok");
        assert_eq!(thaw(&app, &a).await["outcome"]["_tag"], "Ok");
        assert_eq!(freeze(&app, &a).await["outcome"]["_tag"], "Ok");
        None
    };
    gate.release();
    let old = observing.await.unwrap();
    assert_eq!(old["outcome"]["payload"]["active"]["launchId"], a, "{old}");
    assert_eq!(
        old["outcome"]["payload"]["active"]["phase"], "focus-failed",
        "{old}"
    );
    if let Some(b) = b {
        assert_overlay(&app, &b).await;
    } else {
        let status = rpc(&app, "app.session.status", json!({})).await;
        assert_eq!(
            status["outcome"]["payload"]["active"]["phase"], "frozen",
            "{status}"
        );
        assert!(
            status["outcome"]["payload"].get("overlay").is_none(),
            "recovery must not resurrect returned Home: {status}"
        );
    }
}
#[tokio::test]
async fn metadata_delayed_recovered_status_cannot_restore_returned_same_launch_home() {
    held_recovered_status_preserves_newer_control(false).await;
}
#[tokio::test]
async fn metadata_delayed_recovered_status_preserves_replacement_home() {
    held_recovered_status_preserves_newer_control(true).await;
}

async fn delayed_terminal_control_preserves_newly_started_exact_home(method: &'static str) {
    let root = tempfile::tempdir().unwrap();
    let (runtime, _) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let _release = GateRelease(gate.clone());
    let (app, native) = native_routers(delay_session_reply(runtime, method, &gate), root.path());
    let id = reserve(&app).await;
    let controlling_app = native.clone();
    let controlling_id = id.clone();
    let controlling = tokio::spawn(async move {
        rpc(
            &controlling_app,
            method,
            json!({"expectedLaunchId":controlling_id}),
        )
        .await
    });
    gate.entered().await;
    // The reserved ID can become active after a terminal control observation.
    assert_eq!(start(&app, &id).await["outcome"]["_tag"], "Ok");
    assert_eq!(freeze(&native, &id).await["outcome"]["_tag"], "Ok");
    assert_overlay(&app, &id).await;
    gate.release();
    let old = controlling.await.unwrap();
    assert_eq!(
        old["outcome"]["payload"]["code"], "NoActiveSession",
        "{old}"
    );
    assert_overlay(&app, &id).await;
}

#[tokio::test]
async fn metadata_delayed_terminal_freeze_keeps_new_home_for_newly_active_reserved_identity() {
    delayed_terminal_control_preserves_newly_started_exact_home("app.session.freeze").await;
}

#[tokio::test]
async fn metadata_delayed_terminal_thaw_keeps_new_home_for_newly_active_reserved_identity() {
    delayed_terminal_control_preserves_newly_started_exact_home("app.session.thaw").await;
}

#[tokio::test]
async fn metadata_delayed_terminal_stop_keeps_new_home_for_newly_active_reserved_identity() {
    delayed_terminal_control_preserves_newly_started_exact_home("app.session.stop").await;
}

#[tokio::test]
async fn metadata_late_successful_stop_helper_preserves_replacement() {
    late_stop_helper_preserves_replacement(false, "app.session.stop").await;
}

#[tokio::test]
async fn metadata_late_failed_stop_helper_preserves_replacement() {
    late_stop_helper_preserves_replacement(true, "app.session.stop").await;
}

#[tokio::test]
async fn metadata_terminal_status_cleans_home_without_retargeting_fullscreen_leave() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let (app, native) = native_routers(runtime, root.path());
    let a = reserve(&app).await;
    assert_eq!(start(&app, &a).await["outcome"]["_tag"], "Ok");
    assert_eq!(freeze(&native, &a).await["outcome"]["_tag"], "Ok");
    assert_overlay(&app, &a).await;
    backend.complete_live();
    let completed = rpc(&app, "app.session.status", json!({})).await;
    assert_eq!(
        completed["outcome"]["payload"]["code"], "SessionCompleted",
        "{completed}"
    );
    let b = reserve(&app).await;
    assert_eq!(start(&app, &b).await["outcome"]["_tag"], "Ok");
    assert_eq!(freeze(&app, &b).await["outcome"]["_tag"], "Ok");
    let status = rpc(&app, "app.session.status", json!({})).await;
    assert_eq!(
        status["outcome"]["payload"]["active"]["launchId"], b,
        "{status}"
    );
    assert!(
        status["outcome"]["payload"].get("overlay").is_none(),
        "{status}"
    );
}

#[tokio::test]
async fn pending_startup_status_public_protocol_probe() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let _release = GateRelease(gate.clone());
    let blocked = gate.clone();
    let app = router(
        runtime.with_reserved_route_probe(Arc::new(move || blocked.block())),
        root.path(),
        TOKEN,
        PortalPermission::LocalSessions,
    );
    let id = reserve(&app).await;
    let starting_app = app.clone();
    let starting_id = id.clone();
    let starting = tokio::spawn(async move { start(&starting_app, &starting_id).await });
    gate.entered().await;
    let fresh_portal_status = rpc(&app, "app.session.status", json!({})).await;
    println!("PRE-ACTIVE PENDING STARTUP STATUS: {fresh_portal_status}");
    assert_eq!(
        fresh_portal_status["outcome"]["_tag"], "Ok",
        "{fresh_portal_status}"
    );
    assert_eq!(
        fresh_portal_status["outcome"]["payload"]["pendingLaunches"][0]["session"]["launchId"], id,
        "{fresh_portal_status}"
    );
    assert_eq!(
        fresh_portal_status["outcome"]["payload"]["pendingLaunches"][0]["phase"], "preparing",
        "{fresh_portal_status}"
    );
    assert!(fresh_portal_status["outcome"]["payload"]
        .get("active")
        .is_none());
    assert_eq!(backend.launch_count(), 0);
    assert!(!root
        .path()
        .join("private/host-session/active.json")
        .exists());
    gate.release();
    assert_eq!(starting.await.unwrap()["outcome"]["_tag"], "Ok");
    let active = rpc(&app, "app.session.status", json!({})).await;
    println!("AFTER UNIT CREATION STATUS: {active}");
    assert_eq!(
        active["outcome"]["payload"]["active"]["launchId"], id,
        "{active}"
    );
}
