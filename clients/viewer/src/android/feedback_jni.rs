// SPDX-License-Identifier: GPL-3.0-only
//! The controllers' feedback on Android (`feedback.rs`): vibrators, lights
//! and batteries through JNI into Android's own classes, all from API 31
//! (ADR 0064): `InputDevice.getVibratorManager()`, `getVibrator()`,
//! `getLightsManager()` and `getBatteryState()`, and the phone's own
//! `VibratorManager` and `Vibrator` services for a built-in controller.
//!
//! Every call runs on the feedback thread, which `AndroidOutputs::new`
//! attaches to the VM for the thread's whole life; Rust's thread-local
//! guard detaches it when the thread ends. No Korri Java: the calls are
//! Artemis's (`ControllerHandler.java`, whose terms this file keeps), made
//! from Rust.
use crate::{
    devices::now_ms,
    feedback::{Battery, Call, Outputs, Target, Worker},
    input::core_sink,
};
use anyhow::{Context as _, Result, bail};
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JIntArray, JObject, JValue},
};
use std::collections::{HashMap, hash_map::Entry};

/// A one-shot's length: the host sends a new value or a stop well before.
const ONE_SHOT_MS: i64 = 60_000;
/// Android 13: vibrations take `VibrationAttributes.USAGE_MEDIA`.
const API_USAGE_MEDIA: i32 = 33;

/// Start the feedback thread of a stream: Android's side on it, its battery
/// reports to the core behind the input's gate.
pub fn start() -> Result<Worker> {
    Worker::start(AndroidOutputs::new, core_sink(), now_ms)
}

/// Android's vibrators, lights and batteries, with the objects that the
/// feedback thread keeps for each device.
pub struct AndroidOutputs {
    vm: JavaVM,
    api_level: i32,
    /// The application's `Context`, for the phone's own services.
    context: GlobalRef,
    managers: HashMap<Target, GlobalRef>,
    vibrators: HashMap<Target, GlobalRef>,
    sessions: HashMap<i32, GlobalRef>,
}

impl AndroidOutputs {
    /// On the feedback thread: attach it to the VM for good.
    pub fn new() -> Result<Self> {
        let android = ndk_context::android_context();
        // SAFETY: the process's VM, which android-activity gave ndk-context.
        let vm = unsafe { JavaVM::from_raw(android.vm().cast()) }?;
        let mut env = vm.attach_current_thread_permanently()?;
        let api_level = env
            .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
            .i()?;
        // SAFETY: ndk-context's global reference to the application context,
        // alive for the process; borrowed here, never deleted.
        let context = unsafe { JObject::from_raw(android.context().cast()) };
        let context = env.new_global_ref(&context)?;
        Ok(Self {
            vm,
            api_level,
            context,
            managers: HashMap::new(),
            vibrators: HashMap::new(),
            sessions: HashMap::new(),
        })
    }
}

