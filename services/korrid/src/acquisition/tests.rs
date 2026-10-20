use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

/// A real HTTP server exercises reqwest's interrupted-body handling and Range.
/// Each scenario uses the same transfer path, not a mocked response stream.
#[tokio::test]
async fn resumes_verifies_restarts_wrong_hash_and_416_and_fails_a_stall() {
    for scenario in ["drop", "hash", "416", "stall", "redirect"] {
        let data = vec![b'x'; 128 * 1024];
        let digest = hex::encode(Sha256::digest(&data));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = requests.clone();
        let bytes = data.clone();
        let server = thread::spawn(move || {
            let count = match scenario {
                "stall" => 6,
                "redirect" => 6,
                _ => 2,
            };
            for index in 0..count {
                let (mut socket, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut line = String::new();
                let mut offset = None;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(range) = line.to_lowercase().strip_prefix("range: bytes=") {
                        offset = Some(range.trim().trim_end_matches('-').parse::<usize>().unwrap());
                    }
                }
                observed.lock().unwrap().push(offset);
                if scenario == "redirect" {
                    write!(socket, "HTTP/1.1 302 Found\r\nLocation: /next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else if scenario == "stall" {
                    write!(
                        socket,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    )
                    .unwrap();
                    // Do not block accepting retries while this connection stalls.
                    thread::spawn(move || {
                        thread::sleep(Duration::from_secs(1));
                        drop(socket);
                    });
                } else if scenario == "416" && index == 0 {
                    write!(socket, "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    let start = offset.unwrap_or(0);
                    if offset.is_some() {
                        write!(socket, "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {}-{}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", start, bytes.len()-1, bytes.len(), bytes.len()-start).unwrap();
                    } else {
                        write!(
                            socket,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            bytes.len()
                        )
                        .unwrap();
                    }
                    if scenario == "drop" && index == 0 {
                        socket.write_all(&bytes[..65536]).unwrap();
                    } else if scenario == "hash" && index == 0 {
                        socket.write_all(&vec![b'y'; bytes.len()]).unwrap();
                    } else {
                        socket.write_all(&bytes[start..]).unwrap();
                    }
                }
            }
        });
        let root = tempfile::tempdir().unwrap();
        if scenario == "416" {
            fs::write(root.path().join(format!("{digest}.part")), b"old").unwrap();
        }
        let file = crate::plugin::ArtifactFile {
            url: format!("http://{address}/file"),
            sha256: format!("sha256:{digest}"),
            bytes: None,
        };
        let engine = download::DownloadEngine::for_test(Duration::from_millis(100));
        let mut progress = Vec::new();
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            engine.fetch(&file, root.path(), |sample| progress.push(sample)),
        )
        .await
        .unwrap();
        if scenario == "stall" {
            assert_eq!(result.unwrap_err(), Failure::Stalled);
        } else if scenario == "redirect" {
            assert_eq!(result.unwrap_err(), Failure::Redirect);
        } else {
            let path = result.unwrap();
            assert_eq!(fs::read(&path).unwrap(), data);
            assert_eq!(path.file_name().unwrap(), digest.as_str());
            assert!(!root.path().join(format!("{digest}.part")).exists());
            assert!(progress.iter().any(|p| p.total == Some(data.len() as u64)));
        }
        server.join().unwrap();
        let observed = requests.lock().unwrap();
        match scenario {
            "drop" => {
                assert_eq!(observed[0], None);
                assert!(observed[1].is_some_and(|offset| offset > 0 && offset < data.len()));
            }
            "hash" => assert_eq!(*observed, [None, None]),
            "416" => assert_eq!(*observed, [Some(3), None]),
            "stall" | "redirect" => assert_eq!(observed.len(), 6),
            _ => unreachable!(),
        }
    }
}

#[test]
fn failures_expose_only_bounded_plain_copy() {
    let storage = Failure::from(std::io::Error::new(
        std::io::ErrorKind::Other,
        "https://user:password@example.com/token sha256:private /private/path",
    ));
    for error in [
        storage,
        Failure::NoSpace,
        Failure::UnsafeUrl,
        Failure::UnsafePath,
        Failure::Redirect,
        Failure::Hash,
        Failure::Stalled,
        Failure::Unpack,
    ] {
        let failure = error.rpc();
        assert!(failure.message.len() <= crate::launcher::redaction::MAX_REASON_CHARS);
        for private in ["https:", "sha256:", "/", "password", "token"] {
            assert!(!failure.message.contains(private));
        }
    }
}

fn registry(url: &str, digest: &str) -> PluginRegistry {
    let declaration = serde_json::json!({
        "name":"runtime", "title":"Runtime",
        "artifacts":{"payload":{"id":"@test:runtime/payload", "version":"v1", "files":[{"url":url, "sha256":format!("sha256:{digest}"), "bytes":4}]}},
        "runners":{"run":{"id":"@test:runtime/run", "command":"run", "artifacts":["@test:runtime/payload"]}}
    });
    let plugin =
        crate::plugin::decode_plugin_declaration("@test", &declaration.to_string()).unwrap();
    PluginRegistry::new(vec![plugin], ["@test:runtime".into()]).unwrap()
}
pub(super) fn runner_route(runner: &str) -> crate::config::resolver::ResolvedRoute {
    crate::config::resolver::ResolvedRoute {
        playable_id: "game".into(),
        title: None,
        release_id: String::new(),
        identity: None,
        provider_id: String::new(),
        system_id: String::new(),
        system_title: None,
        runner_id: runner.into(),
        family_id: None,
        integration_token: String::new(),
        flattened_target: String::new(),
        linux_runner: None,
        core_path: None,
        file_target: None,
        provider_target: None,
        artifact_target: None,
        package_target: None,
    }
}
async fn until(
    coordinator: &AcquisitionCoordinator,
    root: &Path,
    registry: &PluginRegistry,
    done: impl Fn(&ArtifactState) -> bool,
) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !done(&state(coordinator, root, registry)) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
/// The worker has finished every job and released the sleep block.
async fn idle(coordinator: &AcquisitionCoordinator) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            {
                let queue = coordinator.lock();
                if !queue.worker && queue.running.is_none() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
fn state(
    coordinator: &AcquisitionCoordinator,
    root: &Path,
    registry: &PluginRegistry,
) -> ArtifactState {
    coordinator
        .status(root, registry, None)
        .unwrap()
        .artifacts
        .remove(0)
        .state
}
async fn settled(
    coordinator: &AcquisitionCoordinator,
    root: &Path,
    registry: &PluginRegistry,
) -> ArtifactState {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let current = state(coordinator, root, registry);
            if !matches!(
                current,
                ArtifactState::Queued
                    | ArtifactState::Downloading { .. }
                    | ArtifactState::Unpacking
            ) {
                return current;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn cache_truth_idempotent_start_retry_and_launch_paths() {
    let root = tempfile::tempdir().unwrap();
    let hash = hex::encode(Sha256::digest(b"data"));
    let registry = registry("https://example.com/payload%20file", &hash);
    let coordinator = AcquisitionCoordinator::default();
    assert!(matches!(
        state(&coordinator, root.path(), &registry),
        ArtifactState::Missing
    ));
    assert_eq!(
        require_ready(root.path(), &registry, &runner_route("@test:runtime/run"))
            .unwrap_err()
            .code,
        "ArtifactsNotReady"
    );
    assert!(
        launch_artifacts(root.path(), &registry, "@test:runtime/run")
            .unwrap()
            .is_empty()
    );
    assert!(coordinator
        .start(root.path(), root.path(), &registry, &["unknown".into()])
        .is_err());
    let artifact = registry.artifact("@test:runtime/payload").unwrap();
    let base = placement::prepare(root.path(), artifact).unwrap();
    let downloads = placement::downloads(&base).unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(&downloads).unwrap().permissions().mode() & 0o777,
        0o700
    );
    fs::write(downloads.join(&hash), b"data").unwrap();
    let ids = [artifact.id.clone()];
    coordinator
        .start(root.path(), root.path(), &registry, &ids)
        .unwrap();
    coordinator
        .start(root.path(), root.path(), &registry, &ids)
        .unwrap();
    assert!(matches!(
        settled(&coordinator, root.path(), &registry).await,
        ArtifactState::Ready { .. }
    ));
    let ready = placement::ready_path(root.path(), artifact);
    assert_eq!(fs::read(ready.join("payload file")).unwrap(), b"data");
    assert_eq!(
        fs::metadata(&ready).unwrap().permissions().mode() & 0o222,
        0
    );
    assert_eq!(
        fs::metadata(ready.join("payload file"))
            .unwrap()
            .permissions()
            .mode()
            & 0o222,
        0
    );
    // Ready is the atomic rename, not completion of post-publication cleanup.
    // Wait for the writer to release .lock before checking download removal.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if placement::lock(&base).unwrap().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(!downloads.exists());
    require_ready(root.path(), &registry, &runner_route("@test:runtime/run")).unwrap();
    assert_eq!(
        launch_artifacts(root.path(), &registry, "@test:runtime/run").unwrap()["payload"],
        ready.display().to_string()
    );
    assert!(matches!(
        state(&AcquisitionCoordinator::default(), root.path(), &registry),
        ArtifactState::Ready { .. }
    ));
    // Finish the worker before this test simulates external cache deletion.
    // Publication is already Ready while its async completion is still queued.
    idle(&coordinator).await;
    // The directory, not a stale in-memory success, remains authoritative.
    fs::set_permissions(&ready, fs::Permissions::from_mode(0o755)).unwrap();
    fs::remove_dir_all(&ready).unwrap();
    assert!(matches!(
        state(&coordinator, root.path(), &registry),
        ArtifactState::Missing
    ));
    assert!(matches!(
        state(&AcquisitionCoordinator::default(), root.path(), &registry),
        ArtifactState::Missing
    ));
}

#[tokio::test]
async fn failure_is_retryable_and_restart_forgets_failure_but_keeps_partial_bytes() {
    let root = tempfile::tempdir().unwrap();
    let digest = hex::encode(Sha256::digest(b"data"));
    let registry = registry("file:///secret", &digest);
    let coordinator = AcquisitionCoordinator::default();
    let ids = ["@test:runtime/payload".into()];
    coordinator
        .start(root.path(), root.path(), &registry, &ids)
        .unwrap();
    assert!(matches!(
        settled(&coordinator, root.path(), &registry).await,
        ArtifactState::Failed { .. }
    ));
    assert!(matches!(
        state(&AcquisitionCoordinator::default(), root.path(), &registry),
        ArtifactState::Missing
    ));
    let base = placement::contribution(root.path(), registry.artifact(&ids[0]).unwrap());
    fs::write(base.join(".downloads").join(&digest), b"data").unwrap();
    coordinator
        .start(root.path(), root.path(), &registry, &ids)
        .unwrap();
    assert!(matches!(
        settled(&coordinator, root.path(), &registry).await,
        ArtifactState::Ready { .. }
    ));
}

#[tokio::test]
async fn cancellation_deletes_partial_bytes_and_cross_process_lock_blocks_a_second_writer() {
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(socket.try_clone().unwrap());
        let mut line = String::new();
        loop {
            line.clear();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
        }
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: 100000\r\n\r\npart"
        )
        .unwrap();
        thread::sleep(Duration::from_millis(500));
    });
    let registry = registry(&format!("http://{address}/payload"), &"a".repeat(64));
    let coordinator = AcquisitionCoordinator {
        engine: download::DownloadEngine::for_test(Duration::from_secs(2)),
        ..Default::default()
    };
    let artifact = registry.artifact("@test:runtime/payload").unwrap();
    let base = placement::prepare(root.path(), artifact).unwrap();
    let lock = placement::lock(&base).unwrap().unwrap();
    assert!(placement::lock(&base).unwrap().is_none());
    coordinator
        .start(root.path(), root.path(), &registry, &[artifact.id.clone()])
        .unwrap();
    until(&coordinator, root.path(), &registry, |state| {
        matches!(state, ArtifactState::Downloading { .. })
    })
    .await;
    // Queueing wrote only the zero-byte marker; the writer waits for .lock.
    let marker = base
        .join(".downloads")
        .join(format!("{}.part", "a".repeat(64)));
    assert_eq!(fs::metadata(&marker).unwrap().len(), 0);
    drop(lock);
    let partial = base
        .join(".downloads")
        .join(format!("{}.part", "a".repeat(64)));
    tokio::time::timeout(Duration::from_secs(2), async {
        while fs::metadata(&partial).map(|m| m.len()).unwrap_or(0) < 4 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    coordinator
        .cancel(root.path(), &registry, &artifact.id)
        .await
        .unwrap();
    assert!(matches!(
        state(&coordinator, root.path(), &registry),
        ArtifactState::Missing
    ));
    // ADR 0013: Cancel deletes downloaded bytes, so anything left in
    // .downloads always means an interrupted download.
    assert!(!partial.exists());
    assert!(!base.join(".downloads").exists());
    assert!(placement::interrupted_since(&base).is_none());
    assert!(matches!(
        state(&AcquisitionCoordinator::default(), root.path(), &registry),
        ArtifactState::Missing
    ));
    server.join().unwrap();
}

#[test]
fn unpack_receives_verified_files_in_declared_order_and_never_publishes_failure() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let registry = registry("https://example.com/file", &"a".repeat(64));
    let artifact = registry.artifact("@test:runtime/payload").unwrap();
    let base = placement::prepare(root.path(), artifact).unwrap();
    let downloads = placement::downloads(&base).unwrap();
    let first = downloads.join("first");
    let second = downloads.join("second");
    fs::write(&first, b"first").unwrap();
    fs::write(&second, b"second").unwrap();
    let unpack = root.path().join("unpack");
    fs::write(&unpack, "#!/bin/sh\ncat \"$2\" \"$3\" > \"$1/result\"\n").unwrap();
    fs::set_permissions(&unpack, fs::Permissions::from_mode(0o755)).unwrap();
    placement::publish(
        &base,
        root.path(),
        artifact,
        Some(&unpack),
        &[first, second],
    )
    .unwrap();
    assert_eq!(fs::read(base.join("v1/result")).unwrap(), b"firstsecond");
    let mut next = artifact.clone();
    next.version = "v2".into();
    placement::downloads(&base).unwrap();
    fs::write(&unpack, "#!/bin/sh\nexit 1\n").unwrap();
    assert_eq!(
        placement::publish(&base, root.path(), &next, Some(&unpack), &[]),
        Err(Failure::Unpack)
    );
    assert!(!base.join("v2").exists());
    assert!(!fs::read_dir(&base).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".tmp-")));
}

