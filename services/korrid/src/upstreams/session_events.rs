//! The Linux reveal boundary. The holder remains the only sound/input gate.
use super::*;
use crate::linux_viewer::ViewerFirstFrame;
use tokio::sync::watch;

#[derive(Clone, Default)]
struct Observation {
    status: Option<Result<UpstreamSessionStatus, UpstreamError>>,
    revealed: bool,
}
pub(super) struct Events {
    launch: String,
    observation: watch::Receiver<Observation>,
    task: tokio::task::AbortHandle,
}
impl Drop for Events {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Uses the registry's existing intent and transition lock. A late first frame
/// must not focus after Open Korri or Disconnect, or focus a replacement.
#[derive(Clone)]
struct RevealAuthority {
    selection: Arc<Mutex<Option<SelectedRemoteSession>>>,
    transition: Arc<tokio::sync::Mutex<()>>,
    intent: watch::Sender<()>,
}
impl RevealAuthority {
    fn enabled(&self, launch: &str) -> bool {
        self.selection
            .lock()
            .expect("selected session")
            .as_ref()
            .is_some_and(|selected| {
                selected.launch_id == launch && selected.viewer_target == LinuxViewerStatus::Running
            })
    }
}

impl UpstreamRegistry {
    pub(crate) fn session_changes(&self) -> watch::Receiver<()> {
        self.session_changes.subscribe()
    }

    /// Production Linux peers use the lease's captured iroh connection, never
    /// a second connection or a polling fallback. Android keeps its own reveal.
    pub(super) async fn start_session_events(
        &self,
        selected: &SelectedRemoteSession,
        client: &NativeClient,
        limit: Duration,
    ) -> Result<Option<tokio::sync::oneshot::Sender<()>>, UpstreamError> {
        if !cfg!(target_os = "linux") {
            return Ok(None);
        }
        let Some(reach) = selected.viewer.as_ref().and_then(|v| v.reach.as_ref()) else {
            return Ok(None);
        };
        let events = client
            .session_events(reach.connection(), &selected.launch_id)
            .await?;
        let events = futures::stream::unfold(events, |mut events| async move {
            Some((events.next().await, events))
        });
        let (sender, observation) = watch::channel(Observation::default());
        let launch = selected.launch_id.clone();
        let viewer = self.viewer.clone();
        let changes = self.session_changes.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(run(
            Box::pin(events),
            ready,
            launch.clone(),
            viewer,
            RevealAuthority {
                selection: self.selected_remote_session.clone(),
                transition: self.remote_prepare_mutation.clone(),
                intent: self.viewer_intent_changes.clone(),
            },
            sender,
            changes,
            limit,
        ));
        *self.session_events.lock().expect("session events") = Some(Events {
            launch,
            observation,
            task: task.abort_handle(),
        });
        Ok(Some(started))
    }

    pub(super) async fn until_session_ends(&self, launch: &str) -> UpstreamError {
        let receiver = self
            .session_events
            .lock()
            .expect("session events")
            .as_ref()
            .filter(|events| events.launch == launch)
            .map(|events| events.observation.clone());
        let Some(mut receiver) = receiver else {
            return std::future::pending().await;
        };
        loop {
            let state = receiver.borrow_and_update().clone();
            match state.status {
                Some(Err(error)) => return error,
                Some(Ok(UpstreamSessionStatus::SessionStatus { active: None })) => {
                    return UpstreamError::NoActiveSession
                }
                _ => {}
            }
            if receiver.changed().await.is_err() {
                return UpstreamError::Unreachable("session events stopped".into());
            }
        }
    }

