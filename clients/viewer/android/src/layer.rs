// SPDX-License-Identifier: GPL-3.0-or-later
//! The video layer: a `SurfaceControl` made through JNI calls into Android's
//! own classes (API 29 and 31), as the D5 spike found
//! (`docs/research/2026-11-14-android-overlay-spike.md`, design C). The NDK
//! cannot make a child of the NativeActivity window: on API 31 and 34
//! `ASurfaceControl_createFromWindow` returns NULL for it.
//!
//! ```text
//! window.getDecorView().getRootSurfaceControl()          View, API 31
//! new SurfaceControl.Builder().setName().setBufferSize().build()   API 29
//! root.buildReparentTransaction(layer)                   API 31
//!     .setLayer().setVisibility().setGeometry().apply()  API 29
//! new Surface(layer) → ANativeWindow_fromSurface         API 29, NDK
//! ```
//!
//! The decoder renders into the layer's Surface; the app runs no code for a
//! frame. One `setGeometry` places the video, letterboxed in the window.
use android_activity::AndroidApp;
use anyhow::{Context, Result, anyhow};
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JObject, JValue},
};
use korri_viewer::placement::{Rect, letterbox};
use ndk::{hardware_buffer_format::HardwareBufferFormat, native_window::NativeWindow};
use std::{ptr::NonNull, sync::mpsc, time::Duration};

pub struct VideoLayer {
    surface: NativeWindow,
    /// The `SurfaceControl` and its `Surface`. A SurfaceControl's finalizer
    /// would remove the layer; the app releases both itself.
    layer: GlobalRef,
    java_surface: GlobalRef,
    pub placed: Rect,
}

impl VideoLayer {
    /// The layer for a stream of `stream` pixels, placed in `window`. Waits
    /// for Android's main thread, so never call it inside an event callback
    /// of android-activity: the main thread waits there for this thread.
    pub fn make(app: &AndroidApp, window: &NativeWindow, stream: (i32, i32)) -> Result<Self> {
        let placed = letterbox(stream, (window.width(), window.height()));
        let (sender, receiver) = mpsc::channel();
        let vm = app.vm_as_ptr() as usize;
        let activity = app.activity_as_ptr() as usize;
        app.run_on_java_main_thread(Box::new(move || {
            let made = on_main_thread(vm, |env| {
                // SAFETY: android-activity's global reference to the
                // activity, borrowed for this call.
                let activity = unsafe { JObject::from_raw(activity as jni::sys::jobject) };
                make(env, &activity, stream, placed)
            });
            let _ = sender.send(made);
        }));
        let (surface, layer, java_surface) = receiver
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| anyhow!("Android's main thread did not make the video layer in 5 s"))??;
        Ok(Self {
            surface,
            layer,
            java_surface,
            placed,
        })
    }

    /// The Surface that the decoder renders into.
    pub fn surface(&self) -> NativeWindow {
        self.surface.clone()
    }

    /// Remove the layer, on Android's main thread, without waiting: the main
    /// thread may wait for this one (in `onDestroy`).
    pub fn release(self, app: &AndroidApp) {
        let vm = app.vm_as_ptr() as usize;
        let Self {
            surface,
            layer,
            java_surface,
            ..
        } = self;
        drop(surface);
        app.run_on_java_main_thread(Box::new(move || {
            let _ = on_main_thread(vm, |env| {
                env.call_method(&java_surface, "release", "()V", &[])?;
                env.call_method(&layer, "release", "()V", &[])?;
                Ok(())
            });
        }));
    }
}

type Made = (NativeWindow, GlobalRef, GlobalRef);

fn on_main_thread<T>(
    vm: usize,
    work: impl FnOnce(&mut JNIEnv) -> jni::errors::Result<T>,
) -> Result<T> {
    // SAFETY: android-activity's VM, alive for the process.
    let vm = unsafe { JavaVM::from_raw(vm as *mut jni::sys::JavaVM) }?;
    let mut env = vm.attach_current_thread()?;
    let result = env.with_local_frame(32, work);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result.map_err(|error| anyhow!("{error}"))
}

fn rect<'a>(env: &mut JNIEnv<'a>, r: Rect) -> jni::errors::Result<JObject<'a>> {
    env.new_object(
        "android/graphics/Rect",
        "(IIII)V",
        &[r.left.into(), r.top.into(), r.right.into(), r.bottom.into()],
    )
}

