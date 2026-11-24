use super::*;
#[test]
fn viewer_asks_for_the_picture_as_seen_on_a_quarter_turned_main_display() {
    // The Thor on 2026-11-20: portrait panels shown landscape; both outputs active.
    let thor = r#"[
        {"name":"DSI-1","active":true,"transform":"90","current_mode":{"width":1080,"height":1240}},
        {"name":"DSI-2","active":true,"transform":"90","current_mode":{"width":1080,"height":1920}}
    ]"#;
    assert_eq!(viewer_dimensions(thor, Some("DSI-2")), Ok((1920, 1080)));
    // Without display facts the first active output is used, turned.
    assert_eq!(viewer_dimensions(thor, None), Ok((1240, 1080)));
    // An inactive main display falls back to the first active output.
    let off = r#"[{"name":"DSI-2","active":false,"current_mode":{"width":1080,"height":1920}},
                  {"name":"HDMI-A-1","active":true,"transform":"normal","current_mode":{"width":1280,"height":720}}]"#;
    assert_eq!(viewer_dimensions(off, Some("DSI-2")), Ok((1280, 720)));
    // Unturned and transform-less outputs keep their mode.
    let plain = r#"[{"active":true,"current_mode":{"width":1280,"height":720}}]"#;
    assert_eq!(viewer_dimensions(plain, None), Ok((1280, 720)));
    let odin = r#"[{"name":"DSI-1","active":true,"transform":"270","current_mode":{"width":1080,"height":1920}}]"#;
    assert_eq!(viewer_dimensions(odin, Some("DSI-1")), Ok((1920, 1080)));
    assert!(viewer_dimensions("[]", None).is_err());
}

/// Build the picture proof's command through the production argument builder.
/// The caller supplies observed device facts and the installed plugin's own
/// rendered settings. This performs no network, process or device effect.
#[test]
#[ignore = "manual installed-viewer proof; requires KORRI_PROOF_* observations"]
fn emit_the_installed_handheld_proof_command() {
    let required = |name| std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"));
    let manifest: serde_json::Value =
        serde_json::from_str(&required("KORRI_PROOF_MANIFEST")).unwrap();
    let file =
        |name: &str| PathBuf::from(manifest["files"][name].as_str().expect("installed file"));
    let files = ViewerFiles {
        executable: file("viewer"),
        mapping: file("mapping"),
        bootstrap: file("identity-bootstrap"),
    };
    let rendered: Vec<String> =
        serde_json::from_str(&required("KORRI_PROOF_RENDERED_ARGS")).unwrap();
    let seats: Vec<PathBuf> = serde_json::from_str(&required("KORRI_PROOF_INPUT_NODES")).unwrap();
    let platform = platform_from_path(
        Path::new(&required("KORRI_PROOF_COMPATIBLE_FILE")),
        "aarch64",
        Path::new("/dev/dri"),
        |cause| panic!("the observed compatible file is unreadable: {cause}"),
    );
    assert_eq!(platform, "v4l2m2m");
    let command = stream_args(
        &files,
        Path::new(&required("KORRI_PROOF_KEYDIR")),
        &required("KORRI_PROOF_ADDRESS"),
        &rendered,
        platform,
        &seats,
    )
    .unwrap();
    println!(
        "KORRI_PROOF_COMMAND={}",
        serde_json::to_string(&command).unwrap()
    );
}

