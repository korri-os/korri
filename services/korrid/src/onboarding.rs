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
    fn storage_failure_does_not_report_finished() {
        let root = tempfile::NamedTempFile::new().unwrap();
        assert!(matches!(
            finish(root.path()),
            OnboardingStatusOutcome::Err(_)
        ));
    }
}
