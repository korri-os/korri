//! Native launch facts and effects for ADR 0040, kept behind the holder's
//! exact-session transition guard.
use super::identity::{read_active, ActiveSession};
use super::retroarch_control::RetroarchCommand;
use super::*;
use crate::launcher::plugin_launch::{LaunchComposition, PluginLaunchInput};
use crate::netplay::{NetplayRunner, NetplaySession, PluginLaunchNetplay};

pub(crate) struct NetplayLocal {
    pub facts: NetplaySession,
    pub content: PathBuf,
    pub config: PathBuf,
}
impl HostRuntime {
    pub(crate) fn netplay_launch_input(
        &self,
        command: &mut [String],
        client: Option<(PluginLaunchNetplay, PathBuf)>,
    ) -> Result<(), RpcFailure> {
        if command.get(1).map(String::as_str) != Some("plugin-prepare") {
            return Ok(());
        }
        let mut input: PluginLaunchInput = serde_json::from_str(&command[3])
            .map_err(|_| dynamic_failure("invalid launch input"))?;
        let registry = self.route_registry()?;
        if registry
            .native_runner(&input.runner_id)
            .ok()
            .and_then(|runner| runner.netplay.as_ref())
            .is_none()
        {
            return Ok(());
        }
        let mut composition: LaunchComposition = serde_json::from_str(&command[4])
            .map_err(|_| dynamic_failure("invalid launch composition"))?;
        let (netplay, source) = match client {
            Some(client) => client,
            None => (
                PluginLaunchNetplay::Host {
                    password_config: crate::netplay::password::GAME_CONFIG.into(),
                },
                crate::netplay::password::create(&self.private_state_root, None, None)
                    .map_err(|_| dynamic_failure("netplay credential could not be created"))?,
            ),
        };
        if let Some(previous) = composition.netplay_config_source.take() {
            let _ = std::fs::remove_file(previous);
        }
        if netplay.is_client() {
            input.save_root = "/tmp/korri-netplay-saves".into();
        }
        input.netplay = Some(netplay);
        composition.netplay_config_source = Some(source);
        command[3] =
            serde_json::to_string(&input).map_err(|_| dynamic_failure("invalid launch input"))?;
        command[4] = serde_json::to_string(&composition)
            .map_err(|_| dynamic_failure("invalid launch composition"))?;
        Ok(())
    }
    pub(crate) fn netplay_local(&self, expected: &str) -> Result<NetplayLocal, RpcFailure> {
        let (game, runner, release, config) = self.control()?.netplay_launch(expected)?;
        let root = self.route_root()?;
        let registry = self.route_registry()?;
        let runner_record = registry
            .native_runner(&runner)
            .map_err(|_| dynamic_failure("netplay runner unavailable"))?;
        let netplay = runner_record
            .netplay
            .clone()
            .ok_or_else(|| dynamic_failure("netplay runner unavailable"))?;
        let (snapshot, _) = crate::config::settings::runner_choice_snapshot(root)
            .map_err(crate::game_routes::settings_failure)?;
        if snapshot.library.get(&game).into_iter().flat_map(|game| &game.releases).any(|candidate| matches!(candidate.identity.as_ref(), Some(crate::config::library::LibraryIdentity::Parts { value, .. }) if value == &release)) {
            return Err(crate::netplay::failure("MultipartReleaseUnsupported", "This release needs several files. Use stream play."));
        }
        let route = crate::config::resolver::resolve_linux_route(
            root,
            &snapshot,
            &registry,
            &game,
            Some(&runner),
        )
        .map_err(|error| crate::route_diagnostic_failure(&error))?;
        if route.release_id != release {
            return Err(dynamic_failure("live release route changed"));
        }
        let launch = crate::launcher::linux_plugin::launch_route(
            root,
            &snapshot,
            &registry,
            &route,
            None,
            None,
            &self.launch_person()?,
        )
        .map_err(|error| dynamic_failure(error.to_string()))?;
        let input: PluginLaunchInput = serde_json::from_str(&launch.command[3])
            .map_err(|_| dynamic_failure("invalid launch input"))?;
        let content = PathBuf::from(
            input
                .content_path
                .ok_or_else(|| dynamic_failure("netplay requires file content"))?,
        );
        if std::fs::read_to_string(&config)
            .is_ok_and(|text| text.contains("netplay_request_device_p"))
        {
            return Err(crate::netplay::failure(
                "NoNetplayRoute",
                "A receiving device cannot host this session.",
            ));
        }
        let metadata = std::fs::metadata(&content)
            .map_err(|_| dynamic_failure("netplay release unavailable"))?;
        if !metadata.is_file() || crate::acquisition::placement::ready_name(&release).is_none() {
            return Err(dynamic_failure("netplay requires a file release"));
        }
        let plugin_id = runner
            .rsplit_once('/')
            .ok_or_else(|| dynamic_failure("invalid runner identity"))?
            .0
            .to_owned();
        Ok(NetplayLocal {
            facts: NetplaySession {
                release_key: release,
                release_bytes: metadata.len(),
                runner: NetplayRunner {
                    plugin_id,
                    runner_id: runner,
                    netplay,
                },
            },
            content,
            config: config.into(),
        })
    }
    pub(crate) fn netplay_host(&self, expected: &str) -> Result<(), RpcFailure> {
        self.control()?.observe_status(|_, _, _| {
            // The guard spans the command/readback, so End or replacement
            // cannot turn the toggle into an effect on another launch.
            let record = read_active(&self.private_state_root.join("host-session"))
                .map_err(|_| dynamic_failure("live session unavailable"))?;
            if record.as_ref().map(ActiveSession::launch_id) != Some(expected) {
                return Err(dynamic_failure("live session changed"));
            }
            let command = self
                .retroarch_control
                .prepare(RetroarchCommand::NetplayHostToggle)
                .and_then(|command| self.retroarch_control.invoke(command));
            command.map_err(|_| {
                crate::netplay::failure(
                    "NoNetplayRoute",
                    "RetroArch could not start netplay hosting.",
                )
            })
        })
    }
    pub(crate) fn netplay_reserve_seat(
        &self,
        launch_id: &str,
        device: &str,
    ) -> Result<u8, RpcFailure> {
        self.control()?.observe_status(|_, _, _| {
            match read_active(&self.private_state_root.join("host-session"))
                .map_err(|_| dynamic_failure("live session unavailable"))?
            {
                Some(ActiveSession::Running {
                    launch_id: live,
                    stream_players,
                    ..
                }) if live == launch_id => {
                    if stream_players
                        .iter()
                        .any(|player| player.device_public_key != device)
                    {
                        return Err(crate::netplay::failure(
                            "NetplayReceiverPresent",
                            "This session already has a receiving device.",
                        ));
                    }
                }
                _ => {
                    return Err(crate::netplay::failure(
                        "LaunchMismatch",
                        "That launch is no longer live.",
                    ))
                }
            }
            self.input_coordinator
                .as_ref()
                .ok_or_else(|| dynamic_failure("input coordinator unavailable"))?
                .request(korri_input_contract::SeatRequest::ReserveNetplay {
                    launch_id: launch_id.into(),
                    device_public_key: device.into(),
                })
                .map_err(dynamic_failure)?
                .slot
                .ok_or_else(|| dynamic_failure("no free controller seat"))
        })
    }
    pub(crate) fn prepare_netplay_client(
        &self,
        game: &str,
        runner: &str,
        release: &str,
        input: PluginLaunchNetplay,
        source: PathBuf,
    ) -> Result<crate::game_routes::SelectedGameLaunch, RpcFailure> {
        let mut launch = self.prepare_launch_command(game, Some(runner), None)?;
        if launch.command.get(5).map(String::as_str) != Some(release) {
            let root = self.route_root()?;
            let registry = self.route_registry()?;
            let (snapshot, _) = crate::config::settings::runner_choice_snapshot(root)
                .map_err(crate::game_routes::settings_failure)?;
            let routes = crate::config::resolver::resolve_launchable_routes(
                root,
                &snapshot,
                &registry,
                std::iter::empty(),
            );
            let route = routes
                .routes
                .iter()
                .find(|route| {
                    route.playable_id == game
                        && route.runner_id == runner
                        && route.release_id == release
                })
                .ok_or_else(|| {
                    crate::netplay::failure("NoNetplayRoute", "The exact release cannot run here.")
                })?;
            launch.command = crate::launcher::linux_plugin::launch_route(
                root,
                &snapshot,
                &registry,
                route,
                None,
                None,
                &self.launch_person()?,
            )
            .map_err(|error| dynamic_failure(error.to_string()))?
            .command;
        }
        self.netplay_launch_input(&mut launch.command, Some((input, source)))?;
        self.control()?.prepare_with_limits(
            game,
            Some(&self.launch_person()?),
            Ok(&launch.command),
            &self.config.as_ref().map_err(config_failure)?.environment,
            launch.limits.as_ref(),
            launch.warnings,
            None,
        )
    }
}
impl HostSessionControl {
    fn netplay_launch(
        &self,
        expected: &str,
    ) -> Result<(String, String, String, String), RpcFailure> {
        self.observe_status(|status, _, _| {
            let crate::SessionStatusOutcome::Ok(status) =
                crate::host_session_status_outcome(Ok(status))
            else {
                return Err(crate::netplay::failure(
                    "NoNetplayRoute",
                    "No live session.",
                ));
            };
            if status.active.as_ref().is_none_or(|active| {
                active.launch_id != expected || active.phase.as_deref() == Some("stopping")
            }) {
                return Err(crate::netplay::failure(
                    "LaunchMismatch",
                    "That launch is no longer live.",
                ));
            }
            match read_active(&self.identity_root)
                .map_err(|_| dynamic_failure("live session unavailable"))?
            {
                Some(ActiveSession::Running {
                    game_id,
                    runner_id: Some(runner),
                    release_id: Some(release),
                    netplay_config: Some(config),
                    ..
                }) => Ok((game_id, runner, release, config)),
                _ => Err(crate::netplay::failure(
                    "NoNetplayRoute",
                    "This live session cannot host netplay.",
                )),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        authorization::{AuthorizationContext, Principal},
        netplay::{SessionNetplayOutcome, SessionNetplayRequest},
        RpcRequest, RpcResponse,
    };
    use sha2::{Digest, Sha256};
    use std::{collections::BTreeMap, io::Read, sync::Mutex};
    #[derive(Default)]
    struct RecordedControl(Mutex<Vec<RetroarchCommand>>);
    impl RetroarchControlExecutor for RecordedControl {
        fn prepare(
            &self,
            command: RetroarchCommand,
        ) -> Result<super::super::retroarch_control::PreparedRetroarchCommand, String> {
            assert_eq!(command, RetroarchCommand::NetplayHostToggle);
            Ok(super::super::retroarch_control::PreparedRetroarchCommand::NetplayHostToggle)
        }
        fn invoke(
            &self,
            _: super::super::retroarch_control::PreparedRetroarchCommand,
        ) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .push(RetroarchCommand::NetplayHostToggle);
            Ok(())
        }
    }
    fn peer(device: &str) -> AuthorizationContext {
        AuthorizationContext::Peer(Principal::OwnerDevice {
            device_public_key: device.into(),
            owner_public_key: "44".repeat(32),
            owner_statement: "verified".into(),
        })
    }
    #[tokio::test]
    async fn netplay_peer_preparation_requires_exact_live_launch_one_receiver_and_its_object_seat()
    {
        use super::super::input_coordination::test_support;
        let root = tempfile::tempdir().unwrap();
        let key = format!("sha256:{}", hex::encode(Sha256::digest(b"game bytes")));
        std::fs::create_dir_all(root.path().join("roms")).unwrap();
        std::fs::create_dir_all(root.path().join("catalog")).unwrap();
        std::fs::write(root.path().join("roms/Turtles.sfc"), b"game bytes").unwrap();
        std::fs::write(
            root.path().join("device.yaml"),
            format!("locations:\n  '{key}':\n    - storage: roms\n      path: Turtles.sfc\n"),
        )
        .unwrap();
        std::fs::write(root.path().join("catalog/library.yaml"), format!("library:\n  turtles:\n    title: Turtles\n    releases:\n      - id: snes\n        system: snes\n        identity: {{kind: hash, value: '{key}'}}\n")).unwrap();
        let package = root.path().join("plugin");
        std::fs::create_dir(&package).unwrap();
        std::fs::write(package.join("plugin.ts"), r#"
            export const name='core';
            export const systems={snes:{id:'snes',title:'Super Nintendo',content:{extensions:['sfc']}}};
            export const runners={core:{id:'@test:core/core',program:'program',systems:['snes'],netplay:{library:'Snes9x 2010',version:'1.52.4fe690dd'}}};
            export const handlers={'launch.prepare':input=>({command:input.program,args:[],saveLocations:[{key:'saves',kind:'save',path:input.saveRoot+'/saves'}]})};
        "#).unwrap();
        std::fs::write(
            package.join("manifest.json"),
            r#"{"entry":"plugin.ts","sources":["plugin.ts"]}"#,
        )
        .unwrap();
        let registry = crate::plugin::PluginRegistry::from_installed(vec![
            crate::plugin_installation::EnabledPackage {
                id: "@test:core".into(),
                package,
                entry: "plugin.ts".into(),
                sources: vec!["plugin.ts".into()],
                requires: vec![],
                hosts: vec![],
                files: BTreeMap::from([("program".into(), std::env::current_exe().unwrap())]),
            },
        ])
        .unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"Holder\"\ngames = []\n").unwrap();
        let mut host = HostRuntime::from_paths_with_backend(
            &config,
            Some(root.path().into()),
            root.path().join("private"),
            Arc::new(super::super::systemd_unit::InMemoryLaunchUnitBackend::default()),
        )
        .with_route_registry(root.path().into(), registry);
        let toggles = Arc::new(RecordedControl::default());
        host.retroarch_control = toggles.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let (coordinator, receiver) = test_support::coordinator(move |request| {
            let mut reply = test_support::reply();
            if matches!(
                request,
                korri_input_contract::SeatRequest::ReserveNetplay { .. }
            ) {
                reply.slot = Some(2);
            }
            seen.lock().unwrap().push(request);
            reply
        });
        host.input_coordinator = Some(coordinator.clone());
        host.claim_save_owner();
        let live = host
            .prepare("turtles", host.owner_public_key().as_deref())
            .await
            .unwrap()
            .session;
        let (state, _) = crate::app_states(host.clone());
        let device = "33".repeat(32);
        let other = "55".repeat(32);
        let request = |launch: &str| {
            RpcRequest::SessionNetplay(SessionNetplayRequest {
                launch_id: launch.into(),
                device_label: "Receiver".into(),
            })
        };
        let response = crate::dispatch(&state, &peer(&device), request("wrong"))
            .await
            .unwrap();
        assert!(
            matches!(response, RpcResponse::SessionNetplay(SessionNetplayOutcome::Err(crate::RpcFailure { code, .. })) if code == "LaunchMismatch")
        );
        let response = crate::dispatch(&state, &peer(&device), request(&live.launch_id))
            .await
            .unwrap();
        let RpcResponse::SessionNetplay(SessionNetplayOutcome::Ok(prepared)) = response else {
            panic!("{response:?}");
        };
        assert_eq!(prepared.seat, 2);
        assert_eq!(prepared.port, 55435);
        assert_eq!(prepared.release_key, key);
        assert_eq!(prepared.release_bytes, 10);
        assert_eq!(prepared.runner.netplay.version, "1.52.4fe690dd");
        assert!(
            matches!(crate::dispatch(&state, &peer(&other), request(&live.launch_id)).await.unwrap(), RpcResponse::SessionNetplay(SessionNetplayOutcome::Err(crate::RpcFailure { code, .. })) if code == "NetplayReceiverPresent")
        );
        assert!(matches!(
            crate::dispatch(&state, &peer(&device), request(&live.launch_id))
                .await
                .unwrap(),
            RpcResponse::SessionNetplay(SessionNetplayOutcome::Ok(_))
        ));
        assert_eq!(toggles.0.lock().unwrap().len(), 1);
        assert_eq!(
            requests
                .lock()
                .unwrap()
                .iter()
                .filter(|request| matches!(
                    request,
                    korri_input_contract::SeatRequest::ReserveNetplay { .. }
                ))
                .count(),
            1
        );
        let mut file = crate::netplay::holder::object(&state, &peer(&device), &key)
            .await
            .unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"game bytes");
        assert!(crate::netplay::holder::object(&state, &peer(&other), &key)
            .await
            .is_err());
        assert!(crate::netplay::holder::object(
            &state,
            &peer(&device),
            &format!("sha256:{}", "0".repeat(64))
        )
        .await
        .is_err());
        host.session_stop(&live.launch_id).await.unwrap();
        assert!(crate::netplay::holder::object(&state, &peer(&device), &key)
            .await
            .is_err());
        // The holder cannot serve a multipart release through this one-file
        // treaty. Use the existing ordered-parts producer's aggregate key.
        let parts = [
            format!("sha256:{}", "1".repeat(64)),
            format!("sha256:{}", "2".repeat(64)),
        ];
        let multipart = format!(
            "sha256:{}",
            hex::encode(Sha256::digest(parts.join("\n").as_bytes()))
        );
        std::fs::write(root.path().join("catalog/library.yaml"), format!("library:\n  turtles:\n    title: Turtles\n    releases:\n      - id: snes\n        system: snes\n        identity: {{kind: parts, value: '{multipart}', parts: ['{}', '{}']}}\n", parts[0], parts[1])).unwrap();
        std::fs::write(
            root.path().join("device.yaml"),
            format!("locations:\n  '{multipart}':\n    - storage: roms\n      path: Turtles.sfc\n"),
        )
        .unwrap();
        let multipart_live = host
            .prepare("turtles", host.owner_public_key().as_deref())
            .await
            .unwrap()
            .session;
        let mut description: crate::ActiveSession = serde_json::from_value(
            serde_json::json!({"launchId":multipart_live.launch_id,"gameId":"turtles"}),
        )
        .unwrap();
        host.describe_active_session(&mut description).unwrap();
        assert!(description.netplay.is_none());
        assert!(
            matches!(crate::dispatch(&state, &peer(&device), request(&multipart_live.launch_id)).await.unwrap(), RpcResponse::SessionNetplay(SessionNetplayOutcome::Err(crate::RpcFailure { code, .. })) if code == "MultipartReleaseUnsupported")
        );
        host.session_stop(&multipart_live.launch_id).await.unwrap();
        coordinator.fence();
        receiver.join().unwrap();
    }
}