#[test]
fn linux_viewer_argv_uses_fixed_app_and_explicit_occupied_seats() {
    let files = ViewerFiles {
        executable: "/nix/store/viewer/bin/korri-viewer".into(),
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
        &["-codec".into(), "h264".into()],
        "v4l2m2m",
        &seats,
    )
    .unwrap();
    assert!(args.windows(2).any(|v| v == ["-app", "Korri Stream"]));
    assert!(args.windows(2).any(|v| v == ["-codec", "h264"]));
    assert!(!args.iter().any(|v| v == "-fps" || v == "-bitrate"));
    assert_eq!(args.iter().filter(|v| *v == "-input").count(), 2);
    let request = stream_args(
        &files,
        Path::new("/keys"),
        "host",
        &[],
        "v4l2request",
        &seats,
    )
    .unwrap();
    assert_eq!(request.iter().filter(|v| *v == "-input").count(), 2);
    // Moonlight's SDL system (what `x11` resolves to) exits on `-input`.
    let sdl = stream_args(
        &files,
        Path::new("/home/korri/.config/moonlight"),
        "192.0.2.1:47989",
        &[],
        "x11",
        &seats,
    )
    .unwrap();
    assert!(!sdl.iter().any(|v| v == "-input" || v == "-viewonly"));
    assert!(!args
        .iter()
        .any(|v| v == "pair" || v == "-quitappafter" || v == "-viewonly"));
    assert!(
        stream_args(&files, Path::new("/keys"), "host", &[], "x11", &[])
            .unwrap()
            .contains(&"-viewonly".into())
    );
    assert!(stream_args(
        &files,
        Path::new("/keys"),
        "host",
        &[],
        "x11",
        &["/dev/input/../secret".into()]
    )
    .is_err());
}
#[test]
fn linux_viewer_vaapi_argv_keeps_sdl_fallback_interactive() {
    let files = ViewerFiles {
        executable: "/nix/store/viewer/bin/korri-viewer".into(),
        mapping: "/nix/store/viewer/mapping".into(),
        bootstrap: "/nix/store/viewer/bootstrap".into(),
    };
    let seats = [PathBuf::from("/dev/input/event12")];
    let args = stream_args(&files, Path::new("/keys"), "host", &[], "x11_vaapi", &seats).unwrap();
    assert!(args
        .windows(2)
        .any(|pair| pair == ["-platform", "x11_vaapi"]));
    // VA-API uses evdev/udev auto-discovery without explicit inputs. If its
    // init fails, platform_check returns SDL, which exits on any -input.
    assert!(!args.iter().any(|arg| arg == "-input" || arg == "-viewonly"));
    let watch = stream_args(&files, Path::new("/keys"), "host", &[], "x11_vaapi", &[]).unwrap();
    assert!(watch.contains(&"-viewonly".into()));
    assert!(!watch.contains(&"-input".into()));
}

#[test]
fn linux_viewer_argv_unbrackets_ipv6_without_changing_the_control_url() {
    let files = ViewerFiles {
        executable: "/nix/store/viewer/bin/korri-viewer".into(),
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
        let args = stream_args(&files, Path::new("/keys"), address, &[], "x11", &[]).unwrap();
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
struct ViewerShell;
impl crate::host::ShellUnit for ViewerShell {
    fn window_pids(&self) -> Result<std::collections::BTreeSet<i32>, crate::host::LaunchUnitError> {
        Ok([4343].into_iter().collect())
    }
    fn freeze(&self) -> Result<(), crate::host::LaunchUnitError> {
        Ok(())
    }
    fn thaw(&self) -> Result<(), crate::host::LaunchUnitError> {
        Ok(())
    }
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
    certificates: std::sync::Mutex<Vec<String>>,
    controllers: std::sync::Mutex<Vec<Option<u32>>>,
    bootstraps: std::sync::Mutex<Vec<String>>,
    /// State reads that still see the unit deactivating after a stop.
    deactivating: std::sync::atomic::AtomicUsize,
    /// The next state read sees a unit whose process ended unsuccessfully.
    failed: std::sync::atomic::AtomicBool,
    stops: std::sync::atomic::AtomicUsize,
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
        controller: Option<u32>,
    ) -> Result<(), crate::host::LaunchUnitError> {
        self.certificates.lock().unwrap().push(certificate.into());
        self.controllers.lock().unwrap().push(controller);
        self.launch(id, command, env, sandbox)
    }
    fn state(&self, _: &str) -> Result<LaunchUnitState, crate::host::LaunchUnitError> {
        use std::sync::atomic::Ordering::SeqCst;
        if self.failed.swap(false, SeqCst) {
            return Err(crate::host::LaunchUnitError {
                kind: crate::host::LaunchUnitErrorKind::UnitFailed,
                message: "viewer exited with status 1".into(),
            });
        }
        if self
            .deactivating
            .fetch_update(SeqCst, SeqCst, |left| left.checked_sub(1))
            .is_ok()
        {
            return Ok(LaunchUnitState::Stopping);
        }
        Ok(if self.running.load(std::sync::atomic::Ordering::SeqCst) {
            LaunchUnitState::Running
        } else {
            LaunchUnitState::Completed
        })
    }
    fn stop(&self, _: &str) -> Result<(), crate::host::LaunchUnitError> {
        self.stops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
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
    fn runner_id(&self, id: &str) -> Result<Option<String>, crate::host::LaunchUnitError> {
        Ok(self
            .launches
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|launch| launch.id == id)
            .and_then(|launch| launch.environment.get(crate::host::RUNNER_ID_ENV).cloned()))
    }
}

#[tokio::test]
async fn linux_viewer_production_lifecycle_uses_broker_seats_focus_and_confirmed_stop() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let certificate = "-----BEGIN CERTIFICATE-----\nY2VydA==\n-----END CERTIFICATE-----\n";
    let focus = root.path().join("focus");
    std::fs::write(&focus, "shell").unwrap();
    let commands = root.path().join("commands");
    let compositor = root.path().join("swaymsg");
    std::fs::write(&compositor, format!(r##"#!/bin/sh
case "$*" in
 *get_outputs*) printf '[{{"active":true,"current_mode":{{"width":1280,"height":720}}}}]' ;;
 *get_tree*)
   viewer=false; shell=false
   if [ "$(cat '{focus}')" = viewer ]; then viewer=true; else shell=true; fi
   printf '{{"nodes":[{{"id":11,"pid":4242,"focused":%s}},{{"id":22,"pid":4343,"app_id":"korri-shell","focused":%s}}]}}' "$viewer" "$shell" ;;
 *con_id=11*) printf viewer > '{focus}'; printf '%s\n' "$*" >> '{commands}'; printf '[{{"success":true}}]' ;;
 *con_id=22*) printf shell > '{focus}'; printf '%s\n' "$*" >> '{commands}'; printf '[{{"success":true}}]' ;;
 *) exit 1 ;;
