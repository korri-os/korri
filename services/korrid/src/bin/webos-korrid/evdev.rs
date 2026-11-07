//! webOS physical input edge. Raw evdev samples enter the existing native
//! source; button mapping, neutral rearm and repeat stay in the shell mapper.
use std::{
    collections::{BTreeMap, HashSet},
    fs::OpenOptions,
    io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use evdev::{
    raw_stream::{EventStream, RawDevice},
    InputEvent, KeyCode,
};
use futures::{
    stream::{self, SelectAll},
    Stream, StreamExt,
};
use korrid::{native_input::*, surface_host_input::SurfaceHostInputSource};
use tokio::task::JoinHandle;

// Linux 4.4 ARM EABI input records use 32-bit timeval, not time64.
// Check the target ABI in the actual cross-build rather than the host layout.
#[cfg(all(target_arch = "arm", target_os = "linux"))]
const _: () = assert!(std::mem::size_of::<libc::input_event>() == 16);

const INPUT_ROOT: &str = "/dev/input";
const SCAN: Duration = Duration::from_secs(1);

type PadStream = Pin<Box<dyn Stream<Item = (PathBuf, io::Result<()>)> + Send>>;

pub fn start(source: Arc<SurfaceHostInputSource>) -> JoinHandle<()> {
    tokio::spawn(run(source, PathBuf::from(INPUT_ROOT)))
}

async fn run(source: Arc<SurfaceHostInputSource>, root: PathBuf) {
    let mut pads = SelectAll::<PadStream>::new();
    let mut open = HashSet::new();
    let mut warned = HashSet::new();
    let mut scan = tokio::time::interval(SCAN);
    scan.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = scan.tick() => {
                let entries = match std::fs::read_dir(&root) {
                    Ok(entries) => { warned.remove(&root); entries },
                    Err(error) => {
                        if warned.insert(root.clone()) { eprintln!("webos-korrid: input directory: {error}"); }
                        continue;
                    }
                };
                let paths: HashSet<_> = entries.filter_map(Result::ok).map(|entry| entry.path())
                    .filter(|path| event_node(path)).collect();
                warned.retain(|path| paths.contains(path));
                for path in paths {
                    if open.contains(&path) || open.len() >= korri_input_contract::MAX_PHYSICAL_SOURCES { continue; }
                    match Pad::open(&path, Arc::clone(&source)) {
                        Ok(Some(pad)) => {
                            warned.remove(&path);
                            open.insert(path.clone());
                            pads.push(Box::pin(stream::unfold(Some(pad), move |pad| {
                                let path = path.clone();
                                async move {
                                    let mut pad = pad?;
                                    let result = pad.next().await;
                                    let next = result.is_ok().then_some(pad);
                                    Some(((path, result), next))
                                }
                            })));
                        }
                        Ok(None) => {},
                        Err(error) => {
                            if warned.insert(path.clone()) { eprintln!("webos-korrid: input {}: {error}", path.display()); }
                        }
                    }
                }
            }
            Some((path, result)) = pads.next(), if !pads.is_empty() => {
                if let Err(error) = result {
                    open.remove(&path);
                    eprintln!("webos-korrid: input {} disconnected: {error}", path.display());
                }
            }
        }
    }
}

fn event_node(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("event"))
        .is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        })
}

// Same positive capability test as inputd/src/devices.rs. No name, USB ID or
// fixed event number heuristic; no InputPlumber/uinput dependency on the TV.
fn gamepad(keys: Option<&evdev::AttributeSetRef<KeyCode>>) -> bool {
    keys.is_some_and(|keys| {
        keys.contains(KeyCode::BTN_SOUTH) || keys.contains(KeyCode::BTN_TRIGGER)
    })
}

struct Pad {
    id: String,
    events: EventStream,
    source: Arc<SurfaceHostInputSource>,
    sample: Sample,
}

