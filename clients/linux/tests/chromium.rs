use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::{fs::PermissionsExt, net::UnixDatagram},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CAPABILITY: &str = "private-test-capability-not-for-command-lines";

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("korri-shell-test-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        // The shell must receive TERM so it can retire its Chromium group.
        unsafe {
            libc::kill(self.0.id() as libc::pid_t, libc::SIGTERM);
        }
        let _ = self.0.wait();
    }
}

fn request(stream: &mut TcpStream) -> Option<String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut result = Vec::new();
    let mut byte = [0];
    while !result.ends_with(b"\r\n\r\n") {
        assert!(result.len() < 65536);
        // Chromium can abandon a speculative connection before an HTTP request.
        stream.read_exact(&mut byte).ok()?;
        result.push(byte[0]);
    }
    String::from_utf8(result).ok()
}
fn respond(stream: &mut TcpStream, body: &str) {
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
}

#[test]
fn invalid_configuration_does_not_print_credentials() {
    let output = Command::new(env!("CARGO_BIN_EXE_korri-portal-shell"))
        .env_remove("CREDENTIALS_DIRECTORY")
        .env("KORRID_RPC_CAPABILITY", CAPABILITY)
        .arg("/nonexistent/chromium")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains(CAPABILITY));
    assert!(output.stdout.is_empty());
}

#[test]
fn group_readable_credentials_are_rejected_before_chromium_starts() {
    let root = Directory::new();
    let credential = root.0.join("KORRID_RPC_CAPABILITY");
    fs::write(&credential, CAPABILITY).unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o640)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_korri-portal-shell"))
        .env("CREDENTIALS_DIRECTORY", &root.0)
        .arg("/nonexistent/chromium")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("private regular file"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(CAPABILITY));
}

#[test]
fn closed_browser_pipe_stops_the_shell_without_disclosing_credentials() {
    let root = Directory::new();
    let credential = root.0.join("KORRID_RPC_CAPABILITY");
    fs::write(&credential, CAPABILITY).unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    // The real test executable exits after listing its tests. It closes the
    // inherited pipe without producing a browser response.
    let output = Command::new(env!("CARGO_BIN_EXE_korri-portal-shell"))
        .env("CREDENTIALS_DIRECTORY", &root.0)
        .env("KORRID_ADDRESS", "0.0.0.0:39217")
        .env("KORRID_PORTAL_ORIGIN", "http://127.0.0.1:8099")
        .env("KORRI_WEB_SURFACE_URL", "http://127.0.0.1:8099/")
        .arg(std::env::current_exe().unwrap())
        .arg("--list")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains(CAPABILITY));
}

