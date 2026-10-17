//! Native append text is opaque, but its shell words and launch env values must
//! not appear in diagnostics. This module has no filesystem or process effects.
use super::plugin_launch::LaunchComposition;

pub(crate) const MAX_REASON_CHARS: usize = 300;
pub(crate) const MAX_REASON_LINES: usize = 3;
pub(crate) const REDACTED: &str = "[redacted]";
pub(crate) const APPEND_SPLIT_ERROR: &str = "config.append could not be split into words";

#[derive(Clone, Debug, Default)]
pub(crate) struct LaunchRedaction {
    append: Vec<Vec<String>>,
    env: Vec<String>,
}

impl LaunchRedaction {
    pub(crate) fn from_composition(composition: &LaunchComposition) -> Result<Self, String> {
        let append = composition
            .modifiers
            .iter()
            .map(|modifier| append_words(modifier.config.append.as_deref().unwrap_or("")))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            append,
            env: Vec::new(),
        })
    }
    pub(crate) fn add_env(&mut self, values: impl IntoIterator<Item = String>) {
        self.env
            .extend(values.into_iter().filter(|value| !value.is_empty()));
    }
    pub(crate) fn words(&self) -> Vec<String> {
        let mut words: Vec<_> = self
            .append
            .iter()
            .flatten()
            .chain(&self.env)
            .filter(|v| !v.is_empty())
            .cloned()
            .collect();
        words.sort_unstable();
        words.dedup();
        words
    }
    pub(crate) fn text(&self, text: &str) -> String {
        redact_text(text, &self.words())
    }
    pub(crate) fn operation_failure(&self, failure: &crate::script::OperationFailure) -> String {
        match failure {
            crate::script::OperationFailure::Plugin(error) => {
                // Reserve the code and punctuation within the total cause
                // budget. An oversized code cannot bypass the same bound.
                let code = status_text(&self.text(&error.code), MAX_REASON_CHARS - 3);
                let suffix = format!(" ({code})");
                let message = status_text(
                    &self.text(&error.message),
                    MAX_REASON_CHARS - suffix.chars().count(),
                );
                format!("{message}{suffix}")
            }
            other => status_text(&self.text(&other.to_string()), MAX_REASON_CHARS),
        }
    }
    pub(crate) fn argv(&self, args: &[String], runner: &[String]) -> Vec<String> {
        let mut result = args.to_vec();
        // The runner argv is the untouched suffix, not native append. Choose
        // the last matching append sequence before it, preserving repeated
        // typed flags/values and the final -- plus runner arguments.
        let end = if args.ends_with(runner) {
            args.len() - runner.len()
        } else {
            args.len()
        };
        for words in &self.append {
            if words.is_empty() {
                continue;
            }
            if let Some(start) = args[..end]
                .windows(words.len())
                .rposition(|window| window == words)
            {
                for word in &mut result[start..start + words.len()] {
                    *word = REDACTED.into();
                }
            }
        }
        for arg in &mut result {
            *arg = redact_text(arg, &self.env);
        }
        result
    }
}

pub(crate) fn redact_text(text: &str, words: &[String]) -> String {
    redact_with_prefix(text, words, 0)
}

fn redact_with_prefix(text: &str, words: &[String], prefix: usize) -> String {
    let mut words: Vec<&str> = words
        .iter()
        .filter(|v| !v.is_empty())
        .map(String::as_str)
        .collect();
    words.sort_unstable();
    words.dedup();
    words.sort_by_key(|word| std::cmp::Reverse(word.len()));
    let mut result = if prefix > 0 {
        REDACTED.to_owned()
    } else {
        String::new()
    };
    let mut covered_until = prefix;
    // Continue checking covered positions: another word can overlap a match
    // (or the masked cut prefix) and extend past it. Skipping those positions
    // would expose that word's trailing characters.
    for (index, ch) in text.char_indices() {
        let rest = &text[index..];
        if let Some(word) = words.iter().find(|word| rest.starts_with(**word)) {
            if index >= covered_until {
                result.push_str(REDACTED);
            }
            covered_until = covered_until.max(index + word.len());
        }
        if index >= covered_until {
            result.push(ch);
        }
    }
    result
}

/// The caller has already selected at most three lines / 300 characters and
/// validated at most 256 words / 16 KiB. Simple matching is negligible within
/// these bounds; no matcher is ever run over the stderr file itself.
pub(crate) fn redact_display_tail(text: &str, words: &[String], truncated_start: bool) -> String {
    // Normal debug tests must also catch moving the display limit after the
    // matcher: even accepted word lists must never scan the stored 64 KiB.
    debug_assert!(text.chars().count() <= MAX_REASON_CHARS);
    debug_assert!(text.split_inclusive('\n').count() <= MAX_REASON_LINES);
    // Selecting the display window can cut inside a private word (including a
    // multiline env value). Mask any matching suffix at the window's start,
    // rather than scanning preceding stderr or showing a partial credential.
    let prefix = if truncated_start {
        words
            .iter()
            .flat_map(|word| word.char_indices().map(|(index, _)| &word[index..]))
            .filter(|suffix| text.starts_with(suffix))
            .map(str::len)
            .max()
            .unwrap_or(0)
    } else {
        0
    };
    redact_with_prefix(text, words, prefix)
}

