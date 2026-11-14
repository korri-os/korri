// SPDX-License-Identifier: GPL-3.0-or-later
use korri_viewer::lifecycle::Lifecycle;
use korri_viewer_treaty::{
    ViewerCommand as C, ViewerDisplay, ViewerEvent as E, ViewerMessage as M, ViewerReply as R,
    ViewerRequest, ViewerStatus,
};
use korri_viewer_treaty::{read_message, write_message};
use std::{
    io::{BufReader, Read, Write},
    net::TcpListener,
    os::unix::net::UnixStream,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

mod support;

fn request(socket: &mut BufReader<UnixStream>, launch: &str, command: C) -> R {
    write_message(
        socket.get_mut(),
        &ViewerRequest {
            launch_id: launch.into(),
            command,
        },
    )
    .unwrap();
    loop {
        match read_message(socket).unwrap().unwrap() {
            M::Reply(reply) => return reply,
            M::Event(_) => (),
        }
    }
}

#[test]
fn exact_launch_connected_events_and_confirmed_stop_over_real_socket() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let lifecycle = Lifecycle::bind(&path, "launch-a", unsafe { libc::geteuid() }).unwrap();
    let mut socket = BufReader::new(UnixStream::connect(&path).unwrap());
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    assert!(matches!(
        request(&mut socket, "launch-b", C::Status),
        R::Error { .. }
    ));
    let runtime = lifecycle.clone();
    let running = std::thread::spawn(move || {
        let start = runtime.wait_start().unwrap().expect("Start came first");
        assert_eq!(start.address, "127.0.0.1");
        runtime
            .connected(ViewerDisplay {
                width: 1280,
                height: 720,
            })
            .unwrap();
        while !runtime.stop_requested() {
            std::thread::sleep(Duration::from_millis(1));
        }
        runtime.stopped();
    });
    assert!(matches!(
        request(
            &mut socket,
            "launch-a",
            C::Start {
                address: "127.0.0.1".into(),
                display: Some(ViewerDisplay {
                    width: 1920,
                    height: 1080
                })
            }
        ),
        R::Ok
    ));
    assert!(
        matches!(request(&mut socket, "launch-a", C::Snapshot), R::Snapshot { snapshot: Some(s) } if s.connected == Some(true) && s.shape.as_ref().unwrap().encoded.width == 1280)
    );
    assert!(matches!(
        request(&mut socket, "launch-a", C::Background),
        R::Ok
    ));
    assert!(matches!(
        request(&mut socket, "launch-a", C::Status),
        R::Status(ViewerStatus::Backgrounded)
    ));
    assert!(matches!(
        request(&mut socket, "launch-a", C::ResumeAndFocus),
        R::Ok
    ));
    for command in [
        C::Resize {
            connection: "old".into(),
            display: ViewerDisplay {
                width: 640,
                height: 480,
            },
        },
        C::Apply {
            control: "unknown".into(),
            value: None,
        },
        C::Notice {
            message: "notice".into(),
        },
    ] {
        assert!(matches!(
            request(&mut socket, "launch-a", command),
            R::Error { .. }
        ));
    }
    write_message(
        socket.get_mut(),
        &ViewerRequest {
            launch_id: "launch-a".into(),
            command: C::Stop,
        },
    )
    .unwrap();
    let mut stopped = false;
    loop {
        match read_message(&mut socket).unwrap().unwrap() {
            M::Event(E::Status(ViewerStatus::Stopped)) => stopped = true,
            M::Reply(R::Ok) => break,
            _ => (),
        }
    }
    assert!(
        stopped,
        "Stop must not acknowledge before the runtime finishes"
    );
    running.join().unwrap();
}

#[test]
fn existing_path_is_not_deleted() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    std::fs::create_dir(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "keep").unwrap();
    assert!(Lifecycle::bind(&path, "a", unsafe { libc::geteuid() }).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "keep");
}

#[test]
fn socket_name_too_long_for_its_pinned_address_fails_closed() {
    let dir = support::scratch();
    // The address is /proc/self/fd/<descriptor>/<name> in 108 bytes.
    let path = dir.path().join("control").join("s".repeat(100));
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    assert!(Lifecycle::bind(&path, "a", unsafe { libc::geteuid() }).is_err());
    assert!(
        !path.parent().unwrap().exists(),
        "a refused bind must remove the directory it created"
    );
}

