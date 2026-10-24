//! Peer controls follow the device that started the exact session.
use crate::{
    authorization::AuthorizationContext, host::HostRuntime, RpcFailure, RpcRequest, RpcResponse,
    SessionControlFailure, SessionControlFailureReason, SessionControlInvokeOutcome,
    SessionControlsOutcome, SessionEndOutcome, SessionTransitionOutcome,
};

pub(crate) async fn refusal(
    host: &HostRuntime,
    authorization: &AuthorizationContext,
    request: &RpcRequest,
) -> Option<RpcResponse> {
    let AuthorizationContext::Peer(principal) = authorization else {
        return None;
    };
    let expected = match request {
        RpcRequest::SessionEnd(r) => r.expected_launch_id.as_deref(),
        RpcRequest::SessionLeave(r) => r.expected_launch_id.as_deref(),
        RpcRequest::SessionResume(r) => r.expected_launch_id.as_deref(),
        RpcRequest::SessionControls(r) => Some(r.launch_id.as_str()),
        RpcRequest::SessionControlInvoke(r) => Some(r.launch_id.as_str()),
        _ => return None,
    };
    let failure = host
        .require_session_starter(principal.device_public_key(), expected)
        .await
        .err()?;
    Some(refused_response(request, failure))
}

pub(crate) fn brain_refusal(
    authorization: &AuthorizationContext,
    request: &RpcRequest,
) -> Option<RpcResponse> {
    if !matches!(authorization, AuthorizationContext::Peer(_))
        || !matches!(
            request,
            RpcRequest::SessionEnd(_)
                | RpcRequest::SessionLeave(_)
                | RpcRequest::SessionResume(_)
                | RpcRequest::SessionControls(_)
                | RpcRequest::SessionControlInvoke(_)
        )
    {
        return None;
    }
    Some(refused_response(
        request,
        RpcFailure {
            code: "NotSessionStarter".into(),
            message: "Peer session controls must address the holding device, not be forwarded."
                .into(),
        },
    ))
}

