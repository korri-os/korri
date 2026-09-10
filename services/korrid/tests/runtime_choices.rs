#[path = "fixtures/readable.rs"]
mod readable;
use korrid::config::settings::{self, RuntimeChoiceScope, SettingsError};
use std::{fs, sync::Mutex};

#[test]
fn runtime_choices_use_existing_records_preserve_fields_and_reject_stale_writes() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    readable::combined(root.path());
    let lock = Mutex::new(());
    let revisions = settings::runtime_choice_revisions(root.path()).unwrap();
    settings::set_runtime_choice(
        root.path(),
        private.path(),
        &lock,
        &revisions.device,
        &RuntimeChoiceScope::System("gba".into()),
        Some("@test:build/core"),
    )
    .unwrap();
    let before = fs::read(root.path().join("device.yaml")).unwrap();
    assert!(matches!(
        settings::set_runtime_choice(
            root.path(),
            private.path(),
            &lock,
            &revisions.device,
            &RuntimeChoiceScope::System("gba".into()),
            Some("@test:other/core")
        ),
        Err(SettingsError::Conflict)
    ));
    assert_eq!(fs::read(root.path().join("device.yaml")).unwrap(), before);
    settings::set_runtime_choice(
        root.path(),
        private.path(),
        &lock,
        &revisions.games,
        &RuntimeChoiceScope::Game(readable::GBA_ID.into()),
        Some("@missing:build/core"),
    )
    .unwrap();
    let state = korrid::config::snapshot::ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(state.diagnostic.is_none(), "{:?}", state.diagnostic);
    assert_eq!(
        state.snapshot.systems["gba"].runtime.as_ref().unwrap().0,
        "@test:build/core"
    );
    let game = &state.snapshot.games[readable::GBA_ID];
    assert_eq!(game.title, "Wario Land 4");
    assert_eq!(game.runtime.as_ref().unwrap().0, "@missing:build/core");
    assert_eq!(game.releases.len(), 1);
    let revisions = settings::runtime_choice_revisions(root.path()).unwrap();
    settings::set_runtime_choice(
        root.path(),
        private.path(),
        &lock,
        &revisions.games,
        &RuntimeChoiceScope::Game(readable::GBA_ID.into()),
        None,
    )
    .unwrap();
    let state = korrid::config::snapshot::ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(state.snapshot.games[readable::GBA_ID].runtime.is_none());
    assert!(state.snapshot.systems["gba"].runtime.is_some());
    assert!(matches!(
        settings::set_runtime_choice(
            root.path(),
            private.path(),
            &lock,
            &revisions.device,
            &RuntimeChoiceScope::Game("missing".into()),
            Some("not-a-runtime")
        ),
        Err(SettingsError::Invalid(_))
    ));
}
