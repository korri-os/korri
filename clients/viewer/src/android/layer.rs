// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's layers: a black backdrop, the video above it and, above both,
//! the gameplay overlay. Each is a `SurfaceControl` made through JNI calls
//! into Android's own classes (API 29 and 31), as the D5 spike found
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
//! The backdrop (#495) is opaque black and covers the window, above the
//! window's own buffer and under the video, so the bars beside or above the
//! picture are black, and so is what shows through the overlay there. The
//! Korri app cannot fill its window black, as the viewer's own app does: the
//! shell draws its library into that window and goes on drawing it during
//! the stream. The backdrop's buffer is filled once with the CPU; when the
//! window's size changes, the same `setGeometry` transaction that places the
//! video stretches it over the window. It shows from the stream's first
//! decoded picture on ([`Layers::restream`]), as Artemis's Korri mode turns
//! its window black at the first frame (`Game.revealKorriStream`): until
//! then the window shows what it showed.
//!
//! The overlay layer (slice D5) is translucent and starts hidden. It stays
//! out of the camera cutout, as Artemis keeps its overlay view inside the
//! display cutout's safe area (`KorriStreamCutout.keepOutOfCutout`, #494):
//! it covers the window less the cutout's insets
//! (`placement::keep_out_of_cutout`), which Android's main thread reads from
//! the window (`View.getRootWindowInsets`) each time it places the layer.
//! Its buffers take that size (`Transaction.setBufferSize`), so whatever
//! draws into its Surface draws at that size, and a touch reaches the
//! shell's overlay from the layer's corner ([`Layers::overlay_origin`]). The
//! layer follows the cutout when the window's size changes
//! ([`Layers::follow`]), and when a main event says that the cutout may have
//! moved while the size stayed, as in a turn by 180°
//! ([`may_move_the_cutout`], [`Layers::follow_cutout`]). The video and the
//! backdrop still reach the window's edges, into the cutout, as Artemis's
//! stream does. Showing and hiding the overlay is one `setVisibility`; a
//! hidden layer is not composed.
//! What draws into it is the app's [`Presenter`]: the viewer's own app draws
//! D5's stand-in once with the CPU, and again when the window's size
//! changes; the Korri app lends its Surface to the shell's overlay renderer
//! (`clients/shell/vendor/i-slint-backend-android-activity/surface_window.rs`,
//! #434 slice D5b), and nothing here draws into it: a Surface that the CPU
//! has drawn into stays the CPU's, and EGL cannot draw into it. The stream's
//! size does not change it.
//!
//! Two apps make these layers: the viewer's own (`clients/viewer/android`),
//! which waits for them in its loop ([`Layers::make`]), and the Korri app
//! (`clients/android-app`), whose loop is Slint's: it asks for them
//! ([`Layers::request`]) and reads the answer at each tick
//! ([`Pending::poll`]), so its thread never waits for Android's main thread.
//! When an app stops waiting (a stop, or the window goes), the main thread
//! removes the layers that it made, also those that it sent already
//! (`answer.rs`, #497).
use crate::{
    answer::{self, Answer},
    gestures::Gesture,
    overlay::Presenter,
    pan_zoom::PanZoom,
    placement::{Fill, Insets, Rect, geometry, keep_out_of_cutout, letterbox, place},
};
use android_activity::{AndroidApp, MainEvent, PollEvent};
use anyhow::{Context, Result, anyhow};
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JObject, JValue},
};
use ndk::{hardware_buffer_format::HardwareBufferFormat, native_window::NativeWindow};
use std::{
    ptr::NonNull,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    time::{Duration, Instant},
};

/// How long Android's main thread may take to make the layers.
const MAKE_DEADLINE: Duration = Duration::from_secs(5);

/// `android.graphics.PixelFormat.TRANSLUCENT`.
const TRANSLUCENT: i32 = -3;

/// The layers' order among the window root's children. The window's own
/// buffer, which the shell draws into in the Korri app, is below 0.
const BACKDROP_Z: i32 = 0;
const VIDEO_Z: i32 = 1;
const OVERLAY_Z: i32 = 2;

/// One layer, its Surface and the native window of that Surface. A
/// SurfaceControl's finalizer would remove the layer; the app releases both
/// itself.
struct Layer {
    surface: NativeWindow,
    control: GlobalRef,
    java_surface: GlobalRef,
}

