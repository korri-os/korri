use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use super::*;
use crate::plugin::{BytesRule, DeclaredRelease, HeaderRule, RemainderRule};

fn rules(system: &str, extensions: &[&str]) -> ContentRules {
    ContentRules {
        system: system.into(),
        extensions: extensions.iter().map(|ext| (*ext).into()).collect(),
        ..ContentRules::default()
    }
}

fn scan(root: &Path, members: &[&ContentRules], group: bool) -> SourceScan {
    scan_options(
        root,
        members,
        group,
        &[],
        &TraversalBudget::default(),
        &|_, _, _| None,
        &|_| vec![],
        &mut HashCache::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn scan_options(
    root: &Path,
    members: &[&ContentRules],
    group: bool,
    excluded: &[PathBuf],
    budget: &TraversalBudget,
    assigned: &dyn Fn(&str, &str, &str) -> Option<String>,
    declared: &dyn Fn(&str) -> Vec<String>,
    cache: &mut HashCache,
) -> SourceScan {
    let source = SourceInput {
        source_id: "games",
        storage_id: "selected",
        root,
        system: "paired",
        excluded_roots: excluded,
    };
    identify_with_rules(&source, members, group, assigned, declared, cache, budget)
}

fn write(root: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn lynx_sources_silently_skip_unclaimed_object_files_and_readmes() {
    let root = tempfile::tempdir().unwrap();
    let lynx = rules("lynx", &["lnx", "lyx"]);
    assert!(scan(root.path(), &[&lynx], false).units.is_empty());
    write(root.path(), "build.o", b"object");
    write(root.path(), "README.md", b"documentation");
    let report = scan(root.path(), &[&lynx], false);
    assert!(report.units.is_empty());
    assert!(report.attention.is_empty());
    assert_eq!(report.hashed_bytes, 0);
}

#[test]
fn a_readme_in_a_paired_mega_drive_source_is_claimed_by_the_md_extension() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "README.md", b"documentation");
    write(root.path(), "build.o", b"object");
    let md = rules("md", &["md", "gen", "smd", "bin"]);
    let report = scan(root.path(), &[&md], false);
    assert_eq!(report.units.len(), 1);
    assert_eq!(report.units[0].path, "README.md");
    assert_eq!(report.units[0].system, "md");
    assert!(report.attention.is_empty());
}

#[test]
fn mini_v2_build_trees_and_readmes_under_users_are_never_scanned() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "users/default/build/compiled.o", b"object");
    write(root.path(), "users/default/README.md", b"documentation");
    let md = rules("md", &["md", "gen", "smd", "bin"]);
    let excluded = vec![root.path().join("users").canonicalize().unwrap()];
    let report = scan_options(
        root.path(),
        &[&md],
        false,
        &excluded,
        &TraversalBudget::default(),
        &|_, _, _| None,
        &|_| vec![],
        &mut HashCache::default(),
    );
    assert!(report.units.is_empty());
    assert!(report.attention.is_empty());
    assert_eq!(report.hashed_bytes, 0);
    assert!(report.seen_paths.is_empty());
}

#[test]
fn extension_is_case_insensitive_and_layouts_are_source_relative() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "sets/one/Game.P8.PNG", b"cart");
    write(root.path(), "ordinary.png", b"image");
    let mut rule = rules("pico8", &["p8", "p8.png"]);
    rule.layouts = vec!["sets/*/{title}.{ext}".into()];
    let report = scan(root.path(), &[&rule], false);
    assert_eq!(report.units.len(), 1);
    assert_eq!(report.units[0].title, "Game");
    assert_eq!(report.units[0].path, "sets/one/Game.P8.PNG");
    assert!(report.attention.is_empty());
    write(root.path(), "outside.p8", b"cart");
    let report = scan(root.path(), &[&rule], false);
    assert_eq!(report.attention.len(), 1);
    assert_eq!(report.attention[0].kind, AttentionKind::Unmatched);
    assert!(report.attention[0].identity.is_some());
    let previous = report.attention[0].identity.clone();
    write(root.path(), "outside.p8", b"changed cart");
    assert_ne!(
        scan(root.path(), &[&rule], false).attention[0].identity,
        previous
    );
}

