//! Exercise native prepare and catalog placement against one controlled Sway process.
use super::*;
use crate::launcher::plugin_launch::PluginDisplay;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn isolated(name: &str) -> bool {
    const CHILD: &str = "KORRI_CLIENT_SHAPE_AUTHORITY_TEST";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("host::client_shape_tests::{name}"),
            "--nocapture",
        ])
        .env(CHILD, name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

struct Fixture {
    root: tempfile::TempDir,
    runtime: HostRuntime,
    backend: Arc<super::control::InMemoryLaunchUnitBackend>,
}
impl Fixture {
    fn new(displays: usize) -> Self {
        let root = tempfile::tempdir().unwrap();
        crate::plugin_test_fixtures::write_gba_library(root.path());
        fs::create_dir(root.path().join("roms")).unwrap();
        fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        let registry = crate::plugin_test_fixtures::installed(root.path());
        fs::write(root.path().join("mode"), "1920x1080").unwrap();
        let sway = root.path().join("swaymsg");
        fs::write(&sway, format!(r#"#!/bin/sh
cd '{}' || exit 1
for argument do command="$argument"; done
case "$command" in
 get_outputs)
  mode=$(cat mode); width=${{mode%x*}}; height=${{mode#*x}}
  printf '[{{"name":"HEADLESS-1","active":true,"focused":true,"rect":{{"width":%s,"height":%s}},"current_mode":{{"width":%s,"height":%s,"refresh":60000}}}}]' "$width" "$height" "$width" "$height";;
 get_tree) printf '{{"nodes":[{{"id":10,"pid":4100,"app_id":"korri-shell","focused":true}}]}}';;
 *)
  printf '%s\n' "$command" >> commands
  case "$command" in
   *'mode --custom'*)
    printf 1576x1312 > mode
    touch shaped
    if test -f block-shape; then
      while ! test -f release-shape; do sleep 0.01; done
    fi;;
   *'mode 1920x1080@60Hz'*) printf 1920x1080 > mode;;
  esac
  printf '[{{"success":true}}]';;
esac
"#, root.path().display())).unwrap();
        fs::set_permissions(&sway, fs::Permissions::from_mode(0o700)).unwrap();
        std::env::set_var("KORRID_SWAYMSG", &sway);
        std::env::set_var(
            "KORRID_COMPOSITOR_CONTROL_SOCKET",
            root.path().join("socket"),
        );
        let mut facts = serde_json::json!([
            {"name":"HEADLESS-1","main":true,"unwatched":true,"mode":"1920x1080@60Hz","x":0,"y":0,"width":1920,"height":1080,"touch":null},
            {"name":"HEADLESS-2","main":false,"unwatched":false,"mode":"800x600@60Hz","x":1920,"y":0,"width":800,"height":600,"touch":null}
        ]);
        facts.as_array_mut().unwrap().truncate(displays);
        std::env::set_var("KORRID_DISPLAYS", facts.to_string());
        let config = root.path().join("host.toml");
        fs::write(&config, "label = 'shape-test'\n").unwrap();
        let backend = Arc::new(super::control::InMemoryLaunchUnitBackend::default());
        let runtime = HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.path().join("private"),
            backend.clone(),
        )
        .with_route_registry(root.path().into(), registry)
        .with_shell_unit(Arc::new(super::control::RecordingShellUnit::identity_only()));
        runtime.claim_save_owner();
        Self {
            root,
            runtime,
            backend,
        }
    }
    fn catalog(&self) -> Result<Vec<display_placement::SurfaceDisplay>, String> {
        catalog(&self.runtime)
    }
    fn mode(&self) -> String {
        fs::read_to_string(self.root.path().join("mode")).unwrap()
    }
}
fn catalog(runtime: &HostRuntime) -> Result<Vec<display_placement::SurfaceDisplay>, String> {
    runtime
        .control()
        .unwrap()
        .surface_displays(display_placement::SurfaceDisplaysRequest {
            displays: display_placement::SurfaceDisplayScope::Main,
            overlay: false,
            launch_id: None,
        })
}
fn prepare(runtime: &HostRuntime) -> Result<crate::game_routes::SelectedGameLaunch, RpcFailure> {
    runtime.prepare_blocking(
        crate::plugin_test_fixtures::GBA_ID,
        runtime.owner_public_key().as_deref(),
        Some(PluginDisplay {
            width: 2176,
            height: 1812,
        }),
    )
}