#[tokio::test]
async fn unpacking_is_observable_and_cannot_be_cancelled() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let package = root.path().join("package");
    fs::create_dir(&package).unwrap();
    let digest = hex::encode(Sha256::digest(b"data"));
    fs::write(package.join("plugin.ts"), format!("export const name='runtime'; export const artifacts={{payload:{{id:'@test:runtime/payload',version:'v1',files:[{{url:'https://example.com/data',sha256:'sha256:{digest}'}}],unpack:'unpack'}}}};")).unwrap();
    let program = package.join("unpack");
    fs::write(&program, "#!/bin/sh\nprintf begun > \"$1/begun\"\nwhile [ ! -f \"$1/release\" ]; do sleep 0.01; done\ncat \"$2\" > \"$1/data\"\n").unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let registry =
        PluginRegistry::from_installed(vec![crate::plugin_installation::EnabledPackage {
            id: "@test:runtime".into(),
            package,
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            requires: vec![],
            hosts: vec![],
            files: BTreeMap::from([("unpack".into(), program)]),
        }])
        .unwrap();
    let artifact = registry.artifact("@test:runtime/payload").unwrap();
    let base = placement::prepare(root.path(), artifact).unwrap();
    let downloads = placement::downloads(&base).unwrap();
    fs::write(downloads.join(digest), b"data").unwrap();
    let coordinator = AcquisitionCoordinator::default();
    coordinator
        .start(root.path(), root.path(), &registry, &[artifact.id.clone()])
        .unwrap();
    let work = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(work) = fs::read_dir(&base)
                .unwrap()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| path.join("begun").exists())
            {
                break work;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        state(&coordinator, root.path(), &registry),
        ArtifactState::Unpacking
    ));
    let cancelled = coordinator
        .cancel(root.path(), &registry, &artifact.id)
        .await
        .unwrap();
    assert!(matches!(
        cancelled.artifacts[0].state,
        ArtifactState::Unpacking
    ));
    fs::write(work.join("release"), b"continue").unwrap();
    assert!(matches!(
        settled(&coordinator, root.path(), &registry).await,
        ArtifactState::Ready { .. }
    ));
}

