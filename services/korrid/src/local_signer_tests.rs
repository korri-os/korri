use super::*;
use crate::identity::{DeviceIdentity, IdentityState, OwnerStatementStatus};

/// NIP-06's 12-word test vector and its public key.
const WORDS: &str =
    "leader monkey parrot ring guide accident before fence cannon height naive bean";
const WORDS_PUBLIC_KEY: &str = "17162c921dc4d2518f9a101db33695df1afb56ab82f5ff3e5da6eec3ca5cd917";

struct Device {
    _root: tempfile::TempDir,
    identity: DeviceIdentity,
    key: String,
}

fn device() -> Device {
    let root = tempfile::tempdir().unwrap();
    let identity = DeviceIdentity::load_or_create(root.path()).unwrap();
    let key = identity.device_public_key().unwrap().to_owned();
    Device {
        _root: root,
        identity,
        key,
    }
}

fn statement(
    device: &Device,
    status: OwnerStatementStatus,
    created_at: u64,
) -> PersonSignerRequest {
    PersonSignerRequest {
        unsigned_event_template: device
            .identity
            .owner_statement_template(status, created_at)
            .unwrap(),
    }
}

/// The answers a person who wrote the words down gives.
fn answers(shown: &ShownWords) -> Vec<String> {
    let words: Vec<&str> = shown.words.split(' ').collect();
    shown
        .positions
        .iter()
        .map(|position| words[position - 1].to_owned())
        .collect()
}

fn as_strs(answers: &[String]) -> Vec<&str> {
    answers.iter().map(String::as_str).collect()
}

fn approved(state: PersonSignerState) -> (String, String) {
    match state {
        PersonSignerState::Approved {
            owner_public_key,
            signed_event_json,
            ..
        } => (owner_public_key, signed_event_json),
        state => panic!("the signer did not approve: {state:?}"),
    }
}

#[test]
fn new_words_wait_until_confirmed_then_sign_and_are_erased() {
    let mut device = device();
    let signer = LocalSigner::new(&device.key).unwrap();
    let shown = signer.make_words();
    assert_eq!(shown.words.split(' ').count(), 12);
    assert!(signer.holds_words());
    let right = answers(&shown);
    let person = RecoveryWords::parse(&shown.words)
        .unwrap()
        .person_keys()
        .unwrap()
        .public_key()
        .to_hex();

    let request = statement(&device, OwnerStatementStatus::Owned, 100);
    let mut wrong = right.clone();
    // "zoo" is a valid recovery word; about one run in 2048 asks for exactly
    // it at this position, and the "wrong" answer was then right.
    wrong[1] = if right[1] == "zoo" { "zone" } else { "zoo" }.into();
    let ConfirmedWords::Mismatch { message } =
        signer.confirm_words(&as_strs(&wrong), &request).unwrap()
    else {
        panic!("wrong words confirmed");
    };
    assert!(message.contains(&shown.positions[1].to_string()));
    assert!(signer.holds_words(), "a wrong try keeps the words");

    let ConfirmedWords::Signed(state) = signer.confirm_words(&as_strs(&right), &request).unwrap()
    else {
        panic!("the right words did not confirm");
    };
    let (owner, event) = approved(state);
    assert_eq!(owner, person);
    device
        .identity
        .apply_signed_owner_binding(&request.unsigned_event_template, &owner, &event)
        .unwrap();
    assert!(matches!(
        device.identity.state(),
        IdentityState::Owned { .. }
    ));

    assert!(!signer.holds_words(), "confirmed words are erased");
    assert!(signer.confirm_words(&as_strs(&right), &request).is_err());
}

#[test]
fn confirmed_words_sign_only_this_devices_statement() {
    let (this, other) = (device(), device());
    let signer = LocalSigner::new(&this.key).unwrap();
    let shown = signer.make_words();
    let request = statement(&other, OwnerStatementStatus::Owned, 100);
    let ConfirmedWords::Signed(state) = signer
        .confirm_words(&as_strs(&answers(&shown)), &request)
        .unwrap()
    else {
        panic!("the right words did not confirm");
    };
    assert!(matches!(state, PersonSignerState::InvalidResponse { .. }));
    assert!(
        signer.holds_words(),
        "nothing was signed, so the words stay"
    );
}