    pub(super) async fn pushed_status(
        &self,
        selected: &SelectedRemoteSession,
    ) -> Option<Result<UpstreamSessionStatus, UpstreamError>> {
        let mut observation = self
            .session_events
            .lock()
            .expect("session events")
            .as_ref()
            .filter(|events| events.launch == selected.launch_id)?
            .observation
            .clone();
        loop {
            let snapshot = observation.borrow_and_update().clone();
            if let Some(status) = snapshot.status {
                return Some(status.map(|mut status| {
                    if let UpstreamSessionStatus::SessionStatus {
                        active: Some(active),
                    } = &mut status
                    {
                        // The surface's existing Starting/Cancel boundary now
                        // includes the local presenter's first frame and focus.
                        active.initial_handoff = Some(if snapshot.revealed {
                            crate::InitialHandoff::Observed
                        } else {
                            crate::InitialHandoff::Waiting
                        });
                        if let SelectedRemoteRoute::Native { device_public_key } = &selected.route {
                            active.host = self
                                .native_host_by_route_key(device_public_key)
                                .map(|host| host.label.clone());
                        }
                    }
                    status
                }));
            }
            if observation.changed().await.is_err() {
                return Some(Err(UpstreamError::Unreachable(
                    "session events stopped".into(),
                )));
            }
        }
    }

