use super::*;
#[test]
fn linux_viewer_argv_uses_fixed_app_and_explicit_occupied_seats() {
    let files = MoonlightFiles {
        executable: "/nix/store/viewer/bin/moonlight".into(),
        mapping: "/nix/store/viewer/mapping".into(),
    };
    let seats = vec![
        PathBuf::from("/dev/input/event8"),
        PathBuf::from("/dev/input/event12"),
    ];
    let args = stream_args(
        &files,
        Path::new("/home/korri/.config/moonlight"),
        "192.0.2.1:47989",
        (1920, 1080),
        "x11",
        &seats,
    )
    .unwrap();
    assert!(args.windows(2).any(|v| v == ["-app", "Korri Stream"]));
    assert!(args.windows(2).any(|v| v == ["-codec", "h264"]));
    assert!(args.windows(2).any(|v| v == ["-fps", "60"]));
    assert_eq!(args.iter().filter(|v| *v == "-input").count(), 2);
    assert!(!args
        .iter()
        .any(|v| v == "pair" || v == "-quitappafter" || v == "-viewonly"));
    assert!(
        stream_args(&files, Path::new("/keys"), "host", (640, 480), "x11", &[])
            .unwrap()
            .contains(&"-viewonly".into())
    );
    assert!(stream_args(
        &files,
        Path::new("/keys"),
        "host",
        (640, 480),
        "x11",
        &["/dev/input/../secret".into()]
    )
    .is_err());
}
#[test]
fn linux_viewer_boundaries_refuse_inexact_launch_and_address_injection() {
    assert!(validate_launch("0123456789abcdef0123456789abcdef").is_ok());
    for id in ["", "latest", "../game", "0123456789abcdef0123456789abcdeF"] {
        assert!(validate_launch(id).is_err());
    }
    for address in ["host/path", "user@host", "host?query", "host#fragment"] {
        assert!(server_url(address).is_err());
    }
    assert!(server_url("[::1]:47989").is_ok());
    assert_eq!(
        server_url("host").unwrap().port_or_known_default(),
        Some(47989)
    );
    assert_eq!(
        server_url("host:80").unwrap().port_or_known_default(),
        Some(80)
    );
}
#[test]
fn linux_viewer_helper_exercises_real_command_and_propagates_failure() {
    assert_eq!(
        run(
            Path::new("/bin/sh"),
            &["-c".into(), "printf output; exit 3".into()]
        )
        .unwrap(),
        (false, "output".into())
    );
    assert!(run(Path::new("/nonexistent/helper"), &[]).is_err());
}
#[test]
fn linux_viewer_portal_target_is_unique_and_includes_floating_windows() {
    assert_eq!(
        portal_node(
            r#"{"floating_nodes":[{"id":4,"app_id":"chromium-browser"}]}"#,
            &["chromium-browser"]
        )
        .unwrap(),
        Some(4)
    );
    assert!(portal_node(
        r#"{"nodes":[{"id":4,"app_id":"chromium-browser"},{"id":5,"app_id":"chromium-browser"}]}"#,
        &["chromium-browser"]
    )
    .is_err());
    assert_eq!(portal_node("{}", &["chromium-browser"]).unwrap(), None);
}

struct RecordedLaunch {
    id: String,
    command: Vec<String>,
    environment: BTreeMap<String, String>,
}

