//! Stream registration keeps the label supplied by its authenticated caller.
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
        if request.device_label.trim().is_empty()
            || request.device_label.len() > 256
            || request.device_label.chars().any(char::is_control)
        {
            return Err(RpcFailure {
                code: "InvalidRequest".into(),
                message: "Stream play requires this device's label.".into(),
            });
        }
        let active = state
            .netplay_holder
            .stream_play(
                host,
                &request.launch_id,
                StreamPlayer {
                    device_public_key: device_public_key.clone(),
                    label: request.device_label.clone(),
                },
            )
            .await?;
        Ok(active)
    }
    .await;
    result
        .map(SessionStreamPlayOutcome::Ok)
        .unwrap_or_else(SessionStreamPlayOutcome::Err)
}

pub(crate) fn caller_label(state: &AppState) -> Result<String, RpcFailure> {
    let snapshot = match &state.mode {
        ServerMode::Host(host) => {
            crate::config::snapshot::ConfigSnapshotCoordinator::new(host.route_root()?).reload()
        }
        ServerMode::Brain(brain) => brain.config_snapshot.reload(),
    };
    let unavailable = || RpcFailure {
        code: "DeviceLabelUnavailable".into(),
        message: "This device needs a name before stream play.".into(),
    };
    if snapshot.authorization != crate::config::snapshot::SnapshotAuthorization::Authorized {
        return Err(unavailable());
    }
    crate::federation::coordinator::device_label(&snapshot.snapshot, std::env::var("HOSTNAME").ok())
        .ok_or_else(unavailable)
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
                device_label: "Pocket Mini".into(),
            })
        };
        let response = crate::dispatch(&state, &caller, request("absent"))
            .await
            .unwrap();
        assert!(
            matches!(response, RpcResponse::SessionStreamPlay(SessionStreamPlayOutcome::Err(RpcFailure { code, .. })) if code == "NoActiveSession")
        );
        let session = runtime.prepare("game", None, None).await.unwrap().session;
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
                label: "Pocket Mini".into()
            }]
        );
        assert_eq!(active.title.as_deref(), Some("Game"));
        let response = crate::dispatch(
            &state,
            &caller,
            RpcRequest::SessionStreamPlay(SessionStreamPlayRequest {
                launch_id: session.launch_id.clone(),
                device_label: "Renamed device".into(),
            }),
        )
        .await
        .unwrap();
        let RpcResponse::SessionStreamPlay(SessionStreamPlayOutcome::Ok(active)) = response else {
            panic!("re-registration failed")
        };
        assert_eq!(active.stream_players[0].label, "Renamed device");
        assert_ne!(active.stream_players[0].label, device);
        for label in ["", "  ", "bad\nlabel", &"x".repeat(257)] {
            let response = crate::dispatch(
                &state,
                &caller,
                RpcRequest::SessionStreamPlay(SessionStreamPlayRequest {
                    launch_id: session.launch_id.clone(),
                    device_label: label.into(),
                }),
            )
            .await
            .unwrap();
            assert!(
                matches!(response, RpcResponse::SessionStreamPlay(SessionStreamPlayOutcome::Err(RpcFailure { code, .. })) if code == "InvalidRequest")
            );
        }
    }
    #[tokio::test]
    async fn stream_play_caller_sends_the_label_from_device_settings() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("device.yaml"),
            "host:\n  title: Pocket Mini\n",
        )
        .unwrap();
        let config = crate::config::snapshot::ConfigSnapshotCoordinator::new(root.path());
        let (state, _) = crate::brain_app_state(
            "cap",
            "http://shell.local",
            root.path(),
            root.path().join("private"),
            crate::plugin_policy::RegistrySource::Installed,
            config,
            crate::discovery::FolderSelectionGrantStore::default(),
            None,
            None,
            None,
        );
        assert_eq!(caller_label(&state).unwrap(), "Pocket Mini");
    }
}
