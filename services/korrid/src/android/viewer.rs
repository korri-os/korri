//! Artemis is an effect adapter for korrid's exact selected launch.
use crate::linux_viewer::{LinuxStreamViewer, LinuxViewerError, LinuxViewerStatus};
use futures::future::BoxFuture;
use jni::{
    objects::{GlobalRef, JClass, JValue},
    JNIEnv, JavaVM,
};
use std::sync::Arc;

static VIEWER_CHANGED: std::sync::OnceLock<Arc<tokio::sync::Notify>> = std::sync::OnceLock::new();
fn viewer_changed() -> Arc<tokio::sync::Notify> {
    VIEWER_CHANGED
        .get_or_init(|| Arc::new(tokio::sync::Notify::new()))
        .clone()
}
#[no_mangle]
pub extern "system" fn Java_com_simonwjackson_korri_korrid_KorridServer_viewerDisplayChanged(
    _env: JNIEnv,
    _class: JClass,
) {
    viewer_changed().notify_one();
}

const CONTROL: &str = "(Ljava/lang/String;)Ljava/lang/String;";
const DISPLAY: &str = "()Ljava/lang/String;";
const START: &str = "(Ljava/lang/String;Ljava/lang/String;II)Ljava/lang/String;";

struct AndroidViewer {
    resize_gate: Arc<tokio::sync::Mutex<()>>,
    vm: JavaVM,
    class: GlobalRef,
}

impl AndroidViewer {
    fn local(&self, method: &str, arguments: &[String]) -> Result<String, LinuxViewerError> {
        let failure = || LinuxViewerError::new("Android local viewer effect failed");
        let mut env = self.vm.attach_current_thread().map_err(|_| failure())?;
        let result = env.with_local_frame(12, |env| -> Result<String, jni::errors::Error> {
            let class: &JClass<'_> = self.class.as_obj().into();
            let strings = arguments
                .iter()
                .map(|value| env.new_string(value))
                .collect::<Result<Vec<_>, _>>()?;
            let values = strings
                .iter()
                .map(|value| JValue::Object(value.as_ref()))
                .collect::<Vec<_>>();
            let signature = match arguments.len() {
                1 => CONTROL,
                2 => "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                3 => "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                4 => "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                _ => return Err(jni::errors::Error::JniCall(jni::errors::JniError::InvalidArguments)),
            };
            let result = env
                .call_static_method(class, method, signature, &values)?
                .l()?;
            let result = jni::objects::JString::from(result);
            let value = String::from(env.get_string(&result)?);
            Ok(value)
        });
        if result.is_err() && env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        result.map_err(|_| failure())
    }

    fn invoke(
        &self,
        method: &str,
        launch_id: &str,
        address: Option<&str>,
        display: Option<crate::launcher::plugin_launch::PluginDisplay>,
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
                let (width, height) = display
                    .as_ref()
                    .map(|d| (d.width, d.height))
                    .unwrap_or((0, 0));
                let width = i32::try_from(width).map_err(|_| {
                    jni::errors::Error::JniCall(jni::errors::JniError::InvalidArguments)
                })?;
                let height = i32::try_from(height).map_err(|_| {
                    jni::errors::Error::JniCall(jni::errors::JniError::InvalidArguments)
                })?;
                arguments.push(JValue::Int(width));
                arguments.push(JValue::Int(height));
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
        display: Option<crate::launcher::plugin_launch::PluginDisplay>,
    ) -> Result<LinuxViewerStatus, LinuxViewerError> {
        let viewer = self.0.clone();
        let launch = launch.to_owned();
        let address = address.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            viewer.invoke(method, &launch, address.as_deref(), display)
        })
        .await
        .map_err(|_| LinuxViewerError::new("Android viewer worker failed"))?
    }

    async fn confirm(
        &self,
        method: &'static str,
        launch: &str,
        address: Option<&str>,
        display: Option<crate::launcher::plugin_launch::PluginDisplay>,
        expected: LinuxViewerStatus,
    ) -> Result<(), LinuxViewerError> {
        if self.observe(method, launch, address, display).await? == expected {
            Ok(())
        } else {
            Err(LinuxViewerError::new(
                "Android viewer did not confirm the requested state",
            ))
        }
    }
}

