//! Ports of `clients/portal/src/overlay/overlay-model.ts`,
//! `live-overlay-controller.ts`, `reconcile-focused-session.ts` and the
//! presentation switch in `clients/portal/src/main.tsx`.
//!
//! The portal opened the gameplay overlay when its window gained focus and
//! korrid reported its Home intent (`SessionStatus.overlay`) for a launch that
//! is frozen or focus-failed. The native shell sends korrid no focus events,
//! so korrid inspects that same intent on a timer. After Open Korri the
//! portal reopened only a focus-failed launch while its window kept focus;
//! korrid keeps that rule, and reopens a frozen launch only when the intent
//! names a different launch.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use korri_surface_treaty::{
    SurfaceCatalog, SurfaceGameplayControl, SurfaceGameplayControlGroup,
    SurfaceGameplayControlInteraction, SurfaceGameplayControlValue, SurfaceGameplayOverlayKind,
    SurfaceGameplayOverlayPresentation, SurfaceModel, SurfacePresentation, SurfaceSettingChoice,
    SurfaceSettingsStatus, SurfaceStatus,
};

use super::{korrid::Korrid, Notify};
use crate::{
    ActiveSession, SessionControl, SessionControlFailureReason, SessionControlGroup,
    SessionControlInteraction, SessionControlInvokeOutcome, SessionControlValue, SessionControls,
    SessionControlsOutcome, SessionEndOutcome, SessionEndPhase, SessionStatusOutcome,
    SessionTransitionOutcome, SettingsSnapshotOutcome,
};

pub const OVERLAY_RESUME_CONTROL_ID: &str = "overlay:resume";
pub const OVERLAY_DISCONNECT_CONTROL_ID: &str = "overlay:disconnect";
pub const OVERLAY_END_CONTROL_ID: &str = "overlay:end";
pub const OVERLAY_OPEN_KORRI_CONTROL_ID: &str = "overlay:open-korri";
const STATUS_TIMEOUT: Duration = Duration::from_millis(2000);

fn core_control(
    id: &str,
    label: &str,
    description: Option<&str>,
    destructive: bool,
) -> SurfaceGameplayControl {
    SurfaceGameplayControl {
        id: id.into(),
        label: label.into(),
        description: description.map(Into::into),
        enabled: true,
        disabled_reason: None,
        destructive,
        dismiss_on_success: true,
        interaction: SurfaceGameplayControlInteraction::Command,
    }
}

fn core_controls() -> Vec<SurfaceGameplayControl> {
    core_controls_for(cfg!(target_os = "android"))
}

fn core_controls_for(stream_viewer: bool) -> Vec<SurfaceGameplayControl> {
    let mut controls = vec![
        core_control(OVERLAY_RESUME_CONTROL_ID, "Resume", None, false),
        core_control(
            OVERLAY_END_CONTROL_ID,
            "End",
            Some("Unsaved progress will be lost."),
            true,
        ),
        core_control(OVERLAY_OPEN_KORRI_CONTROL_ID, "Open Korri", None, false),
    ];
    if stream_viewer {
        controls.insert(
            2,
            core_control(OVERLAY_DISCONNECT_CONTROL_ID, "Disconnect", None, false),
        );
    }
    controls
}

fn presentation_interaction(
    interaction: &SessionControlInteraction,
) -> SurfaceGameplayControlInteraction {
    match interaction {
        SessionControlInteraction::Command => SurfaceGameplayControlInteraction::Command,
        SessionControlInteraction::Toggle {
            value,
            true_label,
            false_label,
        } => SurfaceGameplayControlInteraction::Toggle {
            value: *value,
            true_label: true_label.clone(),
            false_label: false_label.clone(),
        },
        SessionControlInteraction::Choice { value, options } => {
            SurfaceGameplayControlInteraction::Choice {
                value: value.clone(),
                options: options
                    .iter()
                    .map(|option| SurfaceSettingChoice {
                        value: option.value.clone(),
                        label: option.label.clone(),
                    })
                    .collect(),
            }
        }
        SessionControlInteraction::Range {
            value,
            min,
            max,
            step,
        } => SurfaceGameplayControlInteraction::Range {
            value: *value,
            min: *min,
            max: *max,
            step: *step,
        },
    }
}

