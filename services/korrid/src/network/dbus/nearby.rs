//! NetworkManager's volatile nearby link and crash-safe home restoration.
use super::*;

pub(in crate::network) const PROFILE_ID: &str = "korri-nearby";
pub(in crate::network) const ROLLBACK_SECONDS: u32 = 120;
const PROFILE_INTERFACE: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const CHECKPOINT_INTERFACE: &str = "org.freedesktop.NetworkManager.Checkpoint";

pub(in crate::network) struct Session {
    pub checkpoint: OwnedObjectPath,
    pub active: OwnedObjectPath,
}

pub(in crate::network) async fn has_default_route(bus: &Connection) -> zbus::Result<bool> {
    let paths: Vec<OwnedObjectPath> = proxy(bus, ROOT, NM)
        .await?
        .get_property("ActiveConnections")
        .await?;
    for path in paths {
        let active = proxy(bus, path.as_str(), ACTIVE).await?;
        if active.get_property::<bool>("Default").await?
            || active.get_property::<bool>("Default6").await?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(in crate::network) async fn scan_for(
    bus: &Connection,
    ssid: &str,
) -> zbus::Result<Option<OwnedObjectPath>> {
    let device = wifi_device(bus)
        .await?
        .ok_or(zbus::Error::Failure("no Wi-Fi adapter".into()))?;
    let wireless = proxy(bus, device.as_str(), WIRELESS).await?;
    let previous: i64 = wireless.get_property("LastScan").await?;
    // A hidden beacon does not carry its SSID. A directed probe supplies it.
    // korrid reads the AP list after NM completes this scan.
    wireless
        .call::<_, _, ()>(
            "RequestScan",
            &(HashMap::from([(
                "ssids",
                Value::from(vec![ssid.as_bytes().to_vec()]),
            )]),),
        )
        .await?;
    loop {
        if wireless.get_property::<i64>("LastScan").await? > previous {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let points: Vec<OwnedObjectPath> = wireless.call("GetAllAccessPoints", &()).await?;
    for point in points {
        let bytes: Vec<u8> = proxy(bus, point.as_str(), AP)
            .await?
            .get_property("Ssid")
            .await?;
        if bytes == ssid.as_bytes() {
            return Ok(Some(point));
        }
    }
    Ok(None)
}

pub(in crate::network) async fn checkpoint(bus: &Connection) -> zbus::Result<OwnedObjectPath> {
    let device = wifi_device(bus)
        .await?
        .ok_or(zbus::Error::Failure("no Wi-Fi adapter".into()))?;
    // NM owns the rollback timer. NM restores home Wi-Fi if korrid crashes.
    // DELETE_NEW_CONNECTIONS also removes the volatile nearby profile.
    proxy(bus, ROOT, NM)
        .await?
        .call("CheckpointCreate", &(vec![device], ROLLBACK_SECONDS, 2u32))
        .await
}

fn settings<'a>(
    ssid: &'a str,
    password: &'a str,
    starter: bool,
) -> HashMap<&'a str, HashMap<&'a str, Value<'a>>> {
    let mut settings = activation_settings(ssid, true, Some(password));
    let security = settings
        .get_mut("802-11-wireless-security")
        .expect("security settings");
    security.insert("proto", Value::from(vec!["rsn"]));
    security.insert("pairwise", Value::from(vec!["ccmp"]));
    security.insert("group", Value::from(vec!["ccmp"]));
    let connection = settings.get_mut("connection").expect("connection settings");
    connection.insert("id", Value::from(PROFILE_ID));
    connection.insert("autoconnect", Value::from(false));
    let wireless = settings
        .get_mut("802-11-wireless")
        .expect("wireless settings");
    wireless.insert(
        "mode",
        Value::from(if starter { "ap" } else { "infrastructure" }),
    );
    if starter {
        // The probe exercised these radio settings on ath11k and ath12k.
        wireless.insert("band", Value::from("a"));
        wireless.insert("channel", Value::from(36u32));
    }
    settings.insert(
        "ipv4",
        HashMap::from([
            ("method", Value::from("manual")),
            ("never-default", Value::from(true)),
            ("ignore-auto-dns", Value::from(true)),
            (
                "address-data",
                Value::from(vec![HashMap::from([
                    (
                        "address",
                        Value::from(if starter { "10.77.77.1" } else { "10.77.77.2" }),
                    ),
                    ("prefix", Value::from(24u32)),
                ])]),
            ),
        ]),
    );
    settings.insert("ipv6", HashMap::from([("method", Value::from("disabled"))]));
    settings
}

pub(in crate::network) async fn activate(
    bus: &Connection,
    ssid: &str,
    password: &str,
    starter: bool,
    point: Option<&OwnedObjectPath>,
) -> zbus::Result<OwnedObjectPath> {
    let device = wifi_device(bus)
        .await?
        .ok_or(zbus::Error::Failure("no Wi-Fi adapter".into()))?;
    let root = OwnedObjectPath::try_from("/")?;
    let (_profile, active, _result): (
        OwnedObjectPath,
        OwnedObjectPath,
        HashMap<String, OwnedValue>,
    ) = proxy(bus, ROOT, NM)
        .await?
        .call(
            "AddAndActivateConnection2",
            &(
                settings(ssid, password, starter),
                device,
                point.unwrap_or(&root),
                HashMap::from([("persist", Value::from("volatile"))]),
            ),
        )
        .await?;
    loop {
        match proxy(bus, active.as_str(), ACTIVE)
            .await?
            .get_property::<u32>("State")
            .await?
        {
            2 => return Ok(active),
            4 => return Err(zbus::Error::Failure("nearby activation failed".into())),
            _ => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }
}

pub(in crate::network) async fn refresh(bus: &Connection, session: &Session) -> zbus::Result<()> {
    if proxy(bus, session.active.as_str(), ACTIVE)
        .await?
        .get_property::<u32>("State")
        .await?
        != 2
    {
        return Err(zbus::Error::Failure("nearby link ended".into()));
    }
    proxy(bus, ROOT, NM)
        .await?
        .call(
            "CheckpointAdjustRollbackTimeout",
            &(&session.checkpoint, ROLLBACK_SECONDS),
        )
        .await
}

pub(in crate::network) async fn restore(
    bus: &Connection,
    checkpoint: &OwnedObjectPath,
) -> zbus::Result<()> {
    let manager = proxy(bus, ROOT, NM).await?;
    let checkpoints: Vec<OwnedObjectPath> = manager.get_property("Checkpoints").await?;
    if !checkpoints.contains(checkpoint) {
        // NM's rollback timer already restored the network.
        // NM removed the checkpoint.
        return Ok(());
    }
    let results: HashMap<String, u32> = manager.call("CheckpointRollback", &(checkpoint,)).await?;
    if results.values().any(|result| *result != 0) {
        // korrid keeps the checkpoint if NM cannot restore a device.
        return Err(zbus::Error::Failure("nearby rollback failed".into()));
    }
    let remaining: Vec<OwnedObjectPath> = manager.get_property("Checkpoints").await?;
    if remaining.contains(checkpoint) {
        manager
            .call::<_, _, ()>("CheckpointDestroy", &(checkpoint,))
            .await?;
    }
    Ok(())
}

pub(in crate::network) async fn recover_stale(bus: &Connection) -> zbus::Result<()> {
    let paths: Vec<OwnedObjectPath> =
        proxy(bus, SETTINGS, "org.freedesktop.NetworkManager.Settings")
            .await?
            .call("ListConnections", &())
            .await?;
    let mut stale = Vec::new();
    for path in paths {
        let values: Settings = proxy(bus, path.as_str(), PROFILE_INTERFACE)
            .await?
            .call("GetSettings", &())
            .await?;
        if values
            .get("connection")
            .and_then(|c| c.get("id"))
            .and_then(|id| <&str>::try_from(id).ok())
            == Some(PROFILE_ID)
        {
            stale.push(path);
        }
    }
    if stale.is_empty() {
        return Ok(());
    }
    let device = wifi_device(bus)
        .await?
        .ok_or(zbus::Error::Failure("no Wi-Fi adapter".into()))?;
    let checkpoints: Vec<OwnedObjectPath> = proxy(bus, ROOT, NM)
        .await?
        .get_property("Checkpoints")
        .await?;
    for checkpoint in checkpoints {
        let devices: Vec<OwnedObjectPath> = proxy(bus, checkpoint.as_str(), CHECKPOINT_INTERFACE)
            .await?
            .get_property("Devices")
            .await?;
        if devices.contains(&device) {
            restore(bus, &checkpoint).await?;
        }
    }
    // korrid reads the profile list again after rollback.
    // NM can remove profiles during restoration.
    let remaining: Vec<OwnedObjectPath> =
        proxy(bus, SETTINGS, "org.freedesktop.NetworkManager.Settings")
            .await?
            .call("ListConnections", &())
            .await?;
    for path in stale {
        if remaining.contains(&path) {
            proxy(bus, path.as_str(), PROFILE_INTERFACE)
                .await?
                .call::<_, _, ()>("Delete", &())
                .await?;
        }
    }
    Ok(())
}
