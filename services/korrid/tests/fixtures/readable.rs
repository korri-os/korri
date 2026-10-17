#![allow(dead_code)]
use std::path::Path;

pub const ANDROID_ID: &str = "tmnt-shredders-revenge";
pub const GBA_ID: &str = "wario-land-4";
pub const OTHER_ID: &str = "other-game";
pub const ANDROID_RELEASE: &str = "@korri:android-app/com.playdigious.tmnt";
pub const GBA_RELEASE: &str =
    "sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414";
pub const ANDROID_DEVICE: &str =
    include_str!("../../../../docs/research/android-app-plugin-schema-checkpoint/device.yaml");
pub const ANDROID_LIBRARY: &str = "library:\n  tmnt-shredders-revenge:\n    title: \"TMNT: Shredder's Revenge\"\n    releases:\n      - id: android\n        system: android\n";
pub const DEVICE: &str =
    include_str!("../../../../docs/research/retroarch-plugin-route/device.yaml");
pub const LIBRARY: &str = "library:\n  tmnt-shredders-revenge:\n    title: \"TMNT: Shredder's Revenge\"\n    releases:\n      - id: android\n        system: android\n  wario-land-4:\n    title: Wario Land 4\n    releases:\n      - id: gba\n        system: gba\n        identity: {kind: hash, value: 'sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414'}\n";

pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    let path = path.as_ref();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, contents)
}
pub fn install(root: &Path, device: &str, library: &str) {
    write(root.join("device.yaml"), device).unwrap();
    write(root.join("catalog/library.yaml"), library).unwrap();
}
pub fn android(root: &Path) {
    install(root, ANDROID_DEVICE, ANDROID_LIBRARY);
}
pub fn combined(root: &Path) {
    install(root, DEVICE, LIBRARY);
}
pub fn gba_library() -> String {
    format!("library:\n  {GBA_ID}:\n    title: Wario Land 4\n    releases:\n      - id: gba\n        system: gba\n        identity: {{kind: hash, value: '{GBA_RELEASE}'}}\n")
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
        &gba_library(),
    );
}
