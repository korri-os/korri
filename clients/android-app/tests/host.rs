// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's rules that need no Android, on the build machine. The library
//! builds for Android only (`src/lib.rs`), so this test crate takes the
//! rules' file itself.
#[path = "../src/viewer/rules.rs"]
mod rules;