esac
"##, focus=focus.display(), commands=commands.display())).unwrap();
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
    let _sway_peer =
        crate::host::sway_test_peer::SwayTestPeer::start(compositor, root.path().join("sway.sock"));
    let viewer = ProductionLinuxStreamViewer {
        settings: Arc::new(|plugin, display| {
            assert_eq!(plugin, ViewerPlugin::Korri);
            Ok(vec![
                "-width".into(),
                display.width.to_string(),
                "-height".into(),
                display.height.to_string(),
                "-codec".into(),
                "h264".into(),
            ])
        }),
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
        resolve_files: Arc::new(|plugin| {
            assert_eq!(plugin, ViewerPlugin::Korri);
            Ok(ViewerFiles {
                executable: "/nix/store/viewer/bin/korri-viewer".into(),
                mapping: "/nix/store/viewer/mapping".into(),
                bootstrap: "/nix/store/viewer/bootstrap".into(),
            })
        }),
        units: units.clone(),
        compositor: SwayCompositorControl::new(
            root.path().join("sway.sock"),
            Duration::from_secs(3),
        ),
        shell: Some(Arc::new(ViewerShell)),
        keydir: root.path().join("unreadable-identity"),
        operation: Arc::new(tokio::sync::Mutex::new(())),
    };
    let launch = "0123456789abcdef0123456789abcdef";
    assert_eq!(
        viewer.display(),
        Some(crate::launcher::plugin_launch::PluginDisplay {
            width: 1280,
            height: 720
        })
    );
    viewer.start_lifecycle(launch, &address).await.unwrap();
    // Focus remains a separate effect, requested only after socket Start.
    viewer.mutate(launch, Some(false)).await.unwrap();
    assert_eq!(
        &*provisioned.lock().unwrap(),
        &[("sunshine-host".into(), certificate.into())]
    );
    assert_eq!(
        &*units.certificates.lock().unwrap(),
        &[certificate.to_owned()]
    );
    assert_eq!(
        &*units.controllers.lock().unwrap(),
        &[Some(unsafe { libc::geteuid() })]
    );
    assert!(
        !viewer.keydir.exists(),
        "korrid never creates the identity directory"
    );
    viewer.mutate(launch, Some(true)).await.unwrap();
    assert_eq!(std::fs::read_to_string(&focus).unwrap(), "shell");
    viewer.mutate(launch, Some(false)).await.unwrap();
    assert_eq!(std::fs::read_to_string(&focus).unwrap(), "viewer");
    // The viewer fills its output on every return; the shell never does.
    let commands = std::fs::read_to_string(&commands).unwrap();
    let viewer_commands: Vec<_> = commands
        .lines()
        .filter(|c| c.contains("con_id=11"))
        .collect();
    assert!(!viewer_commands.is_empty());
    assert!(viewer_commands
        .iter()
        .all(|c| c.ends_with("[con_id=11] move container to workspace current; [con_id=11] fullscreen enable; [con_id=11] focus")));
    assert!(commands
        .lines()
        .filter(|c| c.contains("con_id=22"))
        .all(|c| !c.contains("fullscreen")));
    {
        let launches = units.launches.lock().unwrap();
        assert!(!launches[0]
            .environment
            .contains_key("KORRI_MOONLIGHT_SERVER_CERTIFICATE"));
        assert_eq!(launches[0].id, launch);
        // Occupied broker seats make the viewer interactive. Only evdev
        // platforms name them; X11 choices auto-discover controllers.
        assert!(!launches[0].command.iter().any(|arg| arg == "-viewonly"));
        assert_eq!(
            launches[0]
                .command
                .windows(2)
                .any(|args| args == ["-input", "/dev/input/event12"]),
            !matches!(platform(), "x11" | "x11_vaapi")
        );
        assert!(launches[0]
            .command
            .windows(2)
            .any(|args| args == ["-width", "1280"]));
        assert_eq!(
            launches[0].environment["KORRI_VIEWER_DISABLE_GAMEPAD_QUIT"],
            "1"
        );
        // Gamepad events must reach Moonlight without X11 window focus.
        assert_eq!(
            launches[0].environment["SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS"],
            "1"
        );
    }
    assert_eq!(
        units.launches.lock().unwrap()[0].environment[crate::host::RUNNER_ID_ENV],
        "@korri:viewer/viewer"
    );
    // A --no-block stop leaves the unit deactivating for a moment (#472).
    units
        .deactivating
        .store(3, std::sync::atomic::Ordering::SeqCst);
    viewer.end_unit(launch).await.unwrap();
    assert_eq!(
        units.deactivating.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "never claim stop until systemd confirms it"
    );
    assert_eq!(units.stops.load(std::sync::atomic::Ordering::SeqCst), 1);
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
    let dri = root.path().join("dri");
    std::fs::create_dir(&dri).unwrap();
    assert_eq!(
        platform_from_path(&path, "x86_64", &dri, |_| panic!(
            "Non-DT hardware is not an error"
        )),
        "x11"
    );
    // A render node must not override any device-tree platform decision.
    std::fs::write(dri.join("renderD128"), b"").unwrap();
    for compatible in [
        b"device\0qcom,sm8550\0".as_slice(),
        b"qcom,sm8250\0".as_slice(),
        b"samsung,gts6lwifi\0qcom,sm8150p\0qcom,sm8150\0".as_slice(),
        b"oneplus,enchilada\0qcom,sdm845\0".as_slice(),
    ] {
        std::fs::write(&path, compatible).unwrap();
        assert_eq!(
            platform_from_path(&path, "x86_64", &dri, |_| panic!("readable hardware fact")),
            "v4l2m2m"
        );
    }
    for compatible in [
        b"anbernic,rg353p\0rockchip,rk3566\0".as_slice(),
        b"anbernic,rg35xx-pro\0allwinner,sun50i-h700\0".as_slice(),
        b"anbernic,rg35xx-sp\0allwinner,sun50i-h700\0".as_slice(),
    ] {
        std::fs::write(&path, compatible).unwrap();
        assert_eq!(
            platform_from_path(&path, "x86_64", &dri, |_| panic!("readable hardware fact")),
            "v4l2request"
        );
    }
    std::fs::write(&path, b"other-device\0").unwrap();
    assert_eq!(
        platform_from_path(&path, "x86_64", &dri, |_| panic!("readable hardware fact")),
        "x11"
    );
    let mut reason = None;
    assert_eq!(
        platform_from_path(root.path(), "x86_64", &dri, |error| {
            reason = Some(error.to_string())
        }),
        "x11"
    );
    assert!(reason.is_some_and(|reason| !reason.is_empty()));
}

