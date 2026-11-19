// SPDX-License-Identifier: GPL-3.0-or-later
//! The emulator proof's trigger for the stream's controls (#434, slice
//! D11). Only the fixture APK carries it: its library is built with the
//! cargo feature `proof`, and the product library's build checks that none
//! of this is in it (`default.nix`).
//!
//! korrid reads the viewer's snapshot itself while a stream runs (its
//! live-shape poll), but only its gameplay overlay calls Apply (the shell's
//! `InvokeGameplayControl`), and in this app that overlay comes with slice
//! D5b. Until then the proof writes one command at a time into
//! `<cache>/proof-command` with `run-as`, and this thread runs it against
//! korrid's own viewer adapter, the object that korrid holds
//! (`viewer::korrids_adapter`), and logs the answer as the treaty's JSON:
//!
//! | Command | Call |
//! | --- | --- |
//! | `snapshot LAUNCH` | `LinuxStreamViewer::snapshot` |
//! | `apply LAUNCH CONTROL toggle true\|false` | `LinuxStreamViewer::apply` with a toggle |
//! | `apply LAUNCH CONTROL choice VALUE` | with a choice |
//! | `apply LAUNCH CONTROL range NUMBER` | with a range value |
//! | `apply LAUNCH CONTROL none` | with no value (a command) |
//! | `resize LAUNCH WIDTH HEIGHT` | korrid's resize request to the holder (#476 slice 3, `android_app::proof_resize_stream`) |
//! | `display WIDTH HEIGHT`, `display real` | the display's size that the loop thread reads from now on, in the place of Android's, or Android's again (#509, [`display`]) |
//!
//! The resize answer is the holder's: `Applied`, `Refused` or `Unavailable`.
//! It stands in for korrid's Portrait control, which asks the same request
//! only once the viewer reports that it can present a new size.
//!
//! `display` stands in for a phone's other panel. The emulator's fold keeps
//! its one panel and its mode, and only forces another logical size, so the
//! display's physical size, which the viewer reports as its screen
//! (`korri_viewer::status::screen`), never changes there. Everything after
//! that reading is the product's: the settle time, the lifecycle's shape,
//! and korrid's reads of it.
//!
//! The viewer streams with its default network core, nightjar-core (#391
//! step E). A file `<cache>/proof-network-core` that holds `c` makes the
//! next streams use the C core, as `-networkcore c` does on Linux: the proof
//! measures the fixture APK with each core (`network_core`). korrid's launch
//! names no core, so the product has no such choice.
use crate::{logcat, viewer};
use korri_viewer_treaty::SurfaceGameplayControlValue as Value;
use korrid::linux_viewer::LinuxStreamViewer;
use std::{
    path::PathBuf,
    sync::{Mutex, Once, PoisonError},
    thread,
    time::Duration,
};

const COMMAND: &str = "proof-command";
const NETWORK_CORE: &str = "proof-network-core";
const LOG: &str = "korri-app: proof:";

/// The cache directory that `watch` was given.
static CACHE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The network core that `<cache>/proof-network-core` names for the next
/// stream, or none for the viewer's default. A name that the viewer does not
/// know is logged and left out.
pub fn network_core() -> Option<korri_viewer::network_core::Kind> {
    let name = std::fs::read_to_string(CACHE.get()?.join(NETWORK_CORE)).ok()?;
    match korri_viewer::network_core::Kind::parse(name.trim()) {
        Ok(kind) => {
            logcat::info(&format!(
                "{LOG} network core {} for this stream",
                name.trim()
            ));
            Some(kind)
        }
        Err(cause) => {
            logcat::error(&format!("{LOG} network core {}: {cause:#}", name.trim()));
            None
        }
    }
}

/// The display's size that the `display` command gave.
static DISPLAY: Mutex<Option<(i32, i32)>> = Mutex::new(None);

