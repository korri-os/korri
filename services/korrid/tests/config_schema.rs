#[path = "fixtures/readable.rs"]
mod readable;
use korrid::config::{classify_snapshot_support, decode_config_documents};
use readable::*;

#[test]
fn checkpoint_documents_decode_as_supported_readable_contract() {
    let snapshot =
        decode_config_documents(ANDROID_DEVICE, ANDROID_GAMES, ANDROID_RELEASES).unwrap();
    assert_eq!(snapshot.host.unwrap().title.as_deref(), Some("usu"));
    assert_eq!(snapshot.games[ANDROID_ID].title, "TMNT: Shredder's Revenge");
    assert_eq!(snapshot.releases[ANDROID_RELEASE].game.0, ANDROID_ID);
    classify_snapshot_support(
        &decode_config_documents(ANDROID_DEVICE, ANDROID_GAMES, ANDROID_RELEASES).unwrap(),
    )
    .unwrap();
}

#[test]
fn empty_documents_decode_as_the_initial_snapshot() {
    let snapshot = decode_config_documents("{}", "{}", "{}").unwrap();
    assert_eq!(snapshot, korrid::config::ConfigSnapshot::default());
}

#[test]
fn readable_sections_keep_non_executable_opinions_decodable() {
    let readable = include_str!("fixtures/readable-sections/device-all-sections.yaml");
    // Executable runners come only from installed contributions. These records
    // are settings opinions and cannot redeclare commands or native files.
    let snapshot = decode_config_documents(readable, "{}", "{}").unwrap();
    assert_eq!(snapshot.storage.len(), 2);
    assert_eq!(snapshot.providers.len(), 2);
    assert_eq!(snapshot.systems.len(), 2);
    assert_eq!(snapshot.families.len(), 1);
    assert_eq!(snapshot.runners.len(), 1);
    assert_eq!(snapshot.profiles.len(), 1);
    assert_eq!(snapshot.hooks.len(), 1);
    assert!(classify_snapshot_support(&snapshot).is_err());
}

#[test]
fn unknown_top_level_and_nested_fields_fail() {
    for device in [
        "unexpected: {}",
        "host:\n  hooks:\n    before:\n      - run: 'true'\n        comand: typo",
    ] {
        assert!(decode_config_documents(device, "{}", "{}").is_err());
    }
    for section in ["library", "users", "provider-links", "retired", "aliases"] {
        assert!(decode_config_documents(&format!("{section}: {{}}"), "{}", "{}").is_err());
    }
    for field in [
        "target: {}",
        "launch: {use: '@korri:retroarch/retroarch'}",
        "launch: {}",
        "id: gba",
        "app: retroarch",
        "runner: mgba",
    ] {
        let releases = format!("{}    {field}\n", gba_releases());
        assert!(
            decode_config_documents("{}", &gba_games(), &releases).is_err(),
            "{field}"
        );
    }
}

#[test]
fn catalog_releases_cannot_carry_hook_commands() {
    for hook in [
        "hooks:\n      before:\n        - run: 'true'",
        "hooks:\n      after:\n        - run: 'true'",
    ] {
        let releases = format!("{}    {hook}\n", gba_releases());
        let error = decode_config_documents("{}", &gba_games(), &releases).unwrap_err();
        assert!(error.to_string().contains("commands"), "{error}");
    }
    decode_config_documents(
        "hooks:\n  device-check:\n    before:\n      - run: 'true'",
        "{}",
        "{}",
    )
    .unwrap();
}

#[test]
fn explicit_null_is_not_absence() {
    assert!(decode_config_documents("host:\n  title: null", "{}", "{}")
        .unwrap_err()
        .to_string()
        .contains("explicit null"));
    for releases in [
        gba_releases().replace("identity: file", "identity: null"),
        gba_releases().replace("system: gba", "system: null"),
    ] {
        assert!(
            decode_config_documents("{}", &gba_games(), &releases).is_err(),
            "{releases}"
        );
    }
}