#[test]
fn both_digests_are_correct_and_the_cache_survives_serialization() {
    let root = tempfile::tempdir().unwrap();
    let path = write(root.path(), "Game.gb", b"abc");
    let rule = rules("gb", &["gb"]);
    let mut cache = HashCache::default();
    let identity = identity_of(&path, &rule, &mut cache).unwrap();
    assert_eq!(
        identity,
        ContentIdentity::Hash {
            sha256: "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                .into(),
            sha1: "sha1:a9993e364706816aba3e25717850c26c9cd0d89d".into(),
        }
    );
    let mut restored: HashCache =
        serde_json::from_slice(&serde_json::to_vec(&cache).unwrap()).unwrap();
    assert_eq!(identity_of(&path, &rule, &mut restored).unwrap(), identity);
    assert_eq!(restored.read_bytes, 0);
    fs::write(&path, b"changed size").unwrap();
    assert_ne!(identity_of(&path, &rule, &mut restored).unwrap(), identity);
}

#[test]
fn nes_lynx_and_snes_headers_are_excluded_before_both_digests() {
    let root = tempfile::tempdir().unwrap();
    for (system, header_size, magic, remainder) in [
        ("nes", 16, b"NES\x1a".as_slice(), None),
        ("lynx", 64, b"LYNX".as_slice(), None),
        (
            "snes",
            512,
            b"".as_slice(),
            Some(RemainderRule {
                divisor: 1024,
                value: 512,
            }),
        ),
    ] {
        let payload = vec![42; 128 * 1024];
        let mut headered = vec![0; header_size];
        headered[..magic.len()].copy_from_slice(magic);
        headered.extend_from_slice(&payload);
        let plain = write(root.path(), &format!("plain.{system}"), &payload);
        let headered = write(root.path(), &format!("headered.{system}"), &headered);
        let mut rule = rules(system, &[system]);
        rule.header.push(HeaderRule {
            size: header_size as u64,
            bytes: (!magic.is_empty()).then(|| BytesRule {
                offset: 0,
                hex: hex::encode(magic),
            }),
            remainder,
        });
        let mut cache = HashCache::default();
        assert_eq!(
            identity_of(&plain, &rule, &mut cache).unwrap(),
            identity_of(&headered, &rule, &mut cache).unwrap(),
            "{system}"
        );
    }
}

#[test]
fn first_applicable_header_wins_and_cache_does_not_cross_header_rules() {
    let root = tempfile::tempdir().unwrap();
    let path = write(root.path(), "Game.nes", b"NES\x1aabcdefghijklmnopqrst");
    let mut rule = rules("nes", &["nes"]);
    let mut cache = HashCache::default();
    let plain = identity_of(&path, &rule, &mut cache).unwrap();
    for size in [4, 8] {
        rule.header.push(HeaderRule {
            size,
            bytes: Some(BytesRule {
                offset: 0,
                hex: "4e45531a".into(),
            }),
            remainder: None,
        });
    }
    let actual = identity_of(&path, &rule, &mut cache).unwrap();
    let expected = write(root.path(), "payload.nes", b"abcdefghijklmnopqrst");
    assert_eq!(
        actual,
        identity_of(&expected, &rules("nes", &["nes"]), &mut cache).unwrap()
    );
    assert_ne!(actual, plain);
}

#[test]
fn header_and_evidence_bytes_cannot_read_past_64_kib() {
    let root = tempfile::tempdir().unwrap();
    let mut data = vec![0; 65537];
    data[65536] = 42;
    let path = write(root.path(), "Game.gb", &data);
    let mut gb = rules("gb", &["gb"]);
    let gbc = rules("gbc", &["gb"]);
    let bytes = BytesRule {
        offset: 65536,
        hex: "2a".into(),
    };
    gb.evidence.bytes.push(bytes.clone());
    let original = identity_of(&path, &gb, &mut HashCache::default()).unwrap();
    gb.header.push(HeaderRule {
        size: 1,
        bytes: Some(bytes),
        remainder: None,
    });
    assert_eq!(
        identity_of(&path, &gb, &mut HashCache::default()).unwrap(),
        original
    );
    let report = scan(root.path(), &[&gb, &gbc], true);
    assert!(report.units.is_empty());
    assert_eq!(report.attention[0].kind, AttentionKind::Clash);
    assert!(!hash::matches_bytes(
        &data,
        &BytesRule {
            offset: u64::MAX,
            hex: "2a".into()
        }
    ));
}

