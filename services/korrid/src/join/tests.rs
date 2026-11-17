//! Two devices join through one in-process relay, with a clock the test
//! moves.
use super::*;
use crate::{
    identity::{DeviceIdentity, OwnerStatementStatus, OWNER_EVENT_KIND},
    relay::{CoordinatedRelays, InProcessRelayNetwork, RelayCoordinator},
};
use crate::{
    identity_switch::IdentitySwitchCoordinator,
    local_signer::{serve_connection, LocalSigner},
    recovery_words::RecoveryWords,
};
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::Keys,
    types::Timestamp,
};
use std::sync::atomic::AtomicBool;

/// Bind a device to a person key with a statement that key signed.
fn bind(root: &std::path::Path, person: &Keys) -> String {
    let mut identity = DeviceIdentity::load_or_create(root).unwrap();
    let key = identity.device_public_key().unwrap().to_owned();
    identity
        .apply_owner_statement(&statement(person, &key))
        .unwrap();
    key
}

struct AutomaticDevice {
    device: Device,
    /// The person key of the device's automatic identity.
    person: String,
    signer: Arc<LocalSigner>,
    _sockets: tempfile::TempDir,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for AutomaticDevice {
    fn drop(&mut self) {
        self.server.abort();
    }
}

const START: u64 = 1_800_000_000;

fn statement(person: &Keys, device: &str) -> String {
    statement_at(person, device, START - 100)
}

fn statement_at(person: &Keys, device: &str, at: u64) -> String {
    EventBuilder::new(Kind::Custom(OWNER_EVENT_KIND), "")
        .tags([
            Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
            Tag::parse(["device", device]).unwrap(),
            Tag::parse(["status", "owned"]).unwrap(),
        ])
        .custom_created_at(Timestamp::from(at))
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
        if owned {
            bind(root.path(), &self.person);
        }
        self.joining(root, None)
    }

    /// A device owned by its automatic identity: the person key of `words`
    /// signed its statement. Its local signer runs on a socket and erases
    /// what it signs with, as in production.
    fn automatic_device(&self, words: &str) -> AutomaticDevice {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        // The federation store and the identity switch accept only a
        // private state root.
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person = RecoveryWords::parse(words).unwrap().person_keys().unwrap();
        let key = bind(root.path(), &person);
        let sockets = tempfile::tempdir().unwrap();
        let socket = sockets.path().join("signer.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let signer = Arc::new(LocalSigner::new(&key).unwrap());
        let server = {
            let signer = signer.clone();
            tokio::spawn(async move {
                loop {
                    let (stream, _) = listener.accept().await.unwrap();
                    let _ = serve_connection(stream, signer.clone()).await;
                }
            })
        };
        let switch = Arc::new(IdentitySwitchCoordinator::new(
            root.path().into(),
            socket,
            crate::FederationResources::open(root.path()).unwrap(),
        ));
        AutomaticDevice {
            device: self.joining(root, Some(switch)),
            person: person.public_key().to_hex(),
            signer,
            _sockets: sockets,
            server,
        }
    }

    fn joining(
        &self,
        root: tempfile::TempDir,
        switch: Option<Arc<IdentitySwitchCoordinator>>,
    ) -> Device {
        let identity = DeviceIdentity::load_or_create(root.path()).unwrap();
        let key = identity.device_public_key().unwrap().to_owned();
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
                answer_wait: 30,
                watch: 60,
                poll: Duration::from_millis(5),
            },
            joined: Arc::new(move || flag.store(true, Ordering::SeqCst)),
            switch,
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
        new.join
            .enter(&code.code, None, false, facts())
            .await
            .unwrap(),
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
    new.join
        .enter(&wrong(&first.code), None, false, facts())
        .await
        .unwrap();
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
    guesser
        .join
        .enter(&wrong(&first.code), None, false, facts())
        .await
        .unwrap();
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
        device
            .join
            .enter(&first.code, None, false, facts())
            .await
            .unwrap();
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
    new.join
        .enter(&code.code, None, false, facts())
        .await
        .unwrap();
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
        failure(owned.join.enter("1234561234", None, false, facts()).await),
        "JoinNeedsWords"
    );
    assert_eq!(
        failure(new.join.enter("1234567", None, false, facts()).await),
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
        failure(new.join.enter("1234561234", None, false, unnamed).await),
        "JoinUnavailable"
    );
}

/// The ids of a stored owner statement's events, own event first.
fn chain_ids(owner_statement: &str) -> Vec<String> {
    let value: serde_json::Value = serde_json::from_str(owner_statement).unwrap();
    match value {
        serde_json::Value::Array(events) => events
            .iter()
            .map(|event| event["id"].as_str().unwrap().to_owned())
            .collect(),
        event => vec![event["id"].as_str().unwrap().to_owned()],
    }
}

