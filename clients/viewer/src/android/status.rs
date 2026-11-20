// SPDX-License-Identifier: GPL-3.0-or-later
//! The stream's status for its controller (#434, slice D11): the device's
//! battery and the stream's health, read as Artemis's snapshot reads them
//! (`KorriViewerControls.snapshot`, `MediaCodecDecoderRenderer.viewerHealth`).
//! A controller passes [`readings`] to `Lifecycle::controls`; it runs on the
//! thread that asks for a snapshot. [`touchscreen`] says whether the stream
//! may offer the touch controls, and [`chromeos`] whether a touch screen may
//! have the mouse's capture (`capture.rs`). [`screen`] is the display's
//! physical size, the screen that a controller reports for the viewer
//! (`KorriStreamViewer.screenStreamSize`).
use crate::{controls::Readings, log, picture};
use jni::{
    JavaVM,
    objects::{JObject, JValue},
};
use korri_viewer_treaty::SurfaceStreamHealth;

/// `Context.BATTERY_SERVICE`, `BatteryManager.BATTERY_PROPERTY_CAPACITY`
/// and `BatteryManager.BATTERY_PROPERTY_STATUS` (API 26).
const BATTERY_SERVICE: &str = "batterymanager";
const BATTERY_PROPERTY_CAPACITY: i32 = 4;
const BATTERY_PROPERTY_STATUS: i32 = 6;

/// `Context.DISPLAY_SERVICE` and `Display.DEFAULT_DISPLAY`.
const DISPLAY_SERVICE: &str = "display";
const DEFAULT_DISPLAY: i32 = 0;

/// `PackageManager.FEATURE_TOUCHSCREEN`.
const FEATURE_TOUCHSCREEN: &str = "android.hardware.touchscreen";
/// The feature by which `AndroidNativePointerCaptureProvider` knows
/// ChromeOS.
const FEATURE_CHROMEOS: &str = "org.chromium.arc.device_management";

/// Whether the device has a touch screen
/// (`PackageManager.hasSystemFeature(FEATURE_TOUCHSCREEN)`), as
/// `KorriViewerControls.supported` asks: Artemis lists its touch controls
/// only then. Through JNI on this thread; false when it cannot be read.
pub fn touchscreen() -> bool {
    feature(FEATURE_TOUCHSCREEN, "the touch screen")
}

/// Whether the device runs ChromeOS, as Artemis's capture provider asks:
/// there a touch screen may have the mouse's capture. Through JNI on this
/// thread; false when it cannot be read.
pub fn chromeos() -> bool {
    feature(FEATURE_CHROMEOS, "ChromeOS")
}

/// `PackageManager.hasSystemFeature`; false when it cannot be read.
fn feature(name: &str, what: &str) -> bool {
    let context = ndk_context::android_context();
    // SAFETY: the process's VM, which android-activity gave ndk-context.
    let Ok(vm) = (unsafe { JavaVM::from_raw(context.vm().cast()) }) else {
        return false;
    };
    let Ok(mut env) = vm.attach_current_thread() else {
        return false;
    };
    // SAFETY: ndk-context's global reference to the activity, which it
    // keeps; this wrapper does not delete it.
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    let has = env.with_local_frame(8, |env| -> jni::errors::Result<bool> {
        let manager = env
            .call_method(
                &activity,
                "getPackageManager",
                "()Landroid/content/pm/PackageManager;",
                &[],
            )?
            .l()?;
        let feature = env.new_string(name)?;
        env.call_method(
            &manager,
            "hasSystemFeature",
            "(Ljava/lang/String;)Z",
            &[JValue::Object(&feature)],
        )?
        .z()
    });
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    match has {
        Ok(has) => has,
        Err(error) => {
            log::line(&format!("status: {what} is unknown: {error}"));
            false
        }
    }
}

