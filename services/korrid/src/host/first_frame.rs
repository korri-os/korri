//! The local first-frame boundary. The event connection exists before a game
//! can start. Status reads cannot manufacture this signal from focus.
use super::*;

const LIMIT: Duration = Duration::from_secs(30);
const EXIT_CHECK: Duration = Duration::from_millis(50);

impl HostSessionControl {
    pub(crate) fn spawn_first_frame_watch(&self) {
        let Some(compositor) = self.compositor.clone() else {
            return;
        };
        // Subscribe synchronously, before startup opens its launch entry points.
        let subscription = compositor.subscribe_windows();
        let control = self.clone();
        std::thread::Builder::new()
            .name("first-frame".into())
            .spawn(move || {
                let mut subscription = subscription;
                loop {
                    match &mut subscription {
                        Ok(events) => match events.next(EXIT_CHECK) {
                            Ok(event) => {
                                control.observe_first_frame(event.as_deref(), Instant::now())
                            }
                            Err(reason) => {
                                eprintln!(
                                    "korrid: FirstFrameSubscriptionLost {{ reason: {reason} }}"
                                );
                                subscription = Err(reason);
                            }
                        },
                        Err(_) => {
                            control.observe_first_frame(None, Instant::now());
                            std::thread::sleep(Duration::from_millis(250));
                            subscription = compositor.subscribe_windows();
                            // Events from the gap are gone. The new
                            // subscription exists first, so the tree read
                            // after it misses no window.
                            if subscription.is_ok() {
                                control.reconcile_first_frame_windows(Instant::now());
                            }
                        }
                    }
                    if control
                        .shutting_down
                        .load(std::sync::atomic::Ordering::SeqCst)
                    {
                        return;
                    }
                }
            })
            .expect("start first-frame observer");
    }

    pub(crate) fn observe_first_frame(&self, event: Option<&str>, now: Instant) {
        let mut state = self.lock_state_named("first_frame");
        // The native hide rule also holds new shell windows. Only the shell
        // unit's own PIDs authorize showing its primary window; pane placement
        // remains with the surface-display request.
        if let (Some(event), Some(compositor)) = (event, self.compositor.as_ref()) {
            if let Ok(pids) = self.shell_pids() {
                if matches!(
                    super::super::compositor_focus::first_frame_window(
                        event,
                        &pids,
                        &Default::default()
                    ),
                    Ok(Some(_))
                ) {
                    if let Err(reason) = super::super::compositor_focus::present_shell_window(
                        compositor.as_ref(),
                        &pids,
                    ) {
                        eprintln!("korrid: ShellFramePlacementFailed {{ reason: {reason} }}");
                    }
                    state.reconcile = false;
                    return;
                }
            }
        }
        // No unit calls or reconciliation effects on every idle event-loop turn.
        let pending = self
            .initial_handoff
            .lock()
            .expect("handoff mutex poisoned")
            .as_ref()
            .is_some_and(|(_, handoff)| matches!(handoff, FirstFrameHandoff::Waiting { .. }));
        if !pending && event.is_none() {
            state.reconcile = false;
            return;
        }
        let before = status_from_state(&state);
        self.refresh_native_state_locked(&mut state);
        if before != status_from_state(&state) {
            self.session_changes.send_replace(());
        }
        let waiting = self.initial_handoff_for(&state) == Some(InitialHandoff::Waiting);
        let launch_id = match &*state {
            ActiveState::Running { launch_id, .. } => launch_id.clone(),
            _ => return,
        };
        if let Some(event) = event {
            let candidate = (|| {
                let pids = self
                    .backend
                    .window_pids(&launch_id)
                    .map_err(|e| e.message)?;
                let excluded = self.shell_pids()?;
                super::super::compositor_focus::first_frame_window(event, &pids, &excluded)
            })();
            match candidate {
                Ok(Some(candidate)) => {
                    if let Err(reason) = self.place_frame_window(&launch_id, candidate.node_id) {
                        eprintln!("korrid: FirstFramePlacementFailed {{ launch_id: {launch_id}, reason: {reason} }}");
                        return;
                    }
                    if waiting {
                        if let Err(reason) = self.record_initial_handoff(&launch_id, HandoffEnd::Signal, now) {
                            eprintln!("korrid: FirstFrameAudioFailed {{ launch_id: {launch_id}, reason: {reason} }}");
                        }
                    }
                    return;
                }
                Ok(None) => (),
                Err(reason) => eprintln!("korrid: FirstFrameEventRejected {{ launch_id: {launch_id}, reason: {reason} }}"),
            }
        }
        let expired = self.initial_handoff.lock().expect("handoff mutex poisoned").as_ref()
            .is_some_and(|(exact, phase)| exact == &launch_id && matches!(phase, FirstFrameHandoff::Waiting { since } if now.saturating_duration_since(*since) >= LIMIT));
        if waiting && expired {
            let _ = self.focus_launch(&launch_id);
            if let Err(reason) = self.record_initial_handoff(&launch_id, HandoffEnd::TimeLimit, now)
            {
                eprintln!(
                    "korrid: FirstFrameAudioFailed {{ launch_id: {launch_id}, reason: {reason} }}"
                );
            }
        }
    }

