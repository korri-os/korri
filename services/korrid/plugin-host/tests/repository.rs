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

#[test]
#[ignore = "writes build-machine Nix store; run explicitly with KORRI_PUBLISH_NIX"]
fn release_evidence_selects_only_a_unique_bound_signed_package_with_real_nix() {
    use base64::Engine;
    use korri_plugin_host::{package, provenance::Provenance, release};
    use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt, path::Path};

    let nix = std::env::var("KORRI_PUBLISH_NIX").expect("supply the build-machine Nix executable");
    let run = |args: &[&str]| {
        let output = Command::new(&nix)
            .args(["--extra-experimental-features", "nix-command"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let files = tempfile::tempdir().unwrap();
    let root = files.path().to_path_buf();
    let secret = run(&["key", "generate-secret", "--key-name", "release-test"]);
    let other_secret = run(&["key", "generate-secret", "--key-name", "release-test"]);
    let secret_file = root.join("key");
    let other_file = root.join("other-key");
    fs::write(&secret_file, &secret).unwrap();
    fs::write(&other_file, &other_secret).unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(secret.split_once(':').unwrap().1)
        .unwrap();
    let public = format!(
        "release-test:{}",
        base64::engine::general_purpose::STANDARD.encode(&raw[32..])
    );
    let cache = format!("file://{}", root.join("cache").display());
    let make_package = |directory: &str, name: &str, key: &Path| {
        let input = root.join(directory);
        fs::create_dir(&input).unwrap();
        fs::write(
            input.join("manifest.json"),
            r#"{"publisher":{"namespace":"@example"},"entry":"plugin.ts","sources":["plugin.ts"]}"#,
        )
        .unwrap();
        fs::write(
            input.join("plugin.ts"),
            format!("export const name = '{name}';"),
        )
        .unwrap();
        fs::write(input.join("test-run"), input.to_str().unwrap()).unwrap();
        let path = run(&["store", "add-path", input.to_str().unwrap()]);
        run(&[
            "copy",
            "--to",
            &format!("{cache}?secret-key={}", key.display()),
            &path,
        ]);
        PathBuf::from(path)
    };
    let first = make_package("first", "game", &secret_file);
    let unrelated = make_package("unrelated", "other", &secret_file);
    let ambiguous = make_package("ambiguous", "game", &secret_file);
    let untrusted = make_package("untrusted", "game", &other_file);
    let malformed = make_package(
        "malformed",
        "game'; export const unexpected = true; //",
        &secret_file,
    );
    // Remove only this test's unique output so success exercises a real NAR
    // download and content verification, not merely a cached local package.
    run(&["store", "delete", first.to_str().unwrap()]);
    assert!(!first.exists());
    let served = root.clone();
    let server = HttpsServer::start(move |request, _| {
        let target = request.split_whitespace().nth(1).unwrap();
        if target.contains("..") {
            return b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n".to_vec();
        }
        match fs::read(served.join(target.trim_start_matches('/'))) {
            Ok(body) => ok(&body),
            Err(_) => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
        }
    });
    let base = server.url.as_str().strip_suffix("catalog").unwrap();
    let source = format!("{base}cache");
    let batch = root.join("build-0123456789ab");
    fs::create_dir(&batch).unwrap();
    fs::write(batch.join("revision.txt"), format!("{BATCH_REVISION}\n")).unwrap();
    // This wrapper only configures the actual Nix process. It neither supplies
    // responses nor replaces Nix metadata/signature/import implementations.
    let configured_nix = root.join("nix");
    fs::write(&configured_nix, format!(
        "#!{}\nexec '{}' --option trusted-public-keys '{}' --option ssl-cert-file '{}' --option substituters '' --option narinfo-cache-negative-ttl 0 --option narinfo-cache-positive-ttl 0 \"$@\"\n",
        std::env::var("SHELL").unwrap(), nix, public, server.ca.display(),
    )).unwrap();
    fs::set_permissions(&configured_nix, fs::Permissions::from_mode(0o700)).unwrap();
    let bindings = BTreeMap::from([(
        "@example".into(),
        package::PublisherBinding {
            public_key: public,
            cache_url: source.clone(),
        },
    )]);
    let inspect = |path: &Path| {
        package::import(&configured_nix, &source, path)?;
        package::verify_publisher(&configured_nix, path, Some(&source), &bindings)?;
        package::load(
            &configured_nix,
            path,
            Provenance::RawCache {
                cache_url: source.clone(),
            },
        )
        .map(|report| report.id)
    };
    let select = |paths: &[&Path], id: &str| {
        let text = paths
            .iter()
            .map(|p| format!("{}\n", p.display()))
            .collect::<String>();
        fs::write(batch.join("paths-x86_64-linux.txt"), text).unwrap();
        let paths = fetch_batch(&server, "x86_64-linux")?;
        release::select_output(&paths, id, inspect)
    };
    assert_eq!(
        select(&[&first, &unrelated], "@example:game").unwrap(),
        first
    );
    assert!(select(&[&first, &unrelated], "@example:missing")
        .unwrap_err()
        .contains("no verified output"));
    assert!(select(&[&first, &ambiguous], "@example:game")
        .unwrap_err()
        .contains("multiple outputs"));
    let error = select(&[&first, &untrusted], "@example:game").unwrap_err();
    assert!(error.contains("not signed by the full key"), "{error}");
    assert!(
        select(&[&first, &malformed], "@example:game").is_err(),
        "a signed but invalid later declaration must fail the batch"
    );
    assert!(package::verify_publisher(
        &configured_nix,
        &first,
        Some("https://different.example/cache"),
        &bindings
    )
    .is_err());
    let missing = Path::new("/nix/store/00000000000000000000000000000000-unavailable");
    assert!(
        select(&[&first, missing], "@example:game").is_err(),
        "a cache miss after a match must fail, never build or choose the earlier match"
    );
    // Instantiate (never build) a real derivation on this build machine. The
    // consumer sees only its exact output and must refuse its available builder.
    let marker = root.join("builder-ran");
    let expression = format!("derivation {{ name = \"release-no-build\"; system = builtins.currentSystem; builder = \"/bin/sh\"; args = [ \"-c\" \"touch {}; mkdir $out\" ]; }}", marker.display());
    let instantiated = Command::new(Path::new(&nix).with_file_name("nix-instantiate"))
        .args(["--expr", &expression])
        .output()
        .unwrap();
    assert!(
        instantiated.status.success(),
        "{}",
        String::from_utf8_lossy(&instantiated.stderr)
    );
    let drv = String::from_utf8(instantiated.stdout).unwrap();
    let queried = Command::new(Path::new(&nix).with_file_name("nix-store"))
        .args(["--query", "--outputs", drv.trim()])
        .output()
        .unwrap();
    assert!(queried.status.success());
    let output = String::from_utf8(queried.stdout).unwrap();
    let output = output.trim();
    let error = select(&[&first, Path::new(output)], "@example:game").unwrap_err();
    assert!(
        error.contains("no substituter"),
        "the consumer must refuse the absent output instead of evaluating its derivation: {error}"
    );
    assert!(!marker.exists());
    assert!(!Path::new(output).exists());
    let report = package::load(
        &configured_nix,
        &first,
        Provenance::RawCache {
            cache_url: source.clone(),
        },
    )
    .unwrap();
    assert_eq!(report.package, first);
    assert_eq!(report.id, "@example:game");
    assert!(!report.approval.is_empty());
    // A valid signature over metadata must not admit damaged payload bytes.
    let hash = &first.file_name().unwrap().to_str().unwrap()[..32];
    let narinfo = fs::read_to_string(root.join(format!("cache/{hash}.narinfo"))).unwrap();
    let nar = narinfo
        .lines()
        .find_map(|line| line.strip_prefix("URL: "))
        .unwrap();
    let payload = root.join("cache").join(nar);
    let mut bytes = fs::read(&payload).unwrap();
    bytes[0] ^= 0xff;
    fs::write(payload, bytes).unwrap();
    run(&["store", "delete", first.to_str().unwrap()]);
    let error = select(&[&first], "@example:game").unwrap_err();
    assert!(
        error.contains("input compression not recognized"),
        "Nix must refuse the damaged payload bytes: {error}"
    );
    assert!(
        !first.exists(),
        "damaged NAR must not become an installed store output"
    );
    let requests = server.requests.lock().unwrap();
    assert!(
        requests.iter().any(|r| r.contains(".narinfo ")),
        "Nix must verify real remote metadata, not just local signatures"
    );
    assert!(requests.iter().any(|r| r.contains("/nix-cache-info ")));
    assert!(
        requests.iter().any(|r| r.starts_with("GET /cache/nar/")),
        "the selected output must be downloaded from the real HTTPS cache"
    );
}

const BATCH_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const BATCH_OUTPUT: &str = "/nix/store/0123456789abcdfghijklmnpqrsvwxyz-plugin";

fn fetch_batch(server: &HttpsServer, system: &str) -> Result<Vec<PathBuf>, String> {
    let base = server.url.as_str().strip_suffix("catalog").unwrap();
    korri_plugin_host::release::fetch_paths(
        &curl(),
        &SourceUrl::parse(&format!("{base}build-0123456789ab/")).unwrap(),
        BATCH_REVISION,
        system,
        Some(&server.ca),
    )
}

#[test]
fn release_lookup_fetches_exact_revision_then_only_the_host_architecture() {
    for system in ["x86_64-linux", "aarch64-linux"] {
        let server = HttpsServer::start(move |request, _| {
            if request.starts_with("GET /build-0123456789ab/revision.txt ") {
                ok(format!("{BATCH_REVISION}\n").as_bytes())
            } else if request.starts_with(&format!("GET /build-0123456789ab/paths-{system}.txt ")) {
                ok(format!("{BATCH_OUTPUT}\n").as_bytes())
            } else {
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
            }
        });
        assert_eq!(
            fetch_batch(&server, system).unwrap(),
            vec![PathBuf::from(BATCH_OUTPUT)]
        );
        assert_eq!(server.requests.lock().unwrap().len(), 2);
    }
}

#[test]
fn release_lookup_refuses_wrong_or_malformed_revision_before_requesting_paths() {
    for revision in [
        format!("{}\n", "f".repeat(40)),
        format!("{}{}\n", &BATCH_REVISION[..12], "f".repeat(28)),
        format!("{}\n", BATCH_REVISION.to_uppercase()),
        BATCH_REVISION.into(),
        format!("{BATCH_REVISION}\r\n"),
        format!("{BATCH_REVISION}\n{BATCH_REVISION}\n"),
        "latest\n".into(),
        String::new(),
    ] {
        let server = HttpsServer::start(move |_, _| ok(revision.as_bytes()));
        assert!(fetch_batch(&server, "x86_64-linux").is_err());
        assert_eq!(server.requests.lock().unwrap().len(), 1);
    }
}

#[test]
fn release_lookup_keeps_https_authentication_and_refuses_unknown_architectures() {
    let server = HttpsServer::start(|_, _| redirect("http://127.0.0.1/batch"));
    assert!(fetch_batch(&server, "x86_64-linux").is_err());
    assert_eq!(server.requests.lock().unwrap().len(), 1);
    assert!(fetch_batch(&server, "armv7l-linux").is_err());
    assert_eq!(
        server.requests.lock().unwrap().len(),
        1,
        "unsupported architecture must not fetch"
    );
    let other = HttpsServer::start(|_, _| ok(b"unused"));
    let base = SourceUrl::parse(server.url.as_str().strip_suffix("catalog").unwrap()).unwrap();
    assert!(korri_plugin_host::release::fetch_paths(
        &curl(),
        &base,
        BATCH_REVISION,
        "x86_64-linux",
        Some(&other.ca)
    )
    .is_err());
    assert_eq!(
        server.requests.lock().unwrap().len(),
        1,
        "untrusted TLS must not reach the batch endpoint"
    );
}

#[test]
fn release_lookup_refuses_http_errors_malformed_paths_and_incomplete_transfers() {
    for response in [
        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
        b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".to_vec(),
        b"HTTP/1.1 500 Server Error\r\nContent-Length: 0\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n/nix/store/".to_vec(),
        ok(b"not an output\n"),
        ok(format!("{BATCH_OUTPUT}\n{BATCH_OUTPUT}\n").as_bytes()),
        ok(&vec![b'x'; 64 * 1024 + 1]),
    ] {
        let server = HttpsServer::start(move |request, _| {
            if request.contains("/revision.txt ") {
                ok(format!("{BATCH_REVISION}\n").as_bytes())
            } else {
                response.clone()
            }
        });
        assert!(fetch_batch(&server, "x86_64-linux").is_err());
        assert_eq!(server.requests.lock().unwrap().len(), 2);
    }
}
