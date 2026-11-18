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
pub mod protocol;
pub mod provider;
pub mod qr;
pub mod saves;

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
