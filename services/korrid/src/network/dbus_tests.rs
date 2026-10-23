//! The tests exercise real zbus serialization on an isolated Unix socket.
//! The tests do not access the system bus, a radio, saved profiles or user keys.
use super::*;
use std::sync::{
    atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering},
    Arc, Mutex,
};
use zbus::connection::Builder;

const WIFI_PATH: &str = "/wifi";
const AP_PATH: &str = "/ap";
const ACTIVE_PATH: &str = "/active";
const PROFILE: &str = "/profile";
fn path(value: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(value).unwrap()
}
#[derive(Default)]
pub(in crate::network) struct State {
    connection_type: Mutex<String>,
    primary: Mutex<String>,
    pub(in crate::network) activation: AtomicU32,
    reason: AtomicU32,
    scan: AtomicI64,
    pub(in crate::network) calls: Mutex<Vec<&'static str>>,
    pub(in crate::network) nearby_ssid: Mutex<Option<String>>,
    pub(in crate::network) nearby_visible: AtomicBool,
    delayed_scan: AtomicBool,
    pub(in crate::network) scan_requests: AtomicU32,
    stale: AtomicBool,
    checkpoint: AtomicBool,
    pub(in crate::network) home: AtomicBool,
    clock: AtomicU32,
    deadline: AtomicU32,
    pub(in crate::network) default_route: AtomicBool,
}
const NEARBY_PROFILE: &str = "/nearby_profile";
const CHECKPOINT: &str = "/checkpoint";
impl State {
    fn advance(&self, seconds: u32) {
        let now = self.clock.fetch_add(seconds, Ordering::SeqCst) + seconds;
        if self.checkpoint.load(Ordering::SeqCst) && now >= self.deadline.load(Ordering::SeqCst) {
            self.stale.store(false, Ordering::SeqCst);
            self.home.store(true, Ordering::SeqCst);
            self.checkpoint.store(false, Ordering::SeqCst);
            self.calls.lock().unwrap().push("TimedRollback");
        }
    }
}
struct Manager(Arc<State>);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![path(WIFI_PATH)]
    }
    #[zbus(property)]
    fn active_connections(&self) -> Vec<OwnedObjectPath> {
        vec![path(ACTIVE_PATH)]
    }
    #[zbus(property)]
    fn checkpoints(&self) -> Vec<OwnedObjectPath> {
        if self.0.checkpoint.load(Ordering::SeqCst) {
            vec![path(CHECKPOINT)]
        } else {
            vec![]
        }
    }
    fn checkpoint_create(
        &self,
        devices: Vec<OwnedObjectPath>,
        timeout: u32,
        flags: u32,
    ) -> OwnedObjectPath {
        assert_eq!(devices, vec![path(WIFI_PATH)]);
        assert_eq!(timeout, 120);
        assert_eq!(flags, 2);
        assert!(
            self.0.home.load(Ordering::SeqCst),
            "checkpoint captures the active home profile"
        );
        assert!(!self.0.checkpoint.swap(true, Ordering::SeqCst));
        self.0.deadline.store(
            self.0.clock.load(Ordering::SeqCst) + timeout,
            Ordering::SeqCst,
        );
        self.0.calls.lock().unwrap().push("CheckpointCreate");
        path(CHECKPOINT)
    }
    fn checkpoint_adjust_rollback_timeout(
        &self,
        checkpoint: OwnedObjectPath,
        timeout: u32,
    ) -> zbus::fdo::Result<()> {
        assert_eq!(checkpoint.as_str(), CHECKPOINT);
        if !self.0.checkpoint.load(Ordering::SeqCst) {
            return Err(zbus::fdo::Error::Failed("expired".into()));
        }
        assert_eq!(timeout, 120);
        self.0.deadline.store(
            self.0.clock.load(Ordering::SeqCst) + timeout,
            Ordering::SeqCst,
        );
        self.0
            .calls
            .lock()
            .unwrap()
            .push("CheckpointAdjustRollbackTimeout");
        Ok(())
    }
    fn checkpoint_rollback(&self, checkpoint: OwnedObjectPath) -> HashMap<String, u32> {
        assert_eq!(checkpoint.as_str(), CHECKPOINT);
        self.0.stale.store(false, Ordering::SeqCst);
        self.0.home.store(true, Ordering::SeqCst);
        self.0.calls.lock().unwrap().push("CheckpointRollback");
        HashMap::from([(WIFI_PATH.into(), 0)])
    }
    fn checkpoint_destroy(&self, checkpoint: OwnedObjectPath) {
        assert_eq!(checkpoint.as_str(), CHECKPOINT);
        assert!(self.0.home.load(Ordering::SeqCst));
        self.0.checkpoint.store(false, Ordering::SeqCst);
        self.0.calls.lock().unwrap().push("CheckpointDestroy");
    }
    fn add_and_activate_connection2(
        &self,
        settings: Settings,
        device: OwnedObjectPath,
        point: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
    ) -> (
        OwnedObjectPath,
        OwnedObjectPath,
        HashMap<String, OwnedValue>,
    ) {
        assert!(
            self.0.checkpoint.load(Ordering::SeqCst),
            "restore must be armed first"
        );
        assert_eq!(device.as_str(), WIFI_PATH);
        assert_eq!(<&str>::try_from(&options["persist"]).unwrap(), "volatile");
        assert_eq!(
            <&str>::try_from(&settings["connection"]["id"]).unwrap(),
            nearby::PROFILE_ID
        );
        assert!(!bool::try_from(&settings["connection"]["autoconnect"]).unwrap());
        assert!(bool::try_from(&settings["802-11-wireless"]["hidden"]).unwrap());
        let ssid: Vec<u8> = settings["802-11-wireless"]["ssid"]
            .try_clone()
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(
            ssid,
            self.0
                .nearby_ssid
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .as_bytes()
        );
        let mode = <&str>::try_from(&settings["802-11-wireless"]["mode"]).unwrap();
        assert!(matches!(mode, "ap" | "infrastructure"));
        assert_eq!(point.as_str(), if mode == "ap" { "/" } else { AP_PATH });
        assert_eq!(
            <&str>::try_from(&settings["802-11-wireless-security"]["key-mgmt"]).unwrap(),
            "wpa-psk"
        );
        for (setting, expected) in [("proto", "rsn"), ("pairwise", "ccmp"), ("group", "ccmp")] {
            let values: Vec<String> = settings["802-11-wireless-security"][setting]
                .try_clone()
                .unwrap()
                .try_into()
                .unwrap();
            assert_eq!(values, vec![expected]);
        }
        let password = <&str>::try_from(&settings["802-11-wireless-security"]["psk"]).unwrap();
        assert_eq!(password.len(), 60);
        assert_eq!(
            <&str>::try_from(&settings["ipv4"]["method"]).unwrap(),
            "manual"
        );
        assert!(bool::try_from(&settings["ipv4"]["never-default"]).unwrap());
        assert!(bool::try_from(&settings["ipv4"]["ignore-auto-dns"]).unwrap());
        assert!(!settings["ipv4"].contains_key("gateway"));
        assert!(!settings["ipv4"].contains_key("dns"));
        let addresses: Vec<HashMap<String, OwnedValue>> = settings["ipv4"]["address-data"]
            .try_clone()
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(addresses.len(), 1);
        assert_eq!(
            <&str>::try_from(&addresses[0]["address"]).unwrap(),
            if mode == "ap" {
                "10.77.77.1"
            } else {
                "10.77.77.2"
            }
        );
        assert_eq!(u32::try_from(&addresses[0]["prefix"]).unwrap(), 24);
        assert_eq!(
            <&str>::try_from(&settings["ipv6"]["method"]).unwrap(),
            "disabled"
        );
        self.0.stale.store(true, Ordering::SeqCst);
        self.0.home.store(false, Ordering::SeqCst);
        self.0.calls.lock().unwrap().push(if mode == "ap" {
            "NearbyAP"
        } else {
            "NearbyClient"
        });
        (path(NEARBY_PROFILE), path(ACTIVE_PATH), HashMap::new())
    }
    #[zbus(property)]
    fn primary_connection(&self) -> OwnedObjectPath {
        path(&self.0.primary.lock().unwrap())
    }
    fn activate_connection(
        &self,
        connection: OwnedObjectPath,
        device: OwnedObjectPath,
        point: OwnedObjectPath,
    ) -> OwnedObjectPath {
        assert_eq!(connection.as_str(), PROFILE);
        assert_eq!(device.as_str(), WIFI_PATH);
        assert_eq!(point.as_str(), AP_PATH);
        self.0.calls.lock().unwrap().push("ActivateConnection");
        path(ACTIVE_PATH)
    }
    fn add_and_activate_connection(
        &self,
        settings: Settings,
        device: OwnedObjectPath,
        point: OwnedObjectPath,
    ) -> (OwnedObjectPath, OwnedObjectPath) {
        assert_eq!(device.as_str(), WIFI_PATH);
        assert!(matches!(point.as_str(), "/" | AP_PATH));
        assert!(settings.contains_key("802-11-wireless"));
        // The receiver validates secrets. It then zeroizes its local copy.
        // The recording contains method names only, never settings or keys.
        if let Some(security) = settings.get("802-11-wireless-security") {
            assert_eq!(<&str>::try_from(&security["key-mgmt"]).unwrap(), "wpa-psk");
            let mut password = String::try_from(security["psk"].try_clone().unwrap()).unwrap();
            assert_eq!(password, "test-passphrase");
            use zeroize::Zeroize;
            password.zeroize();
        }
        self.0
            .calls
            .lock()
            .unwrap()
            .push("AddAndActivateConnection");
        (path(PROFILE), path(ACTIVE_PATH))
    }
}
struct Device(Arc<State>);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn state_reason(&self) -> (u32, u32) {
        (
            if self.0.reason.load(Ordering::SeqCst) == 0 {
                100
            } else {
                120
            },
            self.0.reason.load(Ordering::SeqCst),
        )
    }
}
struct Wireless(Arc<State>);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    fn get_all_access_points(&self) -> Vec<OwnedObjectPath> {
        vec![path(AP_PATH), path("/weak"), path("/invalid")]
    }
    #[zbus(property)]
    fn last_scan(&self) -> i64 {
        self.0.scan.load(Ordering::SeqCst)
    }
    fn request_scan(&self, options: HashMap<String, OwnedValue>) {
        if let Some(ssid) = self.0.nearby_ssid.lock().unwrap().as_ref() {
            let ssids: Vec<Vec<u8>> = options["ssids"].try_clone().unwrap().try_into().unwrap();
            assert_eq!(ssids, vec![ssid.as_bytes()]);
        }
        self.0.scan_requests.fetch_add(1, Ordering::SeqCst);
        if !self.0.delayed_scan.load(Ordering::SeqCst) {
            self.0.scan.fetch_add(1, Ordering::SeqCst);
        }
    }
}
struct Point {
    ssid: Vec<u8>,
    strength: u8,
    nearby: Option<Arc<State>>,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl Point {
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        self.nearby
            .as_ref()
            .and_then(|state| {
                state.nearby_ssid.lock().unwrap().as_ref().map(|ssid| {
                    if state.nearby_visible.load(Ordering::SeqCst) {
                        ssid.as_bytes().to_vec()
                    } else {
                        vec![]
                    }
                })
            })
            .unwrap_or_else(|| self.ssid.clone())
    }
    #[zbus(property)]
    fn strength(&self) -> u8 {
        self.strength
    }
    #[zbus(property)]
    fn wpa_flags(&self) -> u32 {
        1
    }
    #[zbus(property)]
    fn rsn_flags(&self) -> u32 {
        0
    }
}
struct Active(Arc<State>);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.0.activation.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn default(&self) -> bool {
        self.0.default_route.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn default6(&self) -> bool {
        false
    }
    #[zbus(property, name = "Type")]
    fn connection_type(&self) -> String {
        self.0.connection_type.lock().unwrap().clone()
    }
    #[zbus(property)]
    fn specific_object(&self) -> OwnedObjectPath {
        path(AP_PATH)
    }
}
struct Saved(Arc<State>);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl Saved {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        if self.0.stale.load(Ordering::SeqCst) {
            vec![path(PROFILE), path(NEARBY_PROFILE)]
        } else {
            vec![path(PROFILE)]
        }
    }
}
struct Profile;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Profile {
    fn get_settings(&self) -> Settings {
        HashMap::from([(
            "802-11-wireless".into(),
            HashMap::from([(
                "ssid".into(),
                Value::from(b"Home".as_slice()).try_to_owned().unwrap(),
            )]),
        )])
    }
}
pub(in crate::network) async fn bus() -> (Connection, Connection, Arc<State>) {
    let state = Arc::new(State::default());
    state.nearby_visible.store(true, Ordering::SeqCst);
    state.home.store(true, Ordering::SeqCst);
    *state.primary.lock().unwrap() = ACTIVE_PATH.into();
    *state.connection_type.lock().unwrap() = "802-11-wireless".into();
    state.activation.store(2, Ordering::SeqCst);
    let (server, client) = tokio::net::UnixStream::pair().unwrap();
    let guid = zbus::Guid::generate();
    let server = Builder::unix_stream(server)
        .server(guid)
        .unwrap()
        .p2p()
        .serve_at(ROOT, Manager(state.clone()))
        .unwrap()
        .serve_at(WIFI_PATH, Device(state.clone()))
        .unwrap()
        .serve_at(WIFI_PATH, Wireless(state.clone()))
        .unwrap()
        .serve_at(
            AP_PATH,
            Point {
                ssid: b"Home".to_vec(),
                strength: 90,
                nearby: Some(state.clone()),
            },
        )
        .unwrap()
        .serve_at(
            "/weak",
            Point {
                ssid: b"Home".to_vec(),
                strength: 20,
                nearby: None,
            },
        )
        .unwrap()
        .serve_at(
            "/invalid",
            Point {
                ssid: vec![0xff],
                strength: 100,
                nearby: None,
            },
        )
        .unwrap()
        .serve_at(ACTIVE_PATH, Active(state.clone()))
        .unwrap()
        .serve_at(SETTINGS, Saved(state.clone()))
        .unwrap()
        .serve_at(PROFILE, Profile)
        .unwrap()
        .serve_at(NEARBY_PROFILE, NearbyProfile(state.clone()))
        .unwrap()
        .serve_at(CHECKPOINT, Checkpoint)
        .unwrap()
        .build();
    let client = Builder::unix_stream(client).p2p().build();
    let (server, client) = tokio::try_join!(server, client).unwrap();
    (server, client, state)
}
#[tokio::test]
async fn dbus_reads_primary_merges_utf8_ssids_marks_saved_and_uses_native_activation() {
    let (_server, client, state) = bus().await;
    let snapshot = read(&client).await.unwrap();
    assert!(
        matches!(snapshot.connection, NetworkConnection::Wifi { ssid, strength_percent: Some(90) } if ssid == "Home")
    );
    let Wifi::Available { networks, .. } = snapshot.wifi else {
        panic!("adapter")
    };
    assert_eq!(networks.len(), 1);
    assert!(networks[0].known && networks[0].secured);
    assert_eq!(networks[0].id, AP_PATH);
    tokio::time::timeout(Duration::from_secs(2), scan(&client))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.scan.load(Ordering::SeqCst), 1);
    connect(&client, AP_PATH, false, None).await.unwrap();
    connect(&client, "Hidden", true, Some("test-passphrase"))
        .await
        .unwrap();
    connect(&client, "Open hidden", true, None).await.unwrap();
    assert_eq!(
        *state.calls.lock().unwrap(),
        [
            "ActivateConnection",
            "AddAndActivateConnection",
            "AddAndActivateConnection"
        ]
    );
    *state.connection_type.lock().unwrap() = "802-3-ethernet".into();
    assert!(matches!(
        read(&client).await.unwrap().connection,
        NetworkConnection::Wired
    ));
    *state.primary.lock().unwrap() = "/".into();
    assert!(matches!(
        read(&client).await.unwrap().connection,
        NetworkConnection::Offline
    ));
}
#[tokio::test]
async fn dbus_failed_activation_reports_device_reason_and_pending_activation_is_bounded() {
    let (_server, client, state) = bus().await;
    state.activation.store(4, Ordering::SeqCst);
    state.reason.store(7, Ordering::SeqCst);
    assert!(
        connect(&client, AP_PATH, false, None)
            .await
            .unwrap_err()
            .password_rejected
    );
    state.activation.store(1, Ordering::SeqCst);
    state.reason.store(0, Ordering::SeqCst);
    assert!(tokio::time::timeout(
        Duration::from_millis(50),
        connect(&client, AP_PATH, false, None)
    )
    .await
    .is_err());
}

