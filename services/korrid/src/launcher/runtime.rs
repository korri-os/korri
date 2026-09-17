//! Whether a runner can start this target now, asked before anything runs.
//!
//! A runner owns the runtime it needs. korrid holds no table of runtimes, so it
//! cannot know that a Proton closure is absent, that a FEX rootfs is missing,
//! or that a thunk set is unavailable on this device. `runtime.resolve` lets
//! the runner say so, and the launch path refuses before it spawns anything.
//!
//! This is not `launch.compose`. A modifier changes the plan; this operation
//! answers a question and changes nothing. The runner still performs no effect
//! and reaches for no host service.
//!
//! The request is the existing launch treaty, [`PluginLaunchInput`]. Both
//! operations ask about the same runner and the same target, so the host builds
//! that shape once and asks twice. A second near-identical request type would
//! be a second treaty to keep in step.
//!
//! The result is deliberately smaller than the design draft. The draft also
//! carries selected immutable references and a diagnostic list. Neither has a
//! consumer: `launch.prepare` already owns the plan that uses those references,
//! and the refusal message is what reaches the person. See
//! `docs/briefs/2026-09-15-plugin-model/OPERATIONS.md`, the `runtime.resolve`
//! row, for the two fields this keeps.
use super::plugin_launch::PluginLaunchInput;
use crate::script::{self, OperationFailure};

/// What the runner answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeReadiness {
    /// The runner implements no `runtime.resolve` handler, so it declares no
    /// runtime of its own. The launch proceeds exactly as it did before this
    /// operation existed. This is the common case for a runner whose runtime is
    /// its own package.
    NotDeclared,
    /// The runner named its runtime and has it.
    Ready,
    /// The runner cannot start this target. `missing` is what it needs, in the
    /// runner's own words.
    NotReady { missing: Vec<String> },
}

impl RuntimeReadiness {
    /// Why the launch must stop, when it must.
    ///
    /// `None` means the launch may proceed. A runner that answers "not ready"
    /// without a reason still stops the launch: an unexplained refusal must
    /// never read as permission.
    pub fn refusal(&self) -> Option<String> {
        match self {
            Self::NotDeclared | Self::Ready => None,
            Self::NotReady { missing } if missing.is_empty() => {
                Some("the runner cannot start this target".to_owned())
            }
            Self::NotReady { missing } => Some(missing.join("; ")),
        }
    }
}

/// The runner's answer on the wire. Unknown fields are refused: a runner that
/// still returns a superseded field hears about it here, not silently.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeResolveResult {
    ready: bool,
    #[serde(default)]
    missing: Vec<String>,
}

/// Ask the runner whether it can start this target.
///
/// A runner with no handler declares no runtime and is not an error. Any other
/// failure is: korrid will not guess that a runner is ready when it did not
/// answer.
pub fn resolve(
    snapshot: &script::source::SourceSnapshot,
    input: &PluginLaunchInput,
) -> Result<RuntimeReadiness, String> {
    let input = serde_json::to_string(input).map_err(|error| error.to_string())?;
    let result =
        match script::call_plugin_operation_snapshot(snapshot, script::RUNTIME_RESOLVE, &input) {
            Ok(result) => result,
            Err(OperationFailure::Unimplemented { .. }) => {
                return Ok(RuntimeReadiness::NotDeclared)
            }
            Err(failure) => return Err(failure.to_string()),
        };
    let answer: RuntimeResolveResult = serde_json::from_str(&result)
        .map_err(|error| format!("invalid runtime.resolve result: {error}"))?;
    Ok(if answer.ready {
        RuntimeReadiness::Ready
    } else {
        RuntimeReadiness::NotReady {
            missing: answer.missing,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn input() -> PluginLaunchInput {
        PluginLaunchInput {
            runner_id: "@test:runtime/runner".into(),
            family_id: None,
            program: "runner".into(),
            core_path: None,
            content_path: "/games/target.rom".into(),
            account_root: "/users/default".into(),
            files: HashMap::new(),
            overrides: None,
        }
    }

    fn resolve_source(source: &str) -> Result<RuntimeReadiness, String> {
        let snapshot = script::source::SourceSnapshot::plugin(source).unwrap();
        resolve(&snapshot, &input())
    }

    #[test]
    fn a_runner_with_no_handler_declares_no_runtime() {
        let source = "export const name = 'runner'; export const handlers = {'launch.prepare': () => ({command: 'runner', args: []})}";
        assert_eq!(
            resolve_source(source).unwrap(),
            RuntimeReadiness::NotDeclared
        );
        assert_eq!(resolve_source(source).unwrap().refusal(), None);
    }

    #[test]
    fn a_ready_runner_lets_the_launch_proceed() {
        let source = "export const name = 'runner'; export const handlers = {'runtime.resolve': () => ({ready: true})}";
        assert_eq!(resolve_source(source).unwrap(), RuntimeReadiness::Ready);
        assert_eq!(resolve_source(source).unwrap().refusal(), None);
    }

    #[test]
    fn a_runner_names_what_is_missing_and_that_refuses_the_launch() {
        let source = "export const name = 'runner'; export const handlers = {'runtime.resolve': () => ({ready: false, missing: ['Proton 10 is not installed', 'the wine prefix is absent']})}";
        assert_eq!(
            resolve_source(source).unwrap(),
            RuntimeReadiness::NotReady {
                missing: vec![
                    "Proton 10 is not installed".to_owned(),
                    "the wine prefix is absent".to_owned(),
                ],
            }
        );
        assert_eq!(
            resolve_source(source).unwrap().refusal().as_deref(),
            Some("Proton 10 is not installed; the wine prefix is absent")
        );
    }

    #[test]
    fn an_unexplained_refusal_still_stops_the_launch() {
        let source = "export const name = 'runner'; export const handlers = {'runtime.resolve': () => ({ready: false})}";
        assert_eq!(
            resolve_source(source).unwrap(),
            RuntimeReadiness::NotReady { missing: vec![] }
        );
        assert_eq!(
            resolve_source(source).unwrap().refusal().as_deref(),
            Some("the runner cannot start this target")
        );
    }

    #[test]
    fn the_runner_receives_the_launch_facts() {
        let source = "export const name = 'runner'; export const handlers = {'runtime.resolve': input => ({ready: false, missing: [input.runnerId, input.contentPath]})}";
        assert_eq!(
            resolve_source(source).unwrap(),
            RuntimeReadiness::NotReady {
                missing: vec![
                    "@test:runtime/runner".to_owned(),
                    "/games/target.rom".to_owned(),
                ],
            }
        );
    }

    #[test]
    fn a_malformed_answer_is_an_error_and_never_reads_as_ready() {
        for source in [
            "export const name = 'runner'; export const handlers = {'runtime.resolve': () => ({ready: 'yes'})}",
            "export const name = 'runner'; export const handlers = {'runtime.resolve': () => ({ready: true, runtime: 'linux-user'})}",
            "export const name = 'runner'; export const handlers = {'runtime.resolve': () => 42}",
            "export const name = 'runner'; export const handlers = {'runtime.resolve': () => { throw new Error('no runtime table') }}",
        ] {
            assert!(
                resolve_source(source).is_err(),
                "a malformed answer must be an error: {source}"
            );
        }
    }
}
