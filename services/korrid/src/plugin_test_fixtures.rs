//! Real filesystem package for registry tests; production reads the root-owned
//! committed selection projection instead. Sources are the actual plugin files.
use crate::{plugin::PluginRegistry, plugin_installation::EnabledPackage};
use std::{collections::BTreeMap, fs, path::Path};

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
    PluginRegistry::from_installed(vec![EnabledPackage {
        id: "@korri:mgba".into(),
        package: mgba,
        files: BTreeMap::from([
            ("retroarch".into(), root.join("retroarch")),
            ("autoconfig".into(), root.join("autoconfig")),
            ("mgba".into(), root.join("mgba.so")),
        ]),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into(), "retroarch.ts".into()],
    }])
    .unwrap()
}
