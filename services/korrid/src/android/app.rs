//! korrid in the Android app with no Korri Java (#434, slice D8). The app's
//! own library calls [`start`] from `android_main`, in the same process as
//! the shell. What `KorriBrainService`, `KorridServer` and their Java
//! callbacks did for the Artemis app is Rust here, or absent until its slice:
//!
//! | Java today | Here |
//! | --- | --- |
//! | `KorriBrainService.ensureRunning`, `privateStateRoot` | [`start`], once per process; `android_storage` |
//! | `KorridNetwork.snapshot` | `framework_network.rs`, JNI into Android's classes |
//! | `KorriStreamViewer` (`viewer*` callbacks) | none: korrid's unconfigured viewer refuses a start until slice D10 |
//! | `KorriAppLauncher` (local Android games, ADR 0054) | none until slice D13 |
//! | the foreground service | none (owner decision O2): korrid lives as long as the process |
//!
//! iroh's TLS verifier is initialised with the application's class loader,
//! as `KorridServer.start` does. This APK carries no verifier classes yet, so
//! a relay's TLS certificate is refused, and the refusal is an error, never a
//! panic. Slice D9 bundles them (`clients/android-app/README.md`).
use super::framework_network;
use crate::{
    local_server_port, relay::RelayList, start_local_shell_server, EmbeddedPlatform, ServerError,
};
use jni::{objects::JObject, JNIEnv};
use std::path::Path;

/// The official catalog the Android app offers (ADR 0043), as `android.rs`.
const ANDROID_OFFICIAL_CATALOG: Option<&str> = option_env!("KORRI_OFFICIAL_CATALOG_URL");

/// Start korrid in this process for `activity`'s application, or return the
/// port of the korrid that an earlier activity of this process started.
/// `product_relays` is the product's relay list as JSON
/// (`clients/android/product.nix`). The capability stays in this process
/// (`local_server_capability`).
pub fn start(
    env: &mut JNIEnv,
    activity: &JObject,
    product_relays: &str,
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
    init_verifier(env, &context)?;
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
        viewer: None,
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

/// iroh's rustls verifier needs the application's class loader before
/// federation networking starts. Both JNI wrappers borrow the same live call.
fn init_verifier(env: &mut JNIEnv, context: &JObject) -> Result<(), ServerError> {
    let context = context.as_raw();
    let mut verifier_env = unsafe { jni_runtime::EnvUnowned::from_raw(env.get_raw().cast()) };
    verifier_env
        .with_env(|runtime| {
            let context =
                unsafe { jni_runtime::objects::JObject::from_raw(runtime, context.cast()) };
            rustls_platform_verifier::android::init_with_env(runtime, context)
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
