use super::*;
use crate::host::{
    live_shape::{DisplayChangeContext, LiveShape, ShapeAttempt},
    live_shape_effect as effect,
};
use crate::launcher::plugin_launch::PluginDisplay;
use crate::live_display::{SessionViewerDisplay, SessionViewerDisplayRequest, ViewerDisplayEvent};

impl HostSessionControl {
    fn save_display_context(
        &self,
        launch: &str,
        context: &DisplayChangeContext,
    ) -> Result<(), RpcFailure> {
        let mut active = read_active(&self.identity_root)
            .map_err(|_| recovery_blocked_failure())?
            .ok_or_else(recovery_blocked_failure)?;
        match &mut active {
            ActiveSession::Running {
                launch_id,
                display_change,
                ..
            } if launch_id == launch => *display_change = Some(context.clone()),
            _ => {
                return Err(failure(
                    "StaleSession",
                    "The exact streamed session is no longer active.",
                ))
            }
        }
        replace_active(&self.identity_root, &active).map_err(|_| {
            failure(
                "DisplayPersistenceFailed",
                "Display-change recovery state could not be saved.",
            )
        })
    }
    fn display_hook(
        &self,
        launch: &str,
        context: &DisplayChangeContext,
        index: usize,
        size: Option<&PluginDisplay>,
    ) -> Result<String, String> {
        let program = context
            .programs
            .get(index)
            .ok_or("display hook identity changed")?;
        let mut env = context.environment.clone();
        env.insert("KORRI_HOOK_PHASE".into(), "display-change".into());
        env.insert("KORRI_LAUNCH_ID".into(), launch.into());
        env.insert(
            "KORRI_GAME_PIDS".into(),
            self.backend
                .window_pids(launch)
                .map_err(|e| e.message)?
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" "),
        );
        env.insert(
            "KORRI_GAME_CGROUP".into(),
            self.backend.control_group(launch).map_err(|e| e.message)?,
        );
        let args = size
            .map(|s| format!("{} {}", s.width, s.height))
            .unwrap_or_else(|| "--query".into());
        let run = format!("'{}' {args}", program.replace('\'', "'\\''"));
        self.backend
            .run_live_hook(
                launch,
                &context.command,
                &run,
                Duration::from_secs(5),
                &env,
                &context.sandbox,
            )
            .map_err(|e| {
                // The viewer sees only "Couldn't reshape". Keep the hook's own
                // reason for the operator; it named the cause on 2026-11-11.
                eprintln!(
                    "korrid: display hook failed: launch={launch} hook={index} request={args}: {}",
                    e.message
                );
                e.message
            })
    }
    fn display_hook_sizes(
        &self,
        launch: &str,
        context: &DisplayChangeContext,
    ) -> Result<Vec<PluginDisplay>, String> {
        context
            .programs
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let text = self.display_hook(launch, context, index, None)?;
                if text.len() > 128 {
                    return Err("invalid display hook observation".into());
                }
                let values: Vec<_> = text.split_whitespace().collect();
                if values.len() != 2 {
                    return Err("invalid display hook observation".into());
                }
                let width = values[0]
                    .parse()
                    .map_err(|_| "invalid display hook width")?;
                let height = values[1]
                    .parse()
                    .map_err(|_| "invalid display hook height")?;
                let observed = PluginDisplay { width, height };
                if observed.width == 0 || observed.height == 0 {
                    return Err("display hook reported an empty output".into());
                }
                Ok(observed)
            })
            .collect()
    }
    pub(crate) fn viewer_display_report(
        &self,
        peer: &str,
        request: &SessionViewerDisplayRequest,
    ) -> Result<SessionViewerDisplay, RpcFailure> {
        let mut state = self.lock_state_named("viewer_display_report");
        self.refresh_native_state_locked(&mut state);
        let launch = match &*state {
            ActiveState::Running { launch_id, .. }
            | ActiveState::Frozen { launch_id, .. }
            | ActiveState::FocusFailed { launch_id, .. }
                if *launch_id == request.expected_launch_id =>
            {
                launch_id.clone()
            }
            _ => {
                return Err(failure(
                    "StaleSession",
                    "The exact streamed session is no longer active.",
                ))
            }
        };
        let mut context =
            match read_active(&self.identity_root).map_err(|_| recovery_blocked_failure())? {
                Some(ActiveSession::Running {
                    launch_id,
                    display_change: Some(context),
                    starter_device_public_key,
                    stream_players,
                    ..
                }) if launch_id == launch => {
                    if starter_device_public_key.as_deref() != Some(peer)
                        && !stream_players.iter().any(|p| p.device_public_key == peer)
                    {
                        return Err(failure(
                            "NotStreamParticipant",
                            "Only an admitted participant can report this viewer.",
                        ));
                    }
                    context
                }
                _ => {
                    return Err(failure(
                        "DisplayChangeUnavailable",
                        "The active launch has no display-change authority.",
                    ))
                }
            };
        if self.boot_identity.as_ref().ok() != Some(&context.boot_id) {
            return Err(failure(
                "DisplayChangeUnavailable",
                "Display-change authority belongs to another boot.",
            ));
        }
        let can_apply = matches!(&*state, ActiveState::Running { .. })
            && (context.programs.is_empty()
                || self.initial_handoff_for(&state) != Some(InitialHandoff::Waiting));
        let control = self
            .compositor
            .as_ref()
            .ok_or_else(|| failure("DisplayChangeUnavailable", "Compositor unavailable."))?;
        let displays = crate::host::display_placement::configured_displays()
            .map_err(|_| failure("DisplayChangeUnavailable", "Display facts unavailable."))?;
        let main = displays
            .iter()
            .find(|d| d.main)
            .ok_or_else(|| failure("DisplayChangeUnavailable", "Main display unavailable."))?;
        let placement = self
            .backend
            .placement(&launch)
            .map_err(|_| failure("DisplayChangeUnavailable", "Live placement unavailable."))?
            .ok_or_else(|| failure("DisplayChangeUnavailable", "Live placement unavailable."))?;
        effect::eligible(&displays, &placement)
            .map_err(|m| failure("DisplayChangeUnavailable", m))?;
        let current = || {
            effect::current(control.as_ref(), &main.name).map_err(|_| {
                failure(
                    "DisplayChangeUnavailable",
                    "Display mode could not be observed.",
                )
            })
        };
        let restore = |snapshot: &effect::Snapshot,
                       context: &DisplayChangeContext|
         -> Result<(), RpcFailure> {
            if !can_apply {
                return Err(failure(
                    "DisplayChangePending",
                    "The previous shape will be restored when the game resumes.",
                ));
            }
            if snapshot.hooks.len() != context.programs.len() {
                return Err(recovery_blocked_failure());
            }
            effect::restore(
                control.as_ref(),
                &displays,
                &placement,
                snapshot,
                |index, size| {
                    self.display_hook(&launch, context, index, Some(size))
                        .map(|_| ())
                },
            )
            .map_err(|_| {
                failure(
                    "DisplayRollbackFailed",
                    "Couldn't reshape; previous display state could not be confirmed.",
                )
            })
        };
        let mut guard = self.live_shape.lock().expect("live shape poisoned");
        if guard.as_ref().is_none_or(|(id, _)| *id != launch) {
            *guard = Some((launch.clone(), LiveShape::with_portrait(context.portrait)));
        }
        let shape = &mut guard.as_mut().expect("shape initialized").1;
        let mut notice: Option<String> = None;
        let mut recovered_pending = false;
        // A prior process may have died between any two effects. The journal
        // was written before the first effect, so recovery never guesses.
        if shape.pending.is_none() {
            if let Some(previous) = context.pending.clone() {
                restore(&previous, &context)?;
                context.pending = None;
                self.save_display_context(&launch, &context)?;
                shape.portrait = context.portrait;
                recovered_pending = true;
                notice = Some("Couldn't reshape".into());
            }
        }
        let now = Instant::now();
        shape.expire(now, Duration::from_secs(10));
        let before_portrait = shape.portrait;
        let connection = request.event.connection();
        match &request.event {
            ViewerDisplayEvent::Report {
                connection,
                display,
            } => {
                let old = shape.desired();
                shape
                    .report(peer, connection, display.clone(), now)
                    .map_err(|m| failure("StaleViewer", m))?;
                if shape.desired() != old {
                    shape.failed_target = None;
                }
            }
            ViewerDisplayEvent::Leave { connection } => {
                shape.leave(peer, connection);
            }
            ViewerDisplayEvent::Portrait {
                connection,
                enabled,
            } => {
                if shape.pending.is_some() {
                    return Err(failure(
                        "DisplayChangeBusy",
                        "A display change is already pending.",
                    ));
                }
                if !can_apply {
                    return Err(failure(
                        "DisplayChangeUnavailable",
                        "The game is not ready to reshape.",
                    ));
                }
                shape
                    .portrait(peer, connection, *enabled)
                    .map_err(|m| failure("NotShapeOwner", m))?;
                shape.failed_target = None;
            }
            ViewerDisplayEvent::Applied {
                connection,
                revision,
            }
            | ViewerDisplayEvent::Refused {
                connection,
                revision,
            } => {
                if !shape.pending.as_ref().is_some_and(|p| {
                    p.peer == peer && p.connection == *connection && p.revision == *revision
                }) {
                    return Err(failure(
                        "StaleViewer",
                        "The display change is no longer pending.",
                    ));
                }
                if matches!(request.event, ViewerDisplayEvent::Applied { .. }) {
                    if shape.is_owner(peer, connection)
                        && shape.pending.as_ref().is_some_and(|p| now < p.deadline)
                    {
                        context.pending = None;
                        context.portrait = shape.portrait;
                        self.save_display_context(&launch, &context)?;
                        shape.pending = None;
                    }
                } else {
                    let pending = shape.pending.as_ref().expect("exact pending").clone();
                    restore(&pending.previous, &context)?;
                    context.pending = None;
                    context.portrait = pending.previous_portrait;
                    self.save_display_context(&launch, &context)?;
                    shape.pending = None;
                    shape.portrait = pending.previous_portrait;
                    shape.failed_target = Some((peer.into(), connection.clone(), pending.target));
                    notice = Some("Couldn't reshape".into());
                }
            }
        }
        if shape
            .pending
            .as_ref()
            .is_some_and(|p| now >= p.deadline || !shape.is_owner(&p.peer, &p.connection))
        {
            let pending = shape.pending.as_ref().expect("rollback pending").clone();
            restore(&pending.previous, &context)?;
            context.pending = None;
            context.portrait = pending.previous_portrait;
            self.save_display_context(&launch, &context)?;
            shape.pending = None;
            shape.portrait = pending.previous_portrait;
            shape.failed_target = Some((pending.peer, pending.connection, pending.target));
            notice = Some("Couldn't reshape".into());
        }
        if recovered_pending {
            if let (Some(target), Some((owner, instance))) =
                (shape.desired(), shape.owner_identity())
            {
                shape.failed_target = Some((owner, instance, target));
            }
        }
        if can_apply
            && shape.pending.is_none()
            && !matches!(
                request.event,
                ViewerDisplayEvent::Applied { .. } | ViewerDisplayEvent::Refused { .. }
            )
        {
            if let (Some(target), Some((owner, instance))) =
                (shape.desired(), shape.owner_identity())
            {
                let key = (owner.clone(), instance.clone(), target.clone());
                if current()? != target && shape.failed_target.as_ref() != Some(&key) {
                    let hooks = match self.display_hook_sizes(&launch, &context) {
                        Ok(hooks) => hooks,
                        Err(_) => {
                            shape.failed_target = Some(key);
                            shape.portrait = before_portrait;
                            shape.queue_notice();
                            return Ok(SessionViewerDisplay {
                                display: current()?,
                                owner: shape.is_owner(peer, connection),
                                portrait: shape.portrait,
                                revision: None,
                                notice: shape.take_notice(peer, connection),
                            });
                        }
                    };
                    let previous = effect::capture(control.as_ref(), &displays, &placement, hooks)
                        .map_err(|m| failure("DisplayChangeUnavailable", m))?;
                    context.pending = Some(previous.clone());
                    self.save_display_context(&launch, &context)?;
                    match effect::reshape(
                        control.as_ref(),
                        &displays,
                        &placement,
                        &target,
                        &previous,
                        |index, size| {
                            self.display_hook(&launch, &context, index, Some(size))
                                .map(|_| ())
                        },
                    ) {
                        Ok(target) => {
                            shape.pending = Some(ShapeAttempt {
                                revision: crate::generate_launch_id(),
                                peer: owner,
                                connection: instance,
                                previous,
                                target,
                                previous_portrait: before_portrait,
                                deadline: Instant::now() + Duration::from_secs(10),
                            })
                        }
                        Err(effect::ShapeFailure::RecoveryRequired) => {
                            shape.pending = Some(ShapeAttempt {
                                revision: crate::generate_launch_id(),
                                peer: owner,
                                connection: instance,
                                previous,
                                target,
                                previous_portrait: before_portrait,
                                deadline: now,
                            });
                            return Err(failure(
                                "DisplayRollbackFailed",
                                "Couldn't reshape; previous display state could not be confirmed.",
                            ));
                        }
                        Err(_) => {
                            context.pending = None;
                            self.save_display_context(&launch, &context)?;
                            shape.failed_target = Some(key);
                            shape.portrait = before_portrait;
                            notice = Some("Couldn't reshape".into());
                        }
                    }
                }
            }
        }
        if shape.pending.is_none() && context.portrait != shape.portrait {
            context.portrait = shape.portrait;
            self.save_display_context(&launch, &context)?;
        }
        self.invalidate_catalog_placement();
        if notice.is_some() {
            shape.queue_notice();
        }
        let owner = shape.is_owner(peer, connection);
        let revision = shape
            .pending
            .as_ref()
            .filter(|p| p.peer == peer && p.connection == connection)
            .map(|p| p.revision.clone());
        Ok(SessionViewerDisplay {
            display: current()?,
            owner,
            portrait: shape.portrait,
            revision,
            notice: shape.take_notice(peer, connection),
        })
    }
}
