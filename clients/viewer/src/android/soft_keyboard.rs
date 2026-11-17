// SPDX-License-Identifier: GPL-3.0-or-later
//! Android's soft keyboard over the stream, for the stream control "Toggle
//! keyboard" (#434, toward D7′): `InputMethodManager.toggleSoftInput(0, 0)`,
//! as Artemis's `Game.toggleKeyboard` toggles it.
//!
//! The keyboard types into the view that has the focus. Artemis's stream
//! view has it while it streams, and is no text editor, so Android's input
//! method sends what the keyboard types to it as key events, which Artemis
//! maps as a hardware keyboard's. In the Korri app the focus can be in
//! Slint's text view (`SlintInputView`, a text editor that Slint's Android
//! backend adds to the activity), and what the keyboard types there goes to
//! the shell. So NativeActivity's own view takes the focus while a stream
//! plays ([`take_focus`], and again before each toggle): it is no text
//! editor either, so the keys come through the activity's input queue, and
//! the stream's input maps them as it maps a hardware keyboard's (slice D4).
//! When the stream ends, the keyboard hides and the view gives the focus
//! back ([`give_back`]).
//!
//! Android 12 draws the keyboard only once the window's insets controller
//! shows the IME's leash. With any `WindowInsetsAnimation.Callback` in the
//! window it runs that animation on the UI thread (`InsetsController`,
//! `hasAnimationCallbacks`), and in this NativeActivity window the leash then
//! stays hidden: the keyboard is shown but never drawn (Slint's issue #11791;
//! `clients/android-app/proof/android-app-ime/`). Slint's helper sets such a
//! callback on its text view. [`without_insets_animations`] takes every
//! callback off the window's views: the controller then animates on its own
//! thread and shows the leash.
//!
//! Text that no key event carries (`KeyEvent.ACTION_MULTIPLE`, which Artemis
//! sends with `LiSendUtf8TextEvent`) does not reach the stream: the NDK gives
//! no characters (slice D4).
//!
//! Every call runs on Android's main thread, which owns the views.
use android_activity::AndroidApp;
use anyhow::{Result, anyhow, bail};
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JObject, JString, JValue},
};
use std::sync::mpsc;

/// `android.R.id.content`: the activity's content, where NativeActivity puts
/// its own view and Slint adds its text view.
const CONTENT: i32 = 0x0102_0002;
/// NativeActivity's own view (`NativeActivity.NativeContentView`).
const NATIVE_VIEW: &str = "android.app.NativeActivity$NativeContentView";
/// `Context.INPUT_METHOD_SERVICE`.
const INPUT_METHOD_SERVICE: &str = "input_method";

/// What the main thread found when it toggled.
enum Found {
    Toggled,
    NoView,
    NotFocused,
    NotServed,
}

/// The stream's view takes the focus from Slint's text view, on Android's
/// main thread, as Artemis's stream view holds it while it streams.
pub fn take_focus(app: &AndroidApp) {
    let vm = app.vm_as_ptr() as usize;
    let activity = match activity(app) {
        Ok(activity) => activity,
        Err(cause) => {
            crate::log::line(&format!(
                "keyboard: the stream's view has no focus: {cause:#}"
            ));
            return;
        }
    };
    app.run_on_java_main_thread(Box::new(move || {
        match on_main(vm, |env| focus_on_main(env, activity.as_obj())) {
            Ok(Some(true)) => crate::log::line("keyboard: the stream's view has the focus"),
            Ok(Some(false)) => {
                crate::log::line("keyboard: the stream's view did not take the focus")
            }
            Ok(None) => crate::log::line("keyboard: the activity has no NativeActivity view"),
            Err(cause) => crate::log::line(&format!(
                "keyboard: the stream's view has no focus: {cause:#}"
            )),
        }
    }));
}

