//! Plugin answers that korrid gives again without running the plugin (#289).
//!
//! An idle device asks the same questions on timers. The portal polls save
//! marks, and the invite watch projects netplay routes. Each poll used to
//! transpile and evaluate the same plugins with the same input. Most of an
//! idle korrid's CPU went there.
//!
//! The sandbox has no I/O, so an answer depends only on the source graph, the
//! operation and the input. korrid keys each answer on a digest of exactly
//! those. A changed source, setting, path or file list is a different key, so
//! a kept answer is never stale. Only completed answers and `Unimplemented`
//! are kept. Every other failure, including the deadline, runs again.

use super::{source::SourceSnapshot, OperationFailure};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

/// The operations that korrid repeats with unchanged input while idle.
/// Discovery and acquisition turns read new outside facts each time, so their
/// answers are not kept.
const REUSED_OPERATIONS: [&str; 6] = [
    super::SETTINGS_DESCRIBE,
    super::SETTINGS_VALIDATE,
    super::SETTINGS_RENDER,
    super::RUNTIME_RESOLVE,
    super::LAUNCH_PREPARE,
    super::LAUNCH_COMPOSE,
];

/// haku asks about 400 different questions. The bounds leave room for a
/// larger library and stop growth when inputs keep changing.
const MAX_ANSWERS: usize = 4096;
const MAX_ANSWER_BYTES: usize = 16 * 1024 * 1024;

/// One question: a declaration, or one operation with its input.
pub(super) enum Question<'a> {
    Declaration,
    Operation { name: &'a str, input: &'a str },
}

impl Question<'_> {
    fn reused(&self) -> bool {
        match self {
            Self::Declaration => true,
            Self::Operation { name, .. } => REUSED_OPERATIONS.contains(name),
        }
    }
}

#[derive(Clone)]
enum Kept {
    Answer(String),
    Unimplemented,
}

#[derive(Default)]
struct Answers {
    kept: HashMap<[u8; 32], Kept>,
    bytes: usize,
}

fn answers() -> &'static Mutex<Answers> {
    static ANSWERS: OnceLock<Mutex<Answers>> = OnceLock::new();
    ANSWERS.get_or_init(Mutex::default)
}

fn digest(snapshot: &SourceSnapshot, entry: &str, question: &Question<'_>) -> [u8; 32] {
    let mut hash = Sha256::new();
    // Length prefixes keep field boundaries unambiguous.
    let mut field = |bytes: &[u8]| {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    field(entry.as_bytes());
    for (name, identity, bytes) in snapshot.canonical_entries() {
        field(name.as_bytes());
        field(identity.as_bytes());
        field(bytes);
    }
    match question {
        Question::Declaration => field(b"declaration"),
        Question::Operation { name, input } => {
            field(b"operation");
            field(name.as_bytes());
            field(input.as_bytes());
        }
    }
    hash.finalize().into()
}

/// Give the kept answer to `question`, or ask the plugin through `evaluate`
/// and keep a completed answer.
pub(super) fn answer(
    snapshot: &SourceSnapshot,
    entry: &str,
    question: Question<'_>,
    evaluate: impl FnOnce() -> Result<String, OperationFailure>,
) -> Result<String, OperationFailure> {
    if !question.reused() {
        return evaluate();
    }
    let key = digest(snapshot, entry, &question);
    let kept = answers()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .kept
        .get(&key)
        .cloned();
    match (kept, &question) {
        (Some(Kept::Answer(answer)), _) => return Ok(answer),
        (Some(Kept::Unimplemented), Question::Operation { name, .. }) => {
            return Err(OperationFailure::Unimplemented {
                operation: (*name).into(),
            })
        }
        _ => {}
    }
    let outcome = evaluate();
    let kept = match &outcome {
        Ok(answer) => Kept::Answer(answer.clone()),
        Err(OperationFailure::Unimplemented { .. }) => Kept::Unimplemented,
        Err(_) => return outcome,
    };
    let size = match &kept {
        Kept::Answer(answer) => answer.len(),
        Kept::Unimplemented => 0,
    };
    let mut answers = answers()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if answers.kept.len() >= MAX_ANSWERS || answers.bytes + size > MAX_ANSWER_BYTES {
        *answers = Answers::default();
    }
    if answers.kept.insert(key, kept).is_none() {
        answers.bytes += size;
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn source(text: &str) -> SourceSnapshot {
        SourceSnapshot::plugin(text).unwrap()
    }

    #[test]
    fn same_source_operation_and_input_is_answered_once() {
        let snapshot = source("export const name = 'reuse-same';");
        let runs = Cell::new(0);
        let ask = |input| {
            answer(
                &snapshot,
                "plugin.ts",
                Question::Operation {
                    name: super::super::SETTINGS_VALIDATE,
                    input,
                },
                || {
                    runs.set(runs.get() + 1);
                    Ok(format!("answer {}", runs.get()))
                },
            )
            .unwrap()
        };
        assert_eq!(ask("{\"values\":{}}"), "answer 1");
        assert_eq!(ask("{\"values\":{}}"), "answer 1");
        assert_eq!(ask("{\"values\":{\"x\":1}}"), "answer 2");
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn changed_source_is_a_new_question() {
        let runs = Cell::new(0);
        let ask = |text| {
            answer(&source(text), "plugin.ts", Question::Declaration, || {
                runs.set(runs.get() + 1);
                Ok(text.to_owned())
            })
            .unwrap()
        };
        let first = "export const name = 'reuse-source-a';";
        let second = "export const name = 'reuse-source-b';";
        assert_eq!(ask(first), first);
        assert_eq!(ask(second), second);
        assert_eq!(ask(first), first);
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn failures_run_again_and_unimplemented_is_kept() {
        let snapshot = source("export const name = 'reuse-failures';");
        let runs = Cell::new(0);
        let ask = |name, failure: fn(&str) -> OperationFailure| {
            answer(
                &snapshot,
                "plugin.ts",
                Question::Operation { name, input: "{}" },
                || {
                    runs.set(runs.get() + 1);
                    Err(failure(name))
                },
            )
        };
        let deadline = |_: &str| OperationFailure::Failed("plugin exceeded its deadline".into());
        let missing = |name: &str| OperationFailure::Unimplemented {
            operation: name.into(),
        };
        assert!(ask(super::super::LAUNCH_PREPARE, deadline).is_err());
        assert!(ask(super::super::LAUNCH_PREPARE, deadline).is_err());
        assert_eq!(runs.get(), 2);
        assert!(ask(super::super::RUNTIME_RESOLVE, missing)
            .unwrap_err()
            .is_unimplemented());
        assert!(ask(super::super::RUNTIME_RESOLVE, missing)
            .unwrap_err()
            .is_unimplemented());
        assert_eq!(runs.get(), 3);
    }

    #[test]
    fn discovery_answers_are_not_kept() {
        let snapshot = source("export const name = 'reuse-discovery';");
        let runs = Cell::new(0);
        for _ in 0..2 {
            answer(
                &snapshot,
                "plugin.ts",
                Question::Operation {
                    name: "discovery.discover",
                    input: "{}",
                },
                || {
                    runs.set(runs.get() + 1);
                    Ok(String::new())
                },
            )
            .unwrap();
        }
        assert_eq!(runs.get(), 2);
    }
}
