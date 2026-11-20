//! Real filesystem package for registry tests; production reads the root-owned
//! committed selection projection instead. Sources are the actual plugin files.
use crate::{plugin::PluginRegistry, plugin_installation::EnabledPackage};
use std::{collections::BTreeMap, fs, path::Path};

/// A system that identifies `.gba` files and a runner that builds a launch plan.
/// It imports nothing, so it loads from source without a module graph.
const GBA_CLAIM: &str = r#"
export const name = "core";
export const title = "GBA Core";
export const systems = { gba: { title: "Game Boy Advance", content: { extensions: ["gba"] } } };
export const runners = {
  core: { program: "retroarch", systems: ["gba"] },
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

/// A Steam plugin `@korri:steam` whose client runner `steam` and engine
/// runner `engine` (ADR 0083) both run the GBA fixture game. The engine
/// package is ready in `root`'s cache, and the plugin has its engine host.
pub fn installed_steam_pair(root: &Path) -> PluginRegistry {
    steam_pair(root, true)
}

/// `installed_steam_pair` whose package carries no engine host.
pub fn installed_steam_pair_without_host(root: &Path) -> PluginRegistry {
    steam_pair(root, false)
}

fn steam_pair(root: &Path, host: bool) -> PluginRegistry {
    let runners = |name: &str| {
        let artifacts = if name == "engine" {
            "    artifacts: [\"@korri:steam/engine-package\"],\n"
        } else {
            ""
        };
        format!(
            "  \"{name}\": {{\n    family: \"@korri:retroarch\",\n    program: \"retroarch\",\n    core: \"mgba\",\n    systems: [\"gba\"],\n{artifacts}  }},\n"
        )
    };
    let source = GENERATED_CORE
        .split("export const controls")
        .next()
        .unwrap()
        .replace("export const name = \"mgba\"", "export const name = \"steam\"")
        .replace("export const title = \"mGBA\"", "export const title = \"Steam\"")
        .replace(
            "  \"mgba\": {\n    family: \"@korri:retroarch\",\n    program: \"retroarch\",\n    core: \"mgba\",\n    systems: [\"gba\"],\n  },\n",
            &(runners("steam") + &runners("engine")),
        );
    assert!(source.contains("\"engine\""), "{source}");
    let source = format!(
        "{source}\nexport const artifacts = {{ \"engine-package\": {{ id: \"@korri:steam/engine-package\", version: \"1\", files: [{{ url: \"https://client-update.steamstatic.com/engine.zip\", sha256: \"sha256:e043d05b4e7bcc43e22cc2be3e8872ed1132e79e989cbbe181eb3a5d3f849df7\" }}] }} }}\n"
    );
    fs::create_dir_all(root.join("cache/steam/engine-package/1")).unwrap();
    let mut package = core_package(root, "steam", source);
    package.files.insert("mgba".into(), root.join("mgba.so"));
    if host {
        package
            .files
            .insert("korri-steam-host".into(), root.join("korri-steam-host"));
    }
    PluginRegistry::from_installed(vec![package]).unwrap()
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
    fs::write(
        package.join("manifest.json"),
        r#"{"entry":"plugin.ts","sources":["plugin.ts","retroarch.ts","settings.ts"]}"#,
    )
    .unwrap();
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
        hosts: vec![],
        credential_host: None,
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
    fs::write(
        mgba.join("manifest.json"),
        r#"{"entry":"plugin.ts","sources":["plugin.ts","retroarch.ts","settings.ts"]}"#,
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
        hosts: vec![],
        credential_host: None,
    }])
    .unwrap()
}

/// The id of the artwork plugin fixture (ADR 0032).
pub const ART_PLUGIN_ID: &str = "@korri:steamgriddb";

/// An installed artwork plugin: `source` is its `plugin.ts`, `hosts` the
/// hosts it may reach and `credential_host` the one that receives its secret
/// (ADR 0034).
pub fn art_package(
    root: &Path,
    source: &str,
    hosts: &[&str],
    credential_host: Option<&str>,
) -> EnabledPackage {
    art_package_as(root, ART_PLUGIN_ID, source, hosts, credential_host)
}

/// `art_package` under another plugin id, in its own folder.
pub fn art_package_as(
    root: &Path,
    id: &str,
    source: &str,
    hosts: &[&str],
    credential_host: Option<&str>,
) -> EnabledPackage {
    let folder = id.rsplit(':').next().unwrap_or(id);
    let package = root.join(format!("plugin-{folder}"));
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("plugin.ts"), source).unwrap();
    fs::write(
        package.join("manifest.json"),
        r#"{"entry":"plugin.ts","sources":["plugin.ts"]}"#,
    )
    .unwrap();
    EnabledPackage {
        id: id.into(),
        package,
        files: BTreeMap::new(),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
        requires: Vec::new(),
        hosts: hosts.iter().map(|host| (*host).to_owned()).collect(),
        credential_host: credential_host.map(str::to_owned),
    }
}

