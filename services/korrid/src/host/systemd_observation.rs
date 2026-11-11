//! Native systemd property observations for the synchronous launch boundary.
//! One runtime owns the bus and signal-maintained zbus property caches. Callers
//! never start a helper process or a thread for an individual observation.

use futures::StreamExt;
use std::{
    collections::BTreeMap,
    sync::mpsc,
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc as async_mpsc, watch};
use zbus::{proxy::CacheProperties, zvariant::OwnedObjectPath, Connection, Proxy};

#[cfg(test)]
#[path = "systemd_observation_tests.rs"]
mod tests;

#[derive(Clone, Debug)]
pub(super) struct UnitProperties {
    pub values: BTreeMap<String, String>,
    pub environment: Vec<String>,
}

enum RequestKind {
    Read(String),
    Invalidate,
}

struct Request {
    kind: RequestKind,
    deadline: Instant,
    reply: mpsc::SyncSender<Result<Option<UnitProperties>, String>>,
}

pub(super) struct NativeUnitReader {
    requests: async_mpsc::Sender<Request>,
    quit: watch::Sender<bool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for NativeUnitReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeUnitReader").finish_non_exhaustive()
    }
}

impl NativeUnitReader {
    pub(super) fn new() -> std::io::Result<Self> {
        Self::connect_to(None)
    }

    fn connect_to(address: Option<String>) -> std::io::Result<Self> {
        let (requests, receiver) = async_mpsc::channel(32);
        let (quit, stopping) = watch::channel(false);
        let thread = std::thread::Builder::new()
            .name("systemd-observer".into())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                runtime.block_on(run(receiver, stopping, address));
            })?;
        Ok(Self {
            requests,
            quit,
            thread: Some(thread),
        })
    }

    pub(super) fn read(
        &self,
        unit: &str,
        timeout: Duration,
    ) -> Result<Option<UnitProperties>, String> {
        self.request(RequestKind::Read(unit.into()), timeout)
    }

    pub(super) fn invalidate(&self, timeout: Duration) -> Result<(), String> {
        self.request(RequestKind::Invalidate, timeout).map(|_| ())
    }

    fn request(
        &self,
        kind: RequestKind,
        timeout: Duration,
    ) -> Result<Option<UnitProperties>, String> {
        let (reply, receive) = mpsc::sync_channel(1);
        self.requests
            .try_send(Request {
                kind,
                deadline: Instant::now() + timeout,
                reply,
            })
            .map_err(|error| format!("systemd observation unavailable: {error}"))?;
        receive
            .recv_timeout(timeout)
            .map_err(|error| format!("systemd observation did not complete: {error}"))?
    }
}

