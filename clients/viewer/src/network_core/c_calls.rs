// SPDX-License-Identifier: GPL-3.0-or-later
//! The carried C picture and input code's calls into the network core
//! (`c_calls.h` renames them to these): each becomes one `Packet`, or one
//! reading, of the active core. A `LiSendKeyboardEvent` is a
//! `LiSendKeyboardEvent2` with no flags and a `LiSendScrollEvent` of N clicks
//! a high-resolution scroll of N × 120, as the C core itself makes them
//! (`InputStream.c` at the pin).
#![allow(
    non_snake_case,
    reason = "Limelight.h's names, under the seam's prefix"
)]
use super::{ControllerState, Packet, active};
use crate::{callback::guarded, network as ffi};
use std::ffi::{c_char, c_int, c_schar, c_short, c_uchar};

/// Moonlight's `LI_WHEEL_DELTA`: one scroll click.
const WHEEL_DELTA: i16 = 120;

fn send(packet: Packet) -> c_int {
    // A panic must not unwind into the C caller.
    guarded("input", -1, || active().send(packet))
}

#[unsafe(no_mangle)]
extern "C" fn viewer_core_LiSendKeyboardEvent(
    code: c_short,
    action: c_char,
    modifiers: c_char,
) -> c_int {
    send(Packet::Keyboard {
        code,
        down: action as u8 == ffi::KEY_ACTION_DOWN as u8,
        modifiers: modifiers as u8,
        flags: 0,
    })
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments, reason = "Limelight.h's signature")]
extern "C" fn viewer_core_LiSendMultiControllerEvent(
    number: c_short,
    active_mask: c_short,
    buttons: c_int,
    left_trigger: c_uchar,
    right_trigger: c_uchar,
    left_stick_x: c_short,
    left_stick_y: c_short,
    right_stick_x: c_short,
    right_stick_y: c_short,
) -> c_int {
    send(Packet::Controller(ControllerState {
        number: number as u16,
        active_mask: active_mask as u16,
        buttons: buttons as u32,
        left_trigger,
        right_trigger,
        left_stick_x,
        left_stick_y,
        right_stick_x,
        right_stick_y,
    }))
}

#[unsafe(no_mangle)]
extern "C" fn viewer_core_LiSendControllerArrivalEvent(
    number: u8,
    active_mask: u16,
    kind: u8,
    buttons: u32,
    capabilities: u16,
) -> c_int {
    send(Packet::Arrival {
        number,
        active_mask,
        kind,
        buttons,
        capabilities,
    })
}

#[unsafe(no_mangle)]
extern "C" fn viewer_core_LiSendMouseMoveEvent(dx: c_short, dy: c_short) -> c_int {
    send(Packet::MouseMove { dx, dy })
}

#[unsafe(no_mangle)]
extern "C" fn viewer_core_LiSendMousePositionEvent(
    x: c_short,
    y: c_short,
    width: c_short,
    height: c_short,
) -> c_int {
    send(Packet::MousePosition {
        x,
        y,
        width,
        height,
    })
}

#[unsafe(no_mangle)]
extern "C" fn viewer_core_LiSendMouseButtonEvent(action: c_char, button: c_int) -> c_int {
    send(Packet::MouseButton {
        pressed: action as u8 == ffi::BUTTON_ACTION_PRESS as u8,
        // The C core keeps the button's low byte.
        button: button as u8,
    })
}

#[unsafe(no_mangle)]
extern "C" fn viewer_core_LiSendScrollEvent(clicks: c_schar) -> c_int {
    send(Packet::Scroll {
        amount: i16::from(clicks) * WHEEL_DELTA,
    })
}

#[unsafe(no_mangle)]
extern "C" fn viewer_core_LiSendHScrollEvent(clicks: c_schar) -> c_int {
    send(Packet::HScroll {
        amount: i16::from(clicks) * WHEEL_DELTA,
    })
}

#[unsafe(no_mangle)]
extern "C" fn viewer_core_LiSendHighResScrollEvent(amount: c_short) -> c_int {
    send(Packet::Scroll { amount })
}

#[unsafe(no_mangle)]
extern "C" fn viewer_core_LiSendHighResHScrollEvent(amount: c_short) -> c_int {
    send(Packet::HScroll { amount })
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments, reason = "Limelight.h's signature")]
extern "C" fn viewer_core_LiSendTouchEvent(
    kind: u8,
    pointer: u32,
    x: f32,
    y: f32,
    pressure: f32,
    contact_major: f32,
    contact_minor: f32,
    rotation: u16,
) -> c_int {
    send(Packet::Touch {
        kind,
        pointer,
        x,
        y,
        pressure,
        contact_major,
        contact_minor,
        rotation,
    })
}

#[unsafe(no_mangle)]
extern "C" fn viewer_core_LiSendControllerTouchEvent(
    number: u8,
    kind: u8,
    pointer: u32,
    x: f32,
    y: f32,
    pressure: f32,
) -> c_int {
    send(Packet::ControllerTouch {
        number,
        kind,
        pointer,
        x,
        y,
        pressure,
    })
}

#[unsafe(no_mangle)]
extern "C" fn viewer_core_LiSendControllerMotionEvent(
    number: u8,
    kind: u8,
    x: f32,
    y: f32,
    z: f32,
) -> c_int {
    send(Packet::ControllerMotion {
        number,
        kind,
        x,
        y,
        z,
    })
}

