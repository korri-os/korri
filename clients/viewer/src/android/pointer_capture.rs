// SPDX-License-Identifier: GPL-3.0-or-later
//! The mouse's pointer capture over the stream through JNI (#434, toward
//! D7′): the calls that `capture.rs` decides, made on NativeActivity's own
//! view on Android's main thread, in their order, as Artemis's capture
//! provider makes them on its stream view. That view holds the focus while
//! a stream plays (`soft_keyboard::take_focus`), and the window's events
//! reach the app's input queue first, a captured mouse's too
//! (`SOURCE_MOUSE_RELATIVE`).
//!
//! Android's own calls: `View.setPointerIcon` with
//! `PointerIcon.getSystemIcon(context, TYPE_NULL)` or none,
//! `View.requestPointerCapture`, `View.releasePointerCapture` and
//! `View.hasPointerCapture`. Android grants or ends the capture later, on
//! its own: the log line says what was asked, and whether the view had the
//! capture before.
use super::{capture::Call, soft_keyboard};
use android_activity::AndroidApp;
use jni::{
    JNIEnv,
    objects::{JObject, JValue},
};

/// `PointerIcon.TYPE_NULL`.
const TYPE_NULL: i32 = 0;

/// Make `calls` on the stream's view, without waiting for the main thread.
pub fn perform(app: &AndroidApp, calls: Vec<Call>) {
    if calls.is_empty() {
        return;
    }
    let vm = app.vm_as_ptr() as usize;
    let activity = match soft_keyboard::activity(app) {
        Ok(activity) => activity,
        Err(cause) => {
            crate::log::line(&format!("pointer: no {calls:?}: {cause:#}"));
            return;
        }
    };
    app.run_on_java_main_thread(Box::new(move || {
        match soft_keyboard::on_main(vm, |env| on_main(env, activity.as_obj(), &calls)) {
            Ok(Some(had)) => crate::log::line(&format!(
                "pointer: {calls:?} on the stream's view, which had pointer capture: {had}"
            )),
            Ok(None) => crate::log::line(&format!(
                "pointer: no {calls:?}: the activity has no NativeActivity view"
            )),
            Err(cause) => crate::log::line(&format!("pointer: {calls:?} failed: {cause:#}")),
        }
    }));
}

/// The calls on NativeActivity's view. `None`: there is no such view;
/// else whether the view had the capture before them.
fn on_main(
    env: &mut JNIEnv,
    activity: &JObject,
    calls: &[Call],
) -> jni::errors::Result<Option<bool>> {
    let Some(view) = soft_keyboard::native_view(env, activity)? else {
        return Ok(None);
    };
    let has = |env: &mut JNIEnv| -> jni::errors::Result<bool> {
        env.call_method(&view, "hasPointerCapture", "()Z", &[])?.z()
    };
    let had = has(env)?;
    for call in calls {
        match *call {
            Call::HideIcon => {
                let icon = env
                    .call_static_method(
                        "android/view/PointerIcon",
                        "getSystemIcon",
                        "(Landroid/content/Context;I)Landroid/view/PointerIcon;",
                        &[JValue::Object(activity), TYPE_NULL.into()],
                    )?
                    .l()?;
                set_icon(env, &view, &icon)?;
            }
            Call::ShowIcon => set_icon(env, &view, &JObject::null())?,
            Call::Request => capture(env, &view, true)?,
            Call::Release => capture(env, &view, false)?,
            // `onInputDeviceRemoved`, then `onInputDeviceAdded`.
            Call::Follow { compatible } => {
                if has(env)? && !compatible {
                    capture(env, &view, false)?;
                }
                if !has(env)? && compatible {
                    capture(env, &view, true)?;
                }
            }
        }
    }
    Ok(Some(had))
}

fn set_icon(env: &mut JNIEnv, view: &JObject, icon: &JObject) -> jni::errors::Result<()> {
    env.call_method(
        view,
        "setPointerIcon",
        "(Landroid/view/PointerIcon;)V",
        &[JValue::Object(icon)],
    )?;
    Ok(())
}

fn capture(env: &mut JNIEnv, view: &JObject, on: bool) -> jni::errors::Result<()> {
    let method = if on {
        "requestPointerCapture"
    } else {
        "releasePointerCapture"
    };
    env.call_method(view, method, "()V", &[])?;
    Ok(())
}
