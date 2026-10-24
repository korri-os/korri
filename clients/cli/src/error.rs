use korrid::RpcFailure;

#[derive(Debug)]
pub struct CliError {
    pub failure: RpcFailure,
    pub next: String,
    pub exit: u8,
}

impl CliError {
    pub fn failure(code: &str, message: impl Into<String>, next: impl Into<String>) -> Self {
        Self {
            failure: RpcFailure {
                code: code.into(),
                message: message.into(),
            },
            next: next.into(),
            exit: 1,
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            exit: 2,
            ..Self::failure("Usage", message, "Run korri --help for usage.")
        }
    }

    pub fn unreachable() -> Self {
        Self {
            exit: 4,
            ..Self::failure(
                "KorridUnreachable",
                "Local korrid cannot be reached.",
                "Start korrid on this device and try again.",
            )
        }
    }

    pub fn rpc(failure: RpcFailure) -> Self {
        Self {
            failure,
            next: "Check korri status, then try again.".into(),
            exit: 1,
        }
    }

    pub fn print(&self, json: bool) {
        if json {
            // Errors use the existing Rust failure type, never transport Debug
            // text (which could include credentials or request bodies).
            eprintln!(
                "{}",
                serde_json::to_string(&self.failure).expect("serialize RPC failure")
            );
        } else {
            eprintln!(
                "{}\n{}",
                crate::output::text(&self.failure.message),
                crate::output::text(&self.next)
            );
        }
    }
}
