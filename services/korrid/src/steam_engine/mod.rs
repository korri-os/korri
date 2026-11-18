//! `P/engine`: Steam games on Valve's engine with no Steam client (ADR
//! 0083, korri-os/korri#518), where `P` is the Steam plugin's own id. The
//! runner, the shim and the host's source are the Steam plugin's; korrid owns
//! the effects: the engine package, the person's engine folder, the host as a
//! launch service and as a job, the sign-in model, the provider listing and
//! jobs, and the save snapshots.
//!
//! korrid finds the Steam plugin by its short name `steam`, in any publisher
//! namespace, and derives every id from its plugin id `P` (ADR 0045's
//! `<pluginId>/<key>`): the provider `P`, the engine runner `P/engine`, the
//! client runner `P/steam`, the engine package `P/engine-package` and the
//! discovery `P/installed-apps`. Every key here is a closed first-party
//! name, like the Sunshine launch service and the RetroArch session
//! controls. Nothing is per game.
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

pub use crate::plugin_discovery::is_steam_plugin;

/// The engine runner's key. The client runner `P/steam` stays the default;
/// this one is chosen per game or for the system.
pub const ENGINE_KEY: &str = "engine";
/// The client runner's key, the fallback.
pub const CLIENT_KEY: &str = "steam";
/// The key of Valve's pinned `bins_sdk_linuxarm64_linuxarm64` package.
pub const PACKAGE_KEY: &str = "engine-package";
/// The system both Steam runners run.
pub const SYSTEM_ID: &str = "steam";
/// The Steam plugin's package file key of the engine host program.
pub const HOST_FILE: &str = "korri-steam-host";

/// `P/engine`.
pub fn engine_runner(plugin: &str) -> String {
    format!("{plugin}/{ENGINE_KEY}")
}

/// `P/steam`.
pub fn client_runner(plugin: &str) -> String {
    format!("{plugin}/{CLIENT_KEY}")
}

/// `P/engine-package`.
pub fn engine_artifact(plugin: &str) -> String {
    format!("{plugin}/{PACKAGE_KEY}")
}

/// The Steam plugin `P` and the key of an id `P/<key>`.
fn steam_parts(id: &str) -> Option<(&str, &str)> {
    id.split_once('/')
        .filter(|(plugin, _)| is_steam_plugin(plugin))
}

/// True for `P/engine` of a Steam plugin `P`.
pub fn is_engine_runner(id: &str) -> bool {
    steam_parts(id).is_some_and(|(_, key)| key == ENGINE_KEY)
}

/// True for `P/steam` of a Steam plugin `P`.
pub fn is_client_runner(id: &str) -> bool {
    steam_parts(id).is_some_and(|(_, key)| key == CLIENT_KEY)
}

/// The Steam plugin `P` of a runner or other contribution id `P/<key>`.
pub fn plugin_of(id: &str) -> Option<&str> {
    steam_parts(id).map(|(plugin, _)| plugin)
}

/// The enabled Steam plugin that declares an engine runner, if any. With
/// more than one (two namespaces), the first by id.
pub fn steam_plugin(registry: &crate::plugin::PluginRegistry) -> Option<String> {
    registry
        .runners()
        .keys()
        .find(|id| is_engine_runner(id))
        .and_then(|id| plugin_of(id))
        .map(str::to_owned)
}

/// The one file of that package the runner uses.
pub const ENGINE_FILE: &str = "linuxarm64/steamclient.so";
/// The refusal the shim reports when the launch sync finds a Steam Cloud
/// conflict (`STATUS=plugin-launch: steam-cloud-conflict`): no game ran,
/// and korrid has written the split.
pub const CONFLICT_REFUSAL: &str = "steam-cloud-conflict";
/// The Proton plugin's artifact key the engine uses as its compat tool. The
/// Proton plugin is found by its short name `proton`, in any namespace, as
/// the Steam plugin is.
pub const PROTON_KEY: &str = "cachyos";

/// True for `@<namespace>:proton/cachyos`.
fn is_proton_artifact(id: &str) -> bool {
    id.strip_prefix('@')
        .and_then(|rest| rest.split_once(':'))
        .is_some_and(|(namespace, rest)| {
            !namespace.is_empty() && rest == format!("proton/{PROTON_KEY}")
        })
}

/// The host plan for one engine-runner launch, from the runner's ready
/// artifacts as `launch_artifacts` names them: the runner's own package by
/// its key, another plugin's by its full id.
pub fn launch_plan(
    registry: &crate::plugin::PluginRegistry,
    runner_id: &str,
    artifacts: &std::collections::BTreeMap<String, String>,
    account_root: &std::path::Path,
) -> Result<host::HostPlan, String> {
    let host_program = registry
        .installed_file(runner_id, HOST_FILE)
        .map_err(|_| "This Steam plugin has no engine host.".to_owned())?;
    let engine = artifacts.get(PACKAGE_KEY).ok_or_else(|| {
        availability::Unavailable::PackageNotDownloaded
            .reason()
            .to_owned()
    })?;
    Ok(host::HostPlan {
        host_program,
        engine: std::path::Path::new(engine).join(ENGINE_FILE),
        proton: artifacts
            .iter()
            .find(|(id, _)| is_proton_artifact(id))
            .map(|(_, path)| path.into()),
        account_root: account_root.to_path_buf(),
    })
}

static CONFLICTS: std::sync::OnceLock<std::sync::Arc<saves::Conflicts>> =
    std::sync::OnceLock::new();
static UNAVAILABLE: std::sync::OnceLock<
    std::sync::Arc<std::sync::Mutex<Option<availability::Unavailable>>>,
> = std::sync::OnceLock::new();
static SESSIONS: std::sync::OnceLock<sessions::Sessions> = std::sync::OnceLock::new();

/// Open Steam Cloud conflicts of this korrid, kept in its private state
/// root and shared by the session watcher, the session's completion and the
/// save pick. The first caller's root is the device's one root.
pub fn conflicts(private_root: &std::path::Path) -> &'static std::sync::Arc<saves::Conflicts> {
    CONFLICTS.get_or_init(|| std::sync::Arc::new(saves::Conflicts::at(private_root)))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every id follows the Steam plugin's own id, in any namespace.
    #[test]
    fn ids_follow_the_plugin_in_any_namespace() {
        for plugin in ["@korri:steam", "@other:steam"] {
            assert!(is_steam_plugin(plugin));
            assert!(is_engine_runner(&engine_runner(plugin)));
            assert!(is_client_runner(&client_runner(plugin)));
            assert_eq!(plugin_of(&engine_runner(plugin)), Some(plugin));
            assert_eq!(engine_artifact(plugin), format!("{plugin}/engine-package"));
        }
        assert_eq!(engine_runner("@other:steam"), "@other:steam/engine");
        for not in [
            "@korri:steamgriddb",
            "@other:steamy",
            "@:steam",
            "korri:steam",
            "@korri:steam2",
        ] {
            assert!(!is_steam_plugin(not), "{not}");
            assert!(!is_engine_runner(&format!("{not}/engine")), "{not}");
        }
        assert!(
            !is_engine_runner("@korri:steam/steam") && !is_client_runner("@korri:steam/engine")
        );
        assert!(
            is_proton_artifact("@korri:proton/cachyos")
                && is_proton_artifact("@other:proton/cachyos")
        );
        assert!(
            !is_proton_artifact("@korri:proton/other") && !is_proton_artifact("proton/cachyos")
        );
    }
}