/// The black backdrop under the video and the size of its one buffer: the
/// window's when it was made.
struct Backdrop {
    layer: Layer,
    buffer: (i32, i32),
    /// Asked to show: it shows from the stream's first decoded picture on.
    shown: bool,
}

pub struct Layers {
    video: Layer,
    /// The black backdrop; none when Android did not make it. The bars then
    /// show the window's own buffer.
    backdrop: Option<Backdrop>,
    /// The gameplay overlay's layer; none when Android did not make it.
    /// The stream plays without an overlay then.
    overlay: Option<Layer>,
    /// Where the overlay layer is in the window, out of the display cutout,
    /// as Android's main thread last placed it.
    overlay_place: Arc<Mutex<Rect>>,
    /// The activity, whose window's insets give the display cutout.
    activity: GlobalRef,
    presenter: Presenter,
    /// The overlay's buffers took a new size since it was last asked
    /// ([`Layers::overlay_resized`]): Android's main thread sets it once it
    /// has applied the size.
    overlay_resized: Arc<AtomicBool>,
    pub stream: (i32, i32),
    /// The window's size that the placement follows.
    pub window: (i32, i32),
    /// Where the whole picture goes, as the newest transaction places it;
    /// when it fills the window, it reaches past it on two sides, and when
    /// it is zoomed, it is larger than its fitted or filled place.
    pub placed: Rect,
    /// "Crop to fill": applied by Android's main thread, or on its way.
    fill: Fill,
    /// "Pan & zoom": the picture's zoom in its fitted or filled place.
    pan_zoom: PanZoom,
}

/// What Android's main thread sends back: the video layer, and the backdrop
/// and the overlay layer or why there is none.
struct Made {
    video: Layer,
    backdrop: Result<Layer>,
    overlay: Result<Layer>,
}

impl Made {
    /// Remove every layer, on Android's main thread.
    fn remove(self, vm: usize) {
        remove(vm, self.video);
        for layer in [self.backdrop, self.overlay].into_iter().flatten() {
            remove(vm, layer);
        }
    }
}

/// Layers that Android's main thread is making. Dropping it before it gave
/// the layers is safe: the main thread then removes what it made, whether
/// it sent them already or not.
pub struct Pending {
    answer: Answer<Result<Made>>,
    until: Instant,
    stream: (i32, i32),
    window: (i32, i32),
    placed: Rect,
    presenter: Presenter,
    overlay_place: Arc<Mutex<Rect>>,
    activity: GlobalRef,
}

impl Pending {
    /// The layers once the main thread has made them; `None` while it works.
    /// An error when it failed, dropped them or took longer than 5 s.
    pub fn poll(&mut self) -> Option<Result<Layers>> {
        self.wait(Duration::ZERO)
    }

    /// As [`Pending::poll`], waiting at most `timeout` for the answer.
    fn wait(&mut self, timeout: Duration) -> Option<Result<Layers>> {
        let made = match self.answer.recv_timeout(timeout) {
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
        Some(made.map(
            |Made {
                 video,
                 backdrop,
                 overlay,
             }| {
                let backdrop = match backdrop {
                    Ok(layer) => Some(Backdrop {
                        layer,
                        buffer: self.window,
                        shown: false,
                    }),
                    Err(error) => {
                        crate::log::line(&format!(
                            "picture: no backdrop layer; the bars show the window: {error:#}"
                        ));
                        None
                    }
                };
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
                    backdrop,
                    overlay,
                    overlay_place: self.overlay_place.clone(),
                    activity: self.activity.clone(),
                    presenter: self.presenter,
                    overlay_resized: Arc::new(AtomicBool::new(false)),
                    stream: self.stream,
                    window: self.window,
                    placed: self.placed,
                    fill: Fill::default(),
                    pan_zoom: PanZoom::default(),
                }
            },
        ))
    }
}

