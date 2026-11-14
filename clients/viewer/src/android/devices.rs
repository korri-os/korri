// SPDX-License-Identifier: GPL-3.0-or-later
//! Android's input devices, from `android.view.InputDevice` through JNI:
//! the NDK gives an event's device ID and nothing about the device. The
//! input (`input.rs`) asks for a device's name, vendor, product,
//! descriptor, whether it is external, its sources, its keyboard type, its
//! joystick axes' ranges, which controller keys it has, its vibrators and
//! whether a light has RGB control, as Artemis's `ControllerHandler` asks
//! the same class. Each device is asked once. The phone's own vibrators and
//! Android version are asked once too.
//!
//! Every call runs on the thread that asks, attached to the VM for good: the
//! app's own thread, which handles the events.
use crate::{
    feedback::{Phone, Vibration, Vibrator, Vibrators},
    input::{Device, Devices, Range, axis, probed_keys, source},
};
use anyhow::{Result, anyhow};
use jni::{
    JNIEnv, JavaVM,
    objects::{JIntArray, JObject, JString, JValue},
};
use std::{collections::HashMap, rc::Rc};

/// `InputDevice` through JNI, with each answer kept.
pub struct AndroidDevices {
    vm: JavaVM,
    known: HashMap<i32, Option<Rc<Device>>>,
    phone: Option<Phone>,
}

impl AndroidDevices {
    pub fn new() -> Result<Self> {
        let context = ndk_context::android_context();
        // SAFETY: the process's VM, which android-activity gave ndk-context.
        let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }?;
        Ok(Self {
            vm,
            known: HashMap::new(),
            phone: None,
        })
    }

    fn with<T>(&self, work: impl FnOnce(&mut JNIEnv) -> jni::errors::Result<T>) -> Result<T> {
        let mut env = self.vm.attach_current_thread_permanently()?;
        let result = env.with_local_frame(64, work);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        result.map_err(|error| anyhow!("{error}"))
    }
}

impl Devices for AndroidDevices {
    fn describe(&mut self, id: i32) -> Option<Rc<Device>> {
        if let Some(known) = self.known.get(&id) {
            return known.clone();
        }
        let described = match self.with(|env| describe(env, id)) {
            Ok(device) => device.map(Rc::new),
            Err(error) => {
                crate::log::line(&format!("input: cannot describe device {id}: {error:#}"));
                None
            }
        };
        self.known.insert(id, described.clone());
        described
    }

    fn ids(&mut self) -> Option<Vec<i32>> {
        let ids = self.with(|env| {
            let ids = env
                .call_static_method("android/view/InputDevice", "getDeviceIds", "()[I", &[])?
                .l()?;
            let ids = JIntArray::from(ids);
            let length = env.get_array_length(&ids)? as usize;
            let mut values = vec![0; length];
            env.get_int_array_region(&ids, 0, &mut values)?;
            Ok(values)
        });
        match ids {
            Ok(ids) => {
                // A device that went is asked about again if it ever sends.
                self.known.retain(|id, _| ids.contains(id));
                Some(ids)
            }
            Err(error) => {
                crate::log::line(&format!("input: cannot list the devices: {error:#}"));
                None
            }
        }
    }

    fn phone(&mut self) -> Phone {
        if let Some(phone) = self.phone {
            return phone;
        }
        let phone = self.with(phone).unwrap_or_else(|error| {
            crate::log::line(&format!(
                "input: cannot describe the phone's vibrators: {error:#}"
            ));
            Phone::default()
        });
        crate::log::line(&format!("input: the phone: {phone:?}"));
        self.phone = Some(phone);
        phone
    }
}

