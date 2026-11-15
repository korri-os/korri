// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's layers: the video and, above it, the gameplay overlay. Each is
//! a `SurfaceControl` made through JNI calls into Android's own classes (API
//! 29 and 31), as the D5 spike found
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
//! The decoder renders into the video layer's Surface; the app runs no code
//! for a frame. One `setGeometry` places the video, letterboxed in the
//! window, or filling it with its edges cut ("Crop to fill", slice D11:
//! the source crop is the stream's part that shows). When the decoded
//! picture's size changes in the middle of the stream (#476), one
//! transaction gives the video layer's buffer queue that size
//! (`setBufferSize`) and places the new shape (`setGeometry`).
//!
//! The overlay layer (slice D5) is translucent, covers the window and starts
//! hidden. Its buffers follow the window's size (`Transaction.setBufferSize`),
//! so whatever draws into its Surface draws at the window's size. Showing
//! and hiding it is one `setVisibility`; a hidden layer is not composed.
//! The shell's overlay renderer takes such a Surface
//! (`clients/shell/vendor/i-slint-backend-android-activity/surface_window.rs`).
//! Until slice D5 brings it in, a stand-in is drawn into it once with the
//! CPU, and again when the window's size changes. The stream's size does not
//! change it.
//!
//! Two apps make these layers: the viewer's own (`clients/viewer/android`),
//! which waits for them in its loop ([`Layers::make`]), and the Korri app
//! (`clients/android-app`), whose loop is Slint's: it asks for them
//! ([`Layers::request`]) and reads the answer at each tick
//! ([`Pending::poll`]), so its thread never waits for Android's main thread.
use crate::placement::{Fill, Rect, geometry, letterbox, place};
use android_activity::{AndroidApp, MainEvent, PollEvent};
use anyhow::{Context, Result, anyhow};
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JObject, JValue},
};
use ndk::{hardware_buffer_format::HardwareBufferFormat, native_window::NativeWindow};
use std::{
    ptr::NonNull,
    sync::mpsc::{self, RecvTimeoutError},
    time::{Duration, Instant},
};

/// How long Android's main thread may take to make the layers.
const MAKE_DEADLINE: Duration = Duration::from_secs(5);

/// `android.graphics.PixelFormat.TRANSLUCENT`.
const TRANSLUCENT: i32 = -3;

/// One layer, its Surface and the native window of that Surface. A
/// SurfaceControl's finalizer would remove the layer; the app releases both
/// itself.
struct Layer {
    surface: NativeWindow,
    control: GlobalRef,
    java_surface: GlobalRef,
}

pub struct Layers {
    video: Layer,
    /// The gameplay overlay's layer; none when Android did not make it.
    /// The stream plays without an overlay then.
    overlay: Option<Layer>,
    pub stream: (i32, i32),
    /// The window's size that the placement follows.
    pub window: (i32, i32),
    /// Where the whole picture goes, as the newest transaction places it;
    /// when it fills the window, it reaches past it on two sides.
    pub placed: Rect,
    /// "Crop to fill": applied by Android's main thread, or on its way.
    fill: Fill,
}

/// What Android's main thread sends back: the video layer, and the overlay
/// layer or why there is none.
type Made = Result<(Layer, Result<Layer>)>;

/// Layers that Android's main thread is making. Dropping it before they are
/// made is safe: the main thread then removes what it made.
pub struct Pending {
    receiver: mpsc::Receiver<Made>,
    until: Instant,
    stream: (i32, i32),
    window: (i32, i32),
    placed: Rect,
}

impl Pending {
    /// The layers once the main thread has made them; `None` while it works.
    /// An error when it failed, dropped them or took longer than 5 s.
    pub fn poll(&mut self) -> Option<Result<Layers>> {
        self.wait(Duration::ZERO)
    }