#[test]
fn linux_viewer_uses_vaapi_only_on_non_dt_x86_with_an_existing_render_node() {
    let root = tempfile::tempdir().unwrap();
    let compatible = root.path().join("compatible");
    let dri = root.path().join("dri");
    let select = |architecture| {
        platform_from_path(&compatible, architecture, &dri, |_| {
            panic!("an absent device tree is normal")
        })
    };
    assert_eq!(select("x86_64"), "x11", "missing DRM directory");
    std::fs::create_dir(&dri).unwrap();
    assert_eq!(select("x86_64"), "x11", "no render node");
    for name in ["card0", "renderD", "renderDfoo", "renderD128extra"] {
        std::fs::write(dri.join(name), b"").unwrap();
    }
    assert_eq!(select("x86_64"), "x11", "not a render node name");
    let render = dri.join("renderD129");
    std::os::unix::fs::symlink(root.path().join("absent-device"), &render).unwrap();
    assert_eq!(select("x86_64"), "x11", "broken node symlink");
    std::fs::remove_file(&render).unwrap();
    std::fs::write(&render, b"").unwrap();
    assert_eq!(select("x86_64"), "x11_vaapi");
    for architecture in ["aarch64", "arm", "riscv64"] {
        assert_eq!(select(architecture), "x11", "unchanged non-x86 fallback");
    }
    std::fs::write(&compatible, b"unknown-device\0").unwrap();
    assert_eq!(select("x86_64"), "x11", "a device tree keeps its old rule");
}

