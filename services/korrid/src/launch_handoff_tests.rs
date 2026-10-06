//! Regressions through the capability-bound production RPC boundary.
use super::*;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Condvar,
};
use std::time::Duration;
use tower::ServiceExt;

#[path = "launch_handoff_metadata_tests.rs"]
mod metadata;

const TOKEN: &str = "handoff-test-capability";
const ORIGIN: &str = "http://127.0.0.1:8099";

async fn call(app: &Router, token: &str, method: &str, payload: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rpc")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::ORIGIN, ORIGIN)
                .body(Body::from(
                    json!({"_tag":method,"payload":payload}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}
async fn rpc(app: &Router, method: &str, payload: Value) -> Value {
    let (status, value) = call(app, TOKEN, method, payload).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}
fn runtime(
    root: &Path,
) -> (
    host::HostRuntime,
    Arc<host::control::InMemoryLaunchUnitBackend>,
) {
    let config = root.join("host.toml");
    std::fs::write(
        &config,
        "label = \"handoff\"\n[[games]]\nid = \"one\"\ntitle = \"One\"\ncommand = [\"game\"]\n",
    )
    .unwrap();
    let backend = Arc::new(host::control::InMemoryLaunchUnitBackend::default());
    (
        host::HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.join("private"),
            backend.clone(),
        ),
        backend,
    )
}
fn router(
    runtime: host::HostRuntime,
    root: &Path,
    token: &str,
    permission: PortalPermission,
) -> Router {
    secure_host_routers(
        runtime,
        &root.join("private"),
        Some(PortalAccess::new(token, ORIGIN, permission)),
    )
    .0
}
async fn reserve(app: &Router) -> String {
    let value = rpc(app, "app.session.reserve", json!({"gameId":"one"})).await;
    assert_eq!(value["outcome"]["_tag"], "Ok", "{value}");
    value["outcome"]["payload"]["launchId"]
        .as_str()
        .unwrap()
        .into()
}
async fn start(app: &Router, id: &str) -> Value {
    rpc(
        app,
        "app.session.start",
        json!({"gameId":"one","expectedLaunchId":id}),
    )
    .await
}
async fn cancel(app: &Router, id: &str) -> Value {
    rpc(app, "app.session.cancel", json!({"expectedLaunchId":id})).await
}

#[tokio::test]
async fn reserved_selected_and_stored_routes_keep_exact_identity_overrides_and_warnings() {
    let root = tempfile::tempdir().unwrap();
    // The existing GBA producer fixture supplies the real library and plugin schema.
    crate::config::test_fixtures::gba(root.path());
    std::fs::create_dir(root.path().join("roms")).unwrap();
    std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let registry = crate::plugin_test_fixtures::installed(root.path());
    let config = root.path().join("host.toml");
    std::fs::write(&config, "label = \"handoff-routes\"\ngames = []\n").unwrap();
    let backend = Arc::new(host::control::InMemoryLaunchUnitBackend::default());
    let runtime = host::HostRuntime::from_paths_with_backend(
        &config,
        Some(root.path().into()),
        root.path().join("private"),
        backend.clone(),
    )
    .with_route_registry(root.path().into(), registry);
    let app = router(runtime, root.path(), TOKEN, PortalPermission::LocalSessions);
    let game_id = crate::config::test_fixtures::GBA_ID;
    for runner in [Some("@korri:mgba/mgba"), None] {
        let identity = rpc(&app, "app.session.reserve", json!({"gameId":game_id})).await;
        let launch_id = identity["outcome"]["payload"]["launchId"].as_str().unwrap();
        let launch = rpc(
            &app,
            "app.session.start",
            json!({
                "gameId":game_id,"expectedLaunchId":launch_id,"runnerId":runner,
                "overrides":{"settings":{"video_vsync":false,"absent_key":1}}
            }),
        )
        .await;
        assert_eq!(launch["outcome"]["_tag"], "Ok", "{launch}");
        assert_eq!(
            launch["outcome"]["payload"]["session"],
            identity["outcome"]["payload"]
        );
        assert_eq!(
            launch["outcome"]["payload"]["warnings"][0]["setting"],
            "absent_key"
        );
        assert_eq!(
            cancel(&app, launch_id).await["outcome"]["payload"]["phase"],
            "stopped"
        );
    }
    assert_eq!(backend.launch_count(), 2);
}

#[tokio::test]
async fn reserve_cancel_before_start_cannot_spawn_or_write_the_journal() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let app = router(runtime, root.path(), TOKEN, PortalPermission::LocalSessions);
    let id = reserve(&app).await;
    assert!(!root
        .path()
        .join("private/host-session/active.json")
        .exists());
    let cancelled = cancel(&app, &id).await;
    assert_eq!(
        cancelled["outcome"]["payload"]["phase"], "stopped",
        "{cancelled}"
    );
    let late = start(&app, &id).await;
    assert_eq!(
        late["outcome"]["payload"]["code"], "StaleLaunchIdentity",
        "{late}"
    );
    assert_eq!(backend.launch_count(), 0);
    assert!(!root
        .path()
        .join("private/host-session/active.json")
        .exists());
}

#[tokio::test]
async fn cancel_live_exact_launch_and_stale_cancel_never_end_a_replacement() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let app = router(runtime, root.path(), TOKEN, PortalPermission::LocalSessions);
    let a = reserve(&app).await;
    let launched = start(&app, &a).await;
    assert_eq!(launched["outcome"]["payload"]["session"]["launchId"], a);
    assert_eq!(launched["outcome"]["payload"]["warnings"], json!([]));
    assert_eq!(
        cancel(&app, &a).await["outcome"]["payload"]["phase"],
        "stopped"
    );
    let b = reserve(&app).await;
    assert_eq!(start(&app, &b).await["outcome"]["_tag"], "Ok");
    let late = cancel(&app, &a).await;
    assert_eq!(late["outcome"]["payload"]["code"], "StaleLaunchIdentity");
    assert_eq!(
        start(&app, &a).await["outcome"]["payload"]["code"],
        "StaleLaunchIdentity"
    );
    let status = rpc(&app, "app.session.status", json!({})).await;
    assert_eq!(status["outcome"]["payload"]["active"]["launchId"], b);
    assert_eq!(backend.launch_count(), 2);
}