    /// As [`Pending::poll`], waiting at most `timeout` for the answer.
    fn wait(&mut self, timeout: Duration) -> Option<Result<Layers>> {
        let made = match self.receiver.recv_timeout(timeout) {
            Ok(made) => made,
            Err(RecvTimeoutError::Disconnected) => {
                return Some(Err(anyhow!("Android's main thread dropped the layers")));
            }
            Err(RecvTimeoutError::Timeout) if Instant::now() >= self.until => {
                return Some(Err(anyhow!(
                    "Android's main thread did not make the layers in 5 s"
                )));
            }
            Err(RecvTimeoutError::Timeout) => return None,
        };
        Some(made.map(|(video, overlay)| {
            let overlay = match overlay {
                Ok(overlay) => Some(overlay),
                Err(error) => {
                    crate::log::line(&format!(
                        "overlay: no layer; the stream plays without an overlay: {error:#}"
                    ));
                    None
                }
            };
            Layers {
                video,
                overlay,
                stream: self.stream,
                window: self.window,
                placed: self.placed,
                fill: Fill::default(),
            }
        }))
    }
}

impl Layers {
    /// Ask Android's main thread for the layers of a stream of `stream`
    /// pixels, placed in a window of `window` pixels. It returns at once;
    /// [`Pending::poll`] gives the layers. The window must exist: call it
    /// outside android-activity's event callback, after `InitWindow`.
    pub fn request(app: &AndroidApp, window: (i32, i32), stream: (i32, i32)) -> Result<Pending> {
        let placed = letterbox(stream, window);
        let vm = app.vm_as_ptr() as usize;
        // Our own reference to the activity, taken here: android-activity's
        // may be gone by the time the main thread runs the closure.
        let activity = attached(vm, |env| {
            // SAFETY: android-activity's global reference to the activity,
            // valid on this thread until it handles Destroy.
            let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
            env.new_global_ref(activity)
        })?;
        let (sender, receiver) = mpsc::channel();
        app.run_on_java_main_thread(Box::new(move || {
            let made = attached(vm, |env| {
                make_video(env, activity.as_obj(), stream, placed, window)
            })
            .map(|video| {
                let overlay = attached(vm, |env| make_overlay(env, activity.as_obj(), window))
                    .and_then(|overlay| match draw_stand_in(&overlay.surface) {
                        Ok(()) => Ok(overlay),
                        Err(error) => {
                            remove(vm, overlay);
                            Err(error)
                        }
                    });
                (video, overlay)
            });
            // The asking thread stopped waiting: remove what it will never use.
            if let Err(mpsc::SendError(Ok((video, overlay)))) = sender.send(made) {
                remove(vm, video);
                if let Ok(overlay) = overlay {
                    remove(vm, overlay);
                }
            }
        }));
        Ok(Pending {
            receiver,
            until: Instant::now() + MAKE_DEADLINE,
            stream,
            window,
            placed,
        })
    }

    /// The layers for a stream of `stream` pixels, placed in `window`; none
    /// when the app leaves (its window or the activity goes) first. They are
    /// made on Android's main thread. While this thread waits for them, it
    /// handles android-activity's events, so that the main thread, which
    /// waits for this one in `onDestroy`, is not held. Never call it inside
    /// an event callback of android-activity, and only from a thread that
    /// polls `AndroidApp` itself.
    pub fn make(
        app: &AndroidApp,
        window: &NativeWindow,
        stream: (i32, i32),
    ) -> Result<Option<Self>> {
        let mut pending = Self::request(app, (window.width(), window.height()), stream)?;
        loop {
            if let Some(made) = pending.wait(Duration::from_millis(10)) {
                return made.map(Some);
            }
            let mut left = false;
            app.poll_events(Some(Duration::ZERO), |event| {
                if let PollEvent::Main(MainEvent::Destroy | MainEvent::TerminateWindow { .. }) =
                    event
                {
                    left = true;
                }
            });
            if left {
                return Ok(None);
            }
        }
    }

