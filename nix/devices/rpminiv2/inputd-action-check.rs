use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

use korri_inputd::action_catalog::{
    commands_from_environment, ActionCommand, ActionConfigError, ActionId,
};

// This is the JSON emitted by korri-input.nix, not a second action config.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EmittedAction {
    executable: PathBuf,
    argv: Vec<String>,
    environment: BTreeMap<String, String>,
}

impl EmittedAction {
    fn command(&self, executable: PathBuf) -> Result<ActionCommand, ActionConfigError> {
        ActionCommand::new(
            executable,
            self.argv.iter().map(OsString::from),
            self.environment
                .iter()
                .map(|(name, value)| (OsString::from(name), OsString::from(value)))
                .collect(),
        )
    }
}

fn emitted_action() -> (String, EmittedAction) {
    let path = std::env::var_os("KORRI_TEST_CONTROLLER_ACTIVITY_FILE").unwrap();
    let raw = std::fs::read_to_string(path).unwrap();
    let action = serde_json::from_str(&raw).unwrap();
    (raw, action)
}

#[test]
fn emitted_controller_activity_passes_startup_validation() {
    let (raw, action) = emitted_action();
    let command = action
        .command(action.executable.clone())
        .unwrap_or_else(|error| panic!("emitted controller-activity: {error}"));
    assert_eq!(
        command.argv(),
        [
            OsString::from("-s"),
            OsString::from("/run/korri-compositor/sway-ipc.sock"),
            OsString::from("seat * idle_notify"),
        ]
    );
    assert!(command.environment().is_empty());

    // Exercise the production environment parser as well as the constructor.
    let environment = BTreeMap::from([(
        OsString::from("KORRI_INPUTD_CONTROLLER_ACTIVITY"),
        OsString::from(raw),
    )]);
    let (commands, _) = commands_from_environment(&environment).unwrap();
    assert_eq!(commands.get(ActionId::ControllerActivity), Some(&command));
    println!(
        "accepted emitted executable: {}",
        command.executable().display()
    );
}

#[test]
fn wrapped_cross_store_symlink_remains_rejected() {
    let (_, action) = emitted_action();
    let wrapped = PathBuf::from(std::env::var_os("KORRI_TEST_WRAPPED_SWAYMSG").unwrap());
    // Fail if the fixture is missing or ceases to be the real cross-store link.
    assert!(std::fs::symlink_metadata(&wrapped).unwrap().is_symlink());
    let canonical = std::fs::canonicalize(&wrapped).unwrap();
    assert_ne!(wrapped.components().nth(3), canonical.components().nth(3));
    let error = action.command(wrapped.clone()).unwrap_err();
    assert_eq!(error, ActionConfigError::ExecutableNotImmutable(wrapped));
    println!(
        "rejected wrapped executable: {error}; resolves to {}",
        canonical.display()
    );
}
