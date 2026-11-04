//! Korri's native shell: surface packages (`.slint` views and QuickJS logic)
//! loaded at run time over the local korrid. Binding contract:
//! `docs/briefs/2026-10-31-native-surfaces-contract.md`.

pub mod backend;
pub mod bench;
pub mod catalog;
pub mod config;
pub mod engine;
pub mod fonts;
pub mod images;
pub mod input;
pub mod korrid;
pub mod logic;
pub mod memory;
pub mod messages;
pub mod package;
pub mod pico8;
pub mod shell;
pub mod targets;
pub mod verify;
pub mod view;

#[cfg(feature = "fixture")]
pub mod fixture;
