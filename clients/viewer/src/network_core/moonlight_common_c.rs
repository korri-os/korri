// SPDX-License-Identifier: GPL-3.0-or-later
//! Moonlight's C core, `moonlight-common-c` at the pin in `default.nix`,
//! through the binding that `build.rs` generates from its own `Limelight.h`.
//! Each call is the core's own; nothing is added.
use super::{Connection, NetworkCore, Packet};
use crate::network as ffi;
use anyhow::Result;
use std::{ffi::CStr, ptr::null_mut};

pub(crate) struct MoonlightCommonC;

impl NetworkCore for MoonlightCommonC {
    fn launch_parameters(&self) -> Result<&'static str> {
        // SAFETY: a process-lifetime immutable string, owned by C.
        Ok(unsafe { CStr::from_ptr(ffi::LiGetLaunchUrlQueryParameters()) }.to_str()?)
    }

    fn start(&self, c: Connection<'_>) -> i32 {
        // SAFETY: the caller keeps every structure, string and callback
        // table alive until `stop` has joined the core's threads.
        unsafe {
            ffi::LiStartConnection(
                c.server,
                c.stream,
                c.listener,
                c.video,
                c.audio,
                null_mut(),
                c.video_flags,
                c.audio_context,
                0,
            )
        }
    }

    fn interrupt(&self) {
        // SAFETY: no preconditions; the core reads the flag at its own points.
        unsafe { ffi::LiInterruptConnection() }
    }

    fn stop(&self) {
        // SAFETY: after a successful start, from the thread that owns it.
        unsafe { ffi::LiStopConnection() }
    }

    fn send(&self, packet: Packet) -> i32 {
        // SAFETY: each call copies its arguments into the core's queue, and
        // answers -2 when no stream runs.
        unsafe {
            match packet {
                Packet::Keyboard {
                    code,
                    down,
                    modifiers,
                    flags,
                } => ffi::LiSendKeyboardEvent2(
                    code,
                    if down {
                        ffi::KEY_ACTION_DOWN
                    } else {
                        ffi::KEY_ACTION_UP
                    } as _,
                    modifiers as _,
                    flags as _,
                ),
                Packet::Controller(state) => ffi::LiSendMultiControllerEvent(
                    state.number as i16,
                    state.active_mask as i16,
                    state.buttons as i32,
                    state.left_trigger,
                    state.right_trigger,
                    state.left_stick_x,
                    state.left_stick_y,
                    state.right_stick_x,
                    state.right_stick_y,
                ),
                Packet::Arrival {
                    number,
                    active_mask,
                    kind,
                    buttons,
                    capabilities,
                } => ffi::LiSendControllerArrivalEvent(
                    number,
                    active_mask,
                    kind,
                    buttons,
                    capabilities,
                ),
                Packet::MouseMove { dx, dy } => ffi::LiSendMouseMoveEvent(dx, dy),
                Packet::MousePosition {
                    x,
                    y,
                    width,
                    height,
                } => ffi::LiSendMousePositionEvent(x, y, width, height),
                Packet::MouseButton { pressed, button } => ffi::LiSendMouseButtonEvent(
                    if pressed {
                        ffi::BUTTON_ACTION_PRESS
                    } else {
                        ffi::BUTTON_ACTION_RELEASE
                    } as _,
                    i32::from(button),
                ),
                Packet::Scroll { amount } => ffi::LiSendHighResScrollEvent(amount),
                Packet::HScroll { amount } => ffi::LiSendHighResHScrollEvent(amount),
                Packet::Touch {
                    kind,
                    pointer,
                    x,
                    y,
                    pressure,
                    contact_major,
                    contact_minor,
                    rotation,
                } => ffi::LiSendTouchEvent(
                    kind,
                    pointer,
                    x,
                    y,
                    pressure,
                    contact_major,
                    contact_minor,
                    rotation,
                ),
                Packet::ControllerTouch {
                    number,
                    kind,
                    pointer,
                    x,
                    y,
                    pressure,
                } => ffi::LiSendControllerTouchEvent(number, kind, pointer, x, y, pressure),
                Packet::ControllerMotion {
                    number,
                    kind,
                    x,
                    y,
                    z,
                } => ffi::LiSendControllerMotionEvent(number, kind, x, y, z),
                Packet::Battery {
                    number,
                    state,
                    percentage,
                } => ffi::LiSendControllerBatteryEvent(number, state, percentage),
            }
        }
    }

    fn now_us(&self) -> u64 {
        // SAFETY: no preconditions.
        unsafe { ffi::LiGetMicroseconds() }
    }

    fn rtt_ms(&self) -> Option<u32> {
        let mut rtt = 0;
        // SAFETY: the core reads its peer under its own lock and writes only
        // the one estimate asked for.
        let known = unsafe { ffi::LiGetEstimatedRttInfo(&mut rtt, null_mut()) };
        known.then_some(rtt)
    }

    fn pending_audio_ms(&self) -> i32 {
        // SAFETY: no preconditions.
        unsafe { ffi::LiGetPendingAudioDuration() }
    }

    /// The C core is the fallback core: its stream is not watched (#544).
    fn received(&self) -> Option<crate::link_watch::Received> {
        None
    }
}