impl Drop for Pad {
    fn drop(&mut self) {
        let _ = self
            .source
            .publish(NativeInputEvent::DeviceRemoved(NativeInputDeviceRemoved {
                kind: NativeInputDeviceRemovedKind::DeviceRemoved,
                device_id: self.id.clone(),
            }));
    }
}

impl Pad {
    fn open(path: &Path, source: Arc<SurfaceHostInputSource>) -> io::Result<Option<Self>> {
        // Read-only, never EVIOCGRAB. Linux inputd grabs its validated normalized
        // InputPlumber capture target, not raw physical devices shared with webOS.
        let device = RawDevice::from_fd(OpenOptions::new().read(true).open(path)?.into())?;
        if !gamepad(device.supported_keys()) {
            return Ok(None);
        }
        // inputd/physical_sources.rs uses uniq, then phys; event numbers are
        // discovery paths, never reconnect identities. Preserve that rule.
        let id = device
            .unique_name()
            .filter(|id| !id.is_empty())
            .or_else(|| device.physical_path().filter(|id| !id.is_empty()))
            .filter(|id| id.len() <= 256 && !id.chars().any(char::is_control))
            .ok_or_else(|| io::Error::other("gamepad has no valid uniq/phys identity"))?
            .to_owned();
        let events = device.into_event_stream()?;
        Self::baseline(events.device(), &id, &source)?;
        Ok(Some(Self {
            id,
            events,
            source,
            sample: Sample::default(),
        }))
    }

    fn baseline(device: &RawDevice, id: &str, source: &SurfaceHostInputSource) -> io::Result<()> {
        let name = device
            .name()
            .filter(|name| {
                !name.is_empty() && name.len() <= 512 && !name.chars().any(char::is_control)
            })
            .ok_or_else(|| io::Error::other("invalid gamepad name"))?;
        let mut values = Vec::new();
        for key in device.get_key_state()?.iter() {
            values.push(input(id, 1, key.0, 1));
        }
        let mut axes = Vec::new();
        for (axis, info) in device.get_absinfo()? {
            axes.push(NativeInputAxisInfo {
                code: axis.0.into(),
                minimum: info.minimum().into(),
                maximum: info.maximum().into(),
                flat: Some(info.flat().into()),
            });
            values.push(input(id, 3, axis.0, info.value()));
        }
        source
            .publish_device(
                NativeInputDeviceInfo {
                    device_id: id.to_owned(),
                    class: NativeInputDeviceClass::Gamepad,
                    name: name.to_owned(),
                    capabilities: [
                        (device.supported_keys().is_some(), "EV_KEY"),
                        (device.supported_absolute_axes().is_some(), "EV_ABS"),
                    ]
                    .into_iter()
                    .filter(|(supported, _)| *supported)
                    .map(|(_, name)| name.to_owned())
                    .collect(),
                    axes: Some(axes),
                },
                values,
            )
            .map_err(io::Error::other)
    }

    async fn next(&mut self) -> io::Result<()> {
        let event = self.events.next_event().await?;
        match self.sample.push(event) {
            SampleResult::Pending => {}
            SampleResult::Frame(values) => {
                let values = values
                    .into_iter()
                    .map(|((kind, code), value)| input(&self.id, kind, code, value))
                    .collect();
                self.source
                    .publish_frame(&self.id, values)
                    .map_err(io::Error::other)?;
            }
            SampleResult::Dropped => {
                // Retire immediately: never repeat a held direction while the
                // stream waits for its recovery barrier (which may not arrive).
                self.source
                    .publish(NativeInputEvent::DeviceRemoved(NativeInputDeviceRemoved {
                        kind: NativeInputDeviceRemovedKind::DeviceRemoved,
                        device_id: self.id.clone(),
                    }))
                    .map_err(io::Error::other)?;
            }
            SampleResult::Lost => {
                // A measured baseline rearms; it is not new heard evidence.
                Self::baseline(self.events.device(), &self.id, &self.source)?;
            }
        }
        Ok(())
    }
}

