//! ADR 0054: games that run on the Android device itself.
//!
//! On Linux, korrid's `host` runs a live session as systemd units. The
//! Android app has no systemd and cannot stop another app (Android 14+), so
//! this module keeps the live session itself and asks the Android edge to
//! start the game's app. It owns exactly one live session.
//!
//! - **Launch** evaluates the runner's `launch.prepare` (an `android` plan),
//!   checks it, and asks the edge to start the package.
//! - **Leave** brings Korri forward; the game keeps running.
//! - **Resume** brings the game's app forward again.
//! - **End** closes the live session and brings Korri forward. The app may
//!   keep running (CONTEXT.md, "End").
//! - The edge reports when the game's app leaves the foreground or closes
//!   (usage events); a closed app ends the session.

use crate::{
    config::{resolver::ResolvedRoute, storage, ConfigSnapshot},
    launcher::plugin_launch::{self, AndroidLaunchOutput, PluginLaunchInput},
    plugin::PluginRegistry,
    RpcFailure,
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

/// The Android edge. The JNI adapter implements it; tests use a fake.
pub trait AppLauncher: Send + Sync {
    fn is_installed(&self, package: &str) -> bool;
    /// Start the plan. `content_path` is the game file when `data` is
    /// `content`; the edge makes a read-only grant for exactly that file.
    fn start(&self, plan: &AndroidLaunchOutput, content_path: Option<&str>) -> Result<(), String>;
    /// Bring the package's task forward again.
    fn resume(&self, package: &str) -> Result<(), String>;
    /// Bring Korri's own screen forward.
    fn show_korri(&self) -> Result<(), String>;
    /// End: if the app carries the Korri save bridge, ask it to quit and copy
    /// its save folders back into Korri's (ADR 0054, case 2). Otherwise only
    /// bring Korri forward; the app may keep running.
    fn end_session(&self, session: &LocalSession) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Playing,
    Left,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalSession {
    pub launch_id: String,
    pub game_id: String,
    pub title: Option<String>,
    pub package: String,
    pub phase: Phase,
    /// The plan's save folders, keyed as the runner declared them. The save
    /// bridge copies them in before start and out after End.
    pub save_locations: Vec<crate::saves::types::SaveLocation>,
}

pub struct AndroidLocalSessions {
    launcher: Arc<dyn AppLauncher>,
    active: Mutex<Option<LocalSession>>,
    /// A launch the shell reserved (SessionReserve) and has not started yet.
    pending: Mutex<Option<(String, String)>>,
}

fn failure(code: &str, message: impl Into<String>) -> RpcFailure {
    RpcFailure {
        code: code.into(),
        message: message.into(),
    }
}

impl AndroidLocalSessions {
    pub fn new(launcher: Arc<dyn AppLauncher>) -> Self {
        Self {
            launcher,
            active: Mutex::new(None),
            pending: Mutex::new(None),
        }
    }

    pub fn active(&self) -> Option<LocalSession> {
        self.active.lock().expect("android session lock").clone()
    }

    /// The shell's two-step launch: reserve an exact launch ID first.
    pub fn reserve(&self, game_id: &str) -> String {
        let launch_id = new_launch_id();
        *self.pending.lock().expect("android pending lock") =
            Some((game_id.to_owned(), launch_id.clone()));
        launch_id
    }

    /// Drop a reservation. True when it was the reserved launch.
    pub fn cancel(&self, launch_id: &str) -> bool {
        let mut pending = self.pending.lock().expect("android pending lock");
        if pending.as_ref().is_some_and(|(_, id)| id == launch_id) {
            *pending = None;
            true
        } else {
            false
        }
    }

    /// Take the reservation for this exact game and launch.
    pub fn take_reservation(&self, game_id: &str, launch_id: &str) -> Result<(), RpcFailure> {
        let mut pending = self.pending.lock().expect("android pending lock");
        match pending.as_ref() {
            Some((game, id)) if game == game_id && id == launch_id => {
                *pending = None;
                Ok(())
            }
            _ => Err(failure(
                "SessionMismatch",
                "That launch is no longer reserved.",
            )),
        }
    }

    /// Prepare and start one Android route. Refuses while another live
    /// session exists, as the Linux host does.
    pub fn launch(
        &self,
        root: &Path,
        snapshot: &ConfigSnapshot,
        registry: &PluginRegistry,
        route: &ResolvedRoute,
        person: &str,
    ) -> Result<LocalSession, RpcFailure> {
        self.launch_as(root, snapshot, registry, route, person, None)
    }

    pub fn launch_as(
        &self,
        root: &Path,
        snapshot: &ConfigSnapshot,
        registry: &PluginRegistry,
        route: &ResolvedRoute,
        person: &str,
        launch_id: Option<String>,
    ) -> Result<LocalSession, RpcFailure> {
        let mut active = self.active.lock().expect("android session lock");
        if active.is_some() {
            return Err(failure(
                "SessionBusy",
                "another game is already live on this device",
            ));
        }
        let (plan, content_path) =
            prepare(root, snapshot, registry, route, person, &*self.launcher)?;
        self.launcher
            .start(&plan, content_path.as_deref())
            .map_err(|message| failure("LocalRouteUnavailable", message))?;
        let session = LocalSession {
            launch_id: launch_id.unwrap_or_else(new_launch_id),
            game_id: route.playable_id.clone(),
            title: route.title.clone(),
            package: plan.android.package.clone(),
            phase: Phase::Playing,
            save_locations: plan.save_locations.clone(),
        };
        *active = Some(session.clone());
        Ok(session)
    }

    fn exact(&self, expected: Option<&str>) -> Result<LocalSession, RpcFailure> {
        let active = self.active().ok_or_else(|| {
            failure(
                "NoActiveSession",
                "This device holds no local game session.",
            )
        })?;
        if expected.is_some_and(|id| id != active.launch_id) {
            return Err(failure(
                "SessionMismatch",
                "That game is no longer the live session.",
            ));
        }
        Ok(active)
    }

    pub fn leave(&self, expected: Option<&str>) -> Result<(LocalSession, bool), RpcFailure> {
        let session = self.exact(expected)?;
        self.launcher
            .show_korri()
            .map_err(|m| failure("HostControlFailed", m))?;
        Ok((
            self.set_phase(&session.launch_id, Phase::Left),
            session.phase != Phase::Left,
        ))
    }

    pub fn resume(&self, expected: Option<&str>) -> Result<(LocalSession, bool), RpcFailure> {
        let session = self.exact(expected)?;
        self.launcher
            .resume(&session.package)
            .map_err(|m| failure("HostControlFailed", m))?;
        Ok((
            self.set_phase(&session.launch_id, Phase::Playing),
            session.phase != Phase::Playing,
        ))
    }

    /// Closes the live session. The app may keep running.
    pub fn end(&self, expected: Option<&str>) -> Result<LocalSession, RpcFailure> {
        let session = self.exact(expected)?;
        *self.active.lock().expect("android session lock") = None;
        if let Err(message) = self.launcher.end_session(&session) {
            eprintln!(
                "korrid android: end of {} incomplete: {message}",
                session.package
            );
        }
        Ok(session)
    }

    /// Usage events: the game's app left the foreground (`closed` = false)
    /// or its task is gone (`closed` = true).
    pub fn app_left(&self, package: &str, closed: bool) {
        let mut active = self.active.lock().expect("android session lock");
        if active.as_ref().is_some_and(|s| s.package == package) {
            if closed {
                *active = None;
            } else if let Some(session) = active.as_mut() {
                session.phase = Phase::Left;
            }
        }
    }

    fn set_phase(&self, launch_id: &str, phase: Phase) -> LocalSession {
        let mut active = self.active.lock().expect("android session lock");
        let session = active
            .as_mut()
            .filter(|s| s.launch_id == launch_id)
            .expect("exact session");
        session.phase = phase;
        session.clone()
    }
}

fn new_launch_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Build the runner's input and ask it for its Android plan.
pub fn prepare(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    route: &ResolvedRoute,
    person: &str,
    launcher: &dyn AppLauncher,
) -> Result<(AndroidLaunchOutput, Option<String>), RpcFailure> {
    let unavailable = |m: String| failure("LocalRouteUnavailable", m);
    let runner = registry
        .runners()
        .get(&route.runner_id)
        .filter(|runner| runner.is_android())
        .ok_or_else(|| unavailable(format!("{} is not an Android runner", route.runner_id)))?;
    let package = registry
        .installed_package(&route.runner_id)
        .map_err(|e| unavailable(e.to_string()))?;
    let content_path = route
        .file_target
        .as_ref()
        .map(|target| {
            storage::resolve_file_target(root, snapshot, target)
                .map(|file| file.path.display().to_string())
                .map_err(|e| unavailable(e.to_string()))
        })
        .transpose()?;
    let provider_ref = route
        .provider_target
        .as_ref()
        .map(|t| t.provider_ref.clone());
    // A provider-ref runner (no androidPackages) may start exactly the
    // game's own package; otherwise the first installed declared package.
    let allowed: Vec<String> = match &runner.android_packages {
        Some(packages) => packages.clone(),
        None => provider_ref
            .iter()
            .filter(|r| crate::plugin::is_android_package(r))
            .cloned()
            .collect(),
    };
    let android_package = allowed.iter().find(|p| launcher.is_installed(p)).cloned();
    let Some(android_package) = android_package else {
        return Err(failure(
            "AndroidAppMissing",
            format!(
                "Install {} to play this game here",
                allowed.first().map_or("the app", String::as_str)
            ),
        ));
    };
    let account_root = crate::saves::account_root(root, person).map_err(unavailable)?;
    let input = PluginLaunchInput {
        display: None,
        netplay: None,
        layout: None,
        runner_id: route.runner_id.clone(),
        family_id: route.family_id.clone(),
        program: String::new(),
        core_path: None,
        content_path: content_path.clone(),
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
        files: Default::default(),
        artifacts: crate::acquisition::launch_artifacts(root, registry, &route.runner_id)
            .map_err(|f| unavailable(f.message))?,
        overrides: None,
        android_package: Some(android_package),
    };
    let source = crate::script::source::SourceSnapshot::package_plugin(
        &package.package,
        &package.entry,
        &package.sources,
    )
    .map_err(unavailable)?;
    let plan = plugin_launch::evaluate_android(&source, &input, &allowed).map_err(unavailable)?;
    Ok((plan, content_path))
}

#[cfg(test)]
#[path = "android_local_tests.rs"]
mod tests;
