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

async fn wire_event(socket: &mut Socket) -> Value {
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

// Legacy assertions concern events. Dedicated barrier tests inspect wire_event.
async fn event(socket: &mut Socket) -> Value {
    loop {
        let value = wire_event(socket).await;
        if value["kind"] != "initialization-complete" && value["kind"] != "device-state-complete" {
            return value;
        }
    }
}

async fn silent(socket: &mut Socket) {
    assert!(
        timeout(Duration::from_millis(25), event(socket))
            .await
            .is_err(),
        "socket sent data before expected"
    );
}

async fn closed(socket: &mut Socket) {
    loop {
        match timeout(WAIT, socket.next())
            .await
            .expect("socket must close")
        {
            None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
            Some(Ok(Message::Text(text))) => {
                let value: Value = serde_json::from_str(&text).unwrap();
                assert!(
                    value["kind"] == "initialization-complete"
                        || value["kind"] == "device-state-complete",
                    "closed connection must not drain queued events"
                );
            }
            _ => panic!("closed connection must not drain queued events"),
        }
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
async fn current_baseline_only_after_auth_and_subscription_no_action_replay() {
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
    assert_eq!(wire_event(&mut socket).await["kind"], "device-added");
    assert_eq!(wire_event(&mut socket).await["value"], 1.0);
    assert_eq!(
        wire_event(&mut socket).await["kind"],
        "initialization-complete"
    );
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
    assert_eq!(event(&mut socket).await["value"], 1.0);
    source.publish(input("pad", "gamepad", 1.0)).unwrap();
    source.publish(removed("keyboard")).unwrap();
    assert_eq!(event(&mut socket).await["kind"], "device-removed");
    silent(&mut socket).await;
    socket.close(None).await.unwrap();
    drop(socket);
    let mut socket = connect(&server).await;
    authenticate(&mut socket, &["gamepad", "keyboard"]).await;
    assert_eq!(event(&mut socket).await["device"]["deviceId"], "pad");
    assert_eq!(event(&mut socket).await["value"], 1.0);
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
    assert_eq!(event(&mut fresh).await["value"], 1.0);
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
async fn active_client_disposal_releases_slot_and_reconnect_gets_current_baseline() {
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
    assert_eq!(event(&mut replacement).await["value"], 1.0);
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

#[test]
fn lifecycle_struct_union_preserves_exact_flat_wire_and_rejects_extra_fields() {
    for value in [
        json!({"kind":"initialization-complete","generation":"1"}),
        json!({"kind":"device-state-complete","deviceId":"pad"}),
        json!({"kind":"suspend","generation":"1","requestId":"2"}),
        json!({"kind":"resume","generation":"1","requestId":"2"}),
    ] {
        let control: NativeInputControl = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(control).unwrap(), value);
        let mut extra = value;
        extra["payload"] = json!({});
        assert!(serde_json::from_value::<NativeInputControl>(extra).is_err());
    }
    let value = json!({"kind":"suspended","generation":"1","requestId":"2"});
    let ack: NativeInputAcknowledgement = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(ack).unwrap(), value);
    for bad in [
        json!({"kind":"resume","generation":"1","requestId":"2"}),
        json!({"kind":"suspended","generation":"1","requestId":"2","input":1}),
        json!({"kind":"suspended","generation":"1"}),
    ] {
        assert!(serde_json::from_value::<NativeInputAcknowledgement>(bad).is_err());
    }
}

#[tokio::test]
async fn all_physical_and_remote_sources_attach_above_64_without_expanding_live_queues() {
    let count = korri_input_contract::MAX_PHYSICAL_SOURCES + 16;
    assert_eq!(PortalInputLimits::default().devices, count);
    let (server, source) = server_with(
        PortalInputLimits {
            queued_events: 2,
            ..limits()
        },
        PortalPermission::LocalSessions,
    )
    .await;
    for index in 0..count {
        let id = format!("source-{index}");
        source.publish(added(&id, "gamepad")).unwrap();
        source.publish(input(&id, "gamepad", 1.0)).unwrap();
    }
    assert_eq!(
        source.publish(added("over-budget", "gamepad")),
        Err(PublishError::DeviceLimit)
    );
    // Each subscriber gets the full bounded current-state baseline, not a
    // 64-source truncation and not a replay through the small live queue.
    for _ in 0..2 {
        let mut socket = connect(&server).await;
        authenticate(&mut socket, &["gamepad"]).await;
        let mut known = std::collections::HashSet::new();
        for _ in 0..count {
            let metadata = wire_event(&mut socket).await;
            assert_eq!(metadata["kind"], "device-added");
            let id = metadata["device"]["deviceId"].as_str().unwrap();
            assert!(known.insert(id.to_owned()));
            let held = wire_event(&mut socket).await;
            assert_eq!(held["deviceId"], id);
            assert_eq!(held["value"], 1.0);
        }
        assert_eq!(known.len(), count);
        assert_eq!(
            wire_event(&mut socket).await["kind"],
            "initialization-complete"
        );
        // The last source is not silently excluded after the full baseline.
        source
            .publish(input(&format!("source-{}", count - 1), "gamepad", 1.0))
            .unwrap();
        assert_eq!(wire_event(&mut socket).await["kind"], "input");
        // Finite live queue policy still applies at this device count.
        for _ in 0..3 {
            source.publish(input("source-0", "gamepad", 1.0)).unwrap();
        }
        closed(&mut socket).await;
    }
}

fn key_value(device_id: &str, code: f64, value: f64) -> NativeInputInput {
    let NativeInputEvent::Input(mut input) = input(device_id, "gamepad", value) else {
        unreachable!()
    };
    input.code = code;
    input
}

#[tokio::test]
async fn measured_frames_preserve_short_press_release_and_canonical_report_boundary() {
    let (server, source) = server().await;
    source.publish(added("pad", "gamepad")).unwrap();
    let (mut socket, _) = attach(&server).await;
    for value in [1.0, 0.0] {
        source
            .publish_frame("pad", vec![key_value("pad", 304.0, value)])
            .unwrap();
        let key = wire_event(&mut socket).await;
        assert_eq!(key["code"], 304.0);
        assert_eq!(key["value"], value);
        let report = wire_event(&mut socket).await;
        assert_eq!(report["kind"], "input");
        assert_eq!(report["deviceId"], "pad");
        assert_eq!(report["type"], 0.0);
        assert_eq!(report["code"], 0.0);
        assert_eq!(report["value"], 0.0);
    }
    // A malformed later delta cannot commit an earlier release or emit a prefix.
    assert_eq!(
        source.publish_frame(
            "pad",
            vec![key_value("pad", 304.0, 1.0), key_value("pad", 315.0, 3.0)]
        ),
        Err(PublishError::InvalidInput)
    );
    silent(&mut socket).await;
    let mut fresh = connect(&server).await;
    authenticate(&mut fresh, &["gamepad"]).await;
    assert_eq!(wire_event(&mut fresh).await["kind"], "device-added");
    let baseline = wire_event(&mut fresh).await;
    assert_eq!(baseline["code"], 304.0);
    assert_eq!(baseline["value"], 0.0);
    assert_eq!(
        wire_event(&mut fresh).await["kind"],
        "initialization-complete"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn attachment_racing_a_split_start_to_a_frame_never_captures_transient_neutral() {
    use std::sync::Arc;
    let (server, source) = server_with(
        PortalInputLimits {
            queued_events: 4096,
            ..limits()
        },
        PortalPermission::LocalSessions,
    )
    .await;
    let source = Arc::new(source);
    let NativeInputEvent::DeviceAdded(added) = added("pad", "gamepad") else {
        unreachable!()
    };
    source
        .publish_device(
            added.device,
            vec![key_value("pad", 315.0, 1.0), key_value("pad", 304.0, 0.0)],
        )
        .unwrap();
    for _ in 0..8 {
        let mut socket = connect(&server).await;
        text(&mut socket, format!("Bearer {CAPABILITY}")).await;
        let publisher = Arc::clone(&source);
        let task = tokio::task::spawn_blocking(move || {
            for index in 0..256 {
                let a = if index % 2 == 0 { 1.0 } else { 0.0 };
                publisher
                    .publish_frame(
                        "pad",
                        vec![key_value("pad", 315.0, 1.0 - a), key_value("pad", 304.0, a)],
                    )
                    .unwrap();
                std::thread::yield_now();
            }
        });
        text(&mut socket, json!({"classes":["gamepad"]}).to_string()).await;
        assert_eq!(wire_event(&mut socket).await["kind"], "device-added");
        let mut baseline = std::collections::BTreeMap::new();
        loop {
            let frame = wire_event(&mut socket).await;
            if frame["kind"] == "initialization-complete" {
                break;
            }
            assert_eq!(frame["kind"], "input");
            baseline.insert(
                frame["code"].as_f64().unwrap() as u16,
                frame["value"].as_f64().unwrap(),
            );
        }
        assert_eq!(baseline.len(), 2);
        assert_eq!(
            baseline[&304] + baseline[&315],
            1.0,
            "attachment captured partial sample"
        );
        task.await.unwrap();
        socket.close(None).await.unwrap();
    }
}

#[tokio::test]
async fn local_retirement_winning_selection_releases_budget_and_is_not_a_suspend_participant() {
    let (server, source) = server_with(
        PortalInputLimits {
            connections: 1,
            ..limits()
        },
        PortalPermission::LocalSessions,
    )
    .await;
    for _ in 0..8 {
        let (mut socket, generation) = attach(&server).await;
        text(
            &mut socket,
            json!({"kind":"retire","generation":generation}).to_string(),
        )
        .await;
        assert_eq!(
            wire_event(&mut socket).await,
            json!({"kind":"retired","generation":generation})
        );
        // The release decision preceded its reply under the selection mutex.
        // Even if close has not completed, suspend must not await this client.
        assert_eq!(source.suspend(WAIT).await, Ok(()));
        closed(&mut socket).await;
        source.resume();
    }
}

#[tokio::test]
async fn suspend_selection_winning_local_retirement_keeps_exact_ack_channel() {
    use std::sync::Arc;
    let (server, source) = server().await;
    let source = Arc::new(source);
    let (mut socket, generation) = attach(&server).await;
    let worker = Arc::clone(&source);
    let pending = tokio::spawn(async move { worker.suspend(WAIT).await });
    let selected = wire_event(&mut socket).await;
    assert_eq!(selected["kind"], "suspend");
    text(
        &mut socket,
        json!({"kind":"retire","generation":generation}).to_string(),
    )
    .await;
    silent(&mut socket).await; // no retired permission to close a selected socket
    assert!(!pending.is_finished()); // retire is not a substitute for suspended ACK
    acknowledge(&mut socket, &selected).await;
    assert_eq!(pending.await.unwrap(), Ok(()));
    source.resume();
    assert_eq!(wire_event(&mut socket).await["kind"], "resume");
    closed(&mut socket).await;
}

#[tokio::test]
async fn local_retirement_does_not_turn_selected_eof_into_success_or_accept_foreign_generation() {
    use portal_input::SuspendError;
    use std::sync::Arc;
    for valid_generation in [true, false] {
        let (server, source) = server().await;
        let source = Arc::new(source);
        let (mut socket, generation) = attach(&server).await;
        let worker = Arc::clone(&source);
        let pending = tokio::spawn(async move { worker.suspend(WAIT).await });
        assert_eq!(wire_event(&mut socket).await["kind"], "suspend");
        text(&mut socket, json!({"kind":"retire","generation":if valid_generation { generation } else { "999".to_owned() }}).to_string()).await;
        if valid_generation {
            silent(&mut socket).await;
            socket.close(None).await.unwrap();
        }
        closed(&mut socket).await;
        assert_eq!(pending.await.unwrap(), Err(SuspendError::Disconnected));
        source.resume();
    }
}

#[test]
fn local_retirement_wire_is_generation_matched_without_new_input_authority() {
    let request = json!({"kind":"retire","generation":"7"});
    let decoded: NativeInputRetirement = serde_json::from_value(request.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), request);
    let reply = json!({"kind":"retired","generation":"7"});
    let decoded: NativeInputControl = serde_json::from_value(reply.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), reply);
    for bad in [
        json!({"kind":"retire"}),
        json!({"kind":"retire","generation":"7","input":1}),
        json!({"kind":"retired","generation":"7"}),
    ] {
        assert!(serde_json::from_value::<NativeInputRetirement>(bad).is_err());
    }
}

async fn attach(server: &Server) -> (Socket, String) {
    let mut socket = connect(server).await;
    authenticate(&mut socket, &["gamepad"]).await;
    loop {
        let frame = wire_event(&mut socket).await;
        if frame["kind"] == "initialization-complete" {
            return (socket, frame["generation"].as_str().unwrap().to_owned());
        }
    }
}

async fn acknowledge(socket: &mut Socket, command: &Value) {
    text(socket, json!({"kind":"suspended", "generation":command["generation"], "requestId":command["requestId"]}).to_string()).await;
}

#[tokio::test]
async fn atomic_arrival_baseline_and_completion_precede_live_input_for_all_subscribers() {
    let (server, source) = server().await;
    let NativeInputEvent::DeviceAdded(added) = added("held", "gamepad") else {
        unreachable!()
    };
    let NativeInputEvent::Input(held) = input("held", "gamepad", 1.0) else {
        unreachable!()
    };
    let (mut live, _) = attach(&server).await;
    source.publish_device(added.device, vec![held]).unwrap();
    assert_eq!(wire_event(&mut live).await["kind"], "device-added");
    assert_eq!(wire_event(&mut live).await["value"], 1.0);
    assert_eq!(
        wire_event(&mut live).await,
        json!({"kind":"device-state-complete","deviceId":"held"})
    );
    let mut fresh = connect(&server).await;
    authenticate(&mut fresh, &["gamepad"]).await;
    assert_eq!(wire_event(&mut fresh).await["kind"], "device-added");
    assert_eq!(wire_event(&mut fresh).await["value"], 1.0);
    assert_eq!(
        wire_event(&mut fresh).await["kind"],
        "initialization-complete"
    );
    source.publish(input("held", "gamepad", 0.0)).unwrap();
    assert_eq!(wire_event(&mut live).await["value"], 0.0);
    assert_eq!(wire_event(&mut fresh).await["value"], 0.0);
}

#[tokio::test]
async fn baseline_keeps_only_latest_values_not_input_history() {
    let (server, source) = server().await;
    source.publish(added("pad", "gamepad")).unwrap();
    source.publish(input("pad", "gamepad", 1.0)).unwrap();
    source.publish(input("pad", "gamepad", 2.0)).unwrap();
    source.publish(input("pad", "gamepad", 0.0)).unwrap();
    let mut socket = connect(&server).await;
    authenticate(&mut socket, &["gamepad"]).await;
    assert_eq!(wire_event(&mut socket).await["kind"], "device-added");
    assert_eq!(wire_event(&mut socket).await["value"], 0.0);
    assert_eq!(
        wire_event(&mut socket).await["kind"],
        "initialization-complete"
    );
    silent(&mut socket).await;
}

#[tokio::test]
async fn suspension_waits_for_every_subscriber_then_resume_requires_new_generation_and_baseline() {
    use std::sync::Arc;
    let (server, source) = server().await;
    let source = Arc::new(source);
    source.publish(added("pad", "gamepad")).unwrap();
    let (mut first, first_generation) = attach(&server).await;
    let (mut second, second_generation) = attach(&server).await;
    assert_ne!(first_generation, second_generation);
    let worker = Arc::clone(&source);
    let suspended = tokio::spawn(async move { worker.suspend(WAIT).await });
    let a = wire_event(&mut first).await;
    let b = wire_event(&mut second).await;
    assert_eq!(a["kind"], "suspend");
    assert_eq!(a["generation"], first_generation);
    assert_eq!(b["generation"], second_generation);
    assert_eq!(a["requestId"], b["requestId"]);
    acknowledge(&mut first, &a).await;
    assert!(!suspended.is_finished());
    // Inactive input updates current state but is never queued for delivery.
    source.publish(input("pad", "gamepad", 1.0)).unwrap();
    silent(&mut first).await;
    silent(&mut second).await;
    acknowledge(&mut second, &b).await;
    assert_eq!(suspended.await.unwrap(), Ok(()));
    source.resume();
    let resumed = wire_event(&mut first).await;
    assert_eq!(
        resumed,
        json!({"kind":"resume", "generation":first_generation, "requestId":a["requestId"]})
    );
    assert_eq!(wire_event(&mut second).await["kind"], "resume");
    closed(&mut first).await;
    closed(&mut second).await;
    let mut fresh = connect(&server).await;
    authenticate(&mut fresh, &["gamepad"]).await;
    assert_eq!(wire_event(&mut fresh).await["kind"], "device-added");
    assert_eq!(wire_event(&mut fresh).await["value"], 1.0);
    let complete = wire_event(&mut fresh).await;
    assert_eq!(complete["kind"], "initialization-complete");
    assert_ne!(complete["generation"], first_generation);
}

#[tokio::test]
async fn stale_or_forged_lifecycle_ack_cannot_complete_freezer_barrier() {
    use portal_input::SuspendError;
    use std::sync::Arc;
    for wrong_field in ["generation", "requestId"] {
        let (server, source) = server().await;
        let source = Arc::new(source);
        let (mut socket, _) = attach(&server).await;
        let worker = Arc::clone(&source);
        let suspended = tokio::spawn(async move { worker.suspend(WAIT).await });
        let mut command = wire_event(&mut socket).await;
        command[wrong_field] = json!("999");
        acknowledge(&mut socket, &command).await;
        closed(&mut socket).await;
        assert_eq!(suspended.await.unwrap(), Err(SuspendError::Disconnected));
        // Failure never silently reenables delivery/freezer transition.
        assert_eq!(
            source.suspend(WAIT).await,
            Err(SuspendError::AlreadySuspended)
        );
        source.resume();
        let (_replacement, _) = attach(&server).await;
    }
}

#[tokio::test]
async fn acknowledgement_from_previous_suspension_cannot_retire_a_new_connection() {
    use portal_input::SuspendError;
    use std::sync::Arc;
    let (server, source) = server().await;
    let source = Arc::new(source);
    let (mut first, _) = attach(&server).await;
    let worker = Arc::clone(&source);
    let pending = tokio::spawn(async move { worker.suspend(WAIT).await });
    let old = wire_event(&mut first).await;
    acknowledge(&mut first, &old).await;
    assert_eq!(pending.await.unwrap(), Ok(()));
    source.resume();
    assert_eq!(wire_event(&mut first).await["kind"], "resume");
    closed(&mut first).await;
    let (mut replacement, _) = attach(&server).await;
    let worker = Arc::clone(&source);
    let pending = tokio::spawn(async move { worker.suspend(WAIT).await });
    let current = wire_event(&mut replacement).await;
    assert_ne!(current["generation"], old["generation"]);
    assert_ne!(current["requestId"], old["requestId"]);
    acknowledge(&mut replacement, &old).await;
    closed(&mut replacement).await;
    assert_eq!(pending.await.unwrap(), Err(SuspendError::Disconnected));
    source.resume();
}

#[tokio::test]
async fn missing_ack_is_explicit_timeout_and_unsolicited_ack_is_not_authority() {
    use portal_input::SuspendError;
    use std::sync::Arc;
    let (server, source) = server().await;
    let source = Arc::new(source);
    let (mut socket, generation) = attach(&server).await;
    text(
        &mut socket,
        json!({"kind":"suspended","generation":generation,"requestId":"1"}).to_string(),
    )
    .await;
    closed(&mut socket).await;
    let (mut socket, _) = attach(&server).await;
    let worker = Arc::clone(&source);
    let suspended = tokio::spawn(async move { worker.suspend(Duration::from_millis(50)).await });
    assert_eq!(wire_event(&mut socket).await["kind"], "suspend");
    assert_eq!(suspended.await.unwrap(), Err(SuspendError::Timeout));
    source.resume();
    assert_eq!(wire_event(&mut socket).await["kind"], "resume");
    closed(&mut socket).await;
}

#[tokio::test]
async fn producer_reset_clears_metadata_and_held_state_then_allows_fresh_attachment() {
    let (server, source) = server().await;
    let mut socket = subscribed(&server, &source).await;
    source.publish(input("pad", "gamepad", 1.0)).unwrap();
    source.reset();
    closed(&mut socket).await;
    let mut fresh = connect(&server).await;
    authenticate(&mut fresh, &["gamepad"]).await;
    assert_eq!(
        wire_event(&mut fresh).await["kind"],
        "initialization-complete"
    );
    silent(&mut fresh).await;
}

#[tokio::test]
async fn current_state_key_space_and_values_are_bounded_by_linux_evdev() {
    let (_server, source) = server().await;
    source.publish(added("pad", "gamepad")).unwrap();
    for (event_type, code, value) in [
        (1.0, 768.0, 1.0),
        (3.0, 64.0, 1.0),
        (1.0, 304.0, 3.0),
        (1.0, 304.5, 1.0),
        (32.0, 0.0, 0.0),
        (3.0, 0.0, 2147483648.0),
    ] {
        let mut event = input("pad", "gamepad", value);
        if let NativeInputEvent::Input(input) = &mut event {
            input.input_type = event_type;
            input.code = code;
        }
        assert_eq!(source.publish(event), Err(PublishError::InvalidInput));
    }
}