#[test]
fn new_words_replace_waiting_words_and_forget_erases_them() {
    let device = device();
    let signer = LocalSigner::new(&device.key).unwrap();
    let first = signer.make_words();
    let second = signer.make_words();
    assert_ne!(first.words, second.words);
    let request = statement(&device, OwnerStatementStatus::Owned, 100);
    let first_answers = answers(&first);
    let second_answers = answers(&second);
    // The first words no longer confirm, unless they happen to give the
    // same answers.
    if first_answers != second_answers {
        assert!(matches!(
            signer.confirm_words(&as_strs(&first_answers), &request),
            Ok(ConfirmedWords::Mismatch { .. })
        ));
    }
    signer.forget_words();
    assert!(!signer.holds_words());
    assert!(signer
        .confirm_words(&as_strs(&second_answers), &request)
        .is_err());
}

#[test]
fn typed_words_sign_one_statement_for_the_named_device() {
    let mut device = device();
    for (status, created_at) in [
        (OwnerStatementStatus::Owned, 100),
        (OwnerStatementStatus::Revoked, 101),
    ] {
        let request = statement(&device, status, created_at);
        let (owner, event) = approved(sign_with_words(WORDS, &device.key, &request).unwrap());
        assert_eq!(owner, WORDS_PUBLIC_KEY);
        device.identity.apply_owner_statement(&event).unwrap();
    }
    assert!(matches!(
        device.identity.state(),
        IdentityState::Revoked { .. }
    ));
    let other = self::device();
    let request = statement(&other, OwnerStatementStatus::Owned, 100);
    assert!(matches!(
        sign_with_words(WORDS, &device.key, &request).unwrap(),
        PersonSignerState::InvalidResponse { .. }
    ));
}

#[test]
fn typed_words_that_are_not_words_sign_nothing() {
    let device = device();
    let request = statement(&device, OwnerStatementStatus::Owned, 100);
    let error = sign_with_words(
        "leader monkey parrot ring guide accident before fence cannon heigth naive bean",
        &device.key,
        &request,
    )
    .unwrap_err()
    .to_string();
    assert_eq!(error, "Word 10 is not a recovery word. Check its spelling.");
    assert!(sign_with_words("leader monkey", &device.key, &request).is_err());
}

#[test]
fn only_an_owner_statement_or_a_person_name_is_signed() {
    let device = device();
    let words = RecoveryWords::parse(WORDS).unwrap();
    let signer = LocalPersonSigner::from_words(&words, &device.key).unwrap();
    let canonical: serde_json::Value = serde_json::from_str(
        &statement(&device, OwnerStatementStatus::Owned, 100).unsigned_event_template,
    )
    .unwrap();
    let mut invalid = Vec::new();
    let mut value = canonical.clone();
    value["kind"] = serde_json::json!(1);
    invalid.push(value);
    let mut value = canonical.clone();
    value["content"] = serde_json::json!("arbitrary owner data");
    invalid.push(value);
    let mut value = canonical.clone();
    value["tags"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!(["extra", "authority"]));
    invalid.push(value);
    let mut value = canonical;
    value["tags"][2][1] = serde_json::json!("unknown");
    invalid.push(value);
    for template in invalid {
        let state = signer.approve(&PersonSignerRequest {
            unsigned_event_template: serde_json::to_string(&template).unwrap(),
        });
        assert!(
            matches!(state, PersonSignerState::InvalidResponse { .. }),
            "{template}: {state:?}"
        );
    }
}