    /// Place the video again when the window's size has changed: the turn
    /// to landscape can come after Android gives the window (it was 1x1 in
    /// one emulator run), and a fold changes it too. One `setGeometry`, and
    /// the overlay's buffers take the window's size; then the stand-in is
    /// drawn again at that size. Posted to the main thread without waiting.
    /// False when the size is the same.
    pub fn follow(&mut self, app: &AndroidApp, window: &NativeWindow) -> bool {
        let size = (window.width(), window.height());
        if size == self.window {
            return false;
        }
        self.window = size;
        self.placed = place(self.stream, size, self.fill.placing());
        let vm = app.vm_as_ptr() as usize;
        let (video, stream, placed) = (self.video.control.clone(), self.stream, self.placed);
        let overlay = self
            .overlay
            .as_ref()
            .map(|overlay| (overlay.control.clone(), overlay.surface.clone()));
        app.run_on_java_main_thread(Box::new(move || {
            let resized = attached(vm, |env| {
                let transaction =
                    env.new_object("android/view/SurfaceControl$Transaction", "()V", &[])?;
                set_geometry(env, &transaction, video.as_obj(), stream, placed, size)?;
                if let Some((overlay, _)) = &overlay {
                    env.call_method(
                        &transaction,
                        "setBufferSize",
                        "(Landroid/view/SurfaceControl;II)Landroid/view/SurfaceControl$Transaction;",
                        &[JValue::Object(overlay.as_obj()), size.0.into(), size.1.into()],
                    )?;
                }
                env.call_method(&transaction, "apply", "()V", &[])?;
                Ok(())
            });
            // `apply` gave the overlay's Surface its new default size.
            if let (Ok(()), Some((_, surface))) = (resized, overlay)
                && let Err(error) = draw_stand_in(&surface)
            {
                crate::log::line(&format!(
                    "overlay: the stand-in was not drawn again: {error:#}"
                ));
            }
        }));
        true
    }

    /// Place the video for a new size of the decoded picture: the sender
    /// changed the stream's size. One transaction, posted to the main thread
    /// without waiting: `setBufferSize`, so that the video layer's buffer
    /// queue scales the decoder's buffers to the new size and not to the old
    /// one, and `setGeometry` with the new shape's letterbox. The overlay
    /// covers the window and keeps its place and buffers. False when the
    /// size is the same.
    pub fn restream(&mut self, app: &AndroidApp, stream: (i32, i32)) -> bool {
        if stream == self.stream || stream.0 <= 0 || stream.1 <= 0 {
            return false;
        }
        self.stream = stream;
        self.placed = place(stream, self.window, self.fill.placing());
        let vm = app.vm_as_ptr() as usize;
        let (video, placed, window) = (self.video.control.clone(), self.placed, self.window);
        app.run_on_java_main_thread(Box::new(move || {
            let _ =
                attached(vm, |env| {
                    let transaction =
                        env.new_object("android/view/SurfaceControl$Transaction", "()V", &[])?;
                    env.call_method(
                    &transaction,
                    "setBufferSize",
                    "(Landroid/view/SurfaceControl;II)Landroid/view/SurfaceControl$Transaction;",
                    &[JValue::Object(video.as_obj()), stream.0.into(), stream.1.into()],
                )?;
                    set_geometry(env, &transaction, video.as_obj(), stream, placed, window)?;
                    env.call_method(&transaction, "apply", "()V", &[])?;
                    Ok(())
                });
        }));
        true
    }

    /// Fill the window with the picture, its edges cut, or fit it with bars
    /// again ("Crop to fill", Artemis's `Game.toggleFillMode`): one
    /// `setGeometry`, posted to the main thread without waiting. The answer
    /// comes once the main thread has applied it, or could not; give it to
    /// `fill_answered`. Only then does the fill count (`fill`). None when the
    /// placement does not change: the fill counts at once.
    pub fn set_fill(&mut self, app: &AndroidApp, fill: bool) -> Option<mpsc::Receiver<Result<()>>> {
        self.fill.ask(fill);
        let placed = place(self.stream, self.window, fill);
        if placed == self.placed {
            self.fill.answered(true);
            return None;
        }
        Some(self.place_video(app, placed))
    }

