#[path = "fixtures/readable.rs"]
mod readable;
use korrid::config::{classify_snapshot_support, decode_config_documents};
use readable::*;

#[test]
fn library_decodes_as_supported_readable_contract() {
    let snapshot = decode_config_documents(DEVICE, LIBRARY).unwrap();
    assert_eq!(
        snapshot.host.as_ref().unwrap().title.as_deref(),
        Some("usu")
    );
    assert_eq!(
        snapshot.library[GBA_ID].title.as_deref(),
        Some("Wario Land 4")
    );
    assert_eq!(
        snapshot.library[GBA_ID].releases[0]
            .identity
            .as_ref()
            .unwrap()
            .value(),
        GBA_RELEASE
    );
    classify_snapshot_support(&snapshot).unwrap();
}
#[test]
fn empty_documents_decode_as_the_initial_snapshot() {
    assert_eq!(
        decode_config_documents("{}", "{}").unwrap(),
        korrid::config::ConfigSnapshot::default()
    );
}
#[test]
fn readable_sections_keep_non_executable_opinions_decodable() {
    let device = include_str!("fixtures/readable-sections/device-all-sections.yaml");
    let snapshot = decode_config_documents(device, "{}").unwrap();
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
fn unknown_fields_old_sections_and_wrong_document_sections_fail() {
    for device in [
        "unexpected: {}",
        "games: {}",
        "releases: {}",
        "library: {}",
        "host: {typo: true}",
    ] {
        assert!(decode_config_documents(device, "{}").is_err(), "{device}");
    }
    for library in [
        "host: {}",
        "locations: {}",
        "sources: {}",
        "games: {}",
        "releases: {}",
    ] {
        assert!(decode_config_documents("{}", library).is_err(), "{library}");
    }
    for field in [
        "target: {}",
        "launch: {}",
        "app: retroarch",
        "runner: mgba",
        "typo: true",
    ] {
        let library = format!("{}        {field}\n", gba_library());
        assert!(decode_config_documents("{}", &library).is_err(), "{field}");
    }
}
#[test]
fn catalog_releases_cannot_carry_hook_commands() {
    let library = format!(
        "{}        hooks:\n          before:\n            - run: 'true'\n",
        gba_library()
    );
    let error = decode_config_documents("{}", &library).unwrap_err();
    assert!(error.to_string().contains("commands"), "{error}");
    decode_config_documents("hooks: {device-check: {before: [{run: 'true'}]}}", "{}").unwrap();
}
#[test]
fn explicit_null_is_not_absence() {
    for device in [
        "host: {title: null}",
        "sources: null",
        "provider-links: null",
    ] {
        assert!(decode_config_documents(device, "{}").is_err());
    }
    for library in [
        gba_library().replace("system: gba", "system: null"),
        gba_library().replace("identity: {", "identity: null # {"),
    ] {
        assert!(decode_config_documents("{}", &library).is_err());
    }
}
#[test]
fn ids_are_readable_local_segments_not_ulids() {
    for id in ["wario", "wario-land-4-2", "game.v1", "game_alt"] {
        decode_config_documents("{}", &gba_library().replace(GBA_ID, id)).unwrap();
    }
    for id in [
        "Bad Id",
        "01K4J6K8Y00000000000000002",
        "../game",
        "game/part",
        "-game",
    ] {
        assert!(
            decode_config_documents("{}", &gba_library().replace(GBA_ID, id)).is_err(),
            "{id}"
        );
    }
}
#[test]
fn duplicate_keys_release_ids_and_empty_release_lists_fail() {
    assert!(decode_config_documents("storage: {a: {root: /a}, a: {root: /b}}", "{}").is_err());
    let mut library: serde_yaml::Value = serde_yaml::from_str(&gba_library()).unwrap();
    let releases = library["library"][GBA_ID]["releases"]
        .as_sequence_mut()
        .unwrap();
    releases.push(releases[0].clone());
    assert!(decode_config_documents("{}", &serde_yaml::to_string(&library).unwrap()).is_err());
    library["library"][GBA_ID]["releases"] = serde_yaml::Value::Sequence(vec![]);
    assert!(decode_config_documents("{}", &serde_yaml::to_string(&library).unwrap()).is_err());
}
#[test]
fn content_keys_validate_sha256_sha1_chd_and_parts() {
    decode_config_documents("{}", &gba_library()).unwrap();
    for key in [
        GBA_RELEASE.to_uppercase(),
        "sha1:deadbeef".into(),
        "crc32:abc".into(),
    ] {
        assert!(decode_config_documents("{}", &gba_library().replace(GBA_RELEASE, &key)).is_err());
    }
    let chd = gba_library()
        .replace("kind: hash", "kind: chd")
        .replace(GBA_RELEASE, &format!("sha1:{}", "a".repeat(40)));
    decode_config_documents("{}", &chd).unwrap();
    assert!(
        decode_config_documents("{}", &gba_library().replace("kind: hash", "kind: chd")).is_err()
    );
}
#[test]
fn sources_and_provider_links_are_device_data_and_have_valid_references() {
    let device = format!("storage: {{gba: {{root: /games}}}}\nsources: {{gba: {{kind: [files], storage: gba, system: gba}}}}\nprovider-links:\n  '@test:games/wario':\n    provider: '@test:games'\n    playable: {GBA_ID}\n    refs: [{{kind: provider-item-id, value: wario}}]\n");
    let snapshot = decode_config_documents(&device, &gba_library()).unwrap();
    assert_eq!(snapshot.sources["gba"].system.0, "gba");
    assert_eq!(
        snapshot.provider_links["@test:games/wario"].playable.0,
        GBA_ID
    );
    assert!(decode_config_documents(
        &device.replace("storage: gba", "storage: missing"),
        &gba_library()
    )
    .is_err());
    assert!(decode_config_documents(
        &device.replace(&format!("playable: {GBA_ID}"), "playable: missing"),
        &gba_library()
    )
    .is_err());
}
#[test]
fn locations_keep_legacy_fields_and_validate_discovery_times() {
    let good = gba_locations("roms", "wl4.gba", true);
    decode_config_documents(&good, &gba_library()).unwrap();
    for device in [
        good.replace("storage: roms", "storage: null"),
        good.replace("path: wl4.gba", "path: null"),
        good.replace("2026-08-05T00:00:00Z", "yesterday"),
    ] {
        assert!(decode_config_documents(&device, &gba_library()).is_err());
    }
    for field in ["kind: file", "typo: true", "discovery: null"] {
        assert!(decode_config_documents(
            &format!("{}      {field}\n", gba_locations("roms", "wl4.gba", false)),
            &gba_library()
        )
        .is_err());
    }
    assert!(decode_config_documents(
        &ANDROID_DEVICE.replace("ref: com.playdigious.tmnt", "ref: wrong.package"),
        ANDROID_LIBRARY
    )
    .is_err());
}

#[test]
fn player_count_defaults_only_from_device_configuration() {
    assert_eq!(korrid::config::ConfigSnapshot::default().player_count(), 4);
    for device in [
        "{}",
        "host: {}",
        "host: {title: handheld}",
        "host: {preferences: {}}",
    ] {
        let snapshot = decode_config_documents(device, "{}").unwrap();
        assert_eq!(snapshot.player_count(), 4, "{device}");
        classify_snapshot_support(&snapshot).unwrap();
    }
    for count in [1, 6, 255] {
        let device = format!("host: {{preferences: {{playerCount: {count}}}}}");
        let snapshot = decode_config_documents(&device, &gba_library()).unwrap();
        assert_eq!(snapshot.player_count(), count);
        classify_snapshot_support(&snapshot).unwrap();
    }
}

#[test]
fn player_count_rejects_non_positive_out_of_range_and_non_integer_values() {
    for value in [
        "0", "-1", "256", "1.5", "6.0", "null", "'6'", "true", "false", "[]", "{}",
    ] {
        let device = format!("host: {{preferences: {{playerCount: {value}}}}}");
        assert!(decode_config_documents(&device, "{}").is_err(), "{value}");
    }
    for device in [
        "host: {preferences: null}",
        "host: {preferences: {playerCount: 6, typo: true}}",
        "host: {preferences: {player_count: 6}}",
        "host: {playerCount: 6}",
    ] {
        assert!(decode_config_documents(device, "{}").is_err(), "{device}");
    }
}

#[test]
fn player_count_is_not_an_inheritable_or_catalog_preference() {
    for device in [
        "profiles: {local: {preferences: {playerCount: 6}}}",
        "systems: {gba: {preferences: {playerCount: 6}}}",
        "families: {'@korri:retroarch': {preferences: {playerCount: 6}}}",
        "runners: {'@korri:mgba/mgba': {preferences: {playerCount: 6}}}",
    ] {
        assert!(decode_config_documents(device, "{}").is_err(), "{device}");
    }
    let games = format!("{}    preferences: {{playerCount: 6}}\n", gba_library());
    assert!(decode_config_documents("{}", &games).is_err());
    let releases = format!("{}    preferences: {{playerCount: 6}}\n", gba_library());
    assert!(decode_config_documents("{}", &releases).is_err());
    assert!(serde_yaml::from_str::<korrid::config::Preferences>("playerCount: 6").is_err());
    assert!(serde_yaml::from_str::<korrid::config::InheritableLayer>(
        "preferences: {playerCount: 6}"
    )
    .is_err());
    assert!(decode_config_documents("{}", "host: {preferences: {playerCount: 6}}").is_err());
}

#[test]
fn player_count_does_not_enable_unsupported_launch_preferences() {
    for launch in [
        "{}",
        "{video: {fullscreen: true, resolution: {width: 640, height: 480}, aspect-ratio: '4:3'}}",
        "{audio: {volume: 75}}",
    ] {
        for count in ["", "playerCount: 6, "] {
            let device = format!("host: {{preferences: {{{count}launch: {launch}}}}}");
            let snapshot = decode_config_documents(&device, "{}").unwrap();
            assert!(classify_snapshot_support(&snapshot)
                .unwrap_err()
                .to_string()
                .contains("host.preferences"));
        }
    }
    for launch in [
        "null",
        "{typo: true}",
        "{audio: {volume: 101}}",
        "{video: {fullscreen: null}}",
    ] {
        let device = format!("host: {{preferences: {{playerCount: 6, launch: {launch}}}}}");
        assert!(decode_config_documents(&device, "{}").is_err(), "{launch}");
    }
    let releases = format!(
        "{}        preferences: {{launch: {{audio: {{volume: 75}}}}}}\n",
        gba_library()
    );
    let snapshot =
        decode_config_documents("host: {preferences: {playerCount: 6}}", &releases).unwrap();
    assert!(classify_snapshot_support(&snapshot).is_err());
}

#[test]
fn unsupported_populated_behavior_is_reported_explicitly() {
    let snapshot = decode_config_documents(
        "host:\n  moonlight:\n    platform:\n      name: v4l2m2m",
        "{}",
    )
    .unwrap();
    assert!(classify_snapshot_support(&snapshot)
        .unwrap_err()
        .to_string()
        .contains("host.moonlight"));
}

const PROVIDER_LIBRARY: &str = "library:
  flinthook:
    title: Flinthook
    releases:
      - id: steam
        system: steam
        identity: {kind: provider, value: '@korri:steam/401710'}
";
const PROVIDER_DEVICE: &str = "locations:
  '@korri:steam/401710': [{provider: '@korri:steam', ref: '401710'}]
";

#[test]
fn provider_identity_decodes_and_keeps_its_game_identity() {
    let snapshot = decode_config_documents(PROVIDER_DEVICE, PROVIDER_LIBRARY).unwrap();
    assert_eq!(
        snapshot.library["flinthook"].releases[0]
            .identity
            .as_ref()
            .unwrap()
            .value(),
        "@korri:steam/401710"
    );
    assert_eq!(
        snapshot.game_identity("flinthook"),
        Some(korrid::GameIdentity::Provider(
            korrid::GameProviderIdentity {
                provider: "@korri:steam".into(),
                provider_ref: "401710".into(),
            }
        ))
    );
}

#[test]
fn provider_identity_rejects_hashes_invalid_keys_and_duplicate_identities() {
    for key in [
        format!("sha256:{}", "a".repeat(64)),
        format!("sha1:{}", "a".repeat(40)),
        "steam/401710".into(),
        "@korri:steam/".into(),
        "@korri/401710".into(),
    ] {
        let library = PROVIDER_LIBRARY.replace("@korri:steam/401710", &key);
        assert!(decode_config_documents("{}", &library).is_err(), "{key}");
    }
    let duplicate = format!(
        "{PROVIDER_LIBRARY}      - id: duplicate\n        identity: {{kind: provider, value: '@korri:steam/401710'}}\n"
    );
    assert!(decode_config_documents(PROVIDER_DEVICE, &duplicate).is_err());
}

#[test]
fn provider_identity_requires_a_matching_provider_location() {
    for location in [
        "{storage: roms, path: game.rom}",
        "{storage: roms, files: [{id: game, path: game.rom}]}",
        "{path: /bin/game}",
        "{value: 'https://example.invalid/game'}",
        "{provider: '@korri:steam', ref: 'other'}",
    ] {
        let device = format!("locations: {{'@korri:steam/401710': [{location}]}}");
        assert!(
            decode_config_documents(&device, PROVIDER_LIBRARY).is_err(),
            "{location}"
        );
    }
    // A catalog entry need not have an available device location yet.
    decode_config_documents("{}", PROVIDER_LIBRARY).unwrap();
}
