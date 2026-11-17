//! Recovery words at a terminal (ADRs 0072 and 0082). Korri shows new words
//! and their QR code on the terminal's other screen, so they do not stay in
//! its scrollback, and reads words with the terminal's echo off. Words never
//! go to stdout, into a JSON response, or into an argument.
use crate::{error::CliError, prompt, recovery_qr};
use korrid::recovery_words::{RecoveryWords, WORD_COUNT};
use std::io::{self, BufRead, IsTerminal, Write};
use zeroize::Zeroizing;

/// The longest word a person types: the longest BIP39 word has 8 letters.
const WORD_LIMIT: u64 = 64;
/// Twelve words with any spacing.
const WORDS_LIMIT: u64 = 1024;
/// Switch to the terminal's other screen and clear it; switch back.
const OTHER_SCREEN: &str = "\x1b[?1049h\x1b[H\x1b[2J";
const MAIN_SCREEN: &str = "\x1b[2J\x1b[?1049l";

fn needs_terminal() -> CliError {
    CliError::usage(
        "Recovery words need a person at a terminal. Run this at the device's terminal or over SSH with a terminal.",
    )
}

/// Words are shown and typed only at a terminal: stdin, stdout and stderr.
pub(crate) fn require_terminal() -> Result<(), CliError> {
    let terminal = prompt::interactive(io::stdin().is_terminal(), io::stdout().is_terminal())
        && io::stderr().is_terminal();
    terminal.then_some(()).ok_or_else(needs_terminal)
}

/// The words in three rows of four, numbered from 1.
pub(crate) fn numbered(words: &str) -> Zeroizing<String> {
    let mut table = Zeroizing::new(String::with_capacity(512));
    for (index, word) in words.split(' ').enumerate() {
        let cell = Zeroizing::new(format!("{:>3} {:<10}", index + 1, word));
        table.push_str(&cell);
        if index % 4 == 3 {
            table.push('\n');
        }
    }
    table
}

/// Show new words and their QR code until the person presses Enter.
pub(crate) fn show(words: &str) -> Result<(), CliError> {
    require_terminal()?;
    let qr = recovery_qr::render(words)?;
    let table = numbered(words);
    let mut screen = io::stderr().lock();
    let shown = write!(
        screen,
        "{OTHER_SCREEN}These are your recovery words. Write all {WORD_COUNT} on paper, in this order.\n\
         They are the only copy of your person key: no device keeps it.\n\
         You need them to move this device to another person's devices, to undo a removal,\n\
         and when every device is lost. Keep the paper safe.\n\n{}\n\
         The QR code holds the same {WORD_COUNT} words as plain text:\n\n{}\n\
         Press Enter when the words are on paper.",
        table.as_str(),
        qr.as_str()
    )
    .and_then(|()| screen.flush());
    let mut line = Zeroizing::new(String::new());
    let read = shown.and_then(|()| io::stdin().lock().read_line(&mut line));
    let back = write!(screen, "{MAIN_SCREEN}").and_then(|()| screen.flush());
    read.and(back)
        .map(|_| ())
        .map_err(|_| CliError::usage("Could not show the recovery words."))
}

/// Read one hidden line.
fn hidden(label: &str, limit: u64) -> Result<Zeroizing<String>, CliError> {
    require_terminal()?;
    let bytes = Zeroizing::new(crate::private_terminal::read_line(label, limit)?);
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| CliError::usage("Words are plain letters."))?
        .trim_end_matches(['\r', '\n']);
    Ok(Zeroizing::new(text.to_owned()))
}

/// Ask for the words at these places, from 1 to 12, one at a time.
pub(crate) fn ask_confirmation(positions: &[u8]) -> Result<Zeroizing<Vec<String>>, CliError> {
    eprintln!("Type the words from your paper to confirm them.");
    let mut answers = Zeroizing::new(Vec::with_capacity(positions.len()));
    for position in positions {
        let answer = hidden(&format!("Word {position}"), WORD_LIMIT)?;
        answers.push(answer.trim().to_owned());
    }
    Ok(answers)
}

/// Read 12 words with echo off and check them here first: the right count,
/// known words, a valid checksum. Returns them lowercase with one space
/// between words.
pub(crate) fn read_words(label: &str) -> Result<Zeroizing<String>, CliError> {
    let typed = hidden(label, WORDS_LIMIT)?;
    let words = RecoveryWords::parse(&typed).map_err(|error| {
        CliError::failure(
            "RecoveryWordsInvalid",
            error.to_string(),
            "Check the words on your paper and try again.",
        )
    })?;
    Ok(words.phrase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_numbered_in_three_rows_of_four() {
        let table = numbered(
            "leader monkey parrot ring guide accident before fence cannon height naive bean",
        );
        let rows: Vec<&str> = table.lines().collect();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].starts_with("  1 leader"));
        assert!(rows[1].contains("  6 accident"));
        assert!(rows[2].trim_end().ends_with("12 bean"));
    }
}