    /// The main thread's answer to `set_fill`. A fill that it did not apply
    /// is undone: the video is placed again for the fill that counts, posted
    /// without waiting.
    pub fn fill_answered(&mut self, app: &AndroidApp, applied: bool) {
        let fill = self.fill.answered(applied);
        let placed = place(self.stream, self.window, fill);
        if placed != self.placed {
            let _ = self.place_video(app, placed);
        }
    }

    /// The fill that Android's main thread has applied.
    pub fn fill(&self) -> bool {
        self.fill.applied()
    }

    /// One `setGeometry` of the video to `placed`, posted to the main
    /// thread; the answer says whether it applied it.
    fn place_video(&mut self, app: &AndroidApp, placed: Rect) -> mpsc::Receiver<Result<()>> {
        self.placed = placed;
        let vm = app.vm_as_ptr() as usize;
        let (video, stream, window) = (self.video.control.clone(), self.stream, self.window);
        let (sender, answer) = mpsc::channel();
        app.run_on_java_main_thread(Box::new(move || {
            let applied = attached(vm, |env| {
                let transaction =
                    env.new_object("android/view/SurfaceControl$Transaction", "()V", &[])?;
                set_geometry(env, &transaction, video.as_obj(), stream, placed, window)?;
                env.call_method(&transaction, "apply", "()V", &[])?;
                Ok(())
            });
            let _ = sender.send(applied);
        }));
        answer
    }

    /// The Surface that the decoder renders into.
    pub fn surface(&self) -> NativeWindow {
        self.video.surface.clone()
    }

    /// Whether there is an overlay to show.
    pub fn has_overlay(&self) -> bool {
        self.overlay.is_some()
    }

    /// Show or hide the overlay: one `setVisibility`, posted to the main
    /// thread without waiting. The main thread logs when it has applied it.
    pub fn show_overlay(&self, app: &AndroidApp, visible: bool) {
        let Some(overlay) = &self.overlay else {
            return;
        };
        let vm = app.vm_as_ptr() as usize;
        let control = overlay.control.clone();
        let posted = Instant::now();
        app.run_on_java_main_thread(Box::new(move || {
            let began = Instant::now();
            let applied = attached(vm, |env| {
                let transaction =
                    env.new_object("android/view/SurfaceControl$Transaction", "()V", &[])?;
                env.call_method(
                    &transaction,
                    "setVisibility",
                    "(Landroid/view/SurfaceControl;Z)Landroid/view/SurfaceControl$Transaction;",
                    &[JValue::Object(control.as_obj()), visible.into()],
                )?;
                env.call_method(&transaction, "apply", "()V", &[])?;
                Ok(())
            });
            let what = if visible { "shown" } else { "hidden" };
            match applied {
                Ok(()) => crate::log::line(&format!(
                    "overlay: layer {what}: posted {:.1} ms before, applied in {:.2} ms",
                    began.duration_since(posted).as_secs_f64() * 1000.0,
                    began.elapsed().as_secs_f64() * 1000.0
                )),
                Err(error) => {
                    crate::log::line(&format!("overlay: the layer was not {what}: {error:#}"))
                }
            }
        }));
    }

    /// Remove the layers, on Android's main thread, without waiting: the
    /// main thread may wait for this one (in `onDestroy`).
    pub fn release(self, app: &AndroidApp) {
        let vm = app.vm_as_ptr() as usize;
        let Self { video, overlay, .. } = self;
        app.run_on_java_main_thread(Box::new(move || {
            remove(vm, video);
            if let Some(overlay) = overlay {
                remove(vm, overlay);
            }
        }));
    }
}

