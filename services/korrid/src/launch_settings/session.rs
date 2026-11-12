use super::*;
use crate::{
    config::{resolver::ResolvedRoute, ConfigSnapshot},
    launcher::typed_settings::LaunchWarning,
};
use std::collections::HashMap;

/// The device's clock values while one launch names its own governor
/// (ADR 0066), and the values to put back when that launch ends. The governor
/// is one device value, so the helper's device call carries both.
#[derive(Clone, Debug)]
pub struct SessionGovernor {
    pub during: LaunchDeviceDefaults,
    pub after: LaunchDeviceDefaults,
}
impl SessionGovernor {
    /// None when the launch names no governor, or names the device's own.
    pub fn for_launch(
        root: &std::path::Path,
        client: &helper::Client,
        governor: Option<&str>,
    ) -> Result<Option<Self>, String> {
        let Some(governor) = governor else {
            return Ok(None);
        };
        let after = super::resolution::device_defaults(root, client)?;
        if after.governor == governor {
            return Ok(None);
        }
        Ok(Some(Self {
            during: LaunchDeviceDefaults {
                governor: governor.into(),
                ..after.clone()
            },
            after,
        }))
    }
}

/// Device values to put back, by game unit. A korrid restart applies the
/// device's own values again, so a lost entry cannot keep a launch's governor.
fn governors_to_restore() -> &'static std::sync::Mutex<HashMap<String, LaunchDeviceDefaults>> {
    static RESTORE: std::sync::OnceLock<std::sync::Mutex<HashMap<String, LaunchDeviceDefaults>>> =
        std::sync::OnceLock::new();
    RESTORE.get_or_init(Default::default)
}

#[derive(Clone, Debug)]
pub struct SessionLimits {
    pub cpu: LaunchPercentRange,
    pub gpu: LaunchPercentRange,
    pub governor: Option<SessionGovernor>,
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
        let resolved = super::resolve_launch(snapshot, route, None, facts)?.limits;
        let gpu = &resolved.gpu;
        Ok(Self {
            cpu: LaunchPercentRange {
                min: resolved.cpu.min.value,
                max: resolved.cpu.max.value,
            },
            gpu: LaunchPercentRange {
                min: gpu.min.value,
                max: gpu.max.value,
            },
            governor: None,
            runner_id: route.runner_id.clone(),
            build,
        })
    }
    pub fn apply(&self, client: &helper::Client, launch_id: &str) -> Option<LaunchWarning> {
        let warning = |message: String| LaunchWarning {
            setting: "launch-settings".into(),
            runner_id: self.runner_id.clone(),
            build: self.build.clone(),
            message,
        };
        let governor = self.governor.as_ref().and_then(|governor| {
            match client.apply_device(&governor.during) {
                Ok(()) => {
                    governors_to_restore()
                        .lock()
                        .expect("governor restore poisoned")
                        .insert(unit(launch_id), governor.after.clone());
                    None
                }
                Err(message) => Some(warning(format!(
                    "The CPU governor for this launch was not applied: {message}"
                ))),
            }
        });
        let limits = client
            .apply_session(&unit(launch_id), &self.cpu, &self.gpu)
            .err()
            .map(|message| warning(format!("Launch limits were not applied: {message}")));
        governor.or(limits)
    }
}
pub fn unit(launch_id: &str) -> String {
    format!("korri-game-{launch_id}.service")
}
pub fn end(client: &helper::Client, launch_id: &str) {
    if let Err(error) = client.end_session(&unit(launch_id)) {
        eprintln!("korrid: session limits restore unavailable: {error}");
    }
    let restore = governors_to_restore()
        .lock()
        .expect("governor restore poisoned")
        .remove(&unit(launch_id));
    if let Some(device) = restore {
        if let Err(error) = client.apply_device(&device) {
            eprintln!("korrid: device governor restore unavailable: {error}");
        }
    }
}
