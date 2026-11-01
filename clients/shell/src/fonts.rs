//! Fonts across switches (contract 4: memory flat over 20 switches).
//!
//! Slint 1.18.1's software renderer registered a `.slint` file's imported
//! fonts again on every component creation, and its swash font cache keeps
//! every registered font blob for the life of the thread. A shell that
//! switches surfaces all day grew by each package's font files per switch
//! (Zone's Anton: 123 KiB). Unregistering the faces from Slint's shared
//! fontique collection after a switch does not release that memory, because
//! the swash cache still holds the blobs (measured: `tests/memory_probe.rs`).
//!
//! The vendored renderer therefore registers each unchanged font file once
//! per process (`vendor/README.md`, `register_font_from_path`). A package's
//! faces stay registered after the shell switches away; switching back adds
//! nothing. The memory held is bounded by the installed packages' font files.

use slint::fontique_011::shared_collection;

/// Registered faces of a family in Slint's shared collection (tests, stats).
pub fn registered_faces(family: &str) -> usize {
    let mut collection = shared_collection();
    collection
        .family_id(family)
        .and_then(|id| collection.family(id))
        .map_or(0, |info| info.fonts().len())
}