#[test]
fn startup_failure_returns_typed_error_and_stopped_event() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let lifecycle = Lifecycle::bind(&path, "a", unsafe { libc::geteuid() }).unwrap();
    let mut socket = BufReader::new(UnixStream::connect(&path).unwrap());
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let runtime = lifecycle.clone();
    let failed = std::thread::spawn(move || {
        runtime.wait_start().unwrap().expect("Start came first");
        runtime.failed("certificate rejected".into());
    });
    assert!(
        matches!(request(&mut socket, "a", C::Start { address: "127.0.0.1".into(), display: None }), R::Error { message } if message == "certificate rejected")
    );
    failed.join().unwrap();
}

#[test]
fn real_binary_reports_missing_pin_over_socket_and_exits() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_korri-viewer"))
        .args([
            "--lifecycle-socket",
            path.to_str().unwrap(),
            "--launch-id",
            "a",
            "--controller-uid",
            &unsafe { libc::geteuid() }.to_string(),
            "stream",
            "-keydir",
            dir.path().to_str().unwrap(),
            "127.0.0.1",
        ])
        .env_remove("KORRI_MOONLIGHT_SERVER_CERTIFICATE")
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(2);
    let mut socket = loop {
        match UnixStream::connect(&path) {
            Ok(socket) => break BufReader::new(socket),
            Err(_) if std::time::Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(5))
            }
            error => panic!("viewer socket did not start: {error:?}"),
        }
    };
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    assert!(
        matches!(request(&mut socket, "a", C::Start { address: "127.0.0.1:47989".into(), display: None }), R::Error { message } if message.contains("KORRI_MOONLIGHT_SERVER_CERTIFICATE is required"))
    );
    assert!(!child.wait().unwrap().success());
    assert!(
        !path.exists(),
        "the exited process must remove its own socket"
    );
}

#[test]
fn start_address_preserves_port_and_ipv6_and_rejects_url_injection() {
    use korri_viewer::{lifecycle::Start, options::Options};
    let mut options =
        Options::parse(["stream", "-keydir", "/tmp/keys", "localhost"].map(str::to_owned)).unwrap();
    for (address, host, port) in [
        ("127.0.0.1:48000", "127.0.0.1", 48000),
        ("[::1]:48001", "::1", 48001),
        ("localhost", "localhost", 47989),
        ("localhost:80", "localhost", 80),
    ] {
        Start {
            address: address.into(),
            display: None,
        }
        .configure(&mut options)
        .unwrap();
        assert_eq!((&*options.host, options.port), (host, port));
    }
    for address in [
        "name@localhost",
        "localhost/path",
        "localhost?x=1",
        "localhost#x",
        "localhost:0",
    ] {
        assert!(
            Start {
                address: address.into(),
                display: None
            }
            .configure(&mut options)
            .is_err()
        );
    }
}

#[test]
fn shutdown_closes_a_partial_reader_and_removes_the_owned_socket() {
    use std::io::{Read, Write};
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let lifecycle = Lifecycle::bind(&path, "a", unsafe { libc::geteuid() }).unwrap();
    let mut socket = BufReader::new(UnixStream::connect(&path).unwrap());
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    // A reply proves the server accepted this controller before shutdown.
    assert!(matches!(
        request(&mut socket, "a", C::Status),
        R::Error { .. }
    ));
    socket.get_mut().write_all(b"{\"launchId\":").unwrap();
    lifecycle.failed("startup deadline".into());
    lifecycle.drain();
    assert!(
        !path.exists(),
        "shutdown must unlink its socket even while the runtime handle exists"
    );
    let mut remaining = Vec::new();
    socket
        .read_to_end(&mut remaining)
        .expect("shutdown must release the blocked socket reader");
}

