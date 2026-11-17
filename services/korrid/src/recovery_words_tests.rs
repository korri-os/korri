use super::*;
use nostr::nips::nip19::ToBech32;
use rand::{rngs::StdRng, SeedableRng};

/// The test vectors of NIP-06, copied from
/// https://github.com/nostr-protocol/nips/blob/master/06.md on 2026-11-17:
/// words, private key, nsec, public key, npub.
const NIP06_VECTORS: [(&str, &str, &str, &str, &str); 2] = [
    (
        "leader monkey parrot ring guide accident before fence cannon height naive bean",
        "7f7ff03d123792d6ac594bfa67bf6d0c0ab55b6b1fdb6249303fe861f1ccba9a",
        "nsec10allq0gjx7fddtzef0ax00mdps9t2kmtrldkyjfs8l5xruwvh2dq0lhhkp",
        "17162c921dc4d2518f9a101db33695df1afb56ab82f5ff3e5da6eec3ca5cd917",
        "npub1zutzeysacnf9rru6zqwmxd54mud0k44tst6l70ja5mhv8jjumytsd2x7nu",
    ),
    (
        "what bleak badge arrange retreat wolf trade produce cricket blur garlic valid proud rude strong choose busy staff weather area salt hollow arm fade",
        "c15d739894c81a2fcfd3a2df85a0d2c0dbc47a280d092799f144d73d7ae78add",
        "nsec1c9wh8xy5eqdzln7n5t0ctgxjcrdug73gp5yj0x03gntn67h83twssdfhel",
        "d41b22899549e1f3d335a31002cfd382174006e166d3e658e3a5eecdb6463573",
        "npub16sdj9zv4f8sl85e45vgq9n7nsgt5qphpvmf7vk8r5hhvmdjxx4es8rq74h",
    ),
];

fn assert_vector(keys: &Keys, secret: &str, nsec: &str, public: &str, npub: &str) {
    assert_eq!(keys.secret_key().to_secret_hex(), secret);
    assert_eq!(keys.secret_key().to_bech32().unwrap(), nsec);
    assert_eq!(keys.public_key().to_hex(), public);
    assert_eq!(keys.public_key().to_bech32().unwrap(), npub);
}

#[test]
fn twelve_words_make_the_nip06_person_key() {
    let (words, secret, nsec, public, npub) = NIP06_VECTORS[0];
    let keys = RecoveryWords::parse(words).unwrap().person_keys().unwrap();
    assert_vector(&keys, secret, nsec, public, npub);
}

#[test]
fn the_derivation_matches_every_nip06_vector() {
    // Recovery words are 12 words, but the derivation is NIP-06's for any
    // BIP39 length. The 24-word vector checks it through the same function.
    for (words, secret, nsec, public, npub) in NIP06_VECTORS {
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, words).unwrap();
        let keys = derive_person_keys(&mnemonic).unwrap();
        assert_vector(&keys, secret, nsec, public, npub);
    }
}

#[test]
fn only_twelve_words_are_recovery_words() {
    assert_eq!(
        RecoveryWords::parse(NIP06_VECTORS[1].0).unwrap_err(),
        RecoveryWordsError::WordCount { found: 24 }
    );
    assert_eq!(
        RecoveryWords::parse("leader monkey parrot").unwrap_err(),
        RecoveryWordsError::WordCount { found: 3 }
    );
    assert_eq!(
        RecoveryWords::parse("").unwrap_err(),
        RecoveryWordsError::WordCount { found: 0 }
    );
}

#[test]
fn words_are_read_in_any_case_with_any_spaces() {
    let typed =
        "  LEADER Monkey\tparrot ring guide accident\nbefore fence cannon height naive   bean \n";
    let words = RecoveryWords::parse(typed).unwrap();
    assert_eq!(words.phrase().as_str(), NIP06_VECTORS[0].0);
    assert_eq!(
        words.person_keys().unwrap().public_key().to_hex(),
        NIP06_VECTORS[0].3
    );
}

