use korri_plugin_host::{package, provenance::Provenance};
use std::path::PathBuf;

#[test]
#[ignore = "requires built plugin.nix payload and the real build-machine Nix; no VM"]
fn real_game_payloads_pass_the_host_package_boundary_before_approval() {
    let nix = PathBuf::from(std::env::var_os("KORRI_TEST_NIX").expect("immutable Nix executable"));
    let selected =
        PathBuf::from(std::env::var_os("KORRI_TEST_GAME_PACKAGE").expect("built mGBA package"));
    let provenance = Provenance::RawCache {
        cache_url: "https://cache.example.test".into(),
    };
    let game = package::load(&nix, &selected, provenance).unwrap();
    assert_eq!(game.id, "@korri:mgba");
    assert!(game.native_unit.is_none());
    assert!(game.files["mgba"].is_file());
    assert!(game.files["retroarch"].is_file());
    assert!(game.files["autoconfig"].is_dir());
    assert_eq!(game.approval.len(), 64);
}
