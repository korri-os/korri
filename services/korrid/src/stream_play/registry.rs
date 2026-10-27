//! Playing on a stream never grants control over the far session.
use super::*;
use crate::{
    stream_play::{StreamPlayEndCause, StreamPlayEnded},
    ActiveSession,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Participation {
    Starter,
    StreamPlayer {
        game_id: Option<String>,
        game_title: Option<String>,
        device_label: String,
    },
}

impl UpstreamRegistry {
    pub(crate) fn stream_play_game_id(&self) -> Option<String> {
        match self.selected()?.participation {
            Participation::StreamPlayer { game_id, .. } => game_id,
            _ => None,
        }
    }

    pub(crate) fn stream_play_launch_id(&self) -> Option<String> {
        let selected = self.selected()?;
        matches!(selected.participation, Participation::StreamPlayer { .. })
            .then_some(selected.launch_id)
    }

    /// Only verified same-owner directory members produce idle signals. This
    /// reads the peer directly: session_status() would recover a selection.
    pub(crate) async fn invite_sessions(
        &self,
        same_owner: &BTreeSet<String>,
    ) -> Result<Vec<crate::invites::Invite>, UpstreamError> {
        let registry = self.resolved()?;
        let polls = registry.hosts.iter().filter_map(|host| {
            let key = host.native_route_key()?;
            if !same_owner.contains(&key) {
                return None;
            }
            let RegisteredClient::Native(client) = &host.client else {
                return None;
            };
            Some(async move {
                let status = timeout(Duration::from_secs(2), client.invite_session_status())
                    .await
                    .ok()?
                    .ok()?;
                let crate::SessionStatusOutcome::Ok(status) =
                    crate::session_status_outcome(Ok(host.qualify_status(status)))
                else {
                    return None;
                };
                let active = status.active?;
                Some(crate::invites::Invite {
                    invite_id: String::new(),
                    device_public_key: key,
                    device_label: host.label.clone(),
                    person_name: host.label.clone(),
                    session: active,
                })
            })
        });
        Ok(join_all(polls).await.into_iter().flatten().collect())
    }

    pub(crate) fn has_peer_label(&self, label: &str) -> Result<bool, UpstreamError> {
        Ok(self
            .resolved()?
            .hosts
            .iter()
            .any(|peer| peer.label == label))
    }

    pub(crate) fn clear_stream_play_end(&self) {
        self.last_stream_play_end
            .lock()
            .expect("stream play end mutex poisoned")
            .take();
    }

    pub(crate) fn stream_play_ended(&self) -> Option<StreamPlayEnded> {
        self.last_stream_play_end
            .lock()
            .expect("stream play end mutex poisoned")
            .clone()
    }

    pub(crate) async fn stream_play(
        &self,
        device_public_key: &str,
        launch_id: &str,
        device_label: &str,
    ) -> Result<ActiveSession, UpstreamError> {
        let registry = self.resolved()?;
        let _mutation = registry.remote_prepare_mutation.lock().await;
        if registry.selected().is_some() {
            return Err(UpstreamError::ActiveRemoteSessionConflict);
        }
        let host = registry
            .native_host_by_route_key(device_public_key)
            .ok_or(UpstreamError::SourcePeerNotFound)?;
        let RegisteredClient::Native(client) = &host.client else {
            unreachable!()
        };
        let native_address = host.viewer_address().await.ok();
        let viewer = crate::federation::reach::prepare_peer_viewer(
            registry.directory.as_ref(),
            device_public_key,
            native_address.as_deref(),
        )
        .await?;
        let address = viewer.moonlight_address.clone();
        let mut active = client.stream_play(launch_id, device_label).await?;
        let selected = SelectedRemoteSession {
            participation: Participation::StreamPlayer {
                game_id: active.game_id.clone(),
                game_title: active.title.clone(),
                device_label: host.label.clone(),
            },
            route: SelectedRemoteRoute::Native {
                device_public_key: device_public_key.into(),
            },
            launch_id: launch_id.into(),
            viewer: Some(viewer),
        };
        registry.set_selected(selected.clone());
        if let Err(error) = registry.viewer.start(launch_id, &address).await {
            if let Some(reach) = selected
                .viewer
                .as_ref()
                .and_then(|viewer| viewer.reach.as_ref())
            {
                reach.release();
            }
            // The remote holder and its seat reservations survive every local
            // viewer failure. In particular, never use prepare compensation.
            registry
                .viewer
                .stop(launch_id)
                .await
                .map_err(|cleanup| viewer_error("ViewerControlFailed", cleanup.to_string()))?;
            registry.clear_selected_if(&selected);
            return Err(viewer_error("ViewerStartFailed", error.to_string()));
        }
        registry.set_viewer_confirmed(&selected, true);
        registry
            .last_stream_play_end
            .lock()
            .expect("stream play end mutex poisoned")
            .take();
        active.host = Some(host.label.clone());
        if matches!(active.phase.as_deref(), Some("frozen" | "focus-failed")) {
            registry
                .viewer
                .background(launch_id)
                .await
                .map_err(|error| viewer_error("ViewerControlFailed", error.to_string()))?;
        }
        Ok(active)
    }

    pub(super) async fn stop_stream_player(
        &self,
        selected: &SelectedRemoteSession,
    ) -> Result<UpstreamSessionStop, UpstreamError> {
        self.viewer
            .stop(&selected.launch_id)
            .await
            .map_err(|error| viewer_error("ViewerControlFailed", error.to_string()))?;
        self.clear_selected_if(selected);
        Ok(UpstreamSessionStop::Stopped {
            launch_id: Some(selected.launch_id.clone()),
        })
    }

    pub(super) async fn observe_stream_player(
        &self,
        selected: &SelectedRemoteSession,
    ) -> Result<UpstreamSessionStatus, UpstreamError> {
        let result = self.selected_status(selected).await;
        if let Ok(status) = &result {
            if let Some(active) =
                active_session(status).filter(|a| a.launch_id == selected.launch_id)
            {
                self.recover_viewer(selected, RecoveredFarPhase::parse(active.phase.as_deref())?)
                    .await?;
                return Ok(status.clone());
            }
        }
        match &result {
            Err(error) => eprintln!(
                "korrid: stream play {} ends: holder status read failed: {error:?}",
                selected.launch_id
            ),
            Ok(_) => eprintln!(
                "korrid: stream play {} ends: holder no longer runs it",
                selected.launch_id
            ),
        }
        self.viewer
            .stop(&selected.launch_id)
            .await
            .map_err(|error| viewer_error("ViewerControlFailed", error.to_string()))?;
        if let Participation::StreamPlayer {
            game_id,
            game_title,
            device_label,
        } = &selected.participation
        {
            *self
                .last_stream_play_end
                .lock()
                .expect("stream play end mutex poisoned") = Some(StreamPlayEnded {
                launch_id: selected.launch_id.clone(),
                game_id: game_id.clone(),
                game_title: game_title.clone(),
                device_label: device_label.clone(),
                cause: if result.is_err() {
                    StreamPlayEndCause::DeviceUnreachable
                } else {
                    StreamPlayEndCause::SessionEnded
                },
            });
        }
        self.clear_selected_if(selected);
        Ok(UpstreamSessionStatus::SessionStatus { active: None })
    }
}
