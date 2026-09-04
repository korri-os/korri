use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use korrid::config::resolver::{
    resolve_moonlight_transport, resolve_session_controls, ActiveRouteContext, RouteContribution,
    RoutePlatform, SessionExecutorAvailability,
};
use korrid::plugin::{
    load_plugin_source, PluginRegistry, SessionControlExecutor, SessionControlOwnerKind,
};
use tower::ServiceExt;

const CANONICAL_MOONLIGHT_PLUGIN: &str = include_str!("../../../plugins/moonlight/plugin.ts");

const MOONLIGHT_PLUGIN: &str = r#"
({
  namespace: "@korri",
  name: "moonlight",
  title: "Moonlight",
  contributes: {
    config: {
      transports: {
        moonlight: { id: "@korri:moonlight/moonlight" },
      },
    },
    sessionControls: {
      disconnect: {
        order: 0,
        id: "@korri:moonlight/disconnect",
        owner: { kind: "transport", id: "@korri:moonlight/moonlight" },
        label: "Disconnect",
        description: "Leave the host game running",
        interaction: { kind: "command" },
        effect: "@korri:moonlight/disconnect",
        destructive: true,
        dismissOnSuccess: true,
      },
      fill: {
        order: 1,
        id: "@korri:moonlight/fill",
        owner: { kind: "transport", id: "@korri:moonlight/moonlight" },
        label: "Fill screen",
        interaction: { kind: "toggle", trueLabel: "Crop", falseLabel: "Fit" },
        effect: "@korri:moonlight/set-fill-mode",
      },
      mouseMode: {
        order: 2,
        id: "@korri:moonlight/mouse-mode",
        owner: { kind: "transport", id: "@korri:moonlight/moonlight" },
        label: "Mouse mode",
        interaction: {
          kind: "choice",
          options: [
            { value: "trackpad", label: "Trackpad" },
            { value: "direct", label: "Direct" },
          ],
        },
        effect: "@korri:moonlight/set-mouse-mode",
      },
      sharpness: {
        order: 3,
        id: "@korri:moonlight/sharpness",
        owner: { kind: "transport", id: "@korri:moonlight/moonlight" },
        label: "Sharpness",
        interaction: { kind: "range", min: 0, max: 100, step: 5 },
        effect: "@korri:moonlight/set-sgsr-sharpness",
      },
    },
  },
})
"#;

const RETROARCH_PLUGIN: &str = r#"
({
  namespace: "@korri",
  name: "retroarch",
  title: "RetroArch",
  contributes: {
    config: {
      launchers: {
        retroarch: {
          id: "@korri:retroarch/retroarch",
          plugin: "@korri:retroarch",
          command: "retroarch",
          android: {
            packageName: "com.korri.retroarch",
            className: "com.retroarch.browser.retroactivity.RetroActivityFuture",
          },
        },
      },
    },
    sessionControls: {
      openMenu: {
        order: 0,
        id: "@korri:retroarch/open-menu",
        owner: { kind: "launcher", id: "@korri:retroarch/retroarch" },
        label: "Open RetroArch menu",
        interaction: { kind: "command" },
        effect: "@korri:retroarch/open-menu",
        dismissOnSuccess: true,
      },
    },
  },
})
"#;

fn registry(enabled: &[&str]) -> PluginRegistry {
    PluginRegistry::new(
        vec![
            load_plugin_source(MOONLIGHT_PLUGIN).expect("Moonlight declaration"),
            load_plugin_source(RETROARCH_PLUGIN).expect("RetroArch declaration"),
        ],
        enabled.iter().map(|id| (*id).to_owned()),
    )
    .expect("session-control registry")
}

fn android_route(executors: &[SessionControlExecutor]) -> ActiveRouteContext {
    ActiveRouteContext {
        platform: RoutePlatform::Android,
        contributors: vec![
            RouteContribution {
                kind: SessionControlOwnerKind::Transport,
                id: "@korri:moonlight/moonlight".into(),
            },
            RouteContribution {
                kind: SessionControlOwnerKind::Launcher,
                id: "@korri:retroarch/retroarch".into(),
            },
        ],
        executor_availability: SessionExecutorAvailability::from_available(
            executors.iter().copied(),
        ),
    }
}