#[test]
fn linux_viewer_drops_plugin_invariant_flags_and_preserves_native_settings() {
    let rendered: Vec<String> = [
        "-app",
        "Other",
        "-port=123",
        "-keydir",
        "/keys",
        "-mapping",
        "/map",
        "-platform",
        "drm",
        "-input",
        "/dev/input/event1",
        "-viewonly",
        "-windowed",
        "-autowindowresize",
        "-width",
        "854",
        "-height",
        "480",
        "-codec",
        "h264",
        "stream",
        "other-host",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let (args, warnings) = plugin_args(&rendered);
    assert_eq!(args, ["-width", "854", "-height", "480", "-codec", "h264"]);
    assert_eq!(warnings.len(), 11);
    assert!(!args.iter().any(|v| v == "-fps" || v == "-bitrate"));
}

#[test]
fn linux_viewer_settings_use_local_device_and_matching_game_system_selected_profile() {
    use serde_json::json;
    const ID: &str = "@korri:viewer/viewer";
    let root = tempfile::tempdir().unwrap();
    let package = root.path().join("plugin");
    std::fs::create_dir(&package).unwrap();
    let source = r#"export const name='viewer';export const runners={viewer:{program:'program'}};export const handlers={
      'settings.describe':()=>({revision:'v1',schema:{}}),
      'settings.validate': input=>({valid:false,diagnostics:input.values.plugin?.video?.codec==='bad'?[{severity:'error',code:'codec',path:['plugin','video','codec'],message:'unsupported codec'}]:[]}),
      'settings.render':input=>({args:[JSON.stringify(input.settings),String(input.display.width)]})
    };"#;
    std::fs::write(package.join("plugin.ts"), source).unwrap();
    let registry = crate::plugin::PluginRegistry::from_installed(vec![
        crate::plugin_installation::EnabledPackage {
            id: "@korri:viewer".into(),
            package,
            files: BTreeMap::from([("program".into(), "/bin/sh".into())]),
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            requires: vec![],
            hosts: vec![],
            credential_host: None,
            credential_token_url: None,
        },
    ])
    .unwrap();
    let device = json!({"host":{"runners":{ID:{"settings":{"plugin":{"video":{"fps":30,"codec":"h264"}}}}}},"systems":{"gba":{"runners":{ID:{"settings":{"plugin":{"video":{"fps":45}}}}}}}});
    let library = json!({"library":{"game":{"releases":[{"id":"one","system":"gba","identity":{"kind":"provider","value":"@korri:content/one"},"runners":{ID:{"settings":{"plugin":{"video":{"fps":5}}}}}}],"runners":{ID:{"settings":{"plugin":{"video":{"codec":"h265"}}}}},"profiles":{"quiet":{"title":"Quiet","runners":{ID:{"settings":{"plugin":{"video":{"fps":90}}}}}},"unselected":{"title":"Unselected","runners":{ID:{"settings":{"plugin":{"video":{"fps":120}}}}}}},"profile":"quiet"}}});
    std::fs::create_dir(root.path().join("catalog")).unwrap();
    std::fs::write(root.path().join("device.yaml"), device.to_string()).unwrap();
    std::fs::write(
        root.path().join("catalog/library.yaml"),
        library.to_string(),
    )
    .unwrap();
    let display = crate::launcher::plugin_launch::PluginDisplay {
        width: 854,
        height: 480,
    };
    let args = render_settings(root.path(), &registry, ID, Some("game"), display.clone()).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&args[0]).unwrap(),
        json!({"plugin":{"video":{"fps":90,"codec":"h265"}}})
    );
    assert_eq!(args[1], "854");
    let args = render_settings(
        root.path(),
        &registry,
        ID,
        Some("not-in-local-library"),
        display.clone(),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&args[0]).unwrap(),
        json!({"plugin":{"video":{"fps":30,"codec":"h264"}}})
    );
    let snapshot = crate::script::source::SourceSnapshot::plugin(source).unwrap();
    let settings =
        serde_json::from_value(json!({"plugin":{"video":{"fps":60,"codec":"bad"}}})).unwrap();
    let (args, warnings) =
        crate::launcher::typed_settings::render(&snapshot, ID, &settings, Some(display), "build")
            .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&args[0]).unwrap(),
        json!({"plugin":{"video":{"fps":60}}})
    );
    assert_eq!(warnings[0].setting, "plugin.video.codec");
}

