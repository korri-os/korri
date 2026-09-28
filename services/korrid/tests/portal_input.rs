//! Real loopback WebSockets; no device, inputd, browser injection, or IPC stub.
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use korrid::{
    native_input::*,
    portal_access::{PortalAccess, PortalPermission},
    portal_input::{self, PortalInputLimits, PortalInputSource, PublishError},
};
use serde_json::{json, Value};
use tokio::{net::TcpStream, task::JoinHandle, time::timeout};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::StatusCode, Error, Message},
    MaybeTlsStream, WebSocketStream,
};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
const CAPABILITY: &str = "native-input-test-capability";
const ORIGIN: &str = "http://127.0.0.1:8099";
const WAIT: Duration = Duration::from_secs(3);

struct Server {
    url: String,
    task: JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn limits() -> PortalInputLimits {
    PortalInputLimits {
        handshake_timeout: Duration::from_millis(500),
        max_queue_age: Duration::from_secs(2),
        ..PortalInputLimits::default()
    }
}

async fn server_with(
    limits: PortalInputLimits,
    permission: PortalPermission,
) -> (Server, PortalInputSource) {
    let (router, source) =
        portal_input::router(PortalAccess::new(CAPABILITY, ORIGIN, permission), limits);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (Server { url, task }, source)
}

async fn server() -> (Server, PortalInputSource) {
    server_with(limits(), PortalPermission::LocalSessions).await
}

async fn connect(server: &Server) -> Socket {
    let mut request = server.url.clone().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Origin", ORIGIN.parse().unwrap());
    timeout(WAIT, connect_async(request))
        .await
        .unwrap()
        .unwrap()
        .0
}

async fn text(socket: &mut Socket, value: impl Into<String>) {
    socket
        .send(Message::Text(value.into().into()))
        .await
        .unwrap();
}

async fn authenticate(socket: &mut Socket, classes: &[&str]) {
    text(socket, format!("Bearer {CAPABILITY}")).await;
    text(socket, json!({"classes": classes}).to_string()).await;
}

async fn event(socket: &mut Socket) -> Value {
    let message = timeout(WAIT, socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Text(text) = message else {
        panic!("expected event text")
    };
    serde_json::from_str(&text).unwrap()
}

async fn silent(socket: &mut Socket) {
    assert!(
        timeout(Duration::from_millis(25), socket.next())
            .await
            .is_err(),
        "socket sent data before expected"
    );
}

async fn closed(socket: &mut Socket) {
    match timeout(WAIT, socket.next())
        .await
        .expect("socket must close")
    {
        None | Some(Err(_)) | Some(Ok(Message::Close(_))) => {}
        _ => panic!("closed connection must not drain queued events"),
    }
}

fn decode(value: Value) -> NativeInputEvent {
    serde_json::from_value(value).unwrap()
}

fn added(id: &str, class: &str) -> NativeInputEvent {
    decode(json!({"kind":"device-added","device":{
        "deviceId":id,"class":class,"name":"Test pad","capabilities":["EV_KEY","EV_ABS"],
        "axes":[{"code":0,"minimum":-32768,"maximum":32767,"flat":128}]
    }}))
}

fn input(id: &str, class: &str, value: f64) -> NativeInputEvent {
    decode(
        json!({"kind":"input","deviceId":id,"class":class,"type":1,"code":304,"value":value,"timestamp":42}),
    )
}

fn removed(id: &str) -> NativeInputEvent {
    decode(json!({"kind":"device-removed","deviceId":id}))
}

async fn subscribed(server: &Server, source: &PortalInputSource) -> Socket {
    source.publish(added("pad", "gamepad")).unwrap();
    let mut socket = connect(server).await;
    authenticate(&mut socket, &["gamepad"]).await;
    assert_eq!(event(&mut socket).await["device"]["deviceId"], "pad");
    socket
}

#[test]
fn legacy_wire_round_trips_without_wrapper_or_renamed_fields() {
    for value in [
        json!({"kind":"input","deviceId":"pad","class":"gamepad","type":3,"code":0,"value":-0.5,"timestamp":123.25}),
        json!({"kind":"device-added","device":{"deviceId":"pad","class":"gamepad","name":"Pad","capabilities":[],"axes":[{"code":0,"minimum":-1,"maximum":1},{"code":1,"minimum":0,"maximum":255,"flat":2}]}}),
        json!({"kind":"device-added","device":{"deviceId":"key","class":"keyboard","name":"Keyboard","capabilities":[]}}),
        json!({"kind":"device-removed","deviceId":"pad"}),
        json!({"kind":"action","class":"system","action":"system","timestamp":42.5}),
    ] {
        // Compare with the original fixture too: a Rust-only round trip can
        // silently lose an optional legacy field during the first decode.
        let first = decode(value.clone());
        let serialized = serde_json::to_value(&first).unwrap();
        assert_legacy_value(&value, &serialized);
        assert_eq!(first, decode(serialized));
    }
    let subscription: NativeInputSubscription =
        serde_json::from_value(json!({"classes":["gamepad","system"]})).unwrap();
    assert_eq!(
        serde_json::to_value(subscription).unwrap(),
        json!({"classes":["gamepad","system"]})
    );
    for bad in [
        json!({"kind":"action","class":"system","action":"launch","timestamp":1}),
        json!({"kind":"unknown"}),
    ] {
        assert!(serde_json::from_value::<NativeInputEvent>(bad).is_err());
    }
}

fn assert_legacy_value(expected: &Value, actual: &Value) {
    match (expected, actual) {
        // Legacy Schema.Number and JavaScript JSON do not distinguish 1/1.0.
        (Value::Number(expected), Value::Number(actual)) => {
            assert_eq!(expected.as_f64(), actual.as_f64());
        }
        (Value::Array(expected), Value::Array(actual)) => {
            assert_eq!(expected.len(), actual.len());
            for (expected, actual) in expected.iter().zip(actual) {
                assert_legacy_value(expected, actual);
            }
        }
        (Value::Object(expected), Value::Object(actual)) => {
            assert_eq!(expected.len(), actual.len());
            for (name, expected) in expected {
                assert_legacy_value(expected, actual.get(name).expect("legacy field is present"));
            }
        }
        _ => assert_eq!(expected, actual),
    }
}

#[tokio::test]
async fn trace_logger_cannot_capture_capability_from_messages_or_close_frames() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};

