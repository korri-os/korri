use super::*;
use std::{
    io::{Read, Write},
    os::unix::net::UnixListener,
    sync::mpsc,
};

fn response(stream: &mut UnixStream, kind: u32, body: &[u8]) {
    stream.write_all(b"i3-ipc").unwrap();
    stream
        .write_all(&(body.len() as u32).to_le_bytes())
        .unwrap();
    stream.write_all(&kind.to_le_bytes()).unwrap();
    stream.write_all(body).unwrap();
}

fn request(stream: &mut UnixStream, kind: u32) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut header = [0; 14];
    stream.read_exact(&mut header).unwrap();
    assert_eq!(&header[..6], b"i3-ipc");
    assert_eq!(u32::from_le_bytes(header[10..].try_into().unwrap()), kind);
    let mut payload = vec![0; u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize];
    stream.read_exact(&mut payload).unwrap();
}

#[test]
fn unchanged_reads_do_no_ipc_and_an_event_refreshes_the_tree() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sway.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (event, send_event) = mpsc::channel();
    let (checked, idle_checked) = mpsc::channel();
    let (release, hold) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut events, _) = listener.accept().unwrap();
        request(&mut events, 2);
        response(&mut events, 2, br#"{"success":true}"#);
        let (mut queries, _) = listener.accept().unwrap();
        request(&mut queries, 4);
        response(&mut queries, 4, br#"{"id":1}"#);
        queries
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut byte = [0];
        let error = queries.read(&mut byte).unwrap_err();
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ));
        checked.send(()).unwrap();
        send_event.recv_timeout(Duration::from_secs(2)).unwrap();
        response(&mut events, (1 << 31) | 3, b"{}");
        request(&mut queries, 4);
        response(&mut queries, 4, br#"{"id":2}"#);
        hold.recv_timeout(Duration::from_secs(2)).unwrap();
    });
    let observer = SwayObservation::start(path, Duration::from_secs(1)).unwrap();
    let mut changes = observer.subscribe();
    for _ in 0..20 {
        assert_eq!(&*observer.tree().unwrap(), r#"{"id":1}"#);
    }
    idle_checked.recv_timeout(Duration::from_secs(2)).unwrap();
    changes.borrow_and_update();
    event.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if observer.tree().as_deref() == Ok(r#"{"id":2}"#) {
            break;
        }
        assert!(Instant::now() < deadline, "event never refreshed the tree");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(changes.has_changed().unwrap());
    let started = Instant::now();
    drop(observer);
    assert!(started.elapsed() < Duration::from_secs(1));
    release.send(()).unwrap();
    server.join().unwrap();
}

#[test]
fn reconnect_subscribes_again_and_reads_a_new_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sway.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (advance, commands) = mpsc::channel();
    let server = std::thread::spawn(move || {
        for tree in [br#"{"id":1}"#, br#"{"id":2}"#] {
            let (mut events, _) = listener.accept().unwrap();
            request(&mut events, 2);
            response(&mut events, 2, br#"{"success":true}"#);
            let (mut queries, _) = listener.accept().unwrap();
            request(&mut queries, 4);
            response(&mut queries, 4, tree);
            commands.recv_timeout(Duration::from_secs(2)).unwrap();
        }
    });
    let observer = SwayObservation::start(path, Duration::from_secs(1)).unwrap();
    assert_eq!(&*observer.tree().unwrap(), r#"{"id":1}"#);
    advance.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if observer.tree().as_deref() == Ok(r#"{"id":2}"#) {
            break;
        }
        assert!(Instant::now() < deadline, "reconnect did not resync");
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(observer);
    advance.send(()).unwrap();
    server.join().unwrap();
}

#[test]
fn a_disconnected_feed_never_keeps_a_successful_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sway.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (disconnect, stop) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut events, _) = listener.accept().unwrap();
        request(&mut events, 2);
        response(&mut events, 2, br#"{"success":true}"#);
        let (mut queries, _) = listener.accept().unwrap();
        request(&mut queries, 4);
        response(&mut queries, 4, br#"{"id":1}"#);
        stop.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(events);
        drop(queries);
    });
    let observer = SwayObservation::start(path, Duration::from_secs(1)).unwrap();
    assert!(observer.tree().is_ok());
    disconnect.send(()).unwrap();
    server.join().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while observer.tree().is_ok() {
        assert!(
            Instant::now() < deadline,
            "disconnected feed kept a stale tree"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(observer);
}