fn refused_response(request: &RpcRequest, failure: RpcFailure) -> RpcResponse {
    let control_failure = || SessionControlFailure {
        reason: if failure.code == "NotSessionStarter" {
            SessionControlFailureReason::NotSessionStarter
        } else {
            SessionControlFailureReason::StaleSession
        },
        message: failure.message.clone(),
    };
    match request {
        RpcRequest::SessionEnd(_) => RpcResponse::SessionEnd(SessionEndOutcome::Err(failure)),
        RpcRequest::SessionLeave(_) => {
            RpcResponse::SessionLeave(SessionTransitionOutcome::Err(failure))
        }
        RpcRequest::SessionResume(_) => {
            RpcResponse::SessionResume(SessionTransitionOutcome::Err(failure))
        }
        RpcRequest::SessionControls(_) => {
            RpcResponse::SessionControls(SessionControlsOutcome::Err(control_failure()))
        }
        RpcRequest::SessionControlInvoke(_) => {
            RpcResponse::SessionControlInvoke(SessionControlInvokeOutcome::Err(control_failure()))
        }
        _ => unreachable!("only session controls enter starter refusal"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{authorization::Principal, host::control::InMemoryLaunchUnitBackend};
    use serde_json::json;
    use std::sync::Arc;

    fn peer(device: &str) -> AuthorizationContext {
        AuthorizationContext::Peer(Principal::OwnerDevice {
            device_public_key: device.into(),
            owner_statement: "verified".into(),
            owner_public_key: "owner".into(),
        })
    }

    #[tokio::test]
    async fn every_peer_control_is_refused_unless_the_peer_started_the_session() {
        for starter in [None, Some("starter")] {
            for (tag, extra) in [
                ("app.session.end", json!({})),
                ("app.session.end", json!({"force":true})),
                ("app.session.leave", json!({})),
                ("app.session.resume", json!({})),
                ("app.session.controls", json!({})),
                ("app.session.control.invoke", json!({"controlId":"control"})),
            ] {
                let root = tempfile::tempdir().unwrap();
                let config = root.path().join("host.toml");
                std::fs::write(&config, "label = \"device\"\n[[games]]\nid = \"game\"\ntitle = \"Game\"\ncommand = [\"game\"]\n").unwrap();
                let runtime = HostRuntime::from_paths_with_backend(
                    &config,
                    None,
                    root.path().join("private"),
                    Arc::new(InMemoryLaunchUnitBackend::default()),
                );
                runtime.claim_save_owner();
                let session = runtime
                    .clone()
                    .for_starting_device(starter.map(str::to_owned))
                    .prepare("game", None)
                    .await
                    .unwrap()
                    .session;
                let (state, _) = crate::app_states(runtime);
                let mut payload = extra;
                payload[if tag.contains("controls") || tag.contains("control.invoke") {
                    "launchId"
                } else {
                    "expectedLaunchId"
                }] = json!(session.launch_id);
                let wire = json!({"_tag":tag, "payload":payload});
                for device in ["other", "starter"] {
                    let request: RpcRequest = serde_json::from_value(wire.clone()).unwrap();
                    let allowed = starter == Some(device);
                    let response = crate::dispatch(&state, &peer(device), request)
                        .await
                        .unwrap();
                    let response = serde_json::to_value(response).unwrap();
                    let failure = &response["outcome"]["payload"];
                    let denied = failure["code"] == "NotSessionStarter"
                        || failure["reason"] == "NotSessionStarter";
                    assert_eq!(denied, !allowed, "{starter:?} {device} {tag}: {response}");
                }
            }
        }
    }
    #[derive(Debug)]
    struct FarFocus;
    impl crate::host::CompositorControl for FarFocus {
        fn tree(&self) -> Result<String, String> {
            Ok(r#"{"nodes":[{"id":2,"pid":9100,"focused":true}]}"#.into())
        }
        fn focus(&self, _: i64) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn stream_play_brain_refuses_peer_controls_but_local_controls_still_reach_the_holder() {
        let root = tempfile::tempdir().unwrap();
        let far_config = root.path().join("host.toml");
        std::fs::write(
            &far_config,
            "label = \"far\"\n[[games]]\nid = \"game\"\ntitle = \"Game\"\ncommand = [\"game\"]\n",
        )
        .unwrap();
        let units = Arc::new(InMemoryLaunchUnitBackend::default());
        let holder = HostRuntime::from_paths_with_backend(
            &far_config,
            None,
            root.path().join("far-private"),
            units.clone(),
        )
        .with_compositor(Arc::new(FarFocus), vec![]);
        holder.claim_save_owner();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = crate::plain_host_routers_for_tests(holder.clone()).0;
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let upstream = crate::upstreams::UpstreamRegistry::new(vec![
            crate::upstreams::UpstreamHostConfig::native("far", format!("http://{address}")),
        ]);
        let launch = upstream
            .prepare_stream("game", Some("far"))
            .await
            .unwrap()
            .session
            .launch_id;
        units.set_window_pids(&launch, [9100].into());
        let (state, _) = crate::brain_app_state(
            "cap",
            "http://portal.local",
            root.path(),
            root.path().join("local-private"),
            crate::plugin_policy::RegistrySource::Installed,
            crate::config::snapshot::ConfigSnapshotCoordinator::new(root.path()),
            crate::discovery::FolderSelectionGrantStore::default(),
            Some(upstream),
            None,
            None,
        );
        for tag in [
            "app.session.end",
            "app.session.leave",
            "app.session.resume",
            "app.session.controls",
            "app.session.control.invoke",
        ] {
            let payload = if tag == "app.session.control.invoke" {
                json!({"launchId":launch,"controlId":"plugin/action"})
            } else if tag == "app.session.controls" {
                json!({"launchId":launch})
            } else {
                json!({"expectedLaunchId":launch})
            };
            let wire = json!({"_tag":tag,"payload":payload});
            for local in [
                AuthorizationContext::LocalBrowser,
                AuthorizationContext::LocalUnixControl,
            ] {
                let request = serde_json::from_value(wire.clone()).unwrap();
                assert!(brain_refusal(&local, &request).is_none());
            }
            let request = serde_json::from_value(wire).unwrap();
            let response = crate::dispatch(&state, &peer("other"), request)
                .await
                .unwrap();
            let value = serde_json::to_value(response).unwrap();
            let payload = &value["outcome"]["payload"];
            assert!(
                payload["code"] == "NotSessionStarter" || payload["reason"] == "NotSessionStarter",
                "{value}"
            );
        }
        assert!(matches!(
            holder.session_status().await.unwrap(),
            crate::host::control::HostSessionStatus::Running { .. }
                | crate::host::control::HostSessionStatus::FocusFailed { .. }
        ));
        for tag in ["app.session.leave", "app.session.resume", "app.session.end"] {
            let request =
                serde_json::from_value(json!({"_tag":tag,"payload":{"expectedLaunchId":launch}}))
                    .unwrap();
            let response = crate::dispatch(&state, &AuthorizationContext::LocalBrowser, request)
                .await
                .unwrap();
            assert_eq!(
                serde_json::to_value(response).unwrap()["outcome"]["_tag"],
                "Ok",
                "{tag}"
            );
        }
        server.abort();
    }
}
