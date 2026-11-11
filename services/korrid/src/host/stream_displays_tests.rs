use super::super::systemd_unit::InMemoryLaunchUnitBackend;
use super::*;
use serde_json::json;

const PERSON: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";

#[derive(Debug, Default)]
struct DisplayControl {
    commands: Mutex<Vec<String>>,
    deny_power_on: std::sync::atomic::AtomicBool,
}
impl CompositorControl for DisplayControl {
    fn focus(&self, _: i64) -> Result<(), String> {
        Ok(())
    }
    fn tree(&self) -> Result<String, String> {
        Ok("{}".into())
    }
    fn command(&self, command: &str) -> Result<(), String> {
        self.commands.lock().unwrap().push(command.into());
        if command.ends_with("power on")
            && self.deny_power_on.load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err("display power denied".into());
        }
        Ok(())
    }
}

#[test]
fn remote_launch_wakes_watched_output_before_sunshine_and_game() {
    const CHILD: &str = "KORRI_STREAM_DISPLAY_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "host::session_state::stream_displays_tests::remote_launch_wakes_watched_output_before_sunshine_and_game", "--nocapture"])
            .env(CHILD, "1")
            .env("KORRID_DISPLAYS", r#"[{"name":"DP-1","main":true,"unwatched":false,"mode":"1920x1080@60Hz","x":0,"y":0,"width":1920,"height":1080,"touch":null},{"name":"HDMI-A-1","main":false,"unwatched":false,"mode":"1920x1080@60Hz","x":1920,"y":0,"width":1920,"height":1080,"touch":null}]"#)
            .output().unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let compositor = Arc::new(DisplayControl::default());
    let backend = Arc::new(InMemoryLaunchUnitBackend::default());
    let observed = compositor.clone();
    backend.set_service_probe(Arc::new(move || {
        assert!(
            observed
                .commands
                .lock()
                .unwrap()
                .contains(&"output DP-1 power on".into()),
            "Sunshine must not probe a powered-off watched output"
        );
        Ok(())
    }));
    let observed = compositor.clone();
    backend.set_launch_probe(Arc::new(move || {
        assert!(observed
            .commands
            .lock()
            .unwrap()
            .contains(&"output DP-1 power on".into()));
    }));
    let control = HostSessionControl::new(root.path(), backend.clone())
        .with_compositor(compositor.clone())
        .for_starting_device(Some("33".repeat(32)));
    let command = vec![
        "korrid".into(), "plugin-launch".into(), "unused-source".into(),
        json!({"runnerId":"@test:runner/main","program":"game","accountRoot":root.path().join("users").join("f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9"),"saveRoot":root.path().join("saves"),"cacheRoot":root.path(),"files":{},"artifacts":{}}).to_string(),
        json!({"modifiers":[],"sunshineArgs":["max_bitrate=3000"],"placement":{"layout":null,"global":false,"width":1920,"height":1080,"outputs":["DP-1"]}}).to_string(),
        json!({"plan":{"command":"game","args":[]},"runnerArgv":["game"]}).to_string(),
    ];
    let session = control
        .prepare("game", Some(PERSON), Ok(&command), &BTreeMap::new())
        .unwrap();
    assert_eq!(backend.launch_count(), 1);
    compositor.commands.lock().unwrap().clear();
    control.idle_displays().unwrap();
    assert_eq!(
        *compositor.commands.lock().unwrap(),
        ["output HDMI-A-1 power off", "seat * idle_notify"]
    );
    compositor.commands.lock().unwrap().clear();
    control
        .prepare("game", Some(PERSON), Ok(&command), &BTreeMap::new())
        .unwrap();
    assert!(
        compositor
            .commands
            .lock()
            .unwrap()
            .contains(&"output DP-1 power on".into()),
        "resuming an existing remote launch must also wake it"
    );
    control.stop(&session.launch_id);
    compositor
        .deny_power_on
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        control
            .prepare("other", Some(PERSON), Ok(&command), &BTreeMap::new())
            .unwrap_err()
            .code,
        "PlacementFailed"
    );
    assert_eq!(
        backend.launch_count(),
        1,
        "a failed wake cannot start capture or a game"
    );
}

