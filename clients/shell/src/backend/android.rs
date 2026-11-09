//! NativeActivity carries the same Shell, Pico package and app.surface.*
//! connection as Linux. Android owns the window and its native event loop.
use super::{Backend, FrameCounter, HostEvent, Placement};
use crate::{
    config::Connection,
    input::{android::AndroidInput, drag::Drag, native::NativeEvent},
    korrid::Korrid,
    package::Package,
    shell::{Event, Options, Post, Shell},
};
use i_slint_backend_android_activity::{
    android_activity::{
        input::{Axis, InputEvent, KeyAction, MotionAction},
        MainEvent, PollEvent,
    },
    surface_window::SurfaceWindow,
    AndroidPlatform,
};
use jni::{
    objects::{JIntArray, JObject, JValue},
    JavaVM,
};
use slint::android::AndroidApp;
use slint_interpreter::Value;
use std::{
    cell::RefCell,
    path::Path,
    rc::Rc,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

type Sink = Rc<dyn Fn(HostEvent)>;
type Front = Rc<RefCell<Option<(Sink, FrameCounter, bool)>>>;

struct Android {
    front: Front,
    input: Rc<RefCell<AndroidInput>>,
    notifier_installed: std::cell::Cell<bool>,
    overlay: RefCell<Option<Rc<SurfaceWindow>>>,
    overlay_front: Front,
    attaching_overlay: std::cell::Cell<bool>,
    native_focus: Rc<std::cell::Cell<bool>>,
    overlay_focus: Rc<std::cell::Cell<bool>>,
}

impl Android {
    fn install(app: AndroidApp, post: mpsc::Sender<Event>) -> anyhow::Result<Self> {
        let front: Front = Default::default();
        let events = front.clone();
        let touch = front.clone();
        let drag = Rc::new(RefCell::new(Drag::default()));
        let focus_drag = drag.clone();
        let input = Rc::new(RefCell::new(AndroidInput::default()));
        let focus_input = input.clone();
        let event_input = input.clone();
        let touch_app = app.clone();
        let native_focus = Rc::new(std::cell::Cell::new(false));
        let overlay_focus = Rc::new(std::cell::Cell::new(false));
        let activity_focus = native_focus.clone();
        let surface_focus = overlay_focus.clone();
        let platform = AndroidPlatform::new_with_event_listener(app, move |event| {
            let host = match event {
                PollEvent::Main(MainEvent::GainedFocus) => {
                    activity_focus.set(true);
                    focus_input.borrow_mut().set_active(true);
                    Some(HostEvent::Focus(true))
                }
                PollEvent::Main(MainEvent::LostFocus) => {
                    activity_focus.set(false);
                    focus_input.borrow_mut().set_active(surface_focus.get());
                    focus_drag.borrow_mut().cancel();
                    Some(HostEvent::Focus(false))
                }
                PollEvent::Main(
                    MainEvent::InitWindow { .. }
                    | MainEvent::WindowResized { .. }
                    | MainEvent::ContentRectChanged { .. }
                    | MainEvent::ConfigChanged { .. },
                ) => Some(HostEvent::Resized),
                _ => None,
            };
            if let (Some(host), Some((sink, ..))) = (host, events.borrow().as_ref()) {
                sink(host);
            }
        })
        .with_input_event_filter(Box::new(move |event| {
            let input = &event_input;
            let send = |events: Vec<NativeEvent>| {
                let _ = post.send(Event::Native(events));
            };
            if let InputEvent::KeyEvent(key) = event {
                let code: u32 = key.key_code().into();
                let pressed = match key.action() {
                    KeyAction::Down => true,
                    KeyAction::Up => false,
                    _ => return false,
                };
                return if let Some(events) = input.borrow_mut().key(key.device_id(), code, pressed)
                {
                    send(events);
                    true
                } else {
                    false
                };
            }
            let InputEvent::MotionEvent(motion) = event else {
                return false;
            };
            let source: u32 = motion.source().into();
            // SOURCE_CLASS_JOYSTICK. Never interpret a touchscreen's pixel X/Y
            // as stick axes or let gamepad motion enter the touch drag path.
            if source & 0x10 != 0 {
                if let Some(pointer) = motion.pointers().next() {
                    let axis = |axis| {
                        if motion.action() == MotionAction::Cancel {
                            0.0
                        } else {
                            pointer.axis_value(axis)
                        }
                    };
                    send(input.borrow_mut().axes(
                        motion.device_id(),
                        axis(Axis::X),
                        axis(Axis::Y),
                        axis(Axis::HatX),
                        axis(Axis::HatY),
                    ));
                }
                return true;
            }
            // Only single-finger drags become momentary directional presses;
            // no surface hit-testing or focus model lives at this edge.
            let mut drag = drag.borrow_mut();
            if motion.pointer_count() != 1 {
                drag.cancel();
                return true;
            }
            let Some(pointer) = motion.pointers().next() else {
                return true;
            };
            match motion.action() {
                MotionAction::Down => drag.begin(pointer.x(), pointer.y()),
                MotionAction::Move => {
                    if let Some(window) = touch_app.native_window() {
                        for key in drag.moved(
                            pointer.x(),
                            pointer.y(),
                            window.width() as f32,
                            window.height() as f32,
                        ) {
                            if let Some((sink, ..)) = touch.borrow().as_ref() {
                                sink(HostEvent::Key { key, pressed: true });
                                sink(HostEvent::Key {
                                    key,
                                    pressed: false,
                                });
                            }
                        }
                    }
                }
                MotionAction::Up | MotionAction::Cancel | MotionAction::Outside => drag.cancel(),
                _ => (),
            }
            true
        }));
        slint::platform::set_platform(Box::new(platform))?;
        Ok(Self {
            front,
            input,
            notifier_installed: Default::default(),
            overlay: Default::default(),
            overlay_front: Default::default(),
            attaching_overlay: Default::default(),
            native_focus,
            overlay_focus,
        })
    }
    fn release_overlay(&self, shell: &mut Shell) {
        shell.detach_overlay_window();
        *self.overlay_front.borrow_mut() = None;
        if let Some(window) = self.overlay.borrow_mut().take() {
            // A failed EGL release cannot safely hand the Surface back to Android.
            if let Err(error) = window.release() {
                eprintln!("Overlay Surface release failed: {error}");
                std::process::abort();
            }
        }
        self.overlay_focus.set(false);
        // Detach reset the mapper. Reset its Android device cache too, even
        // when GainedFocus arrived before the hosted Surface was destroyed.
        self.input.borrow_mut().reset(self.native_focus.get());
    }
}

impl Backend for Android {
    fn shared_window(&self) -> bool {
        true
    }

    fn overlay_available(&self) -> bool {
        self.overlay.borrow().is_some()
    }

    fn create_overlay(
        &self,
        definition: &slint_interpreter::ComponentDefinition,
    ) -> Result<slint_interpreter::ComponentInstance, slint::PlatformError> {
        let overlay = self.overlay.borrow();
        let window = overlay
            .as_ref()
            .ok_or_else(|| slint::PlatformError::from("Overlay Surface is unavailable"))?;
        self.attaching_overlay.set(true);
        definition.create_with_existing_window(window.window())
    }

    fn attach(&self, window: &slint::Window, sink: Sink, frames: FrameCounter) {
        if self.attaching_overlay.replace(false) {
            sink(HostEvent::Focus(true));
            *self.overlay_front.borrow_mut() = Some((sink, frames, true));
            return;
        }
        *self.front.borrow_mut() = Some((sink, frames, true));
        if !self.notifier_installed.replace(true) {
            let front = self.front.clone();
            window
                .set_rendering_notifier(move |state, _| {
                    if matches!(state, slint::RenderingState::AfterRendering) {
                        if let Some((sink, frames, first)) = front.borrow_mut().as_mut() {
                            frames.add();
                            if std::mem::take(first) {
                                sink(HostEvent::FirstFrame);
                            }
                        }
                    }
                })
                .expect("Android Skia supports rendering notifications");
        }
    }

    fn place(&self, window: &slint::Window, _placement: Placement) {
        window.set_fullscreen(true);
    }
}

struct Queue(mpsc::Sender<Event>);
impl Post for Queue {
    fn post(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

#[no_mangle]
fn android_main(app: AndroidApp) {
    // Never format JNI errors: they can contain bootstrap arguments.
    if run(app).is_err() {
        eprintln!("Korri native shell startup failed");
    }
}

fn run(app: AndroidApp) -> anyhow::Result<()> {
    let vm = Arc::new(unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }?);
    let mut env = vm.attach_current_thread()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let port = env.call_method(&activity, "shellPort", "()I", &[])?.i()?;
    let capability = env
        .call_method(&activity, "shellCapability", "()Ljava/lang/String;", &[])?
        .l()?;
    let capability: String = env.get_string(&capability.into())?.into();
    let package = env
        .call_method(&activity, "shellPackagePath", "()Ljava/lang/String;", &[])?
        .l()?;
    let package: String = env.get_string(&package.into())?.into();
    let origin = env
        .call_method(&activity, "shellOrigin", "()Ljava/lang/String;", &[])?
        .l()?;
    let origin: String = env.get_string(&origin.into())?.into();
    let activity = env.new_global_ref(activity)?;
    drop(env);
    let brain = Korrid::new(Connection::new(port as u16, origin, capability));
    let (tx, rx) = mpsc::channel();
    let backend = Rc::new(Android::install(app, tx.clone())?);
    let input = backend.input.clone();
    let mut shell = Shell::new(
        vec![
            Package::at(&Path::new(&package).join("pico"))?,
            Package::at(&Path::new(&package).join("zone"))?,
        ],
        brain,
        backend.clone(),
        Arc::new(Queue(tx)),
        Options {
            placement: Placement::Fullscreen,
            ..Options::default()
        },
    );
    shell.on_quit(Box::new(|_| {
        let _ = slint::quit_event_loop();
    }));
    shell.start()?;
    let frames = shell.frames();
    let timer = slint::Timer::default();
    let mut last_frame = 0;
    let mut last_device_poll = Instant::now();
    let teardown_backend = backend.clone();
    let teardown_vm = vm.clone();
    let teardown_activity = activity.clone();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            if let Ok(mut env) = vm.attach_current_thread() {
                let result: jni::errors::Result<()> = env.with_local_frame(8, |env| {
                    if env
                        .call_method(activity.as_obj(), "shellOverlayClosing", "()Z", &[])?
                        .z()?
                    {
                        backend.release_overlay(&mut shell);
                        env.call_method(
                            activity.as_obj(),
                            "shellOverlayReleased",
                            "(Z)V",
                            &[JValue::Bool(0)],
                        )?;
                    }
                    let surface = env
                        .call_method(
                            activity.as_obj(),
                            "shellTakeOverlaySurface",
                            "()Landroid/view/Surface;",
                            &[],
                        )?
                        .l()?;
                    if !surface.is_null() {
                        let density = env
                            .call_method(activity.as_obj(), "shellOverlayDensity", "()F", &[])?
                            .f()?;
                        match unsafe {
                            SurfaceWindow::from_surface(
                                env.get_raw().cast(),
                                surface.as_raw().cast(),
                                density,
                            )
                        } {
                            Ok(window) => {
                                *backend.overlay.borrow_mut() = Some(window);
                                backend.overlay_focus.set(true);
                                input.borrow_mut().set_active(true);
                                shell.overlay_focus(true);
                            }
                            Err(error) => {
                                eprintln!("Overlay Surface creation failed: {error}");
                                backend.release_overlay(&mut shell);
                                env.call_method(
                                    activity.as_obj(),
                                    "shellOverlayReleased",
                                    "(Z)V",
                                    &[JValue::Bool(1)],
                                )?;
                            }
                        }
                    }
                    if env
                        .call_method(activity.as_obj(), "shellOverlayResized", "()Z", &[])?
                        .z()?
                    {
                        if let Some(window) = backend.overlay.borrow().as_ref() {
                            let density = env
                                .call_method(activity.as_obj(), "shellOverlayDensity", "()F", &[])?
                                .f()?;
                            window.resize(density);
                            if let Some((sink, ..)) = backend.overlay_front.borrow().as_ref() {
                                sink(HostEvent::Resized);
                            }
                        }
                    }
                    let keys = env
                        .call_method(activity.as_obj(), "shellTakeOverlayKeys", "()[I", &[])?
                        .l()?;
                    let keys = JIntArray::from(keys);
                    let mut flat = vec![0; env.get_array_length(&keys)? as usize];
                    env.get_int_array_region(&keys, 0, &mut flat)?;
                    for key in flat.chunks_exact(3) {
                        if !shell.overlay_shown() {
                            continue;
                        }
                        if let Some(events) =
                            input.borrow_mut().key(key[0], key[1] as u32, key[2] != 0)
                        {
                            shell.handle(Event::Native(events));
                        }
                    }
                    let axes = env
                        .call_method(activity.as_obj(), "shellTakeOverlayAxes", "()[I", &[])?
                        .l()?;
                    let axes = JIntArray::from(axes);
                    let mut flat = vec![0; env.get_array_length(&axes)? as usize];
                    env.get_int_array_region(&axes, 0, &mut flat)?;
                    for sample in flat.chunks_exact(5) {
                        if shell.overlay_shown() {
                            let events = input.borrow_mut().axes(
                                sample[0],
                                sample[1] as f32 / 1000.0,
                                sample[2] as f32 / 1000.0,
                                sample[3] as f32 / 1000.0,
                                sample[4] as f32 / 1000.0,
                            );
                            shell.handle(Event::Native(events));
                        }
                    }
                    Ok(())
                });
                if result.is_err() {
                    // JNI exceptions can contain credentials. Do not format them.
                    let _ = env.exception_clear();
                    eprintln!("Overlay Surface bridge failed");
                }
            }
            for event in rx.try_iter() {
                shell.handle(event);
            }
            let rendered = backend
                .overlay
                .borrow()
                .as_ref()
                .map(|window| window.render_if_needed());
            match rendered {
                Some(Ok(true)) => {
                    if let Some((sink, frames, first)) = backend.overlay_front.borrow_mut().as_mut()
                    {
                        frames.add();
                        if std::mem::take(first) {
                            sink(HostEvent::FirstFrame);
                            sink(HostEvent::Resized);
                        }
                    }
                }
                Some(Err(error)) => {
                    eprintln!("Overlay Surface rendering failed: {error}");
                    backend.release_overlay(&mut shell);
                    if let Ok(mut env) = vm.attach_current_thread() {
                        let _ = env.call_method(
                            activity.as_obj(),
                            "shellOverlayReleased",
                            "(Z)V",
                            &[JValue::Bool(1)],
                        );
                    }
                }
                _ => (),
            }
            if last_device_poll.elapsed() >= Duration::from_millis(100) {
                last_device_poll = Instant::now();
                if let Ok(mut env) = vm.attach_current_thread() {
                    let removed: jni::errors::Result<Vec<i32>> = env.with_local_frame(4, |env| {
                        let array = env
                            .call_method(
                                activity.as_obj(),
                                "shellTakeRemovedInputDevices",
                                "()[I",
                                &[],
                            )?
                            .l()?;
                        let array = JIntArray::from(array);
                        let mut ids = vec![0; env.get_array_length(&array)? as usize];
                        env.get_int_array_region(&array, 0, &mut ids)?;
                        Ok(ids)
                    });
                    if let Ok(ids) = removed {
                        for id in ids {
                            shell.handle(Event::Native(vec![input.borrow_mut().removed(id)]));
                        }
                    }
                }
            }
            if frames.get() > last_frame {
                last_frame = frames.get();
                // Read the rendered surface, not a test-only input shortcut.
                let pico = shell.current_id() == Some("pico");
                let home = shell.surface_property("home");
                let number = |name: &str| {
                    let value = if pico {
                        match &home {
                            Some(Value::Struct(home)) => home.get_field(name).cloned(),
                            _ => None,
                        }
                    } else {
                        shell.surface_property(name)
                    };
                    match value {
                        Some(Value::Number(n)) => n as i32,
                        _ => 0,
                    }
                };
                if let Ok(mut env) = vm.attach_current_thread() {
                    let _: jni::errors::Result<()> = env.with_local_frame(4, |env| {
                        let surface: JObject =
                            env.new_string(if pico { "pico" } else { "zone" })?.into();
                        let page = match shell.surface_property("page") {
                            Some(Value::String(page)) => page.to_string(),
                            _ => String::new(),
                        };
                        let page: JObject = env.new_string(page)?.into();
                        env.call_method(
                            activity.as_obj(),
                            "shellRendered",
                            "(Ljava/lang/String;Ljava/lang/String;II)V",
                            &[
                                JValue::Object(&surface),
                                JValue::Object(&page),
                                JValue::Int(number("total")),
                                JValue::Int(number(if pico {
                                    "selected-index"
                                } else {
                                    "focused-card"
                                })),
                            ],
                        )?;
                        Ok(())
                    });
                }
            }
        },
    );
    let result = slint::run_event_loop_until_quit().map_err(|error| {
        eprintln!("Korri Android event loop failed: {error}");
        anyhow::anyhow!("event loop failed")
    });
    // Timer owns Shell: drop its watch, logic and request workers before the
    // next activity's android_main starts. No process-global shell/Activity.
    drop(timer);
    if let Some(window) = teardown_backend.overlay.borrow_mut().take() {
        window.release()?;
    }
    if let Ok(mut env) = teardown_vm.attach_current_thread() {
        let _ = env.call_method(
            teardown_activity.as_obj(),
            "shellOverlayReleased",
            "(Z)V",
            &[JValue::Bool(1)],
        );
    }
    result
}
