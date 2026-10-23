//! Peer controls follow the device that started the exact session.
use crate::{
    authorization::AuthorizationContext, host::HostRuntime, RpcFailure, RpcRequest, RpcResponse,
    SessionControlFailure, SessionControlFailureReason, SessionControlInvokeOutcome,
    SessionControlsOutcome, SessionFreezeOutcome, SessionStopOutcome,
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
        RpcRequest::SessionStop(r) => r.expected_launch_id.as_deref(),
        RpcRequest::SessionFreeze(r) => r.expected_launch_id.as_deref(),
        RpcRequest::SessionThaw(r) => r.expected_launch_id.as_deref(),
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
        RpcRequest::SessionStop(_) => RpcResponse::SessionStop(SessionStopOutcome::Err(failure)),
        RpcRequest::SessionFreeze(_) => {
            RpcResponse::SessionFreeze(SessionFreezeOutcome::Err(failure))
        }
        RpcRequest::SessionThaw(_) => RpcResponse::SessionThaw(SessionFreezeOutcome::Err(failure)),
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
                ("app.session.stop", json!({})),
                ("app.session.stop", json!({"force":true})),
                ("app.session.freeze", json!({})),
                ("app.session.thaw", json!({})),
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
}
