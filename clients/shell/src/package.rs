//! A surface package on disk (contract 3.1):
//!
//! ```text
//! <surface id>/
//!   main.slint   export component Surface inherits Window
//!                export component Overlay inherits Window   (when it draws the gameplay overlay)
//!   logic.js     defines globalThis.surface (bundled from logic.ts)
//!   fonts/, art/ anything else, referenced only from inside the folder
//! ```
//!
//! The folder name is the surface id korrid knows (`pico`, `zone`).

use anyhow::{bail, Context, Result};
use slint_interpreter::{Compiler, ComponentDefinition};
use std::path::{Path, PathBuf};

pub const VIEW: &str = "main.slint";
pub const LOGIC: &str = "logic.js";

/// A surface id: the package folder's name. Lowercase letters, digits and
/// dashes, like the existing `pico` and `zone`.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub id: String,
    pub dir: PathBuf,
}

impl Package {
    pub fn at(dir: &Path) -> Result<Self> {
        let id = dir
            .file_name()
            .and_then(|n| n.to_str())
            .context("package folder has no name")?
            .to_owned();
        if !valid_id(&id) {
            bail!("package folder {id:?} is not a surface id ([a-z0-9-])");
        }
        for entry in [VIEW, LOGIC] {
            if !dir.join(entry).is_file() {
                bail!("package {id} has no {entry}");
            }
        }
        Ok(Self {
            id,
            dir: dir.to_owned(),
        })
    }

    /// Every package folder directly inside `root`, by id.
    pub fn discover(root: &Path) -> Result<Vec<Self>> {
        let mut packages = Vec::new();
        for entry in std::fs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            match Self::at(&entry.path()) {
                Ok(package) => packages.push(package),
                Err(error) => eprintln!(
                    "korri-shell: skipping {}: {error:#}",
                    entry.path().display()
                ),
            }
        }
        packages.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(packages)
    }

    pub fn logic_source(&self) -> Result<String> {
        crate::engine::read_source(&self.dir.join(LOGIC))
    }
}

/// The compiled views of one package. A clone shares the definitions.
#[derive(Clone)]
pub struct Views {
    pub surface: ComponentDefinition,
    pub overlay: Option<ComponentDefinition>,
    pub diagnostics: Vec<String>,
}

/// Compile `main.slint` with the pinned interpreter. Diagnostics are kept.
pub fn compile(package: &Package) -> Result<Views> {
    let mut compiler = Compiler::default();
    // The shell runs a definition from its lowered tree; the compiler's object
    // trees (about 40 MB for Zone) go with the compilation result.
    compiler.set_retain_type_loaders(false);
    let result = pollster::block_on(compiler.build_from_path(package.dir.join(VIEW)));
    let diagnostics: Vec<String> = result.diagnostics().map(|d| d.to_string()).collect();
    if result.has_errors() {
        bail!(
            "{} does not compile: {}",
            package.id,
            diagnostics.join("; ")
        );
    }
    let surface = result
        .component("Surface")
        .with_context(|| format!("{} must export component Surface", package.id))?;
    let overlay = result.component("Overlay");
    drop(result);
    crate::memory::release_free_heap();
    Ok(Views {
        surface,
        overlay,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_folder_names() {
        assert!(valid_id("pico"));
        assert!(valid_id("zone-2"));
        assert!(!valid_id("Zone"));
        assert!(!valid_id("../x"));
        assert!(!valid_id(""));
    }

    /// The compiler's object trees are not kept with the views: the definition runs
    /// from its lowered tree, and the trees were about 40 MB of Zone's heap.
    #[test]
    #[should_panic(expected = "without compiler state")]
    fn compiled_views_keep_no_object_tree() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/zone");
        let views = compile(&Package::at(&dir).unwrap()).unwrap();
        let _ = views.surface.root_component();
    }

    #[test]
    fn a_package_needs_both_entry_points() -> Result<()> {
        let root = tempfile::tempdir()?;
        let dir = root.path().join("demo");
        std::fs::create_dir(&dir)?;
        std::fs::write(
            dir.join(VIEW),
            "export component Surface inherits Window {}",
        )?;
        assert!(Package::at(&dir).is_err());
        std::fs::write(dir.join(LOGIC), "")?;
        let package = Package::at(&dir)?;
        assert_eq!(Package::discover(root.path())?, vec![package.clone()]);
        let views = compile(&package)?;
        assert!(views.overlay.is_none());
        Ok(())
    }
}
