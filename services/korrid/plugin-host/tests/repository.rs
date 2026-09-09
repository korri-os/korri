use korri_plugin_host::repository::{fetch_catalog, SourceUrl, MAX_CATALOG_BYTES};
use rcgen::{CertificateParams, IsCa, KeyUsagePurpose};
use rustls::ServerConfig;
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct HttpsServer {
    url: SourceUrl,
    ca: PathBuf,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    _files: tempfile::TempDir,
}

impl HttpsServer {
    fn start(response: impl Fn(&str, u16) -> Vec<u8> + Send + 'static) -> Self {
        static CRYPTO: OnceLock<()> = OnceLock::new();
        CRYPTO.get_or_init(|| {
            rustls::crypto::ring::default_provider()
                .install_default()
                .unwrap()
        });
        let mut ca_params = CertificateParams::new(vec![]).unwrap();
        ca_params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "Korri test CA");
        ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
        let ca_key = rcgen::KeyPair::generate().unwrap();
        let ca_cert = ca_params.self_signed(&ca_key).unwrap();
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = CertificateParams::new(vec!["127.0.0.1".into()])
            .unwrap()
            .signed_by(&key, &ca_cert, &ca_key)
            .unwrap();
        let config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(
                    vec![cert.der().clone()],
                    rustls::pki_types::PrivateKeyDer::try_from(key.serialize_der()).unwrap(),
                )
                .unwrap(),
        );
        let files = tempfile::tempdir().unwrap();
        let ca = files.path().join("ca.pem");
        std::fs::write(&ca, ca_cert.pem()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let (socket, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("accept: {e}"),
                };
                socket
                    .set_read_timeout(Some(Duration::from_millis(500)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_millis(500)))
                    .unwrap();
                let connection = rustls::ServerConnection::new(config.clone()).unwrap();
                let mut stream = rustls::StreamOwned::new(connection, socket);
                let deadline = Instant::now() + Duration::from_secs(2);
                let mut request = Vec::new();
                let mut byte = [0];
                while request.len() < 8192 && Instant::now() < deadline {
                    match stream.read(&mut byte) {
                        Ok(1) => request.push(byte[0]),
                        _ => break,
                    }
                    if request.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                if !request.ends_with(b"\r\n\r\n") {
                    continue;
                }
                let request = String::from_utf8(request).unwrap();
                captured.lock().unwrap().push(request.clone());
                let _ = stream.write_all(&response(&request, port));
                let _ = stream.flush();
            }
        });
        Self {
            url: SourceUrl::parse(&format!("https://127.0.0.1:{port}/catalog")).unwrap(),
            ca,
            requests,
            stop,
            thread: Some(thread),
            _files: files,
        }
    }

    fn fetch(&self) -> Result<korri_plugin_host::catalog::Catalog, String> {
        fetch_catalog(&curl(), &self.url, Some(&self.ca))
    }
}

