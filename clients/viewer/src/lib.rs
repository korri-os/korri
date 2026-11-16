// SPDX-License-Identifier: GPL-3.0-or-later
// The frame record and the player's picture, sound and input are platform
// parts. Linux carries Moonlight's C code for them; Android's are Rust.
mod callback;
pub mod controls;
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

// Android's picture, sound and input. The pure parts also build on the host
// for their tests.
#[cfg(any(test, target_os = "android"))]
#[path = "android/answer.rs"]
mod answer;
#[cfg(any(test, target_os = "android"))]
#[path = "android/audio.rs"]
mod audio;
#[cfg(any(test, target_os = "android"))]
#[path = "android/codec.rs"]
mod codec;
#[cfg(any(test, target_os = "android"))]
#[path = "android/controller_list.rs"]
mod controller_list;
#[cfg(any(test, target_os = "android"))]
#[path = "android/decoder.rs"]
mod decoder;
#[cfg(target_os = "android")]
#[path = "android/devices.rs"]
pub mod devices;
// An app's own parts around the stream: android-activity's events to the
// input, the video and overlay layers, and the input with the overlay on the
// app's thread. The viewer's own app and the Korri app share them.
#[cfg(target_os = "android")]
#[path = "android/events.rs"]
pub mod events;
#[cfg(any(test, target_os = "android"))]
#[path = "android/feedback.rs"]
pub mod feedback;
#[cfg(target_os = "android")]
#[path = "android/feedback_jni.rs"]
pub mod feedback_jni;
#[cfg(any(test, target_os = "android"))]
#[path = "android/gamepad.rs"]
mod gamepad;
#[cfg(any(test, target_os = "android"))]
#[path = "android/input.rs"]
pub mod input;
#[cfg(any(test, target_os = "android"))]
#[path = "android/keyboard.rs"]
mod keyboard;
#[cfg(target_os = "android")]
#[path = "android/layer.rs"]
pub mod layer;
#[cfg(any(test, target_os = "android"))]
#[path = "android/overlay.rs"]
pub mod overlay;
#[cfg(target_os = "android")]
#[path = "android/picture.rs"]
pub mod picture;
#[cfg(any(test, target_os = "android"))]
#[path = "android/placement.rs"]
pub mod placement;
#[cfg(target_os = "android")]
#[path = "android/play.rs"]
pub mod play;
#[cfg(target_os = "android")]
#[path = "android/sound.rs"]
mod sound;
#[cfg(any(test, target_os = "android"))]
#[path = "android/sps.rs"]
mod sps;
// The stream's battery and health for its controller (slice D11).
#[cfg(target_os = "android")]
#[path = "android/status.rs"]
pub mod status;
#[cfg(any(test, target_os = "android"))]
#[path = "android/touch.rs"]
mod touch;
#[cfg(any(test, target_os = "android"))]
#[path = "android/video_stats.rs"]
mod video_stats;
