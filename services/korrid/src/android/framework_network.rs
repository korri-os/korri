//! The app with no Korri Java (#434): the network and power observations of
//! the Artemis app's `KorridNetwork.java` (tag `artemis-final`), read from
//! Android's own classes through JNI. The rules that turn them into a
//! snapshot are in `android_network.rs`.
use crate::{
    android_network::{snapshot, Capabilities, Link, ObservedNetwork, Snapshot},
    federation::reach::{Environment, RelayEnvironment, RelayFacts},
    relay::RelayList,
};
use futures::future::BoxFuture;
use jni::{
    errors::Result as JniResult,
    objects::{GlobalRef, JByteArray, JObject, JObjectArray, JValue},
    JNIEnv, JavaVM,
};
use std::{io, net::IpAddr, sync::Arc};

// android.net.NetworkCapabilities and android.os.BatteryManager constants.
const TRANSPORT_VPN: i32 = 4;
const NET_CAPABILITY_NOT_METERED: i32 = 11;

struct Reader {
    vm: JavaVM,
    /// The application `Context`.
    context: GlobalRef,
}

impl Reader {
    fn read(&self) -> io::Result<Snapshot> {
        let unavailable = || io::Error::other("Android network observations are unavailable");
        let mut env = self.vm.attach_current_thread().map_err(|_| unavailable())?;
        // TLS may have permanently attached this worker. Bound our temporary
        // references even when dropping AttachGuard will not detach it.
        let read = env.with_local_frame(64, |env| observe(env, self.context.as_obj()));
        if read.is_err() && env.exception_check().unwrap_or(false) {
            // Do not print platform exceptions: they can carry addresses.
            let _ = env.exception_clear();
        }
        read.map_err(|_| unavailable())
    }
}

impl RelayEnvironment for Reader {
    fn facts(&self) -> BoxFuture<'_, io::Result<RelayFacts>> {
        Box::pin(async move { self.read().map(|snapshot| snapshot.facts) })
    }
}

pub(super) fn environment(
    env: &mut JNIEnv<'_>,
    context: &JObject<'_>,
    relays: RelayList,
) -> JniResult<Environment> {
    let reader = Arc::new(Reader {
        vm: env.get_java_vm()?,
        context: env.new_global_ref(context)?,
    });
    let interfaces = reader.clone();
    Ok(Environment {
        interfaces: Arc::new(move || interfaces.read().map(|snapshot| snapshot.interfaces)),
        relay: reader,
        relays: Some(relays),
    })
}

/// The interface names and facts, for one startup line in the log. No address.
pub(super) fn summary(environment: &Environment) -> String {
    match (environment.interfaces)() {
        Ok(addresses) => {
            let mut names: Vec<&str> = addresses.iter().map(|a| a.interface.as_str()).collect();
            names.dedup();
            format!("{} addresses on [{}]", addresses.len(), names.join(", "))
        }
        Err(error) => error.to_string(),
    }
}

fn observe(env: &mut JNIEnv, context: &JObject) -> JniResult<Snapshot> {
    let name = env.new_string("connectivity")?;
    let connectivity = env
        .call_method(
            context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&name)],
        )?
        .l()?;
    if connectivity.is_null() {
        return Err(jni::errors::Error::NullPtr("ConnectivityManager"));
    }
    let active = env
        .call_method(
            &connectivity,
            "getActiveNetwork",
            "()Landroid/net/Network;",
            &[],
        )?
        .l()?;
    let all = JObjectArray::from(
        env.call_method(
            &connectivity,
            "getAllNetworks",
            "()[Landroid/net/Network;",
            &[],
        )?
        .l()?,
    );
    let mut networks = Vec::new();
    for index in 0..env.get_array_length(&all)? {
        let network = env.get_object_array_element(&all, index)?;
        networks.push(observe_network(env, &connectivity, &network, &active)?);
    }
    let external_power = plugged(env, context)?;
    Ok(snapshot(&networks, !active.is_null(), external_power))
}