#[tokio::test]
async fn two_file_progress_is_cumulative_and_combines_declared_and_observed_sizes() {
    two_file_progress(true).await;
}

#[tokio::test]
async fn completed_chunked_file_fills_an_unknown_artifact_size() {
    two_file_progress(false).await;
}

async fn two_file_progress(declare_first_size: bool) {
    let first = vec![b'a'; 16_384];
    let second = b"second file".to_vec();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (release, wait) = std::sync::mpsc::channel();
    let payloads = [first.clone(), second.clone()];
    let server = thread::spawn(move || {
        for (index, bytes) in payloads.into_iter().enumerate() {
            let (mut socket, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut line = String::new();
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
            }
            if index == 0 {
                write!(socket, "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n", bytes.len()).unwrap();
                socket.write_all(&bytes).unwrap();
                socket.write_all(b"\r\n0\r\n\r\n").unwrap();
            } else {
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                )
                .unwrap();
                socket.write_all(&bytes[..4]).unwrap();
                wait.recv_timeout(Duration::from_secs(5)).unwrap();
                socket.write_all(&bytes[4..]).unwrap();
            }
        }
    });
    let root = tempfile::tempdir().unwrap();
    let mut declaration = serde_json::json!({"name":"runtime", "artifacts":{"payload":{
        "id":"@test:runtime/payload", "version":"v1", "files":[
            {"url":format!("http://{address}/first"), "sha256":format!("sha256:{}", hex::encode(Sha256::digest(&first))), "bytes":first.len()},
            {"url":format!("http://{address}/second"), "sha256":format!("sha256:{}", hex::encode(Sha256::digest(&second)))},
        ]
    }}});
    if !declare_first_size {
        declaration["artifacts"]["payload"]["files"][0]
            .as_object_mut()
            .unwrap()
            .remove("bytes");
    }
    let plugin =
        crate::plugin::decode_plugin_declaration("@test", &declaration.to_string()).unwrap();
    let registry = PluginRegistry::new(vec![plugin], ["@test:runtime".into()]).unwrap();
    let coordinator = AcquisitionCoordinator {
        engine: download::DownloadEngine::for_test(Duration::from_secs(5)),
        ..Default::default()
    };
    coordinator
        .start(
            root.path(),
            root.path(),
            &registry,
            &["@test:runtime/payload".into()],
        )
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ArtifactState::Downloading {
                file: 2,
                file_count,
                bytes_done,
                bytes_total,
                ..
            } = state(&coordinator, root.path(), &registry)
            {
                if bytes_done == first.len() as u64 + 4 {
                    assert_eq!(file_count, 2);
                    assert_eq!(bytes_total, Some((first.len() + second.len()) as u64));
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    release.send(()).unwrap();
    assert!(matches!(
        settled(&coordinator, root.path(), &registry).await,
        ArtifactState::Ready { .. }
    ));
    server.join().unwrap();
    assert_eq!(
        fs::read(root.path().join("cache/runtime/payload/v1/first")).unwrap(),
        first
    );
    assert_eq!(
        fs::read(root.path().join("cache/runtime/payload/v1/second")).unwrap(),
        second
    );
}
