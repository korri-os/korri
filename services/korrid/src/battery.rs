//! This device's battery for a surface (ADR 0094). korrid reads
//! `/sys/class/power_supply` once at start and then only when the kernel
//! sends a `power_supply` uevent (`NETLINK_KOBJECT_UEVENT`). No timer reads it.
//!
//! The primary supply is legacy `readBatterySnapshot`'s
//! (`product/apps/portal/api/stream-control/device-control-service.ts` on
//! `legacy`): the first supply of `type` "Battery" that has a capacity, else
//! the first "Battery". `charging` is its `status` "Charging". The percent
//! goes out only when the device lists "percent" in
//! `services.korriProduct.battery.verified`: a driver that sends no uevent
//! when the capacity moves would otherwise leave a stale percent on screen.
use std::{io, path::Path};

use korri_surface_treaty::SurfaceBattery;

/// The kernel's power supply class.
pub const POWER_SUPPLY: &str = "/sys/class/power_supply";

/// The device's verified battery facts as JSON, from
/// `services.korriProduct.battery.verified` (`nix/product/server.nix`).
pub const VERIFIED_ENV: &str = "KORRID_BATTERY_VERIFIED";

/// The battery facts whose kernel events a device has verified.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Verified {
    /// The driver sends a uevent when the capacity percent changes.
    pub percent: bool,
}

impl Verified {
    /// The JSON list that the device's Nix module writes. No value is the
    /// empty list. A value that is not a list of known facts is refused, so
    /// a wrong value can never show a percent.
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let facts: Vec<String> = serde_json::from_str(value)
            .map_err(|_| format!("{VERIFIED_ENV} must be a JSON list of strings"))?;
        let mut verified = Self::default();
        for fact in facts {
            match fact.as_str() {
                "percent" => verified.percent = true,
                _ => return Err(format!("{VERIFIED_ENV} names an unknown fact")),
            }
        }
        Ok(verified)
    }
}

struct Supply {
    capacity: Option<u8>,
    status: Option<String>,
}

/// A file's trimmed text. A missing file or an empty one is `None`; any
/// other failure is an error.
fn optional_text(path: &Path) -> io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let text = text.trim();
            Ok((!text.is_empty()).then(|| text.to_owned()))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The primary battery under `root`, or `None` when there is no battery.
fn primary(root: &Path) -> io::Result<Option<Supply>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(root)? {
        names.push(entry?.file_name());
    }
    names.sort();
    let mut batteries = Vec::new();
    for name in names {
        let supply = root.join(&name);
        if optional_text(&supply.join("type"))?.as_deref() != Some("Battery") {
            continue;
        }
        let capacity = optional_text(&supply.join("capacity"))?
            .and_then(|text| text.parse::<u8>().ok())
            .filter(|percent| *percent <= 100);
        batteries.push(Supply {
            capacity,
            status: optional_text(&supply.join("status"))?,
        });
    }
    let first_with_capacity = batteries.iter().position(|b| b.capacity.is_some());
    Ok(match first_with_capacity {
        Some(index) => Some(batteries.swap_remove(index)),
        None => batteries.into_iter().next(),
    })
}

/// The battery as a surface sees it. `None` when `root` holds no battery or
/// cannot be read.
pub fn read(root: &Path, verified: Verified) -> Option<SurfaceBattery> {
    let supply = primary(root).ok()??;
    Some(SurfaceBattery {
        percent: supply.capacity.filter(|_| verified.percent).map(f64::from),
        charging: supply.status.as_deref() == Some("Charging"),
    })
}

/// Whether one `NETLINK_KOBJECT_UEVENT` datagram is the kernel's event for
/// a power supply: `ACTION@DEVPATH`, then `KEY=VALUE` fields, each ended by
/// a zero byte, one of them `SUBSYSTEM=power_supply`. udev's own messages
/// (`libudev` magic) are not kernel events.
pub fn is_power_supply_uevent(datagram: &[u8]) -> bool {
    let mut fields = datagram.split(|byte| *byte == 0);
    let Some(header) = fields.next() else {
        return false;
    };
    if header.starts_with(b"libudev") || !header.contains(&b'@') {
        return false;
    }
    fields.any(|field| field == b"SUBSYSTEM=power_supply")
}

