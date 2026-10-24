//! Prompt policy is shared by areas. `--yes` confirms destruction only. Areas
//! must not use it to skip plugin permission review or provide missing inputs.
use crate::error::CliError;
use std::io::{self, IsTerminal, Write};

pub fn interactive(stdin_terminal: bool, stdout_terminal: bool) -> bool {
    stdin_terminal && stdout_terminal
}

pub fn read(label: &str) -> Result<String, CliError> {
    if !interactive(io::stdin().is_terminal(), io::stdout().is_terminal()) {
        return Err(CliError::usage(
            "Missing input. Scripts must provide all inputs explicitly.",
        ));
    }
    eprint!("{}: ", crate::output::text(label));
    io::stderr()
        .flush()
        .map_err(|_| CliError::usage("Could not show the prompt."))?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|_| CliError::usage("Could not read input."))?;
    let input = input.trim().to_owned();
    if input.is_empty() {
        return Err(CliError::usage("Input is required."));
    }
    Ok(input)
}

pub fn confirm(label: &str, yes: bool) -> Result<(), CliError> {
    if yes {
        return Ok(());
    }
    let answer = read(&format!("{label} [y/N]"))?;
    if answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes") {
        Ok(())
    } else {
        Err(CliError::failure(
            "Cancelled",
            "The action was not confirmed.",
            "Nothing changed.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompts_require_both_terminals() {
        for (input, output, expected) in [
            (false, false, false),
            (false, true, false),
            (true, false, false),
            (true, true, true),
        ] {
            assert_eq!(interactive(input, output), expected);
        }
    }
}
