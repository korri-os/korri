//! Post-response kiosk readiness through the production portal RPC router.
use super::*;
use std::{os::unix::fs::PermissionsExt, sync::atomic::AtomicI32};

#[derive(Debug)]
struct Portal(AtomicI32);
impl host::PortalUnit for Portal {
    fn freezer_enabled(&self) -> bool {
        false
    }
    fn freeze(&self) -> Result<(), host::LaunchUnitError> {
        Ok(())
    }
    fn thaw(&self) -> Result<(), host::LaunchUnitError> {
        Ok(())
    }
    fn window_pids(&self) -> Result<std::collections::BTreeSet<i32>, host::LaunchUnitError> {
        Ok([self.0.load(Ordering::SeqCst)].into_iter().collect())
    }
}

fn display_request() -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/rpc")
        .extension(axum::extract::ConnectInfo(
            "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
        ))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::ORIGIN, ORIGIN)
        .body(Body::from(
            json!({"_tag":"app.surface.displays","payload":{"displays":"all","overlay":false}})
                .to_string(),
        ))
        .unwrap()
}

#[tokio::test]
async fn catalog_reply_precedes_one_readiness_reflow_and_newer_placement_cancels_it() {
    const CHILD: &str = "KORRI_PORTAL_READINESS_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "launch_handoff_tests::readiness::catalog_reply_precedes_one_readiness_reflow_and_newer_placement_cancels_it", "--nocapture"])
            .env(CHILD, "1")
            .env("KORRID_DISPLAYS", r#"[{"name":"DSI-2","main":true,"x":0,"y":0,"width":640,"height":480,"touch":null},{"name":"DSI-1","main":false,"x":0,"y":480,"width":640,"height":480,"touch":null}]"#)
            .output().unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("PortalReadinessRestore"), "{stderr}");
        for reason in [
            "newer_placement",
            "portal_instance_changed",
            "reply_not_completed",
        ] {
            assert!(stderr.contains(reason), "missing {reason}: {stderr}");
        }
        for line in stderr
            .lines()
            .filter(|line| line.contains("since_initial_restore_ms:"))
        {
            let elapsed: u128 = line
                .split("since_initial_restore_ms: ")
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            assert!(elapsed >= 500, "{line}");
        }
        return;
    }
    for cancellation in [
        "http",
        "concurrent",
        "game",
        "instance",
        "abort",
        "busy",
        "tcp",
    ] {
        let root = tempfile::tempdir().unwrap();
        let tree = |pid| {
            json!({"nodes":[{"id":2,"pid":pid,"focused":true,"rect":{"x":0,"y":0,"width":640,"height":960}}, {"id":3,"pid":9100,"focused":false}]}).to_string()
        };
        std::fs::write(root.path().join("tree"), tree(4100)).unwrap();
        let swaymsg = root.path().join("swaymsg");
        std::fs::write(
            &swaymsg,
            format!(
                r#"#!/bin/sh
cd '{}' || exit 1
for argument do command="$argument"; done
case "$command" in get_tree) cat tree; exit;; esac
printf '%s\n' "$command" >> commands
case "$command" in *'height 900 px'*)
  if test -e block; then
    touch blocked
    while ! test -e release; do sleep 0.01; done
  fi;;
esac
printf '[{{"success":true}}]'
"#,
                root.path().display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&swaymsg, std::fs::Permissions::from_mode(0o755)).unwrap();
        let compositor = Arc::new(
            host::compositor_focus::SwaymsgCompositorControl::new(
                swaymsg,
                root.path().join("socket"),
                Duration::from_secs(5),
            )
            .unwrap(),
        );
        let portal = Arc::new(Portal(AtomicI32::new(4100)));
        let done = Arc::new(tokio::sync::Notify::new());
        let events = Arc::new(Mutex::new(Vec::new()));
        let observed_done = done.clone();
        let observed_events = events.clone();
        let (runtime, backend) = runtime(root.path());
        let runtime = runtime
            .with_compositor(compositor)
            .with_portal_unit(portal.clone())
            .with_session_rpc_probe(Arc::new(move |phase| {
                if phase == "app.surface.displays.reply_complete" {
                    observed_events.lock().unwrap().push("reply");
                }
                if phase == "app.surface.displays.readiness_done" {
                    observed_events.lock().unwrap().push("done");
                    observed_done.notify_one();
                }
            }));
        let scope = || host::display_placement::SurfaceDisplaysRequest {
            displays: host::display_placement::SurfaceDisplayScope::All,
            overlay: false,
            launch_id: None,
        };
        let count = || {
            std::fs::read_to_string(root.path().join("commands"))
                .unwrap_or_default()
                .lines()
                .filter(|line| line.contains("height 900 px"))
                .count()
        };
        let app = router(
            runtime.clone(),
            root.path(),
            TOKEN,
            PortalPermission::LocalSessions,
        );
        if cancellation == "tcp" {
            // Exercise real Hyper response draining, not just Router::oneshot.
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let serving = app.clone();
            let server = tokio::spawn(async move {
                axum::serve(
                    listener,
                    serving.into_make_service_with_connect_info::<std::net::SocketAddr>(),
                )
                .await
                .unwrap();
            });
            let client = reqwest::Client::new();
            for expected in [1, 2] {
                let reply: Value = client.post(format!("http://{address}/rpc"))
                    .bearer_auth(TOKEN).header(header::ORIGIN,ORIGIN)
                    .json(&json!({"_tag":"app.surface.displays","payload":{"displays":"all","overlay":false}}))
                    .send().await.unwrap().json().await.unwrap();
                assert_eq!(reply["outcome"]["_tag"], "Ok");
                if expected == 1 {
                    tokio::time::timeout(Duration::from_secs(3), done.notified())
                        .await
                        .unwrap();
                }
                assert_eq!(count(), 2, "real HTTP must run only one follow-up");
            }
            assert_eq!(*events.lock().unwrap(), ["reply", "done"]);
            server.abort();
            continue;
        }
        // Also cover startup placement completed before the mounted request.
        // The production cold path (http) performs it in that request itself.
        if !matches!(cancellation, "http" | "concurrent") {
            runtime.surface_displays(scope()).await.unwrap();
        }
        let (response, simultaneous) = if cancellation == "concurrent" {
            let (first, second) = tokio::join!(
                app.clone().oneshot(display_request()),
                app.clone().oneshot(display_request())
            );
            (first.unwrap(), Some(second.unwrap()))
        } else {
            (app.clone().oneshot(display_request()).await.unwrap(), None)
        };
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            count(),
            1,
            "first mounted request returns cached facts, not another synchronous placement"
        );
        // Holding the body beyond the deadline proves this isn't queued from
        // inside dispatch before the reply has been consumed by transport.
        tokio::time::sleep(Duration::from_millis(650)).await;
        assert_eq!(count(), 1, "no readiness effect before reply completion");
        assert!(events.lock().unwrap().is_empty());
        if cancellation == "abort" {
            drop(response);
            let second = rpc(
                &app,
                "app.surface.displays",
                json!({"displays":"all","overlay":false}),
            )
            .await;
            assert_eq!(second["outcome"]["_tag"], "Ok");
            tokio::time::sleep(Duration::from_millis(650)).await;
            assert_eq!(count(), 1, "an aborted reply must not schedule or rearm");
            assert!(events.lock().unwrap().is_empty());
            continue;
        }
        if cancellation == "busy" {
            std::fs::write(root.path().join("block"), "").unwrap();
        }
        if cancellation == "game" {
            let id = reserve(&app).await;
            assert_eq!(start(&app, &id).await["outcome"]["_tag"], "Ok");
            backend.set_window_pids(&id, [9100].into());
            runtime.session_status().await.unwrap();
            assert!(
                std::fs::read_to_string(root.path().join("commands"))
                    .unwrap()
                    .contains("[con_id=3]"),
                "the cancellation must follow a real game placement"
            );
        } else if cancellation == "instance" {
            portal.0.store(4200, Ordering::SeqCst);
            std::fs::write(root.path().join("tree"), tree(4200)).unwrap();
        }
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let reply: Value = serde_json::from_slice(&body).unwrap();
        if let Some(second) = simultaneous {
            let body = to_bytes(second.into_body(), usize::MAX).await.unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap()["outcome"]["_tag"],
                "Ok"
            );
        }
        assert_eq!(reply["outcome"]["_tag"], "Ok", "{reply}");
        if cancellation == "busy" {
            tokio::time::timeout(Duration::from_secs(2), async {
                while !root.path().join("blocked").exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let cached = tokio::time::timeout(
                Duration::from_secs(1),
                rpc(
                    &app,
                    "app.surface.displays",
                    json!({"displays":"all","overlay":false}),
                ),
            )
            .await;
            std::fs::write(root.path().join("release"), "").unwrap();
            assert_eq!(
                cached.expect("cached facts must not wait for native reflow")["outcome"]["_tag"],
                "Ok"
            );
        }
        tokio::time::timeout(Duration::from_secs(3), done.notified())
            .await
            .expect("readiness task must finish or cancel");
        assert_eq!(*events.lock().unwrap(), ["reply", "done"]);
        assert_eq!(
            count(),
            if matches!(cancellation, "http" | "concurrent" | "busy") {
                2
            } else {
                1
            },
            "{cancellation}"
        );
        if matches!(cancellation, "http" | "concurrent" | "busy") {
            let second = rpc(
                &app,
                "app.surface.displays",
                json!({"displays":"all","overlay":false}),
            )
            .await;
            assert_eq!(second["outcome"]["_tag"], "Ok");
            tokio::time::sleep(Duration::from_millis(650)).await;
            assert_eq!(count(), 2, "second request must not reflow again");
            assert_eq!(*events.lock().unwrap(), ["reply", "done"]);
        }
        if cancellation == "instance" {
            let next = rpc(
                &app,
                "app.surface.displays",
                json!({"displays":"all","overlay":false}),
            )
            .await;
            assert_eq!(next["outcome"]["_tag"], "Ok");
            tokio::time::timeout(Duration::from_secs(3), done.notified())
                .await
                .unwrap();
            assert_eq!(
                count(),
                3,
                "new kiosk gets its own sync restore and one reflow"
            );
            assert_eq!(*events.lock().unwrap(), ["reply", "done", "reply", "done"]);
        }
    }
}
