//! The download queue (ADR 0013): one job at a time, launch first, Cancel
//! deletes partial bytes, and the queue comes back from `.part` files.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

const SLOW: usize = 64 * 1024;
const FIRST: usize = 16 * 1024;

/// Serves `/slow` (sends FIRST bytes, then waits for the gate unless the
/// request resumes with Range) and `/a`, `/b` at once. Logs path and offset.
type RequestLog = Arc<Mutex<Vec<(String, Option<usize>)>>>;

struct Server {
    address: std::net::SocketAddr,
    log: RequestLog,
    gate: Arc<AtomicBool>,
    slow: Vec<u8>,
}
impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let gate = Arc::new(AtomicBool::new(false));
        let slow = vec![b's'; SLOW];
        let (requests, open, body) = (log.clone(), gate.clone(), slow.clone());
        thread::spawn(move || {
            for socket in listener.incoming() {
                let Ok(mut socket) = socket else { break };
                let (requests, open, body) = (requests.clone(), open.clone(), body.clone());
                thread::spawn(move || {
                    let mut reader = BufReader::new(socket.try_clone().unwrap());
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let path = line.split_whitespace().nth(1).unwrap_or("").to_owned();
                    let mut offset = None;
                    loop {
                        line.clear();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                        if let Some(range) = line.to_lowercase().strip_prefix("range: bytes=") {
                            offset = range.trim().trim_end_matches('-').parse::<usize>().ok();
                        }
                    }
                    requests.lock().unwrap().push((path.clone(), offset));
                    let bytes = if path == "/slow" {
                        body
                    } else {
                        path.as_bytes().to_vec()
                    };
                    let start = offset.unwrap_or(0);
                    let _ = if offset.is_some() {
                        write!(socket, "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {}-{}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", start, bytes.len() - 1, bytes.len(), bytes.len() - start)
                    } else {
                        write!(
                            socket,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            bytes.len()
                        )
                    };
                    if path == "/slow" && offset.is_none() {
                        let _ = socket.write_all(&bytes[..FIRST]);
                        let _ = socket.flush();
                        while !open.load(Ordering::SeqCst) {
                            thread::sleep(Duration::from_millis(5));
                        }
                        let _ = socket.write_all(&bytes[FIRST..]);
                    } else {
                        let _ = socket.write_all(&bytes[start..]);
                    }
                });
            }
        });
        Self {
            address,
            log,
            gate,
            slow,
        }
    }
    fn paths(&self) -> Vec<(String, Option<usize>)> {
        self.log.lock().unwrap().clone()
    }
}

fn registry(server: &Server) -> PluginRegistry {
    let file = |path: &str, bytes: &[u8]| {
        serde_json::json!([{"url": format!("http://{}{path}", server.address),
            "sha256": format!("sha256:{}", hex::encode(Sha256::digest(bytes)))}])
    };
    let declaration = serde_json::json!({"name":"queue", "artifacts":{
        "slow":{"id":"@test:queue/slow", "version":"v1", "files":file("/slow", &server.slow)},
        "a":{"id":"@test:queue/a", "version":"v1", "files":file("/a", b"/a")},
        "b":{"id":"@test:queue/b", "version":"v1", "files":file("/b", b"/b")},
        "multi":{"id":"@test:queue/multi", "version":"v1", "files":[
            file("/m1", b"/m1")[0].clone(), file("/m2", b"/m2")[0].clone(),
        ]},
    }});
    let plugin =
        crate::plugin::decode_plugin_declaration("@test", &declaration.to_string()).unwrap();
    PluginRegistry::new(vec![plugin], ["@test:queue".into()]).unwrap()
}

fn coordinator() -> (AcquisitionCoordinator, Arc<sleep::InMemoryBlocks>) {
    let blocks = Arc::new(sleep::InMemoryBlocks::default());
    (
        AcquisitionCoordinator {
            engine: download::DownloadEngine::for_test(Duration::from_secs(5)),
            sleep: sleep::SleepBlock::InMemory(blocks.clone()),
            ..Default::default()
        },
        blocks,
    )
}

