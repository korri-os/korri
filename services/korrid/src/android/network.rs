//! Android's network APIs supply live evidence; Linux route dumps do not run here.
use crate::{
    federation::{
        interfaces::InterfaceAddress,
        reach::{Environment, RelayEnvironment, RelayFacts},
    },
    relay::RelayList,
};
use futures::future::BoxFuture;
use jni::{
    objects::{GlobalRef, JClass, JObject, JString, JValue},
    JNIEnv, JavaVM,
};
use serde::Deserialize;
use std::{io, sync::Arc};

#[derive(Deserialize)]
struct Snapshot {
    interfaces: Vec<InterfaceAddress>,
    #[serde(flatten)]
    facts: RelayFacts,
}
struct Reader {
    vm: JavaVM,
    class: GlobalRef,
    context: GlobalRef,
}
impl Reader {
    fn read(&self) -> io::Result<Snapshot> {
        let unavailable = || io::Error::other("Android network observations are unavailable");
        let mut env = self.vm.attach_current_thread().map_err(|_| unavailable())?;
        // TLS may have permanently attached this worker. Bound our temporary
        // references even when dropping AttachGuard will not detach it.
        let encoded = env.with_local_frame(8, |env| -> Result<String, jni::errors::Error> {
            let class: &JClass<'_> = self.class.as_obj().into();
            let value = env
                .call_static_method(
                    class,
                    "networkSnapshot",
                    "(Landroid/content/Context;)Ljava/lang/String;",
                    &[JValue::Object(self.context.as_obj())],
                )?
                .l()?;
            let value = JString::from(value);
            let encoded = String::from(env.get_string(&value)?);
            Ok(encoded)
        });
        if encoded.is_err() && env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        serde_json::from_str(&encoded.map_err(|_| unavailable())?).map_err(|_| unavailable())
    }
}
impl RelayEnvironment for Reader {
    fn facts(&self) -> BoxFuture<'_, io::Result<RelayFacts>> {
        Box::pin(async move { self.read().map(|snapshot| snapshot.facts) })
    }
}

pub(super) fn environment(
    env: &mut JNIEnv<'_>,
    class: &JClass<'_>,
    context: &JObject<'_>,
    relays: RelayList,
) -> Result<Environment, jni::errors::Error> {
    // Resolve in the application's loader while its JNI frame is live. Worker
    // threads retain neither borrowed JNI objects nor a thread-local JNIEnv.
    env.get_static_method_id(
        class,
        "networkSnapshot",
        "(Landroid/content/Context;)Ljava/lang/String;",
    )?;
    let reader = Arc::new(Reader {
        vm: env.get_java_vm()?,
        class: env.new_global_ref(class)?,
        context: env.new_global_ref(context)?,
    });
    let interfaces = reader.clone();
    Ok(Environment {
        interfaces: Arc::new(move || interfaces.read().map(|snapshot| snapshot.interfaces)),
        relay: reader,
        relays: Some(relays),
    })
}
