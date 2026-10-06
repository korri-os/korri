//! Daemon-owned startup facts exercised through the production protocols.
use super::*;
use std::collections::BTreeSet;

#[path = "launch_review_fix_tests.rs"]
mod review_fix;

async fn status(app: &Router) -> Value {
    rpc(app, "app.session.status", json!({})).await
}
async fn foreign_status(app: &Router) -> Value {
    let (status, value) = call(app, "foreign-capability", "app.session.status", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}
fn pending_ids(value: &Value) -> BTreeSet<String> {
    value["outcome"]["payload"]["pendingLaunches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pending| pending["session"]["launchId"].as_str().unwrap().into())
        .collect()
}
fn pending_phase(value: &Value, id: &str) -> String {
    value["outcome"]["payload"]["pendingLaunches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pending| pending["session"]["launchId"] == id)
        .unwrap()["phase"]
        .as_str()
        .unwrap()
        .into()
}
async fn assert_handoff(app: &Router, id: &str, expected: &str) -> Value {
    let value = status(app).await;
    assert_eq!(
        value["outcome"]["payload"]["active"]["launchId"], id,
        "{value}"
    );
    assert_eq!(
        value["outcome"]["payload"]["active"]["initialHandoff"], expected,
        "{value}"
    );
    value
}

#[tokio::test]
async fn daemon_pending_projection_preserves_all_owned_reservations_and_hides_foreign_callers() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let (app, native) = native_routers(runtime.clone(), root.path());
    let foreign = router(
        runtime.clone(),
        root.path(),
        "foreign-capability",
        PortalPermission::LocalSessions,
    );
    let readonly = router(runtime, root.path(), TOKEN, PortalPermission::ReadOnly);
    let a = reserve(&app).await;
    let b = reserve(&app).await;
    let own = status(&app).await;
    assert_eq!(own["outcome"]["_tag"], "Ok", "{own}");
    assert_eq!(pending_ids(&own), BTreeSet::from([a.clone(), b.clone()]));
    assert_eq!(pending_phase(&own, &a), "reserved");
    assert_eq!(pending_phase(&own, &b), "reserved");
    assert!(own["outcome"]["payload"].get("active").is_none());
    assert_eq!(pending_ids(&status(&readonly).await), pending_ids(&own));
    for hidden in [foreign_status(&foreign).await, status(&native).await] {
        assert_eq!(
            hidden["outcome"]["payload"]["code"], "NoActiveSession",
            "{hidden}"
        );
        assert!(hidden["outcome"]["payload"]
            .get("pendingLaunches")
            .is_none());
    }
    let (denied_status, _) = call(&app, "wrong-capability", "app.session.status", json!({})).await;
    assert_eq!(denied_status, StatusCode::UNAUTHORIZED);
    let denied = call(
        &foreign,
        "foreign-capability",
        "app.session.cancel",
        json!({"expectedLaunchId":a}),
    )
    .await
    .1;
    assert_eq!(denied["outcome"]["payload"]["code"], "AuthorizationDenied");
    assert_eq!(
        cancel(&app, &a).await["outcome"]["payload"]["phase"],
        "stopped"
    );
    assert_eq!(
        pending_ids(&status(&app).await),
        BTreeSet::from([b.clone()])
    );
    assert_eq!(
        cancel(&app, &b).await["outcome"]["payload"]["phase"],
        "stopped"
    );
    assert_eq!(
        status(&app).await["outcome"]["payload"]["code"],
        "NoActiveSession"
    );
    assert_eq!(backend.launch_count(), 0);
    assert!(!own.to_string().contains(TOKEN));
    assert!(!own.to_string().contains("foreign-capability"));
}

#[tokio::test]
async fn daemon_preparing_survives_lost_start_request_and_is_reload_observable_without_relaunch() {
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
    starting.abort();
    let _ = starting.await;
    let fresh = status(&app).await;
    assert_eq!(fresh["outcome"]["_tag"], "Ok", "{fresh}");
    assert_eq!(pending_ids(&fresh), BTreeSet::from([id.clone()]));
    assert_eq!(pending_phase(&fresh, &id), "preparing");
    assert_eq!(backend.launch_count(), 0);
    assert!(!root
        .path()
        .join("private/host-session/active.json")
        .exists());
    gate.release();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let observed = status(&app).await;
            if observed["outcome"]["payload"]["active"]["launchId"] == id {
                assert_eq!(
                    observed["outcome"]["payload"]["active"]["initialHandoff"],
                    "waiting"
                );
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(backend.launch_count(), 1);
}

#[tokio::test]
async fn daemon_committing_and_cancelling_are_prompt_unknown_observations_and_do_not_expose_foreign_reservations(
) {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let _release = GateRelease(gate.clone());
    let blocked = gate.clone();
    backend.set_launch_probe(Arc::new(move || blocked.block()));
    let app = router(
        runtime.clone(),
        root.path(),
        TOKEN,
        PortalPermission::LocalSessions,
    );
    let foreign = router(
        runtime,
        root.path(),
        "foreign-capability",
        PortalPermission::LocalSessions,
    );
    let id = reserve(&app).await;
    let starting_app = app.clone();
    let starting_id = id.clone();
    let starting = tokio::spawn(async move { start(&starting_app, &starting_id).await });
    gate.entered().await;
    let busy = tokio::time::timeout(Duration::from_secs(2), status(&app)).await;
    if busy.is_err() {
        gate.release();
    }
    let busy = busy.expect("owned pending status must not wait for native committing effects");
    assert_eq!(pending_phase(&busy, &id), "committing");
    assert_eq!(
        busy["outcome"]["payload"]["observationFailure"]["code"], "HostSessionBusy",
        "{busy}"
    );
    assert!(busy["outcome"]["payload"].get("active").is_none());
    let mut foreign_observation = tokio::spawn(async move { foreign_status(&foreign).await });
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut foreign_observation)
            .await
            .is_err(),
        "a caller with no owned pending launch waits for native observation"
    );
    assert_eq!(
        cancel(&app, &id).await["outcome"]["payload"]["phase"],
        "pending"
    );
    let cancelling = status(&app).await;
    assert_eq!(pending_phase(&cancelling, &id), "cancelling");
    assert_eq!(
        cancelling["outcome"]["payload"]["observationFailure"]["code"],
        "HostSessionBusy"
    );
    gate.release();
    assert_eq!(
        starting.await.unwrap()["outcome"]["payload"]["code"],
        "LaunchCancelled"
    );
    let hidden = tokio::time::timeout(Duration::from_secs(2), foreign_observation)
        .await
        .unwrap()
        .unwrap();
    assert!(
        hidden["outcome"]["_tag"] == "Ok"
            || matches!(
                hidden["outcome"]["payload"]["code"].as_str(),
                Some("NoActiveSession" | "SessionCompleted")
            ),
        "{hidden}"
    );
    assert!(hidden["outcome"]["payload"]
        .get("pendingLaunches")
        .is_none());
    assert!(hidden["outcome"]["payload"]
        .get("observationFailure")
        .is_none());
    let retired = status(&app).await;
    assert!(retired["outcome"]["payload"]
        .get("pendingLaunches")
        .is_none());
    let replacement = reserve(&app).await;
    assert_eq!(start(&app, &replacement).await["outcome"]["_tag"], "Ok");
    assert_handoff(&app, &replacement, "waiting").await;
    assert_eq!(
        cancel(&app, &id).await["outcome"]["payload"]["code"],
        "StaleLaunchIdentity"
    );
}

#[tokio::test]
async fn daemon_initial_handoff_is_exact_latched_and_not_recreated_by_focus_loss_or_replacement() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let compositor = focusable_compositor();
    let (app, native) = native_routers(
        runtime.with_compositor(compositor.clone(), vec!["chromium-browser".into()]),
        root.path(),
    );
    let a = reserve(&app).await;
    assert_eq!(start(&app, &a).await["outcome"]["_tag"], "Ok");
    assert_handoff(&app, &a, "waiting").await;
    backend.set_window_pids(&a, [9100].into());
    *compositor.tree.lock().unwrap() = json!(null);
    assert_handoff(&app, &a, "waiting").await;
    *compositor.tree.lock().unwrap() = json!({"id":1,"nodes":[{"id":3,"pid":9100,"focused":false},{"id":9,"pid":9900,"focused":true}]});
    assert_handoff(&app, &a, "waiting").await;
    *compositor.tree.lock().unwrap() = json!({"id":1,"nodes":[{"id":3,"pid":9100,"focused":true},{"id":4,"pid":9100,"focused":false}]});
    assert_handoff(&app, &a, "waiting").await;
    *compositor.tree.lock().unwrap() = json!({"id":1,"nodes":[{"id":3,"pid":9100,"focused":true}]});
    assert_handoff(&app, &a, "observed").await;
    *compositor.tree.lock().unwrap() = json!({"id":1,"nodes":[{"id":3,"pid":9100,"focused":false},{"id":2,"pid":4100,"app_id":"chromium-browser","focused":true}]});
    let lost = assert_handoff(&app, &a, "observed").await;
    assert_eq!(
        lost["outcome"]["payload"]["active"]["focusOwnership"],
        "excluded"
    );
    *compositor.tree.lock().unwrap() = json!({"id":1,"nodes":[{"id":3,"pid":9100,"focused":false},{"id":9,"pid":9900,"focused":true}]});
    let other = assert_handoff(&app, &a, "observed").await;
    assert_eq!(
        other["outcome"]["payload"]["active"]["focusOwnership"],
        "other"
    );
    *compositor.tree.lock().unwrap() = json!(null);
    let unavailable = assert_handoff(&app, &a, "observed").await;
    assert!(unavailable["outcome"]["payload"]["active"]
        .get("focusOwnership")
        .is_none());
    assert_eq!(freeze(&native, &a).await["outcome"]["_tag"], "Ok");
    let frozen = assert_handoff(&app, &a, "observed").await;
    assert_eq!(frozen["outcome"]["payload"]["active"]["phase"], "frozen");
    assert_overlay(&app, &a).await;
    backend.complete_live();
    assert_eq!(
        status(&app).await["outcome"]["payload"]["code"],
        "SessionCompleted"
    );
    let b = reserve(&app).await;
    assert_eq!(start(&app, &b).await["outcome"]["_tag"], "Ok");
    assert_handoff(&app, &b, "waiting").await;
    assert_eq!(
        cancel(&app, &a).await["outcome"]["payload"]["code"],
        "StaleLaunchIdentity"
    );
    assert_handoff(&app, &b, "waiting").await;
}