/// Take every `WindowInsetsAnimation.Callback` off the window's decor view,
/// its content view and the content's children (NativeActivity's view and
/// Slint's text view), so that the input method's keyboard is drawn (above).
/// On Android's main thread. The count says how many views it cleared.
pub fn without_insets_animations(env: &mut JNIEnv, activity: &JObject) -> jni::errors::Result<i32> {
    const SET: &str = "setWindowInsetsAnimationCallback";
    const CALLBACK: &str = "(Landroid/view/WindowInsetsAnimation$Callback;)V";
    let none = JObject::null();
    let window = env
        .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
        .l()?;
    let decor = env
        .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
        .l()?;
    env.call_method(&decor, SET, CALLBACK, &[JValue::Object(&none)])?;
    let content = env
        .call_method(
            activity,
            "findViewById",
            "(I)Landroid/view/View;",
            &[CONTENT.into()],
        )?
        .l()?;
    if content.is_null() {
        return Ok(1);
    }
    env.call_method(&content, SET, CALLBACK, &[JValue::Object(&none)])?;
    let count = env
        .call_method(&content, "getChildCount", "()I", &[])?
        .i()?;
    for index in 0..count {
        let child = env
            .call_method(
                &content,
                "getChildAt",
                "(I)Landroid/view/View;",
                &[index.into()],
            )?
            .l()?;
        if !child.is_null() {
            env.call_method(&child, SET, CALLBACK, &[JValue::Object(&none)])?;
        }
    }
    Ok(2 + count)
}

/// Toggle the soft keyboard over the stream. The answer comes once the main
/// thread has made the call with the stream's view as the keyboard's
/// target, or says why it could not.
pub fn toggle(app: &AndroidApp) -> mpsc::Receiver<Result<()>> {
    let (sender, answer) = mpsc::channel();
    let vm = app.vm_as_ptr() as usize;
    let activity = match activity(app) {
        Ok(activity) => activity,
        Err(cause) => {
            let _ = sender.send(Err(cause));
            return answer;
        }
    };
    app.run_on_java_main_thread(Box::new(move || {
        let toggled =
            on_main(vm, |env| toggle_on_main(env, activity.as_obj())).and_then(|toggled| {
                match toggled {
                    Found::Toggled => Ok(()),
                    Found::NoView => bail!("the activity has no NativeActivity view"),
                    Found::NotFocused => bail!("the stream's view did not take the focus"),
                    Found::NotServed => {
                        bail!("the input method does not serve the stream's view")
                    }
                }
            });
        match &toggled {
            Ok(()) => crate::log::line(
                "keyboard: the soft keyboard toggled; the stream's view has the focus",
            ),
            Err(cause) => crate::log::line(&format!("keyboard: no toggle: {cause:#}")),
        }
        let _ = sender.send(toggled);
    }));
    answer
}

/// The stream has ended: the soft keyboard hides, and NativeActivity's view
/// gives the focus back, so Slint's text view can take it again.
pub fn give_back(app: &AndroidApp) {
    let vm = app.vm_as_ptr() as usize;
    let activity = match activity(app) {
        Ok(activity) => activity,
        Err(cause) => {
            crate::log::line(&format!("keyboard: the focus stays: {cause:#}"));
            return;
        }
    };
    app.run_on_java_main_thread(Box::new(move || {
        match on_main(vm, |env| give_back_on_main(env, activity.as_obj())) {
            Ok(true) => crate::log::line(
                "keyboard: the soft keyboard hides; the stream's view gives the focus back",
            ),
            Ok(false) => crate::log::line("keyboard: the activity has no NativeActivity view"),
            Err(cause) => crate::log::line(&format!("keyboard: the focus stays: {cause:#}")),
        }
    }));
}

/// Our own reference to the activity, taken on the calling thread:
/// android-activity's may be gone by the time the main thread runs.
pub(crate) fn activity(app: &AndroidApp) -> Result<GlobalRef> {
    // SAFETY: android-activity's VM, alive for the process, and its global
    // reference to the activity, valid on this thread until it handles
    // Destroy.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }?;
    let env = vm.attach_current_thread()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    Ok(env.new_global_ref(activity)?)
}

/// `work` on this thread, attached to the VM; a Java exception is logged and
/// cleared.
pub(crate) fn on_main<T>(
    vm: usize,
    work: impl FnOnce(&mut JNIEnv) -> jni::errors::Result<T>,
) -> Result<T> {
    // SAFETY: android-activity's VM, alive for the process.
    let vm = unsafe { JavaVM::from_raw(vm as *mut jni::sys::JavaVM) }?;
    let mut env = vm.attach_current_thread()?;
    let result = env.with_local_frame(16, work);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result.map_err(|error| anyhow!("{error}"))
}