fn make(
    env: &mut JNIEnv,
    activity: &JObject,
    stream: (i32, i32),
    placed: Rect,
) -> jni::errors::Result<Made> {
    const TRANSACTION: &str = "Landroid/view/SurfaceControl$Transaction;";
    let window = env
        .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
        .l()?;
    let decor = env
        .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
        .l()?;
    let root = env
        .call_method(
            &decor,
            "getRootSurfaceControl",
            "()Landroid/view/AttachedSurfaceControl;",
            &[],
        )?
        .l()?;
    if root.is_null() {
        return Err(jni::errors::Error::NullPtr("getRootSurfaceControl"));
    }
    let builder = env.new_object("android/view/SurfaceControl$Builder", "()V", &[])?;
    let name = env.new_string("korri-viewer-video")?;
    env.call_method(
        &builder,
        "setName",
        "(Ljava/lang/String;)Landroid/view/SurfaceControl$Builder;",
        &[JValue::Object(&name)],
    )?;
    env.call_method(
        &builder,
        "setBufferSize",
        "(II)Landroid/view/SurfaceControl$Builder;",
        &[stream.0.into(), stream.1.into()],
    )?;
    let layer = env
        .call_method(&builder, "build", "()Landroid/view/SurfaceControl;", &[])?
        .l()?;
    let transaction = env
        .call_method(
            &root,
            "buildReparentTransaction",
            format!("(Landroid/view/SurfaceControl;){TRANSACTION}"),
            &[JValue::Object(&layer)],
        )?
        .l()?;
    if transaction.is_null() {
        // The window's own surface is not valid (yet).
        return Err(jni::errors::Error::NullPtr("buildReparentTransaction"));
    }
    // Above the window's own buffer, which shows in the bars.
    env.call_method(
        &transaction,
        "setLayer",
        format!("(Landroid/view/SurfaceControl;I){TRANSACTION}"),
        &[JValue::Object(&layer), 0.into()],
    )?;
    env.call_method(
        &transaction,
        "setVisibility",
        format!("(Landroid/view/SurfaceControl;Z){TRANSACTION}"),
        &[JValue::Object(&layer), true.into()],
    )?;
    // The placement: the stream's whole picture into its letterbox, with no
    // rotation. API 29; deprecated at 33 for setPosition, setScale and setCrop.
    let source = rect(
        env,
        Rect {
            left: 0,
            top: 0,
            right: stream.0,
            bottom: stream.1,
        },
    )?;
    let destination = rect(env, placed)?;
    env.call_method(
        &transaction,
        "setGeometry",
        format!("(Landroid/view/SurfaceControl;Landroid/graphics/Rect;Landroid/graphics/Rect;I){TRANSACTION}"),
        &[
            JValue::Object(&layer),
            JValue::Object(&source),
            JValue::Object(&destination),
            0.into(),
        ],
    )?;
    env.call_method(&transaction, "apply", "()V", &[])?;
    let java_surface = env.new_object(
        "android/view/Surface",
        "(Landroid/view/SurfaceControl;)V",
        &[JValue::Object(&layer)],
    )?;
    // SAFETY: a live JNI environment and Surface; the call acquires its own
    // reference to the native window, which NativeWindow releases.
    let surface = unsafe {
        ndk::native_window::NativeWindow::from_surface(env.get_raw(), java_surface.as_raw())
    }
    .ok_or(jni::errors::Error::NullPtr("ANativeWindow_fromSurface"))?;
    Ok((
        surface,
        env.new_global_ref(&layer)?,
        env.new_global_ref(&java_surface)?,
    ))
}

/// One CPU fill of the app's own window, black: it shows in the letterbox
/// bars, under the video layer. Once, not for each frame.
pub fn fill_black(window: &NativeWindow) -> Result<()> {
    window
        .set_buffers_geometry(0, 0, Some(HardwareBufferFormat::R8G8B8A8_UNORM))
        .context("cannot set the window's format")?;
    let mut buffer = window.lock(None).context("cannot lock the window")?;
    let (stride, height) = (buffer.stride(), buffer.height());
    let pixels = NonNull::new(buffer.bits().cast::<u32>()).context("the window has no pixels")?;
    // SAFETY: the locked buffer holds stride × height pixels of 4 bytes.
    unsafe { std::slice::from_raw_parts_mut(pixels.as_ptr(), stride * height) }.fill(0xff00_0000);
    Ok(())
}