/// Publish `next` only when it differs from what the surface has.
pub fn publish(
    sender: &tokio::sync::watch::Sender<Option<SurfaceBattery>>,
    next: Option<SurfaceBattery>,
) -> bool {
    sender.send_if_modified(|current| {
        if *current == next {
            return false;
        }
        *current = next;
        true
    })
}

/// This device's battery, read by one thread for the whole process: once at
/// start, then on each kernel power_supply uevent.
#[cfg(target_os = "linux")]
pub fn device() -> tokio::sync::watch::Receiver<Option<SurfaceBattery>> {
    use std::sync::OnceLock;
    static BATTERY: OnceLock<tokio::sync::watch::Receiver<Option<SurfaceBattery>>> =
        OnceLock::new();
    BATTERY
        .get_or_init(|| {
            let verified = Verified::parse(std::env::var(VERIFIED_ENV).ok().as_deref())
                .unwrap_or_else(|message| {
                    eprintln!("korrid: battery percent off: {message}");
                    Verified::default()
                });
            let (sender, receiver) = tokio::sync::watch::channel(None);
            let started = std::thread::Builder::new()
                .name("korrid-battery".into())
                .spawn(move || listen(Path::new(POWER_SUPPLY), verified, &sender));
            if started.is_err() {
                eprintln!("korrid: battery reader did not start");
            }
            receiver
        })
        .clone()
}