pub(crate) fn status_text(text: &str, limit: usize) -> String {
    text.chars()
        .filter(|ch| !ch.is_control())
        .take(limit)
        .collect()
}

fn append_words(text: &str) -> Result<Vec<String>, String> {
    if text.contains('\0') {
        return Err(APPEND_SPLIT_ERROR.into());
    }
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    // Grounded in the approved native-append producer: POSIX quoting and
    // backslash rules, with no expansions, comments or operator execution.
    let mut input = text.chars().peekable();
    let (mut words, mut word, mut started, mut quote) = (Vec::new(), String::new(), false, None);
    while let Some(ch) = input.next() {
        if quote == Some('\'') {
            if ch == '\'' {
                quote = None;
            } else {
                word.push(ch);
            }
        } else if ch == '\\' {
            let next = input.peek().copied().ok_or(APPEND_SPLIT_ERROR)?;
            if quote.is_none() || matches!(next, '$' | '`' | '"' | '\\' | '\n') {
                input.next();
                if next != '\n' {
                    word.push(next);
                    started = true;
                }
            } else {
                word.push(ch);
            }
        } else if quote == Some('"') {
            if ch == '"' {
                quote = None;
            } else {
                word.push(ch);
            }
        } else if matches!(ch, '\'' | '"') {
            quote = Some(ch);
            started = true;
        } else if matches!(ch, ' ' | '\t' | '\n') {
            if started {
                words.push(std::mem::take(&mut word));
            }
            started = false;
        } else {
            word.push(ch);
            started = true;
        }
    }
    if quote.is_some() {
        return Err(APPEND_SPLIT_ERROR.into());
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn append_words_match_native_quoting_without_expansion() {
        assert_eq!(
            append_words("--flag 'private value' \"$HOME\" ~ *.rom #literal a\\ b \"a\\q\" ''")
                .unwrap(),
            [
                "--flag",
                "private value",
                "$HOME",
                "~",
                "*.rom",
                "#literal",
                "a b",
                "a\\q",
                ""
            ]
        );
        for value in ["", " \t\n\r"] {
            assert!(append_words(value).unwrap().is_empty());
        }
        for value in ["'secret", "trailing\\", "a\0b"] {
            assert_eq!(append_words(value).unwrap_err(), APPEND_SPLIT_ERROR);
        }
    }
    #[test]
    fn logs_preserve_typed_and_runner_argv_while_redacting_only_native_words_and_env() {
        let redaction = LaunchRedaction {
            append: vec![vec!["--limit".into(), "60".into()]],
            env: vec!["credential".into()],
        };
        let args: Vec<String> = [
            "--fixed",
            "--limit",
            "60",
            "--limit",
            "60",
            "--",
            "/runner",
            "--limit",
            "60",
            "credential",
        ]
        .map(Into::into)
        .into();
        let runner = args[6..].to_vec();
        assert_eq!(
            redaction.argv(&args, &runner),
            [
                "--fixed", "--limit", "60", REDACTED, REDACTED, "--", "/runner", "--limit", "60",
                REDACTED
            ]
        );
    }
}

#[cfg(test)]
mod error_bound_tests {
    use super::*;
    #[test]
    fn a_long_message_keeps_its_structured_error_code_after_redaction() {
        let failure =
            crate::script::OperationFailure::Plugin(crate::script::PluginOperationError {
                code: "setting-range".into(),
                message: "x".repeat(500),
                retryable: Some(false),
            });
        let shown = LaunchRedaction::default().operation_failure(&failure);
        assert_eq!(shown.chars().count(), 300);
        assert_eq!(
            shown,
            format!(
                "{} (setting-range)",
                "x".repeat(300 - " (setting-range)".len())
            )
        );
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test]
    fn producer_words_are_deduplicated_without_changing_native_argv_order() {
        let redaction = LaunchRedaction {
            append: vec![vec!["private".into(), "private".into()]],
            env: vec!["private".into(), "environment".into()],
        };
        assert_eq!(redaction.words(), ["environment", "private"]);
        assert_eq!(redaction.append[0], ["private", "private"]);
    }
    #[test]
    fn clipping_and_overlapping_words_do_not_expose_private_suffixes() {
        let words = vec!["prefix-abc".into(), "bcdef".into()];
        assert_eq!(redact_display_tail("abcdef", &words, true), REDACTED);
        assert_eq!(redact_text("prefix-abcdef", &words), REDACTED);
        assert_eq!(
            redact_text("bcdefbcdef", &words),
            format!("{REDACTED}{REDACTED}")
        );
    }
    #[test]
    fn total_cause_limit_counts_unicode_and_oversized_codes() {
        for code in ["range-界".to_owned(), "界".repeat(500)] {
            let failure =
                crate::script::OperationFailure::Plugin(crate::script::PluginOperationError {
                    code: code.clone(),
                    message: "🙂".repeat(500),
                    retryable: Some(false),
                });
            let cause = LaunchRedaction::default().operation_failure(&failure);
            assert_eq!(cause.chars().count(), MAX_REASON_CHARS);
            assert!(cause.ends_with(&format!(" ({})", status_text(&code, MAX_REASON_CHARS - 3))));
        }
    }
}
