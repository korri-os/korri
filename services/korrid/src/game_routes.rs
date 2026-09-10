//! Installed Linux route choices. Android and peer launch contracts remain separate.
use crate::{
    config::{
        resolver,
        settings::{self, RuntimeChoiceRevisions, RuntimeChoiceScope},
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
    pub runtime_id: String,
    pub launcher_id: String,
    pub launcher_kind: String,
    pub system_id: String,
    pub runtime_build: String,
    pub launcher_build: String,
    pub program: String,
    pub warnings: Vec<LaunchWarning>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "runtimeId")]
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
    pub game_runtime: Option<String>,
    pub system_runtimes: std::collections::HashMap<String, String>,
    pub revisions: RuntimeChoiceRevisions,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameRuntimeSetRequest {
    pub scope: RuntimeChoiceScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_id: Option<String>,
    pub expected_revision: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectedGameLaunchRequest {
    pub game_id: String,
    pub runtime_id: String,
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
pub enum GameRuntimeSetOutcome {
    Ok(RuntimeChoiceRevisions),
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
    let (snapshot, revisions) =
        settings::runtime_choice_snapshot(root).map_err(settings_failure)?;
    let candidates = resolver::linux_route_candidates(root, &snapshot, registry, game_id)
        .map_err(|error| crate::route_diagnostic_failure(&error))?;
    let selection = resolver::resolve_linux_route(root, &snapshot, registry, game_id, None)
        .map(|route| GameRouteSelection::Selected(route.runtime.expect("Linux runtime").id))
        .unwrap_or(GameRouteSelection::Choose);
    let mut routes = Vec::new();
    let mut system_runtimes = std::collections::HashMap::new();
    for route in candidates {
        if let Some(id) = snapshot
            .systems
            .get(&route.system_id)
            .and_then(|system| system.runtime.as_ref())
        {
            system_runtimes.insert(route.system_id.clone(), id.0.clone());
        }
        let launch = linux_plugin::launch_route(root, &snapshot, registry, &route, None)
            .map_err(|error| unavailable(error.to_string()))?;
        let runtime = route.runtime.as_ref().expect("Linux runtime");
        let package = |id: &str| {
            registry
                .installed_package(id)
                .map(|package| package.package.display().to_string())
                .map_err(|error| unavailable(error.to_string()))
        };
        routes.push(GameRoute {
            runtime_id: runtime.id.clone(),
            launcher_id: route.launcher_id.clone(),
            launcher_kind: route.launcher_kind,
            runtime_build: package(&runtime.id)?,
            launcher_build: package(&route.launcher_id)?,
            system_id: route.system_id,
            program: route.linux_launcher.expect("Linux launcher").program,
            warnings: launch.warnings,
        });
    }
    Ok(GameRoutes {
        game_id: game_id.into(),
        routes,
        selection,
        revisions,
        system_runtimes,
        game_runtime: snapshot
            .games
            .get(game_id)
            .and_then(|game| game.runtime.as_ref())
            .map(|id| id.0.clone()),
    })
}

pub fn selected_launch(
    root: &Path,
    registry: &PluginRegistry,
    request: &SelectedGameLaunchRequest,
) -> Result<linux_plugin::LinuxLaunchSpec, RpcFailure> {
    let (snapshot, _) = settings::runtime_choice_snapshot(root).map_err(settings_failure)?;
    let route = resolver::resolve_linux_route(
        root,
        &snapshot,
        registry,
        &request.game_id,
        Some(&request.runtime_id),
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
