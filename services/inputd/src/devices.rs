use std::{
    io,
    os::fd::AsFd,
    path::{Path, PathBuf},
    pin::Pin,
};

use evdev::{raw_stream::RawDevice, InputEvent, InputId};
use futures_util::Stream;

use crate::virtual_targets::{TargetRouter, UinputTargetRouter};

pub const XB360_TARGET_NAME: &str = "Microsoft X-Box 360 pad";
pub const GAME_TARGET_NAME: &str = "Microsoft X-Box 360 pad (Korri game)";
pub const PORTAL_TARGET_NAME: &str = "Microsoft X-Box 360 pad (Korri portal)";
pub const GAME_TARGET_PHYS: &str = "korri/inputd/game";
pub const PORTAL_TARGET_PHYS: &str = "korri/inputd/portal";

// InputPlumber 0.75.2 src/input/target/xpad.rs builds `xb360` with these
// identity and capability fields. It does not set uinput phys or uniq fields.
const XB360_INPUT_ID: DeviceInputId = DeviceInputId {
    bus: 0x0003,
    vendor: 0x045e,
    product: 0x028e,
    version: 0x0001,
};
const XB360_KEYS: [u16; 15] = [
    0x130, 0x131, 0x133, 0x134, 0x136, 0x137, 0x13a, 0x13b, 0x13c, 0x13d, 0x13e, 0x2c0, 0x2c1,
    0x2c2, 0x2c3,
];
const XB360_ABSOLUTE_AXES: [u16; 8] = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x10, 0x11];

