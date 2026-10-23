use super::{
    plugin_launch::{
        LaunchComposition, PluginDisplay, PluginLaunchInput, PluginLaunchOverrides,
        ResolvedLaunchModifier,
    },
    LaunchError,
};
use crate::{
    config::{resolver::ResolvedRoute, storage, ConfigSnapshot},
    plugin::PluginRegistry,
};
use std::path::Path;

#[cfg(test)]
#[path = "linux_plugin_tests.rs"]
mod tests;

#[derive(Clone, Debug)]
pub struct LinuxLaunchSpec {
    pub command: Vec<String>,
    pub warnings: Vec<super::typed_settings::LaunchWarning>,
}

enum RouteInspection {
    Unowned(Vec<super::typed_settings::LaunchWarning>),
    Owned(LinuxLaunchSpec),
}

/// Read owner-independent route facts without inventing an account or save root.
/// When an owner exists, also ask the runner about its person-specific runtime.
pub fn preflight_route(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    route: &ResolvedRoute,
    person: Option<&str>,
) -> Result<Vec<super::typed_settings::LaunchWarning>, LaunchError> {
    match inspect_route(root, snapshot, registry, route, None, None, person)? {
        RouteInspection::Unowned(warnings) => Ok(warnings),
        RouteInspection::Owned(launch) => Ok(launch.warnings),
    }
}

/// Build argv only. The runner callback returns the native launch plan; Korri
/// owns materialization and process/session lifecycle.
pub fn launch_route(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    route: &ResolvedRoute,
    launch: Option<crate::launch_settings::LaunchOverride>,
    display: Option<PluginDisplay>,
    person: &str,
) -> Result<LinuxLaunchSpec, LaunchError> {
    match inspect_route(
        root,
        snapshot,
        registry,
        route,
        launch,
        display,
        Some(person),
    )? {
        RouteInspection::Owned(launch) => Ok(launch),
        RouteInspection::Unowned(_) => unreachable!("launch_route supplied an owner"),
    }
}

