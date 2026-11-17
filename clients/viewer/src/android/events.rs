// SPDX-License-Identifier: GPL-3.0-or-later
//! An app's input events, as the stream's input takes them (`input.rs`,
//! slice D4): android-activity's key and motion events become plain values,
//! and the app's thread hands them over. The mapping itself, Artemis's, is
//! in `input.rs`. Both the viewer's own app and the Korri app (whose events
//! come through the shell's input filter) use these.
use crate::{
    gestures::GestureConfig,
    input::{self as stream, HistorySample, Pointer, axis},
};
use android_activity::{
    AndroidApp,
    input::{Axis, KeyAction, KeyEvent, MotionEvent},
};
use jni::{
    JNIEnv, JavaVM,
    objects::{JObject, JValue},
};

/// android-activity's times are `System.nanoTime()`'s; the input's are
/// milliseconds of the same clock.
fn ms(nanos: i64) -> i64 {
    nanos / 1_000_000
}

pub fn key(event: &KeyEvent) -> stream::KeyEvent {
    stream::KeyEvent {
        device: event.device_id(),
        source: event.source().into(),
        action: match event.action() {
            KeyAction::Down => stream::KeyAction::Down,
            KeyAction::Up => stream::KeyAction::Up,
            _ => stream::KeyAction::Multiple,
        },
        key_code: event.key_code().into(),
        scan_code: event.scan_code(),
        meta_state: event.meta_state().0,
        // android-activity gives no key flags: no key is Android's virtual
        // navigation key or a soft keyboard's (`clients/viewer/README.md`).
        flags: 0,
        repeat_count: event.repeat_count(),
        event_time_ms: ms(event.event_time()),
        down_time_ms: ms(event.down_time()),
    }
}

pub fn motion(event: &MotionEvent) -> stream::MotionEvent {
    let pointers: Vec<Pointer> = event
        .pointers()
        .map(|p| Pointer {
            id: p.pointer_id(),
            tool_type: p.tool_type().into(),
            x: p.x(),
            y: p.y(),
            pressure: p.pressure(),
            touch_major: p.touch_major(),
            touch_minor: p.touch_minor(),
            tool_major: p.tool_major(),
            tool_minor: p.tool_minor(),
            orientation: p.orientation(),
            distance: p.axis_value(Axis::Distance),
        })
        .collect();
    let mut axes = [0.0; axis::COUNT];
    let mut history = Vec::new();
    if event.pointer_count() > 0 {
        let first = event.pointer_at_index(0);
        for (id, value) in axes.iter_mut().enumerate() {
            *value = first.axis_value(Axis::from(id as u32));
        }
        // The earlier places of a batched move, for the absolute mouse.
        history = first
            .history()
            .map(|sample| HistorySample {
                time_ms: ms(sample.event_time()),
                positions: Vec::new(),
            })
            .collect();
        for p in event.pointers() {
            for (at, sample) in p.history().enumerate() {
                if let Some(entry) = history.get_mut(at) {
                    entry.positions.push((sample.x(), sample.y()));
                }
            }
        }
    }
    stream::MotionEvent {
        device: event.device_id(),
        source: event.source().into(),
        action: event.action().into(),
        action_index: event.pointer_index(),
        button_state: event.button_state().0,
        flags: event.flags().0,
        event_time_ms: ms(event.event_time()),
        pointers,
        history,
        axes,
    }
}

