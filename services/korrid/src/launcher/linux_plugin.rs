use super::{
    plugin_launch::{PluginLaunchInput, PluginLaunchOverrides},
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

/// Build argv only. No callback output or runtime file is written by korrid.
pub fn launch_route(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    route: &ResolvedRoute,
    overrides: Option<PluginLaunchOverrides>,
) -> Result<LinuxLaunchSpec, LaunchError> {
    let error = |message| LaunchError::RouteUnavailable(message);
    let (instance, kind) = registry
        .native_launcher(&route.launcher_id)
        .map_err(|e| error(e.to_string()))?;
    let kind_package = registry
        .installed_package(&kind.id)
        .map_err(|e| error(e.to_string()))?;
    let runtime = route
        .runtime
        .as_ref()
        .ok_or_else(|| error("native route has no runtime".into()))?;
    let launcher = route
        .linux_launcher
        .as_ref()
        .ok_or_else(|| error("native route has no program".into()))?;
    let target = route
        .file_target
        .as_ref()
        .ok_or_else(|| error("native route has no file".into()))?;
    let content = storage::resolve_file_target(root, snapshot, target).map_err(|e| {
        if e.is_missing_target() {
            LaunchError::RomMissing(e.to_string())
        } else if e.is_storage_access() {
            LaunchError::StorageAccess(e.to_string())
        } else {
            error(e.to_string())
        }
    })?;
    let overrides = overrides.map(|value| crate::config::cascade::LauncherConfig {
        settings: value.settings,
        config: value.config,
    });
    let folded = crate::config::cascade::resolve(snapshot, route, overrides.as_ref());
    let package = registry
        .installed_package(&route.launcher_id)
        .map_err(|e| error(e.to_string()))?;
    let build = package.package.display().to_string();
    let evidence = super::typed_settings::PackagedSettings::read(
        package,
        instance
            .program
            .as_deref()
            .expect("native launcher has a program"),
    )
    .map_err(error)?;
    let (settings, warnings) = match evidence {
        Some(evidence) => evidence.validate(
            folded.settings,
            &route.launcher_id,
            &build,
            &launcher.program,
        ),
        None => super::typed_settings::validate(folded.settings, &route.launcher_id, &build, None),
    };
    let input = PluginLaunchInput {
        launcher_id: route.launcher_id.clone(),
        launcher_kind: kind.id.clone(),
        runtime_id: runtime.id.clone(),
        program: launcher.program.clone(),
        runtime_path: runtime.path.clone(),
        content_path: content.path.display().to_string(),
        account_root: root.join("users/default").display().to_string(),
        files: kind_package
            .files
            .iter()
            .map(|(key, path)| (key.clone(), path.display().to_string()))
            .collect(),
        overrides: Some(PluginLaunchOverrides {
            settings,
            config: folded.config,
        }),
    };
    Ok(LinuxLaunchSpec {
        warnings,
        command: vec![
            std::env::current_exe()
                .map_err(|e| error(e.to_string()))?
                .display()
                .to_string(),
            "plugin-launch".into(),
            kind_package.package.join("plugin.ts").display().to_string(),
            serde_json::to_string(&input).map_err(|e| error(e.to_string()))?,
        ],
    })
}
