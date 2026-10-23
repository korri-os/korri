//! Resolve registration labels from the same verified peer-list producer.
use crate::*;

pub(crate) async fn register(
    state: &AppState,
    authorization: &authorization::AuthorizationContext,
    request: &SessionStreamPlayRequest,
) -> SessionStreamPlayOutcome {
    let result = async {
        let (
            ServerMode::Host(host),
            authorization::AuthorizationContext::Peer(authorization::Principal::OwnerDevice {
                device_public_key,
                ..
            }),
        ) = (&state.mode, authorization)
        else {
            return Err(installed_routes_unsupported());
        };
        let mut active = host
            .stream_play(
                &request.launch_id,
                StreamPlayer {
                    device_public_key: device_public_key.clone(),
                    label: device_public_key.clone(),
                },
            )
            .await?;
        label_players(state, &mut active);
        Ok(active)
    }
    .await;
    result
        .map(SessionStreamPlayOutcome::Ok)
        .unwrap_or_else(SessionStreamPlayOutcome::Err)
}

/// PeerListEntry is the label producer. Refresh labels on every read, so a
/// newly discovered endpoint replaces the device-key fallback immediately.
pub(crate) fn label_players(state: &AppState, active: &mut ActiveSession) {
    let peers = match peer_list(state) {
        PeerListOutcome::Ok(peers) => peers.peers,
        _ => Vec::new(),
    };
    for player in &mut active.stream_players {
        player.label = peers
            .iter()
            .find(|peer| peer.device_public_key == player.device_public_key)
            .map(|peer| peer.label.clone())
            .unwrap_or_else(|| player.device_public_key.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        authorization::{AuthorizationContext, Principal},
        host::control::InMemoryLaunchUnitBackend,
    };

    #[tokio::test]
    async fn authenticated_owner_device_without_a_directory_entry_can_play_on_the_stream() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"holder\"\n[[games]]\nid = \"game\"\ntitle = \"Game\"\ncommand = [\"game\"]\n").unwrap();
        let runtime = host::HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.path().join("private"),
            Arc::new(InMemoryLaunchUnitBackend::default()),
        );
        runtime.claim_save_owner();
        let (state, _) = crate::app_states(runtime.clone());
        let device = "33".repeat(32);
        let caller = AuthorizationContext::Peer(Principal::OwnerDevice {
            device_public_key: device.clone(),
            owner_statement: "verified".into(),
            owner_public_key: runtime.owner_public_key().unwrap(),
        });
        let request = |launch_id: &str| {
            RpcRequest::SessionStreamPlay(SessionStreamPlayRequest {
                launch_id: launch_id.into(),
            })
        };
        let response = crate::dispatch(&state, &caller, request("absent"))
            .await
            .unwrap();
        assert!(
            matches!(response, RpcResponse::SessionStreamPlay(SessionStreamPlayOutcome::Err(RpcFailure { code, .. })) if code == "NoActiveSession")
        );
        let session = runtime.prepare("game", None).await.unwrap().session;
        let response = crate::dispatch(&state, &caller, request("wrong"))
            .await
            .unwrap();
        assert!(
            matches!(response, RpcResponse::SessionStreamPlay(SessionStreamPlayOutcome::Err(RpcFailure { code, .. })) if code == "LaunchMismatch")
        );
        let response = crate::dispatch(&state, &caller, request(&session.launch_id))
            .await
            .unwrap();
        let RpcResponse::SessionStreamPlay(SessionStreamPlayOutcome::Ok(active)) = response else {
            panic!("registration failed")
        };
        assert_eq!(
            active.stream_players,
            vec![StreamPlayer {
                device_public_key: device.clone(),
                label: device
            }]
        );
        assert_eq!(active.title.as_deref(), Some("Game"));
    }
}
