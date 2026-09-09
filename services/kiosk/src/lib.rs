mod browser;
mod cdp;
mod kiosk;
mod pipe;
mod runtime;

pub use kiosk::{Options, run};

pub const PORTAL_ORIGIN: &str = "http://127.0.0.1:8099";
pub const PORTAL_URL: &str = "http://127.0.0.1:8099/";
pub const BOOTSTRAP_URL: &str = "http://127.0.0.1:8099/kiosk-blank.html";
pub const RUNTIME_URL: &str = "http://127.0.0.1:8099/runtime.json";
pub const BRAIN_FILE: &str = "/run/korrid-browser/brain.json";

// Never carry external strings in errors: CDP errors, JSON parse errors and
// Chromium diagnostics may contain the capability or a fulfilled response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Configuration,
    Io,
    Protocol,
    Timeout,
    Closed,
    Interrupted,
    Browser,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Configuration => "invalid kiosk configuration",
            Self::Io => "kiosk I/O failed",
            Self::Protocol => "Chromium control protocol failed",
            Self::Timeout => "Chromium control deadline expired",
            Self::Closed => "Chromium control pipe closed",
            Self::Interrupted => "kiosk interrupted",
            Self::Browser => "Chromium process failed",
        })
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}

#[cfg(test)]
mod tests;
