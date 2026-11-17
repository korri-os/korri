use korri_viewer::session::{Identity, Transport};
use openssl::ssl::{SslAcceptor, SslMethod};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

#[test]
fn https_accepts_only_the_authorized_certificate() {
    let dir = tempfile::tempdir().unwrap();
    let identity = Identity::load_or_generate(dir.path()).unwrap();
    let other = tempfile::tempdir().unwrap();
    Identity::load_or_generate(other.path()).unwrap();
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();
    let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    acceptor.set_private_key(&identity.key).unwrap();
    acceptor.set_certificate(&identity.cert).unwrap();
    let acceptor = acceptor.build();
    let handle = thread::spawn(move || {
        for _ in 0..2 {
            let (socket, _) = server.accept().unwrap();
            if let Ok(mut tls) = acceptor.accept(socket) {
                let mut request = [0; 4096];
                if tls.read(&mut request).unwrap_or(0) > 0 {
                    assert!(String::from_utf8_lossy(&request).starts_with("GET /serverinfo?"));
                    let body = "<root status_code=\"200\"><PairStatus>1</PairStatus></root>";
                    write!(
                        tls,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                }
            }
        }
    });
    let mut transport = Transport::new(
        "127.0.0.1",
        port,
        dir.path(),
        &dir.path().join("client.pem"),
    )
    .unwrap();
    transport.https_port = port;
    assert!(
        transport
            .get(true, "serverinfo", &[])
            .unwrap()
            .contains("PairStatus")
    );
    let mut wrong = Transport::new(
        "127.0.0.1",
        port,
        dir.path(),
        &other.path().join("client.pem"),
    )
    .unwrap();
    wrong.https_port = port;
    assert!(wrong.get(true, "serverinfo", &[]).is_err());
    handle.join().unwrap();
}

#[test]
fn malformed_pin_fails_without_a_network_request() {
    let dir = tempfile::tempdir().unwrap();
    Identity::load_or_generate(dir.path()).unwrap();
    let pin = dir.path().join("bad.pem");
    std::fs::write(&pin, "not a certificate").unwrap();
    assert!(Transport::new("127.0.0.1", 9, dir.path(), &pin).is_err());
}

#[test]
fn rust_launch_chain_uses_authenticated_serverinfo_and_the_selected_app() {
    use korri_viewer::{options::Options, stream};
    let dir = tempfile::tempdir().unwrap();
    let identity = Identity::load_or_generate(dir.path()).unwrap();
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();
    let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    acceptor.set_private_key(&identity.key).unwrap();
    acceptor.set_certificate(&identity.cert).unwrap();
    let acceptor = acceptor.build();
    let handle = thread::spawn(move || {
        let mut paths = vec![];
        for body in [
            "<PairStatus>1</PairStatus><appversion>7.1.431.0</appversion><ServerCodecModeSupport>1</ServerCodecModeSupport><currentgame>0</currentgame><state>SUNSHINE_SERVER_FREE</state>",
            "<App><AppTitle>Not this app</AppTitle><ID>1</ID></App><App><AppTitle>Korri Stream</AppTitle><ID>42</ID></App>",
            // Stop after input/presenter setup, before connecting the network core.
            "<gamesession>0</gamesession>",
        ] {
            let (socket, _) = server.accept().unwrap();
            let mut tls = acceptor.accept(socket).unwrap();
            let mut request = [0; 8192];
            let n = tls.read(&mut request).unwrap();
            paths.push(
                String::from_utf8_lossy(&request[..n])
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_owned(),
            );
            let body = format!("<root status_code=\"200\">{body}</root>");
            write!(
                tls,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        paths
    });
    let mut transport = Transport::new(
        "127.0.0.1",
        port,
        dir.path(),
        &dir.path().join("client.pem"),
    )
    .unwrap();
    transport.https_port = port;
    let options = Options::parse(
        [
            "stream",
            "-viewonly",
            "-keydir",
            dir.path().to_str().unwrap(),
            "-app",
            "Korri Stream",
            "-width",
            "1920",
            "-height",
            "1080",
            "-fps",
            "67",
            "127.0.0.1",
        ]
        .map(str::to_owned),
    )
    .unwrap();
    assert!(
        stream::run(&mut transport, &options)
            .unwrap_err()
            .to_string()
            .contains("launch refused")
    );
    let paths = handle.join().unwrap();
    assert!(paths[0].starts_with("/serverinfo?"));
    assert!(paths[1].starts_with("/applist?"));
    assert!(paths[2].starts_with("/launch?"));
    assert!(paths[2].contains("&appid=42&mode=1920x1080x67&"));
    assert!(paths[2].contains("&surroundAudioInfo=196610&remoteControllersBitmap=0&gcmap=0"));
    let key = paths[2]
        .split("&rikey=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();
    assert_eq!(key.len(), 32);
    assert!(key.bytes().all(|b| b.is_ascii_hexdigit()));
}

#[test]
fn each_https_connection_performs_fresh_client_authentication() {
    use openssl::ssl::{SslSessionCacheMode, SslVersion};
    let dir = tempfile::tempdir().unwrap();
    let identity = Identity::load_or_generate(dir.path()).unwrap();
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();
    let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    acceptor.set_private_key(&identity.key).unwrap();
    acceptor.set_certificate(&identity.cert).unwrap();
    acceptor
        .set_max_proto_version(Some(SslVersion::TLS1_2))
        .unwrap();
    acceptor.set_session_id_context(b"viewer-test").unwrap();
    acceptor.set_session_cache_mode(SslSessionCacheMode::SERVER);
    let acceptor = acceptor.build();
    let handle = thread::spawn(move || {
        let mut reused = vec![];
        for _ in 0..2 {
            let (socket, _) = server.accept().unwrap();
            let mut tls = acceptor.accept(socket).unwrap();
            reused.push(tls.ssl().session_reused());
            let mut request = [0; 4096];
            assert!(tls.read(&mut request).unwrap() > 0);
            let body = "<root status_code=\"200\"/>";
            write!(
                tls,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            let _ = tls.shutdown();
        }
        reused
    });
    let mut transport = Transport::new(
        "127.0.0.1",
        port,
        dir.path(),
        &dir.path().join("client.pem"),
    )
    .unwrap();
    transport.https_port = port;
    transport.get(true, "serverinfo", &[]).unwrap();
    transport.get(true, "applist", &[]).unwrap();
    assert_eq!(handle.join().unwrap(), vec![false, false]);
}

/// Sunshine answers each HTTPS request without `Connection: close`, sends
/// its close_notify and waits for the client to close; until it does, the
/// fixture's Sunshine serves no other client (#510, korri#443). libcurl kept
/// each connection for reuse: the launch's stayed open for the whole stream,
/// and the next request went into the closed one first. Each connection now
/// closes once its answer is in, and the next request opens its own.
#[test]
fn each_answer_closes_its_connection_and_the_next_request_opens_another() {
    use std::{sync::mpsc, time::Duration};
    let dir = tempfile::tempdir().unwrap();
    let identity = Identity::load_or_generate(dir.path()).unwrap();
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();
    let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    acceptor.set_private_key(&identity.key).unwrap();
    acceptor.set_certificate(&identity.cert).unwrap();
    let acceptor = acceptor.build();
    // For each connection: whether the client closed it within 5 s of the
    // answer, and the bytes that it wrote after the answer.
    let (closed, heard) = mpsc::channel();
    let handle = thread::spawn(move || {
        for _ in 0..2 {
            let (socket, _) = server.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut tls = acceptor.accept(socket).unwrap();
            let mut request = [0; 4096];
            assert!(tls.read(&mut request).unwrap() > 0);
            let body = "<root status_code=\"200\"/>";
            write!(
                tls,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            let _ = tls.shutdown();
            let mut after = 0;
            let ended = loop {
                match tls.read(&mut request) {
                    Ok(0) => break true,
                    Ok(n) => after += n,
                    // A timeout: the client kept it open.
                    Err(error) => {
                        break !matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        );
                    }
                }
            };
            closed.send((ended, after)).unwrap();
        }
    });
    let mut transport = Transport::new(
        "127.0.0.1",
        port,
        dir.path(),
        &dir.path().join("client.pem"),
    )
    .unwrap();
    transport.https_port = port;
    transport.get(true, "serverinfo", &[]).unwrap();
    // The transport lives on, as it does for the whole stream.
    assert_eq!(
        heard.recv_timeout(Duration::from_secs(10)).unwrap(),
        (true, 0),
        "the first connection closes after its answer, with nothing more in it"
    );
    transport.get(true, "applist", &[]).unwrap();
    assert_eq!(
        heard.recv_timeout(Duration::from_secs(10)).unwrap(),
        (true, 0)
    );
    handle.join().unwrap();
}

/// The app makes the client key when it starts, while a launch may already
/// ask for it (#510): two callers at once make one key between them, and
/// both get it.
#[test]
fn two_callers_at_once_make_one_client_key() {
    use std::sync::{Arc, Barrier};
    for _ in 0..4 {
        let dir = tempfile::tempdir().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let callers: Vec<_> = (0..2)
            .map(|_| {
                let (dir, barrier) = (dir.path().to_owned(), barrier.clone());
                thread::spawn(move || {
                    barrier.wait();
                    Identity::load_or_generate(&dir).map(|identity| identity.cert.to_der().unwrap())
                })
            })
            .collect();
        let certificates: Vec<_> = callers
            .into_iter()
            .map(|caller| caller.join().unwrap().unwrap())
            .collect();
        assert_eq!(certificates[0], certificates[1]);
        assert_eq!(
            Identity::load(dir.path()).unwrap().cert.to_der().unwrap(),
            certificates[0]
        );
    }
}