/// `installed` beside an artwork plugin that declares a secret for
/// `www.steamgriddb.com` and finds nothing: enough for the settings rows and
/// the secret RPCs.
pub fn installed_with_secret_plugin(root: &Path) -> PluginRegistry {
    installed_with_art(
        root,
        art_package(
            root,
            "export const name = 'steamgriddb'; export const title = 'SteamGridDB'; export const handlers = {'game-assets.candidates': () => ({candidates: []})};",
            &["www.steamgriddb.com", "cdn2.steamgriddb.com"],
            Some("www.steamgriddb.com"),
        ),
    )
}

/// The mGBA core of `installed` beside one artwork plugin.
pub fn installed_with_art(root: &Path, art: EnabledPackage) -> PluginRegistry {
    installed_with_arts(root, vec![art])
}

/// The mGBA core of `installed` beside several artwork plugins.
pub fn installed_with_arts(root: &Path, arts: Vec<EnabledPackage>) -> PluginRegistry {
    let mut packages = vec![core_package(root, "mgba", GENERATED_CORE.to_owned())];
    packages.extend(arts);
    PluginRegistry::from_installed(packages).unwrap()
}

/// The installed Steam declaration and its real interpreter callback.
pub fn steam(root: &Path) -> PluginRegistry {
    steam_in(
        root,
        "@korri",
        include_str!("../tests/fixtures/first-party/steam/plugin.ts"),
    )
}

/// ADR 0083 and ADR 0045: a Steam plugin as `NS:steam` publishes it, with
/// full ids only (`@NS` stands for its namespace; see `steam_source`): no
/// `providers` export, and a discovery handler that builds `P` and `P/steam`
/// from `input.pluginId`.
pub const STEAM_PLUGIN: &str = r#"
export const name = "steam"
export const title = "Steam"
export const discovery = [{ id: "@NS:steam/installed-apps", title: "Steam installed apps" }]
export const systems = { steam: { title: "Steam" } }
export const artifacts = {
  "engine-package": {
    id: "@NS:steam/engine-package",
    version: "1792618285",
    files: [{ url: "https://client-update.steamstatic.com/bins_sdk_linuxarm64_linuxarm64.zip.vz.1424aaa3", sha256: "sha256:e043d05b4e7bcc43e22cc2be3e8872ed1132e79e989cbbe181eb3a5d3f849df7", bytes: 10044409 }],
  },
}
export const runners = {
  steam: { program: "steam-session", systems: ["steam"] },
  engine: { program: "steam-engine-session", systems: ["steam"], artifacts: ["@NS:steam/engine-package"] },
}
function discover(input) {
  const out = []
  for (const file of input.files) {
    const m = /^steamapps\/appmanifest_(\d+)\.acf$/.exec(file.relativePath)
    const name = /"name"\s+"([^"]+)"/.exec(file.text)
    if (!m || !name) continue
    out.push({
      kind: "provider-ref-release", confidence: "high",
      source: { relativePath: file.relativePath },
      target: { provider: input.pluginId, ref: m[1] },
      release: { id: "steam", title: name[1], system: "steam" },
      launch: { use: `${input.pluginId}/steam` },
    })
  }
  return out
}
export const handlers = { "discovery.discover": discover }
"#;

/// `STEAM_PLUGIN` under `namespace` (`@korri`, `@other`).
pub fn steam_source(namespace: &str) -> String {
    STEAM_PLUGIN.replace("@NS", namespace)
}

/// The Steam fixture under `namespace`, with this `plugin.ts`.
pub fn steam_in(root: &Path, namespace: &str, source: &str) -> PluginRegistry {
    let package = root.join("plugin-steam");
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("plugin.ts"), source).unwrap();
    let mut dependencies = Vec::new();
    for (name, source, keys) in [
        (
            "fex",
            include_str!("../tests/fixtures/first-party/fex/plugin.ts"),
            vec!["fex-unpack", "config"],
        ),
        (
            "proton",
            include_str!("../tests/fixtures/first-party/proton/plugin.ts"),
            vec!["cachyos-unpack"],
        ),
    ] {
        let directory = root.join(format!("plugin-{name}"));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("plugin.ts"), source).unwrap();
        dependencies.push(EnabledPackage {
            id: format!("@korri:{name}"),
            package: directory.clone(),
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            requires: vec![],
            hosts: vec![],
            credential_host: None,
            files: keys
                .into_iter()
                .map(|key| (key.into(), directory.join(key)))
                .collect(),
        });
    }
    let requires = dependencies
        .iter()
        .map(|package| package.package.clone())
        .collect();
    dependencies.push(EnabledPackage {
        id: format!("{namespace}:steam"),
        package: package.clone(),
        files: BTreeMap::from([
            ("steam-session".into(), package.join("steam-session")),
            (
                "steam-engine-session".into(),
                package.join("steam-engine-session"),
            ),
        ]),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
        requires,
        hosts: vec![],
        credential_host: None,
    });
    PluginRegistry::from_installed(dependencies).unwrap()
}
