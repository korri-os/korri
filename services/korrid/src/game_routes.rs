//! Installed Linux runner choices. Android and peer launch contracts remain separate.
use crate::{
    config::{
        resolver,
        settings::{self, RunnerChoiceRevisions, RunnerChoiceScope},
    },
    launcher::{linux_plugin, plugin_launch::PluginLaunchOverrides, typed_settings::LaunchWarning},
    plugin::PluginRegistry,
    RpcFailure, SessionPrepared,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use typeshare::typeshare;

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameRoutesRequest {
    pub game_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRoute {
    pub runner_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family_id: Option<String>,
    pub system_id: String,
    pub runner_build: String,
    pub program: String,
    pub warnings: Vec<LaunchWarning>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "runnerId")]
pub enum GameRouteSelection {
    Selected(String),
    Choose,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRoutes {
    pub game_id: String,
    pub routes: Vec<GameRoute>,
    pub selection: GameRouteSelection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_runner: Option<String>,
    pub system_runners: std::collections::HashMap<String, String>,
    pub revisions: RunnerChoiceRevisions,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameRunnerSetRequest {
    pub scope: RunnerChoiceScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_id: Option<String>,
    pub expected_revision: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectedGameLaunchRequest {
    pub game_id: String,
    pub runner_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<PluginLaunchOverrides>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SelectedGameLaunch {
    pub session: SessionPrepared,
    pub warnings: Vec<LaunchWarning>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum GameRoutesOutcome {
    Ok(GameRoutes),
    Err(RpcFailure),
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum GameRunnerSetOutcome {
    Ok(RunnerChoiceRevisions),
    Err(RpcFailure),
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SelectedGameLaunchOutcome {
    Ok(SelectedGameLaunch),
    Err(RpcFailure),
}

pub fn list(
    root: &Path,
    registry: &PluginRegistry,
    game_id: &str,
) -> Result<GameRoutes, RpcFailure> {
    let (snapshot, revisions) = settings::runner_choice_snapshot(root).map_err(settings_failure)?;
    let candidates = resolver::linux_route_candidates(root, &snapshot, registry, game_id)
        .map_err(|error| crate::route_diagnostic_failure(&error))?;
    let selection = resolver::resolve_linux_route(root, &snapshot, registry, game_id, None)
        .map(|route| GameRouteSelection::Selected(route.runner_id))
        .unwrap_or(GameRouteSelection::Choose);
    let mut routes = Vec::new();
    let mut system_runners = std::collections::HashMap::new();
    for route in candidates {
        if let Some(id) = snapshot
            .systems
            .get(&route.system_id)
            .and_then(|system| system.runner.as_ref())
        {
            system_runners.insert(route.system_id.clone(), id.0.clone());
        }
        let launch = linux_plugin::launch_route(root, &snapshot, registry, &route, None)
            .map_err(|error| unavailable(error.to_string()))?;
        let package = registry
            .installed_package(&route.runner_id)
            .map_err(|error| unavailable(error.to_string()))?;
        routes.push(GameRoute {
            runner_id: route.runner_id,
            family_id: route.family_id,
            runner_build: package.package.display().to_string(),
            system_id: route.system_id,
            program: route.linux_runner.expect("Linux runner").program,
            warnings: launch.warnings,
        });
    }
    Ok(GameRoutes {
        game_id: game_id.into(),
        routes,
        selection,
        revisions,
        system_runners,
        game_runner: snapshot
            .games
            .get(game_id)
            .and_then(|game| game.runner.as_ref())
            .map(|id| id.0.clone()),
    })
}

pub fn selected_launch(
    root: &Path,
    registry: &PluginRegistry,
    request: &SelectedGameLaunchRequest,
) -> Result<linux_plugin::LinuxLaunchSpec, RpcFailure> {
    let (snapshot, _) = settings::runner_choice_snapshot(root).map_err(settings_failure)?;
    let route = resolver::resolve_linux_route(
        root,
        &snapshot,
        registry,
        &request.game_id,
        Some(&request.runner_id),
    )
    .map_err(|error| crate::route_diagnostic_failure(&error))?;
    linux_plugin::launch_route(root, &snapshot, registry, &route, request.overrides.clone())
        .map_err(|error| unavailable(error.to_string()))
}

pub(crate) fn settings_failure(error: settings::SettingsError) -> RpcFailure {
    RpcFailure {
        code: match &error {
            settings::SettingsError::Conflict => "SettingsConflict",
            settings::SettingsError::Invalid(_) => "SettingsInvalid",
            settings::SettingsError::Storage(_) => "SettingsStorageUnavailable",
            settings::SettingsError::Candidate(_) => "LocalConfigReloadFailed",
        }
        .into(),
        message: error.to_string(),
    }
}

pub(crate) fn unavailable(message: String) -> RpcFailure {
    RpcFailure {
        code: "LocalRouteUnavailable".into(),
        message,
    }
}
