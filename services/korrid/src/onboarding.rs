use crate::{
    config::settings::ONBOARDING_FINISHED_FILE_NAME,
    identity::{DeviceIdentity, IdentityState, OwnerStatementStatus},
    local_signer::{ConfirmedWords, SetupWords},
    remote_signer::{PersonSignerRequest, PersonSignerState},
    RpcFailure,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
};
use typeshare::typeshare;
use zeroize::Zeroize;

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OnboardingStatusRequest {}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OnboardingFinishRequest {}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OnboardingRestartRequest {}

/// Make new recovery words for a device in setup (ADRs 0072 and 0082).
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingRecoveryWordsRequest {}

/// The words a device in setup shows, and the places of the words the
/// person types back. korrid carries them in memory only; Debug never shows
/// the words.
#[typeshare]
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryWordsShown {
    /// The 12 words, lowercase, one space between words. The recovery QR
    /// code holds exactly this text.
    pub words: String,
    /// The places, from 1 to 12, of the 3 words to type back.
    pub confirm_positions: Vec<u8>,
}
impl std::fmt::Debug for RecoveryWordsShown {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecoveryWordsShown")
            .field("confirm_positions", &self.confirm_positions)
            .finish_non_exhaustive()
    }
}
impl Drop for RecoveryWordsShown {
    fn drop(&mut self) {
        self.words.zeroize();
    }
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum RecoveryWordsOutcome {
    Ok(RecoveryWordsShown),
    Err(RpcFailure),
}

/// The person's answers: the words at the places that
/// `RecoveryWordsShown.confirmPositions` names, in that order. When they
/// match, the device gets its automatic identity and setup ends.
#[typeshare]
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingConfirmWordsRequest {
    pub words: Vec<String>,
}
impl std::fmt::Debug for OnboardingConfirmWordsRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OnboardingConfirmWordsRequest")
            .finish_non_exhaustive()
    }
}
impl Drop for OnboardingConfirmWordsRequest {
    fn drop(&mut self) {
        self.words.iter_mut().for_each(Zeroize::zeroize);
    }
}
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

/// Whether setup has ended on this device. A marker Korri cannot read
/// counts as setup still running.
pub fn setup_finished(root: &Path) -> bool {
    matches!(
        status(root),
        OnboardingStatusOutcome::Ok(OnboardingStatus { needed: false })
    )
}

fn fail(code: &str, message: &str) -> RpcFailure {
    RpcFailure {
        code: code.into(),
        message: message.into(),
    }
}

fn identity_state(root: &Path) -> Result<IdentityState, RpcFailure> {
    DeviceIdentity::load_or_create(root)
        .map(|identity| identity.state().clone())
        .map_err(|_| fail("IdentityUnavailable", "Device identity is unavailable."))
}

/// End setup on a device that has an owner: it joined during setup (ADR
/// 0022). A device without an owner ends setup only when the person
/// confirms its recovery words (ADR 0082, `confirm_words`). Any words that
/// still wait in the local signer are erased.
pub async fn finish_with_owner<S>(root: &Path, signer: &S) -> OnboardingStatusOutcome
where
    S: SetupWords + ?Sized,
{
    let _owner_write = crate::identity::owner_write().await;
    match identity_state(root) {
        Ok(IdentityState::Owned { .. }) => {
            let _ = signer.forget_words().await;
            finish(root)
        }
        Ok(IdentityState::Unowned { .. }) => OnboardingStatusOutcome::Err(fail(
            "RecoveryWordsNeeded",
            "Setup ends when the person confirms this device's recovery words. Run korri setup finish on this device.",
        )),
        Ok(_) => OnboardingStatusOutcome::Err(fail(
            "IdentityUnavailable",
            "This device's identity cannot end setup.",
        )),
        Err(failure) => OnboardingStatusOutcome::Err(failure),
    }
}

/// New recovery words for a device in setup that has no owner. They wait
/// in the local signer until the person confirms them; new words replace
/// them.
pub async fn recovery_words<S>(root: &Path, signer: &S) -> RecoveryWordsOutcome
where
    S: SetupWords + ?Sized,
{
    let state = match identity_state(root) {
        Ok(state) => state,
        Err(failure) => return RecoveryWordsOutcome::Err(failure),
    };
    if setup_finished(root) || !matches!(state, IdentityState::Unowned { .. }) {
        return RecoveryWordsOutcome::Err(fail(
            "RecoveryWordsNotNeeded",
            "This device already has an owner. It needs no new recovery words.",
        ));
    }
    match signer.make_words().await {
        Ok(shown) => RecoveryWordsOutcome::Ok(RecoveryWordsShown {
            words: shown.words.as_str().to_owned(),
            confirm_positions: shown
                .positions
                .iter()
                .map(|position| *position as u8)
                .collect(),
        }),
        Err(_) => RecoveryWordsOutcome::Err(fail(
            "RecoveryWordsUnavailable",
            "Korri could not make recovery words. Try again.",
        )),
    }
}

/// Check the person's answers in the local signer. When they match, the
/// signer signs this device's owner statement with the words' person key
/// and erases the words. The device stores the statement, so it has its
/// automatic identity, and setup ends (ADRs 0022 and 0082). The second value
/// says whether an owner was bound, so the caller restarts korrid with it.
pub async fn confirm_words<S>(
    root: &Path,
    signer: &S,
    answers: &[String],
    now: u64,
) -> (OnboardingStatusOutcome, bool)
where
    S: SetupWords + ?Sized,
{
    let failed = |code, message| (OnboardingStatusOutcome::Err(fail(code, message)), false);
    // A join may be storing its vouch: read the identity under the same lock.
    let _owner_write = crate::identity::owner_write().await;
    let mut identity = match DeviceIdentity::load_or_create(root) {
        Ok(identity) => identity,
        Err(_) => return failed("IdentityUnavailable", "Device identity is unavailable."),
    };
    if setup_finished(root) || !matches!(identity.state(), IdentityState::Unowned { .. }) {
        let _ = signer.forget_words().await;
        return failed(
            "RecoveryWordsNotNeeded",
            "This device got an owner during setup. Throw away these words: they belong to no device.",
        );
    }
    let Ok(template) = identity.owner_statement_template(OwnerStatementStatus::Owned, now) else {
        return failed("IdentityUnavailable", "Device identity is unavailable.");
    };
    let request = PersonSignerRequest {
        unsigned_event_template: template.clone(),
    };
    let identity_failed = || {
        failed(
            "AutomaticIdentityFailed",
            "Korri could not create this device's identity. Throw away these words and finish setup again.",
        )
    };
    match signer.confirm_words(answers, request).await {
        Ok(ConfirmedWords::Mismatch { message }) => (
            OnboardingStatusOutcome::Err(fail("RecoveryWordsMismatch", &message)),
            false,
        ),
        Ok(ConfirmedWords::Signed(PersonSignerState::Approved {
            owner_public_key,
            unsigned_event_template,
            signed_event_json,
        })) if unsigned_event_template == template => {
            if identity
                .apply_signed_owner_binding(&template, &owner_public_key, &signed_event_json)
                .is_err()
            {
                return identity_failed();
            }
            (finish(root), true)
        }
        Ok(ConfirmedWords::Signed(_)) => identity_failed(),
        Err(_) => failed(
            "RecoveryWordsUnavailable",
            "No recovery words wait for confirmation. Show new words and write them down.",
        ),
    }
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
        let unrelated = root.join("unrelated-test-data");
        fs::write(&unrelated, b"preserve these bytes").unwrap();
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
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        assert_eq!(fs::read(&unrelated).unwrap(), b"preserve these bytes");
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