#[test]
fn active_route_owns_the_only_controls_that_resolve_in_deterministic_order() {
    let registry = registry(&["@korri:moonlight", "@korri:retroarch"]);
    let context = android_route(&[
        SessionControlExecutor::AndroidMoonlight,
        SessionControlExecutor::RetroarchControl,
    ]);

    let controls = resolve_session_controls(&registry, &context);
    assert_eq!(
        controls
            .iter()
            .map(|control| control.id.as_str())
            .collect::<Vec<_>>(),
        [
            "@korri:moonlight/disconnect",
            "@korri:moonlight/fill",
            "@korri:moonlight/mouse-mode",
            "@korri:moonlight/sharpness",
            "@korri:retroarch/open-menu",
        ]
    );
    assert_eq!(controls[0].plugin_id, "@korri:moonlight");
    assert_eq!(controls[4].plugin_id, "@korri:retroarch");

    let reversed_route = ActiveRouteContext {
        contributors: context.contributors.iter().cloned().rev().collect(),
        ..context.clone()
    };
    assert_eq!(
        resolve_session_controls(&registry, &reversed_route)
            .iter()
            .map(|control| control.id.as_str())
            .collect::<Vec<_>>(),
        [
            "@korri:retroarch/open-menu",
            "@korri:moonlight/disconnect",
            "@korri:moonlight/fill",
            "@korri:moonlight/mouse-mode",
            "@korri:moonlight/sharpness",
        ]
    );

    let retroarch_only = ActiveRouteContext {
        contributors: vec![RouteContribution {
            kind: SessionControlOwnerKind::Launcher,
            id: "@korri:retroarch/retroarch".into(),
        }],
        ..context
    };
    assert_eq!(
        resolve_session_controls(&registry, &retroarch_only)
            .iter()
            .map(|control| control.id.as_str())
            .collect::<Vec<_>>(),
        ["@korri:retroarch/open-menu"]
    );
}

