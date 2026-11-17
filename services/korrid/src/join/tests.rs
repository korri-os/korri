//! Two devices join through one in-process relay, with a clock the test
//! moves.
use super::*;
use crate::{
    identity::{DeviceIdentity, OwnerStatementStatus, OWNER_EVENT_KIND},
    relay::{CoordinatedRelays, InProcessRelayNetwork, RelayCoordinator},
};
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::Keys,
    types::Timestamp,
};
use std::sync::atomic::AtomicBool;

const START: u64 = 1_800_000_000;

fn statement(person: &Keys, device: &str) -> String {
    EventBuilder::new(Kind::Custom(OWNER_EVENT_KIND), "")
        .tags([
            Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
            Tag::parse(["device", device]).unwrap(),
            Tag::parse(["status", "owned"]).unwrap(),
        ])
        .custom_created_at(Timestamp::from(START - 100))
        .finalize(person)
        .unwrap()
        .as_json()
}

struct Device {
    root: tempfile::TempDir,
    key: String,
    join: JoinCoordinator,
    joined: Arc<AtomicBool>,
}

impl Device {
    fn identity(&self) -> DeviceIdentity {
        DeviceIdentity::load_or_create(self.root.path()).unwrap()
    }
}

struct Network {
    person: Keys,
    clock: Arc<AtomicU64>,
    relays: RelayList,
    network: InProcessRelayNetwork,
}

impl Network {
    fn new() -> Self {
        let relays = RelayList::configured(vec!["wss://relay.join.test".into()]).unwrap();
        Self {
            person: Keys::generate(),
            clock: Arc::new(AtomicU64::new(START)),
            network: InProcessRelayNetwork::new(&relays),
            relays,
        }
    }

    fn device(&self, owned: bool) -> Device {
        let root = tempfile::tempdir().unwrap();
        let mut identity = DeviceIdentity::load_or_create(root.path()).unwrap();
        let key = identity.device_public_key().unwrap().to_owned();
        if owned {
            identity
                .apply_owner_statement(&statement(&self.person, &key))
                .unwrap();
        }
        let joined = Arc::new(AtomicBool::new(false));
        let flag = joined.clone();
        let clock = self.clock.clone();
        let join = JoinCoordinator::new(JoinContext {
            credentials: PeerCredentials::from_identity(identity),
            directory: None,
            private_state_root: root.path().into(),
            relays: Some(self.relays.clone()),
            transport: Arc::new(self.network.clone()),
            clock: Arc::new(move || clock.load(Ordering::SeqCst)),
            timing: JoinTiming {
                code_lifetime: 300,
                accept_wait: 300,
                watch: 60,
                poll: Duration::from_millis(5),
            },
            joined: Arc::new(move || flag.store(true, Ordering::SeqCst)),
        });
        Device {
            root,
            key,
            join,
            joined,
        }
    }

    fn advance(&self, seconds: u64) {
        self.clock.fetch_add(seconds, Ordering::SeqCst);
    }
}

fn facts() -> JoiningDevice {
    JoiningDevice {
        name: Some("Living room".into()),
        kind: "linux x86_64".into(),
        connection: Some(NetworkConnection::Wired),
    }
}

