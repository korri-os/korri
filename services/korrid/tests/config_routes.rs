use korrid::{
    config::{decode_config_documents, resolver::linux_route_candidates},
    plugin::PluginRegistry,
    plugin_installation::EnabledPackage,
    GameIdentity,
};
use std::{collections::BTreeMap, fs};

fn registry(root: &std::path::Path, hashes: bool) -> PluginRegistry {
    let hash = format!("sha256:{}", "a".repeat(64));
    let selector = if hashes {
        format!("releases: ['{hash}']")
    } else {
        "systems: ['gba']".into()
    };
    fs::write(root.join("plugin.ts"), format!("export const name = 'runner'; export const systems = {{ gba: {{title: 'Game Boy Advance', content: {{extensions: ['gba', 'cue', 'm3u'], layout: ['**/{{title}} (Disc {{disc}}).{{ext}}', '**/{{title}}.{{ext}}']}}}} }}; export const runners = {{ core: {{program: 'runner', {selector}}} }}; export const handlers = {{'launch.prepare': () => ({{command: 'runner', args: []}})}};")).unwrap();
    PluginRegistry::from_installed(vec![EnabledPackage {
        id: "@test:runner".into(),
        package: root.into(),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
        files: BTreeMap::from([("runner".into(), root.join("runner"))]),
        requires: Vec::new(),
        hosts: vec![],
    }])
    .unwrap()
}
fn library(system: bool) -> String {
    format!("library:\n  game:\n    title: Game\n    releases:\n      - id: prepared\n        {}identity: {{kind: hash, value: 'sha256:{}'}}\n", if system { "system: gba\n        " } else { "" }, "a".repeat(64))
}
fn device(root: &std::path::Path) -> String {
    format!("storage: {{selected: {{root: '{}'}}}}\nlocations:\n  'sha256:{}':\n    - storage: selected\n      path: missing.gba\n    - storage: selected\n      path: game.gba\n", root.display(), "a".repeat(64))
}
#[test]
fn runner_release_hash_routes_without_inventing_a_system() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("game.gba"), b"game").unwrap();
    let registry = registry(root.path(), true);
    let snapshot = decode_config_documents(&device(root.path()), &library(false)).unwrap();
    let routes = linux_route_candidates(root.path(), &snapshot, &registry, "game").unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].system_id, "");
    assert_eq!(routes[0].release_id, format!("sha256:{}", "a".repeat(64)));
    assert_eq!(routes[0].file_target.as_ref().unwrap().path, "game.gba");
}
#[test]
fn provider_identity_precedes_first_release_hash_for_every_route() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("game.gba"), b"game").unwrap();
    let registry = registry(root.path(), false);
    let device = format!("{}provider-links:\n  '@test:games/game':\n    provider: '@test:games'\n    playable: game\n    refs: [{{kind: provider-item-id, value: game}}]\n", device(root.path()));
    let snapshot = decode_config_documents(&device, &library(true)).unwrap();
    let route = linux_route_candidates(root.path(), &snapshot, &registry, "game")
        .unwrap()
        .remove(0);
    assert_eq!(
        route.identity,
        Some(GameIdentity::Provider(korrid::GameProviderIdentity {
            provider: "@test:games".into(),
            provider_ref: "game".into()
        }))
    );
}
#[test]
fn fileset_routes_validate_every_part_and_skip_missing_since_locations() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("game.cue"), b"descriptor").unwrap();
    fs::write(root.path().join("track1.bin"), b"one").unwrap();
    fs::write(root.path().join("track2.bin"), b"two").unwrap();
    let registry = registry(root.path(), false);
    let device = format!("storage: {{selected: {{root: '{}'}}}}\nlocations:\n  'sha256:{}':\n    - storage: selected\n      files: [{{id: descriptor, role: descriptor, path: game.cue}}, {{id: '1', role: track, path: track1.bin}}, {{id: '2', path: track2.bin}}]\n      discovery: {{first-seen-at: '2026-10-16T00:00:00Z'}}\n", root.path().display(), "a".repeat(64));
    let snapshot = decode_config_documents(&device, &library(true)).unwrap();
    assert_eq!(
        linux_route_candidates(root.path(), &snapshot, &registry, "game").unwrap()[0]
            .file_target
            .as_ref()
            .unwrap()
            .path,
        "game.cue"
    );
    fs::remove_file(root.path().join("track2.bin")).unwrap();
    assert!(linux_route_candidates(root.path(), &snapshot, &registry, "game").is_err());
    let missing = device.replace(
        "first-seen-at:",
        "missing-since: '2026-10-17T00:00:00Z', first-seen-at:",
    );
    fs::write(root.path().join("track2.bin"), b"two").unwrap();
    let snapshot = decode_config_documents(&missing, &library(true)).unwrap();
    assert!(linux_route_candidates(root.path(), &snapshot, &registry, "game").is_err());
}