#[test]
fn cue_tracks_form_one_ordered_identity_and_are_not_separate_units() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "01 track.bin", b"audio");
    write(root.path(), "02.bin", b"data");
    let cue = write(root.path(), "Game.cue", b"FILE \"01 track.bin\" BINARY\n TRACK 01 AUDIO\nFILE 02.bin BINARY\n TRACK 02 MODE1/2352\n");
    let rule = rules("psx", &["cue", "bin"]);
    let mut cache = HashCache::default();
    let track1 = identity_of(&root.path().join("01 track.bin"), &rule, &mut cache).unwrap();
    let track2 = identity_of(&root.path().join("02.bin"), &rule, &mut cache).unwrap();
    let expected = hash::parts_identity(vec![track1.value().into(), track2.value().into()]);
    assert_eq!(identity_of(&cue, &rule, &mut cache).unwrap(), expected);
    let report = scan(root.path(), &[&rule], false);
    assert_eq!(report.units.len(), 1);
    assert_eq!(report.units[0].identity, expected);
    assert_eq!(report.units[0].parts, ["01 track.bin", "02.bin"]);
    assert_eq!(report.hashed_bytes, 9);
    assert!(report.attention.is_empty());
    fs::write(
        cue,
        b"REM changed presentation\nFILE \"01 track.bin\" BINARY\nFILE 02.bin BINARY\n",
    )
    .unwrap();
    assert_eq!(
        scan(root.path(), &[&rule], false).units[0].identity,
        expected
    );
}

#[test]
fn playlist_order_controls_one_unit_and_consumes_nested_cues_and_tracks() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "a.bin", b"first");
    write(root.path(), "a.cue", b"FILE a.bin BINARY\n");
    write(root.path(), "b.bin", b"second");
    write(root.path(), "b.cue", b"FILE b.bin BINARY\n");
    let playlist = write(root.path(), "Game.m3u", b"#EXTM3U\n\nb.cue\n./a.cue\n");
    let rule = rules("psx", &["m3u", "cue", "bin"]);
    let mut cache = HashCache::default();
    let a = identity_of(&root.path().join("a.cue"), &rule, &mut cache).unwrap();
    let b = identity_of(&root.path().join("b.cue"), &rule, &mut cache).unwrap();
    let report = scan(root.path(), &[&rule], false);
    assert_eq!(report.units.len(), 1);
    assert_eq!(
        report.units[0].identity,
        hash::parts_identity(vec![b.value().into(), a.value().into()])
    );
    assert_eq!(report.units[0].parts, ["b.cue", "b.bin", "a.cue", "a.bin"]);
    assert_eq!(report.hashed_bytes, 11);
    fs::write(playlist, b"a.cue\nb.cue\n").unwrap();
    assert_ne!(
        scan(root.path(), &[&rule], false).units[0].identity,
        report.units[0].identity
    );
}

#[test]
fn invalid_descriptors_and_cycles_are_unreadable() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "missing.cue", b"FILE nonexistent.bin BINARY\n");
    write(root.path(), "cycle.m3u", b"cycle.m3u\n");
    write(root.path(), "empty.m3u", b"# just a comment\n");
    let rule = rules("psx", &["cue", "m3u"]);
    let report = scan(root.path(), &[&rule], false);
    assert!(report.units.is_empty());
    assert_eq!(report.attention.len(), 3);
    assert!(report
        .attention
        .iter()
        .all(|item| item.kind == AttentionKind::Unreadable));
}

#[test]
fn chd_reads_rawsha1_and_rejects_old_version_or_bad_magic() {
    let root = tempfile::tempdir().unwrap();
    let mut bytes = vec![0; 124];
    bytes[..8].copy_from_slice(b"MComprHD");
    bytes[8..12].copy_from_slice(&124u32.to_be_bytes());
    bytes[12..16].copy_from_slice(&5u32.to_be_bytes());
    bytes[0x40..0x54].fill(0x12);
    bytes[0x54..0x68].fill(0x34);
    let path = write(root.path(), "Game.chd", &bytes);
    let rule = rules("psx", &["chd"]);
    assert_eq!(
        identity_of(&path, &rule, &mut HashCache::default()).unwrap(),
        ContentIdentity::Chd {
            sha1: format!("sha1:{}", "12".repeat(20))
        }
    );
    bytes[12..16].copy_from_slice(&4u32.to_be_bytes());
    fs::write(&path, &bytes).unwrap();
    assert!(identity_of(&path, &rule, &mut HashCache::default()).is_err());
    bytes[12..16].copy_from_slice(&5u32.to_be_bytes());
    bytes[0] = b'X';
    fs::write(path, bytes).unwrap();
    let report = scan(root.path(), &[&rule], false);
    assert_eq!(report.attention[0].kind, AttentionKind::Unreadable);
}