impl Layers {
    /// Ask Android's main thread for the layers of a stream of `stream`
    /// pixels, placed in a window of `window` pixels, with an overlay layer
    /// for `presenter`. It returns at once; [`Pending::poll`] gives the
    /// layers. The window must exist: call it outside android-activity's
    /// event callback, after `InitWindow`.
    pub fn request(
        app: &AndroidApp,
        window: (i32, i32),
        stream: (i32, i32),
        presenter: Presenter,
    ) -> Result<Pending> {
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
        let post = {
            let app = app.clone();
            Box::new(move |work: answer::Work| app.run_on_java_main_thread(Box::new(work)))
        };
        // Made layers that no one takes any more: the main thread removes them.
        let (reply, answer) = answer::channel(post, move |made: Result<Made>| {
            if let Ok(made) = made {
                made.remove(vm);
            }
        });
        // The layers keep the activity, for the cutout of its window.
        let (kept, overlay_place) = (activity.clone(), Arc::new(Mutex::new(whole(window))));
        let place = overlay_place.clone();
        app.run_on_java_main_thread(Box::new(move || {
            let made = attached(vm, |env| {
                make_video(env, activity.as_obj(), stream, placed, window)
            })
            .map(|video| {
                let backdrop = attached(vm, |env| make_backdrop(env, activity.as_obj(), window))
                    .and_then(|backdrop| match fill_black(&backdrop.surface) {
                        Ok(()) => Ok(backdrop),
                        Err(error) => {
                            remove(vm, backdrop);
                            Err(error)
                        }
                    });
                let overlay = attached(vm, |env| {
                    make_overlay(env, activity.as_obj(), window, &place)
                })
                .and_then(|overlay| match presenter {
                    Presenter::Shell => Ok(overlay),
                    Presenter::StandIn => match draw_stand_in(&overlay.surface) {
                        Ok(()) => Ok(overlay),
                        Err(error) => {
                            remove(vm, overlay);
                            Err(error)
                        }
                    },
                });
                Made {
                    video,
                    backdrop,
                    overlay,
                }
            });
            // When the asking thread stopped waiting, this removes them.
            reply.send(made);
        }));
        Ok(Pending {
            answer,
            until: Instant::now() + MAKE_DEADLINE,
            stream,
            window,
            placed,
            presenter,
            overlay_place,
            activity: kept,
        })
    }

    /// The layers for a stream of `stream` pixels, placed in `window`, with
    /// D5's stand-in in the overlay layer; none when the app leaves (its
    /// window or the activity goes) first. They are
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
        let mut pending = Self::request(
            app,
            (window.width(), window.height()),
            stream,
            Presenter::StandIn,
        )?;
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
    /// one emulator run), and a fold changes it too. One `setGeometry` for
    /// the video and one that stretches the backdrop over the window, and
    /// the overlay goes out of the display cutout that the window has now,
    /// its buffers of that size; then the stand-in is drawn again at that
    /// size, or the shell's renderer hears of the new size
    /// ([`Layers::overlay_resized`]). Posted to the main thread without
    /// waiting. False when the size is the same.
    pub fn follow(&mut self, app: &AndroidApp, window: &NativeWindow) -> bool {
        let size = (window.width(), window.height());
        if size == self.window {
            return false;
        }
        self.window = size;
        self.placed = self.zoomed(self.fill.placing());
        let vm = app.vm_as_ptr() as usize;
        let (video, stream, placed) = (self.video.control.clone(), self.stream, self.placed);
        let backdrop = self
            .backdrop
            .as_ref()
            .map(|backdrop| (backdrop.layer.control.clone(), backdrop.buffer));
        let overlay = self
            .overlay
            .as_ref()
            .map(|overlay| (overlay.control.clone(), overlay.surface.clone()));
        let (activity, place) = (self.activity.clone(), self.overlay_place.clone());
        let (presenter, overlay_resized) = (self.presenter, self.overlay_resized.clone());
        app.run_on_java_main_thread(Box::new(move || {
            let moved = attached(vm, |env| {
                let transaction =
                    env.new_object("android/view/SurfaceControl$Transaction", "()V", &[])?;
                set_geometry(env, &transaction, video.as_obj(), stream, placed, size)?;
                if let Some((backdrop, buffer)) = &backdrop {
                    cover(env, &transaction, backdrop.as_obj(), *buffer, size)?;
                }
                let moved = match &overlay {
                    Some((overlay, _)) => {
                        let out = out_of_cutout(env, activity.as_obj(), size)?;
                        put_overlay(env, &transaction, overlay.as_obj(), out.bounds)?;
                        Some(out)
                    }
                    None => None,
                };
                env.call_method(&transaction, "apply", "()V", &[])?;
                Ok(moved)
            });
            // `apply` gave the overlay's Surface its new default size.
            if let (Ok(Some(moved)), Some((_, surface))) = (moved, overlay) {
                overlay_moved(&place, moved, size, presenter, &overlay_resized, &surface);
            }
        }));
        true
    }

