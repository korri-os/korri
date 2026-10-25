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

/// Ordinary (never secret) choices. No raw terminal mode or default selection:
/// a number or exact ID is an explicit choice, including after filtering.
pub fn select(label: &str, choices: &[(String, String)]) -> Result<String, CliError> {
    if !interactive(io::stdin().is_terminal(), io::stdout().is_terminal()) {
        return Err(CliError::usage(
            "Selection requires both stdin and stdout to be terminals. Supply an exact ID.",
        ));
    }
    let mut visible = choices.iter().collect::<Vec<_>>();
    loop {
        for (index, (id, title)) in visible.iter().enumerate() {
            eprintln!(
                "{}. {} ({})",
                index + 1,
                crate::output::text(title),
                crate::output::text(id)
            );
        }
        let answer = read(&format!(
            "Choose {label}: number or exact ID; /text filters, / resets, q cancels"
        ))?;
        // An exact ID wins even when it is numeric, `q`, or starts with `/`.
        if let Some((id, _)) = choices.iter().find(|(id, _)| id == &answer) {
            return Ok(id.clone());
        }
        if answer == "q" {
            return Err(CliError::failure(
                "Cancelled",
                "Selection was cancelled.",
                "Nothing changed.",
            ));
        }
        if let Some(query) = answer.strip_prefix('/') {
            let query = query.to_lowercase();
            visible = choices
                .iter()
                .filter(|(id, title)| {
                    id.to_lowercase().contains(&query) || title.to_lowercase().contains(&query)
                })
                .collect();
            if visible.is_empty() {
                eprintln!("No matches. Use / to show all choices.");
            }
            continue;
        }
        if let Some(index) = choice_number(&answer, visible.len()) {
            return Ok(visible[index].0.clone());
        }
        eprintln!("Choose a listed number or exact ID. Nothing has been selected.");
    }
}

fn choice_number(input: &str, count: usize) -> Option<usize> {
    input
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
        .filter(|index| *index < count)
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
    fn picker_numbers_are_explicit_and_in_range() {
        assert_eq!(choice_number("1", 2), Some(0));
        assert_eq!(choice_number("2", 2), Some(1));
        for input in ["0", "3", "-1", "", "abc", "99999999999999999999999999"] {
            assert_eq!(choice_number(input, 2), None);
        }
        assert_eq!(choice_number("1", 0), None);
    }

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