#[derive(Debug, Default)]
struct RouteGate {
    entered: AtomicBool,
    released: Mutex<bool>,
    wake: Condvar,
}
impl RouteGate {
    fn block(&self) {
        self.entered.store(true, Ordering::SeqCst);
        let mut released = self.released.lock().unwrap();
        while !*released {
            released = self.wake.wait(released).unwrap();
        }
    }
    async fn entered(&self) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !self.entered.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.wake.notify_all();
    }
}

#[tokio::test]
async fn cancel_does_not_wait_for_delayed_route_preparation_and_late_start_cannot_spawn() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let blocked = gate.clone();
    let app = router(
        runtime.with_reserved_route_probe(Arc::new(move || {
            if !blocked.entered.load(Ordering::SeqCst) {
                blocked.block();
            }
        })),
        root.path(),
        TOKEN,
        PortalPermission::LocalSessions,
    );
    let id = reserve(&app).await;
    let starting_app = app.clone();
    let starting_id = id.clone();
    let starting = tokio::spawn(async move { start(&starting_app, &starting_id).await });
    gate.entered().await;
    let cancellation = tokio::time::timeout(Duration::from_secs(2), cancel(&app, &id)).await;
    if cancellation.is_err() {
        gate.release();
    }
    let cancellation = cancellation.expect("Cancel must not wait for plugin preparation");
    // A replacement may start while the cancelled preparation is still blocked.
    let replacement = reserve(&app).await;
    let replacement_start =
        tokio::time::timeout(Duration::from_secs(2), start(&app, &replacement)).await;
    // Release even when an assertion fails, so a regression cannot hang shutdown.
    gate.release();
    assert_eq!(cancellation["outcome"]["payload"]["phase"], "stopped");
    assert_eq!(replacement_start.unwrap()["outcome"]["_tag"], "Ok");
    assert_eq!(
        starting.await.unwrap()["outcome"]["payload"]["code"],
        "LaunchCancelled"
    );
    assert_eq!(backend.launch_count(), 1);
    assert_eq!(
        rpc(&app, "app.session.status", json!({})).await["outcome"]["payload"]["active"]
            ["launchId"],
        replacement
    );
}