/// The display's size that the proof gave in the place of Android's
/// reading (`display WIDTH HEIGHT`), or none for Android's own.
pub fn display() -> Option<(i32, i32)> {
    *DISPLAY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The activity's app, for the input method's commands (DIAGNOSIS ONLY).
static APP: std::sync::OnceLock<slint::android::AndroidApp> = std::sync::OnceLock::new();

/// Watch for the proof's commands, once per process.
pub fn watch(cache: PathBuf, app: &slint::android::AndroidApp) {
    let _ = APP.set(app.clone());
    let _ = CACHE.set(cache.clone());
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        let path = cache.join(COMMAND);
        let spawned = thread::Builder::new()
            .name("korri-proof".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(cause) => {
                        logcat::error(&format!("{LOG} no runtime: {cause}"));
                        return;
                    }
                };
                logcat::info(&format!("{LOG} watching {}", path.display()));
                loop {
                    thread::sleep(Duration::from_millis(250));
                    let Ok(text) = std::fs::read_to_string(&path) else {
                        continue;
                    };
                    let _ = std::fs::remove_file(&path);
                    run(&runtime, text.trim());
                }
            });
        if let Err(cause) = spawned {
            logcat::error(&format!("{LOG} the watcher did not start: {cause}"));
        }
    });
}

fn run(runtime: &tokio::runtime::Runtime, command: &str) {
    if let ["resize", launch, width, height] =
        command.split_whitespace().collect::<Vec<_>>().as_slice()
    {
        let (Ok(width), Ok(height)) = (width.parse(), height.parse()) else {
            return logcat::error(&format!("{LOG} not a size: {command}"));
        };
        let display = korrid::launcher::plugin_launch::PluginDisplay { width, height };
        return match korrid::android_app::proof_resize_stream(launch, display) {
            Ok(answer) => logcat::info(&format!(
                "{LOG} resize {launch} {width}x{height} {answer:?}"
            )),
            Err(cause) => logcat::error(&format!(
                "{LOG} resize {launch} {width}x{height} failed: {cause}"
            )),
        };
    }
    if let ["ime", what] = command.split_whitespace().collect::<Vec<_>>().as_slice() {
        return ime(what);
    }
    if let ["display", size @ ..] = command.split_whitespace().collect::<Vec<_>>().as_slice() {
        let display = match size {
            ["real"] => None,
            [width, height] => match (width.parse(), height.parse()) {
                (Ok(width), Ok(height)) => Some((width, height)),
                _ => return logcat::error(&format!("{LOG} not a size: {command}")),
            },
            _ => return logcat::error(&format!("{LOG} no such display: {command}")),
        };
        *DISPLAY.lock().unwrap_or_else(PoisonError::into_inner) = display;
        return logcat::info(&format!("{LOG} {command} set"));
    }
    let Some(adapter) = viewer::korrids_adapter() else {
        return logcat::error(&format!("{LOG} korrid holds no viewer adapter yet"));
    };
    let words: Vec<&str> = command.split_whitespace().collect();
    match words.as_slice() {
        ["snapshot", launch] => match runtime.block_on(adapter.snapshot(launch)) {
            Ok(snapshot) => logcat::info(&format!(
                "{LOG} snapshot {launch} {}",
                serde_json::to_string(&snapshot).unwrap_or_default()
            )),
            Err(cause) => logcat::error(&format!("{LOG} snapshot {launch} failed: {cause}")),
        },
        ["apply", launch, control, value @ ..] => {
            let value = match value {
                ["toggle", "true"] => Some(Value::Toggle { value: true }),
                ["toggle", "false"] => Some(Value::Toggle { value: false }),
                ["choice", choice] => Some(Value::Choice {
                    value: (*choice).to_owned(),
                }),
                ["range", number] => match number.parse() {
                    Ok(value) => Some(Value::Range { value }),
                    Err(_) => return logcat::error(&format!("{LOG} not a number: {command}")),
                },
                ["none"] => None,
                _ => return logcat::error(&format!("{LOG} no such value: {command}")),
            };
            let asked = serde_json::to_string(&value).unwrap_or_default();
            match runtime.block_on(adapter.apply(launch, control, value)) {
                Ok(()) => logcat::info(&format!("{LOG} apply {launch} {control} {asked} ok")),
                Err(cause) => logcat::error(&format!(
                    "{LOG} apply {launch} {control} {asked} failed: {cause}"
                )),
            }
        }
        _ => logcat::error(&format!("{LOG} no such command: {command}")),
    }
}