fn presentation_control(control: &SessionControl) -> SurfaceGameplayControl {
    SurfaceGameplayControl {
        id: control.id.clone(),
        label: control.label.clone(),
        description: control.description.clone(),
        enabled: control.enabled,
        disabled_reason: control.disabled_reason.clone(),
        destructive: control.destructive,
        dismiss_on_success: control.dismiss_on_success,
        interaction: presentation_interaction(&control.interaction),
    }
}

fn presentation_group(group: &SessionControlGroup) -> Option<SurfaceGameplayControlGroup> {
    let controls: Vec<_> = group
        .controls
        .iter()
        .filter(|control| control.enabled)
        .map(presentation_control)
        .collect();
    (!controls.is_empty()).then(|| SurfaceGameplayControlGroup {
        id: group.id.clone(),
        label: group.label.clone(),
        controls,
    })
}

/// Strip active-session and integration authority while translating korrid's
/// materialized controls into the surface model.
pub fn gameplay_overlay_presentation_from(
    controls: &SessionControls,
    stream_players: &[String],
) -> SurfaceGameplayOverlayPresentation {
    SurfaceGameplayOverlayPresentation {
        kind: SurfaceGameplayOverlayKind,
        battery_percent: None,
        stream_health: None,
        title: controls.title.clone(),
        controls: core_controls(),
        groups: controls
            .groups
            .iter()
            .filter_map(presentation_group)
            .collect(),
        stream_players: (!stream_players.is_empty()).then(|| stream_players.to_vec()),
    }
}

fn model_for(launch_id: &str, title: Option<String>) -> SurfaceModel {
    SurfaceModel {
        displays: None,
        presentation: SurfacePresentation::GameplayOverlay(gameplay_overlay_presentation_from(
            &SessionControls {
                launch_id: launch_id.into(),
                title,
                groups: Vec::new(),
                retroarch_telemetry: None,
            },
            &[],
        )),
        catalog: SurfaceCatalog::Empty,
        status: SurfaceStatus::Busy {
            kicker: "Loading session…".into(),
            detail: None,
            progress: None,
            steps: None,
            actions: None,
            decline_action_id: None,
            game_id: None,
        },
        save_history: None,
        runner_choice: None,
        launch_settings: None,
        clock_label: None,
        actions: Vec::new(),
        settings: Vec::new(),
        settings_status: SurfaceSettingsStatus::Idle,
        identity_management: None,
        build_label: None,
        network: None,
        onboarding: None,
        store: None,
        invites: None,
        focused_game: None,
    }
}

fn is_stale_session_code(code: &str) -> bool {
    code == "StaleLaunchIdentity" || code == "SelectedRemoteSessionReplaced"
}

fn action_failure(reason: SessionControlFailureReason, message: &str) -> String {
    match reason {
        SessionControlFailureReason::NotSessionStarter => {
            "Only the device that started this session can use its gameplay actions.".into()
        }
        SessionControlFailureReason::StaleSession => {
            "That game is no longer running. Reload the session before acting.".into()
        }
        SessionControlFailureReason::UnknownControl => {
            "That gameplay action is no longer available.".into()
        }
        SessionControlFailureReason::Disabled if message.is_empty() => {
            "That gameplay action is unavailable right now.".into()
        }
        SessionControlFailureReason::Unavailable if message.is_empty() => {
            "That gameplay action did not answer.".into()
        }
        SessionControlFailureReason::Disabled | SessionControlFailureReason::Unavailable => {
            message.into()
        }
        SessionControlFailureReason::InvalidValue => {
            "That value is not valid for this gameplay action.".into()
        }
    }
}

/// What the presentation switch should do after an overlay operation.
#[derive(Clone, Debug, PartialEq)]
pub enum OverlayExit {
    /// Show the catalog: the session ended.
    SessionEnded,
    /// Show the catalog with the existing launch notice.
    SessionFailed(String),
    /// The live Android viewer has resumed and released its Surface.
    Resumed,
    /// Show the catalog while watching the exact launch's return.
    OpenKorri,
}

#[derive(Clone)]
enum Retry {
    Refresh,
    Resume,
    End,
    Viewer(crate::SessionViewerAction),
    Invoke(String, Option<SurfaceGameplayControlValue>),
}

struct Inner {
    current: SurfaceModel,
    destroyed: bool,
    epoch: u64,
    refresh_generation: u64,
    retry: Option<Retry>,
    sticky: bool,
    viewer: Option<(std::time::Instant, crate::linux_viewer::ViewerSnapshot)>,
    viewer_generation: u64,
}

