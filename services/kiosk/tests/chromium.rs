//! Opt-in real Chromium gate. All HTTP responses are static test fixtures;
//! runtime.json is always 404 on the server, including for direct local callers.
use std::fs::DirBuilder;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::DirBuilderExt;
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

const PAGE: &str = r#"<!doctype html><script>
(async () => {
  const config = await (await fetch('/runtime.json')).json();
  if (config.korridPort !== 45231 || config.korridCapability !== 'a'.repeat(64) || config.surfaceId !== 'pico') throw Error('initial');
  const altered = await fetch('/runtime.json?probe');
  if (altered.status !== 404) throw Error('exact url');
  await fetch('/replace');
  const refreshed = await (await fetch('/runtime.json')).json();
  if (refreshed.korridPort !== 45232 || refreshed.korridCapability !== 'b'.repeat(64)) throw Error('refresh');
  await new Promise((resolve, reject) => {
    const frame = document.createElement('iframe');
    window.addEventListener('message', event => {
      if (event.source !== frame.contentWindow || event.origin !== location.origin) return;
      event.data === 'blocked' ? resolve() : reject(Error('subframe'));
    });
    frame.src = '/child'; document.body.append(frame);
  });
  await fetch('/passed');
  location.href = 'http://127.0.0.1:8100/attack';
})().catch(() => fetch('/failed'));
</script><body></body>"#;
const CHILD: &str = r#"<!doctype html><script>
fetch('/runtime.json').then(r => r.json()).then(() => parent.postMessage('leaked', location.origin))
.catch(() => parent.postMessage('blocked', location.origin));
</script>"#;
const ATTACK: &str = r#"<!doctype html><script>
fetch('http://127.0.0.1:8099/runtime.json').then(r => r.json())
.then(() => fetch('/failed')).catch(() => fetch('/done'));
</script>"#;

#[test]
#[ignore = "requires KORRI_TEST_CHROMIUM, a sandbox-capable user, and free loopback ports 8099 and 8100"]
fn chromium_pipe_delivers_privately_refreshes_and_refuses_untrusted_frames() {
    let chromium = std::env::var_os("KORRI_TEST_CHROMIUM")
        .expect("set KORRI_TEST_CHROMIUM to the Chromium executable");
    let root = TestDirectory(
        std::env::temp_dir().join(format!("korri-kiosk-chromium-test-{}", std::process::id())),
    );
    DirBuilder::new().mode(0o700).create(&root.0).unwrap();
    let profile = root.0.join("profiles");
    DirBuilder::new().mode(0o700).create(&profile).unwrap();
    let brain = root.0.join("brain.json");
    std::fs::write(&brain, payload(45231, 'a')).unwrap();
    let portal = TcpListener::bind("127.0.0.1:8099").unwrap();
    let attacker = TcpListener::bind("127.0.0.1:8100").unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let public_runtime_requests = Arc::new(AtomicUsize::new(0));
    let (reports, receive) = mpsc::channel();
    let mut workers = Vec::new();
    for listener in [portal, attacker] {
        listener.set_nonblocking(true).unwrap();
        let stop = stop.clone();
        let requests = public_runtime_requests.clone();
        let reports = reports.clone();
        let brain = brain.clone();
        workers.push(std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => { std::thread::sleep(Duration::from_millis(5)); continue; },
                    Err(e) => panic!("fixture accept failed: {e}"),
                };
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while request.len() < 16384 && !request.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap_or(0) == 0 { break; }
                    request.push(byte[0]);
                }
                let request = String::from_utf8_lossy(&request);
                let path = request.split_whitespace().nth(1).unwrap_or("");
                let (status, body) = match path {
                    "/" => (200, PAGE),
                    "/child" => (200, CHILD),
                    "/attack" => (200, ATTACK),
                    "/runtime.json" => { requests.fetch_add(1, Ordering::SeqCst); (404, "not found") },
                    "/replace" => {
                        let next = brain.with_extension("next");
                        std::fs::write(&next, payload(45232, 'b')).unwrap();
                        std::fs::rename(next, &brain).unwrap();
                        (200, "replaced")
                    },
                    "/passed" | "/failed" | "/done" => { reports.send(path.to_string()).unwrap(); (200, "reported") },
                    _ => (404, "not found"),
                };
                let response = format!("HTTP/1.1 {status} OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = stream.write_all(response.as_bytes());
            }
        }));
    }
    assert!(direct_request().starts_with("HTTP/1.1 404"));
    let child = Command::new(env!("CARGO_BIN_EXE_korri-kiosk"))
        .arg("--chromium")
        .arg(chromium)
        .arg("--profile-parent")
        .arg(&profile)
        .arg("--brain-file")
        .arg(&brain)
        .args(["--surface-id", "pico", "--headless"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut kiosk = Kiosk(child);
    let first = receive.recv_timeout(Duration::from_secs(30));
    let second = if first.as_deref() == Ok("/passed") {
        receive.recv_timeout(Duration::from_secs(10))
    } else {
        Err(mpsc::RecvTimeoutError::Timeout)
    };
    assert!(direct_request().starts_with("HTTP/1.1 404"));
    kiosk.terminate();
    let status = kiosk.0.wait().unwrap();
    let mut stdout = String::new();
    let mut stderr = String::new();
    kiosk
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    kiosk
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    stop.store(true, Ordering::Relaxed);
    for worker in workers {
        worker.join().unwrap();
    }
    assert!(!stdout.contains(&"a".repeat(64)) && !stderr.contains(&"a".repeat(64)));
    assert!(!stdout.contains(&"b".repeat(64)) && !stderr.contains(&"b".repeat(64)));
    assert_eq!(
        first.as_deref(),
        Ok("/passed"),
        "Chromium fixture failed; launcher diagnostics: {stderr}"
    );
    assert_eq!(second.as_deref(), Ok("/done"));
    assert!(status.success(), "launcher diagnostics: {stderr}");
    assert!(stdout.is_empty() && stderr.is_empty());
    assert_eq!(
        public_runtime_requests.load(Ordering::SeqCst),
        2,
        "only the two unauthenticated probes reached HTTP"
    );
    assert_eq!(
        std::fs::read_dir(profile).unwrap().count(),
        0,
        "profile removed on SIGTERM"
    );
}

fn payload(port: u16, character: char) -> String {
    serde_json::json!({"korridPort":port,"korridCapability":character.to_string().repeat(64)})
        .to_string()
}
fn direct_request() -> String {
    let mut stream = TcpStream::connect("127.0.0.1:8099").unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .write_all(
            b"GET /runtime.json HTTP/1.1\r\nHost: 127.0.0.1:8099\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}
struct TestDirectory(std::path::PathBuf);
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Kiosk(Child);
impl Kiosk {
    fn terminate(&mut self) {
        if self.0.try_wait().unwrap().is_some() {
            return;
        }
        unsafe {
            libc::kill(self.0.id() as i32, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if self.0.try_wait().unwrap().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Drop for Kiosk {
    fn drop(&mut self) {
        self.terminate();
    }
}
