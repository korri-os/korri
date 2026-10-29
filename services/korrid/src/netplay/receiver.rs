use super::*;
use crate::*;
use futures::StreamExt;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;

const RECONNECT_WINDOW: Duration = Duration::from_secs(10);
const MONITOR_INTERVAL: Duration = Duration::from_millis(500);
const STATUS_TIMEOUT: Duration = Duration::from_secs(3);
#[derive(Clone, Default)]
pub(crate) struct Receiver(Arc<Mutex<State>>);
#[derive(Default)]
enum State {
    #[default]
    Idle,
    Preparing(Run),
    Active {
        run: Run,
        session: Box<ActiveSession>,
    },
    Ended(StreamPlayEnded),
}
#[derive(Clone)]
struct Run {
    id: String,
    status: NetplayStatus,
    cancel: watch::Sender<bool>,
}
impl State {
    fn run(&self) -> Option<&Run> {
        match self {
            Self::Preparing(run) | Self::Active { run, .. } => Some(run),
            Self::Idle | Self::Ended(_) => None,
        }
    }
    fn run_mut(&mut self) -> Option<&mut Run> {
        match self {
            Self::Preparing(run) | Self::Active { run, .. } => Some(run),
            Self::Idle | Self::Ended(_) => None,
        }
    }
}
impl Receiver {
    pub(crate) fn busy(&self) -> bool {
        self.0.lock().expect("netplay receiver").run().is_some()
    }
    fn begin(&self, status: NetplayStatus) -> Result<(String, watch::Receiver<bool>), RpcFailure> {
        let mut local = self.0.lock().expect("netplay receiver");
        if local.run().is_some() {
            return Err(failure(
                "ActiveSessionConflict",
                "This device already has a live session.",
            ));
        }
        let id = status.launch_id.clone();
        let (cancel, cancelled) = watch::channel(false);
        *local = State::Preparing(Run {
            id: id.clone(),
            status,
            cancel,
        });
        Ok((id, cancelled))
    }
    fn failed_preparation(&self, id: &str) {
        let mut local = self.0.lock().expect("netplay receiver");
        if matches!(&*local, State::Preparing(run) if run.id == id) {
            *local = State::Idle;
        }
    }
    fn activate(&self, id: &str, session: ActiveSession) -> Result<(), RpcFailure> {
        let mut local = self.0.lock().expect("netplay receiver");
        let State::Preparing(run) = &*local else {
            return Err(failure("NoNetplayRoute", "The launch was cancelled."));
        };
        if run.id != id || *run.cancel.borrow() {
            return Err(failure("NoNetplayRoute", "The launch was cancelled."));
        }
        let mut run = run.clone();
        // The public receiving run survives replacement native clients.
        run.status.phase = NetplayPhase::Connecting;
        *local = State::Active {
            run,
            session: Box::new(session),
        };
        Ok(())
    }
    fn phase(&self, id: &str, phase: NetplayPhase) {
        if let Some(run) = self
            .0
            .lock()
            .expect("netplay receiver")
            .run_mut()
            .filter(|run| run.id == id)
        {
            run.status.phase = phase;
        }
    }
    fn owns_copy(&self, id: &str, launch: &str) -> bool {
        matches!(&*self.0.lock().expect("netplay receiver"), State::Active { run, session } if run.id == id && session.launch_id == launch)
    }
    fn retarget(&self, id: &str, launch: &str) {
        let mut local = self.0.lock().expect("netplay receiver");
        if let State::Active { run, session } = &mut *local {
            if run.id == id {
                session.launch_id = launch.into();
            }
        }
    }
}