impl Drop for NativeUnitReader {
    fn drop(&mut self) {
        self.quit.send_replace(true);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Unit {
    unit: Proxy<'static>,
    service: Proxy<'static>,
    control_group: Option<(String, u32, String)>,
}

struct Bus {
    connection: Connection,
    manager: Proxy<'static>,
    owner: zbus::proxy::OwnerChangedStream<'static>,
    removed: zbus::proxy::SignalStream<'static>,
    reloading: zbus::proxy::SignalStream<'static>,
    units: BTreeMap<String, Unit>,
}

impl Bus {
    async fn connect(address: Option<&str>) -> zbus::Result<Self> {
        let connection = match address {
            Some(address) => zbus::connection::Builder::address(address)?.build().await?,
            None => Connection::system().await?,
        };
        Self::on_connection(connection).await
    }

    async fn on_connection(connection: Connection) -> zbus::Result<Self> {
        let manager = proxy(
            &connection,
            "/org/freedesktop/systemd1".try_into()?,
            "org.freedesktop.systemd1.Manager",
            CacheProperties::No,
        )
        .await?;
        let owner = manager.receive_owner_changed().await?;
        let removed = manager.receive_signal("UnitRemoved").await?;
        let reloading = manager.receive_signal("Reloading").await?;
        // systemd emits unit lifecycle signals only for subscribed bus peers.
        manager.call::<_, _, ()>("Subscribe", &()).await?;
        Ok(Self {
            connection,
            manager,
            owner,
            removed,
            reloading,
            units: BTreeMap::new(),
        })
    }

    async fn read(&mut self, name: &str) -> zbus::Result<Option<UnitProperties>> {
        if !self.units.contains_key(name) {
            let path: OwnedObjectPath = match self.manager.call("GetUnit", &(name,)).await {
                Ok(path) => path,
                Err(zbus::Error::MethodError(name, _, _))
                    if name.as_str() == "org.freedesktop.systemd1.NoSuchUnit" =>
                {
                    return Ok(None)
                }
                Err(error) => return Err(error),
            };
            let unit = proxy(
                &self.connection,
                path.clone(),
                "org.freedesktop.systemd1.Unit",
                CacheProperties::Yes,
            )
            .await?;
            let service = proxy(
                &self.connection,
                path,
                "org.freedesktop.systemd1.Service",
                CacheProperties::Yes,
            )
            .await?;
            self.units.insert(
                name.into(),
                Unit {
                    unit,
                    service,
                    control_group: None,
                },
            );
        }
        let entry = self.units.get_mut(name).expect("observed unit");
        let mut values = BTreeMap::<String, String>::new();
        for property in ["LoadState", "ActiveState", "FreezerState"] {
            values.insert(
                property.into(),
                entry.unit.get_property::<String>(property).await?,
            );
        }
        for property in ["ExecMainCode", "ExecMainStatus"] {
            values.insert(
                property.into(),
                entry
                    .service
                    .get_property::<i32>(property)
                    .await?
                    .to_string(),
            );
        }
        values.insert(
            "StatusText".into(),
            entry.service.get_property::<String>("StatusText").await?,
        );
        let main_pid = entry.service.get_property::<u32>("MainPID").await?;
        let active = &values["ActiveState"];
        // ControlGroup explicitly does not emit PropertiesChanged on systemd.
        // Its path belongs to the service lifecycle, not a historical PID.
        // Refresh on lifecycle change, reload, or an explicit control barrier.
        let refresh_group = entry
            .control_group
            .as_ref()
            .is_none_or(|(state, pid, group)| {
                state != active || *pid != main_pid || (group.is_empty() && main_pid != 0)
            });
        if refresh_group {
            let group = entry.service.get_property::<String>("ControlGroup").await?;
            entry.control_group = Some((active.clone(), main_pid, group));
        }
        values.insert(
            "ControlGroup".into(),
            entry
                .control_group
                .as_ref()
                .expect("observed cgroup")
                .2
                .clone(),
        );
        values.insert("MainPID".into(), main_pid.to_string());
        let environment = entry
            .service
            .get_property::<Vec<String>>("Environment")
            .await?;
        Ok(Some(UnitProperties {
            values,
            environment,
        }))
    }

    async fn changed(&mut self) -> bool {
        tokio::select! {
            _ = self.connection.closed() => false,
            _ = self.owner.next() => false,
            reloading = self.reloading.next() => {
                if reloading.is_none() { return false; }
                self.units.clear();
                true
            },
            removed = self.removed.next() => {
                let Some(message) = removed else { return false; };
                match message.body().deserialize::<(String, OwnedObjectPath)>() {
                    Ok((name, _)) => { self.units.remove(&name); true }
                    Err(_) => false,
                }
            }
        }
    }
}

async fn proxy(
    connection: &Connection,
    path: OwnedObjectPath,
    interface: &'static str,
    cache: CacheProperties,
) -> zbus::Result<Proxy<'static>> {
    zbus::proxy::Builder::new(connection)
        .destination("org.freedesktop.systemd1")?
        .path(path)?
        .interface(interface)?
        .cache_properties(cache)
        .uncached_properties(&["ControlGroup"])
        .build()
        .await
}

async fn run(
    mut requests: async_mpsc::Receiver<Request>,
    mut stopping: watch::Receiver<bool>,
    address: Option<String>,
) {
    let mut bus: Option<Bus> = None;
    loop {
        tokio::select! {
            biased;
            _ = stopping.changed() => return,
            keep = async {
                match &mut bus {
                    Some(bus) => bus.changed().await,
                    None => std::future::pending().await,
                }
            } => { if !keep { bus = None; } },
            request = requests.recv() => {
                let Some(request) = request else { return; };
                let unit = match &request.kind {
                    RequestKind::Read(unit) => unit,
                    RequestKind::Invalidate => {
                        if let Some(bus) = &mut bus { bus.units.clear(); }
                        let _ = request.reply.send(Ok(None));
                        continue;
                    }
                };
                let Some(timeout) = request.deadline.checked_duration_since(Instant::now()) else {
                    let _ = request.reply.send(Err("systemd observation expired in queue".into()));
                    continue;
                };
                let result = tokio::time::timeout(timeout, async {
                    if bus.as_ref().is_some_and(|bus| bus.connection.is_closed()) { bus = None; }
                    if bus.is_none() { bus = Some(Bus::connect(address.as_deref()).await?); }
                    bus.as_mut().expect("connected").read(unit).await
                }).await;
                let result = match result {
                    Ok(Ok(value)) => Ok(value),
                    Ok(Err(error)) => { bus = None; Err(error.to_string()) },
                    Err(_) => { bus = None; Err("systemd observation timed out".into()) },
                };
                let _ = request.reply.send(result);
            }
        }
    }
}