/// `Build.VERSION.SDK_INT`, and the phone's own `VibratorManager` and
/// `Vibrator` services, from the application's context.
fn phone(env: &mut JNIEnv) -> jni::errors::Result<Phone> {
    let api_level = env
        .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
        .i()?;
    // SAFETY: ndk-context's global reference to the application context,
    // alive for the process; borrowed here, never deleted.
    let context = unsafe { JObject::from_raw(ndk_context::android_context().context().cast()) };
    let manager = system_service(env, &context, "vibrator_manager")?;
    let vibrator = system_service(env, &context, "vibrator")?;
    Ok(Phone {
        api_level,
        vibration: Vibration {
            manager: vibrators(env, &manager)?,
            vibrator: one_vibrator(env, &vibrator)?,
        },
    })
}

/// `Context.getSystemService(name)`.
fn system_service<'local>(
    env: &mut JNIEnv<'local>,
    context: &JObject,
    name: &str,
) -> jni::errors::Result<JObject<'local>> {
    let name = JObject::from(env.new_string(name)?);
    env.call_method(
        context,
        "getSystemService",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        &[JValue::Object(&name)],
    )?
    .l()
}

/// A `VibratorManager`'s vibrators, and whether each has amplitude control.
fn vibrators(env: &mut JNIEnv, manager: &JObject) -> jni::errors::Result<Vibrators> {
    if manager.is_null() {
        return Ok(Vibrators::default());
    }
    let ids = JIntArray::from(
        env.call_method(manager, "getVibratorIds", "()[I", &[])?
            .l()?,
    );
    let mut values = vec![0; env.get_array_length(&ids)? as usize];
    env.get_int_array_region(&ids, 0, &mut values)?;
    let mut amplitude = true;
    for id in &values {
        let vibrator = env
            .call_method(
                manager,
                "getVibrator",
                "(I)Landroid/os/Vibrator;",
                &[(*id).into()],
            )?
            .l()?;
        amplitude &= env
            .call_method(&vibrator, "hasAmplitudeControl", "()Z", &[])?
            .z()?;
    }
    Ok(Vibrators {
        count: values.len(),
        amplitude,
    })
}

/// A `Vibrator`: there (`hasVibrator`), and with amplitude control.
fn one_vibrator(env: &mut JNIEnv, vibrator: &JObject) -> jni::errors::Result<Vibrator> {
    if vibrator.is_null() {
        return Ok(Vibrator::default());
    }
    Ok(Vibrator {
        present: env.call_method(vibrator, "hasVibrator", "()Z", &[])?.z()?,
        amplitude: env
            .call_method(vibrator, "hasAmplitudeControl", "()Z", &[])?
            .z()?,
    })
}

/// The device's own vibrators (`getVibratorManager`, `getVibrator`), and
/// whether a light of its `LightsManager` has RGB control.
fn feedback(env: &mut JNIEnv, device: &JObject) -> jni::errors::Result<(Vibration, bool)> {
    let manager = env
        .call_method(
            device,
            "getVibratorManager",
            "()Landroid/os/VibratorManager;",
            &[],
        )?
        .l()?;
    let vibrator = env
        .call_method(device, "getVibrator", "()Landroid/os/Vibrator;", &[])?
        .l()?;
    let vibration = Vibration {
        manager: vibrators(env, &manager)?,
        vibrator: one_vibrator(env, &vibrator)?,
    };
    let lights = env
        .call_method(
            device,
            "getLightsManager",
            "()Landroid/hardware/lights/LightsManager;",
            &[],
        )?
        .l()?;
    let all = env
        .call_method(&lights, "getLights", "()Ljava/util/List;", &[])?
        .l()?;
    let mut rgb = false;
    for at in 0..env.call_method(&all, "size", "()I", &[])?.i()? {
        let light = env
            .call_method(&all, "get", "(I)Ljava/lang/Object;", &[JValue::Int(at)])?
            .l()?;
        rgb |= env.call_method(&light, "hasRgbControl", "()Z", &[])?.z()?;
    }
    Ok((vibration, rgb))
}

