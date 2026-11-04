// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use std::rc::{Rc, Weak};

use i_slint_core::timers::{Timer, TimerMode};

#[cfg(target_vendor = "apple")]
mod apple_display_link;

use crate::winitwindowadapter::WinitWindowAdapter;

pub fn create_frame_throttle(
    window_adapter: Weak<WinitWindowAdapter>,
    _winit_window: &winit::window::Window,
    _is_wayland: bool,
) -> Box<dyn FrameThrottle> {
    if _is_wayland {
        PacedWinitFrameThrottle::create(window_adapter)
    } else {
        #[cfg(target_vendor = "apple")]
        if let Some(throttle) =
            apple_display_link::try_create(window_adapter.clone(), _winit_window)
        {
            return throttle;
        }
        TimerBasedFrameThrottle::create(window_adapter)
    }
}

pub trait FrameThrottle {
    fn request_throttled_redraw(&self, winit_window: &winit::window::Window);
}

struct TimerBasedFrameThrottle {
    window_adapter: Weak<WinitWindowAdapter>,
    timer: Rc<Timer>,
}

impl TimerBasedFrameThrottle {
    fn create(window_adapter: Weak<WinitWindowAdapter>) -> Box<dyn FrameThrottle> {
        Box::new(Self { window_adapter, timer: Rc::new(Timer::default()) })
    }
}

impl FrameThrottle for TimerBasedFrameThrottle {
    fn request_throttled_redraw(&self, winit_window: &winit::window::Window) {
        if self.timer.running() {
            return;
        }
        let refresh_interval_millihertz = winit_window
            .current_monitor()
            .and_then(|monitor| monitor.refresh_rate_millihertz())
            .unwrap_or(60000) as u64;
        let window_adapter = self.window_adapter.clone();
        let timer = Rc::downgrade(&self.timer);
        let interval =
            std::time::Duration::from_millis((1000 * 1000) / refresh_interval_millihertz);
        self.timer.start(TimerMode::Repeated, interval, move || {
            redraw_now(&window_adapter);

            let Some(timer) = timer.upgrade() else { return };
            let Some(window_adapter) = window_adapter.upgrade() else { return };

            let keep_running = window_adapter.pending_redraw();

            if timer.running() {
                if !keep_running {
                    timer.stop();
                }
            } else if keep_running {
                timer.restart();
            }
        });
    }
}

fn redraw_now(window_adapter: &Weak<WinitWindowAdapter>) {
    let Some(winit_window) = window_adapter.upgrade().and_then(|adapter| adapter.winit_window())
    else {
        return;
    };
    winit_window.request_redraw();
}

std::thread_local! {
    /// Whether the last frame drawn on this thread was presented. Wayland paces redraws by
    /// frame callbacks, which only a presented frame gets.
    static LAST_FRAME_PRESENTED: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// Note whether the frame just drawn was presented (its damage was not empty).
pub(crate) fn frame_presented(presented: bool) {
    LAST_FRAME_PRESENTED.with(|p| p.set(presented));
}

/// Wayland: a redraw follows the compositor's frame callback after a presented frame. A
/// frame that drew nothing was not presented and gets no callback, so while animations
/// keep asking for frames the next one would come at once, thousands a second. After
/// such a frame the next redraw waits for the next frame interval instead.
struct PacedWinitFrameThrottle {
    window_adapter: Weak<WinitWindowAdapter>,
    timer: Rc<Timer>,
}

impl PacedWinitFrameThrottle {
    fn create(window_adapter: Weak<WinitWindowAdapter>) -> Box<dyn FrameThrottle> {
        Box::new(Self { window_adapter, timer: Rc::new(Timer::default()) })
    }
}

/// When the next redraw may come: at once after a presented frame (the frame callback
/// paces it), else after one frame interval of the display's refresh rate.
#[doc(hidden)]
pub fn redraw_delay(
    last_presented: bool,
    refresh_millihertz: Option<u32>,
) -> Option<std::time::Duration> {
    if last_presented {
        return None;
    }
    let refresh = refresh_millihertz.unwrap_or(60000).max(1) as u64;
    Some(std::time::Duration::from_micros(1_000_000_000 / refresh))
}

impl FrameThrottle for PacedWinitFrameThrottle {
    fn request_throttled_redraw(&self, winit_window: &winit::window::Window) {
        let refresh = winit_window.current_monitor().and_then(|m| m.refresh_rate_millihertz());
        let Some(interval) = redraw_delay(LAST_FRAME_PRESENTED.with(|p| p.get()), refresh) else {
            winit_window.request_redraw();
            return;
        };
        if self.timer.running() {
            return;
        }
        let window_adapter = self.window_adapter.clone();
        self.timer.start(TimerMode::SingleShot, interval, move || redraw_now(&window_adapter));
    }
}

#[allow(dead_code)]
struct WinitBasedFrameThrottle;

#[allow(dead_code)]
impl WinitBasedFrameThrottle {
    fn create() -> Box<dyn FrameThrottle> {
        Box::new(Self)
    }
}

impl FrameThrottle for WinitBasedFrameThrottle {
    fn request_throttled_redraw(&self, winit_window: &winit::window::Window) {
        winit_window.request_redraw();
    }
}

