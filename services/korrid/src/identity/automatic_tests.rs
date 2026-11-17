//! Setup's end and the automatic identity (ADRs 0022 and 0082): a device
//! with no owner ends setup only when the person confirms its recovery
//! words. The local signer runs in process here; `local_signer_tests.rs`
//! covers its socket.
use super::*;
use crate::{
    local_signer::LocalSigner,
    onboarding::{
        self, OnboardingStatus, OnboardingStatusOutcome, RecoveryWordsOutcome, RecoveryWordsShown,
    },
    recovery_words::RecoveryWords,
};

struct SetupDevice {
    root: tempfile::TempDir,
    signer: LocalSigner,
}

impl SetupDevice {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let device = DeviceIdentity::load_or_create(root.path())
            .unwrap()
            .device_public_key()
            .unwrap()
            .to_owned();
        Self {
            signer: LocalSigner::new(&device).unwrap(),
            root,
        }
    }

    fn path(&self) -> &std::path::Path {
        self.root.path()
    }

    fn state(&self) -> IdentityState {
        DeviceIdentity::load_or_create(self.path())
            .unwrap()
            .state()
            .clone()
    }

    async fn words(&self) -> RecoveryWordsShown {
        match onboarding::recovery_words(self.path(), &self.signer).await {
            RecoveryWordsOutcome::Ok(shown) => shown,
            RecoveryWordsOutcome::Err(failure) => panic!("no words: {failure:?}"),
        }
    }

    async fn confirm(&self, answers: Vec<String>) -> (OnboardingStatusOutcome, bool) {
        onboarding::confirm_words(self.path(), &self.signer, &answers, 100).await
    }

    /// Join another person's devices, as a stored vouch does.
    fn join(&self) -> String {
        let device = DeviceIdentity::load_or_create(self.path())
            .unwrap()
            .device_public_key()
            .unwrap()
            .to_owned();
        let voucher_root = tempfile::tempdir().unwrap();
        let mut voucher = DeviceIdentity::load_or_create(voucher_root.path()).unwrap();
        let person = Keys::generate();
        let statement = EventBuilder::new(Kind::Custom(OWNER_EVENT_KIND), "")
            .tags(
                owner_tags(
                    voucher.device_public_key().unwrap(),
                    OwnerStatementStatus::Owned,
                )
                .into_iter()
                .map(|tag| Tag::parse(tag).unwrap()),
            )
            .custom_created_at(Timestamp::from(50))
            .finalize(&person)
            .unwrap()
            .as_json();
        voucher.apply_owner_statement(&statement).unwrap();
        let vouch = voucher.vouch_for(&device, 60).unwrap();
        DeviceIdentity::load_or_create(self.path())
            .unwrap()
            .apply_owner_statement(&vouch.owner_statement)
            .unwrap();
        person.public_key().to_hex()
    }
}

/// What a person who wrote the words down types back.
fn answers(shown: &RecoveryWordsShown) -> Vec<String> {
    let words: Vec<&str> = shown.words.split(' ').collect();
    shown
        .confirm_positions
        .iter()
        .map(|position| words[usize::from(*position) - 1].to_owned())
        .collect()
}

fn failure(outcome: OnboardingStatusOutcome) -> crate::RpcFailure {
    match outcome {
        OnboardingStatusOutcome::Err(failure) => failure,
        outcome => panic!("expected a failure: {outcome:?}"),
    }
}

fn setup_needed(device: &SetupDevice) -> bool {
    matches!(
        onboarding::status(device.path()),
        OnboardingStatusOutcome::Ok(OnboardingStatus { needed: true })
    )
}