#[test]
fn linux_viewer_passes_plugin_audio_and_bitrate_unchanged_without_host_defaults() {
    let files = ViewerFiles {
        executable: "/nix/store/viewer/bin/korri-viewer".into(),
        mapping: "/nix/store/viewer/mapping".into(),
        bootstrap: "/nix/store/viewer/bootstrap".into(),
    };
    let rendered = vec!["-localaudio".into(), "-bitrate".into(), "6173".into()];
    let args = stream_args(&files, Path::new("/keys"), "host", &rendered, "x11", &[]).unwrap();
    assert!(args.ends_with(&rendered));
    assert_eq!(args.iter().filter(|arg| *arg == "-localaudio").count(), 1);
    assert_eq!(args.iter().filter(|arg| *arg == "-bitrate").count(), 1);

    let defaults = stream_args(&files, Path::new("/keys"), "host", &[], "x11", &[]).unwrap();
    for flag in [
        "-localaudio",
        "-bitrate",
        "-fps",
        "-width",
        "-height",
        "-codec",
    ] {
        assert!(!defaults.iter().any(|arg| arg == flag), "{flag}");
    }
}

#[tokio::test]
async fn korri_viewer_unit_reports_to_korrid_and_waits_for_start_before_focus() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let commands = root.path().join("commands");
    let compositor = root.path().join("swaymsg");
    std::fs::write(
        &compositor,
        format!(
            r##"#!/bin/sh
case "$*" in
 *get_outputs*) printf '[{{"active":true,"current_mode":{{"width":1280,"height":720}}}}]' ;;
 *) printf '%s\n' "$*" >> '{commands}'; exit 1 ;;
esac
"##,
            commands = commands.display()
        ),
    )
    .unwrap();
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
    let units = Arc::new(ViewerUnits::default());
    let _sway_peer =
        crate::host::sway_test_peer::SwayTestPeer::start(compositor, root.path().join("sway.sock"));
    let certificate = "-----BEGIN CERTIFICATE-----\nY2VydA==\n-----END CERTIFICATE-----\n";
    let viewer = ProductionLinuxStreamViewer {
        settings: Arc::new(|plugin, _| {
            // The viewer's own runner, never Moonlight's stored settings.
            assert_eq!(plugin.runner(), "@korri:viewer/viewer");
            Ok(vec!["-codec".into(), "h264".into(), "-localaudio".into()])
        }),
        provision: Arc::new(move |_, _| {
            Box::pin(async move {
                Ok(crate::MoonlightCertificateProvisioned {
                    server_certificate: certificate.into(),
                })
            })
        }),
        local_seats: Arc::new(|| Box::pin(async { Ok(vec![]) })),
        resolve_files: Arc::new(|plugin| {
            assert_eq!(plugin, ViewerPlugin::Korri);
            Ok(ViewerFiles {
                executable: "/nix/store/viewer/bin/korri-viewer".into(),
                mapping: "/nix/store/viewer/mapping".into(),
                bootstrap: "/nix/store/viewer/bootstrap".into(),
            })
        }),
        units: units.clone(),
        compositor: SwayCompositorControl::new(
            root.path().join("sway.sock"),
            Duration::from_secs(3),
        ),
        shell: Some(Arc::new(ViewerShell)),
        keydir: root.path().join("unreadable-identity"),
        operation: Arc::new(tokio::sync::Mutex::new(())),
    };
    let launch = "0123456789abcdef0123456789abcdef";
    let socket = viewer.start_lifecycle(launch, &address).await.unwrap();
    assert_eq!(
        socket,
        Path::new("/run/korri-game-0123456789abcdef0123456789abcdef/control/viewer.sock")
    );
    let controller = unsafe { libc::geteuid() };
    assert_eq!(&*units.controllers.lock().unwrap(), &[Some(controller)]);
    assert_eq!(
        &*units.certificates.lock().unwrap(),
        &[certificate.to_owned()]
    );
    let launches = units.launches.lock().unwrap();
    let command = &launches[0].command;
    assert_eq!(
        command[..8],
        [
            "/nix/store/viewer/bin/korri-viewer".to_owned(),
            "--lifecycle-socket".into(),
            socket.display().to_string(),
            "--launch-id".into(),
            launch.into(),
            "--controller-uid".into(),
            controller.to_string(),
            "stream".into(),
        ]
    );
    // Moonlight's arguments follow unchanged, with the rendered settings last.
    assert!(command
        .windows(2)
        .any(|pair| pair == ["-app", "Korri Stream"]));
    assert!(command.contains(&"-viewonly".to_owned()));
    assert!(command.ends_with(&["-codec".into(), "h264".into(), "-localaudio".into()]));
    // No focus yet: the socket adapter asks for it after Start is confirmed.
    assert!(!commands.exists());
    // The unit names its viewer, so a restarted korrid can route to it.
    assert_eq!(
        launches[0].environment.get(crate::host::RUNNER_ID_ENV),
        Some(&"@korri:viewer/viewer".to_owned())
    );
    drop(launches);
    task.abort();
}