#[test]
fn multi_disc_sets_use_numeric_order_and_keep_folder_version_region_boundaries() {
    let root = tempfile::tempdir().unwrap();
    let mut rule = rules("psx", &["bin"]);
    rule.layouts = vec!["**/{title} (Disc {disc}) v{version} ({region}).{ext}".into()];
    for name in [
        "Game (Disc 10) v1 (USA).bin",
        "Game (Disc 2) v1 (USA).bin",
        "Game (Disc 1) v2 (USA).bin",
        "Game (Disc 1) v1 (EU).bin",
        "other/Game (Disc 1) v1 (USA).bin",
    ] {
        write(root.path(), name, name.as_bytes());
    }
    let report = scan(root.path(), &[&rule], false);
    assert_eq!(report.units.len(), 4);
    let combined = report
        .units
        .iter()
        .find(|unit| unit.parts.len() == 2)
        .unwrap();
    assert_eq!(combined.title, "Game");
    assert_eq!(
        combined.parts,
        ["Game (Disc 2) v1 (USA).bin", "Game (Disc 10) v1 (USA).bin"]
    );
    assert!(matches!(combined.identity, ContentIdentity::Parts { .. }));
}

#[test]
fn disc_labels_are_lexical_when_any_label_is_not_numeric() {
    let root = tempfile::tempdir().unwrap();
    let mut rule = rules("psx", &["bin"]);
    rule.layouts = vec!["{title}-{disc}.{ext}".into()];
    for disc in ["B", "A", "2"] {
        write(root.path(), &format!("Game-{disc}.bin"), disc.as_bytes());
    }
    assert_eq!(
        scan(root.path(), &[&rule], false).units[0].parts,
        ["Game-2.bin", "Game-A.bin", "Game-B.bin"]
    );
}

#[test]
fn gb_gbc_group_clashes_without_evidence_but_pairing_does_not_need_evidence() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "Game.gb", b"game");
    let gb = rules("gb", &["gb"]);
    let gbc = rules("gbc", &["gb"]);
    let report = scan(root.path(), &[&gb, &gbc], true);
    assert!(report.units.is_empty());
    assert_eq!(report.attention[0].kind, AttentionKind::Clash);
    assert_eq!(report.attention[0].candidates, ["gb", "gbc"]);
    let report = scan(root.path(), &[&gb], false);
    assert_eq!(report.units[0].evidence, EvidenceLevel::Extension);
}

#[test]
fn group_rank_is_declared_hash_then_bytes_then_name_then_extension() {
    let root = tempfile::tempdir().unwrap();
    let path = write(root.path(), "Game.gb", b"game");
    let mut gb = rules("gb", &["gb"]);
    let mut gbc = rules("gbc", &["gb"]);
    gb.evidence.names = vec!["^Game".into()];
    assert_eq!(
        scan(root.path(), &[&gb, &gbc], true).units[0].evidence,
        EvidenceLevel::Name
    );
    gbc.evidence.bytes.push(BytesRule {
        offset: 0,
        hex: hex::encode(b"game"),
    });
    let report = scan(root.path(), &[&gb, &gbc], true);
    assert_eq!(report.units[0].system, "gbc");
    assert_eq!(report.units[0].evidence, EvidenceLevel::Bytes);
    let identity = identity_of(&path, &gb, &mut HashCache::default()).unwrap();
    let ContentIdentity::Hash { sha1, .. } = identity else {
        panic!()
    };
    let lookup = |hash: &str| {
        if hash == sha1 {
            vec!["gb".into()]
        } else {
            vec![]
        }
    };
    let report = scan_options(
        root.path(),
        &[&gb, &gbc],
        true,
        &[],
        &TraversalBudget::default(),
        &|_, _, _| None,
        &lookup,
        &mut HashCache::default(),
    );
    assert_eq!(report.units[0].system, "gb");
    assert_eq!(report.units[0].evidence, EvidenceLevel::DeclaredHash);
    // A declared hash without a matching member system carries no evidence.
    let report = scan_options(
        root.path(),
        &[&gb, &gbc],
        true,
        &[],
        &TraversalBudget::default(),
        &|_, _, _| None,
        &|_| vec!["nes".into()],
        &mut HashCache::default(),
    );
    assert_eq!(report.units[0].system, "gbc");
}

