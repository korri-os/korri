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

/// Build argv only. The runner callback returns the native launch plan; Korri
/// owns materialization and process/session lifecycle.
pub fn launch_route(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    route: &ResolvedRoute,
    overrides: Option<PluginLaunchOverrides>,
    display: Option<PluginDisplay>,
) -> Result<LinuxLaunchSpec, LaunchError> {
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
    let (content_path, provider_ref) = match (
        &route.file_target,
        &route.provider_target,
        &route.artifact_target,
    ) {
        (Some(target), None, None) => {
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
        (None, Some(target), None) => (None, Some(target.provider_ref.clone())),
        (None, None, Some(target)) => (
            Some(
                crate::acquisition::artifact_content_path(root, registry, target)
                    .map_err(|failure| error(failure.message))?
                    .display()
                    .to_string(),
            ),
            None,
        ),
        _ => {
            return Err(error(
                "native route must have one file, provider or artifact target".into(),
            ))
        }
    };
    let overrides = overrides.map(|value| crate::config::cascade::RunnerConfig {
        settings: value.settings,
        config: value.config,
    });
    let folded = crate::config::cascade::resolve(snapshot, route, overrides.as_ref());
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
    let settings = folded.settings;
    let input = PluginLaunchInput {
        runner_id: route.runner_id.clone(),
        family_id: route.family_id.clone(),
        program: native.program.clone(),
        core_path: route.core_path.clone(),
        content_path,
        provider_ref,
        account_root: root.join("users/default").display().to_string(),
        cache_root: root.join("cache").display().to_string(),
        files: files
            .into_iter()
            .map(|(key, path)| (key, path.display().to_string()))
            .collect(),
        artifacts: crate::acquisition::launch_artifacts(root, registry, &route.runner_id)
            .map_err(|failure| error(failure.message))?,
        overrides: Some(PluginLaunchOverrides {
            settings,
            config: folded.config,
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
    let mut composition = LaunchComposition {
        display,
        ..Default::default()
    };
    for (id, record) in crate::config::modifiers::resolve(snapshot, route).iter() {
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
    Ok(LinuxLaunchSpec {
        warnings,
        command: vec![
            std::env::current_exe()
                .map_err(|error_value| error(error_value.to_string()))?
                .display()
                .to_string(),
            "plugin-launch".into(),
            package.package.join(&package.entry).display().to_string(),
            serde_json::to_string(&input).map_err(|error_value| error(error_value.to_string()))?,
            serde_json::to_string(&composition)
                .map_err(|error_value| error(error_value.to_string()))?,
        ],
    })
}
