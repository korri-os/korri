#![allow(dead_code)]
use std::path::Path;

pub const ANDROID_ID: &str = "01K4J6K8Y00000000000000001";
pub const GBA_ID: &str = "01K4J6K8Y00000000000000002";
pub const OTHER_ID: &str = "01K4J6K8Y00000000000000003";
pub const ANDROID_RELEASE: &str = "@korri:android-app/com.playdigious.tmnt";
pub const GBA_RELEASE: &str =
    "sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414";
pub const ANDROID_DEVICE: &str =
    include_str!("../../../../docs/research/android-app-plugin-schema-checkpoint/device.yaml");
pub const ANDROID_GAMES: &str = include_str!(
    "../../../../docs/research/android-app-plugin-schema-checkpoint/catalog/games.yaml"
);
pub const ANDROID_RELEASES: &str = include_str!(
    "../../../../docs/research/android-app-plugin-schema-checkpoint/catalog/releases.yaml"
);
pub const DEVICE: &str =
    include_str!("../../../../docs/research/retroarch-plugin-route/device.yaml");
pub const GAMES: &str =
    include_str!("../../../../docs/research/retroarch-plugin-route/catalog/games.yaml");
pub const RELEASES: &str =
    include_str!("../../../../docs/research/retroarch-plugin-route/catalog/releases.yaml");

pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    let path = path.as_ref();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, contents)
}

pub fn install(root: &Path, device: &str, games: &str, releases: &str) {
    write(root.join("device.yaml"), device).unwrap();
    write(root.join("catalog/games.yaml"), games).unwrap();
    write(root.join("catalog/releases.yaml"), releases).unwrap();
}

pub fn android(root: &Path) {
    install(root, ANDROID_DEVICE, ANDROID_GAMES, ANDROID_RELEASES);
}
pub fn combined(root: &Path) {
    install(root, DEVICE, GAMES, RELEASES);
}

pub fn gba_games() -> String {
    format!("games:\n  {GBA_ID}:\n    title: Wario Land 4\n    releases: ['{GBA_RELEASE}']\n")
}
pub fn gba_releases() -> String {
    format!(
        "releases:\n  '{GBA_RELEASE}':\n    game: {GBA_ID}\n    system: gba\n    identity: file\n"
    )
}
pub fn gba_locations(storage: &str, path: &str, discovery: bool) -> String {
    let discovered = if discovery {
        "      discovery:\n        first-seen-at: 2026-08-05T00:00:00Z\n"
    } else {
        ""
    };
    format!("locations:\n  '{GBA_RELEASE}':\n    - storage: {storage}\n      path: {path}\n{discovered}")
}
pub fn gba(root: &Path) {
    install(
        root,
        &gba_locations("roms", "wl4.gba", false),
        &gba_games(),
        &gba_releases(),
    );
}