struct NearbyProfile(Arc<State>);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl NearbyProfile {
    fn get_settings(&self) -> Settings {
        HashMap::from([(
            "connection".into(),
            HashMap::from([(
                "id".into(),
                Value::from(nearby::PROFILE_ID).try_to_owned().unwrap(),
            )]),
        )])
    }
    fn delete(&self) {
        self.0.stale.store(false, Ordering::SeqCst);
        self.0.calls.lock().unwrap().push("DeleteNearby");
    }
}
struct Checkpoint;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Checkpoint")]
impl Checkpoint {
    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        vec![path(WIFI_PATH)]
    }
}

#[tokio::test]
async fn nearby_dbus_starts_volatile_hidden_ap_and_restores_home_on_stop_and_failure() {
    for activation in [2, 4] {
        let (_server, client, state) = bus().await;
        *state.nearby_ssid.lock().unwrap() = Some("derived-test-ssid".into());
        state.activation.store(activation, Ordering::SeqCst);
        let checkpoint = nearby::checkpoint(&client).await.unwrap();
        let result =
            nearby::activate(&client, "derived-test-ssid", &"a".repeat(60), true, None).await;
        assert_eq!(result.is_ok(), activation == 2);
        assert!(!state.home.load(Ordering::SeqCst));
        assert!(state.stale.load(Ordering::SeqCst));
        nearby::restore(&client, &checkpoint).await.unwrap();
        assert!(state.home.load(Ordering::SeqCst));
        assert!(!state.stale.load(Ordering::SeqCst));
        assert_eq!(
            *state.calls.lock().unwrap(),
            [
                "CheckpointCreate",
                "NearbyAP",
                "CheckpointRollback",
                "CheckpointDestroy"
            ]
        );
    }
}

