use super::*;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

const PATH: &str = "/org/freedesktop/systemd1/unit/game";

struct PrivateBus {
    child: Child,
    address: String,
}
impl PrivateBus {
    fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            child,
            address: address.trim().into(),
        }
    }
}
impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Manager;
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    fn get_unit(&self, _name: &str) -> OwnedObjectPath {
        PATH.try_into().unwrap()
    }
    fn subscribe(&self) {}
}

struct UnitState {
    active: Arc<AtomicBool>,
    reads: Arc<AtomicUsize>,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl UnitState {
    #[zbus(property)]
    fn load_state(&self) -> &str {
        "loaded"
    }
    #[zbus(property)]
    fn active_state(&self) -> &str {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.active.load(Ordering::SeqCst) {
            "active"
        } else {
            "inactive"
        }
    }
    #[zbus(property)]
    fn freezer_state(&self) -> &str {
        "running"
    }
}

struct ServiceState;
#[zbus::interface(name = "org.freedesktop.systemd1.Service")]
impl ServiceState {
    #[zbus(property)]
    fn exec_main_code(&self) -> i32 {
        0
    }
    #[zbus(property)]
    fn exec_main_status(&self) -> i32 {
        0
    }
    #[zbus(property)]
    fn status_text(&self) -> &str {
        "ready"
    }
    #[zbus(property)]
    fn control_group(&self) -> &str {
        "/korri.slice/korri-games.slice/game.service"
    }
    #[zbus(property, name = "MainPID")]
    fn main_pid(&self) -> u32 {
        1234
    }
    #[zbus(property)]
    fn environment(&self) -> Vec<String> {
        vec!["VALUE=spaces and \"quotes\"".into()]
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repeated_reads_use_the_property_cache_and_signals_update_it() {
    let bus = PrivateBus::start();
    let active = Arc::new(AtomicBool::new(true));
    let reads = Arc::new(AtomicUsize::new(0));
    let server = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("org.freedesktop.systemd1")
        .unwrap()
        .serve_at("/org/freedesktop/systemd1", Manager)
        .unwrap()
        .serve_at(
            PATH,
            UnitState {
                active: active.clone(),
                reads: reads.clone(),
            },
        )
        .unwrap()
        .serve_at(PATH, ServiceState)
        .unwrap()
        .build()
        .await
        .unwrap();
    let reader = Arc::new(NativeUnitReader::connect_to(Some(bus.address.clone())).unwrap());
    let launch = "01".repeat(16);
    let unit = format!("korri-game-{launch}.service");
    for _ in 0..10 {
        let snapshot = reader.read(&unit, Duration::from_secs(2)).unwrap().unwrap();
        assert_eq!(snapshot.values["ActiveState"], "active");
        assert_eq!(snapshot.values["MainPID"], "1234");
        assert_eq!(snapshot.environment, vec!["VALUE=spaces and \"quotes\""]);
    }
    use crate::host::{LaunchUnitBackend, LaunchUnitState, SystemdLaunchUnitBackend};
    let backend = SystemdLaunchUnitBackend::new(
        "/missing-systemd-run".into(),
        "/missing-systemctl".into(),
        1000,
        1000,
    )
    .unwrap()
    .with_native_observation(Arc::clone(&reader));
    assert_eq!(backend.state(&launch).unwrap(), LaunchUnitState::Running);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        1,
        "unchanged reads queried the daemon again"
    );
    active.store(false, Ordering::SeqCst);
    let interface = server
        .object_server()
        .interface::<_, UnitState>(PATH)
        .await
        .unwrap();
    interface
        .get()
        .await
        .active_state_changed(interface.signal_emitter())
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if reader
            .read(&unit, Duration::from_secs(2))
            .unwrap()
            .unwrap()
            .values["ActiveState"]
            == "inactive"
        {
            break;
        }
        assert!(Instant::now() < deadline, "property signal was not applied");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    // A local control operation discards observations before its caller reads
    // status again, even when its PropertiesChanged signal is not delivered.
    active.store(true, Ordering::SeqCst);
    reader.invalidate(Duration::from_secs(2)).unwrap();
    assert_eq!(
        reader
            .read(&unit, Duration::from_secs(2))
            .unwrap()
            .unwrap()
            .values["ActiveState"],
        "active"
    );
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    drop(interface);
    drop(reader);
}

async fn serve_unit(address: &str, active: bool) -> Connection {
    zbus::connection::Builder::address(address)
        .unwrap()
        .name("org.freedesktop.systemd1")
        .unwrap()
        .serve_at("/org/freedesktop/systemd1", Manager)
        .unwrap()
        .serve_at(
            PATH,
            UnitState {
                active: Arc::new(AtomicBool::new(active)),
                reads: Arc::new(AtomicUsize::new(0)),
            },
        )
        .unwrap()
        .serve_at(PATH, ServiceState)
        .unwrap()
        .build()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn losing_the_manager_drops_cached_facts_and_a_new_owner_resynchronizes() {
    let bus = PrivateBus::start();
    let server = serve_unit(&bus.address, true).await;
    let reader = NativeUnitReader::connect_to(Some(bus.address.clone())).unwrap();
    assert_eq!(
        reader
            .read("game.service", Duration::from_secs(2))
            .unwrap()
            .unwrap()
            .values["ActiveState"],
        "active"
    );
    server.close().await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while reader.read("game.service", Duration::from_secs(1)).is_ok() {
        assert!(
            Instant::now() < deadline,
            "cached facts survived manager loss"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let replacement = serve_unit(&bus.address, false).await;
    assert_eq!(
        reader
            .read("game.service", Duration::from_secs(2))
            .unwrap()
            .unwrap()
            .values["ActiveState"],
        "inactive"
    );
    drop(reader);
    replacement.close().await.unwrap();
}
