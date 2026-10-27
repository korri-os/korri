use super::*;
use std::{collections::HashMap, time::Duration};
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection, Proxy,
};

const NM: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP: &str = "org.freedesktop.NetworkManager.AccessPoint";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const SETTINGS: &str = "/org/freedesktop/NetworkManager/Settings";
type Settings = HashMap<String, HashMap<String, OwnedValue>>;

pub(super) mod nearby;

type Connect = Box<dyn Fn() -> BoxFuture<'static, zbus::Result<Connection>> + Send + Sync>;

/// NetworkManager over one system bus connection. The portal reads network
/// status every 2 s. A new connection for each read made korrid, dbus-daemon
/// and NetworkManager busy on an idle device (#289). Every failed operation
/// drops the connection, so the next operation connects again.
pub struct DbusNetworkManager {
    connect: Connect,
    bus: tokio::sync::Mutex<Option<Connection>>,
}
impl DbusNetworkManager {
    pub fn system() -> Self {
        Self::connecting_with(Box::new(|| Box::pin(Connection::system())))
    }
    fn connecting_with(connect: Connect) -> Self {
        Self {
            connect,
            bus: tokio::sync::Mutex::new(None),
        }
    }
    async fn bus(&self) -> zbus::Result<Connection> {
        let mut bus = self.bus.lock().await;
        if let Some(bus) = bus.as_ref() {
            return Ok(bus.clone());
        }
        let connection = (self.connect)().await?;
        *bus = Some(connection.clone());
        Ok(connection)
    }
    /// Run one operation on the shared connection within `limit`.
    async fn on_bus<'a, T>(
        &'a self,
        limit: Duration,
        operation: impl FnOnce(Connection) -> BoxFuture<'a, Result<T, NetworkFailure>>,
    ) -> Result<T, NetworkFailure> {
        let outcome = tokio::time::timeout(limit, async {
            let bus = self
                .bus()
                .await
                .map_err(|_| NetworkFailure::unavailable())?;
            operation(bus).await
        })
        .await
        .unwrap_or_else(|_| Err(NetworkFailure::timeout()));
        if outcome.is_err() {
            self.bus.lock().await.take();
        }
        outcome
    }
}

