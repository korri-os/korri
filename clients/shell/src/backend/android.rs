//! NativeActivity entry point. M0 exercises the production interpreter and
//! QuickJS on Android without introducing an Android-only UI engine.
use crate::{config::Connection, korrid::Korrid};
use jni::{objects::JObject, JavaVM};
use slint::{android::AndroidApp, ComponentHandle};
use std::{sync::mpsc, time::Duration};

#[no_mangle]
fn android_main(app: AndroidApp) {
    // Never format JNI errors: they can contain arguments from a failed call.
    if run(app).is_err() {
        eprintln!("Korri native shell startup failed");
    }
}

fn run(app: AndroidApp) -> anyhow::Result<()> {
    // The VM and activity are borrowed from AndroidApp for this invocation.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let port = env.call_method(&activity, "shellPort", "()I", &[])?.i()?;
    let version = env
        .call_method(&activity, "shellVersion", "()Ljava/lang/String;", &[])?
        .l()?;
    let version: String = env.get_string(&version.into())?.into();
    let capability = env
        .call_method(&activity, "shellCapability", "()Ljava/lang/String;", &[])?
        .l()?;
    let capability: String = env.get_string(&capability.into())?.into();
    let brain = Korrid::new(Connection::new(port as u16, String::new(), capability));

    slint::android::init(app)?;
    let runtime = rquickjs::Runtime::new()?;
    let context = rquickjs::Context::full(&runtime)?;
    anyhow::ensure!(context.with(|ctx| ctx.eval::<i32, _>("21 * 2"))? == 42);

    let compiler = slint_interpreter::Compiler::default();
    let result = pollster::block_on(compiler.build_from_source(
        include_str!("android-m0.slint").into(),
        "android-m0.slint".into(),
    ));
    anyhow::ensure!(!result.has_errors(), "M0 view did not compile");
    let definition = result
        .component("AndroidM0")
        .ok_or_else(|| anyhow::anyhow!("M0 component is missing"))?;
    let ui = definition.create()?;
    ui.set_property(
        "app-version",
        slint_interpreter::Value::String(version.into()),
    )?;

    // RPC never blocks the Android event loop. Only a boolean crosses back to
    // the UI. Dropping the receiver stops the worker after its bounded request.
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || loop {
        if tx.send(port != 0 && brain.health()).is_err() {
            break;
        }
        std::thread::sleep(Duration::from_secs(2));
    });
    let weak = ui.as_weak();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(100),
        move || {
            for running in rx.try_iter() {
                if let Some(ui) = weak.upgrade() {
                    let _ = ui.set_property(
                        "brain-status",
                        slint_interpreter::Value::String(
                            if running {
                                "korrid is running"
                            } else {
                                "korrid is unavailable. Close and reopen Korri to retry."
                            }
                            .into(),
                        ),
                    );
                }
            }
        },
    );
    env.call_method(&activity, "shellStarted", "()V", &[])?;
    ui.run()?;
    Ok(())
}
