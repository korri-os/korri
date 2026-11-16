//! Opt-in CPU phase diagnostics. Never logs view/model/input payloads.
use std::sync::OnceLock;

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("KORRI_SHELL_PROFILE").is_some_and(|v| v == "1"))
}
