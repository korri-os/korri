// SPDX-License-Identifier: GPL-3.0-or-later
//! Private FFI, generated against exactly the network core Nix builds.
#![allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    dead_code,
    clippy::all
)]
include!(concat!(env!("OUT_DIR"), "/network.rs"));
