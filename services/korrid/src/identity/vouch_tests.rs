//! A vouch chain (ADR 0069): each link is checked, and it must end in a
//! statement that the person key signed.
use super::*;

fn statement(person: &Keys, device: &str, status: OwnerStatementStatus, at: u64) -> String {
    signed(person, owner_tags(device, status), at)
}

fn signed(signer: &Keys, tags: Vec<Vec<String>>, at: u64) -> String {
    EventBuilder::new(Kind::Custom(OWNER_EVENT_KIND), "")
        .tags(tags.into_iter().map(|tag| Tag::parse(tag).unwrap()))
        .custom_created_at(Timestamp::from(at))
        .finalize(signer)
        .unwrap()
        .as_json()
}

fn id(event_json: &str) -> String {
    DeviceIdentity::verify_event(event_json).unwrap().id
}

fn chain(events: &[&str]) -> String {
    format!("[{}]", events.join(","))
}

/// An owned device that the person key signed for, and a new device.
struct Devices {
    _roots: Vec<tempfile::TempDir>,
    person: Keys,
    owned: DeviceIdentity,
    new: DeviceIdentity,
}

impl Devices {
    fn new() -> Self {
        let roots = vec![tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
        let person = Keys::generate();
        let mut owned = DeviceIdentity::load_or_create(roots[0].path()).unwrap();
        let key = owned.device_public_key().unwrap().to_owned();
        owned
            .apply_owner_statement(&statement(&person, &key, OwnerStatementStatus::Owned, 100))
            .unwrap();
        let new = DeviceIdentity::load_or_create(roots[1].path()).unwrap();
        Self {
            _roots: roots,
            person,
            owned,
            new,
        }
    }

    fn new_key(&self) -> String {
        self.new.device_public_key().unwrap().into()
    }

    fn owned_key(&self) -> String {
        self.owned.device_public_key().unwrap().into()
    }
}

#[test]
fn a_vouch_names_the_person_key_and_the_vouchers_statement() {
    let devices = Devices::new();
    let vouch = devices.owned.vouch_for(&devices.new_key(), 200).unwrap();
    let event = DeviceIdentity::verify_event(&vouch.event_json).unwrap();
    assert_eq!(event.author, devices.owned_key());
    assert_eq!(event.kind, OWNER_EVENT_KIND);
    assert_eq!(
        event.tags,
        vec![
            vec![
                "d".to_owned(),
                format!("{OWNER_EVENT_PREFIX}{}", devices.new_key())
            ],
            vec!["device".into(), devices.new_key()],
            vec!["status".into(), "owned".into()],
            vec!["p".into(), devices.person.public_key().to_hex()],
            vec![
                "e".into(),
                devices
                    .owned
                    .owner_statement_json()
                    .map(|json| id(&json))
                    .unwrap()
            ],
        ]
    );
}

#[test]
fn a_valid_chain_binds_the_new_device_to_the_person_key() {
    let devices = Devices::new();
    let vouch = devices.owned.vouch_for(&devices.new_key(), 200).unwrap();
    let verified =
        DeviceIdentity::verify_owner_statement(&vouch.owner_statement, &devices.new_key()).unwrap();
    assert_eq!(
        verified.owner_public_key,
        devices.person.public_key().to_hex()
    );
    assert_eq!(verified.device_public_key, devices.new_key());
    assert_eq!(verified.status, OwnerStatementStatus::Owned);
    assert_eq!(verified.event_id, id(&vouch.event_json));
    assert_eq!(verified.created_at, 200);
    assert_eq!(verified.vouchers, vec![devices.owned_key()]);
    assert_eq!(
        DeviceIdentity::derive_owner_statement(&vouch.owner_statement).unwrap(),
        verified
    );

    // The new device stores the chain and reads it back after a restart.
    let root = devices.new.directory.parent().unwrap().to_owned();
    let mut new = DeviceIdentity::load_or_create(&root).unwrap();
    new.apply_owner_statement(&vouch.owner_statement).unwrap();
    let reloaded = DeviceIdentity::load_or_create(&root).unwrap();
    assert_eq!(reloaded.state(), new.state());
    assert!(matches!(
        reloaded.state(),
        IdentityState::Owned { owner_public_key, event_id, .. }
            if *owner_public_key == devices.person.public_key().to_hex()
                && *event_id == verified.event_id
    ));
    assert_eq!(
        reloaded.owner_statement_json().as_deref(),
        Some(vouch.owner_statement.as_str())
    );
    assert_eq!(
        reloaded.owner_statement_events(),
        vec![
            vouch.event_json.clone(),
            devices.owned.owner_statement_json().unwrap()
        ]
    );
}

#[test]
fn a_vouched_device_vouches_in_turn_and_the_chain_grows() {
    let devices = Devices::new();
    let root = devices.new.directory.parent().unwrap().to_owned();
    let mut middle = DeviceIdentity::load_or_create(&root).unwrap();
    middle
        .apply_owner_statement(
            &devices
                .owned
                .vouch_for(&devices.new_key(), 200)
                .unwrap()
                .owner_statement,
        )
        .unwrap();
    let last = Keys::generate().public_key().to_hex();
    let vouch = middle.vouch_for(&last, 300).unwrap();
    let verified = DeviceIdentity::verify_owner_statement(&vouch.owner_statement, &last).unwrap();
    assert_eq!(
        verified.owner_public_key,
        devices.person.public_key().to_hex()
    );
    assert_eq!(
        verified.vouchers,
        vec![devices.new_key(), devices.owned_key()]
    );
}

#[test]
fn a_broken_chain_is_refused() {
    let devices = Devices::new();
    let person = devices.person.public_key().to_hex();
    let new = devices.new_key();
    let owned = devices.owned_key();
    let root = devices.owned.owner_statement_json().unwrap();
    let voucher = devices.owned.keys.clone().unwrap();
    let good = signed(&voucher, vouch_tags(&new, &person, &id(&root)), 200);
    assert!(DeviceIdentity::verify_owner_statement(&chain(&[&good, &root]), &new).is_ok());

    let other_root = statement(&devices.person, &owned, OwnerStatementStatus::Owned, 101);
    let wrong_link = signed(&voucher, vouch_tags(&new, &person, &id(&other_root)), 200);
    let mut tampered: serde_json::Value = serde_json::from_str(&good).unwrap();
    tampered["created_at"] = serde_json::json!(201);
    let revoked_root = statement(&devices.person, &owned, OwnerStatementStatus::Revoked, 102);
    let to_revoked = signed(&voucher, vouch_tags(&new, &person, &id(&revoked_root)), 200);
    let root_for_other_device = statement(
        &devices.person,
        &Keys::generate().public_key().to_hex(),
        OwnerStatementStatus::Owned,
        100,
    );
    let to_other_device = signed(
        &voucher,
        vouch_tags(&new, &person, &id(&root_for_other_device)),
        200,
    );
    let mut revoked_vouch_tags = vouch_tags(&new, &person, &id(&root));
    revoked_vouch_tags[2][1] = "revoked".into();
    let revoked_vouch = signed(&voucher, revoked_vouch_tags, 200);
    let mut extra_tag = vouch_tags(&new, &person, &id(&root));
    extra_tag.push(vec!["t".into(), "extra".into()]);
    let with_extra_tag = signed(&voucher, extra_tag, 200);
    let self_vouch = signed(
        devices.new.keys.as_ref().unwrap(),
        vouch_tags(&new, &person, &id(&root)),
        200,
    );
    let person_vouch = signed(&devices.person, vouch_tags(&new, &person, &id(&root)), 200);

    for (case, evidence, device) in [
        ("missing root", chain(&[&good]), new.clone()),
        (
            "link to another statement",
            chain(&[&wrong_link, &root]),
            new.clone(),
        ),
        (
            "changed vouch",
            chain(&[&tampered.to_string(), &root]),
            new.clone(),
        ),
        (
            "root that revokes",
            chain(&[&to_revoked, &revoked_root]),
            new.clone(),
        ),
        (
            "root for another device",
            chain(&[&to_other_device, &root_for_other_device]),
            new.clone(),
        ),
        (
            "vouch that revokes",
            chain(&[&revoked_vouch, &root]),
            new.clone(),
        ),
        ("extra tag", chain(&[&with_extra_tag, &root]), new.clone()),
        ("self vouch", chain(&[&self_vouch, &root]), new.clone()),
        (
            "vouch signed by the person key",
            chain(&[&person_vouch, &root]),
            new.clone(),
        ),
        ("reversed", chain(&[&root, &good]), new.clone()),
        ("root alone in a chain", chain(&[&root]), owned.clone()),
        ("for another device", chain(&[&good, &root]), owned.clone()),
        ("not JSON", "[{".into(), new.clone()),
    ] {
        assert!(
            DeviceIdentity::verify_owner_statement(&evidence, &device).is_err(),
            "{case}"
        );
    }
    let too_long = chain(&vec![good.as_str(); MAX_OWNER_CHAIN + 1]);
    assert!(DeviceIdentity::verify_owner_statement(&too_long, &new).is_err());
}

#[test]
fn a_chain_that_names_another_person_key_is_refused() {
    let devices = Devices::new();
    let new = devices.new_key();
    let root = devices.owned.owner_statement_json().unwrap();
    let voucher = devices.owned.keys.clone().unwrap();
    let stranger = Keys::generate();
    // The vouch names a person key that did not sign the root.
    let named_other = signed(
        &voucher,
        vouch_tags(&new, &stranger.public_key().to_hex(), &id(&root)),
        200,
    );
    assert!(DeviceIdentity::verify_owner_statement(&chain(&[&named_other, &root]), &new).is_err());
    // The root is signed by another key than the one the vouch names.
    let stranger_root = statement(
        &stranger,
        &devices.owned_key(),
        OwnerStatementStatus::Owned,
        100,
    );
    let to_stranger_root = signed(
        &voucher,
        vouch_tags(
            &new,
            &devices.person.public_key().to_hex(),
            &id(&stranger_root),
        ),
        200,
    );
    assert!(DeviceIdentity::verify_owner_statement(
        &chain(&[&to_stranger_root, &stranger_root]),
        &new
    )
    .is_err());
    // A full chain to another person key is valid, but binds the device to
    // that key; a device of this person compares the key and refuses it.
    let root = devices.new.directory.parent().unwrap().to_owned();
    let mut joined = DeviceIdentity::load_or_create(&root).unwrap();
    let vouch = devices.owned.vouch_for(&new, 200).unwrap();
    joined
        .apply_owner_statement(&vouch.owner_statement)
        .unwrap();
    let foreign = statement(&stranger, &new, OwnerStatementStatus::Owned, 300);
    assert!(joined.apply_owner_statement(&foreign).is_err());
}

#[test]
fn only_an_owned_device_vouches_and_never_for_itself() {
    let devices = Devices::new();
    assert!(devices.new.vouch_for(&devices.owned_key(), 200).is_err());
    assert!(devices.owned.vouch_for(&devices.owned_key(), 200).is_err());
    assert!(devices.owned.vouch_for("not-a-key", 200).is_err());
}

#[test]
fn a_joined_device_decrypts_only_its_peers_messages() {
    let devices = Devices::new();
    let sent = devices
        .owned
        .encrypt_tagged_event(
            &devices.new_key(),
            OWNER_EVENT_KIND,
            vec![vec!["d".into(), "org.korri.join:123:offer".into()]],
            "hello",
            50,
        )
        .unwrap();
    let opened = devices
        .new
        .decrypt_event_from(&sent.json, &devices.owned_key())
        .unwrap();
    assert_eq!(opened.content, "hello");
    assert!(devices
        .new
        .decrypt_event_from(&sent.json, &devices.new_key())
        .is_err());
    let stranger_root = tempfile::tempdir().unwrap();
    let stranger = DeviceIdentity::load_or_create(stranger_root.path()).unwrap();
    assert!(stranger
        .decrypt_event_from(&sent.json, &devices.owned_key())
        .is_err());
}
