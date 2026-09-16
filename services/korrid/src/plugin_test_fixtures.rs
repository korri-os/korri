//! Real filesystem package for registry tests; production reads the root-owned
//! committed selection projection instead. Sources are the actual plugin files.
use crate::{plugin::PluginRegistry, plugin_installation::EnabledPackage};
use std::{collections::BTreeMap, fs, path::Path};

/// A game id that both fixture runners admit, so a caller can prove how the
/// library treats one game reachable by more than one runner.
pub const GBA_ID: &str = "01K4J6K8Y00000000000000002";
const GBA_RELEASE: &str = "@korri:mgba/gba-files:wl4.gba";

/// The smallest readable library that resolves to `GBA_ID`: one game, one
/// release on the `gba` system, one file location under `roms`.
pub fn write_gba_library(root: &Path) {
    let write = |name: &str, body: String| {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    };
    write(
        "device.yaml",
        format!(
            "locations:\n  '{GBA_RELEASE}':\n    - storage: roms\n      path: wl4.gba\n"
        ),
    );
    write(
        "catalog/games.yaml",
        format!("games:\n  {GBA_ID}:\n    title: Wario Land 4\n    releases: ['{GBA_RELEASE}']\n"),
    );
    write(
        "catalog/releases.yaml",
        format!(
            "releases:\n  '{GBA_RELEASE}':\n    game: {GBA_ID}\n    system: gba\n    identity: file\n"
        ),
    );
}

/// Two installed cores of the RetroArch family that both run `gba` content.
/// Derived from the one committed generated example, so the pair cannot drift
/// from what the catalogue actually emits.
pub fn installed_pair(root: &Path) -> PluginRegistry {
    let mut packages = vec![core_package(root, "mgba", GENERATED_CORE.to_owned())];
    packages.push(core_package(
        root,
        "beetle-gba",
        GENERATED_CORE.replace("mgba", "beetle-gba").replace("mGBA", "Beetle GBA"),
    ));
    PluginRegistry::from_installed(packages).unwrap()
}

fn core_package(root: &Path, name: &str, source: String) -> EnabledPackage {
    let package = root.join(format!("plugin-{name}"));
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("plugin.ts"), source).unwrap();
    fs::write(
        package.join("retroarch.ts"),
        include_str!("../../../plugins/libretro/retroarch.ts"),
    )
    .unwrap();
    fs::write(package.join("settings.ts"), SETTINGS_MODULE).unwrap();
    EnabledPackage {
        id: format!("@korri:{name}"),
        package,
        files: BTreeMap::from([
            ("retroarch".into(), root.join("retroarch")),
            ("autoconfig".into(), root.join("autoconfig")),
            (name.into(), root.join(format!("{name}.so"))),
        ]),
        entry: "plugin.ts".into(),
        sources: vec![
            "plugin.ts".into(),
            "retroarch.ts".into(),
            "settings.ts".into(),
        ],
    }
}

const GENERATED_CORE: &str = include_str!("../examples/libretro-core.plugin.ts");
const SETTINGS_MODULE: &str =
    "export const version = \"1.22.2\"\nexport const keys = {\n  video_vsync: \"Boolean\",\n  audio_volume: \"Number\",\n}\n";

pub fn installed(root: &Path) -> PluginRegistry {
    let mgba = root.join("plugin-mgba");
    fs::create_dir_all(&mgba).unwrap();
    fs::write(
        mgba.join("plugin.ts"),
        include_str!("../examples/libretro-core.plugin.ts"),
    )
    .unwrap();
    fs::write(
        mgba.join("retroarch.ts"),
        include_str!("../../../plugins/libretro/retroarch.ts"),
    )
    .unwrap();
    // The catalogue writes this module from the pinned program's own source.
    // A shortened table is enough here; the schema belongs to the plugin.
    fs::write(
        mgba.join("settings.ts"),
        "export const version = \"1.22.2\"\nexport const keys = {\n  video_vsync: \"Boolean\",\n  audio_volume: \"Number\",\n}\n",
    )
    .unwrap();
    PluginRegistry::from_installed(vec![EnabledPackage {
        id: "@korri:mgba".into(),
        package: mgba,
        files: BTreeMap::from([
            ("retroarch".into(), root.join("retroarch")),
            ("autoconfig".into(), root.join("autoconfig")),
            ("mgba".into(), root.join("mgba.so")),
        ]),
        entry: "plugin.ts".into(),
        sources: vec![
            "plugin.ts".into(),
            "retroarch.ts".into(),
            "settings.ts".into(),
        ],
    }])
    .unwrap()
}
