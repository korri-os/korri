use super::*;

fn arguments(flags: &[&str]) -> Result<(std::ffi::OsString, Vec<std::ffi::OsString>)> {
    chromium_arguments(
        std::iter::once("chromium")
            .chain(flags.iter().copied())
            .map(std::ffi::OsString::from),
    )
}

#[test]
fn shell_owns_one_inert_app_without_a_regular_tab_or_early_navigation() {
    let (executable, flags) = arguments(&["--kiosk", "--ozone-platform=wayland"]).unwrap();
    assert_eq!(executable, "chromium");
    assert_eq!(
        flags,
        [
            "--kiosk",
            "--ozone-platform=wayland",
            "--remote-debugging-pipe",
            "--disable-breakpad",
            "--disable-crash-reporter",
            "--app=data:text/html,%3Ctitle%3EKorri%3C/title%3E",
        ]
        .map(std::ffi::OsString::from)
    );
}

#[test]
fn callers_cannot_replace_the_app_navigation_or_private_debugging_pipe() {
    for flag in [
        "--app=about:blank",
        "--app=http://127.0.0.1:8099/",
        "--app=data:text/html,%3Ctitle%3EKorri%3C/title%3E",
        "data:text/html,%3Cscript%3Ealert(1)%3C/script%3E",
        "--app-id=other",
        "about:blank",
        "http://127.0.0.1:8099/",
        "--remote-debugging-port=9222",
        "--remote-debugging-pipe",
        "--restore-last-session",
        "--no-sandbox",
    ] {
        assert!(arguments(&[flag]).is_err(), "{flag}");
    }
}

#[test]
fn inert_app_receives_the_bridge_before_trusted_navigation_and_consumption() {
    let (input, mut responses) = UnixStream::pair().unwrap();
    let (output, requests) = UnixStream::pair().unwrap();
    let server = std::thread::spawn(move || {
        use std::io::BufRead;
        let mut requests = std::io::BufReader::new(requests);
        for (method, result) in [
            (
                "Target.getTargets",
                json!({"targetInfos": [{"type": "page", "url": BOOTSTRAP_URL, "targetId": "app"}]}),
            ),
            ("Target.attachToTarget", json!({"sessionId": "session"})),
            ("Page.enable", json!({})),
            (
                "Page.getFrameTree",
                json!({"frameTree": {"frame": {"id": "frame"}}}),
            ),
            ("Runtime.enable", json!({})),
            ("Runtime.addBinding", json!({})),
            ("Page.addScriptToEvaluateOnNewDocument", json!({})),
            ("Page.navigate", json!({})),
        ] {
            let mut bytes = Vec::new();
            requests.read_until(0, &mut bytes).unwrap();
            let request: Value = serde_json::from_slice(&bytes[..bytes.len() - 1]).unwrap();
            assert_eq!(request["method"], method);
            match method {
                "Target.attachToTarget" => {
                    assert_eq!(
                        request["params"],
                        json!({"targetId": "app", "flatten": true})
                    );
                }
                "Runtime.addBinding" => {
                    assert_eq!(request["params"]["name"], CONSUMED_BINDING);
                }
                "Page.addScriptToEvaluateOnNewDocument" => {
                    assert_eq!(
                        request["params"]["source"],
                        configuration().binding_script()
                    );
                }
                "Page.navigate" => {
                    assert_eq!(request["params"]["url"], configuration().url);
                }
                _ => {}
            }
            if !method.starts_with("Target.") {
                assert_eq!(request["sessionId"], "session");
            }
            let response = json!({"id": request["id"], "result": result});
            write!(responses, "{response}\0").unwrap();
        }
        for event in [
            json!({"method": "Runtime.executionContextCreated", "sessionId": "session", "params": {"context": {"id": 1, "origin": configuration().origin, "auxData": {"isDefault": true, "frameId": "frame"}}}}),
            json!({"method": "Runtime.bindingCalled", "sessionId": "session", "params": {"name": CONSUMED_BINDING, "payload": "", "executionContextId": 1}}),
        ] {
            write!(responses, "{event}\0").unwrap();
        }
    });
    let mut protocol = Protocol {
        input,
        output,
        pending: Vec::new(),
        sequence: 0,
        readiness: None,
    };
    bootstrap_portal(&mut protocol, &configuration()).unwrap();
    assert!(protocol.readiness.unwrap().consumed.is_some());
    server.join().unwrap();
}

#[test]
fn unexpected_initial_pages_fail_without_creating_or_attaching_a_regular_tab() {
    for targets in [
        json!([]),
        json!([{"type": "page", "url": "about:blank", "targetId": "regular"}]),
        json!([{"type": "page", "url": "http://127.0.0.1:8099/", "targetId": "early"}]),
        json!([{"type": "page", "url": format!("{BOOTSTRAP_URL}#other"), "targetId": "other"}]),
        json!([{"type": "page", "url": BOOTSTRAP_URL, "targetId": "app"}, {"type": "page", "url": "about:blank", "targetId": "regular"}]),
        json!([{"type": "page", "url": BOOTSTRAP_URL, "targetId": "app"}, {"type": "page", "url": BOOTSTRAP_URL, "targetId": "duplicate"}]),
    ] {
        let (input, mut responses) = UnixStream::pair().unwrap();
        let (output, mut requests) = UnixStream::pair().unwrap();
        write!(
            responses,
            "{}\0",
            json!({"id": 1, "result": {"targetInfos": targets}})
        )
        .unwrap();
        let mut protocol = Protocol {
            input,
            output,
            pending: Vec::new(),
            sequence: 0,
            readiness: None,
        };
        assert_eq!(
            bootstrap_portal(&mut protocol, &configuration()),
            Err("Chromium must start exactly one inert app page")
        );
        drop(protocol);
        let mut commands = String::new();
        requests.read_to_string(&mut commands).unwrap();
        let commands: Vec<Value> = commands
            .split('\0')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0]["method"], "Target.getTargets");
    }
}