#[test]
fn real_binary_removes_socket_after_idle_controller_startup_timeout() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_korri-viewer"))
        .args([
            "--lifecycle-socket",
            path.to_str().unwrap(),
            "--launch-id",
            "a",
            "--controller-uid",
            &unsafe { libc::geteuid() }.to_string(),
            "stream",
            "-keydir",
            dir.path().to_str().unwrap(),
            "127.0.0.1",
        ])
        .env_remove("KORRI_MOONLIGHT_SERVER_CERTIFICATE")
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(2);
    let _idle_controller = loop {
        match UnixStream::connect(&path) {
            Ok(socket) => break socket,
            Err(_) if std::time::Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(5))
            }
            error => {
                let _ = child.kill();
                panic!("viewer socket did not start: {error:?}");
            }
        }
    };
    let until = std::time::Instant::now() + Duration::from_secs(35);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= until {
            child.kill().unwrap();
            panic!("viewer did not honor its startup deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(!status.success());
    assert!(
        !path.exists(),
        "the idle controller must not keep the exited viewer's socket path alive"
    );
}

/// The real process, as a launcher starts it. Nothing in it can connect a
/// stream without a certificate pin, so it waits for Start or fails after it.
fn lifecycle_viewer(path: &Path, keys: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_korri-viewer"));
    command
        .args([
            "--lifecycle-socket",
            path.to_str().unwrap(),
            "--launch-id",
            "a",
            "--controller-uid",
            &unsafe { libc::geteuid() }.to_string(),
            "stream",
            "-viewonly",
            "-keydir",
            keys.to_str().unwrap(),
            "127.0.0.1",
        ])
        .env_remove("KORRI_MOONLIGHT_SERVER_CERTIFICATE")
        .stderr(Stdio::null());
    command
}

/// The same process with a host to reach: it pins the host's certificate.
fn viewer_for(host: &Host, path: &Path) -> Command {
    let mut command = lifecycle_viewer(path, &host.keys);
    command.env(
        "KORRI_MOONLIGHT_SERVER_CERTIFICATE",
        host.keys.join("client.pem"),
    );
    command
}

fn start(controller: &mut BufReader<UnixStream>, port: u16) {
    write_message(
        controller.get_mut(),
        &ViewerRequest {
            launch_id: "a".into(),
            command: C::Start {
                address: format!("127.0.0.1:{port}"),
                display: None,
            },
        },
    )
    .unwrap();
}

/// A GameStream host on loopback, as Sunshine answers the viewer: HTTP port
/// discovery, then HTTPS serverinfo, applist and launch. It uses the viewer's
/// own certificate, which the viewer pins. It reports each request path, and
/// it answers the `held` endpoint only after `release`.
struct Host {
    keys: std::path::PathBuf,
    http_port: u16,
    requests: mpsc::Receiver<String>,
    release: mpsc::Sender<()>,
}
impl Host {
    fn start(keys: &Path, held: Option<&'static str>, rtsp: String) -> Self {
        use openssl::ssl::{SslAcceptor, SslMethod};
        let identity = korri_viewer::session::Identity::load_or_generate(keys).unwrap();
        let http = TcpListener::bind("127.0.0.1:0").unwrap();
        let https = TcpListener::bind("127.0.0.1:0").unwrap();
        let http_port = http.local_addr().unwrap().port();
        let https_port = https.local_addr().unwrap().port();
        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor.set_private_key(&identity.key).unwrap();
        acceptor.set_certificate(&identity.cert).unwrap();
        let acceptor = acceptor.build();
        let (report, requests) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();
        let discovery = report.clone();
        std::thread::spawn(move || {
            for socket in http.incoming() {
                let Ok(mut socket) = socket else { return };
                let Some(path) = request_path(&mut socket) else {
                    continue;
                };
                if discovery.send(path).is_err() {
                    return;
                }
                answer(&mut socket, &format!("<HttpsPort>{https_port}</HttpsPort>"));
            }
        });
        std::thread::spawn(move || {
            for socket in https.incoming() {
                let Ok(socket) = socket else { return };
                let Ok(mut tls) = acceptor.accept(socket) else {
                    continue;
                };
                let Some(path) = request_path(&mut tls) else {
                    continue;
                };
                let endpoint = path[1..].split('?').next().unwrap().to_owned();
                if report.send(path).is_err()
                    || held == Some(endpoint.as_str()) && released.recv().is_err()
                {
                    return;
                }
                let body = match endpoint.as_str() {
                    "serverinfo" => "<PairStatus>1</PairStatus><appversion>7.1.431.0</appversion><ServerCodecModeSupport>1</ServerCodecModeSupport><currentgame>0</currentgame><state>SUNSHINE_SERVER_FREE</state>".into(),
                    "applist" => "<App><AppTitle>Korri Stream</AppTitle><ID>42</ID></App>".into(),
                    _ => format!("<gamesession>1</gamesession><sessionUrl0>{rtsp}</sessionUrl0>"),
                };
                answer(&mut tls, &body);
            }
        });
        Self {
            keys: keys.to_owned(),
            http_port,
            requests,
            release,
        }
    }

    /// Every request path up to and including the first one to `endpoint`.
    fn wait_for(&self, endpoint: &str, child: &mut Child) -> Vec<String> {
        let mut seen = Vec::new();
        while !seen
            .last()
            .is_some_and(|p: &String| p.starts_with(endpoint))
        {
            match self.requests.recv_timeout(Duration::from_secs(10)) {
                Ok(path) => seen.push(path),
                Err(error) => {
                    let _ = child.kill();
                    panic!("no {endpoint} request after {seen:?}: {error}");
                }
            }
        }
        seen
    }
}

fn request_path(stream: &mut impl Read) -> Option<String> {
    let mut request = [0; 8192];
    let length = stream.read(&mut request).ok()?;
    let text = String::from_utf8_lossy(&request[..length]);
    text.split_whitespace().nth(1).map(str::to_owned)
}

fn answer(stream: &mut impl Write, body: &str) {
    let body = format!("<root status_code=\"200\">{body}</root>");
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn wait_for_removal(directory: &Path) {
    let until = Instant::now() + Duration::from_secs(2);
    while directory.exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !directory.exists(),
        "the socket and its directory must go at once"
    );
}

/// A controller that the viewer has accepted: its first request has a reply.
fn accepted_controller(path: &Path, child: &mut Child) -> BufReader<UnixStream> {
    let until = Instant::now() + Duration::from_secs(2);
    let socket = loop {
        match UnixStream::connect(path) {
            Ok(socket) => break socket,
            Err(_) if Instant::now() < until => std::thread::sleep(Duration::from_millis(5)),
            error => {
                let _ = child.kill();
                panic!("viewer socket did not start: {error:?}");
            }
        }
    };
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut socket = BufReader::new(socket);
    assert!(matches!(
        request(&mut socket, "a", C::Status),
        R::Error { .. }
    ));
    socket
}

fn signal(child: &Child, signal: libc::c_int) {
    assert_eq!(unsafe { libc::kill(child.id() as libc::pid_t, signal) }, 0);
}

fn exit_within(child: &mut Child, limit: Duration) -> ExitStatus {
    let until = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= until {
            child.kill().unwrap();
            panic!("viewer did not exit within {limit:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Every message until the viewer closes the controller's connection.
fn remaining_messages(socket: &mut BufReader<UnixStream>) -> Vec<M> {
    std::iter::from_fn(|| read_message(socket).unwrap()).collect()
}

// #468: a signal before Start used to kill the process with its default
// action and leave the socket and its private directory on disk.
#[test]
fn real_binary_stops_and_removes_socket_and_directory_on_a_signal_before_start() {
    for number in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP, libc::SIGQUIT] {
        let dir = support::scratch();
        let path = dir.path().join("control/viewer.sock");
        if support::refused_below_an_unmapped_root(&path) {
            return;
        }
        let mut child = lifecycle_viewer(&path, dir.path()).spawn().unwrap();
        let mut controller = accepted_controller(&path, &mut child);

        signal(&child, number);

        let status = exit_within(&mut child, Duration::from_secs(5));
        assert!(
            status.success(),
            "signal {number}: a stop before Start is not a failure: {status:?}"
        );
        assert!(
            remaining_messages(&mut controller)
                .iter()
                .any(|m| matches!(m, M::Event(E::Status(ViewerStatus::Stopped)))),
            "signal {number}: the controller must see Stopped"
        );
        assert!(!path.exists(), "signal {number}: socket left behind");
        assert!(
            !path.parent().unwrap().exists(),
            "signal {number}: private directory left behind"
        );
    }
}

// #468 review (P1): after Start, the main thread runs the connection steps.
// A request may take 10 s to connect and 120 s to answer. A signal must end
// a request that the host never answers, not wait for its timeout.
#[test]
fn real_binary_ends_a_held_request_and_stops_at_once_on_a_signal_after_start() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let keys = dir.path().join("keys");
    korri_viewer::session::Identity::load_or_generate(&keys).unwrap();
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let port = server.local_addr().unwrap().port();
    let mut child = lifecycle_viewer(&path, &keys)
        .env(
            "KORRI_MOONLIGHT_SERVER_CERTIFICATE",
            keys.join("client.pem"),
        )
        .spawn()
        .unwrap();
    let mut controller = accepted_controller(&path, &mut child);
    start(&mut controller, port);
    // The viewer's first request after Start. Hold it without an answer.
    let until = Instant::now() + Duration::from_secs(5);
    let _held = loop {
        match server.accept() {
            Ok((socket, _)) => break socket,
            Err(_) if Instant::now() < until => std::thread::sleep(Duration::from_millis(5)),
            error => {
                let _ = child.kill();
                panic!("viewer did not connect after Start: {error:?}");
            }
        }
    };

    signal(&child, libc::SIGTERM);

    wait_for_removal(path.parent().unwrap());
    let status = exit_within(&mut child, Duration::from_secs(5));
    assert!(
        status.success(),
        "a stop after Start is not a failure: {status:?}"
    );
    let messages = remaining_messages(&mut controller);
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, M::Reply(R::Error { .. }))),
        "Start must still get its reply: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, M::Event(E::Status(ViewerStatus::Stopped)))),
        "{messages:?}"
    );
    assert!(
        server.accept().is_err(),
        "no request may follow the stopped one"
    );
}