impl Outputs for AndroidOutputs {
    fn call(&mut self, call: &Call) -> Result<()> {
        let Self {
            vm,
            api_level,
            context,
            managers,
            vibrators,
            sessions,
        } = self;
        let api_level = *api_level;
        let mut env = vm.attach_current_thread_permanently()?;
        let done = env.with_local_frame(32, |env| -> Result<()> {
            match call {
                Call::Combined { target, amplitudes } => {
                    let manager = service(env, context, managers, *target, Kind::Manager)?
                        .context("no VibratorManager")?;
                    combined(env, manager.as_obj(), amplitudes, api_level)
                }
                Call::CancelManager { target } => {
                    // A device that went has nothing left to stop.
                    if let Some(manager) = service(env, context, managers, *target, Kind::Manager)?
                    {
                        env.call_method(manager.as_obj(), "cancel", "()V", &[])?;
                    }
                    Ok(())
                }
                Call::OneShot { target, amplitude } => {
                    let vibrator = service(env, context, vibrators, *target, Kind::Vibrator)?
                        .context("no Vibrator")?;
                    let effect = env
                        .call_static_method(
                            "android/os/VibrationEffect",
                            "createOneShot",
                            "(JI)Landroid/os/VibrationEffect;",
                            &[
                                JValue::Long(ONE_SHOT_MS),
                                JValue::Int(i32::from(*amplitude)),
                            ],
                        )?
                        .l()?;
                    vibrate(env, vibrator.as_obj(), &effect, api_level)
                }
                Call::Pulse {
                    target,
                    on_ms,
                    off_ms,
                } => {
                    let vibrator = service(env, context, vibrators, *target, Kind::Vibrator)?
                        .context("no Vibrator")?;
                    let pattern = env.new_long_array(3)?;
                    env.set_long_array_region(&pattern, 0, &[0, *on_ms, *off_ms])?;
                    let effect = env
                        .call_static_method(
                            "android/os/VibrationEffect",
                            "createWaveform",
                            "([JI)Landroid/os/VibrationEffect;",
                            &[JValue::Object(&JObject::from(pattern)), JValue::Int(0)],
                        )?
                        .l()?;
                    vibrate(env, vibrator.as_obj(), &effect, api_level)
                }
                Call::CancelVibrator { target } => {
                    if let Some(vibrator) =
                        service(env, context, vibrators, *target, Kind::Vibrator)?
                    {
                        env.call_method(vibrator.as_obj(), "cancel", "()V", &[])?;
                    }
                    Ok(())
                }
                Call::Light { device, argb } => light(env, sessions, *device, *argb),
                Call::CloseLight { device } => {
                    if let Some(session) = sessions.remove(device) {
                        env.call_method(session.as_obj(), "close", "()V", &[])?;
                    }
                    Ok(())
                }
            }
        });
        cleared(&mut env, done)
    }

    fn battery(&mut self, device: i32) -> Result<Option<Battery>> {
        let mut env = self.vm.attach_current_thread_permanently()?;
        let read = env.with_local_frame(16, |env| -> Result<Option<Battery>> {
            let Some(input_device) = input_device(env, device)? else {
                return Ok(None);
            };
            let state = env
                .call_method(
                    &input_device,
                    "getBatteryState",
                    "()Landroid/hardware/BatteryState;",
                    &[],
                )?
                .l()?;
            if state.is_null() || !env.call_method(&state, "isPresent", "()Z", &[])?.z()? {
                return Ok(None);
            }
            Ok(Some(Battery {
                status: env.call_method(&state, "getStatus", "()I", &[])?.i()?,
                capacity: env.call_method(&state, "getCapacity", "()F", &[])?.f()?,
            }))
        });
        cleared(&mut env, read)
    }

    fn forget(&mut self, device: i32) {
        self.managers.remove(&Target::Device(device));
        self.vibrators.remove(&Target::Device(device));
        self.sessions.remove(&device);
    }
}

/// A pending Java exception goes to logcat and is cleared: the next call
/// on this thread must not run with it.
fn cleared<T>(env: &mut JNIEnv, result: Result<T>) -> Result<T> {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result
}

#[derive(Clone, Copy)]
enum Kind {
    Manager,
    Vibrator,
}

/// `InputDevice.getDevice(id)`; none when it is gone.
fn input_device<'local>(env: &mut JNIEnv<'local>, id: i32) -> Result<Option<JObject<'local>>> {
    let device = env
        .call_static_method(
            "android/view/InputDevice",
            "getDevice",
            "(I)Landroid/view/InputDevice;",
            &[id.into()],
        )?
        .l()?;
    Ok((!device.is_null()).then_some(device))
}

