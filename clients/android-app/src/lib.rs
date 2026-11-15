// SPDX-License-Identifier: GPL-3.0-or-later
//! The Korri Android app with no Korri Java (#434, slice D8). Android's own
//! `android.app.NativeActivity` loads this one library. Its `android_main`
//! starts korrid in this process (`korrid::android_app`), then runs the
//! native shell (`korri_shell::backend::android::run`) on the activity's
//! window. Rust gives the shell korrid's port and capability; neither
//! crosses JNI, a URL, a file or the log.
//!
//! The steps of `KorriShellActivity.onCreate` and its callbacks, here:
//!
//! | Artemis (Java) | Here |
//! | --- | --- |
//! | `KorriBrainService.ensureRunning` | `korrid::android_app::start`, once per process |
//! | `copyPackage` for Velvet, Pico and Zone | `assets::unpack` |
//! | `shellPort`, `shellCapability`, `shellOrigin`, `shellPackagePath` | `Bootstrap`, in Rust |
//! | `InputDeviceListener` | `host.rs`: `InputDevice.getDeviceIds()` every 100 ms |
//! | `applyFullscreen` on create, resume and focus | `host.rs`, on Android's main thread |
//! | `onBackPressed` swallowed | the shell's input filter consumes Back (`input/android.rs`) |
//! | `shellRendered` (the device test's readiness) | a logcat line (`host.rs`) |
//! | `KorriSurfaceHost` (the overlay Surface) | none until slice D5 |
#![cfg(target_os = "android")]

mod assets;
mod host;
mod logcat;

use anyhow::{anyhow, Context, Result};
use jni::{objects::JObject, JavaVM};
use korri_shell::{
    backend::android::{run as run_shell, Bootstrap},
    config::Connection,
    korrid::Korrid,
};
use slint::android::AndroidApp;

/// The product's relay list (`clients/android/product.nix`), as the Artemis
/// app's `BuildConfig.PRODUCT_RELAYS`. The build sets the path.
const PRODUCT_RELAYS: &str = include_str!(env!("KORRI_ANDROID_RELAYS_FILE"));

/// The packages the app carries; the first is the one it opens (#424).
const SURFACES: [&str; 3] = ["velvet", "pico", "zone"];

#[no_mangle]
fn android_main(app: AndroidApp) {
    logcat::capture();
    match run(app) {
        Ok(()) => logcat::info("korri-app: the activity ended"),
        Err(error) => logcat::error(&format!("korri-app: startup failed: {error:#}")),
    }
}

fn run(app: AndroidApp) -> Result<()> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    // SAFETY: android-activity's global reference to the activity, valid on
    // this thread until it handles Destroy.
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    // korrid's errors carry no secret; JNI's are never formatted.
    let port = korrid::android_app::start(&mut env, &activity, PRODUCT_RELAYS)
        .map_err(|error| anyhow!("korrid did not start: {error}"))?;
    let capability = korrid::local_server_capability().context("korrid has no capability")?;
    let connection = Connection::new(port, format!("http://127.0.0.1:{port}"), capability);
    logcat::info(&format!("korri-app: korrid serves on 127.0.0.1:{port}"));
    // The shell's own authenticated liveness call: korrid answers its RPC.
    if Korrid::new(connection.clone()).health() {
        logcat::info("korri-app: korrid answered system.health");
    } else {
        logcat::error("korri-app: korrid did not answer system.health");
    }
    let packages = assets::unpack(&app, &mut env, &activity, &SURFACES)?;
    let host = host::AppHost::new(&app, &mut env, &activity)?;
    drop(env);
    run_shell(
        app,
        Bootstrap {
            connection,
            packages,
        },
        Box::new(host),
    )
}