#[test]
fn assignment_overrides_a_clash_only_while_bytes_match() {
    let root = tempfile::tempdir().unwrap();
    let path = write(root.path(), "Game.gb", b"game");
    let gb = rules("gb", &["gb"]);
    let gbc = rules("gbc", &["gb"]);
    let identity = identity_of(&path, &gb, &mut HashCache::default()).unwrap();
    let assigned = |source: &str, path: &str, hash: &str| {
        (source == "games" && path == "Game.gb" && hash == identity.value()).then(|| "gbc".into())
    };
    let report = scan_options(
        root.path(),
        &[&gb, &gbc],
        true,
        &[],
        &TraversalBudget::default(),
        &assigned,
        &|_| vec![],
        &mut HashCache::default(),
    );
    assert_eq!(report.units[0].system, "gbc");
    assert_eq!(report.units[0].evidence, EvidenceLevel::Assigned);
    fs::write(path, b"different").unwrap();
    let report = scan_options(
        root.path(),
        &[&gb, &gbc],
        true,
        &[],
        &TraversalBudget::default(),
        &assigned,
        &|_| vec![],
        &mut HashCache::default(),
    );
    assert!(report.units.is_empty());
    assert_eq!(report.attention[0].kind, AttentionKind::Clash);
}

#[test]
fn every_traversal_budget_is_enforced_and_descriptors_cannot_bypass_it() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "a.m3u", b"z.bin\n");
    write(root.path(), "z.bin", b"game");
    write(root.path(), "nested/Game.bin", b"other");
    let rule = rules("psx", &["m3u", "bin"]);
    for budget in [
        TraversalBudget {
            max_entries: 0,
            ..TraversalBudget::default()
        },
        TraversalBudget {
            max_directories: 0,
            ..TraversalBudget::default()
        },
        TraversalBudget {
            max_sortable_entries: 1,
            ..TraversalBudget::default()
        },
    ] {
        let report = scan_options(
            root.path(),
            &[&rule],
            false,
            &[],
            &budget,
            &|_, _, _| None,
            &|_| vec![],
            &mut HashCache::default(),
        );
        assert!(report.units.is_empty());
        assert_eq!(report.hashed_bytes, 0);
        assert_eq!(
            report.diagnostics[0].code,
            DiscoveryDiagnosticCode::TraversalLimitReached
        );
    }
    let budget = TraversalBudget {
        max_entries: 1,
        ..TraversalBudget::default()
    };
    let report = scan_options(
        root.path(),
        &[&rule],
        false,
        &[],
        &budget,
        &|_, _, _| None,
        &|_| vec![],
        &mut HashCache::default(),
    );
    assert_eq!(report.hashed_bytes, 0);
    assert_eq!(report.attention[0].kind, AttentionKind::Unreadable);
    let budget = TraversalBudget {
        max_depth: 0,
        ..TraversalBudget::default()
    };
    let report = scan_options(
        root.path(),
        &[&rule],
        false,
        &[],
        &budget,
        &|_, _, _| None,
        &|_| vec![],
        &mut HashCache::default(),
    );
    assert!(!report
        .units
        .iter()
        .any(|unit| unit.path.starts_with("nested/")));
    assert!(report
        .diagnostics
        .iter()
        .any(|item| item.code == DiscoveryDiagnosticCode::TraversalLimitReached));
}