pub(crate) fn availability(
    facts: &NetplaySession,
    matching_core: bool,
    conflicting_core: bool,
    has_release: bool,
    enough_space: bool,
    plugin_title: Option<&str>,
) -> InviteNetplay {
    if conflicting_core {
        return InviteNetplay::Unavailable {
            reason: "CoreVersionMismatch".into(),
        };
    }
    if !has_release && !enough_space {
        return InviteNetplay::Unavailable {
            reason: "NotEnoughSpace".into(),
        };
    }
    if !matching_core {
        return match plugin_title {
            Some(title) => InviteNetplay::NeedsPlugin {
                plugin_id: facts.runner.plugin_id.clone(),
                title: title.into(),
                bytes: (!has_release).then_some(facts.release_bytes as f64),
            },
            None => InviteNetplay::Unavailable {
                reason: "NoNetplayRoute".into(),
            },
        };
    }
    if has_release {
        InviteNetplay::Ready
    } else {
        InviteNetplay::NeedsCopy {
            bytes: facts.release_bytes as f64,
        }
    }
}
fn release_game(context: &acquisition::GameContext, key: &str) -> Option<String> {
    let snapshot = context.discovery.published().ok()?;
    let routes = config::resolver::resolve_launchable_routes(
        &context.root,
        &snapshot,
        &context.registry,
        std::iter::empty(),
    );
    routes
        .routes
        .into_iter()
        .find(|route| {
            route.release_id == key
                && match (
                    &route.file_target,
                    &route.artifact_target,
                    &route.package_target,
                    &route.cache_target,
                ) {
                    (Some(target), _, _, _) => {
                        config::storage::resolve_file_target(&context.root, &snapshot, target)
                            .is_ok()
                    }
                    (_, Some(target), _, _) => {
                        acquisition::artifact_content_path(&context.root, &context.registry, target)
                            .is_ok()
                    }
                    (_, _, Some(target), _) => {
                        config::resolver::package_content_path(&context.registry, target)
                            .is_ok_and(|path| path.is_file())
                    }
                    (_, _, _, Some(path)) => path.is_file(),
                    _ => false,
                }
        })
        .map(|route| route.playable_id)
}
fn core(context: &acquisition::GameContext, facts: &NetplaySession) -> Option<String> {
    context
        .registry
        .runners()
        .values()
        .find(|runner| runner.netplay.as_ref() == Some(&facts.runner.netplay))
        .map(|runner| runner.id.clone())
}
async fn plugin_title(state: &AppState, id: &str) -> Option<String> {
    use plugin_admin::{PluginAdminRequest as Request, PluginAdminResponse as Response};
    let admin = state.plugin_admin.as_ref()?;
    let Response::Repositories(repositories) = admin
        .request(&Request::Repositories(
            plugin_admin::PluginRepositoriesRequest {},
        ))
        .await
        .ok()?
    else {
        return None;
    };
    for url in repositories
        .official
        .into_iter()
        .chain(repositories.user_added)
    {
        if let Ok(Response::Catalog(catalog)) = admin
            .request(&Request::Catalog(plugin_admin::PluginRepositoryRequest {
                url,
            }))
            .await
        {
            if let Some(entry) = catalog.entries.iter().find(|entry| entry.plugin_id == id) {
                return Some(entry.title.clone().unwrap_or_else(|| id.into()));
            }
        }
    }
    None
}
pub(crate) async fn project(state: &AppState, invite: &mut invites::Invite) {
    let Some(facts) = &invite.session.netplay else {
        invite.netplay = None;
        return;
    };
    let Ok(context) = state.game_context() else {
        invite.netplay = Some(InviteNetplay::Unavailable {
            reason: "NoNetplayRoute".into(),
        });
        return;
    };
    let has_release = release_game(&context, &facts.release_key).is_some();
    let matching = core(&context, facts).is_some();
    let conflicting = !matching
        && context.registry.runners().values().any(|runner| {
            runner
                .netplay
                .as_ref()
                .is_some_and(|netplay| netplay.library == facts.runner.netplay.library)
        });
    let title = if matching || conflicting {
        None
    } else {
        tokio::time::timeout(
            Duration::from_secs(2),
            plugin_title(state, &facts.runner.plugin_id),
        )
        .await
        .ok()
        .flatten()
    };
    invite.netplay = Some(availability(
        facts,
        matching,
        conflicting,
        has_release,
        acquisition::netplay::room(&context.root, facts.release_bytes),
        title.as_deref(),
    ));
}

pub(crate) async fn start(state: &AppState, request: &InviteRequest) -> SessionStreamPlayOutcome {
    start_inner(state, request)
        .await
        .map(SessionStreamPlayOutcome::Ok)
        .unwrap_or_else(SessionStreamPlayOutcome::Err)
}

