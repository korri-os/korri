use super::*;
use crate::{
    config::{resolver::ResolvedRoute, ConfigSnapshot},
    launcher::typed_settings::LaunchWarning,
};

#[derive(Clone, Debug)]
pub struct SessionLimits {
    pub cpu: LaunchPercentRange,
    pub gpu: LaunchPercentRange,
    pub runner_id: String,
    pub build: String,
}
impl SessionLimits {
    pub fn for_route(
        snapshot: &ConfigSnapshot,
        route: &ResolvedRoute,
        build: String,
        facts: Option<&LaunchDeviceFacts>,
    ) -> Result<Self, crate::config::settings::SettingsError> {
        let layers = super::resolution::layers(snapshot, Some(&route.playable_id), Some(route))?;
        // ADR 0019: device CPU bounds are cpufreq, not game uclamp. GPU has
        // one global devfreq range, so the session must inherit device GPU bounds.
        let resolved = resolve(&layers[1..]);
        let gpu = super::resolution::resolve_with_defaults(&layers, facts).gpu;
        Ok(Self {
            cpu: LaunchPercentRange {
                min: resolved.cpu.min.value,
                max: resolved.cpu.max.value,
            },
            gpu: LaunchPercentRange {
                min: gpu.min.value,
                max: gpu.max.value,
            },
            runner_id: route.runner_id.clone(),
            build,
        })
    }
    pub fn apply(&self, client: &helper::Client, launch_id: &str) -> Option<LaunchWarning> {
        client
            .apply_session(&unit(launch_id), &self.cpu, &self.gpu)
            .err()
            .map(|message| LaunchWarning {
                setting: "launch-settings".into(),
                runner_id: self.runner_id.clone(),
                build: self.build.clone(),
                message: format!("Launch limits were not applied: {message}"),
            })
    }
}
pub fn unit(launch_id: &str) -> String {
    format!("korri-game-{launch_id}.service")
}
pub fn end(client: &helper::Client, launch_id: &str) {
    if let Err(error) = client.end_session(&unit(launch_id)) {
        eprintln!("korrid: session limits restore unavailable: {error}");
    }
}