#[tokio::test]
async fn cancel_during_launch_effects_reports_pending_then_ends_only_that_launch() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let blocked = gate.clone();
    backend.set_launch_probe(Arc::new(move || blocked.block()));
    let app = router(runtime, root.path(), TOKEN, PortalPermission::LocalSessions);
    let id = reserve(&app).await;
    let starting_app = app.clone();
    let starting_id = id.clone();
    let starting = tokio::spawn(async move { start(&starting_app, &starting_id).await });
    gate.entered().await;
    let cancellation = tokio::time::timeout(Duration::from_secs(2), cancel(&app, &id)).await;
    gate.release();
    let cancellation = cancellation
        .expect("committing cancellation must report Pending without waiting for effects");
    assert_eq!(
        cancellation["outcome"]["payload"]["phase"], "pending",
        "{cancellation}"
    );
    assert_eq!(
        starting.await.unwrap()["outcome"]["payload"]["code"],
        "LaunchCancelled"
    );
    assert_eq!(
        cancel(&app, &id).await["outcome"]["payload"]["phase"],
        "stopped"
    );
    let replacement = reserve(&app).await;
    assert_eq!(start(&app, &replacement).await["outcome"]["_tag"], "Ok");
    assert_eq!(
        cancel(&app, &id).await["outcome"]["payload"]["code"],
        "StaleLaunchIdentity"
    );
    assert_eq!(
        rpc(&app, "app.session.status", json!({})).await["outcome"]["payload"]["active"]
            ["launchId"],
        replacement
    );
    assert_eq!(backend.launch_count(), 2);
}

#[tokio::test]
async fn reservations_are_bound_to_capability_and_permissions_do_not_expand() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let app = router(
        runtime.clone(),
        root.path(),
        TOKEN,
        PortalPermission::LocalSessions,
    );
    let other = router(
        runtime.clone(),
        root.path(),
        "other-test-capability",
        PortalPermission::LocalSessions,
    );
    let readonly = router(runtime, root.path(), TOKEN, PortalPermission::ReadOnly);
    let id = reserve(&app).await;
    for (method, payload) in [
        ("app.session.reserve", json!({"gameId":"one"})),
        (
            "app.session.start",
            json!({"gameId":"one","expectedLaunchId":id}),
        ),
        ("app.session.cancel", json!({"expectedLaunchId":id})),
    ] {
        assert_eq!(
            call(&app, "wrong-test-capability", method, payload.clone())
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call(&readonly, TOKEN, method, payload).await.0,
            StatusCode::FORBIDDEN
        );
    }
    for (method, payload) in [
        (
            "app.session.start",
            json!({"gameId":"one","expectedLaunchId":id}),
        ),
        ("app.session.cancel", json!({"expectedLaunchId":id})),
    ] {
        let denied = call(&other, "other-test-capability", method, payload)
            .await
            .1;
        assert_eq!(
            denied["outcome"]["payload"]["code"], "AuthorizationDenied",
            "{denied}"
        );
    }
    assert_eq!(start(&app, &id).await["outcome"]["_tag"], "Ok");
    assert_eq!(backend.launch_count(), 1);
}

#[derive(Debug)]
struct Compositor {
    tree: Mutex<Value>,
    gate: Mutex<Option<Arc<RouteGate>>>,
}
impl host::CompositorControl for Compositor {
    fn tree(&self) -> Result<String, String> {
        let gate = self.gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            gate.block();
        }
        Ok(self.tree.lock().unwrap().to_string())
    }
    fn focus(&self, id: i64) -> Result<(), String> {
        let mut tree = self.tree.lock().unwrap();
        let nodes = tree["nodes"].as_array_mut().ok_or("missing nodes")?;
        if !nodes.iter().any(|node| node["id"] == id) {
            return Err("no such compositor node".into());
        }
        for node in nodes {
            node["focused"] = json!(node["id"] == id);
        }
        Ok(())
    }
}
#[tokio::test]
async fn identity_and_focus_snapshot_hold_the_transition_lock_against_exact_stop() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let compositor = Arc::new(Compositor {
        tree: Mutex::new(json!({"id":1,"nodes":[{"id":3,"pid":9100,"focused":true}]})),
        gate: Mutex::new(None),
    });
    let app = router(
        runtime.with_compositor(compositor.clone(), vec![]),
        root.path(),
        TOKEN,
        PortalPermission::LocalSessions,
    );
    let id = reserve(&app).await;
    assert_eq!(start(&app, &id).await["outcome"]["_tag"], "Ok");
    backend.set_window_pids(&id, [9100].into());
    let gate = Arc::new(RouteGate::default());
    *compositor.gate.lock().unwrap() = Some(gate.clone());
    let observing_app = app.clone();
    let observing =
        tokio::spawn(async move { rpc(&observing_app, "app.session.status", json!({})).await });
    gate.entered().await;
    let stopping_app = app.clone();
    let stopping_id = id.clone();
    let mut stopping = tokio::spawn(async move { cancel(&stopping_app, &stopping_id).await });
    let raced = tokio::time::timeout(Duration::from_millis(50), &mut stopping).await;
    gate.release();
    assert!(
        raced.is_err(),
        "exact stop must not interleave with identity + focus snapshot"
    );
    let observed = observing.await.unwrap();
    assert_eq!(observed["outcome"]["payload"]["active"]["launchId"], id);
    assert_eq!(
        observed["outcome"]["payload"]["active"]["focusOwnership"],
        "launch"
    );
    assert_eq!(
        stopping.await.unwrap()["outcome"]["payload"]["phase"],
        "stopped"
    );
    let replacement = reserve(&app).await;
    assert_eq!(start(&app, &replacement).await["outcome"]["_tag"], "Ok");
    let observed = rpc(&app, "app.session.status", json!({})).await;
    assert_eq!(
        observed["outcome"]["payload"]["active"]["launchId"],
        replacement
    );
    assert!(observed["outcome"]["payload"]["active"]
        .get("focusOwnership")
        .is_none());
}

