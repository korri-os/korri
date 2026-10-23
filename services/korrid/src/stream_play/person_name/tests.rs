use super::*;
use crate::{
    identity::{DeviceIdentity, OwnerStatementStatus},
    local_signer::LocalPersonSigner,
    remote_signer::{PersonSigner, PersonSignerRequest, PersonSignerState},
};
use ::nostr::event::FinalizeEvent;
use ::nostr::prelude::{EventBuilder, Keys, Kind, Tag, Timestamp};

fn signed(keys: &Keys, kind: u16, content: &str, created_at: u64, tags: Vec<Vec<&str>>) -> String {
    EventBuilder::new(Kind::Custom(kind), content)
        .custom_created_at(Timestamp::from(created_at))
        .tags(tags.into_iter().map(|tag| Tag::parse(tag).unwrap()))
        .finalize(keys)
        .unwrap()
        .as_json()
}

#[test]
fn setup_name_precedes_profile_and_empty_setup_falls_back_to_profile() {
    let person = Keys::generate();
    let key = person.public_key().to_hex();
    let setup = signed(
        &person,
        KIND,
        r#"{"name":"Simon"}"#,
        10,
        vec![vec!["d", D_TAG]],
    );
    let profile = signed(
        &person,
        0,
        r#"{"display_name":"Profile","name":"Name"}"#,
        20,
        vec![],
    );
    assert_eq!(
        resolve(&[profile.clone(), setup], &key).as_deref(),
        Some("Simon")
    );
    let empty = signed(&person, KIND, r#"{"name":""}"#, 30, vec![vec!["d", D_TAG]]);
    assert_eq!(resolve(&[profile, empty], &key).as_deref(), Some("Profile"));
    let profile = signed(
        &person,
        0,
        r#"{"display_name":"","name":"Name"}"#,
        20,
        vec![],
    );
    assert_eq!(resolve(&[profile], &key).as_deref(), Some("Name"));
    assert_eq!(resolve(&[], &key), None);
    assert!(template(&"x".repeat(65), 1).is_err());
    assert!(template("", 1).is_ok());
}

#[tokio::test]
async fn local_person_signer_stores_a_name_that_survives_restart_without_a_network() {
    let root = tempfile::tempdir().unwrap();
    let signer_root = tempfile::tempdir().unwrap();
    let mut identity = DeviceIdentity::load_or_create(root.path()).unwrap();
    let signer = LocalPersonSigner::load_or_create(
        signer_root.path(),
        &identity.device_public_key().unwrap(),
    )
    .unwrap();
    let binding = identity
        .owner_statement_template(OwnerStatementStatus::Owned, 1)
        .unwrap();
    let PersonSignerState::Approved {
        owner_public_key,
        signed_event_json,
        ..
    } = signer
        .request(PersonSignerRequest {
            unsigned_event_template: binding.clone(),
        })
        .await
    else {
        panic!("binding failed")
    };
    identity
        .apply_signed_owner_binding(&binding, &owner_public_key, &signed_event_json)
        .unwrap();
    let unsigned = template("Simon", 2).unwrap();
    let PersonSignerState::Approved {
        signed_event_json, ..
    } = signer
        .request(PersonSignerRequest {
            unsigned_event_template: unsigned,
        })
        .await
    else {
        panic!("name failed")
    };
    identity.store_person_name(&signed_event_json).unwrap();
    drop(identity);
    let identity = DeviceIdentity::load_or_create(root.path()).unwrap();
    let local = identity.person_name_event().unwrap();
    assert_eq!(
        verified_name(&local, &owner_public_key).as_deref(),
        Some("Simon")
    );
    let other = Keys::generate();
    let foreign = signed(
        &other,
        KIND,
        r#"{"name":"Other"}"#,
        3,
        vec![vec!["d", D_TAG]],
    );
    assert!(identity.store_person_name(&foreign).is_err());
    std::fs::write(root.path().join("identity/person-name.event.json"), foreign).unwrap();
    assert!(identity.person_name_event().is_none());
    std::fs::write(
        root.path().join("identity/person-name.event.json"),
        "invalid",
    )
    .unwrap();
    assert!(identity.person_name_event().is_none());
}

#[tokio::test]
async fn relay_records_are_signed_person_authored_and_resolve_on_another_reader() {
    use crate::relay::{CoordinatedRelays, InProcessRelayNetwork, RelayList};
    use std::sync::Arc;
    let root = tempfile::tempdir().unwrap();
    let identity = DeviceIdentity::load_or_create(root.path()).unwrap();
    let person = Keys::generate();
    let public_key = person.public_key().to_hex();
    let list = RelayList::configured(vec!["wss://relay.example.com".into()]).unwrap();
    let network = Arc::new(InProcessRelayNetwork::new(&list));
    let publisher =
        CoordinatedRelays::new(list.clone(), identity.clone(), network.clone()).unwrap();
    let event = signed(
        &person,
        KIND,
        r#"{"name":"Simon"}"#,
        5,
        vec![vec!["d", D_TAG]],
    );
    assert!(publisher
        .publish_person_name(&event, &public_key)
        .await
        .unwrap()
        .accepted());
    let reader = CoordinatedRelays::new(list, identity, network).unwrap();
    let records = reader.read_person_records(&public_key).await.unwrap();
    assert_eq!(resolve(&records, &public_key).as_deref(), Some("Simon"));
}