#[test]
fn wire_values_never_show_words_and_erase_them() {
    let mut request = LocalSignerWireRequest::SignWithWords {
        words: WORDS.into(),
        expected_device_public_key: WORDS_PUBLIC_KEY.into(),
        request: PersonSignerRequest {
            unsigned_event_template: "{}".into(),
        },
    };
    assert_eq!(format!("{request:?}"), "SignWithWords");
    request.erase();
    let LocalSignerWireRequest::SignWithWords { words, .. } = &request else {
        unreachable!()
    };
    assert!(words.is_empty());

    let mut confirm = LocalSignerWireRequest::ConfirmWords {
        answers: vec!["leader".into(), "ring".into(), "bean".into()],
        request: PersonSignerRequest {
            unsigned_event_template: "{}".into(),
        },
    };
    assert_eq!(format!("{confirm:?}"), "ConfirmWords");
    confirm.erase();
    let LocalSignerWireRequest::ConfirmWords { answers, .. } = &confirm else {
        unreachable!()
    };
    assert!(answers.iter().all(String::is_empty));

    let mut response = LocalSignerWireResponse::Words {
        words: WORDS.into(),
        positions: [1, 4, 12],
    };
    let shown = format!("{response:?}");
    assert!(!shown.contains("leader") && shown.contains("[1, 4, 12]"));
    response.erase();
    let LocalSignerWireResponse::Words { words, .. } = &response else {
        unreachable!()
    };
    assert!(words.is_empty());
    let shown = ShownWords {
        words: Zeroizing::new(WORDS.into()),
        positions: [1, 4, 12],
    };
    assert!(!format!("{shown:?}").contains("leader"));
}

#[tokio::test]
async fn korrid_reaches_the_signer_over_its_socket() {
    let device = device();
    let sockets = tempfile::tempdir().unwrap();
    let socket = sockets.path().join("signer.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let signer = Arc::new(LocalSigner::new(&device.key).unwrap());
    let server = {
        let signer = signer.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                serve_connection(stream, signer.clone()).await.unwrap();
            }
        })
    };
    let client = UnixLocalSigner::new(socket);
    let request = statement(&device, OwnerStatementStatus::Owned, 100);

    let shown = client.make_words().await.unwrap();
    assert!(signer.holds_words());
    let mut wrong = answers(&shown);
    // "zoo" is itself a recovery word; never let it be the right answer.
    wrong[0] = if wrong[0] == "zoo" { "zone" } else { "zoo" }.into();
    assert!(matches!(
        client.confirm_words(&wrong, request.clone()).await.unwrap(),
        ConfirmedWords::Mismatch { .. }
    ));
    let ConfirmedWords::Signed(state) = client
        .confirm_words(&answers(&shown), request.clone())
        .await
        .unwrap()
    else {
        panic!("the right words did not confirm");
    };
    approved(state);
    assert!(!signer.holds_words());

    let (owner, _) = approved(
        client
            .sign_with_words(WORDS, &device.key, request.clone())
            .await
            .unwrap(),
    );
    assert_eq!(owner, WORDS_PUBLIC_KEY);
    assert!(client
        .sign_with_words("not words", &device.key, request)
        .await
        .is_err());

    client.make_words().await.unwrap();
    client.forget_words().await.unwrap();
    assert!(!signer.holds_words());
    server.abort();
}

#[test]
fn a_statement_signed_from_words_makes_the_device_a_member_for_its_peers() {
    use std::os::unix::fs::PermissionsExt;
    // A device of the person key that the words make, bound with them.
    let peer_root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(peer_root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut peer = DeviceIdentity::load_or_create(peer_root.path()).unwrap();
    let peer_key = peer.device_public_key().unwrap().to_owned();
    let template = peer
        .owner_statement_template(OwnerStatementStatus::Owned, 50)
        .unwrap();
    let (owner, event) = approved(
        sign_with_words(
            WORDS,
            &peer_key,
            &PersonSignerRequest {
                unsigned_event_template: template.clone(),
            },
        )
        .unwrap(),
    );
    peer.apply_signed_owner_binding(&template, &owner, &event)
        .unwrap();

    // A device moved to the same person key with the words (ADR 0073).
    let moved = device();
    let request = statement(&moved, OwnerStatementStatus::Owned, 100);
    let (owner, statement) = approved(sign_with_words(WORDS, &moved.key, &request).unwrap());
    assert_eq!(owner, WORDS_PUBLIC_KEY);

    let resources = crate::FederationResources::open(peer_root.path()).unwrap();
    resources
        .directory
        .apply_membership(&resources.directory.begin_work().unwrap(), &[statement])
        .unwrap();
    let members = resources.directory.snapshot().unwrap();
    let member = members
        .iter()
        .find(|member| member.device_public_key == moved.key)
        .expect("the peer did not accept the moved device");
    assert_eq!(member.membership.owner_public_key, WORDS_PUBLIC_KEY);
    assert!(member.membership.vouchers.is_empty());
}