#[cfg(unix)]
#[test]
fn symlinks_and_descriptors_cannot_escape_root_or_enter_plugin_data() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let secret = write(outside.path(), "secret.bin", b"secret");
    symlink(&secret, root.path().join("escaped.bin")).unwrap();
    symlink(outside.path(), root.path().join("escaped-dir")).unwrap();
    write(root.path(), "escape.cue", b"FILE escaped.bin BINARY\n");
    write(root.path(), "users/default/Game.bin", b"plugin data");
    symlink(
        root.path().join("users/default/Game.bin"),
        root.path().join("plugin.bin"),
    )
    .unwrap();
    write(root.path(), "plugin.cue", b"FILE plugin.bin BINARY\n");
    let rule = rules("psx", &["bin", "cue"]);
    let excluded = vec![root.path().join("users").canonicalize().unwrap()];
    let report = scan_options(
        root.path(),
        &[&rule],
        false,
        &excluded,
        &TraversalBudget::default(),
        &|_, _, _| None,
        &|_| vec![],
        &mut HashCache::default(),
    );
    assert!(report.units.is_empty());
    assert_eq!(report.hashed_bytes, 0);
    assert_eq!(report.attention.len(), 2);
    assert!(report
        .diagnostics
        .iter()
        .any(|item| item.code == DiscoveryDiagnosticCode::EntryEscaped));
    assert!(!report
        .seen_paths
        .iter()
        .any(|path| path.starts_with("users/")));
}

#[test]
fn declared_files_match_sha1_without_system_headers_and_report_only_existing_mismatches() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "game.cgo", b"abc");
    write(root.path(), "wrong.cgo", b"wrong");
    let mut releases = indexmap::IndexMap::new();
    for file in ["game.cgo", "wrong.cgo", "absent.cgo"] {
        releases.insert(
            file.into(),
            DeclaredRelease {
                system: None,
                hashes: vec!["sha1:a9993e364706816aba3e25717850c26c9cd0d89d".into()],
                artifacts: Vec::new(),
                file: Some(crate::plugin::DeclaredReleaseFile::AccountRoot(file.into())),
            },
        );
    }
    releases.insert(
        "original".into(),
        DeclaredRelease {
            system: Some("ps2".into()),
            hashes: vec![format!("sha1:{}", "0".repeat(40))],
            artifacts: Vec::new(),
            file: None,
        },
    );
    let games = BTreeMap::from([(
        "@test:plugin/game".into(),
        DeclaredGame {
            provider: "@test:plugin".into(),
            key: "game".into(),
            title: "Game".into(),
            releases,
        },
    )]);
    let accounts = [AccountRoot {
        user: "default".into(),
        storage_id: "account-default".into(),
        root: root.path().to_owned(),
    }];
    let mut cache = HashCache::default();
    let report = resolve_declared_games(&accounts, &games, &mut cache);
    assert_eq!(report.found().count(), 1);
    assert_eq!(report.files.len(), 3);
    assert!(report
        .files
        .iter()
        .any(|file| file.path == "absent.cgo" && file.status == DeclaredFileStatus::Absent));
    assert!(report
        .files
        .iter()
        .any(|file| file.path == "wrong.cgo" && file.status == DeclaredFileStatus::Mismatch));
    assert_eq!(report.found().next().unwrap().0.game, "@test:plugin/game");
    assert_eq!(
        report.found().next().unwrap().0.storage_id,
        "account-default"
    );
    assert_eq!(report.attention.len(), 1);
    assert_eq!(report.attention[0].kind, AttentionKind::DeclaredMismatch);
    assert_eq!(report.hashed_bytes, 8);
    assert_eq!(
        resolve_declared_games(&accounts, &games, &mut cache).hashed_bytes,
        0
    );
}

#[cfg(unix)]
#[test]
fn declared_files_cannot_escape_the_account_root() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let secret = write(outside.path(), "secret.cgo", b"secret");
    symlink(secret, root.path().join("game.cgo")).unwrap();
    let releases = indexmap::IndexMap::from([(
        "prepared".into(),
        DeclaredRelease {
            system: None,
            hashes: vec!["sha1:unused".into()],
            artifacts: Vec::new(),
            file: Some(crate::plugin::DeclaredReleaseFile::AccountRoot(
                "game.cgo".into(),
            )),
        },
    )]);
    let games = BTreeMap::from([(
        "@test:plugin/game".into(),
        DeclaredGame {
            provider: "@test:plugin".into(),
            key: "game".into(),
            title: "Game".into(),
            releases,
        },
    )]);
    let accounts = [AccountRoot {
        user: "default".into(),
        storage_id: "account-default".into(),
        root: root.path().to_owned(),
    }];
    let report = resolve_declared_games(&accounts, &games, &mut HashCache::default());
    assert!(report.found().next().is_none());
    assert_eq!(report.hashed_bytes, 0);
    assert_eq!(report.attention[0].kind, AttentionKind::Unreadable);
}