fn input(id: &str, kind: u16, code: u16, value: i32) -> NativeInputInput {
    NativeInputInput {
        kind: NativeInputInputKind::Input,
        device_id: id.into(),
        class: NativeInputDeviceClass::Gamepad,
        input_type: kind.into(),
        code: code.into(),
        value: value.into(),
        timestamp: 0.0,
    }
}

#[derive(Default)]
struct Sample {
    values: BTreeMap<(u16, u16), i32>,
    dropped: bool,
}
#[derive(Debug, PartialEq)]
enum SampleResult {
    Pending,
    Frame(BTreeMap<(u16, u16), i32>),
    Dropped,
    Lost,
}
impl Sample {
    fn push(&mut self, event: InputEvent) -> SampleResult {
        match (event.event_type().0, event.code(), event.value()) {
            (0, 3, _) => {
                self.values.clear();
                if !std::mem::replace(&mut self.dropped, true) {
                    return SampleResult::Dropped;
                }
            }
            (0, 0, _) if self.dropped => {
                self.dropped = false;
                return SampleResult::Lost;
            }
            (0, 0, _) if !self.values.is_empty() => {
                return SampleResult::Frame(std::mem::take(&mut self.values))
            }
            (1, code @ 0..=0x2ff, value @ 0..=1) if !self.dropped => {
                self.values.insert((1, code), value);
            }
            (3, code @ 0..=0x3f, value) if !self.dropped => {
                self.values.insert((3, code), value);
            }
            _ => {}
        }
        SampleResult::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent::new(kind, code, value)
    }

    #[test]
    fn complete_samples_coalesce_fields_and_ignore_repeat_and_non_input() {
        let mut sample = Sample::default();
        for e in [
            event(1, 0x130, 1),
            event(3, 0, -1234),
            event(3, 0, 1234),
            event(1, 0x130, 2),
            event(4, 4, 1),
        ] {
            assert_eq!(sample.push(e), SampleResult::Pending);
        }
        assert_eq!(
            sample.push(event(0, 0, 0)),
            SampleResult::Frame(BTreeMap::from([((1, 0x130), 1), ((3, 0), 1234)]))
        );
        assert_eq!(sample.push(event(0, 0, 0)), SampleResult::Pending);
    }

    #[test]
    fn dropped_samples_require_a_fresh_measured_baseline_not_replayed_edges() {
        let mut sample = Sample::default();
        assert_eq!(sample.push(event(1, 0x130, 1)), SampleResult::Pending);
        assert_eq!(sample.push(event(0, 3, 0)), SampleResult::Dropped);
        // Repeated drop notices and the invalid tail never publish input.
        for e in [event(0, 3, 0), event(1, 0x130, 0), event(3, 16, 1)] {
            assert_eq!(sample.push(e), SampleResult::Pending);
        }
        assert_eq!(sample.push(event(0, 0, 0)), SampleResult::Lost);
        assert_eq!(sample.push(event(0, 0, 0)), SampleResult::Pending);
        sample.push(event(1, 0x131, 1));
        assert_eq!(
            sample.push(event(0, 0, 0)),
            SampleResult::Frame(BTreeMap::from([((1, 0x131), 1)]))
        );
    }

    #[test]
    fn only_positive_gamepad_or_joystick_buttons_select_a_device() {
        for key in [KeyCode::BTN_SOUTH, KeyCode::BTN_TRIGGER] {
            let keys: evdev::AttributeSet<KeyCode> = [key].into_iter().collect();
            assert!(gamepad(Some(&keys)));
        }
        let keyboard: evdev::AttributeSet<KeyCode> = [KeyCode::KEY_A].into_iter().collect();
        assert!(!gamepad(Some(&keyboard)));
        assert!(!gamepad(None));
    }