fn rendezvous_for(network: &Network, device: &Device) -> rendezvous::Rendezvous {
    rendezvous::Rendezvous::new(
        network.relays.clone(),
        Arc::new(network.network.clone()),
        device.identity(),
    )
}

#[tokio::test]
async fn a_hello_with_no_answer_spends_the_code_and_the_next_code_says_so() {
    let network = Network::new();
    let (owned, squatter) = (network.device(true), network.device(false));
    let first = shown(owned.join.start().await.unwrap());
    let nameplate = code::Nameplate::parse(&first.code[..3]).unwrap();
    let squatting = rendezvous_for(&network, &squatter);
    let open = squatting
        .find_open(nameplate, START, 300)
        .await
        .unwrap()
        .expect("the owned device's open event");
    squatting
        .send(
            nameplate,
            &open.owned_device,
            &rendezvous::Message::Hello { open: open.id },
            START,
            START + 300,
        )
        .await
        .unwrap();
    // The owned device answers the hello; the squatter never answers back.
    let mut offered = false;
    for _ in 0..2000 {
        offered = !squatting
            .receive(
                nameplate,
                Some(&owned.key),
                &[rendezvous::Step::Offer],
                START,
                START,
            )
            .await
            .unwrap()
            .is_empty();
        if offered {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert!(offered, "the owned device never offered");
    network.advance(29);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(matches!(
        owned.join.status(),
        JoinStatus::ShowingCode(code) if code.code == first.code
    ));
    network.advance(1);
    let second = shown(
        until(
            &owned,
            |status| matches!(status, JoinStatus::ShowingCode(code) if code.code != first.code),
            "the owned device shows a new code",
        )
        .await,
    );
    assert!(second.wrong_code_tried);
}

#[tokio::test]
async fn open_events_dated_in_the_future_or_held_too_long_are_ignored() {
    let network = Network::new();
    let reader = network.device(false);
    let stranger = Keys::generate();
    let open = |nameplate: &str, created_at: u64, expires_at: u64| {
        let event = EventBuilder::new(Kind::Custom(OWNER_EVENT_KIND), "")
            .tags([
                Tag::parse(["d", &format!("org.korri.join:{nameplate}")]).unwrap(),
                Tag::parse(["expiration", &expires_at.to_string()]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(created_at))
            .finalize(&stranger)
            .unwrap()
            .as_json();
        network
            .network
            .inject(&network.relays.as_slice()[0], &event)
            .unwrap();
    };
    open("111", START + 61, START + 300);
    open("222", START, START + 300 + 61 + 1);
    open("333", START, START + 300);
    let rendezvous = rendezvous_for(&network, &reader);
    let nameplate = |digits: &str| code::Nameplate::parse(digits).unwrap();
    assert_eq!(
        rendezvous.nameplates_in_use(START, 300).await.unwrap(),
        [nameplate("333")].into_iter().collect()
    );
    for ignored in ["111", "222"] {
        assert!(rendezvous
            .find_open(nameplate(ignored), START, 300)
            .await
            .unwrap()
            .is_none());
    }
    assert_eq!(
        rendezvous
            .find_open(nameplate("333"), START, 300)
            .await
            .unwrap()
            .map(|open| open.owned_device),
        Some(stranger.public_key().to_hex())
    );
}

#[tokio::test]
async fn the_vouch_names_the_owned_devices_statement_at_accept_time() {
    let network = Network::new();
    let (owned, new) = (network.device(true), network.device(false));
    let code = shown(owned.join.start().await.unwrap());
    new.join
        .enter(&code.code, None, false, facts())
        .await
        .unwrap();
    until(
        &owned,
        |status| matches!(status, JoinStatus::AwaitingAccept(_)),
        "owned device asks for Accept",
    )
    .await;
    // The person key signs a newer statement for the owned device while it
    // waits for Accept.
    let newer = statement_at(&network.person, &owned.key, START - 10);
    owned.identity().apply_owner_statement(&newer).unwrap();
    owned
        .join
        .0
        .context
        .credentials
        .reload_identity(owned.root.path())
        .unwrap();
    owned.join.decide(true).await.unwrap();
    until(
        &new,
        |status| matches!(status, JoinStatus::Joined(_)),
        "new device joins",
    )
    .await;
    let ids = chain_ids(&new.identity().owner_statement_json().unwrap());
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[1], chain_ids(&newer)[0]);
}

#[test]
fn debug_output_never_shows_a_codes_secret() {
    let status = JoinStatus::ShowingCode(JoinCodeShown {
        code: "123 4561234".into(),
        expires_at: START,
        wrong_code_tried: false,
    });
    let printed = format!("{status:?}");
    assert!(printed.contains("123"), "{printed}");
    assert!(!printed.contains("4561234"), "{printed}");
    let response = crate::RpcResponse::JoinStatus(JoinOutcome::Ok(status));
    assert!(!format!("{response:?}").contains("4561234"));
}

#[tokio::test]
async fn setup_cannot_finish_while_the_device_is_joining() {
    let network = Network::new();
    let (owned, new) = (network.device(true), network.device(false));
    assert!(!new.join.joining());
    let code = shown(owned.join.start().await.unwrap());
    assert!(!owned.join.joining());
    new.join
        .enter(&code.code, None, false, facts())
        .await
        .unwrap();
    assert!(new.join.joining());
    until(
        &new,
        |status| matches!(status, JoinStatus::WaitingForAccept),
        "new device waits for Accept",
    )
    .await;
    assert!(new.join.joining());
    owned.join.decide(false).await.unwrap();
    until(
        &new,
        |status| matches!(status, JoinStatus::Refused),
        "new device learns of the refusal",
    )
    .await;
    assert!(!new.join.joining());
}

/// A new device, a vouch for it, and the local signer that holds the words
/// setup's end confirms.
struct SetupRace {
    owned: Device,
    new: Device,
    vouch: String,
    signer: crate::local_signer::LocalSigner,
    answers: Vec<String>,
    person: String,
}

impl SetupRace {
    async fn new(network: &Network) -> Self {
        let (owned, new) = (network.device(true), network.device(false));
        let vouch = owned
            .identity()
            .vouch_for(&new.key, START)
            .unwrap()
            .owner_statement;
        let signer = crate::local_signer::LocalSigner::new(&new.key).unwrap();
        let crate::onboarding::RecoveryWordsOutcome::Ok(shown) =
            crate::onboarding::recovery_words(new.root.path(), &signer).await
        else {
            panic!("the new device made no recovery words")
        };
        let words: Vec<&str> = shown.words.split(' ').collect();
        let answers = shown
            .confirm_positions
            .iter()
            .map(|position| words[usize::from(*position) - 1].to_owned())
            .collect();
        Self {
            owned,
            new,
            vouch,
            signer,
            answers,
            person: network.person.public_key().to_hex(),
        }
    }

    async fn store_vouch(&self) -> JoinStatus {
        session::store_vouch(
            &self.new.join.0.context,
            &session::Vouched {
                own: self.new.key.clone(),
                peer: self.owned.key.clone(),
                owner_statement: self.vouch.clone(),
            },
        )
        .await
    }

    /// The person confirms the words. True when that bound the automatic
    /// identity; false when the device had joined first.
    async fn finish_setup(&self) -> bool {
        let (outcome, bound) = crate::onboarding::confirm_words(
            self.new.root.path(),
            &self.signer,
            &self.answers,
            START,
        )
        .await;
        match &outcome {
            crate::onboarding::OnboardingStatusOutcome::Ok(_) => assert!(bound),
            crate::onboarding::OnboardingStatusOutcome::Err(failure) => {
                assert!(!bound);
                assert_eq!(failure.code, "RecoveryWordsNotNeeded");
            }
        }
        bound
    }

    fn owner(&self) -> String {
        match self.new.identity().state() {
            IdentityState::Owned {
                owner_public_key, ..
            } => owner_public_key.clone(),
            state => panic!("the new device has no owner: {state:?}"),
        }
    }
}

#[tokio::test]
async fn setup_that_ends_after_a_stored_vouch_keeps_the_vouch() {
    let network = Network::new();
    let race = SetupRace::new(&network).await;
    assert!(matches!(race.store_vouch().await, JoinStatus::Joined(_)));
    assert!(!race.finish_setup().await);
    assert_eq!(race.owner(), race.person);
}

#[tokio::test]
async fn a_vouch_that_arrives_after_setup_ended_is_refused() {
    let network = Network::new();
    let race = SetupRace::new(&network).await;
    assert!(race.finish_setup().await);
    let JoinStatus::Failed(failure) = race.store_vouch().await else {
        panic!("the vouch replaced the automatic owner")
    };
    assert_eq!(failure.code, "JoinStoreFailed");
    assert_ne!(race.owner(), race.person);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_vouch_and_setup_end_at_once_leave_exactly_one_owner() {
    let network = Network::new();
    for _ in 0..8 {
        let race = Arc::new(SetupRace::new(&network).await);
        let (stored, bound) = tokio::join!(
            {
                let race = race.clone();
                tokio::spawn(async move { race.store_vouch().await })
            },
            {
                let race = race.clone();
                tokio::spawn(async move { race.finish_setup().await })
            }
        );
        let (stored, bound) = (stored.unwrap(), bound.unwrap());
        let joined = matches!(stored, JoinStatus::Joined(_));
        assert_ne!(joined, bound, "{stored:?}");
        assert_eq!(race.owner() == race.person, joined);
    }
}

#[tokio::test]
async fn the_roster_keeps_a_person_signed_statement_over_a_newer_vouch_chain() {
    let network = Network::new();
    let (owned, reader) = (network.device(true), network.device(true));
    let device = Keys::generate().public_key().to_hex();
    let person_signed = statement_at(&network.person, &device, START - 50);
    let vouch = owned.identity().vouch_for(&device, START).unwrap();
    for event in [
        person_signed.clone(),
        vouch.event_json.clone(),
        owned.identity().owner_statement_json().unwrap(),
    ] {
        network
            .network
            .inject(&network.relays.as_slice()[0], &event)
            .unwrap();
    }
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
        .find(|entry| entry.latest.statement.device_public_key == device)
        .unwrap();
    assert!(entry.latest.statement.vouchers.is_empty());
    assert_eq!(entry.latest.event_json, person_signed);
}

/// NIP-06's 12-word test vector, and other words.
const AUTOMATIC_WORDS: &str =
    "leader monkey parrot ring guide accident before fence cannon height naive bean";
const OTHER_WORDS: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

#[tokio::test]
async fn an_automatic_identity_joins_only_with_its_own_words() {
    let network = Network::new();
    let automatic = network.automatic_device(AUTOMATIC_WORDS);
    let device = &automatic.device;
    let failure = |result: Result<JoinStatus, RpcFailure>| result.unwrap_err();
    let code = "1234561234";
    assert_eq!(
        failure(device.join.enter(code, None, true, facts()).await).code,
        "JoinNeedsWords"
    );
    assert_eq!(
        failure(
            device
                .join
                .enter(code, Some(AUTOMATIC_WORDS), false, facts())
                .await
        )
        .code,
        "JoinNeedsConfirmation"
    );
    let refused = failure(
        device
            .join
            .enter(code, Some(OTHER_WORDS), true, facts())
            .await,
    );
    assert_eq!(refused.code, "JoinWordsRefused");
    assert_eq!(
        refused.message,
        "These are not the recovery words of this device's person key"
    );
    let not_words = failure(
        device
            .join
            .enter(code, Some("leader monkey"), true, facts())
            .await,
    );
    assert_eq!(not_words.code, "JoinWordsRefused");
    assert!(!not_words.message.contains("leader"));
    // Nothing changed, and nothing waits on disk.
    assert!(matches!(
        device.identity().state(),
        IdentityState::Owned { ref owner_public_key, .. } if *owner_public_key == automatic.person
    ));
    assert!(!device
        .root
        .path()
        .join("identity-switch/next-root")
        .exists());
    assert!(matches!(device.join.status(), JoinStatus::Idle));
}

#[tokio::test]
async fn an_automatic_identity_moves_to_the_person_key_it_joins() {
    let network = Network::new();
    let owned = network.device(true);
    let automatic = network.automatic_device(AUTOMATIC_WORDS);
    let device = &automatic.device;
    let old_key = device.key.clone();

    let code = shown(owned.join.start().await.unwrap());
    assert!(matches!(
        device
            .join
            .enter(&code.code, Some(AUTOMATIC_WORDS), true, facts())
            .await
            .unwrap(),
        JoinStatus::Matching
    ));
    // The old person key signed the removal and is gone from memory.
    assert!(!automatic.signer.holds_words());
    until(
        &owned,
        |status| matches!(status, JoinStatus::AwaitingAccept(_)),
        "owned device asks for Accept",
    )
    .await;
    owned.join.decide(true).await.unwrap();
    let JoinStatus::Joined(joined) = until(
        device,
        |status| matches!(status, JoinStatus::Joined(_) | JoinStatus::Failed(_)),
        "the automatic device joins",
    )
    .await
    else {
        panic!("the join failed: {:?}", device.join.status())
    };
    let person = network.person.public_key().to_hex();
    assert_eq!(joined.owner_public_key, person);
    assert_ne!(
        joined.device_public_key, old_key,
        "a switch makes a new device key"
    );

    // The device now holds the new device key, owned through the vouch.
    let identity = device.identity();
    let IdentityState::Owned {
        device_public_key,
        owner_public_key,
        ..
    } = identity.state()
    else {
        panic!("the device has no owner: {:?}", identity.state())
    };
    assert_eq!(*device_public_key, joined.device_public_key);
    assert_eq!(*owner_public_key, person);
    let statement = DeviceIdentity::verify_owner_statement(
        &identity.owner_statement_json().unwrap(),
        device_public_key,
    )
    .unwrap();
    assert_eq!(statement.vouchers, vec![owned.key.clone()]);
    // The switch finished: no journal and no staged identity are left.
    let switch = device.root.path().join("identity-switch");
    assert!(!switch.join("journal.json").exists());
    assert!(!switch.join("next-root").exists());
    for _ in 0..4000 {
        if device.joined.load(Ordering::SeqCst) {
            return;
        }
        device.join.status();
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    panic!("the device did not restart with its new owner");
}