#[tokio::test]
async fn setup_ends_only_when_the_person_confirms_the_words() {
    let device = SetupDevice::new();
    let refused = failure(onboarding::finish_with_owner(device.path(), &device.signer).await);
    assert_eq!(refused.code, "RecoveryWordsNeeded");
    assert!(setup_needed(&device));

    let shown = device.words().await;
    assert_eq!(shown.words.split(' ').count(), 12);
    assert_eq!(shown.confirm_positions.len(), 3);
    assert!(matches!(device.state(), IdentityState::Unowned { .. }));
    let person = RecoveryWords::parse(&shown.words)
        .unwrap()
        .person_keys()
        .unwrap()
        .public_key()
        .to_hex();

    let mut wrong = answers(&shown);
    wrong[2] = "zoo".into();
    let (outcome, bound) = device.confirm(wrong).await;
    assert!(!bound);
    assert_eq!(failure(outcome).code, "RecoveryWordsMismatch");
    assert!(matches!(device.state(), IdentityState::Unowned { .. }));
    assert!(setup_needed(&device));

    let (outcome, bound) = device.confirm(answers(&shown)).await;
    assert!(bound);
    assert!(matches!(
        outcome,
        OnboardingStatusOutcome::Ok(OnboardingStatus { needed: false })
    ));
    let IdentityState::Owned {
        owner_public_key,
        created_at,
        ..
    } = device.state()
    else {
        panic!("confirmed words did not bind the automatic identity")
    };
    assert_eq!(owner_public_key, person);
    assert_eq!(created_at, 100);
    assert!(!device.signer.holds_words(), "the person key was erased");

    // Setup has ended: no new words, and the same owner after a restart.
    let RecoveryWordsOutcome::Err(again) =
        onboarding::recovery_words(device.path(), &device.signer).await
    else {
        panic!("a device whose setup ended made new words")
    };
    assert_eq!(again.code, "RecoveryWordsNotNeeded");
    assert!(matches!(
        device.state(),
        IdentityState::Owned { owner_public_key: ref owner, .. } if *owner == person
    ));
}

#[tokio::test]
async fn new_words_replace_words_that_were_never_confirmed() {
    let device = SetupDevice::new();
    let first = device.words().await;
    let second = device.words().await;
    assert_ne!(first.words, second.words);
    let (outcome, bound) = device.confirm(answers(&second)).await;
    assert!(bound, "{outcome:?}");
    let IdentityState::Owned {
        owner_public_key, ..
    } = device.state()
    else {
        panic!("no owner")
    };
    let second_person = RecoveryWords::parse(&second.words)
        .unwrap()
        .person_keys()
        .unwrap()
        .public_key()
        .to_hex();
    assert_eq!(owner_public_key, second_person);
}

#[tokio::test]
async fn confirming_with_no_words_waiting_changes_nothing() {
    let device = SetupDevice::new();
    let (outcome, bound) = device
        .confirm(vec!["abandon".into(), "ability".into(), "able".into()])
        .await;
    assert!(!bound);
    assert_eq!(failure(outcome).code, "RecoveryWordsUnavailable");
    assert!(matches!(device.state(), IdentityState::Unowned { .. }));
    assert!(setup_needed(&device));
}

#[tokio::test]
async fn a_device_that_joined_during_setup_needs_no_words() {
    let device = SetupDevice::new();
    let shown = device.words().await;
    let person = device.join();

    // Words shown before the join own nothing, and their key is erased.
    let (outcome, bound) = device.confirm(answers(&shown)).await;
    assert!(!bound);
    assert_eq!(failure(outcome).code, "RecoveryWordsNotNeeded");
    assert!(!device.signer.holds_words());
    let RecoveryWordsOutcome::Err(refused) =
        onboarding::recovery_words(device.path(), &device.signer).await
    else {
        panic!("an owned device made new words")
    };
    assert_eq!(refused.code, "RecoveryWordsNotNeeded");

    assert!(matches!(
        onboarding::finish_with_owner(device.path(), &device.signer).await,
        OnboardingStatusOutcome::Ok(OnboardingStatus { needed: false })
    ));
    assert!(matches!(
        device.state(),
        IdentityState::Owned { ref owner_public_key, created_at: 60, .. }
            if *owner_public_key == person
    ));
}

#[tokio::test]
async fn finishing_a_joined_device_erases_words_that_wait() {
    let device = SetupDevice::new();
    device.words().await;
    device.join();
    assert!(device.signer.holds_words());
    assert!(matches!(
        onboarding::finish_with_owner(device.path(), &device.signer).await,
        OnboardingStatusOutcome::Ok(OnboardingStatus { needed: false })
    ));
    assert!(!device.signer.holds_words());
}
