use std::{collections::BTreeSet, path::Path};

use serde::{Deserialize, Serialize};

use crate::{plugin::PluginRegistry, GameIdentity};

pub use super::linux_routes::{linux_route_candidates, resolve_linux_route, stored_runner};
use super::ConfigSnapshot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteCatalog {
    pub routes: Vec<ResolvedRoute>,
    pub diagnostics: Vec<RouteDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedLinuxRunner {
    pub program: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedFileTarget {
    pub storage_id: String,
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRoute {
    pub playable_id: String,
    pub title: Option<String>,
    pub release_id: String,
    pub identity: Option<GameIdentity>,
    pub provider_id: String,
    pub system_id: String,
    pub system_title: Option<String>,
    pub runner_id: String,
    pub family_id: Option<String>,
    pub integration_token: String,
    pub flattened_target: String,
    pub linux_runner: Option<ResolvedLinuxRunner>,
    pub core_path: Option<String>,
    pub file_target: Option<ResolvedFileTarget>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RouteDiagnosticCode {
    LocalRomMissing,
    LocalRouteUnavailable,
    LocalRouteCollision,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RouteDiagnostic {
    pub code: RouteDiagnosticCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playable_id: Option<String>,
}

pub type RouteUnavailable = RouteDiagnostic;

/// Every route this device can run, one entry per runner that admits the
/// content. A game with two runners yields two routes; choosing between them
/// belongs to the caller, not here.
pub fn resolve_launchable_routes<'a>(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    static_playable_ids: impl IntoIterator<Item = &'a str>,
) -> RouteCatalog {
    let static_ids: BTreeSet<_> = static_playable_ids.into_iter().collect();
    let mut catalog = RouteCatalog {
        routes: Vec::new(),
        diagnostics: Vec::new(),
    };
    for id in snapshot.games.keys() {
        if static_ids.contains(id.as_str()) {
            catalog.diagnostics.push(static_playable_collision(id));
            continue;
        }
        match linux_route_candidates(root, snapshot, registry, id) {
            Ok(routes) => catalog.routes.extend(routes),
            Err(error) => catalog.diagnostics.push(error),
        }
    }
    catalog
}

/// The single route this device would use for one piece of content, honouring
/// the person's runner choice. Errors when the choice is still open.
pub fn resolve_route<'a>(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    static_playable_ids: impl IntoIterator<Item = &'a str>,
    playable_id: &str,
) -> Result<ResolvedRoute, RouteUnavailable> {
    if static_playable_ids.into_iter().any(|id| id == playable_id) {
        return Err(static_playable_collision(playable_id));
    }
    resolve_linux_route(root, snapshot, registry, playable_id, None)
}

fn static_playable_collision(playable_id: &str) -> RouteUnavailable {
    collision(
        Some(playable_id),
        format!("dynamic local route {playable_id} collides with an existing static local game"),
    )
}

fn collision(playable_id: Option<&str>, message: String) -> RouteUnavailable {
    RouteUnavailable {
        code: RouteDiagnosticCode::LocalRouteCollision,
        message,
        playable_id: playable_id.map(str::to_owned),
    }
}