#[test]
#[ignore = "requires KORRI_TEST_CHROMIUM pointing to an actual Chromium executable"]
fn actual_chromium_receives_credentials_only_on_the_trusted_origin() {
    let chromium = std::env::var("KORRI_TEST_CHROMIUM").expect("set KORRI_TEST_CHROMIUM");
    let root = Directory::new();
    let notify_path = root.0.join("notify.sock");
    let notify = UnixDatagram::bind(&notify_path).unwrap();
    notify
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let credential = root.0.join("KORRID_RPC_CAPABILITY");
    fs::write(&credential, format!("{CAPABILITY}\n")).unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    let trusted = TcpListener::bind("127.0.0.1:0").unwrap();
    let untrusted = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", trusted.local_addr().unwrap());
    let untrusted_origin = format!("http://{}", untrusted.local_addr().unwrap());
    let (reports, received) = mpsc::channel();
    let trusted_report = reports.clone();
    let (allow_navigation, navigation_allowed) = mpsc::channel();
    thread::spawn(move || {
        for stream in trusted.incoming() {
            let mut stream = stream.unwrap();
            let Some(headers) = request(&mut stream) else {
                continue;
            };
            if headers.starts_with("GET /report ") {
                trusted_report.send(headers).unwrap();
                navigation_allowed
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap();
                respond(&mut stream, "ok");
                break;
            }
            let page = format!(
                r#"<!doctype html><script>
const descriptor = Object.getOwnPropertyDescriptor(window, 'KorriRpc');
const frozen = Object.isFrozen(window.KorriRpc);
fetch('/report', {{headers: {{
 Authorization: 'Bearer ' + window.KorriRpc.korridCapability(),
 'X-Korri-Port': String(window.KorriRpc.korridPort()),
 'X-Binding-Private': String(!descriptor.enumerable && !descriptor.writable && !descriptor.configurable && frozen),
 'X-Storage-Empty': String(localStorage.length === 0 && sessionStorage.length === 0)
}}}}).then(() => {{ location.href = '{untrusted_origin}/'; }});
</script>"#
            );
            respond(&mut stream, &page);
        }
    });
    thread::spawn(move || {
        for stream in untrusted.incoming() {
            let mut stream = stream.unwrap();
            let Some(headers) = request(&mut stream) else {
                continue;
            };
            if headers.starts_with("GET /report ") {
                reports.send(headers).unwrap();
                respond(&mut stream, "ok");
                break;
            }
            respond(&mut stream, "<!doctype html><script>fetch('/report', {headers: {'X-Binding-Type': typeof window.KorriRpc}})</script>");
        }
    });
    let child = Command::new(env!("CARGO_BIN_EXE_korri-portal-shell"))
        .env("CREDENTIALS_DIRECTORY", &root.0)
        .env("KORRID_ADDRESS", "0.0.0.0:39217")
        .env("KORRID_PORTAL_ORIGIN", &origin)
        .env("KORRI_WEB_SURFACE_URL", format!("{origin}/"))
        .env("KORRID_RPC_CAPABILITY", "must-not-reach-chromium")
        .env("NOTIFY_SOCKET", &notify_path)
        .arg(chromium)
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--disable-extensions",
        ])
        .arg(format!(
            "--user-data-dir={}",
            root.0.join("profile").display()
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut child = Process(child);
    let deadline = Instant::now() + Duration::from_secs(30);
    let children_file = format!("/proc/{0}/task/{0}/children", child.0.id());
    let chromium_pid = loop {
        let children = fs::read_to_string(&children_file).unwrap_or_default();
        if let Some(pid) = children.split_whitespace().next() {
            break pid.to_string();
        }
        if let Some(exit) = child.0.try_wait().unwrap() {
            let mut error = String::new();
            child
                .0
                .stderr
                .take()
                .unwrap()
                .read_to_string(&mut error)
                .unwrap();
            panic!("shell exited {exit}: {error}");
        }
        assert!(Instant::now() < deadline, "Chromium did not start");
        thread::sleep(Duration::from_millis(20));
    };
    let command = fs::read(format!("/proc/{chromium_pid}/cmdline")).unwrap();
    let environment = fs::read(format!("/proc/{chromium_pid}/environ")).unwrap();
    assert!(!String::from_utf8_lossy(&command).contains(CAPABILITY));
    assert!(!String::from_utf8_lossy(&command).contains("--remote-debugging-port"));
    assert!(!String::from_utf8_lossy(&environment).contains("KORRID_RPC_CAPABILITY="));
    assert!(!String::from_utf8_lossy(&environment).contains("CREDENTIALS_DIRECTORY="));
    assert!(!String::from_utf8_lossy(&environment).contains("NOTIFY_SOCKET="));
    let mut notification = [0; 128];
    let count = notify
        .recv(&mut notification)
        .expect("portal readiness notification");
    assert_eq!(&notification[..count], b"READY=1");
    let first = received
        .recv_timeout(Duration::from_secs(30))
        .expect("trusted page report")
        .to_ascii_lowercase();
    assert!(first.contains(&format!(
        "authorization: bearer {}",
        CAPABILITY.to_ascii_lowercase()
    )));
    assert!(first.contains("x-korri-port: 39217"));
    assert!(first.contains("x-binding-private: true"));
    assert!(first.contains("x-storage-empty: true"));
    allow_navigation.send(()).unwrap();
    let second = received
        .recv_timeout(Duration::from_secs(15))
        .expect("untrusted page report")
        .to_ascii_lowercase();
    assert!(second.contains("x-binding-type: undefined"));
    assert!(!second.contains(CAPABILITY));
    assert!(
        child.0.try_wait().unwrap().is_none(),
        "shell must supervise Chromium"
    );
    drop(child);
    assert!(
        !PathBuf::from(format!("/proc/{chromium_pid}")).exists(),
        "Chromium was not reaped"
    );
}

struct PageServer {
    address: std::net::SocketAddr,
    stopped: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    reports: mpsc::Receiver<String>,
}
impl PageServer {
    fn new(page: String, child_page: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = stopped.clone();
        let (sent, reports) = mpsc::channel();
        let thread = thread::spawn(move || {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let Some(headers) = request(&mut stream) else {
                            continue;
                        };
                        if headers.starts_with("GET /attempt ") {
                            sent.send(headers).unwrap();
                            respond(&mut stream, "ok");
                        } else if headers.starts_with("GET /child ") {
                            respond(&mut stream, &child_page);
                        } else {
                            respond(&mut stream, &page);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("HTTP acceptance server failed: {error}"),
                }
            }
        });
        Self {
            address,
            stopped,
            thread: Some(thread),
            reports,
        }
    }
}
impl Drop for PageServer {
    fn drop(&mut self) {
        self.stopped
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = self.thread.take().unwrap().join();
    }
}