#[test]
fn playlists_may_reference_siblings_but_not_paths_above_the_source() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "disc.bin", b"game");
    write(root.path(), "lists/Game.m3u", b"../disc.bin\n");
    let rule = rules("psx", &["m3u", "bin"]);
    let report = scan(root.path(), &[&rule], false);
    assert_eq!(report.units.len(), 1);
    assert_eq!(report.units[0].path, "lists/Game.m3u");
    assert_eq!(report.units[0].parts, ["disc.bin"]);
    write(root.path(), "lists/Game.m3u", b"../../outside.bin\n");
    let report = scan(root.path(), &[&rule], false);
    assert!(report
        .attention
        .iter()
        .any(|item| item.kind == AttentionKind::Unreadable));
}

#[test]
fn only_best_ranked_members_are_clash_candidates_and_bad_assignments_are_ignored() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "Game.gb", b"game");
    let mut gb = rules("gb", &["gb"]);
    let mut gbc = rules("gbc", &["gb"]);
    let other = rules("other", &["gb"]);
    for rule in [&mut gb, &mut gbc] {
        rule.evidence.names.push("^Game".into());
    }
    let report = scan_options(
        root.path(),
        &[&other, &gbc, &gb],
        true,
        &[],
        &TraversalBudget::default(),
        &|_, _, _| Some("nes".into()),
        &|_| vec![],
        &mut HashCache::default(),
    );
    assert!(report.units.is_empty());
    assert_eq!(report.attention[0].candidates, ["gb", "gbc"]);
}

#[test]
fn descriptor_references_have_a_work_budget_even_with_cached_repeated_targets() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "disc.bin", b"game");
    write(
        root.path(),
        "Game.m3u",
        b"disc.bin\ndisc.bin\ndisc.bin\ndisc.bin\n",
    );
    let rule = rules("psx", &["m3u"]);
    let budget = TraversalBudget {
        max_entries: 3,
        ..TraversalBudget::default()
    };
    let report = scan_options(
        root.path(),
        &[&rule],
        false,
        &[],
        &budget,
        &|_, _, _| None,
        &|_| vec![],
        &mut HashCache::default(),
    );
    assert!(report.units.is_empty());
    assert_eq!(report.attention[0].kind, AttentionKind::Unreadable);
    assert!(report.attention[0].message.contains("reference budget"));
}

#[test]
fn declared_descriptor_files_are_hashed_as_bytes_instead_of_interpreted() {
    let root = tempfile::tempdir().unwrap();
    let path = write(root.path(), "game.cue", b"not a cue descriptor");
    let expected = HashCache::default().hash(&path, &[]).unwrap();
    let releases = indexmap::IndexMap::from([(
        "prepared".into(),
        DeclaredRelease {
            system: None,
            hashes: vec![expected.value().to_owned()],
            artifacts: Vec::new(),
            file: Some(crate::plugin::DeclaredReleaseFile::AccountRoot(
                "game.cue".into(),
            )),
        },
    )]);
    let games = BTreeMap::from([(
        "@test:plugin/game".into(),
        DeclaredGame {
            provider: "@test:plugin".into(),
            key: "game".into(),
            title: "Game".into(),
            releases,
        },
    )]);
    let accounts = [AccountRoot {
        user: "default".into(),
        storage_id: "account-default".into(),
        root: root.path().to_owned(),
    }];
    let report = resolve_declared_games(&accounts, &games, &mut HashCache::default());
    assert_eq!(report.found().next().unwrap().1, &expected);
    assert!(report.attention.is_empty());
}

#[test]
fn registry_and_attention_adapter_identifies_real_plugin_content() {
    use crate::plugin::load_plugin_source;
    let registry = PluginRegistry::new(
        vec![load_plugin_source(
            "@test",
            r#"
export const name = "content";
export const title = "Content";
export const systems = {
    gb: { id: "gb", title: "Game Boy", content: { extensions: ["gb"] } },
};
"#,
        )
        .unwrap()],
        vec!["@test:content".into()],
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "Game.gb", b"game");
    let source = SourceInput {
        source_id: "games",
        storage_id: "selected",
        root: root.path(),
        system: "gb",
        excluded_roots: &[],
    };
    let report = identify_source(
        &source,
        &registry,
        &AttentionDecisions::default(),
        &mut HashCache::default(),
        &TraversalBudget::default(),
    );
    assert_eq!(report.units.len(), 1);
    assert_eq!(report.units[0].system, "gb");
}