#[test]
fn stream_play_wakes_before_reply_and_idle_resumes_after_the_session_ends() {
    const CHILD: &str = "KORRI_STREAM_IDLE_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "host::session_state::stream_displays_tests::stream_play_wakes_before_reply_and_idle_resumes_after_the_session_ends", "--nocapture"])
            .env(CHILD, "1")
            .env("KORRID_DISPLAYS", r#"[{"name":"DP-1","main":true,"unwatched":false,"mode":"1920x1080@60Hz","x":0,"y":0,"width":1920,"height":1080,"touch":null},{"name":"HDMI-A-1","main":false,"unwatched":false,"mode":"1920x1080@60Hz","x":1920,"y":0,"width":1920,"height":1080,"touch":null},{"name":"VIRTUAL-1","main":false,"unwatched":true,"mode":"1920x1080@60Hz","x":3840,"y":0,"width":1920,"height":1080,"touch":null}]"#)
            .output().unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let compositor = Arc::new(DisplayControl::default());
    let backend = Arc::new(InMemoryLaunchUnitBackend::default());
    let control =
        HostSessionControl::new(root.path(), backend.clone()).with_compositor(compositor.clone());
    let session = control
        .prepare("game", None, Ok(&["game".into()]), &BTreeMap::new())
        .unwrap();
    control.idle_displays().unwrap();
    assert_eq!(
        *compositor.commands.lock().unwrap(),
        ["output DP-1 power off", "output HDMI-A-1 power off"]
    );
    compositor.commands.lock().unwrap().clear();
    let player = crate::stream_play::StreamPlayer {
        device_public_key: "33".repeat(32),
        label: "Phone".into(),
    };
    assert!(control.stream_play("wrong", player.clone()).is_err());
    assert!(compositor.commands.lock().unwrap().is_empty());
    control.stream_play(&session.launch_id, player).unwrap();
    assert!(compositor
        .commands
        .lock()
        .unwrap()
        .contains(&"output DP-1 power on".into()));
    compositor.commands.lock().unwrap().clear();
    // A recovered korrid uses the journal, not an in-memory inhibitor flag.
    let recovered =
        HostSessionControl::new(root.path(), backend).with_compositor(compositor.clone());
    recovered.idle_displays().unwrap();
    assert_eq!(
        *compositor.commands.lock().unwrap(),
        ["output HDMI-A-1 power off", "seat * idle_notify"]
    );
    // The normal session watcher resolves startup recovery before stop. This
    // fixture has no mapped game window/PID, so recovery must fail focus closed;
    // stream power protection does not depend on successful focus recovery.
    assert!(matches!(
        recovered.status(),
        HostSessionStatus::FocusFailed { .. }
    ));
    assert!(matches!(
        recovered.stop(&session.launch_id),
        HostSessionStop::Completed { .. }
    ));
    compositor.commands.lock().unwrap().clear();
    recovered.idle_displays().unwrap();
    assert_eq!(
        *compositor.commands.lock().unwrap(),
        ["output DP-1 power off", "output HDMI-A-1 power off"]
    );
    // Exercise the typed native RPC over a real loopback connection as well.
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"holder\"\n[[games]]\nid = \"game\"\ntitle = \"Game\"\ncommand = [\"game\"]\n").unwrap();
        let runtime = super::super::HostRuntime::from_paths_with_backend(
            &config, None, root.path().join("rpc-private"), Arc::new(InMemoryLaunchUnitBackend::default()),
        ).with_compositor(compositor.clone());
        let (_, app) = crate::plain_host_routers_for_tests(runtime);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
        compositor.commands.lock().unwrap().clear();
        let response = reqwest::Client::new().post(format!("http://{address}/rpc"))
            .json(&json!({"_tag":"app.displays.idle","payload":{}})).send().await.unwrap();
        assert!(response.status().is_success());
        let reply: serde_json::Value = response.json().await.unwrap();
        assert_eq!(reply, json!({"_tag":"app.displays.idle","outcome":{"_tag":"Ok"}}));
        assert_eq!(*compositor.commands.lock().unwrap(), ["output DP-1 power off", "output HDMI-A-1 power off"]);
        server.abort();
    });
}