#[test]
fn korri_viewer_cleanup_waits_for_its_unit_to_end_and_resets_a_failed_one() {
    use std::sync::atomic::Ordering::SeqCst;
    let units = Arc::new(ViewerUnits::default());
    let viewer = ProductionLinuxStreamViewer {
        settings: Arc::new(|_, _| Ok(vec![])),
        provision: Arc::new(|_, _| Box::pin(async { Err(LinuxViewerError::new("unused")) })),
        local_seats: Arc::new(|| Box::pin(async { Ok(vec![]) })),
        resolve_files: Arc::new(|_| Err(LinuxViewerError::new("unused"))),
        units: units.clone(),
        compositor: None,
        shell: None,
        keydir: "/unused".into(),
        operation: Arc::new(tokio::sync::Mutex::new(())),
    };
    let launch = "0123456789abcdef0123456789abcdef";
    // A --no-block stop leaves the unit deactivating for a moment.
    units.deactivating.store(3, SeqCst);
    viewer.ended(launch).unwrap();
    assert_eq!(units.deactivating.load(SeqCst), 0);
    // A viewer that ended unsuccessfully is reset by a second stop.
    let before = units.stops.load(SeqCst);
    units.failed.store(true, SeqCst);
    viewer.ended(launch).unwrap();
    assert_eq!(units.stops.load(SeqCst), before + 2);
}

/// #472, against the real systemd backend and a scripted `systemctl`: after
/// the --no-block stop the unit stays deactivating for five reads. Moonlight's
/// first stop request waits until systemd reports the unit gone, inactive or
/// failed, and claims no stop that systemd cannot confirm.
#[tokio::test]
async fn viewer_stop_waits_for_a_deactivating_unit_to_end() {
    use std::os::unix::fs::PermissionsExt;
    let launch = "0123456789abcdef0123456789abcdef";
    let unit = format!("korri-game-{launch}.service");
    let failed = "LoadState=loaded\nActiveState=failed\nExecMainCode=1\nExecMainStatus=1\n";
    // (what systemd reports after deactivating, its exit status, stop
    // requests, reset-failed requests, the error if the stop is not confirmed)
    for (end, status, stops, resets, refusal) in [
        ("LoadState=not-found\n", 0, 1, 0, None),
        ("LoadState=loaded\nActiveState=inactive\n", 0, 1, 0, None),
        // Moonlight can exit unsuccessfully on SIGTERM. Its unit then holds
        // the name until a second stop resets it.
        (failed, 0, 2, 1, None),
        ("", 1, 1, 0, Some("systemctl show failed")),
    ] {
        let root = tempfile::tempdir().unwrap();
        let state = root.path();
        std::fs::write(state.join("deactivating"), "5").unwrap();
        std::fs::write(state.join("end"), end).unwrap();
        std::fs::write(state.join("status"), status.to_string()).unwrap();
        let systemctl = state.join("systemctl");
        std::fs::write(
            &systemctl,
            format!(
                r#"#!/bin/sh
state='{state}'
echo "$*" >> "$state/calls"
test "$4" = '{unit}' || exit 2
case "$3" in
 show)
   if ! test -e "$state/stopped"; then
     printf 'LoadState=loaded\nActiveState=active\n'
   elif left=$(cat "$state/deactivating"); test "$left" -gt 0; then
     echo $((left - 1)) > "$state/deactivating"
     printf 'LoadState=loaded\nActiveState=deactivating\n'
   else
     cat "$state/end"
     exit "$(cat "$state/status")"
   fi ;;
 stop) case "$*" in *--no-block*) touch "$state/stopped" ;; *) exit 42 ;; esac ;;
 reset-failed) printf 'LoadState=not-found\n' > "$state/end" ;;
 *) exit 1 ;;
