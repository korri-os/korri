//! The Artemis app's host: `KorriShellActivity` (Java, clients/android). It
//! starts korrid and unpacks the packages before `android_main`, and the
//! shell reads both from it through JNI calls on the activity. Its overlay
//! Surface comes from `KorriSurfaceHost`, which Artemis's `Game` feeds.
use super::{Bootstrap, Host, Overlay, Rendered};
use crate::config::Connection;
use jni::{
    objects::{GlobalRef, JFloatArray, JIntArray, JObject, JValue},
    JNIEnv, JavaVM,
};
use slint::android::AndroidApp;
use std::{path::PathBuf, sync::Arc};

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
    super::run(
        app,
        Bootstrap {
            connection: Connection::new(port as u16, origin, capability),
            packages: PathBuf::from(package),
        },
        Box::new(Activity { vm, activity }),
    )
}

struct Activity {
    vm: Arc<JavaVM>,
    activity: GlobalRef,
}

impl Activity {
    fn ints(&self, env: &mut JNIEnv, method: &str) -> jni::errors::Result<Vec<i32>> {
        let array = env
            .call_method(self.activity.as_obj(), method, "()[I", &[])?
            .l()?;
        let array = JIntArray::from(array);
        let mut flat = vec![0; env.get_array_length(&array)? as usize];
        env.get_int_array_region(&array, 0, &mut flat)?;
        Ok(flat)
    }
}

impl Host for Activity {
    // Android's device-removal callback has no android-activity equivalent;
    // the activity's listener queues the removals.
    fn removed_input_devices(&mut self) -> Vec<i32> {
        let Ok(mut env) = self.vm.attach_current_thread() else {
            return Vec::new();
        };
        env.with_local_frame(4, |env| self.ints(env, "shellTakeRemovedInputDevices"))
            .unwrap_or_default()
    }

    fn rendered(&mut self, rendered: &Rendered) {
        if let Ok(mut env) = self.vm.attach_current_thread() {
            let _: jni::errors::Result<()> = env.with_local_frame(4, |env| {
                let surface: JObject = env.new_string(&rendered.surface)?.into();
                let page: JObject = env.new_string(&rendered.page)?.into();
                env.call_method(
                    self.activity.as_obj(),
                    "shellRendered",
                    "(Ljava/lang/String;Ljava/lang/String;II)V",
                    &[
                        JValue::Object(&surface),
                        JValue::Object(&page),
                        JValue::Int(rendered.count),
                        JValue::Int(rendered.selected),
                    ],
                )?;
                Ok(())
            });
        }
    }

    fn overlay(&mut self) -> Option<&mut dyn Overlay> {
        Some(self)
    }
}

// Host-neutral Surface lifecycle. Rust releases its EGL/ANativeWindow before ack.
impl Overlay for Activity {
    fn closing(&mut self, env: &mut JNIEnv) -> jni::errors::Result<bool> {
        env.call_method(self.activity.as_obj(), "shellOverlayClosing", "()Z", &[])?
            .z()
    }

    fn released(&mut self, env: &mut JNIEnv, failed: bool) -> jni::errors::Result<()> {
        env.call_method(
            self.activity.as_obj(),
            "shellOverlayReleased",
            "(Z)V",
            &[JValue::Bool(u8::from(failed))],
        )?;
        Ok(())
    }

    fn take_surface<'local>(
        &mut self,
        env: &mut JNIEnv<'local>,
    ) -> jni::errors::Result<JObject<'local>> {
        env.call_method(
            self.activity.as_obj(),
            "shellTakeOverlaySurface",
            "()Landroid/view/Surface;",
            &[],
        )?
        .l()
    }

    fn density(&mut self, env: &mut JNIEnv) -> jni::errors::Result<f32> {
        env.call_method(self.activity.as_obj(), "shellOverlayDensity", "()F", &[])?
            .f()
    }

    fn resized(&mut self, env: &mut JNIEnv) -> jni::errors::Result<bool> {
        env.call_method(self.activity.as_obj(), "shellOverlayResized", "()Z", &[])?
            .z()
    }

    fn take_keys(&mut self, env: &mut JNIEnv) -> jni::errors::Result<Vec<i32>> {
        self.ints(env, "shellTakeOverlayKeys")
    }

    fn take_axes(&mut self, env: &mut JNIEnv) -> jni::errors::Result<Vec<i32>> {
        self.ints(env, "shellTakeOverlayAxes")
    }

    fn take_pointers(&mut self, env: &mut JNIEnv) -> jni::errors::Result<Vec<f32>> {
        let array = env
            .call_method(
                self.activity.as_obj(),
                "shellTakeOverlayPointers",
                "()[F",
                &[],
            )?
            .l()?;
        let array = JFloatArray::from(array);
        let mut flat = vec![0.0; env.get_array_length(&array)? as usize];
        env.get_float_array_region(&array, 0, &mut flat)?;
        Ok(flat)
    }
}
