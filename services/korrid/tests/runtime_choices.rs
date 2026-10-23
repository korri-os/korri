#[path = "fixtures/readable.rs"]
mod readable;
use korrid::config::settings::{self, SettingsError};
use korrid::launch_settings::{
    self, LaunchLayer, LaunchPath, LaunchPlace, LaunchRunnerScope, LaunchSetRequest,
};
use std::{fs, sync::Mutex};

#[test]
fn runner_choices_use_existing_records_preserve_fields_and_reject_stale_writes() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    readable::combined(root.path());
    let lock = Mutex::new(());
    let revisions = settings::runner_choice_revisions(root.path()).unwrap();
    set_choice(
        root.path(),
        private.path(),
        &lock,
        &revisions.device,
        &LaunchLayer::System("gba".into()),
        Some("@test:build/core"),
    )
    .unwrap();
    let before = fs::read(root.path().join("device.yaml")).unwrap();
    assert!(matches!(
        set_choice(
            root.path(),
            private.path(),
            &lock,
            &revisions.device,
            &LaunchLayer::System("gba".into()),
            Some("@test:other/core")
        ),
        Err(SettingsError::Conflict)
    ));
    assert_eq!(fs::read(root.path().join("device.yaml")).unwrap(), before);
    set_choice(
        root.path(),
        private.path(),
        &lock,
        &revisions.games,
        &LaunchLayer::Game(readable::GBA_ID.into()),
        Some("@missing:build/core"),
    )
    .unwrap();
    let state = korrid::config::snapshot::ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(state.diagnostic.is_none(), "{:?}", state.diagnostic);
    assert_eq!(
        state.snapshot.systems["gba"].runner.as_ref().unwrap().0,
        "@test:build/core"
    );
    let game = &state.snapshot.library[readable::GBA_ID];
    assert_eq!(game.title.as_deref(), Some("Wario Land 4"));
    assert_eq!(game.runner.as_ref().unwrap().0, "@missing:build/core");
    assert_eq!(game.releases.len(), 1);
    let revisions = settings::runner_choice_revisions(root.path()).unwrap();
    set_choice(
        root.path(),
        private.path(),
        &lock,
        &revisions.games,
        &LaunchLayer::Game(readable::GBA_ID.into()),
        None,
    )
    .unwrap();
    let state = korrid::config::snapshot::ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(state.snapshot.library[readable::GBA_ID].runner.is_none());
    assert!(state.snapshot.systems["gba"].runner.is_some());
    assert!(matches!(
        set_choice(
            root.path(),
            private.path(),
            &lock,
            &revisions.device,
            &LaunchLayer::Game("missing".into()),
            Some("not-a-runner")
        ),
        Err(SettingsError::Invalid(_))
    ));
}

fn set_choice(
    root: &std::path::Path,
    private: &std::path::Path,
    lock: &Mutex<()>,
    revision: &str,
    layer: &LaunchLayer,
    runner: Option<&str>,
) -> Result<korrid::config::settings::RunnerChoiceRevisions, SettingsError> {
    launch_settings::set(
        root,
        private,
        lock,
        &LaunchSetRequest {
            place: LaunchPlace {
                layer: layer.clone(),
                scope: LaunchRunnerScope::Any,
            },
            path: LaunchPath::Runner,
            value: runner.map(|v| serde_json::json!(v)),
            expected_revision: revision.into(),
        },
        &launch_settings::helper::Client {
            socket: root.join("absent.sock"),
        },
    )
}
