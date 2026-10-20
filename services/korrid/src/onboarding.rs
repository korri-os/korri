use crate::{config::settings::ONBOARDING_FINISHED_FILE_NAME, RpcFailure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
};
use typeshare::typeshare;

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OnboardingStatusRequest {}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OnboardingFinishRequest {}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OnboardingRestartRequest {}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingStatus {
    pub needed: bool,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum OnboardingStatusOutcome {
    Ok(OnboardingStatus),
    Err(RpcFailure),
}

fn failure() -> RpcFailure {
    RpcFailure {
        code: "OnboardingStorageUnavailable".into(),
        message: "Could not read or save setup progress. Try again.".into(),
    }
}
pub fn status(root: &Path) -> OnboardingStatusOutcome {
    match fs::symlink_metadata(root.join(ONBOARDING_FINISHED_FILE_NAME)) {
        Ok(metadata) if metadata.is_file() && metadata.len() == 0 => {
            OnboardingStatusOutcome::Ok(OnboardingStatus { needed: false })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            OnboardingStatusOutcome::Ok(OnboardingStatus { needed: true })
        }
        _ => OnboardingStatusOutcome::Err(failure()),
    }
}
pub fn finish(root: &Path) -> OnboardingStatusOutcome {
    let temporary = root.join(format!(
        ".{ONBOARDING_FINISHED_FILE_NAME}.{:016x}.tmp",
        rand::random::<u64>()
    ));
    let result = (|| -> std::io::Result<()> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)?;
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.sync_all()?;
        fs::rename(&temporary, root.join(ONBOARDING_FINISHED_FILE_NAME))?;
        File::open(root)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return OnboardingStatusOutcome::Err(failure());
    }
    status(root)
}
/// Put the device back into first-boot setup. Removes only the marker; every
/// other setting, profile and identity stays. Idempotent: a device already in
/// setup stays there.
pub fn restart(root: &Path) -> OnboardingStatusOutcome {
    let result = (|| -> std::io::Result<()> {
        match fs::remove_file(root.join(ONBOARDING_FINISHED_FILE_NAME)) {
            Ok(()) => File::open(root)?.sync_all(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    })();
    if result.is_err() {
        return OnboardingStatusOutcome::Err(failure());
    }
    status(root)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_an_empty_marker_survives_restart_and_finish_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("private");
        assert!(matches!(
            status(&root),
            OnboardingStatusOutcome::Ok(OnboardingStatus { needed: true })
        ));
        for _ in 0..2 {
            assert!(matches!(
                finish(&root),
                OnboardingStatusOutcome::Ok(OnboardingStatus { needed: false })
            ));
        }
        assert!(matches!(
            status(&root),
            OnboardingStatusOutcome::Ok(OnboardingStatus { needed: false })
        ));
        let entries: Vec<_> = fs::read_dir(&root).unwrap().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            fs::read(root.join(ONBOARDING_FINISHED_FILE_NAME)).unwrap(),
            b""
        );
    }
    #[test]
    fn restart_removes_the_marker_and_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("private");
        // Before any marker exists: restart is a no-op that still reports setup.
        assert!(matches!(
            restart(&root),
            OnboardingStatusOutcome::Ok(OnboardingStatus { needed: true })
        ));
        finish(&root);
        for _ in 0..2 {
            assert!(matches!(
                restart(&root),
                OnboardingStatusOutcome::Ok(OnboardingStatus { needed: true })
            ));
        }
        assert!(matches!(
            status(&root),
            OnboardingStatusOutcome::Ok(OnboardingStatus { needed: true })
        ));
        assert!(fs::read_dir(&root).unwrap().next().is_none());
        // Setup can be finished again afterwards.
        assert!(matches!(
            finish(&root),
            OnboardingStatusOutcome::Ok(OnboardingStatus { needed: false })
        ));
    }
    #[test]
    fn restart_reports_a_marker_it_cannot_remove() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("private");
        finish(&root);
        // A directory where the marker file should be cannot be unlinked.
        fs::remove_file(root.join(ONBOARDING_FINISHED_FILE_NAME)).unwrap();
        fs::create_dir(root.join(ONBOARDING_FINISHED_FILE_NAME)).unwrap();
        assert!(matches!(restart(&root), OnboardingStatusOutcome::Err(_)));
    }
    #[test]
    fn storage_failure_does_not_report_finished() {
        let root = tempfile::NamedTempFile::new().unwrap();
        assert!(matches!(
            finish(root.path()),
            OnboardingStatusOutcome::Err(_)
        ));
    }
}