/// NativeActivity's view becomes focusable and takes the focus. `None`:
/// there is no such view; `Some(false)`: it did not take the focus.
fn focus_on_main(env: &mut JNIEnv, activity: &JObject) -> jni::errors::Result<Option<bool>> {
    let Some(view) = native_view(env, activity)? else {
        return Ok(None);
    };
    env.call_method(&view, "setFocusable", "(Z)V", &[true.into()])?;
    env.call_method(&view, "setFocusableInTouchMode", "(Z)V", &[true.into()])?;
    Ok(Some(
        env.call_method(&view, "requestFocus", "()Z", &[])?.z()?,
    ))
}

/// The window's views lose their insets animation callbacks, the view takes
/// the focus, the input method serves it, then `toggleSoftInput(0, 0)`,
/// `Game.toggleKeyboard`'s call.
fn toggle_on_main(env: &mut JNIEnv, activity: &JObject) -> jni::errors::Result<Found> {
    without_insets_animations(env, activity)?;
    let Some(view) = native_view(env, activity)? else {
        return Ok(Found::NoView);
    };
    match focus_on_main(env, activity)? {
        Some(true) => {}
        Some(false) => return Ok(Found::NotFocused),
        None => return Ok(Found::NoView),
    }
    let manager = input_methods(env, activity)?;
    if manager.is_null() {
        return Ok(Found::NotServed);
    }
    // isActive checks the focus first: the manager serves the view that has
    // it now, not the one that had it before this call.
    let served = env
        .call_method(
            &manager,
            "isActive",
            "(Landroid/view/View;)Z",
            &[JValue::Object(&view)],
        )?
        .z()?;
    if !served {
        return Ok(Found::NotServed);
    }
    env.call_method(&manager, "toggleSoftInput", "(II)V", &[0.into(), 0.into()])?;
    Ok(Found::Toggled)
}

/// `hideSoftInputFromWindow`, then the view is no longer focusable and gives
/// the focus up. False when there is no such view.
fn give_back_on_main(env: &mut JNIEnv, activity: &JObject) -> jni::errors::Result<bool> {
    let Some(view) = native_view(env, activity)? else {
        return Ok(false);
    };
    let manager = input_methods(env, activity)?;
    let token = env
        .call_method(&view, "getWindowToken", "()Landroid/os/IBinder;", &[])?
        .l()?;
    if !manager.is_null() && !token.is_null() {
        env.call_method(
            &manager,
            "hideSoftInputFromWindow",
            "(Landroid/os/IBinder;I)Z",
            &[JValue::Object(&token), 0.into()],
        )?;
    }
    env.call_method(&view, "setFocusableInTouchMode", "(Z)V", &[false.into()])?;
    env.call_method(&view, "setFocusable", "(Z)V", &[false.into()])?;
    Ok(true)
}

/// NativeActivity's own view, among the activity's content.
pub(crate) fn native_view<'local>(
    env: &mut JNIEnv<'local>,
    activity: &JObject,
) -> jni::errors::Result<Option<JObject<'local>>> {
    let content = env
        .call_method(
            activity,
            "findViewById",
            "(I)Landroid/view/View;",
            &[CONTENT.into()],
        )?
        .l()?;
    if content.is_null() {
        return Ok(None);
    }
    let count = env
        .call_method(&content, "getChildCount", "()I", &[])?
        .i()?;
    for index in 0..count {
        let child = env
            .call_method(
                &content,
                "getChildAt",
                "(I)Landroid/view/View;",
                &[index.into()],
            )?
            .l()?;
        if child.is_null() {
            continue;
        }
        let class = env
            .call_method(&child, "getClass", "()Ljava/lang/Class;", &[])?
            .l()?;
        let name = JString::from(
            env.call_method(&class, "getName", "()Ljava/lang/String;", &[])?
                .l()?,
        );
        let name: String = env.get_string(&name)?.into();
        if name == NATIVE_VIEW {
            return Ok(Some(child));
        }
    }
    Ok(None)
}

/// `Context.getSystemService(INPUT_METHOD_SERVICE)`; null when there is none.
fn input_methods<'local>(
    env: &mut JNIEnv<'local>,
    activity: &JObject,
) -> jni::errors::Result<JObject<'local>> {
    let name = env.new_string(INPUT_METHOD_SERVICE)?;
    env.call_method(
        activity,
        "getSystemService",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        &[JValue::Object(&name)],
    )?
    .l()
}