/// The frame record's clock (`picture/frame_times.c`): the active core's,
/// whose clock stamps each decode unit.
#[unsafe(no_mangle)]
extern "C" fn viewer_core_LiGetMicroseconds() -> u64 {
    guarded("clock", 0, || active().now_us())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_core::tests::{Recording, installed};
    use std::sync::{LazyLock, Mutex};

    static CORE: LazyLock<Recording> = LazyLock::new(|| Recording {
        packets: Mutex::new(Vec::new()),
        answer: 0,
        clock_us: 987_654,
    });

    #[test]
    fn each_carried_call_is_one_packet_of_the_active_core() {
        let _installed = installed(&*CORE);
        CORE.packets.lock().unwrap().clear();
        let down = ffi::KEY_ACTION_DOWN as c_char;
        let up = ffi::KEY_ACTION_UP as c_char;
        let press = ffi::BUTTON_ACTION_PRESS as c_char;
        let release = ffi::BUTTON_ACTION_RELEASE as c_char;
        assert_eq!(viewer_core_LiSendKeyboardEvent(0x41, down, 0x01), 0);
        viewer_core_LiSendKeyboardEvent(-32_704, up, 0);
        viewer_core_LiSendMultiControllerEvent(1, 3, 0x1000, 255, 0, -32_768, 32_767, 5, -5);
        viewer_core_LiSendControllerArrivalEvent(2, 7, 1, 0xffff, 0x04);
        viewer_core_LiSendMouseMoveEvent(-3, 4);
        viewer_core_LiSendMousePositionEvent(10, 20, 1920, 1080);
        viewer_core_LiSendMouseButtonEvent(press, 1);
        viewer_core_LiSendMouseButtonEvent(release, 3);
        viewer_core_LiSendScrollEvent(-2);
        viewer_core_LiSendHScrollEvent(127);
        viewer_core_LiSendHighResScrollEvent(30);
        viewer_core_LiSendHighResHScrollEvent(-30);
        viewer_core_LiSendTouchEvent(1, 9, 0.25, 0.75, 1.0, 0.1, 0.2, 90);
        viewer_core_LiSendControllerTouchEvent(0, 1, 2, 0.5, 0.5, 0.9);
        viewer_core_LiSendControllerMotionEvent(0, 1, 1.0, 2.0, 3.0);
        assert_eq!(viewer_core_LiGetMicroseconds(), 987_654);
        assert_eq!(
            *CORE.packets.lock().unwrap(),
            [
                Packet::Keyboard {
                    code: 0x41,
                    down: true,
                    modifiers: 1,
                    flags: 0
                },
                Packet::Keyboard {
                    code: -32_704,
                    down: false,
                    modifiers: 0,
                    flags: 0
                },
                Packet::Controller(ControllerState {
                    number: 1,
                    active_mask: 3,
                    buttons: 0x1000,
                    left_trigger: 255,
                    right_trigger: 0,
                    left_stick_x: -32_768,
                    left_stick_y: 32_767,
                    right_stick_x: 5,
                    right_stick_y: -5,
                }),
                Packet::Arrival {
                    number: 2,
                    active_mask: 7,
                    kind: 1,
                    buttons: 0xffff,
                    capabilities: 0x04
                },
                Packet::MouseMove { dx: -3, dy: 4 },
                Packet::MousePosition {
                    x: 10,
                    y: 20,
                    width: 1920,
                    height: 1080
                },
                Packet::MouseButton {
                    pressed: true,
                    button: 1
                },
                Packet::MouseButton {
                    pressed: false,
                    button: 3
                },
                Packet::Scroll { amount: -240 },
                Packet::HScroll { amount: 15_240 },
                Packet::Scroll { amount: 30 },
                Packet::HScroll { amount: -30 },
                Packet::Touch {
                    kind: 1,
                    pointer: 9,
                    x: 0.25,
                    y: 0.75,
                    pressure: 1.0,
                    contact_major: 0.1,
                    contact_minor: 0.2,
                    rotation: 90
                },
                Packet::ControllerTouch {
                    number: 0,
                    kind: 1,
                    pointer: 2,
                    x: 0.5,
                    y: 0.5,
                    pressure: 0.9
                },
                Packet::ControllerMotion {
                    number: 0,
                    kind: 1,
                    x: 1.0,
                    y: 2.0,
                    z: 3.0
                },
            ]
        );
    }

    /// Every `Li*` call in the carried C code goes through the seam: a call
    /// that `c_calls.h` does not rename would reach the C core whichever
    /// core streams.
    #[test]
    fn the_carried_c_code_calls_the_core_only_through_the_seam() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let header = std::fs::read_to_string(root.join("src/network_core/c_calls.h")).unwrap();
        let renamed: std::collections::BTreeSet<&str> = header
            .lines()
            .filter_map(|line| line.strip_prefix("#define "))
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        let mut called = std::collections::BTreeSet::new();
        let mut stack = vec![root.join("picture"), root.join("io")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "c") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    for (at, _) in text.match_indices("Li") {
                        let before = text[..at].chars().next_back();
                        if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                            continue;
                        }
                        let name: String = text[at..]
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect();
                        let rest = text[at + name.len()..].trim_start();
                        if name.len() > 2
                            && name.as_bytes()[2].is_ascii_uppercase()
                            && rest.starts_with('(')
                        {
                            called.insert(name);
                        }
                    }
                }
            }
        }
        assert!(called.contains("LiSendKeyboardEvent"), "{called:?}");
        let missing: Vec<_> = called
            .iter()
            .filter(|name| !renamed.contains(name.as_str()))
            .collect();
        assert!(missing.is_empty(), "not renamed by c_calls.h: {missing:?}");
    }
}
