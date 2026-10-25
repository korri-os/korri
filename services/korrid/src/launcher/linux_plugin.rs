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
    let mut resolved =
        crate::launch_settings::resolve_launch(snapshot, route, launch.as_ref(), None)
            .map_err(|e| error(e.to_string()))?;
    let folded = &mut resolved.runner;
    let build = package.package.display().to_string();
    // Ask before starting anything. Only error paths are removed; the authored
    // file remains unchanged and every diagnostic keeps its setting path.
    let source = crate::script::source::SourceSnapshot::package_plugin(
        &package.package,
        &package.entry,
        &package.sources,
    )
    .map_err(error)?;
    let mut warnings =
        super::typed_settings::validate(&source, &mut folded.settings, &route.runner_id, &build)
            .map_err(error)?
            .warnings;
    warnings.extend(resolved.notices.iter().filter_map(|notice| match notice {
        crate::launch_settings::LaunchNotice::SettingSkipped { path, message, .. } => {
            let (setting, id) = match path {
                crate::launch_settings::LaunchPath::Setting { key } => (key, &route.runner_id),
                crate::launch_settings::LaunchPath::ModifierSetting { id, key }
                | crate::launch_settings::LaunchPath::ServiceSetting { id, key } => (key, id),
                _ => return None,
            };
            Some(super::typed_settings::LaunchWarning {
                setting: setting.clone(),
                runner_id: id.clone(),
                build: registry
                    .installed_package(id)
                    .map(|p| p.package.display().to_string())
                    .unwrap_or_default(),
                message: message.clone(),
            })
        }
        _ => None,
    }));
    let mut composition = LaunchComposition {
        display,
        env: resolved.env.clone(),
        args: resolved.args.clone(),
        cwd: resolved.cwd.clone(),
        ..Default::default()
    };
    for (id, record) in resolved.modifiers.iter_mut() {
        if record.enabled != Some(true) {
            continue;
        }
        registry
            .enabled_modifier(id)
            .map_err(|cause| error(cause.to_string()))?;
        let modifier_package = registry
            .installed_package(id)
            .map_err(|cause| error(cause.to_string()))?;
        let modifier_source = crate::script::source::SourceSnapshot::package_plugin(
            &modifier_package.package,
            &modifier_package.entry,
            &modifier_package.sources,
        )
        .map_err(error)?;
        warnings.extend(
            super::typed_settings::validate(
                &modifier_source,
                &mut record.settings,
                id,
                &modifier_package.package.display().to_string(),
            )
            .map_err(error)?
            .warnings,
        );
        composition.modifiers.push(ResolvedLaunchModifier {
            modifier_id: id.clone(),
            manifest: modifier_package.package.join("manifest.json"),
            entry: modifier_package.entry.clone(),
            settings: record.settings.clone(),
            config: record.config.clone().unwrap_or_default(),
        });
    }
    let service_id = crate::launch_settings::resolution::SUNSHINE;
    if let Ok(package) = registry.installed_package(service_id) {
        let service_source = crate::script::source::SourceSnapshot::package_plugin(
            &package.package,
            &package.entry,
            &package.sources,
        )
        .map_err(error)?;
        let settings = resolved
            .services
            .get(service_id)
            .map(|s| s.settings.clone())
            .unwrap_or_default();
        let (args, service_warnings) = super::typed_settings::render(
            &service_source,
            service_id,
            &settings,
            None,
            &package.package.display().to_string(),
        )
        .map_err(error)?;
        warnings.extend(service_warnings);
        composition.sunshine_args = Some(args);
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
pub fn resolved_plan_environment_changes(command: &[String]) -> Result<PlanEnvironment, String> {
    // Preparation includes layer environment and modifier composition. It is the
    // exact plan that plugin-launch executes, never a second hook-only merge.
    let plan = super::plugin_launch::prepared_plan(command)?;
    // Game units inherit their explicit HostConfig baseline, not korrid's
    // process environment. The host merges these changes over that baseline.
    Ok(PlanEnvironment {
        values: plan.env.into_iter().collect(),
        unset: plan.env_unset,
    })
}

#[derive(Clone, Debug, Default)]
pub struct PlanEnvironment {
    pub values: std::collections::BTreeMap<String, String>,
    pub unset: Vec<String>,
}
