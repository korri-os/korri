//! Font release on switch (contract 4: memory flat over 20 switches).
//!
//! Slint's software renderer reads and registers every font a `.slint` file
//! imports each time a component is created, into one process-wide fontique
//! collection, and never removes it. A shell that switches surfaces all day
//! would keep every copy. The shell therefore counts, per family name, the
//! live surfaces whose package ships that family, and unregisters the
//! family's faces when the last one is unloaded.
//!
//! Which families a package ships is read from its font files with a private
//! collection that has no system fonts, so the shared collection is never
//! scanned and system families are never touched.

use slint::fontique_011::fontique;
use std::{
    collections::HashMap,
    path::Path,
    sync::Arc,
};

const FONT_EXTENSIONS: [&str; 4] = ["ttf", "otf", "ttc", "otc"];

/// Family names declared by the font files inside a package.
pub fn package_families(dir: &Path) -> Vec<String> {
    let mut collection = fontique::Collection::new(fontique::CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    let mut names = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| FONT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            {
                let Ok(bytes) = std::fs::read(&path) else { continue };
                for (family, _) in collection.register_fonts(fontique::Blob::new(Arc::new(bytes)), None) {
                    if let Some(name) = collection.family_name(family) {
                        if !names.iter().any(|n| n == name) {
                            names.push(name.to_owned());
                        }
                    }
                }
            }
        }
    }
    names.sort();
    names
}

/// The families one loaded surface holds.
#[derive(Debug, Default)]
pub struct FontLease {
    families: Vec<String>,
}

#[derive(Default)]
pub struct FontLedger {
    live: HashMap<String, usize>,
}

impl FontLedger {
    pub fn acquire(&mut self, families: Vec<String>) -> FontLease {
        for family in &families {
            *self.live.entry(family.clone()).or_default() += 1;
        }
        FontLease { families }
    }

    /// Call after the surface's components are dropped. Returns the families
    /// whose faces were unregistered.
    pub fn release(&mut self, lease: FontLease) -> Vec<String> {
        let mut released = Vec::new();
        for family in lease.families {
            let Some(count) = self.live.get_mut(&family) else { continue };
            *count -= 1;
            if *count == 0 {
                self.live.remove(&family);
                if unregister(&family) {
                    released.push(family);
                }
            }
        }
        released
    }
}

/// Remove every registered face of `family` from Slint's shared collection.
/// System fonts are a separate store and are never affected.
pub fn unregister(family: &str) -> bool {
    let mut collection = slint::fontique_011::shared_collection();
    let Some(id) = collection.family_id(family) else {
        return false;
    };
    let Some(info) = collection.family(id) else {
        return false;
    };
    let faces: Vec<_> = info.fonts().iter().map(|f| (f.width(), f.style(), f.weight())).collect();
    let mut removed = false;
    for (width, style, weight) in faces {
        removed |= collection.unregister_font(id, width, style, weight);
    }
    removed
}

/// Registered faces of a family in the shared collection (tests, stats).
pub fn registered_faces(family: &str) -> usize {
    let mut collection = slint::fontique_011::shared_collection();
    collection
        .family_id(family)
        .and_then(|id| collection.family(id))
        .map_or(0, |info| info.fonts().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_package_families_without_system_fonts() {
        let pico = Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/pico");
        assert_eq!(package_families(&pico), vec!["Pico Tiny5".to_owned()]);
        let dir = tempfile::tempdir().unwrap();
        assert!(package_families(dir.path()).is_empty());
    }

    #[test]
    fn counts_live_holders_before_releasing() {
        let mut ledger = FontLedger::default();
        let a = ledger.acquire(vec!["Never Registered".into()]);
        let b = ledger.acquire(vec!["Never Registered".into()]);
        assert_eq!(ledger.live["Never Registered"], 2);
        ledger.release(a);
        assert_eq!(ledger.live["Never Registered"], 1);
        drop(b);
    }
}
