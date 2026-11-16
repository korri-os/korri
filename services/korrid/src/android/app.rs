//! korrid in the Android app with no Korri Java (#434, slice D8). The app's
//! own library calls [`start`] from `android_main`, in the same process as
//! the shell. What `KorriBrainService`, `KorridServer` and their Java
//! callbacks did for the Artemis app is Rust here, or absent until its slice:
//!
//! | Java today | Here |
//! | --- | --- |
//! | `KorriBrainService.ensureRunning`, `privateStateRoot` | [`start`], once per process; `android_storage` |
//! | `KorridNetwork.snapshot` | `framework_network.rs`, JNI into Android's classes |
//! | `KorriStreamViewer` (`viewer*` callbacks) | the app's own Rust viewer, which [`start`] takes (slice D10); it provisions its client certificate with [`provision_moonlight_certificate`] |
//! | `KorriStreamViewer.leave` (`KorridServer.leaveViewer`) | [`leave_viewer`], when the person opens the gameplay overlay (slice D5b) |
//! | `KorriAppLauncher` (local Android games, ADR 0054) | none until slice D13 |
//! | the foreground service | none (owner decision O2): korrid lives as long as the process |
//!
//! rustls-platform-verifier checks a TLS certificate through its Kotlin
//! component. The app has no dex, so it hands [`start`] that component's
//! classes and the Kotlin standard library as one dex file
//! (`clients/android-app/README.md`); korrid loads them in memory, as Slint
//! loads its Java helper, and gives the verifier that class loader. When they
//! cannot load, the verifier gets the application's class loader, which has
//! no such class: a certificate check is then refused with an error, never a
//! panic.
use super::framework_network;
use crate::{
    linux_viewer::{LinuxStreamViewer, LinuxViewerError},
    local_server_port,
    relay::RelayList,
    start_local_shell_server, EmbeddedPlatform, ServerError,
};
use jni::{
    objects::{JObject, JValue},
    JNIEnv,
};
use std::path::Path;

/// The official catalog the Android app offers (ADR 0043), as `android.rs`.
const ANDROID_OFFICIAL_CATALOG: Option<&str> = option_env!("KORRI_OFFICIAL_CATALOG_URL");

/// Start korrid in this process for `activity`'s application, or return the
/// port of the korrid that an earlier activity of this process started.
/// `product_relays` is the product's relay list as JSON
/// (`clients/android/product.nix`). `verifier_classes` is the dex of
/// rustls-platform-verifier's Kotlin component with the Kotlin standard
/// library. `viewer` is the app's stream viewer, which runs in this process
/// (ADR 0060); korrid drives it as it drives a Linux viewer. The capability
/// stays in this process (`local_server_capability`).
pub fn start(
    env: &mut JNIEnv,
    activity: &JObject,
    product_relays: &str,
    verifier_classes: &'static [u8],
    viewer: std::sync::Arc<dyn LinuxStreamViewer>,
) -> Result<u16, ServerError> {
    if let Some(port) = local_server_port() {
        return Ok(port);
    }
    let failed = |details: &str| ServerError::StartFailed {
        details: details.to_owned(),
    };
    // Never format a JNI error: it can carry platform details.
    let jni = |_| failed("Android application context is unavailable");
    let context = env
        .call_method(
            activity,
            "getApplicationContext",
            "()Landroid/content/Context;",
            &[],
        )
        .and_then(|value| value.l())
        .map_err(jni)?;
    init_verifier(env, &context, verifier_classes)?;
    let local_storage_root = directory(env, &context, "getFilesDir")?;
    let no_backup = directory(env, &context, "getNoBackupFilesDir")?;
    let private_state_root = crate::android_storage::private_state_root(Path::new(&no_backup))
        .map_err(|_| failed("Android private storage is unavailable"))?;
    let private_state_root = private_state_root
        .to_str()
        .ok_or_else(|| failed("Android private storage path is not UTF-8"))?
        .to_owned();
    let relays = serde_json::from_str::<Vec<String>>(product_relays)
        .ok()
        .and_then(|values| RelayList::configured(values).ok())
        .ok_or_else(|| failed("Android product relays are invalid"))?;
    let reach = framework_network::environment(env, &context, relays.clone()).map_err(jni)?;
    eprintln!(
        "korrid: Android network observed: {}",
        framework_network::summary(&reach)
    );
    let platform = EmbeddedPlatform {
        native_input: None,
        relays: Some(relays),
        owner_binding: Some(crate::owner_binding::OwnerBindingCoordinator::default()),
        reach: Some(reach),
        viewer: Some(viewer),
    };
    // ADR 0054: plugins live in app storage; korrid installs them itself.
    crate::nixfree_install::use_android_store(Path::new(&private_state_root));
    crate::use_android_plugin_admin(
        Path::new(&private_state_root),
        ANDROID_OFFICIAL_CATALOG.map(str::to_owned),
    );
    match start_local_shell_server(&local_storage_root, &private_state_root, platform) {
        // Another activity of this process won the start.
        Err(ServerError::AlreadyRunning) => local_server_port().ok_or(ServerError::AlreadyRunning),
        started => started,
    }
}

