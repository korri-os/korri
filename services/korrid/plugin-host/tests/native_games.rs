use korri_plugin_host::{package, provenance::Provenance};
use std::path::PathBuf;

#[test]
#[ignore = "requires built plugin.nix payload and the real build-machine Nix; no VM"]
fn real_game_payloads_pass_the_host_package_boundary_before_approval() {
    let nix = PathBuf::from(std::env::var_os("KORRI_TEST_NIX").expect("immutable Nix executable"));
    let selected =
        PathBuf::from(std::env::var_os("KORRI_TEST_GAME_PACKAGE").expect("built mGBA package"));
    let provenance = || Provenance::RawCache {
        cache_url: "https://cache.example.test".into(),
    };
    let core = package::load(&nix, &selected, provenance()).unwrap();
    assert_eq!(core.id, "@korri:mgba");
    assert!(core.native_unit.is_none());
    assert!(core.files["mgba"].is_file());
    assert_eq!(core.brings.len(), 1);
    assert_eq!(core.brings[0].id, "@korri:retroarch");
    assert_eq!(core.brings[0].package, core.requires[0]);
    assert!(core.brings[0].files["retroarch"].is_file());
    let launcher = package::load(&nix, &core.requires[0], provenance()).unwrap();
    assert_eq!(launcher.id, "@korri:retroarch");
    assert!(launcher.native_unit.is_none());
    assert!(launcher.files["retroarch"].is_file());
    assert!(launcher.files["autoconfig"].is_dir());
    assert!(launcher
        .declaration
        .session_controls
        .as_ref()
        .unwrap()
        .contains_key("quit"));
    assert_eq!(launcher.approval.len(), 64);
    korri_plugin_host::plugin_references::validate([core, launcher].into_iter().map(|report| {
        korri_plugin_host::plugin_references::PackageDeclaration {
            id: report.id,
            package: report.package,
            requires: report.requires,
            declaration: serde_json::to_value(report.declaration).unwrap(),
        }
    }))
    .unwrap();
}
