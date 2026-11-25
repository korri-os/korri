//! Narrow, ungrabbed access to physical volume-key nodes, independent of InputPlumber.
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    future::poll_fn,
    io,
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    task::Poll,
};

use crate::devices::InputEventStream;
use evdev::{raw_stream::RawDevice, EventType};

// Linux input-event-codes.h: KEY_1..KEY_0, Q..P, A..L, Z..M, SPACE, ENTER.
// Explicitly omit punctuation and media keys: Consumer Control nodes are allowed.
pub const TYPING_KEYS: &[u16] = &[
    2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 30, 31, 32, 33, 34, 35,
    36, 37, 38, 44, 45, 46, 47, 48, 49, 50, 57, 28,
];

pub fn eligible_keys(keys: &[u16]) -> bool {
    keys.iter().any(|key| matches!(key, 114 | 115))
        && !keys.iter().any(|key| TYPING_KEYS.contains(key))
}

pub fn is_uinput(sys_root: &Path, path: &Path) -> bool {
    path.starts_with(sys_root.join("devices/virtual/input"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemKeyDevice {
    pub id: String,
    pub path: PathBuf,
    sysfs: PathBuf,
    rdev: u64,
}

/// Discovery reads only public sysfs facts. Open validates the descriptor again.
pub fn discover(sys_root: &Path, device_root: &Path) -> io::Result<Vec<SystemKeyDevice>> {
    let mut devices = Vec::new();
    for entry in fs::read_dir(sys_root.join("class/input"))? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(number) = name.to_str().and_then(|s| s.strip_prefix("event")) else {
            continue;
        };
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let inspect = || -> Option<SystemKeyDevice> {
            let sysfs = fs::canonicalize(entry.path()).ok()?;
            if !sysfs.starts_with(sys_root.join("devices")) || is_uinput(sys_root, &sysfs) {
                return None;
            }
            let capabilities = entry.path().join("device/capabilities");
            let events = crate::source_topology::parse_bitmap(
                &fs::read_to_string(capabilities.join("ev")).ok()?,
            )?;
            if !crate::source_topology::bitmap_has(&events, 1) {
                return None;
            }
            let bitmap = crate::source_topology::parse_bitmap(
                &fs::read_to_string(capabilities.join("key")).ok()?,
            )?;
            let keys = (0..=0x2ff)
                .filter(|key| crate::source_topology::bitmap_has(&bitmap, usize::from(*key)))
                .collect::<Vec<u16>>();
            if !eligible_keys(&keys) {
                return None;
            }
            let dev = fs::read_to_string(entry.path().join("dev")).ok()?;
            let (major, minor) = dev.trim().split_once(':')?;
            let rdev = libc::makedev(major.parse().ok()?, minor.parse().ok()?);
            Some(SystemKeyDevice {
                id: format!(
                    "system-keys:{}",
                    sysfs.strip_prefix(sys_root).ok()?.display()
                ),
                path: device_root.join(name.clone()),
                sysfs,
                rdev,
            })
        };
        if let Some(device) = inspect() {
            devices.push(device);
        }
    }
    devices.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(devices)
}

fn open(expected: &SystemKeyDevice, sys_root: &Path) -> io::Result<InputEventStream> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(&expected.path)?;
    let metadata = file.metadata()?;
    let sysfs = fs::canonicalize(sys_root.join(format!(
        "dev/char/{}:{}",
        libc::major(metadata.rdev()),
        libc::minor(metadata.rdev())
    )))?;
    if !metadata.file_type().is_char_device()
        || metadata.rdev() != expected.rdev
        || sysfs != expected.sysfs
        || is_uinput(sys_root, &sysfs)
    {
        return Err(io::Error::other("system key descriptor identity changed"));
    }
    let device = RawDevice::from_fd(file.into())?;
    let keys = device
        .supported_keys()
        .map(|keys| keys.iter().map(|k| k.0).collect::<Vec<_>>())
        .unwrap_or_default();
    if !device.supported_events().contains(EventType::KEY) || !eligible_keys(&keys) {
        return Err(io::Error::other(
            "system key descriptor capabilities changed",
        ));
    }
    // EV_SYN's mask is the event-type mask, not the SYN code mask (evdev ABI).
    set_mask(device.as_raw_fd(), 0, &[0b11])?;
    let mut keys = [0u8; 15];
    keys[114 / 8] |= 1 << (114 % 8);
    keys[115 / 8] |= 1 << (115 % 8);
    set_mask(device.as_raw_fd(), 1, &keys)?;
    Ok(Box::pin(device.into_event_stream()?))
}

fn set_mask(fd: i32, event_type: u32, codes: &[u8]) -> io::Result<()> {
    #[repr(C)]
    struct InputMask {
        event_type: u32,
        codes_size: u32,
        codes_ptr: u64,
    }
    let mask = InputMask {
        event_type,
        codes_size: codes.len() as u32,
        codes_ptr: codes.as_ptr() as u64,
    };
    // _IOW('E', 0x93, struct input_mask), fixed-size Linux UAPI on both targets.
    if unsafe { libc::ioctl(fd, 0x40104593, &mask) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

struct Opened {
    descriptor: SystemKeyDevice,
    events: InputEventStream,
}
#[derive(Default)]
pub struct SystemKeys {
    opened: BTreeMap<String, Opened>,
    cursor: usize,
}

pub enum SystemKeyEvent {
    Key { id: String, code: u16, value: i32 },
    Lost { id: String },
}

impl SystemKeys {
    /// Returned source ids must be cleared from shortcut state before more input.
    pub fn reconcile(&mut self, sys_root: &Path, device_root: &Path) -> Vec<String> {
        let devices = match discover(sys_root, device_root) {
            Ok(devices) => devices,
            Err(error) => {
                tracing::warn!(event = "inputd_system_keys_discovery_failed", error_kind = ?error.kind());
                Vec::new()
            }
        };
        let retired = self
            .opened
            .iter()
            .filter(|(_, opened)| !devices.contains(&opened.descriptor))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in &retired {
            self.opened.remove(id);
        }
        for descriptor in devices {
            if self.opened.contains_key(&descriptor.id) {
                continue;
            }
            match open(&descriptor, sys_root) {
                Ok(events) => {
                    self.opened
                        .insert(descriptor.id.clone(), Opened { descriptor, events });
                }
                Err(error) => {
                    tracing::debug!(event = "inputd_system_keys_open_failed", error_kind = ?error.kind())
                }
            }
        }
        retired
    }

    /// Remains pending with no devices, so it can run independently of controllers.
    pub async fn next_event(&mut self) -> SystemKeyEvent {
        loop {
            let (id, event) = poll_fn(|cx| {
                let ids = self.opened.keys().cloned().collect::<Vec<_>>();
                for offset in 0..ids.len() {
                    let index = (self.cursor + offset) % ids.len();
                    if let Poll::Ready(event) = self
                        .opened
                        .get_mut(&ids[index])
                        .unwrap()
                        .events
                        .as_mut()
                        .poll_next(cx)
                    {
                        self.cursor = (index + 1) % ids.len();
                        return Poll::Ready((ids[index].clone(), event));
                    }
                }
                Poll::Pending
            })
            .await;
            match event {
                Some(Ok(event)) if event.event_type().0 == 0 && event.code() != 3 => {}
                Some(Ok(event)) if event.event_type().0 == 1 => {
                    return SystemKeyEvent::Key {
                        id,
                        code: event.code(),
                        value: event.value(),
                    }
                }
                _ => {
                    self.opened.remove(&id);
                    return SystemKeyEvent::Lost { id };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[tokio::test]
    async fn dropped_stream_retires_the_source_without_replaying_queued_events() {
        let mut keys = SystemKeys::default();
        let id = "system-keys:gpio".to_owned();
        keys.opened.insert(
            id.clone(),
            Opened {
                descriptor: SystemKeyDevice {
                    id: id.clone(),
                    path: "/dev/input/event0".into(),
                    sysfs: "/sys/devices/gpio/event0".into(),
                    rdev: 0,
                },
                events: Box::pin(futures_util::stream::iter([
                    Ok(evdev::InputEvent::new(1, 114, 1)),
                    Ok(evdev::InputEvent::new(0, 3, 0)),
                    Ok(evdev::InputEvent::new(1, 115, 1)),
                ])),
            },
        );
        assert!(matches!(
            keys.next_event().await,
            SystemKeyEvent::Key {
                code: 114,
                value: 1,
                ..
            }
        ));
        assert!(matches!(keys.next_event().await, SystemKeyEvent::Lost { id: lost } if lost == id));
        assert!(keys.opened.is_empty());
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), keys.next_event())
                .await
                .is_err()
        );
    }

    #[test]
    fn every_typing_key_excludes_a_volume_node() {
        let expected = (2..=11)
            .chain(16..=25)
            .chain(30..=38)
            .chain(44..=50)
            .chain([57, 28])
            .collect::<Vec<_>>();
        assert_eq!(TYPING_KEYS, expected);
        for key in expected {
            assert!(!eligible_keys(&[115, key]));
        }
        assert!(eligible_keys(&[113, 114, 115, 164]));
        assert!(!eligible_keys(&[164]));
    }

    #[test]
    fn discovery_allows_gpio_resin_and_uhid_but_not_typing_or_uinput() {
        let root = tempfile::tempdir().unwrap();
        let sys = root.path();
        fs::create_dir_all(sys.join("class/input")).unwrap();
        for (index, device, keys, ev) in [
            (0, "platform/gpio-keys", vec![115], "3"),
            (1, "platform/resin", vec![114], "3"),
            (2, "virtual/misc/uhid/pad", vec![114, 115, 164], "3"),
            (3, "platform/keyboard", vec![115, 16], "3"),
            (4, "virtual/input/pad", vec![115], "3"),
            (5, "platform/no-ev-key", vec![115], "1"),
        ] {
            let event = sys.join(format!("devices/{device}/input/input{index}/event{index}"));
            fs::create_dir_all(event.join("device/capabilities")).unwrap();
            let mut bitmap = [0u64; 3];
            for key in keys {
                bitmap[key / 64] |= 1 << (key % 64);
            }
            fs::write(
                event.join("device/capabilities/key"),
                bitmap
                    .iter()
                    .rev()
                    .map(|word| format!("{word:x}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            )
            .unwrap();
            fs::write(event.join("device/capabilities/ev"), ev).unwrap();
            fs::write(event.join("dev"), format!("13:{}", 64 + index)).unwrap();
            symlink(&event, sys.join(format!("class/input/event{index}"))).unwrap();
        }
        let found = discover(sys, Path::new("/dev/input")).unwrap();
        assert_eq!(found.len(), 3);
        assert!(found
            .iter()
            .any(|d| d.path == Path::new("/dev/input/event2")));
    }
}