#[test]
fn paired_playlist_launches_its_descriptor_and_requires_nested_cues_and_tracks() {
    let root = tempfile::tempdir().unwrap();
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Game.m3u"), b"disc.cue\n").unwrap();
    fs::write(
        root.path().join("disc.cue"),
        b"FILE track.bin BINARY\nFILE audio.data WAVE\n",
    )
    .unwrap();
    fs::write(root.path().join("track.bin"), b"content").unwrap();
    fs::write(root.path().join("audio.data"), b"audio").unwrap();
    let registry = registry(root.path(), false);
    korrid::discovery::DiscoveryCoordinator::new(readable.path(), private.path())
        .with_registry_source(korrid::plugin_policy::RegistrySource::Selected(
            registry.clone().into(),
        ))
        .pair(root.path().into(), "gba", &Default::default())
        .unwrap();
    let state = korrid::config::snapshot::ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert!(state.diagnostic.is_none());
    assert_eq!(state.snapshot.library.len(), 1);
    let route = linux_route_candidates(readable.path(), &state.snapshot, &registry, "game")
        .unwrap()
        .remove(0);
    assert_eq!(route.file_target.unwrap().path, "Game.m3u");
    let location = state
        .snapshot
        .locations
        .values()
        .next()
        .unwrap()
        .first()
        .unwrap();
    let korrid::config::Location::FileSet { files, .. } = location else {
        panic!("playlist must be a FileSet")
    };
    assert_eq!(
        files
            .0
            .iter()
            .map(|part| (part.path.0.as_str(), part.role.as_ref().unwrap().0.as_str()))
            .collect::<Vec<_>>(),
        [
            ("Game.m3u", "descriptor"),
            ("disc.cue", "disc"),
            ("track.bin", "track"),
            ("audio.data", "track")
        ]
    );
    fs::remove_file(root.path().join("disc.cue")).unwrap();
    assert!(linux_route_candidates(readable.path(), &state.snapshot, &registry, "game").is_err());
}

#[test]
fn grouped_cue_discs_keep_every_descriptor_and_explicit_track_role() {
    let root = tempfile::tempdir().unwrap();
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    for (disc, track) in [(1, "one.data"), (2, "two.audio")] {
        fs::write(
            root.path().join(format!("Game (Disc {disc}).cue")),
            format!("FILE {track} BINARY\n"),
        )
        .unwrap();
        fs::write(root.path().join(track), track.as_bytes()).unwrap();
    }
    let registry = registry(root.path(), false);
    korrid::discovery::DiscoveryCoordinator::new(readable.path(), private.path())
        .with_registry_source(korrid::plugin_policy::RegistrySource::Selected(
            registry.clone().into(),
        ))
        .pair(root.path().into(), "gba", &Default::default())
        .unwrap();
    let state = korrid::config::snapshot::ConfigSnapshotCoordinator::new(readable.path()).reload();
    assert!(state.diagnostic.is_none());
    assert_eq!(state.snapshot.library.len(), 1);
    let route = linux_route_candidates(readable.path(), &state.snapshot, &registry, "game")
        .unwrap()
        .remove(0);
    assert_eq!(route.file_target.unwrap().path, "Game (Disc 1).cue");
    let location = state
        .snapshot
        .locations
        .values()
        .next()
        .unwrap()
        .first()
        .unwrap();
    let korrid::config::Location::FileSet { files, .. } = location else {
        panic!("grouped discs must be a FileSet")
    };
    assert_eq!(
        files
            .0
            .iter()
            .map(|part| (part.path.0.as_str(), part.role.as_ref().unwrap().0.as_str()))
            .collect::<Vec<_>>(),
        [
            ("Game (Disc 1).cue", "descriptor"),
            ("one.data", "track"),
            ("Game (Disc 2).cue", "disc"),
            ("two.audio", "track")
        ]
    );
    fs::remove_file(root.path().join("Game (Disc 2).cue")).unwrap();
    assert!(linux_route_candidates(readable.path(), &state.snapshot, &registry, "game").is_err());
}

#[test]
fn launch_settings_release_layer_matches_the_real_route_content_identity() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("game.gba"), b"game").unwrap();
    let registry = registry(root.path(), false);
    let library = library(true).replace(
        "        identity:",
        "        cpu: {max: 37}\n        gpu: {min: 12}\n        identity:",
    );
    let config = decode_config_documents(&device(root.path()), &library).unwrap();
    let route = linux_route_candidates(root.path(), &config, &registry, "game")
        .unwrap()
        .remove(0);
    assert_ne!(route.release_id, config.library["game"].releases[0].id.0);
    let limits = korrid::launch_settings::session::SessionLimits::for_route(
        &config,
        &route,
        "build".into(),
        None,
    )
    .unwrap();
    assert_eq!((limits.cpu.max, limits.gpu.min), (37, 12));
}