/// Subscribe before the first read, so no event between the two is lost.
/// When the kernel's events cannot be received, the first read stays.
#[cfg(target_os = "linux")]
fn listen(
    root: &Path,
    verified: Verified,
    sender: &tokio::sync::watch::Sender<Option<SurfaceBattery>>,
) {
    use netlink_sys::{protocols::NETLINK_KOBJECT_UEVENT, Socket, SocketAddr};
    // Multicast group 1: the kernel's own uevents.
    let socket = Socket::new(NETLINK_KOBJECT_UEVENT).and_then(|mut socket| {
        socket.bind(&SocketAddr::new(0, 1))?;
        Ok(socket)
    });
    publish(sender, read(root, verified));
    let socket = match socket {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!("korrid: battery events unavailable: {error}");
            return;
        }
    };
    let mut buffer = vec![0u8; 8192];
    loop {
        let (length, from) = match socket.recv_from(&mut &mut buffer[..], 0) {
            Ok(received) => received,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            // The kernel dropped events for a full queue: one of them may
            // have been the battery's.
            Err(error) if error.raw_os_error() == Some(libc::ENOBUFS) => {
                publish(sender, read(root, verified));
                continue;
            }
            Err(error) => {
                eprintln!("korrid: battery events ended: {error}");
                return;
            }
        };
        if from.port_number() == 0 && is_power_supply_uevent(&buffer[..length.min(buffer.len())]) {
            publish(sender, read(root, verified));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn supply(root: &Path, name: &str, files: &[(&str, &str)]) {
        let directory = root.join(name);
        fs::create_dir_all(&directory).unwrap();
        for (file, text) in files {
            fs::write(directory.join(file), format!("{text}\n")).unwrap();
        }
    }

    const PERCENT: Verified = Verified { percent: true };

    #[test]
    fn a_uevent_counts_only_from_the_kernel_for_a_power_supply() {
        let event = b"change@/devices/platform/battery/power_supply/battery\0ACTION=change\0DEVPATH=/devices/platform/battery/power_supply/battery\0SUBSYSTEM=power_supply\0POWER_SUPPLY_CAPACITY=81\0";
        assert!(is_power_supply_uevent(event));
        assert!(!is_power_supply_uevent(
            b"change@/devices/virtual/net/wlan0\0ACTION=change\0SUBSYSTEM=net\0"
        ));
        // A field only names the subsystem when it is the whole field.
        assert!(!is_power_supply_uevent(
            b"change@/devices/x\0SUBSYSTEM=power_supply_x\0X=SUBSYSTEM=power_supply\0"
        ));
        assert!(!is_power_supply_uevent(
            b"libudev\0\xfe\xed\xca\xfeSUBSYSTEM=power_supply\0"
        ));
        assert!(!is_power_supply_uevent(b"SUBSYSTEM=power_supply\0"));
        assert!(!is_power_supply_uevent(b""));
    }

    #[test]
    fn the_first_battery_with_a_capacity_is_the_primary() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        supply(root, "ac", &[("type", "Mains"), ("online", "1")]);
        supply(
            root,
            "a-gauge",
            &[("type", "Battery"), ("status", "Charging")],
        );
        supply(
            root,
            "b-battery",
            &[
                ("type", "Battery"),
                ("status", "Discharging"),
                ("capacity", "82"),
            ],
        );
        assert_eq!(
            read(root, PERCENT),
            Some(SurfaceBattery {
                percent: Some(82.0),
                charging: false
            })
        );
    }

    #[test]
    fn without_a_capacity_the_first_battery_gives_only_charging() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        supply(root, "b", &[("type", "Battery"), ("status", "Discharging")]);
        supply(root, "a", &[("type", "Battery"), ("status", "Charging")]);
        supply(root, "c", &[("type", "Battery"), ("capacity", "101")]);
        assert_eq!(
            read(root, PERCENT),
            Some(SurfaceBattery {
                percent: None,
                charging: true
            })
        );
    }

    #[test]
    fn the_percent_goes_out_only_when_the_device_verified_it() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        supply(
            root,
            "battery",
            &[
                ("type", "Battery"),
                ("status", "Charging"),
                ("capacity", "12"),
            ],
        );
        assert_eq!(
            read(root, Verified::default()),
            Some(SurfaceBattery {
                percent: None,
                charging: true
            })
        );
        assert_eq!(
            read(root, PERCENT),
            Some(SurfaceBattery {
                percent: Some(12.0),
                charging: true
            })
        );
        // "Full" and "Not charging" are not charging.
        for status in ["Full", "Not charging", "Unknown"] {
            supply(root, "battery", &[("status", status)]);
            assert_eq!(read(root, PERCENT).map(|b| b.charging), Some(false));
        }
    }

    #[test]
    fn no_battery_or_a_read_error_leaves_the_battery_absent() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        assert_eq!(read(&root.join("missing"), PERCENT), None);
        assert_eq!(read(root, PERCENT), None);
        supply(root, "usb", &[("type", "USB"), ("online", "1")]);
        assert_eq!(read(root, PERCENT), None);
        supply(root, "battery", &[("type", "Battery"), ("capacity", "50")]);
        assert!(read(root, PERCENT).is_some());
        // A capacity that cannot be read (here, a directory) is a read error.
        fs::remove_file(root.join("battery/capacity")).unwrap();
        fs::create_dir(root.join("battery/capacity")).unwrap();
        assert_eq!(read(root, PERCENT), None);
    }

    #[test]
    fn a_reading_publishes_only_when_it_changed() {
        let (sender, mut receiver) = tokio::sync::watch::channel(None);
        let battery = |percent: Option<f64>, charging| Some(SurfaceBattery { percent, charging });
        assert!(!publish(&sender, None));
        assert!(!receiver.has_changed().unwrap());
        assert!(publish(&sender, battery(Some(82.0), false)));
        assert!(receiver.has_changed().unwrap());
        receiver.borrow_and_update();
        assert!(!publish(&sender, battery(Some(82.0), false)));
        assert!(!receiver.has_changed().unwrap());
        assert!(publish(&sender, battery(Some(81.0), false)));
        assert!(publish(&sender, battery(Some(81.0), true)));
        assert!(publish(&sender, None));
        assert_eq!(*receiver.borrow_and_update(), None);
    }

    #[test]
    fn the_verified_list_is_checked_at_the_boundary() {
        assert_eq!(Verified::parse(None), Ok(Verified::default()));
        assert_eq!(Verified::parse(Some("[]")), Ok(Verified::default()));
        assert_eq!(Verified::parse(Some(r#"["percent"]"#)), Ok(PERCENT));
        assert!(Verified::parse(Some(r#"["percent","voltage"]"#)).is_err());
        assert!(Verified::parse(Some("percent")).is_err());
        assert!(Verified::parse(Some(r#"{"percent":true}"#)).is_err());
    }
}
