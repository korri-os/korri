//! Real filesystem package for registry tests; production reads the root-owned
//! committed selection projection instead. Sources are the actual plugin files.
use crate::{plugin::PluginRegistry, plugin_installation::EnabledPackage};
use std::{collections::BTreeMap, fs, path::Path};

/// A system that identifies `.gba` files and a runner that builds a launch plan.
/// It imports nothing, so it loads from source without a module graph.
const GBA_CLAIM: &str = r#"
export const name = "core";
export const title = "GBA Core";
export const systems = { gba: { id: "gba", title: "Game Boy Advance", content: { extensions: ["gba"] } } };
export const runners = {
  core: { id: "@korri:core/core", program: "retroarch", systems: ["gba"] },
};

export const handlers = {
  "launch.prepare": () => ({ command: "retroarch", args: [] }),
};
"#;

/// One enabled plugin that claims `.gba` files and nothing else. Production
/// reads the root-owned installation record instead, which a test root does
/// not have.
pub fn claims_only_registry() -> PluginRegistry {
    PluginRegistry::new(
        vec![crate::plugin::load_plugin_source("@korri", GBA_CLAIM).unwrap()],
        vec!["@korri:core".into()],
    )
    .unwrap()
}

/// A game id that both fixture runners admit, so a caller can prove how the
/// library treats one game reachable by more than one runner.
pub const GBA_ID: &str = "wario-land-4";
const GBA_RELEASE: &str = "sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414";

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
        format!("locations:\n  '{GBA_RELEASE}':\n    - storage: roms\n      path: wl4.gba\n"),
    );
    write(
        "catalog/library.yaml",
        format!("library:\n  {GBA_ID}:\n    title: Wario Land 4\n    releases:\n      - id: gba\n        system: gba\n        identity: {{kind: hash, value: '{GBA_RELEASE}'}}\n"),
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
        GENERATED_CORE
            .replace("mgba", "beetle-gba")
            .replace("mGBA", "Beetle GBA"),
    ));
    PluginRegistry::from_installed(packages).unwrap()
}

fn core_package(root: &Path, name: &str, source: String) -> EnabledPackage {
    let package = root.join(format!("plugin-{name}"));
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("plugin.ts"), source).unwrap();
    fs::write(
        package.join("retroarch.ts"),
        include_str!("../examples/libretro-retroarch.ts"),
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
        requires: Vec::new(),
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
        include_str!("../examples/libretro-retroarch.ts"),
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
        requires: Vec::new(),
    }])
    .unwrap()
}