    /// Place the overlay out of the display cutout again when the cutout may
    /// have moved while the window kept its size ([`may_move_the_cutout`]):
    /// a turn by 180° takes it to the other side. Android's main thread reads
    /// the window's cutout and moves the layer only when its place changed:
    /// one `setGeometry`, and `setBufferSize` when its size changed too. Posted
    /// without waiting.
    pub fn follow_cutout(&self, app: &AndroidApp) {
        let Some(overlay) = &self.overlay else {
            return;
        };
        let vm = app.vm_as_ptr() as usize;
        let (control, surface) = (overlay.control.clone(), overlay.surface.clone());
        let (activity, place, window) = (
            self.activity.clone(),
            self.overlay_place.clone(),
            self.window,
        );
        let (presenter, overlay_resized) = (self.presenter, self.overlay_resized.clone());
        app.run_on_java_main_thread(Box::new(move || {
            let moved = attached(vm, |env| {
                let moved = out_of_cutout(env, activity.as_obj(), window)?;
                if moved.bounds == *lock(&place) {
                    return Ok(None);
                }
                let transaction =
                    env.new_object("android/view/SurfaceControl$Transaction", "()V", &[])?;
                put_overlay(env, &transaction, control.as_obj(), moved.bounds)?;
                env.call_method(&transaction, "apply", "()V", &[])?;
                Ok(Some(moved))
            });
            match moved {
                Ok(Some(moved)) => {
                    overlay_moved(&place, moved, window, presenter, &overlay_resized, &surface)
                }
                Ok(None) => {}
                Err(error) => crate::log::line(&format!(
                    "overlay: the layer was not placed out of the display cutout again: {error:#}"
                )),
            }
        }));
    }

