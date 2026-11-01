//! Port of `clients/portal/src/surface/runner-chooser.ts`: the installed
//! runner choice for a local game. Each chooser read or write has one
//! generation. A dispatched startup belongs to the shared launch lifecycle;
//! panel Cancel only withdraws a choice or a save.

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use korri_surface_treaty::{
    SurfaceAction, SurfaceRunnerChoice, SurfaceRunnerChoiceContent, SurfaceRunnerRoute,
    SurfaceRunnerSaved,
};

use super::{
    korrid::{ask, Korrid},
    launchables::{driver::SessionStartResult, state::is_authoritative_session_status},
    Notify,
};
use crate::{
    game_routes::{GameRouteSelection, GameRoutes, SelectedGameLaunchOutcome},
    launch_settings::{
        LaunchLayer, LaunchPath, LaunchPlace, LaunchRunnerScope, LaunchSetRequest,
        LaunchSettingsWriteOutcome,
    },
    launcher::typed_settings::LaunchWarning,
    ActiveSession, RpcFailure, SessionEndOutcome, SessionStatusOutcome,
};

pub type StartFuture = Pin<Box<dyn Future<Output = Option<SessionStartResult>> + Send>>;

/// The chooser's road into the shared launch lifecycle.
pub struct RunnerLaunchIntegration {
    /// Starts in the shared Busy; the chooser owns choices, not startup.
    pub start_launch: Box<dyn Fn(String, String) -> StartFuture + Send + Sync>,
    pub return_session: Box<dyn Fn(ActiveSession) + Send + Sync>,
    pub reload: Box<dyn Fn() + Send + Sync>,
}

fn cancel_action() -> SurfaceAction {
    SurfaceAction::new("runner:cancel", "Cancel", true)
}

fn warning_text(warning: &LaunchWarning) -> String {
    format!(
        "{} · {} · {}: {}",
        warning.setting, warning.runner_id, warning.build, warning.message
    )
}

#[derive(Clone, Copy, PartialEq)]
pub enum Intent {
    Launch,
    Inspect,
}

struct Inner {
    state: SurfaceRunnerChoice,
    generation: u64,
    title: String,
    game_id: String,
    snapshot: Option<GameRoutes>,
    active: Option<ActiveSession>,
}

pub struct RunnerChooser {
    korrid: Korrid,
    inner: Mutex<Inner>,
    notify: Notify,
    launches: RunnerLaunchIntegration,
}

fn failure_of(status: &SessionStatusOutcome) -> Option<RpcFailure> {
    match status {
        SessionStatusOutcome::Err(failure) => Some(failure.clone()),
        SessionStatusOutcome::Ok(payload) => payload.observation_failure.clone(),
    }
}