#[tokio::test]
async fn daemon_return_focus_failure_and_freeze_do_not_manufacture_an_initial_handoff() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, _) = runtime(root.path());
    let (app, native) = native_routers(
        runtime.with_compositor(focusable_compositor(), vec!["chromium-browser".into()]),
        root.path(),
    );
    let id = reserve(&app).await;
    assert_eq!(start(&app, &id).await["outcome"]["_tag"], "Ok");
    assert_eq!(freeze(&native, &id).await["outcome"]["_tag"], "Ok");
    assert_handoff(&app, &id, "waiting").await;
    let returned = thaw(&app, &id).await;
    assert_eq!(
        returned["outcome"]["payload"]["code"], "HostFocusFailed",
        "{returned}"
    );
    let failed = assert_handoff(&app, &id, "waiting").await;
    assert_eq!(
        failed["outcome"]["payload"]["active"]["phase"],
        "focus-failed"
    );
    assert_overlay(&app, &id).await;
}

#[tokio::test]
async fn daemon_reconstructed_live_units_are_not_fabricated_new_initial_startups() {
    for frozen in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (runtime, backend) = runtime(root.path());
        let (initial, native) = native_routers(runtime, root.path());
        let id = reserve(&initial).await;
        assert_eq!(start(&initial, &id).await["outcome"]["_tag"], "Ok");
        if frozen {
            assert_eq!(freeze(&native, &id).await["outcome"]["_tag"], "Ok");
        }
        backend.set_window_pids(&id, [9100].into());
        let restarted = host::HostRuntime::from_paths_with_backend(
            &root.path().join("host.toml"),
            None,
            root.path().join("private"),
            backend,
        )
        .with_compositor(focusable_compositor(), vec!["chromium-browser".into()]);
        let app = router(
            restarted,
            root.path(),
            TOKEN,
            PortalPermission::LocalSessions,
        );
        let recovered = assert_handoff(&app, &id, "recovered").await;
        assert_eq!(
            recovered["outcome"]["payload"]["active"]["phase"],
            if frozen { "frozen" } else { "focus-failed" }
        );
        assert!(recovered["outcome"]["payload"]
            .get("pendingLaunches")
            .is_none());
    }
}

