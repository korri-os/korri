//! `@korri:steam/engine`: Steam games on Valve's engine with no Steam client
//! (ADR 0083, korri-os/korri#518). The runner, the shim and the host's source
//! are the Steam plugin's; korrid owns the effects: the engine package, the
//! person's engine folder, the host as a launch service and as a job, the
//! sign-in model, the provider listing and jobs, and the save snapshots.
//!
//! Every name here is a closed first-party constant, like the Sunshine
//! launch service and the RetroArch session controls. Nothing is per game.
pub mod account;
pub mod availability;
pub mod folder;
pub mod host;
pub mod jobs;
pub mod protocol;
pub mod provider;
pub mod qr;
pub mod saves;
pub mod sessions;
pub mod store;

/// The plugin that declares the engine runner, its host and its package.
pub const PLUGIN_ID: &str = "@korri:steam";
/// The engine runner. The client runner `@korri:steam/steam` stays the
/// default; this one is chosen per game or for the system.
pub const RUNNER_ID: &str = "@korri:steam/engine";
/// The client runner, the fallback.
pub const CLIENT_RUNNER_ID: &str = "@korri:steam/steam";
/// The `@korri:steam` package file key of the engine host program.
pub const HOST_FILE: &str = "korri-steam-host";
/// Valve's pinned `bins_sdk_linuxarm64_linuxarm64` package.
pub const ENGINE_ARTIFACT: &str = "@korri:steam/engine-package";
/// The one file of that package the runner uses.
pub const ENGINE_FILE: &str = "linuxarm64/steamclient.so";
/// korrid's Proton artifact, the engine's compat tool.
pub const PROTON_ARTIFACT: &str = "@korri:proton/cachyos";

/// The host plan for one engine-runner launch, from the runner's ready
/// artifacts as `launch_artifacts` names them: the runner's own package by
/// its key, another plugin's by its full id.
pub fn launch_plan(
    registry: &crate::plugin::PluginRegistry,
    artifacts: &std::collections::BTreeMap<String, String>,
    account_root: &std::path::Path,
) -> Result<host::HostPlan, String> {
    let host_program = registry
        .installed_file(RUNNER_ID, HOST_FILE)
        .map_err(|_| "This Steam plugin has no engine host.".to_owned())?;
    let (_, package_key) = ENGINE_ARTIFACT.split_once('/').expect("artifact id");
    let engine = artifacts.get(package_key).ok_or_else(|| {
        availability::Unavailable::PackageNotDownloaded
            .reason()
            .to_owned()
    })?;
    Ok(host::HostPlan {
        host_program,
        engine: std::path::Path::new(engine).join(ENGINE_FILE),
        proton: artifacts.get(PROTON_ARTIFACT).map(Into::into),
        account_root: account_root.to_path_buf(),
    })
}

static CONFLICTS: std::sync::OnceLock<std::sync::Arc<saves::Conflicts>> =
    std::sync::OnceLock::new();
static UNAVAILABLE: std::sync::OnceLock<
    std::sync::Arc<std::sync::Mutex<Option<availability::Unavailable>>>,
> = std::sync::OnceLock::new();
static SESSIONS: std::sync::OnceLock<sessions::Sessions> = std::sync::OnceLock::new();

/// Open Steam Cloud conflicts of this korrid, shared by the session watcher
/// and the save pick.
pub fn conflicts() -> &'static std::sync::Arc<saves::Conflicts> {
    CONFLICTS.get_or_init(Default::default)
}

/// The engine state the last host reported on this device.
pub fn unavailable() -> &'static std::sync::Arc<std::sync::Mutex<Option<availability::Unavailable>>>
{
    UNAVAILABLE.get_or_init(Default::default)
}

/// The device's session hosts, made once with the launch backend's identity.
pub fn sessions(make: impl FnOnce() -> sessions::Sessions) -> &'static sessions::Sessions {
    SESSIONS.get_or_init(make)
}

static STORE: std::sync::OnceLock<std::sync::Arc<store::SteamStore>> = std::sync::OnceLock::new();

/// The Steam provider's listing cache and jobs on this device.
pub fn store() -> &'static std::sync::Arc<store::SteamStore> {
    STORE.get_or_init(Default::default)
}

static LIVE: std::sync::OnceLock<std::sync::Arc<std::sync::atomic::AtomicUsize>> =
    std::sync::OnceLock::new();

/// Session hosts running on this device now.
pub fn live_sessions() -> &'static std::sync::Arc<std::sync::atomic::AtomicUsize> {
    LIVE.get_or_init(Default::default)
}
