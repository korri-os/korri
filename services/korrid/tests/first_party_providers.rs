//! The three published provider plugins, as byte-identical producer snapshots
//! (tests/fixtures/first-party, korri-os/plugins 824050bfa). Core loads them
//! through the real registry; it does not run their plugin unit tests.
use korrid::plugin::{load_plugin_source, DeclaredReleaseFile, PluginRegistry};

const PICO8: &str = include_str!("fixtures/first-party/pico8/plugin.ts");
const HOMEBREWHUB: &str = include_str!("fixtures/first-party/homebrewhub/plugin.ts");
const RETROBREWS: &str = include_str!("fixtures/first-party/retrobrews/plugin.ts");

/// Every artifact the declared games name must be enabled, as on a device.
fn registry() -> PluginRegistry {
    PluginRegistry::new(
        vec![
            load_plugin_source("@korri", PICO8).unwrap(),
            load_plugin_source("@korri", HOMEBREWHUB).unwrap(),
            load_plugin_source("@korri", RETROBREWS).unwrap(),
        ],
        [
            "@korri:pico8".into(),
            "@korri:homebrewhub".into(),
            "@korri:retrobrews".into(),
        ],
    )
    .unwrap()
}

#[test]
fn only_pico8_is_a_search_provider() {
    let registry = registry();
    let providers: Vec<_> = registry
        .acquisition_providers()
        .map(|(record, owner)| (record.id.as_str(), record.title.as_deref(), owner))
        .collect();
    assert_eq!(
        providers,
        [("@korri:pico8", Some("PICO-8 BBS"), "@korri:pico8")]
    );
    // The fixture layout carries plugin.ts only, no manifest, so hosts are
    // not part of this snapshot (the producer's plugin.nix declares them).
    assert!(registry.plugin_hosts("@korri:pico8").is_empty());
}

#[test]
fn homebrewhub_and_retrobrews_declare_games_from_their_artifacts() {
    let registry = registry();
    let games = registry.declared_games();
    for (game, system, hash, artifact, path) in [
        (
            "@korri:homebrewhub/2048gb",
            "gb",
            "sha256:3ea2376b15b34bd26b10e6b31d2753bf8b086098dbdb5ee84d9c0f03d66d3d7f",
            "@korri:homebrewhub/2048gb",
            "2048.gb",
        ),
        (
            // The release hash is taken after the nes header rule strips the
            // 16-byte iNES header, not the hash of the whole file.
            "@korri:retrobrews/ambushed",
            "nes",
            "sha256:b9c6afa600568aa0144aed787acf778d6237187f89776b7d3548889700cd589a",
            "@korri:retrobrews/ambushed",
            "ambushed.nes",
        ),
        (
            "@korri:retrobrews/anguna",
            "gba",
            "sha256:cd3b969d5ed6b38b2039c11b894d6db2fc3fad209a7fd046bee241cc66069851",
            "@korri:retrobrews/anguna",
            "anguna.gba",
        ),
    ] {
        let declared = games.get(game).unwrap_or_else(|| panic!("{game}"));
        assert_eq!(declared.releases.len(), 1, "{game}");
        let release = declared.releases.values().next().unwrap();
        assert_eq!(release.system.as_deref(), Some(system), "{game}");
        assert_eq!(release.hashes, [hash], "{game}");
        assert_eq!(release.artifacts, [artifact], "{game}");
        match &release.file {
            Some(DeclaredReleaseFile::Artifact(file)) => {
                assert_eq!(
                    (file.artifact.as_str(), file.path.as_str()),
                    (artifact, path)
                )
            }
            Some(DeclaredReleaseFile::AccountRoot(file)) => {
                panic!("{game}: account-root file {file}")
            }
            Some(DeclaredReleaseFile::Package(file)) => panic!("{game}: package file {file:?}"),
            None => panic!("{game}: no content file"),
        }
        assert!(registry.artifact(artifact).is_ok(), "{artifact}");
        assert!(registry.acquisition_provider(&declared.provider).is_none());
    }
    assert_eq!(games.len(), 3);
}