#[tokio::test]
async fn daemon_pending_projection_preserves_genuine_observation_failure_and_home_intent() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let compositor = focusable_compositor();
    *compositor.tree.lock().unwrap() = json!({"id":1,"nodes":[{"id":3,"pid":9100,"focused":true}]});
    let (app, native) = native_routers(
        runtime.clone().with_compositor(compositor, vec![]),
        root.path(),
    );
    let foreign = router(
        runtime,
        root.path(),
        "foreign-capability",
        PortalPermission::LocalSessions,
    );
    let a = reserve(&app).await;
    backend.set_window_pids(&a, [9100].into());
    assert_eq!(start(&app, &a).await["outcome"]["_tag"], "Ok");
    assert_eq!(freeze(&native, &a).await["outcome"]["_tag"], "Ok");
    assert_overlay(&app, &a).await;
    let b = reserve(&app).await;
    backend.set_state_unavailable(true);
    let unavailable = status(&app).await;
    assert_eq!(pending_ids(&unavailable), BTreeSet::from([b.clone()]));
    assert_eq!(
        unavailable["outcome"]["payload"]["observationFailure"]["code"], "HostRecoveryBlocked",
        "{unavailable}"
    );
    assert!(unavailable["outcome"]["payload"].get("active").is_none());
    assert!(unavailable["outcome"]["payload"].get("overlay").is_none());
    let hidden = foreign_status(&foreign).await;
    assert_eq!(
        hidden["outcome"]["payload"]["code"], "HostRecoveryBlocked",
        "{hidden}"
    );
    assert!(hidden["outcome"]["payload"]
        .get("pendingLaunches")
        .is_none());
    backend.set_state_unavailable(false);
    assert_overlay(&app, &a).await;
    assert_handoff(&app, &a, "observed").await;
    assert_eq!(
        cancel(&app, &b).await["outcome"]["payload"]["phase"],
        "stopped"
    );
    assert_overlay(&app, &a).await;
}