impl RunnerChooser {
    pub fn new(korrid: Korrid, notify: Notify, launches: RunnerLaunchIntegration) -> Arc<Self> {
        Arc::new(Self {
            korrid,
            inner: Mutex::new(Inner {
                state: SurfaceRunnerChoice::Closed,
                generation: 0,
                title: String::new(),
                game_id: String::new(),
                snapshot: None,
                active: None,
            }),
            notify,
            launches,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("runner chooser poisoned")
    }

    pub fn snapshot(&self) -> SurfaceRunnerChoice {
        self.lock().state.clone()
    }

    pub fn is_closed(&self) -> bool {
        matches!(self.lock().state, SurfaceRunnerChoice::Closed)
    }

    fn action(&self, id: &str, label: &str) -> SurfaceAction {
        SurfaceAction::new(
            format!("runner:{}:{id}", self.lock().generation),
            label,
            true,
        )
    }

    fn publish(&self, next: SurfaceRunnerChoice) {
        self.lock().state = next;
        (self.notify)();
    }

    fn blank(&self, message: impl Into<String>) -> SurfaceRunnerChoiceContent {
        SurfaceRunnerChoiceContent {
            game_title: self.lock().title.clone(),
            message: message.into(),
            routes: Vec::new(),
            saved: Vec::new(),
            warnings: Vec::new(),
            actions: vec![cancel_action()],
        }
    }

    pub fn cancel(&self) {
        self.lock().generation += 1;
        self.publish(SurfaceRunnerChoice::Closed);
    }

    fn current(&self, operation: u64) -> bool {
        self.lock().generation == operation
    }

    fn bump(&self) -> u64 {
        let mut inner = self.lock();
        inner.generation += 1;
        inner.generation
    }

    fn problem(&self, failure: &RpcFailure) {
        let mut content = self.blank(failure.message.clone());
        content.actions = vec![self.action("reload", "Reload runners"), cancel_action()];
        self.publish(if failure.code == "SettingsConflict" {
            SurfaceRunnerChoice::Conflict(content)
        } else {
            SurfaceRunnerChoice::Error(content)
        });
    }

    fn show(&self, record: GameRoutes) {
        let mut saved: Vec<SurfaceRunnerSaved> = record
            .game_runner
            .iter()
            .map(|runner_id| SurfaceRunnerSaved {
                label: "This game".into(),
                runner_id: runner_id.clone(),
            })
            .collect();
        let mut systems: Vec<(&String, &String)> = record.system_runners.iter().collect();
        systems.sort();
        saved.extend(
            systems
                .iter()
                .map(|(system_id, runner_id)| SurfaceRunnerSaved {
                    label: format!("System {system_id}"),
                    runner_id: (*runner_id).clone(),
                }),
        );
        let stale = saved.iter().any(|choice| {
            !record
                .routes
                .iter()
                .any(|route| route.runner_id == choice.runner_id)
        });
        let message = if stale {
            "A saved runner is unavailable for this game. The choice is still saved. Choose another runner or clear it.".to_owned()
        } else {
            format!(
                "{}Launch once does not change saved choices. A game choice overrides a system choice.",
                match &record.selection {
                    GameRouteSelection::Selected(runner_id) => format!("Current runner: {runner_id}. "),
                    GameRouteSelection::Choose => "Choose a runner on this device. ".into(),
                }
            )
        };
        let routes = record
            .routes
            .iter()
            .enumerate()
            .map(|(index, route)| SurfaceRunnerRoute {
                runner_id: route.runner_id.clone(),
                family_id: route.family_id.clone(),
                system_id: route.system_id.clone(),
                runner_build: route.runner_build.clone(),
                program: route.program.clone(),
                warnings: route.warnings.iter().map(warning_text).collect(),
                actions: vec![
                    self.action(&format!("launch:{index}"), "Launch once"),
                    self.action(&format!("game:{index}"), "Remember for this game"),
                    self.action(
                        &format!("system:{index}"),
                        &format!("Remember for system {}", route.system_id),
                    ),
                ],
            })
            .collect();
        let mut actions = Vec::new();
        if record.game_runner.is_some() {
            actions.push(self.action("clear-game", "Clear game choice"));
        }
        for (index, (system_id, _)) in systems.iter().enumerate() {
            actions.push(self.action(
                &format!("clear-system:{index}"),
                &format!("Clear system {system_id} choice"),
            ));
        }
        actions.push(self.action("reload", "Reload runners"));
        actions.push(cancel_action());
        let content = SurfaceRunnerChoiceContent {
            game_title: self.lock().title.clone(),
            message,
            routes,
            saved,
            warnings: Vec::new(),
            actions,
        };
        self.lock().snapshot = Some(record);
        self.publish(if stale {
            SurfaceRunnerChoice::Stale(content)
        } else {
            SurfaceRunnerChoice::Ready(content)
        });
    }

    async fn conflict(&self, operation: u64) {
        let result = self.korrid.session_status(None).await;
        if !self.current(operation) {
            return;
        }
        if !is_authoritative_session_status(&result) {
            if let Some(failure) = failure_of(&result) {
                self.problem(&failure);
            }
            return;
        }
        let active = match result {
            SessionStatusOutcome::Ok(payload) => payload.active,
            SessionStatusOutcome::Err(_) => None,
        };
        self.lock().active = active.clone();
        let mut content = self.blank(match &active {
            Some(active) => format!(
                "End {} before switching runners. Nothing will launch automatically after ending it.",
                active
                    .title
                    .clone()
                    .or_else(|| active.game_id.clone())
                    .unwrap_or_else(|| "the live session".into())
            ),
            None => "The active session changed. Reload runners before launching.".into(),
        });
        content.actions = active
            .iter()
            .map(|_| self.action("end", "End live session"))
            .chain([self.action("reload", "Reload runners"), cancel_action()])
            .collect();
        self.publish(SurfaceRunnerChoice::Conflict(content));
    }

    async fn launch(self: &Arc<Self>, runner_id: String, operation: u64) {
        let requested = self.lock().game_id.clone();
        // Shared startup and chooser dismissal happen in one dispatch.
        self.publish(SurfaceRunnerChoice::Closed);
        let Some(result) = (self.launches.start_launch)(requested, runner_id).await else {
            return;
        };
        if !self.current(operation) {
            return;
        }
        match result {
            SelectedGameLaunchOutcome::Err(failure) => {
                if failure.code == "ActiveSessionConflict" {
                    self.conflict(operation).await;
                } else {
                    self.problem(&failure);
                }
            }
            SelectedGameLaunchOutcome::Ok(payload) => {
                if payload.warnings.is_empty() {
                    self.cancel();
                } else {
                    let mut content = self.blank("Launched with settings warnings");
                    content.warnings = payload.warnings.iter().map(warning_text).collect();
                    content.actions = vec![self.action("dismiss", "Done")];
                    self.publish(SurfaceRunnerChoice::Warnings(content));
                }
            }
        }
    }

    pub fn open(self: &Arc<Self>, id: String, game_title: String, intent: Intent) {
        let this = Arc::clone(self);
        tokio::spawn(async move { this.open_now(id, game_title, intent).await });
    }

    async fn open_now(self: &Arc<Self>, id: String, game_title: String, intent: Intent) {
        let operation = {
            let mut inner = self.lock();
            inner.generation += 1;
            inner.game_id = id.clone();
            inner.title = game_title;
            inner.snapshot = None;
            inner.active = None;
            inner.generation
        };
        let content = self.blank("Reading installed runners…");
        self.publish(SurfaceRunnerChoice::Loading(content));
        if intent == Intent::Launch {
            let status = self
                .korrid
                .session_status(Some(Duration::from_millis(3000)))
                .await;
            if !self.current(operation) {
                return;
            }
            if !is_authoritative_session_status(&status) {
                if let Some(failure) = failure_of(&status) {
                    self.problem(&failure);
                }
                return;
            }
            if let SessionStatusOutcome::Ok(payload) = &status {
                if let Some(active) = payload
                    .active
                    .clone()
                    .filter(|active| active.game_id.as_ref() == Some(&id))
                {
                    // Continue addresses the observed exact launch.
                    self.publish(SurfaceRunnerChoice::Closed);
                    (self.launches.return_session)(active);
                    return;
                }
            }
        }
        let result = ask!(
            self.korrid,
            Route,
            GameRoutes,
            crate::game_routes::GameRoutesRequest { game_id: id },
            crate::game_routes::GameRoutesOutcome
        );
        if !self.current(operation) {
            return;
        }
        let record = match result {
            crate::game_routes::GameRoutesOutcome::Err(failure) => {
                self.problem(&failure);
                return;
            }
            crate::game_routes::GameRoutesOutcome::Ok(record) => record,
        };
        let selected = match &record.selection {
            GameRouteSelection::Selected(runner_id) => Some(runner_id.clone()),
            GameRouteSelection::Choose => None,
        };
        self.show(record);
        if intent == Intent::Launch {
            if let Some(runner_id) = selected {
                Box::pin(self.launch(runner_id, operation)).await;
            }
        }
    }

    fn published_actions(&self) -> Vec<SurfaceAction> {
        match &self.lock().state {
            SurfaceRunnerChoice::Closed => Vec::new(),
            SurfaceRunnerChoice::Loading(content)
            | SurfaceRunnerChoice::Ready(content)
            | SurfaceRunnerChoice::Stale(content)
            | SurfaceRunnerChoice::Conflict(content)
            | SurfaceRunnerChoice::Error(content)
            | SurfaceRunnerChoice::Busy(content)
            | SurfaceRunnerChoice::Warnings(content) => content
                .actions
                .iter()
                .chain(content.routes.iter().flat_map(|route| &route.actions))
                .cloned()
                .collect(),
        }
    }

    /// A `runner:` action id, without its prefix.
    pub fn act(self: &Arc<Self>, published_id: String) {
        let this = Arc::clone(self);
        tokio::spawn(async move { this.act_now(published_id).await });
    }

    async fn act_now(self: &Arc<Self>, published_id: String) {
        if self.is_closed() {
            return;
        }
        // Closing withdraws a choice and mutates nothing.
        if published_id == "cancel" {
            self.cancel();
            return;
        }
        // Only published actions are authority.
        let full = format!("runner:{published_id}");
        if !self
            .published_actions()
            .iter()
            .any(|action| action.id == full && action.enabled)
        {
            return;
        }
        let id = published_id[published_id.find(':').map_or(0, |at| at + 1)..].to_owned();
        if id == "dismiss" {
            self.cancel();
            return;
        }
        let (game_id, title) = {
            let inner = self.lock();
            (inner.game_id.clone(), inner.title.clone())
        };
        if id == "reload" {
            Box::pin(self.open_now(game_id, title, Intent::Inspect)).await;
            return;
        }
        let operation = self.bump();
        let active = self.lock().active.clone();
        if let (true, Some(active)) = (id == "end", active) {
            let launch_id = active.launch_id;
            self.publish(SurfaceRunnerChoice::Busy(self.blank(
                "Ending the exact live session… Cancel does not undo the end request.",
            )));
            if let SessionEndOutcome::Err(failure) =
                self.korrid.session_end(launch_id.clone()).await
            {
                if self.current(operation) {
                    self.problem(&failure);
                }
                return;
            }
            if !self.current(operation) {
                return;
            }
            let deadline = Instant::now() + Duration::from_secs(8);
            while self.current(operation) && Instant::now() < deadline {
                let status = self
                    .korrid
                    .session_status(Some(Duration::from_millis(3000)))
                    .await;
                if !self.current(operation) {
                    return;
                }
                if !is_authoritative_session_status(&status) {
                    if let Some(failure) = failure_of(&status) {
                        self.problem(&failure);
                    }
                    return;
                }
                let ended = match &status {
                    SessionStatusOutcome::Err(_) => true,
                    SessionStatusOutcome::Ok(payload) => {
                        payload.active.as_ref().map(|active| &active.launch_id) != Some(&launch_id)
                    }
                };
                if ended {
                    (self.launches.reload)();
                    Box::pin(self.open_now(game_id, title, Intent::Inspect)).await;
                    return;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            if self.current(operation) {
                self.problem(&RpcFailure {
                    code: "EndTimeout".into(),
                    message: "The session has not ended. Reload before trying again.".into(),
                });
            }
            return;
        }
        let Some(record) = self.lock().snapshot.clone() else {
            return;
        };
        let (kind, index) = id.split_once(':').unwrap_or((id.as_str(), ""));
        let route = index
            .parse::<usize>()
            .ok()
            .and_then(|index| record.routes.get(index));
        if kind == "launch" {
            if let Some(route) = route {
                Box::pin(self.launch(route.runner_id.clone(), operation)).await;
            }
            return;
        }
        let mut systems: Vec<&String> = record.system_runners.keys().collect();
        systems.sort();
        let system_id = if kind == "clear-system" {
            index
                .parse::<usize>()
                .ok()
                .and_then(|index| systems.get(index))
                .map(|id| (*id).clone())
        } else {
            route.map(|route| route.system_id.clone())
        };
        let layer = if kind == "game" || kind == "clear-game" {
            LaunchLayer::Game(record.game_id.clone())
        } else {
            match system_id {
                Some(system_id) => LaunchLayer::System(system_id),
                None => return,
            }
        };
        let expected_revision = if matches!(layer, LaunchLayer::Game(_)) {
            record.revisions.games.clone()
        } else {
            record.revisions.device.clone()
        };
        self.publish(SurfaceRunnerChoice::Busy(self.blank(
            "Saving… Cancel closes this panel; it cannot undo a save already sent.",
        )));
        // A remembered runner is the Runner value of the game's or system's
        // place for any runner (ADR 0028); clearing it removes the value.
        let value = if id.starts_with("clear-") {
            None
        } else {
            route.map(|route| serde_json::Value::String(route.runner_id.clone()))
        };
        let result = ask!(
            self.korrid,
            Route,
            LaunchSettingsSet,
            LaunchSetRequest {
                place: LaunchPlace {
                    layer,
                    scope: LaunchRunnerScope::Any
                },
                path: LaunchPath::Runner,
                value,
                clear: false,
                expected_revision,
            },
            LaunchSettingsWriteOutcome
        );
        if !self.current(operation) {
            return;
        }
        if let LaunchSettingsWriteOutcome::Err(failure) = result {
            self.problem(&failure);
            return;
        }
        Box::pin(self.open_now(game_id, title, Intent::Inspect)).await;
    }

    /// Warnings from a launch that did not go through this panel.
    pub fn show_warnings(&self, game_title: String, warnings: Vec<LaunchWarning>) {
        if warnings.is_empty() {
            return;
        }
        {
            let mut inner = self.lock();
            inner.generation += 1;
            inner.title = game_title;
        }
        let mut content = self.blank("Launched with settings warnings");
        content.warnings = warnings.iter().map(warning_text).collect();
        content.actions = vec![self.action("dismiss", "Done")];
        self.publish(SurfaceRunnerChoice::Warnings(content));
    }
}