#[tokio::test]
async fn status_observes_exact_focus_without_claiming_no_window_is_ready() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let compositor = Arc::new(Compositor {
        gate: Mutex::new(None),
        tree: Mutex::new(json!({"id":1,"nodes":[
            {"id":2,"pid":4100,"app_id":"chromium-browser","focused":true}
        ]})),
    });
    let app = router(
        runtime.with_compositor(compositor.clone(), vec!["chromium-browser".into()]),
        root.path(),
        TOKEN,
        PortalPermission::LocalSessions,
    );
    let id = reserve(&app).await;
    assert_eq!(start(&app, &id).await["outcome"]["_tag"], "Ok");
    backend.set_window_pids(&id, [9100].into());
    let status = rpc(&app, "app.session.status", json!({})).await;
    let active = &status["outcome"]["payload"]["active"];
    assert_eq!(active["launchId"], id);
    assert_eq!(active["phase"], "running");
    assert!(active.get("focusOwnership").is_none(), "{status}");
    for (focused, expected) in [(2, "excluded"), (3, "launch"), (4, "other")] {
        *compositor.tree.lock().unwrap() = json!({"id":1,"nodes":[
            {"id":2,"pid":4100,"app_id":"chromium-browser","focused":focused==2},
            {"id":3,"pid":9100,"focused":focused==3},
            {"id":4,"pid":9200,"focused":focused==4}
        ]});
        let status = rpc(&app, "app.session.status", json!({})).await;
        assert_eq!(status["outcome"]["payload"]["active"]["launchId"], id);
        assert_eq!(
            status["outcome"]["payload"]["active"]["focusOwnership"],
            expected
        );
    }
    *compositor.tree.lock().unwrap() = json!({"id":1,"nodes":[
        {"id":3,"pid":9100,"focused":true},{"id":5,"pid":9100,"focused":false}
    ]});
    assert!(
        rpc(&app, "app.session.status", json!({})).await["outcome"]["payload"]["active"]
            .get("focusOwnership")
            .is_none()
    );
}

