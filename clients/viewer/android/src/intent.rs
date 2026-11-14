// SPDX-License-Identifier: GPL-3.0-or-later
//! The extras of the intent that started the activity, read through JNI from
//! Android's own `Activity` and `Intent` classes. No Java of Korri's.
use android_activity::AndroidApp;
use anyhow::{Context, Result, anyhow};
use jni::{
    JNIEnv, JavaVM,
    objects::{JObject, JObjectArray, JString},
};

pub struct Extras {
    vm: JavaVM,
    activity: usize,
}

impl Extras {
    pub fn of(app: &AndroidApp) -> Result<Self> {
        // SAFETY: android-activity owns the VM and the activity's global
        // reference for the life of the activity, which outlives this value.
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
        Ok(Self {
            vm,
            activity: app.activity_as_ptr() as usize,
        })
    }

    pub fn string(&self, name: &str) -> Result<Option<String>> {
        self.read(|env, intent| {
            let key = env.new_string(name)?;
            let value = env
                .call_method(
                    intent,
                    "getStringExtra",
                    "(Ljava/lang/String;)Ljava/lang/String;",
                    &[(&key).into()],
                )?
                .l()?;
            if value.is_null() {
                return Ok(None);
            }
            Ok(Some(env.get_string(&JString::from(value))?.into()))
        })
        .with_context(|| format!("read the intent extra {name}"))
    }

    pub fn strings(&self, name: &str) -> Result<Option<Vec<String>>> {
        self.read(|env, intent| {
            let key = env.new_string(name)?;
            let value = env
                .call_method(
                    intent,
                    "getStringArrayExtra",
                    "(Ljava/lang/String;)[Ljava/lang/String;",
                    &[(&key).into()],
                )?
                .l()?;
            if value.is_null() {
                return Ok(None);
            }
            let array = JObjectArray::from(value);
            let mut strings = Vec::new();
            for index in 0..env.get_array_length(&array)? {
                let item = JString::from(env.get_object_array_element(&array, index)?);
                strings.push(env.get_string(&item)?.into());
            }
            Ok(Some(strings))
        })
        .with_context(|| format!("read the intent extra {name}"))
    }

    fn read<T>(
        &self,
        read: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<T>,
    ) -> Result<T> {
        let mut env = self.vm.attach_current_thread()?;
        let result = env.with_local_frame(16, |env| {
            // SAFETY: the activity's global reference, borrowed; not deleted here.
            let activity = unsafe { JObject::from_raw(self.activity as jni::sys::jobject) };
            let intent = env
                .call_method(&activity, "getIntent", "()Landroid/content/Intent;", &[])?
                .l()?;
            read(env, &intent)
        });
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        result.map_err(|error| anyhow!("{error}"))
    }
}
