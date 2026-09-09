use super::*;
use serde_json::json;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

#[test]
fn pipe_accepts_fragmented_and_coalesced_nul_frames() {
    let (read, mut write) = UnixStream::pair().unwrap();
    let mut pipe = transport_input(read);
    write.write_all(b"{\"id\":").unwrap();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        write
            .write_all(b"1,\"result\":{}}\0{\"method\":\"Page.loadEventFired\"}\0")
            .unwrap();
    });
    assert_eq!(pipe.receive(deadline()).unwrap()["id"], 1);
    assert_eq!(
        pipe.receive(deadline()).unwrap()["method"],
        "Page.loadEventFired"
    );
}

#[test]
fn pipe_bounds_frames_and_deadlines_and_reports_truncated_eof() {
    let mut oversized = serde_json::to_vec(&json!("x".repeat(pipe::MAX_FRAME))).unwrap();
    oversized.push(0);
    for bytes in [oversized, b"{\"id\":1}".to_vec()] {
        let (read, mut write) = UnixStream::pair().unwrap();
        let worker = std::thread::spawn(move || {
            let _ = write.write_all(&bytes);
        });
        let mut pipe = transport_input(read);
        assert_eq!(pipe.receive(deadline()), Err(Error::Protocol));
        drop(pipe);
        worker.join().unwrap();
    }
    let (read, _write) = UnixStream::pair().unwrap();
    let mut pipe = transport_input(read);
    assert_eq!(
        pipe.receive(Instant::now() + Duration::from_millis(20)),
        Err(Error::Timeout)
    );
}

#[test]
fn malformed_protocol_never_echoes_payload() {
    let secret = "a".repeat(64);
    let (read, mut write) = UnixStream::pair().unwrap();
    let mut pipe = transport_input(read);
    write
        .write_all(format!("{{bad {secret}}}\0").as_bytes())
        .unwrap();
    let error = pipe.receive(deadline()).unwrap_err();
    assert!(!format!("{error:?} {error}").contains(&secret));
    assert_eq!(error, Error::Protocol);
}

#[test]
fn cdp_matches_ids_and_accepts_interleaved_events() {
    let mut pending = cdp::Pending::default();
    pending.insert(1, "session", 10).unwrap();
    pending.insert(2, "session", 20).unwrap();
    assert!(matches!(
        pending
            .accept(json!({"method":"Page.loadEventFired","sessionId":"session","params":{}}))
            .unwrap(),
        cdp::Message::Event(_)
    ));
    let cdp::Message::Response { action, .. } = pending
        .accept(json!({"id":2,"sessionId":"session","result":{}}))
        .unwrap()
    else {
        panic!("response expected")
    };
    assert_eq!(action, 20);
    assert!(
        pending
            .accept(json!({"id":2,"sessionId":"session","result":{}}))
            .is_err()
    );
    assert!(
        pending
            .accept(json!({"id":1,"sessionId":"other","result":{}}))
            .is_err()
    );
}

#[test]
fn cdp_errors_are_redacted_and_missing_results_fail() {
    for response in [
        json!({"id":1,"error":{"message":"secret token"}}),
        json!({"id":1}),
    ] {
        let mut pending = cdp::Pending::default();
        pending.insert(1, "", ()).unwrap();
        let error = pending.accept(response).err().unwrap();
        assert_eq!(error, Error::Protocol);
        assert!(!format!("{error:?} {error}").contains("secret token"));
    }
}

