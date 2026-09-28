use futures_util::{SinkExt, StreamExt};
use korri_input_contract::{GamepadState, RemoteSource, SeatReply, SeatRequest};
use korri_inputd::producer::PrivateInputChannel;
use std::time::Duration;
use tokio::net::UnixListener;
use tokio_tungstenite::tungstenite::Message;

fn baseline() -> SeatReply {
    SeatReply {
        failure: None,
        count: 4,
        session: None,
        recovery_required: false,
        slot: None,
        remote_sources: vec![RemoteSource {
            launch_id: "live-launch".into(),
            controller_number: 1,
            state: GamepadState {
                buttons: 0x1000,
                ..GamepadState::neutral()
            },
            slot: None,
        }],
        remote_events: vec![],
    }
}
fn connect() -> SeatRequest {
    SeatRequest::PhysicalConnected {
        device_id: "usb-physical-controller/input0".into(),
        name: "recorded pad".into(),
        state: GamepadState {
            left_trigger: 80,
            ..GamepadState::neutral()
        },
    }
}

struct InspectPrivateHandshake;

impl tokio_tungstenite::tungstenite::handshake::server::Callback for InspectPrivateHandshake {
    fn on_request(
        self,
        request: &tokio_tungstenite::tungstenite::handshake::server::Request,
        response: tokio_tungstenite::tungstenite::handshake::server::Response,
    ) -> Result<
        tokio_tungstenite::tungstenite::handshake::server::Response,
        tokio_tungstenite::tungstenite::handshake::server::ErrorResponse,
    > {
        assert_eq!(request.uri().path(), "/");
        assert!(!request.headers().contains_key("origin"));
        assert!(!request.headers().contains_key("authorization"));
        Ok(response)
    }
}

#[tokio::test]
async fn actual_unix_websocket_carries_canonical_physical_and_held_remote_baselines() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("existing-control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (received, wait) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_hdr_async(stream, InspectPrivateHandshake)
            .await
            .unwrap();
        socket
            .send(Message::Text(
                serde_json::to_string(&baseline()).unwrap().into(),
            ))
            .await
            .unwrap();
        let Message::Text(text) = socket.next().await.unwrap().unwrap() else {
            panic!("canonical text frame expected")
        };
        assert_eq!(
            serde_json::from_str::<SeatRequest>(text.as_ref()).unwrap(),
            connect()
        );
        wait.await.unwrap();
    });
    let mut channel = PrivateInputChannel::connect(&path).await.unwrap();
    channel.send(connect()).unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), channel.next())
            .await
            .unwrap()
            .unwrap(),
        baseline()
    );
    received.send(()).unwrap();
    server.await.unwrap();
    assert!(tokio::time::timeout(Duration::from_secs(1), channel.next())
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn malformed_feedback_retires_channel_instead_of_promoting_unknown_input() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        socket
            .send(Message::Text("{\"kind\":\"arbitrary-input\"}".into()))
            .await
            .unwrap();
    });
    let mut channel = PrivateInputChannel::connect(&path).await.unwrap();
    assert!(tokio::time::timeout(Duration::from_secs(1), channel.next())
        .await
        .unwrap()
        .is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn producer_cannot_send_count_session_or_remote_authority_and_queue_is_finite() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (finished, wait) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let _socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        wait.await.unwrap();
    });
    let channel = PrivateInputChannel::connect(&path).await.unwrap();
    for request in [
        SeatRequest::Hello,
        SeatRequest::Poll,
        SeatRequest::ApplyCount { count: 8 },
        SeatRequest::BeginSession {
            launch_id: "forged".into(),
        },
    ] {
        assert!(channel.send(request).is_err());
    }
    for _ in 0..256 {
        channel.send(connect()).unwrap();
    }
    assert!(
        channel.send(connect()).is_err(),
        "a slow consumer cannot grow the producer queue"
    );
    finished.send(()).unwrap();
    drop(channel);
    server.await.unwrap();
}
