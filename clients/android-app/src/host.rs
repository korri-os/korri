// SPDX-License-Identifier: GPL-3.0-or-later
//! The activity around the shell, in Rust: what `KorriShellActivity` did for
//! it besides starting korrid and unpacking the packages.
use crate::logcat;
use anyhow::Result;
use jni::{
    objects::{GlobalRef, JIntArray, JObject, JValue},
    JNIEnv, JavaVM,
};
use korri_shell::{
    backend::android::{Host, Rendered},
    input::android::DevicePresence,
};
use slint::android::AndroidApp;

// android.view.WindowManager.LayoutParams and WindowInsetsController values.
const FLAG_KEEP_SCREEN_ON: i32 = 0x80;
const LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS: i32 = 3;
const BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE: i32 = 2;

pub struct AppHost {
    app: AndroidApp,
    vm: JavaVM,
    /// The app's own reference: android-activity's can go before a post to
    /// the main thread runs.
    activity: GlobalRef,
    presence: DevicePresence,
    last: Option<Rendered>,
}

impl AppHost {
    pub fn new(app: &AndroidApp, env: &mut JNIEnv, activity: &JObject) -> Result<Self> {
        Ok(Self {
            app: app.clone(),
            vm: env.get_java_vm()?,
            activity: env.new_global_ref(activity)?,
            presence: DevicePresence::default(),
            last: None,
        })
    }
}

impl Host for AppHost {
    /// Android tells no native code that a device went (that needs
    /// `InputDeviceListener`, a Java interface). Ask which are there.
    fn removed_input_devices(&mut self) -> Vec<i32> {
        let Ok(mut env) = self.vm.attach_current_thread() else {
            return Vec::new();
        };
        let ids = env.with_local_frame(4, |env| -> jni::errors::Result<Vec<i32>> {
            let ids = env
                .call_static_method("android/view/InputDevice", "getDeviceIds", "()[I", &[])?
                .l()?;
            let ids = JIntArray::from(ids);
            let mut flat = vec![0; env.get_array_length(&ids)? as usize];
            env.get_int_array_region(&ids, 0, &mut flat)?;
            Ok(flat)
        });
        match ids {
            Ok(ids) => self.presence.removed(&ids),
            Err(_) => {
                let _ = env.exception_clear();
                Vec::new()
            }
        }
    }

    /// The readiness line that replaces `shellRendered`: the first frame
    /// after each resume, then once for each change of the front surface,
    /// its page, its library count or its cursor.
    fn rendered(&mut self, rendered: &Rendered) {
        if self.last.as_ref() == Some(rendered) {
            return;
        }
        logcat::info(&format!(
            "korri-app: shell rendered surface={} page={} count={} selected={}",
            rendered.surface, rendered.page, rendered.count, rendered.selected
        ));
        self.last = Some(rendered.clone());
    }

    /// `KorriShellActivity.applyFullscreen`: edge to edge into the cutout,
    /// the system bars hidden until a swipe, and the screen kept on. Window
    /// calls belong on Android's main thread; this does not wait for it.
    fn shown(&mut self) {
        self.last = None;
        let vm = self.app.vm_as_ptr() as usize;
        let activity = self.activity.clone();
        self.app.run_on_java_main_thread(Box::new(move || {
            // SAFETY: the process's one Java VM, which outlives the app.
            let Ok(vm) = (unsafe { JavaVM::from_raw(vm as *mut jni::sys::JavaVM) }) else {
                return;
            };
            let Ok(mut env) = vm.attach_current_thread() else {
                return;
            };
            match env.with_local_frame(8, |env| fullscreen(env, activity.as_obj())) {
                Ok(hidden) => logcat::info(&format!(
                    "korri-app: window flags applied; system bars hidden: {hidden}"
                )),
                Err(_) => {
                    let _ = env.exception_clear();
                    logcat::error("korri-app: the window flags were not applied");
                }
            }
        }));
    }
}

/// True when the insets controller took the request to hide the bars.
fn fullscreen(env: &mut JNIEnv, activity: &JObject) -> jni::errors::Result<bool> {
    let window = env
        .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
        .l()?;
    env.call_method(
        &window,
        "setDecorFitsSystemWindows",
        "(Z)V",
        &[JValue::Bool(0)],
    )?;
    let attributes = env
        .call_method(
            &window,
            "getAttributes",
            "()Landroid/view/WindowManager$LayoutParams;",
            &[],
        )?
        .l()?;
    env.set_field(
        &attributes,
        "layoutInDisplayCutoutMode",
        "I",
        JValue::Int(LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS),
    )?;
    env.call_method(
        &window,
        "setAttributes",
        "(Landroid/view/WindowManager$LayoutParams;)V",
        &[JValue::Object(&attributes)],
    )?;
    env.call_method(
        &window,
        "addFlags",
        "(I)V",
        &[JValue::Int(FLAG_KEEP_SCREEN_ON)],
    )?;
    let insets = env
        .call_method(
            &window,
            "getInsetsController",
            "()Landroid/view/WindowInsetsController;",
            &[],
        )?
        .l()?;
    if insets.is_null() {
        return Ok(false);
    }
    let bars = env
        .call_static_method("android/view/WindowInsets$Type", "systemBars", "()I", &[])?
        .i()?;
    env.call_method(&insets, "hide", "(I)V", &[JValue::Int(bars)])?;
    env.call_method(
        &insets,
        "setSystemBarsBehavior",
        "(I)V",
        &[JValue::Int(BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE)],
    )?;
    Ok(true)
}