#[test]
fn a_wrong_word_names_its_place_but_not_the_word() {
    let error = RecoveryWords::parse(
        "leader monkey parrot ring guide accident before fence cannon heigth naive bean",
    )
    .unwrap_err();
    assert_eq!(error, RecoveryWordsError::UnknownWord { position: 10 });
    assert!(!error.to_string().contains("heigth"));
    // Two real words in the wrong places break the checksum.
    assert_eq!(
        RecoveryWords::parse(
            "monkey leader parrot ring guide accident before fence cannon height naive bean"
        )
        .unwrap_err(),
        RecoveryWordsError::Checksum
    );
    // Non-ASCII letters are never recovery words.
    assert_eq!(
        RecoveryWords::parse(
            "léader monkey parrot ring guide accident before fence cannon height naive bean"
        )
        .unwrap_err(),
        RecoveryWordsError::UnknownWord { position: 1 }
    );
}

#[test]
fn new_words_are_twelve_words_that_read_back() {
    let first = RecoveryWords::generate();
    let second = RecoveryWords::generate();
    for words in [&first, &second] {
        let phrase = words.phrase();
        assert_eq!(phrase.split(' ').count(), WORD_COUNT);
        let read_back = RecoveryWords::parse(&phrase).unwrap();
        assert_eq!(
            read_back.person_keys().unwrap().public_key(),
            words.person_keys().unwrap().public_key()
        );
    }
    assert_ne!(first.phrase(), second.phrase());
}

#[test]
fn words_come_from_bip39_entropy() {
    // BIP39's first English test vector: 128 zero bits.
    let words = RecoveryWords::from_entropy(&[0; ENTROPY_BYTES]);
    assert_eq!(
        words.phrase().as_str(),
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
    );
    // The same random source makes the same words.
    let one = RecoveryWords::generate_with(&mut StdRng::seed_from_u64(7));
    let two = RecoveryWords::generate_with(&mut StdRng::seed_from_u64(7));
    assert_eq!(one.phrase(), two.phrase());
}

#[test]
fn debug_never_shows_the_words() {
    let words = RecoveryWords::parse(NIP06_VECTORS[0].0).unwrap();
    let shown = format!("{words:?}");
    assert_eq!(shown, "RecoveryWords { .. }");
}

#[test]
fn a_word_is_read_by_its_place_from_one_to_twelve() {
    let words = RecoveryWords::parse(NIP06_VECTORS[0].0).unwrap();
    assert_eq!(words.word(1), Some("leader"));
    assert_eq!(words.word(12), Some("bean"));
    assert_eq!(words.word(0), None);
    assert_eq!(words.word(13), None);
}

#[test]
fn a_confirmation_asks_for_three_different_places_in_order() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..200 {
        let positions = Confirmation::choose_with(&mut rng).positions();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(positions
            .iter()
            .all(|place| (1..=WORD_COUNT).contains(place)));
        seen.extend(positions);
    }
    // Every place can be asked for.
    assert_eq!(seen.len(), WORD_COUNT);
    let positions = Confirmation::choose().positions();
    assert_eq!(
        Confirmation::from_positions(positions).unwrap().positions(),
        positions
    );
}

#[test]
fn positions_from_elsewhere_are_checked() {
    for positions in [[0, 2, 3], [1, 2, 13], [3, 2, 1], [4, 4, 5]] {
        assert_eq!(
            Confirmation::from_positions(positions).unwrap_err(),
            ConfirmationError::Positions
        );
    }
    assert!(Confirmation::from_positions([1, 6, 12]).is_ok());
}

#[test]
fn the_right_words_confirm() {
    let words = RecoveryWords::parse(NIP06_VECTORS[0].0).unwrap();
    let confirmation = Confirmation::from_positions([2, 7, 12]).unwrap();
    assert_eq!(
        confirmation.check(&words, &["monkey", " Before ", "BEAN"]),
        Ok(())
    );
}

#[test]
fn wrong_words_name_their_places() {
    let words = RecoveryWords::parse(NIP06_VECTORS[0].0).unwrap();
    let confirmation = Confirmation::from_positions([2, 7, 12]).unwrap();
    let error = confirmation
        .check(&words, &["monkey", "fence", "beans"])
        .unwrap_err();
    assert_eq!(
        error,
        ConfirmationError::Mismatch {
            positions: vec![7, 12]
        }
    );
    assert_eq!(
        error.to_string(),
        "Words 7, 12 do not match. Check the words you wrote down."
    );
    assert_eq!(
        confirmation
            .check(&words, &["", "before", "bean"])
            .unwrap_err()
            .to_string(),
        "Word 2 does not match. Check the words you wrote down."
    );
    assert_eq!(
        confirmation.check(&words, &["monkey", "before"]),
        Err(ConfirmationError::AnswerCount {
            expected: 3,
            found: 2
        })
    );
}
