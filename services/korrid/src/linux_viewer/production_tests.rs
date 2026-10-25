use super::*;
#[test]
fn linux_viewer_argv_uses_fixed_app_and_explicit_occupied_seats() {
    let files = MoonlightFiles {
        executable: "/nix/store/viewer/bin/moonlight".into(),
        mapping: "/nix/store/viewer/mapping".into(),
        bootstrap: "/nix/store/viewer/bootstrap".into(),
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
        "v4l2m2m",
        &seats,
    )
    .unwrap();
    assert!(args.windows(2).any(|v| v == ["-app", "Korri Stream"]));
    assert!(args.windows(2).any(|v| v == ["-codec", "h264"]));
    assert!(args.windows(2).any(|v| v == ["-fps", "60"]));
    assert_eq!(args.iter().filter(|v| *v == "-input").count(), 2);
    // Moonlight's SDL system (what `x11` resolves to) exits on `-input`.
    let sdl = stream_args(
        &files,
        Path::new("/home/korri/.config/moonlight"),
        "192.0.2.1:47989",
        (640, 480),
        "x11",
        &seats,
    )
    .unwrap();
    assert!(!sdl.iter().any(|v| v == "-input" || v == "-viewonly"));
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
fn linux_viewer_argv_unbrackets_ipv6_without_changing_the_control_url() {
    let files = MoonlightFiles {
        executable: "/nix/store/viewer/bin/moonlight".into(),
        mapping: "/nix/store/viewer/mapping".into(),
        bootstrap: "/nix/store/viewer/bootstrap".into(),
    };
    for (address, host, port) in [
        ("[::1]:47989", "::1", "47989"),
        ("[2001:db8::1]:49217", "2001:db8::1", "49217"),
        ("192.0.2.1:47989", "192.0.2.1", "47989"),
        ("sunshine:80", "sunshine", "80"),
        ("sunshine", "sunshine", "47989"),
    ] {
        let args =
            stream_args(&files, Path::new("/keys"), address, (640, 480), "x11", &[]).unwrap();
        assert_eq!(args.last().map(String::as_str), Some(host));
        assert!(args.windows(2).any(|pair| pair == ["-port", port]));
    }
    assert_eq!(
        server_url("[::1]:47989").unwrap().as_str(),
        "http://[::1]:47989/serverinfo"
    );
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
    certificates: std::sync::Mutex<Vec<String>>,
    bootstraps: std::sync::Mutex<Vec<String>>,
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
    fn bootstrap_viewer(
        &self,
        id: &str,
        _: &[String],
        _: &BTreeMap<String, String>,
        _: &LaunchSandbox,
    ) -> Result<String, crate::host::LaunchUnitError> {
        self.bootstraps.lock().unwrap().push(id.into());
        Ok("-----BEGIN CERTIFICATE-----\nY2VydA==\n-----END CERTIFICATE-----\n".into())
    }
    fn launch_viewer(
        &self,
        id: &str,
        command: &[String],
        env: &BTreeMap<String, String>,
        sandbox: &LaunchSandbox,
        certificate: &str,
    ) -> Result<(), crate::host::LaunchUnitError> {
        self.certificates.lock().unwrap().push(certificate.into());
        self.launch(id, command, env, sandbox)
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
                bootstrap: "/nix/store/viewer/bootstrap".into(),
            })
        }),
        units: units.clone(),
        swaymsg: compositor,
        sway_socket: root.path().join("sway.sock"),
        portal_ids: vec!["chromium-browser".into()],
        keydir: root.path().join("unreadable-identity"),
        operation: Arc::new(tokio::sync::Mutex::new(())),
    };
    let launch = "0123456789abcdef0123456789abcdef";
    viewer.start(launch, &address).await.unwrap();
    assert_eq!(
        &*provisioned.lock().unwrap(),
        &[("sunshine-host".into(), certificate.into())]
    );
    assert_eq!(
        &*units.certificates.lock().unwrap(),
        &[certificate.to_owned()]
    );
    assert!(
        !viewer.keydir.exists(),
        "korrid never opens or creates the identity directory"
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
        assert!(!launches[0]
            .environment
            .contains_key("KORRI_MOONLIGHT_SERVER_CERTIFICATE"));
        assert_eq!(launches[0].id, launch);
        // Occupied broker seats make the viewer interactive. Only evdev
        // platforms name them; the SDL `x11` system reads every controller.
        assert!(!launches[0].command.iter().any(|arg| arg == "-viewonly"));
        assert_eq!(
            launches[0]
                .command
                .windows(2)
                .any(|args| args == ["-input", "/dev/input/event12"]),
            platform() != "x11"
        );
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
    units
        .refuse_stop
        .store(false, std::sync::atomic::Ordering::SeqCst);
    viewer.stop(launch).await.unwrap();
    assert_eq!(
        viewer.status(launch).await.unwrap(),
        LinuxViewerStatus::Stopped
    );
    task.abort();
}

#[test]
fn linux_viewer_bootstrap_rejects_live_and_selected_unit_id_collisions() {
    let live = "11".repeat(16);
    let selected = "22".repeat(16);
    let fresh = "33".repeat(16);
    let mut candidates = [live.clone(), selected.clone(), fresh.clone()].into_iter();
    let id = fresh_bootstrap_id(&[live], &selected, || candidates.next().unwrap());
    assert_eq!(id, fresh);
    assert!(validate_launch(&id).is_ok());
}

#[test]
fn linux_viewer_reads_sysfs_device_tree_and_reports_read_failures() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("compatible");
    assert_eq!(
        platform_from_path(&path, |_| panic!("Non-DT hardware is not an error")),
        "x11"
    );
    for compatible in [
        b"device\0qcom,sm8550\0".as_slice(),
        b"qcom,sm8250\0".as_slice(),
    ] {
        std::fs::write(&path, compatible).unwrap();
        assert_eq!(
            platform_from_path(&path, |_| panic!("readable hardware fact")),
            "v4l2m2m"
        );
    }
    std::fs::write(&path, b"other-device\0").unwrap();
    assert_eq!(
        platform_from_path(&path, |_| panic!("readable hardware fact")),
        "x11"
    );
    let mut reason = None;
    assert_eq!(
        platform_from_path(root.path(), |error| reason = Some(error.to_string())),
        "x11"
    );
    assert!(reason.is_some_and(|reason| !reason.is_empty()));
}