impl Drop for HttpsServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn curl() -> PathBuf {
    std::env::var_os("KORRI_TEST_CURL")
        .expect("run through nix develop .#plugin-host; HTTPS tests require KORRI_TEST_CURL")
        .into()
}
fn ok(body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}
fn redirect(url: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 302 Found\r\nLocation: {url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .into_bytes()
}

#[test]
fn fetches_a_real_https_catalog() {
    let server = HttpsServer::start(|_, _| ok(br#"{"records":[]}"#));
    assert!(server.fetch().unwrap().records.is_empty());
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

#[test]
fn fetches_literal_brackets_and_ranges_without_curl_expansion() {
    let server = HttpsServer::start(|_, _| ok(br#"{"records":[]}"#));
    let base = server.url.as_str().strip_suffix("/catalog").unwrap();
    for target in ["/catalog?filters[platform]=linux", "/catalog[1-2].json"] {
        let url = SourceUrl::parse(&format!("{base}{target}")).unwrap();
        fetch_catalog(&curl(), &url, Some(&server.ca)).unwrap();
    }
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("GET /catalog?filters[platform]=linux HTTP/"));
    assert!(requests[1].starts_with("GET /catalog[1-2].json HTTP/"));
}

#[test]
fn follows_https_redirects_and_bounds_redirect_loops() {
    let server = HttpsServer::start(|request, port| {
        if request.starts_with("GET /catalog ") {
            redirect(&format!("https://127.0.0.1:{port}/final"))
        } else {
            ok(br#"{"records":[]}"#)
        }
    });
    server.fetch().unwrap();
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    let looping =
        HttpsServer::start(|_, port| redirect(&format!("https://127.0.0.1:{port}/catalog")));
    assert!(looping.fetch().unwrap_err().contains("redirect"));
    assert_eq!(looping.requests.lock().unwrap().len(), 4);
}

#[test]
fn never_follows_an_http_downgrade() {
    let http = TcpListener::bind("127.0.0.1:0").unwrap();
    http.set_nonblocking(true).unwrap();
    let port = http.local_addr().unwrap().port();
    let server =
        HttpsServer::start(move |_, _| redirect(&format!("http://127.0.0.1:{port}/catalog")));
    let error = server.fetch().unwrap_err();
    assert!(
        error.contains("http") && error.contains("disabled"),
        "{error}"
    );
    assert_eq!(
        http.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn invalid_certificates_fail_and_tls_threads_finish() {
    let start = Instant::now();
    let server = HttpsServer::start(|_, _| ok(br#"{"records":[]}"#));
    let other = HttpsServer::start(|_, _| ok(b""));
    assert!(fetch_catalog(&curl(), &server.url, Some(&other.ca)).is_err());
    drop(server);
    drop(other);
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn rejects_oversized_malformed_partial_and_forged_official_catalogs() {
    for body in [
        vec![b' '; MAX_CATALOG_BYTES as usize + 1],
        b"not json".to_vec(),
        br#"{"records":[],"official":true}"#.to_vec(),
    ] {
        let server = HttpsServer::start(move |_, _| ok(&body));
        assert!(server.fetch().is_err());
    }
    let server =
        HttpsServer::start(|_, _| b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{}".to_vec());
    assert!(server.fetch().is_err());
}

#[test]
fn timeout_terminates_a_slow_https_transfer() {
    let server = HttpsServer::start(|_, _| {
        thread::sleep(Duration::from_millis(200));
        ok(br#"{"records":[]}"#)
    });
    let output = tempfile::NamedTempFile::new().unwrap();
    let start = Instant::now();
    let error = korri_plugin_host::repository::download(
        &curl(),
        &server.url,
        Some(&server.ca),
        output.path(),
        MAX_CATALOG_BYTES,
        Duration::from_millis(50),
    )
    .unwrap_err();
    assert!(
        error.contains("timed out") || error.contains("timeout"),
        "{error}"
    );
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn downloaded_archive_hash_is_measured_before_extraction() {
    use sha2::{Digest, Sha256};
    let bytes = b"not even a tar: hash rejection precedes extraction";
    let server =
        HttpsServer::start(|_, _| ok(b"not even a tar: hash rejection precedes extraction"));
    let output = tempfile::NamedTempFile::new().unwrap();
    korri_plugin_host::repository::download(
        &curl(),
        &server.url,
        Some(&server.ca),
        output.path(),
        4096,
        Duration::from_secs(2),
    )
    .unwrap();
    korri_plugin_host::repository::verify_archive_hash(
        output.path(),
        &hex::encode(Sha256::digest(bytes)),
    )
    .unwrap();
    assert!(
        korri_plugin_host::repository::verify_archive_hash(output.path(), &"0".repeat(64))
            .unwrap_err()
            .contains("SHA256")
    );
}

#[test]
fn streamed_oversize_is_bounded_without_content_length() {
    let server = HttpsServer::start(|_, _| {
        let mut bytes = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
        bytes.resize(MAX_CATALOG_BYTES as usize + 1024, b' ');
        bytes
    });
    assert!(server.fetch().is_err());
}

#[test]
fn download_stops_at_its_byte_limit_before_catalog_parsing() {
    for content_length in [true, false] {
        for oversized in [true, false] {
            let body = if oversized {
                vec![b'x'; 128]
            } else {
                b"small".to_vec()
            };
            let server = HttpsServer::start(move |_, _| {
                if content_length {
                    ok(&body)
                } else {
                    let mut response = format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n", body.len()).into_bytes();
                    response.extend_from_slice(&body);
                    response.extend_from_slice(b"\r\n0\r\n\r\n");
                    response
                }
            });
            let output = tempfile::NamedTempFile::new().unwrap();
            let result = korri_plugin_host::repository::download(
                &curl(),
                &server.url,
                Some(&server.ca),
                output.path(),
                8,
                Duration::from_secs(2),
            );
            if oversized {
                assert!(result.is_err());
                assert!(output.as_file().metadata().unwrap().len() <= 8);
            } else {
                result.unwrap();
                assert_eq!(std::fs::read(output.path()).unwrap(), b"small");
            }
        }
    }
}

#[test]
fn ambient_netrc_and_curlrc_cannot_supply_credentials_or_disable_tls() {
    let server = HttpsServer::start(|_, _| ok(br#"{"records":[]}"#));
    let home = tempfile::tempdir().unwrap();
    let netrc = home.path().join(".netrc");
    std::fs::write(
        &netrc,
        "machine 127.0.0.1 login stolen-user password stolen-secret\n",
    )
    .unwrap();
    std::fs::write(
        home.path().join(".curlrc"),
        "netrc\ninsecure\nuser = curlrc-user:curlrc-secret\n",
    )
    .unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "credential_probe", "--ignored", "--nocapture"])
        .env("HOME", home.path())
        .env("CURL_HOME", home.path())
        .env("NETRC", &netrc)
        .env("KORRI_PROBE_URL", server.url.as_str())
        .env("KORRI_PROBE_CA", &server.ca)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(!requests[0].to_lowercase().contains("authorization:"));
}

#[test]
#[ignore = "subprocess entrypoint; exercised by ambient_netrc_and_curlrc_cannot_supply_credentials_or_disable_tls"]
fn credential_probe() {
    let url = SourceUrl::parse(&std::env::var("KORRI_PROBE_URL").unwrap()).unwrap();
    let ca = PathBuf::from(std::env::var_os("KORRI_PROBE_CA").unwrap());
    fetch_catalog(&curl(), &url, Some(&ca)).unwrap();
    // The ambient curlrc's insecure setting must not trust this private CA.
    assert!(fetch_catalog(&curl(), &url, None).is_err());
}