    pub(super) fn reveal_waiting(&self, selected: &SelectedRemoteSession) -> bool {
        self.session_events
            .lock()
            .expect("session events")
            .as_ref()
            .is_some_and(|events| {
                events.launch == selected.launch_id && !events.observation.borrow().revealed
            })
    }
}

/// A deadline replaces only the absent holder fact. It cannot invent a local
/// picture or turn a failed/ended/replaced exact launch into a reveal.
fn may_reveal(
    status: &Result<UpstreamSessionStatus, UpstreamError>,
    launch: &str,
    first_frame: bool,
    expired: bool,
) -> bool {
    matches!(status, Ok(UpstreamSessionStatus::SessionStatus { active: Some(active) })
        if active.launch_id == launch && active.phase.as_deref() != Some("stopping")
        && first_frame && (expired || active.initial_handoff == Some(crate::InitialHandoff::Observed)))
}

async fn run(
    mut events: futures::stream::BoxStream<'static, Result<UpstreamSessionStatus, UpstreamError>>,
    mut ready: tokio::sync::oneshot::Receiver<()>,
    launch: String,
    viewer: Arc<dyn LinuxStreamViewer>,
    authority: RevealAuthority,
    observation: watch::Sender<Observation>,
    changes: watch::Sender<()>,
    limit: Duration,
) {
    let deadline = tokio::time::sleep(limit);
    tokio::pin!(deadline);
    let mut intent = authority.intent.subscribe();
    let mut frame: Option<watch::Receiver<ViewerFirstFrame>> = None;
    let mut expired = false;
    let mut started = false;
    let mut state = Observation::default();
    loop {
        tokio::select! {
            biased;
            result = &mut ready, if !started => {
                if result.is_err() {
                    state.status = Some(Err(viewer_error("ViewerStartFailed", "Viewer start ended before connection")));
                    observation.send_replace(state);
                    changes.send_replace(());
                    return;
                }
                started = true;
            }
            status = events.next() => {
                let status = status.unwrap_or_else(|| Err(UpstreamError::Unreachable("session events stream closed".into())));
                eprintln!("korrid: SessionEventArrived {{ launch_id: {launch}, at_ms: {:?} }}", crate::linux_viewer::monotonic_ms());
                state.status = Some(status);
            }
            _ = intent.changed() => {},
            _ = async { if let Some(frame) = &mut frame { let _ = frame.changed().await; } else { std::future::pending::<()>().await; } }, if !state.revealed && authority.enabled(&launch) => {}
            _ = &mut deadline, if !expired => { expired = true; }
        }
        if state.status.is_none() {
            continue;
        }
        let live = matches!(&state.status, Some(Ok(UpstreamSessionStatus::SessionStatus { active: Some(active) })) if active.launch_id == launch);
        if live && started && !state.revealed {
            let _transition = authority.transition.lock().await;
            if authority.enabled(&launch) {
                if frame.is_none() {
                    match viewer.first_frame(&launch).await {
                        Ok(receiver) => frame = Some(receiver),
                        Err(error) => {
                            state.status =
                                Some(Err(viewer_error("ViewerUnavailable", error.to_string())))
                        }
                    }
                }
                let first = frame.as_ref().map(|frame| *frame.borrow());
                if first == Some(ViewerFirstFrame::Stopped) {
                    state.status = Some(Err(viewer_error(
                        "ViewerStopped",
                        "Viewer stopped before its first frame",
                    )));
                }
                if may_reveal(
                    state.status.as_ref().expect("status"),
                    &launch,
                    first == Some(ViewerFirstFrame::Shown),
                    expired,
                ) {
                    eprintln!("korrid: StreamRevealRequested {{ launch_id: {launch}, at_ms: {:?}, time_limit: {expired} }}", crate::linux_viewer::monotonic_ms());
                    match viewer.resume_and_focus(&launch).await {
                        Ok(()) => {
                            state.revealed = true;
                            eprintln!("korrid: StreamRevealed {{ launch_id: {launch}, at_ms: {:?}, time_limit: {expired} }}", crate::linux_viewer::monotonic_ms());
                        }
                        Err(error) => {
                            state.status =
                                Some(Err(viewer_error("ViewerUnavailable", error.to_string())));
                        }
                    }
                }
            }
        }
        let terminal = !matches!(&state.status, Some(Ok(UpstreamSessionStatus::SessionStatus { active: Some(active) })) if active.launch_id == launch);
        observation.send_replace(state.clone());
        changes.send_replace(());
        if terminal {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux_viewer::test_backend::{LinuxViewerAction, TestLinuxStreamViewer};
    const LAUNCH: &str = "0123456789abcdef0123456789abcdef";

    fn status(handoff: crate::InitialHandoff) -> Result<UpstreamSessionStatus, UpstreamError> {
        Ok(UpstreamSessionStatus::SessionStatus {
            active: Some(crate::upstream::UpstreamActiveSession {
                steps: None,
                initial_handoff: Some(handoff),
                netplay: None,
                person_public_key: None,
                stream_players: Vec::new(),
                launch_id: LAUNCH.into(),
                host: None,
                game_id: Some("game".into()),
                title: None,
                phase: Some("running".into()),
            }),
        })
    }
    type Status = Result<UpstreamSessionStatus, UpstreamError>;
    fn watch_viewer(
        first: bool,
    ) -> (
        Arc<TestLinuxStreamViewer>,
        tokio::sync::mpsc::Sender<Status>,
        watch::Receiver<Observation>,
        tokio::task::JoinHandle<()>,
        RevealAuthority,
    ) {
        let viewer = Arc::new(TestLinuxStreamViewer::default());
        viewer.seed(LAUNCH, LinuxViewerStatus::Running);
        viewer.set_first_frame(first);
        let (send, receive) = tokio::sync::mpsc::channel(8);
        let stream = futures::stream::unfold(receive, |mut receive| async move {
            receive.recv().await.map(|status| (status, receive))
        });
        let (observation, receive) = watch::channel(Observation::default());
        let (started, ready) = tokio::sync::oneshot::channel();
        started.send(()).unwrap();
        let authority = RevealAuthority {
            selection: Arc::new(Mutex::new(Some(SelectedRemoteSession {
                display: None,
                participation: Participation::Starter,
                route: SelectedRemoteRoute::Native {
                    device_public_key: "peer".into(),
                },
                launch_id: LAUNCH.into(),
                viewer: None,
                viewer_target: LinuxViewerStatus::Running,
            }))),
            transition: Default::default(),
            intent: watch::channel(()).0,
        };
        let task = tokio::spawn(run(
            Box::pin(stream),
            ready,
            LAUNCH.into(),
            viewer.clone(),
            authority.clone(),
            observation,
            watch::channel(()).0,
            Duration::from_secs(30),
        ));
        (viewer, send, receive, task, authority)
    }
    fn reveals(viewer: &TestLinuxStreamViewer) -> usize {
        viewer
            .calls()
            .iter()
            .filter(|call| call.action == LinuxViewerAction::ResumeAndFocus)
            .count()
    }
    #[tokio::test]
    async fn holder_first_waits_for_the_presenter_and_reveals_only_once() {
        let (viewer, send, mut observed, task, _) = watch_viewer(false);
        send.send(status(crate::InitialHandoff::Observed))
            .await
            .unwrap();
        observed.changed().await.unwrap();
        assert_eq!(reveals(&viewer), 0);
        viewer.set_first_frame(true);
        observed.changed().await.unwrap();
        assert!(observed.borrow().revealed);
        assert_eq!(reveals(&viewer), 1);
        send.send(status(crate::InitialHandoff::Observed))
            .await
            .unwrap();
        observed.changed().await.unwrap();
        assert_eq!(reveals(&viewer), 1);
        task.abort();
    }
    #[tokio::test]
    async fn presenter_first_waits_for_the_holder() {
        let (viewer, send, mut observed, task, _) = watch_viewer(true);
        send.send(status(crate::InitialHandoff::Waiting))
            .await
            .unwrap();
        observed.changed().await.unwrap();
        assert_eq!(reveals(&viewer), 0);
        send.send(status(crate::InitialHandoff::Observed))
            .await
            .unwrap();
        observed.changed().await.unwrap();
        assert_eq!(reveals(&viewer), 1);
        task.abort();
    }
    #[tokio::test(start_paused = true)]
    async fn thirty_seconds_replaces_only_the_missing_holder_frame() {
        let (viewer, send, mut observed, task, _) = watch_viewer(true);
        send.send(status(crate::InitialHandoff::Waiting))
            .await
            .unwrap();
        observed.changed().await.unwrap();
        tokio::time::advance(Duration::from_secs(29)).await;
        assert_eq!(reveals(&viewer), 0);
        tokio::time::advance(Duration::from_secs(1)).await;
        observed.changed().await.unwrap();
        assert_eq!(reveals(&viewer), 1);
        task.abort();
    }
    #[tokio::test(start_paused = true)]
    async fn an_early_exit_is_published_at_once_and_never_revealed() {
        let (viewer, send, mut observed, task, _) = watch_viewer(true);
        send.send(status(crate::InitialHandoff::Waiting))
            .await
            .unwrap();
        observed.changed().await.unwrap();
        send.send(Err(viewer_error(
            "HostLaunchFailed",
            "unit exited before a picture",
        )))
        .await
        .unwrap();
        observed.changed().await.unwrap();
        assert!(
            matches!(&observed.borrow().status, Some(Err(error)) if error.code() == "HostLaunchFailed")
        );
        task.await.unwrap();
        tokio::time::advance(Duration::from_secs(31)).await;
        assert_eq!(reveals(&viewer), 0);
    }
    #[tokio::test]
    async fn open_korri_keeps_a_late_picture_hidden_until_resume() {
        let (viewer, send, mut observed, task, authority) = watch_viewer(false);
        authority
            .selection
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .viewer_target = LinuxViewerStatus::Backgrounded;
        send.send(status(crate::InitialHandoff::Observed))
            .await
            .unwrap();
        observed.changed().await.unwrap();
        viewer.set_first_frame(true);
        send.send(status(crate::InitialHandoff::Observed))
            .await
            .unwrap();
        observed.changed().await.unwrap();
        assert_eq!(reveals(&viewer), 0);
        authority
            .selection
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .viewer_target = LinuxViewerStatus::Running;
        authority.intent.send_replace(());
        observed.changed().await.unwrap();
        assert_eq!(reveals(&viewer), 1);
        task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn the_deadline_still_waits_for_a_local_picture() {
        let (viewer, send, mut observed, task, _) = watch_viewer(false);
        send.send(status(crate::InitialHandoff::Waiting))
            .await
            .unwrap();
        observed.changed().await.unwrap();
        tokio::time::advance(Duration::from_secs(30)).await;
        observed.changed().await.unwrap();
        assert_eq!(reveals(&viewer), 0);
        viewer.set_first_frame(true);
        observed.changed().await.unwrap();
        assert_eq!(reveals(&viewer), 1);
        task.abort();
    }

    #[test]
    fn neither_a_replacement_nor_a_stopping_game_can_reveal_at_the_deadline() {
        assert!(!may_reveal(
            &status(crate::InitialHandoff::Observed),
            "another",
            true,
            true
        ));
        let mut stopping = status(crate::InitialHandoff::Observed);
        if let Ok(UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        }) = &mut stopping
        {
            active.phase = Some("stopping".into());
        }
        assert!(!may_reveal(&stopping, LAUNCH, true, true));
        assert!(!may_reveal(
            &status(crate::InitialHandoff::Observed),
            LAUNCH,
            false,
            true
        ));
    }
}