fn observe_network(
    env: &mut JNIEnv,
    connectivity: &JObject,
    network: &JObject,
    active: &JObject,
) -> JniResult<ObservedNetwork> {
    let is_active = !active.is_null()
        && env
            .call_method(
                network,
                "equals",
                "(Ljava/lang/Object;)Z",
                &[JValue::Object(active)],
            )?
            .z()?;
    let capabilities = env
        .call_method(
            connectivity,
            "getNetworkCapabilities",
            "(Landroid/net/Network;)Landroid/net/NetworkCapabilities;",
            &[JValue::Object(network)],
        )?
        .l()?;
    let capabilities = if capabilities.is_null() {
        None
    } else {
        Some(Capabilities {
            vpn: env
                .call_method(
                    &capabilities,
                    "hasTransport",
                    "(I)Z",
                    &[JValue::Int(TRANSPORT_VPN)],
                )?
                .z()?,
            not_metered: env
                .call_method(
                    &capabilities,
                    "hasCapability",
                    "(I)Z",
                    &[JValue::Int(NET_CAPABILITY_NOT_METERED)],
                )?
                .z()?,
        })
    };
    let link = env
        .call_method(
            connectivity,
            "getLinkProperties",
            "(Landroid/net/Network;)Landroid/net/LinkProperties;",
            &[JValue::Object(network)],
        )?
        .l()?;
    let link = if link.is_null() {
        None
    } else {
        Some(observe_link(env, &link)?)
    };
    Ok(ObservedNetwork {
        capabilities,
        link,
        active: is_active,
    })
}

fn observe_link(env: &mut JNIEnv, link: &JObject) -> JniResult<Link> {
    let interface = env
        .call_method(link, "getInterfaceName", "()Ljava/lang/String;", &[])?
        .l()?;
    let interface = if interface.is_null() {
        None
    } else {
        Some(String::from(env.get_string(&interface.into())?))
    };
    let mut default_route = false;
    for route in list(env, link, "getRoutes")? {
        default_route |= env.call_method(&route, "isDefaultRoute", "()Z", &[])?.z()?;
    }
    let mut addresses = Vec::new();
    for address in list(env, link, "getLinkAddresses")? {
        let ip = env
            .call_method(&address, "getAddress", "()Ljava/net/InetAddress;", &[])?
            .l()?;
        let bytes = JByteArray::from(env.call_method(&ip, "getAddress", "()[B", &[])?.l()?);
        let bytes = env.convert_byte_array(&bytes)?;
        if let Some(ip) = ip_from(&bytes) {
            addresses.push(ip);
        }
    }
    Ok(Link {
        interface,
        default_route,
        addresses,
    })
}

/// The elements of a `java.util.List` that `method` returns.
fn list<'local>(
    env: &mut JNIEnv<'local>,
    owner: &JObject,
    method: &str,
) -> JniResult<Vec<JObject<'local>>> {
    let list = env
        .call_method(owner, method, "()Ljava/util/List;", &[])?
        .l()?;
    let length = env.call_method(&list, "size", "()I", &[])?.i()?;
    let mut items = Vec::new();
    for index in 0..length {
        items.push(
            env.call_method(&list, "get", "(I)Ljava/lang/Object;", &[JValue::Int(index)])?
                .l()?,
        );
    }
    Ok(items)
}

fn ip_from(bytes: &[u8]) -> Option<IpAddr> {
    match bytes.len() {
        4 => Some(IpAddr::from(<[u8; 4]>::try_from(bytes).ok()?)),
        16 => Some(IpAddr::from(<[u8; 16]>::try_from(bytes).ok()?)),
        _ => None,
    }
}

/// The sticky `ACTION_BATTERY_CHANGED` intent, read without a receiver.
fn plugged(env: &mut JNIEnv, context: &JObject) -> JniResult<bool> {
    let action = env.new_string("android.intent.action.BATTERY_CHANGED")?;
    let filter = env.new_object(
        "android/content/IntentFilter",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&action)],
    )?;
    let battery = env
        .call_method(
            context,
            "registerReceiver",
            "(Landroid/content/BroadcastReceiver;Landroid/content/IntentFilter;)Landroid/content/Intent;",
            &[JValue::Object(&JObject::null()), JValue::Object(&filter)],
        )?
        .l()?;
    if battery.is_null() {
        return Ok(false);
    }
    let extra = env.new_string("plugged")?;
    Ok(env
        .call_method(
            &battery,
            "getIntExtra",
            "(Ljava/lang/String;I)I",
            &[JValue::Object(&extra), JValue::Int(0)],
        )?
        .i()?
        > 0)
}