// #468 review (P1): the connection to the host may take 10 s to time out.
// A signal must end it at once, not wait for the connect timeout.
#[test]
fn real_binary_ends_a_hanging_connect_at_once_on_a_signal_after_start() {
    use std::os::fd::AsRawFd;
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let keys = dir.path().join("keys");
    korri_viewer::session::Identity::load_or_generate(&keys).unwrap();
    // A full accept queue: the kernel drops the viewer's connection attempt,
    // as from a host that does not answer.
    let host = TcpListener::bind("127.0.0.1:0").unwrap();
    assert_eq!(unsafe { libc::listen(host.as_raw_fd(), 0) }, 0);
    let _queued = std::net::TcpStream::connect(host.local_addr().unwrap()).unwrap();
    let mut child = lifecycle_viewer(&path, &keys)
        .env(
            "KORRI_MOONLIGHT_SERVER_CERTIFICATE",
            keys.join("client.pem"),
        )
        .spawn()
        .unwrap();
    let mut controller = accepted_controller(&path, &mut child);
    start(&mut controller, host.local_addr().unwrap().port());
    // The viewer reaches its first connect within milliseconds of Start.
    std::thread::sleep(Duration::from_millis(500));

    signal(&child, libc::SIGTERM);

    let status = exit_within(&mut child, Duration::from_secs(3));
    assert!(
        status.success(),
        "a stop after Start is not a failure: {status:?}"
    );
    assert!(!path.parent().unwrap().exists());
    assert!(
        remaining_messages(&mut controller)
            .iter()
            .any(|m| matches!(m, M::Event(E::Status(ViewerStatus::Stopped))))
    );
}