/// Wait until the device's join status fits, and return it.
async fn until(device: &Device, fits: impl Fn(&JoinStatus) -> bool, what: &str) -> JoinStatus {
    for _ in 0..4000 {
        let status = device.join.status();
        if fits(&status) {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    panic!("{what}: still {:?}", device.join.status());
}

fn shown(status: JoinStatus) -> JoinCodeShown {
    match status {
        JoinStatus::ShowingCode(shown) => shown,
        status => panic!("no code shown: {status:?}"),
    }
}

/// The same nameplate with a different secret.
fn wrong(code: &str) -> String {
    let mut digits: Vec<u8> = code.bytes().filter(u8::is_ascii_digit).collect();
    let last = digits.last_mut().unwrap();
    *last = if *last == b'6' { b'1' } else { *last + 1 };
    String::from_utf8(digits).unwrap()
}

#[tokio::test]
async fn a_matching_code_and_one_accept_make_the_new_device_owned() {
    let network = Network::new();
    let (owned, new) = (network.device(true), network.device(false));
    let code = shown(owned.join.start().await.unwrap());
    assert!(!code.wrong_code_tried);
    assert_eq!(code.expires_at, START + 300);
    assert!(matches!(
        new.join.enter(&code.code, facts()).unwrap(),
        JoinStatus::Matching
    ));
    let JoinStatus::AwaitingAccept(seen) = until(
        &owned,
        |status| matches!(status, JoinStatus::AwaitingAccept(_)),
        "owned device asks for Accept",
    )
    .await
    else {
        unreachable!()
    };
    assert_eq!(seen.name.as_deref(), Some("Living room"));
    assert_eq!(seen.kind, "linux x86_64");
    assert!(matches!(seen.connection, Some(NetworkConnection::Wired)));
    until(
        &new,
        |status| matches!(status, JoinStatus::WaitingForAccept),
        "new device waits for Accept",
    )
    .await;
    assert!(matches!(
        new.identity().state(),
        IdentityState::Unowned { .. }
    ));

    let JoinStatus::Joined(vouched) = owned.join.decide(true).await.unwrap() else {
        panic!("Accept did not vouch")
    };
    assert_eq!(vouched.device_public_key, new.key);
    assert_eq!(
        vouched.owner_public_key,
        network.person.public_key().to_hex()
    );
    let JoinStatus::Joined(joined) = until(
        &new,
        |status| matches!(status, JoinStatus::Joined(_)),
        "new device joins",
    )
    .await
    else {
        unreachable!()
    };
    assert_eq!(joined.device_public_key, new.key);
    assert_eq!(
        joined.owner_public_key,
        network.person.public_key().to_hex()
    );
    // The restart waits until someone saw that the device joined.
    for _ in 0..200 {
        if new.joined.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(new.joined.load(Ordering::SeqCst));

    // The new device is owned through the owned device's vouch.
    let identity = new.identity();
    assert!(matches!(
        identity.state(),
        IdentityState::Owned { owner_public_key, .. }
            if *owner_public_key == network.person.public_key().to_hex()
    ));
    let statement =
        DeviceIdentity::verify_owner_statement(&identity.owner_statement_json().unwrap(), &new.key)
            .unwrap();
    assert_eq!(statement.status, OwnerStatementStatus::Owned);
    assert_eq!(statement.vouchers, vec![owned.key.clone()]);

    // Another device of the person finds the vouch on the relays and
    // follows its chain.
    CoordinatedRelays::new(
        network.relays.clone(),
        owned.identity(),
        Arc::new(network.network.clone()),
    )
    .unwrap()
    .publish_owner_statement()
    .await
    .unwrap();
    let reader = network.device(true);
    let roster = CoordinatedRelays::new(
        network.relays.clone(),
        reader.identity(),
        Arc::new(network.network.clone()),
    )
    .unwrap()
    .read_owner_roster()
    .await
    .unwrap();
    let entry = roster
        .iter()
        .find(|entry| entry.latest.statement.device_public_key == new.key)
        .expect("the vouched device is in the roster");
    assert!(entry.is_owned());
    assert_eq!(entry.latest.statement.vouchers, vec![owned.key.clone()]);
    assert!(roster
        .iter()
        .any(|entry| entry.latest.statement.device_public_key == owned.key));
}

#[tokio::test]
async fn a_wrong_code_ends_that_code_and_the_next_code_says_so() {
    let network = Network::new();
    let (owned, new) = (network.device(true), network.device(false));
    let first = shown(owned.join.start().await.unwrap());
    new.join.enter(&wrong(&first.code), facts()).unwrap();
    let JoinStatus::Failed(failure) = until(
        &new,
        |status| matches!(status, JoinStatus::Failed(_)),
        "new device learns that the code did not match",
    )
    .await
    else {
        unreachable!()
    };
    assert_eq!(failure.code, "JoinCodeMismatch");
    let second = shown(
        until(
            &owned,
            |status| matches!(status, JoinStatus::ShowingCode(code) if code.code != first.code),
            "owned device shows a new code",
        )
        .await,
    );
    assert!(second.wrong_code_tried);
    assert!(matches!(
        new.identity().state(),
        IdentityState::Unowned { .. }
    ));
    assert!(!new.joined.load(Ordering::SeqCst));
}

#[tokio::test]
async fn after_one_wrong_guess_the_right_digits_of_that_code_no_longer_join() {
    let network = Network::new();
    let (owned, guesser, late) = (
        network.device(true),
        network.device(false),
        network.device(false),
    );
    let first = shown(owned.join.start().await.unwrap());
    guesser.join.enter(&wrong(&first.code), facts()).unwrap();
    until(
        &owned,
        |status| matches!(status, JoinStatus::ShowingCode(code) if code.wrong_code_tried),
        "the wrong guess spends the code",
    )
    .await;
    // The device that guessed wrong, and another device, both try the right
    // digits. Neither gets an answer, and the guesser does not take the
    // messages of its earlier attempt.
    for device in [&guesser, &late] {
        device.join.enter(&first.code, facts()).unwrap();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    for device in [&guesser, &late] {
        assert!(matches!(device.join.status(), JoinStatus::Matching));
    }
    assert!(matches!(owned.join.status(), JoinStatus::ShowingCode(_)));
    network.advance(400);
    for device in [&guesser, &late] {
        let JoinStatus::Failed(failure) = until(
            device,
            |status| matches!(status, JoinStatus::Failed(_)),
            "the spent code gets no answer",
        )
        .await
        else {
            unreachable!()
        };
        assert_eq!(failure.code, "JoinNoAnswer");
        assert!(matches!(
            device.identity().state(),
            IdentityState::Unowned { .. }
        ));
    }
}

#[tokio::test]
async fn an_expired_code_is_replaced_by_a_new_one() {
    let network = Network::new();
    let owned = network.device(true);
    let first = shown(owned.join.start().await.unwrap());
    network.advance(300);
    let second = shown(
        until(
            &owned,
            |status| matches!(status, JoinStatus::ShowingCode(code) if code.code != first.code),
            "owned device shows a new code",
        )
        .await,
    );
    assert!(!second.wrong_code_tried);
    assert_eq!(second.expires_at, START + 600);
}

#[tokio::test]
async fn a_refused_device_stays_unowned() {
    let network = Network::new();
    let (owned, new) = (network.device(true), network.device(false));
    let code = shown(owned.join.start().await.unwrap());
    new.join.enter(&code.code, facts()).unwrap();
    until(
        &owned,
        |status| matches!(status, JoinStatus::AwaitingAccept(_)),
        "owned device asks for Accept",
    )
    .await;
    assert!(matches!(
        owned.join.decide(false).await.unwrap(),
        JoinStatus::Refused
    ));
    until(
        &new,
        |status| matches!(status, JoinStatus::Refused),
        "new device learns of the refusal",
    )
    .await;
    assert!(matches!(
        new.identity().state(),
        IdentityState::Unowned { .. }
    ));
    assert!(!new.joined.load(Ordering::SeqCst));
}

#[tokio::test]
async fn an_owned_join_that_no_one_watches_ends() {
    let network = Network::new();
    let owned = network.device(true);
    shown(owned.join.start().await.unwrap());
    network.advance(61);
    for _ in 0..4000 {
        // Read the status without watching it.
        if matches!(*owned.join.0.status.borrow(), JoinStatus::Idle) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    panic!("an unwatched join kept showing codes");
}

#[tokio::test]
async fn each_side_needs_its_own_identity_state_and_a_valid_code() {
    let network = Network::new();
    let (owned, new) = (network.device(true), network.device(false));
    let failure = |result: Result<JoinStatus, RpcFailure>| result.unwrap_err().code;
    assert_eq!(failure(new.join.start().await), "JoinNeedsOwner");
    assert_eq!(
        failure(owned.join.enter("1234561234", facts())),
        "JoinNeedsNewDevice"
    );
    assert_eq!(
        failure(new.join.enter("1234567", facts())),
        "JoinCodeInvalid"
    );
    assert_eq!(
        failure(owned.join.decide(true).await),
        "JoinNotAwaitingAccept"
    );
    let unnamed = JoiningDevice {
        name: Some("bad\u{7}name".into()),
        ..facts()
    };
    assert_eq!(
        failure(new.join.enter("1234561234", unnamed)),
        "JoinUnavailable"
    );
}