async fn start_inner(
    state: &AppState,
    request: &InviteRequest,
) -> Result<ActiveSession, RpcFailure> {
    if !invites::device::idle(state).await {
        return Err(failure(
            "ActiveSessionConflict",
            "This device already has a live session.",
        ));
    }
    let invite = state
        .invites
        .netplay_invite(&request.invite_id)
        .ok_or_else(|| failure("NoNetplayRoute", "This invite is no longer available."))?;
    let facts = invite
        .session
        .netplay
        .as_ref()
        .cloned()
        .ok_or_else(|| failure("NoNetplayRoute", "This runner cannot use netplay."))?;
    let context = state.game_context()?;
    let runner = core(&context, &facts).ok_or_else(|| {
        failure(
            "CoreVersionMismatch",
            &format!(
                "{} runs a different version of {}",
                invite.device_label, facts.runner.netplay.library
            ),
        )
    })?;
    let peer = state
        .remote
        .as_ref()
        .ok_or_else(|| failure("NoNetplayRoute", "No peer route is available."))?
        .netplay_client(&invite.device_public_key)
        .map_err(|_| failure("NoNetplayRoute", "No peer route is available."))?;
    let directory = state
        .federation
        .as_ref()
        .ok_or_else(|| failure("NoNetplayRoute", "No federation route is available."))?;
    let (attempt, mut cancelled) = state.netplay_receiver.begin(NetplayStatus {
        launch_id: crate::generate_launch_id(),
        device_label: invite.device_label.clone(),
        game_title: invite.session.title.clone(),
        phase: NetplayPhase::Connecting,
    })?;
    let result = async {
        // Holder preparation reserves the authenticated seat before the object RPC.
        let label = stream_play::registration::caller_label(state)?;
        let prepared = tokio::select! {
            _ = cancelled.changed() => return Err(failure("NoNetplayRoute", "The launch was cancelled.")),
            response = peer.call(RpcRequest::SessionNetplay(SessionNetplayRequest { launch_id: invite.session.launch_id.clone(), device_label: label })) => match response.map_err(|_| failure("NoNetplayRoute", "The holding device could not prepare netplay."))? {
                RpcResponse::SessionNetplay(SessionNetplayOutcome::Ok(prepared)) => prepared,
                RpcResponse::SessionNetplay(SessionNetplayOutcome::Err(_)) => return Err(failure("NoNetplayRoute", "The holding device could not prepare netplay.")),
                _ => return Err(failure("NoNetplayRoute", "The holding device returned an invalid reply.")),
            }
        };
        if prepared.runner.netplay != facts.runner.netplay {
            return Err(failure(
                "CoreVersionMismatch",
                &format!(
                    "{} runs a different version of {}",
                    invite.device_label, facts.runner.netplay.library
                ),
            ));
        }
        if prepared.release_key != facts.release_key || prepared.release_bytes != facts.release_bytes {
            return Err(failure(
                "CopyFailed",
                "The holding device's release changed.",
            ));
        }
        let game = match release_game(&context, &prepared.release_key) {
            Some(game) => game,
            None => {
                if !acquisition::netplay::room(&context.root, prepared.release_bytes) {
                    return Err(failure(
                        "NotEnoughSpace",
                        &format!("The copy needs {} bytes.", prepared.release_bytes),
                    ));
                }
                state.netplay_receiver.phase(&attempt, NetplayPhase::Copying {
                    done: 0,
                    total: prepared.release_bytes,
                });
                let transfer = async {
                    let _turn = acquisition::netplay::turn(
                        &state.acquisition,
                        &context.root,
                        &prepared.release_key,
                    )
                    .await;
                    let mut response = peer
                        .release_object(&prepared.release_key)
                        .await
                        .map_err(|_| failure("CopyFailed", "The release could not be copied."))?;
                    if !response.status().is_success()
                        || response.content_length() != Some(prepared.release_bytes)
                    {
                        return Err(failure(
                            "CopyFailed",
                            "The release copy has an unexpected length.",
                        ));
                    }
                    let encoded = response
                        .headers()
                        .get(axum::http::header::CONTENT_DISPOSITION)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.strip_prefix("attachment; filename*=UTF-8''"))
                        .ok_or_else(|| failure("CopyFailed", "The release has no file name."))?;
                    let encoded = format!("name={encoded}");
                    let name = url::form_urlencoded::parse(encoded.as_bytes())
                        .next()
                        .map(|(_, name)| name.into_owned())
                        .ok_or_else(|| failure("CopyFailed", "The release name is invalid."))?;
                    let writer_context = context.clone();
                    let bytes = prepared.release_bytes;
                    let mut copy = tokio::task::spawn_blocking(move || {
                        acquisition::netplay::Copy::begin(&writer_context, &name, bytes)
                    })
                    .await
                    .map_err(|_| failure("CopyFailed", "The release copy could not start."))??;
                    let mut done = 0;
                    loop {
                        let chunk = tokio::time::timeout(Duration::from_secs(60), response.chunk())
                            .await
                            .map_err(|_| failure("CopyFailed", "The release copy stopped."))?
                            .map_err(|_| failure("CopyFailed", "The release copy failed."))?;
                        let Some(chunk) = chunk else {
                            break;
                        };
                        let count = chunk.len() as u64;
                        copy = tokio::task::spawn_blocking(move || {
                            copy.write(&chunk)?;
                            Ok::<_, RpcFailure>(copy)
                        })
                        .await
                        .map_err(|_| failure("CopyFailed", "The release copy failed."))??;
                        done += count;
                        state.netplay_receiver.phase(&attempt, NetplayPhase::Copying {
                            done,
                            total: prepared.release_bytes,
                        });
                    }
                    let context = context.clone();
                    let key = prepared.release_key.clone();
                    let title = invite
                        .session
                        .title
                        .clone()
                        .unwrap_or_else(|| "Game".into());
                    tokio::task::spawn_blocking(move || copy.finish(&context, &key, &title))
                        .await
                        .map_err(|_| failure("CopyFailed", "The release could not be verified."))?
                };
                tokio::select! {
                    _ = cancelled.changed() => return Err(failure("NoNetplayRoute", "The launch was cancelled.")),
                    result = transfer => result?,
                }
            }
        };
        state.netplay_receiver.phase(&attempt, NetplayPhase::Connecting);
        let forward = tokio::select! {
            _ = cancelled.changed() => return Err(failure("NoNetplayRoute", "The launch was cancelled.")),
            forward = directory.netplay_forward(&invite.device_public_key) => forward.map_err(|_| failure("NoNetplayRoute", "No federation route is available."))?,
        };
        let source = password::create(
            state.private_state_root(),
            Some(prepared.seat),
            Some(&prepared.password),
        )
        .map_err(|_| {
            failure(
                "NoNetplayRoute",
                "The session credential could not be created.",
            )
        })?;
        let input = PluginLaunchNetplay::Client {
            address: forward.address.ip().to_string(),
            port: forward.address.port(),
            player: prepared.seat,
            password_config: password::GAME_CONFIG.into(),
        };
        let ServerMode::Host(host) = &state.mode else {
            return Err(failure(
                "NoNetplayRoute",
                "A local game runner is required.",
            ));
        };
        let host = host.clone();
        let release = prepared.release_key.clone();
        let launched = tokio::task::spawn_blocking(move || {
            host.prepare_netplay_client(&game, &runner, &release, input, source)
        })
        .await
        .map_err(|_| failure("NoNetplayRoute", "The local game could not start."))??;
        let ServerMode::Host(host) = &state.mode else {
            unreachable!()
        };
        let mut active = match crate::host_session_status_outcome(host.session_status().await) {
            SessionStatusOutcome::Ok(status) => status
                .active
                .ok_or_else(|| failure("NoNetplayRoute", "The local game did not start."))?,
            SessionStatusOutcome::Err(error) => return Err(error),
        };
        active.title = invite.session.title.clone();
        if *cancelled.borrow() {
            let _ = host.session_stop(&launched.session.launch_id).await;
            return Err(failure("NoNetplayRoute", "The launch was cancelled."));
        }
        state.netplay_receiver.activate(&attempt, active.clone())?;
        state.invites.netplay_clear();
        tokio::spawn(monitor(
            state.clone(),
            invite,
            peer,
            MonitoredCopy { attempt_id: attempt.clone(), launch_id: active.launch_id.clone(), forward },
            prepared,
            cancelled,
        ));
        active.launch_id = attempt.clone();
        Ok(active)
    }.await;
    if result.is_err() {
        // A refused second Start never owns the first run's state or sender.
        state.netplay_receiver.failed_preparation(&attempt);
    }
    result
}