// #468 review (P1): the host must not start the application after a stop.
// The signal arrives while applist is held, after discovery and serverinfo.
#[test]
fn real_binary_never_launches_after_a_signal_right_after_start() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let host = Host::start(
        &dir.path().join("keys"),
        Some("applist"),
        "rtsp://127.0.0.1:9".into(),
    );
    let mut child = viewer_for(&host, &path).spawn().unwrap();
    let mut controller = accepted_controller(&path, &mut child);
    start(&mut controller, host.http_port);
    host.wait_for("/applist", &mut child);

    signal(&child, libc::SIGTERM);

    wait_for_removal(path.parent().unwrap());
    host.release.send(()).unwrap();
    let status = exit_within(&mut child, Duration::from_secs(5));
    assert!(
        status.success(),
        "a stop after Start is not a failure: {status:?}"
    );
    let later: Vec<_> = host.requests.try_iter().collect();
    assert!(
        later.is_empty(),
        "no request, and no launch, may follow a stop: {later:?}"
    );
    assert!(
        remaining_messages(&mut controller)
            .iter()
            .any(|m| matches!(m, M::Event(E::Status(ViewerStatus::Stopped))))
    );
}

// #468 review (P1): LiStartConnection blocks for the whole handshake. Here
// no RTSP port listens, so the core retries its connection for 10 s unless
// it is interrupted. The signal must interrupt it.
#[test]
fn real_binary_interrupts_its_stream_connection_on_a_signal() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let refused = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let host = Host::start(
        &dir.path().join("keys"),
        None,
        format!("rtsp://127.0.0.1:{refused}"),
    );
    let mut child = viewer_for(&host, &path)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // The core logs its stages without a line end until each completes.
    let log = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let mut stdout = child.stdout.take().unwrap();
    let written = log.clone();
    std::thread::spawn(move || {
        let mut chunk = [0; 4096];
        while let Ok(length @ 1..) = stdout.read(&mut chunk) {
            written
                .lock()
                .unwrap()
                .push_str(&String::from_utf8_lossy(&chunk[..length]));
        }
    });
    let mut controller = accepted_controller(&path, &mut child);
    start(&mut controller, host.http_port);
    host.wait_for("/launch", &mut child);
    let until = Instant::now() + Duration::from_secs(5);
    while !log.lock().unwrap().contains("Starting RTSP handshake") {
        if Instant::now() >= until {
            let _ = child.kill();
            panic!(
                "the core did not start its handshake: {}",
                log.lock().unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    signal(&child, libc::SIGTERM);

    let status = exit_within(&mut child, Duration::from_secs(5));
    assert!(
        status.success(),
        "a stop after Start is not a failure: {status:?}"
    );
    assert!(!path.parent().unwrap().exists());
    assert!(
        remaining_messages(&mut controller)
            .iter()
            .any(|m| matches!(m, M::Event(E::Status(ViewerStatus::Stopped))))
    );
}

// The signal may arrive before, during or after the socket's creation. None
// of those moments may leave the socket or its directory behind.
#[test]
fn a_signal_at_any_moment_of_startup_leaves_no_socket_or_directory() {
    let dir = support::scratch();
    if support::refused_below_an_unmapped_root(&dir.path().join("control/viewer.sock")) {
        return;
    }
    // Step the delay until several signals arrive after the socket exists.
    // Earlier ones end the process before it makes anything.
    let mut handled = 0;
    let mut attempt = 0u64;
    while handled < 5 {
        assert!(
            attempt < 500,
            "no signal reached the viewer after its socket existed"
        );
        let path = dir.path().join(format!("control-{attempt}/viewer.sock"));
        let mut child = lifecycle_viewer(&path, dir.path()).spawn().unwrap();
        std::thread::sleep(Duration::from_micros(attempt * 200));
        signal(&child, libc::SIGTERM);
        let status = exit_within(&mut child, Duration::from_secs(5));
        handled += usize::from(status.success());
        assert!(
            !path.parent().unwrap().exists(),
            "attempt {attempt} ({status:?}) left {} behind",
            path.parent().unwrap().display()
        );
        attempt += 1;
    }
}

#[test]
fn lifecycle_socket_requires_an_explicit_controller_uid() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_korri-viewer"))
        .args([
            "--lifecycle-socket",
            path.to_str().unwrap(),
            "--launch-id",
            "a",
            "stream",
            "-keydir",
            dir.path().to_str().unwrap(),
            "127.0.0.1",
        ])
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if std::time::Instant::now() >= until {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("viewer accepted a lifecycle socket with no controller UID");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--controller-uid"));
    assert!(!path.exists());
}

/// A controller in the viewer's own process, as on Android (ADR 0060).
fn in_process(launch: &str) -> (Lifecycle, mpsc::Receiver<E>) {
    let (events, received) = mpsc::channel();
    let events = std::sync::Mutex::new(events);
    let lifecycle = Lifecycle::in_process(launch, move |event| {
        let _ = events.lock().unwrap().send(event.clone());
    })
    .unwrap();
    (lifecycle, received)
}

fn ask(lifecycle: &Lifecycle, launch: &str, command: C) -> R {
    lifecycle.request(ViewerRequest {
        launch_id: launch.into(),
        command,
    })
}

#[test]
fn in_process_controller_gets_the_socket_replies_and_events_without_a_socket() {
    let (lifecycle, events) = in_process("launch-a");
    assert!(matches!(
        ask(&lifecycle, "launch-b", C::Status),
        R::Error { message } if message == "viewer launch identity does not match"
    ));
    let runtime = lifecycle.clone();
    let running = std::thread::spawn(move || {
        let start = runtime.wait_start().unwrap().expect("Start came first");
        assert_eq!(start.address, "127.0.0.1:47989");
        runtime
            .connected(ViewerDisplay {
                width: 1280,
                height: 720,
            })
            .unwrap();
        while !runtime.stop_requested() {
            std::thread::sleep(Duration::from_millis(1));
        }
        runtime.stopped();
    });
    assert!(matches!(
        ask(
            &lifecycle,
            "launch-a",
            C::Start {
                address: "127.0.0.1:47989".into(),
                display: None,
            }
        ),
        R::Ok
    ));
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(2)).unwrap(),
        E::Status(ViewerStatus::Running)
    ));
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(2)).unwrap(),
        E::Snapshot(s) if s.connected == Some(true)
    ));
    assert!(matches!(ask(&lifecycle, "launch-a", C::Background), R::Ok));
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(2)).unwrap(),
        E::Status(ViewerStatus::Backgrounded)
    ));
    // #476: the Rust viewer has no live resolution on either platform.
    assert!(matches!(
        ask(
            &lifecycle,
            "launch-a",
            C::Resize {
                connection: "any".into(),
                display: ViewerDisplay {
                    width: 1080,
                    height: 2340,
                },
            }
        ),
        R::Error { message } if message == "Live resolution is unavailable"
    ));
    assert!(matches!(ask(&lifecycle, "launch-a", C::Stop), R::Ok));
    let after: Vec<_> = events.try_iter().collect();
    assert!(
        matches!(after.last(), Some(E::Status(ViewerStatus::Stopped))),
        "Stop must not reply before the runtime reports Stopped: {after:?}"
    );
    running.join().unwrap();
}