    /// Place the video for a new size of the decoded picture: the sender
    /// changed the stream's size. One transaction, posted to the main thread
    /// without waiting: `setBufferSize`, so that the video layer's buffer
    /// queue scales the decoder's buffers to the new size and not to the old
    /// one, and `setGeometry` with the new shape's letterbox. The overlay
    /// keeps its place and buffers. False when the size is the same.
    ///
    /// The decoder gives its first size with its first picture: the black
    /// backdrop then shows under the video, once ([`Layers::reveal`]).
    pub fn restream(&mut self, app: &AndroidApp, stream: (i32, i32)) -> bool {
        self.reveal(app);
        if stream == self.stream || stream.0 <= 0 || stream.1 <= 0 {
            return false;
        }
        self.stream = stream;
        self.placed = self.zoomed(self.fill.placing());
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

    /// Show the black backdrop under the video, the first time only: one
    /// `setVisibility`, posted to the main thread without waiting. The main
    /// thread logs when it has applied it.
    fn reveal(&mut self, app: &AndroidApp) {
        let Some(backdrop) = self.backdrop.as_mut().filter(|backdrop| !backdrop.shown) else {
            return;
        };
        backdrop.shown = true;
        let vm = app.vm_as_ptr() as usize;
        let control = backdrop.layer.control.clone();
        app.run_on_java_main_thread(Box::new(move || {
            let applied = attached(vm, |env| {
                let transaction =
                    env.new_object("android/view/SurfaceControl$Transaction", "()V", &[])?;
                env.call_method(
                    &transaction,
                    "setVisibility",
                    format!("(Landroid/view/SurfaceControl;Z){TRANSACTION}"),
                    &[JValue::Object(control.as_obj()), true.into()],
                )?;
                env.call_method(&transaction, "apply", "()V", &[])?;
                Ok(())
            });
            match applied {
                Ok(()) => crate::log::line(
                    "picture: the black backdrop shows under the video, with its first picture",
                ),
                Err(error) => crate::log::line(&format!(
                    "picture: the black backdrop was not shown; the bars show the window: {error:#}"
                )),
            }
        }));
    }

    /// Fill the window with the picture, its edges cut, or fit it with bars
    /// again ("Crop to fill", Artemis's `Game.toggleFillMode`): one
    /// `setGeometry`, posted to the main thread without waiting. The answer
    /// comes once the main thread has applied it, or could not; give it to
    /// `fill_answered`. Only then does the fill count (`fill`). None when the
    /// placement does not change: the fill counts at once.
    pub fn set_fill(&mut self, app: &AndroidApp, fill: bool) -> Option<mpsc::Receiver<Result<()>>> {
        self.fill.ask(fill);
        let placed = self.zoomed(fill);
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
        let placed = self.zoomed(fill);
        if placed != self.placed {
            let _ = self.place_video(app, placed);
        }
    }

    /// The fill that Android's main thread has applied.
    pub fn fill(&self) -> bool {
        self.fill.applied()
    }

    /// The picture's place for `fill` with its zoom: the fitted or filled
    /// place is the zoom's container, which keeps its zoom when its size
    /// changes (`PanZoomHandler.handleSurfaceChange`).
    fn zoomed(&mut self, fill: bool) -> Rect {
        let container = place(self.stream, self.window, fill);
        self.pan_zoom.resized(container);
        self.pan_zoom.rect(container)
    }

    /// One gesture of "Pan & zoom": the video moves to its new place, one
    /// `setGeometry` posted to the main thread without waiting. True when
    /// it moved: the input then maps touches into the new place.
    pub fn zoom(&mut self, app: &AndroidApp, gesture: Gesture) -> bool {
        let container = place(self.stream, self.window, self.fill.placing());
        if !self.pan_zoom.gesture(gesture, container) {
            return false;
        }
        let placed = self.pan_zoom.rect(container);
        let _ = self.place_video(app, placed);
        true
    }

    /// The picture's zoom: 1 when it is not zoomed.
    pub fn zoom_scale(&self) -> f32 {
        self.pan_zoom.scale()
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

    /// The overlay layer's `android.view.Surface`, for the shell's renderer
    /// (`Presenter::Shell`). It stays valid until [`Layers::release`]; the
    /// renderer must let it go before that.
    pub fn overlay_surface(&self) -> Option<&GlobalRef> {
        self.overlay.as_ref().map(|overlay| &overlay.java_surface)
    }

    /// The overlay layer's corner in the window, where Android's main thread
    /// last placed it: a touch reaches the shell's overlay in the layer's
    /// pixels, the window's less this corner.
    pub fn overlay_origin(&self) -> (f32, f32) {
        let place = *lock(&self.overlay_place);
        (place.left as f32, place.top as f32)
    }

    /// Whether the overlay's buffers took a new size since the last call:
    /// its renderer reads the new size from the Surface.
    pub fn overlay_resized(&self) -> bool {
        self.overlay_resized.swap(false, Ordering::AcqRel)
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
        let Self {
            video,
            backdrop,
            overlay,
            ..
        } = self;
        app.run_on_java_main_thread(Box::new(move || {
            remove(vm, video);
            if let Some(backdrop) = backdrop {
                remove(vm, backdrop.layer);
            }
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
    put(env, transaction, layer, source, shown)
}

/// The backdrop's buffer of `buffer` pixels, stretched over the whole
/// window: the stretch of one black buffer is black.
fn cover(
    env: &mut JNIEnv,
    transaction: &JObject,
    layer: &JObject,
    buffer: (i32, i32),
    window: (i32, i32),
) -> jni::errors::Result<()> {
    put(env, transaction, layer, whole(buffer), whole(window))
}

fn whole(size: (i32, i32)) -> Rect {
    Rect {
        left: 0,
        top: 0,
        right: size.0,
        bottom: size.1,
    }
}

/// One `setGeometry`, with no rotation: the layer's buffer part `source`
/// into `destination`, in window pixels.
fn put(
    env: &mut JNIEnv,
    transaction: &JObject,
    layer: &JObject,
    source: Rect,
    destination: Rect,
) -> jni::errors::Result<()> {
    let source = rect(env, source)?;
    let destination = rect(env, destination)?;
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

/// How a layer's buffers cover what is under it. It is fixed when the layer
/// is built: on API 31 the child layer that holds a layer's buffers
/// (`bbq-wrapper`) takes its opacity from the layer's build, and a later
/// `Transaction.setOpaque` does not reach it (SurfaceFlinger's dump in the
/// #495 proof).
#[derive(Clone, Copy)]
enum Blend {
    /// As the decoder's buffers are.
    Decoded,
    /// Opaque: the compositor leaves out what is under it.
    Opaque,
    /// With alpha.
    Translucent,
}

/// A layer named `name` with buffers of `size`, its reparent transaction to
/// the window's root (not yet applied) and its Surface.
fn make_layer<'a>(
    env: &mut JNIEnv<'a>,
    activity: &JObject,
    name: &str,
    size: (i32, i32),
    blend: Blend,
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
    let opaque = match blend {
        Blend::Decoded => None,
        Blend::Opaque => Some(true),
        Blend::Translucent => {
            env.call_method(
                &builder,
                "setFormat",
                "(I)Landroid/view/SurfaceControl$Builder;",
                &[TRANSLUCENT.into()],
            )?;
            Some(false)
        }
    };
    if let Some(opaque) = opaque {
        env.call_method(
            &builder,
            "setOpaque",
            "(Z)Landroid/view/SurfaceControl$Builder;",
            &[opaque.into()],
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

/// The video layer: above the backdrop, and placed.
fn make_video(
    env: &mut JNIEnv,
    activity: &JObject,
    stream: (i32, i32),
    placed: Rect,
    window: (i32, i32),
) -> jni::errors::Result<Layer> {
    let (layer, transaction) =
        make_layer(env, activity, "korri-viewer-video", stream, Blend::Decoded)?;
    stack(env, &transaction, &layer, VIDEO_Z, true)?;
    set_geometry(env, &transaction, &layer, stream, placed, window)?;
    env.call_method(&transaction, "apply", "()V", &[])?;
    finish(env, &layer)
}

/// The black backdrop: above the window's own buffer, under the video, with
/// a buffer of the window's size over the window, opaque, so that the
/// compositor leaves out the window under it, and hidden. Its buffer is not
/// filled yet.
fn make_backdrop(
    env: &mut JNIEnv,
    activity: &JObject,
    window: (i32, i32),
) -> jni::errors::Result<Layer> {
    let (layer, transaction) = make_layer(
        env,
        activity,
        "korri-viewer-backdrop",
        window,
        Blend::Opaque,
    )?;
    stack(env, &transaction, &layer, BACKDROP_Z, false)?;
    cover(env, &transaction, &layer, window, window)?;
    env.call_method(&transaction, "apply", "()V", &[])?;
    finish(env, &layer)
}

/// The overlay layer: above the video, out of the display cutout that the
/// window has now, hidden. Its place goes into `place` once it is applied.
fn make_overlay(
    env: &mut JNIEnv,
    activity: &JObject,
    window: (i32, i32),
    place: &Mutex<Rect>,
) -> jni::errors::Result<Layer> {
    let out = out_of_cutout(env, activity, window)?;
    let (layer, transaction) = make_layer(
        env,
        activity,
        "korri-viewer-overlay",
        buffer_size(out.bounds),
        Blend::Translucent,
    )?;
    stack(env, &transaction, &layer, OVERLAY_Z, false)?;
    put_overlay(env, &transaction, &layer, out.bounds)?;
    env.call_method(&transaction, "apply", "()V", &[])?;
    let made = finish(env, &layer)?;
    *lock(place) = out.bounds;
    log_overlay(out, window);
    Ok(made)
}

/// The overlay's place in a window of `window` pixels and the display
/// cutout's insets that give it.
#[derive(Clone, Copy)]
struct OutOfCutout {
    bounds: Rect,
    cutout: Insets,
}

/// The overlay's place out of the display cutout that the activity's window
/// has now (`placement::keep_out_of_cutout`).
fn out_of_cutout(
    env: &mut JNIEnv,
    activity: &JObject,
    window: (i32, i32),
) -> jni::errors::Result<OutOfCutout> {
    let cutout = cutout(env, activity)?;
    Ok(OutOfCutout {
        bounds: keep_out_of_cutout(window, cutout),
        cutout,
    })
}

/// The display cutout's insets on the activity's window, as its views have
/// them now: `getRootWindowInsets().getInsets(WindowInsets.Type.displayCutout())`,
/// the insets that reach Artemis's overlay view. A window that has no
/// insets yet (not attached) has no cutout, as Artemis's view has no margins
/// before its first insets. API 30.
fn cutout(env: &mut JNIEnv, activity: &JObject) -> jni::errors::Result<Insets> {
    let window = env
        .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
        .l()?;
    let decor = env
        .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
        .l()?;
    let insets = env
        .call_method(
            &decor,
            "getRootWindowInsets",
            "()Landroid/view/WindowInsets;",
            &[],
        )?
        .l()?;
    if insets.is_null() {
        return Ok(Insets::default());
    }
    let mask = env
        .call_static_method(
            "android/view/WindowInsets$Type",
            "displayCutout",
            "()I",
            &[],
        )?
        .i()?;
    let cutout = env
        .call_method(
            &insets,
            "getInsets",
            "(I)Landroid/graphics/Insets;",
            &[mask.into()],
        )?
        .l()?;
    let mut side = |name: &str| {
        env.get_field(&cutout, name, "I")
            .and_then(|value| value.i())
    };
    Ok(Insets {
        left: side("left")?,
        top: side("top")?,
        right: side("right")?,
        bottom: side("bottom")?,
    })
}

/// The overlay layer's buffers for its place: its size, at least a pixel
/// each way, as a buffer must have.
fn buffer_size(bounds: Rect) -> (i32, i32) {
    (
        (bounds.right - bounds.left).max(1),
        (bounds.bottom - bounds.top).max(1),
    )
}

/// The overlay layer at `bounds` in the window, its buffers of that size,
/// drawn one to one.
fn put_overlay(
    env: &mut JNIEnv,
    transaction: &JObject,
    layer: &JObject,
    bounds: Rect,
) -> jni::errors::Result<()> {
    let buffer = buffer_size(bounds);
    env.call_method(
        transaction,
        "setBufferSize",
        format!("(Landroid/view/SurfaceControl;II){TRANSACTION}"),
        &[JValue::Object(layer), buffer.0.into(), buffer.1.into()],
    )?;
    put(env, transaction, layer, whole(buffer), bounds)
}

/// Android's main thread applied the overlay's new place: touches count from
/// it now. When its size changed, the stand-in is drawn again at that size,
/// or the shell's renderer hears of it ([`Layers::overlay_resized`]).
fn overlay_moved(
    place: &Mutex<Rect>,
    moved: OutOfCutout,
    window: (i32, i32),
    presenter: Presenter,
    resized: &AtomicBool,
    surface: &NativeWindow,
) {
    let before = std::mem::replace(&mut *lock(place), moved.bounds);
    log_overlay(moved, window);
    if buffer_size(before) == buffer_size(moved.bounds) {
        return;
    }
    match presenter {
        Presenter::Shell => resized.store(true, Ordering::Release),
        Presenter::StandIn => {
            if let Err(error) = draw_stand_in(surface) {
                crate::log::line(&format!(
                    "overlay: the stand-in was not drawn again: {error:#}"
                ));
            }
        }
    }
}

fn log_overlay(out: OutOfCutout, window: (i32, i32)) {
    let OutOfCutout { bounds, cutout } = out;
    crate::log::line(&format!(
        "overlay: layer placed at ({}, {})-({}, {}) in the window {}x{}, out of the display cutout (left {}, top {}, right {}, bottom {})",
        bounds.left,
        bounds.top,
        bounds.right,
        bounds.bottom,
        window.0,
        window.1,
        cutout.left,
        cutout.top,
        cutout.right,
        cutout.bottom
    ));
}

/// Whether an app's main event may have moved the display cutout in its
/// window while the window kept its size, so that the app asks for
/// [`Layers::follow_cutout`]. A turn by 180° changes no size; Android asks
/// the window to draw again after it (`RedrawNeeded`). The others come with
/// a new configuration, content area or window size.
pub fn may_move_the_cutout(event: &MainEvent<'_>) -> bool {
    matches!(
        event,
        MainEvent::RedrawNeeded { .. }
            | MainEvent::ConfigChanged { .. }
            | MainEvent::ContentRectChanged { .. }
            | MainEvent::WindowResized { .. }
            | MainEvent::InsetsChanged { .. }
    )
}

/// The lock, also after a panic on another thread held it: the place it
/// guards is whole at every step.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
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

/// One CPU fill of a window, black: the viewer's own app fills its own
/// window, and the backdrop's buffer is filled so. Once, not for each frame.
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