#[test]
fn only_the_exact_trusted_top_frame_fetch_is_authorized() {
    let mut trust = kiosk::Trust::new("top".into());
    trust.navigated(&json!({"id":"top","url":PORTAL_URL,"securityOrigin":PORTAL_ORIGIN}));
    let request = json!({"frameId":"top","resourceType":"Fetch","request":{"url":RUNTIME_URL,"method":"GET"}});
    let tree =
        json!({"frameTree":{"frame":{"id":"top","url":PORTAL_URL,"securityOrigin":PORTAL_ORIGIN}}});
    assert!(trust.authorizes(&request, &tree));
    for url in [
        "http://127.0.0.1:8099/runtime.json?x",
        "http://127.0.0.1:8099.evil/runtime.json",
        "http://localhost:8099/runtime.json",
        "https://127.0.0.1:8099/runtime.json",
    ] {
        let mut other = request.clone();
        other["request"]["url"] = json!(url);
        assert!(!trust.authorizes(&other, &tree));
    }
    let mut subframe = request.clone();
    subframe["frameId"] = json!("hostile-child");
    assert!(!trust.authorizes(&subframe, &tree));
    let mut document = request.clone();
    document["resourceType"] = json!("Document");
    assert!(!trust.authorizes(&document, &tree));
    let mut hostile_tree = tree.clone();
    hostile_tree["frameTree"]["frame"]["url"] = json!("http://attacker.invalid/");
    assert!(!trust.authorizes(&request, &hostile_tree));
    trust.navigated(&json!({"id":"top","url":"http://attacker.invalid/","securityOrigin":"http://attacker.invalid"}));
    assert!(!trust.authorizes(&request, &tree));
    trust.navigated(&json!({"id":"top","url":PORTAL_URL,"securityOrigin":PORTAL_ORIGIN}));
    assert!(
        !trust.authorizes(&request, &tree),
        "navigation away permanently revokes this session"
    );
}

#[test]
fn config_validation_matches_the_portal_contract_and_redacts_errors() {
    let good = json!({"korridPort":45231,"korridCapability":"a".repeat(64)});
    assert!(runtime::validate(good.clone(), None).is_ok());
    for port in [
        json!(0),
        json!(65536),
        json!(1.5),
        json!("8099"),
        json!(null),
    ] {
        let mut bad = good.clone();
        bad["korridPort"] = port;
        assert!(runtime::validate(bad, None).is_err());
    }
    for token in ["A".repeat(64), "a".repeat(63), "secret token".into()] {
        let mut bad = good.clone();
        bad["korridCapability"] = json!(token);
        let error = runtime::validate(bad, None).unwrap_err();
        assert!(!format!("{error:?} {error}").contains(&token));
    }
    assert!(runtime::validate(good.clone(), Some("")).is_err());
    assert_eq!(
        runtime::validate(good, Some("pico")).unwrap()["surfaceId"],
        "pico"
    );
}