#[derive(Default)]
struct ViewerUnits {
    launches: std::sync::Mutex<Vec<RecordedLaunch>>,
    running: std::sync::atomic::AtomicBool,
    refuse_stop: std::sync::atomic::AtomicBool,
}
impl LaunchUnitBackend for ViewerUnits {
    fn launch(
        &self,
        id: &str,
        command: &[String],
        env: &BTreeMap<String, String>,
        _sandbox: &LaunchSandbox,
    ) -> Result<(), crate::host::LaunchUnitError> {
        self.launches.lock().unwrap().push(RecordedLaunch {
            id: id.into(),
            command: command.into(),
            environment: env.clone(),
        });
        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    fn state(&self, _: &str) -> Result<LaunchUnitState, crate::host::LaunchUnitError> {
        Ok(if self.running.load(std::sync::atomic::Ordering::SeqCst) {
            LaunchUnitState::Running
        } else {
            LaunchUnitState::Completed
        })
    }
    fn stop(&self, _: &str) -> Result<(), crate::host::LaunchUnitError> {
        if !self.refuse_stop.load(std::sync::atomic::Ordering::SeqCst) {
            self.running
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }
    fn freeze(&self, _: &str) -> Result<(), crate::host::LaunchUnitError> {
        panic!("viewer must not freeze")
    }
    fn thaw(&self, _: &str) -> Result<(), crate::host::LaunchUnitError> {
        panic!("viewer must not thaw")
    }
    fn live_launch_ids(&self) -> Result<Vec<String>, crate::host::LaunchUnitError> {
        Ok(if self.running.load(std::sync::atomic::Ordering::SeqCst) {
            self.launches
                .lock()
                .unwrap()
                .iter()
                .map(|launch| launch.id.clone())
                .collect()
        } else {
            vec![]
        })
    }
    fn window_pids(
        &self,
        _: &str,
    ) -> Result<std::collections::BTreeSet<i32>, crate::host::LaunchUnitError> {
        Ok([4242].into())
    }
}

#[tokio::test]
async fn linux_viewer_production_lifecycle_uses_broker_seats_focus_and_confirmed_stop() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let certificate = "-----BEGIN CERTIFICATE-----\nY2VydA==\n-----END CERTIFICATE-----\n";
    for (file, value) in [
        ("client.pem", certificate),
        ("key.pem", "private key stays local"),
        ("uniqueid.dat", "client-id"),
    ] {
        std::fs::write(root.path().join(file), value).unwrap();
    }
    let focus = root.path().join("focus");
    std::fs::write(&focus, "portal").unwrap();
    let compositor = root.path().join("swaymsg");
    std::fs::write(&compositor, format!(r##"#!/bin/sh
case "$*" in
 *get_outputs*) printf '[{{"active":true,"current_mode":{{"width":1280,"height":720}}}}]' ;;
 *get_tree*)
   viewer=false; portal=false
   if [ "$(cat '{focus}')" = viewer ]; then viewer=true; else portal=true; fi
   printf '{{"nodes":[{{"id":11,"pid":4242,"focused":%s}},{{"id":22,"pid":4343,"app_id":"chromium-browser","focused":%s}}]}}' "$viewer" "$portal" ;;
 *con_id=11*) printf viewer > '{focus}'; printf '[{{"success":true}}]' ;;
 *con_id=22*) printf portal > '{focus}'; printf '[{{"success":true}}]' ;;
 *) exit 1 ;;