fn describe(env: &mut JNIEnv, id: i32) -> jni::errors::Result<Option<Device>> {
    let device = env
        .call_static_method(
            "android/view/InputDevice",
            "getDevice",
            "(I)Landroid/view/InputDevice;",
            &[id.into()],
        )?
        .l()?;
    if device.is_null() {
        return Ok(None);
    }
    let int = |env: &mut JNIEnv, name: &str| -> jni::errors::Result<i32> {
        env.call_method(&device, name, "()I", &[])?.i()
    };
    let string = |env: &mut JNIEnv, name: &str| -> jni::errors::Result<String> {
        let value = JString::from(
            env.call_method(&device, name, "()Ljava/lang/String;", &[])?
                .l()?,
        );
        if value.is_null() {
            return Ok(String::new());
        }
        Ok(env.get_string(&value)?.into())
    };
    let name = string(env, "getName")?;
    let descriptor = string(env, "getDescriptor")?;
    let vendor = int(env, "getVendorId")? as u16;
    let product = int(env, "getProductId")? as u16;
    let sources = int(env, "getSources")? as u32;
    let keyboard_type = int(env, "getKeyboardType")?;
    // API 29.
    let external = env.call_method(&device, "isExternal", "()Z", &[])?.z()?;
    let mut ranges = Vec::new();
    for axis in axis::JOYSTICK {
        let found = match motion_range(env, &device, axis, source::JOYSTICK)? {
            Some(range) => Some(range),
            None => motion_range(env, &device, axis, source::GAMEPAD)?,
        };
        ranges.extend(found);
    }
    let mut pointer_ranges = Vec::new();
    for axis in [axis::ORIENTATION, axis::DISTANCE] {
        pointer_ranges.extend(motion_range(env, &device, axis, source::TOUCHSCREEN)?);
    }
    let probed = probed_keys();
    let codes: Vec<i32> = probed.iter().map(|&key| key as i32).collect();
    let array = env.new_int_array(codes.len() as i32)?;
    env.set_int_array_region(&array, 0, &codes)?;
    let present = env
        .call_method(
            &device,
            "hasKeys",
            "([I)[Z",
            &[JValue::Object(&JObject::from(array))],
        )?
        .l()?;
    let present = jni::objects::JBooleanArray::from(present);
    let mut has = vec![0u8; probed.len()];
    env.get_boolean_array_region(&present, 0, &mut has)?;
    let keys = probed
        .into_iter()
        .zip(has)
        .filter(|&(_, has)| has != 0)
        .map(|(key, _)| key)
        .collect();
    // A device whose vibrators or lights cannot be read still plays: it
    // only gets no rumble or light.
    let (vibration, rgb_light) = feedback(env, &device).unwrap_or_else(|error| {
        let _ = env.exception_clear();
        crate::log::line(&format!(
            "input: no rumble or light for device {id}: {error}"
        ));
        (Vibration::default(), false)
    });
    Ok(Some(Device {
        id,
        name,
        vendor,
        product,
        descriptor,
        external,
        sources,
        keyboard_type,
        ranges,
        pointer_ranges,
        keys,
        vibration,
        rgb_light,
    }))
}

/// `getMotionRange(axis, source)`; none when the device has no such axis.
fn motion_range(
    env: &mut JNIEnv,
    device: &JObject,
    axis: u32,
    source: u32,
) -> jni::errors::Result<Option<Range>> {
    let range = env
        .call_method(
            device,
            "getMotionRange",
            "(II)Landroid/view/InputDevice$MotionRange;",
            &[(axis as i32).into(), (source as i32).into()],
        )?
        .l()?;
    if range.is_null() {
        return Ok(None);
    }
    let float = |env: &mut JNIEnv, name: &str| -> jni::errors::Result<f32> {
        env.call_method(&range, name, "()F", &[])?.f()
    };
    Ok(Some(Range {
        axis,
        min: float(env, "getMin")?,
        max: float(env, "getMax")?,
        flat: float(env, "getFlat")?,
    }))
}

/// `CLOCK_MONOTONIC` in milliseconds: the clock of Android's event times.
pub fn now_ms() -> i64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: a valid clock and a live timespec.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    now.tv_sec * 1000 + now.tv_nsec / 1_000_000
}