/// The server certificate of the Sunshine at a Moonlight `address`, after
/// this device's broker has provisioned `client_certificate` (one PEM) there:
/// the host UUID from Sunshine's `serverinfo`, as the Linux viewers read it,
/// then the broker's provision through korrid's own RPC, as the Artemis
/// viewer asks it. The holder that attests that UUID performs it through its
/// Sunshine's certificate control.
pub async fn provision_moonlight_certificate(
    address: &str,
    client_certificate: &str,
) -> Result<String, LinuxViewerError> {
    let host_uuid = crate::linux_viewer::sunshine_host_uuid(address).await?;
    super::provision_locally(host_uuid, client_certificate.to_owned())
        .await
        .map_err(|code| {
            LinuxViewerError::new(format!(
                "the Moonlight certificate was not provisioned: {code}"
            ))
        })
}

/// The person left the stream of the exact `launch` for the gameplay
/// overlay: korrid's `session.leave`, as the Artemis viewer asks it
/// (`KorriStreamViewer.leave`). korrid then opens its gameplay overlay
/// (`surface/overlay.rs`), and its Resume comes back as the viewer's
/// `resume_and_focus`. True when korrid answered that this launch runs on.
/// It blocks: never call it on a runtime's thread.
pub fn leave_viewer(launch: &str) -> bool {
    super::leave_locally(launch)
}

/// The emulator proof's trigger for korrid's resize request (#476 slice 3).
/// It asks the holder of the exact selected `launch` for the stream's
/// encode size, as the Portrait control does once the viewer is ready to
/// present a new size. It goes through korrid's local RPC, as
/// [`leave_viewer`] does, so that the peer call runs on korrid's own
/// runtime. Only the fixture APK's korrid has it (cargo feature `proof`).
/// It blocks: never call it on a runtime's thread.
#[cfg(feature = "proof")]
pub fn proof_resize_stream(
    launch: &str,
    display: crate::launcher::plugin_launch::PluginDisplay,
) -> Result<crate::live_display::ViewerResize, String> {
    super::resize_locally(launch, display)
}

/// The class that rustls-platform-verifier loads first.
const VERIFIER_CLASS: &str = "org.rustls.platformverifier.CertificateVerifier";

/// iroh's rustls verifier needs its classes' loader and the application
/// context before federation networking starts. Both JNI wrappers borrow the
/// same live call.
fn init_verifier(
    env: &mut JNIEnv,
    context: &JObject,
    classes: &'static [u8],
) -> Result<(), ServerError> {
    let loader = match verifier_loader(env, context, classes) {
        Ok(loader) => {
            eprintln!("korrid: Android TLS verifier classes loaded");
            Some(loader)
        }
        Err(_) => {
            // Never format a JNI error: it can carry platform details.
            let _ = env.exception_clear();
            eprintln!(
                "korrid: Android TLS verifier classes did not load; a certificate check through Android is refused"
            );
            None
        }
    };
    let context = context.as_raw();
    let loader = loader.as_ref().map(|loader| loader.as_raw());
    let mut verifier_env = unsafe { jni_runtime::EnvUnowned::from_raw(env.get_raw().cast()) };
    verifier_env
        .with_env(|runtime| -> jni_runtime::errors::Result<()> {
            let context =
                unsafe { jni_runtime::objects::JObject::from_raw(runtime, context.cast()) };
            let Some(loader) = loader else {
                return rustls_platform_verifier::android::init_with_env(runtime, context);
            };
            let loader = unsafe { jni_runtime::objects::JObject::from_raw(runtime, loader.cast()) };
            let loader = runtime.cast_local::<jni_runtime::objects::JClassLoader>(loader)?;
            rustls_platform_verifier::android::init_with_refs(
                runtime.get_java_vm()?,
                runtime.new_global_ref(context)?,
                runtime.new_global_ref(loader)?,
            );
            Ok(())
        })
        .resolve::<jni_runtime::errors::ThrowRuntimeExAndDefault>();
    if env.exception_check().unwrap_or(true) {
        let _ = env.exception_clear();
        return Err(ServerError::StartFailed {
            details: "Android TLS verifier initialisation failed".into(),
        });
    }
    Ok(())
}

/// An in-memory class loader for `classes`, below the application's own, in
/// which the verifier's first class resolves.
fn verifier_loader<'local>(
    env: &mut JNIEnv<'local>,
    context: &JObject,
    classes: &'static [u8],
) -> jni::errors::Result<JObject<'local>> {
    env.with_local_frame_returning_local(8, |env| {
        let parent = env
            .call_method(context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
            .l()?;
        // SAFETY: `classes` lives as long as the process; the loader only reads it.
        let buffer =
            unsafe { env.new_direct_byte_buffer(classes.as_ptr().cast_mut(), classes.len()) }?;
        let loader = env.new_object(
            "dalvik/system/InMemoryDexClassLoader",
            "(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V",
            &[JValue::Object(&buffer), JValue::Object(&parent)],
        )?;
        let name = env.new_string(VERIFIER_CLASS)?;
        env.call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&name)],
        )?;
        Ok(loader)
    })
}

/// The absolute path of a `java.io.File` that `method` of the context returns.
fn directory(env: &mut JNIEnv, context: &JObject, method: &str) -> Result<String, ServerError> {
    let read = env.with_local_frame(4, |env| -> jni::errors::Result<String> {
        let file = env
            .call_method(context, method, "()Ljava/io/File;", &[])?
            .l()?;
        let path = env
            .call_method(&file, "getAbsolutePath", "()Ljava/lang/String;", &[])?
            .l()?;
        Ok(env.get_string(&path.into())?.into())
    });
    read.map_err(|_| {
        let _ = env.exception_clear();
        ServerError::StartFailed {
            details: format!("Android {method} is unavailable"),
        }
    })
}
