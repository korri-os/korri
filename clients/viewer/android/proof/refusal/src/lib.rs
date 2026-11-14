// SPDX-License-Identifier: GPL-3.0-or-later
//! A test app for the viewer's proof (`../run.py`), never shipped. When
//! Android creates its activity, it asks Android to start the viewer's
//! activity with `Activity.startActivity`, as any app could, and writes the
//! answer to logcat with the tag `korri-refusal`:
//!
//! - `refused: <the exception>`: Android refused the start;
//! - `started`: Android accepted it.
//!
//! Then it finishes. It sends no launch, so a viewer that does start ends at
//! once for want of one.
#![cfg(target_os = "android")]

use jni::{
    JNIEnv,
    objects::{JObject, JValue},
};
use std::ffi::{CString, c_char, c_int, c_void};

const VIEWER: (&str, &str) = (
    "com.simonwjackson.korri.viewer",
    "android.app.NativeActivity",
);

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

fn log(text: &str) {
    let text = CString::new(text.replace('\0', "")).unwrap_or_default();
    // SAFETY: two live C strings. 4: ANDROID_LOG_INFO.
    unsafe { __android_log_write(4, c"korri-refusal".as_ptr(), text.as_ptr()) };
}

fn start_viewer(env: &mut JNIEnv, activity: &JObject) -> jni::errors::Result<()> {
    let intent = env.new_object("android/content/Intent", "()V", &[])?;
    let package = env.new_string(VIEWER.0)?;
    let class = env.new_string(VIEWER.1)?;
    env.call_method(
        &intent,
        "setClassName",
        "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&package), JValue::Object(&class)],
    )?;
    env.call_method(
        activity,
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[JValue::Object(&intent)],
    )?;
    Ok(())
}

fn answer(env: &mut JNIEnv, activity: &JObject) -> String {
    match start_viewer(env, activity) {
        Ok(()) => "started".into(),
        Err(jni::errors::Error::JavaException) => {
            let thrown = env.exception_occurred();
            let _ = env.exception_clear();
            let text = thrown
                .and_then(|thrown| {
                    env.call_method(&thrown, "toString", "()Ljava/lang/String;", &[])
                })
                .and_then(|text| text.l())
                .and_then(|text| env.get_string(&text.into()).map(String::from));
            format!(
                "refused: {}",
                text.unwrap_or_else(|error| format!("an exception ({error})"))
            )
        }
        Err(error) => format!("failed: {error}"),
    }
}

/// NativeActivity calls this on Android's main thread, with that thread's
/// JNI environment.
///
/// # Safety
/// `activity` is the live `ANativeActivity` that NativeActivity passes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ANativeActivity_onCreate(
    activity: *mut ndk_sys::ANativeActivity,
    _: *mut c_void,
    _: usize,
) {
    // SAFETY: NativeActivity's own structure, its env and its activity object.
    let (mut env, object) = unsafe {
        (
            JNIEnv::from_raw((*activity).env.cast())
                .expect("NativeActivity gives a JNI environment"),
            JObject::from_raw((*activity).clazz.cast()),
        )
    };
    log(&answer(&mut env, &object));
    // SAFETY: the same live activity.
    unsafe { ndk_sys::ANativeActivity_finish(activity) };
}
