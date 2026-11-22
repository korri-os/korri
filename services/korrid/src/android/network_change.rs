//! Android's netwatch monitor is a no-op. Forward changes in the active
//! network handle or link addresses to iroh, without interpreting its paths.
use std::{future::Future, net::IpAddr};

#[derive(Clone, Debug, Eq, PartialEq)]
struct Snapshot {
    handle: Option<i64>,
    addresses: Vec<IpAddr>,
}
#[derive(Default)]
struct Changes(Option<Snapshot>);
impl Changes {
    async fn observe<F: Future<Output = ()>>(
        &mut self,
        mut next: Snapshot,
        notify: impl FnOnce() -> F,
    ) {
        next.addresses.sort();
        next.addresses.dedup();
        let changed = self.0.as_ref().is_some_and(|previous| previous != &next);
        self.0 = Some(next);
        if changed {
            notify().await;
        }
    }
}

#[cfg(target_os = "android")]
mod platform {
    use super::*;
    use jni::{
        objects::{GlobalRef, JByteArray, JObject, JValue},
        JNIEnv, JavaVM,
    };

    pub(crate) struct Reader {
        vm: JavaVM,
        context: GlobalRef,
    }
    impl Reader {
        pub(crate) fn new(env: &mut JNIEnv, context: &JObject) -> jni::errors::Result<Self> {
            Ok(Self {
                vm: env.get_java_vm()?,
                context: env.new_global_ref(context)?,
            })
        }
        fn read(&self) -> Result<Snapshot, ()> {
            let mut env = self.vm.attach_current_thread().map_err(|_| ())?;
            let result = env.with_local_frame(32, |env| read(env, self.context.as_obj()));
            if result.is_err() {
                let _ = env.exception_clear();
            }
            result.map_err(|_| ())
        }
        pub(crate) async fn run(self, directory: crate::federation::FederationDirectory) {
            let mut changes = Changes::default();
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                match self.read() {
                    Ok(snapshot) => {
                        changes
                            .observe(snapshot, || directory.network_change())
                            .await
                    }
                    Err(()) => log::debug!("Android network notification unavailable"),
                }
            }
        }
    }
    fn read(env: &mut JNIEnv, context: &JObject) -> jni::errors::Result<Snapshot> {
        let name = env.new_string("connectivity")?;
        let connectivity = env
            .call_method(
                context,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&name)],
            )?
            .l()?;
        let network = env
            .call_method(
                &connectivity,
                "getActiveNetwork",
                "()Landroid/net/Network;",
                &[],
            )?
            .l()?;
        if network.is_null() {
            return Ok(Snapshot {
                handle: None,
                addresses: Vec::new(),
            });
        }
        let handle = env
            .call_method(&network, "getNetworkHandle", "()J", &[])?
            .j()?;
        let properties = env
            .call_method(
                &connectivity,
                "getLinkProperties",
                "(Landroid/net/Network;)Landroid/net/LinkProperties;",
                &[JValue::Object(&network)],
            )?
            .l()?;
        let mut addresses = Vec::new();
        if !properties.is_null() {
            let links = env
                .call_method(&properties, "getLinkAddresses", "()Ljava/util/List;", &[])?
                .l()?;
            let count = env.call_method(&links, "size", "()I", &[])?.i()?;
            for index in 0..count {
                let address = env.with_local_frame(4, |env| -> jni::errors::Result<IpAddr> {
                    let link = env
                        .call_method(
                            &links,
                            "get",
                            "(I)Ljava/lang/Object;",
                            &[JValue::Int(index)],
                        )?
                        .l()?;
                    let address = env
                        .call_method(&link, "getAddress", "()Ljava/net/InetAddress;", &[])?
                        .l()?;
                    let bytes = env.call_method(&address, "getAddress", "()[B", &[])?.l()?;
                    let bytes = env.convert_byte_array(JByteArray::from(bytes))?;
                    match bytes.as_slice() {
                        [a, b, c, d] => Ok(IpAddr::V4([*a, *b, *c, *d].into())),
                        bytes if bytes.len() == 16 => {
                            let mut octets = [0; 16];
                            octets.copy_from_slice(bytes);
                            Ok(IpAddr::V6(octets.into()))
                        }
                        _ => Err(jni::errors::Error::NullPtr("network address")),
                    }
                })?;
                addresses.push(address);
            }
        }
        Ok(Snapshot {
            handle: Some(handle),
            addresses,
        })
    }
}
#[cfg(target_os = "android")]
pub(crate) use platform::Reader;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn changed_network_notifies_once_and_identical_snapshots_do_not_notify() {
        let calls = AtomicUsize::new(0);
        let mut changes = Changes::default();
        let first = Snapshot {
            handle: Some(1),
            addresses: vec!["192.0.2.1".parse().unwrap()],
        };
        changes
            .observe(first.clone(), || async {
                calls.fetch_add(1, Ordering::SeqCst);
            })
            .await;
        changes
            .observe(first, || async {
                calls.fetch_add(1, Ordering::SeqCst);
            })
            .await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let second = Snapshot {
            handle: Some(2),
            addresses: vec!["192.0.2.1".parse().unwrap()],
        };
        changes
            .observe(second.clone(), || async {
                calls.fetch_add(1, Ordering::SeqCst);
            })
            .await;
        changes
            .observe(second, || async {
                calls.fetch_add(1, Ordering::SeqCst);
            })
            .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let changed_address = Snapshot {
            handle: Some(2),
            addresses: vec!["192.0.2.2".parse().unwrap()],
        };
        changes
            .observe(changed_address, || async {
                calls.fetch_add(1, Ordering::SeqCst);
            })
            .await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
