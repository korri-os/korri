//! ADR 0054: the Android edge for games that run on this device. korrid
//! decides; `KorriAppLauncher.java` only performs the platform calls.
use crate::android_local::AppLauncher;
use crate::launcher::plugin_launch::AndroidLaunchOutput;
use jni::{
    objects::{GlobalRef, JClass, JValue},
    JNIEnv, JavaVM,
};

const CLASS: &str = "com/simonwjackson/korri/korrid/KorriAppLauncher";

pub struct JniAppLauncher {
    vm: JavaVM,
    class: GlobalRef,
}

pub fn adapter(env: &mut JNIEnv) -> Result<JniAppLauncher, jni::errors::Error> {
    let class = env.find_class(CLASS)?;
    Ok(JniAppLauncher {
        vm: env.get_java_vm()?,
        class: env.new_global_ref(class)?,
    })
}

impl JniAppLauncher {
    /// Call a static `String m(String...)` method; "ok" means success.
    fn call(&self, method: &str, args: &[&str]) -> Result<String, String> {
        let mut env = self.vm.attach_current_thread().map_err(|e| e.to_string())?;
        let signature = format!(
            "({})Ljava/lang/String;",
            "Ljava/lang/String;".repeat(args.len())
        );
        let result = env.with_local_frame(8, |env| -> Result<String, jni::errors::Error> {
            let class: &JClass<'_> = self.class.as_obj().into();
            let strings = args
                .iter()
                .map(|a| env.new_string(a))
                .collect::<Result<Vec<_>, _>>()?;
            let values: Vec<JValue> = strings.iter().map(|s| JValue::Object(s.as_ref())).collect();
            let value = env
                .call_static_method(class, method, &signature, &values)?
                .l()?;
            let value = jni::objects::JString::from(value);
            let text = String::from(env.get_string(&value)?);
            Ok(text)
        });
        if result.is_err() && env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        result.map_err(|_| format!("Android {method} failed"))
    }
    fn ok(&self, method: &str, args: &[&str]) -> Result<(), String> {
        match self.call(method, args)?.as_str() {
            "ok" => Ok(()),
            message => Err(message.to_owned()),
        }
    }
}

impl AppLauncher for JniAppLauncher {
    fn is_installed(&self, package: &str) -> bool {
        self.call("isInstalled", &[package])
            .is_ok_and(|v| v == "yes")
    }
    fn start(&self, plan: &AndroidLaunchOutput, content_path: Option<&str>) -> Result<(), String> {
        let json = serde_json::to_string(&plan.android).map_err(|e| e.to_string())?;
        let saves = serde_json::to_string(&plan.save_locations).map_err(|e| e.to_string())?;
        self.ok("start", &[&json, content_path.unwrap_or(""), &saves])
    }
    fn resume(&self, package: &str) -> Result<(), String> {
        self.ok("resume", &[package])
    }
    fn show_korri(&self) -> Result<(), String> {
        self.ok("showKorri", &[])
    }
    fn end_session(&self, session: &crate::android_local::LocalSession) -> Result<(), String> {
        let saves = serde_json::to_string(&session.save_locations).map_err(|e| e.to_string())?;
        self.ok("endSession", &[&session.package, &saves])
    }
}
