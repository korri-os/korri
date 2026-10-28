//! Artemis is an effect adapter for korrid's exact selected launch.
use crate::linux_viewer::{LinuxStreamViewer, LinuxViewerError, LinuxViewerStatus};
use futures::future::BoxFuture;
use jni::{
    objects::{GlobalRef, JClass, JValue},
    JNIEnv, JavaVM,
};
use std::sync::Arc;

const CONTROL: &str = "(Ljava/lang/String;)Ljava/lang/String;";
const START: &str = "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;";

struct AndroidViewer {
    vm: JavaVM,
    class: GlobalRef,
}

impl AndroidViewer {
    fn invoke(
        &self,
        method: &str,
        launch_id: &str,
        address: Option<&str>,
    ) -> Result<LinuxViewerStatus, LinuxViewerError> {
        let unavailable = || LinuxViewerError::new("Android exact-launch viewer effect failed");
        let mut env = self.vm.attach_current_thread().map_err(|_| unavailable())?;
        // TLS can attach this worker permanently. Local references still have
        // a bounded lifetime, including on an exception from the platform.
        let result = env.with_local_frame(8, |env| -> Result<String, jni::errors::Error> {
            let class: &JClass<'_> = self.class.as_obj().into();
            let launch = env.new_string(launch_id)?;
            let mut arguments = vec![JValue::Object(launch.as_ref())];
            let address = address.map(|address| env.new_string(address)).transpose()?;
            if let Some(address) = &address {
                arguments.push(JValue::Object(address.as_ref()));
            }
            let signature = if address.is_some() { START } else { CONTROL };
            let value = env
                .call_static_method(class, method, signature, &arguments)?
                .l()?;
            let value = jni::objects::JString::from(value);
            let value = String::from(env.get_string(&value)?);
            Ok(value)
        });
        if result.is_err() && env.exception_check().unwrap_or(false) {
            // Do not print platform exceptions, addresses or certificate data.
            let _ = env.exception_clear();
        }
        match result.map_err(|_| unavailable())?.as_str() {
            "Running" => Ok(LinuxViewerStatus::Running),
            "Backgrounded" => Ok(LinuxViewerStatus::Backgrounded),
            "Stopped" => Ok(LinuxViewerStatus::Stopped),
            _ => Err(unavailable()),
        }
    }
}

struct Adapter(Arc<AndroidViewer>);
impl Adapter {
    async fn observe(
        &self,
        method: &'static str,
        launch: &str,
        address: Option<&str>,
    ) -> Result<LinuxViewerStatus, LinuxViewerError> {
        let viewer = self.0.clone();
        let launch = launch.to_owned();
        let address = address.map(str::to_owned);
        tokio::task::spawn_blocking(move || viewer.invoke(method, &launch, address.as_deref()))
            .await
            .map_err(|_| LinuxViewerError::new("Android viewer worker failed"))?
    }

    async fn confirm(
        &self,
        method: &'static str,
        launch: &str,
        address: Option<&str>,
        expected: LinuxViewerStatus,
    ) -> Result<(), LinuxViewerError> {
        if self.observe(method, launch, address).await? == expected {
            Ok(())
        } else {
            Err(LinuxViewerError::new(
                "Android viewer did not confirm the requested state",
            ))
        }
    }
}

impl LinuxStreamViewer for Adapter {
    fn start<'a>(
        &'a self,
        launch: &'a str,
        address: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(self.confirm(
            "viewerStart",
            launch,
            Some(address),
            LinuxViewerStatus::Running,
        ))
    }
    fn status<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<LinuxViewerStatus, LinuxViewerError>> {
        Box::pin(self.observe("viewerStatus", launch, None))
    }
    fn background<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(self.confirm(
            "viewerBackground",
            launch,
            None,
            LinuxViewerStatus::Backgrounded,
        ))
    }
    fn resume_and_focus<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(self.confirm("viewerResume", launch, None, LinuxViewerStatus::Running))
    }
    fn stop<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(self.confirm("viewerStop", launch, None, LinuxViewerStatus::Stopped))
    }
}

pub(super) fn adapter(
    env: &mut JNIEnv<'_>,
    class: &JClass<'_>,
) -> Result<Arc<dyn LinuxStreamViewer>, jni::errors::Error> {
    env.get_static_method_id(class, "viewerStart", START)?;
    for method in [
        "viewerStatus",
        "viewerBackground",
        "viewerResume",
        "viewerStop",
    ] {
        env.get_static_method_id(class, method, CONTROL)?;
    }
    Ok(Arc::new(Adapter(Arc::new(AndroidViewer {
        vm: env.get_java_vm()?,
        class: env.new_global_ref(class)?,
    }))))
}
