//! Real filesystem packages for registry tests; production reads the root-owned
//! committed selection projection instead. Sources are the actual plugin files.
use crate::{plugin::PluginRegistry, plugin_installation::EnabledPackage};
use std::{collections::BTreeMap, fs, path::Path};

pub fn installed(root: &Path) -> PluginRegistry {
    let retroarch = root.join("plugin-retroarch");
    let mgba = root.join("plugin-mgba");
    fs::create_dir_all(&retroarch).unwrap();
    fs::create_dir_all(&mgba).unwrap();
    fs::write(
        retroarch.join("plugin.ts"),
        include_str!("../examples/linux-retroarch.plugin.ts"),
    )
    .unwrap();
    fs::write(
        mgba.join("plugin.ts"),
        include_str!("../examples/linux-mgba.plugin.ts"),
    )
    .unwrap();
    PluginRegistry::from_installed(vec![
        EnabledPackage {
            id: "@korri:retroarch".into(),
            package: retroarch.clone(),
            files: BTreeMap::from([
                ("retroarch".into(), root.join("retroarch")),
                ("autoconfig".into(), root.join("autoconfig")),
            ]),
            requires: vec![],
        },
        EnabledPackage {
            id: "@korri:mgba".into(),
            package: mgba,
            files: BTreeMap::from([("mgba".into(), root.join("mgba.so"))]),
            requires: vec![retroarch],
        },
    ])
    .unwrap()
}
