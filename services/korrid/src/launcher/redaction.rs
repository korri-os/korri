//! Native append text is opaque, but its shell words and launch env values must
//! not appear in diagnostics. This module has no filesystem or process effects.
use super::plugin_launch::LaunchComposition;

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
        self.append
            .iter()
            .flatten()
            .chain(&self.env)
            .filter(|v| !v.is_empty())
            .cloned()
            .collect()
    }
    pub(crate) fn text(&self, text: &str) -> String {
        redact_text(text, &self.words())
    }
    pub(crate) fn operation_failure(&self, failure: &crate::script::OperationFailure) -> String {
        match failure {
            crate::script::OperationFailure::Plugin(error) => format!(
                "{} ({})",
                status_text(&self.text(&error.message), 300),
                self.text(&error.code)
            ),
            other => status_text(&self.text(&other.to_string()), 300),
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
    // Longest first: a short overlapping value must not leave part of a longer
    // credential visible. Replace in one pass so tokens cannot alter markers.
    let mut words: Vec<_> = words
        .iter()
        .filter(|v| !v.is_empty())
        .map(String::as_str)
        .collect();
    words.sort_by_key(|word| std::cmp::Reverse(word.len()));
    let mut result = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(word) = words.iter().find(|word| rest.starts_with(**word)) {
            result.push_str(REDACTED);
            rest = &rest[word.len()..];
        } else {
            let ch = rest.chars().next().expect("nonempty text");
            result.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    result
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
        assert_eq!(shown, format!("{} (setting-range)", "x".repeat(300)));
    }
}