/// The target's `VibratorManager` or `Vibrator`, kept once found; none when
/// its device is gone.
fn service<'c>(
    env: &mut JNIEnv,
    context: &GlobalRef,
    cache: &'c mut HashMap<Target, GlobalRef>,
    target: Target,
    kind: Kind,
) -> Result<Option<&'c GlobalRef>> {
    if let Entry::Vacant(entry) = cache.entry(target) {
        let found = match target {
            Target::Device(id) => {
                let Some(device) = input_device(env, id)? else {
                    return Ok(None);
                };
                let (name, signature) = match kind {
                    Kind::Manager => ("getVibratorManager", "()Landroid/os/VibratorManager;"),
                    Kind::Vibrator => ("getVibrator", "()Landroid/os/Vibrator;"),
                };
                env.call_method(&device, name, signature, &[])?.l()?
            }
            Target::Phone => {
                let name = JObject::from(env.new_string(match kind {
                    Kind::Manager => "vibrator_manager",
                    Kind::Vibrator => "vibrator",
                })?);
                env.call_method(
                    context.as_obj(),
                    "getSystemService",
                    "(Ljava/lang/String;)Ljava/lang/Object;",
                    &[JValue::Object(&name)],
                )?
                .l()?
            }
        };
        if found.is_null() {
            bail!("Android gave no vibrator for {target:?}");
        }
        entry.insert(env.new_global_ref(found)?);
    }
    Ok(cache.get(&target))
}

/// `rumbleDualVibrators` and `rumbleQuadVibrators`: one parallel vibration,
/// each vibrator by its place in `getVibratorIds()`.
fn combined(env: &mut JNIEnv, manager: &JObject, amplitudes: &[u8], api_level: i32) -> Result<()> {
    let ids = JIntArray::from(
        env.call_method(manager, "getVibratorIds", "()[I", &[])?
            .l()?,
    );
    let mut values = vec![0; env.get_array_length(&ids)? as usize];
    env.get_int_array_region(&ids, 0, &mut values)?;
    let combination = env
        .call_static_method(
            "android/os/CombinedVibration",
            "startParallel",
            "()Landroid/os/CombinedVibration$ParallelCombination;",
            &[],
        )?
        .l()?;
    for (&id, &amplitude) in values.iter().zip(amplitudes) {
        // An effect of amplitude 0 is illegal; leaving the vibrator out stops it.
        if amplitude == 0 {
            continue;
        }
        let effect = env
            .call_static_method(
                "android/os/VibrationEffect",
                "createOneShot",
                "(JI)Landroid/os/VibrationEffect;",
                &[JValue::Long(ONE_SHOT_MS), JValue::Int(i32::from(amplitude))],
            )?
            .l()?;
        env.call_method(
            &combination,
            "addVibrator",
            "(ILandroid/os/VibrationEffect;)Landroid/os/CombinedVibration$ParallelCombination;",
            &[JValue::Int(id), JValue::Object(&effect)],
        )?;
    }
    let vibration = env
        .call_method(
            &combination,
            "combine",
            "()Landroid/os/CombinedVibration;",
            &[],
        )?
        .l()?;
    let attributes = vibration_attributes(env, api_level)?;
    env.call_method(
        manager,
        "vibrate",
        "(Landroid/os/CombinedVibration;Landroid/os/VibrationAttributes;)V",
        &[JValue::Object(&vibration), JValue::Object(&attributes)],
    )?;
    Ok(())
}

/// `VibrationAttributes`: usage media from Android 13, as Artemis sets it.
fn vibration_attributes<'local>(
    env: &mut JNIEnv<'local>,
    api_level: i32,
) -> Result<JObject<'local>> {
    let builder = env.new_object("android/os/VibrationAttributes$Builder", "()V", &[])?;
    if api_level >= API_USAGE_MEDIA {
        let media = env
            .get_static_field("android/os/VibrationAttributes", "USAGE_MEDIA", "I")?
            .i()?;
        env.call_method(
            &builder,
            "setUsage",
            "(I)Landroid/os/VibrationAttributes$Builder;",
            &[JValue::Int(media)],
        )?;
    }
    Ok(env
        .call_method(&builder, "build", "()Landroid/os/VibrationAttributes;", &[])?
        .l()?)
}