/// One live overlay for one exact launch.
pub struct OverlayController {
    launch_id: String,
    korrid: Korrid,
    inner: Mutex<Inner>,
    notify: Notify,
    exit: Box<dyn Fn(OverlayExit) + Send + Sync>,
    viewer_operation: tokio::sync::Mutex<()>,
}

impl OverlayController {
    pub fn new(
        launch_id: String,
        title: Option<String>,
        korrid: Korrid,
        notify: Notify,
        exit: Box<dyn Fn(OverlayExit) + Send + Sync>,
    ) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                current: model_for(&launch_id, title),
                destroyed: false,
                epoch: 0,
                refresh_generation: 0,
                retry: None,
                sticky: false,
                viewer: None,
                viewer_generation: 0,
            }),
            launch_id,
            korrid,
            viewer_operation: tokio::sync::Mutex::new(()),
            notify,
            exit,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("overlay poisoned")
    }

    pub fn launch_id(&self) -> &str {
        &self.launch_id
    }

    pub fn model(&self) -> SurfaceModel {
        let inner = self.lock();
        let mut model = inner.current.clone();
        if let SurfacePresentation::GameplayOverlay(presentation) = &mut model.presentation {
            super::viewer::project(
                presentation,
                inner.viewer.as_ref(),
                std::time::Instant::now(),
            );
        }
        model
    }

    /// Local observations never wait for holder plugin discovery or mutate holder authority.
    pub async fn refresh_viewer(&self) -> bool {
        let generation = {
            let mut inner = self.lock();
            if inner.destroyed {
                return false;
            }
            inner.viewer_generation += 1;
            inner.viewer_generation
        };
        let snapshot = match &self.korrid.viewer {
            Some(viewer) => {
                tokio::time::timeout(Duration::from_secs(1), viewer.snapshot(&self.launch_id))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .flatten()
            }
            None => None,
        };
        let mut inner = self.lock();
        if inner.destroyed {
            return false;
        }
        if generation != inner.viewer_generation {
            return true;
        }
        inner.viewer = snapshot.map(|snapshot| (std::time::Instant::now(), snapshot));
        drop(inner);
        (self.notify)();
        true
    }

    fn publish(&self, change: impl FnOnce(&mut SurfaceModel), expected_epoch: Option<u64>) {
        {
            let mut inner = self.lock();
            if inner.destroyed || expected_epoch.is_some_and(|epoch| epoch != inner.epoch) {
                return;
            }
            change(&mut inner.current);
        }
        (self.notify)();
    }

    fn problem(&self, kicker: &str, reason: String, retry: Option<Retry>) {
        let can_retry = retry.is_some();
        self.lock().retry = retry;
        self.publish(
            |model| {
                model.status = SurfaceStatus::Problem {
                    kicker: kicker.into(),
                    reason,
                    can_retry,
                    game_id: None,
                    game_title: None,
                }
            },
            None,
        );
    }

    fn stale(&self) {
        {
            let mut inner = self.lock();
            inner.retry = None;
            inner.sticky = false;
        }
        self.publish(
            |model| {
                if let SurfacePresentation::GameplayOverlay(presentation) = &mut model.presentation
                {
                    presentation.groups.clear();
                }
                model.status = SurfaceStatus::Problem {
                    kicker: "Session changed".into(),
                    reason: "That game is no longer running. Reload the session before acting."
                        .into(),
                    can_retry: false,
                    game_id: None,
                    game_title: None,
                };
            },
            None,
        );
    }

    fn remove_plugin_control(&self, control_id: &str) {
        self.publish(
            |model| {
                if let SurfacePresentation::GameplayOverlay(presentation) = &mut model.presentation
                {
                    for group in &mut presentation.groups {
                        group.controls.retain(|control| control.id != control_id);
                    }
                    presentation
                        .groups
                        .retain(|group| !group.controls.is_empty());
                }
            },
            None,
        );
    }

    fn could_not_resume(&self) {
        self.problem(
            "Could not resume",
            "The game is running, but Korri could not bring its window forward. Resume is still available.".into(),
            Some(Retry::Resume),
        );
    }

    pub async fn refresh(&self) {
        let (expected_epoch, expected_refresh) = {
            let mut inner = self.lock();
            if inner.destroyed {
                return;
            }
            inner.refresh_generation += 1;
            (inner.epoch, inner.refresh_generation)
        };
        let is_current = || {
            let inner = self.lock();
            !inner.destroyed
                && inner.epoch == expected_epoch
                && inner.refresh_generation == expected_refresh
        };
        let status = self.korrid.session_status(Some(STATUS_TIMEOUT)).await;
        if !is_current() {
            return;
        }
        let active = match status {
            SessionStatusOutcome::Err(failure) => {
                if is_stale_session_code(&failure.code) {
                    self.stale();
                } else if failure.code == "HostLaunchFailed" {
                    self.lock().sticky = false;
                    (self.exit)(OverlayExit::SessionFailed(failure.message));
                } else if failure.code == "SessionCompleted" || failure.code == "NoActiveSession" {
                    self.lock().sticky = false;
                    (self.exit)(OverlayExit::SessionEnded);
                } else if !self.lock().sticky {
                    self.problem("Session unavailable", failure.message, Some(Retry::Refresh));
                }
                return;
            }
            SessionStatusOutcome::Ok(status) => status.active,
        };
        let Some(active) = active else {
            self.lock().sticky = false;
            (self.exit)(OverlayExit::SessionEnded);
            return;
        };
        if active.launch_id != self.launch_id {
            self.stale();
            return;
        }
        if active.phase.as_deref() == Some("focus-failed")
            || (!cfg!(target_os = "android") && active.phase.as_deref() == Some("running"))
        {
            self.lock().sticky = false;
            self.could_not_resume();
            return;
        }
        let listed = self.korrid.session_controls(self.launch_id.clone()).await;
        if !is_current() {
            return;
        }
        let controls = match listed {
            SessionControlsOutcome::Err(failure)
                if cfg!(target_os = "android")
                    && failure.reason == SessionControlFailureReason::Unavailable =>
            {
                SessionControls {
                    launch_id: self.launch_id.clone(),
                    title: active.title.clone(),
                    groups: Vec::new(),
                    retroarch_telemetry: None,
                }
            }
            SessionControlsOutcome::Err(failure) => {
                if failure.reason == SessionControlFailureReason::StaleSession {
                    self.stale();
                } else if !self.lock().sticky {
                    self.problem(
                        "Actions unavailable",
                        action_failure(failure.reason, &failure.message),
                        Some(Retry::Refresh),
                    );
                }
                return;
            }
            SessionControlsOutcome::Ok(controls) => controls,
        };
        if controls.launch_id != self.launch_id {
            self.stale();
            return;
        }
        if self.lock().sticky {
            return;
        }
        let person_name = if active.stream_players.is_empty() {
            None
        } else {
            match self.korrid.settings_snapshot().await {
                SettingsSnapshotOutcome::Ok(settings) => settings.person_name,
                SettingsSnapshotOutcome::Err(_) => None,
            }
        };
        if !is_current() {
            return;
        }
        let stream_players: Vec<String> = active
            .stream_players
            .iter()
            .map(|player| match &person_name {
                Some(name) if !name.trim().is_empty() => name.clone(),
                _ => player.label.clone(),
            })
            .collect();
        self.lock().retry = None;
        self.publish(
            |model| {
                model.presentation = SurfacePresentation::GameplayOverlay(
                    gameplay_overlay_presentation_from(&controls, &stream_players),
                );
                model.status = SurfaceStatus::Browsing;
            },
            Some(expected_epoch),
        );
    }

    /// Begin a command: later replies of older operations are dropped.
    fn begin(&self) -> u64 {
        let mut inner = self.lock();
        inner.retry = None;
        inner.sticky = false;
        inner.refresh_generation += 1;
        inner.epoch += 1;
        inner.epoch
    }

    fn current(&self, epoch: u64) -> bool {
        let inner = self.lock();
        !inner.destroyed && inner.epoch == epoch
    }

    pub async fn resume_game(&self) {
        let epoch = self.begin();
        let outcome = self.korrid.session_resume(self.launch_id.clone()).await;
        if !self.current(epoch) {
            return;
        }
        let SessionTransitionOutcome::Err(failure) = outcome else {
            if cfg!(target_os = "android") {
                (self.exit)(OverlayExit::Resumed);
            }
            return;
        };
        if failure.code == "HostFocusFailed" {
            self.could_not_resume();
        } else if failure.code == "HostLaunchFailed" {
            self.lock().sticky = false;
            (self.exit)(OverlayExit::SessionFailed(failure.message));
        } else if matches!(
            failure.code.as_str(),
            "SessionCompleted" | "NoActiveSession"
        ) {
            self.lock().sticky = false;
            (self.exit)(OverlayExit::SessionEnded);
        } else if is_stale_session_code(&failure.code) {
            self.stale();
        } else {
            self.problem("Could not resume", failure.message, Some(Retry::Resume));
        }
    }

    async fn end_session(&self) {
        let epoch = self.begin();
        let outcome = self.korrid.session_end(self.launch_id.clone()).await;
        if !self.current(epoch) {
            return;
        }
        match outcome {
            SessionEndOutcome::Ok(result) => {
                if matches!(result.phase, SessionEndPhase::Ended) {
                    (self.exit)(OverlayExit::SessionEnded);
                    return;
                }
                self.lock().sticky = true;
                self.problem(
                    "Game is still stopping",
                    "Korri has not confirmed that this exact game stopped.".into(),
                    Some(Retry::End),
                );
            }
            SessionEndOutcome::Err(failure) => {
                if is_stale_session_code(&failure.code)
                    || failure.code == "NoActiveSession"
                    || failure.code == "SessionCompleted"
                {
                    self.stale();
                } else {
                    self.problem("Could not end game", failure.message, Some(Retry::End));
                }
            }
        }
    }

    async fn invoke_plugin(&self, control_id: String, value: Option<SurfaceGameplayControlValue>) {
        let epoch = self.begin();
        let wire = value.clone().map(|value| match value {
            SurfaceGameplayControlValue::Toggle { value } => SessionControlValue::Toggle(value),
            SurfaceGameplayControlValue::Choice { value } => SessionControlValue::Choice(value),
            SurfaceGameplayControlValue::Range { value } => SessionControlValue::Range(value),
        });
        let outcome = self
            .korrid
            .invoke_session_control(self.launch_id.clone(), control_id.clone(), wire)
            .await;
        if !self.current(epoch) {
            return;
        }
        if let SessionControlInvokeOutcome::Err(failure) = outcome {
            if failure.reason == SessionControlFailureReason::StaleSession {
                self.stale();
            } else {
                self.lock().sticky = true;
                let unknown = failure.reason == SessionControlFailureReason::UnknownControl;
                if unknown {
                    self.remove_plugin_control(&control_id);
                }
                self.problem(
                    "Action refused",
                    action_failure(failure.reason, &failure.message),
                    (!unknown).then_some(Retry::Invoke(control_id, value)),
                );
            }
            return;
        }
        let dismiss = match &self.lock().current.presentation {
            SurfacePresentation::GameplayOverlay(presentation) => presentation
                .groups
                .iter()
                .flat_map(|group| &group.controls)
                .find(|control| control.id == control_id)
                .is_some_and(|control| control.dismiss_on_success),
            SurfacePresentation::Catalog(_) => false,
        };
        if !dismiss {
            self.refresh().await;
            return;
        }
        let status = self.korrid.session_status(Some(STATUS_TIMEOUT)).await;
        if !self.current(epoch) {
            return;
        }
        match status {
            SessionStatusOutcome::Err(failure)
                if failure.code == "SessionCompleted" || failure.code == "NoActiveSession" =>
            {
                (self.exit)(OverlayExit::SessionEnded)
            }
            SessionStatusOutcome::Err(failure) => {
                self.problem("Action completed", failure.message, Some(Retry::Refresh))
            }
            SessionStatusOutcome::Ok(status)
                if status.active.as_ref().map(|active| &active.launch_id)
                    != Some(&self.launch_id) =>
            {
                self.stale()
            }
            SessionStatusOutcome::Ok(_) => Box::pin(self.resume_game()).await,
        }
    }

    async fn viewer_action(&self, action: crate::SessionViewerAction) {
        let epoch = self.begin();
        let outcome = self
            .korrid
            .session_viewer(self.launch_id.clone(), action)
            .await;
        if !self.current(epoch) {
            return;
        }
        match outcome {
            crate::SessionViewerOutcome::Ok => (self.exit)(match action {
                crate::SessionViewerAction::OpenKorri => OverlayExit::OpenKorri,
                crate::SessionViewerAction::Disconnect => OverlayExit::SessionEnded,
            }),
            crate::SessionViewerOutcome::Err(failure) => self.problem(
                "Could not leave the stream",
                failure.message,
                Some(Retry::Viewer(action)),
            ),
        }
    }

    pub async fn invoke(&self, control_id: String, value: Option<SurfaceGameplayControlValue>) {
        if self.lock().destroyed {
            return;
        }
        if control_id.starts_with("viewer:") {
            self.invoke_viewer(control_id, value).await;
            return;
        }
        match control_id.as_str() {
            OVERLAY_RESUME_CONTROL_ID => self.resume_game().await,
            OVERLAY_END_CONTROL_ID => self.end_session().await,
            OVERLAY_OPEN_KORRI_CONTROL_ID if cfg!(target_os = "android") => {
                self.viewer_action(crate::SessionViewerAction::OpenKorri)
                    .await
            }
            OVERLAY_DISCONNECT_CONTROL_ID if cfg!(target_os = "android") => {
                self.viewer_action(crate::SessionViewerAction::Disconnect)
                    .await
            }
            OVERLAY_OPEN_KORRI_CONTROL_ID => (self.exit)(OverlayExit::OpenKorri),
            _ => self.invoke_plugin(control_id, value).await,
        }
    }

    async fn invoke_viewer(&self, control_id: String, value: Option<SurfaceGameplayControlValue>) {
        let _operation = self.viewer_operation.lock().await;
        if self.lock().destroyed {
            return;
        }
        let control = match self.model().presentation {
            SurfacePresentation::GameplayOverlay(presentation) => presentation
                .groups
                .into_iter()
                .filter(|group| group.id == super::viewer::GROUP_ID)
                .flat_map(|group| group.controls)
                .find(|control| control.id == control_id),
            _ => None,
        };
        let Some(control) = control.filter(|control| {
            control.enabled && super::viewer::valid(&control.interaction, &value)
        }) else {
            self.lock().sticky = true;
            self.problem(
                "Control unavailable",
                "That stream control changed. Reopen the overlay and try again.".into(),
                None,
            );
            return;
        };
        let Some(viewer) = &self.korrid.viewer else {
            return;
        };
        let epoch = self.begin();
        if control.dismiss_on_success {
            if let SessionTransitionOutcome::Err(failure) =
                self.korrid.session_resume(self.launch_id.clone()).await
            {
                self.problem("Could not resume", failure.message, Some(Retry::Resume));
                return;
            }
        }
        if !self.current(epoch) {
            return;
        }
        let result = viewer.apply(&self.launch_id, &control_id, value).await;
        if !self.current(epoch) {
            return;
        }
        match result {
            Ok(()) if control.dismiss_on_success => (self.exit)(OverlayExit::Resumed),
            Ok(()) => {
                self.refresh_viewer().await;
            }
            Err(_) => {
                self.lock().sticky = true;
                self.problem(
                    "Stream control refused",
                    "The local viewer could not apply that stream control.".into(),
                    None,
                )
            }
        }
    }

    pub async fn retry(&self) {
        let operation = {
            let mut inner = self.lock();
            inner.sticky = false;
            inner.retry.take()
        };
        match operation {
            None => {}
            Some(Retry::Refresh) => self.refresh().await,
            Some(Retry::Resume) => self.resume_game().await,
            Some(Retry::End) => self.end_session().await,
            Some(Retry::Viewer(action)) => self.viewer_action(action).await,
            Some(Retry::Invoke(control_id, value)) => {
                Box::pin(self.invoke_plugin(control_id, value)).await
            }
        }
    }

    pub fn destroy(&self) {
        let mut inner = self.lock();
        inner.destroyed = true;
        inner.epoch += 1;
        inner.refresh_generation += 1;
    }
}

