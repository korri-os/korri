//! Producer ingress exists only on the credential-filtered private Unix listener.
use super::input_coordination::SeatCoordinator;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use korri_input_contract::{SeatRequest, MAX_COORDINATION_BYTES, MAX_PHYSICAL_SOURCES};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) fn router(coordinator: Arc<SeatCoordinator>) -> Router {
    Router::new()
        .route("/", get(upgrade))
        .with_state(coordinator)
}
async fn upgrade(
    State(coordinator): State<Arc<SeatCoordinator>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // No web caller has private producer authority. Unix peer credentials are
    // the authority here; reject browser-shaped requests even on that listener.
    if headers.contains_key(axum::http::header::ORIGIN) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(permit) = coordinator.producer.clone().try_acquire_owned() else {
        return StatusCode::CONFLICT.into_response();
    };
    if coordinator.ready().is_err() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    ws.max_message_size(MAX_COORDINATION_BYTES)
        .max_frame_size(MAX_COORDINATION_BYTES)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            run(socket, coordinator).await
        })
}
async fn run(socket: WebSocket, coordinator: Arc<SeatCoordinator>) {
    let (mut output, mut input) = socket.split();
    let mut sources = BTreeMap::new();
    let mut health = tokio::time::interval(Duration::from_millis(100));
    let Ok((mut initial, mut feedback)) = coordinator.subscribe() else {
        return;
    };
    initial.remote_events.clear(); // snapshot is a baseline, not edge replay
    let Ok(text) = serde_json::to_string(&initial) else {
        return;
    };
    if !matches!(
        tokio::time::timeout(
            Duration::from_millis(250),
            output.send(Message::Text(text.into()))
        )
        .await,
        Ok(Ok(()))
    ) {
        return;
    }
    loop {
        tokio::select! {
            message = input.next() => {
                let Some(Ok(Message::Text(text))) = message else { break; };
                let Ok(request) = serde_json::from_str::<SeatRequest>(&text) else { break; };
                match &request {
                    SeatRequest::PhysicalConnected { device_id, name, .. } => {
                        if device_id.is_empty() || device_id.len() > 512 || name.is_empty() || name.len() > 256 || sources.len() >= MAX_PHYSICAL_SOURCES || sources.contains_key(device_id) { break; }
                        sources.insert(device_id.clone(), Instant::now());
                    }
                    SeatRequest::PhysicalState { device_id, .. } if sources.contains_key(device_id) => { sources.insert(device_id.clone(), Instant::now()); },
                    SeatRequest::PhysicalDisconnected { device_id } if sources.remove(device_id).is_some() => {},
                    _ => break, // no count/session/route or remote-source injection
                }
                let worker = coordinator.clone();
                if !matches!(tokio::task::spawn_blocking(move || worker.request(request)).await, Ok(Ok(_))) { break; }
            }
            _ = health.tick() => {
                if coordinator.ready().is_err() || sources.values().any(|last| last.elapsed() >= Duration::from_millis(1000)) { break; }
            }
            reply = feedback.recv() => {
                let Ok(reply) = reply else { break; }; // lag is loss, never skip an edge
                let Ok(text) = serde_json::to_string(&reply) else { break; };
                if !matches!(tokio::time::timeout(Duration::from_millis(250), output.send(Message::Text(text.into()))).await, Ok(Ok(()))) { break; }
            }
        }
    }
    // Preserve disconnected reservations during gameplay. The permit remains
    // held until cleanup is acknowledged, so no replacement producer races it.
    let cleanup = coordinator.clone();
    let _ = tokio::task::spawn_blocking(move || {
        for device_id in sources.into_keys() {
            if cleanup
                .request(SeatRequest::PhysicalDisconnected { device_id })
                .is_err()
            {
                cleanup.fence();
                break;
            }
        }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::super::input_coordination::test_support;
    use super::*;
    use std::sync::Mutex;
    use tokio_tungstenite::{
        client_async,
        tungstenite::{client::IntoClientRequest, Message as ClientMessage},
    };

    type Browser = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;
    async fn native_batch(browser: &mut Browser, marker: &str) -> Vec<serde_json::Value> {
        tokio::time::timeout(Duration::from_secs(2), async {
            let mut frames = Vec::new();
            loop {
                let text = browser.next().await.unwrap().unwrap().into_text().unwrap();
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                let done = if marker == "syn" {
                    value["kind"] == "input"
                        && value["type"].as_f64() == Some(0.0)
                        && value["code"].as_f64() == Some(0.0)
                } else {
                    value["kind"] == marker
                };
                frames.push(value);
                if done {
                    return frames;
                }
            }
        })
        .await
        .unwrap()
    }
    async fn attach_browser(url: &str) -> Browser {
        let mut request = url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Origin", "http://portal.local".parse().unwrap());
        let (mut browser, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        browser
            .send(ClientMessage::Text("Bearer native-integration-test".into()))
            .await
            .unwrap();
        browser
            .send(ClientMessage::Text(r#"{"classes":["gamepad"]}"#.into()))
            .await
            .unwrap();
        browser
    }

    /// Crosses the real Unix WS -> seqpacket coordinator -> native WS path.
    /// The root receiver replies are configured test outcomes, not a uinput
    /// kernel implementation. Receiver/kernel behavior needs the separate VM.
    #[tokio::test]
    async fn physical_and_overflow_remote_samples_cross_private_seqpacket_and_native_sockets() {
        use crate::portal_access::{PortalAccess, PortalPermission};
        use korri_input_contract::{GamepadState, RemoteEvent, RemoteSource};
        let (native_app, native_source) = crate::portal_input::router(
            PortalAccess::new(
                "native-integration-test",
                "http://portal.local",
                PortalPermission::LocalSessions,
            ),
            Default::default(),
        );
        let native_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", native_listener.local_addr().unwrap());
        let native_app =
            native_app.layer(axum::middleware::from_fn(crate::require_loopback_native));
        let native_server = tokio::spawn(async move {
            axum::serve(
                native_listener,
                native_app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        let remote_events = Arc::new(Mutex::new(Vec::<RemoteEvent>::new()));
        let events = remote_events.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let received = requests.clone();
        let mut remote_current = BTreeMap::new();
        let (coordinator, receiver) =
            test_support::coordinator_with_native(Some(Arc::new(native_source)), move |request| {
                received.lock().unwrap().push(request);
                let mut reply = test_support::reply();
                reply.remote_events = std::mem::take(&mut *events.lock().unwrap());
                for event in &reply.remote_events {
                    match event {
                        RemoteEvent::Connected { source } | RemoteEvent::State { source } => {
                            remote_current.insert(
                                (source.launch_id.clone(), source.controller_number),
                                source.clone(),
                            );
                        }
                        RemoteEvent::Disconnected {
                            launch_id,
                            controller_number,
                        } => {
                            remote_current.remove(&(launch_id.clone(), *controller_number));
                        }
                    }
                }
                reply.remote_sources = remote_current.values().cloned().collect();
                reply // slot=None is an accepted connected overflow source
            });
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("private.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let private_app = router(coordinator.clone());
        let private_server = tokio::spawn(async move {
            axum::serve(listener, private_app).await.unwrap();
        });
        let (mut producer, _) = client_async(
            "ws://localhost/",
            tokio::net::UnixStream::connect(&path).await.unwrap(),
        )
        .await
        .unwrap();
        producer.next().await.unwrap().unwrap();

        let mut forged = url.as_str().into_client_request().unwrap();
        forged
            .headers_mut()
            .insert("Origin", "http://wrong-origin".parse().unwrap());
        assert!(
            matches!(tokio_tungstenite::connect_async(forged).await.unwrap_err(), tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == StatusCode::FORBIDDEN)
        );
        let mut unauthorized = url.as_str().into_client_request().unwrap();
        unauthorized
            .headers_mut()
            .insert("Origin", "http://portal.local".parse().unwrap());
        let (mut unauthorized, _) = tokio_tungstenite::connect_async(unauthorized)
            .await
            .unwrap();
        unauthorized
            .send(ClientMessage::Text("Bearer invalid".into()))
            .await
            .unwrap();
        let denied = tokio::time::timeout(Duration::from_secs(2), unauthorized.next())
            .await
            .unwrap();
        assert!(!matches!(denied, Some(Ok(ClientMessage::Text(_)))));
        drop(unauthorized);

        let mut browser = attach_browser(&url).await;
        native_batch(&mut browser, "initialization-complete").await;
        let connected = SeatRequest::PhysicalConnected {
            device_id: "validated-source".into(),
            name: "Pad".into(),
            state: GamepadState {
                buttons: 0x10,
                ..Default::default()
            },
        };
        producer
            .send(ClientMessage::Text(
                serde_json::to_string(&connected).unwrap().into(),
            ))
            .await
            .unwrap();
        let arrival = native_batch(&mut browser, "device-state-complete").await;
        assert_eq!(
            arrival[0]["device"]["deviceId"],
            "physical:validated-source"
        );
        assert!(arrival
            .iter()
            .any(|frame| frame["code"].as_f64() == Some(315.0)
                && frame["value"].as_f64() == Some(1.0)));
        assert!(arrival
            .iter()
            .any(|frame| frame["code"].as_f64() == Some(304.0)
                && frame["value"].as_f64() == Some(0.0)));
        // No measured neutral sample exists between held Start and held A.
        let switched = SeatRequest::PhysicalState {
            device_id: "validated-source".into(),
            state: GamepadState {
                buttons: 0x1000,
                ..Default::default()
            },
        };
        producer
            .send(ClientMessage::Text(
                serde_json::to_string(&switched).unwrap().into(),
            ))
            .await
            .unwrap();
        let frame = native_batch(&mut browser, "syn").await;
        let controls: Vec<_> = frame
            .iter()
            .map(|event| {
                (
                    event["type"].as_f64().unwrap(),
                    event["code"].as_f64().unwrap(),
                    event["value"].as_f64().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            controls,
            [(1.0, 315.0, 0.0), (1.0, 304.0, 1.0), (0.0, 0.0, 0.0)]
        );
        // The consumer sees one commit boundary, never a neutral commit after
        // Start release. A new subscriber sees the complete held A baseline.
        let mut second = attach_browser(&url).await;
        let snapshot = native_batch(&mut second, "initialization-complete").await;
        assert!(snapshot
            .iter()
            .any(|frame| frame["code"].as_f64() == Some(304.0)
                && frame["value"].as_f64() == Some(1.0)));
        assert!(snapshot
            .iter()
            .any(|frame| frame["code"].as_f64() == Some(315.0)
                && frame["value"].as_f64() == Some(0.0)));
        second.close(None).await.unwrap();
        drop(second);
        producer
            .send(ClientMessage::Text(
                serde_json::to_string(&SeatRequest::PhysicalDisconnected {
                    device_id: "validated-source".into(),
                })
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();
        native_batch(&mut browser, "device-removed").await;

        let source = RemoteSource {
            launch_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            controller_number: 15,
            state: GamepadState::neutral(),
            slot: None,
        };
        remote_events.lock().unwrap().push(RemoteEvent::Connected {
            source: source.clone(),
        });
        let arrival = native_batch(&mut browser, "device-state-complete").await;
        assert_eq!(
            arrival[0]["device"]["deviceId"],
            "remote:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:15"
        );
        let held = RemoteSource {
            state: GamepadState {
                buttons: 0x1000,
                ..Default::default()
            },
            ..source.clone()
        };
        remote_events.lock().unwrap().extend([
            RemoteEvent::State { source: held },
            RemoteEvent::State {
                source: source.clone(),
            },
        ]);
        let pressed = native_batch(&mut browser, "syn").await;
        let released = native_batch(&mut browser, "syn").await;
        assert_eq!(pressed[0]["code"].as_f64(), Some(304.0));
        assert_eq!(pressed[0]["value"].as_f64(), Some(1.0));
        assert_eq!(released[0]["code"].as_f64(), Some(304.0));
        assert_eq!(released[0]["value"].as_f64(), Some(0.0));
        // The same authenticated remote edges also reach the inputd connection.
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let text = producer.next().await.unwrap().unwrap().into_text().unwrap();
                let reply: korri_input_contract::SeatReply = serde_json::from_str(&text).unwrap();
                if reply.remote_events.len() == 2 { assert!(matches!(&reply.remote_events[0], RemoteEvent::State { source } if source.slot.is_none() && source.state.buttons == 0x1000)); break; }
            }
        }).await.unwrap();
        browser
            .send(ClientMessage::Text(
                serde_json::to_string(&connected).unwrap().into(),
            ))
            .await
            .unwrap();
        let closed = tokio::time::timeout(Duration::from_secs(2), browser.next())
            .await
            .unwrap();
        assert!(!matches!(closed, Some(Ok(ClientMessage::Text(_)))));
        assert_eq!(
            requests
                .lock()
                .unwrap()
                .iter()
                .filter(|request| matches!(request, SeatRequest::PhysicalConnected { .. }))
                .count(),
            1
        );
        producer.close(None).await.unwrap();
        drop(producer);
        drop(browser);
        let permit = tokio::time::timeout(
            Duration::from_secs(2),
            coordinator.producer.clone().acquire_owned(),
        )
        .await
        .unwrap()
        .unwrap();
        drop(permit);
        private_server.abort();
        let _ = private_server.await;
        native_server.abort();
        let _ = native_server.await;
        drop(coordinator);
        receiver.join().unwrap();
    }

    #[tokio::test]
    async fn real_unix_websocket_accepts_only_physical_source_operations() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let received = seen.clone();
        let (coordinator, receiver) = test_support::coordinator(move |request| {
            received.lock().unwrap().push(request.clone());
            let mut reply = test_support::reply();
            if matches!(request, SeatRequest::PhysicalConnected { .. }) {
                reply.slot = Some(1);
            }
            reply
        });
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("private.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let app = router(coordinator.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        let (mut client, _) = client_async("ws://localhost/", stream).await.unwrap();
        client.next().await.unwrap().unwrap(); // initial remote baseline
        let physical = SeatRequest::PhysicalConnected {
            device_id: "validated-controller".into(),
            name: "Gamepad".into(),
            state: Default::default(),
        };
        client
            .send(ClientMessage::Text(
                serde_json::to_string(&physical).unwrap().into(),
            ))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let text = client.next().await.unwrap().unwrap().into_text().unwrap();
                let reply: korri_input_contract::SeatReply = serde_json::from_str(&text).unwrap();
                if reply.slot == Some(1) {
                    break;
                }
            }
        })
        .await
        .unwrap();
        client
            .send(ClientMessage::Text(
                serde_json::to_string(&SeatRequest::ApplyCount { count: 255 })
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(Ok(message)) = client.next().await {
                if message.is_close() {
                    break;
                }
            }
        })
        .await
        .unwrap();
        {
            let requests = seen.lock().unwrap();
            assert!(requests.contains(&physical));
            assert!(!requests
                .iter()
                .any(|request| matches!(request, SeatRequest::ApplyCount { .. })));
            assert!(requests.iter().any(|request| matches!(request, SeatRequest::PhysicalDisconnected { device_id } if device_id == "validated-controller")));
        }
        drop(client);
        server.abort();
        let _ = server.await;
        drop(coordinator);
        receiver.join().unwrap();
    }

    #[tokio::test]
    async fn real_unix_websocket_rejects_origin_and_second_producer() {
        let (coordinator, receiver) = test_support::coordinator(|_| test_support::reply());
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("private.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let app = router(coordinator.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let mut request = "ws://localhost/".into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Origin", "http://portal.local".parse().unwrap());
        let stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        let error = client_async(request, stream).await.unwrap_err();
        assert!(
            matches!(error, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == StatusCode::FORBIDDEN)
        );
        let stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        let (mut first, _) = client_async("ws://localhost/", stream).await.unwrap();
        first.next().await.unwrap().unwrap();
        let stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        let error = client_async("ws://localhost/", stream).await.unwrap_err();
        assert!(
            matches!(error, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == StatusCode::CONFLICT)
        );
        first.close(None).await.unwrap();
        drop(first);
        // Release the upgraded handler before shutting down its listener.
        let permit = tokio::time::timeout(
            Duration::from_secs(2),
            coordinator.producer.clone().acquire_owned(),
        )
        .await
        .unwrap()
        .unwrap();
        drop(permit);
        server.abort();
        let _ = server.await;
        drop(coordinator);
        receiver.join().unwrap();
    }
}