esac
"##, focus=focus.display())).unwrap();
    std::fs::set_permissions(&compositor, std::fs::Permissions::from_mode(0o700)).unwrap();
    let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = server.local_addr().unwrap().to_string();
    let task = tokio::spawn(async move {
        axum::serve(
            server,
            axum::Router::new().route(
                "/serverinfo",
                axum::routing::get(|| async { "<root><uniqueid>sunshine-host</uniqueid></root>" }),
            ),
        )
        .await
        .unwrap();
    });
    let provisioned = Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = provisioned.clone();
    let units = Arc::new(ViewerUnits::default());
    let viewer = ProductionLinuxStreamViewer {
        provision: Arc::new(move |uuid, cert| {
            let record = record.clone();
            Box::pin(async move {
                record.lock().unwrap().push((uuid, cert));
                Ok(crate::MoonlightCertificateProvisioned {
                    server_certificate: certificate.into(),
                })
            })
        }),
        local_seats: Arc::new(|| Box::pin(async { Ok(vec!["/dev/input/event12".into()]) })),
        resolve_files: Arc::new(|| {
            Ok(MoonlightFiles {
                executable: "/nix/store/viewer/bin/moonlight".into(),
                mapping: "/nix/store/viewer/mapping".into(),
            })
        }),
        units: units.clone(),
        swaymsg: compositor,
        sway_socket: root.path().join("sway.sock"),
        portal_ids: vec!["chromium-browser".into()],
        keydir: root.path().into(),
        operation: Arc::new(tokio::sync::Mutex::new(())),
    };
    let launch = "0123456789abcdef0123456789abcdef";
    viewer.start(launch, &address).await.unwrap();
    assert_eq!(
        &*provisioned.lock().unwrap(),
        &[("sunshine-host".into(), certificate.into())]
    );
    assert_eq!(
        std::fs::read_to_string(viewer.pin_path(launch)).unwrap(),
        certificate
    );
    assert_eq!(
        std::fs::metadata(viewer.pin_path(launch))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        viewer.status(launch).await.unwrap(),
        LinuxViewerStatus::Running
    );
    viewer.background(launch).await.unwrap();
    assert_eq!(
        viewer.status(launch).await.unwrap(),
        LinuxViewerStatus::Backgrounded
    );
    viewer.resume_and_focus(launch).await.unwrap();
    assert_eq!(
        viewer.status(launch).await.unwrap(),
        LinuxViewerStatus::Running
    );
    {
        let launches = units.launches.lock().unwrap();
        assert_eq!(
            launches[0].environment["KORRI_MOONLIGHT_SERVER_CERTIFICATE"],
            viewer.pin_path(launch).display().to_string()
        );
        assert_eq!(launches[0].id, launch);
        assert!(launches[0]
            .command
            .windows(2)
            .any(|args| args == ["-input", "/dev/input/event12"]));
        assert!(launches[0]
            .command
            .windows(2)
            .any(|args| args == ["-width", "1280"]));
        assert_eq!(
            launches[0].environment["KORRI_MOONLIGHT_DISABLE_GAMEPAD_QUIT"],
            "1"
        );
    }
    units
        .refuse_stop
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(
        viewer.stop(launch).await.is_err(),
        "never claim stop until systemd confirms it"
    );
    assert!(viewer.pin_path(launch).exists());
    units
        .refuse_stop
        .store(false, std::sync::atomic::Ordering::SeqCst);
    viewer.stop(launch).await.unwrap();
    assert!(!viewer.pin_path(launch).exists());
    assert_eq!(
        viewer.status(launch).await.unwrap(),
        LinuxViewerStatus::Stopped
    );
    task.abort();
}

#[test]
fn linux_viewer_stale_pin_cleanup_preserves_live_launch_pins() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let units = Arc::new(ViewerUnits::default());
    let viewer = ProductionLinuxStreamViewer {
        provision: Arc::new(|_, _| Box::pin(async { panic!("no broker call during cleanup") })),
        local_seats: Arc::new(|| Box::pin(async { panic!("no input call during cleanup") })),
        resolve_files: Arc::new(|| panic!("no plugin call during cleanup")),
        units: units.clone(),
        swaymsg: "/unused".into(),
        sway_socket: "/unused".into(),
        portal_ids: vec![],
        keydir: root.path().into(),
        operation: Arc::new(tokio::sync::Mutex::new(())),
    };
    let launch = "0123456789abcdef0123456789abcdef";
    let path = viewer.pin_path(launch);
    std::fs::write(&path, "stale pin").unwrap();
    viewer.cleanup_pins().unwrap();
    assert!(!path.exists());
    units
        .launch(
            launch,
            &[],
            &BTreeMap::new(),
            &LaunchSandbox::new(&[], &BTreeMap::new()),
        )
        .unwrap();
    std::fs::write(&path, "live pin").unwrap();
    viewer.cleanup_pins().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "live pin");
    units
        .running
        .store(false, std::sync::atomic::Ordering::SeqCst);
    viewer.cleanup_pins().unwrap();
    assert!(!path.exists());
}
