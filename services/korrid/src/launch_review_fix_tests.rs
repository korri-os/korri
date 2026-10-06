//! Reviewed contention and failed-start facts, through production RPC routers.
use super::*;

async fn short_watcher_hold(pending: bool) {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let compositor = focusable_compositor();
    let runtime = runtime.with_compositor(compositor.clone(), vec!["chromium-browser".into()]);
    let (app, native) = native_routers(runtime.clone(), root.path());
    let active = reserve(&app).await;
    assert_eq!(start(&app, &active).await["outcome"]["_tag"], "Ok");
    backend.set_window_pids(&active, [9100].into());
    let pending = if pending {
        Some(reserve(&app).await)
    } else {
        None
    };
    let gate = Arc::new(RouteGate::default());
    let _release = GateRelease(gate.clone());
    *compositor.gate.lock().unwrap() = Some(gate.clone());
    // The actual production watcher acquires transition authority before
    // querying this configured compositor, exactly like an ordinary tick.
    runtime.spawn_portal_watch();
    gate.entered().await;
    let mut portal_status = tokio::spawn(async move { status(&app).await });
    let mut native_status = tokio::spawn(async move { status(&native).await });
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut portal_status)
            .await
            .is_err(),
        "ordinary watcher contention must not immediately become HostSessionBusy"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut native_status)
            .await
            .is_err(),
        "native readers retain blocking observation even with foreign pending work"
    );
    gate.release();
    let portal_status = tokio::time::timeout(Duration::from_secs(2), portal_status)
        .await
        .unwrap()
        .unwrap();
    let native_status = tokio::time::timeout(Duration::from_secs(2), native_status)
        .await
        .unwrap()
        .unwrap();
    for observed in [&portal_status, &native_status] {
        assert_eq!(observed["outcome"]["_tag"], "Ok", "{observed}");
        assert_eq!(
            observed["outcome"]["payload"]["active"]["launchId"], active,
            "{observed}"
        );
        assert!(
            observed["outcome"]["payload"]
                .get("observationFailure")
                .is_none(),
            "{observed}"
        );
    }
    assert!(native_status["outcome"]["payload"]
        .get("pendingLaunches")
        .is_none());
    if let Some(pending) = pending {
        assert_eq!(pending_ids(&portal_status), BTreeSet::from([pending]));
    } else {
        assert!(portal_status["outcome"]["payload"]
            .get("pendingLaunches")
            .is_none());
    }
}

#[tokio::test]
async fn non_pending_status_waits_for_an_ordinary_watcher_hold() {
    short_watcher_hold(false).await;
}

#[tokio::test]
async fn owned_pending_status_absorbs_an_ordinary_short_watcher_hold() {
    short_watcher_hold(true).await;
}

#[tokio::test]
async fn input_transition_failed_start_retains_this_daemons_handoff_history() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let portal = Arc::new(host::control::RecordingPortalUnit::left_frozen());
    portal.refuse_next_thaws(1);
    let (app, _) = native_routers(runtime.with_portal_unit(portal.clone()), root.path());
    let id = reserve(&app).await;
    let failed = start(&app, &id).await;
    assert_eq!(
        failed["outcome"]["payload"]["code"], "InputTransitionFailed",
        "{failed}"
    );
    assert!(portal.requests().contains(&"thaw"));
    assert_eq!(backend.launch_count(), 1);
    let live = assert_handoff(&app, &id, "waiting").await;
    assert_eq!(
        live["outcome"]["payload"]["active"]["phase"],
        "focus-failed"
    );
    assert!(live["outcome"]["payload"].get("pendingLaunches").is_none());
    assert_eq!(
        cancel(&app, &id).await["outcome"]["payload"]["phase"],
        "stopped"
    );
    assert_eq!(
        status(&app).await["outcome"]["payload"]["code"],
        "SessionCompleted"
    );
    let replacement = reserve(&app).await;
    assert_eq!(start(&app, &replacement).await["outcome"]["_tag"], "Ok");
    assert_handoff(&app, &replacement, "waiting").await;
}

#[tokio::test]
async fn already_stopping_start_retains_this_daemons_handoff_history() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    backend.set_launch_stopping();
    let (app, _) = native_routers(runtime, root.path());
    let id = reserve(&app).await;
    let failed = start(&app, &id).await;
    assert_eq!(
        failed["outcome"]["payload"]["code"], "HostLaunchFailed",
        "{failed}"
    );
    assert_eq!(backend.launch_count(), 1);
    let live = assert_handoff(&app, &id, "waiting").await;
    assert_eq!(live["outcome"]["payload"]["active"]["phase"], "stopping");
    assert!(live["outcome"]["payload"].get("pendingLaunches").is_none());
    backend.complete_live();
    assert_eq!(
        status(&app).await["outcome"]["payload"]["code"],
        "SessionCompleted"
    );
    backend.set_launch_completed(false);
    let replacement = reserve(&app).await;
    assert_eq!(start(&app, &replacement).await["outcome"]["_tag"], "Ok");
    assert_handoff(&app, &replacement, "waiting").await;
}