#[test]
fn canonical_moonlight_resolves_typed_artemis_availability_only_when_enabled() {
    let plugin = load_plugin_source(CANONICAL_MOONLIGHT_PLUGIN).expect("Moonlight declaration");
    let enabled = PluginRegistry::new(vec![plugin.clone()], vec!["@korri:moonlight".to_owned()])
        .expect("enabled Moonlight registry");

    let resolved = resolve_moonlight_transport(&enabled, RoutePlatform::Android)
        .expect("Android Artemis should be available");
    assert_eq!(resolved.transport_id, "@korri:moonlight/moonlight");
    assert_eq!(resolved.implementation.as_str(), "artemis");
    assert_eq!(resolved.sunshine_app, "Korri Stream");
    let controls = resolve_session_controls(
        &enabled,
        &ActiveRouteContext {
            platform: RoutePlatform::Android,
            contributors: vec![RouteContribution {
                kind: SessionControlOwnerKind::Transport,
                id: resolved.transport_id.clone(),
            }],
            executor_availability: SessionExecutorAvailability::from_available([
                SessionControlExecutor::AndroidMoonlight,
            ]),
        },
    );
    assert_eq!(
        controls
            .iter()
            .map(|control| control.id.as_str())
            .collect::<Vec<_>>(),
        [
            "@korri:moonlight/fill",
            "@korri:moonlight/keyboard",
            "@korri:moonlight/full-keyboard",
            "@korri:moonlight/pan-zoom",
            "@korri:moonlight/mouse-mode",
            "@korri:moonlight/local-cursor",
            "@korri:moonlight/rotate-screen",
            "@korri:moonlight/hud",
            "@korri:moonlight/floating-menu",
            "@korri:moonlight/keyboard-controller",
            "@korri:moonlight/touch-sensitivity",
            "@korri:moonlight/sgsr-sharpness",
            "@korri:moonlight/sgsr-edge-threshold",
            "@korri:moonlight/face-button-flip",
            "@korri:moonlight/rumble",
            "@korri:moonlight/picture-in-picture",
            "@korri:moonlight/stream-bitrate",
            "@korri:moonlight/restore-stream-bitrate",
            "@korri:moonlight/stream-fps",
            "@korri:moonlight/restore-stream-fps",
            "@korri:moonlight/stream-width",
            "@korri:moonlight/restore-stream-resolution",
            "@korri:moonlight/disconnect",
            "@korri:moonlight/quit-host",
        ]
    );
    let by_id = |id: &str| controls.iter().find(|control| control.id == id).unwrap();
    assert_eq!(by_id("@korri:moonlight/fill").label, "Screen fit");
    assert_eq!(
        by_id("@korri:moonlight/fill").interaction,
        korrid::plugin::SessionControlDeclarationInteraction::Toggle {
            true_label: Some("crop to fill".into()),
            false_label: Some("fit (letterbox)".into()),
        }
    );
    assert_eq!(
        by_id("@korri:moonlight/disconnect").description.as_deref(),
        Some("Leave the host game running")
    );
    assert!(by_id("@korri:moonlight/quit-host").destructive);
    for id in [
        "@korri:moonlight/keyboard",
        "@korri:moonlight/full-keyboard",
        "@korri:moonlight/pan-zoom",
        "@korri:moonlight/mouse-mode",
        "@korri:moonlight/local-cursor",
        "@korri:moonlight/rotate-screen",
        "@korri:moonlight/hud",
        "@korri:moonlight/floating-menu",
        "@korri:moonlight/keyboard-controller",
        "@korri:moonlight/touch-sensitivity",
        "@korri:moonlight/disconnect",
        "@korri:moonlight/quit-host",
    ] {
        assert!(by_id(id).dismiss_on_success, "{id} must hide before effect");
    }
    match &by_id("@korri:moonlight/mouse-mode").interaction {
        korrid::plugin::SessionControlDeclarationInteraction::Choice { options } => {
            assert_eq!(
                options
                    .iter()
                    .map(|option| (option.value.as_str(), option.label.as_str()))
                    .collect::<Vec<_>>(),
                [
                    ("0", "Multi touch"),
                    ("1", "Absolute touch"),
                    ("2", "Track pad(Natural/Double tap to drag)"),
                    ("3", "Track pad(Gaming/Long press to drag)"),
                    ("4", "Disabled"),
                    ("5", "Absolute touch (left/right click swapped)"),
                ]
            );
        }
        other => panic!("expected mouse choice, got {other:?}"),
    }
    assert_eq!(
        by_id("@korri:moonlight/local-cursor").label,
        "Toggle local mouse cursor(physical mouse needed)"
    );
    assert_eq!(
        by_id("@korri:moonlight/sgsr-sharpness").interaction,
        korrid::plugin::SessionControlDeclarationInteraction::Range {
            min: 0.0,
            max: 50.0,
            step: 1.0,
        }
    );
    assert_eq!(
        by_id("@korri:moonlight/sgsr-edge-threshold").interaction,
        korrid::plugin::SessionControlDeclarationInteraction::Range {
            min: 1.0,
            max: 32.0,
            step: 1.0,
        }
    );

    let disabled = PluginRegistry::new(vec![plugin], Vec::new())
        .expect("disabled Moonlight should remain registered");
    assert!(resolve_moonlight_transport(&disabled, RoutePlatform::Android).is_none());
    assert!(resolve_moonlight_transport(&enabled, RoutePlatform::Linux).is_none());
}

#[test]
fn platform_and_live_executor_availability_are_both_required() {
    let registry = registry(&["@korri:moonlight", "@korri:retroarch"]);

    assert!(resolve_session_controls(&registry, &android_route(&[])).is_empty());

    let linux = ActiveRouteContext {
        platform: RoutePlatform::Linux,
        contributors: vec![RouteContribution {
            kind: SessionControlOwnerKind::Launcher,
            id: "@korri:retroarch/retroarch".into(),
        }],
        executor_availability: SessionExecutorAvailability::from_available([
            SessionControlExecutor::RetroarchControl,
        ]),
    };
    assert!(resolve_session_controls(&registry, &linux).is_empty());
}