/// Port of `reconcileFocusedSession`: the exact launch korrid's Home intent
/// names, when it is frozen or focus-failed.
pub async fn focused_overlay_session(
    korrid: &Korrid,
    allow_frozen: bool,
    expected_launch_id: Option<&str>,
    attempts: usize,
) -> Result<Option<ActiveSession>, crate::RpcFailure> {
    for attempt in 0..attempts.max(1) {
        match korrid.session_status(Some(STATUS_TIMEOUT)).await {
            SessionStatusOutcome::Ok(status) => {
                return Ok(status.overlay.filter(|active| {
                    expected_launch_id.is_none_or(|expected| active.launch_id == expected)
                        && ((allow_frozen && matches!(active.phase.as_deref(), Some("frozen"))
                            || (allow_frozen
                                && cfg!(target_os = "android")
                                && active.phase.as_deref() == Some("running")))
                            || active.phase.as_deref() == Some("focus-failed"))
                }));
            }
            SessionStatusOutcome::Err(failure) if failure.code == "HostLaunchFailed" => {
                return Err(failure);
            }
            SessionStatusOutcome::Err(_) => {
                if attempt + 1 < attempts {
                    tokio::time::sleep(Duration::from_millis(75)).await;
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod live_overlay_tests {
    use super::*;

    #[tokio::test]
    async fn viewer_controls_apply_locally_read_back_and_never_invoke_streamed_session() {
        use crate::linux_viewer::{
            test_backend::TestLinuxStreamViewer, LinuxViewerStatus, ViewerSnapshot,
        };
        let viewer = Arc::new(TestLinuxStreamViewer::default());
        viewer.seed("launch", LinuxViewerStatus::Running);
        viewer.set_snapshot(Some(ViewerSnapshot {
            controls: vec![SurfaceGameplayControl {
                id: "viewer:launch:1:1:fill".into(),
                label: "Crop to fill".into(),
                description: None,
                enabled: true,
                disabled_reason: None,
                destructive: false,
                dismiss_on_success: false,
                interaction: SurfaceGameplayControlInteraction::Toggle {
                    value: false,
                    true_label: "On".into(),
                    false_label: "Off".into(),
                },
            }],
            ..Default::default()
        }));
        let korrid = Korrid::new(Arc::new(|_| {
            panic!("a local viewer operation must never invoke holder RPC")
        }))
        .with_viewer(Some(viewer.clone()));
        let overlay = OverlayController::new(
            "launch".into(),
            Some("Game".into()),
            korrid,
            Arc::new(|| {}),
            Box::new(|_| panic!("toggle must stay open")),
        );
        overlay.refresh_viewer().await;
        overlay
            .invoke(
                "viewer:launch:1:1:fill".into(),
                Some(SurfaceGameplayControlValue::Toggle { value: true }),
            )
            .await;
        assert_eq!(
            viewer.applications(),
            vec![("launch".into(), "viewer:launch:1:1:fill".into())]
        );
        let SurfacePresentation::GameplayOverlay(presentation) = overlay.model().presentation
        else {
            panic!()
        };
        assert!(matches!(
            presentation.groups[0].controls[0].interaction,
            SurfaceGameplayControlInteraction::Toggle { value: true, .. }
        ));
        overlay.invoke("viewer:old:1:1:fill".into(), None).await;
        overlay
            .invoke(
                "viewer:launch:1:1:fill".into(),
                Some(SurfaceGameplayControlValue::Range { value: 2.0 }),
            )
            .await;
        assert_eq!(viewer.applications().len(), 1);
        viewer.set_snapshot(None);
        overlay.refresh_viewer().await;
        let SurfacePresentation::GameplayOverlay(presentation) = overlay.model().presentation
        else {
            panic!()
        };
        assert!(presentation.groups.is_empty() && presentation.stream_health.is_none());
        overlay.destroy();
        assert!(!overlay.refresh_viewer().await);
    }

    #[test]
    fn android_stream_overlay_offers_disconnect_without_platform_instructions() {
        let controls = core_controls_for(true);
        assert_eq!(
            controls
                .iter()
                .map(|c| c.label.as_str())
                .collect::<Vec<_>>(),
            ["Resume", "End", "Disconnect", "Open Korri"]
        );
        let control = &controls[2];
        assert!(control.enabled && control.dismiss_on_success && !control.destructive);
        assert_eq!(
            control.interaction,
            SurfaceGameplayControlInteraction::Command
        );
        // No new surface request or special meaning for packages to infer.
        let request: korri_surface_treaty::SurfaceRequest =
            serde_json::from_value(serde_json::json!({
                "_tag": "InvokeGameplayControl", "controlId": control.id,
            }))
            .unwrap();
        assert_eq!(request.tag(), "InvokeGameplayControl");
        assert_eq!(
            core_controls_for(false)
                .iter()
                .map(|c| c.label.as_str())
                .collect::<Vec<_>>(),
            ["Resume", "End", "Open Korri"]
        );
    }
}
