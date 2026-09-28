use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use korri_inputd::{
    dbus::{DbusRuntimeError, INPUTPLUMBER_BUS_NAME, INPUTPLUMBER_ROOT_PATH},
    devices::{parse_proc_bus_input_devices, validate_opened_descriptor, DeviceDescriptor},
    physical_sources::{discover_physical_sources, PhysicalSources},
    source_topology::SysfsSourceClassifier,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::Notify,
};

const TARGET0: &str = "/org/shadowblip/InputPlumber/devices/target/gamepad0";
const TARGET1: &str = "/org/shadowblip/InputPlumber/devices/target/gamepad1";

#[derive(Clone)]
struct Composite {
    sources: Arc<Mutex<Vec<String>>>,
    targets: Arc<Mutex<Vec<String>>>,
}

impl Composite {
    fn new(sources: &[&str], targets: &[&str]) -> Self {
        Self {
            sources: Arc::new(Mutex::new(strings(sources))),
            targets: Arc::new(Mutex::new(strings(targets))),
        }
    }
}

#[zbus::interface(name = "org.shadowblip.Input.CompositeDevice")]
impl Composite {
    #[zbus(property)]
    fn source_device_paths(&self) -> Vec<String> {
        self.sources.lock().unwrap().clone()
    }

    #[zbus(property)]
    fn target_devices(&self) -> Vec<String> {
        self.targets.lock().unwrap().clone()
    }
}

#[derive(Default)]
struct ReadGate {
    entered: Notify,
    resume: Notify,
}

#[derive(Clone)]
struct Target {
    device_type: Arc<Mutex<String>>,
    fail_paths: bool,
    paths: Arc<Mutex<Vec<String>>>,
    reads: Arc<AtomicUsize>,
    gate: Option<Arc<ReadGate>>,
}

impl Target {
    fn new(paths: &[&str]) -> Self {
        Self {
            device_type: Arc::new(Mutex::new("xb360".into())),
            fail_paths: false,
            paths: Arc::new(Mutex::new(strings(paths))),
            reads: Arc::new(AtomicUsize::new(0)),
            gate: None,
        }
    }

    fn auxiliary(device_type: &str) -> Self {
        Self {
            device_type: Arc::new(Mutex::new(device_type.into())),
            fail_paths: true,
            ..Self::new(&[])
        }
    }
}

#[zbus::interface(name = "org.shadowblip.Input.Target")]
impl Target {
    #[zbus(property)]
    fn device_type(&self) -> String {
        self.device_type.lock().unwrap().clone()
    }

    #[zbus(property)]
    async fn device_paths(&self) -> zbus::fdo::Result<Vec<String>> {
        // Snapshot before the gate so changes are visible on the second read.
        let paths = self.paths.lock().unwrap().clone();
        if self.reads.fetch_add(1, Ordering::SeqCst) == 0 {
            if let Some(gate) = &self.gate {
                gate.entered.notify_one();
                gate.resume.notified().await;
            }
        }
        if self.fail_paths {
            return Err(zbus::fdo::Error::Failed(
                "target does not implement DevicePaths".into(),
            ));
        }
        Ok(paths)
    }
}

struct TargetWithoutPaths;

#[zbus::interface(name = "org.shadowblip.Input.Target")]
impl TargetWithoutPaths {
    #[zbus(property)]
    fn device_type(&self) -> &str {
        "xb360"
    }

    #[zbus(property)]
    fn name(&self) -> &str {
        "Microsoft X-Box 360 pad"
    }
}

struct WrongPathType;

#[zbus::interface(name = "org.shadowblip.Input.Target")]
impl WrongPathType {
    #[zbus(property)]
    fn device_type(&self) -> &str {
        "xb360"
    }