fn assert_not_ready(page: &str, child: &str, external_report: Option<&PageServer>) {
    let chromium = std::env::var("KORRI_TEST_CHROMIUM").expect("set KORRI_TEST_CHROMIUM");
    let root = Directory::new();
    let credential = root.0.join("KORRID_RPC_CAPABILITY");
    fs::write(&credential, CAPABILITY).unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    let server = PageServer::new(page.into(), child.into());
    let origin = format!("http://{}", server.address);
    let notify_path = root.0.join("notify.sock");
    let notify = UnixDatagram::bind(&notify_path).unwrap();
    notify.set_nonblocking(true).unwrap();
    let mut child = Process(
        Command::new(env!("CARGO_BIN_EXE_korri-portal-shell"))
            .env("CREDENTIALS_DIRECTORY", &root.0)
            .env("KORRID_ADDRESS", "0.0.0.0:39217")
            .env("KORRID_PORTAL_ORIGIN", &origin)
            .env("KORRI_WEB_SURFACE_URL", format!("{origin}/"))
            .env("NOTIFY_SOCKET", &notify_path)
            .arg(chromium)
            .args([
                "--headless=new",
                "--disable-gpu",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-background-networking",
                "--disable-extensions",
            ])
            .arg(format!(
                "--user-data-dir={}",
                root.0.join("profile").display()
            ))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let report = external_report
        .unwrap_or(&server)
        .reports
        .recv_timeout(Duration::from_secs(10))
        .expect("the real browser must execute the non-consuming test page");
    assert!(
        report
            .to_ascii_lowercase()
            .contains("x-attempt-executed: true"),
        "{report}"
    );
    let children_file = format!("/proc/{0}/task/{0}/children", child.0.id());
    let browser_pid = fs::read_to_string(children_file)
        .unwrap()
        .split_whitespace()
        .next()
        .expect("Chromium must exist while the readiness deadline is pending")
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(25);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "the shell did not enforce its readiness timeout"
        );
        thread::sleep(Duration::from_millis(20));
    };
    assert!(
        !status.success(),
        "a fixture or spoofed readiness must fail startup"
    );
    let mut message = [0; 128];
    assert_eq!(
        notify.recv(&mut message).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let mut stdout = String::new();
    child
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    assert!(stdout.is_empty());
    let mut stderr = String::new();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(
        stderr.contains("the portal did not consume its RPC binding"),
        "{stderr}"
    );
    assert!(!stderr.contains(CAPABILITY));
    assert!(
        !PathBuf::from(format!("/proc/{browser_pid}")).exists(),
        "Chromium was not reaped"
    );
}

#[test]
#[ignore = "requires KORRI_TEST_CHROMIUM; exercises the actual 15-second readiness deadline"]
fn old_fixture_bundle_never_reports_ready() {
    assert_not_ready(
        r#"<!doctype html><h1>Fixture</h1><script>
fetch('/attempt', {headers: {'X-Attempt-Executed': 'true'}});
</script>"#,
        "",
        None,
    );
}

#[test]
#[ignore = "requires KORRI_TEST_CHROMIUM; exercises the actual 15-second readiness deadline"]
fn reading_only_the_port_never_reports_ready() {
    assert_not_ready(
        r#"<!doctype html><script>
const port = window.KorriRpc.korridPort();
fetch('/attempt', {headers: {'X-Attempt-Executed': String(port === 39217)}});
</script>"#,
        "",
        None,
    );
}

const SPOOF_READINESS: &str = r#"<!doctype html><script>
const present = typeof window.__korriRpcConsumed === 'function';
if (present) window.__korriRpcConsumed('');
fetch('/attempt', {headers: {'X-Attempt-Executed': String(present)}});
</script>"#;

#[test]
#[ignore = "requires KORRI_TEST_CHROMIUM; exercises the actual 15-second readiness deadline"]
fn a_child_frame_cannot_report_ready() {
    assert_not_ready(
        "<!doctype html><iframe src='/child'></iframe>",
        SPOOF_READINESS,
        None,
    );
}

#[test]
#[ignore = "requires KORRI_TEST_CHROMIUM; exercises the actual 15-second readiness deadline"]
fn an_untrusted_origin_cannot_report_ready() {
    let untrusted = PageServer::new(SPOOF_READINESS.into(), String::new());
    let redirect = format!(
        "<!doctype html><script>location.replace('http://{}/');</script>",
        untrusted.address
    );
    assert_not_ready(&redirect, "", Some(&untrusted));
}