#[tokio::test]
async fn nearby_dbus_waits_for_fresh_directed_scan_then_connects_to_derived_hidden_ssid() {
    let (_server, client, state) = bus().await;
    *state.nearby_ssid.lock().unwrap() = Some("derived-test-ssid".into());
    state.delayed_scan.store(true, Ordering::SeqCst);
    let scan = nearby::scan_for(&client, "derived-test-ssid");
    tokio::pin!(scan);
    assert!(tokio::time::timeout(Duration::from_millis(50), &mut scan)
        .await
        .is_err());
    assert_eq!(state.scan_requests.load(Ordering::SeqCst), 1);
    state.scan.fetch_add(1, Ordering::SeqCst);
    let point = scan.await.unwrap().unwrap();
    assert_eq!(point.as_str(), AP_PATH);
    let checkpoint = nearby::checkpoint(&client).await.unwrap();
    nearby::activate(
        &client,
        "derived-test-ssid",
        &"a".repeat(60),
        false,
        Some(&point),
    )
    .await
    .unwrap();
    nearby::restore(&client, &checkpoint).await.unwrap();
    assert_eq!(
        *state.calls.lock().unwrap(),
        [
            "CheckpointCreate",
            "NearbyClient",
            "CheckpointRollback",
            "CheckpointDestroy"
        ]
    );
}