async fn proxy<'a>(
    bus: &'a Connection,
    path: &'a str,
    interface: &'a str,
) -> zbus::Result<Proxy<'a>> {
    // Each read is a fresh physical fact. In particular, polling must not
    // retain the State/LastScan value read before an activation or scan.
    zbus::proxy::Builder::new(bus)
        .destination(NM)?
        .path(path)?
        .interface(interface)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
}
async fn wifi_device(bus: &Connection) -> zbus::Result<Option<OwnedObjectPath>> {
    let devices: Vec<OwnedObjectPath> = proxy(bus, ROOT, NM).await?.call("GetDevices", &()).await?;
    for path in devices {
        if proxy(bus, path.as_str(), DEVICE)
            .await?
            .get_property::<u32>("DeviceType")
            .await?
            == 2
        {
            return Ok(Some(path));
        }
    }
    Ok(None)
}
async fn saved(bus: &Connection) -> zbus::Result<HashMap<String, OwnedObjectPath>> {
    let settings = proxy(bus, SETTINGS, "org.freedesktop.NetworkManager.Settings").await?;
    let paths: Vec<OwnedObjectPath> = settings.call("ListConnections", &()).await?;
    let mut saved = HashMap::new();
    for path in paths {
        // GetSettings deliberately excludes secrets. Never call GetSecrets.
        let values: Settings = proxy(
            bus,
            path.as_str(),
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?
        .call("GetSettings", &())
        .await?;
        if let Some(ssid) = values
            .get("802-11-wireless")
            .and_then(|wireless| wireless.get("ssid"))
        {
            if let Ok(bytes) = ssid.try_clone().and_then(Vec::<u8>::try_from) {
                if let Ok(ssid) = String::from_utf8(bytes) {
                    saved.insert(ssid, path);
                }
            }
        }
    }
    Ok(saved)
}
async fn access_point(
    bus: &Connection,
    path: &OwnedObjectPath,
    saved: &HashMap<String, OwnedObjectPath>,
) -> zbus::Result<Option<WifiNetwork>> {
    let ap = proxy(bus, path.as_str(), AP).await?;
    let bytes: Vec<u8> = ap.get_property("Ssid").await?;
    let Ok(ssid) = String::from_utf8(bytes) else {
        return Ok(None);
    };
    if ssid.is_empty() {
        return Ok(None);
    }
    Ok(Some(WifiNetwork {
        id: path.to_string(),
        known: saved.contains_key(&ssid),
        ssid,
        strength_percent: ap.get_property::<u8>("Strength").await?.min(100),
        secured: ap.get_property::<u32>("WpaFlags").await? != 0
            || ap.get_property::<u32>("RsnFlags").await? != 0,
    }))
}
fn merge_access_points(points: impl IntoIterator<Item = WifiNetwork>) -> Vec<WifiNetwork> {
    let mut strongest: HashMap<String, WifiNetwork> = HashMap::new();
    for point in points {
        let entry = strongest
            .entry(point.ssid.clone())
            .or_insert_with(|| point.clone());
        if point.strength_percent > entry.strength_percent {
            *entry = point;
        }
    }
    let mut networks: Vec<_> = strongest.into_values().collect();
    networks.sort_by(|a, b| {
        b.strength_percent
            .cmp(&a.strength_percent)
            .then_with(|| a.ssid.cmp(&b.ssid))
    });
    networks
}
async fn read(bus: &Connection) -> zbus::Result<NetworkSnapshot> {
    let manager = proxy(bus, ROOT, NM).await?;
    let primary: OwnedObjectPath = manager.get_property("PrimaryConnection").await?;
    let mut connection = NetworkConnection::Offline;
    if primary.as_str() != "/" {
        let active = proxy(bus, primary.as_str(), ACTIVE).await?;
        if active.get_property::<u32>("State").await? == 2 {
            if active.get_property::<String>("Type").await? == "802-11-wireless" {
                let point: OwnedObjectPath = active.get_property("SpecificObject").await?;
                if point.as_str() != "/" {
                    if let Some(point) = access_point(bus, &point, &HashMap::new()).await? {
                        connection = NetworkConnection::Wifi {
                            ssid: point.ssid,
                            strength_percent: Some(point.strength_percent),
                        };
                    }
                }
            } else {
                connection = NetworkConnection::Wired;
            }
        }
    }
    let wifi = if let Some(device) = wifi_device(bus).await? {
        let saved = saved(bus).await?;
        let paths: Vec<OwnedObjectPath> = proxy(bus, device.as_str(), WIRELESS)
            .await?
            .call("GetAllAccessPoints", &())
            .await?;
        let mut points = Vec::new();
        for path in paths {
            if let Some(point) = access_point(bus, &path, &saved).await? {
                points.push(point);
            }
        }
        Wifi::Available {
            scanning: false,
            networks: merge_access_points(points),
            attempt: WifiAttempt::Idle,
        }
    } else {
        Wifi::NoAdapter
    };
    Ok(NetworkSnapshot { connection, wifi })
}
async fn scan(bus: &Connection) -> Result<(), NetworkFailure> {
    let device = wifi_device(&bus)
        .await
        .map_err(|_| NetworkFailure::unavailable())?
        .ok_or_else(NetworkFailure::unavailable)?;
    let wireless = proxy(&bus, device.as_str(), WIRELESS)
        .await
        .map_err(|_| NetworkFailure::unavailable())?;
    let previous: i64 = wireless
        .get_property("LastScan")
        .await
        .map_err(|_| NetworkFailure::unavailable())?;
    wireless
        .call::<_, _, ()>("RequestScan", &(HashMap::<&str, Value<'_>>::new(),))
        .await
        .map_err(|_| NetworkFailure::unavailable())?;
    loop {
        if wireless
            .get_property::<i64>("LastScan")
            .await
            .map_err(|_| NetworkFailure::unavailable())?
            > previous
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

// Borrow the password directly for D-Bus serialization. No owned settings value
// or profile is retained in korrid. NetworkManager owns the activated profile.
fn activation_settings<'a>(
    ssid: &'a str,
    hidden: bool,
    password: Option<&'a str>,
) -> HashMap<&'a str, HashMap<&'a str, Value<'a>>> {
    let mut settings = HashMap::from([
        (
            "connection",
            HashMap::from([
                ("type", Value::from("802-11-wireless")),
                ("id", Value::from(ssid)),
            ]),
        ),
        (
            "802-11-wireless",
            HashMap::from([
                ("ssid", Value::from(ssid.as_bytes())),
                ("hidden", Value::from(hidden)),
            ]),
        ),
    ]);
    if let Some(password) = password {
        settings.insert(
            "802-11-wireless-security",
            HashMap::from([
                ("key-mgmt", Value::from("wpa-psk")),
                ("psk", Value::from(password)),
            ]),
        );
    }
    settings
}
async fn connect(
    bus: &Connection,
    target: &str,
    hidden: bool,
    password: Option<&str>,
) -> Result<(), NetworkFailure> {
    let device = wifi_device(&bus)
        .await
        .map_err(|_| NetworkFailure::unavailable())?
        .ok_or_else(NetworkFailure::unavailable)?;
    let manager = proxy(&bus, ROOT, NM)
        .await
        .map_err(|_| NetworkFailure::unavailable())?;
    let saved = saved(&bus)
        .await
        .map_err(|_| NetworkFailure::unavailable())?;
    let root = OwnedObjectPath::try_from("/").expect("root path");
    let (ssid, point) = if hidden {
        (target.to_owned(), root.clone())
    } else {
        let point = OwnedObjectPath::try_from(target).map_err(|_| NetworkFailure::unavailable())?;
        let network = access_point(&bus, &point, &saved)
            .await
            .map_err(|_| NetworkFailure::unavailable())?
            .ok_or_else(NetworkFailure::unavailable)?;
        (network.ssid, point)
    };
    let activation: zbus::Result<OwnedObjectPath> = if !hidden && saved.contains_key(&ssid) {
        manager
            .call(
                "ActivateConnection",
                &(saved.get(&ssid).expect("known profile"), &device, &point),
            )
            .await
    } else {
        let settings = activation_settings(&ssid, hidden, password);
        manager
            .call::<_, _, (OwnedObjectPath, OwnedObjectPath)>(
                "AddAndActivateConnection",
                &(&settings, &device, &point),
            )
            .await
            .map(|(_, active)| active)
    };
    let active = match activation {
        Ok(active) => active,
        Err(_) => {
            // Immediate activation errors may already have a device reason.
            // Do not expose the D-Bus error: it is dependency-controlled text.
            if let Ok(device) = proxy(&bus, device.as_str(), DEVICE).await {
                if let Ok((_, reason)) = device.get_property::<(u32, u32)>("StateReason").await {
                    if (7..=11).contains(&reason) {
                        return Err(NetworkFailure::state_reason(reason));
                    }
                }
            }
            return Err(NetworkFailure::unavailable());
        }
    };
    let active = proxy(&bus, active.as_str(), ACTIVE)
        .await
        .map_err(|_| NetworkFailure::unavailable())?;
    let device = proxy(&bus, device.as_str(), DEVICE)
        .await
        .map_err(|_| NetworkFailure::unavailable())?;
    loop {
        let state: u32 = active
            .get_property("State")
            .await
            .map_err(|_| NetworkFailure::unavailable())?;
        let (device_state, reason): (u32, u32) = device
            .get_property("StateReason")
            .await
            .map_err(|_| NetworkFailure::unavailable())?;
        if state == 2 {
            return Ok(());
        }
        if state == 4 || device_state == 120 {
            return Err(NetworkFailure::state_reason(reason));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
impl NetworkManager for DbusNetworkManager {
    fn read(&self) -> BoxFuture<'_, Result<NetworkSnapshot, NetworkFailure>> {
        Box::pin(self.on_bus(Duration::from_secs(5), |bus| {
            Box::pin(async move { read(&bus).await.map_err(|_| NetworkFailure::unavailable()) })
        }))
    }
    fn scan(&self) -> BoxFuture<'_, Result<(), NetworkFailure>> {
        Box::pin(self.on_bus(Duration::from_secs(15), |bus| {
            Box::pin(async move { scan(&bus).await })
        }))
    }
    fn connect<'a>(
        &'a self,
        target: &'a str,
        hidden: bool,
        password: Option<&'a str>,
    ) -> BoxFuture<'a, Result<(), NetworkFailure>> {
        Box::pin(self.on_bus(Duration::from_secs(35), move |bus| {
            Box::pin(async move { connect(&bus, target, hidden, password).await })
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strongest_ssid_wins_and_list_is_sorted() {
        let point = |id: &str, ssid: &str, strength| WifiNetwork {
            id: id.into(),
            ssid: ssid.into(),
            strength_percent: strength,
            secured: true,
            known: true,
        };
        let networks = merge_access_points([
            point("a", "Home", 20),
            point("b", "Cafe", 60),
            point("c", "Home", 90),
        ]);
        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].id, "c");
        assert_eq!(networks[1].ssid, "Cafe");
    }
    #[test]
    fn activation_uses_native_hidden_and_security_settings() {
        let settings = activation_settings("Hidden", true, Some("test-passphrase"));
        assert_eq!(settings["802-11-wireless"]["hidden"], Value::from(true));
        assert_eq!(
            settings["802-11-wireless-security"]["key-mgmt"],
            Value::from("wpa-psk")
        );
        assert_eq!(
            settings["802-11-wireless-security"]["psk"],
            Value::from("test-passphrase")
        );
        assert!(!activation_settings("Open", true, None).contains_key("802-11-wireless-security"));
    }
}

#[cfg(test)]
#[path = "dbus_tests.rs"]
pub(in crate::network) mod dbus_tests;
