use super::*;
use std::{fs, os::unix::fs::PermissionsExt, sync::Mutex as StdMutex};
const CERT: &str = "-----BEGIN CERTIFICATE-----\nY2VydA==\n-----END CERTIFICATE-----\n";

struct Fixture {
    viewer: WebosViewer,
    root: tempfile::TempDir,
    presentations: Arc<StdMutex<Vec<bool>>>,
}
impl Fixture {
    fn new(behavior: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("korri-webos");
        let python = Command::new("sh")
            .args(["-c", "command -v python3"])
            .output()
            .unwrap();
        let python = String::from_utf8(python.stdout).unwrap();
        let script = format!(
            r##"#!{python}
import sys, socket, os, time
assert sys.argv[1] == 'viewer'
assert 'CREDENTIALS_DIRECTORY' not in os.environ
assert 'KORRID_RPC_CAPABILITY' not in os.environ
mode = sys.argv[2]
if mode == 'identity':
    sys.stdout.write({cert:?})
elif mode == 'discover':
    print('test-uuid\n47984')
elif mode == 'stream':
    assert sys.stdin.read() == {cert:?}
    assert all('CERTIFICATE' not in arg for arg in sys.argv)
    with open({pid:?}, 'w') as file: file.write(str(os.getpid()))
    channel = socket.socket(fileno=3)
    {behavior}
    channel.sendall(b'R')
    with open({frames:?}, 'wb') as file:
        while True:
            data = channel.recv(16384)
            if not data: break
            file.write(data); file.flush()
else: sys.exit(2)
"##,
            python = python.trim(),
            cert = CERT,
            pid = root.path().join("pid").display().to_string(),
            frames = root.path().join("frames").display().to_string()
        );
        fs::write(&executable, script).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let presentations = Arc::new(StdMutex::new(Vec::new()));
        let observed = presentations.clone();
        let viewer = WebosViewer {
            executable,
            identity: root.path().join("moonlight"),
            provision: Arc::new(|uuid, cert| {
                Box::pin(async move {
                    assert_eq!(uuid, "test-uuid");
                    assert_eq!(cert, CERT);
                    Ok(crate::MoonlightCertificateProvisioned {
                        server_certificate: CERT.into(),
                    })
                })
            }),
            presenting: Arc::new(move |value| observed.lock().unwrap().push(value)),
            input: Arc::new(|| None),
            session: Arc::new(Mutex::new(None)),
            readiness_timeout: Duration::from_secs(2),
        };
        Self {
            viewer,
            root,
            presentations,
        }
    }
    fn pid(&self) -> i32 {
        fs::read_to_string(self.root.path().join("pid"))
            .unwrap()
            .parse()
            .unwrap()
    }
    fn assert_reaped(&self, pid: i32) {
        // SAFETY: signal zero only observes existence; it changes no process.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
}

#[tokio::test]
async fn ready_is_required_and_pin_and_capability_stay_out_of_arguments() {
    let fixture = Fixture::new("pass");
    fixture
        .viewer
        .start("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "127.0.0.1")
        .await
        .unwrap();
    let pid = fixture.pid();
    assert_eq!(
        fixture
            .viewer
            .status("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .await
            .unwrap(),
        LinuxViewerStatus::Running
    );
    assert_eq!(*fixture.presentations.lock().unwrap(), [true]);
    fixture
        .viewer
        .stop("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .await
        .unwrap();
    fixture.assert_reaped(pid);
    assert_eq!(*fixture.presentations.lock().unwrap(), [true, false]);
}

#[tokio::test]
async fn readiness_eof_or_timeout_reaps_child_before_failed_start_returns() {
    for behavior in ["sys.exit(1)", "time.sleep(10)"] {
        let fixture = Fixture::new(behavior);
        assert!(fixture
            .viewer
            .start("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "127.0.0.1")
            .await
            .is_err());
        fixture.assert_reaped(fixture.pid());
        assert!(!fixture.presentations.lock().unwrap().contains(&true));
    }
}

#[tokio::test]
async fn leave_releases_decoder_without_becoming_stopped_and_resume_reconnects() {
    let fixture = Fixture::new("pass");
    fixture
        .viewer
        .start("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "127.0.0.1")
        .await
        .unwrap();
    let pid = fixture.pid();
    fixture
        .viewer
        .background("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .await
        .unwrap();
    fixture.assert_reaped(pid);
    assert_eq!(
        fixture
            .viewer
            .status("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .await
            .unwrap(),
        LinuxViewerStatus::Backgrounded
    );
    assert_eq!(
        fixture
            .viewer
            .status("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .await
            .unwrap(),
        LinuxViewerStatus::Backgrounded
    );
    fixture
        .viewer
        .resume_and_focus("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .await
        .unwrap();
    assert_eq!(
        fixture
            .viewer
            .status("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .await
            .unwrap(),
        LinuxViewerStatus::Running
    );
    fixture
        .viewer
        .stop("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .await
        .unwrap();
}

#[tokio::test]
async fn stale_stop_never_targets_replacement_and_stop_is_idempotent() {
    let fixture = Fixture::new("pass");
    fixture
        .viewer
        .start("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "127.0.0.1")
        .await
        .unwrap();
    fixture
        .viewer
        .stop("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .await
        .unwrap();
    fixture
        .viewer
        .start("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "127.0.0.1")
        .await
        .unwrap();
    fixture
        .viewer
        .stop("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .await
        .unwrap();
    assert_eq!(
        fixture
            .viewer
            .status("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
            .await
            .unwrap(),
        LinuxViewerStatus::Running
    );
    assert!(fixture
        .viewer
        .background("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .await
        .is_err());
    fixture
        .viewer
        .stop("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        .await
        .unwrap();
    fixture
        .viewer
        .stop("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        .await
        .unwrap();
}

#[tokio::test]
async fn unexpected_exit_uses_shared_stopped_recovery_not_game_end() {
    let fixture = Fixture::new("pass");
    fixture
        .viewer
        .start("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "127.0.0.1")
        .await
        .unwrap();
    {
        let mut guard = fixture.viewer.session.lock().await;
        let child = guard.as_mut().unwrap().child.as_mut().unwrap();
        child.child.kill().unwrap();
        child.child.wait().unwrap();
    }
    assert_eq!(
        fixture
            .viewer
            .status("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .await
            .unwrap(),
        LinuxViewerStatus::Stopped
    );
    assert_eq!(*fixture.presentations.lock().unwrap(), [true, false]);
    fixture
        .viewer
        .start("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "127.0.0.1")
        .await
        .unwrap();
    fixture
        .viewer
        .stop("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .await
        .unwrap();
}

#[tokio::test]
async fn invalid_route_or_failed_attestation_cannot_start_stream() {
    let mut fixture = Fixture::new("pass");
    for address in [
        "host.invalid",
        "127.0.0.1/path",
        "user@127.0.0.1",
        "127.0.0.1?x=y",
    ] {
        assert!(fixture
            .viewer
            .start("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", address)
            .await
            .is_err());
    }
    fixture.viewer.provision =
        Arc::new(|_, _| Box::pin(async { Err(error("attestation refused")) }));
    assert!(fixture
        .viewer
        .start("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "127.0.0.1")
        .await
        .is_err());
    assert!(!fixture.root.path().join("pid").exists());
}

#[tokio::test]
async fn child_gets_existing_native_frames_and_neutralization_before_close() {
    use crate::{
        native_input::*,
        surface_host_access::{SurfaceHostAccess, SurfaceHostPermission},
        surface_host_input,
    };
    let (_, source) = surface_host_input::router(
        SurfaceHostAccess::new(
            "test-capability",
            "http://127.0.0.1:1",
            SurfaceHostPermission::LocalSessions,
        ),
        surface_host_input::SurfaceHostInputLimits::default(),
    );
    let source = Arc::new(source);
    let value = NativeInputInput {
        kind: NativeInputInputKind::Input,
        device_id: "pad".into(),
        class: NativeInputDeviceClass::Gamepad,
        input_type: 1.0,
        code: 304.0,
        value: 1.0,
        timestamp: 0.0,
    };
    source
        .publish_device(
            NativeInputDeviceInfo {
                device_id: "pad".into(),
                class: NativeInputDeviceClass::Gamepad,
                name: "Pad".into(),
                capabilities: vec!["EV_KEY".into()],
                axes: None,
            },
            vec![value.clone()],
        )
        .unwrap();
    let mut f = Fixture::new("pass");
    let attachment = source.clone();
    f.viewer.input = Arc::new(move || Some(attachment.clone()));
    f.viewer
        .start("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "127.0.0.1")
        .await
        .unwrap();
    source
        .publish_frame(
            "pad",
            vec![NativeInputInput {
                value: 0.0,
                ..value
            }],
        )
        .unwrap();
    let path = f.root.path().join("frames");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let text = fs::read_to_string(&path).unwrap_or_default();
        if text.contains("\"value\":0.0") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native sample did not reach child"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    f.viewer
        .stop("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .await
        .unwrap();
    let text = fs::read_to_string(path).unwrap();
    let events: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events[0]["kind"], "device-added");
    assert_eq!(events[1]["value"], 1.0);
    assert_eq!(events[2]["kind"], "device-state-complete");
    assert_eq!(events.last().unwrap()["kind"], "device-removed");
}

#[tokio::test]
async fn dropping_viewer_reaps_child_without_a_parent_thread_reference_cycle() {
    let fixture = Fixture::new("pass");
    fixture
        .viewer
        .start("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "127.0.0.1")
        .await
        .unwrap();
    let pid = fixture.pid();
    let session = Arc::downgrade(&fixture.viewer.session);
    let presentations = fixture.presentations.clone();
    drop(fixture);
    assert!(session.upgrade().is_none());
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    assert_eq!(*presentations.lock().unwrap(), [true, false]);
}

#[tokio::test]
async fn idle_child_exit_clears_presentation_without_waiting_for_far_status() {
    let fixture = Fixture::new("channel.sendall(b'R'); time.sleep(0.2); sys.exit(1)");
    fixture
        .viewer
        .start("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "127.0.0.1")
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture.presentations.lock().unwrap().last() != Some(&false) {
        assert!(
            Instant::now() < deadline,
            "child EOF must clear presentation without a status RPC"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    fixture
        .viewer
        .stop("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .await
        .unwrap();
    assert_eq!(*fixture.presentations.lock().unwrap(), [true, false]);
}