impl LinuxStreamViewer for Adapter {
    fn change_signal(&self) -> Option<Arc<tokio::sync::Notify>> {
        Some(viewer_changed())
    }
    fn notice<'a>(
        &'a self,
        launch: &'a str,
        message: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        let viewer = self.0.clone();
        let arguments = vec![launch.into(), message.into()];
        Box::pin(async move {
            tokio::task::spawn_blocking(move || viewer.local("viewerNotice", &arguments))
                .await
                .map_err(|_| LinuxViewerError::new("Viewer worker failed"))??;
            Ok(())
        })
    }
    fn resize<'a>(
        &'a self,
        launch: &'a str,
        connection: &'a str,
        display: crate::launcher::plugin_launch::PluginDisplay,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        let viewer = self.0.clone();
        let arguments = vec![
            launch.into(),
            connection.into(),
            display.width.to_string(),
            display.height.to_string(),
        ];
        Box::pin(async move {
            let _guard = viewer.resize_gate.clone().lock_owned().await;
            let answer =
                tokio::task::spawn_blocking(move || viewer.local("viewerResize", &arguments))
                    .await
                    .map_err(|_| LinuxViewerError::new("Viewer worker failed"))??;
            if answer == "Applied" {
                Ok(())
            } else {
                Err(LinuxViewerError::new("Viewer resize not confirmed"))
            }
        })
    }
    fn snapshot<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<Option<crate::linux_viewer::ViewerSnapshot>, LinuxViewerError>> {
        let viewer = self.0.clone();
        let arguments = vec![launch.to_owned()];
        Box::pin(async move {
            let json =
                tokio::task::spawn_blocking(move || viewer.local("viewerSnapshot", &arguments))
                    .await
                    .map_err(|_| LinuxViewerError::new("Viewer worker failed"))??;
            let mut snapshot: crate::linux_viewer::ViewerSnapshot = serde_json::from_str(&json)
                .map_err(|_| LinuxViewerError::new("Invalid viewer observation"))?;
            // SystemClock.uptimeMillis and Rust's Android Instant both use CLOCK_MONOTONIC.
            // Carry the producer's age across JNI; receiving an old sample is not a new sample.
            let mut time = libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };
            // SAFETY: clock_gettime writes only into this owned, initialized timespec.
            let clock_ok = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } == 0;
            let now_ms = time.tv_sec as f64 * 1000.0 + time.tv_nsec as f64 / 1e6;
            let age = snapshot
                .sampled_at_ms
                .filter(|at| clock_ok && at.is_finite() && *at >= 0.0 && *at <= now_ms)
                .and_then(|at| std::time::Duration::try_from_secs_f64((now_ms - at) / 1000.0).ok());
            snapshot.sampled_at = age.and_then(|age| std::time::Instant::now().checked_sub(age));
            if snapshot.sampled_at.is_none() {
                snapshot.stream_health = None;
            }
            Ok(Some(snapshot))
        })
    }

    fn apply<'a>(
        &'a self,
        launch: &'a str,
        control: &'a str,
        value: Option<korri_surface_treaty::SurfaceGameplayControlValue>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        let viewer = self.0.clone();
        let arguments = vec![
            launch.to_owned(),
            control.to_owned(),
            serde_json::to_string(&value).expect("finite validated control value"),
        ];
        Box::pin(async move {
            let result =
                tokio::task::spawn_blocking(move || viewer.local("viewerApply", &arguments))
                    .await
                    .map_err(|_| LinuxViewerError::new("Viewer worker failed"))??;
            if result == "Applied" {
                Ok(())
            } else {
                Err(LinuxViewerError::new("Viewer did not confirm control"))
            }
        })
    }

    fn display(&self) -> Option<crate::launcher::plugin_launch::PluginDisplay> {
        let mut env = self.0.vm.attach_current_thread().ok()?;
        let result = env.with_local_frame(4, |env| -> Result<String, jni::errors::Error> {
            let class: &JClass<'_> = self.0.class.as_obj().into();
            let value = env
                .call_static_method(class, "viewerDisplay", DISPLAY, &[])?
                .l()?;
            let value = jni::objects::JString::from(value);
            let value = String::from(env.get_string(&value)?);
            Ok(value)
        });
        if result.is_err() && env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        let value = result.ok()?;
        let (width, height) = value.split_once('x')?;
        let width = width.parse::<u32>().ok().filter(|v| *v > 0)?;
        let height = height.parse::<u32>().ok().filter(|v| *v > 0)?;
        Some(crate::launcher::plugin_launch::PluginDisplay { width, height })
    }

    fn start<'a>(
        &'a self,
        launch: &'a str,
        address: &'a str,
        display: Option<crate::launcher::plugin_launch::PluginDisplay>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(self.confirm(
            "viewerStart",
            launch,
            Some(address),
            display,
            LinuxViewerStatus::Running,
        ))
    }
    fn status<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<LinuxViewerStatus, LinuxViewerError>> {
        Box::pin(self.observe("viewerStatus", launch, None, None))
    }
    fn background<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(self.confirm(
            "viewerBackground",
            launch,
            None,
            None,
            LinuxViewerStatus::Backgrounded,
        ))
    }
    fn resume_and_focus<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(self.confirm(
            "viewerResume",
            launch,
            None,
            None,
            LinuxViewerStatus::Running,
        ))
    }
    fn stop<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(self.confirm("viewerStop", launch, None, None, LinuxViewerStatus::Stopped))
    }
}

pub(super) fn adapter(
    env: &mut JNIEnv<'_>,
    class: &JClass<'_>,
) -> Result<Arc<dyn LinuxStreamViewer>, jni::errors::Error> {
    env.get_static_method_id(class, "viewerDisplay", DISPLAY)?;
    env.get_static_method_id(class, "viewerSnapshot", CONTROL)?;
    env.get_static_method_id(
        class,
        "viewerNotice",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
    )?;
    env.get_static_method_id(class, "viewerResize", "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;")?;
    env.get_static_method_id(
        class,
        "viewerApply",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
    )?;
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
        resize_gate: Arc::new(tokio::sync::Mutex::new(())),
        vm: env.get_java_vm()?,
        class: env.new_global_ref(class)?,
    }))))
}