/// The physical size of the default display in its current mode, as
/// Artemis's `KorriStreamViewer.screenStreamSize` reads it
/// (`Display.getMode().getPhysicalWidth()` and `getPhysicalHeight()`): the
/// whole panel in its natural orientation, whatever the size of the app's
/// window or a forced logical size (`wm size`, the emulator's fold). On a
/// phone with two panels Android's default display is the panel in use, so
/// this size changes with a fold: read in AOSP (`LogicalDisplayMapper`),
/// not yet seen on a phone with this code. Through JNI on this thread; None
/// when it cannot be read.
pub fn screen() -> Option<(i32, i32)> {
    let context = ndk_context::android_context();
    // SAFETY: the process's VM, which android-activity gave ndk-context.
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    // SAFETY: ndk-context's global reference to the activity, which it
    // keeps; this wrapper does not delete it.
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    let size = env.with_local_frame(8, |env| -> jni::errors::Result<Option<(i32, i32)>> {
        let name = env.new_string(DISPLAY_SERVICE)?;
        let manager = env
            .call_method(
                &activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&name)],
            )?
            .l()?;
        if manager.is_null() {
            return Ok(None);
        }
        let display = env
            .call_method(
                &manager,
                "getDisplay",
                "(I)Landroid/view/Display;",
                &[DEFAULT_DISPLAY.into()],
            )?
            .l()?;
        if display.is_null() {
            return Ok(None);
        }
        let mode = env
            .call_method(&display, "getMode", "()Landroid/view/Display$Mode;", &[])?
            .l()?;
        if mode.is_null() {
            return Ok(None);
        }
        let width = env
            .call_method(&mode, "getPhysicalWidth", "()I", &[])?
            .i()?;
        let height = env
            .call_method(&mode, "getPhysicalHeight", "()I", &[])?
            .i()?;
        Ok(Some((width, height)))
    });
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    match size {
        Ok(size) => size.filter(|(width, height)| *width > 0 && *height > 0),
        Err(error) => {
            log::line(&format!("status: the screen is unreadable: {error}"));
            None
        }
    }
}

/// The battery and the health now; each is absent when it cannot be read.
pub fn readings() -> Readings {
    let health = picture::health();
    let (battery_percent, battery_charging) = battery();
    Readings {
        battery_percent,
        battery_charging,
        sampled_at_ms: health.map(|(sample, _)| sample.at_ms as f64),
        stream_health: health.map(|(sample, rtt)| SurfaceStreamHealth {
            rtt_ms: rtt.map(f64::from),
            delivered_bitrate_kbps: Some(sample.delivered_bitrate_kbps),
            frames_dropped: Some(sample.frames_dropped as f64),
        }),
    }
}

/// `BatteryManager.getIntProperty` of `BATTERY_PROPERTY_CAPACITY` and of
/// `BATTERY_PROPERTY_STATUS`, through JNI on this thread: the percent and
/// whether the battery is charging (ADR 0094). Each is absent when it
/// cannot be read.
fn battery() -> (Option<f64>, Option<bool>) {
    let context = ndk_context::android_context();
    // SAFETY: the process's VM, which android-activity gave ndk-context.
    let Ok(vm) = (unsafe { JavaVM::from_raw(context.vm().cast()) }) else {
        return (None, None);
    };
    let Ok(mut env) = vm.attach_current_thread() else {
        return (None, None);
    };
    // SAFETY: ndk-context's global reference to the activity, which it
    // keeps; this wrapper does not delete it.
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    let read = env.with_local_frame(8, |env| -> jni::errors::Result<(i32, i32)> {
        let name = env.new_string(BATTERY_SERVICE)?;
        let manager = env
            .call_method(
                &activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&name)],
            )?
            .l()?;
        if manager.is_null() {
            return Ok((i32::MIN, i32::MIN));
        }
        let mut property = |id: i32| -> jni::errors::Result<i32> {
            env.call_method(&manager, "getIntProperty", "(I)I", &[id.into()])?
                .i()
        };
        Ok((
            property(BATTERY_PROPERTY_CAPACITY)?,
            property(BATTERY_PROPERTY_STATUS)?,
        ))
    });
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    match read {
        Ok((capacity, status)) => (Readings::battery(capacity), Readings::charging(status)),
        Err(error) => {
            log::line(&format!("status: the battery is unreadable: {error}"));
            (None, None)
        }
    }
}