/// DIAGNOSIS ONLY: the input method's steps, one at a time, on Android's
/// main thread. `toggle` and `give-back` are the viewer's own calls;
/// `layout` asks the window for one traversal; `callback-off` takes the
/// insets animation callback off Slint's text view; `state` logs the decor
/// view (its flags) and whether the IME's insets are visible.
fn ime(what: &str) {
    use jni::{objects::JValue, JavaVM};
    let Some(app) = APP.get() else {
        return logcat::error(&format!("{LOG} ime {what}: no app"));
    };
    match what {
        "toggle" => {
            let answer = korri_viewer::soft_keyboard::toggle(app);
            let got = answer.recv_timeout(Duration::from_secs(5));
            return logcat::info(&format!("{LOG} ime toggle {got:?}"));
        }
        "give-back" => {
            korri_viewer::soft_keyboard::give_back(app);
            return logcat::info(&format!("{LOG} ime give-back posted"));
        }
        "layout" | "callback-off" | "state" => {}
        _ => return logcat::error(&format!("{LOG} ime {what}: no such step")),
    }
    let vm = app.vm_as_ptr() as usize;
    let activity = (|| -> anyhow::Result<jni::objects::GlobalRef> {
        let vm = unsafe { JavaVM::from_raw(vm as *mut jni::sys::JavaVM) }?;
        let env = vm.attach_current_thread()?;
        let activity =
            unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
        Ok(env.new_global_ref(activity)?)
    })();
    let Ok(activity) = activity else {
        return logcat::error(&format!("{LOG} ime {what}: no activity"));
    };
    let what = what.to_owned();
    app.run_on_java_main_thread(Box::new(move || {
        let Ok(vm) = (unsafe { JavaVM::from_raw(vm as *mut jni::sys::JavaVM) }) else {
            return;
        };
        let Ok(mut env) = vm.attach_current_thread() else {
            return;
        };
        let done = env.with_local_frame(32, |env| -> jni::errors::Result<String> {
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
            match what.as_str() {
                "layout" => {
                    env.call_method(&decor, "requestLayout", "()V", &[])?;
                    Ok("requested".into())
                }
                "callback-off" => {
                    let content = env
                        .call_method(
                            activity.as_obj(),
                            "findViewById",
                            "(I)Landroid/view/View;",
                            &[0x0102_0002i32.into()],
                        )?
                        .l()?;
                    let count = env
                        .call_method(&content, "getChildCount", "()I", &[])?
                        .i()?;
                    let mut cleared = Vec::new();
                    for index in 0..count {
                        let child = env
                            .call_method(
                                &content,
                                "getChildAt",
                                "(I)Landroid/view/View;",
                                &[index.into()],
                            )?
                            .l()?;
                        let class = env
                            .call_method(&child, "getClass", "()Ljava/lang/Class;", &[])?
                            .l()?;
                        let name = env
                            .call_method(&class, "getName", "()Ljava/lang/String;", &[])?
                            .l()?;
                        let name: String = env.get_string(&name.into())?.into();
                        if name.ends_with("SlintInputView") {
                            env.call_method(
                                &child,
                                "setWindowInsetsAnimationCallback",
                                "(Landroid/view/WindowInsetsAnimation$Callback;)V",
                                &[JValue::Object(&jni::objects::JObject::null())],
                            )?;
                            cleared.push(name);
                        }
                    }
                    Ok(format!("cleared {cleared:?}"))
                }
                _ => {
                    let text = env
                        .call_method(&decor, "toString", "()Ljava/lang/String;", &[])?
                        .l()?;
                    let text: String = env.get_string(&text.into())?.into();
                    let laid = env
                        .call_method(&decor, "isLayoutRequested", "()Z", &[])?
                        .z()?;
                    let insets = env
                        .call_method(
                            &decor,
                            "getRootWindowInsets",
                            "()Landroid/view/WindowInsets;",
                            &[],
                        )?
                        .l()?;
                    let ime = env
                        .call_static_method("android/view/WindowInsets$Type", "ime", "()I", &[])?
                        .i()?;
                    let visible = if insets.is_null() {
                        None
                    } else {
                        Some(
                            env.call_method(&insets, "isVisible", "(I)Z", &[ime.into()])?
                                .z()?,
                        )
                    };
                    Ok(format!(
                        "decor {text} layout requested {laid} ime visible {visible:?}"
                    ))
                }
            }
        });
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        match done {
            Ok(said) => logcat::info(&format!("{LOG} ime {what} {said}")),
            Err(cause) => logcat::error(&format!("{LOG} ime {what} failed: {cause}")),
        }
    }));
}