    struct RecordingLogs {
        credential_seen: AtomicBool,
        info_seen: AtomicBool,
    }
    impl log::Log for RecordingLogs {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            // Retain only a verdict, never the received diagnostic or credential.
            if record.args().to_string().contains(CAPABILITY) {
                self.credential_seen.store(true, Ordering::Relaxed);
            }
            if record.target() == "native_input_logging_control" {
                self.info_seen.store(true, Ordering::Relaxed);
            }
        }
        fn flush(&self) {}
    }
    static LOGS: RecordingLogs = RecordingLogs {
        credential_seen: AtomicBool::new(false),
        info_seen: AtomicBool::new(false),
    };
    log::set_logger(&LOGS).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    log::info!(target: "native_input_logging_control", "logging remains enabled");

    let (server, source) = server().await;
    let mut socket = subscribed(&server, &source).await;
    socket
        .close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: CAPABILITY.into(),
        }))
        .await
        .unwrap();
    closed(&mut socket).await;

    assert!(LOGS.info_seen.load(Ordering::Relaxed));
    assert!(
        !LOGS.credential_seen.load(Ordering::Relaxed),
        "dependency logging exposed the WebSocket credential"
    );
}

#[tokio::test]
async fn accepted_origin_and_capability_under_each_existing_permission() {
    for permission in [
        PortalPermission::Full,
        PortalPermission::ReadOnly,
        PortalPermission::LocalSessions,
    ] {
        let (server, source) = server_with(limits(), permission).await;
        let mut socket = subscribed(&server, &source).await;
        source.publish(input("pad", "gamepad", 1.0)).unwrap();
        assert_eq!(event(&mut socket).await["kind"], "input");
    }
}

