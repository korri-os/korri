//! NativeActivity carries the same Shell, Pico package and app.surface.*
//! connection as Linux. Android owns the window and its native event loop.
use super::{Backend, FrameCounter, HostEvent, Placement};
use crate::{
    config::Connection,
    input::drag::Drag,
    korrid::Korrid,
    package::Package,
    shell::{Event, Options, Post, Shell},
};
use i_slint_backend_android_activity::{
    android_activity::{
        input::{InputEvent, MotionAction},
        MainEvent, PollEvent,
    },
    AndroidPlatform,
};
use jni::{
    objects::{JObject, JValue},
    JavaVM,
};
use slint::android::AndroidApp;
use slint_interpreter::Value;
use std::{
    cell::RefCell,
    path::Path,
    rc::Rc,
    sync::{mpsc, Arc},
    time::Duration,
};

type Sink = Rc<dyn Fn(HostEvent)>;
type Front = Rc<RefCell<Option<(Sink, FrameCounter, bool)>>>;

struct Android {
    front: Front,
    notifier_installed: std::cell::Cell<bool>,
}

impl Android {
    fn install(app: AndroidApp) -> anyhow::Result<Self> {
        let front: Front = Default::default();
        let events = front.clone();
        let touch = front.clone();
        let drag = RefCell::new(Drag::default());
        let touch_app = app.clone();
        let platform = AndroidPlatform::new_with_event_listener(app, move |event| {
            let host = match event {
                PollEvent::Main(MainEvent::GainedFocus) => Some(HostEvent::Focus(true)),
                PollEvent::Main(MainEvent::LostFocus) => Some(HostEvent::Focus(false)),
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
            // Controller/key mapping and taps are M2. Only single-finger drags
            // become momentary directional presses; no view hit-testing here.
            let InputEvent::MotionEvent(motion) = event else {
                return false;
            };
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
            notifier_installed: Default::default(),
        })
    }
}

impl Backend for Android {
    fn shared_window(&self) -> bool {
        true
    }

    fn attach(&self, window: &slint::Window, sink: Sink, frames: FrameCounter) {
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
    let backend = Rc::new(Android::install(app)?);
    let (tx, rx) = mpsc::channel();
    let mut shell = Shell::new(
        vec![Package::at(Path::new(&package))?],
        brain,
        backend,
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
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            for event in rx.try_iter() {
                shell.handle(event);
            }
            if frames.get() > last_frame {
                last_frame = frames.get();
                // Observe the actual bound Pico view, only after a rendered frame.
                // This has no bootstrap or model content beyond two public counts.
                if let Some(Value::Struct(home)) = shell.surface_property("home") {
                    let number = |name| match home.get_field(name) {
                        Some(Value::Number(n)) => *n as i32,
                        _ => 0,
                    };
                    if let Ok(mut env) = vm.attach_current_thread() {
                        let _ = env.call_method(
                            activity.as_obj(),
                            "shellRendered",
                            "(II)V",
                            &[
                                JValue::Int(number("total")),
                                JValue::Int(number("selected-index")),
                            ],
                        );
                    }
                }
            }
        },
    );
    slint::run_event_loop_until_quit().map_err(|error| {
        eprintln!("Korri Android event loop failed: {error}");
        anyhow::anyhow!("event loop failed")
    })?;
    // Timer owns Shell: drop its watch, logic and request workers before the
    // next activity's android_main starts. No process-global shell/Activity.
    drop(timer);
    Ok(())
}
