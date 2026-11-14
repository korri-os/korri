// SPDX-License-Identifier: GPL-3.0-or-later
// The frame record and the player's picture, sound and input are platform
// parts. Linux carries Moonlight's C code for them; Android's picture is
// Rust, and it has no sound or input yet.
mod callback;
mod frame_record;
#[cfg(not(target_os = "android"))]
mod frame_times;
pub mod launch;
mod log;
mod network;
pub mod options;
pub mod pair;
#[cfg(not(target_os = "android"))]
mod player_io;
#[cfg(target_os = "android")]
#[path = "android/player_io.rs"]
mod player_io;
pub mod session;
pub mod stream;

pub mod lifecycle;

// Android's picture. The pure parts also build on the host for their tests.
#[cfg(any(test, target_os = "android"))]
#[path = "android/codec.rs"]
mod codec;
#[cfg(any(test, target_os = "android"))]
#[path = "android/decoder.rs"]
mod decoder;
#[cfg(target_os = "android")]
#[path = "android/picture.rs"]
pub mod picture;
#[cfg(any(test, target_os = "android"))]
#[path = "android/placement.rs"]
pub mod placement;
#[cfg(any(test, target_os = "android"))]
#[path = "android/sps.rs"]
mod sps;
