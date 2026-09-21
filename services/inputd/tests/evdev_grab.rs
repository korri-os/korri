use std::{io, time::Duration};

use evdev::{
    uinput::VirtualDevice, AttributeSet, BusType, Device, EventType, InputEvent, InputId, KeyCode,
};
use korri_inputd::{
    action_catalog::{ActionId, DispatchMode},
    dbus::{Signal, DBUS_INPUT_MEMBER, DBUS_TARGET_INTERFACE, DBUS_TARGET_PATH},
    devices::{
        DeviceCapabilities, DeviceClass, DeviceDescriptor, OpenedTarget, TargetProvider,
        XB360_TARGET_NAME,
    },
    runtime::{Runtime, RuntimeState},
};

struct OpenOnceProvider {
    descriptor: DeviceDescriptor,
    opened: Option<OpenedTarget>,
}

impl TargetProvider for OpenOnceProvider {
    fn enumerate(&mut self) -> io::Result<Vec<DeviceDescriptor>> {
        Ok(vec![self.descriptor.clone()])
    }

    fn open(&mut self, _expected: &DeviceDescriptor) -> io::Result<OpenedTarget> {
        self.opened
            .take()
            .ok_or_else(|| io::Error::other("fixture target already opened"))
    }
}

async fn open_event_after_udev(path: &std::path::Path) -> io::Result<Device> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        match Device::open(path) {
            Ok(device) => return Ok(device),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                ) && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires writable /dev/uinput and /dev/input"]
async fn authenticated_dbus_shortcut_survives_an_exclusive_evdev_grab() -> io::Result<()> {
    let mut keys = AttributeSet::<KeyCode>::new();
    keys.insert(KeyCode::BTN_MODE);
    keys.insert(KeyCode::BTN_TL);
    let mut virtual_device = VirtualDevice::builder()?
        .name(XB360_TARGET_NAME)
        .input_id(InputId::new(BusType(3), 0x045e, 0x028e, 0x0001))
        .with_keys(&keys)?
        .build()?;
    let event_path = virtual_device
        .enumerate_dev_nodes_blocking()?
        .next()
        .transpose()?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "virtual event node"))?;

    let event_device = open_event_after_udev(&event_path).await?;
    let input_id = event_device.input_id().into();
    let event_stream = event_device.into_event_stream()?;
    let descriptor = DeviceDescriptor {
        path: event_path.clone(),
        name: XB360_TARGET_NAME.to_owned(),
        physical_path: None,
        unique_id: None,
        sysfs_path: Some("/devices/virtual/input/test-target".to_owned()),
        input_id,
        capabilities: DeviceCapabilities::inputplumber_xb360(),
        class: DeviceClass::Gamepad,
        device_number: None,
    };
    let mut provider = OpenOnceProvider {
        descriptor: descriptor.clone(),
        opened: Some(OpenedTarget {
            descriptor,
            events: Box::pin(event_stream),
            router: Box::new(korri_inputd::virtual_targets::DisabledTargetRouter::default()),
        }),
    };
    let mut runtime = Runtime::default();
    runtime.set_dbus_owner(Some(":1.42"));
    runtime.reconcile(&mut provider);
    assert!(matches!(runtime.state(), RuntimeState::Ready { .. }));

    let mut grabber = open_event_after_udev(&event_path).await?;
    grabber.grab()?;
    assert!(runtime
        .handle_dbus_signal(&Signal {
            sender: ":1.42",
            path: DBUS_TARGET_PATH,
            interface: DBUS_TARGET_INTERFACE,
            member: DBUS_INPUT_MEMBER,
            capability: "ui_guide",
            value: 1.0,
        })
        .is_empty());
    virtual_device.emit(&[InputEvent::new(EventType::KEY.0, KeyCode::BTN_TL.0, 1)])?;
    assert!(
        tokio::time::timeout(Duration::from_millis(100), runtime.next_evdev_actions())
            .await
            .is_err()
    );

    let actions = runtime.handle_dbus_signal(&Signal {
        sender: ":1.42",
        path: DBUS_TARGET_PATH,
        interface: DBUS_TARGET_INTERFACE,
        member: DBUS_INPUT_MEMBER,
        capability: "ui_l1",
        value: 1.0,
    });
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].id, ActionId::WorkspacePrev);
    assert_eq!(actions[0].dispatch_mode, DispatchMode::Direct);
    Ok(())
}