#[test]
fn runtime_reopens_the_brain_file_after_atomic_replacement() {
    let directory =
        std::env::temp_dir().join(format!("korri-kiosk-runtime-test-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("brain.json");
    let first = json!({"korridPort":1234,"korridCapability":"a".repeat(64)});
    let second = json!({"korridPort":4321,"korridCapability":"b".repeat(64)});
    std::fs::write(&path, first.to_string()).unwrap();
    assert_eq!(runtime::load(&path, None).unwrap(), first);
    let replacement = directory.join("replacement");
    std::fs::write(&replacement, second.to_string()).unwrap();
    std::fs::rename(replacement, &path).unwrap();
    assert_eq!(runtime::load(&path, None).unwrap(), second);
    std::fs::write(&path, vec![b'x'; runtime::MAX_CONFIG + 1]).unwrap();
    assert!(runtime::load(&path, None).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn base64_matches_cdp_wire_encoding() {
    for (plain, encoded) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(runtime::base64(plain.as_bytes()), encoded);
    }
}

#[test]
fn pipe_writes_nul_framed_json_and_bounds_backpressure() {
    use std::io::Read;
    let (output, mut reader) = UnixStream::pair().unwrap();
    let (input, _writer) = UnixStream::pair().unwrap();
    let mut transport =
        pipe::Transport::new(OwnedFd::from(input).into(), OwnedFd::from(output).into()).unwrap();
    transport.send(&json!({"id":1}), deadline()).unwrap();
    let mut bytes = [0; 9];
    reader.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"{\"id\":1}\0");
    let result = transport.send(
        &json!({"body":"x".repeat(pipe::MAX_FRAME - 100)}),
        Instant::now() + Duration::from_millis(20),
    );
    assert_eq!(result, Err(Error::Timeout));
}

#[test]
fn cdp_bounds_outstanding_commands() {
    let mut pending = cdp::Pending::default();
    for id in 0..128 {
        pending.insert(id, "", ()).unwrap();
    }
    assert_eq!(pending.insert(128, "", ()), Err(Error::Protocol));
}

#[test]
fn runtime_refuses_symlinks_and_nonregular_files_without_blocking() {
    use std::os::unix::fs::symlink;
    let directory =
        std::env::temp_dir().join(format!("korri-kiosk-file-test-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let target = directory.join("target");
    let link = directory.join("link");
    std::fs::write(
        &target,
        json!({"korridPort":1234,"korridCapability":"a".repeat(64)}).to_string(),
    )
    .unwrap();
    symlink(&target, &link).unwrap();
    assert!(runtime::load(&link, None).is_err());
    assert!(runtime::load(&directory, None).is_err());
    let fifo = directory.join("fifo");
    let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert_eq!(runtime::load(&fifo, None), Err(Error::Configuration));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn launcher_rejects_unsafe_parent_and_cleans_profile_on_spawn_failure() {
    use std::os::unix::fs::PermissionsExt;
    let directory =
        std::env::temp_dir().join(format!("korri-kiosk-profile-test-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();
    let options = Options {
        chromium: directory.join("absent-chromium"),
        profile_parent: directory.clone(),
        brain_file: directory.join("brain.json"),
        surface_id: None,
        headless: true,
    };
    assert!(matches!(
        browser::Browser::spawn(&options),
        Err(Error::Configuration)
    ));
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        browser::Browser::spawn(&options),
        Err(Error::Browser)
    ));
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn launcher_cleans_stale_profiles_and_preserves_unrelated_parent_entries() {
    let options = profile_options("stale");
    let parent = &options.profile_parent;
    for name in ["chromium-123", &format!("chromium-{}", std::process::id())] {
        let stale = parent.join(name);
        std::fs::create_dir_all(stale.join("Default")).unwrap();
        std::fs::write(stale.join("Default/Cookies"), "private").unwrap();
    }
    for name in ["keep", "chromium-", "chromium-old", "chromium-123-extra"] {
        std::fs::write(parent.join(name), "unrelated").unwrap();
    }
    std::fs::create_dir(parent.join("other-directory")).unwrap();
    assert!(matches!(
        browser::Browser::spawn(&options),
        Err(Error::Browser)
    ));
    assert!(!parent.join("chromium-123").exists());
    assert!(
        !parent
            .join(format!("chromium-{}", std::process::id()))
            .exists()
    );
    for name in ["keep", "chromium-", "chromium-old", "chromium-123-extra"] {
        assert_eq!(
            std::fs::read_to_string(parent.join(name)).unwrap(),
            "unrelated"
        );
    }
    assert!(parent.join("other-directory").is_dir());
    assert_eq!(std::fs::read_dir(parent).unwrap().count(), 5);
    std::fs::remove_dir_all(parent).unwrap();
}

#[test]
fn launcher_rejects_matching_symlinks_and_files_without_following_them() {
    use std::os::unix::fs::symlink;
    let options = profile_options("reject-link");
    let parent = &options.profile_parent;
    let target = parent.join("unrelated-target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), "private").unwrap();
    let stale = parent.join("chromium-123");
    symlink(&target, &stale).unwrap();
    assert!(matches!(
        browser::Browser::spawn(&options),
        Err(Error::Configuration)
    ));
    assert!(std::fs::symlink_metadata(&stale).unwrap().is_symlink());
    assert_eq!(
        std::fs::read_to_string(target.join("keep")).unwrap(),
        "private"
    );
    std::fs::remove_file(&stale).unwrap();
    std::fs::write(&stale, "not a directory").unwrap();
    assert!(matches!(
        browser::Browser::spawn(&options),
        Err(Error::Configuration)
    ));
    assert_eq!(std::fs::read_to_string(&stale).unwrap(), "not a directory");
    std::fs::remove_dir_all(parent).unwrap();
}

#[test]
fn launcher_stale_cleanup_does_not_follow_nested_symlinks() {
    use std::os::unix::fs::symlink;
    let options = profile_options("nested-link");
    let parent = &options.profile_parent;
    let target = parent.join("unrelated-target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), "private").unwrap();
    let stale = parent.join("chromium-123");
    std::fs::create_dir(&stale).unwrap();
    symlink(&target, stale.join("link")).unwrap();
    assert!(matches!(
        browser::Browser::spawn(&options),
        Err(Error::Browser)
    ));
    assert!(!stale.exists());
    assert_eq!(
        std::fs::read_to_string(target.join("keep")).unwrap(),
        "private"
    );
    std::fs::remove_dir_all(parent).unwrap();
}

#[test]
fn launcher_busy_parent_lock_refuses_stale_cleanup() {
    use std::os::fd::AsRawFd;
    let options = profile_options("busy-lock");
    let parent = &options.profile_parent;
    let stale = parent.join("chromium-123");
    std::fs::create_dir(&stale).unwrap();
    std::fs::write(stale.join("keep"), "active").unwrap();
    let lock = std::fs::File::open(parent).unwrap();
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    assert!(matches!(
        browser::Browser::spawn(&options),
        Err(Error::Configuration)
    ));
    assert_eq!(
        std::fs::read_to_string(stale.join("keep")).unwrap(),
        "active"
    );
    assert_eq!(std::fs::read_dir(parent).unwrap().count(), 1);
    drop(lock);
    assert!(matches!(
        browser::Browser::spawn(&options),
        Err(Error::Browser)
    ));
    assert!(parent.is_dir());
    assert_eq!(std::fs::read_dir(parent).unwrap().count(), 0);
    std::fs::remove_dir(parent).unwrap();
}

fn profile_options(case: &str) -> Options {
    use std::os::unix::fs::DirBuilderExt;
    let parent = std::env::temp_dir().join(format!(
        "korri-kiosk-profile-{case}-test-{}",
        std::process::id()
    ));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&parent)
        .unwrap();
    Options {
        chromium: parent.join("absent-chromium"),
        brain_file: parent.join("brain.json"),
        profile_parent: parent,
        surface_id: None,
        headless: true,
    }
}

#[test]
fn launch_options_reject_arbitrary_flags_and_nonabsolute_paths_without_echoing() {
    let valid = [
        "--chromium",
        "/trusted/chromium",
        "--profile-parent",
        "/private/profile",
    ];
    assert!(Options::parse(valid.map(str::to_owned)).is_ok());
    for option in [
        "--remote-debugging-port=9222",
        "--no-sandbox",
        "--secret-token-value",
    ] {
        let mut args: Vec<_> = valid.into_iter().map(str::to_owned).collect();
        args.push(option.into());
        let error = Options::parse(args).err().unwrap();
        assert_eq!(error, Error::Configuration);
        assert!(!error.to_string().contains(option));
    }
    assert!(
        Options::parse(
            ["--chromium", "relative", "--profile-parent", "/private"].map(str::to_owned)
        )
        .is_err()
    );
}

fn transport_input(input: UnixStream) -> pipe::Transport {
    let (output, _sink) = UnixStream::pair().unwrap();
    pipe::Transport::new(OwnedFd::from(input).into(), OwnedFd::from(output).into()).unwrap()
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(2)
}