    /// After a lost subscription, the exact launch's mapped windows in the
    /// compositor tree stand in for the `new` events that were not delivered.
    /// The same PID-ownership rule decides; the earliest window is the first.
    pub(crate) fn reconcile_first_frame_windows(&self, now: Instant) {
        let Some(compositor) = self.compositor.as_ref() else {
            return;
        };
        let candidate = {
            let state = self.lock_state_named("first_frame_reconcile");
            if self.initial_handoff_for(&state) != Some(InitialHandoff::Waiting) {
                return;
            }
            let ActiveState::Running { launch_id, .. } = &*state else {
                return;
            };
            let launch_id = launch_id.clone();
            let found = (|| {
                let pids = self
                    .backend
                    .window_pids(&launch_id)
                    .map_err(|e| e.message)?;
                let excluded = self.shell_pids()?;
                Ok::<_, String>(super::super::compositor_focus::select_focus_target(
                    &compositor.tree()?,
                    &pids,
                    &excluded,
                ))
            })();
            use super::super::compositor_focus::FocusTarget;
            match found {
                Ok(FocusTarget::Window(candidate)) => Some((launch_id, candidate)),
                Ok(FocusTarget::Ambiguous(candidates)) => candidates
                    .into_iter()
                    .next()
                    .map(|candidate| (launch_id, candidate)),
                Ok(FocusTarget::NoWindow) => None,
                Ok(FocusTarget::Unreadable(reason)) | Err(reason) => {
                    eprintln!("korrid: FirstFrameReconcileFailed {{ launch_id: {launch_id}, reason: {reason} }}");
                    None
                }
            }
        };
        if let Some((launch_id, candidate)) = candidate {
            eprintln!(
                "korrid: FirstFrameReconciled {{ launch_id: {launch_id}, node: {} }}",
                candidate.node_id
            );
            let event = serde_json::json!({
                "change": "new",
                "container": {"id": candidate.node_id, "pid": candidate.pid},
            })
            .to_string();
            self.observe_first_frame(Some(&event), now);
        }
    }

    fn place_frame_window(&self, launch_id: &str, node: i64) -> Result<(), String> {
        let compositor = self.compositor.as_ref().ok_or("compositor unavailable")?;
        let displays = super::super::display_placement::configured_displays()?;
        if displays.is_empty() {
            // Moving out of scratchpad shows the window but leaves it floating.
            // Tiling it removes scratchpad membership before fullscreen sizing.
            compositor.command(&format!("[con_id={node}] move container to workspace current; [con_id={node}] floating disable; [con_id={node}] fullscreen enable"))?;
            return compositor.focus_game(node);
        }
        let placement = self.launch_placement(launch_id, &displays)?;
        compositor.command(
            &super::super::display_placement::game_commands(&displays, &placement, node, true)?
                .join("; "),
        )?;
        *self
            .display_content
            .lock()
            .expect("display content mutex poisoned") = placement.outputs;
        *self
            .placement_applied
            .lock()
            .expect("placement mutex poisoned") =
            Some(AppliedDisplayPlacement::Game(launch_id.into()));
        Ok(())
    }
}
