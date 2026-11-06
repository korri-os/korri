use super::*;
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::Keys,
    types::Timestamp,
};
use std::io::Cursor;

fn approved(template: &str, owner: &Keys) -> PersonSignerState {
    let value: serde_json::Value = serde_json::from_str(template).unwrap();
    let event = EventBuilder::new(Kind::Custom(30_078), "")
        .tags(value["tags"].as_array().unwrap().iter().map(|tag| {
            Tag::parse(
                tag.as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_str().unwrap()),
            )
            .unwrap()
        }))
        .custom_created_at(Timestamp::from(value["created_at"].as_u64().unwrap()))
        .finalize(owner)
        .unwrap()
        .as_json();
    PersonSignerState::Approved {
        owner_public_key: owner.public_key().to_hex(),
        unsigned_event_template: template.into(),
        signed_event_json: event,
    }
}

fn request(root: &Path) -> String {
    crate::identity_cli::run(
        &["owner-binding-request".into()],
        root,
        42,
        &mut Cursor::new([]),
    )
    .unwrap()
}

#[test]
fn verified_output_is_accepted_by_the_unchanged_tv_import_contract() {
    let tv = tempfile::tempdir().unwrap();
    let template = request(tv.path());
    let owner = Keys::generate();
    let event = verify_result(
        &template,
        &owner.public_key().to_hex(),
        approved(&template, &owner),
    )
    .unwrap();
    let imported = crate::identity_cli::run(
        &["import".into(), "--stdin".into()],
        tv.path(),
        43,
        &mut Cursor::new(&event),
    )
    .unwrap();
    assert!(imported.contains("\"_tag\":\"Owned\""));
    assert!(imported.contains(&owner.public_key().to_hex()));
    assert!(!event.contains("bunker"));
    assert!(!event.contains("private"));
}

#[test]
fn refuses_wrong_owner_device_template_or_unapproved_result() {
    let tv = tempfile::tempdir().unwrap();
    let other_tv = tempfile::tempdir().unwrap();
    let template = request(tv.path());
    let other_template = request(other_tv.path());
    let owner = Keys::generate();
    let other_owner = Keys::generate();
    let key = owner.public_key().to_hex();
    assert!(verify_result(&template, &key, approved(&template, &other_owner)).is_err());
    assert!(verify_result(&template, &key, approved(&other_template, &owner)).is_err());
    let mut state = approved(&other_template, &owner);
    if let PersonSignerState::Approved {
        unsigned_event_template,
        ..
    } = &mut state
    {
        *unsigned_event_template = template.clone();
    }
    assert!(verify_result(&template, &key, state).is_err());
    let mut changed: serde_json::Value = serde_json::from_str(&template).unwrap();
    changed["created_at"] = serde_json::json!(43);
    let mut state = approved(&changed.to_string(), &owner);
    if let PersonSignerState::Approved {
        unsigned_event_template,
        ..
    } = &mut state
    {
        *unsigned_event_template = template.clone();
    }
    assert!(verify_result(&template, &key, state).is_err());
    assert!(verify_result(
        &template,
        &key,
        PersonSignerState::Denied {
            message: "declined".into()
        }
    )
    .is_err());
}

#[test]
fn operator_inputs_are_bounded_and_malformed_templates_never_reach_the_signer() {
    assert!(bounded(&mut Cursor::new(vec![b'x'; MAX_BYTES + 1])).is_err());
    assert!(bounded(&mut Cursor::new([255])).is_err());
    assert_eq!(bounded(&mut Cursor::new(b"public")).unwrap(), "public");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        assert!(sign("{}", &"11".repeat(32), "bunker://never-contacted")
            .await
            .is_err());
        let tv = tempfile::tempdir().unwrap();
        let template = request(tv.path());
        assert!(sign(&template, "not-a-key", "bunker://never-contacted")
            .await
            .is_err());
        assert!(sign(
            &template,
            &Keys::generate().public_key().to_hex(),
            "nostrconnect://unsupported"
        )
        .await
        .is_err());
    });
}

#[test]
fn connection_diagnostics_do_not_echo_signer_text_or_connection_secrets() {
    let message = connection_failure(crate::remote_signer::RemoteSignerError::Denied {
        method: "connect".into(),
        reason: "bunker://private-connection?secret=one-use-secret".into(),
    });
    assert!(!message.contains("private-connection"));
    assert!(!message.contains("one-use-secret"));
    assert!(!message.contains("bunker://"));
}