#[test]
fn a_socket_lifecycle_takes_no_in_process_request() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let lifecycle = Lifecycle::bind(&path, "a", unsafe { libc::geteuid() }).unwrap();
    assert!(matches!(
        ask(&lifecycle, "a", C::Stop),
        R::Error { message } if message.contains("socket")
    ));
    assert!(!lifecycle.stop_requested());
    lifecycle.stopped();
}

// The steps that the Android app runs: the real launch chain under an
// in-process controller. Stop while the host holds the launch request ends
// the request at once; nothing is launched, and the controller sees Stopped.
#[test]
fn in_process_stop_ends_a_real_launch_and_reports_stopped() {
    let dir = support::scratch();
    let host = Host::start(
        &dir.path().join("keys"),
        Some("launch"),
        "rtsp://127.0.0.1:9".into(),
    );
    let (lifecycle, events) = in_process("a");
    let options = korri_viewer::options::Options::parse(
        [
            "stream",
            "-viewonly",
            "-keydir",
            host.keys.to_str().unwrap(),
            "127.0.0.1",
        ]
        .map(str::to_owned),
    )
    .unwrap();
    let runtime = lifecycle.clone();
    let pin = host.keys.join("client.pem");
    let viewer =
        std::thread::spawn(move || korri_viewer::launch::run(options, Some(&runtime), Some(pin)));
    let controller = lifecycle.clone();
    let address = format!("127.0.0.1:{}", host.http_port);
    let started = std::thread::spawn(move || {
        ask(
            &controller,
            "a",
            C::Start {
                address,
                display: None,
            },
        )
    });
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let path = host
            .requests
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .expect("the viewer sent no launch request");
        if path.starts_with("/launch") {
            break;
        }
    }

    let asked = Instant::now();
    assert!(matches!(ask(&lifecycle, "a", C::Stop), R::Ok));

    assert!(
        asked.elapsed() < Duration::from_secs(2),
        "{:?}",
        asked.elapsed()
    );
    assert!(
        viewer.join().unwrap().is_ok(),
        "a stop after Start is not a failure"
    );
    assert!(matches!(started.join().unwrap(), R::Error { .. }));
    let seen: Vec<_> = events.try_iter().collect();
    assert!(
        matches!(seen.last(), Some(E::Status(ViewerStatus::Stopped))),
        "{seen:?}"
    );
    host.release.send(()).ok();
    assert!(
        host.requests
            .try_iter()
            .all(|path| !path.starts_with("/launch"))
    );
}