fn app_window(pid: u32) -> String {
    // X11 app identity differs from regular Chromium's profile-based WM_CLASS.
    // Check the real window, not argv or CDP's generic page target type.
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    loop {
        let tree = Command::new("xwininfo")
            .args(["-root", "-tree"])
            .output()
            .expect("install xwininfo");
        assert!(tree.status.success());
        let tree = String::from_utf8(tree.stdout).unwrap();
        let mut owned = Vec::new();
        for line in tree
            .lines()
            .filter(|line| line.contains("\"Chromium-browser\""))
        {
            let id = line.split_whitespace().next().unwrap();
            let properties = Command::new("xprop")
                .args(["-id", id, "_NET_WM_PID", "WM_CLASS"])
                .output()
                .expect("install xprop");
            assert!(properties.status.success());
            let properties = String::from_utf8(properties.stdout).unwrap();
            if properties
                .lines()
                .any(|line| line == format!("_NET_WM_PID(CARDINAL) = {pid}"))
            {
                assert!(properties.contains("WM_CLASS(STRING) = \"text_html,%3Ctitle%3EKorri%3C_title%3E\", \"Chromium-browser\""), "not the inert app window: {properties}");
                owned.push(id.to_owned());
            }
        }
        if !owned.is_empty() {
            assert_eq!(owned.len(), 1, "expected one app window: {tree}");
            return owned.remove(0);
        }
        assert!(
            Instant::now() < deadline,
            "Chromium app window did not appear: {tree}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn configuration() -> Configuration {
    Configuration {
        capability: "test-capability".into(),
        port: 39217,
        origin: "http://127.0.0.1:8099".into(),
        url: "http://127.0.0.1:8099/".into(),
    }
}

#[test]
#[ignore = "requires KORRI_TEST_CHROMIUM, DISPLAY (for example Xvfb), xwininfo and xprop; headless cannot prove app presentation"]
fn actual_chromium_navigates_the_only_inert_app_without_creating_a_regular_tab() {
    use std::net::TcpListener;
    let chromium = env::var_os("KORRI_TEST_CHROMIUM").expect("set KORRI_TEST_CHROMIUM");
    let profile = env::temp_dir().join(format!("korri-app-target-test-{}", std::process::id()));
    fs::create_dir(&profile).unwrap();
    struct Profile(std::path::PathBuf);
    impl Drop for Profile {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let profile = Profile(profile);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let config = Configuration {
        origin: origin.clone(),
        url: format!("{origin}/"),
        ..configuration()
    };
    let (requested, requests) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + COMMAND_TIMEOUT;
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_read_timeout(Some(COMMAND_TIMEOUT)).unwrap();
                    let mut headers = Vec::new();
                    let mut byte = [0];
                    while !headers.ends_with(b"\r\n\r\n") {
                        if stream.read_exact(&mut byte).is_err() {
                            break;
                        }
                        headers.push(byte[0]);
                    }
                    if headers.is_empty() {
                        continue;
                    }
                    requested.send(()).unwrap();
                    let body = "<!doctype html><title>Korri portal test</title><script>window.KorriRpc.korridPort(); window.KorriRpc.korridCapability();</script>";
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "portal was not requested");
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("{error}"),
            }
        }
    });
    let (_, flags) = arguments(&[
        "--ozone-platform=x11",
        "--kiosk",
        "--disable-gpu",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--disable-extensions",
        // Keep CDP's complete target list deterministic, including non-page targets.
        "--disable-component-extensions-with-background-pages",
        &format!("--user-data-dir={}", profile.0.display()),
    ])
    .unwrap();
    let (browser, mut protocol) = start_chromium(chromium, flags).unwrap();
    fn targets(protocol: &mut Protocol) -> Vec<Value> {
        protocol
            .command("Target.getTargets", json!({}), None)
            .unwrap()["targetInfos"]
            .as_array()
            .unwrap()
            .to_vec()
    }
    let before = targets(&mut protocol);
    assert_eq!(before.len(), 1, "startup targets: {before:?}");
    assert_eq!(before[0]["type"], "page");
    assert_eq!(before[0]["url"], BOOTSTRAP_URL);
    let window = app_window(browser.0.id());
    assert!(matches!(
        requests.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    bootstrap_portal(&mut protocol, &config).unwrap();
    let after = targets(&mut protocol);
    assert_eq!(
        after.len(),
        1,
        "bootstrap must not create a regular tab: before={before:?}, after={after:?}"
    );
    assert_eq!(
        app_window(browser.0.id()),
        window,
        "navigation must retain app presentation in the same window"
    );
    assert_eq!(after[0]["targetId"], before[0]["targetId"]);
    assert_eq!(after[0]["url"], config.url);
    assert!(protocol.readiness.as_ref().unwrap().consumed.is_some());
    server.join().unwrap();
    assert_eq!(
        requests.try_iter().count(),
        1,
        "only controlled portal navigation may request the portal"
    );
}
