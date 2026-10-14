//! Exercise the real zbus serializer, properties and activation calls on an
//! isolated Unix socket. No system bus, radio, profiles or user keys are touched.
use super::*;
use std::sync::{
    atomic::{AtomicI64, AtomicU32, Ordering},
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
struct State {
    connection_type: Mutex<String>,
    primary: Mutex<String>,
    activation: AtomicU32,
    reason: AtomicU32,
    scan: AtomicI64,
    calls: Mutex<Vec<&'static str>>,
}
struct Manager(Arc<State>);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![path(WIFI_PATH)]
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
        // Validate secrets at the receiver and then zeroize its local copy.
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
    fn request_scan(&self, _options: HashMap<String, OwnedValue>) {
        self.0.scan.fetch_add(1, Ordering::SeqCst);
    }
}
struct Point {
    ssid: Vec<u8>,
    strength: u8,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl Point {
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        self.ssid.clone()
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
    #[zbus(property, name = "Type")]
    fn connection_type(&self) -> String {
        self.0.connection_type.lock().unwrap().clone()
    }
    #[zbus(property)]
    fn specific_object(&self) -> OwnedObjectPath {
        path(AP_PATH)
    }
}
struct Saved;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl Saved {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        vec![path(PROFILE)]
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
async fn bus() -> (Connection, Connection, Arc<State>) {
    let state = Arc::new(State::default());
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
            },
        )
        .unwrap()
        .serve_at(
            "/weak",
            Point {
                ssid: b"Home".to_vec(),
                strength: 20,
            },
        )
        .unwrap()
        .serve_at(
            "/invalid",
            Point {
                ssid: vec![0xff],
                strength: 100,
            },
        )
        .unwrap()
        .serve_at(ACTIVE_PATH, Active(state.clone()))
        .unwrap()
        .serve_at(SETTINGS, Saved)
        .unwrap()
        .serve_at(PROFILE, Profile)
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
