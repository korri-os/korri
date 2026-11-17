// SPDX-License-Identifier: GPL-3.0-only
//! A test app for the viewer's proof (`../run.py`), never shipped. The
//! proof's host is a stock Sunshine whose controllers are off, so it sends
//! no rumble and no light colour. This app plays the core's part instead,
//! with the viewer's own code from the input onwards:
//!
//! 1. It finds the virtual DualShock 4 that the proof made, and presses A
//!    on it through the viewer's input (`Input`), as the stream's first
//!    press would: the input seats it with the feedback thread
//!    (`feedback_jni::start`), which reads its battery.
//! 2. On a thread of its own, not attached to the VM, as the core's is, it
//!    calls the very callbacks that the stream gives the core
//!    (`feedback::rumble`, `rumble_triggers`, `set_controller_led`), one step
//!    every 3 s, and logs each step with the tag `korri-feedback-probe`.
//! 3. It stops the feedback as the stream's stop does, and ends.
//!
//! No core runs, so the input's gate stays closed: the arrival and the
//! battery report go nowhere. The proof reads what reached Android from the
//! pad's own output reports and from `dumpsys input`.
#![cfg(target_os = "android")]

use android_activity::AndroidApp;
use anyhow::{Context, Result};
use korri_viewer::{
    devices::{AndroidDevices, now_ms},
    events,
    feedback::{self, STOP_WAIT},
    feedback_jni,
    gestures::GestureConfig,
    input::{Devices, Input, KeyAction, KeyEvent, Settings, core_sink, source},
    placement::Rect,
};
use std::{
    ffi::{CString, c_char, c_int},
    thread,
    time::Duration,
};

const SONY: u16 = 0x054c;
/// `KeyEvent.KEYCODE_BUTTON_A`, and Linux's `BTN_SOUTH`.
const BUTTON_A: u32 = 96;
const BTN_SOUTH: i32 = 304;
const STEP: Duration = Duration::from_secs(3);

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

fn log(text: &str) {
    let text = CString::new(text.replace('\0', "")).unwrap_or_default();
    // SAFETY: two live C strings. 4: ANDROID_LOG_INFO.
    unsafe { __android_log_write(4, c"korri-feedback-probe".as_ptr(), text.as_ptr()) };
}

/// The proof's virtual DualShock 4: Sony's, with joystick axes.
fn dualshock(devices: &mut AndroidDevices) -> Result<i32> {
    let ids = devices.ids().context("no device list")?;
    ids.into_iter()
        .find(|&id| {
            devices.describe(id).is_some_and(|device| {
                device.vendor == SONY && device.sources & source::JOYSTICK == source::JOYSTICK
            })
        })
        .context("no DualShock 4")
}

fn press(input: &mut Input, devices: &mut AndroidDevices, device: i32) {
    let mut sink = core_sink();
    for action in [KeyAction::Down, KeyAction::Up] {
        let now = now_ms();
        let event = KeyEvent {
            device,
            source: source::GAMEPAD,
            action,
            key_code: BUTTON_A,
            scan_code: BTN_SOUTH,
            event_time_ms: now,
            down_time_ms: now,
            ..Default::default()
        };
        input.key(&event, now, devices, &mut sink);
    }
}

/// The core's part: its callbacks, from a thread that is not attached.
fn core_steps() {
    let step = |name: &str, call: &dyn Fn()| {
        log(&format!("step {name}"));
        call();
        thread::sleep(STEP);
    };
    // SAFETY: the callbacks take plain values.
    step("rumble low c000 high 4000", &|| unsafe {
        feedback::rumble(0, 0xc000, 0x4000)
    });
    step("light 446688", &|| unsafe {
        feedback::set_controller_led(0, 0x44, 0x66, 0x88)
    });
    step("trigger rumble ffff ffff", &|| unsafe {
        feedback::rumble_triggers(0, 0xffff, 0xffff)
    });
    step("rumble off", &|| unsafe { feedback::rumble(0, 0, 0) });
    step("rumble low 4000 high c000", &|| unsafe {
        feedback::rumble(0, 0x4000, 0xc000)
    });
}

fn probe(gestures: GestureConfig) -> Result<()> {
    let mut devices = AndroidDevices::new()?;
    let pad = dualshock(&mut devices)?;
    log(&format!("pad {pad}"));
    let mut worker = feedback_jni::start()?;
    let mut input = Input::new(
        Settings {
            mouse_emulation: false,
            gestures,
        },
        0,
        Rect {
            left: 0,
            top: 0,
            right: 1,
            bottom: 1,
        },
        Box::new(worker.seats()),
    );
    log("step seat");
    press(&mut input, &mut devices, pad);
    thread::sleep(STEP);
    worker.serve_core();
    thread::Builder::new()
        .name("core-callbacks".into())
        .spawn(core_steps)?
        .join()
        .map_err(|_| anyhow::anyhow!("the core's steps panicked"))?;
    log("step stop");
    let stopped = worker.stop(STOP_WAIT);
    log(&format!("stopped {stopped}"));
    thread::sleep(STEP);
    Ok(())
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    log("started");
    // The probe runs beside the activity's events, which this thread
    // answers so that Android's main thread never waits for it.
    let gestures = events::gesture_config(&app);
    let probe = thread::spawn(move || probe(gestures));
    while !probe.is_finished() {
        app.poll_events(Some(Duration::from_millis(100)), |_| {});
    }
    match probe.join() {
        Ok(Ok(())) => log("ended"),
        Ok(Err(error)) => log(&format!("failed: {error:#}")),
        Err(_) => log("failed: panicked"),
    }
}