#[tokio::test]
async fn daemon_busy_pending_projection_does_not_clear_home_during_a_failed_return() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let compositor = focusable_compositor();
    *compositor.tree.lock().unwrap() = json!({"id":1,"nodes":[{"id":3,"pid":9100,"focused":true}]});
    let (app, native) = native_routers(
        runtime.with_compositor(compositor.clone(), vec!["chromium-browser".into()]),
        root.path(),
    );
    let a = reserve(&app).await;
    backend.set_window_pids(&a, [9100].into());
    assert_eq!(start(&app, &a).await["outcome"]["_tag"], "Ok");
    assert_eq!(freeze(&native, &a).await["outcome"]["_tag"], "Ok");
    *compositor.tree.lock().unwrap() =
        json!({"id":1,"nodes":[{"id":2,"pid":4100,"app_id":"chromium-browser","focused":true}]});
    assert_overlay(&app, &a).await;
    let b = reserve(&app).await;
    let gate = Arc::new(RouteGate::default());
    let _release = GateRelease(gate.clone());
    *compositor.gate.lock().unwrap() = Some(gate.clone());
    let returning_app = app.clone();
    let returning_a = a.clone();
    let returning = tokio::spawn(async move { thaw(&returning_app, &returning_a).await });
    gate.entered().await;
    let busy = tokio::time::timeout(Duration::from_secs(2), status(&app)).await;
    if busy.is_err() {
        gate.release();
    }
    let busy = busy.expect("pending identities must not block behind native Return");
    assert_eq!(pending_ids(&busy), BTreeSet::from([b.clone()]));
    assert_eq!(
        busy["outcome"]["payload"]["observationFailure"]["code"], "HostSessionBusy",
        "{busy}"
    );
    gate.release();
    assert_eq!(
        returning.await.unwrap()["outcome"]["payload"]["code"],
        "HostFocusFailed"
    );
    assert_overlay(&app, &a).await;
    assert_handoff(&app, &a, "observed").await;
    assert_eq!(
        cancel(&app, &b).await["outcome"]["payload"]["phase"],
        "stopped"
    );
    assert_overlay(&app, &a).await;
}