/// `rumbleSingleVibrator`'s vibrate: with `VibrationAttributes` from
/// Android 13, before it with `AudioAttributes` of usage game.
fn vibrate(env: &mut JNIEnv, vibrator: &JObject, effect: &JObject, api_level: i32) -> Result<()> {
    if api_level >= API_USAGE_MEDIA {
        let attributes = vibration_attributes(env, api_level)?;
        env.call_method(
            vibrator,
            "vibrate",
            "(Landroid/os/VibrationEffect;Landroid/os/VibrationAttributes;)V",
            &[JValue::Object(effect), JValue::Object(&attributes)],
        )?;
        return Ok(());
    }
    let builder = env.new_object("android/media/AudioAttributes$Builder", "()V", &[])?;
    let game = env
        .get_static_field("android/media/AudioAttributes", "USAGE_GAME", "I")?
        .i()?;
    env.call_method(
        &builder,
        "setUsage",
        "(I)Landroid/media/AudioAttributes$Builder;",
        &[JValue::Int(game)],
    )?;
    let attributes = env
        .call_method(&builder, "build", "()Landroid/media/AudioAttributes;", &[])?
        .l()?;
    env.call_method(
        vibrator,
        "vibrate",
        "(Landroid/os/VibrationEffect;Landroid/media/AudioAttributes;)V",
        &[JValue::Object(effect), JValue::Object(&attributes)],
    )?;
    Ok(())
}

/// `handleSetControllerLED`: the device's light session, opened at the
/// first colour, asks each light with RGB control for it.
fn light(
    env: &mut JNIEnv,
    sessions: &mut HashMap<i32, GlobalRef>,
    device: i32,
    argb: u32,
) -> Result<()> {
    let input_device = input_device(env, device)?.context("the device is gone")?;
    let lights = env
        .call_method(
            &input_device,
            "getLightsManager",
            "()Landroid/hardware/lights/LightsManager;",
            &[],
        )?
        .l()?;
    if let Entry::Vacant(entry) = sessions.entry(device) {
        let session = env
            .call_method(
                &lights,
                "openSession",
                "()Landroid/hardware/lights/LightsManager$LightsSession;",
                &[],
            )?
            .l()?;
        entry.insert(env.new_global_ref(session)?);
    }
    let state = env.new_object("android/hardware/lights/LightState$Builder", "()V", &[])?;
    env.call_method(
        &state,
        "setColor",
        "(I)Landroid/hardware/lights/LightState$Builder;",
        &[JValue::Int(argb as i32)],
    )?;
    let state = env
        .call_method(
            &state,
            "build",
            "()Landroid/hardware/lights/LightState;",
            &[],
        )?
        .l()?;
    let request = env.new_object("android/hardware/lights/LightsRequest$Builder", "()V", &[])?;
    let all = env
        .call_method(&lights, "getLights", "()Ljava/util/List;", &[])?
        .l()?;
    for at in 0..env.call_method(&all, "size", "()I", &[])?.i()? {
        let light = env
            .call_method(&all, "get", "(I)Ljava/lang/Object;", &[JValue::Int(at)])?
            .l()?;
        if env.call_method(&light, "hasRgbControl", "()Z", &[])?.z()? {
            env.call_method(
                &request,
                "addLight",
                "(Landroid/hardware/lights/Light;Landroid/hardware/lights/LightState;)Landroid/hardware/lights/LightsRequest$Builder;",
                &[JValue::Object(&light), JValue::Object(&state)],
            )?;
        }
    }
    let request = env
        .call_method(
            &request,
            "build",
            "()Landroid/hardware/lights/LightsRequest;",
            &[],
        )?
        .l()?;
    let session = &sessions[&device];
    env.call_method(
        session.as_obj(),
        "requestLights",
        "(Landroid/hardware/lights/LightsRequest;)V",
        &[JValue::Object(&request)],
    )?;
    Ok(())
}