    #[zbus(property)]
    fn device_paths(&self) -> &str {
        "/dev/input/event10"
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

fn write_capabilities(root: &Path, event: &str, key: &str, abs: &str) {
    let path = root
        .join("class/input")
        .join(event)
        .join("device/capabilities");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("key"), format!("{key}\n")).unwrap();
    fs::write(path.join("abs"), format!("{abs}\n")).unwrap();
}

fn descriptors() -> Vec<DeviceDescriptor> {
    let mut devices = parse_proc_bus_input_devices(
        include_str!("fixtures/proc-bus-input/one-virtual-one-raw.txt"),
        Path::new("/dev/input"),
    );
    let mut second = devices[0].clone();
    second.path = PathBuf::from("/dev/input/event11");
    second.sysfs_path = Some("/devices/virtual/input/input22".into());
    devices.push(second);
    devices
}

struct PrivateBus {
    _root: tempfile::TempDir,
    _daemon: tokio::process::Child,
    address: String,
    client: zbus::Connection,
    classifier: SysfsSourceClassifier,
}

impl PrivateBus {
    async fn start() -> Self {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("bus.conf");
        // No machine/session policy, activation, or host bus. GetAll, Set and
        // all direct provider methods are refused by the actual private bus.
        fs::write(
            &config,
            format!(
                r#"<busconfig>
          <type>session</type>
          <listen>unix:path={}/bus</listen>
          <auth>EXTERNAL</auth>
          <policy context="default">
            <allow user="*"/><allow own="*"/>
            <allow send_destination="*"/><allow receive_sender="*"/>
            <deny send_interface="org.freedesktop.DBus.Properties" send_member="GetAll"/>
            <deny send_interface="org.freedesktop.DBus.Properties" send_member="Set"/>
            <deny send_interface="org.shadowblip.Input.CompositeDevice"/>
            <deny send_interface="org.shadowblip.Input.Target"/>
          </policy>
        </busconfig>"#,
                root.path().display()
            ),
        )
        .unwrap();
        let mut daemon = tokio::process::Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--nopidfile", "--print-address=1"])
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("dbus-daemon from the pinned inputd shell");
        let mut output = BufReader::new(daemon.stdout.take().unwrap()).lines();
        let address = tokio::time::timeout(Duration::from_secs(5), output.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let client = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        // Observed Mini V2 source bitmaps, not guessed device-name classes.
        for event in ["event4", "event7"] {
            write_capabilities(
                root.path(),
                event,
                "f00000000 0 0 0 7cdb000000400000 0 0 0 0",
                "30003f",
            );
        }
        write_capabilities(root.path(), "event3", "8000000000000 0", "0");
        write_capabilities(root.path(), "event2", "4000000000000 0", "0");
        let classifier = SysfsSourceClassifier::new(root.path());
        Self {
            _root: root,
            _daemon: daemon,
            address,
            client,
            classifier,
        }
    }

    async fn serve(
        &self,
        composites: &[Composite],
        targets: &[(&str, Target)],
    ) -> zbus::Connection {
        let server = zbus::connection::Builder::address(self.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        for (index, composite) in composites.iter().enumerate() {
            server
                .object_server()
                .at(
                    format!("{INPUTPLUMBER_ROOT_PATH}/CompositeDevice{index}"),
                    composite.clone(),
                )
                .await
                .unwrap();
        }
        for (path, target) in targets {
            server
                .object_server()
                .at(*path, target.clone())
                .await
                .unwrap();
        }
        server.request_name(INPUTPLUMBER_BUS_NAME).await.unwrap();
        server
    }

    async fn discover(
        &self,
        devices: &[DeviceDescriptor],
    ) -> Result<PhysicalSources, DbusRuntimeError> {
        discover_physical_sources(&self.client, devices, &self.classifier).await
    }
}

fn one_composite() -> Composite {
    Composite::new(&["/dev/input/event4"], &[TARGET0])
}

#[tokio::test]
async fn identical_controllers_resolve_independently_without_dbus0_or_get_all() {
    let bus = PrivateBus::start().await;
    let first = Target::new(&["/dev/input/event10"]);
    let second = Target::new(&["/dev/input/event11"]);
    let server = bus
        .serve(
            &[
                one_composite(),
                Composite::new(&["/dev/input/event7"], &[TARGET1]),
            ],
            &[(TARGET0, first.clone()), (TARGET1, second.clone())],
        )
        .await;
    let devices = descriptors();
    assert_eq!(devices[0].stable_identity(), devices[4].stable_identity());
    let discovered = bus.discover(&devices).await.unwrap();
    assert_eq!(discovered.owner, server.unique_name().unwrap().as_str());
    assert_eq!(discovered.controllers.len(), 2);
    for (index, expected) in [(&devices[0], TARGET0), (&devices[4], TARGET1)]
        .iter()
        .enumerate()
    {
        let source = &discovered.controllers[index];
        assert_eq!(&source.descriptor, expected.0);
        assert_eq!(source.target_path, expected.1);
        assert_eq!(
            source.composite_path,
            format!("{INPUTPLUMBER_ROOT_PATH}/CompositeDevice{index}")
        );
    }
    assert_eq!(first.reads.load(Ordering::SeqCst), 2);
    assert_eq!(second.reads.load(Ordering::SeqCst), 2);
    // Prove that the private bus really denies broad property access.
    let denied = bus
        .client
        .call_method(
            Some(server.unique_name().unwrap().as_str()),
            TARGET0,
            Some("org.freedesktop.DBus.Properties"),
            "GetAll",
            &("org.shadowblip.Input.Target",),
        )
        .await;
    assert!(denied.unwrap_err().to_string().contains("AccessDenied"));
    assert_eq!(first.reads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn shipped_xb360_mouse_keyboard_profile_never_reads_auxiliary_device_paths() {
    let bus = PrivateBus::start().await;
    let sources = [
        "/dev/input/event3",
        "/dev/input/event4",
        "/dev/input/event2",
    ];
    let keyboard = "/org/shadowblip/InputPlumber/devices/target/keyboard0";
    let mouse = "/org/shadowblip/InputPlumber/devices/target/mouse0";
    let dbus = "/org/shadowblip/InputPlumber/devices/target/dbus5";
    // The built 60-xbox_one_gamepad.yaml declares xb360, mouse and keyboard.
    // Its patched provider errors on mouse/keyboard DevicePaths. The existing
    // DBus target is auxiliary too; its DevicePaths must not be needed either.
    let keyboard_target = Target::auxiliary("keyboard");
    let mouse_target = Target::auxiliary("mouse");
    let dbus_target = Target::auxiliary("dbus");
    let _server = bus
        .serve(
            &[Composite::new(&sources, &[keyboard, TARGET0, mouse, dbus])],
            &[
                (keyboard, keyboard_target.clone()),
                (mouse, mouse_target.clone()),
                (TARGET0, Target::new(&["/dev/input/event10"])),
                (dbus, dbus_target.clone()),
            ],
        )
        .await;
    let discovered = bus.discover(&descriptors()).await.unwrap();
    assert_eq!(discovered.controllers.len(), 1);
    let mut expected_sources = strings(&sources);
    expected_sources.sort();
    assert_eq!(
        discovered.controllers[0].source_device_paths,
        expected_sources
    );
    assert_eq!(discovered.controllers[0].target_path, TARGET0);
    for target in [keyboard_target, mouse_target, dbus_target] {
        assert_eq!(target.reads.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn missing_device_paths_fails_closed() {
    let bus = PrivateBus::start().await;
    let server = bus.serve(&[one_composite()], &[]).await;
    server
        .object_server()
        .at(TARGET0, TargetWithoutPaths)
        .await
        .unwrap();
    let error = bus.discover(&descriptors()).await.unwrap_err();
    assert!(matches!(error, DbusRuntimeError::Zbus(_)));
    assert!(error.to_string().contains("DevicePaths"), "{error}");
}

#[tokio::test]
async fn wrong_property_type_and_stale_target_fail_closed() {
    let bus = PrivateBus::start().await;
    let server = bus.serve(&[one_composite()], &[]).await;
    assert!(bus.discover(&descriptors()).await.is_err());
    server
        .object_server()
        .at(TARGET0, WrongPathType)
        .await
        .unwrap();
    assert!(bus.discover(&descriptors()).await.is_err());
}

#[tokio::test]
async fn malformed_and_raw_reported_nodes_never_fall_back_to_other_normalized_devices() {
    let bus = PrivateBus::start().await;
    let target = Target::new(&[]);
    let _server = bus
        .serve(&[one_composite()], &[(TARGET0, target.clone())])
        .await;
    let mut devices = descriptors();
    // Even the complete XB360 fingerprint is insufficient on a physical sysfs path.
    let mut raw_lookalike = devices[0].clone();
    raw_lookalike.path = "/dev/input/event15".into();
    raw_lookalike.sysfs_path = Some("/devices/pci0000:00/usb1/input/input15".into());
    devices.push(raw_lookalike);
    for path in [
        "",
        "/dev/input/event",
        "/dev/input/event-1",
        "/dev/input/event10/",
        "/dev/input/event10/../event11",
        "/dev/input/../input/event10",
        "/dev/input/event10x",
        "/dev/input//event10",
        "event10",
        "/dev/hidraw0",
        "/dev/inputplumber/sources/event10",
        "/dev/input/event4",
        "/dev/input/event12",
        "/dev/input/event15",
        "/dev/input/event99",
    ] {
        *target.paths.lock().unwrap() = strings(&[path]);
        assert!(bus.discover(&devices).await.is_err(), "accepted {path:?}");
    }
}

#[tokio::test]
async fn malformed_target_object_paths_are_rejected() {
    let bus = PrivateBus::start().await;
    let composite = one_composite();
    let _server = bus
        .serve(
            std::slice::from_ref(&composite),
            &[(TARGET0, Target::new(&["/dev/input/event10"]))],
        )
        .await;
    for path in [
        "",
        "/",
        "/other/gamepad0",
        "/org/shadowblip/InputPlumber/devices/target/",
        "/org/shadowblip/InputPlumber/devices/target/gamepad0/child",
        "not a path",
    ] {
        *composite.targets.lock().unwrap() = strings(&[path]);
        assert!(
            bus.discover(&descriptors()).await.is_err(),
            "accepted {path:?}"
        );
    }
}

#[tokio::test]
async fn duplicate_nodes_and_multiple_normalized_nodes_are_ambiguous() {
    let bus = PrivateBus::start().await;
    let target = Target::new(&[]);
    let _server = bus
        .serve(&[one_composite()], &[(TARGET0, target.clone())])
        .await;
    for paths in [
        vec![],
        vec!["/dev/input/event10", "/dev/input/event10"],
        vec!["/dev/input/event10", "/dev/input/event11"],
    ] {
        *target.paths.lock().unwrap() = strings(&paths);
        assert!(bus.discover(&descriptors()).await.is_err());
    }
    *target.paths.lock().unwrap() = strings(&["/dev/input/event10"]);
    let mut devices = descriptors();
    devices.push(devices[0].clone());
    assert!(bus.discover(&devices).await.is_err());
}

#[tokio::test]
async fn cross_composite_nodes_targets_and_sources_cannot_be_reused() {
    let bus = PrivateBus::start().await;
    let second = Composite::new(&["/dev/input/event7"], &[TARGET1]);
    let second_target = Target::new(&["/dev/input/event10"]);
    let _server = bus
        .serve(
            &[one_composite(), second.clone()],
            &[
                (TARGET0, Target::new(&["/dev/input/event10"])),
                (TARGET1, second_target.clone()),
            ],
        )
        .await;
    assert!(bus.discover(&descriptors()).await.is_err());
    *second_target.paths.lock().unwrap() = strings(&["/dev/input/event11"]);
    *second.targets.lock().unwrap() = strings(&[TARGET0]);
    assert!(bus.discover(&descriptors()).await.is_err());
    *second.targets.lock().unwrap() = strings(&[TARGET1]);
    *second.sources.lock().unwrap() = strings(&["/dev/input/event4"]);
    assert!(bus.discover(&descriptors()).await.is_err());
    *second.sources.lock().unwrap() = strings(&["/dev/input/event7"]);
    assert_eq!(
        bus.discover(&descriptors())
            .await
            .unwrap()
            .controllers
            .len(),
        2
    );
}

#[tokio::test]
async fn missing_ambiguous_and_disconnected_gamepad_topologies_are_rejected() {
    let bus = PrivateBus::start().await;
    let composite = one_composite();
    let _server = bus
        .serve(
            std::slice::from_ref(&composite),
            &[(TARGET0, Target::new(&["/dev/input/event10"]))],
        )
        .await;
    for sources in [
        vec!["/dev/input/event4", "/dev/input/event7"],
        vec!["/dev/input/event4", "/dev/input/event4"],
        vec!["/dev/input/event4", "/dev/input/event99"],
        vec!["/dev/input/event"],
        vec!["/dev/input/event4/../event7"],
    ] {
        *composite.sources.lock().unwrap() = strings(&sources);
        assert!(bus.discover(&descriptors()).await.is_err());
    }
    *composite.sources.lock().unwrap() = strings(&[
        "/dev/input/event3",
        "/dev/input/event4",
        "/dev/input/event2",
    ]);
    assert!(bus.discover(&descriptors()).await.is_ok());
    fs::remove_dir_all(bus._root.path().join("class/input/event4")).unwrap();
    assert!(bus.discover(&descriptors()).await.is_err());
}

fn start_discovery(
    bus: &PrivateBus,
) -> tokio::task::JoinHandle<Result<PhysicalSources, DbusRuntimeError>> {
    let client = bus.client.clone();
    let classifier = bus.classifier.clone();
    tokio::spawn(
        async move { discover_physical_sources(&client, &descriptors(), &classifier).await },
    )
}

async fn entered(gate: &ReadGate) {
    tokio::time::timeout(Duration::from_secs(5), gate.entered.notified())
        .await
        .unwrap();
}

#[tokio::test]
async fn owner_replacement_is_rejected_without_following_the_well_known_name() {
    let bus = PrivateBus::start().await;
    let gate = Arc::new(ReadGate::default());
    let mut target = Target::new(&["/dev/input/event10"]);
    target.gate = Some(gate.clone());
    let old = bus
        .serve(&[one_composite()], &[(TARGET0, target.clone())])
        .await;
    let discovery = start_discovery(&bus);
    entered(&gate).await;
    old.release_name(INPUTPLUMBER_BUS_NAME).await.unwrap();
    let replacement = Target::new(&["/dev/input/event11"]);
    let new = bus
        .serve(&[one_composite()], &[(TARGET0, replacement.clone())])
        .await;
    gate.resume.notify_one();
    let error = discovery.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("owner changed"), "{error}");
    assert_eq!(target.reads.load(Ordering::SeqCst), 2);
    assert_eq!(replacement.reads.load(Ordering::SeqCst), 0);
    let retry = bus.discover(&descriptors()).await.unwrap();
    assert_eq!(retry.owner, new.unique_name().unwrap().as_str());
    assert_eq!(
        retry.controllers[0].descriptor.path,
        Path::new("/dev/input/event11")
    );
}

#[tokio::test]
async fn changing_target_paths_under_the_same_owner_are_rejected() {
    let bus = PrivateBus::start().await;
    let gate = Arc::new(ReadGate::default());
    let mut target = Target::new(&["/dev/input/event10"]);
    target.gate = Some(gate.clone());
    let _server = bus
        .serve(&[one_composite()], &[(TARGET0, target.clone())])
        .await;
    let discovery = start_discovery(&bus);
    entered(&gate).await;
    *target.paths.lock().unwrap() = strings(&["/dev/input/event11"]);
    gate.resume.notify_one();
    assert!(discovery
        .await
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("target changed"));
}

#[tokio::test]
async fn changing_composite_mapping_under_the_same_owner_is_rejected() {
    let bus = PrivateBus::start().await;
    let gate = Arc::new(ReadGate::default());
    let mut target = Target::new(&["/dev/input/event10"]);
    target.gate = Some(gate.clone());
    let composite = one_composite();
    let _server = bus
        .serve(std::slice::from_ref(&composite), &[(TARGET0, target)])
        .await;
    let discovery = start_discovery(&bus);
    entered(&gate).await;
    *composite.sources.lock().unwrap() = strings(&["/dev/input/event7"]);
    gate.resume.notify_one();
    assert!(discovery
        .await
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("composite changed"));
}

#[tokio::test]
async fn a_stalled_provider_has_a_bounded_deadline() {
    let bus = PrivateBus::start().await;
    let gate = Arc::new(ReadGate::default());
    let mut target = Target::new(&["/dev/input/event10"]);
    target.gate = Some(gate.clone());
    let _server = bus.serve(&[one_composite()], &[(TARGET0, target)]).await;
    let discovery = start_discovery(&bus);
    entered(&gate).await;
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), discovery)
            .await
            .unwrap()
            .unwrap(),
        Err(DbusRuntimeError::TimedOut)
    ));
    gate.resume.notify_one();
}

#[tokio::test]
async fn absent_owner_and_oversized_metadata_fail_closed() {
    let bus = PrivateBus::start().await;
    assert!(bus.discover(&descriptors()).await.is_err());
    let target = Target::new(&[]);
    let _server = bus
        .serve(&[one_composite()], &[(TARGET0, target.clone())])
        .await;
    *target.paths.lock().unwrap() = (0..257)
        .map(|index| format!("/dev/input/event{index}"))
        .collect();
    assert!(bus
        .discover(&descriptors())
        .await
        .unwrap_err()
        .to_string()
        .contains("bounds"));
}

#[tokio::test]
async fn discovery_does_not_replace_opened_descriptor_revalidation() {
    let bus = PrivateBus::start().await;
    let _server = bus
        .serve(
            &[one_composite()],
            &[(TARGET0, Target::new(&["/dev/input/event10"]))],
        )
        .await;
    let discovered = bus.discover(&descriptors()).await.unwrap();
    let expected = &discovered.controllers[0].descriptor;
    let mut replaced = expected.clone();
    replaced.device_number = Some(12345);
    assert!(validate_opened_descriptor(expected, &replaced).is_err());
    replaced.device_number = expected.device_number;
    replaced.sysfs_path = Some("/devices/pci0000:00/input/input20".into());
    assert!(validate_opened_descriptor(expected, &replaced).is_err());
}

#[tokio::test]
async fn persisted_empty_or_volume_only_composite_does_not_hide_a_connected_controller() {
    let bus = PrivateBus::start().await;
    let disconnected = one_composite();
    let _server = bus
        .serve(
            &[
                disconnected.clone(),
                Composite::new(&["/dev/input/event7"], &[TARGET1]),
            ],
            &[(TARGET1, Target::new(&["/dev/input/event11"]))],
        )
        .await;
    // TARGET0 is deliberately absent: no target metadata is required for a
    // persisted composite with no gamepad. Do not stop that composite either.
    for sources in [vec![], vec!["/dev/input/event3", "/dev/input/event2"]] {
        *disconnected.sources.lock().unwrap() = strings(&sources);
        let discovered = bus.discover(&descriptors()).await.unwrap();
        assert_eq!(discovered.controllers.len(), 1);
        assert_eq!(discovered.controllers[0].target_path, TARGET1);
        assert_eq!(
            discovered.controllers[0].descriptor.path,
            Path::new("/dev/input/event11")
        );
    }
}

#[tokio::test]
async fn skipped_composite_sources_are_still_rechecked() {
    for sources in [vec![], vec!["/dev/input/event3", "/dev/input/event2"]] {
        let bus = PrivateBus::start().await;
        let disconnected = Composite::new(&sources, &[TARGET0]);
        let gate = Arc::new(ReadGate::default());
        let mut target = Target::new(&["/dev/input/event11"]);
        target.gate = Some(gate.clone());
        let _server = bus
            .serve(
                &[
                    disconnected.clone(),
                    Composite::new(&["/dev/input/event7"], &[TARGET1]),
                ],
                &[(TARGET1, target)],
            )
            .await;
        let discovery = start_discovery(&bus);
        entered(&gate).await;
        *disconnected.sources.lock().unwrap() = strings(&["/dev/input/event4"]);
        gate.resume.notify_one();
        assert!(discovery
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("composite changed"));
    }
}

#[tokio::test]
async fn device_type_not_object_name_selects_xb360_and_gamepad_outputs() {
    let bus = PrivateBus::start().await;
    // A valid target object need not encode its type in its path.
    let path = "/org/shadowblip/InputPlumber/devices/target/opaque0";
    let target = Target::new(&["/dev/input/event10"]);
    let _server = bus
        .serve(
            &[Composite::new(&["/dev/input/event4"], &[path])],
            &[(path, target.clone())],
        )
        .await;
    for device_type in ["xb360", "gamepad"] {
        *target.device_type.lock().unwrap() = device_type.into();
        assert_eq!(
            bus.discover(&descriptors()).await.unwrap().controllers[0].target_path,
            path
        );
    }
    for device_type in ["mouse", "keyboard", "xbox-series"] {
        *target.device_type.lock().unwrap() = device_type.into();
        let reads = target.reads.load(Ordering::SeqCst);
        assert!(bus.discover(&descriptors()).await.is_err());
        assert_eq!(target.reads.load(Ordering::SeqCst), reads);
    }
}

#[tokio::test]
async fn target_type_changes_fail_closed_on_selected_and_auxiliary_targets() {
    for change_selected in [true, false] {
        let bus = PrivateBus::start().await;
        let aux_path = "/org/shadowblip/InputPlumber/devices/target/a0";
        let auxiliary = Target::auxiliary("keyboard");
        let gate = Arc::new(ReadGate::default());
        let mut target = Target::new(&["/dev/input/event10"]);
        target.gate = Some(gate.clone());
        let _server = bus
            .serve(
                &[Composite::new(&["/dev/input/event4"], &[TARGET0, aux_path])],
                &[(TARGET0, target.clone()), (aux_path, auxiliary.clone())],
            )
            .await;
        let discovery = start_discovery(&bus);
        entered(&gate).await;
        if change_selected {
            *target.device_type.lock().unwrap() = "keyboard".into();
        } else {
            *auxiliary.device_type.lock().unwrap() = "xb360".into();
        }
        gate.resume.notify_one();
        assert!(discovery
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("target type changed"));
        assert_eq!(auxiliary.reads.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn reordered_source_target_and_node_lists_do_not_change_the_mapping() {
    let bus = PrivateBus::start().await;
    let keyboard = "/org/shadowblip/InputPlumber/devices/target/keyboard0";
    let composite = Composite::new(
        &[
            "/dev/input/event4",
            "/dev/input/event3",
            "/dev/input/event2",
        ],
        &[keyboard, TARGET0],
    );
    let gate = Arc::new(ReadGate::default());
    // DevicePaths is a list contract. Only event10 in this list matches the
    // supplied normalized descriptor; other reported nodes remain unselected.
    let mut target = Target::new(&["/dev/input/event20", "/dev/input/event10"]);
    target.gate = Some(gate.clone());
    let auxiliary = Target::auxiliary("keyboard");
    let _server = bus
        .serve(
            std::slice::from_ref(&composite),
            &[(TARGET0, target.clone()), (keyboard, auxiliary.clone())],
        )
        .await;
    let discovery = start_discovery(&bus);
    entered(&gate).await;
    composite.sources.lock().unwrap().reverse();
    composite.targets.lock().unwrap().reverse();
    target.paths.lock().unwrap().reverse();
    gate.resume.notify_one();
    let discovered = discovery.await.unwrap().unwrap();
    assert_eq!(discovered.controllers.len(), 1);
    assert_eq!(
        discovered.controllers[0].descriptor.path,
        Path::new("/dev/input/event10")
    );
    assert_eq!(auxiliary.reads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn duplicate_paths_in_recheck_are_not_treated_as_harmless_reordering() {
    for duplicate in ["sources", "targets", "nodes"] {
        let bus = PrivateBus::start().await;
        let composite = one_composite();
        let gate = Arc::new(ReadGate::default());
        let mut target = Target::new(&["/dev/input/event10"]);
        target.gate = Some(gate.clone());
        let _server = bus
            .serve(
                std::slice::from_ref(&composite),
                &[(TARGET0, target.clone())],
            )
            .await;
        let discovery = start_discovery(&bus);
        entered(&gate).await;
        match duplicate {
            "sources" => composite
                .sources
                .lock()
                .unwrap()
                .push("/dev/input/event4".into()),
            "targets" => composite.targets.lock().unwrap().push(TARGET0.into()),
            "nodes" => target
                .paths
                .lock()
                .unwrap()
                .push("/dev/input/event10".into()),
            _ => unreachable!(),
        }
        gate.resume.notify_one();
        assert!(discovery
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("duplicate"));
    }
}