#[tokio::test]
async fn foreign_missing_duplicate_and_combined_origin_rejected_before_upgrade() {
    let (server, _source) = server().await;
    for origins in [
        vec![],
        vec!["https://foreign.example"],
        vec!["null"],
        vec![ORIGIN, ORIGIN],
        vec![ORIGIN, "https://foreign.example"],
        vec!["http://127.0.0.1:8099, http://127.0.0.1:8099"],
        vec!["http://localhost:8099"],
    ] {
        let mut request = server.url.clone().into_client_request().unwrap();
        for origin in origins {
            request
                .headers_mut()
                .append("Origin", origin.parse().unwrap());
        }
        let result = timeout(WAIT, connect_async(request)).await.unwrap();
        let Err(Error::Http(response)) = result else {
            panic!("origin accepted")
        };
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn query_strings_are_rejected_not_used_as_authority() {
    let (server, _source) = server().await;
    let mut request = format!("{}?capability=not-a-credential", server.url)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", ORIGIN.parse().unwrap());
    let Err(Error::Http(response)) = connect_async(request).await else {
        panic!("query accepted")
    };
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn wrong_capability_and_wrong_first_frame_close_without_metadata() {
    let (server, source) = server().await;
    source.publish(added("pad", "gamepad")).unwrap();
    for bad in [
        "Bearer wrong".to_owned(),
        CAPABILITY.to_owned(),
        format!("bearer {CAPABILITY}"),
        format!("Bearer {CAPABILITY} "),
        json!({"capability":CAPABILITY}).to_string(),
        json!({"classes":["gamepad"]}).to_string(),
    ] {
        let mut socket = connect(&server).await;
        text(&mut socket, bad).await;
        closed(&mut socket).await;
    }
    let mut socket = connect(&server).await;
    socket
        .send(Message::Binary(
            format!("Bearer {CAPABILITY}").into_bytes().into(),
        ))
        .await
        .unwrap();
    closed(&mut socket).await;
}

#[tokio::test]
async fn neither_header_nor_subprotocol_replaces_first_frame() {
    let (server, source) = server().await;
    source.publish(added("pad", "gamepad")).unwrap();
    let mut request = server.url.clone().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Origin", ORIGIN.parse().unwrap());
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {CAPABILITY}").parse().unwrap(),
    );
    let (mut socket, _) = connect_async(request.clone()).await.unwrap();
    text(&mut socket, json!({"classes":["gamepad"]}).to_string()).await;
    closed(&mut socket).await;
    request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "not-authority".parse().unwrap());
    // The server does not negotiate a credential-bearing subprotocol. The
    // strict test client rejects the upgrade when none is selected.
    assert!(matches!(
        connect_async(request).await,
        Err(Error::Protocol(_))
    ));
}

#[tokio::test]
async fn silent_unauthenticated_and_unsubscribed_sockets_expire_without_events() {
    let (server, source) = server().await;
    source.publish(added("pad", "gamepad")).unwrap();
    for bearer in [false, true] {
        let mut socket = connect(&server).await;
        if bearer {
            text(&mut socket, format!("Bearer {CAPABILITY}")).await;
        }
        source.publish(input("pad", "gamepad", 1.0)).unwrap();
        silent(&mut socket).await;
        closed(&mut socket).await;
    }
}

#[tokio::test]
async fn metadata_only_after_auth_and_subscription_no_preauth_input_or_action_replay() {
    let (server, source) = server().await;
    let mut socket = connect(&server).await;
    source.publish(added("pad", "gamepad")).unwrap();
    source.publish(input("pad", "gamepad", 1.0)).unwrap();
    source
        .publish(decode(
            json!({"kind":"action","class":"system","action":"system","timestamp":1}),
        ))
        .unwrap();
    silent(&mut socket).await;
    text(&mut socket, format!("Bearer {CAPABILITY}")).await;
    silent(&mut socket).await;
    text(
        &mut socket,
        json!({"classes":["gamepad","system"]}).to_string(),
    )
    .await;
    assert_eq!(event(&mut socket).await["kind"], "device-added");
    silent(&mut socket).await;
    source.publish(input("pad", "gamepad", 0.0)).unwrap();
    assert_eq!(event(&mut socket).await["value"], 0.0);
}

#[tokio::test]
async fn oversized_auth_subscription_and_fragmented_auth_are_closed() {
    use tokio_tungstenite::tungstenite::protocol::frame::{
        coding::{Data, OpCode},
        Frame,
    };
    let (server, _source) = server_with(
        PortalInputLimits {
            incoming_bytes: 128,
            ..limits()
        },
        PortalPermission::Full,
    )
    .await;
    for authenticated in [false, true] {
        let mut socket = connect(&server).await;
        if authenticated {
            text(&mut socket, format!("Bearer {CAPABILITY}")).await;
        }
        text(&mut socket, "x".repeat(129)).await;
        closed(&mut socket).await;
    }
    let mut socket = connect(&server).await;
    socket
        .send(Message::Frame(Frame::message(
            vec![b'x'; 80],
            OpCode::Data(Data::Text),
            false,
        )))
        .await
        .unwrap();
    socket
        .send(Message::Frame(Frame::message(
            vec![b'x'; 80],
            OpCode::Data(Data::Continue),
            true,
        )))
        .await
        .unwrap();
    closed(&mut socket).await;
}

#[tokio::test]
async fn browser_injection_and_malformed_subscription_close_the_connection() {
    let (server, source) = server().await;
    source.publish(added("pad", "gamepad")).unwrap();
    for bad in [
        serde_json::to_string(&input("pad", "gamepad", 1.0)).unwrap(),
        "{".into(),
        json!({"classes":["invalid"]}).to_string(),
        json!({"classes":["gamepad"],"kind":"input","value":1}).to_string(),
    ] {
        let mut socket = connect(&server).await;
        authenticate(&mut socket, &["gamepad"]).await;
        assert_eq!(event(&mut socket).await["kind"], "device-added");
        text(&mut socket, bad).await;
        closed(&mut socket).await;
    }
    let mut socket = connect(&server).await;
    text(&mut socket, format!("Bearer {CAPABILITY}")).await;
    text(&mut socket, "{\"classes\":[],\"classes\":[\"gamepad\"]}").await;
    closed(&mut socket).await;
}

#[tokio::test]
async fn class_filter_snapshot_lifecycle_subscription_change_and_reconnect() {
    let (server, source) = server().await;
    source.publish(added("keyboard", "keyboard")).unwrap();
    let mut socket = subscribed(&server, &source).await;
    source.publish(input("keyboard", "keyboard", 1.0)).unwrap();
    source.publish(added("second", "gamepad")).unwrap();
    source.publish(input("second", "gamepad", 1.0)).unwrap();
    source.publish(removed("second")).unwrap();
    assert_eq!(event(&mut socket).await["device"]["deviceId"], "second");
    assert_eq!(event(&mut socket).await["deviceId"], "second");
    assert_eq!(
        event(&mut socket).await,
        json!({"kind":"device-removed","deviceId":"second"})
    );
    text(&mut socket, json!({"classes":["keyboard"]}).to_string()).await;
    assert_eq!(event(&mut socket).await["device"]["deviceId"], "keyboard");
    source.publish(input("pad", "gamepad", 1.0)).unwrap();
    source.publish(removed("keyboard")).unwrap();
    assert_eq!(event(&mut socket).await["kind"], "device-removed");
    silent(&mut socket).await;
    socket.close(None).await.unwrap();
    drop(socket);
    let mut socket = connect(&server).await;
    authenticate(&mut socket, &["gamepad", "keyboard"]).await;
    assert_eq!(event(&mut socket).await["device"]["deviceId"], "pad");
    silent(&mut socket).await;
}

#[tokio::test]
async fn source_loss_closes_authenticated_and_pending_sockets_and_refuses_reconnect() {
    let (server, source) = server().await;
    let mut active = subscribed(&server, &source).await;
    let mut pending = connect(&server).await;
    source.publish(input("pad", "gamepad", 1.0)).unwrap();
    drop(source); // queued input must not drain after loss
    closed(&mut active).await;
    closed(&mut pending).await;
    let mut request = server.url.clone().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Origin", ORIGIN.parse().unwrap());
    let Err(Error::Http(response)) = connect_async(request).await else {
        panic!("source loss accepted reconnect")
    };
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn connection_budget_counts_unauthenticated_sockets_and_reclaims_expired_slot() {
    let (server, _source) = server_with(
        PortalInputLimits {
            connections: 1,
            ..limits()
        },
        PortalPermission::Full,
    )
    .await;
    let mut first = connect(&server).await;
    let mut request = server.url.clone().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Origin", ORIGIN.parse().unwrap());
    let Err(Error::Http(response)) = connect_async(request).await else {
        panic!("connection limit ignored")
    };
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    closed(&mut first).await;
    let mut replacement = connect(&server).await;
    text(&mut replacement, "invalid").await;
    closed(&mut replacement).await;
    let _replacement = connect(&server).await;
}

#[tokio::test]
async fn per_connection_queue_overflow_disconnects_without_draining_and_does_not_harm_other_classes(
) {
    let (server, source) = server_with(
        PortalInputLimits {
            queued_events: 2,
            ..limits()
        },
        PortalPermission::Full,
    )
    .await;
    let mut lagging = subscribed(&server, &source).await;
    source.publish(added("keyboard", "keyboard")).unwrap();
    let mut healthy = connect(&server).await;
    authenticate(&mut healthy, &["keyboard"]).await;
    assert_eq!(event(&mut healthy).await["device"]["deviceId"], "keyboard");
    // Current-thread runtime: no await lets the server drain during this burst.
    for _ in 0..3 {
        source.publish(input("pad", "gamepad", 1.0)).unwrap();
    }
    closed(&mut lagging).await;
    source.publish(input("keyboard", "keyboard", 1.0)).unwrap();
    assert_eq!(event(&mut healthy).await["kind"], "input");
    let mut fresh = connect(&server).await;
    authenticate(&mut fresh, &["gamepad"]).await;
    assert_eq!(event(&mut fresh).await["kind"], "device-added");
    silent(&mut fresh).await;
}

#[tokio::test]
async fn stale_queued_event_closes_instead_of_replaying_after_executor_stall() {
    let (server, source) = server_with(
        PortalInputLimits {
            max_queue_age: Duration::from_millis(75),
            ..limits()
        },
        PortalPermission::Full,
    )
    .await;
    let mut socket = subscribed(&server, &source).await;
    source.publish(input("pad", "gamepad", 1.0)).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    closed(&mut socket).await;
}

#[tokio::test]
async fn metadata_count_and_serialized_bytes_are_bounded_without_corrupting_snapshot() {
    let (server, source) = server_with(
        PortalInputLimits {
            devices: 1,
            event_bytes: 512,
            ..limits()
        },
        PortalPermission::Full,
    )
    .await;
    assert_eq!(
        source.publish(added(&"x".repeat(513), "gamepad")),
        Err(PublishError::TooLarge)
    );
    source.publish(added("pad", "gamepad")).unwrap();
    assert_eq!(
        source.publish(added("second", "gamepad")),
        Err(PublishError::DeviceLimit)
    );
    assert_eq!(
        source.publish(added("pad", "gamepad")),
        Err(PublishError::DuplicateDevice)
    );
    assert_eq!(
        source.publish(input("absent", "gamepad", 1.0)),
        Err(PublishError::UnknownDevice)
    );
    assert_eq!(
        source.publish(input("pad", "keyboard", 1.0)),
        Err(PublishError::UnknownDevice)
    );
    let mut socket = connect(&server).await;
    authenticate(&mut socket, &["gamepad"]).await;
    assert_eq!(event(&mut socket).await["device"]["deviceId"], "pad");
    silent(&mut socket).await;
    source.publish(removed("pad")).unwrap();
    source.publish(added("second", "gamepad")).unwrap();
    assert_eq!(event(&mut socket).await["kind"], "device-removed");
    assert_eq!(event(&mut socket).await["device"]["deviceId"], "second");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_and_live_lifecycle_are_ordered_under_concurrent_publication() {
    let (server, source) = server_with(
        PortalInputLimits {
            devices: 64,
            queued_events: 256,
            ..limits()
        },
        PortalPermission::Full,
    )
    .await;
    source.publish(added("pad", "gamepad")).unwrap();
    let mut socket = connect(&server).await;
    authenticate(&mut socket, &["gamepad"]).await;
    let publisher = tokio::task::spawn_blocking(move || {
        for i in 0..32 {
            let id = format!("pad-{i:02}");
            source.publish(added(&id, "gamepad")).unwrap();
            source.publish(input(&id, "gamepad", 1.0)).unwrap();
            source.publish(removed(&id)).unwrap();
        }
        source
    });
    let mut known = std::collections::HashSet::new();
    let source = publisher.await.unwrap();
    // Seeing pad proves subscription/snapshot installation, so the final marker
    // cannot race ahead of subscription (the earlier lifecycle burst can).
    let first = event(&mut socket).await;
    assert_eq!(first["kind"], "device-added");
    known.insert(first["device"]["deviceId"].as_str().unwrap().to_owned());
    source
        .publish(decode(
            json!({"kind":"action","class":"gamepad","action":"system","timestamp":99}),
        ))
        .unwrap();
    loop {
        let next = event(&mut socket).await;
        let id = next["deviceId"].as_str().unwrap_or("");
        match next["kind"].as_str().unwrap() {
            "device-added" => {
                assert!(known.insert(next["device"]["deviceId"].as_str().unwrap().to_owned()));
            }
            "input" => assert!(known.contains(id), "input preceded device metadata"),
            "device-removed" => assert!(known.remove(id), "removal preceded metadata"),
            "action" => break,
            _ => panic!("unexpected wire event"),
        }
    }
    assert_eq!(known, ["pad".to_owned()].into());
}

#[tokio::test]
async fn non_reading_socket_hits_write_deadline_and_releases_connection_budget() {
    use std::os::fd::AsRawFd;
    let (server, source) = server_with(
        PortalInputLimits {
            connections: 1,
            devices: 8,
            event_bytes: 2 * 1024 * 1024,
            write_timeout: Duration::from_millis(100),
            max_queue_age: Duration::from_secs(10),
            ..limits()
        },
        PortalPermission::Full,
    )
    .await;
    for id in 0..8 {
        let NativeInputEvent::DeviceAdded(mut device) = added(&id.to_string(), "gamepad") else {
            unreachable!()
        };
        device.device.name = "n".repeat(1024 * 1024);
        source
            .publish(NativeInputEvent::DeviceAdded(device))
            .unwrap();
    }
    let address = server
        .url
        .strip_prefix("ws://")
        .unwrap()
        .trim_end_matches('/');
    let stream = TcpStream::connect(address).await.unwrap();
    let receive_bytes: libc::c_int = 4096;
    // Limit the real client TCP receive window, so the snapshot cannot fit in
    // kernel buffers. This tests write backpressure, not a mocked send failure.
    let result = unsafe {
        libc::setsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVBUF,
            (&receive_bytes as *const libc::c_int).cast(),
            std::mem::size_of_val(&receive_bytes) as libc::socklen_t,
        )
    };
    assert_eq!(result, 0);
    let mut request = server.url.clone().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Origin", ORIGIN.parse().unwrap());
    let (mut blocked, _) =
        tokio_tungstenite::client_async(request.clone(), MaybeTlsStream::Plain(stream))
            .await
            .unwrap();
    authenticate(&mut blocked, &["gamepad"]).await;
    let MaybeTlsStream::Plain(stream) = blocked.get_ref() else {
        unreachable!()
    };
    let mut prefix = [0; 128];
    timeout(WAIT, async {
        loop {
            let count = stream.peek(&mut prefix).await.unwrap();
            if prefix[..count]
                .windows(b"device-added".len())
                .any(|bytes| bytes == b"device-added")
            {
                break;
            }
            assert!(count > 0, "snapshot must start after authentication");
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    // Never read blocked: checking slot recovery must not relieve its pressure.
    timeout(WAIT, async {
        loop {
            tokio::time::sleep(Duration::from_millis(20)).await;
            match connect_async(request.clone()).await {
                Ok((mut replacement, _)) => {
                    text(&mut replacement, "invalid").await;
                    closed(&mut replacement).await;
                    break;
                }
                Err(Error::Http(response)) => {
                    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS)
                }
                Err(_) => panic!("unexpected reconnect failure"),
            }
        }
    })
    .await
    .expect("write deadline must free the connection slot");
    drop(blocked);
}

#[tokio::test]
async fn active_client_disposal_releases_its_slot_and_reconnect_gets_only_metadata() {
    let (server, source) = server_with(
        PortalInputLimits {
            connections: 1,
            ..limits()
        },
        PortalPermission::Full,
    )
    .await;
    let mut socket = subscribed(&server, &source).await;
    source.publish(input("pad", "gamepad", 1.0)).unwrap();
    assert_eq!(event(&mut socket).await["kind"], "input");
    socket.close(None).await.unwrap();
    drop(socket);
    let mut request = server.url.clone().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Origin", ORIGIN.parse().unwrap());
    let mut replacement = timeout(WAIT, async {
        loop {
            match connect_async(request.clone()).await {
                Ok((socket, _)) => break socket,
                Err(Error::Http(response)) => {
                    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS)
                }
                Err(_) => panic!("unexpected reconnect failure"),
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    authenticate(&mut replacement, &["gamepad"]).await;
    assert_eq!(event(&mut replacement).await["kind"], "device-added");
    silent(&mut replacement).await;
}

#[tokio::test]
async fn nonfinite_values_are_rejected_without_emitting_null_numbers() {
    let (server, source) = server().await;
    let mut socket = subscribed(&server, &source).await;
    let NativeInputEvent::Input(mut input) = input("pad", "gamepad", 1.0) else {
        unreachable!()
    };
    input.value = f64::NAN;
    assert_eq!(
        source.publish(NativeInputEvent::Input(input)),
        Err(PublishError::NonFinite)
    );
    let NativeInputEvent::DeviceAdded(mut device) = added("invalid", "gamepad") else {
        unreachable!()
    };
    device.device.axes.as_mut().unwrap()[0].maximum = f64::INFINITY;
    assert_eq!(
        source.publish(NativeInputEvent::DeviceAdded(device)),
        Err(PublishError::NonFinite)
    );
    silent(&mut socket).await;
}