#[test]
fn projected_sources_use_future_names_and_match_real_identification_without_early_writes() {
    let original = tempfile::tempdir().unwrap();
    let target = original.path().join("future");
    let physical = write(original.path(), "old/Unrelated.gb", b"game");
    let logical = target.join("sets/Chosen.gb");
    let changes = BTreeMap::from([(logical.clone(), Some(physical.clone()))]);
    let mut rule = rules("gb", &["gb"]);
    rule.layouts = vec!["sets/{title}.{ext}".into()];
    let input = SourceInput {
        source_id: "gb",
        storage_id: "gb",
        system: "gb",
        root: &target,
        excluded_roots: &[],
    };
    let projected = identify_planned_with_rules(
        &input,
        &[&rule],
        false,
        &|_, _, _| None,
        &|_| vec![],
        &changes,
        &mut Default::default(),
        &TraversalBudget::default(),
    );
    assert_eq!(projected.units[0].title, "Chosen");
    assert_eq!(projected.units[0].path, "sets/Chosen.gb");
    assert!(!target.exists());
    assert!(physical.is_file());
    fs::create_dir_all(logical.parent().unwrap()).unwrap();
    fs::rename(physical, logical).unwrap();
    let actual = identify_with_rules(
        &input,
        &[&rule],
        false,
        &|_, _, _| None,
        &|_| vec![],
        &mut Default::default(),
        &TraversalBudget::default(),
    );
    assert_eq!(projected, actual);
}

#[test]
fn projected_budgets_include_existing_empty_directories_and_incoming_parents() {
    let original = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    fs::create_dir(target.path().join("empty")).unwrap();
    let physical = write(original.path(), "Game.gb", b"game");
    let changes = BTreeMap::from([(target.path().join("new/Game.gb"), Some(physical))]);
    let input = SourceInput {
        source_id: "gb",
        storage_id: "gb",
        system: "gb",
        root: target.path(),
        excluded_roots: &[],
    };
    let rule = rules("gb", &["gb"]);
    for budget in [
        TraversalBudget {
            max_directories: 2,
            ..Default::default()
        },
        TraversalBudget {
            max_entries: 2,
            ..Default::default()
        },
        TraversalBudget {
            max_sortable_entries: 1,
            ..Default::default()
        },
    ] {
        let projected = identify_planned_with_rules(
            &input,
            &[&rule],
            false,
            &|_, _, _| None,
            &|_| vec![],
            &changes,
            &mut Default::default(),
            &budget,
        );
        assert!(crate::discovery::scanner::incomplete(&projected));
    }
}

#[test]
fn an_artifact_file_is_never_hashed_as_an_account_root_path() {
    let root = tempfile::tempdir().unwrap();
    // Same relative path under the account root as the artifact's content
    // file. Reading it as an account-root path would join a wrong copy.
    let path = write(root.path(), "Mega Man/data.pck", b"account copy");
    let expected = HashCache::default().hash(&path, &[]).unwrap();
    let releases = indexmap::IndexMap::from([(
        "v1".to_owned(),
        DeclaredRelease {
            system: None,
            hashes: vec![expected.value().to_owned()],
            artifacts: vec!["@test:plugin/content".into()],
            file: Some(crate::plugin::DeclaredReleaseFile::Artifact(
                crate::plugin::ArtifactContentFile {
                    artifact: "@test:plugin/content".into(),
                    path: "Mega Man/data.pck".into(),
                },
            )),
        },
    )]);
    let games = BTreeMap::from([(
        "@test:plugin/game".into(),
        DeclaredGame {
            provider: "@test:plugin".into(),
            key: "game".into(),
            title: "Game".into(),
            releases,
        },
    )]);
    let accounts = [AccountRoot {
        user: "default".into(),
        storage_id: "account-default".into(),
        root: root.path().to_owned(),
    }];
    let report = resolve_declared_games(&accounts, &games, &mut HashCache::default());
    assert!(report.files.is_empty());
    assert!(report.attention.is_empty());
    assert_eq!(report.hashed_bytes, 0);
}
