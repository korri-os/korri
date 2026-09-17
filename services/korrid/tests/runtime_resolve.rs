//! The runner owns its runtime, so the launch path asks it before it spawns
//! anything. These tests drive the real path: a real plugin package, a real
//! route, and the real `launch_route` call.
//!
//! Only the `handlers` block differs between the cases. The declarations stay
//! identical, so a difference in outcome is a difference in the answer.
#[path = "fixtures/readable.rs"]
mod readable;

use korrid::{
    config::{resolver::resolve_linux_route, snapshot::ConfigSnapshotCoordinator},
    game_routes::{selected_launch, SelectedGameLaunchRequest},
    launcher::{linux_plugin::launch_route, LaunchError},
    plugin::PluginRegistry,
    plugin_installation::EnabledPackage,
};
use std::{collections::BTreeMap, fs, path::Path};

const RUNNER_ID: &str = "@korri:mgba/mgba";

/// A native runner for GBA content. A native runner must offer
/// `launch.prepare`; `runtime.resolve` is the only variable.
fn plugin_source(runtime_resolve: Option<&str>) -> String {
    let resolve = match runtime_resolve {
        Some(handler) => format!(r#""runtime.resolve": {handler},"#),
        None => String::new(),
    };
    format!(
        r#"
export const name = "mgba"
export const systems = {{ gba: {{ id: "gba", title: "Game Boy Advance" }} }}
export const runners = {{ mgba: {{ id: "{RUNNER_ID}", program: "retroarch", core: "mgba", systems: ["gba"] }} }}
export const discovery = {{ fileReleases: {{ "gba-files": {{ id: "@korri:mgba/gba-files", title: "Game Boy Advance files", extensions: ["gba"], system: "gba", runners: ["{RUNNER_ID}"] }} }} }}
export const handlers = {{
  "launch.prepare": () => ({{ command: "retroarch", args: [] }}),
  {resolve}
}}
"#
    )
}

fn installed(root: &Path, runtime_resolve: Option<&str>) -> PluginRegistry {
    let package = root.join("plugin-mgba");
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("plugin.ts"), plugin_source(runtime_resolve)).unwrap();
    PluginRegistry::from_installed(vec![EnabledPackage {
        id: "@korri:mgba".into(),
        package,
        files: BTreeMap::from([
            ("retroarch".into(), root.join("retroarch")),
            ("mgba".into(), root.join("mgba.so")),
        ]),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
    }])
    .unwrap()
}

fn setup(
    root: &Path,
    runtime_resolve: Option<&str>,
) -> (korrid::config::ConfigSnapshot, PluginRegistry) {
    readable::combined(root);
    fs::create_dir_all(root.join("roms")).unwrap();
    fs::write(root.join("roms/wl4.gba"), b"rom").unwrap();
    let snapshot = (*ConfigSnapshotCoordinator::new(root).reload().snapshot).clone();
    (snapshot, installed(root, runtime_resolve))
}

fn launch(
    root: &Path,
    runtime_resolve: Option<&str>,
) -> Result<korrid::launcher::linux_plugin::LinuxLaunchSpec, LaunchError> {
    let (snapshot, registry) = setup(root, runtime_resolve);
    let route = resolve_linux_route(
        root,
        &snapshot,
        &registry,
        readable::GBA_ID,
        Some(RUNNER_ID),
    )
    .unwrap();
    launch_route(root, &snapshot, &registry, &route, None)
}

#[test]
fn a_runner_that_names_a_missing_runtime_refuses_the_launch() {
    let root = tempfile::tempdir().unwrap();
    let outcome = launch(
        root.path(),
        Some(
            r#"() => ({ready: false, missing: ["Proton 10 is not installed", "the wine prefix is absent"]})"#,
        ),
    );
    let Err(LaunchError::RouteUnavailable(message)) = outcome else {
        panic!("a missing runtime must refuse the launch, got {outcome:?}");
    };
    assert_eq!(
        message,
        "Proton 10 is not installed; the wine prefix is absent"
    );
}

#[test]
fn a_ready_runner_and_a_silent_runner_both_launch() {
    for runtime_resolve in [
        // The runner named its runtime and has it.
        Some(r#"() => ({ready: true})"#),
        // The runner declares no runtime of its own, which is not an error.
        None,
    ] {
        let root = tempfile::tempdir().unwrap();
        let spec = launch(root.path(), runtime_resolve)
            .unwrap_or_else(|error| panic!("{runtime_resolve:?} must launch: {error}"));
        assert_eq!(spec.command[1], "plugin-launch");
    }
}

#[test]
fn the_runner_answers_about_the_target_it_was_asked_about() {
    let root = tempfile::tempdir().unwrap();
    // Echo the request back through the refusal, so the test proves the runner
    // received the launch treaty and not an empty object.
    let outcome = launch(
        root.path(),
        Some(
            r#"input => ({ready: false, missing: [input.runnerId, input.contentPath, input.program]})"#,
        ),
    );
    let Err(LaunchError::RouteUnavailable(message)) = outcome else {
        panic!("expected a refusal, got {outcome:?}");
    };
    let parts: Vec<&str> = message.split("; ").collect();
    assert_eq!(parts[0], RUNNER_ID);
    assert_eq!(
        parts[1],
        root.path().join("roms/wl4.gba").display().to_string()
    );
    assert_eq!(
        parts[2],
        root.path().join("retroarch").display().to_string()
    );
}

#[test]
fn an_unexplained_refusal_never_reads_as_permission() {
    let root = tempfile::tempdir().unwrap();
    let outcome = launch(root.path(), Some(r#"() => ({ready: false})"#));
    let Err(LaunchError::RouteUnavailable(message)) = outcome else {
        panic!("an unexplained refusal must still refuse, got {outcome:?}");
    };
    assert_eq!(message, "the runner cannot start this target");
}

#[test]
fn a_failing_runner_is_an_error_and_never_a_silent_ready() {
    let root = tempfile::tempdir().unwrap();
    // A handler that throws is a failure, never an empty success. The thrown
    // text is not carried: reporting it is the return shape's job, through
    // `missing`, and korrid does not read a failure as permission.
    let outcome = launch(
        root.path(),
        Some(r#"() => { throw new Error("no runtime table for this device") }"#),
    );
    let Err(LaunchError::RouteUnavailable(message)) = outcome else {
        panic!("a throwing runner must not read as ready, got {outcome:?}");
    };
    assert!(message.contains("runtime.resolve"), "{message}");
}

#[test]
fn the_person_sees_the_refusal_through_the_existing_route_error() {
    let root = tempfile::tempdir().unwrap();
    let (_, registry) = setup(
        root.path(),
        Some(r#"() => ({ready: false, missing: ["the FEX rootfs is missing"]})"#),
    );
    let failure = selected_launch(
        root.path(),
        &registry,
        &SelectedGameLaunchRequest {
            game_id: readable::GBA_ID.into(),
            runner_id: RUNNER_ID.into(),
            overrides: None,
        },
    )
    .expect_err("the launch must fail");
    assert_eq!(failure.code, "LocalRouteUnavailable");
    assert_eq!(
        failure.message,
        "local route is unavailable: the FEX rootfs is missing"
    );

    // Listing the routes for the game reports the same refusal. A route korrid
    // cannot start is not offered as a choice.
    let failure = korrid::game_routes::list(root.path(), &registry, readable::GBA_ID)
        .expect_err("an unstartable route must not be listed");
    assert_eq!(failure.code, "LocalRouteUnavailable");
    assert_eq!(
        failure.message,
        "local route is unavailable: the FEX rootfs is missing"
    );
}