/// The device's `ViewConfiguration` for pan and zoom's gestures, through
/// JNI on this thread; Android's defaults at the display's density when it
/// cannot be read.
pub fn gesture_config(app: &AndroidApp) -> GestureConfig {
    let read = (|| -> anyhow::Result<GestureConfig> {
        // SAFETY: android-activity's VM, alive for the process, and its
        // global reference to the activity, valid on this thread until it
        // handles Destroy.
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }?;
        let mut env = vm.attach_current_thread()?;
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
        let config = env.with_local_frame(8, |env| -> jni::errors::Result<GestureConfig> {
            let view = env
                .call_static_method(
                    "android/view/ViewConfiguration",
                    "get",
                    "(Landroid/content/Context;)Landroid/view/ViewConfiguration;",
                    &[JValue::Object(&activity)],
                )?
                .l()?;
            let int = |env: &mut JNIEnv, name: &str| -> jni::errors::Result<i32> {
                env.call_method(&view, name, "()I", &[])?.i()
            };
            let touch_slop = int(env, "getScaledTouchSlop")?;
            let double_tap_slop = int(env, "getScaledDoubleTapSlop")?;
            let min_span = int(env, "getScaledMinimumScalingSpan")?;
            let long_press_ms = env
                .call_static_method(
                    "android/view/ViewConfiguration",
                    "getLongPressTimeout",
                    "()I",
                    &[],
                )?
                .i()?;
            let double_tap_ms = env
                .call_static_method(
                    "android/view/ViewConfiguration",
                    "getDoubleTapTimeout",
                    "()I",
                    &[],
                )?
                .i()?;
            Ok(GestureConfig {
                touch_slop,
                double_tap_slop,
                min_span,
                long_press_ms: long_press_ms.into(),
                double_tap_ms: double_tap_ms.into(),
            })
        });
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        Ok(config?)
    })();
    match read {
        Ok(config) => {
            crate::log::line(&format!("input: the gestures of pan and zoom: {config:?}"));
            config
        }
        Err(error) => {
            crate::log::line(&format!(
                "input: no ViewConfiguration ({error:#}); pan and zoom takes Android's defaults at 160 dpi"
            ));
            // 8 dp, 100 dp and 27 mm at 160 dpi.
            GestureConfig {
                touch_slop: 8,
                double_tap_slop: 100,
                min_span: 170,
                long_press_ms: 400,
                double_tap_ms: 300,
            }
        }
    }
}

/// Artemis's `requestUnbufferedDispatch` for every input class it takes:
/// without it Android batches input to the display's refresh. On the
/// window's decor view, whose root dispatches NativeActivity's input queue
/// too; that it takes effect there is not proven. Posted to Android's main
/// thread, which views require; nothing waits for it.
pub fn unbuffered(app: &AndroidApp) {
    // InputDevice.SOURCE_CLASS_BUTTON | POINTER | TRACKBALL | POSITION | JOYSTICK.
    const CLASSES: i32 = 0x01 | 0x02 | 0x04 | 0x08 | 0x10;
    let vm = app.vm_as_ptr() as usize;
    // Our own reference to the activity, taken here: android-activity's may
    // be gone by the time the main thread runs the closure.
    let activity = (|| -> anyhow::Result<_> {
        // SAFETY: android-activity's VM, alive for the process, and its
        // global reference to the activity, valid on this thread until it
        // handles Destroy.
        let vm = unsafe { JavaVM::from_raw(vm as *mut jni::sys::JavaVM) }?;
        let env = vm.attach_current_thread()?;
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
        Ok(env.new_global_ref(activity)?)
    })();
    let activity = match activity {
        Ok(activity) => activity,
        Err(error) => {
            crate::log::line(&format!("input: unbuffered dispatch: {error:#}"));
            return;
        }
    };
    app.run_on_java_main_thread(Box::new(move || {
        let done = (|| -> anyhow::Result<()> {
            // SAFETY: android-activity's VM, alive for the process.
            let vm = unsafe { JavaVM::from_raw(vm as *mut jni::sys::JavaVM) }?;
            let mut env = vm.attach_current_thread()?;
            let result = env.with_local_frame(8, |env| {
                let window = env
                    .call_method(
                        activity.as_obj(),
                        "getWindow",
                        "()Landroid/view/Window;",
                        &[],
                    )?
                    .l()?;
                let decor = env
                    .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
                    .l()?;
                env.call_method(
                    &decor,
                    "requestUnbufferedDispatch",
                    "(I)V",
                    &[CLASSES.into()],
                )?;
                Ok::<_, jni::errors::Error>(())
            });
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_describe();
                let _ = env.exception_clear();
            }
            Ok(result?)
        })();
        match done {
            Ok(()) => crate::log::line("input: unbuffered dispatch requested"),
            Err(error) => crate::log::line(&format!("input: unbuffered dispatch: {error:#}")),
        }
    }));
}
