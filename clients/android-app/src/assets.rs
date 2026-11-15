// SPDX-License-Identifier: GPL-3.0-or-later
//! The surface packages travel in the APK's assets (`surfaces/<id>/…`) and
//! the shell reads them from disk, as `KorriShellActivity.copyPackage` gives
//! them to it: under the cache directory, `surfaces/<id>`. The NDK lists only
//! a directory's files, not its subdirectories, so `AssetManager.list` comes
//! through JNI and the bytes through the NDK.
//!
//! The relay list travels there too (`relays.json`): the APK's packing
//! chooses it, not the library's build, so one library serves the product's
//! APK and the emulator proof's, whose one relay is the proof's own.
use anyhow::{bail, Context, Result};
use jni::{
    objects::{JObject, JObjectArray, JString, JValue},
    JNIEnv,
};
use slint::android::AndroidApp;
use std::{
    ffi::CString,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

/// Unpack each of `surfaces` again, replacing what an earlier start left, and
/// return the directory that holds them.
pub fn unpack(
    app: &AndroidApp,
    env: &mut JNIEnv,
    activity: &JObject,
    surfaces: &[&str],
) -> Result<PathBuf> {
    let root = cache_dir(env, activity)?.join("surfaces");
    let assets = app.asset_manager();
    let unpacked = env.with_local_frame(16, |env| -> Result<()> {
        let manager = env
            .call_method(
                activity,
                "getAssets",
                "()Landroid/content/res/AssetManager;",
                &[],
            )?
            .l()?;
        for id in surfaces {
            let target = root.join(id);
            match fs::remove_dir_all(&target) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => {
                    return Err(error).with_context(|| format!("clear {}", target.display()))
                }
                _ => {}
            }
            copy(env, &manager, &assets, &format!("surfaces/{id}"), &target)
                .with_context(|| format!("unpack surface {id}"))?;
        }
        Ok(())
    });
    if unpacked.is_err() {
        // Never format a platform exception; clear it before the next call.
        let _ = env.exception_clear();
    }
    unpacked?;
    Ok(root)
}

/// The relay list that the APK carries, as JSON: korrid's product relays
/// (`korrid::android_app::start`), which it validates itself.
pub fn relays(app: &AndroidApp) -> Result<String> {
    let mut list = String::new();
    app.asset_manager()
        .open(c"relays.json")
        .context("the APK carries no relay list")?
        .read_to_string(&mut list)?;
    Ok(list)
}

fn copy(
    env: &mut JNIEnv,
    manager: &JObject,
    assets: &slint::android::android_activity::ndk::asset::AssetManager,
    asset: &str,
    target: &Path,
) -> Result<()> {
    let children = env.with_local_frame(8, |env| -> Result<Vec<String>> {
        let name = env.new_string(asset)?;
        let list = env
            .call_method(
                manager,
                "list",
                "(Ljava/lang/String;)[Ljava/lang/String;",
                &[JValue::Object(&name)],
            )?
            .l()?;
        if list.is_null() {
            bail!("missing surface asset {asset}");
        }
        let list = JObjectArray::from(list);
        let mut names = Vec::new();
        for index in 0..env.get_array_length(&list)? {
            let child = JString::from(env.get_object_array_element(&list, index)?);
            names.push(env.get_string(&child)?.into());
        }
        Ok(names)
    })?;
    if children.is_empty() {
        // A file: `list` names no children for it.
        let path = CString::new(asset)?;
        let mut input = assets
            .open(&path)
            .with_context(|| format!("missing surface asset {asset}"))?;
        let mut bytes = Vec::new();
        input.read_to_end(&mut bytes)?;
        fs::File::create(target)?.write_all(&bytes)?;
        return Ok(());
    }
    fs::create_dir_all(target)?;
    for child in children {
        copy(
            env,
            manager,
            assets,
            &format!("{asset}/{child}"),
            &target.join(&child),
        )?;
    }
    Ok(())
}

/// `Context.getCacheDir()`: the packages' directory, and the viewer's pins.
pub(crate) fn cache_dir(env: &mut JNIEnv, activity: &JObject) -> Result<PathBuf> {
    let path = env.with_local_frame(4, |env| -> jni::errors::Result<String> {
        let file = env
            .call_method(activity, "getCacheDir", "()Ljava/io/File;", &[])?
            .l()?;
        let path = env
            .call_method(&file, "getAbsolutePath", "()Ljava/lang/String;", &[])?
            .l()?;
        Ok(env.get_string(&path.into())?.into())
    });
    path.map(PathBuf::from).map_err(|_| {
        let _ = env.exception_clear();
        anyhow::anyhow!("Android getCacheDir is unavailable")
    })
}
