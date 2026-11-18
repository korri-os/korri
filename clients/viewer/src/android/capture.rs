// SPDX-License-Identifier: GPL-3.0-only
//! The mouse's pointer capture over the stream, and the stream control
//! "Toggle local cursor" (#434, toward D7′). Pure: it decides each call on
//! the stream's view, and the app makes them on Android's main thread
//! (`pointer_capture.rs`).
//!
//! As in Artemis, the stream grabs the input at its start
//! (`setInputGrabState(true)`): the pointer hides over the stream, and
//! Android captures it when some device can be captured. A captured mouse
//! sends its moves, not its place, as `SOURCE_MOUSE_RELATIVE`, and the input
//! sends them to the host as relative moves (`touch::Mouse`). "Toggle local
//! cursor" shows the pointer and lets the capture go, so that the mouse
//! points into the picture as it does without capture; toggled again, the
//! pointer hides and is captured. Android loses the capture with the
//! window's focus: focus regained asks for it again 500 ms later. While the
//! pointer hides, a device that comes or goes makes the view take the
//! capture, or let it go, by what is attached then. At the stream's end the
//! input lets go (`setInputGrabState(false)`): the pointer shows and the
//! capture goes.
//!
//! Android tells native code of no device that comes or goes (that needs
//! `InputDeviceListener`, a Java interface): the input reads the device list
//! twice a second (`Input::check_devices`) and follows each change.
//!
//! Ported from Korri's copy of Artemis (`clients/android`, from
//! `simonwjackson/artemis` at `cf12c432`): `InputCaptureProvider.java`,
//! `AndroidPointerIconCaptureProvider.java` and
//! `AndroidNativePointerCaptureProvider.java`
//! (`binding/input/capture/`), and `Game.java`'s `setInputGrabState`,
//! `toggleLocalCursor`, `onWindowFocusChanged` and `viewerHasPointer`.
//! Artemis ships under the GNU GPL version 3 with no "or later" grant, so
//! this file does too. The Rust adaptation is Korri work dated 2026-11-17.
use super::input::{Device, Devices, source};

/// How long after the focus comes back the capture is asked for again
/// (`AndroidNativePointerCaptureProvider.onWindowFocusChanged`: on Android
/// 10 an earlier request finds a window with no focus).
const RECAPTURE_MS: i64 = 500;

/// One call on the stream's view, which the app makes on Android's main
/// thread, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    /// `setPointerIcon(PointerIcon.getSystemIcon(…, TYPE_NULL))`: no
    /// pointer over the stream.
    HideIcon,
    /// `setPointerIcon(null)`: the pointer shows again.
    ShowIcon,
    /// `requestPointerCapture()`.
    Request,
    /// `releasePointerCapture()`.
    Release,
    /// A device came or went (`onInputDeviceRemoved`, then
    /// `onInputDeviceAdded`): a view with the capture lets it go when no
    /// device can be captured, and a view without it takes it when one can.
    /// Only the main thread knows whether the view has it
    /// (`hasPointerCapture`).
    Follow { compatible: bool },
}

/// What the attached devices offer the mouse, read from the device list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Pointers {
    /// The devices that it was read from, in order.
    ids: Vec<i32>,
    /// Some device is a mouse or a touchpad (`viewerHasPointer`).
    pub(crate) pointer: bool,
    /// Some device can be captured (`hasCaptureCompatibleInputDevice`).
    pub(crate) compatible: bool,
}

impl Pointers {
    /// `chromeos`: the device is a Chromebook
    /// (`org.chromium.arc.device_management`), whose touch screens can be
    /// captured.
    pub(crate) fn read(ids: &[i32], devices: &mut dyn Devices, chromeos: bool) -> Self {
        let mut ids = ids.to_vec();
        ids.sort_unstable();
        let attached: Vec<_> = ids.iter().filter_map(|&id| devices.describe(id)).collect();
        Self {
            pointer: attached
                .iter()
                .any(|d| supports(d, source::MOUSE) || supports(d, source::TOUCHPAD)),
            compatible: attached.iter().any(|d| compatible(d, chromeos)),
            ids,
        }
    }

    /// Whether `other` was read from another set of devices.
    pub(crate) fn changed(&self, other: &Pointers) -> bool {
        self.ids != other.ids
    }
}

/// `InputDevice.supportsSource`.
fn supports(device: &Device, source: u32) -> bool {
    device.sources & source == source
}

/// `hasCaptureCompatibleInputDevice`, for one device: a mouse, a relative
/// mouse or a touchpad. A touch screen never is but on ChromeOS: a Samsung
/// phone's `sec_touchpad` on Android 12 is a touch screen, a keyboard and a
/// mouse, and capturing it breaks the S Pen (moonlight-android #1030).
fn compatible(device: &Device, chromeos: bool) -> bool {
    if supports(device, source::TOUCHSCREEN) && !chromeos {
        return false;
    }
    supports(device, source::MOUSE)
        || supports(device, source::MOUSE_RELATIVE)
        || supports(device, source::TOUCHPAD)
}