struct MonitoredCopy {
    attempt_id: String,
    launch_id: String,
    forward: federation::reach::NetplayForward,
}
enum ConnectionState {
    Connecting { since: tokio::time::Instant },
    Playing,
    Reconnecting { since: tokio::time::Instant },
}

async fn monitor(
    state: AppState,
    invite: invites::Invite,
    peer: upstream_native::NativeClient,
    mut copy: MonitoredCopy,
    prepared: SessionNetplayPrepared,
    mut cancelled: watch::Receiver<bool>,
) {
    let mut connection = ConnectionState::Connecting {
        since: tokio::time::Instant::now(),
    };
    let ServerMode::Host(host) = &state.mode else {
        return;
    };
    let mut holder_left = invite.session.phase.as_deref() == Some("frozen");
    // Keep the in-flight poll across transport ticks. A loaded holder must not
    // delay bridge-loss detection or consume the reconnect window.
    let observations = futures::stream::unfold(&peer, |peer| async move {
        tokio::time::sleep(MONITOR_INTERVAL).await;
        Some((
            tokio::time::timeout(STATUS_TIMEOUT, peer.session_status()).await,
            peer,
        ))
    });
    tokio::pin!(observations);
    loop {
        let status = tokio::select! {
            _ = cancelled.changed() => return,
            status = observations.next() => status,
            _ = tokio::time::sleep(MONITOR_INTERVAL) => None,
        };
        if *cancelled.borrow() {
            return;
        }
        // Serialize native stop/relaunch with Leave and local admission. A
        // stopped copy cannot be replaced behind a successful Leave.
        let _selection = state.session_selection.lock().await;
        if *cancelled.borrow()
            || !state
                .netplay_receiver
                .owns_copy(&copy.attempt_id, &copy.launch_id)
        {
            return;
        }
        match status {
            Some(Ok(Ok(upstream::UpstreamSessionStatus::SessionStatus {
                active: Some(active),
            }))) if active.launch_id == invite.session.launch_id => {
                holder_left = active.phase.as_deref() == Some("frozen");
            }
            Some(Ok(Ok(_))) => {
                if finish(&state, &invite, &copy, false).await {
                    return;
                }
                continue;
            }
            // A timeout or RPC failure says nothing about the game connection.
            // Keep the last observed holder phase until a successful reply.
            _ => {}
        }
        let route_alive = state
            .federation
            .as_ref()
            .is_some_and(|directory| directory.has_iroh_route(&invite.device_public_key));
        if holder_left && route_alive {
            let _ = host.session_freeze(&copy.launch_id).await;
            state
                .netplay_receiver
                .phase(&copy.attempt_id, NetplayPhase::HolderLeft);
            // Waiting for the holder is not a connection-loss deadline.
            if let ConnectionState::Connecting { since } | ConnectionState::Reconnecting { since } =
                &mut connection
            {
                *since = tokio::time::Instant::now();
            }
            continue;
        }
        if !route_alive || !copy.forward.ready() {
            connection = match connection {
                ConnectionState::Playing => ConnectionState::Reconnecting {
                    since: tokio::time::Instant::now(),
                },
                waiting => waiting,
            };
            let (phase, since) = match connection {
                ConnectionState::Connecting { since } => (NetplayPhase::Connecting, since),
                ConnectionState::Reconnecting { since } => (NetplayPhase::Reconnecting, since),
                ConnectionState::Playing => unreachable!(),
            };
            state.netplay_receiver.phase(&copy.attempt_id, phase);
            if reconnect_expired(since, tokio::time::Instant::now()) {
                if finish(&state, &invite, &copy, true).await {
                    return;
                }
                continue;
            }
            if route_alive && copy.forward.lost() {
                // RetroArch has no reconnect command. Start its client again;
                // it receives the holder's current state in a fresh private tmp.
                if let Some(directory) = &state.federation {
                    if let Ok(next) = tokio::time::timeout(
                        Duration::from_secs(1),
                        directory.netplay_forward(&invite.device_public_key),
                    )
                    .await
                    .unwrap_or_else(|_| Err(std::io::Error::from(std::io::ErrorKind::TimedOut)))
                    {
                        if host.session_stop(&copy.launch_id).await.is_ok() {
                            if let (Ok(context), Ok(source)) = (
                                state.game_context(),
                                password::create(
                                    state.private_state_root(),
                                    Some(prepared.seat),
                                    Some(&prepared.password),
                                ),
                            ) {
                                if let (Some(game), Some(runner)) = (
                                    release_game(&context, &prepared.release_key),
                                    core(
                                        &context,
                                        &NetplaySession {
                                            release_key: prepared.release_key.clone(),
                                            release_bytes: prepared.release_bytes,
                                            runner: prepared.runner.clone(),
                                        },
                                    ),
                                ) {
                                    let input = PluginLaunchNetplay::Client {
                                        address: next.address.ip().to_string(),
                                        port: next.address.port(),
                                        player: prepared.seat,
                                        password_config: password::GAME_CONFIG.into(),
                                    };
                                    let runtime = host.clone();
                                    let release = prepared.release_key.clone();
                                    if let Ok(Ok(started)) =
                                        tokio::task::spawn_blocking(move || {
                                            runtime.prepare_netplay_client(
                                                &game, &runner, &release, input, source,
                                            )
                                        })
                                        .await
                                    {
                                        copy.launch_id = started.session.launch_id;
                                        copy.forward = next;
                                        if *cancelled.borrow() {
                                            let _ = host.session_stop(&copy.launch_id).await;
                                            return;
                                        }
                                        state
                                            .netplay_receiver
                                            .retarget(&copy.attempt_id, &copy.launch_id);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            continue;
        }
        connection = ConnectionState::Playing;
        let _ = host.session_thaw(&copy.launch_id).await;
        state
            .netplay_receiver
            .phase(&copy.attempt_id, NetplayPhase::Playing);
    }
}
fn reconnect_expired(since: tokio::time::Instant, now: tokio::time::Instant) -> bool {
    now.duration_since(since) >= RECONNECT_WINDOW
}
async fn finish(
    state: &AppState,
    invite: &invites::Invite,
    copy: &MonitoredCopy,
    lost: bool,
) -> bool {
    if let ServerMode::Host(host) = &state.mode {
        if stop_copy(host, &copy.launch_id).await.is_err() {
            log::warn!("netplay receiver is waiting for native stop recovery");
            return false;
        }
    }
    let mut local = state.netplay_receiver.0.lock().expect("netplay receiver");
    let launch_id = match &*local {
        State::Active { run, session }
            if run.id == copy.attempt_id && session.launch_id == copy.launch_id =>
        {
            run.status.launch_id.clone()
        }
        _ => return true,
    };
    *local = State::Ended(StreamPlayEnded {
        launch_id,
        game_id: invite.session.game_id.clone(),
        game_title: invite.session.title.clone(),
        device_label: invite.device_label.clone(),
        cause: if lost {
            stream_play::StreamPlayEndCause::DeviceUnreachable
        } else {
            stream_play::StreamPlayEndCause::SessionEnded
        },
    });
    true
}

pub(crate) async fn dispatch(
    state: &AppState,
    authorization: &authorization::AuthorizationContext,
    request: &RpcRequest,
) -> Option<RpcResponse> {
    if matches!(authorization, authorization::AuthorizationContext::Peer(_)) {
        // A receiving copy is not another holder and must not advertise an
        // invite of its own, or accept a second route through this device.
        if state.netplay_receiver.busy() {
            return match request {
                RpcRequest::SessionStatus(_) => Some(RpcResponse::SessionStatus(
                    SessionStatusOutcome::Ok(SessionStatus {
                        netplay: None,
                        active: None,
                        stream_play_ended: None,
                        stream_play_launch_id: None,
                        overlay: None,
                        pending_launches: None,
                        observation_failure: None,
                    }),
                )),
                RpcRequest::SessionStreamPlay(_) => Some(RpcResponse::SessionStreamPlay(
                    SessionStreamPlayOutcome::Err(failure(
                        "NotSessionStarter",
                        "This device does not hold the shared session.",
                    )),
                )),
                RpcRequest::SessionNetplay(_) => Some(RpcResponse::SessionNetplay(
                    SessionNetplayOutcome::Err(failure(
                        "NoNetplayRoute",
                        "This device does not hold the shared session.",
                    )),
                )),
                _ => None,
            };
        }
        return None;
    }
    if matches!(
        request,
        RpcRequest::SessionPrepare(_)
            | RpcRequest::SessionStart(_)
            | RpcRequest::SelectedGameLaunch(_)
            | RpcRequest::InvitesStreamPlay(_)
    ) {
        let mut local = state.netplay_receiver.0.lock().expect("netplay receiver");
        if matches!(&*local, State::Ended(_)) {
            *local = State::Idle;
        }
    }
    let (status, active, ended) = {
        let local = state.netplay_receiver.0.lock().expect("netplay receiver");
        match &*local {
            State::Idle => (None, None, None),
            State::Preparing(run) => (Some(run.status.clone()), None, None),
            State::Active { run, session } => {
                let mut active = session.as_ref().clone();
                active.launch_id = run.status.launch_id.clone();
                (Some(run.status.clone()), Some(active), None)
            }
            State::Ended(ended) => (None, None, Some(ended.clone())),
        }
    };
    if let RpcRequest::SessionStatus(_) = request {
        if status.is_some() || ended.is_some() {
            return Some(RpcResponse::SessionStatus(SessionStatusOutcome::Ok(
                SessionStatus {
                    netplay: status,
                    active,
                    stream_play_ended: ended,
                    stream_play_launch_id: None,
                    overlay: None,
                    pending_launches: None,
                    observation_failure: None,
                },
            )));
        }
    }
    let status = status?;
    match request {
        RpcRequest::SessionLeave(request) => {
            let result = leave(state, &status, request.expected_launch_id.as_deref()).await;
            Some(RpcResponse::SessionLeave(
                result
                    .map(|()| {
                        SessionTransitionOutcome::Ok(SessionTransitionResult {
                            launch_id: status.launch_id,
                            state: SessionFreezerState::Frozen,
                            changed: true,
                        })
                    })
                    .unwrap_or_else(SessionTransitionOutcome::Err),
            ))
        }
        // The system-wide exact stop ends a receiving copy, not the held
        // session. Leave also cancels the monitor before it can reconnect.
        RpcRequest::SessionEnd(request)
            if matches!(
                authorization,
                authorization::AuthorizationContext::LocalUnixControl
            ) =>
        {
            let result = leave(state, &status, request.expected_launch_id.as_deref()).await;
            Some(RpcResponse::SessionEnd(
                result
                    .map(|()| {
                        SessionEndOutcome::Ok(SessionEndResult {
                            phase: SessionEndPhase::Ended,
                        })
                    })
                    .unwrap_or_else(SessionEndOutcome::Err),
            ))
        }
        RpcRequest::SessionCancel(request) => {
            let result = leave(state, &status, Some(&request.expected_launch_id)).await;
            Some(RpcResponse::SessionCancel(
                result
                    .map(|()| {
                        SessionEndOutcome::Ok(SessionEndResult {
                            phase: SessionEndPhase::Ended,
                        })
                    })
                    .unwrap_or_else(SessionEndOutcome::Err),
            ))
        }
        RpcRequest::SessionEnd(_) => {
            Some(RpcResponse::SessionEnd(SessionEndOutcome::Err(failure(
                "NotSessionStarter",
                "Only the holding device can end this session.",
            ))))
        }
        RpcRequest::SessionResume(_) => Some(RpcResponse::SessionResume(
            SessionTransitionOutcome::Err(failure(
                "NotSessionStarter",
                "Only the holding device can resume this session.",
            )),
        )),
        RpcRequest::SessionControls(_) | RpcRequest::SessionControlInvoke(_) => {
            stream_play::device::stream_player_control_refusal(&status.launch_id, request)
        }
        _ => None,
    }
}
async fn leave(
    state: &AppState,
    status: &NetplayStatus,
    expected: Option<&str>,
) -> Result<(), RpcFailure> {
    if expected != Some(status.launch_id.as_str()) {
        return Err(failure(
            "StaleLaunchIdentity",
            "The exact launch is required.",
        ));
    }
    {
        let local = state.netplay_receiver.0.lock().expect("netplay receiver");
        if let State::Preparing(run) = &*local {
            if run.status.launch_id != status.launch_id {
                return Err(failure(
                    "StaleLaunchIdentity",
                    "The exact launch is required.",
                ));
            }
            // Preparation owns cleanup until its cancellable work has ended.
            // It already holds the admission gate, so do not wait for it here.
            let _ = run.cancel.send(true);
            return Ok(());
        }
    }
    let _selection = state.session_selection.lock().await;
    let active = {
        let local = state.netplay_receiver.0.lock().expect("netplay receiver");
        match &*local {
            State::Active { run, session } if run.status.launch_id == status.launch_id => {
                session.as_ref().clone()
            }
            _ => {
                return Err(failure(
                    "StaleLaunchIdentity",
                    "The exact launch is required.",
                ))
            }
        }
    };
    if let ServerMode::Host(host) = &state.mode {
        stop_copy(host, &active.launch_id).await?;
    }
    let mut local = state.netplay_receiver.0.lock().expect("netplay receiver");
    if let State::Active { run, .. } = &*local {
        // Cancellation is committed only after the native copy has stopped.
        let _ = run.cancel.send(true);
    }
    *local = State::Idle;
    Ok(())
}

async fn stop_copy(host: &host::HostRuntime, launch: &str) -> Result<(), RpcFailure> {
    use host::control::HostSessionStop;
    loop {
        match host.session_stop(launch).await? {
            HostSessionStop::Completed { .. }
            | HostSessionStop::NoActive
            | HostSessionStop::StaleIdentity { .. } => return Ok(()),
            HostSessionStop::AlreadyStopping { .. } => {
                tokio::time::sleep(Duration::from_millis(250)).await
            }
            HostSessionStop::RecoveryBlocked => {
                return Err(failure(
                    "NoNetplayRoute",
                    "The receiving game could not be ended safely.",
                ))
            }
        }
    }
}

#[cfg(test)]
#[path = "receiver_monitor_tests.rs"]
mod monitor_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn netplay_reconnect_window_is_ten_seconds() {
        let since = tokio::time::Instant::now();
        assert!(!reconnect_expired(
            since,
            since + Duration::from_millis(9999)
        ));
        assert!(reconnect_expired(since, since + Duration::from_secs(10)));
    }
    #[tokio::test]
    async fn netplay_leave_ends_only_the_receiving_copy_and_clears_its_selection() {
        use crate::host::control::InMemoryLaunchUnitBackend;
        fn device(root: &std::path::Path) -> host::HostRuntime {
            let config = root.join("host.toml");
            std::fs::write(&config, "label = \"Device\"\ngames = [{ id = \"game\", title = \"Game\", command = [\"game\"] }]\n").unwrap();
            let host = host::HostRuntime::from_paths_with_backend(
                &config,
                None,
                root.join("private"),
                Arc::new(InMemoryLaunchUnitBackend::default()),
            );
            host.claim_save_owner();
            host
        }
        let holder_root = tempfile::tempdir().unwrap();
        let receiver_root = tempfile::tempdir().unwrap();
        let holder = device(holder_root.path());
        let receiver = device(receiver_root.path());
        let held = holder.prepare("game", None).await.unwrap().session;
        let copy = receiver.prepare("game", None).await.unwrap().session;
        let (state, _) = app_states(receiver.clone());
        let status = NetplayStatus {
            launch_id: "receiving-run".into(),
            device_label: "Holder".into(),
            game_title: Some("Game".into()),
            phase: NetplayPhase::Playing,
        };
        let active = serde_json::from_value(
            serde_json::json!({"launchId":copy.launch_id,"gameId":"game","phase":"running"}),
        )
        .unwrap();
        let (attempt, cancellation) = state.netplay_receiver.begin(status).unwrap();
        state.netplay_receiver.activate(&attempt, active).unwrap();
        let status = dispatch(
            &state,
            &authorization::AuthorizationContext::LocalBrowser,
            &RpcRequest::SessionStatus(SessionStatusRequest {}),
        )
        .await
        .unwrap();
        let RpcResponse::SessionStatus(SessionStatusOutcome::Ok(status)) = status else {
            panic!("status failed");
        };
        assert_eq!(status.active.unwrap().launch_id, "receiving-run");
        assert_eq!(status.netplay.unwrap().launch_id, "receiving-run");
        let response = crate::dispatch(
            &state,
            &authorization::AuthorizationContext::LocalBrowser,
            RpcRequest::SessionEnd(SessionEndRequest {
                expected_launch_id: Some(copy.launch_id.clone()),
                force: None,
            }),
        )
        .await
        .unwrap();
        assert!(
            matches!(response, RpcResponse::SessionEnd(SessionEndOutcome::Err(RpcFailure { code, .. })) if code == "NotSessionStarter")
        );
        let response = crate::dispatch(
            &state,
            &authorization::AuthorizationContext::LocalBrowser,
            RpcRequest::SessionLeave(SessionLeaveRequest {
                expected_launch_id: Some("receiving-run".into()),
            }),
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            RpcResponse::SessionLeave(SessionTransitionOutcome::Ok(_))
        ));
        assert!(*cancellation.borrow());
        assert!(!state.netplay_receiver.busy());
        let SessionStatusOutcome::Ok(held_status) =
            host_session_status_outcome(holder.session_status().await)
        else {
            panic!("holder stopped");
        };
        assert_eq!(held_status.active.unwrap().launch_id, held.launch_id);
        assert!(matches!(
            receiver.session_status().await.unwrap(),
            host::control::HostSessionStatus::NoActive
                | host::control::HostSessionStatus::Completed { .. }
        ));
    }
    pub(super) fn facts() -> NetplaySession {
        NetplaySession {
            release_key: format!("sha256:{}", "1".repeat(64)),
            release_bytes: 123,
            runner: NetplayRunner {
                plugin_id: "@korri:snes9x2010".into(),
                runner_id: "@korri:snes9x2010/snes9x2010".into(),
                netplay: RunnerNetplay {
                    library: "Snes9x 2010".into(),
                    version: "1.52.4fe690dd".into(),
                },
            },
        }
    }
    #[test]
    fn netplay_invite_states_use_local_content_core_space_and_repository_facts() {
        let facts = facts();
        assert_eq!(
            availability(&facts, true, false, true, false, None),
            InviteNetplay::Ready
        );
        assert_eq!(
            availability(&facts, true, false, false, true, None),
            InviteNetplay::NeedsCopy { bytes: 123.0 }
        );
        assert_eq!(
            availability(&facts, false, false, false, true, Some("Snes9x 2010")),
            InviteNetplay::NeedsPlugin {
                plugin_id: facts.runner.plugin_id.clone(),
                title: "Snes9x 2010".into(),
                bytes: Some(123.0)
            }
        );
        assert_eq!(
            availability(&facts, false, false, true, true, Some("Snes9x 2010")),
            InviteNetplay::NeedsPlugin {
                plugin_id: facts.runner.plugin_id.clone(),
                title: "Snes9x 2010".into(),
                bytes: None
            }
        );
        for (matching, conflict, has_release, space, title, code) in [
            (false, true, true, true, None, "CoreVersionMismatch"),
            (true, false, false, false, None, "NotEnoughSpace"),
            (false, false, false, true, None, "NoNetplayRoute"),
        ] {
            assert_eq!(
                availability(&facts, matching, conflict, has_release, space, title),
                InviteNetplay::Unavailable {
                    reason: code.into()
                }
            );
        }
    }
}
