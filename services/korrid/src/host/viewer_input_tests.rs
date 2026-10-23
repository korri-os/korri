use super::*;
use crate::host::{
    input_coordination::test_support,
    input_seat::UnixInputSeatManager,
    systemd_unit::{InMemoryLaunchUnitBackend, LaunchSandbox},
};
use crate::linux_viewer::test_backend::TestLinuxStreamViewer;
use korri_input_contract::{GamepadState, SeatRequest};
use korri_inputd::input_seat::{SeatBackend, SeatRuntime, SeatSpec};
use std::path::PathBuf;

#[derive(Clone, Default)]
struct VirtualSeats(Arc<Mutex<Vec<(u8, GamepadState)>>>);
impl SeatBackend for VirtualSeats {
    fn create(&mut self, _: &SeatSpec) -> Result<(), String> {
        Ok(())
    }
    fn event_device(&self, slot: u8) -> Result<PathBuf, String> {
        Ok(format!("/dev/input/event{slot}").into())
    }
    fn write_state(&mut self, slot: u8, state: GamepadState) -> Result<(), String> {
        self.0.lock().unwrap().push((slot, state));
        Ok(())
    }
    fn destroy(&mut self, _: u8) -> Result<(), String> {
        Ok(())
    }
}
#[derive(Debug)]
struct Compositor;
impl CompositorControl for Compositor {
    fn tree(&self) -> Result<String, String> {
        Ok(r#"{"id":1,"nodes":[{"id":2,"pid":9100,"focused":true}]}"#.into())
    }
    fn focus(&self, _: i64) -> Result<(), String> {
        Ok(())
    }
}
#[tokio::test]
async fn stream_play_local_buttons_reach_seats_only_while_the_viewer_is_focused() {
    let seats = VirtualSeats::default();
    let mut receiver = SeatRuntime::boot(seats.clone()).unwrap();
    let (coordinator, thread) =
        test_support::coordinator(move |request| receiver.coordinate(request, 0));
    let manager = Arc::new(UnixInputSeatManager::new(
        "unused".into(),
        Ok(coordinator.clone()),
    ));
    let root = tempfile::tempdir().unwrap();
    let units = Arc::new(InMemoryLaunchUnitBackend::default());
    let control = HostSessionControl::with_input_seats(root.path(), units.clone(), manager.clone())
        .with_boot_id(super::super::super::hooks::TEST_BOOT_ID)
        .with_compositor(Arc::new(Compositor), vec![]);
    control.initialize_input(2).unwrap();
    control
        .peer_selection
        .0
        .set(Arc::new(|| true))
        .ok()
        .unwrap();
    let launch = "0123456789abcdef0123456789abcdef";
    units
        .launch(
            launch,
            &["viewer".into()],
            &BTreeMap::new(),
            &LaunchSandbox::default(),
        )
        .unwrap();
    units.set_window_pids(launch, [9100].into());
    coordinator
        .request(SeatRequest::PhysicalConnected {
            device_id: "pad".into(),
            name: "Pad".into(),
            state: GamepadState::neutral(),
        })
        .unwrap();
    let viewer = LocalInputViewer {
        viewer: Arc::new(TestLinuxStreamViewer::default()),
        control: Ok(control.clone()),
        coordinator: Some(coordinator.clone()),
        operation: tokio::sync::Mutex::new(()),
    };
    let button = GamepadState {
        buttons: 0x1000,
        ..GamepadState::neutral()
    };
    let send = |state| {
        coordinator
            .request(SeatRequest::PhysicalState {
                device_id: "pad".into(),
                state,
            })
            .unwrap();
    };
    viewer.start(launch, "far").await.unwrap();
    control.status(); // The local-game watcher must not take the viewer's input.
    send(GamepadState::neutral());
    send(button);
    assert_eq!(seats.0.lock().unwrap().last(), Some(&(1, button)));
    viewer.background(launch).await.unwrap();
    assert_eq!(
        seats.0.lock().unwrap().last(),
        Some(&(1, GamepadState::neutral()))
    );
    let writes = seats.0.lock().unwrap().len();
    send(GamepadState::neutral());
    send(button);
    assert_eq!(seats.0.lock().unwrap().len(), writes);
    assert_eq!(
        coordinator.snapshot().unwrap().session.as_deref(),
        Some(launch)
    );
    viewer.resume_and_focus(launch).await.unwrap();
    send(GamepadState::neutral());
    send(button);
    assert_eq!(seats.0.lock().unwrap().last(), Some(&(1, button)));
    viewer.stop(launch).await.unwrap();
    let writes = seats.0.lock().unwrap().len();
    send(GamepadState::neutral());
    send(button);
    assert_eq!(seats.0.lock().unwrap().len(), writes);
    assert!(coordinator.snapshot().unwrap().session.is_none());
    drop(viewer);
    drop(control);
    drop(manager);
    drop(coordinator);
    thread.join().unwrap();
}