/// Work with this thread attached to the VM, in a local frame.
fn attached<T>(vm: usize, work: impl FnOnce(&mut JNIEnv) -> jni::errors::Result<T>) -> Result<T> {
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

/// Take a layer off the screen at once, as the spike did, then release its
/// Surface and its SurfaceControl. A layer that keeps its parent stays in
/// the window's tree after release. The native window goes first.
fn remove(vm: usize, layer: Layer) {
    let Layer {
        surface,
        control,
        java_surface,
    } = layer;
    drop(surface);
    let _ = attached(vm, |env| {
        let transaction = env.new_object("android/view/SurfaceControl$Transaction", "()V", &[])?;
        env.call_method(
            &transaction,
            "reparent",
            "(Landroid/view/SurfaceControl;Landroid/view/SurfaceControl;)Landroid/view/SurfaceControl$Transaction;",
            &[JValue::Object(control.as_obj()), JValue::Object(&JObject::null())],
        )?;
        env.call_method(&transaction, "apply", "()V", &[])?;
        env.call_method(&java_surface, "release", "()V", &[])?;
        env.call_method(&control, "release", "()V", &[])?;
        Ok(())
    });
}

/// The placement, with no rotation: the stream's part that shows in the
/// window, into its place there (`placement::geometry`): all of it into its
/// letterbox, or its middle across the window when it fills it. API 29;
/// deprecated at 33 for setPosition, setScale and setCrop.
fn set_geometry(
    env: &mut JNIEnv,
    transaction: &JObject,
    layer: &JObject,
    stream: (i32, i32),
    placed: Rect,
    window: (i32, i32),
) -> jni::errors::Result<()> {
    let (source, shown) = geometry(stream, placed, window);
    let source = rect(env, source)?;
    let destination = rect(env, shown)?;
    env.call_method(
        transaction,
        "setGeometry",
        "(Landroid/view/SurfaceControl;Landroid/graphics/Rect;Landroid/graphics/Rect;I)Landroid/view/SurfaceControl$Transaction;",
        &[
            JValue::Object(layer),
            JValue::Object(&source),
            JValue::Object(&destination),
            0.into(),
        ],
    )?;
    Ok(())
}

fn rect<'a>(env: &mut JNIEnv<'a>, r: Rect) -> jni::errors::Result<JObject<'a>> {
    env.new_object(
        "android/graphics/Rect",
        "(IIII)V",
        &[r.left.into(), r.top.into(), r.right.into(), r.bottom.into()],
    )
}

const TRANSACTION: &str = "Landroid/view/SurfaceControl$Transaction;";

/// A layer named `name` with buffers of `size`, its reparent transaction to
/// the window's root (not yet applied) and its Surface.
fn make_layer<'a>(
    env: &mut JNIEnv<'a>,
    activity: &JObject,
    name: &str,
    size: (i32, i32),
    translucent: bool,
) -> jni::errors::Result<(JObject<'a>, JObject<'a>)> {
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
    let name = env.new_string(name)?;
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
        &[size.0.into(), size.1.into()],
    )?;
    if translucent {
        env.call_method(
            &builder,
            "setFormat",
            "(I)Landroid/view/SurfaceControl$Builder;",
            &[TRANSLUCENT.into()],
        )?;
        env.call_method(
            &builder,
            "setOpaque",
            "(Z)Landroid/view/SurfaceControl$Builder;",
            &[false.into()],
        )?;
    }
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
    Ok((layer, transaction))
}

/// Put the layer at `z` among the root's children (the window's own buffer
/// is below 0), shown or hidden.
fn stack(
    env: &mut JNIEnv,
    transaction: &JObject,
    layer: &JObject,
    z: i32,
    visible: bool,
) -> jni::errors::Result<()> {
    env.call_method(
        transaction,
        "setLayer",
        format!("(Landroid/view/SurfaceControl;I){TRANSACTION}"),
        &[JValue::Object(layer), z.into()],
    )?;
    env.call_method(
        transaction,
        "setVisibility",
        format!("(Landroid/view/SurfaceControl;Z){TRANSACTION}"),
        &[JValue::Object(layer), visible.into()],
    )?;
    Ok(())
}