fn state_of(
    coordinator: &AcquisitionCoordinator,
    root: &Path,
    registry: &PluginRegistry,
    id: &str,
) -> ArtifactState {
    coordinator
        .status(root, registry, Some(&[id.to_owned()]))
        .unwrap()
        .artifacts
        .remove(0)
        .state
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

fn background(ids: &[&str]) -> GameArtifacts {
    GameArtifacts {
        ids: ids.iter().map(|id| (*id).to_owned()).collect(),
        release: BTreeSet::new(),
    }
}

fn part(root: &Path, registry: &PluginRegistry, id: &str) -> PathBuf {
    let artifact = registry.artifact(id).unwrap();
    let hex = artifact.files[0].sha256.trim_start_matches("sha256:");
    placement::contribution(root, artifact)
        .join(".downloads")
        .join(format!("{hex}.part"))
}

#[tokio::test]
async fn one_job_at_a_time_and_a_launch_goes_first_while_the_background_job_pauses() {
    let server = Server::start();
    let registry = registry(&server);
    let root = tempfile::tempdir().unwrap();
    let (coordinator, blocks) = coordinator();
    let (slow, a, b) = ("@test:queue/slow", "@test:queue/a", "@test:queue/b");
    let snapshot = coordinator
        .download(root.path(), root.path(), &registry, &background(&[slow, b]))
        .unwrap();
    // Queueing creates each marker before any job runs.
    assert!(part(root.path(), &registry, slow).exists());
    assert!(part(root.path(), &registry, b).exists());
    assert!(snapshot.artifacts.iter().all(|artifact| matches!(
        artifact.state,
        ArtifactState::Queued | ArtifactState::Downloading { .. }
    )));
    until("the first bytes of the slow job", || {
        matches!(state_of(&coordinator, root.path(), &registry, slow),
            ArtifactState::Downloading { bytes_done, .. } if bytes_done >= FIRST as u64)
    })
    .await;
    // One at a time: the second background job waits.
    assert!(matches!(
        state_of(&coordinator, root.path(), &registry, b),
        ArtifactState::Queued
    ));
    assert_eq!(server.paths(), [("/slow".to_owned(), None)]);
    assert_eq!(blocks.held.load(Ordering::SeqCst), 1);

    // A launch waits for `a`: it runs first and the slow job pauses.
    coordinator
        .start(root.path(), root.path(), &registry, &[a.to_owned()])
        .unwrap();
    until("the launch artifact", || {
        matches!(
            state_of(&coordinator, root.path(), &registry, a),
            ArtifactState::Ready { .. }
        )
    })
    .await;
    // The paused job resumes with Range, before the older background job.
    server.gate.store(true, Ordering::SeqCst);
    until("every job", || {
        [slow, b].iter().all(|id| {
            matches!(
                state_of(&coordinator, root.path(), &registry, id),
                ArtifactState::Ready { .. }
            )
        })
    })
    .await;
    until("the worker to stop", || {
        let queue = coordinator.lock();
        !queue.worker && queue.running.is_none()
    })
    .await;
    let paths = server.paths();
    assert_eq!(paths[0], ("/slow".to_owned(), None));
    assert_eq!(paths[1], ("/a".to_owned(), None));
    assert_eq!(paths[2].0, "/slow");
    assert!(paths[2]
        .1
        .is_some_and(|offset| (FIRST..SLOW).contains(&offset)));
    assert_eq!(paths[3], ("/b".to_owned(), None));
    let artifact = registry.artifact(slow).unwrap();
    assert_eq!(
        fs::read(placement::ready_path(root.path(), artifact).join("slow")).unwrap(),
        server.slow
    );
    // The sleep block covered the whole queue and is released when idle.
    assert_eq!(blocks.held.load(Ordering::SeqCst), 0);
    assert_eq!(blocks.taken.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancel_deletes_partial_files_of_a_running_or_queued_job_and_leaves_the_queue() {
    let server = Server::start();
    let registry = registry(&server);
    let root = tempfile::tempdir().unwrap();
    let (coordinator, _) = coordinator();
    let (slow, b) = ("@test:queue/slow", "@test:queue/b");
    coordinator
        .download(root.path(), root.path(), &registry, &background(&[slow, b]))
        .unwrap();
    until("partial bytes", || {
        fs::metadata(part(root.path(), &registry, slow)).is_ok_and(|m| m.len() >= FIRST as u64)
    })
    .await;
    coordinator.cancel(root.path(), &registry, b).await.unwrap();
    assert!(!part(root.path(), &registry, b).exists());
    assert!(matches!(
        state_of(&coordinator, root.path(), &registry, b),
        ArtifactState::Missing
    ));
    coordinator
        .cancel(root.path(), &registry, slow)
        .await
        .unwrap();
    // Cancel deletes every downloaded file, so it never looks interrupted.
    let base = placement::contribution(root.path(), registry.artifact(slow).unwrap());
    assert!(!base.join(".downloads").exists());
    assert!(placement::interrupted_since(&base).is_none());
    assert!(matches!(
        state_of(&coordinator, root.path(), &registry, slow),
        ArtifactState::Missing
    ));
    until("the worker to stop", || {
        let queue = coordinator.lock();
        !queue.worker && queue.running.is_none() && queue.waiting.is_empty()
    })
    .await;
    // b never started: cancelled before its turn.
    assert!(server.paths().iter().all(|(path, _)| path != "/b"));
    server.gate.store(true, Ordering::SeqCst);
}

/// No runtime: recovery rebuilds the queue, and the first request starts it.
#[test]
fn restart_queues_every_interrupted_download_by_when_it_was_queued() {
    let server = Server::start();
    let registry = registry(&server);
    let root = tempfile::tempdir().unwrap();
    let (slow, a, b) = ("@test:queue/slow", "@test:queue/a", "@test:queue/b");
    for id in [slow, b] {
        placement::mark_queued(root.path(), registry.artifact(id).unwrap()).unwrap();
        thread::sleep(Duration::from_millis(20));
    }
    // The running job's .part grows after b was queued; its mtime is newer.
    fs::OpenOptions::new()
        .append(true)
        .open(part(root.path(), &registry, slow))
        .unwrap()
        .write_all(&server.slow[..FIRST])
        .unwrap();
    // A ready contribution is not interrupted, whatever its .downloads holds.
    let ready = registry.artifact(a).unwrap();
    placement::mark_queued(root.path(), ready).unwrap();
    fs::create_dir(placement::ready_path(root.path(), ready)).unwrap();

    let (coordinator, _) = coordinator();
    coordinator
        .recover(root.path(), root.path(), &registry)
        .unwrap();
    let queue = coordinator.lock();
    let waiting: Vec<_> = queue.waiting.iter().cloned().collect();
    let base = |id| placement::contribution(root.path(), registry.artifact(id).unwrap());
    assert_eq!(waiting, [base(slow), base(b)]);
    assert!(!queue.worker);
    assert!(matches!(
        queue.jobs[&base(slow)].state,
        ArtifactState::Queued
    ));
    // The marker kept its partial bytes for the Range resume.
    assert_eq!(
        fs::metadata(part(root.path(), &registry, slow))
            .unwrap()
            .len(),
        FIRST as u64
    );
    assert!(!part(root.path(), &registry, a).exists());
    server.gate.store(true, Ordering::SeqCst);
}

#[tokio::test]
async fn a_restarted_queue_resumes_with_range_once_a_runtime_exists() {
    let server = Server::start();
    let registry = registry(&server);
    let root = tempfile::tempdir().unwrap();
    let slow = "@test:queue/slow";
    placement::mark_queued(root.path(), registry.artifact(slow).unwrap()).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(part(root.path(), &registry, slow))
        .unwrap()
        .write_all(&server.slow[..FIRST])
        .unwrap();
    let (coordinator, _) = coordinator();
    coordinator
        .recover(root.path(), root.path(), &registry)
        .unwrap();
    until("the resumed download", || {
        matches!(
            state_of(&coordinator, root.path(), &registry, slow),
            ArtifactState::Ready { .. }
        )
    })
    .await;
    assert_eq!(server.paths(), [("/slow".to_owned(), Some(FIRST))]);
}

fn hex_of(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// FEX's rootfs has eleven files. Once file 1 is verified its marker is
/// renamed, and the earliest remaining `.part` belongs to a later file.
#[test]
fn restart_order_survives_a_finished_first_file_of_a_multi_file_artifact() {
    let server = Server::start();
    let registry = registry(&server);
    let root = tempfile::tempdir().unwrap();
    let (multi, b) = ("@test:queue/multi", "@test:queue/b");
    placement::mark_queued(root.path(), registry.artifact(multi).unwrap()).unwrap();
    thread::sleep(Duration::from_millis(20));
    placement::mark_queued(root.path(), registry.artifact(b).unwrap()).unwrap();
    thread::sleep(Duration::from_millis(20));
    // File 1 finishes and file 2 starts after b was queued.
    let downloads =
        placement::contribution(root.path(), registry.artifact(multi).unwrap()).join(".downloads");
    fs::rename(
        downloads.join(format!("{}.part", hex_of(b"/m1"))),
        downloads.join(hex_of(b"/m1")),
    )
    .unwrap();
    fs::write(downloads.join(format!("{}.part", hex_of(b"/m2"))), b"/").unwrap();
    let (coordinator, _) = coordinator();
    coordinator
        .recover(root.path(), root.path(), &registry)
        .unwrap();
    let base = |id| placement::contribution(root.path(), registry.artifact(id).unwrap());
    let waiting: Vec<_> = coordinator.lock().waiting.iter().cloned().collect();
    assert_eq!(waiting, [base(multi), base(b)]);
    server.gate.store(true, Ordering::SeqCst);
}

/// A restart between two files, or during unpacking, leaves verified files
/// and no `.part`. The download still resumes and fetches only what is left.
#[tokio::test]
async fn a_restart_between_files_or_during_unpacking_still_resumes() {
    let server = Server::start();
    let registry = registry(&server);
    let root = tempfile::tempdir().unwrap();
    let (multi, a) = ("@test:queue/multi", "@test:queue/a");
    // `multi` stopped between its two files; `a` stopped during unpacking.
    for (id, bytes) in [(multi, &b"/m1"[..]), (a, &b"/a"[..])] {
        let artifact = registry.artifact(id).unwrap();
        let downloads = placement::contribution(root.path(), artifact).join(".downloads");
        fs::create_dir_all(&downloads).unwrap();
        fs::write(downloads.join(hex_of(bytes)), bytes).unwrap();
    }
    let (coordinator, _) = coordinator();
    coordinator
        .recover(root.path(), root.path(), &registry)
        .unwrap();
    until("both downloads", || {
        [multi, a].iter().all(|id| {
            matches!(
                state_of(&coordinator, root.path(), &registry, id),
                ArtifactState::Ready { .. }
            )
        })
    })
    .await;
    // Only the missing second file of `multi` went to the network.
    assert_eq!(server.paths(), [("/m2".to_owned(), None)]);
    server.gate.store(true, Ordering::SeqCst);
}