#[test]
fn identity_syntax_and_payload_identity_mismatches_fail() {
    assert!(decode_config_documents("providers:\n  android-app: {}", "{}", "{}").is_err());
    assert!(decode_config_documents(
        "providers:\n  '@korri:android-app':\n    id: '@korri:other'",
        "{}",
        "{}"
    )
    .is_err());
    for id in [
        "Bad Id",
        "wl4",
        "81K4J6K8Y00000000000000002",
        "01k4j6k8y00000000000000002",
    ] {
        assert!(decode_config_documents(
            "{}",
            &gba_games().replace(GBA_ID, id),
            &gba_releases().replace(GBA_ID, id)
        )
        .is_err());
    }
}

#[test]
fn fixed_file_ownership_is_enforced_even_for_empty_sections() {
    for (device, games, releases) in [
        ("games: {}", "{}", "{}"),
        ("releases: {}", "{}", "{}"),
        ("{}", "host: {}", "{}"),
        ("{}", "locations: {}", "{}"),
        ("{}", "{}", "games: {}"),
        ("{}", "releases: {}", "{}"),
    ] {
        assert!(decode_config_documents(device, games, releases)
            .unwrap_err()
            .to_string()
            .contains("not allowed"));
    }
}

#[test]
fn duplicate_record_keys_and_release_lists_fail() {
    assert!(decode_config_documents(
        "providers:\n  '@korri:android-app': {}\n  '@korri:android-app': {}",
        "{}",
        "{}"
    )
    .unwrap_err()
    .to_string()
    .contains("duplicate record key"));
    let games = gba_games().replace(
        &format!("['{GBA_RELEASE}']"),
        &format!("['{GBA_RELEASE}', '{GBA_RELEASE}']"),
    );
    assert!(decode_config_documents("{}", &games, &gba_releases()).is_err());
    let games = gba_games().replace(&format!("['{GBA_RELEASE}']"), "[]");
    assert!(decode_config_documents("{}", &games, &gba_releases()).is_err());
}

#[test]
fn artifact_keys_require_lowercase_sha256_or_provider_ref() {
    decode_config_documents("{}", &gba_games(), &gba_releases()).unwrap();
    decode_config_documents(ANDROID_DEVICE, ANDROID_GAMES, ANDROID_RELEASES).unwrap();
    for key in [
        GBA_RELEASE.to_uppercase(),
        GBA_RELEASE.replace("sha256:", "crc32:"),
        "sha1:deadbeef".into(),
        "@korri:android-app/".into(),
        "android-app/com.playdigious.tmnt".into(),
    ] {
        assert!(
            decode_config_documents(
                "{}",
                &gba_games().replace(GBA_RELEASE, &key),
                &gba_releases().replace(GBA_RELEASE, &key)
            )
            .is_err(),
            "{key}"
        );
    }
}

#[test]
fn catalog_links_must_agree_in_both_directions() {
    assert!(decode_config_documents("{}", &gba_games(), "{}").is_err());
    assert!(decode_config_documents("{}", "{}", &gba_releases()).is_err());
    assert!(decode_config_documents(
        "{}",
        &gba_games(),
        &gba_releases().replace(GBA_ID, OTHER_ID)
    )
    .is_err());
}

#[test]
fn locations_keep_legacy_fields_but_reject_kind_and_unknown_fields() {
    let good = gba_locations("roms", "wl4.gba", true);
    decode_config_documents(&good, &gba_games(), &gba_releases()).unwrap();
    for device in [
        good.replace("storage: roms", "storage: null"),
        good.replace("path: wl4.gba", "path: null"),
    ] {
        assert!(decode_config_documents(&device, &gba_games(), &gba_releases()).is_err());
    }
    for field in ["kind: file", "typo: true", "discovery: null"] {
        let device = format!("{}      {field}\n", gba_locations("roms", "wl4.gba", false));
        assert!(
            decode_config_documents(&device, &gba_games(), &gba_releases()).is_err(),
            "{field}"
        );
    }
    assert!(decode_config_documents(
        &ANDROID_DEVICE.replace("ref: com.playdigious.tmnt", "ref: wrong.package"),
        ANDROID_GAMES,
        ANDROID_RELEASES
    )
    .is_err());
}

#[test]
fn unsupported_populated_behavior_is_reported_explicitly() {
    let snapshot = decode_config_documents(
        "host:\n  moonlight:\n    platform:\n      name: v4l2m2m",
        "{}",
        "{}",
    )
    .unwrap();
    assert!(classify_snapshot_support(&snapshot)
        .unwrap_err()
        .to_string()
        .contains("host.moonlight"));
}