#[tokio::test]
async fn daemon_observes_handoff_without_a_browser_poll_and_retains_it_after_later_focus_loss() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let compositor = focusable_compositor();
    *compositor.tree.lock().unwrap() = json!({"id":1,"nodes":[{"id":3,"pid":9100,"focused":true}]});
    let (app, _) = native_routers(
        runtime.with_compositor(compositor.clone(), vec!["chromium-browser".into()]),
        root.path(),
    );
    let id = reserve(&app).await;
    backend.set_window_pids(&id, [9100].into());
    assert_eq!(start(&app, &id).await["outcome"]["_tag"], "Ok");
    // The native prepare/freezer reconciliation already observed exact focus.
    // A fresh browser sees the history even though its first poll sees loss.
    *compositor.tree.lock().unwrap() = json!({"id":1,"nodes":[{"id":3,"pid":9100,"focused":false},{"id":2,"pid":4100,"app_id":"chromium-browser","focused":true}]});
    let lost = assert_handoff(&app, &id, "observed").await;
    assert_eq!(
        lost["outcome"]["payload"]["active"]["focusOwnership"],
        "excluded"
    );
}

#[tokio::test]
async fn daemon_confirmed_early_exit_retires_pending_identity_before_delayed_ack_and_never_retargets_replacement(
) {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let _release = GateRelease(gate.clone());
    let blocked = gate.clone();
    let runtime = runtime.with_reserved_start_probe(Arc::new(move || {
        if !blocked.entered.swap(true, Ordering::SeqCst) {
            blocked.block();
        }
    }));
    let app = router(runtime, root.path(), TOKEN, PortalPermission::LocalSessions);
    let a = reserve(&app).await;
    let starting_app = app.clone();
    let starting_a = a.clone();
    let starting = tokio::spawn(async move { start(&starting_app, &starting_a).await });
    gate.entered().await;
    backend.complete_live();
    let completed = status(&app).await;
    assert_eq!(
        completed["outcome"]["payload"]["code"], "SessionCompleted",
        "{completed}"
    );
    assert!(completed["outcome"]["payload"]
        .get("pendingLaunches")
        .is_none());
    let b = reserve(&app).await;
    assert_eq!(start(&app, &b).await["outcome"]["_tag"], "Ok");
    assert_handoff(&app, &b, "waiting").await;
    gate.release();
    let late = starting.await.unwrap();
    assert_eq!(late["outcome"]["payload"]["session"]["launchId"], a);
    assert_handoff(&app, &b, "waiting").await;
    assert!(status(&app).await["outcome"]["payload"]
        .get("pendingLaunches")
        .is_none());
}

#[tokio::test]
async fn daemon_exit_before_native_prepare_finishes_reports_failure_without_a_handoff_or_pending_resurrection(
) {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    backend.set_launch_completed(true);
    let app = router(runtime, root.path(), TOKEN, PortalPermission::LocalSessions);
    let a = reserve(&app).await;
    let failed = start(&app, &a).await;
    assert_eq!(
        failed["outcome"]["payload"]["code"], "HostLaunchFailed",
        "{failed}"
    );
    let retired = status(&app).await;
    assert_eq!(
        retired["outcome"]["payload"]["code"], "NoActiveSession",
        "{retired}"
    );
    assert!(!root
        .path()
        .join("private/host-session/active.json")
        .exists());
    backend.set_launch_completed(false);
    let b = reserve(&app).await;
    assert_eq!(start(&app, &b).await["outcome"]["_tag"], "Ok");
    assert_handoff(&app, &b, "waiting").await;
}

#[tokio::test]
async fn daemon_failed_route_retires_only_its_owned_pending_launch() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let app = router(runtime, root.path(), TOKEN, PortalPermission::LocalSessions);
    let a = reserve(&app).await;
    let b = reserve(&app).await;
    let failed = rpc(
        &app,
        "app.session.start",
        json!({"gameId":"one","expectedLaunchId":a,"runnerId":"@korri:mgba/mgba"}),
    )
    .await;
    assert_eq!(failed["outcome"]["_tag"], "Err");
    assert_eq!(
        pending_ids(&status(&app).await),
        BTreeSet::from([b.clone()])
    );
    assert_eq!(start(&app, &b).await["outcome"]["_tag"], "Ok");
    assert_handoff(&app, &b, "waiting").await;
    assert_eq!(backend.launch_count(), 1);
}
