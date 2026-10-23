//! Hook execution and the session's restart-safe after-step journal.
use super::systemd_unit::LaunchUnitBackend;
use crate::{
    config::{HookAfterStep, HookBeforeFailurePolicy, HookBeforeStep},
    launcher::typed_settings::LaunchWarning,
    RpcFailure,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone, Debug, Default)]
pub(super) struct LaunchHooks {
    pub resolved: crate::launch_settings::ResolvedHooks,
    pub context: Context,
    pub game_environment: BTreeMap<String, String>,
}

pub(super) const VARIABLES: [&str; 10] = [
    "KORRI_HOOK_PHASE",
    "KORRI_LAUNCH_ID",
    "KORRI_GAME_ID",
    "KORRI_RELEASE_ID",
    "KORRI_SYSTEM_ID",
    "KORRI_RUNNER_ID",
    "KORRI_PROFILE_ID",
    "KORRI_SESSION_RESULT",
    "KORRI_EXIT_STATUS",
    "KORRI_LIVE_RUNNER_ID",
];

#[derive(Clone, Debug, Default)]
pub(super) struct Context {
    pub release_id: Option<String>,
    pub system_id: Option<String>,
    pub runner_id: Option<String>,
    pub profile_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum SessionResult {
    Exited,
    Failed,
    Stopped,
}

impl SessionResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::Exited => "exited",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

/// Resolved steps and environment belong to the exact launch, not to config
/// reloaded after a daemon restart. The boot identity prevents reboot replay.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AfterHooks {
    pub boot_id: String,
    pub command: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub steps: Vec<HookAfterStep>,
    pub next_step: usize,
    pub result: Option<SessionResult>,
    pub exit_status: Option<i32>,
}

pub(super) fn boot_id() -> Result<String, String> {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map(|id| id.trim().to_owned())
        .map_err(|error| format!("cannot read hook boot identity: {error}"))
}

pub(super) fn environment(
    game_environment: &BTreeMap<String, String>,
    launch_id: &str,
    game_id: &str,
    context: &Context,
    result: Option<SessionResult>,
    exit_status: Option<i32>,
) -> BTreeMap<String, String> {
    let mut environment = game_environment.clone();
    for name in VARIABLES {
        environment.remove(name);
    }
    for (name, value) in [
        (
            "KORRI_HOOK_PHASE",
            Some(if result.is_some() { "after" } else { "before" }),
        ),
        ("KORRI_LAUNCH_ID", Some(launch_id)),
        ("KORRI_GAME_ID", Some(game_id)),
        ("KORRI_RELEASE_ID", context.release_id.as_deref()),
        ("KORRI_SYSTEM_ID", context.system_id.as_deref()),
        ("KORRI_RUNNER_ID", context.runner_id.as_deref()),
        ("KORRI_PROFILE_ID", context.profile_id.as_deref()),
        ("KORRI_SESSION_RESULT", result.map(SessionResult::as_str)),
    ] {
        if let Some(value) = value {
            environment.insert(name.into(), value.into());
        }
    }
    if result == Some(SessionResult::Exited) {
        if let Some(status) = exit_status {
            environment.insert("KORRI_EXIT_STATUS".into(), status.to_string());
        }
    }
    environment
}

pub(super) fn before(
    backend: &dyn LaunchUnitBackend,
    launch_id: &str,
    command: &[String],
    environment: &BTreeMap<String, String>,
    steps: &[HookBeforeStep],
    warnings: &mut Vec<LaunchWarning>,
) -> Result<(), RpcFailure> {
    for (index, step) in steps.iter().enumerate() {
        if let Err(error) = backend.run_hook(
            launch_id,
            index,
            command,
            &step.run.0,
            Duration::from_secs(step.timeout.as_ref().map_or(30, |timeout| timeout.0)),
            environment,
        ) {
            let reason = format!(
                "HookFailed {{ phase: before, step: {index}, reason: {} }}",
                error.message
            );
            if step.on_failure == Some(HookBeforeFailurePolicy::Warn) {
                warnings.push(LaunchWarning {
                    setting: format!("hooks.before.{index}"),
                    runner_id: environment
                        .get("KORRI_RUNNER_ID")
                        .cloned()
                        .unwrap_or_default(),
                    build: String::new(),
                    message: reason,
                });
            } else {
                return Err(RpcFailure {
                    code: "HookFailed".into(),
                    message: reason,
                });
            }
        }
    }
    Ok(())
}

impl AfterHooks {
    pub(super) fn run_next(
        &self,
        backend: &dyn LaunchUnitBackend,
        launch_id: &str,
        step: &HookAfterStep,
    ) {
        let mut environment = self.environment.clone();
        environment.insert("KORRI_HOOK_PHASE".into(), "after".into());
        let result = self.result.unwrap_or(SessionResult::Exited);
        environment.insert("KORRI_SESSION_RESULT".into(), result.as_str().into());
        environment.remove("KORRI_EXIT_STATUS");
        if result == SessionResult::Exited {
            if let Some(status) = self.exit_status {
                environment.insert("KORRI_EXIT_STATUS".into(), status.to_string());
            }
        }
        if let Err(error) = backend.run_hook(
            launch_id,
            self.next_step,
            &self.command,
            &step.run.0,
            Duration::from_secs(step.timeout.as_ref().map_or(30, |timeout| timeout.0)),
            &environment,
        ) {
            eprintln!(
                "korrid: HookFailed {{ phase: after, step: {}, reason: {} }}",
                self.next_step, error.message
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_have_exact_values_and_absent_values_are_unset_not_empty() {
        let game = BTreeMap::from_iter(
            VARIABLES
                .into_iter()
                .map(|key| (key.into(), "forged".into())),
        );
        let context = Context {
            release_id: Some("release".into()),
            system_id: Some("system".into()),
            runner_id: Some("@test:runner/main".into()),
            profile_id: Some("profile".into()),
        };
        let before = environment(&game, "launch", "game", &context, None, None);
        assert_eq!(
            before,
            BTreeMap::from_iter(
                [
                    ("KORRI_HOOK_PHASE", "before"),
                    ("KORRI_LAUNCH_ID", "launch"),
                    ("KORRI_GAME_ID", "game"),
                    ("KORRI_RELEASE_ID", "release"),
                    ("KORRI_SYSTEM_ID", "system"),
                    ("KORRI_RUNNER_ID", "@test:runner/main"),
                    ("KORRI_PROFILE_ID", "profile"),
                ]
                .map(|(key, value)| (key.into(), value.into()))
            )
        );
        let exited = environment(
            &game,
            "launch",
            "game",
            &Context::default(),
            Some(SessionResult::Exited),
            Some(7),
        );
        assert_eq!(
            exited.get("KORRI_EXIT_STATUS").map(String::as_str),
            Some("7")
        );
        for absent in [
            "KORRI_RELEASE_ID",
            "KORRI_SYSTEM_ID",
            "KORRI_RUNNER_ID",
            "KORRI_PROFILE_ID",
            "KORRI_LIVE_RUNNER_ID",
        ] {
            assert!(!exited.contains_key(absent));
        }
        for result in [SessionResult::Failed, SessionResult::Stopped] {
            let after = environment(
                &game,
                "launch",
                "game",
                &Context::default(),
                Some(result),
                Some(7),
            );
            assert_eq!(
                after.get("KORRI_SESSION_RESULT").map(String::as_str),
                Some(result.as_str())
            );
            assert!(!after.contains_key("KORRI_EXIT_STATUS"));
        }
    }
}