pub type InputEventStream = Pin<Box<dyn Stream<Item = io::Result<InputEvent>> + Send + 'static>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceClass {
    Gamepad,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceDescriptor {
    pub path: PathBuf,
    pub name: String,
    pub physical_path: Option<String>,
    pub unique_id: Option<String>,
    pub sysfs_path: Option<String>,
    pub input_id: DeviceInputId,
    pub capabilities: DeviceCapabilities,
    pub class: DeviceClass,
    pub device_number: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceCapabilities {
    pub keys: Vec<u16>,
    pub absolute_axes: Vec<u16>,
    pub force_feedback: bool,
}

impl DeviceCapabilities {
    pub fn inputplumber_xb360() -> Self {
        Self {
            keys: XB360_KEYS.to_vec(),
            absolute_axes: XB360_ABSOLUTE_AXES.to_vec(),
            force_feedback: true,
        }
    }

    fn is_inputplumber_xb360(&self) -> bool {
        self.keys == XB360_KEYS && self.absolute_axes == XB360_ABSOLUTE_AXES && self.force_feedback
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeviceInputId {
    pub bus: u16,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
}

impl From<InputId> for DeviceInputId {
    fn from(value: InputId) -> Self {
        Self {
            bus: value.bus_type().0,
            vendor: value.vendor(),
            product: value.product(),
            version: value.version(),
        }
    }
}

impl DeviceDescriptor {
    pub fn is_validated_inputplumber_xb360(&self) -> bool {
        self.class == DeviceClass::Gamepad
            && self.name == XB360_TARGET_NAME
            && self.input_id == XB360_INPUT_ID
            && self.physical_path.is_none()
            && self.unique_id.is_none()
            && self
                .sysfs_path
                .as_deref()
                .is_some_and(|path| path.starts_with("/devices/virtual/input/"))
            && self.capabilities.is_inputplumber_xb360()
    }

    pub fn stable_identity(&self) -> TargetIdentity {
        TargetIdentity {
            name: self.name.clone(),
            physical_path: self.physical_path.clone(),
            unique_id: self.unique_id.clone(),
            input_id: self.input_id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetIdentity {
    pub name: String,
    pub physical_path: Option<String>,
    pub unique_id: Option<String>,
    pub input_id: DeviceInputId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TargetResolution {
    Found(DeviceDescriptor),
    Missing { raw_gamepads: usize },
    Ambiguous { targets: Vec<DeviceDescriptor> },
}

pub fn resolve_target(devices: &[DeviceDescriptor]) -> TargetResolution {
    let targets = devices
        .iter()
        .filter(|device| device.is_validated_inputplumber_xb360())
        .cloned()
        .collect::<Vec<_>>();
    match targets.as_slice() {
        [target] => TargetResolution::Found(target.clone()),
        [] => TargetResolution::Missing {
            raw_gamepads: devices
                .iter()
                .filter(|device| device.class == DeviceClass::Gamepad)
                .count(),
        },
        _ => TargetResolution::Ambiguous { targets },
    }
}

pub fn validate_opened_descriptor(
    expected: &DeviceDescriptor,
    opened: &DeviceDescriptor,
) -> Result<(), ProvenanceError> {
    if !opened.is_validated_inputplumber_xb360() {
        return Err(ProvenanceError::NotValidatedTarget);
    }
    if expected.stable_identity() != opened.stable_identity()
        || expected.device_number != opened.device_number
    {
        return Err(ProvenanceError::Replaced);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProvenanceError {
    NotValidatedTarget,
    Replaced,
}

pub struct OpenedTarget {
    pub descriptor: DeviceDescriptor,
    pub events: InputEventStream,
    pub router: Box<dyn TargetRouter>,
}

pub trait TargetProvider {
    fn enumerate(&mut self) -> io::Result<Vec<DeviceDescriptor>>;
    fn open(&mut self, expected: &DeviceDescriptor) -> io::Result<OpenedTarget>;
}

pub struct EvdevProvider {
    proc_devices: PathBuf,
    input_root: PathBuf,
}

impl Default for EvdevProvider {
    fn default() -> Self {
        Self {
            proc_devices: PathBuf::from("/proc/bus/input/devices"),
            input_root: PathBuf::from("/dev/input"),
        }
    }
}

impl EvdevProvider {
    pub fn new(proc_devices: impl Into<PathBuf>, input_root: impl Into<PathBuf>) -> Self {
        Self {
            proc_devices: proc_devices.into(),
            input_root: input_root.into(),
        }
    }
}

impl TargetProvider for EvdevProvider {
    fn enumerate(&mut self) -> io::Result<Vec<DeviceDescriptor>> {
        let content = std::fs::read_to_string(&self.proc_devices)?;
        Ok(parse_proc_bus_input_devices(&content, &self.input_root))
    }

    fn open(&mut self, expected: &DeviceDescriptor) -> io::Result<OpenedTarget> {
        // RawDevice exposes SYN_DROPPED. Runtime handles it by closing this
        // stream and reconciling instead of retaining potentially stale state.
        let mut device = RawDevice::open(&expected.path)?;
        let device_number = fstat_device_number(&device)?;
        let sysfs_path = sysfs_path_for_device_number(device_number, Path::new("/sys"));
        let descriptor =
            descriptor_from_opened_device(expected, &device, device_number, sysfs_path);
        let router = Box::new(UinputTargetRouter::from_source(&device)?);
        // Inputd is the only reader of the normalized source. Consumers read
        // one of the two routed targets instead.
        device.grab()?;
        let events = device.into_event_stream()?;
        Ok(OpenedTarget {
            descriptor,
            events: Box::pin(events),
            router,
        })
    }
}

fn descriptor_from_opened_device(
    expected: &DeviceDescriptor,
    device: &RawDevice,
    device_number: u64,
    sysfs_path: Option<String>,
) -> DeviceDescriptor {
    DeviceDescriptor {
        path: expected.path.clone(),
        name: device.name().unwrap_or_default().to_owned(),
        physical_path: device.physical_path().map(str::to_owned),
        unique_id: device.unique_name().map(str::to_owned),
        sysfs_path,
        input_id: device.input_id().into(),
        capabilities: capabilities_from_device(device),
        class: if device.supported_keys().is_some_and(|keys| {
            keys.contains(evdev::KeyCode::new(0x130)) || keys.contains(evdev::KeyCode::new(0x120))
        }) {
            DeviceClass::Gamepad
        } else {
            DeviceClass::Other
        },
        device_number: Some(device_number),
    }
}

fn capabilities_from_device(device: &RawDevice) -> DeviceCapabilities {
    DeviceCapabilities {
        keys: device
            .supported_keys()
            .map(|values| values.iter().map(|value| value.0).collect())
            .unwrap_or_default(),
        absolute_axes: device
            .supported_absolute_axes()
            .map(|values| values.iter().map(|value| value.0).collect())
            .unwrap_or_default(),
        force_feedback: device
            .supported_events()
            .contains(evdev::EventType::FORCEFEEDBACK),
    }
}

fn fstat_device_number(device: &RawDevice) -> io::Result<u64> {
    rustix::fs::fstat(device.as_fd())
        .map(|stat| stat.st_rdev)
        .map_err(io::Error::from)
}

fn sysfs_path_for_device_number(device_number: u64, sysfs_root: &Path) -> Option<String> {
    let major = rustix::fs::major(device_number);
    let minor = rustix::fs::minor(device_number);
    let device_link = sysfs_root
        .join("dev/char")
        .join(format!("{major}:{minor}"))
        .join("device");
    let canonical = std::fs::canonicalize(device_link).ok()?;
    let relative = canonical.strip_prefix(sysfs_root).ok()?;
    Some(format!("/{}", relative.to_string_lossy()))
}

pub fn parse_proc_bus_input_devices(content: &str, input_root: &Path) -> Vec<DeviceDescriptor> {
    content
        .split("\n\n")
        .filter_map(|block| parse_proc_block(block, input_root))
        .collect()
}

fn parse_proc_block(block: &str, input_root: &Path) -> Option<DeviceDescriptor> {
    let value = |prefix: &str| {
        block.lines().find_map(|line| {
            let line = line.trim();
            line.strip_prefix(prefix).map(|rest| {
                rest.trim()
                    .split_once('=')
                    .map_or(rest.trim(), |(_, value)| value.trim())
                    .trim_matches('"')
                    .to_owned()
            })
        })
    };
    let non_empty_value = |prefix: &str| value(prefix).filter(|value| !value.is_empty());
    let name = non_empty_value("N:")?;
    let handlers = non_empty_value("H:")?;
    let event_node = handlers
        .split_whitespace()
        .find(|part| part.starts_with("event") && part[5..].chars().all(|c| c.is_ascii_digit()))?;
    let input = block
        .lines()
        .find_map(|line| line.trim().strip_prefix("I:").map(str::trim))
        .unwrap_or_default();
    let input_id = DeviceInputId {
        bus: parse_hex_field(input, "Bus"),
        vendor: parse_hex_field(input, "Vendor"),
        product: parse_hex_field(input, "Product"),
        version: parse_hex_field(input, "Version"),
    };
    let bitmap = |name: &str| {
        block.lines().find_map(|line| {
            line.trim()
                .strip_prefix(&format!("B: {name}="))
                .map(|bits| bits.split_whitespace().collect::<Vec<_>>())
        })
    };
    let key_words = bitmap("KEY");
    let class = if key_words
        .as_ref()
        .is_some_and(|words| bitmap_has(words, 0x130) || bitmap_has(words, 0x120))
    {
        DeviceClass::Gamepad
    } else {
        DeviceClass::Other
    };
    let path = input_root.join(event_node);
    let device_number = std::fs::metadata(&path).ok().map(|metadata| {
        use std::os::unix::fs::MetadataExt;
        metadata.rdev()
    });
    Some(DeviceDescriptor {
        path,
        name,
        physical_path: non_empty_value("P:"),
        unique_id: non_empty_value("U:"),
        sysfs_path: non_empty_value("S:"),
        input_id,
        capabilities: DeviceCapabilities {
            keys: key_words.as_deref().map(bitmap_values).unwrap_or_default(),
            absolute_axes: bitmap("ABS")
                .as_deref()
                .map(bitmap_values)
                .unwrap_or_default(),
            force_feedback: bitmap("EV")
                .as_ref()
                .is_some_and(|words| bitmap_has(words, evdev::EventType::FORCEFEEDBACK.0.into())),
        },
        class,
        device_number,
    })
}

fn parse_hex_field(input: &str, name: &str) -> u16 {
    input
        .split_whitespace()
        .find_map(|part| part.strip_prefix(&format!("{name}=")))
        .and_then(|value| u16::from_str_radix(value, 16).ok())
        .unwrap_or_default()
}

fn bitmap_has(words: &[&str], bit: usize) -> bool {
    const BITS_PER_WORD: usize = 64;
    let offset = bit / BITS_PER_WORD;
    let Some(index) = words.len().checked_sub(offset + 1) else {
        return false;
    };
    u64::from_str_radix(words[index], 16)
        .map(|word| word & (1_u64 << (bit % BITS_PER_WORD)) != 0)
        .unwrap_or(false)
}

fn bitmap_values(words: &[&str]) -> Vec<u16> {
    (0..words.len() * 64)
        .filter(|bit| bitmap_has(words, *bit))
        .filter_map(|bit| u16::try_from(bit).ok())
        .collect()
}
