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
#[derive(Debug, Default)]
struct Compositor(Mutex<Vec<String>>);
impl CompositorControl for Compositor {
    fn command(&self, command: &str) -> Result<(), String> {
        self.0.lock().unwrap().push(command.into());
        Ok(())
    }
    fn tree(&self) -> Result<String, String> {
        Ok(r#"{"id":1,"nodes":[{"id":2,"pid":9100,"focused":true}]}"#.into())
    }
    fn focus(&self, _: i64) -> Result<(), String> {
        Ok(())
    }
}
#[tokio::test]
async fn stream_play_local_buttons_reach_seats_only_while_the_viewer_is_focused() {
    const CHILD: &str = "KORRI_VIEWER_DISPLAY_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "host::session_state::viewer_input::tests::stream_play_local_buttons_reach_seats_only_while_the_viewer_is_focused", "--nocapture"])
            .env(CHILD, "1")
            .env("KORRID_DISPLAYS", r#"[{"name":"DP-1","main":true,"unwatched":false,"mode":"1920x1080@60Hz","x":0,"y":0,"width":1920,"height":1080,"touch":null}]"#)
            .output().unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
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
    let compositor = Arc::new(Compositor::default());
    let control = HostSessionControl::with_input_seats(root.path(), units.clone(), manager.clone())
        .with_boot_id(super::super::super::hooks::TEST_BOOT_ID)
        .with_compositor(compositor.clone());
    control.initialize_input(2).unwrap();
    let selected = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let selection = selected.clone();
    control
        .peer_selection
        .0
        .set(Arc::new(move || {
            selection.load(std::sync::atomic::Ordering::SeqCst)
        }))
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
    viewer.start(launch, "holder", None).await.unwrap();
    assert!(compositor
        .0
        .lock()
        .unwrap()
        .contains(&"output DP-1 power on".into()));
    compositor.0.lock().unwrap().clear();
    control.idle_displays().unwrap();
    assert_eq!(*compositor.0.lock().unwrap(), ["seat * idle_notify"]);
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
    selected.store(false, std::sync::atomic::Ordering::SeqCst);
    compositor.0.lock().unwrap().clear();
    control.idle_displays().unwrap();
    assert_eq!(*compositor.0.lock().unwrap(), ["output DP-1 power off"]);
    drop(viewer);
    drop(control);
    drop(manager);
    drop(coordinator);
    thread.join().unwrap();
}

#[test]
fn local_input_wrapper_reports_the_inner_viewer_display_without_input_authority() {
    let inner = Arc::new(TestLinuxStreamViewer::default());
    let viewer = LocalInputViewer {
        viewer: inner.clone(),
        control: Err(crate::RpcFailure {
            code: "Unavailable".into(),
            message: "no input".into(),
        }),
        coordinator: None,
        operation: tokio::sync::Mutex::new(()),
    };
    assert_eq!(viewer.display(), None);
    let display = crate::launcher::plugin_launch::PluginDisplay {
        width: 2176,
        height: 1812,
    };
    inner.set_display(display.clone());
    assert_eq!(viewer.display(), Some(display));
    assert!(inner.calls().is_empty());
}