#[tokio::test]
async fn rpc_list_and_invoke_stay_unavailable_without_current_route_context() {
    let root = tempfile::tempdir().expect("host fixture");
    let config = root.path().join("host.toml");
    std::fs::write(&config, "label = \"test\"\n").expect("host config");
    let app = korrid::host_routers_with_storage_and_private(
        &config,
        None::<std::path::PathBuf>,
        root.path().join("private"),
    )
    .1;

    for request in [
        r#"{"_tag":"app.session.controls","payload":{"launchId":"launch-1"}}"#,
        r#"{"_tag":"app.session.control.invoke","payload":{"launchId":"launch-1","controlId":"@korri:retroarch/open-menu"}}"#,
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request))
                    .expect("RPC request"),
            )
            .await
            .expect("RPC response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("RPC response body");
        let body: serde_json::Value = serde_json::from_slice(&body).expect("tagged RPC JSON");
        assert_eq!(body["outcome"]["_tag"], "Err");
        assert_eq!(body["outcome"]["payload"]["reason"], "Unavailable");
        assert!(body["outcome"]["payload"]["message"]
            .as_str()
            .expect("failure message")
            .contains("current active-session route"));
    }
}

fn moonlight_request() -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/rpc")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, "Bearer moonlight-test-capability")
        .body(Body::from(
            r#"{"_tag":"app.moonlight.resolve","payload":{}}"#,
        ))
        .expect("Moonlight RPC request")
}

async fn moonlight_outcome(app: axum::Router) -> serde_json::Value {
    let response = app
        .oneshot(moonlight_request())
        .await
        .expect("Moonlight RPC response");
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("Moonlight RPC body");
    serde_json::from_slice(&body).expect("typed response")
}

#[tokio::test]
async fn standalone_brain_reports_typed_moonlight_unavailable() {
    let root = tempfile::tempdir().expect("Moonlight config root");
    std::fs::write(root.path().join("config.yaml"), "{}\n").expect("default config");
    std::fs::write(root.path().join("library.yaml"), "{}\n").expect("empty library");
    let app = korrid::router_with_capability_and_local_root(
        "moonlight-test-capability",
        "https://appassets.androidplatform.net",
        root.path(),
    );

    let body = moonlight_outcome(app).await;
    assert_eq!(body["outcome"]["_tag"], "Unavailable");
    assert_eq!(body["outcome"]["payload"]["code"], "MoonlightUnavailable");
}

#[tokio::test]
async fn standalone_brain_refuses_to_prepare_moonlight_launches() {
    let root = tempfile::tempdir().expect("Moonlight config root");
    std::fs::write(root.path().join("config.yaml"), "{}\n").expect("default config");
    std::fs::write(root.path().join("library.yaml"), "{}\n").expect("empty library");
    let request = || {
        Request::builder()
            .method("POST")
            .uri("/rpc")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, "Bearer moonlight-test-capability")
            .body(Body::from(
                r#"{"_tag":"app.moonlight.launch.prepare","payload":{"hostUuid":"host-uuid","appId":7}}"#,
            ))
            .expect("Moonlight launch preparation request")
    };
    let response_json = |body: axum::body::Bytes| {
        serde_json::from_slice::<serde_json::Value>(&body).expect("typed response")
    };

    let standalone = korrid::router_with_capability_and_local_root(
        "moonlight-test-capability",
        "https://appassets.androidplatform.net",
        root.path(),
    );
    let response = standalone
        .oneshot(request())
        .await
        .expect("standalone response");
    let body = response_json(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("standalone body"),
    );
    assert_eq!(body["outcome"]["_tag"], "Err");
    assert_eq!(body["outcome"]["payload"]["code"], "MoonlightUnavailable");
}

#[test]
fn layered_disable_removes_controls_without_reassigning_reserved_identity() {
    let disabled = registry(&["@korri:retroarch"]);
    assert!(disabled.owns_registered_session_control_id("@korri:moonlight/disconnect"));
    assert!(!disabled
        .session_controls()
        .contains_key("@korri:moonlight/disconnect"));
    assert_eq!(
        resolve_session_controls(
            &disabled,
            &android_route(&[
                SessionControlExecutor::AndroidMoonlight,
                SessionControlExecutor::RetroarchControl,
            ]),
        )
        .iter()
        .map(|control| control.id.as_str())
        .collect::<Vec<_>>(),
        ["@korri:retroarch/open-menu"]
    );

    let enabled = registry(&["@korri:moonlight", "@korri:retroarch"]);
    assert!(enabled
        .session_controls()
        .contains_key("@korri:moonlight/disconnect"));
}
