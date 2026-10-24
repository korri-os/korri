//! Input for settings areas. Private stdin is always opt-in, bounded and never
//! included in diagnostics. Terminal secrets use the terminal's native echo flag.
use crate::{error::CliError, prompt};
use clap::ArgMatches;
use serde::de::DeserializeOwned;
use std::io::{self, BufRead, IsTerminal, Read};

const PRIVATE_INPUT_LIMIT: u64 = 64 * 1024;

pub fn required(args: &ArgMatches, name: &str, label: &str) -> Result<String, CliError> {
    args.get_one::<String>(name)
        .cloned()
        .map(Ok)
        .unwrap_or_else(|| prompt::read(label))
}

fn unavailable() -> CliError {
    CliError::usage(
        "Could not read private input. Use the explicit private stdin option or a terminal.",
    )
}

pub fn private_line(explicit_stdin: bool, label: &str) -> Result<String, CliError> {
    let stdin_terminal = io::stdin().is_terminal();
    let terminal = prompt::interactive(stdin_terminal, io::stdout().is_terminal());
    if (stdin_terminal || !explicit_stdin) && !terminal {
        return Err(unavailable());
    }
    let mut bytes = if terminal {
        crate::private_terminal::read_line(label, PRIVATE_INPUT_LIMIT)?
    } else {
        let mut bytes = Vec::new();
        io::stdin()
            .lock()
            .take(PRIVATE_INPUT_LIMIT + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| unavailable())?;
        bytes
    };
    if bytes.len() as u64 > PRIVATE_INPUT_LIMIT {
        return Err(unavailable());
    }
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    let input = String::from_utf8(bytes).map_err(|_| unavailable())?;
    if input.is_empty() {
        return Err(CliError::usage("Private input is required."));
    }
    Ok(input)
}

/// Deserializes the producer's existing request type, not a client-side schema.
pub fn private_request<T: DeserializeOwned>() -> Result<T, CliError> {
    if io::stdin().is_terminal() {
        return Err(CliError::usage("Pipe one existing typed request into --private-stdin. Use terminal input without that flag."));
    }
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take(PRIVATE_INPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() as u64 > PRIVATE_INPUT_LIMIT {
        return Err(unavailable());
    }
    serde_json::from_slice(&bytes).map_err(|_| CliError::usage("Private stdin must contain the existing typed request, including dataDisposition and trustLossConfirmed. No input was sent."))
}