/// The grab and the capture: `Game`'s `grabbedInput` and `cursorVisible`,
/// and the capture provider's `isCapturing`, `isCursorVisible` and device
/// listener.
#[derive(Debug)]
pub(crate) struct Capture {
    /// `grabbedInput`: true from the start, as Artemis's.
    grabbed: bool,
    /// `cursorVisible`: the local cursor shows. Off at the start
    /// (`checkbox_mouse_local_cursor`).
    cursor_visible: bool,
    /// `isCapturing`: the mouse is the stream's.
    capturing: bool,
    /// `isCursorVisible`: the provider shows the pointer.
    cursor_shown: bool,
    /// The device listener is registered (`hideCursor` until `showCursor`).
    listening: bool,
    /// The view was last asked to take the capture, not to let it go.
    requested: bool,
    /// When focus regained asks for the capture again.
    recapture_at: Option<i64>,
    calls: Vec<Call>,
}

impl Default for Capture {
    fn default() -> Self {
        Self {
            grabbed: true,
            cursor_visible: false,
            capturing: false,
            cursor_shown: false,
            listening: false,
            requested: false,
            recapture_at: None,
            calls: Vec::new(),
        }
    }
}

impl Capture {
    /// `setInputGrabState`: grab the input, the pointer hidden or shown as
    /// the local cursor says, or let it go.
    pub(crate) fn grab(&mut self, grab: bool, compatible: bool) {
        if grab {
            self.enable(compatible);
            // Enabling capture may hide the cursor again.
            if self.cursor_visible {
                self.show();
            }
        } else {
            self.disable();
        }
        self.grabbed = grab;
    }

    /// `toggleLocalCursor`.
    pub(crate) fn toggle_local_cursor(&mut self, compatible: bool) {
        if !self.grabbed {
            self.enable(compatible);
            self.grabbed = true;
        }
        self.cursor_visible = !self.cursor_visible;
        if self.cursor_visible {
            self.show();
        } else {
            self.hide(compatible);
        }
    }

    /// `onWindowFocusChanged`: Android's capture goes with the window's
    /// focus, so focus regained asks for it again, 500 ms later.
    pub(crate) fn focus(&mut self, focused: bool, now_ms: i64) {
        if !focused || !self.capturing || self.cursor_shown {
            return;
        }
        self.recapture_at = Some(now_ms + RECAPTURE_MS);
    }

    /// The capture that focus regained asks for, when it is due.
    pub(crate) fn tick(&mut self, now_ms: i64, compatible: bool) {
        if self.recapture_at.is_some_and(|at| now_ms >= at) {
            self.recapture_at = None;
            if compatible {
                self.request();
            }
        }
    }

    pub(crate) fn next_deadline(&self) -> Option<i64> {
        self.recapture_at
    }

    /// A device came or went (`onInputDeviceChanged`): followed while the
    /// listener is registered.
    pub(crate) fn devices_changed(&mut self, compatible: bool) {
        if self.listening {
            self.calls.push(Call::Follow { compatible });
            self.requested = compatible;
        }
    }

    /// `grabbedInput`: the stream's input takes the motion events.
    pub(crate) fn grabbed(&self) -> bool {
        self.grabbed
    }

    /// `isCapturingActive`: the mouse is the stream's.
    pub(crate) fn capturing(&self) -> bool {
        self.capturing
    }

    /// `targetView.hasPointerCapture()`, as the view was last asked
    /// (`eventHasRelativeMouseAxes`): Android grants the capture, or ends it
    /// with the window's focus, on its own.
    pub(crate) fn pointer_capture(&self) -> bool {
        self.requested
    }

    /// `Game.localCursor`.
    pub(crate) fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    /// The calls on the view since the last take, in order.
    pub(crate) fn take_calls(&mut self) -> Vec<Call> {
        std::mem::take(&mut self.calls)
    }

    /// `enableCapture`.
    fn enable(&mut self, compatible: bool) {
        self.capturing = true;
        self.hide(compatible);
    }

    /// `disableCapture`.
    fn disable(&mut self) {
        self.capturing = false;
        self.show();
    }

    /// `hideCursor`: the icon hides, the listener registers, and the view
    /// captures now when a device can be captured.
    fn hide(&mut self, compatible: bool) {
        self.cursor_shown = false;
        self.calls.push(Call::HideIcon);
        self.listening = true;
        if compatible {
            self.request();
        }
    }

    /// `showCursor`: the icon shows; the listener goes before the capture,
    /// whose release can change a device. A capture that a regained focus
    /// still waits to ask for is not asked for: the pointer shows now, for
    /// the overlay (`disableCapture`) or the local cursor. Artemis's
    /// delayed request asks anyway.
    fn show(&mut self) {
        self.cursor_shown = true;
        self.recapture_at = None;
        self.calls.push(Call::ShowIcon);
        self.listening = false;
        self.calls.push(Call::Release);
        self.requested = false;
    }

    /// `requestPointerCapture`.
    fn request(&mut self) {
        self.calls.push(Call::Request);
        self.requested = true;
    }
}