/// The layer's Surface and its native window, then global references.
fn finish(env: &mut JNIEnv, layer: &JObject) -> jni::errors::Result<Layer> {
    let java_surface = env.new_object(
        "android/view/Surface",
        "(Landroid/view/SurfaceControl;)V",
        &[JValue::Object(layer)],
    )?;
    // SAFETY: a live JNI environment and Surface; the call acquires its own
    // reference to the native window, which NativeWindow releases.
    let surface = unsafe {
        ndk::native_window::NativeWindow::from_surface(env.get_raw(), java_surface.as_raw())
    }
    .ok_or(jni::errors::Error::NullPtr("ANativeWindow_fromSurface"))?;
    Ok(Layer {
        surface,
        control: env.new_global_ref(layer)?,
        java_surface: env.new_global_ref(&java_surface)?,
    })
}

/// The video layer: above the window's own buffer, which shows in the
/// bars, and placed.
fn make_video(
    env: &mut JNIEnv,
    activity: &JObject,
    stream: (i32, i32),
    placed: Rect,
    window: (i32, i32),
) -> jni::errors::Result<Layer> {
    let (layer, transaction) = make_layer(env, activity, "korri-viewer-video", stream, false)?;
    stack(env, &transaction, &layer, 0, true)?;
    set_geometry(env, &transaction, &layer, stream, placed, window)?;
    env.call_method(&transaction, "apply", "()V", &[])?;
    finish(env, &layer)
}

/// The overlay layer: above the video, the window's size, hidden.
fn make_overlay(
    env: &mut JNIEnv,
    activity: &JObject,
    window: (i32, i32),
) -> jni::errors::Result<Layer> {
    let (layer, transaction) = make_layer(env, activity, "korri-viewer-overlay", window, true)?;
    stack(env, &transaction, &layer, 1, false)?;
    env.call_method(&transaction, "apply", "()V", &[])?;
    finish(env, &layer)
}

/// Fill a window's buffer, with the CPU, once: `pixel` gives the value at
/// a row of a buffer of a height.
fn fill(window: &NativeWindow, pixel: impl Fn(usize, usize) -> u32) -> Result<()> {
    // 0x0: the buffers take the Surface's default size.
    window
        .set_buffers_geometry(0, 0, Some(HardwareBufferFormat::R8G8B8A8_UNORM))
        .context("cannot set the window's format")?;
    let mut buffer = window.lock(None).context("cannot lock the window")?;
    let (width, stride, height) = (buffer.width(), buffer.stride(), buffer.height());
    let pixels = NonNull::new(buffer.bits().cast::<u32>()).context("the window has no pixels")?;
    // SAFETY: the locked buffer holds stride × height pixels of 4 bytes.
    let pixels = unsafe { std::slice::from_raw_parts_mut(pixels.as_ptr(), stride * height) };
    for (y, row) in pixels.chunks_exact_mut(stride).enumerate() {
        row[..width].fill(pixel(y, height));
    }
    Ok(())
}

/// One CPU fill of the app's own window, black: it shows in the letterbox
/// bars, under the video layer. Once, not for each frame.
pub fn fill_black(window: &NativeWindow) -> Result<()> {
    fill(window, |_, _| 0xff00_0000)
}

/// The overlay's stand-in for the shell's overlay: the window's lower half
/// in black at 75 % opacity (premultiplied RGBA), the upper half clear. It
/// is not a design. The proof reads the stream's frame-ID bar through the
/// clear half, and finds the stand-in in the lower half.
fn draw_stand_in(window: &NativeWindow) -> Result<()> {
    fill(
        window,
        |y, height| {
            if y >= height / 2 { 0xc000_0000 } else { 0 }
        },
    )
}