#[test]
fn catalog_during_prepare_waits_and_never_restores_a_running_game() {
    let name = "catalog_during_prepare_waits_and_never_restores_a_running_game";
    if isolated(name) {
        return;
    }
    // Cover both the former lock-free single-output path and cached multi-output path.
    for count in [1, 2] {
        let fixture = Fixture::new(count);
        fixture.catalog().unwrap();
        fs::write(fixture.root.path().join("block-shape"), "").unwrap();
        let runtime = fixture.runtime.clone();
        let preparing = thread::spawn(move || prepare(&runtime));
        let deadline = Instant::now() + Duration::from_secs(10);
        while !fixture.root.path().join("shaped").exists() {
            assert!(Instant::now() < deadline, "prepare never requested a shape");
            thread::sleep(Duration::from_millis(5));
        }
        let (entered, started) = mpsc::channel();
        let (sent, observed) = mpsc::channel();
        let runtime = fixture.runtime.clone();
        let placing = thread::spawn(move || {
            entered.send(()).unwrap();
            sent.send(catalog(&runtime)).unwrap();
        });
        started.recv().unwrap();
        let early = observed.recv_timeout(Duration::from_millis(100));
        fs::write(fixture.root.path().join("release-shape"), "").unwrap();
        preparing.join().unwrap().unwrap();
        placing.join().unwrap();
        assert!(
            early.is_err(),
            "catalog placement crossed the preparing transaction"
        );
        observed.recv().unwrap().unwrap();
        fixture.catalog().unwrap();
        assert_eq!(
            fixture.mode(),
            "1576x1312",
            "catalog must not restore a live game"
        );
    }
}

#[test]
fn failed_prepare_invalidates_catalog_receipt_before_the_output_changes() {
    let name = "failed_prepare_invalidates_catalog_receipt_before_the_output_changes";
    if isolated(name) {
        return;
    }
    let fixture = Fixture::new(2);
    fixture.catalog().unwrap();
    // Effect-free runtime checks now finish before shaping. Exercise a later
    // failure instead: the native unit exits before prepare can hand it back.
    fixture.backend.set_launch_completed(true);
    let failure = prepare(&fixture.runtime).unwrap_err();
    assert!(
        failure.message.contains("exited before prepare completed"),
        "{failure:?}"
    );
    assert_eq!(fixture.mode(), "1576x1312");
    fixture.catalog().unwrap();
    assert_eq!(
        fixture.mode(),
        "1920x1080",
        "a cached catalog receipt must not skip restoration after failed prepare"
    );
}

#[test]
fn catalog_observes_completed_unit_even_when_cached_session_is_stopping() {
    let name = "catalog_observes_completed_unit_even_when_cached_session_is_stopping";
    if isolated(name) {
        return;
    }
    for count in [1, 2] {
        let fixture = Fixture::new(count);
        let launched = prepare(&fixture.runtime).unwrap();
        fixture.backend.configure_stop(false, false, None);
        assert!(matches!(
            fixture
                .runtime
                .control()
                .unwrap()
                .stop(&launched.session.launch_id),
            HostSessionStop::AlreadyStopping { .. }
        ));
        assert_eq!(fixture.mode(), "1576x1312");
        fixture.backend.complete_live();
        // Do not poll status. Catalog placement must observe the native unit
        // itself before deciding whether it may restore the configured mode.
        fixture.catalog().unwrap();
        assert_eq!(fixture.mode(), "1920x1080");
    }
}

#[test]
fn limits_description_does_not_hold_native_session_authority() {
    let name = "limits_description_does_not_hold_native_session_authority";
    if isolated(name) {
        return;
    }
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
    };
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("limits.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::env::set_var("KORRID_LIMITS_SOCKET", &socket);
    let fixture = Fixture::new(1);
    let (describing, entered) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let helper = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&line).unwrap()["_tag"],
            "Describe"
        );
        describing.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(2)).unwrap();
        stream
            .write_all(b"{\"_tag\":\"Err\",\"payload\":\"no test limits\"}\n")
            .unwrap();
    });
    let runtime = fixture.runtime.clone();
    let preparing = thread::spawn(move || prepare(&runtime));
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    let (answer, received) = mpsc::channel();
    let runtime = fixture.runtime.clone();
    let observer = thread::spawn(move || {
        let control = runtime.control().unwrap();
        let status = control.control_status();
        let stop = control.stop("00000000000000000000000000000000");
        answer.send((status, stop)).unwrap();
    });
    let while_describing = received.recv_timeout(Duration::from_millis(250));
    release.send(()).unwrap();
    helper.join().unwrap();
    preparing.join().unwrap().unwrap();
    observer.join().unwrap();
    assert_eq!(
        while_describing.unwrap(),
        (HostSessionStatus::NoActive, HostSessionStop::NoActive)
    );
}

#[test]
fn runtime_refusal_precedes_viewer_output_effects() {
    let name = "runtime_refusal_precedes_viewer_output_effects";
    if isolated(name) {
        return;
    }
    let fixture = Fixture::new(2);
    fixture.catalog().unwrap();
    let path = fixture.root.path().join("plugin-mgba/retroarch.ts");
    let source = fs::read_to_string(&path).unwrap().replace("export const handlers = {", "export const handlers = {\n 'runtime.resolve': () => ({ ready: false, missing: ['runtime refused before shaping'] }),");
    fs::write(path, source).unwrap();
    assert!(prepare(&fixture.runtime)
        .unwrap_err()
        .message
        .contains("runtime refused before shaping"));
    assert!(!fixture.root.path().join("shaped").exists());
    assert_eq!(fixture.mode(), "1920x1080");
}
