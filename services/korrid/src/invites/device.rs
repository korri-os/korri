//! Idle polling never selects or recovers a far session.
use super::{cache::Cache, *};
use crate::*;
use std::time::Duration;

#[derive(Clone, Default)]
pub(crate) struct Invites {
    cache: Arc<Mutex<Cache>>,
    refresh: Arc<tokio::sync::Mutex<()>>,
}

pub(crate) async fn idle(state: &AppState) -> bool {
    match &state.mode {
        ServerMode::Host(host) => {
            !state
                .remote
                .as_ref()
                .is_some_and(|peers| peers.has_selected_session())
                && matches!(
                    host.session_status().await,
                    Ok(host::control::HostSessionStatus::NoActive
                        | host::control::HostSessionStatus::Completed { .. }
                        | host::control::HostSessionStatus::Failed { .. })
                )
        }
        // Brain-mode recovery can adopt an unselected peer session. It cannot
        // safely offer idle invites until that older policy is removed.
        ServerMode::Brain(_) => false,
    }
}

impl Invites {
    async fn refresh(&self, state: &AppState) {
        let _refresh = self.refresh.lock().await;
        if !idle(state).await {
            self.cache.lock().expect("invites mutex poisoned").clear();
            return;
        }
        let mut sessions = match (&state.remote, &state.federation) {
            (Some(peers), Some(directory)) => match directory.snapshot() {
                Ok(same_owner) => peers
                    .invite_sessions(
                        &same_owner
                            .iter()
                            .map(|peer| peer.device_public_key.clone())
                            .collect(),
                    )
                    .await
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            },
            _ => Vec::new(),
        };
        for invite in &mut sessions {
            if let Some(person) = &invite.session.person_public_key {
                if let Some(name) = state.person_names.name(state, person).await {
                    invite.person_name = name;
                }
            }
        }
        let still_idle = idle(state).await;
        let mut cache = self.cache.lock().expect("invites mutex poisoned");
        // Starts share session_selection; a refresh can cross a start, but it
        // must not republish stale invites after that start.
        if still_idle {
            cache.replace(sessions);
        } else {
            cache.clear();
        }
    }
}

pub(crate) fn spawn_watch(state: AppState) {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(Duration::from_millis(100));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut was_idle = false;
        let mut next = tokio::time::Instant::now();
        loop {
            ticks.tick().await;
            let is_idle = idle(&state).await;
            if is_idle && (!was_idle || tokio::time::Instant::now() >= next) {
                next = tokio::time::Instant::now() + Duration::from_millis(2500);
                state.invites.refresh(&state).await;
            } else if !is_idle {
                state
                    .invites
                    .cache
                    .lock()
                    .expect("invites mutex poisoned")
                    .clear();
            }
            was_idle = is_idle;
        }
    });
}

pub(crate) async fn list(state: &AppState) -> InvitesListOutcome {
    let invites = if idle(state).await {
        state
            .invites
            .cache
            .lock()
            .expect("invites mutex poisoned")
            .list()
    } else {
        state
            .invites
            .cache
            .lock()
            .expect("invites mutex poisoned")
            .clear();
        Vec::new()
    };
    InvitesListOutcome::Ok(InvitesList { invites })
}

fn absent() -> RpcFailure {
    RpcFailure {
        code: "InviteUnavailable".into(),
        message: "This invite is no longer available.".into(),
    }
}

pub(crate) async fn stream_play(
    state: &AppState,
    request: &InviteRequest,
) -> SessionStreamPlayOutcome {
    let result = async {
        if !idle(state).await {
            return Err(RpcFailure {
                code: "ActiveSessionConflict".into(),
                message: "This device already has a live session.".into(),
            });
        }
        let invite = state
            .invites
            .cache
            .lock()
            .expect("invites mutex poisoned")
            .get(&request.invite_id)
            .ok_or_else(absent)?;
        let active = state
            .remote
            .as_ref()
            .ok_or_else(absent)?
            .stream_play(&invite.device_public_key, &invite.session.launch_id)
            .await
            .map_err(upstream_failure)?;
        state
            .invites
            .cache
            .lock()
            .expect("invites mutex poisoned")
            .clear();
        Ok(active)
    }
    .await;
    result
        .map(SessionStreamPlayOutcome::Ok)
        .unwrap_or_else(SessionStreamPlayOutcome::Err)
}

pub(crate) fn dismiss(state: &AppState, request: &InviteRequest) -> InviteDismissOutcome {
    if state
        .invites
        .cache
        .lock()
        .expect("invites mutex poisoned")
        .dismiss(&request.invite_id)
    {
        InviteDismissOutcome::Ok(InviteDismissed {})
    } else {
        InviteDismissOutcome::Err(absent())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{authorization::AuthorizationContext, host::control::InMemoryLaunchUnitBackend};

    #[tokio::test]
    async fn portal_lists_dismisses_and_hides_invites_when_this_device_starts() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(
            &config,
            "label = \"local\"\n[[games]]\nid = \"game\"\ntitle = \"Game\"\ncommand = [\"game\"]\n",
        )
        .unwrap();
        let host = host::HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.path().join("private"),
            Arc::new(InMemoryLaunchUnitBackend::default()),
        );
        host.claim_save_owner();
        let (state, _) = app_states(host.clone());
        let invite: Invite = serde_json::from_value(serde_json::json!({
            "inviteId":"", "devicePublicKey":"far", "deviceLabel":"Far", "personName":"Person",
            "session":{"launchId":"exact", "title":"Game", "phase":"running"}
        }))
        .unwrap();
        state
            .invites
            .cache
            .lock()
            .unwrap()
            .replace(vec![invite.clone()]);
        let request = || RpcRequest::InvitesList(InvitesListRequest {});
        let response = crate::dispatch(&state, &AuthorizationContext::LocalBrowser, request())
            .await
            .unwrap();
        let RpcResponse::InvitesList(InvitesListOutcome::Ok(list)) = response else {
            panic!("list failed")
        };
        assert_eq!(list.invites.len(), 1);
        let response = crate::dispatch(
            &state,
            &AuthorizationContext::LocalBrowser,
            RpcRequest::InvitesDismiss(InviteRequest {
                invite_id: list.invites[0].invite_id.clone(),
            }),
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            RpcResponse::InvitesDismiss(InviteDismissOutcome::Ok(_))
        ));
        state.invites.cache.lock().unwrap().replace(vec![invite]);
        let InvitesListOutcome::Ok(list) = list_for_test(&state).await else {
            panic!("list failed")
        };
        assert!(list.invites.is_empty());
        state
            .invites
            .cache
            .lock()
            .unwrap()
            .replace(vec![serde_json::from_value(serde_json::json!({
                "inviteId":"", "devicePublicKey":"far", "deviceLabel":"Far", "personName":"Person",
                "session":{"launchId":"next", "phase":"running"}
            }))
            .unwrap()]);
        host.prepare("game", None).await.unwrap();
        let InvitesListOutcome::Ok(list) = list_for_test(&state).await else {
            panic!("list failed")
        };
        assert!(list.invites.is_empty());
    }

    async fn list_for_test(state: &AppState) -> InvitesListOutcome {
        super::list(state).await
    }
}