async fn late_stop_helper_preserves_replacement(fails: bool, method: &'static str) {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let app = router(runtime, root.path(), TOKEN, PortalPermission::LocalSessions);
    let a = reserve(&app).await;
    assert_eq!(start(&app, &a).await["outcome"]["_tag"], "Ok");
    let gate = Arc::new(RouteGate::default());
    let blocked = gate.clone();
    backend.configure_stop(true, fails, Some(Arc::new(move || blocked.block())));
    let cancelling_app = app.clone();
    let cancelling_a = a.clone();
    let cancelling = tokio::spawn(async move {
        rpc(
            &cancelling_app,
            method,
            json!({"expectedLaunchId":cancelling_a}),
        )
        .await
    });
    gate.entered().await;
    let completed = rpc(&app, "app.session.status", json!({})).await;
    let b = reserve(&app).await;
    let started = start(&app, &b).await;
    let journal_path = root.path().join("private/host-session/active.json");
    let journal = std::fs::read(&journal_path).unwrap();
    gate.release();
    let cancellation = cancelling.await.unwrap();
    assert_eq!(
        completed["outcome"]["payload"]["code"], "SessionCompleted",
        "{completed}"
    );
    assert_eq!(started["outcome"]["_tag"], "Ok", "{started}");
    assert_eq!(
        cancellation["outcome"]["payload"]["code"], "StaleLaunchIdentity",
        "{cancellation}"
    );
    let status = rpc(&app, "app.session.status", json!({})).await;
    assert_eq!(
        status["outcome"]["payload"]["active"]["launchId"], b,
        "{status}"
    );
    assert_eq!(status["outcome"]["payload"]["active"]["phase"], "running");
    assert_eq!(std::fs::read(&journal_path).unwrap(), journal);
    // The replacement's native unit still accepts exact control.
    let frozen = rpc(&app, "app.session.freeze", json!({"expectedLaunchId":b})).await;
    assert_eq!(frozen["outcome"]["_tag"], "Ok", "{frozen}");
    assert_eq!(backend.launch_count(), 2);
}

#[tokio::test]
async fn late_successful_cancel_helper_preserves_replacement() {
    late_stop_helper_preserves_replacement(false, "app.session.cancel").await;
}

#[tokio::test]
async fn late_failed_cancel_helper_preserves_replacement() {
    late_stop_helper_preserves_replacement(true, "app.session.cancel").await;
}

#[tokio::test]
async fn home_accepted_before_delayed_successful_start_reply_remains_authorized() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, _) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let blocked = gate.clone();
    let (app, native) = secure_host_routers(
        runtime.with_reserved_start_probe(Arc::new(move || blocked.block())),
        &root.path().join("private"),
        Some(PortalAccess::new(
            TOKEN,
            ORIGIN,
            PortalPermission::LocalSessions,
        )),
    );
    let id = reserve(&app).await;
    let starting_app = app.clone();
    let starting_id = id.clone();
    let starting = tokio::spawn(async move { start(&starting_app, &starting_id).await });
    gate.entered().await;
    let home = rpc(
        &native,
        "app.session.freeze",
        json!({"expectedLaunchId":id}),
    )
    .await;
    let before = rpc(&app, "app.session.status", json!({})).await;
    gate.release();
    let started = starting.await.unwrap();
    assert_eq!(home["outcome"]["_tag"], "Ok", "{home}");
    assert_eq!(
        before["outcome"]["payload"]["overlay"]["launchId"], id,
        "{before}"
    );
    assert_eq!(started["outcome"]["_tag"], "Ok", "{started}");
    let after = rpc(&app, "app.session.status", json!({})).await;
    assert_eq!(
        after["outcome"]["payload"]["active"]["phase"], "frozen",
        "{after}"
    );
    assert_eq!(
        after["outcome"]["payload"]["overlay"]["launchId"], id,
        "{after}"
    );
}

#[tokio::test]
async fn cancel_during_commit_exposes_cleanup_recovery_failure_over_rpc() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, backend) = runtime(root.path());
    let gate = Arc::new(RouteGate::default());
    let blocked = gate.clone();
    backend.set_launch_probe(Arc::new(move || blocked.block()));
    backend.configure_stop(false, true, None);
    let app = router(runtime, root.path(), TOKEN, PortalPermission::LocalSessions);
    let id = reserve(&app).await;
    let starting_app = app.clone();
    let starting_id = id.clone();
    let starting = tokio::spawn(async move { start(&starting_app, &starting_id).await });
    gate.entered().await;
    let cancellation = cancel(&app, &id).await;
    gate.release();
    assert_eq!(cancellation["outcome"]["payload"]["phase"], "pending");
    let failed = starting.await.unwrap();
    assert_eq!(failed["outcome"]["_tag"], "Err", "{failed}");
    assert_eq!(
        failed["outcome"]["payload"]["code"], "HostRecoveryBlocked",
        "{failed}"
    );
    let status = rpc(&app, "app.session.status", json!({})).await;
    // Status may recover the retained native unit; that does not erase the
    // cleanup failure returned to the original start caller.
    assert_eq!(
        status["outcome"]["payload"]["active"]["launchId"], id,
        "{status}"
    );
    assert!(root
        .path()
        .join("private/host-session/active.json")
        .exists());
    assert_eq!(backend.launch_count(), 1);
}