esac
"#,
                state = state.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&systemctl, std::fs::Permissions::from_mode(0o700)).unwrap();
        let units = SystemdLaunchUnitBackend::new(systemctl.clone(), systemctl, 1000, 1000)
            .unwrap()
            .for_stream_viewer();
        let viewer = ProductionLinuxStreamViewer {
            settings: Arc::new(|_, _| Ok(vec![])),
            provision: Arc::new(|_, _| Box::pin(async { Err(LinuxViewerError::new("unused")) })),
            local_seats: Arc::new(|| Box::pin(async { Ok(vec![]) })),
            resolve_files: Arc::new(|_| Err(LinuxViewerError::new("unused"))),
            units: Arc::new(units),
            compositor: None,
            shell: None,
            keydir: "/unused".into(),
            operation: Arc::new(tokio::sync::Mutex::new(())),
        };
        let result = viewer.end_unit(launch).await;
        let calls = std::fs::read_to_string(state.join("calls")).unwrap();
        let count = |verb: &str| {
            calls
                .lines()
                .filter(|call| call.starts_with(&format!("--system --no-ask-password {verb} ")))
                .count()
        };
        match refusal {
            None => result.unwrap_or_else(|error| panic!("{end:?}: {error}\n{calls}")),
            Some(refusal) => {
                let error = result.expect_err("an unreadable unit is not a confirmed stop");
                assert!(error.to_string().contains(refusal), "{error}");
            }
        }
        assert_eq!(
            std::fs::read_to_string(state.join("deactivating"))
                .unwrap()
                .trim(),
            "0",
            "{end:?}: the stop read the unit through every deactivating read"
        );
        assert_eq!(count("stop"), stops, "{end:?}\n{calls}");
        assert_eq!(count("reset-failed"), resets, "{end:?}\n{calls}");
    }
}

/// One HTTP server on the loopback that answers `/serverinfo` with `body`.
async fn serverinfo(body: String) -> (String, tokio::task::JoinHandle<()>) {
    let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = server.local_addr().unwrap().to_string();
    let task = tokio::spawn(async move {
        axum::serve(
            server,
            axum::Router::new().route(
                "/serverinfo",
                axum::routing::get(move || {
                    let body = body.clone();
                    async move { body }
                }),
            ),
        )
        .await
        .unwrap();
    });
    (address, task)
}

// #434 D10: the Android viewer asks the holder's broker for this Sunshine's
// certificate by the host UUID that the Linux viewer reads.
#[tokio::test]
async fn sunshine_host_uuid_is_read_from_serverinfo_and_checked() {
    let (address, task) = serverinfo(
        "<root status_code=\"200\"><hostname>a</hostname><uniqueid>AB12CD34-0000-4000-8000-0123456789AB</uniqueid></root>".into(),
    )
    .await;
    assert_eq!(
        sunshine_host_uuid(&address).await.unwrap(),
        "AB12CD34-0000-4000-8000-0123456789AB"
    );
    task.abort();
    for (body, refusal) in [
        ("<root></root>".to_owned(), "no host UUID"),
        (
            format!("<root><uniqueid>{}</uniqueid></root>", "a".repeat(257)),
            "host UUID is invalid",
        ),
        (
            format!("<root>{}</root>", "x".repeat(70 * 1024)),
            "exceeds budget",
        ),
    ] {
        let (address, task) = serverinfo(body).await;
        let error = sunshine_host_uuid(&address).await.unwrap_err().to_string();
        assert!(error.contains(refusal), "{error}");
        task.abort();
    }
    assert!(sunshine_host_uuid("name@127.0.0.1")
        .await
        .unwrap_err()
        .to_string()
        .contains("invalid Moonlight address"));
}