fn inspect_route(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    route: &ResolvedRoute,
    launch: Option<crate::launch_settings::LaunchOverride>,
    display: Option<PluginDisplay>,
    person: Option<&str>,
) -> Result<RouteInspection, LaunchError> {
    let error = |message| LaunchError::RouteUnavailable(message);
    // The route must still name a native runner this registry admits.
    registry
        .native_runner(&route.runner_id)
        .map_err(|cause| error(cause.to_string()))?;
    let package = registry
        .installed_package(&route.runner_id)
        .map_err(|cause| error(cause.to_string()))?;
    // A missing required plugin refuses here, before the runner is asked
    // anything and before anything runs.
    let files = registry
        .launch_files(&route.runner_id)
        .map_err(|cause| error(cause.to_string()))?;
    let native = route
        .linux_runner
        .as_ref()
        .ok_or_else(|| error("native route has no program".into()))?;
    let (content_path, provider_ref) =
        match (
            &route.file_target,
            &route.provider_target,
            &route.artifact_target,
            &route.package_target,
            &route.cache_target,
        ) {
            (Some(target), None, None, None, None) => {
                let content =
                    storage::resolve_file_target(root, snapshot, target).map_err(|storage| {
                        if storage.is_missing_target() {
                            LaunchError::RomMissing(storage.to_string())
                        } else if storage.is_storage_access() {
                            LaunchError::StorageAccess(storage.to_string())
                        } else {
                            error(storage.to_string())
                        }
                    })?;
                (Some(content.path.display().to_string()), None)
            }
            (None, Some(target), None, None, None) => (None, Some(target.provider_ref.clone())),
            (None, None, Some(target), None, None) => (
                Some(
                    crate::acquisition::artifact_content_path(root, registry, target)
                        .map_err(|failure| error(failure.message))?
                        .display()
                        .to_string(),
                ),
                None,
            ),
            (None, None, None, Some(target), None) => (
                Some(
                    crate::config::resolver::package_content_path(registry, target)
                        .map_err(|cause| error(cause.to_string()))?
                        .display()
                        .to_string(),
                ),
                None,
            ),
            // ADR 0016: a provider download in korrid's cache.
            (None, None, None, None, Some(path)) => {
                if !path.is_file() {
                    return Err(LaunchError::RomMissing(
                        "the downloaded game is no longer in the cache".into(),
                    ));
                }
                (Some(path.display().to_string()), None)
            }
            _ => return Err(error(
                "native route must have one file, provider, artifact, package or download target"
                    .into(),
            )),
        };
    let resolved = crate::launch_settings::resolve_launch(snapshot, route, launch.as_ref(), None)
        .map_err(|e| error(e.to_string()))?;
    let folded = &resolved.runner;
    let build = package.package.display().to_string();
    // Ask the runner what it cannot apply before starting anything. The values
    // are still handed to launch.prepare exactly as they were authored.
    let source = crate::script::source::SourceSnapshot::package_plugin(
        &package.package,
        &package.entry,
        &package.sources,
    )
    .map_err(error)?;
    let warnings =
        super::typed_settings::validate(&source, &folded.settings, &route.runner_id, &build)
            .map_err(error)?;
    let mut composition = LaunchComposition {
        display,
        env: resolved.env.clone(),
        args: resolved.args.clone(),
        cwd: resolved.cwd.clone(),
        ..Default::default()
    };
    for (id, record) in resolved.modifiers.iter() {
        if record.enabled != Some(true) {
            continue;
        }
        registry
            .enabled_modifier(id)
            .map_err(|cause| error(cause.to_string()))?;
        let modifier_package = registry
            .installed_package(id)
            .map_err(|cause| error(cause.to_string()))?;
        composition.modifiers.push(ResolvedLaunchModifier {
            modifier_id: id.clone(),
            manifest: modifier_package.package.join("manifest.json"),
            entry: modifier_package.entry.clone(),
            settings: record.settings.clone(),
            config: record.config.clone().unwrap_or_default(),
        });
    }
    let Some(person) = person else {
        return Ok(RouteInspection::Unowned(warnings));
    };
    let account_root = crate::saves::account_root(root, person).map_err(error)?;
    let settings = folded.settings.clone();
    let input = PluginLaunchInput {
        runner_id: route.runner_id.clone(),
        family_id: route.family_id.clone(),
        program: native.program.clone(),
        core_path: route.core_path.clone(),
        content_path,
        provider_ref,
        account_root: account_root.display().to_string(),
        save_root: account_root
            .join("saves")
            .join(crate::host::play_log::encode_uri_component(
                &route.playable_id,
            ))
            .display()
            .to_string(),
        cache_root: root.join("cache").display().to_string(),
        files: files
            .into_iter()
            .map(|(key, path)| (key, path.display().to_string()))
            .collect(),
        artifacts: crate::acquisition::launch_artifacts(root, registry, &route.runner_id)
            .map_err(|failure| error(failure.message))?,
        overrides: Some(PluginLaunchOverrides {
            settings,
            config: folded.config.clone(),
        }),
    };
    // Ask the runner whether it can start this target. A runner whose runtime is
    // its own package declares no runtime and proceeds. One that needs
    // something else refuses here, before korrid spawns anything.
    if let Some(refusal) = super::runtime::resolve(&source, &input)
        .map_err(error)?
        .refusal()
    {
        return Err(error(refusal));
    }
    Ok(RouteInspection::Owned(LinuxLaunchSpec {
        warnings,
        command: vec![
            std::env::current_exe()
                .map_err(|error_value| error(error_value.to_string()))?
                .display()
                .to_string(),
            "plugin-prepare".into(),
            package.package.join(&package.entry).display().to_string(),
            serde_json::to_string(&input).map_err(|error_value| error(error_value.to_string()))?,
            serde_json::to_string(&composition)
                .map_err(|error_value| error(error_value.to_string()))?,
            route.release_id.clone(),
        ],
    }))
}

/// Evaluate the same effect-free preparation used by the game handoff.
/// The command is the LinuxLaunchSpec command built for this exact launch.
pub fn resolved_game_environment(
    command: &[String],
    resolved: &crate::launch_settings::ResolvedLaunch,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    // Preparation includes layer environment and modifier composition. It is the
    // exact plan that plugin-launch executes, never a second hook-only merge.
    let _ = resolved;
    let plan = super::plugin_launch::prepared_plan(command)?;
    let mut env: std::collections::BTreeMap<String, String> = std::env::vars().collect();
    for key in &plan.env_unset {
        env.remove(key);
    }
    env.extend(plan.env);
    Ok(env)
}
