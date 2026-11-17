// SPDX-License-Identifier: GPL-3.0-or-later
//! The stream's status for its controller (#434, slice D11): the device's
//! battery and the stream's health, read as Artemis's snapshot reads them
//! (`KorriViewerControls.snapshot`, `MediaCodecDecoderRenderer.viewerHealth`).
//! A controller passes [`readings`] to `Lifecycle::controls`; it runs on the
//! thread that asks for a snapshot. [`touchscreen`] says whether the stream
//! may offer the touch controls, and [`chromeos`] whether a touch screen may
//! have the mouse's capture (`capture.rs`).
use crate::{controls::Readings, log, picture};
use jni::{
    JavaVM,
    objects::{JObject, JValue},
};
use korri_viewer_treaty::SurfaceStreamHealth;

/// `Context.BATTERY_SERVICE` and `BatteryManager.BATTERY_PROPERTY_CAPACITY`.
const BATTERY_SERVICE: &str = "batterymanager";
const BATTERY_PROPERTY_CAPACITY: i32 = 4;

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

/// The battery and the health now; each is absent when it cannot be read.
pub fn readings() -> Readings {
    let health = picture::health();
    Readings {
        battery_percent: battery(),
        sampled_at_ms: health.map(|(sample, _)| sample.at_ms as f64),
        stream_health: health.map(|(sample, rtt)| SurfaceStreamHealth {
            rtt_ms: rtt.map(f64::from),
            delivered_bitrate_kbps: Some(sample.delivered_bitrate_kbps),
            frames_dropped: Some(sample.frames_dropped as f64),
        }),
    }
}

/// `BatteryManager.getIntProperty(BATTERY_PROPERTY_CAPACITY)`, through JNI
/// on this thread.
fn battery() -> Option<f64> {
    let context = ndk_context::android_context();
    // SAFETY: the process's VM, which android-activity gave ndk-context.
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    // SAFETY: ndk-context's global reference to the activity, which it
    // keeps; this wrapper does not delete it.
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    let capacity = env.with_local_frame(8, |env| -> jni::errors::Result<i32> {
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
            return Ok(i32::MIN);
        }
        env.call_method(
            &manager,
            "getIntProperty",
            "(I)I",
            &[BATTERY_PROPERTY_CAPACITY.into()],
        )?
        .i()
    });
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    match capacity {
        Ok(capacity) => Readings::battery(capacity),
        Err(error) => {
            log::line(&format!("status: the battery is unreadable: {error}"));
            None
        }
    }
}
