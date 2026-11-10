//! ADR 0054: which platforms a runner runs on. Shared by korrid's plugin
//! module and the plugin host (plugin-host/src/lib.rs includes this file), so
//! it depends on nothing else in either crate.

/// A runner without `platforms` runs on these, as before ADR 0054.
pub(crate) const LINUX_PLATFORMS: [&str; 2] = ["x86_64-linux", "aarch64-linux"];

/// This device's platform. Other targets match no `platforms` entry.
pub(crate) fn host_platform() -> &'static str {
    if cfg!(target_os = "android") {
        "aarch64-android"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64-linux"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64-linux"
    } else {
        "unsupported"
    }
}

/// A runner's `platforms`, where omitted means every Linux platform.
pub(crate) fn platforms_include(platforms: Option<&[String]>, platform: &str) -> bool {
    match platforms {
        Some(platforms) => platforms.iter().any(|p| p == platform),
        None => LINUX_PLATFORMS.contains(&platform),
    }
}
