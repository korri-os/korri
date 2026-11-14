// SPDX-License-Identifier: GPL-3.0-or-later
// The frame record and the player's picture, sound and input are platform
// parts. Linux carries Moonlight's C code for them; Android carries none yet.
#[cfg(not(target_os = "android"))]
mod frame_times;
pub mod launch;
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