    #[test]
    fn out_of_contract_fields_do_not_enter_a_native_sample() {
        let mut sample = Sample::default();
        for e in [
            event(1, 0x300, 1),
            event(3, 0x40, 1),
            event(1, 0x130, -1),
            event(1, 0x130, 3),
        ] {
            assert_eq!(sample.push(e), SampleResult::Pending);
        }
        assert_eq!(sample.push(event(0, 0, 0)), SampleResult::Pending);
    }

    #[test]
    fn discovers_event_nodes_only() {
        for name in ["event0", "event2", "event100"] {
            assert!(event_node(Path::new(name)));
        }
        for name in ["event", "js0", "event-1", "event2foo"] {
            assert!(!event_node(Path::new(name)));
        }
    }
}

#[cfg(test)]
mod uinput_tests {
    use super::*;
    use evdev::{uinput::VirtualDevice, AbsInfo, AbsoluteAxisCode, AttributeSet, UinputAbsSetup};
    use korrid::{
        surface::SurfaceSlot,
        surface_host_access::{SurfaceHostAccess, SurfaceHostPermission},
        surface_host_input::{self, SurfaceHostInputLimits},
    };

    #[tokio::test]
    #[ignore = "requires workstation access to /dev/uinput and the created event node"]
    async fn virtual_pad_baseline_live_sample_and_disconnect() {
        let keys: AttributeSet<KeyCode> = [KeyCode::BTN_SOUTH].into_iter().collect();
        let name = format!("Korri webOS gamepad test {}", std::process::id());
        // Gamepad buttons and stick axes only. VirtualDevice's Drop destroys
        // the kernel device on success and on assertion/panic unwinding.
        let mut device = VirtualDevice::builder()
            .unwrap()
            .name(&name)
            .with_absolute_axis(&UinputAbsSetup::new(
                AbsoluteAxisCode::ABS_X,
                AbsInfo::new(0, -32768, 32767, 0, 4096, 0),
            ))
            .unwrap()
            .with_absolute_axis(&UinputAbsSetup::new(
                AbsoluteAxisCode::ABS_Y,
                AbsInfo::new(0, -32768, 32767, 0, 4096, 0),
            ))
            .unwrap()
            .with_keys(&keys)
            .unwrap()
            .build()
            .unwrap();
        let path = device
            .enumerate_dev_nodes_blocking()
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let surfaces: SurfaceSlot = Arc::default();
        let (_router, source) = surface_host_input::router_with_surfaces(
            SurfaceHostAccess::new(
                "test-capability",
                "http://127.0.0.1:8099",
                SurfaceHostPermission::LocalSessions,
            ),
            Arc::clone(&surfaces),
            SurfaceHostInputLimits::default(),
        );
        // uinput has no physical uniq/phys identity. Exercise the real kernel
        // stream and measured baseline with a fixture ID, not a production
        // identity fallback. Discovery continues to reject unidentified pads.
        let raw =
            RawDevice::from_fd(OpenOptions::new().read(true).open(&path).unwrap().into()).unwrap();
        assert!(gamepad(raw.supported_keys()));
        let events = raw.into_event_stream().unwrap();
        let source = Arc::new(source);
        Pad::baseline(events.device(), &name, &source).unwrap();
        let mut pad = Pad {
            id: name,
            events,
            source,
            sample: Sample::default(),
        };
        assert_eq!(surfaces.controllers.snapshot().len(), 1);
        assert!(!surfaces.controllers.snapshot()[0].heard);
        device
            .emit(&[InputEvent::new(
                evdev::EventType::KEY.0,
                KeyCode::BTN_SOUTH.0,
                1,
            )])
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !surfaces.controllers.snapshot()[0].heard {
                pad.next().await.unwrap();
            }
        })
        .await
        .unwrap();
        drop(device);
        tokio::time::timeout(Duration::from_secs(3), async {
            while pad.next().await.is_ok() {}
        })
        .await
        .unwrap();
        drop(pad);
        assert!(surfaces.controllers.snapshot().is_empty());
    }
}