#[tokio::test]
async fn nearby_dbus_refresh_extends_deadline_and_missing_refresh_restores_home() {
    let (_server, client, state) = bus().await;
    *state.nearby_ssid.lock().unwrap() = Some("derived-test-ssid".into());
    let checkpoint = nearby::checkpoint(&client).await.unwrap();
    let active = nearby::activate(&client, "derived-test-ssid", &"a".repeat(60), true, None)
        .await
        .unwrap();
    let session = nearby::Session { checkpoint, active };
    state.advance(100);
    nearby::refresh(&client, &session).await.unwrap();
    state.advance(100);
    assert!(!state.home.load(Ordering::SeqCst));
    state.advance(21);
    assert!(state.home.load(Ordering::SeqCst));
    assert!(!state.stale.load(Ordering::SeqCst));
    assert!(nearby::refresh(&client, &session).await.is_err());
    nearby::restore(&client, &session.checkpoint).await.unwrap();
}

#[tokio::test]
async fn nearby_dbus_startup_rolls_back_stale_connection_without_touching_home_profile() {
    let (_server, client, state) = bus().await;
    nearby::recover_stale(&client).await.unwrap();
    assert!(state.calls.lock().unwrap().is_empty());
    let _checkpoint = nearby::checkpoint(&client).await.unwrap();
    state.stale.store(true, Ordering::SeqCst);
    nearby::recover_stale(&client).await.unwrap();
    assert!(state.home.load(Ordering::SeqCst));
    assert!(!state.stale.load(Ordering::SeqCst));
    assert_eq!(
        *state.calls.lock().unwrap(),
        [
            "CheckpointCreate",
            "CheckpointRollback",
            "CheckpointDestroy"
        ]
    );
    // NM's timed rollback removes the checkpoint.
    // The startup check removes orphan volatile profiles.
    // It does not delete a saved home profile.
    state.stale.store(true, Ordering::SeqCst);
    nearby::recover_stale(&client).await.unwrap();
    assert_eq!(state.calls.lock().unwrap().last(), Some(&"DeleteNearby"));
}

#[tokio::test]
async fn nearby_dbus_reads_default_route_fact() {
    let (_server, client, state) = bus().await;
    assert!(!nearby::has_default_route(&client).await.unwrap());
    state.default_route.store(true, Ordering::SeqCst);
    assert!(nearby::has_default_route(&client).await.unwrap());
}
